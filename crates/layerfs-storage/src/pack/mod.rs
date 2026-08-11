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
    COMPARISON_WINDOW_BYTES, IDENTITY_HASHER_BYTES_V1, TAG_OBJECT_CHECKSUM, TAG_PACK,
};
#[cfg(test)]
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

#[cfg(feature = "c3-polymorphism")]
use crate::cas::FsPackAdmissionOutcomeV1;

#[cfg(feature = "c3-polymorphism")]
mod complete_writer;
#[cfg(feature = "c3-polymorphism")]
mod operation_index;

#[cfg(feature = "c3-polymorphism")]
pub(crate) use complete_writer::DirectPackSinkV1;
#[cfg(feature = "c3-polymorphism")]
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
#[cfg(feature = "c3-polymorphism")]
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

#[cfg(feature = "c3-polymorphism")]
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
    #[cfg(any(test, feature = "c3-polymorphism"))]
    #[doc(hidden)]
    fn take_storage_error_typed_v1(&mut self) -> Option<crate::cas::FsCasErrorV1> {
        None
    }
    fn reset(&mut self, maximum_entries: u32) -> Result<(), PackPortErrorV1>;
    fn push(&mut self, entry: PackIndexEntryV1) -> Result<(), PackPortErrorV1>;
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
    if pack.len().map_err(map_read_port)? != sealed.pack_len {
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
        let entry = decode_index_entry(&read_array::<80, _>(pack, offset, counters)?)?;
        match compare_typed_key(entry.id, expected) {
            Ordering::Less => low = middle.checked_add(1).ok_or(CoreError::IntegerOverflow)?,
            Ordering::Greater => high = middle,
            Ordering::Equal => return Ok(Some(entry)),
        }
    }
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
    if ordinal >= sealed.record_count || pack.len().map_err(map_read_port)? != sealed.pack_len {
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
    decode_index_entry(&read_array::<80, _>(pack, offset, counters)?)
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
    validate_record(pack, entry, ProfileSpecV1::frozen().id(), scratch, counters)?;
    Ok(PackObjectLocationV1 {
        object_offset: entry
            .absolute_offset
            .checked_add(4)
            .ok_or(CoreError::IntegerOverflow)?,
        object_len: u64::from(entry.object_len),
    })
}

#[cfg(test)]
pub fn build_dense_pack_v1<O, P, M>(
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

#[cfg(test)]
pub fn validate_pack_v1<P, M>(
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
    )
}

#[cfg(any(test, feature = "c3-polymorphism"))]
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
    validate_pack_inner_v1(pack, metadata, scratch, counters, maximum_entries, control)
}

fn validate_pack_inner_v1<P, M>(
    pack: &mut P,
    metadata: &mut M,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    counters: &mut OperationCountersV1,
    maximum_entries: u32,
    control: &mut dyn crate::limits::OperationWorkControlV1,
) -> CoreResult<SealedPackV1>
where
    P: PackReadPortV1 + ?Sized,
    M: PackIndexSpoolV1 + ?Sized,
{
    let pack_len = pack.len().map_err(map_read_port)?;
    if !(PACK_HEADER_BYTES + PACK_TRAILER_BYTES..=MAX_PACK_BYTES).contains(&pack_len) {
        return Err(CoreError::PackInvalid);
    }
    let header = read_array::<64, _>(pack, 0, counters)?;
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
    let trailer = read_array::<80, _>(pack, trailer_offset, counters)?;
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
    let digest = hash_port_range(pack, 0, checksum_len, TAG_PACK, scratch, counters)?;
    if trailer[48..80] != digest {
        return Err(CoreError::IdMismatch);
    }

    metadata.reset(record_count).map_err(map_spool_port)?;
    let validation = validate_index_and_records(
        pack,
        metadata,
        scratch,
        counters,
        record_count,
        index_offset,
        profile,
        control,
    );
    if validation.is_err() {
        metadata.abort();
    }
    validation?;
    Ok(SealedPackV1 {
        id: PackIdV1::from_digest(digest),
        pack_len,
        record_count,
        index_offset,
    })
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
) -> CoreResult<()>
where
    P: PackReadPortV1 + ?Sized,
    M: PackIndexSpoolV1 + ?Sized,
{
    let mut previous: Option<PackIndexEntryV1> = None;
    for ordinal in 0..record_count {
        let offset = index_offset
            .checked_add(
                u64::from(ordinal)
                    .checked_mul(PACK_INDEX_ENTRY_BYTES)
                    .ok_or(CoreError::IntegerOverflow)?,
            )
            .ok_or(CoreError::IntegerOverflow)?;
        let bytes = read_array::<80, _>(pack, offset, counters)?;
        let entry = decode_index_entry(&bytes)?;
        if previous.is_some_and(|left| left.compare_key(&entry) != Ordering::Less) {
            return Err(CoreError::PackInvalid);
        }
        metadata.push(entry).map_err(map_spool_port)?;
        previous = Some(entry);
    }
    metadata
        .sort_by_offset_controlled(control, counters)
        .map_err(map_spool_port)?;
    metadata.rewind().map_err(map_spool_port)?;
    let mut expected_offset = PACK_HEADER_BYTES;
    let mut consumed = 0_u32;
    while let Some(entry) = metadata.next().map_err(map_spool_port)? {
        if entry.absolute_offset != expected_offset {
            return Err(CoreError::PackInvalid);
        }
        validate_record(pack, entry, profile, scratch, counters)?;
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
    let prefix = read_array::<4, _>(pack, entry.absolute_offset, counters)?;
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
        };
        let decoded =
            decode_physical_object_from_port_v1(&mut object, &mut DiscardStrongEdgesV1, scratch)
                .map_err(map_object_validation)?;
        if object.next_offset != object_len {
            return Err(CoreError::PackInvalid);
        }
        decoded
    };
    let checksum = ObjectChecksumV1::from_digest(checksum_hasher.finish()?);
    if decoded.header().profile_id() != profile || decoded.header().kind() != entry.id.kind() {
        return Err(CoreError::TypeDomain);
    }
    if decoded.physical_id() != entry.id || checksum != entry.object_checksum {
        return Err(CoreError::IdMismatch);
    }
    let pad = record_padding(object_len)?;
    if pad != 0 {
        let mut bytes = [0_u8; 7];
        let padding_offset = object_offset
            .checked_add(object_len)
            .ok_or(CoreError::IntegerOverflow)?;
        pack.read_exact_at(padding_offset, &mut bytes[..usize::from(pad)])
            .map_err(map_read_port)?;
        counters.add(CounterFieldV1::BytesRead, u64::from(pad))?;
        if bytes[..usize::from(pad)].iter().any(|byte| *byte != 0) {
            return Err(CoreError::PackInvalid);
        }
    }
    Ok(())
}

struct PackObjectReadV1<'a, P: PackReadPortV1 + ?Sized> {
    pack: &'a mut P,
    start: u64,
    len: u64,
    counters: &'a mut OperationCountersV1,
    checksum: &'a mut FramedHasherV1,
    next_offset: u64,
}

impl<P: PackReadPortV1 + ?Sized> PhysicalObjectReadPortV1 for PackObjectReadV1<'_, P> {
    fn len(&mut self) -> CoreResult<u64> {
        Ok(self.len)
    }

    fn read_exact_at(&mut self, offset: u64, destination: &mut [u8]) -> CoreResult<()> {
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
        self.pack
            .read_exact_at(absolute, destination)
            .map_err(map_read_port)?;
        self.checksum.write(destination)?;
        self.next_offset = end;
        self.counters.add(CounterFieldV1::BytesRead, requested)?;
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
    let mut hasher = FramedHasherV1::new(tag, len);
    let mut consumed = 0_u64;
    while consumed < len {
        let take = usize::try_from((len - consumed).min(COMPARISON_WINDOW_BYTES as u64))
            .map_err(|_| CoreError::IntegerOverflow)?;
        let offset = start
            .checked_add(consumed)
            .ok_or(CoreError::IntegerOverflow)?;
        pack.read_exact_at(offset, &mut scratch[..take])
            .map_err(map_read_port)?;
        counters.add(CounterFieldV1::BytesRead, take as u64)?;
        hasher.write(&scratch[..take])?;
        consumed = consumed
            .checked_add(take as u64)
            .ok_or(CoreError::IntegerOverflow)?;
    }
    hasher.finish()
}

fn read_array<const N: usize, P: PackReadPortV1 + ?Sized>(
    pack: &mut P,
    offset: u64,
    counters: &mut OperationCountersV1,
) -> CoreResult<[u8; N]> {
    let mut bytes = [0_u8; N];
    pack.read_exact_at(offset, &mut bytes)
        .map_err(map_read_port)?;
    counters.add(CounterFieldV1::BytesRead, N as u64)?;
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
}
