//! Private root extraction and exact-range read coordinators.
//!
//! Root traversal, file-range orchestration, and concrete occupied-object
//! reading have separate owners. None is a public SDK.

use crate::cas::FsCasErrorV1;
use crate::format::MAX_PATH_BYTES;
use crate::identity::COMPARISON_WINDOW_BYTES;
use crate::{CoreError, CoreResult};

pub(crate) mod extraction;
mod object_reader;
mod range;

#[cfg(test)]
pub(crate) use extraction::read_file_range_impl_v1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReadSinkErrorV1 {
    Refused,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReadKindV1 {
    FullExtraction,
    ExactRange,
}

/// Exact private read/extraction failure. FsCas failures are never flattened
/// into a generic source or sink error at this operation boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReadOperationErrorV1 {
    Core(CoreError),
    FsCas(FsCasErrorV1),
    Sink(ReadSinkErrorV1),
}

impl ReadOperationErrorV1 {
    pub(crate) const fn into_fscas_v1(self) -> FsCasErrorV1 {
        match self {
            Self::Core(error) => FsCasErrorV1::Core(error),
            Self::FsCas(error) => error,
            Self::Sink(ReadSinkErrorV1::Refused) => FsCasErrorV1::Core(CoreError::SinkRefused),
        }
    }

    pub(crate) fn dominated_by_fscas_v1(self, dominant: FsCasErrorV1) -> Self {
        Self::FsCas(self.into_fscas_v1().dominated_by_v1(dominant))
    }

    pub(crate) fn retain_terminal_v1(current: Option<Self>, candidate: Self) -> Option<Self> {
        match (current, candidate) {
            (None, candidate) => Some(candidate),
            (Some(first), Self::FsCas(dominant))
                if dominant.has_cleanup_or_invalidation_dominance_v1() =>
            {
                Some(first.dominated_by_fscas_v1(dominant))
            }
            (Some(first), _) => Some(first),
        }
    }
}

impl From<CoreError> for ReadOperationErrorV1 {
    fn from(error: CoreError) -> Self {
        Self::Core(error)
    }
}

impl From<FsCasErrorV1> for ReadOperationErrorV1 {
    fn from(error: FsCasErrorV1) -> Self {
        Self::FsCas(error)
    }
}

/// Transactional bounded consumer for private extraction bytes.
///
/// `finish_read` is the only success boundary. A sink that exposes data
/// before that boundary owns the consequences of its own non-transactional
/// behavior; LayerFS always invokes `abort_read` after a later failure.
pub(crate) trait ReadSinkV1 {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64>;
    fn begin_read(&mut self, kind: ReadKindV1) -> Result<(), ReadSinkErrorV1>;
    fn begin_file(
        &mut self,
        path: &[u8],
        mode: u16,
        logical_len: u64,
        selected_offset: u64,
        selected_len: u64,
    ) -> Result<(), ReadSinkErrorV1>;
    fn write_file_bytes(&mut self, bytes: &[u8]) -> Result<(), ReadSinkErrorV1>;
    fn finish_file(&mut self) -> Result<(), ReadSinkErrorV1>;
    fn finish_read(&mut self, verification_digest: [u8; 32]) -> Result<(), ReadSinkErrorV1>;
    fn abort_read(&mut self);
}

pub(crate) struct ReadBuffersV1<'a> {
    pub(crate) comparison: &'a mut [u8; COMPARISON_WINDOW_BYTES],
    pub(crate) path: &'a mut [u8; MAX_PATH_BYTES],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ReadResultV1 {
    kind: ReadKindV1,
    verification_digest: [u8; 32],
    payload_bytes: u64,
    files: u64,
    directories: u64,
    symlinks: u64,
    ranges: u64,
    objects_traversed: u64,
    closure_direct_bytes: u64,
    closure_direct_calls: u64,
    metadata_direct_bytes: u64,
    metadata_direct_calls: u64,
    payload_direct_bytes: u64,
    payload_direct_calls: u64,
}

impl ReadResultV1 {
    pub(crate) const fn kind(self) -> ReadKindV1 {
        self.kind
    }

    pub(crate) const fn verification_digest(self) -> [u8; 32] {
        self.verification_digest
    }

    pub(crate) const fn payload_bytes(self) -> u64 {
        self.payload_bytes
    }

    pub(crate) const fn files(self) -> u64 {
        self.files
    }

    pub(crate) const fn directories(self) -> u64 {
        self.directories
    }

    pub(crate) const fn symlinks(self) -> u64 {
        self.symlinks
    }

    pub(crate) const fn ranges(self) -> u64 {
        self.ranges
    }

    pub(crate) const fn objects_traversed(self) -> u64 {
        self.objects_traversed
    }

    pub(crate) const fn closure_direct_bytes(self) -> u64 {
        self.closure_direct_bytes
    }

    pub(crate) const fn closure_direct_calls(self) -> u64 {
        self.closure_direct_calls
    }

    pub(crate) const fn metadata_direct_bytes(self) -> u64 {
        self.metadata_direct_bytes
    }

    pub(crate) const fn metadata_direct_calls(self) -> u64 {
        self.metadata_direct_calls
    }

    pub(crate) const fn payload_direct_bytes(self) -> u64 {
        self.payload_direct_bytes
    }

    pub(crate) const fn payload_direct_calls(self) -> u64 {
        self.payload_direct_calls
    }

    pub(crate) const fn direct_fscas_bytes(self) -> u64 {
        self.closure_direct_bytes + self.metadata_direct_bytes + self.payload_direct_bytes
    }

    pub(crate) const fn direct_fscas_calls(self) -> u64 {
        self.closure_direct_calls + self.metadata_direct_calls + self.payload_direct_calls
    }
}
