fn load_directory_node_expected_shallow<S: ObjectRead>(
    store: &S,
    expected: &NodeSummary,
    root: bool,
    counters: &mut NamespaceCounters,
) -> CoreResult<ValidatedNode> {
    load_directory_node_shallow(store, expected.id, root, Some(expected), counters)
}

fn load_directory_node_shallow<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    root: bool,
    expected: Option<&NodeSummary>,
    counters: &mut NamespaceCounters,
) -> CoreResult<ValidatedNode> {
    let node = load_directory_node(store, id, counters)?;
    let summary = directory_node_shape(id, &node, root)?;
    if expected.is_some_and(|expected| {
        summary.max != expected.max
            || summary.entries != expected.entries
            || summary.encoded_bytes != expected.encoded_bytes
            || summary.level != expected.level
    }) {
        return Err(CoreError::InvalidRecord("directory child summary"));
    }
    Ok(ValidatedNode { node, summary })
}

fn walk_directory_node<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    root: bool,
    expected_level: Option<u8>,
    expected_max: Option<&CanonicalName>,
    counters: &mut NamespaceCounters,
    visitor: &mut impl FnMut(&[(CanonicalName, InodeId)]) -> CoreResult<()>,
) -> CoreResult<NodeSummary> {
    let node = load_directory_node(store, id, counters)?;
    let mut summary = directory_node_shape(id, &node, root)?;
    if expected_level.is_some_and(|level| summary.level != level)
        || expected_max.is_some_and(|maximum| summary.max.as_ref() != Some(maximum))
    {
        return Err(CoreError::InvalidRecord("directory child summary"));
    }
    match &node {
        DirectoryNodeV1::Leaf { entries, .. } => visitor(entries)?,
        DirectoryNodeV1::Branch {
            level,
            subtree_entry_count,
            subtree_encoded_bytes,
            children,
        } => {
            let child_level = level
                .checked_sub(1)
                .ok_or(CoreError::InvalidRecord("directory child summary"))?;
            let mut entries = 0_u64;
            let mut bytes = 0_u64;
            let mut minimum = None;
            let mut previous_max: Option<CanonicalName> = None;
            for (maximum, child_id) in children {
                let child = walk_directory_node(
                    store,
                    *child_id,
                    false,
                    Some(child_level),
                    Some(maximum),
                    counters,
                    visitor,
                )?;
                if previous_max
                    .as_ref()
                    .zip(child.min.as_ref())
                    .is_some_and(|(previous, next)| previous >= next)
                {
                    return Err(CoreError::NonCanonicalOrdering);
                }
                if minimum.is_none() {
                    minimum = child.min.clone();
                }
                entries = entries
                    .checked_add(child.entries)
                    .ok_or(CoreError::LengthOverflow)?;
                bytes = bytes
                    .checked_add(child.encoded_bytes)
                    .ok_or(CoreError::LengthOverflow)?;
                previous_max = child.max;
            }
            if entries != *subtree_entry_count || bytes != *subtree_encoded_bytes {
                return Err(CoreError::InvalidRecord("directory branch summary"));
            }
            summary.min = minimum;
        }
    }
    Ok(summary)
}
