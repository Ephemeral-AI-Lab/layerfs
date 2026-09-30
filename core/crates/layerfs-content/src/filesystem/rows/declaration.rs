//! Exact declared spool shape and prospective private-format admission.

use super::binding::check_name_totals;
use crate::error::{ContentError, ContentResult};

pub(super) const HEADER_BYTES: u64 = 48;
pub(super) const FORMAT_VERSION: u64 = 2;
pub(super) const CHECKPOINT_BINDINGS: u64 = 16;

/// Declared sequences and exact wire binding totals of a receive spool.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpoolDeclaration {
    /// Directory rows in parent order.
    pub directories: usize,
    /// Typed values in serial order.
    pub inodes: usize,
    /// Fresh serials in serial order.
    pub fresh: usize,
    /// Final changed names across all directory rows.
    pub bindings: u64,
    /// Sum of wire framing10 plus full name bytes, per binding.
    pub wire_name_bytes: u64,
}

impl SpoolDeclaration {
    /// Validates the shape without touching a private file.
    pub fn check(&self) -> ContentResult<()> {
        table_bytes(self.directories, self.inodes, self.fresh)?;
        check_name_totals(self.bindings, self.wire_name_bytes)?;
        if (self.directories == 0 && self.bindings != 0) || self.fresh > self.inodes {
            return Err(ContentError::InvalidRecord("spool declaration"));
        }
        Ok(())
    }

    /// Conservative complete size before the per-directory distribution is read.
    /// The first checkpoint of every nonempty directory is implicit.
    pub fn required_bytes_upper(&self) -> ContentResult<u64> {
        self.check()?;
        let table = table_bytes(self.directories, self.inodes, self.fresh)?;
        let checkpoints = checkpoint_count(self.bindings)
            .checked_mul(8)
            .ok_or(ContentError::LengthOverflow)?;
        let names = self
            .wire_name_bytes
            .checked_sub(self.bindings)
            .ok_or(ContentError::LengthOverflow)?;
        let inodes = u64::try_from(self.inodes)
            .ok()
            .and_then(|count| count.checked_mul(65))
            .ok_or(ContentError::LengthOverflow)?;
        table
            .checked_add(checkpoints)
            .and_then(|bytes| bytes.checked_add(names))
            .and_then(|bytes| bytes.checked_add(inodes))
            .ok_or(ContentError::LengthOverflow)
    }
}

pub(super) fn table_bytes(directories: usize, inodes: usize, fresh: usize) -> ContentResult<u64> {
    directories
        .checked_add(inodes)
        .and_then(|count| count.checked_add(fresh))
        .and_then(|count| u64::try_from(count).ok())
        .and_then(|count| count.checked_mul(super::SPOOL_SLOT_BYTES))
        .and_then(|bytes| bytes.checked_add(HEADER_BYTES))
        .ok_or(ContentError::LengthOverflow)
}

pub(super) const fn checkpoint_count(bindings: u64) -> u64 {
    if bindings == 0 {
        0
    } else {
        (bindings - 1) / CHECKPOINT_BINDINGS
    }
}
