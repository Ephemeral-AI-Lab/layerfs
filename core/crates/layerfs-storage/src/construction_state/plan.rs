//! Closed private profiles and pre-effect sequential-phase arithmetic.

use crate::error::{StorageError, StorageResult};
use layerfs_content::filesystem::rows::BindingSourceId;
use layerfs_content::filesystem::state::{GraphCapacity, GraphSubject};

use super::profile::{RECORD_BYTES, ROW_LIMIT};

#[derive(Clone, Copy)]
pub(crate) enum Plan {
    Legacy,
    ClaimsThenRoots {
        directories: u64,
        bindings: u64,
    },
    SitesThenRoots {
        directories: u64,
        bindings: u64,
        source: BindingSourceId,
    },
    SitesGraphThenRoots {
        directories: u64,
        bindings: u64,
        source: BindingSourceId,
        capacity: GraphCapacity,
    },
}

impl Plan {
    pub(crate) fn graph(
        directories: u64,
        bindings: u64,
        subject: &GraphSubject,
    ) -> StorageResult<Self> {
        // Prepared D/B limits stay independent of the selected graph capacity.
        let _ = Self::sites(directories, bindings, subject.source_id())?;
        Ok(Self::SitesGraphThenRoots {
            directories,
            bindings,
            source: subject.source_id(),
            capacity: subject.capacity(),
        })
    }

    pub(crate) const fn scratch_bytes(self) -> u64 {
        match self {
            Self::SitesGraphThenRoots { capacity, .. } => capacity.scratch_bytes(),
            _ => super::native::RESERVED_BYTES,
        }
    }

    pub(crate) fn sites(
        directories: u64,
        bindings: u64,
        source: BindingSourceId,
    ) -> StorageResult<Self> {
        // Sites and roots are sequential populations in one unchanged class.
        let _ = Self::phased(directories, bindings)?;
        let bytes = (directories * RECORD_BYTES).max(bindings * 60);
        if bytes > ROW_LIMIT * RECORD_BYTES {
            return Err(StorageError::CapacityExceeded {
                what: "construction scratch site record bytes",
                limit: ROW_LIMIT * RECORD_BYTES,
                actual: bytes,
            });
        }
        Ok(Self::SitesThenRoots {
            directories,
            bindings,
            source,
        })
    }
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
            Self::SitesThenRoots { .. } => 3,
            Self::SitesGraphThenRoots { .. } => 4,
        }
    }

    pub(crate) const fn root_limit(self) -> u64 {
        match self {
            Self::Legacy => ROW_LIMIT,
            Self::ClaimsThenRoots { directories, .. }
            | Self::SitesThenRoots { directories, .. } => directories,
            Self::SitesGraphThenRoots { directories, .. } => directories,
        }
    }
}
