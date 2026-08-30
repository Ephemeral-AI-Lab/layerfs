use super::codec::DirectoryNodeV1;
use super::edit::{directory_insert, directory_remove};
use super::node::{DirectoryStateRoot, NamespaceCounters};
use super::read::{
    directory_lookup, load_directory_node_shallow, DirectoryEntryCursor, StreamingDirectoryDiff,
};
use super::validate::load_directory_state;
use crate::file::rope::{ObjectRead, ObjectStore};
use crate::tree::inode::InodeId;
use crate::{CanonicalName, CoreError, CoreResult, ObjectId};

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

pub(crate) fn reconcile_directory_roots<S: ObjectStore>(
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
    let mut reconciled = destination;
    while let Some(change) = diffs.next(store, &mut counters)? {
        let mut visits = NamespaceCounters::default();
        let current = directory_lookup(store, reconciled, &change.name, &mut visits)?;
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
            let (next, _, visits) = directory_remove(store, reconciled, &change.name)?;
            add_namespace_counters(&mut counters, visits)?;
            reconciled = next;
        }
        if let Some(inode) = selected {
            let (next, visits) = directory_insert(store, reconciled, change.name, inode)?;
            add_namespace_counters(&mut counters, visits)?;
            reconciled = next;
        }
    }
    Ok(Some((reconciled, counters)))
}

fn add_namespace_counters(
    target: &mut NamespaceCounters,
    source: NamespaceCounters,
) -> CoreResult<()> {
    target.nodes_read = target
        .nodes_read
        .checked_add(source.nodes_read)
        .ok_or(CoreError::LengthOverflow)?;
    target.nodes_created = target
        .nodes_created
        .checked_add(source.nodes_created)
        .ok_or(CoreError::LengthOverflow)?;
    Ok(())
}

fn merge_directory_entries(
    old: impl Iterator<Item = CoreResult<(CanonicalName, InodeId)>>,
    new: impl Iterator<Item = CoreResult<(CanonicalName, InodeId)>>,
    visitor: &mut impl FnMut(DirectoryEntryDiff) -> CoreResult<()>,
) -> CoreResult<()> {
    let mut old = old;
    let mut new = new;
    let mut old_entry = old.next().transpose()?;
    let mut new_entry = new.next().transpose()?;
    loop {
        match (old_entry.take(), new_entry.take()) {
            (None, None) => return Ok(()),
            (Some((old_name, before)), Some((new_name, after))) if old_name == new_name => {
                if before != after {
                    visitor(DirectoryEntryDiff {
                        name: old_name,
                        before: Some(before),
                        after: Some(after),
                    })?;
                }
                old_entry = old.next().transpose()?;
                new_entry = new.next().transpose()?;
            }
            (Some((old_name, before)), Some((new_name, after))) if old_name < new_name => {
                visitor(DirectoryEntryDiff {
                    name: old_name,
                    before: Some(before),
                    after: None,
                })?;
                old_entry = old.next().transpose()?;
                new_entry = Some((new_name, after));
            }
            (Some((old_name, before)), Some((new_name, after))) => {
                visitor(DirectoryEntryDiff {
                    name: new_name,
                    before: None,
                    after: Some(after),
                })?;
                old_entry = Some((old_name, before));
                new_entry = new.next().transpose()?;
            }
            (Some((old_name, before)), None) => {
                visitor(DirectoryEntryDiff {
                    name: old_name,
                    before: Some(before),
                    after: None,
                })?;
                old_entry = old.next().transpose()?;
            }
            (None, Some((new_name, after))) => {
                visitor(DirectoryEntryDiff {
                    name: new_name,
                    before: None,
                    after: Some(after),
                })?;
                new_entry = new.next().transpose()?;
            }
        }
    }
}
