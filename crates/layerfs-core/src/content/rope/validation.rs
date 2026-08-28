fn validate_node<S: ObjectRead>(
    store: &S,
    expected: Summary,
    root: bool,
    counters: &mut RopeCounters,
    ancestors: &mut Vec<ObjectId>,
) -> CoreResult<()> {
    if ancestors.contains(&expected.id) {
        return Err(CoreError::MappingCycle);
    }
    ancestors.push(expected.id);
    match load_node(store, expected, root, counters)? {
        ExtentNodeV3::Leaf { extents, .. } => {
            for batch in extents.chunks(64) {
                let ids = batch
                    .iter()
                    .map(|extent| extent.payload_object_id)
                    .collect::<Vec<_>>();
                let mut index = 0_usize;
                store.get_authenticated_payload_lengths_batch(&ids, |id, payload_length| {
                    let extent = batch
                        .get(index)
                        .ok_or(CoreError::InvalidRecord("payload batch cardinality"))?;
                    if id != extent.payload_object_id {
                        return Err(CoreError::IdentityMismatch);
                    }
                    index += 1;
                    if payload_length as usize > crate::cdc::MAXIMUM_CHUNK_BYTES
                        || extent
                            .source_offset
                            .checked_add(extent.logical_length)
                            .is_none_or(|end| end > payload_length)
                    {
                        return Err(CoreError::ChunkLengthMismatch);
                    }
                    Ok(())
                })?;
                if index != batch.len() {
                    return Err(CoreError::InvalidRecord("payload batch cardinality"));
                }
            }
        }
        ExtentNodeV3::Branch {
            level, children, ..
        } => {
            for child in child_summaries(&children, level - 1) {
                validate_node(store, child, false, counters, ancestors)?;
            }
        }
    }
    ancestors.pop();
    Ok(())
}

fn visit_extent_node<S: ObjectRead>(
    store: &S,
    expected: Summary,
    root: bool,
    counters: &mut RopeCounters,
    ancestors: &mut Vec<ObjectId>,
    visitor: &mut impl FnMut(&[ExtentSliceV3]) -> CoreResult<()>,
) -> CoreResult<()> {
    if ancestors.contains(&expected.id) {
        return Err(CoreError::MappingCycle);
    }
    ancestors.push(expected.id);
    match load_node(store, expected, root, counters)? {
        ExtentNodeV3::Leaf { extents, .. } => visitor(&extents)?,
        ExtentNodeV3::Branch {
            level, children, ..
        } => {
            for child in child_summaries(&children, level - 1) {
                visit_extent_node(store, child, false, counters, ancestors, visitor)?;
            }
        }
    }
    ancestors.pop();
    Ok(())
}

fn child_summaries(children: &[ChildDescriptorV3], level: u8) -> Vec<Summary> {
    let mut prior_bytes = 0;
    let mut prior_extents = 0;
    children
        .iter()
        .map(|child| {
            let summary = Summary {
                id: child.child_object_id,
                bytes: child.cumulative_logical_end - prior_bytes,
                extents: child.cumulative_extent_end - prior_extents,
                level,
            };
            prior_bytes = child.cumulative_logical_end;
            prior_extents = child.cumulative_extent_end;
            summary
        })
        .collect()
}

fn coalesce(extents: &mut Vec<ExtentSliceV3>) -> CoreResult<()> {
    let mut index = 1;
    while index < extents.len() {
        let previous = extents[index - 1];
        let current = extents[index];
        if previous.payload_object_id == current.payload_object_id
            && previous.source_offset.checked_add(previous.logical_length)
                == Some(current.source_offset)
        {
            extents[index - 1].logical_length = previous
                .logical_length
                .checked_add(current.logical_length)
                .ok_or(CoreError::LengthOverflow)?;
            extents.remove(index);
        } else {
            index += 1;
        }
    }
    Ok(())
}

fn merge_counters(target: &mut RopeCounters, source: RopeCounters) -> CoreResult<()> {
    target.payload_bytes_read = add(target.payload_bytes_read, source.payload_bytes_read)?;
    target.payload_bytes_written = add(target.payload_bytes_written, source.payload_bytes_written)?;
    target.cdc_bytes_scanned = add(target.cdc_bytes_scanned, source.cdc_bytes_scanned)?;
    target.chunks_created = add(target.chunks_created, source.chunks_created)?;
    target.nodes_read = add(target.nodes_read, source.nodes_read)?;
    target.nodes_created = add(target.nodes_created, source.nodes_created)?;
    target.tree_level_before = match (target.tree_level_before, source.tree_level_before) {
        (None, value) | (value, None) => value,
        (Some(left), Some(right)) if left == right => Some(left),
        (Some(_), Some(_)) => None,
    };
    target.logical_len_before =
        merge_optional_equal(target.logical_len_before, source.logical_len_before);
    target.logical_len_after =
        merge_optional_equal(target.logical_len_after, source.logical_len_after);
    Ok(())
}

fn merge_optional_equal<T: Copy + Eq>(left: Option<T>, right: Option<T>) -> Option<T> {
    match (left, right) {
        (None, value) | (value, None) => value,
        (Some(left), Some(right)) if left == right => Some(left),
        (Some(_), Some(_)) => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_summary(node: &ExtentNodeV3, expected: Summary) -> CoreResult<()> {
    if node.level() != expected.level
        || node.logical_len() != expected.bytes
        || node.extent_count() != expected.extents
    {
        Err(CoreError::InvalidRecord("extent summary"))
    } else {
        Ok(())
    }
}
