fn inode_underfull<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    counters: &mut InodeTableCounters,
) -> CoreResult<bool> {
    Ok(match load(store, id, counters)? {
        InodeTableNodeV1::Leaf(entries) => entries.len() < 64,
        InodeTableNodeV1::Branch { children, .. } => children.len() < 64,
    })
}

fn rebalance_inode_children<S: ObjectStore>(
    store: &mut S,
    child_level: u8,
    children: Vec<(InodeId, ObjectId)>,
    index: usize,
    counters: &mut InodeTableCounters,
) -> CoreResult<Vec<(InodeId, ObjectId)>> {
    if index > 0 {
        if let Some(replacements) = try_inode_borrow(
            store,
            child_level,
            children[index - 1].1,
            children[index].1,
            true,
            counters,
        )? {
            return Ok(replace_inode_pair(children, index - 1, replacements));
        }
    }
    if index + 1 < children.len() {
        if let Some(replacements) = try_inode_borrow(
            store,
            child_level,
            children[index].1,
            children[index + 1].1,
            false,
            counters,
        )? {
            return Ok(replace_inode_pair(children, index, replacements));
        }
    }
    if index > 0 {
        if let Some(replacement) = try_inode_merge(
            store,
            child_level,
            children[index - 1].1,
            children[index].1,
            counters,
        )? {
            return Ok(replace_inode_pair(children, index - 1, vec![replacement]));
        }
    }
    if index + 1 < children.len() {
        if let Some(replacement) = try_inode_merge(
            store,
            child_level,
            children[index].1,
            children[index + 1].1,
            counters,
        )? {
            return Ok(replace_inode_pair(children, index, vec![replacement]));
        }
    }
    Err(CoreError::NonCanonicalPagePartition)
}

fn try_inode_borrow<S: ObjectStore>(
    store: &mut S,
    child_level: u8,
    left_id: ObjectId,
    right_id: ObjectId,
    borrow_left: bool,
    counters: &mut InodeTableCounters,
) -> CoreResult<Option<Vec<Summary>>> {
    let (left, right) = match (
        load(store, left_id, counters)?,
        load(store, right_id, counters)?,
    ) {
        (InodeTableNodeV1::Leaf(mut left), InodeTableNodeV1::Leaf(mut right)) => {
            if borrow_left {
                if left.len() <= 64 {
                    return Ok(None);
                }
                right.insert(0, left.pop().unwrap());
            } else {
                if right.len() <= 64 {
                    return Ok(None);
                }
                left.push(right.remove(0));
            }
            (InodeTableNodeV1::Leaf(left), InodeTableNodeV1::Leaf(right))
        }
        (
            InodeTableNodeV1::Branch {
                children: mut left, ..
            },
            InodeTableNodeV1::Branch {
                children: mut right,
                ..
            },
        ) => {
            if borrow_left {
                if left.len() <= 64 {
                    return Ok(None);
                }
                right.insert(0, left.pop().unwrap());
            } else {
                if right.len() <= 64 {
                    return Ok(None);
                }
                left.push(right.remove(0));
            }
            (
                InodeTableNodeV1::Branch {
                    level: child_level,
                    subtree_entry_count: inode_children_count(store, &left, counters)?,
                    children: left,
                },
                InodeTableNodeV1::Branch {
                    level: child_level,
                    subtree_entry_count: inode_children_count(store, &right, counters)?,
                    children: right,
                },
            )
        }
        _ => return Err(CoreError::WrongLogicalRole),
    };
    Ok(Some(vec![
        emit(store, left, counters)?,
        emit(store, right, counters)?,
    ]))
}

fn try_inode_merge<S: ObjectStore>(
    store: &mut S,
    child_level: u8,
    left_id: ObjectId,
    right_id: ObjectId,
    counters: &mut InodeTableCounters,
) -> CoreResult<Option<Summary>> {
    let node = match (
        load(store, left_id, counters)?,
        load(store, right_id, counters)?,
    ) {
        (InodeTableNodeV1::Leaf(mut left), InodeTableNodeV1::Leaf(right)) => {
            left.extend(right);
            if left.len() > 127 {
                return Ok(None);
            }
            InodeTableNodeV1::Leaf(left)
        }
        (
            InodeTableNodeV1::Branch {
                children: mut left, ..
            },
            InodeTableNodeV1::Branch {
                children: right, ..
            },
        ) => {
            left.extend(right);
            if left.len() > 127 {
                return Ok(None);
            }
            InodeTableNodeV1::Branch {
                level: child_level,
                subtree_entry_count: inode_children_count(store, &left, counters)?,
                children: left,
            }
        }
        _ => return Err(CoreError::WrongLogicalRole),
    };
    emit(store, node, counters).map(Some)
}

fn inode_children_count<S: ObjectRead>(
    store: &S,
    children: &[(InodeId, ObjectId)],
    counters: &mut InodeTableCounters,
) -> CoreResult<u64> {
    children.iter().try_fold(0_u64, |sum, (_, id)| {
        sum.checked_add(summary(store, *id, counters)?.entries)
            .ok_or(CoreError::LengthOverflow)
    })
}

fn replace_inode_pair(
    mut children: Vec<(InodeId, ObjectId)>,
    start: usize,
    replacements: Vec<Summary>,
) -> Vec<(InodeId, ObjectId)> {
    children.splice(
        start..=start + 1,
        replacements.into_iter().map(|item| (item.max, item.id)),
    );
    children
}

fn emit_branch<S: ObjectStore>(
    store: &mut S,
    level: u8,
    children: Vec<Summary>,
    counters: &mut InodeTableCounters,
) -> CoreResult<Summary> {
    let count = children.iter().try_fold(0_u64, |sum, child| {
        sum.checked_add(child.entries)
            .ok_or(CoreError::LengthOverflow)
    })?;
    let descriptors = children
        .into_iter()
        .map(|child| (child.max, child.id))
        .collect();
    emit(
        store,
        InodeTableNodeV1::Branch {
            level,
            subtree_entry_count: count,
            children: descriptors,
        },
        counters,
    )
}

fn emit_branch_descriptors<S: ObjectStore>(
    store: &mut S,
    level: u8,
    children: Vec<(InodeId, ObjectId)>,
    counters: &mut InodeTableCounters,
) -> CoreResult<Summary> {
    let mut count = 0_u64;
    for (_, id) in &children {
        count = count
            .checked_add(summary(store, *id, counters)?.entries)
            .ok_or(CoreError::LengthOverflow)?;
    }
    emit(
        store,
        InodeTableNodeV1::Branch {
            level,
            subtree_entry_count: count,
            children,
        },
        counters,
    )
}

fn emit<S: ObjectStore>(
    store: &mut S,
    node: InodeTableNodeV1,
    counters: &mut InodeTableCounters,
) -> CoreResult<Summary> {
    let (max, entries, level) = match &node {
        InodeTableNodeV1::Leaf(entries) => (
            entries
                .last()
                .ok_or(CoreError::InvalidRecord("empty inode table"))?
                .0,
            entries.len() as u64,
            0,
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
        ),
    };
    let canonical = encode_inode_table_node(&node)?;
    let min = inode_node_min(&node)?;
    let id = store.put(&canonical)?;
    counters.nodes_created = counters
        .nodes_created
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    Ok(Summary {
        id,
        min,
        max,
        entries,
        level,
    })
}
