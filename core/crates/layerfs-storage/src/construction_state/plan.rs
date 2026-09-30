//! The two implemented private profiles and pre-effect phase arithmetic.

use crate::error::{StorageError, StorageResult};

use super::profile::{RECORD_BYTES, ROW_LIMIT};

#[derive(Clone, Copy)]
pub(crate) enum Plan {
    Legacy,
    ClaimsThenRoots { directories: u64, bindings: u64 },
}

impl Plan {
    pub(crate) fn phased(directories: u64, bindings: u64) -> StorageResult<Self> {
        let rows = directories.max(bindings);
        if rows > ROW_LIMIT {
            return Err(StorageError::CapacityExceeded {
                what: "construction scratch phased declared rows",
                limit: ROW_LIMIT,
                actual: rows,
            });
        }
        let bytes = directories
            .checked_mul(RECORD_BYTES)
            .zip(bindings.checked_mul(32))
            .map(|(roots, claims)| roots.max(claims))
            .ok_or(StorageError::Integrity(
                "construction scratch phase arithmetic",
            ))?;
        if bytes > ROW_LIMIT * RECORD_BYTES {
            return Err(StorageError::CapacityExceeded {
                what: "construction scratch phased declared record bytes",
                limit: ROW_LIMIT * RECORD_BYTES,
                actual: bytes,
            });
        }
        Ok(Self::ClaimsThenRoots {
            directories,
            bindings,
        })
    }

    pub(crate) const fn version(self) -> u16 {
        match self {
            Self::Legacy => 1,
            Self::ClaimsThenRoots { .. } => 2,
        }
    }

    pub(crate) const fn root_limit(self) -> u64 {
        match self {
            Self::Legacy => ROW_LIMIT,
            Self::ClaimsThenRoots { directories, .. } => directories,
        }
    }
}
