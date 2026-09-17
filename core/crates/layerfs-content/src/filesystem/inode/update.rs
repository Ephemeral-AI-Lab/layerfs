//! Final typed inode records and tombstones straight into inline leaves.
//!
//! A change row is a typed final value or an absence. There is no intermediate
//! inode-record object, no synthetic identity and no encode/store/reread cycle:
//! the sorted engine writes the same 73-byte value C2 pools.

use crate::error::ContentResult;
use crate::filesystem::objects::FilesystemObjects;
use crate::filesystem::sorted::finish::apply_inode_changes;
use crate::filesystem::sorted::page::MAXIMUM_SCRATCH_BYTES;
use crate::filesystem::sorted::SortedWork;
use crate::object::inode_leaf::InodeValue;
use crate::object::ObjectId;

/// One final inode change.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InodeChange {
    /// A typed final value for this serial.
    Value {
        /// Inode serial.
        serial: u64,
        /// Final typed value.
        value: InodeValue,
    },
    /// This serial is absent from the new filesystem.
    Removal {
        /// Inode serial.
        serial: u64,
    },
}

impl InodeChange {
    /// The serial this change addresses.
    pub const fn serial(self) -> u64 {
        match self {
            Self::Value { serial, .. } | Self::Removal { serial } => serial,
        }
    }

    /// The final value, when this is not a removal.
    pub const fn value(self) -> Option<InodeValue> {
        match self {
            Self::Value { value, .. } => Some(value),
            Self::Removal { .. } => None,
        }
    }
}

/// Applies strictly sorted unique final inode changes to an optional base table.
pub fn apply_inode_values(
    objects: &mut FilesystemObjects<'_>,
    base: Option<ObjectId>,
    changes: impl Iterator<Item = ContentResult<(u64, Option<InodeValue>)>>,
    scratch_limit: usize,
) -> ContentResult<(ObjectId, SortedWork)> {
    apply_inode_changes(objects, base, changes, scratch_limit)
}

/// Applies typed changes with the default operation scratch ceiling.
pub fn apply_changes(
    objects: &mut FilesystemObjects<'_>,
    base: Option<ObjectId>,
    changes: &[InodeChange],
) -> ContentResult<(ObjectId, SortedWork)> {
    apply_inode_values(
        objects,
        base,
        changes
            .iter()
            .map(|change| Ok((change.serial(), change.value()))),
        MAXIMUM_SCRATCH_BYTES,
    )
}

/// Builds a complete inode table from strictly sorted typed values.
pub fn build_table(
    objects: &mut FilesystemObjects<'_>,
    rows: impl Iterator<Item = ContentResult<(u64, InodeValue)>>,
) -> ContentResult<(ObjectId, SortedWork)> {
    apply_inode_values(
        objects,
        None,
        rows.map(|row| row.map(|(serial, value)| (serial, Some(value)))),
        MAXIMUM_SCRATCH_BYTES,
    )
}

/// Checks one requested inode change list for order and duplicate keys.
pub fn check_change_order(changes: &[InodeChange]) -> ContentResult<()> {
    if changes
        .windows(2)
        .any(|pair| pair[0].serial() >= pair[1].serial())
    {
        return Err(crate::error::ContentError::NonCanonicalOrdering);
    }
    Ok(())
}
