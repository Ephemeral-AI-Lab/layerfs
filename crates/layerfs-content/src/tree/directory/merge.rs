use super::diff::DirectoryEntryDiff;
use super::edit::{directory_insert, directory_remove};
use super::node::{DirectoryStateRoot, NamespaceCounters};
use super::read::{directory_lookup, StreamingDirectoryDiff};
use super::validate::load_directory_state;
use crate::file::rope::ObjectStore;
use crate::tree::inode::InodeId;
use crate::{CanonicalName, CoreError, CoreResult};

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

pub(super) fn merge_directory_entries(
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
