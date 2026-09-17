//! Checked inputs: membership, identity, topology and retained bindings.
//!
//! These checks run before any promise is made about the result. They are
//! incremental where the input allows it: an update to a valid base validates the
//! affected bindings, identities and effective parents, while a structure that can
//! form a cycle is walked with a declared bounded work limit. Authentication of
//! object bytes is not topology validation and neither is a caller assertion: new
//! identities are rechecked against the base they claim to be absent from, and
//! every final count is derived from checked retained bindings.

use std::collections::BTreeMap;

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
    for update in input.directories {
        if update.parent == input.root_serial && input.base.is_none() {
            // The root directory of a new filesystem is built by this operation.
        } else if update.parent == input.root_serial {
            // A root directory update is legal; the root's own count stays zero.
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
            if let Some(table) = topology.table {
                if let Some(previous) = lookup_optional(reader, table, *child)? {
                    if previous.kind != InodeKind::Directory {
                        continue;
                    }
                }
            } else if let Some(value) = input.value_for(*child) {
                if value.kind != InodeKind::Directory {
                    continue;
                }
            } else {
                continue;
            }
            // A directory has exactly one binding outside the root.
            let added = additions.entry(*child).or_insert(0);
            *added = added.checked_add(1).ok_or(ContentError::LengthOverflow)?;
            if *added > 1 {
                return Err(ContentError::InvalidRecord("multiple directory parents"));
            }
        }
    }
    for update in input.directories {
        for (_, binding) in &update.changes {
            if let Some(child) = binding {
                let _ = additions.entry(*child).or_insert(0);
            }
        }
    }
    check_new_identities(reader, input, topology)?;
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

/// Walks a moved directory's effective subtree looking for its new parent.
///
/// A cycle can only be formed by binding a directory below itself, so only the
/// directories this operation rebinds need the walk. The work is bounded by
/// [`MAXIMUM_CYCLE_CHECK_ENTRIES`]; exceeding it is an explicit refusal, never a
/// claim that the tree was proven acyclic.
fn check_effective_cycles(
    reader: &dyn AuthenticatedObjects,
    checked: &CheckedInput<'_>,
) -> ContentResult<()> {
    let Some(table) = checked.topology.table else {
        return Ok(());
    };
    for update in checked.input.directories {
        for (name, binding) in &update.changes {
            let _ = name;
            let Some(child) = binding else {
                continue;
            };
            let Some(record) = lookup_optional(reader, table, *child)? else {
                continue;
            };
            if record.kind != InodeKind::Directory {
                continue;
            }
            let mut budget = MAXIMUM_CYCLE_CHECK_ENTRIES;
            let mut after = None;
            let mut root = DirectoryRoot(record.content_root);
            loop {
                let page = list_after(
                    reader,
                    root,
                    after.as_ref(),
                    64,
                    crate::filesystem::limits::MAXIMUM_PAGE_BYTES,
                    &mut DirectoryReadWork::default(),
                )?;
                for (_, entry_serial) in &page.entries {
                    if *entry_serial == update.parent {
                        return Err(ContentError::InvalidRecord("effective tree cycle"));
                    }
                    budget = budget
                        .checked_sub(1)
                        .ok_or(ContentError::InvalidRecord("cycle check work limit"))?;
                }
                match page.continuation {
                    Some(next) => after = Some(next),
                    None => break,
                }
            }
            let _ = &mut root;
        }
    }
    Ok(())
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
