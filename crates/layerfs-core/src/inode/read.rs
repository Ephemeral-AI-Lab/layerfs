pub fn inode_table_from_root<S: ObjectStore>(
    store: &mut S,
    root_inode: InodeId,
    record: ObjectId,
) -> CoreResult<InodeTableRoot> {
    let mut counters = InodeTableCounters::default();
    Ok(InodeTableRoot(
        emit(
            store,
            InodeTableNodeV1::Leaf(vec![(root_inode, record)]),
            &mut counters,
        )?
        .id,
    ))
}

pub fn inode_table_lookup<S: ObjectRead>(
    store: &S,
    root: InodeTableRoot,
    key: InodeId,
    counters: &mut InodeTableCounters,
) -> CoreResult<Option<ObjectId>> {
    let mut current = load_shallow(store, root.0, true, None, counters)?;
    loop {
        match current.node {
            InodeTableNodeV1::Leaf(entries) => {
                return Ok(entries
                    .binary_search_by_key(&key, |entry| entry.0)
                    .ok()
                    .map(|index| entries[index].1))
            }
            InodeTableNodeV1::Branch { children, .. } => {
                let index = children
                    .partition_point(|entry| entry.0 < key)
                    .min(children.len() - 1);
                let expected_max = children[index].0;
                let child = load_shallow(store, children[index].1, false, None, counters)?;
                if child.summary.max != expected_max
                    || child.summary.level.checked_add(1) != Some(current.summary.level)
                {
                    return Err(CoreError::InvalidRecord("inode child summary"));
                }
                current = child;
            }
        }
    }
}

pub fn inode_table_entries<S: ObjectRead>(
    store: &S,
    root: InodeTableRoot,
    counters: &mut InodeTableCounters,
) -> CoreResult<Vec<(InodeId, ObjectId)>> {
    let mut output = Vec::new();
    visit_inode_table_entries(store, root, counters, |entries| {
        output.extend_from_slice(entries);
        Ok(())
    })?;
    Ok(output)
}

pub fn visit_inode_table_entries<S: ObjectRead>(
    store: &S,
    root: InodeTableRoot,
    counters: &mut InodeTableCounters,
    mut visitor: impl FnMut(&[(InodeId, ObjectId)]) -> CoreResult<()>,
) -> CoreResult<()> {
    walk_inode_node(store, root.0, true, None, None, counters, &mut visitor).map(drop)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InodeTableDiff {
    pub inode: InodeId,
    pub before: Option<ObjectId>,
    pub after: Option<ObjectId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InodeTableMergeConflict {
    pub inode: InodeId,
    pub source: Option<ObjectId>,
    pub destination: Option<ObjectId>,
}

/// Merges `base -> source` onto `destination` one changed inode at a time.
/// The two cursors retain only tree frontiers; equal subtrees and unchanged
/// suffixes are never collected. ponytail: adjacent changed keys path-copy
/// separately; batch them only if merge throughput becomes a measured limit.
fn summary<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    counters: &mut InodeTableCounters,
) -> CoreResult<Summary> {
    Ok(load_shallow(store, id, true, None, counters)?.summary)
}

fn walk_inode_node<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    root: bool,
    expected_level: Option<u8>,
    expected_max: Option<InodeId>,
    counters: &mut InodeTableCounters,
    visitor: &mut impl FnMut(&[(InodeId, ObjectId)]) -> CoreResult<()>,
) -> CoreResult<Summary> {
    let mut loaded = load_shallow(store, id, root, None, counters)?;
    if expected_level.is_some_and(|level| loaded.summary.level != level)
        || expected_max.is_some_and(|maximum| loaded.summary.max != maximum)
    {
        return Err(CoreError::InvalidRecord("inode child summary"));
    }
    match &loaded.node {
        InodeTableNodeV1::Leaf(entries) => visitor(entries)?,
        InodeTableNodeV1::Branch {
            level,
            subtree_entry_count,
            children,
        } => {
            let child_level = level
                .checked_sub(1)
                .ok_or(CoreError::InvalidRecord("inode child summary"))?;
            let mut count = 0_u64;
            let mut minimum = None;
            let mut previous_max: Option<InodeId> = None;
            for (maximum, child_id) in children {
                let child = walk_inode_node(
                    store,
                    *child_id,
                    false,
                    Some(child_level),
                    Some(*maximum),
                    counters,
                    visitor,
                )?;
                if previous_max.is_some_and(|previous| previous >= child.min) {
                    return Err(CoreError::NonCanonicalOrdering);
                }
                minimum.get_or_insert(child.min);
                count = count
                    .checked_add(child.entries)
                    .ok_or(CoreError::LengthOverflow)?;
                previous_max = Some(child.max);
            }
            if count != *subtree_entry_count {
                return Err(CoreError::InvalidRecord("inode branch summary"));
            }
            loaded.summary.min = minimum.ok_or(CoreError::NonCanonicalPagePartition)?;
        }
    }
    Ok(loaded.summary)
}

fn load_shallow<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    root: bool,
    expected: Option<&Summary>,
    counters: &mut InodeTableCounters,
) -> CoreResult<ValidatedNode> {
    let node = load(store, id, counters)?;
    let summary = inode_node_shape(id, &node, root)?;
    if expected.is_some_and(|expected| {
        summary.max != expected.max
            || summary.entries != expected.entries
            || summary.level != expected.level
    }) {
        return Err(CoreError::InvalidRecord("inode child summary"));
    }
    Ok(ValidatedNode { node, summary })
}

fn inode_node_shape(id: ObjectId, node: &InodeTableNodeV1, root: bool) -> CoreResult<Summary> {
    let (max, entries, level, count) = match node {
        InodeTableNodeV1::Leaf(entries) => (
            entries
                .last()
                .ok_or(CoreError::InvalidRecord("empty inode table"))?
                .0,
            entries.len() as u64,
            0,
            entries.len(),
        ),
        InodeTableNodeV1::Branch {
            level,
            subtree_entry_count,
            children,
        } => (
            children
                .last()
                .ok_or(CoreError::InvalidRecord("empty inode branch"))?
                .0,
            *subtree_entry_count,
            *level,
            children.len(),
        ),
    };
    if (root && matches!(node, InodeTableNodeV1::Branch { .. }) && count < 2)
        || (!root && !(64..=127).contains(&count))
    {
        return Err(CoreError::NonCanonicalPagePartition);
    }
    Ok(Summary {
        id,
        min: inode_node_min(node)?,
        max,
        entries,
        level,
    })
}

fn inode_node_min(node: &InodeTableNodeV1) -> CoreResult<InodeId> {
    match node {
        InodeTableNodeV1::Leaf(entries) => entries
            .first()
            .map(|entry| entry.0)
            .ok_or(CoreError::InvalidRecord("empty inode table")),
        InodeTableNodeV1::Branch { children, .. } => children
            .first()
            .map(|entry| entry.0)
            .ok_or(CoreError::InvalidRecord("empty inode branch")),
    }
}

fn load<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    counters: &mut InodeTableCounters,
) -> CoreResult<InodeTableNodeV1> {
    counters.nodes_read = counters
        .nodes_read
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    store.with_authenticated_canonical(id, decode_inode_table_node)
}
