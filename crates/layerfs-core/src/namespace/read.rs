pub fn empty_directory<S: ObjectStore>(store: &mut S) -> CoreResult<DirectoryStateRoot> {
    let mut counters = NamespaceCounters::default();
    let node = emit_directory_node(
        store,
        DirectoryNodeV1::Leaf {
            subtree_encoded_bytes: 0,
            entries: Vec::new(),
        },
        &mut counters,
    )?;
    store_directory_state(store, node)
}

pub fn directory_lookup<S: ObjectRead>(
    store: &S,
    root: DirectoryStateRoot,
    name: &CanonicalName,
    counters: &mut NamespaceCounters,
) -> CoreResult<Option<InodeId>> {
    let state = load_directory_state(store, root, counters)?;
    let mut current = load_directory_root_shallow(store, &state, counters)?;
    loop {
        match current.node {
            DirectoryNodeV1::Leaf { entries, .. } => {
                return Ok(entries
                    .binary_search_by(|(candidate, _)| candidate.cmp(name))
                    .ok()
                    .map(|index| entries[index].1))
            }
            DirectoryNodeV1::Branch { children, .. } => {
                let index = children
                    .partition_point(|(maximum, _)| maximum < name)
                    .min(children.len().saturating_sub(1));
                let expected_max = children[index].0.clone();
                let child =
                    load_directory_node_shallow(store, children[index].1, false, None, counters)?;
                if child.summary.max.as_ref() != Some(&expected_max)
                    || child.summary.level.checked_add(1) != Some(current.summary.level)
                {
                    return Err(CoreError::InvalidRecord("directory child summary"));
                }
                current = child;
            }
        }
    }
}

pub fn directory_entries<S: ObjectRead>(
    store: &S,
    root: DirectoryStateRoot,
    counters: &mut NamespaceCounters,
) -> CoreResult<Vec<(CanonicalName, InodeId)>> {
    let mut output = Vec::new();
    visit_directory_entries(store, root, counters, |entries| {
        output.extend_from_slice(entries);
        Ok(())
    })?;
    Ok(output)
}

/// Returns one bounded ordered page strictly after `exclusive_after`.
pub fn directory_page_after<S: ObjectRead>(
    store: &S,
    root: DirectoryStateRoot,
    exclusive_after: Option<&CanonicalName>,
    max_entries: usize,
    max_bytes: usize,
    counters: &mut NamespaceCounters,
) -> CoreResult<DirectoryPage> {
    if max_entries == 0 || max_bytes == 0 {
        return Err(CoreError::ObjectLimitExceeded);
    }
    let state = load_directory_state(store, root, counters)?;
    let mut cursor =
        DirectoryEntryCursor::after(store, &state, exclusive_after, counters)?.peekable();
    let mut entries = Vec::new();
    let mut bytes = 0_usize;
    while entries.len() < max_entries {
        let Some(next) = cursor.peek() else {
            return Ok(DirectoryPage {
                entries,
                continuation: None,
            });
        };
        let width = match next {
            Ok((name, _)) => 34_usize
                .checked_add(name.as_bytes().len())
                .ok_or(CoreError::LengthOverflow)?,
            Err(_) => return Err(cursor.next().expect("peeked entry").unwrap_err()),
        };
        if bytes
            .checked_add(width)
            .is_none_or(|total| total > max_bytes)
        {
            if entries.is_empty() {
                return Err(CoreError::ObjectLimitExceeded);
            }
            break;
        }
        bytes += width;
        entries.push(cursor.next().expect("peeked entry")?);
    }
    Ok(DirectoryPage {
        continuation: entries.last().map(|entry| entry.0.clone()),
        entries,
    })
}

pub fn visit_directory_entries<S: ObjectRead>(
    store: &S,
    root: DirectoryStateRoot,
    counters: &mut NamespaceCounters,
    mut visitor: impl FnMut(&[(CanonicalName, InodeId)]) -> CoreResult<()>,
) -> CoreResult<()> {
    let state = load_directory_state(store, root, counters)?;
    let summary = walk_directory_node(
        store,
        state.mapping_root,
        true,
        None,
        None,
        counters,
        &mut visitor,
    )?;
    if summary.entries != state.entry_count || summary.level != state.tree_level {
        return Err(CoreError::InvalidRecord("directory state summary"));
    }
    Ok(())
}
