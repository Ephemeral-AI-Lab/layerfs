#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryEntryDiff {
    pub name: CanonicalName,
    pub before: Option<InodeId>,
    pub after: Option<InodeId>,
}

/// Streams changed directory entries while pruning equal persistent
/// subtrees. Unequal heights or page partitions use bounded leaf cursors.
pub fn diff_directory_entries<S: ObjectRead>(
    store: &S,
    old: DirectoryStateRoot,
    new: DirectoryStateRoot,
    mut visitor: impl FnMut(DirectoryEntryDiff) -> CoreResult<()>,
) -> CoreResult<NamespaceCounters> {
    let mut counters = NamespaceCounters::default();
    if old == new {
        return Ok(counters);
    }
    let old_state = load_directory_state(store, old, &mut counters)?;
    let new_state = load_directory_state(store, new, &mut counters)?;
    if old_state.mapping_root == new_state.mapping_root {
        if old_state.entry_count != new_state.entry_count
            || old_state.tree_level != new_state.tree_level
        {
            return Err(CoreError::InvalidRecord("directory state summary"));
        }
        return Ok(counters);
    }
    diff_directory_nodes(
        store,
        old_state.mapping_root,
        new_state.mapping_root,
        true,
        &mut counters,
        &mut visitor,
    )?;
    Ok(counters)
}

/// Merges entry-wise changes from `base -> source` onto `destination` without
/// retaining a directory inventory. Conflicting changes to the same name
/// return `None`.
pub fn merge_directory_roots<S: ObjectStore>(
    store: &mut S,
    base: DirectoryStateRoot,
    source: DirectoryStateRoot,
    destination: DirectoryStateRoot,
) -> CoreResult<Option<(DirectoryStateRoot, NamespaceCounters)>> {
    let mut counters = NamespaceCounters::default();
    if source == base || source == destination {
        return Ok(Some((destination, counters)));
    }
    if destination == base {
        return Ok(Some((source, counters)));
    }
    let base_state = load_directory_state(store, base, &mut counters)?;
    let source_state = load_directory_state(store, source, &mut counters)?;
    let destination_state = load_directory_state(store, destination, &mut counters)?;
    for state in [&source_state, &destination_state] {
        if state.profile_id != base_state.profile_id {
            return Err(CoreError::InvalidRecord("directory profile"));
        }
    }
    let mut diffs = StreamingDirectoryDiff::new(base_state.mapping_root, source_state.mapping_root);
    let mut merged = destination;
    while let Some(change) = diffs.next(store, &mut counters)? {
        let mut visits = NamespaceCounters::default();
        let current = directory_lookup(store, merged, &change.name, &mut visits)?;
        add_namespace_counters(&mut counters, visits)?;
        let selected = if change.after == change.before || change.after == current {
            current
        } else if current == change.before {
            change.after
        } else {
            return Ok(None);
        };
        if selected == current {
            continue;
        }
        if current.is_some() {
            let (next, _, visits) = directory_remove(store, merged, &change.name)?;
            add_namespace_counters(&mut counters, visits)?;
            merged = next;
        }
        if let Some(inode) = selected {
            let (next, visits) = directory_insert(store, merged, change.name, inode)?;
            add_namespace_counters(&mut counters, visits)?;
            merged = next;
        }
    }
    Ok(Some((merged, counters)))
}
