//! File-level structural copy-on-write ownership.
//!
//! Range update mechanics live in `content::update`; this module is the home
//! for file-node COW structures as that surface grows beyond directory-tree
//! replacement.

use crate::identity::{LogicalFileIdentityV1, PhysicalFileIdV1};
use crate::{CoreError, CoreResult};

/// Authenticated file root used as the immutable base of one COW update.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthenticatedBaseFileV1 {
    pub(crate) identity: LogicalFileIdentityV1,
    pub(crate) physical_file: PhysicalFileIdV1,
    pub(crate) mode: u16,
    pub(crate) chunk_count: u32,
}

impl AuthenticatedBaseFileV1 {
    pub const fn new(
        identity: LogicalFileIdentityV1,
        physical_file: PhysicalFileIdV1,
        mode: u16,
        chunk_count: u32,
    ) -> Self {
        Self {
            identity,
            physical_file,
            mode,
            chunk_count,
        }
    }

    pub const fn identity(self) -> LogicalFileIdentityV1 {
        self.identity
    }

    pub const fn chunk_count(self) -> u32 {
        self.chunk_count
    }

    pub const fn physical_file(self) -> PhysicalFileIdV1 {
        self.physical_file
    }

    pub const fn mode(self) -> u16 {
        self.mode
    }
}

/// Half-open byte range replaced by one file COW update.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UpdateRangeV1 {
    pub(crate) start: u64,
    pub(crate) end: u64,
}

impl UpdateRangeV1 {
    pub fn new(start: u64, end: u64, base_len: u64) -> CoreResult<Self> {
        if start > end || end > base_len {
            return Err(CoreError::RangeResyncFailed);
        }
        Ok(Self { start, end })
    }

    pub const fn start(self) -> u64 {
        self.start
    }

    pub const fn end(self) -> u64 {
        self.end
    }

    pub const fn len(self) -> u64 {
        self.end - self.start
    }

    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }
}
