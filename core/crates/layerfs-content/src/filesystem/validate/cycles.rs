//! Effective-tree cycle and reachability proofs.
//!
//! A cycle can only be formed by binding a directory below itself, so only the
//! directories this operation rebinds need a subtree walk, and a build's own
//! stated bindings are the whole tree it must prove reachable exactly once.
//! Both walks are bounded by the caller's ordering-memory budget; exceeding it
//! is an explicit refusal, never a claim that the tree was proven acyclic.

use super::entries::EffectiveEntries;
use std::collections::{BTreeMap, BTreeSet};

use super::{charge_site, walk_limit, CheckedOperationInput, ValidationState, ValidationWork};
use crate::error::{ContentError, ContentResult};
use crate::object::inode_leaf::InodeKind;
use crate::object::AuthenticatedObjects;

/// Walks a rebound directory's effective subtree looking for its new parent.
///
/// A cycle can only be formed by binding a directory below itself, so only the
/// directories this operation rebinds need the walk. The work is bounded by
/// the caller's ordering-memory budget; exceeding it is an explicit refusal, never a
/// claim that the tree was proven acyclic. A directory this operation allocates
/// has no stored page yet, so the walk of it follows this operation's own
/// bindings instead.
pub(super) fn check_effective_cycles(
    reader: &dyn AuthenticatedObjects,
    checked: &CheckedOperationInput<'_>,
    unreachable: &BTreeMap<u64, ()>,
    work: &mut ValidationWork,
    state: &mut ValidationState,
) -> ContentResult<()> {
    if checked.input.base().is_none() {
        let before = work.inode_pages_read;
        let result = check_build_reachability(reader, checked, unreachable, work);
        charge_site(work, before, |sites| &mut sites.reachability);
        return result;
    }
    let cycles_before = work.inode_pages_read;
    let table = checked.topology.table;
    let mut visited = 0_usize;
    let mut rows = checked.input.directories()?;
    while let Some(update) = rows.next_row()? {
        for change in update.changes()? {
            let (_, binding) = change?;
            let Some(child) = binding else {
                continue;
            };
            let stored = match table {
                Some(table) => state.lookup_optional(reader, table, child, work)?,
                None => None,
            };
            let kind = match stored {
                Some(record) => record.kind,
                None => match checked.input.value_for(child)? {
                    Some(value) => value.kind,
                    None => continue,
                },
            };
            if kind != InodeKind::Directory {
                continue;
            }
            // A serial with no stored record has no base page to list: the walk
            // of it follows this operation's own bindings alone.
            let seed = stored.map(|record| record.content_root);
            // The walk follows the *effective* bindings: a directory this
            // operation rebinds still holds every name it already had plus the
            // changes, so a cycle formed by two changes is visible here.
            let mut pending = vec![(seed, child)];
            let mut seen: BTreeSet<u64> = BTreeSet::new();
            while let Some((base_root, serial)) = pending.pop() {
                if !seen.insert(serial) {
                    continue;
                }
                let row = checked.input.directory_for(serial)?;
                let mut entries = EffectiveEntries::new(reader, base_root, row.as_ref())?;
                while let Some((_, entry_serial)) =
                    entries.next_entry(&mut visited, walk_limit(checked.input), work)?
                {
                    visited = visited.saturating_add(1);
                    if visited > walk_limit(checked.input) {
                        return Err(ContentError::InvalidRecord("cycle check work limit"));
                    }
                    if entry_serial == update.header.parent || entry_serial == child {
                        return Err(ContentError::InvalidRecord("effective tree cycle"));
                    }
                    let entering = match table {
                        Some(table) => state.lookup_optional(reader, table, entry_serial, work)?,
                        None => None,
                    };
                    if let Some(entry) = entering {
                        if entry.kind == InodeKind::Directory {
                            pending.push((Some(entry.content_root), entry_serial));
                        }
                    } else if checked
                        .input
                        .value_for(entry_serial)?
                        .is_some_and(|value| value.kind == InodeKind::Directory)
                    {
                        pending.push((None, entry_serial));
                    }
                }
            }
        }
    }
    charge_site(work, cycles_before, |sites| &mut sites.cycles);
    Ok(())
}

/// Proves that a build's stated bindings form a tree rooted at the root.
///
/// A build has no stored page to list, so its obligation is the one the root
/// implies: every directory this operation allocates is reachable from the root
/// exactly once, which is what makes a disconnected cycle - two directories
/// binding each other with nothing binding either of them - a refusal. The walk
/// is bounded by the caller's supplied bindings, with no independent count cap.
fn check_build_reachability(
    _reader: &dyn AuthenticatedObjects,
    checked: &CheckedOperationInput<'_>,
    unreachable: &BTreeMap<u64, ()>,
    work: &mut ValidationWork,
) -> ContentResult<()> {
    // A build states its own bindings: the walk reads no base object, and the
    // entries it counts are the ones the caller supplied. A directory the same
    // batch drops is not part of the result, so walking it would charge work the
    // operation does not do - and could refuse a build for a subtree it never
    // builds.
    // The root is reached by definition: it is the walk's own starting point.
    let mut serials = checked.input.new_inodes()?;
    let mut declared: BTreeSet<u64> = BTreeSet::new();
    while let Some(serial) = serials.next_row()? {
        if serial == checked.input.root_serial() || unreachable.contains_key(&serial) {
            continue;
        }
        if checked
            .input
            .value_for(serial)?
            .is_some_and(|value| value.kind == InodeKind::Directory)
        {
            declared.insert(serial);
        }
    }
    drop(serials);
    let mut edges: BTreeMap<u64, u32> = declared.iter().map(|serial| (*serial, 0)).collect();
    let mut seen: BTreeSet<u64> = BTreeSet::new();
    let mut pending = vec![checked.input.root_serial()];
    while let Some(serial) = pending.pop() {
        if !seen.insert(serial) {
            continue;
        }
        if unreachable.contains_key(&serial) {
            continue;
        }
        let Some(row) = checked.input.directory_for(serial)? else {
            continue;
        };
        for change in row.changes()? {
            let (_, binding) = change?;
            let Some(child) = binding else {
                continue;
            };
            work.entries_examined = work.entries_examined.saturating_add(1);
            // A binding is an edge into the child. Only a directory has to be
            // reached exactly once: a regular file may be bound several times.
            if let Some(count) = edges.get_mut(&child) {
                *count = count.checked_add(1).ok_or(ContentError::LengthOverflow)?;
                if *count > 1 {
                    return Err(ContentError::InvalidRecord("multiple parents"));
                }
            }
            pending.push(child);
        }
    }
    // Every directory this operation allocates is held by a binding the root
    // reaches; one that is not is either disconnected or inside a cycle.
    for serial in declared {
        if edges.get(&serial).copied().unwrap_or(0) == 0 {
            return Err(ContentError::InvalidRecord("effective tree cycle"));
        }
    }
    Ok(())
}
