//! Transaction-private immutable object ports.
//!
//! Complete-closure admission is implemented over a bounded random-readable
//! spool. `ClosureObjectV1` remains only as the direct dense-pack builder's
//! borrowed input record; it is deliberately not accepted by admission.

use crate::identity::{COMPARISON_WINDOW_BYTES, IDENTITY_HASHER_BYTES_V1};
use crate::limits::{
    CounterFieldV1, MemoryComponentV1, OperationCountersV1, OperationMemoryPlanV1, ResourceLedgerV1,
};
use crate::object::{
    decode_physical_object_from_port_v1, DiscardStrongEdgesV1, PhysicalObjectReadPortV1,
    TypedPhysicalObjectIdV1,
};
use crate::{CoreError, CoreResult};

pub use crate::cas_stream::{
    admit_complete_immutable_v1, compare_closure_object_ids_v1, AdmissionBuffersV1,
    AdmittedClosureV1, CompleteImmutableClosureReadPortV1,
};

/// Borrowed input record for direct dense-pack construction. This type is not
/// a closure-admission source and therefore cannot bypass the bounded spool
/// port used by `admit_complete_immutable_v1`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClosureObjectV1<'a> {
    expected_id: TypedPhysicalObjectIdV1,
    canonical_bytes: &'a [u8],
}

impl<'a> ClosureObjectV1<'a> {
    pub const fn new(expected_id: TypedPhysicalObjectIdV1, canonical_bytes: &'a [u8]) -> Self {
        Self {
            expected_id,
            canonical_bytes,
        }
    }

    pub const fn expected_id(self) -> TypedPhysicalObjectIdV1 {
        self.expected_id
    }

    pub const fn canonical_bytes(self) -> &'a [u8] {
        self.canonical_bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImmutablePortErrorV1 {
    Failure,
}

/// Exact occupied-key lookup through bounded random reads. Bytes must remain
/// immutable between `occupied_len` and the final read for that key. LayerFS
/// never repairs, replaces, quarantines, or requests the whole occupant.
pub trait OccupiedImmutableReadPortV1 {
    /// Maximum transient userspace memory retained by this adapter. This
    /// declaration is queried before any occupied-key lookup and must not
    /// allocate, cache, or perform I/O.
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64>;
    fn occupied_len(
        &mut self,
        id: TypedPhysicalObjectIdV1,
    ) -> Result<Option<u64>, ImmutablePortErrorV1>;
    fn read_occupied_exact_at(
        &mut self,
        id: TypedPhysicalObjectIdV1,
        offset: u64,
        destination: &mut [u8],
    ) -> Result<(), ImmutablePortErrorV1>;
}

/// Private transaction sink. `make_closure_visible` is the sole visibility
/// boundary and is invoked only after every closure proof succeeds.
pub trait PreparedImmutableClosurePortV1 {
    /// Maximum transient userspace memory retained by this adapter. The query
    /// must be side-effect-free and must not begin private preparation.
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64>;
    fn begin_private_closure(&mut self, object_count: u64) -> Result<(), ImmutablePortErrorV1>;
    fn begin_private_object(
        &mut self,
        id: TypedPhysicalObjectIdV1,
        exact_len: u64,
    ) -> Result<(), ImmutablePortErrorV1>;
    fn write_private_object(
        &mut self,
        canonical_fragment: &[u8],
    ) -> Result<(), ImmutablePortErrorV1>;
    fn finish_private_object(
        &mut self,
        id: TypedPhysicalObjectIdV1,
    ) -> Result<(), ImmutablePortErrorV1>;
    fn note_reused_object(
        &mut self,
        id: TypedPhysicalObjectIdV1,
    ) -> Result<(), ImmutablePortErrorV1>;
    fn make_closure_visible(
        &mut self,
        version_record: TypedPhysicalObjectIdV1,
    ) -> Result<(), ImmutablePortErrorV1>;
    fn abort_private_closure(&mut self);
}

/// Bounded destination for a validated immutable-object read. No unvalidated
/// fragment reaches this sink.
pub trait BoundedImmutableReadSinkV1 {
    /// Maximum transient userspace memory retained by this adapter. The query
    /// must be side-effect-free and must not begin a read transaction.
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64>;
    fn begin_complete_immutable(
        &mut self,
        id: TypedPhysicalObjectIdV1,
        exact_len: u64,
    ) -> Result<(), ImmutablePortErrorV1>;
    fn write_complete_immutable(
        &mut self,
        canonical_fragment: &[u8],
    ) -> Result<(), ImmutablePortErrorV1>;
    fn finish_complete_immutable(
        &mut self,
        id: TypedPhysicalObjectIdV1,
    ) -> Result<(), ImmutablePortErrorV1>;
    fn abort_complete_immutable(&mut self);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompleteImmutableReadV1 {
    id: TypedPhysicalObjectIdV1,
    canonical_len: u64,
}

impl CompleteImmutableReadV1 {
    pub const fn id(self) -> TypedPhysicalObjectIdV1 {
        self.id
    }

    pub const fn canonical_len(self) -> u64 {
        self.canonical_len
    }
}

/// Validate an occupied object completely, then copy it to a bounded sink.
/// The immutable source is read twice: authentication first, delivery second.
pub fn read_complete_immutable_v1<O, S>(
    expected_id: TypedPhysicalObjectIdV1,
    occupied: &mut O,
    sink: &mut S,
    ledger: &ResourceLedgerV1,
    counters: &mut OperationCountersV1,
    comparison_scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
) -> CoreResult<CompleteImmutableReadV1>
where
    O: OccupiedImmutableReadPortV1 + ?Sized,
    S: BoundedImmutableReadSinkV1 + ?Sized,
{
    let port_resident = occupied
        .resident_memory_bound_bytes()?
        .checked_add(sink.resident_memory_bound_bytes()?)
        .ok_or(CoreError::IntegerOverflow)?;
    let memory = OperationMemoryPlanV1::empty()
        .charge(
            MemoryComponentV1::ComparisonWindow,
            comparison_scratch.len() as u64,
        )?
        .charge(MemoryComponentV1::HashState, IDENTITY_HASHER_BYTES_V1)?
        .charge(MemoryComponentV1::MetadataWindow, port_resident)?;
    let _reservation = ledger.reserve_operation_with_plan(memory)?;
    counters.memory_high_water = counters.memory_high_water.max(ledger.high_water_bytes());
    let len = occupied
        .occupied_len(expected_id)
        .map_err(map_source_port)?
        .ok_or(CoreError::MissingClosureEdge)?;
    let mut source = OccupiedObjectReadV1 {
        port: occupied,
        id: expected_id,
        len,
        counters,
    };
    let decoded = decode_physical_object_from_port_v1(
        &mut source,
        &mut DiscardStrongEdgesV1,
        comparison_scratch,
    )?;
    if decoded.header().kind() != expected_id.kind() {
        return Err(CoreError::TypeDomain);
    }
    if decoded.physical_id() != expected_id {
        return Err(CoreError::IdMismatch);
    }

    if let Err(error) = sink.begin_complete_immutable(expected_id, len) {
        sink.abort_complete_immutable();
        return Err(map_sink_port(error));
    }
    let result = copy_validated_object_to_sink(&mut source, sink, expected_id, comparison_scratch);
    if result.is_err() {
        sink.abort_complete_immutable();
    }
    result.map(|()| CompleteImmutableReadV1 {
        id: expected_id,
        canonical_len: len,
    })
}

fn copy_validated_object_to_sink<O, S>(
    source: &mut OccupiedObjectReadV1<'_, O>,
    sink: &mut S,
    expected_id: TypedPhysicalObjectIdV1,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
) -> CoreResult<()>
where
    O: OccupiedImmutableReadPortV1 + ?Sized,
    S: BoundedImmutableReadSinkV1 + ?Sized,
{
    let mut offset = 0_u64;
    while offset < source.len {
        let take = usize::try_from((source.len - offset).min(COMPARISON_WINDOW_BYTES as u64))
            .map_err(|_| CoreError::IntegerOverflow)?;
        source.read_exact_at(offset, &mut scratch[..take])?;
        sink.write_complete_immutable(&scratch[..take])
            .map_err(map_sink_port)?;
        let amount = u64::try_from(take).map_err(|_| CoreError::IntegerOverflow)?;
        source.counters.add(CounterFieldV1::BytesWritten, amount)?;
        offset = offset
            .checked_add(amount)
            .ok_or(CoreError::IntegerOverflow)?;
    }
    sink.finish_complete_immutable(expected_id)
        .map_err(map_sink_port)
}

struct OccupiedObjectReadV1<'a, O: OccupiedImmutableReadPortV1 + ?Sized> {
    port: &'a mut O,
    id: TypedPhysicalObjectIdV1,
    len: u64,
    counters: &'a mut OperationCountersV1,
}

impl<O: OccupiedImmutableReadPortV1 + ?Sized> PhysicalObjectReadPortV1
    for OccupiedObjectReadV1<'_, O>
{
    fn len(&mut self) -> CoreResult<u64> {
        Ok(self.len)
    }

    fn read_exact_at(&mut self, offset: u64, destination: &mut [u8]) -> CoreResult<()> {
        let requested = u64::try_from(destination.len()).map_err(|_| CoreError::IntegerOverflow)?;
        let end = offset
            .checked_add(requested)
            .ok_or(CoreError::IntegerOverflow)?;
        if end > self.len {
            return Err(CoreError::Truncated);
        }
        self.port
            .read_occupied_exact_at(self.id, offset, destination)
            .map_err(map_source_port)?;
        self.counters.add(CounterFieldV1::BytesRead, requested)
    }
}

fn map_source_port(ImmutablePortErrorV1::Failure: ImmutablePortErrorV1) -> CoreError {
    CoreError::SourceFailure
}

fn map_sink_port(ImmutablePortErrorV1::Failure: ImmutablePortErrorV1) -> CoreError {
    CoreError::SinkRefused
}
