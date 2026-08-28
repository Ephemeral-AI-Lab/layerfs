fn remove_node<S: ObjectStore>(
    store: &mut S,
    summary: NodeSummary,
    root: bool,
    name: &CanonicalName,
    counters: &mut NamespaceCounters,
) -> CoreResult<(NodeSummary, InodeId)> {
    let loaded = load_directory_node_expected_shallow(store, &summary, root, counters)?;
    match loaded.node {
        DirectoryNodeV1::Leaf { mut entries, .. } => {
            let index = entries
                .binary_search_by(|(candidate, _)| candidate.cmp(name))
                .map_err(|_| CoreError::PathNotFound)?;
            let removed = entries.remove(index).1;
            Ok((
                emit_directory_node(store, leaf(entries)?, counters)?,
                removed,
            ))
        }
        DirectoryNodeV1::Branch {
            level,
            subtree_entry_count,
            subtree_encoded_bytes,
            mut children,
        } => {
            let index = children
                .partition_point(|(maximum, _)| maximum < name)
                .min(children.len() - 1);
            let old = load_directory_summary(store, children[index].1, counters)?;
            if old.max.as_ref() != Some(&children[index].0)
                || old.level.checked_add(1) != Some(level)
            {
                return Err(CoreError::InvalidRecord("directory child summary"));
            }
            let (next, removed) = remove_node(store, old.clone(), false, name, counters)?;
            children[index] = (
                next.max
                    .clone()
                    .unwrap_or_else(|| children[index].0.clone()),
                next.id,
            );
            if children.len() > 1 && is_underfull(store, next.id, counters)? {
                children = rebalance_children(store, level - 1, children, index, counters)?;
            }
            let entries = subtree_entry_count
                .checked_sub(old.entries)
                .and_then(|value| value.checked_add(next.entries))
                .ok_or(CoreError::LengthOverflow)?;
            let bytes = subtree_encoded_bytes
                .checked_sub(old.encoded_bytes)
                .and_then(|value| value.checked_add(next.encoded_bytes))
                .ok_or(CoreError::LengthOverflow)?;
            let node = DirectoryNodeV1::Branch {
                level,
                subtree_entry_count: entries,
                subtree_encoded_bytes: bytes,
                children,
            };
            Ok((emit_directory_node(store, node, counters)?, removed))
        }
    }
}

fn rebalance_children<S: ObjectStore>(
    store: &mut S,
    child_level: u8,
    children: Vec<(CanonicalName, ObjectId)>,
    index: usize,
    counters: &mut NamespaceCounters,
) -> CoreResult<Vec<(CanonicalName, ObjectId)>> {
    if index > 0 {
        if let Some(replacements) = try_directory_borrow(
            store,
            child_level,
            children[index - 1].1,
            children[index].1,
            true,
            counters,
        )? {
            return replace_directory_pair(children, index - 1, replacements);
        }
    }
    if index + 1 < children.len() {
        if let Some(replacements) = try_directory_borrow(
            store,
            child_level,
            children[index].1,
            children[index + 1].1,
            false,
            counters,
        )? {
            return replace_directory_pair(children, index, replacements);
        }
    }
    if index > 0 {
        if let Some(replacement) = try_directory_merge(
            store,
            child_level,
            children[index - 1].1,
            children[index].1,
            counters,
        )? {
            return replace_directory_pair(children, index - 1, vec![replacement]);
        }
    }
    if index + 1 < children.len() {
        if let Some(replacement) = try_directory_merge(
            store,
            child_level,
            children[index].1,
            children[index + 1].1,
            counters,
        )? {
            return replace_directory_pair(children, index, vec![replacement]);
        }
    }
    Err(CoreError::NonCanonicalPagePartition)
}

fn try_directory_borrow<S: ObjectStore>(
    store: &mut S,
    child_level: u8,
    left_id: ObjectId,
    right_id: ObjectId,
    borrow_left: bool,
    counters: &mut NamespaceCounters,
) -> CoreResult<Option<Vec<NodeSummary>>> {
    let left = load_directory_node(store, left_id, counters)?;
    let right = load_directory_node(store, right_id, counters)?;
    let (left, right) = match (left, right) {
        (
            DirectoryNodeV1::Leaf {
                entries: mut left, ..
            },
            DirectoryNodeV1::Leaf {
                entries: mut right, ..
            },
        ) => loop {
            if borrow_left {
                let Some(entry) = left.pop() else {
                    return Ok(None);
                };
                right.insert(0, entry);
            } else {
                if right.is_empty() {
                    return Ok(None);
                }
                left.push(right.remove(0));
            }
            let left_node = leaf(left.clone())?;
            let right_node = leaf(right.clone())?;
            if !directory_filled(if borrow_left { &left_node } else { &right_node }) {
                return Ok(None);
            }
            if directory_filled(if borrow_left { &right_node } else { &left_node }) {
                break (left_node, right_node);
            }
        },
        (
            DirectoryNodeV1::Branch {
                children: mut left, ..
            },
            DirectoryNodeV1::Branch {
                children: mut right,
                ..
            },
        ) => loop {
            if borrow_left {
                let Some(entry) = left.pop() else {
                    return Ok(None);
                };
                right.insert(0, entry);
            } else {
                if right.is_empty() {
                    return Ok(None);
                }
                left.push(right.remove(0));
            }
            let left_node = branch(store, child_level, left.clone(), counters)?;
            let right_node = branch(store, child_level, right.clone(), counters)?;
            if !directory_filled(if borrow_left { &left_node } else { &right_node }) {
                return Ok(None);
            }
            if directory_filled(if borrow_left { &right_node } else { &left_node }) {
                break (left_node, right_node);
            }
        },
        _ => return Err(CoreError::WrongLogicalRole),
    };
    if !directory_filled(&left) || !directory_filled(&right) {
        return Ok(None);
    }
    Ok(Some(vec![
        emit_directory_node(store, left, counters)?,
        emit_directory_node(store, right, counters)?,
    ]))
}

fn try_directory_merge<S: ObjectStore>(
    store: &mut S,
    child_level: u8,
    left_id: ObjectId,
    right_id: ObjectId,
    counters: &mut NamespaceCounters,
) -> CoreResult<Option<NodeSummary>> {
    let node = match (
        load_directory_node(store, left_id, counters)?,
        load_directory_node(store, right_id, counters)?,
    ) {
        (
            DirectoryNodeV1::Leaf {
                entries: mut left, ..
            },
            DirectoryNodeV1::Leaf { entries: right, .. },
        ) => {
            left.extend(right);
            leaf(left)?
        }
        (
            DirectoryNodeV1::Branch {
                children: mut left, ..
            },
            DirectoryNodeV1::Branch {
                children: right, ..
            },
        ) => {
            left.extend(right);
            branch(store, child_level, left, counters)?
        }
        _ => return Err(CoreError::WrongLogicalRole),
    };
    if encode_directory_node(&node).is_err() {
        return Ok(None);
    }
    emit_directory_node(store, node, counters).map(Some)
}

fn directory_filled(node: &DirectoryNodeV1) -> bool {
    encode_directory_node(node).is_ok_and(|bytes| bytes.len() * 5 >= 8192 * 2)
}

fn replace_directory_pair(
    mut children: Vec<(CanonicalName, ObjectId)>,
    start: usize,
    replacements: Vec<NodeSummary>,
) -> CoreResult<Vec<(CanonicalName, ObjectId)>> {
    children.splice(
        start..=start + 1,
        replacements
            .into_iter()
            .map(|child| {
                Ok((
                    child
                        .max
                        .ok_or(CoreError::InvalidRecord("empty rebalanced child"))?,
                    child.id,
                ))
            })
            .collect::<CoreResult<Vec<_>>>()?,
    );
    Ok(children)
}

fn is_underfull<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    counters: &mut NamespaceCounters,
) -> CoreResult<bool> {
    Ok(encode_directory_node(&load_directory_node(store, id, counters)?)?.len() * 5 < 8192 * 2)
}

fn nearest_half(widths: Vec<usize>) -> usize {
    let total: usize = widths.iter().sum();
    let mut prefix = 0;
    let mut best = 1;
    let mut distance = usize::MAX;
    for (index, width) in widths.iter().enumerate().take(widths.len() - 1) {
        prefix += width;
        let next = total.abs_diff(prefix * 2);
        if next < distance {
            best = index + 1;
            distance = next;
        }
    }
    best
}
