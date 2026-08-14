//! Frozen `ELSPACK1` construction and validation.
//!
//! Object records are appended in caller discovery order. Only fixed-size
//! index metadata enters the caller-provided bounded spool, which is sorted
//! first by typed key for emission and then by physical offset for the
//! independent validation pass. The engine never owns a whole pack, object
//! population, or payload-sized allocation.

use core::cmp::Ordering;

use crate::format::{
    validate_physical_object_len, PhysicalObjectKindV1, MAX_PHYSICAL_OBJECT_BYTES,
};
use crate::identity::{
    FramedHasherV1, ObjectChecksumV1, PackIdV1, PhysicalChunkIdV1, PhysicalFileIdV1,
    PhysicalSymlinkIdV1, PhysicalTreeIdV1, PhysicalVersionRecordIdV1, ProfileId,
    COMPARISON_WINDOW_BYTES, DIGEST_BYTES, IDENTITY_HASHER_BYTES_V1, TAG_OBJECT_CHECKSUM, TAG_PACK,
};
#[cfg(any(test, feature = "operation-polymorphism"))]
use crate::limits::ResourceLedgerV1;
use crate::limits::{
    CounterFieldV1, MemoryComponentV1, ObservationScopeV1, OperationCountersV1,
    OperationMemoryPlanV1, OperationReservationV1, OptionalU64ObservationV1,
};
use crate::object::{
    decode_physical_object_from_port_v1, DiscardStrongEdgesV1, PhysicalObjectReadPortV1,
    TypedPhysicalObjectIdV1,
};
use crate::profile::ProfileSpecV1;
use crate::{CoreError, CoreResult};

#[cfg(feature = "operation-polymorphism")]
use crate::cas::FsPackAdmissionOutcomeV1;

#[cfg(feature = "operation-polymorphism")]
mod complete_writer;
#[cfg(feature = "operation-polymorphism")]
mod operation_index;

#[cfg(feature = "operation-polymorphism")]
pub(crate) use complete_writer::DirectPackSinkV1;
#[cfg(feature = "operation-polymorphism")]
pub(crate) use operation_index::FilePackIndexSpoolV1;

pub const PACK_HEADER_BYTES: u64 = 64;
pub const PACK_INDEX_ENTRY_BYTES: u64 = 80;
pub const PACK_TRAILER_BYTES: u64 = 80;
pub const MAX_PACK_BYTES: u64 = 67_108_864;
pub const MAX_PACK_RECORDS: u64 = 466_032;
pub const MAX_PACK_INDEX_BYTES: u64 = 37_282_560;

const PACK_MAGIC: &[u8; 8] = b"ELSPACK1";
const PACK_TRAILER_MAGIC: &[u8; 8] = b"ELSPEND1";
const OBJECT_HEADER_BYTES: u64 = 52;

/// Pack-owned result of a complete bounded carrier sequence. Lifecycle sees
/// only this immutable summary, never a concrete writer or carrier handle.
#[cfg(feature = "operation-polymorphism")]
#[derive(Clone, Copy)]
pub(crate) struct CompletedPackSetV1 {
    pub(crate) last_sealed: SealedPackV1,
    pub(crate) last_outcome: FsPackAdmissionOutcomeV1,
    pub(crate) carrier_count: u32,
    pub(crate) carriers_installed: u32,
    pub(crate) carriers_reused: u32,
    pub(crate) installed_residue_bytes: u64,
    pub(crate) index_spool_bytes: OptionalU64ObservationV1,
}

#[cfg(feature = "operation-polymorphism")]
impl CompletedPackSetV1 {
    pub(crate) const fn last_sealed(self) -> SealedPackV1 {
        self.last_sealed
    }

    pub(crate) const fn last_outcome(self) -> FsPackAdmissionOutcomeV1 {
        self.last_outcome
    }

    pub(crate) const fn carrier_count(self) -> u32 {
        self.carrier_count
    }

    pub(crate) const fn carriers_installed(self) -> u32 {
        self.carriers_installed
    }

    pub(crate) const fn carriers_reused(self) -> u32 {
        self.carriers_reused
    }

    pub(crate) const fn installed_residue_bytes(self) -> u64 {
        self.installed_residue_bytes
    }

    pub(crate) const fn index_spool_bytes(self) -> OptionalU64ObservationV1 {
        self.index_spool_bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackPortErrorV1 {
    Failure,
    Cancelled,
    Deadline,
    WorkExhausted,
}

/// Random-readable transaction-private pack bytes.
pub trait PackReadPortV1 {
    /// Maximum transient userspace memory retained by this adapter. Durable
    /// pack bytes are carrier storage; caches and I/O windows are resident.
    /// This query must be side-effect-free and perform no I/O.
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64>;
    fn len(&mut self) -> Result<u64, PackPortErrorV1>;
    fn is_empty(&mut self) -> Result<bool, PackPortErrorV1> {
        self.len().map(|len| len == 0)
    }
    fn read_exact_at(&mut self, offset: u64, destination: &mut [u8])
        -> Result<(), PackPortErrorV1>;
}

/// A pack remains private until `seal_private` succeeds after independent
/// validation. `abort_private` must make any partial bytes unreachable.
pub trait PrivatePackPortV1: PackReadPortV1 {
    fn begin_private(&mut self, exact_len: u64) -> Result<(), PackPortErrorV1>;
    fn append(&mut self, bytes: &[u8]) -> Result<(), PackPortErrorV1>;
    fn seal_private(&mut self, id: PackIdV1) -> Result<(), PackPortErrorV1>;
    fn abort_private(&mut self);
}

/// Bounded random-read source for direct-to-pack construction.
///
/// The source owns the immutable object population. LayerFS observes only
/// fixed-size metadata and copies one caller-owned comparison window at a
/// time; it never borrows or materializes the complete closure payload set.
pub trait PackObjectSourceV1 {
    /// Maximum userspace-resident bytes retained by this source while the
    /// operation is active. This declaration is charged before any payload
    /// read or private pack output begins.
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64>;
    /// Immutable count declaration. Like the residency declaration, this is
    /// a pure pre-admission query and may not allocate, cache, or read object
    /// payload/metadata. Later indexed reads are bounded by this count.
    fn declared_object_count(&self) -> CoreResult<u32>;
    fn object_id(&mut self, ordinal: u32) -> Result<TypedPhysicalObjectIdV1, PackPortErrorV1>;
    fn object_len(&mut self, ordinal: u32) -> Result<u64, PackPortErrorV1>;
    fn read_object_exact_at(
        &mut self,
        ordinal: u32,
        offset: u64,
        destination: &mut [u8],
    ) -> Result<(), PackPortErrorV1>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackIndexEntryV1 {
    id: TypedPhysicalObjectIdV1,
    absolute_offset: u64,
    object_len: u32,
    object_checksum: ObjectChecksumV1,
}

impl PackIndexEntryV1 {
    pub(crate) const fn from_validated_parts(
        id: TypedPhysicalObjectIdV1,
        absolute_offset: u64,
        object_len: u32,
        object_checksum: ObjectChecksumV1,
    ) -> Self {
        Self {
            id,
            absolute_offset,
            object_len,
            object_checksum,
        }
    }

    pub const fn id(self) -> TypedPhysicalObjectIdV1 {
        self.id
    }

    pub const fn absolute_offset(self) -> u64 {
        self.absolute_offset
    }

    pub const fn object_len(self) -> u32 {
        self.object_len
    }

    pub const fn object_checksum(self) -> ObjectChecksumV1 {
        self.object_checksum
    }

    pub fn compare_key(&self, other: &Self) -> Ordering {
        kind_byte(self.id.kind())
            .cmp(&kind_byte(other.id.kind()))
            .then_with(|| self.id.as_bytes().cmp(other.id.as_bytes()))
    }

    pub fn compare_offset(&self, other: &Self) -> Ordering {
        self.absolute_offset
            .cmp(&other.absolute_offset)
            .then_with(|| self.compare_key(other))
    }
}

/// Bounded metadata-only spill/sort port. Implementations may use charged
/// files or fixed runs, but payload bytes must never enter this port.
pub trait PackIndexSpoolV1 {
    /// Maximum userspace-resident metadata bytes for `maximum_entries`.
    /// LayerFS charges this declaration before construction or validation;
    /// spill-file bytes themselves are not resident memory.
    fn resident_memory_bound_bytes(&self, maximum_entries: u32) -> CoreResult<u64>;
    /// Exact external spill bytes currently occupied when the implementation
    /// can report them directly. Unavailable observations carry a reason and
    /// no numeric value.
    fn storage_bytes_observation(&self) -> CoreResult<OptionalU64ObservationV1> {
        Ok(OptionalU64ObservationV1::unavailable(
            "pack-index port exposes no direct spill-byte observation",
            ObservationScopeV1::Operation,
        ))
    }
    /// Promote the first concrete storage failure retained by a file-backed
    /// adapter. Pure/synthetic pack spools have no storage cause and keep the
    /// default. This is an internal transitional bridge: the semantic pack
    /// port still returns its bounded portable error, while the FsCas owner
    /// can preserve exact filesystem provenance at the adapter boundary.
    #[cfg(any(test, feature = "operation-polymorphism"))]
    #[doc(hidden)]
    fn take_storage_error_typed_v1(&mut self) -> Option<crate::cas::FsCasErrorV1> {
        None
    }
    fn reset(&mut self, maximum_entries: u32) -> Result<(), PackPortErrorV1>;
    fn reset_controlled_v1(
        &mut self,
        maximum_entries: u32,
        control: &mut dyn crate::limits::OperationWorkControlV1,
        _counters: &mut OperationCountersV1,
    ) -> Result<(), PackPortErrorV1> {
        if control.cancellation_requested_v1() {
            return Err(PackPortErrorV1::Cancelled);
        }
        if control.deadline_exceeded_v1() {
            return Err(PackPortErrorV1::Deadline);
        }
        let result = self.reset(maximum_entries);
        let post_poll = if control.cancellation_requested_v1() {
            Err(PackPortErrorV1::Cancelled)
        } else if control.deadline_exceeded_v1() {
            Err(PackPortErrorV1::Deadline)
        } else {
            Ok(())
        };
        match result {
            Ok(entry) => post_poll.map(|()| entry),
            Err(error) => Err(error),
        }
    }
    fn push(&mut self, entry: PackIndexEntryV1) -> Result<(), PackPortErrorV1>;
    fn push_controlled_v1(
        &mut self,
        entry: PackIndexEntryV1,
        control: &mut dyn crate::limits::OperationWorkControlV1,
        _counters: &mut OperationCountersV1,
    ) -> Result<(), PackPortErrorV1> {
        if control.cancellation_requested_v1() {
            return Err(PackPortErrorV1::Cancelled);
        }
        if control.deadline_exceeded_v1() {
            return Err(PackPortErrorV1::Deadline);
        }
        let result = self.push(entry);
        let post_poll = if control.cancellation_requested_v1() {
            Err(PackPortErrorV1::Cancelled)
        } else if control.deadline_exceeded_v1() {
            Err(PackPortErrorV1::Deadline)
        } else {
            Ok(())
        };
        result.and(post_poll)
    }
    fn sort_by_key(&mut self) -> Result<(), PackPortErrorV1>;
    fn sort_by_offset(&mut self) -> Result<(), PackPortErrorV1>;
    fn sort_by_key_controlled(
        &mut self,
        control: &mut dyn crate::limits::OperationWorkControlV1,
        counters: &mut OperationCountersV1,
    ) -> Result<(), PackPortErrorV1> {
        if control.cancellation_requested_v1() {
            return Err(PackPortErrorV1::Cancelled);
        }
        if control.deadline_exceeded_v1() {
            return Err(PackPortErrorV1::Deadline);
        }
        let result = self.sort_by_key();
        if result.is_ok() {
            counters.file_sort_control_polls = counters
                .file_sort_control_polls
                .checked_add(1)
                .ok_or(PackPortErrorV1::Failure)?;
        }
        result
    }
    fn sort_by_offset_controlled(
        &mut self,
        control: &mut dyn crate::limits::OperationWorkControlV1,
        counters: &mut OperationCountersV1,
    ) -> Result<(), PackPortErrorV1> {
        if control.cancellation_requested_v1() {
            return Err(PackPortErrorV1::Cancelled);
        }
        if control.deadline_exceeded_v1() {
            return Err(PackPortErrorV1::Deadline);
        }
        let result = self.sort_by_offset();
        if result.is_ok() {
            counters.file_sort_control_polls = counters
                .file_sort_control_polls
                .checked_add(1)
                .ok_or(PackPortErrorV1::Failure)?;
        }
        result
    }
    fn rewind(&mut self) -> Result<(), PackPortErrorV1>;
    fn next(&mut self) -> Result<Option<PackIndexEntryV1>, PackPortErrorV1>;
    fn next_controlled_v1(
        &mut self,
        control: &mut dyn crate::limits::OperationWorkControlV1,
        _counters: &mut OperationCountersV1,
    ) -> Result<Option<PackIndexEntryV1>, PackPortErrorV1> {
        if control.cancellation_requested_v1() {
            return Err(PackPortErrorV1::Cancelled);
        }
        if control.deadline_exceeded_v1() {
            return Err(PackPortErrorV1::Deadline);
        }
        let result = self.next();
        let post_poll = if control.cancellation_requested_v1() {
            Err(PackPortErrorV1::Cancelled)
        } else if control.deadline_exceeded_v1() {
            Err(PackPortErrorV1::Deadline)
        } else {
            Ok(())
        };
        match result {
            Ok(entry) => post_poll.map(|()| entry),
            Err(error) => Err(error),
        }
    }
    fn abort(&mut self);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SealedPackV1 {
    id: PackIdV1,
    pack_len: u64,
    record_count: u32,
    index_offset: u64,
}

impl SealedPackV1 {
    pub(crate) const fn from_validated_parts(
        id: PackIdV1,
        pack_len: u64,
        record_count: u32,
        index_offset: u64,
    ) -> Self {
        Self {
            id,
            pack_len,
            record_count,
            index_offset,
        }
    }

    pub const fn id(self) -> PackIdV1 {
        self.id
    }

    pub const fn pack_len(self) -> u64 {
        self.pack_len
    }

    pub const fn record_count(self) -> u32 {
        self.record_count
    }

    pub const fn index_offset(self) -> u64 {
        self.index_offset
    }
}

/// Decode only the immutable shape fields needed after a private pack has
/// already passed complete validation. This deliberately preserves the
/// private-seal I/O order without repeating full validation work.
pub(crate) fn read_sealed_pack_shape_v1<P>(pack: &mut P) -> CoreResult<SealedPackV1>
where
    P: PackReadPortV1 + ?Sized,
{
    let pack_len = pack.len().map_err(map_read_port)?;
    if pack_len < PACK_HEADER_BYTES + PACK_TRAILER_BYTES {
        return Err(CoreError::PackInvalid);
    }
    let mut header = [0_u8; PACK_HEADER_BYTES as usize];
    pack.read_exact_at(0, &mut header).map_err(map_read_port)?;
    let record_count = be_u32(&header[48..52]);
    let index_offset = be_u64(&header[56..64]);
    let digest_offset = pack_len
        .checked_sub(DIGEST_BYTES as u64)
        .ok_or(CoreError::PackInvalid)?;
    let mut digest = [0_u8; DIGEST_BYTES];
    pack.read_exact_at(digest_offset, &mut digest)
        .map_err(map_read_port)?;
    Ok(SealedPackV1::from_validated_parts(
        PackIdV1::from_digest(digest),
        pack_len,
        record_count,
        index_offset,
    ))
}

/// Checked location of one canonical object inside a validated dense pack.
///
/// This is an internal carrier fact, not an independently trusted catalog
/// record. Callers must first validate the complete pack and retain the pack's
/// immutable bytes for the lifetime of the location.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PackObjectLocationV1 {
    pub(crate) object_offset: u64,
    pub(crate) object_len: u64,
}

/// Read the unique canonical index entry for `expected` from an immutable pack
/// whose complete seal was already validated at admission.
pub(crate) fn locate_validated_pack_index_entry_v1<P>(
    pack: &mut P,
    sealed: SealedPackV1,
    expected: TypedPhysicalObjectIdV1,
    counters: &mut OperationCountersV1,
) -> CoreResult<Option<PackIndexEntryV1>>
where
    P: PackReadPortV1 + ?Sized,
{
    let mut control = NeverStopWorkControlV1;
    locate_validated_pack_index_entry_controlled_v1(pack, sealed, expected, counters, &mut control)
}

pub(crate) fn locate_validated_pack_index_entry_controlled_v1<P, C>(
    pack: &mut P,
    sealed: SealedPackV1,
    expected: TypedPhysicalObjectIdV1,
    counters: &mut OperationCountersV1,
    control: &mut C,
) -> CoreResult<Option<PackIndexEntryV1>>
where
    P: PackReadPortV1 + ?Sized,
    C: crate::limits::OperationWorkControlV1 + ?Sized,
{
    locate_validated_pack_index_entry_controlled_recorded_v1(
        pack, sealed, expected, counters, control, None,
    )
}

pub(crate) fn locate_validated_pack_index_entry_controlled_recorded_v1<P, C>(
    pack: &mut P,
    sealed: SealedPackV1,
    expected: TypedPhysicalObjectIdV1,
    counters: &mut OperationCountersV1,
    control: &mut C,
    record_control_poll: Option<PackControlPollRecorderV1>,
) -> CoreResult<Option<PackIndexEntryV1>>
where
    P: PackReadPortV1 + ?Sized,
    C: crate::limits::OperationWorkControlV1 + ?Sized,
{
    poll_work_control_recorded_v1(control, counters, record_control_poll, 0)?;
    let pack_len = pack.len().map_err(map_read_port);
    let pack_len_work = u64::from(pack_len.is_ok());
    let pack_len = complete_work_boundary_v1(
        pack_len,
        pack_len_work,
        control,
        counters,
        record_control_poll,
    )?;
    if pack_len != sealed.pack_len {
        return Err(CoreError::PackInvalid);
    }
    let mut low = 0_u32;
    let mut high = sealed.record_count;
    while low < high {
        let middle = low + (high - low) / 2;
        let offset = sealed
            .index_offset
            .checked_add(
                u64::from(middle)
                    .checked_mul(PACK_INDEX_ENTRY_BYTES)
                    .ok_or(CoreError::IntegerOverflow)?,
            )
            .ok_or(CoreError::IntegerOverflow)?;
        let bytes = read_array_controlled_v1::<80, _, _>(
            pack,
            offset,
            counters,
            control,
            record_control_poll,
            Some(PackStageBoundaryRecordersV1 {
                call: OperationCountersV1::record_locator_index_read_call_v1,
                bytes: OperationCountersV1::record_locator_index_read_bytes_v1,
            }),
        )?;
        record_pack_decode_boundary_v1(
            counters,
            PackStageBoundaryRecordersV1 {
                call: OperationCountersV1::record_locator_index_decode_call_v1,
                bytes: OperationCountersV1::record_locator_index_decode_bytes_v1,
            },
            PACK_INDEX_ENTRY_BYTES,
        )?;
        let entry = complete_work_boundary_v1(
            decode_index_entry(&bytes),
            PACK_INDEX_ENTRY_BYTES,
            control,
            counters,
            record_control_poll,
        )?;
        counters.record_locator_index_probe_v1()?;
        match compare_typed_key(entry.id, expected) {
            Ordering::Less => low = middle.checked_add(1).ok_or(CoreError::IntegerOverflow)?,
            Ordering::Greater => high = middle,
            Ordering::Equal => return Ok(Some(entry)),
        }
    }
    poll_work_control_recorded_v1(control, counters, record_control_poll, 0)?;
    Ok(None)
}

/// Read one canonical index entry by ordinal from an immutable carrier whose
/// complete seal was already validated. This keeps carrier-internal offsets
/// inside `pack` while allowing a publication owner to enumerate cleanup
/// custody without depending on a mutable operation sort spool.
pub(crate) fn read_validated_pack_index_entry_v1<P>(
    pack: &mut P,
    sealed: SealedPackV1,
    ordinal: u32,
    counters: &mut OperationCountersV1,
) -> CoreResult<PackIndexEntryV1>
where
    P: PackReadPortV1 + ?Sized,
{
    let mut control = NeverStopWorkControlV1;
    read_validated_pack_index_entry_controlled_v1(pack, sealed, ordinal, counters, &mut control)
}

pub(crate) fn read_validated_pack_index_entry_controlled_v1<P, C>(
    pack: &mut P,
    sealed: SealedPackV1,
    ordinal: u32,
    counters: &mut OperationCountersV1,
    control: &mut C,
) -> CoreResult<PackIndexEntryV1>
where
    P: PackReadPortV1 + ?Sized,
    C: crate::limits::OperationWorkControlV1 + ?Sized,
{
    poll_work_control_v1(control)?;
    if ordinal >= sealed.record_count {
        return Err(CoreError::PackInvalid);
    }
    let pack_len = pack.len().map_err(map_read_port);
    let pack_len_work = u64::from(pack_len.is_ok());
    let pack_len = complete_work_boundary_v1(pack_len, pack_len_work, control, counters, None)?;
    if pack_len != sealed.pack_len {
        return Err(CoreError::PackInvalid);
    }
    let offset = sealed
        .index_offset
        .checked_add(
            u64::from(ordinal)
                .checked_mul(PACK_INDEX_ENTRY_BYTES)
                .ok_or(CoreError::IntegerOverflow)?,
        )
        .ok_or(CoreError::IntegerOverflow)?;
    let bytes = read_array_controlled_v1::<80, _, _>(
        pack,
        offset,
        counters,
        control,
        None,
        Some(PackStageBoundaryRecordersV1 {
            call: OperationCountersV1::record_locator_index_read_call_v1,
            bytes: OperationCountersV1::record_locator_index_read_bytes_v1,
        }),
    )?;
    record_pack_decode_boundary_v1(
        counters,
        PackStageBoundaryRecordersV1 {
            call: OperationCountersV1::record_locator_index_decode_call_v1,
            bytes: OperationCountersV1::record_locator_index_decode_bytes_v1,
        },
        PACK_INDEX_ENTRY_BYTES,
    )?;
    complete_work_boundary_v1(
        decode_index_entry(&bytes),
        PACK_INDEX_ENTRY_BYTES,
        control,
        counters,
        None,
    )
}

/// Revalidate every canonical byte named by one admitted index entry. This is
/// used for object-level incumbent validation without trusting the locator as
/// an identity oracle.
pub(crate) fn validate_validated_pack_object_v1<P>(
    pack: &mut P,
    entry: PackIndexEntryV1,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    counters: &mut OperationCountersV1,
) -> CoreResult<PackObjectLocationV1>
where
    P: PackReadPortV1 + ?Sized,
{
    let mut control = NeverStopWorkControlV1;
    validate_validated_pack_object_controlled_v1(pack, entry, scratch, counters, &mut control)
}

pub(crate) fn validate_validated_pack_object_controlled_v1<P, C>(
    pack: &mut P,
    entry: PackIndexEntryV1,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    counters: &mut OperationCountersV1,
    control: &mut C,
) -> CoreResult<PackObjectLocationV1>
where
    P: PackReadPortV1 + ?Sized,
    C: crate::limits::OperationWorkControlV1 + ?Sized,
{
    validate_validated_pack_object_controlled_recorded_v1(
        pack, entry, scratch, counters, control, None,
    )
}

pub(crate) fn validate_validated_pack_object_controlled_recorded_v1<P, C>(
    pack: &mut P,
    entry: PackIndexEntryV1,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    counters: &mut OperationCountersV1,
    control: &mut C,
    record_control_poll: Option<PackControlPollRecorderV1>,
) -> CoreResult<PackObjectLocationV1>
where
    P: PackReadPortV1 + ?Sized,
    C: crate::limits::OperationWorkControlV1 + ?Sized,
{
    validate_record_controlled_v1(
        pack,
        entry,
        ProfileSpecV1::frozen().id(),
        scratch,
        counters,
        control,
        record_control_poll,
        PackValidationStageV1::InstalledCarrier,
    )?;
    Ok(PackObjectLocationV1 {
        object_offset: entry
            .absolute_offset
            .checked_add(4)
            .ok_or(CoreError::IntegerOverflow)?,
        object_len: u64::from(entry.object_len),
    })
}

#[cfg(any(test, feature = "operation-polymorphism"))]
pub(crate) fn build_dense_pack_v1<O, P, M>(
    objects: &mut O,
    pack: &mut P,
    metadata: &mut M,
    ledger: &ResourceLedgerV1,
    counters: &mut OperationCountersV1,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
) -> CoreResult<SealedPackV1>
where
    O: PackObjectSourceV1 + ?Sized,
    P: PrivatePackPortV1 + ?Sized,
    M: PackIndexSpoolV1 + ?Sized,
{
    let record_count = objects.declared_object_count()?;
    validate_record_count(record_count)?;
    let metadata_bytes = metadata.resident_memory_bound_bytes(record_count)?;
    let source_bytes = objects.resident_memory_bound_bytes()?;
    let pack_bytes = pack.resident_memory_bound_bytes()?;
    let resident_bytes = metadata_bytes
        .checked_add(source_bytes)
        .and_then(|bytes| bytes.checked_add(pack_bytes))
        .ok_or(CoreError::IntegerOverflow)?;
    let memory = OperationMemoryPlanV1::empty()
        .charge(MemoryComponentV1::ComparisonWindow, scratch.len() as u64)?
        .charge(
            MemoryComponentV1::HashState,
            IDENTITY_HASHER_BYTES_V1
                .checked_mul(2)
                .ok_or(CoreError::IntegerOverflow)?,
        )?
        .charge(MemoryComponentV1::MetadataWindow, resident_bytes)?;
    let _reservation = ledger.reserve_operation_with_plan(memory)?;
    counters.memory_high_water = counters.memory_high_water.max(ledger.high_water_bytes());
    let (_, index_offset, pack_len) = preflight(objects, record_count)?;
    pack.begin_private(pack_len).map_err(map_write_port)?;
    metadata.reset(record_count).map_err(map_spool_port)?;

    let result = build_inner(
        objects,
        pack,
        metadata,
        counters,
        scratch,
        record_count,
        index_offset,
        pack_len,
    );
    if result.is_err() {
        metadata.abort();
        pack.abort_private();
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn build_inner<O, P, M>(
    objects: &mut O,
    pack: &mut P,
    metadata: &mut M,
    counters: &mut OperationCountersV1,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    record_count: u32,
    index_offset: u64,
    pack_len: u64,
) -> CoreResult<SealedPackV1>
where
    O: PackObjectSourceV1 + ?Sized,
    P: PrivatePackPortV1 + ?Sized,
    M: PackIndexSpoolV1 + ?Sized,
{
    let checksum_len = pack_len.checked_sub(32).ok_or(CoreError::IntegerOverflow)?;
    let mut pack_hasher = FramedHasherV1::new(TAG_PACK, checksum_len);
    append_hashed(
        pack,
        &encode_header(record_count, index_offset),
        counters,
        &mut pack_hasher,
    )?;
    let mut expected_offset = PACK_HEADER_BYTES;
    for ordinal in 0..record_count {
        let expected_id = objects.object_id(ordinal).map_err(map_read_port)?;
        let object_len = objects.object_len(ordinal).map_err(map_read_port)?;
        validate_physical_object_len(object_len)?;
        if expected_offset != pack.len().map_err(map_read_port)? {
            return Err(CoreError::PackInvalid);
        }
        let object_len_u32 = u32::try_from(object_len).map_err(|_| CoreError::IntegerOverflow)?;
        append_hashed(
            pack,
            &object_len_u32.to_be_bytes(),
            counters,
            &mut pack_hasher,
        )?;
        let mut checksum = FramedHasherV1::new(TAG_OBJECT_CHECKSUM, object_len);
        let mut offset = 0_u64;
        while offset < object_len {
            let remaining = object_len
                .checked_sub(offset)
                .ok_or(CoreError::IntegerOverflow)?;
            let take = usize::try_from(remaining.min(COMPARISON_WINDOW_BYTES as u64))
                .map_err(|_| CoreError::IntegerOverflow)?;
            let block = &mut scratch[..take];
            objects
                .read_object_exact_at(ordinal, offset, block)
                .map_err(map_read_port)?;
            counters.add(CounterFieldV1::BytesRead, take as u64)?;
            checksum.write(block)?;
            append_hashed(pack, block, counters, &mut pack_hasher)?;
            offset = offset
                .checked_add(take as u64)
                .ok_or(CoreError::IntegerOverflow)?;
        }
        let checksum = ObjectChecksumV1::from_digest(checksum.finish()?);
        let pad = record_padding(object_len)?;
        append_hashed(
            pack,
            &[0_u8; 7][..usize::from(pad)],
            counters,
            &mut pack_hasher,
        )?;
        metadata
            .push(PackIndexEntryV1 {
                id: expected_id,
                absolute_offset: expected_offset,
                object_len: object_len_u32,
                object_checksum: checksum,
            })
            .map_err(map_spool_port)?;
        expected_offset = expected_offset
            .checked_add(record_len(object_len)?)
            .ok_or(CoreError::IntegerOverflow)?;
    }
    if expected_offset != index_offset {
        return Err(CoreError::PackInvalid);
    }

    let mut sort_control = NeverStopWorkControlV1;
    metadata
        .sort_by_key_controlled(&mut sort_control, counters)
        .map_err(map_spool_port)?;
    metadata.rewind().map_err(map_spool_port)?;
    let mut previous: Option<PackIndexEntryV1> = None;
    let mut emitted = 0_u32;
    while let Some(entry) = metadata.next().map_err(map_spool_port)? {
        if previous.is_some_and(|left| left.compare_key(&entry) != Ordering::Less) {
            return Err(CoreError::NonCanonicalOrder);
        }
        append_hashed(pack, &encode_index_entry(entry), counters, &mut pack_hasher)?;
        previous = Some(entry);
        emitted = emitted.checked_add(1).ok_or(CoreError::IntegerOverflow)?;
    }
    if emitted != record_count {
        return Err(CoreError::PackInvalid);
    }

    let index_len = u64::from(record_count)
        .checked_mul(PACK_INDEX_ENTRY_BYTES)
        .ok_or(CoreError::IntegerOverflow)?;
    append_hashed(
        pack,
        &encode_trailer_prefix(pack_len, index_offset, index_len, record_count),
        counters,
        &mut pack_hasher,
    )?;
    if pack.len().map_err(map_read_port)? != checksum_len {
        return Err(CoreError::PackInvalid);
    }
    let pack_digest = pack_hasher.finish()?;
    append(pack, &pack_digest, counters)?;
    if pack.len().map_err(map_read_port)? != pack_len {
        return Err(CoreError::PackInvalid);
    }

    let mut control = NeverStopWorkControlV1;
    let validated = validate_pack_inner_v1(
        pack,
        metadata,
        scratch,
        counters,
        record_count,
        &mut control,
        PackValidationStageV1::PreInstall,
    )?;
    if validated.id.as_bytes() != &pack_digest
        || validated.record_count != record_count
        || validated.index_offset != index_offset
        || validated.pack_len != pack_len
    {
        return Err(CoreError::PackInvalid);
    }
    pack.seal_private(validated.id).map_err(map_write_port)?;
    counters.add(CounterFieldV1::PackEntries, u64::from(record_count))?;
    counters.add(CounterFieldV1::PackBytes, pack_len)?;
    Ok(validated)
}

#[cfg(any(test, feature = "operation-polymorphism"))]
pub(crate) fn validate_pack_v1<P, M>(
    pack: &mut P,
    metadata: &mut M,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    maximum_entries: u32,
    ledger: &ResourceLedgerV1,
    counters: &mut OperationCountersV1,
) -> CoreResult<SealedPackV1>
where
    P: PackReadPortV1 + ?Sized,
    M: PackIndexSpoolV1 + ?Sized,
{
    validate_pack_stage_v1(
        pack,
        metadata,
        scratch,
        maximum_entries,
        ledger,
        counters,
        PackValidationStageV1::PreInstall,
    )
}

#[cfg(any(test, feature = "operation-polymorphism"))]
#[allow(clippy::too_many_arguments)]
pub(crate) fn validate_pack_stage_v1<P, M>(
    pack: &mut P,
    metadata: &mut M,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    maximum_entries: u32,
    ledger: &ResourceLedgerV1,
    counters: &mut OperationCountersV1,
    stage: PackValidationStageV1,
) -> CoreResult<SealedPackV1>
where
    P: PackReadPortV1 + ?Sized,
    M: PackIndexSpoolV1 + ?Sized,
{
    if maximum_entries == 0 || u64::from(maximum_entries) > MAX_PACK_RECORDS {
        return Err(CoreError::ResourceRefused);
    }
    let metadata_bytes = metadata
        .resident_memory_bound_bytes(maximum_entries)?
        .checked_add(pack.resident_memory_bound_bytes()?)
        .ok_or(CoreError::IntegerOverflow)?;
    let memory = OperationMemoryPlanV1::empty()
        .charge(MemoryComponentV1::ComparisonWindow, scratch.len() as u64)?
        .charge(
            MemoryComponentV1::HashState,
            IDENTITY_HASHER_BYTES_V1
                .checked_mul(2)
                .ok_or(CoreError::IntegerOverflow)?,
        )?
        .charge(MemoryComponentV1::MetadataWindow, metadata_bytes)?;
    let _reservation = ledger.reserve_operation_with_plan(memory)?;
    counters.memory_high_water = counters.memory_high_water.max(ledger.high_water_bytes());
    let mut control = NeverStopWorkControlV1;
    validate_pack_inner_v1(
        pack,
        metadata,
        scratch,
        counters,
        maximum_entries,
        &mut control,
        stage,
    )
}

#[cfg(any(test, feature = "operation-polymorphism"))]
pub(crate) fn validate_pack_borrowed_v1<P, M>(
    pack: &mut P,
    metadata: &mut M,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    maximum_entries: u32,
    reservation: &OperationReservationV1<'_>,
    counters: &mut OperationCountersV1,
    control: &mut dyn crate::limits::OperationWorkControlV1,
) -> CoreResult<SealedPackV1>
where
    P: PackReadPortV1 + ?Sized,
    M: PackIndexSpoolV1 + ?Sized,
{
    validate_pack_borrowed_stage_v1(
        pack,
        metadata,
        scratch,
        maximum_entries,
        reservation,
        counters,
        control,
        PackValidationStageV1::PreInstall,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn validate_pack_borrowed_stage_v1<P, M>(
    pack: &mut P,
    metadata: &mut M,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    maximum_entries: u32,
    reservation: &OperationReservationV1<'_>,
    counters: &mut OperationCountersV1,
    control: &mut dyn crate::limits::OperationWorkControlV1,
    stage: PackValidationStageV1,
) -> CoreResult<SealedPackV1>
where
    P: PackReadPortV1 + ?Sized,
    M: PackIndexSpoolV1 + ?Sized,
{
    if maximum_entries == 0 || u64::from(maximum_entries) > MAX_PACK_RECORDS {
        return Err(CoreError::ResourceRefused);
    }
    let metadata_bytes = metadata
        .resident_memory_bound_bytes(maximum_entries)?
        .checked_add(pack.resident_memory_bound_bytes()?)
        .ok_or(CoreError::IntegerOverflow)?;
    let memory = OperationMemoryPlanV1::empty()
        .charge(MemoryComponentV1::ComparisonWindow, scratch.len() as u64)?
        .charge(
            MemoryComponentV1::HashState,
            IDENTITY_HASHER_BYTES_V1
                .checked_mul(2)
                .ok_or(CoreError::IntegerOverflow)?,
        )?
        .charge(MemoryComponentV1::MetadataWindow, metadata_bytes)?;
    reservation.require(memory)?;
    validate_pack_inner_v1(
        pack,
        metadata,
        scratch,
        counters,
        maximum_entries,
        control,
        stage,
    )
}

fn validate_pack_inner_v1<P, M>(
    pack: &mut P,
    metadata: &mut M,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    counters: &mut OperationCountersV1,
    maximum_entries: u32,
    control: &mut dyn crate::limits::OperationWorkControlV1,
    stage: PackValidationStageV1,
) -> CoreResult<SealedPackV1>
where
    P: PackReadPortV1 + ?Sized,
    M: PackIndexSpoolV1 + ?Sized,
{
    poll_validation_control_v1(control, counters, stage, None, 0)?;
    let pack_len = pack.len().map_err(map_read_port);
    let pack_len_work = u64::from(pack_len.is_ok());
    let pack_len =
        complete_validation_boundary_v1(pack_len, pack_len_work, control, counters, stage, None)?;
    if !(PACK_HEADER_BYTES + PACK_TRAILER_BYTES..=MAX_PACK_BYTES).contains(&pack_len) {
        return Err(CoreError::PackInvalid);
    }
    let header =
        read_array_validation_controlled_v1::<64, _, _>(pack, 0, counters, control, stage, None)?;
    if &header[..8] != PACK_MAGIC
        || be_u16(&header[10..12]) != 64
        || be_u32(&header[12..16]) != 0
        || be_u16(&header[52..54]) != 80
        || be_u16(&header[54..56]) != 0
    {
        return Err(CoreError::PackInvalid);
    }
    if be_u16(&header[8..10]) != 1 {
        return Err(CoreError::Schema);
    }
    let profile = ProfileSpecV1::frozen().id();
    if &header[16..48] != profile.as_bytes() {
        return Err(CoreError::TypeDomain);
    }
    let record_count = be_u32(&header[48..52]);
    if record_count == 0
        || record_count > maximum_entries
        || u64::from(record_count) > MAX_PACK_RECORDS
    {
        return Err(CoreError::PackInvalid);
    }
    let index_offset = be_u64(&header[56..64]);
    let index_len = u64::from(record_count)
        .checked_mul(PACK_INDEX_ENTRY_BYTES)
        .ok_or(CoreError::IntegerOverflow)?;
    if index_len > MAX_PACK_INDEX_BYTES || index_offset < PACK_HEADER_BYTES {
        return Err(CoreError::PackInvalid);
    }
    let trailer_offset = index_offset
        .checked_add(index_len)
        .ok_or(CoreError::IntegerOverflow)?;
    let computed_len = trailer_offset
        .checked_add(PACK_TRAILER_BYTES)
        .ok_or(CoreError::IntegerOverflow)?;
    if computed_len != pack_len {
        return Err(CoreError::PackInvalid);
    }
    let trailer = read_array_validation_controlled_v1::<80, _, _>(
        pack,
        trailer_offset,
        counters,
        control,
        stage,
        None,
    )?;
    if &trailer[..8] != PACK_TRAILER_MAGIC
        || be_u16(&trailer[10..12]) != 80
        || be_u32(&trailer[12..16]) != 0
        || be_u64(&trailer[16..24]) != pack_len
        || be_u64(&trailer[24..32]) != index_offset
        || be_u64(&trailer[32..40]) != index_len
        || be_u32(&trailer[40..44]) != record_count
        || be_u32(&trailer[44..48]) != 0
    {
        return Err(CoreError::PackInvalid);
    }
    if be_u16(&trailer[8..10]) != 1 {
        return Err(CoreError::Schema);
    }
    let checksum_len = pack_len.checked_sub(32).ok_or(CoreError::IntegerOverflow)?;
    let digest = hash_port_range_controlled_validation_recorded_v1(
        pack,
        0,
        checksum_len,
        TAG_PACK,
        scratch,
        counters,
        control,
        Some(stage.read_recorders()),
        Some(stage.hash_recorders()),
        Some(stage),
    )?;
    if trailer[48..80] != digest {
        return Err(CoreError::IdMismatch);
    }

    reset_validation_spool_v1(metadata, record_count, counters, control, stage)?;
    let validation = validate_index_and_records(
        pack,
        metadata,
        scratch,
        counters,
        record_count,
        index_offset,
        profile,
        control,
        stage,
    );
    if validation.is_err() {
        // The validation cause is chronologically earlier than any abort
        // control/observation failure and therefore remains authoritative.
        let _ = abort_validation_spool_v1(metadata, counters, control, stage);
    }
    validation?;
    Ok(SealedPackV1 {
        id: PackIdV1::from_digest(digest),
        pack_len,
        record_count,
        index_offset,
    })
}

fn reset_validation_spool_v1<M>(
    metadata: &mut M,
    record_count: u32,
    counters: &mut OperationCountersV1,
    control: &mut dyn crate::limits::OperationWorkControlV1,
    stage: PackValidationStageV1,
) -> CoreResult<()>
where
    M: PackIndexSpoolV1 + ?Sized,
{
    poll_validation_control_v1(control, counters, stage, None, 0)?;
    let reset = metadata.reset(record_count).map_err(map_spool_port);
    let completed_work = u64::from(reset.is_ok());
    complete_validation_boundary_v1(reset, completed_work, control, counters, stage, None)
}

fn push_validation_spool_v1<M>(
    metadata: &mut M,
    entry: PackIndexEntryV1,
    counters: &mut OperationCountersV1,
    control: &mut dyn crate::limits::OperationWorkControlV1,
    stage: PackValidationStageV1,
) -> CoreResult<()>
where
    M: PackIndexSpoolV1 + ?Sized,
{
    poll_validation_control_v1(control, counters, stage, None, 0)?;
    let push = metadata.push(entry).map_err(map_spool_port);
    let completed_work = if push.is_ok() {
        PACK_INDEX_ENTRY_BYTES
    } else {
        0
    };
    complete_validation_boundary_v1(push, completed_work, control, counters, stage, None)
}

fn next_validation_spool_v1<M>(
    metadata: &mut M,
    counters: &mut OperationCountersV1,
    control: &mut dyn crate::limits::OperationWorkControlV1,
    stage: PackValidationStageV1,
) -> CoreResult<Option<PackIndexEntryV1>>
where
    M: PackIndexSpoolV1 + ?Sized,
{
    poll_validation_control_v1(control, counters, stage, None, 0)?;
    let next = metadata.next().map_err(map_spool_port);
    let completed_work = if matches!(next, Ok(Some(_))) {
        PACK_INDEX_ENTRY_BYTES
    } else {
        0
    };
    complete_validation_boundary_v1(next, completed_work, control, counters, stage, None)
}

fn abort_validation_spool_v1<M>(
    metadata: &mut M,
    counters: &mut OperationCountersV1,
    control: &mut dyn crate::limits::OperationWorkControlV1,
    stage: PackValidationStageV1,
) -> CoreResult<()>
where
    M: PackIndexSpoolV1 + ?Sized,
{
    let pre_poll = poll_validation_control_v1(control, counters, stage, None, 0);
    metadata.abort();
    let post_poll = poll_validation_control_v1(control, counters, stage, None, 1);
    pre_poll.and(post_poll)
}

#[allow(clippy::too_many_arguments)]
fn validate_index_and_records<P, M>(
    pack: &mut P,
    metadata: &mut M,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    counters: &mut OperationCountersV1,
    record_count: u32,
    index_offset: u64,
    profile: ProfileId,
    control: &mut dyn crate::limits::OperationWorkControlV1,
    stage: PackValidationStageV1,
) -> CoreResult<()>
where
    P: PackReadPortV1 + ?Sized,
    M: PackIndexSpoolV1 + ?Sized,
{
    let mut previous: Option<PackIndexEntryV1> = None;
    for ordinal in 0..record_count {
        poll_validation_control_v1(control, counters, stage, None, 0)?;
        let offset = index_offset
            .checked_add(
                u64::from(ordinal)
                    .checked_mul(PACK_INDEX_ENTRY_BYTES)
                    .ok_or(CoreError::IntegerOverflow)?,
            )
            .ok_or(CoreError::IntegerOverflow)?;
        let bytes = read_array_validation_controlled_v1::<80, _, _>(
            pack, offset, counters, control, stage, None,
        )?;
        record_pack_decode_boundary_v1(counters, stage.decode_recorders(), PACK_INDEX_ENTRY_BYTES)?;
        let entry = complete_validation_boundary_v1(
            decode_index_entry(&bytes),
            PACK_INDEX_ENTRY_BYTES,
            control,
            counters,
            stage,
            None,
        )?;
        if previous.is_some_and(|left| left.compare_key(&entry) != Ordering::Less) {
            return Err(CoreError::PackInvalid);
        }
        push_validation_spool_v1(metadata, entry, counters, control, stage)?;
        previous = Some(entry);
    }
    poll_validation_control_v1(control, counters, stage, None, 0)?;
    let sort = metadata
        .sort_by_offset_controlled(control, counters)
        .map_err(map_spool_port);
    // The controlled sorter directly owns comparison/read/write/pass work and
    // its bounded polling cadence. This surrounding validation poll records
    // only the stage boundary; it must not derive or double-count sort work.
    complete_validation_boundary_v1(sort, 0, control, counters, stage, None)?;
    metadata.rewind().map_err(map_spool_port)?;
    let mut expected_offset = PACK_HEADER_BYTES;
    let mut consumed = 0_u32;
    while let Some(entry) = next_validation_spool_v1(metadata, counters, control, stage)? {
        if entry.absolute_offset != expected_offset {
            return Err(CoreError::PackInvalid);
        }
        validate_record_controlled_v1(
            pack, entry, profile, scratch, counters, control, None, stage,
        )?;
        expected_offset = expected_offset
            .checked_add(record_len(u64::from(entry.object_len))?)
            .ok_or(CoreError::IntegerOverflow)?;
        consumed = consumed.checked_add(1).ok_or(CoreError::IntegerOverflow)?;
    }
    if consumed != record_count || expected_offset != index_offset {
        return Err(CoreError::PackInvalid);
    }
    Ok(())
}

struct NeverStopWorkControlV1;

impl crate::limits::OperationWorkControlV1 for NeverStopWorkControlV1 {
    fn cancellation_requested_v1(&mut self) -> bool {
        false
    }

    fn deadline_exceeded_v1(&mut self) -> bool {
        false
    }
}

fn validate_record<P: PackReadPortV1 + ?Sized>(
    pack: &mut P,
    entry: PackIndexEntryV1,
    profile: ProfileId,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    counters: &mut OperationCountersV1,
) -> CoreResult<()> {
    let mut control = NeverStopWorkControlV1;
    validate_record_controlled_v1(
        pack,
        entry,
        profile,
        scratch,
        counters,
        &mut control,
        None,
        PackValidationStageV1::PreInstall,
    )
}

fn validate_record_controlled_v1<P, C>(
    pack: &mut P,
    entry: PackIndexEntryV1,
    profile: ProfileId,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    counters: &mut OperationCountersV1,
    control: &mut C,
    record_control_poll: Option<PackControlPollRecorderV1>,
    stage: PackValidationStageV1,
) -> CoreResult<()>
where
    P: PackReadPortV1 + ?Sized,
    C: crate::limits::OperationWorkControlV1 + ?Sized,
{
    let prefix = read_array_validation_controlled_v1::<4, _, _>(
        pack,
        entry.absolute_offset,
        counters,
        control,
        stage,
        record_control_poll,
    )?;
    if be_u32(&prefix) != entry.object_len {
        return Err(CoreError::PackInvalid);
    }
    let object_len = u64::from(entry.object_len);
    validate_physical_object_len(object_len).map_err(|_| CoreError::PackInvalid)?;
    if object_len < OBJECT_HEADER_BYTES + 1 {
        return Err(CoreError::PackInvalid);
    }
    let object_offset = entry
        .absolute_offset
        .checked_add(4)
        .ok_or(CoreError::IntegerOverflow)?;
    let mut checksum_hasher = FramedHasherV1::new(TAG_OBJECT_CHECKSUM, object_len);
    let decoded = {
        let mut object = PackObjectReadV1 {
            pack,
            start: object_offset,
            len: object_len,
            counters,
            checksum: &mut checksum_hasher,
            next_offset: 0,
            control,
            record_control_poll,
            stage,
        };
        (stage.decode_recorders().call)(object.counters)?;
        let decoded =
            decode_physical_object_from_port_v1(&mut object, &mut DiscardStrongEdgesV1, scratch)
                .map_err(map_object_validation);
        decoded.and_then(|decoded| {
            if object.next_offset != object_len {
                Err(CoreError::PackInvalid)
            } else {
                Ok(decoded)
            }
        })
    };
    let decoded =
        complete_validation_boundary_v1(decoded, 0, control, counters, stage, record_control_poll)?;
    let checksum = ObjectChecksumV1::from_digest(checksum_hasher.finish()?);
    if decoded.header().profile_id() != profile || decoded.header().kind() != entry.id.kind() {
        return Err(CoreError::TypeDomain);
    }
    if decoded.physical_id() != entry.id || checksum != entry.object_checksum {
        return Err(CoreError::IdMismatch);
    }
    let pad = record_padding(object_len)?;
    if pad != 0 {
        poll_validation_control_v1(control, counters, stage, record_control_poll, 0)?;
        let mut bytes = [0_u8; 7];
        let padding_offset = object_offset
            .checked_add(object_len)
            .ok_or(CoreError::IntegerOverflow)?;
        record_pack_stage_call_v1(counters, Some(stage.read_recorders()))?;
        let padding = pack
            .read_exact_at(padding_offset, &mut bytes[..usize::from(pad)])
            .map_err(map_read_port);
        let padding_work = if padding.is_ok() { u64::from(pad) } else { 0 };
        let padding = padding.and_then(|()| {
            record_pack_read_bytes_v1(counters, u64::from(pad), Some(stage.read_recorders()))
        });
        complete_validation_boundary_v1(
            padding,
            padding_work,
            control,
            counters,
            stage,
            record_control_poll,
        )?;
        if bytes[..usize::from(pad)].iter().any(|byte| *byte != 0) {
            return Err(CoreError::PackInvalid);
        }
    }
    Ok(())
}

struct PackObjectReadV1<'a, P: PackReadPortV1 + ?Sized, C: ?Sized> {
    pack: &'a mut P,
    start: u64,
    len: u64,
    counters: &'a mut OperationCountersV1,
    checksum: &'a mut FramedHasherV1,
    next_offset: u64,
    control: &'a mut C,
    record_control_poll: Option<PackControlPollRecorderV1>,
    stage: PackValidationStageV1,
}

impl<P, C> PhysicalObjectReadPortV1 for PackObjectReadV1<'_, P, C>
where
    P: PackReadPortV1 + ?Sized,
    C: crate::limits::OperationWorkControlV1 + ?Sized,
{
    fn len(&mut self) -> CoreResult<u64> {
        Ok(self.len)
    }

    fn read_exact_at(&mut self, offset: u64, destination: &mut [u8]) -> CoreResult<()> {
        poll_validation_control_v1(
            self.control,
            self.counters,
            self.stage,
            self.record_control_poll,
            0,
        )?;
        if offset != self.next_offset {
            return Err(CoreError::PackInvalid);
        }
        let requested = u64::try_from(destination.len()).map_err(|_| CoreError::IntegerOverflow)?;
        let end = offset
            .checked_add(requested)
            .ok_or(CoreError::IntegerOverflow)?;
        if end > self.len {
            return Err(CoreError::Truncated);
        }
        let absolute = self
            .start
            .checked_add(offset)
            .ok_or(CoreError::IntegerOverflow)?;
        record_pack_stage_call_v1(self.counters, Some(self.stage.read_recorders()))?;
        let read = self
            .pack
            .read_exact_at(absolute, destination)
            .map_err(map_read_port);
        let completed_work = if read.is_ok() { requested } else { 0 };
        let read = read.and_then(|()| {
            record_pack_read_bytes_v1(self.counters, requested, Some(self.stage.read_recorders()))
        });
        complete_validation_boundary_v1(
            read,
            completed_work,
            self.control,
            self.counters,
            self.stage,
            self.record_control_poll,
        )?;
        record_pack_stage_call_v1(self.counters, Some(self.stage.hash_recorders()))?;
        self.checksum.write(destination)?;
        (self.stage.hash_recorders().bytes)(self.counters, requested)?;
        poll_validation_control_v1(
            self.control,
            self.counters,
            self.stage,
            self.record_control_poll,
            requested,
        )?;
        (self.stage.decode_recorders().bytes)(self.counters, requested)?;
        self.next_offset = end;
        Ok(())
    }
}

fn validate_record_count(record_count: u32) -> CoreResult<()> {
    let count = u64::from(record_count);
    if count == 0 || count > MAX_PACK_RECORDS {
        return Err(CoreError::CountCap);
    }
    Ok(())
}

fn preflight<O: PackObjectSourceV1 + ?Sized>(
    objects: &mut O,
    record_count: u32,
) -> CoreResult<(u32, u64, u64)> {
    validate_record_count(record_count)?;
    let count = u64::from(record_count);
    let mut index_offset = PACK_HEADER_BYTES;
    for ordinal in 0..record_count {
        let len = objects.object_len(ordinal).map_err(map_read_port)?;
        validate_physical_object_len(len)?;
        index_offset = index_offset
            .checked_add(record_len(len)?)
            .ok_or(CoreError::IntegerOverflow)?;
    }
    let index_len = count
        .checked_mul(PACK_INDEX_ENTRY_BYTES)
        .ok_or(CoreError::IntegerOverflow)?;
    if index_len > MAX_PACK_INDEX_BYTES {
        return Err(CoreError::CountCap);
    }
    let pack_len = index_offset
        .checked_add(index_len)
        .and_then(|value| value.checked_add(PACK_TRAILER_BYTES))
        .ok_or(CoreError::IntegerOverflow)?;
    if pack_len > MAX_PACK_BYTES {
        return Err(CoreError::ResourceRefused);
    }
    Ok((record_count, index_offset, pack_len))
}

pub(crate) fn record_padding(object_len: u64) -> CoreResult<u8> {
    let unpadded = object_len
        .checked_add(4)
        .ok_or(CoreError::IntegerOverflow)?;
    Ok(((8 - (unpadded % 8)) % 8) as u8)
}

fn record_len(object_len: u64) -> CoreResult<u64> {
    object_len
        .checked_add(4)
        .and_then(|value| value.checked_add(u64::from(record_padding(object_len).ok()?)))
        .ok_or(CoreError::IntegerOverflow)
}

pub(crate) fn encode_header(record_count: u32, index_offset: u64) -> [u8; 64] {
    let mut bytes = [0_u8; 64];
    bytes[..8].copy_from_slice(PACK_MAGIC);
    bytes[8..10].copy_from_slice(&1_u16.to_be_bytes());
    bytes[10..12].copy_from_slice(&64_u16.to_be_bytes());
    bytes[16..48].copy_from_slice(ProfileSpecV1::frozen().id().as_bytes());
    bytes[48..52].copy_from_slice(&record_count.to_be_bytes());
    bytes[52..54].copy_from_slice(&80_u16.to_be_bytes());
    bytes[56..64].copy_from_slice(&index_offset.to_be_bytes());
    bytes
}

pub(crate) fn encode_index_entry(entry: PackIndexEntryV1) -> [u8; 80] {
    let mut bytes = [0_u8; 80];
    bytes[0] = kind_byte(entry.id.kind());
    bytes[4..36].copy_from_slice(entry.id.as_bytes());
    bytes[36..44].copy_from_slice(&entry.absolute_offset.to_be_bytes());
    bytes[44..48].copy_from_slice(&entry.object_len.to_be_bytes());
    bytes[48..80].copy_from_slice(entry.object_checksum.as_bytes());
    bytes
}

pub(crate) fn decode_index_entry(bytes: &[u8; 80]) -> CoreResult<PackIndexEntryV1> {
    if bytes[1] != 0 || be_u16(&bytes[2..4]) != 0 {
        return Err(CoreError::PackInvalid);
    }
    let kind = PhysicalObjectKindV1::try_from(bytes[0]).map_err(|_| CoreError::UnknownKind)?;
    let digest = <[u8; 32]>::try_from(&bytes[4..36]).map_err(|_| CoreError::PackInvalid)?;
    let checksum = <[u8; 32]>::try_from(&bytes[48..80]).map_err(|_| CoreError::PackInvalid)?;
    Ok(PackIndexEntryV1 {
        id: typed_id_from_digest(kind, digest),
        absolute_offset: be_u64(&bytes[36..44]),
        object_len: be_u32(&bytes[44..48]),
        object_checksum: ObjectChecksumV1::from_digest(checksum),
    })
}

pub(crate) fn encode_trailer_prefix(
    pack_len: u64,
    index_offset: u64,
    index_len: u64,
    record_count: u32,
) -> [u8; 48] {
    let mut bytes = [0_u8; 48];
    bytes[..8].copy_from_slice(PACK_TRAILER_MAGIC);
    bytes[8..10].copy_from_slice(&1_u16.to_be_bytes());
    bytes[10..12].copy_from_slice(&80_u16.to_be_bytes());
    bytes[16..24].copy_from_slice(&pack_len.to_be_bytes());
    bytes[24..32].copy_from_slice(&index_offset.to_be_bytes());
    bytes[32..40].copy_from_slice(&index_len.to_be_bytes());
    bytes[40..44].copy_from_slice(&record_count.to_be_bytes());
    bytes
}

fn append<P: PrivatePackPortV1 + ?Sized>(
    pack: &mut P,
    bytes: &[u8],
    counters: &mut OperationCountersV1,
) -> CoreResult<()> {
    pack.append(bytes).map_err(map_write_port)?;
    counters.add(CounterFieldV1::BytesWritten, bytes.len() as u64)
}

fn append_hashed<P: PrivatePackPortV1 + ?Sized>(
    pack: &mut P,
    bytes: &[u8],
    counters: &mut OperationCountersV1,
    hasher: &mut FramedHasherV1,
) -> CoreResult<()> {
    pack.append(bytes).map_err(map_write_port)?;
    hasher.write(bytes)?;
    counters.add(CounterFieldV1::BytesWritten, bytes.len() as u64)
}

pub(crate) fn hash_port_range<P: PackReadPortV1 + ?Sized>(
    pack: &mut P,
    start: u64,
    len: u64,
    tag: u8,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    counters: &mut OperationCountersV1,
) -> CoreResult<[u8; 32]> {
    let mut control = NeverStopWorkControlV1;
    hash_port_range_controlled_v1(pack, start, len, tag, scratch, counters, &mut control)
}

pub(crate) fn hash_port_range_controlled_v1<P, C>(
    pack: &mut P,
    start: u64,
    len: u64,
    tag: u8,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    counters: &mut OperationCountersV1,
    control: &mut C,
) -> CoreResult<[u8; 32]>
where
    P: PackReadPortV1 + ?Sized,
    C: crate::limits::OperationWorkControlV1 + ?Sized,
{
    hash_port_range_controlled_recorded_v1(
        pack, start, len, tag, scratch, counters, control, None, None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn hash_port_range_controlled_recorded_v1<P, C>(
    pack: &mut P,
    start: u64,
    len: u64,
    tag: u8,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    counters: &mut OperationCountersV1,
    control: &mut C,
    record_read: Option<PackStageBoundaryRecordersV1>,
    record_hash: Option<PackStageBoundaryRecordersV1>,
) -> CoreResult<[u8; 32]>
where
    P: PackReadPortV1 + ?Sized,
    C: crate::limits::OperationWorkControlV1 + ?Sized,
{
    hash_port_range_controlled_validation_recorded_v1(
        pack,
        start,
        len,
        tag,
        scratch,
        counters,
        control,
        record_read,
        record_hash,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn hash_port_range_controlled_validation_recorded_v1<P, C>(
    pack: &mut P,
    start: u64,
    len: u64,
    tag: u8,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    counters: &mut OperationCountersV1,
    control: &mut C,
    record_read: Option<PackStageBoundaryRecordersV1>,
    record_hash: Option<PackStageBoundaryRecordersV1>,
    validation_stage: Option<PackValidationStageV1>,
) -> CoreResult<[u8; 32]>
where
    P: PackReadPortV1 + ?Sized,
    C: crate::limits::OperationWorkControlV1 + ?Sized,
{
    let mut hasher = FramedHasherV1::new(tag, len);
    let mut consumed = 0_u64;
    while consumed < len {
        if let Some(stage) = validation_stage {
            poll_validation_control_v1(control, counters, stage, None, 0)?;
        } else {
            poll_work_control_v1(control)?;
        }
        let take = usize::try_from((len - consumed).min(COMPARISON_WINDOW_BYTES as u64))
            .map_err(|_| CoreError::IntegerOverflow)?;
        let offset = start
            .checked_add(consumed)
            .ok_or(CoreError::IntegerOverflow)?;
        record_pack_stage_call_v1(counters, record_read)?;
        let read = pack
            .read_exact_at(offset, &mut scratch[..take])
            .map_err(map_read_port);
        let completed_work = if read.is_ok() { take as u64 } else { 0 };
        let read =
            read.and_then(|()| record_pack_read_bytes_v1(counters, take as u64, record_read));
        if let Some(stage) = validation_stage {
            complete_validation_boundary_v1(read, completed_work, control, counters, stage, None)?;
        } else {
            complete_work_boundary_v1(read, completed_work, control, counters, None)?;
        }
        record_pack_stage_call_v1(counters, record_hash)?;
        hasher.write(&scratch[..take])?;
        if let Some(recorders) = record_hash {
            (recorders.bytes)(counters, take as u64)?;
        }
        if let Some(stage) = validation_stage {
            poll_validation_control_v1(control, counters, stage, None, take as u64)?;
        }
        consumed = consumed
            .checked_add(take as u64)
            .ok_or(CoreError::IntegerOverflow)?;
    }
    if let Some(stage) = validation_stage {
        poll_validation_control_v1(control, counters, stage, None, 0)?;
    } else {
        poll_work_control_v1(control)?;
    }
    hasher.finish()
}

fn poll_work_control_v1<C>(control: &mut C) -> CoreResult<()>
where
    C: crate::limits::OperationWorkControlV1 + ?Sized,
{
    if control.cancellation_requested_v1() {
        Err(CoreError::Cancelled)
    } else if control.deadline_exceeded_v1() {
        Err(CoreError::Deadline)
    } else {
        Ok(())
    }
}

pub(crate) type PackControlPollRecorderV1 = fn(&mut OperationCountersV1, u64) -> CoreResult<()>;
type PackStageCallRecorderV1 = fn(&mut OperationCountersV1) -> CoreResult<()>;
type PackStageBytesRecorderV1 = fn(&mut OperationCountersV1, u64) -> CoreResult<()>;

#[derive(Clone, Copy)]
pub(crate) struct PackStageBoundaryRecordersV1 {
    call: PackStageCallRecorderV1,
    bytes: PackStageBytesRecorderV1,
}

pub(crate) const PACK_FINALIZATION_READ_RECORDERS_V1: PackStageBoundaryRecordersV1 =
    PackStageBoundaryRecordersV1 {
        call: OperationCountersV1::record_pack_finalization_read_call_v1,
        bytes: OperationCountersV1::record_pack_finalization_read_bytes_v1,
    };
pub(crate) const PACK_FINALIZATION_HASH_RECORDERS_V1: PackStageBoundaryRecordersV1 =
    PackStageBoundaryRecordersV1 {
        call: OperationCountersV1::record_pack_finalization_hash_call_v1,
        bytes: OperationCountersV1::record_pack_finalization_hash_bytes_v1,
    };

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PackValidationStageV1 {
    PreInstall,
    InstalledCarrier,
}

impl PackValidationStageV1 {
    const fn read_recorders(self) -> PackStageBoundaryRecordersV1 {
        match self {
            Self::PreInstall => PackStageBoundaryRecordersV1 {
                call: OperationCountersV1::record_pre_install_validation_read_call_v1,
                bytes: OperationCountersV1::record_pre_install_validation_read_bytes_v1,
            },
            Self::InstalledCarrier => PackStageBoundaryRecordersV1 {
                call: OperationCountersV1::record_installed_carrier_validation_read_call_v1,
                bytes: OperationCountersV1::record_installed_carrier_validation_read_bytes_v1,
            },
        }
    }

    const fn hash_recorders(self) -> PackStageBoundaryRecordersV1 {
        match self {
            Self::PreInstall => PackStageBoundaryRecordersV1 {
                call: OperationCountersV1::record_pre_install_validation_hash_call_v1,
                bytes: OperationCountersV1::record_pre_install_validation_hash_bytes_v1,
            },
            Self::InstalledCarrier => PackStageBoundaryRecordersV1 {
                call: OperationCountersV1::record_installed_carrier_validation_hash_call_v1,
                bytes: OperationCountersV1::record_installed_carrier_validation_hash_bytes_v1,
            },
        }
    }

    const fn decode_recorders(self) -> PackStageBoundaryRecordersV1 {
        match self {
            Self::PreInstall => PackStageBoundaryRecordersV1 {
                call: OperationCountersV1::record_pre_install_validation_decode_call_v1,
                bytes: OperationCountersV1::record_pre_install_validation_decode_bytes_v1,
            },
            Self::InstalledCarrier => PackStageBoundaryRecordersV1 {
                call: OperationCountersV1::record_installed_carrier_validation_decode_call_v1,
                bytes: OperationCountersV1::record_installed_carrier_validation_decode_bytes_v1,
            },
        }
    }

    const fn control_poll_recorder(self) -> PackControlPollRecorderV1 {
        match self {
            Self::PreInstall => OperationCountersV1::record_pre_install_validation_control_poll_v1,
            Self::InstalledCarrier => {
                OperationCountersV1::record_installed_carrier_validation_control_poll_v1
            }
        }
    }
}

fn record_pack_stage_call_v1(
    counters: &mut OperationCountersV1,
    recorders: Option<PackStageBoundaryRecordersV1>,
) -> CoreResult<()> {
    if let Some(recorders) = recorders {
        (recorders.call)(counters)?;
    }
    Ok(())
}

fn record_pack_decode_boundary_v1(
    counters: &mut OperationCountersV1,
    recorders: PackStageBoundaryRecordersV1,
    bytes: u64,
) -> CoreResult<()> {
    let mut checked = *counters;
    (recorders.call)(&mut checked)?;
    (recorders.bytes)(&mut checked, bytes)?;
    *counters = checked;
    Ok(())
}

fn record_pack_read_bytes_v1(
    counters: &mut OperationCountersV1,
    bytes: u64,
    recorders: Option<PackStageBoundaryRecordersV1>,
) -> CoreResult<()> {
    let mut checked = *counters;
    checked.add(CounterFieldV1::BytesRead, bytes)?;
    if let Some(recorders) = recorders {
        (recorders.bytes)(&mut checked, bytes)?;
    }
    *counters = checked;
    Ok(())
}

fn poll_work_control_recorded_v1<C>(
    control: &mut C,
    counters: &mut OperationCountersV1,
    record_control_poll: Option<PackControlPollRecorderV1>,
    completed_work: u64,
) -> CoreResult<()>
where
    C: crate::limits::OperationWorkControlV1 + ?Sized,
{
    if let Some(record) = record_control_poll {
        record(counters, completed_work)?;
    }
    poll_work_control_v1(control)
}

fn complete_work_boundary_v1<T, C>(
    result: CoreResult<T>,
    completed_work: u64,
    control: &mut C,
    counters: &mut OperationCountersV1,
    record_control_poll: Option<PackControlPollRecorderV1>,
) -> CoreResult<T>
where
    C: crate::limits::OperationWorkControlV1 + ?Sized,
{
    let post_poll =
        poll_work_control_recorded_v1(control, counters, record_control_poll, completed_work);
    match (result, post_poll) {
        (Err(error), _) => Err(error),
        (Ok(value), Ok(())) => Ok(value),
        (Ok(_), Err(error)) => Err(error),
    }
}

fn poll_validation_control_v1<C>(
    control: &mut C,
    counters: &mut OperationCountersV1,
    stage: PackValidationStageV1,
    record_control_poll: Option<PackControlPollRecorderV1>,
    completed_work: u64,
) -> CoreResult<()>
where
    C: crate::limits::OperationWorkControlV1 + ?Sized,
{
    let mut checked = *counters;
    (stage.control_poll_recorder())(&mut checked, completed_work)?;
    if let Some(record) = record_control_poll {
        record(&mut checked, completed_work)?;
    }
    *counters = checked;
    poll_work_control_v1(control)
}

fn complete_validation_boundary_v1<T, C>(
    result: CoreResult<T>,
    completed_work: u64,
    control: &mut C,
    counters: &mut OperationCountersV1,
    stage: PackValidationStageV1,
    record_control_poll: Option<PackControlPollRecorderV1>,
) -> CoreResult<T>
where
    C: crate::limits::OperationWorkControlV1 + ?Sized,
{
    let post_poll = poll_validation_control_v1(
        control,
        counters,
        stage,
        record_control_poll,
        completed_work,
    );
    match (result, post_poll) {
        (Err(error), _) => Err(error),
        (Ok(value), Ok(())) => Ok(value),
        (Ok(_), Err(error)) => Err(error),
    }
}

fn read_array_validation_controlled_v1<const N: usize, P, C>(
    pack: &mut P,
    offset: u64,
    counters: &mut OperationCountersV1,
    control: &mut C,
    stage: PackValidationStageV1,
    record_control_poll: Option<PackControlPollRecorderV1>,
) -> CoreResult<[u8; N]>
where
    P: PackReadPortV1 + ?Sized,
    C: crate::limits::OperationWorkControlV1 + ?Sized,
{
    poll_validation_control_v1(control, counters, stage, record_control_poll, 0)?;
    let mut bytes = [0_u8; N];
    record_pack_stage_call_v1(counters, Some(stage.read_recorders()))?;
    let read = pack
        .read_exact_at(offset, &mut bytes)
        .map_err(map_read_port);
    let completed_work = if read.is_ok() { N as u64 } else { 0 };
    let read = read
        .and_then(|()| record_pack_read_bytes_v1(counters, N as u64, Some(stage.read_recorders())));
    complete_validation_boundary_v1(
        read,
        completed_work,
        control,
        counters,
        stage,
        record_control_poll,
    )?;
    Ok(bytes)
}

fn read_array_controlled_v1<const N: usize, P, C>(
    pack: &mut P,
    offset: u64,
    counters: &mut OperationCountersV1,
    control: &mut C,
    record_control_poll: Option<PackControlPollRecorderV1>,
    record_read: Option<PackStageBoundaryRecordersV1>,
) -> CoreResult<[u8; N]>
where
    P: PackReadPortV1 + ?Sized,
    C: crate::limits::OperationWorkControlV1 + ?Sized,
{
    poll_work_control_recorded_v1(control, counters, record_control_poll, 0)?;
    let mut bytes = [0_u8; N];
    record_pack_stage_call_v1(counters, record_read)?;
    let read = pack
        .read_exact_at(offset, &mut bytes)
        .map_err(map_read_port);
    let completed_work = if read.is_ok() { N as u64 } else { 0 };
    let read = read.and_then(|()| record_pack_read_bytes_v1(counters, N as u64, record_read));
    complete_work_boundary_v1(read, completed_work, control, counters, record_control_poll)?;
    Ok(bytes)
}

fn read_array<const N: usize, P: PackReadPortV1 + ?Sized>(
    pack: &mut P,
    offset: u64,
    counters: &mut OperationCountersV1,
) -> CoreResult<[u8; N]> {
    read_array_recorded_v1(pack, offset, counters, None)
}

fn read_array_recorded_v1<const N: usize, P: PackReadPortV1 + ?Sized>(
    pack: &mut P,
    offset: u64,
    counters: &mut OperationCountersV1,
    record_read: Option<PackStageBoundaryRecordersV1>,
) -> CoreResult<[u8; N]> {
    let mut bytes = [0_u8; N];
    record_pack_stage_call_v1(counters, record_read)?;
    pack.read_exact_at(offset, &mut bytes)
        .map_err(map_read_port)?;
    record_pack_read_bytes_v1(counters, N as u64, record_read)?;
    Ok(bytes)
}

const fn kind_byte(kind: PhysicalObjectKindV1) -> u8 {
    match kind {
        PhysicalObjectKindV1::VersionRecord => 0x01,
        PhysicalObjectKindV1::Tree => 0x02,
        PhysicalObjectKindV1::File => 0x03,
        PhysicalObjectKindV1::Symlink => 0x04,
        PhysicalObjectKindV1::Chunk => 0x05,
    }
}

fn compare_typed_key(left: TypedPhysicalObjectIdV1, right: TypedPhysicalObjectIdV1) -> Ordering {
    kind_byte(left.kind())
        .cmp(&kind_byte(right.kind()))
        .then_with(|| left.as_bytes().cmp(right.as_bytes()))
}

fn typed_id_from_digest(kind: PhysicalObjectKindV1, digest: [u8; 32]) -> TypedPhysicalObjectIdV1 {
    match kind {
        PhysicalObjectKindV1::VersionRecord => {
            TypedPhysicalObjectIdV1::VersionRecord(PhysicalVersionRecordIdV1::from_digest(digest))
        }
        PhysicalObjectKindV1::Tree => {
            TypedPhysicalObjectIdV1::Tree(PhysicalTreeIdV1::from_digest(digest))
        }
        PhysicalObjectKindV1::File => {
            TypedPhysicalObjectIdV1::File(PhysicalFileIdV1::from_digest(digest))
        }
        PhysicalObjectKindV1::Symlink => {
            TypedPhysicalObjectIdV1::Symlink(PhysicalSymlinkIdV1::from_digest(digest))
        }
        PhysicalObjectKindV1::Chunk => {
            TypedPhysicalObjectIdV1::Chunk(PhysicalChunkIdV1::from_digest(digest))
        }
    }
}

fn be_u16(bytes: &[u8]) -> u16 {
    u16::from_be_bytes(bytes.try_into().expect("fixed two-byte field"))
}

fn be_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes(bytes.try_into().expect("fixed four-byte field"))
}

fn be_u64(bytes: &[u8]) -> u64 {
    u64::from_be_bytes(bytes.try_into().expect("fixed eight-byte field"))
}

const fn map_write_port(error: PackPortErrorV1) -> CoreError {
    match error {
        PackPortErrorV1::Failure => CoreError::SinkRefused,
        PackPortErrorV1::Cancelled => CoreError::Cancelled,
        PackPortErrorV1::Deadline => CoreError::Deadline,
        PackPortErrorV1::WorkExhausted => CoreError::ResourceRefused,
    }
}

const fn map_read_port(error: PackPortErrorV1) -> CoreError {
    match error {
        PackPortErrorV1::Failure => CoreError::SourceFailure,
        PackPortErrorV1::Cancelled => CoreError::Cancelled,
        PackPortErrorV1::Deadline => CoreError::Deadline,
        PackPortErrorV1::WorkExhausted => CoreError::ResourceRefused,
    }
}

const fn map_spool_port(error: PackPortErrorV1) -> CoreError {
    match error {
        PackPortErrorV1::Failure | PackPortErrorV1::WorkExhausted => CoreError::ResourceRefused,
        PackPortErrorV1::Cancelled => CoreError::Cancelled,
        PackPortErrorV1::Deadline => CoreError::Deadline,
    }
}

#[cfg(feature = "operation-polymorphism")]
pub mod semantic {
    use super::{
        build_dense_pack_v1, validate_pack_v1, PackIndexEntryV1, PackIndexSpoolV1,
        PackObjectSourceV1, PackPortErrorV1, PackReadPortV1, PrivatePackPortV1, SealedPackV1,
        MAX_PACK_BYTES,
    };
    use crate::limits::{OperationCountersV1, ResourceLedgerV1, OPERATION_SLOT_BYTES};
    use crate::object::{decode_physical_object_v1, DiscardStrongEdgesV1, TypedPhysicalObjectIdV1};
    use crate::{CoreError, CoreResult};

    #[derive(Clone, Copy, Debug)]
    pub struct PackRequestV1<'a> {
        objects: &'a [&'a [u8]],
        source_resident_bytes: Option<u64>,
        pack_resident_bytes: Option<u64>,
        index_resident_bytes: Option<u64>,
        sink_failure_after: Option<usize>,
    }

    impl<'a> PackRequestV1<'a> {
        pub const fn new(objects: &'a [&'a [u8]]) -> Self {
            Self {
                objects,
                source_resident_bytes: None,
                pack_resident_bytes: None,
                index_resident_bytes: None,
                sink_failure_after: None,
            }
        }

        pub const fn with_source_residency(mut self, bytes: u64) -> Self {
            self.source_resident_bytes = Some(bytes);
            self
        }

        pub const fn with_pack_residency(mut self, bytes: u64) -> Self {
            self.pack_resident_bytes = Some(bytes);
            self
        }

        pub const fn with_index_residency(mut self, bytes: u64) -> Self {
            self.index_resident_bytes = Some(bytes);
            self
        }

        pub const fn with_sink_failure_after(mut self, bytes: usize) -> Self {
            self.sink_failure_after = Some(bytes);
            self
        }
    }

    #[derive(Clone, Copy, Debug)]
    pub struct ValidationRequestV1<'a> {
        bytes: &'a [u8],
        maximum_entries: u32,
        pack_resident_bytes: Option<u64>,
        fail_reads: bool,
    }

    impl<'a> ValidationRequestV1<'a> {
        pub const fn new(bytes: &'a [u8]) -> Self {
            Self {
                bytes,
                maximum_entries: 10_000,
                pack_resident_bytes: None,
                fail_reads: false,
            }
        }

        pub const fn with_maximum_entries(mut self, maximum_entries: u32) -> Self {
            self.maximum_entries = maximum_entries;
            self
        }

        pub const fn with_pack_residency(mut self, bytes: u64) -> Self {
            self.pack_resident_bytes = Some(bytes);
            self
        }

        pub const fn with_fail_reads(mut self, fail_reads: bool) -> Self {
            self.fail_reads = fail_reads;
            self
        }
    }

    #[derive(Debug)]
    pub struct PackObservationV1 {
        error: Option<CoreError>,
        bytes: Vec<u8>,
        sealed: bool,
        aborted: bool,
        begins: u64,
        len_calls: u64,
        source_metadata_reads: u64,
        source_payload_bytes_read: u64,
        spool_peak: u64,
        pack_entries: u64,
        pack_bytes: u64,
        bytes_read: u64,
        bytes_written: u64,
        memory_high_water: u64,
        pack_len: u64,
        record_count: u32,
        index_offset: u64,
        pack_id: [u8; 32],
        ledger_high_water: u64,
        admitted_slots: u64,
    }

    impl PackObservationV1 {
        pub const fn error(&self) -> Option<CoreError> {
            self.error
        }
        pub fn bytes(&self) -> &[u8] {
            &self.bytes
        }
        pub const fn sealed(&self) -> bool {
            self.sealed
        }
        pub const fn aborted(&self) -> bool {
            self.aborted
        }
        pub const fn begins(&self) -> u64 {
            self.begins
        }
        pub const fn len_calls(&self) -> u64 {
            self.len_calls
        }
        pub const fn source_metadata_reads(&self) -> u64 {
            self.source_metadata_reads
        }
        pub const fn source_payload_bytes_read(&self) -> u64 {
            self.source_payload_bytes_read
        }
        pub const fn spool_peak(&self) -> u64 {
            self.spool_peak
        }
        pub const fn pack_entries(&self) -> u64 {
            self.pack_entries
        }
        pub const fn pack_bytes(&self) -> u64 {
            self.pack_bytes
        }
        pub const fn bytes_read(&self) -> u64 {
            self.bytes_read
        }
        pub const fn bytes_written(&self) -> u64 {
            self.bytes_written
        }
        pub const fn memory_high_water(&self) -> u64 {
            self.memory_high_water
        }
        pub const fn pack_len(&self) -> u64 {
            self.pack_len
        }
        pub const fn record_count(&self) -> u32 {
            self.record_count
        }
        pub const fn index_offset(&self) -> u64 {
            self.index_offset
        }
        pub const fn pack_id(&self) -> [u8; 32] {
            self.pack_id
        }
        pub const fn ledger_high_water(&self) -> u64 {
            self.ledger_high_water
        }
        pub const fn admitted_slots(&self) -> u64 {
            self.admitted_slots
        }
    }

    #[derive(Debug)]
    struct ObjectSource<'a> {
        bytes: &'a [&'a [u8]],
        ids: Vec<TypedPhysicalObjectIdV1>,
        reported_resident_bytes: Option<u64>,
        metadata_reads: u64,
        payload_bytes_read: u64,
    }

    impl<'a> ObjectSource<'a> {
        fn new(bytes: &'a [&'a [u8]], reported_resident_bytes: Option<u64>) -> Self {
            Self {
                bytes,
                ids: bytes
                    .iter()
                    .map(|value| {
                        decode_physical_object_v1(value, &mut DiscardStrongEdgesV1)
                            .expect("semantic pack request contains canonical objects")
                            .physical_id()
                            .expect("canonical object has a physical id")
                    })
                    .collect(),
                reported_resident_bytes,
                metadata_reads: 0,
                payload_bytes_read: 0,
            }
        }
    }

    impl PackObjectSourceV1 for ObjectSource<'_> {
        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            self.reported_resident_bytes.map_or_else(
                || {
                    u64::try_from(self.ids.capacity())
                        .map_err(|_| CoreError::IntegerOverflow)?
                        .checked_mul(core::mem::size_of::<TypedPhysicalObjectIdV1>() as u64)
                        .ok_or(CoreError::IntegerOverflow)
                },
                Ok,
            )
        }

        fn declared_object_count(&self) -> CoreResult<u32> {
            u32::try_from(self.bytes.len()).map_err(|_| CoreError::IntegerOverflow)
        }

        fn object_id(&mut self, ordinal: u32) -> Result<TypedPhysicalObjectIdV1, PackPortErrorV1> {
            self.metadata_reads = self
                .metadata_reads
                .checked_add(1)
                .ok_or(PackPortErrorV1::Failure)?;
            self.ids
                .get(ordinal as usize)
                .copied()
                .ok_or(PackPortErrorV1::Failure)
        }

        fn object_len(&mut self, ordinal: u32) -> Result<u64, PackPortErrorV1> {
            self.metadata_reads = self
                .metadata_reads
                .checked_add(1)
                .ok_or(PackPortErrorV1::Failure)?;
            self.bytes
                .get(ordinal as usize)
                .ok_or(PackPortErrorV1::Failure)
                .and_then(|bytes| u64::try_from(bytes.len()).map_err(|_| PackPortErrorV1::Failure))
        }

        fn read_object_exact_at(
            &mut self,
            ordinal: u32,
            offset: u64,
            destination: &mut [u8],
        ) -> Result<(), PackPortErrorV1> {
            let bytes = self
                .bytes
                .get(ordinal as usize)
                .ok_or(PackPortErrorV1::Failure)?;
            let start = usize::try_from(offset).map_err(|_| PackPortErrorV1::Failure)?;
            let end = start
                .checked_add(destination.len())
                .ok_or(PackPortErrorV1::Failure)?;
            destination.copy_from_slice(bytes.get(start..end).ok_or(PackPortErrorV1::Failure)?);
            self.payload_bytes_read = self
                .payload_bytes_read
                .checked_add(destination.len() as u64)
                .ok_or(PackPortErrorV1::Failure)?;
            Ok(())
        }
    }

    #[derive(Debug, Default)]
    struct PackMemory {
        bytes: Vec<u8>,
        reported_resident_bytes: Option<u64>,
        expected_len: u64,
        sealed: bool,
        aborted: bool,
        begins: u64,
        len_calls: u64,
        fail_after: Option<usize>,
        fail_reads: bool,
    }

    impl PackReadPortV1 for PackMemory {
        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            Ok(self.reported_resident_bytes.unwrap_or(0))
        }
        fn len(&mut self) -> Result<u64, PackPortErrorV1> {
            self.len_calls = self
                .len_calls
                .checked_add(1)
                .ok_or(PackPortErrorV1::Failure)?;
            u64::try_from(self.bytes.len()).map_err(|_| PackPortErrorV1::Failure)
        }
        fn read_exact_at(
            &mut self,
            offset: u64,
            destination: &mut [u8],
        ) -> Result<(), PackPortErrorV1> {
            if self.fail_reads {
                return Err(PackPortErrorV1::Failure);
            }
            let start = usize::try_from(offset).map_err(|_| PackPortErrorV1::Failure)?;
            let end = start
                .checked_add(destination.len())
                .ok_or(PackPortErrorV1::Failure)?;
            destination
                .copy_from_slice(self.bytes.get(start..end).ok_or(PackPortErrorV1::Failure)?);
            Ok(())
        }
    }

    impl PrivatePackPortV1 for PackMemory {
        fn begin_private(&mut self, exact_len: u64) -> Result<(), PackPortErrorV1> {
            if exact_len > MAX_PACK_BYTES || self.begins != 0 {
                return Err(PackPortErrorV1::Failure);
            }
            self.expected_len = exact_len;
            self.begins += 1;
            Ok(())
        }
        fn append(&mut self, bytes: &[u8]) -> Result<(), PackPortErrorV1> {
            if self
                .fail_after
                .is_some_and(|limit| self.bytes.len() >= limit)
            {
                return Err(PackPortErrorV1::Failure);
            }
            let next = self
                .bytes
                .len()
                .checked_add(bytes.len())
                .ok_or(PackPortErrorV1::Failure)?;
            if u64::try_from(next).map_err(|_| PackPortErrorV1::Failure)? > self.expected_len {
                return Err(PackPortErrorV1::Failure);
            }
            self.bytes.extend_from_slice(bytes);
            Ok(())
        }
        fn seal_private(&mut self, _id: crate::identity::PackIdV1) -> Result<(), PackPortErrorV1> {
            if u64::try_from(self.bytes.len()).map_err(|_| PackPortErrorV1::Failure)?
                != self.expected_len
            {
                return Err(PackPortErrorV1::Failure);
            }
            self.sealed = true;
            Ok(())
        }
        fn abort_private(&mut self) {
            self.aborted = true;
            self.sealed = false;
        }
    }

    #[derive(Debug, Default)]
    struct MetadataSpool {
        entries: Vec<PackIndexEntryV1>,
        cursor: usize,
        maximum: usize,
        peak: usize,
        reported_resident_bytes: Option<u64>,
    }

    impl PackIndexSpoolV1 for MetadataSpool {
        fn resident_memory_bound_bytes(&self, maximum_entries: u32) -> CoreResult<u64> {
            self.reported_resident_bytes.map_or_else(
                || {
                    u64::from(maximum_entries)
                        .checked_mul(core::mem::size_of::<PackIndexEntryV1>() as u64)
                        .ok_or(CoreError::IntegerOverflow)
                },
                Ok,
            )
        }
        fn reset(&mut self, maximum_entries: u32) -> Result<(), PackPortErrorV1> {
            self.entries.clear();
            self.cursor = 0;
            self.maximum = maximum_entries as usize;
            Ok(())
        }
        fn push(&mut self, entry: PackIndexEntryV1) -> Result<(), PackPortErrorV1> {
            if self.entries.len() >= self.maximum {
                return Err(PackPortErrorV1::Failure);
            }
            self.entries.push(entry);
            self.peak = self.peak.max(self.entries.len());
            Ok(())
        }
        fn sort_by_key(&mut self) -> Result<(), PackPortErrorV1> {
            self.entries.sort_by(PackIndexEntryV1::compare_key);
            Ok(())
        }
        fn sort_by_offset(&mut self) -> Result<(), PackPortErrorV1> {
            self.entries.sort_by(PackIndexEntryV1::compare_offset);
            Ok(())
        }
        fn rewind(&mut self) -> Result<(), PackPortErrorV1> {
            self.cursor = 0;
            Ok(())
        }
        fn next(&mut self) -> Result<Option<PackIndexEntryV1>, PackPortErrorV1> {
            let next = self.entries.get(self.cursor).copied();
            self.cursor = self
                .cursor
                .checked_add(usize::from(next.is_some()))
                .ok_or(PackPortErrorV1::Failure)?;
            Ok(next)
        }
        fn abort(&mut self) {
            self.entries.clear();
        }
    }

    fn observe(
        error: Option<CoreError>,
        pack: PackMemory,
        source: Option<ObjectSource<'_>>,
        spool: MetadataSpool,
        counters: OperationCountersV1,
        sealed: Option<SealedPackV1>,
        ledger: ResourceLedgerV1,
    ) -> PackObservationV1 {
        let (pack_len, record_count, index_offset, pack_id) =
            sealed.map_or((0, 0, 0, [0; 32]), |sealed| {
                (
                    sealed.pack_len(),
                    sealed.record_count(),
                    sealed.index_offset(),
                    *sealed.id().as_bytes(),
                )
            });
        PackObservationV1 {
            error,
            bytes: pack.bytes,
            sealed: pack.sealed,
            aborted: pack.aborted,
            begins: pack.begins,
            len_calls: pack.len_calls,
            source_metadata_reads: source.as_ref().map_or(0, |source| source.metadata_reads),
            source_payload_bytes_read: source
                .as_ref()
                .map_or(0, |source| source.payload_bytes_read),
            spool_peak: spool.peak as u64,
            pack_entries: counters.pack_entries,
            pack_bytes: counters.pack_bytes,
            bytes_read: counters.bytes_read,
            bytes_written: counters.bytes_written,
            memory_high_water: counters.memory_high_water,
            pack_len,
            record_count,
            index_offset,
            pack_id,
            ledger_high_water: ledger.high_water_bytes(),
            admitted_slots: ledger.admitted_slots(),
        }
    }

    pub fn build_v1(request: PackRequestV1<'_>) -> PackObservationV1 {
        let mut source = ObjectSource::new(request.objects, request.source_resident_bytes);
        let mut pack = PackMemory {
            reported_resident_bytes: request.pack_resident_bytes,
            fail_after: request.sink_failure_after,
            ..PackMemory::default()
        };
        let mut spool = MetadataSpool {
            reported_resident_bytes: request.index_resident_bytes,
            ..MetadataSpool::default()
        };
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut counters = OperationCountersV1::default();
        let mut scratch = [0_u8; 65_536];
        let result = build_dense_pack_v1(
            &mut source,
            &mut pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut scratch,
        );
        observe(
            result.as_ref().err().copied(),
            pack,
            Some(source),
            spool,
            counters,
            result.ok(),
            ledger,
        )
    }

    pub fn validate_v1(request: ValidationRequestV1<'_>) -> PackObservationV1 {
        let mut pack = PackMemory {
            bytes: request.bytes.to_vec(),
            reported_resident_bytes: request.pack_resident_bytes,
            fail_reads: request.fail_reads,
            ..PackMemory::default()
        };
        let mut spool = MetadataSpool::default();
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut counters = OperationCountersV1::default();
        let mut scratch = [0_u8; 65_536];
        let result = validate_pack_v1(
            &mut pack,
            &mut spool,
            &mut scratch,
            request.maximum_entries,
            &ledger,
            &mut counters,
        );
        observe(
            result.as_ref().err().copied(),
            pack,
            None,
            spool,
            counters,
            result.ok(),
            ledger,
        )
    }

    pub const fn operation_slot_bytes() -> u64 {
        OPERATION_SLOT_BYTES
    }
}

const fn map_object_validation(error: CoreError) -> CoreError {
    match error {
        CoreError::SourceFailure
        | CoreError::SinkRefused
        | CoreError::ResourceRefused
        | CoreError::Cancelled
        | CoreError::Deadline
        | CoreError::Schema
        | CoreError::TypeDomain
        | CoreError::UnknownKind
        | CoreError::IdMismatch => error,
        _ => CoreError::PackInvalid,
    }
}

const _: () = assert!(MAX_PHYSICAL_OBJECT_BYTES < u32::MAX as u64);

#[cfg(test)]
mod port_error_mapping_tests {
    use super::*;

    struct StopOnSecondPollV1 {
        cancellation_checks: u64,
    }

    impl crate::limits::OperationWorkControlV1 for StopOnSecondPollV1 {
        fn cancellation_requested_v1(&mut self) -> bool {
            self.cancellation_checks += 1;
            self.cancellation_checks >= 2
        }

        fn deadline_exceeded_v1(&mut self) -> bool {
            false
        }
    }

    struct ShapeReadPortV1 {
        bytes: [u8; 144],
        reads: Vec<(u64, usize)>,
    }

    impl PackReadPortV1 for ShapeReadPortV1 {
        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            Ok(0)
        }

        fn len(&mut self) -> Result<u64, PackPortErrorV1> {
            Ok(self.bytes.len() as u64)
        }

        fn read_exact_at(
            &mut self,
            offset: u64,
            destination: &mut [u8],
        ) -> Result<(), PackPortErrorV1> {
            let start = usize::try_from(offset).map_err(|_| PackPortErrorV1::Failure)?;
            let end = start
                .checked_add(destination.len())
                .ok_or(PackPortErrorV1::Failure)?;
            destination
                .copy_from_slice(self.bytes.get(start..end).ok_or(PackPortErrorV1::Failure)?);
            self.reads.push((offset, destination.len()));
            Ok(())
        }
    }

    struct FailingReadPortV1;

    impl PackReadPortV1 for FailingReadPortV1 {
        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            Ok(0)
        }

        fn len(&mut self) -> Result<u64, PackPortErrorV1> {
            Ok(64)
        }

        fn read_exact_at(
            &mut self,
            _offset: u64,
            _destination: &mut [u8],
        ) -> Result<(), PackPortErrorV1> {
            Err(PackPortErrorV1::Failure)
        }
    }

    struct FailingResetSpoolV1;

    impl PackIndexSpoolV1 for FailingResetSpoolV1 {
        fn resident_memory_bound_bytes(&self, _maximum_entries: u32) -> CoreResult<u64> {
            Ok(0)
        }

        fn reset(&mut self, _maximum_entries: u32) -> Result<(), PackPortErrorV1> {
            Err(PackPortErrorV1::Failure)
        }

        fn push(&mut self, _entry: PackIndexEntryV1) -> Result<(), PackPortErrorV1> {
            unreachable!()
        }

        fn sort_by_key(&mut self) -> Result<(), PackPortErrorV1> {
            unreachable!()
        }

        fn sort_by_offset(&mut self) -> Result<(), PackPortErrorV1> {
            unreachable!()
        }

        fn rewind(&mut self) -> Result<(), PackPortErrorV1> {
            unreachable!()
        }

        fn next(&mut self) -> Result<Option<PackIndexEntryV1>, PackPortErrorV1> {
            unreachable!()
        }

        fn abort(&mut self) {}
    }

    #[test]
    fn read_and_write_ports_preserve_control_and_resource_causes() {
        assert_eq!(
            map_read_port(PackPortErrorV1::Failure),
            CoreError::SourceFailure
        );
        assert_eq!(
            map_write_port(PackPortErrorV1::Failure),
            CoreError::SinkRefused
        );

        for (port_error, expected) in [
            (PackPortErrorV1::Cancelled, CoreError::Cancelled),
            (PackPortErrorV1::Deadline, CoreError::Deadline),
            (PackPortErrorV1::WorkExhausted, CoreError::ResourceRefused),
        ] {
            assert_eq!(map_read_port(port_error), expected);
            assert_eq!(map_write_port(port_error), expected);
        }
    }

    #[test]
    fn sealed_shape_decoder_preserves_exact_output_and_read_order() {
        let mut bytes = [0_u8; 144];
        bytes[48..52].copy_from_slice(&7_u32.to_be_bytes());
        bytes[56..64].copy_from_slice(&96_u64.to_be_bytes());
        let digest = [0xa5_u8; DIGEST_BYTES];
        bytes[112..144].copy_from_slice(&digest);
        let mut port = ShapeReadPortV1 {
            bytes,
            reads: Vec::new(),
        };

        let sealed = read_sealed_pack_shape_v1(&mut port).unwrap();

        assert_eq!(sealed.id(), PackIdV1::from_digest(digest));
        assert_eq!(sealed.pack_len(), 144);
        assert_eq!(sealed.record_count(), 7);
        assert_eq!(sealed.index_offset(), 96);
        assert_eq!(port.reads, [(0, 64), (112, 32)]);
    }

    #[test]
    fn validation_poll_tuple_is_transactional_when_outer_recorder_overflows() {
        let mut counters = OperationCountersV1 {
            pre_install_validation_control_polls: 11,
            pre_install_validation_maximum_work_between_polls: 13,
            closure_validation_control_polls: u64::MAX,
            closure_validation_maximum_work_between_polls: 17,
            ..OperationCountersV1::default()
        };
        let before = counters;
        let mut control = NeverStopWorkControlV1;

        assert_eq!(
            poll_validation_control_v1(
                &mut control,
                &mut counters,
                PackValidationStageV1::PreInstall,
                Some(OperationCountersV1::record_closure_validation_control_poll_v1),
                19,
            ),
            Err(CoreError::IntegerOverflow)
        );
        assert_eq!(counters, before);
    }

    #[test]
    fn decoder_call_and_consumed_window_tuple_is_transactional_on_overflow() {
        let mut counters = OperationCountersV1 {
            locator_index_decode_calls: 11,
            locator_index_decode_bytes: u64::MAX,
            ..OperationCountersV1::default()
        };
        let before = counters;

        assert_eq!(
            record_pack_decode_boundary_v1(
                &mut counters,
                PackStageBoundaryRecordersV1 {
                    call: OperationCountersV1::record_locator_index_decode_call_v1,
                    bytes: OperationCountersV1::record_locator_index_decode_bytes_v1,
                },
                PACK_INDEX_ENTRY_BYTES,
            ),
            Err(CoreError::IntegerOverflow)
        );
        assert_eq!(counters, before);
    }

    #[test]
    fn validation_filesystem_and_spool_errors_precede_later_cancellation() {
        let mut counters = OperationCountersV1::default();
        let mut control = StopOnSecondPollV1 {
            cancellation_checks: 0,
        };
        assert_eq!(
            read_array_validation_controlled_v1::<4, _, _>(
                &mut FailingReadPortV1,
                0,
                &mut counters,
                &mut control,
                PackValidationStageV1::PreInstall,
                None,
            ),
            Err(CoreError::SourceFailure)
        );
        assert_eq!(counters.pre_install_validation_read_calls, 1);
        assert_eq!(counters.pre_install_validation_read_bytes, 0);
        assert_eq!(counters.pre_install_validation_control_polls, 2);

        let mut counters = OperationCountersV1::default();
        let mut control = StopOnSecondPollV1 {
            cancellation_checks: 0,
        };
        assert_eq!(
            reset_validation_spool_v1(
                &mut FailingResetSpoolV1,
                1,
                &mut counters,
                &mut control,
                PackValidationStageV1::InstalledCarrier,
            ),
            Err(CoreError::ResourceRefused)
        );
        assert_eq!(counters.installed_carrier_validation_control_polls, 2);
    }

    #[test]
    fn malformed_index_decode_records_the_consumed_fixed_window() {
        let mut port = ShapeReadPortV1 {
            bytes: [0; 144],
            reads: Vec::new(),
        };
        let sealed = SealedPackV1::from_validated_parts(PackIdV1::from_digest([0; 32]), 144, 1, 0);
        let mut counters = OperationCountersV1::default();
        let mut control = NeverStopWorkControlV1;

        assert_eq!(
            read_validated_pack_index_entry_controlled_v1(
                &mut port,
                sealed,
                0,
                &mut counters,
                &mut control,
            ),
            Err(CoreError::UnknownKind)
        );
        assert_eq!(counters.locator_index_read_calls, 1);
        assert_eq!(counters.locator_index_read_bytes, PACK_INDEX_ENTRY_BYTES);
        assert_eq!(counters.locator_index_decode_calls, 1);
        assert_eq!(counters.locator_index_decode_bytes, PACK_INDEX_ENTRY_BYTES);
    }

    #[test]
    fn malformed_object_records_successfully_consumed_decoder_prefix_bytes() {
        let object_len = OBJECT_HEADER_BYTES + 1;
        let mut bytes = [0_u8; 144];
        bytes[..4].copy_from_slice(&(object_len as u32).to_be_bytes());
        let mut port = ShapeReadPortV1 {
            bytes,
            reads: Vec::new(),
        };
        let entry = PackIndexEntryV1::from_validated_parts(
            typed_id_from_digest(PhysicalObjectKindV1::Chunk, [0; 32]),
            0,
            object_len as u32,
            ObjectChecksumV1::from_digest([0; 32]),
        );
        let mut counters = OperationCountersV1::default();
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];

        assert!(validate_record(
            &mut port,
            entry,
            ProfileSpecV1::frozen().id(),
            &mut scratch,
            &mut counters,
        )
        .is_err());
        assert_eq!(counters.pre_install_validation_decode_calls, 1);
        assert_eq!(
            counters.pre_install_validation_decode_bytes,
            OBJECT_HEADER_BYTES
        );
        assert_eq!(
            counters.pre_install_validation_read_bytes,
            4 + OBJECT_HEADER_BYTES
        );
    }
}
