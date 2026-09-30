//! Effective-tree cycle and reachability proofs over scalar bindings.
//!
//! Base listing and changed-name windows are bounded independently of directory
//! width. The existing declared/seen/frontier graph state remains admitted by the
//! ordering profile; its paged replacement is a separate responsibility.

use std::collections::{BTreeMap, BTreeSet};

use super::binding::{selected, Bindings, Headers};
use super::effective::EffectiveEntries;
use super::{charge_site, walk_limit, CheckedBindingInput, ValidationState, ValidationWork};
use crate::error::{ContentError, ContentResult};
use crate::object::inode_leaf::InodeKind;
use crate::object::AuthenticatedObjects;

/// Walks each rebound directory's effective subtree looking for its new parent.
pub(super) fn check_effective_cycles(
    reader: &dyn AuthenticatedObjects,
    checked: &CheckedBindingInput<'_>,
    unreachable: &BTreeMap<u64, ()>,
    work: &mut ValidationWork,
    state: &mut ValidationState,
) -> ContentResult<()> {
    if checked.input.base().is_none() {
        let before = work.inode_pages_read;
        let result = check_build_reachability(checked, unreachable, work);
        charge_site(work, before, |sites| &mut sites.reachability);
        return result;
    }
    let cycles_before = work.inode_pages_read;
    let table = checked.topology.table;
    let mut visited = 0usize;
    let mut rows = Headers::new(checked.input)?;
    while let Some(header) = rows.next()? {
        let mut bindings = Bindings::new(checked.input, header)?;
        while let Some((_, binding)) = bindings.next()? {
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
            let seed = stored.map(|record| record.content_root);
            let mut pending = vec![(seed, child)];
            let mut seen = BTreeSet::new();
            while let Some((base_root, serial)) = pending.pop() {
                if !seen.insert(serial) {
                    continue;
                }
                let mut entries = EffectiveEntries::new(reader, checked.input, serial, base_root)?;
                while let Some((_, entry_serial)) =
                    entries.next(&mut visited, walk_limit(checked.input), work)?
                {
                    if entry_serial == header.parent() || entry_serial == child {
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

/// Proves that a build's stated directory bindings are reached from its root.
fn check_build_reachability(
    checked: &CheckedBindingInput<'_>,
    unreachable: &BTreeMap<u64, ()>,
    work: &mut ValidationWork,
) -> ContentResult<()> {
    let mut serials = checked.input.new_inodes()?;
    let mut declared = BTreeSet::new();
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
    let mut seen = BTreeSet::new();
    let mut pending = vec![checked.input.root_serial()];
    while let Some(serial) = pending.pop() {
        if !seen.insert(serial) || unreachable.contains_key(&serial) {
            continue;
        }
        let Some(header) = selected(checked.input, serial)? else {
            continue;
        };
        let mut bindings = Bindings::new(checked.input, header)?;
        while let Some((_, child)) = bindings.next()? {
            let Some(child) = child else {
                continue;
            };
            work.entries_examined = work.entries_examined.saturating_add(1);
            if let Some(count) = edges.get_mut(&child) {
                *count = count.checked_add(1).ok_or(ContentError::LengthOverflow)?;
                if *count > 1 {
                    return Err(ContentError::InvalidRecord("multiple parents"));
                }
            }
            pending.push(child);
        }
    }
    for serial in declared {
        if edges.get(&serial).copied().unwrap_or(0) == 0 {
            return Err(ContentError::InvalidRecord("effective tree cycle"));
        }
    }
    Ok(())
}
