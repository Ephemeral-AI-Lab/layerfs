fn diff_directory_nodes<S: ObjectRead>(
    store: &S,
    old: ObjectId,
    new: ObjectId,
    root: bool,
    counters: &mut NamespaceCounters,
    visitor: &mut impl FnMut(DirectoryEntryDiff) -> CoreResult<()>,
) -> CoreResult<()> {
    if old == new {
        return Ok(());
    }
    let old_node = load_directory_node_shallow(store, old, root, None, counters)?;
    let new_node = load_directory_node_shallow(store, new, root, None, counters)?;
    match (&old_node.node, &new_node.node) {
        (
            DirectoryNodeV1::Leaf { entries: old, .. },
            DirectoryNodeV1::Leaf { entries: new, .. },
        ) => merge_directory_entries(
            old.iter().cloned().map(Ok),
            new.iter().cloned().map(Ok),
            visitor,
        ),
        (
            DirectoryNodeV1::Branch {
                level: old_level,
                children: old_children,
                ..
            },
            DirectoryNodeV1::Branch {
                level: new_level,
                children: new_children,
                ..
            },
        ) if old_level == new_level => diff_directory_children(
            store,
            *old_level,
            old_children,
            new_children,
            counters,
            visitor,
        ),
        _ => {
            let mut old_counters = NamespaceCounters::default();
            let mut new_counters = NamespaceCounters::default();
            let result = merge_directory_entries(
                DirectoryEntryCursor::new(store, old, root, &mut old_counters),
                DirectoryEntryCursor::new(store, new, root, &mut new_counters),
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

fn diff_directory_children<S: ObjectRead>(
    store: &S,
    level: u8,
    old: &[(CanonicalName, ObjectId)],
    new: &[(CanonicalName, ObjectId)],
    counters: &mut NamespaceCounters,
    visitor: &mut impl FnMut(DirectoryEntryDiff) -> CoreResult<()>,
) -> CoreResult<()> {
    let child_level = level
        .checked_sub(1)
        .ok_or(CoreError::InvalidRecord("directory child summary"))?;
    let (mut old_index, mut new_index) = (0_usize, 0_usize);
    while old_index < old.len() && new_index < new.len() {
        if old[old_index].0 == new[new_index].0 {
            diff_directory_nodes(
                store,
                old[old_index].1,
                new[new_index].1,
                false,
                counters,
                visitor,
            )?;
            old_index += 1;
            new_index += 1;
            continue;
        }
        let (old_stop, new_stop) = next_directory_boundary(old, new, old_index, new_index)
            .unwrap_or((old.len() - 1, new.len() - 1));
        let mut old_counters = NamespaceCounters::default();
        let mut new_counters = NamespaceCounters::default();
        let result = merge_directory_entries(
            DirectoryEntryCursor::from_children(
                store,
                &old[old_index..=old_stop],
                child_level,
                &mut old_counters,
            ),
            DirectoryEntryCursor::from_children(
                store,
                &new[new_index..=new_stop],
                child_level,
                &mut new_counters,
            ),
            visitor,
        );
        counters.nodes_read = counters
            .nodes_read
            .checked_add(old_counters.nodes_read)
            .and_then(|value| value.checked_add(new_counters.nodes_read))
            .ok_or(CoreError::LengthOverflow)?;
        result?;
        old_index = old_stop + 1;
        new_index = new_stop + 1;
    }
    if old_index < old.len() {
        let mut old_counters = NamespaceCounters::default();
        merge_directory_entries(
            DirectoryEntryCursor::from_children(
                store,
                &old[old_index..],
                child_level,
                &mut old_counters,
            ),
            std::iter::empty(),
            visitor,
        )?;
        counters.nodes_read = counters
            .nodes_read
            .checked_add(old_counters.nodes_read)
            .ok_or(CoreError::LengthOverflow)?;
    }
    if new_index < new.len() {
        let mut new_counters = NamespaceCounters::default();
        merge_directory_entries(
            std::iter::empty(),
            DirectoryEntryCursor::from_children(
                store,
                &new[new_index..],
                child_level,
                &mut new_counters,
            ),
            visitor,
        )?;
        counters.nodes_read = counters
            .nodes_read
            .checked_add(new_counters.nodes_read)
            .ok_or(CoreError::LengthOverflow)?;
    }
    Ok(())
}

fn next_directory_boundary(
    old: &[(CanonicalName, ObjectId)],
    new: &[(CanonicalName, ObjectId)],
    old_start: usize,
    new_start: usize,
) -> Option<(usize, usize)> {
    for (old_index, old_child) in old.iter().enumerate().skip(old_start) {
        if let Some(new_index) = new
            .iter()
            .enumerate()
            .skip(new_start)
            .find_map(|(index, child)| (child.0 == old_child.0).then_some(index))
        {
            return Some((old_index, new_index));
        }
    }
    None
}
