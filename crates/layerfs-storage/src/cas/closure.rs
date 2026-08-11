//! Immutable closure snapshots and completed admission observations.

#[cfg(feature = "c3-polymorphism")]
use super::{FileClosureObjectSpoolV1, FsCasErrorV1, FsCasOccupiedV1};
use super::{ImmutablePortErrorV1, OccupiedImmutableReadPortV1};
use crate::object::TypedPhysicalObjectIdV1;
use crate::{CoreError, CoreResult};

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
    fn direct_storage_read_observation(&mut self) -> Result<(u64, u64), ImmutablePortErrorV1> {
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

/// CAS-owned closure-fence adapter. Operation-metadata reads remain separate
/// from the occupied FsCas payload observation exposed by this port.
#[cfg(feature = "c3-polymorphism")]
pub(crate) struct FsCasClosureSpoolV1<'objects> {
    objects: &'objects mut FileClosureObjectSpoolV1,
    occupied: FsCasOccupiedV1,
}

#[cfg(feature = "c3-polymorphism")]
impl<'objects> FsCasClosureSpoolV1<'objects> {
    pub(crate) fn new(
        objects: &'objects mut FileClosureObjectSpoolV1,
        occupied: FsCasOccupiedV1,
    ) -> Self {
        Self { objects, occupied }
    }

    pub(crate) fn take_first_error_typed_v1(&mut self) -> Option<FsCasErrorV1> {
        // The file-backed closure-index adapter is the first owner that can
        // observe its exact read/write failure. Promote that cause before the
        // generic immutable-port error; occupied payload I/O is the fallback.
        self.objects
            .take_first_error()
            .or_else(|| self.occupied.first_error_typed_v1())
    }
}

#[cfg(feature = "c3-polymorphism")]
impl CompleteImmutableClosureReadPortV1 for FsCasClosureSpoolV1<'_> {
    fn object_count(&mut self) -> Result<u64, ImmutablePortErrorV1> {
        Ok(u64::from(self.objects.count))
    }

    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        self.objects
            .resident_memory_bound_bytes()?
            .checked_add(self.occupied.resident_memory_bound_bytes()?)
            .and_then(|bytes| {
                bytes.checked_add(
                    u64::try_from(core::mem::size_of::<Self>())
                        .map_err(|_| CoreError::IntegerOverflow)
                        .ok()?,
                )
            })
            .ok_or(CoreError::IntegerOverflow)
    }

    fn direct_storage_read_observation(&mut self) -> Result<(u64, u64), ImmutablePortErrorV1> {
        self.occupied.direct_storage_read_observation()
    }

    fn object_id_at(
        &mut self,
        ordinal: u64,
    ) -> Result<TypedPhysicalObjectIdV1, ImmutablePortErrorV1> {
        let ordinal = u32::try_from(ordinal).map_err(|_| ImmutablePortErrorV1::Failure)?;
        self.objects
            .read(ordinal)
            .map(|record| record.id)
            .map_err(|_| ImmutablePortErrorV1::Failure)
    }

    fn object_len_at(&mut self, ordinal: u64) -> Result<u64, ImmutablePortErrorV1> {
        let ordinal = u32::try_from(ordinal).map_err(|_| ImmutablePortErrorV1::Failure)?;
        let record = self
            .objects
            .read(ordinal)
            .map_err(|_| ImmutablePortErrorV1::Failure)?;
        let occupied = match self.occupied.occupied_len(record.id)? {
            Some(occupied) => occupied,
            None => {
                self.occupied
                    .retain_first_error_typed_v1(FsCasErrorV1::MissingOccupant);
                return Err(ImmutablePortErrorV1::Failure);
            }
        };
        if occupied != record.complete_len {
            self.occupied
                .retain_first_error_typed_v1(FsCasErrorV1::Integrity);
            return Err(ImmutablePortErrorV1::Failure);
        }
        Ok(occupied)
    }

    fn read_object_exact_at(
        &mut self,
        ordinal: u64,
        offset: u64,
        destination: &mut [u8],
    ) -> Result<(), ImmutablePortErrorV1> {
        let ordinal = u32::try_from(ordinal).map_err(|_| ImmutablePortErrorV1::Failure)?;
        let record = self
            .objects
            .read(ordinal)
            .map_err(|_| ImmutablePortErrorV1::Failure)?;
        let occupied = match self.occupied.occupied_len(record.id)? {
            Some(occupied) => occupied,
            None => {
                self.occupied
                    .retain_first_error_typed_v1(FsCasErrorV1::MissingOccupant);
                return Err(ImmutablePortErrorV1::Failure);
            }
        };
        if occupied != record.complete_len {
            self.occupied
                .retain_first_error_typed_v1(FsCasErrorV1::Integrity);
            return Err(ImmutablePortErrorV1::Failure);
        }
        self.occupied
            .read_occupied_exact_at(record.id, offset, destination)
    }
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
