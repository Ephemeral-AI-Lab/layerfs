//! Checked inputs: membership, identity, topology and retained bindings.
//!
//! These checks run before any promise is made about the result. They are
//! incremental where the input allows it: an update to a valid base validates the
//! affected bindings, identities and effective parents, while a structure that can
//! form a cycle is walked with a declared bounded work limit. Authentication of
//! object bytes is not topology validation and neither is a caller assertion: new
//! identities are rechecked against the base they claim to be absent from, and
//! every final count is derived from checked retained bindings.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{ContentError, ContentResult};
use crate::filesystem::directory::read::{list_after, DirectoryReadWork};
use crate::filesystem::identity::InodeScope;
use crate::filesystem::inode::read::{lookup_many, InodeReadWork, InodeTable};
use crate::filesystem::input::FilesystemInput;
use crate::filesystem::root::{FilesystemRoot, FilesystemRootId};
use crate::filesystem::sorted::finish::DirectoryRoot;
use crate::object::inode_leaf::{InodeKind, InodeValue};
use crate::object::{AuthenticatedObjects, ObjectId};

/// Entries the effective-tree cycle check may inspect before it refuses.
pub const MAXIMUM_CYCLE_CHECK_ENTRIES: usize = 4_096;
/// Serial demands one allocator-precondition wave may make.
pub const ALLOCATION_CHECK_BATCH: usize = 64;

/// The checked state one operation starts from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FilesystemTopology {
    /// Decoded base root, when the caller supplied one.
    pub base: Option<FilesystemRoot>,
    /// Base inode table, when the caller supplied a base root.
    pub table: Option<InodeTable>,
}

impl FilesystemTopology {
    /// Loads and checks the base root the caller named, if any.
    pub fn load(
        reader: &dyn AuthenticatedObjects,
        base: Option<FilesystemRootId>,
        scope: InodeScope,
        root_serial: u64,
    ) -> ContentResult<Self> {
        let Some(base) = base else {
            return Ok(Self {
                base: None,
                table: None,
            });
        };
        let canonical = reader.read_canonical(base.0)?;
        let root = FilesystemRoot::decode(&canonical)?;
        if root.scope() != scope {
            return Err(ContentError::ScopeMismatch {
                what: "filesystem root scope",
            });
        }
        if root.root_inode().serial() != root_serial {
            return Err(ContentError::InvalidRecord("filesystem root serial"));
        }
        Ok(Self {
            base: Some(root),
            table: Some(InodeTable {
                root: root.inode_table(),
                root_serial,
            }),
        })
    }

    /// The table a lookup should use for base records.
    pub fn table(self) -> InodeTable {
        self.table.unwrap_or(InodeTable {
            root: ObjectId::from_bytes(&[0; 32]).expect("zero identity"),
            root_serial: 0,
        })
    }
}

/// One checked operation input, bound to the base it addresses.
pub struct CheckedInput<'a> {
    /// The validated request.
    pub input: &'a FilesystemInput<'a>,
    /// Checked base state.
    pub topology: FilesystemTopology,
    /// Bindings each child serial gains inside this operation.
    pub additions: BTreeMap<u64, u64>,
    /// Bindings each child serial loses inside this operation.
    pub removals: BTreeMap<u64, u64>,
    /// Serial of every declared new identity.
    pub declared_new: Vec<u64>,
}

/// Checks membership, identity use and effective topology before any mutation.
pub fn check<'a>(
    reader: &dyn AuthenticatedObjects,
    input: &'a FilesystemInput<'a>,
) -> ContentResult<CheckedInput<'a>> {
    input.check()?;
    let topology = FilesystemTopology::load(reader, input.base, input.scope, input.root_serial)?;
    let mut additions: BTreeMap<u64, u64> = BTreeMap::new();
    let removals: BTreeMap<u64, u64> = BTreeMap::new();
    // Children with a stored record that this batch binds exactly once, by the
    // parent that binds them: the final pass decides whether that binding is the
    // one the base already has or a second parent.
    let mut by_parent: BTreeMap<u64, Vec<Vec<u8>>> = BTreeMap::new();
    // The allocator precondition is checked before anything else: a serial the
    // caller calls new must not already exist in the base it addresses.
    check_new_identities(reader, input, topology)?;
    for update in input.directories {
        if update.parent == input.root_serial && input.base.is_none() {
            // The root directory of a new filesystem is built by this operation.
        } else if update.parent == input.root_serial {
            // A root directory update is legal; the root's own count stays zero.
        } else if input.new_inodes.contains(&update.parent) {
            // A directory this operation allocates starts empty; its value must
            // still declare the directory kind it will have.
            let value = input
                .value_for(update.parent)
                .ok_or(ContentError::InvalidRecord("directory parent value"))?;
            if value.kind != InodeKind::Directory {
                return Err(ContentError::InvalidRecord("directory parent kind"));
            }
        } else if let Some(table) = topology.table {
            let record = lookup_one(reader, table, update.parent)?;
            if record.kind != InodeKind::Directory {
                return Err(ContentError::InvalidRecord("directory parent kind"));
            }
        } else {
            let value = input
                .value_for(update.parent)
                .ok_or(ContentError::InvalidRecord("directory parent value"))?;
            if value.kind != InodeKind::Directory {
                return Err(ContentError::InvalidRecord("directory parent kind"));
            }
        }
        for (name, binding) in &update.changes {
            let _ = name;
            let Some(child) = binding else {
                continue;
            };
            if *child == input.root_serial {
                return Err(ContentError::InvalidRecord("root directory binding"));
            }
            // The kind comes from the stored record for an existing inode and
            // from the caller's typed value for one this operation allocates.
            let stored = match topology.table {
                Some(table) => lookup_optional(reader, table, *child)?,
                None => None,
            };
            let previous = stored.or_else(|| input.value_for(*child));
            let previous = previous.ok_or(ContentError::InvalidRecord("binding kind"))?;
            // Only a regular file may carry several bindings, so a file that
            // already has one keeps gaining them.
            let kind = stored.map_or(previous.kind, |record| record.kind);
            if kind == InodeKind::RegularFile {
                continue;
            }
            // A same-batch duplicate is refused here, before any base record is
            // consulted: a directory or a symlink has one binding outside the
            // root and the batch already names it twice.
            let added = additions.entry(*child).or_insert(0);
            *added = added.checked_add(1).ok_or(ContentError::LengthOverflow)?;
            if *added > 1 {
                return Err(ContentError::InvalidRecord("multiple parents"));
            }
            // The batch names it once; a stored record may already own that one
            // binding, and the base name decides whether this is the same
            // binding or a second parent. The decision needs a base listing, so
            // it is deferred to one final pass over the children that have a
            // stored record.
            if stored.is_some() {
                by_parent
                    .entry(update.parent)
                    .or_default()
                    .push(name.as_bytes().to_vec());
            }
        }
    }
    check_parent_aliases(reader, input, &topology, &additions, &by_parent)?;
    for update in input.directories {
        for (_, binding) in &update.changes {
            if let Some(child) = binding {
                let _ = additions.entry(*child).or_insert(0);
            }
        }
    }
    check_root_invariants(input)?;
    if input.base.is_none() {
        for update in input.inodes {
            if update.value.kind == InodeKind::Directory
                && input.update_for(update.serial).is_none()
            {
                return Err(ContentError::InvalidRecord("directory bindings missing"));
            }
        }
    }
    let declared_new = input.new_inodes.to_vec();
    let checked = CheckedInput {
        input,
        topology,
        additions,
        removals,
        declared_new,
    };
    check_effective_cycles(reader, &checked)?;
    Ok(checked)
}

/// Refuses a stored non-file inode that keeps its base binding and gains another.
///
/// A directory or a symlink has exactly one parent. A batch that names one
/// again may only restate the binding the base already has, or move it after
/// unbinding the old name in the same final-state batch. One listing per
/// candidate parent answers "which name does the base bind it under", bounded by
/// the same page ceiling as every other directory read.
fn check_parent_aliases(
    reader: &dyn AuthenticatedObjects,
    input: &FilesystemInput<'_>,
    topology: &FilesystemTopology,
    additions: &BTreeMap<u64, u64>,
    by_parent: &BTreeMap<u64, Vec<Vec<u8>>>,
) -> ContentResult<()> {
    if by_parent.is_empty() {
        return Ok(());
    }
    let Some(table) = topology.table else {
        return Ok(());
    };
    let mut bound: BTreeMap<u64, (Vec<u8>, u64)> = BTreeMap::new();
    for (parent, names) in by_parent {
        let record = lookup_one(reader, table, *parent)?;
        let mut after = None;
        loop {
            let page = list_after(
                reader,
                DirectoryRoot(record.content_root),
                after.as_ref(),
                64,
                crate::filesystem::limits::MAXIMUM_PAGE_BYTES,
                &mut DirectoryReadWork::default(),
            )?;
            for (key, serial) in page.entries {
                let duplicated = additions.get(&serial).copied().unwrap_or(0) > 1;
                if !duplicated && names.iter().any(|name| name.as_slice() == key.as_bytes()) {
                    bound.insert(serial, (key.as_bytes().to_vec(), *parent));
                }
            }
            match page.continuation {
                Some(next) => after = Some(next),
                None => break,
            }
        }
    }
    for (parent, names) in by_parent {
        for name in names {
            let Some(child) = single_binding(input, parent, name) else {
                continue;
            };
            let Some((base_name, base_parent)) = bound.get(&child) else {
                continue;
            };
            // The batch restates the binding the base has, or it unbinds the
            // base name first, which is the one legal way to move the inode.
            let restated = base_parent == parent && base_name.as_slice() == name.as_slice();
            if !restated && base_binding_survives(input, *base_parent, base_name, child) {
                return Err(ContentError::InvalidRecord("multiple parents"));
            }
        }
    }
    Ok(())
}

/// The child one `(parent, name)` pair in this batch binds.
fn single_binding(input: &FilesystemInput<'_>, parent: &u64, name: &[u8]) -> Option<u64> {
    input.update_for(*parent).and_then(|update| {
        update
            .changes
            .iter()
            .find(|(changed, _)| changed.as_bytes() == name)
            .and_then(|(_, binding)| *binding)
    })
}

/// True when the base still binds `child` under `base_name` in `base_parent`.
fn base_binding_survives(
    input: &FilesystemInput<'_>,
    base_parent: u64,
    base_name: &[u8],
    child: u64,
) -> bool {
    match input.update_for(base_parent) {
        Some(update) => match update
            .changes
            .iter()
            .find(|(changed, _)| changed.as_bytes() == base_name)
        {
            Some((_, binding)) => *binding == Some(child),
            None => true,
        },
        None => true,
    }
}

fn check_new_identities(
    _reader: &dyn AuthenticatedObjects,
    input: &FilesystemInput<'_>,
    topology: FilesystemTopology,
) -> ContentResult<()> {
    if input.new_inodes.is_empty() {
        return Ok(());
    }
    let Some(table) = topology.table else {
        return Ok(());
    };
    for wave in input.new_inodes.chunks(ALLOCATION_CHECK_BATCH) {
        let found = lookup_many(_reader, table, wave, &mut InodeReadWork::default())?;
        if found.iter().any(Option::is_some) {
            return Err(ContentError::InvalidRecord("reused inode serial"));
        }
    }
    Ok(())
}

fn check_root_invariants(input: &FilesystemInput<'_>) -> ContentResult<()> {
    if input.base.is_none() {
        let value = input
            .value_for(input.root_serial)
            .ok_or(ContentError::InvalidRecord("root inode value"))?;
        if value.kind != InodeKind::Directory {
            return Err(ContentError::InvalidRecord("root inode kind"));
        }
    }
    for update in input.directories {
        for (_, binding) in &update.changes {
            if *binding == Some(input.root_serial) {
                return Err(ContentError::InvalidRecord("root directory binding"));
            }
        }
    }
    if input.base.is_some() && input.new_inodes.contains(&input.root_serial) {
        return Err(ContentError::InvalidRecord("root inode allocation"));
    }
    Ok(())
}

/// Walks a rebound directory's effective subtree looking for its new parent.
///
/// A cycle can only be formed by binding a directory below itself, so only the
/// directories this operation rebinds need the walk. The work is bounded by
/// [`MAXIMUM_CYCLE_CHECK_ENTRIES`]; exceeding it is an explicit refusal, never a
/// claim that the tree was proven acyclic. A directory this operation allocates
/// has no stored page yet, so the walk of it follows this operation's own
/// bindings instead.
fn check_effective_cycles(
    reader: &dyn AuthenticatedObjects,
    checked: &CheckedInput<'_>,
) -> ContentResult<()> {
    // A build has no stored page to list and therefore no subtree to walk: every
    // directory it names is one it allocates, and its bindings are the ones this
    // operation states. An update is where a rebinding can close a cycle.
    if checked.input.base.is_none() {
        return Ok(());
    }
    let table = checked.topology.table;
    for update in checked.input.directories {
        for (_, binding) in &update.changes {
            let Some(child) = binding else {
                continue;
            };
            let stored = match table {
                Some(table) => lookup_optional(reader, table, *child)?,
                None => None,
            };
            let kind = match stored {
                Some(record) => record.kind,
                None => match checked.input.value_for(*child) {
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
            let mut pending = vec![(seed, *child)];
            let mut visited = 0_usize;
            let mut seen: BTreeSet<u64> = BTreeSet::new();
            while let Some((base_root, serial)) = pending.pop() {
                if !seen.insert(serial) {
                    continue;
                }
                let changes = checked
                    .input
                    .update_for(serial)
                    .map(|update| update.changes.as_slice())
                    .unwrap_or(&[]);
                let entries = match base_root {
                    Some(content_root) => {
                        effective_entries(reader, content_root, changes, &mut visited)?
                    }
                    None => effective_entries_without_base(changes),
                };
                for (_, entry_serial) in entries {
                    if entry_serial == update.parent || entry_serial == *child {
                        return Err(ContentError::InvalidRecord("effective tree cycle"));
                    }
                    let entering = match table {
                        Some(table) => lookup_optional(reader, table, entry_serial)?,
                        None => None,
                    };
                    if let Some(entry) = entering {
                        if entry.kind == InodeKind::Directory {
                            pending.push((Some(entry.content_root), entry_serial));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// This operation's own bindings for a directory that has no stored page yet.
fn effective_entries_without_base(
    changes: &[(crate::filesystem::path::PathName, Option<u64>)],
) -> Vec<(crate::filesystem::path::PathName, u64)> {
    changes
        .iter()
        .filter_map(|(name, binding)| binding.map(|serial| (name.clone(), serial)))
        .collect()
}

/// Base entries of one directory with this operation's changes applied.
fn effective_entries(
    reader: &dyn AuthenticatedObjects,
    content_root: ObjectId,
    changes: &[(crate::filesystem::path::PathName, Option<u64>)],
    visited: &mut usize,
) -> ContentResult<Vec<(crate::filesystem::path::PathName, u64)>> {
    let mut base = Vec::new();
    let mut after = None;
    loop {
        let page = list_after(
            reader,
            DirectoryRoot(content_root),
            after.as_ref(),
            64,
            crate::filesystem::limits::MAXIMUM_PAGE_BYTES,
            &mut DirectoryReadWork::default(),
        )?;
        base.extend(page.entries.iter().cloned());
        *visited = visited.saturating_add(page.entries.len());
        if *visited > MAXIMUM_CYCLE_CHECK_ENTRIES {
            return Err(ContentError::InvalidRecord("cycle check work limit"));
        }
        match page.continuation {
            Some(next) => after = Some(next),
            None => break,
        }
    }
    let mut merged = Vec::with_capacity(base.len() + changes.len());
    let mut index = 0_usize;
    for (name, serial) in base {
        while index < changes.len() && changes[index].0 < name {
            if let Some(binding) = changes[index].1 {
                merged.push((changes[index].0.clone(), binding));
            }
            index += 1;
        }
        if index < changes.len() && changes[index].0 == name {
            if let Some(binding) = changes[index].1 {
                merged.push((name, binding));
            }
            index += 1;
        } else {
            merged.push((name, serial));
        }
    }
    while index < changes.len() {
        if let Some(binding) = changes[index].1 {
            merged.push((changes[index].0.clone(), binding));
        }
        index += 1;
    }
    Ok(merged)
}

fn lookup_one(
    reader: &dyn AuthenticatedObjects,
    table: InodeTable,
    serial: u64,
) -> ContentResult<InodeValue> {
    lookup_optional(reader, table, serial)?.ok_or(ContentError::InvalidRecord("missing base inode"))
}

fn lookup_optional(
    reader: &dyn AuthenticatedObjects,
    table: InodeTable,
    serial: u64,
) -> ContentResult<Option<InodeValue>> {
    let found = lookup_many(reader, table, &[serial], &mut InodeReadWork::default())?;
    Ok(found.into_iter().next().flatten())
}
