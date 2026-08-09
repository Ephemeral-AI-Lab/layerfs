//! Immutable closure snapshots and completed admission observations.

use super::ImmutablePortErrorV1;
use crate::object::TypedPhysicalObjectIdV1;
use crate::CoreResult;

/// Immutable random-readable view of a canonical, typed closure spool.
///
/// The source owns its exact index and payload carrier. `object_id_at` must be
/// O(1) or O(log N), and all methods must observe one immutable snapshot for
/// the duration of admission. No method may return a borrowed payload.
pub trait CompleteImmutableClosureReadPortV1 {
    fn object_count(&mut self) -> Result<u64, ImmutablePortErrorV1>;

    /// Side-effect-free declaration queried before admission. It must not
    /// allocate, cache, read closure metadata, or consume the source.
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64>;

    /// Exact direct storage reads performed by the immutable closure carrier.
    /// Memory-backed and synthetic ports may retain the zero default.
    fn direct_storage_read_observation(&self) -> Result<(u64, u64), ImmutablePortErrorV1> {
        Ok((0, 0))
    }

    fn object_id_at(
        &mut self,
        ordinal: u64,
    ) -> Result<TypedPhysicalObjectIdV1, ImmutablePortErrorV1>;
    fn object_len_at(&mut self, ordinal: u64) -> Result<u64, ImmutablePortErrorV1>;
    fn read_object_exact_at(
        &mut self,
        ordinal: u64,
        offset: u64,
        destination: &mut [u8],
    ) -> Result<(), ImmutablePortErrorV1>;
}

pub fn compare_closure_object_ids_v1(
    left: TypedPhysicalObjectIdV1,
    right: TypedPhysicalObjectIdV1,
) -> core::cmp::Ordering {
    typed_kind_rank(left)
        .cmp(&typed_kind_rank(right))
        .then_with(|| left.as_bytes().cmp(right.as_bytes()))
}

const fn typed_kind_rank(id: TypedPhysicalObjectIdV1) -> u8 {
    match id {
        TypedPhysicalObjectIdV1::VersionRecord(_) => 1,
        TypedPhysicalObjectIdV1::Tree(_) => 2,
        TypedPhysicalObjectIdV1::File(_) => 3,
        TypedPhysicalObjectIdV1::Symlink(_) => 4,
        TypedPhysicalObjectIdV1::Chunk(_) => 5,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdmittedClosureV1 {
    pub(super) version_record: TypedPhysicalObjectIdV1,
    pub(super) object_count: u64,
    pub(super) created_count: u64,
    pub(super) reused_count: u64,
}

impl AdmittedClosureV1 {
    pub const fn version_record(self) -> TypedPhysicalObjectIdV1 {
        self.version_record
    }

    pub const fn object_count(self) -> u64 {
        self.object_count
    }

    pub const fn created_count(self) -> u64 {
        self.created_count
    }

    pub const fn reused_count(self) -> u64 {
        self.reused_count
    }
}
