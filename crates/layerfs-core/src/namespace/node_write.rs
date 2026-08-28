fn insert_node<S: ObjectStore>(
    store: &mut S,
    summary: NodeSummary,
    root: bool,
    name: CanonicalName,
    inode: InodeId,
    counters: &mut NamespaceCounters,
) -> CoreResult<Vec<NodeSummary>> {
    let loaded = load_directory_node_expected_shallow(store, &summary, root, counters)?;
    match loaded.node {
        DirectoryNodeV1::Leaf { mut entries, .. } => {
            match entries.binary_search_by(|(candidate, _)| candidate.cmp(&name)) {
                Ok(_) => return Err(CoreError::NameCollision),
                Err(index) => entries.insert(index, (name, inode)),
            }
            split_leaf_if_needed(store, entries, counters)
        }
        DirectoryNodeV1::Branch {
            level,
            subtree_entry_count,
            subtree_encoded_bytes,
            mut children,
        } => {
            let index = children
                .partition_point(|(maximum, _)| maximum < &name)
                .min(children.len() - 1);
            let child = load_directory_summary(store, children[index].1, counters)?;
            if child.max.as_ref() != Some(&children[index].0)
                || child.level.checked_add(1) != Some(level)
            {
                return Err(CoreError::InvalidRecord("directory child summary"));
            }
            let child_entries = child.entries;
            let child_bytes = child.encoded_bytes;
            let replacements = insert_node(store, child, false, name, inode, counters)?;
            let replacement_entries = replacements.iter().try_fold(0_u64, |sum, item| {
                sum.checked_add(item.entries)
                    .ok_or(CoreError::LengthOverflow)
            })?;
            let replacement_bytes = replacements.iter().try_fold(0_u64, |sum, item| {
                sum.checked_add(item.encoded_bytes)
                    .ok_or(CoreError::LengthOverflow)
            })?;
            let next_entries = subtree_entry_count
                .checked_sub(child_entries)
                .and_then(|value| value.checked_add(replacement_entries))
                .ok_or(CoreError::LengthOverflow)?;
            let next_bytes = subtree_encoded_bytes
                .checked_sub(child_bytes)
                .and_then(|value| value.checked_add(replacement_bytes))
                .ok_or(CoreError::LengthOverflow)?;
            children.splice(
                index..=index,
                replacements.iter().map(|child| {
                    (
                        child.max.clone().expect("nonempty nonroot directory"),
                        child.id,
                    )
                }),
            );
            split_branch_if_needed(store, level, children, next_entries, next_bytes, counters)
        }
    }
}

fn split_leaf_if_needed<S: ObjectStore>(
    store: &mut S,
    entries: Vec<(CanonicalName, InodeId)>,
    counters: &mut NamespaceCounters,
) -> CoreResult<Vec<NodeSummary>> {
    let node = leaf(entries.clone())?;
    if encode_directory_node(&node).is_ok() {
        return emit_directory_node(store, node, counters).map(|node| vec![node]);
    }
    let split = nearest_half(
        entries
            .iter()
            .map(|(name, _)| 34 + name.as_bytes().len())
            .collect(),
    );
    Ok(vec![
        emit_directory_node(store, leaf(entries[..split].to_vec())?, counters)?,
        emit_directory_node(store, leaf(entries[split..].to_vec())?, counters)?,
    ])
}

fn split_branch_if_needed<S: ObjectStore>(
    store: &mut S,
    level: u8,
    children: Vec<(CanonicalName, ObjectId)>,
    entries: u64,
    bytes: u64,
    counters: &mut NamespaceCounters,
) -> CoreResult<Vec<NodeSummary>> {
    let node = DirectoryNodeV1::Branch {
        level,
        subtree_entry_count: entries,
        subtree_encoded_bytes: bytes,
        children: children.clone(),
    };
    if encode_directory_node(&node).is_ok() {
        return emit_directory_node(store, node, counters).map(|node| vec![node]);
    }
    let split = nearest_half(
        children
            .iter()
            .map(|(name, _)| 34 + name.as_bytes().len())
            .collect(),
    );
    Ok(vec![
        emit_directory_node(
            store,
            branch(store, level, children[..split].to_vec(), counters)?,
            counters,
        )?,
        emit_directory_node(
            store,
            branch(store, level, children[split..].to_vec(), counters)?,
            counters,
        )?,
    ])
}

fn leaf(entries: Vec<(CanonicalName, InodeId)>) -> CoreResult<DirectoryNodeV1> {
    let bytes = entries.iter().try_fold(0_u64, |sum, (name, _)| {
        sum.checked_add(34 + name.as_bytes().len() as u64)
            .ok_or(CoreError::LengthOverflow)
    })?;
    Ok(DirectoryNodeV1::Leaf {
        subtree_encoded_bytes: bytes,
        entries,
    })
}

fn branch<S: ObjectRead>(
    store: &S,
    level: u8,
    children: Vec<(CanonicalName, ObjectId)>,
    counters: &mut NamespaceCounters,
) -> CoreResult<DirectoryNodeV1> {
    let mut entry_count = 0_u64;
    let mut encoded_bytes = 0_u64;
    for (_, id) in &children {
        let child = load_directory_summary(store, *id, counters)?;
        entry_count = entry_count
            .checked_add(child.entries)
            .ok_or(CoreError::LengthOverflow)?;
        encoded_bytes = encoded_bytes
            .checked_add(child.encoded_bytes)
            .ok_or(CoreError::LengthOverflow)?;
    }
    Ok(DirectoryNodeV1::Branch {
        level,
        subtree_entry_count: entry_count,
        subtree_encoded_bytes: encoded_bytes,
        children,
    })
}

fn emit_branch_from_summaries<S: ObjectStore>(
    store: &mut S,
    level: u8,
    children: Vec<NodeSummary>,
    counters: &mut NamespaceCounters,
) -> CoreResult<NodeSummary> {
    let entries = children.iter().try_fold(0_u64, |sum, child| {
        sum.checked_add(child.entries)
            .ok_or(CoreError::LengthOverflow)
    })?;
    let bytes = children.iter().try_fold(0_u64, |sum, child| {
        sum.checked_add(child.encoded_bytes)
            .ok_or(CoreError::LengthOverflow)
    })?;
    let descriptors = children
        .into_iter()
        .map(|child| (child.max.expect("nonempty child"), child.id))
        .collect();
    let node = DirectoryNodeV1::Branch {
        level,
        subtree_entry_count: entries,
        subtree_encoded_bytes: bytes,
        children: descriptors,
    };
    emit_directory_node(store, node, counters)
}

fn emit_directory_node<S: ObjectStore>(
    store: &mut S,
    node: DirectoryNodeV1,
    counters: &mut NamespaceCounters,
) -> CoreResult<NodeSummary> {
    let (max, entries, encoded_bytes, level) = node_fields(&node);
    let canonical = encode_directory_node(&node)?;
    let id = store.put(&canonical)?;
    counters.nodes_created = counters
        .nodes_created
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    Ok(NodeSummary {
        id,
        min: node_min(&node),
        max,
        entries,
        encoded_bytes,
        level,
    })
}

fn load_directory_summary<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    counters: &mut NamespaceCounters,
) -> CoreResult<NodeSummary> {
    Ok(load_directory_node_shallow(store, id, true, None, counters)?.summary)
}

fn load_directory_root_shallow<S: ObjectRead>(
    store: &S,
    state: &DirectoryStateV1,
    counters: &mut NamespaceCounters,
) -> CoreResult<ValidatedNode> {
    let loaded = load_directory_node_shallow(store, state.mapping_root, true, None, counters)?;
    if loaded.summary.entries != state.entry_count || loaded.summary.level != state.tree_level {
        return Err(CoreError::InvalidRecord("directory state summary"));
    }
    Ok(loaded)
}
