/// Streams changed inode-table entries while pruning equal persistent
/// subtrees by identity. Unequal heights or page partitions fall back to a
/// bounded leaf cursor rather than collecting the table.
pub fn diff_inode_table_entries<S: ObjectRead>(
    store: &S,
    old: InodeTableRoot,
    new: InodeTableRoot,
    mut visitor: impl FnMut(InodeTableDiff) -> CoreResult<()>,
) -> CoreResult<InodeTableCounters> {
    let mut counters = InodeTableCounters::default();
    if old == new {
        return Ok(counters);
    }
    diff_inode_nodes(store, old.0, new.0, true, &mut counters, &mut visitor)?;
    Ok(counters)
}

fn diff_inode_nodes<S: ObjectRead>(
    store: &S,
    old: ObjectId,
    new: ObjectId,
    root: bool,
    counters: &mut InodeTableCounters,
    visitor: &mut impl FnMut(InodeTableDiff) -> CoreResult<()>,
) -> CoreResult<()> {
    if old == new {
        return Ok(());
    }
    let old_node = load_shallow(store, old, root, None, counters)?;
    let new_node = load_shallow(store, new, root, None, counters)?;
    match (&old_node.node, &new_node.node) {
        (InodeTableNodeV1::Leaf(old), InodeTableNodeV1::Leaf(new)) => merge_inode_entries(
            old.iter().copied().map(Ok),
            new.iter().copied().map(Ok),
            visitor,
        ),
        (
            InodeTableNodeV1::Branch {
                level: old_level,
                children: old_children,
                ..
            },
            InodeTableNodeV1::Branch {
                level: new_level,
                children: new_children,
                ..
            },
        ) if old_level == new_level
            && old_children.len() == new_children.len()
            && old_children
                .iter()
                .zip(new_children)
                .all(|(old, new)| old.0 == new.0) =>
        {
            for ((_, old_child), (_, new_child)) in old_children.iter().zip(new_children) {
                diff_inode_nodes(store, *old_child, *new_child, false, counters, visitor)?;
            }
            Ok(())
        }
        _ => {
            let mut old_counters = InodeTableCounters::default();
            let mut new_counters = InodeTableCounters::default();
            let result = merge_inode_entries(
                InodeEntryCursor::new(store, old, root, &mut old_counters),
                InodeEntryCursor::new(store, new, root, &mut new_counters),
                visitor,
            );
            counters.nodes_read = counters
                .nodes_read
                .checked_add(old_counters.nodes_read)
                .and_then(|value| value.checked_add(new_counters.nodes_read))
                .ok_or(CoreError::LengthOverflow)?;
            result
        }
    }
}

fn merge_inode_entries(
    old: impl Iterator<Item = CoreResult<(InodeId, ObjectId)>>,
    new: impl Iterator<Item = CoreResult<(InodeId, ObjectId)>>,
    visitor: &mut impl FnMut(InodeTableDiff) -> CoreResult<()>,
) -> CoreResult<()> {
    let mut old = old;
    let mut new = new;
    let mut old_entry = old.next().transpose()?;
    let mut new_entry = new.next().transpose()?;
    loop {
        match (old_entry, new_entry) {
            (None, None) => return Ok(()),
            (Some((old_key, before)), Some((new_key, after))) if old_key == new_key => {
                if before != after {
                    visitor(InodeTableDiff {
                        inode: old_key,
                        before: Some(before),
                        after: Some(after),
                    })?;
                }
                old_entry = old.next().transpose()?;
                new_entry = new.next().transpose()?;
            }
            (Some((old_key, before)), Some((new_key, after))) if old_key < new_key => {
                visitor(InodeTableDiff {
                    inode: old_key,
                    before: Some(before),
                    after: None,
                })?;
                old_entry = old.next().transpose()?;
                new_entry = Some((new_key, after));
            }
            (Some((old_key, before)), Some((new_key, after))) => {
                visitor(InodeTableDiff {
                    inode: new_key,
                    before: None,
                    after: Some(after),
                })?;
                old_entry = Some((old_key, before));
                new_entry = new.next().transpose()?;
            }
            (Some((old_key, before)), None) => {
                visitor(InodeTableDiff {
                    inode: old_key,
                    before: Some(before),
                    after: None,
                })?;
                old_entry = old.next().transpose()?;
            }
            (None, Some((new_key, after))) => {
                visitor(InodeTableDiff {
                    inode: new_key,
                    before: None,
                    after: Some(after),
                })?;
                new_entry = new.next().transpose()?;
            }
        }
    }
}
