pub fn metadata_tree_entries<S: ObjectRead>(
    store: &S,
    root: ObjectId,
) -> CoreResult<Vec<MetadataEntryV1>> {
    let mut output = Vec::new();
    visit_metadata_entries(store, root, |entries| {
        output.extend_from_slice(entries);
        Ok(())
    })?;
    Ok(output)
}

pub fn metadata_lookup<S: ObjectRead>(
    store: &S,
    root: ObjectId,
    key: &MetadataKey,
) -> CoreResult<Option<MetadataEntryV1>> {
    let mut found = None;
    visit_metadata_entries(store, root, |entries| {
        if found.is_none() {
            found = entries
                .binary_search_by(|entry| entry.key.cmp(key))
                .ok()
                .map(|index| entries[index].clone());
        }
        Ok(())
    })?;
    Ok(found)
}

/// Three-way merges ordered metadata entries with only bounded tree frontiers.
/// Conflicting values for the same key return `None`.
pub fn visit_metadata_entries<S: ObjectRead>(
    store: &S,
    root: ObjectId,
    mut visitor: impl FnMut(&[MetadataEntryV1]) -> CoreResult<()>,
) -> CoreResult<()> {
    read_metadata_node(store, root, true, None, None, &mut Vec::new(), &mut visitor)?;
    Ok(())
}

fn read_metadata_node<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    root: bool,
    expected_level: Option<u8>,
    expected_max: Option<&MetadataKey>,
    ancestors: &mut Vec<ObjectId>,
    visitor: &mut impl FnMut(&[MetadataEntryV1]) -> CoreResult<()>,
) -> CoreResult<MetadataSummary> {
    if ancestors.contains(&id) {
        return Err(CoreError::MappingCycle);
    }
    ancestors.push(id);
    let loaded = load_metadata_shallow(store, id, root, expected_level, expected_max)?;
    let summary = match loaded.node {
        MetadataNodeV1::Leaf {
            subtree_encoded_bytes,
            entries,
        } => {
            visitor(&entries)?;
            MetadataSummary {
                encoded_bytes: subtree_encoded_bytes,
                ..loaded.summary
            }
        }
        MetadataNodeV1::Branch {
            level,
            subtree_entry_count,
            subtree_encoded_bytes,
            children,
        } => {
            let mut count = 0_u64;
            let mut bytes = 0_u64;
            let mut minimum = None;
            let mut previous_max: Option<MetadataKey> = None;
            let child_level = level
                .checked_sub(1)
                .ok_or(CoreError::InvalidRecord("metadata child summary"))?;
            for (maximum, child) in &children {
                let child = read_metadata_node(
                    store,
                    *child,
                    false,
                    Some(child_level),
                    Some(maximum),
                    ancestors,
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
                count = count
                    .checked_add(child.entries)
                    .ok_or(CoreError::LengthOverflow)?;
                bytes = bytes
                    .checked_add(child.encoded_bytes)
                    .ok_or(CoreError::LengthOverflow)?;
                previous_max = child.max.clone();
            }
            if count != subtree_entry_count || bytes != subtree_encoded_bytes {
                return Err(CoreError::InvalidRecord("metadata branch summary"));
            }
            MetadataSummary {
                id,
                min: minimum,
                max: children.last().map(|child| child.0.clone()),
                entries: count,
                encoded_bytes: bytes,
                level,
            }
        }
    };
    ancestors.pop();
    Ok(summary)
}

fn load_metadata_shallow<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    root: bool,
    expected_level: Option<u8>,
    expected_max: Option<&MetadataKey>,
) -> CoreResult<ValidatedMetadataNode> {
    let node = store.with_authenticated_canonical(id, |canonical| {
        if !root && canonical.len() * 5 < 8192 * 2 {
            return Err(CoreError::NonCanonicalPagePartition);
        }
        decode_metadata_node(canonical)
    })?;
    let summary = metadata_node_shape(id, &node, root)?;
    if expected_level.is_some_and(|level| summary.level != level)
        || expected_max.is_some_and(|maximum| summary.max.as_ref() != Some(maximum))
    {
        return Err(CoreError::InvalidRecord("metadata child summary"));
    }
    Ok(ValidatedMetadataNode { node, summary })
}

fn metadata_node_shape(
    id: ObjectId,
    node: &MetadataNodeV1,
    root: bool,
) -> CoreResult<MetadataSummary> {
    let (min, max, entries, encoded_bytes, level, count) = match node {
        MetadataNodeV1::Leaf {
            subtree_encoded_bytes,
            entries,
        } => (
            entries.first().map(|entry| entry.key.clone()),
            entries.last().map(|entry| entry.key.clone()),
            entries.len() as u64,
            *subtree_encoded_bytes,
            0,
            entries.len(),
        ),
        MetadataNodeV1::Branch {
            level,
            subtree_entry_count,
            subtree_encoded_bytes,
            children,
        } => (
            children.first().map(|child| child.0.clone()),
            children.last().map(|child| child.0.clone()),
            *subtree_entry_count,
            *subtree_encoded_bytes,
            *level,
            children.len(),
        ),
    };
    if matches!(node, MetadataNodeV1::Branch { .. }) && count < 2 || !root && count == 0 {
        return Err(CoreError::NonCanonicalPagePartition);
    }
    Ok(MetadataSummary {
        id,
        min,
        max,
        entries,
        encoded_bytes,
        level,
    })
}
