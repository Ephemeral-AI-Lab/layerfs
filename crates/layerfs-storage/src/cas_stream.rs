//! Bounded, transaction-private immutable admission over a random-readable
//! closure spool. The spool may be backed by a pack or temporary file; this
//! module never requires all object bytes or the complete object index to be
//! resident at once.

use crate::cas::{
    ImmutablePortErrorV1, OccupiedImmutableReadPortV1, PreparedImmutableClosurePortV1,
    ValidatedOccupiedObjectV1,
};
#[cfg(feature = "c3-polymorphism")]
use crate::cdc::algorithms::{C3CdcAlgorithmV1, C3CdcStreamV1};
use crate::cdc::{
    BorrowedChunkV1, BoundaryConsumerV1, CdcBoundaryConsumerErrorV1, CdcControlV1,
    CdcSourceErrorV1, ChunkBoundaryV1, ContinueCdcControlV1, FastCdcV1, FastCdcV1Stream,
    MAXIMUM_CHUNK_BYTES,
};
use crate::format::{
    DirectoryModeContext, ExtentTagV1, PhysicalTreeChildKindV1, TreeSubtypeV1, ValidatedComponent,
    ValidatedSymlinkTarget,
};
use crate::identity::{
    derive_file_node_v1, derive_logical_chunk_spans_v1, derive_symlink_node_v1, derive_version_v1,
    ExplicitDirectoryNodeV1, FileNodeIdV1, ImplicitRootDirectoryV1, LogicalChildIdV1,
    LogicalChunkRefV1, LogicalDirectoryEntryV1, LogicalDirectoryHasherV1, LogicalFileHasherV1,
    PhysicalChunkIdV1, PhysicalFileIdV1, PhysicalSymlinkIdV1, PhysicalTreeIdV1,
    PhysicalVersionRecordIdV1, SymlinkNodeIdV1, COMPARISON_WINDOW_BYTES, IDENTITY_HASHER_BYTES_V1,
};
use crate::limits::{
    CounterFieldV1, MemoryComponentV1, OperationCountersV1, OperationMemoryPlanV1,
    OperationReservationV1, ResourceLedgerV1,
};
use crate::object::{
    decode_physical_object_from_port_v1, DiscardStrongEdgesV1, PhysicalObjectPayloadV1,
    PhysicalObjectReadPortV1, TreeRecordV1, TypedPhysicalObjectIdV1, VersionRecordV1,
    OBJECT_HEADER_BYTES,
};
use crate::profile::{ChunkerSpecV1, DigestSpecV1};
use crate::{CoreError, CoreResult};

const MAX_CLOSURE_TRAVERSAL_DEPTH: usize = 1_028;
const MAX_COMPONENT_BYTES: usize = 255;
const CHARGED_TRAVERSAL_FRAME_BYTES: u64 = 512;

#[derive(Clone, Copy)]
enum ClosureCdcV1 {
    FastCdc,
    #[cfg(feature = "c3-polymorphism")]
    Selected(C3CdcAlgorithmV1),
}

impl ClosureCdcV1 {
    fn stream<'ring, C: CdcControlV1 + ?Sized>(
        self,
        ring: &'ring mut [u8],
        control: &mut C,
    ) -> CoreResult<ClosureCdcStreamV1<'ring>> {
        match self {
            Self::FastCdc => FastCdcV1::new()
                .stream(ring, control)
                .map(ClosureCdcStreamV1::FastCdc),
            #[cfg(feature = "c3-polymorphism")]
            Self::Selected(algorithm) => algorithm
                .stream(ring, control)
                .map(ClosureCdcStreamV1::Selected),
        }
    }
}

enum ClosureCdcStreamV1<'ring> {
    FastCdc(FastCdcV1Stream<'ring>),
    #[cfg(feature = "c3-polymorphism")]
    Selected(C3CdcStreamV1<'ring>),
}

impl ClosureCdcStreamV1<'_> {
    fn push<C: CdcControlV1 + ?Sized, B: BoundaryConsumerV1 + ?Sized>(
        &mut self,
        fragment: Result<&[u8], CdcSourceErrorV1>,
        control: &mut C,
        consumer: &mut B,
    ) -> CoreResult<()> {
        match self {
            Self::FastCdc(stream) => stream.push(fragment, control, consumer),
            #[cfg(feature = "c3-polymorphism")]
            Self::Selected(stream) => stream.push(fragment, control, consumer),
        }
    }

    fn finish<C: CdcControlV1 + ?Sized, B: BoundaryConsumerV1 + ?Sized>(
        &mut self,
        control: &mut C,
        consumer: &mut B,
    ) -> CoreResult<()> {
        match self {
            Self::FastCdc(stream) => stream.finish(control, consumer),
            #[cfg(feature = "c3-polymorphism")]
            Self::Selected(stream) => stream.finish(control, consumer),
        }
    }
}

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
    version_record: TypedPhysicalObjectIdV1,
    object_count: u64,
    created_count: u64,
    reused_count: u64,
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

/// All resident admission workspaces are caller-owned and charged before any
/// canonical object byte is consumed.
pub struct AdmissionBuffersV1<'a> {
    incoming_comparison: &'a mut [u8; COMPARISON_WINDOW_BYTES],
    occupied_comparison: &'a mut [u8; COMPARISON_WINDOW_BYTES],
    source_window: &'a mut [u8; MAXIMUM_CHUNK_BYTES],
    cdc_ring: &'a mut [u8; MAXIMUM_CHUNK_BYTES],
    traversal_state: &'a mut [u8],
}

impl<'a> AdmissionBuffersV1<'a> {
    pub fn new(
        incoming_comparison: &'a mut [u8; COMPARISON_WINDOW_BYTES],
        occupied_comparison: &'a mut [u8; COMPARISON_WINDOW_BYTES],
        source_window: &'a mut [u8; MAXIMUM_CHUNK_BYTES],
        cdc_ring: &'a mut [u8; MAXIMUM_CHUNK_BYTES],
        traversal_state: &'a mut [u8],
    ) -> Self {
        Self {
            incoming_comparison,
            occupied_comparison,
            source_window,
            cdc_ring,
            traversal_state,
        }
    }
}

pub fn admit_complete_immutable_v1<C, O, S>(
    closure: &mut C,
    expected_version_record: TypedPhysicalObjectIdV1,
    occupied: &mut O,
    sink: &mut S,
    ledger: &ResourceLedgerV1,
    counters: &mut OperationCountersV1,
    buffers: AdmissionBuffersV1<'_>,
) -> CoreResult<AdmittedClosureV1>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
    O: OccupiedImmutableReadPortV1 + ?Sized,
    S: PreparedImmutableClosurePortV1 + ?Sized,
{
    validate_expected_version(expected_version_record)?;
    let memory = admission_memory_plan(closure, occupied, sink, &buffers)?;
    let _reservation = ledger.reserve_operation_with_plan(memory)?;
    counters.memory_high_water = counters.memory_high_water.max(ledger.high_water_bytes());
    admit_complete_after_admission(
        closure,
        expected_version_record,
        occupied,
        sink,
        counters,
        buffers,
        ClosureCdcV1::FastCdc,
    )
}

#[cfg(feature = "c3-polymorphism")]
#[allow(clippy::too_many_arguments)]
pub(crate) fn admit_complete_immutable_borrowed_v1<C, O, S>(
    closure: &mut C,
    expected_version_record: TypedPhysicalObjectIdV1,
    occupied: &mut O,
    sink: &mut S,
    reservation: &OperationReservationV1<'_>,
    counters: &mut OperationCountersV1,
    buffers: AdmissionBuffersV1<'_>,
    algorithm: C3CdcAlgorithmV1,
) -> CoreResult<AdmittedClosureV1>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
    O: OccupiedImmutableReadPortV1 + ?Sized,
    S: PreparedImmutableClosurePortV1 + ?Sized,
{
    validate_expected_version(expected_version_record)?;
    let memory = admission_memory_plan(closure, occupied, sink, &buffers)?;
    reservation.require(memory)?;
    admit_complete_after_admission(
        closure,
        expected_version_record,
        occupied,
        sink,
        counters,
        buffers,
        ClosureCdcV1::Selected(algorithm),
    )
}

fn validate_expected_version(expected: TypedPhysicalObjectIdV1) -> CoreResult<()> {
    if matches!(expected, TypedPhysicalObjectIdV1::VersionRecord(_)) {
        Ok(())
    } else {
        Err(CoreError::TypeDomain)
    }
}

fn admission_memory_plan<C, O, S>(
    closure: &C,
    occupied: &O,
    sink: &S,
    buffers: &AdmissionBuffersV1<'_>,
) -> CoreResult<OperationMemoryPlanV1>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
    O: OccupiedImmutableReadPortV1 + ?Sized,
    S: PreparedImmutableClosurePortV1 + ?Sized,
{
    let closure_resident = closure.resident_memory_bound_bytes()?;
    let occupied_resident = occupied.resident_memory_bound_bytes()?;
    let sink_resident = sink.resident_memory_bound_bytes()?;
    let source_resident = closure_resident
        .checked_add(occupied_resident)
        .and_then(|bytes| bytes.checked_add(sink_resident))
        .ok_or(CoreError::IntegerOverflow)?;
    OperationMemoryPlanV1::empty()
        .charge(
            MemoryComponentV1::ComparisonWindow,
            (2 * COMPARISON_WINDOW_BYTES) as u64,
        )?
        .charge(
            MemoryComponentV1::SourceWindow,
            buffers.source_window.len() as u64,
        )?
        .charge(MemoryComponentV1::CdcRing, buffers.cdc_ring.len() as u64)?
        .charge(
            MemoryComponentV1::TraversalState,
            buffers.traversal_state.len() as u64,
        )?
        .charge(
            MemoryComponentV1::PageSummaries,
            u64::try_from(MAX_CLOSURE_TRAVERSAL_DEPTH)
                .map_err(|_| CoreError::IntegerOverflow)?
                .checked_mul(CHARGED_TRAVERSAL_FRAME_BYTES)
                .ok_or(CoreError::IntegerOverflow)?,
        )?
        .charge(MemoryComponentV1::MetadataWindow, source_resident)?
        .charge(MemoryComponentV1::HashState, IDENTITY_HASHER_BYTES_V1)
}

#[allow(clippy::too_many_arguments)]
fn admit_complete_after_admission<C, O, S>(
    closure: &mut C,
    expected_version_record: TypedPhysicalObjectIdV1,
    occupied: &mut O,
    sink: &mut S,
    counters: &mut OperationCountersV1,
    buffers: AdmissionBuffersV1<'_>,
    validation_cdc: ClosureCdcV1,
) -> CoreResult<AdmittedClosureV1>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
    O: OccupiedImmutableReadPortV1 + ?Sized,
    S: PreparedImmutableClosurePortV1 + ?Sized,
{
    let object_count = closure.object_count().map_err(map_source_port)?;
    crate::format::validate_total_object_count(object_count)?;
    let object_count_usize =
        usize::try_from(object_count).map_err(|_| CoreError::IntegerOverflow)?;
    let required_traversal_bytes = object_count_usize
        .checked_mul(2)
        .and_then(|bits| bits.checked_add(7))
        .map(|bits| bits / 8)
        .ok_or(CoreError::IntegerOverflow)?;
    if buffers.traversal_state.len() < required_traversal_bytes {
        return Err(CoreError::ResourceRefused);
    }

    validate_canonical_typed_ids(closure, object_count)?;
    buffers.traversal_state[..required_traversal_bytes].fill(0);
    sink.begin_private_closure(object_count)
        .map_err(map_sink_port)?;
    let validation = admit_inner(
        closure,
        object_count,
        expected_version_record,
        occupied,
        sink,
        counters,
        buffers,
        required_traversal_bytes,
        validation_cdc,
    );
    let source_read_accounting = closure
        .direct_storage_read_observation()
        .map_err(map_source_port)
        .and_then(|(bytes, calls)| counters.record_fscas_read(bytes, calls));
    let occupied_read_accounting = occupied
        .direct_storage_read_observation()
        .map_err(map_source_port)
        .and_then(|(bytes, calls)| counters.record_fscas_read(bytes, calls));
    let result = match (validation, source_read_accounting, occupied_read_accounting) {
        (Err(error), _, _) => Err(error),
        (Ok(_), Err(error), _) | (Ok(_), Ok(()), Err(error)) => Err(error),
        (Ok(admitted), Ok(()), Ok(())) => {
            // All fallible accounting precedes the only visibility
            // transition. Assigning the checked snapshot afterward cannot
            // turn a visible complete closure into an accounting failure.
            let visibility = (|| {
                let mut visible_counters = *counters;
                visible_counters.record_closure_fence()?;
                sink.make_closure_visible(expected_version_record)
                    .map_err(map_sink_port)?;
                *counters = visible_counters;
                Ok(())
            })();
            visibility.map(|()| admitted)
        }
    };
    if result.is_err() {
        sink.abort_private_closure();
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn admit_inner<C, O, S>(
    closure: &mut C,
    object_count: u64,
    expected_version_record: TypedPhysicalObjectIdV1,
    occupied: &mut O,
    sink: &mut S,
    counters: &mut OperationCountersV1,
    buffers: AdmissionBuffersV1<'_>,
    required_traversal_bytes: usize,
    validation_cdc: ClosureCdcV1,
) -> CoreResult<AdmittedClosureV1>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
    O: OccupiedImmutableReadPortV1 + ?Sized,
    S: PreparedImmutableClosurePortV1 + ?Sized,
{
    let mut summaries = ClosureSummariesV1::default();
    let mut version = None;
    let mut created_count = 0_u64;
    let mut reused_count = 0_u64;

    for ordinal in 0..object_count {
        let expected_id = closure.object_id_at(ordinal).map_err(map_source_port)?;
        let len = closure.object_len_at(ordinal).map_err(map_source_port)?;
        let decoded = {
            let mut source = ClosureObjectReadV1::new(closure, ordinal, len, counters);
            decode_physical_object_from_port_v1(
                &mut source,
                &mut DiscardStrongEdgesV1,
                buffers.incoming_comparison,
            )?
        };
        if decoded.header().kind() != expected_id.kind() {
            return Err(CoreError::TypeDomain);
        }
        if decoded.physical_id() != expected_id {
            return Err(CoreError::IdMismatch);
        }
        validate_object_edges(closure, object_count, ordinal, decoded.payload(), counters)?;
        summaries.add(decoded.payload())?;
        if expected_id == expected_version_record {
            let PhysicalObjectPayloadV1::VersionRecord(record) = decoded.payload() else {
                return Err(CoreError::TypeDomain);
            };
            version = Some(record);
        }

        match occupied
            .occupied_len(expected_id)
            .map_err(map_source_port)?
        {
            Some(occupied_len) => {
                let validated = validate_and_compare_occupied(
                    closure,
                    ordinal,
                    len,
                    expected_id,
                    occupied,
                    occupied_len,
                    counters,
                    buffers.incoming_comparison,
                    buffers.occupied_comparison,
                )?;
                sink.note_reused_object(validated).map_err(map_sink_port)?;
                reused_count = reused_count
                    .checked_add(1)
                    .ok_or(CoreError::IntegerOverflow)?;
                counters.add(CounterFieldV1::PhysicalObjectsReused, 1)?;
            }
            None => {
                stage_private_object_bounded(
                    closure,
                    ordinal,
                    len,
                    expected_id,
                    sink,
                    counters,
                    buffers.incoming_comparison,
                )?;
                created_count = created_count
                    .checked_add(1)
                    .ok_or(CoreError::IntegerOverflow)?;
                counters.add(CounterFieldV1::PhysicalObjectsCreated, 1)?;
            }
        }
    }

    let version = version.ok_or(CoreError::MissingClosureEdge)?;
    validate_version_summary(version, summaries, object_count)?;
    let mut visited = 0_u64;
    let version_ordinal = find_ordinal(closure, object_count, expected_version_record)?
        .ok_or(CoreError::MissingClosureEdge)?;
    traversal_enter(
        &mut buffers.traversal_state[..required_traversal_bytes],
        version_ordinal,
    )?;
    let logical_root = reconstruct_root_directory(
        closure,
        object_count,
        version.root_tree_id,
        counters,
        buffers.source_window,
        buffers.cdc_ring,
        &mut buffers.traversal_state[..required_traversal_bytes],
        &mut visited,
        1,
        validation_cdc,
    )?;
    traversal_finish(
        &mut buffers.traversal_state[..required_traversal_bytes],
        version_ordinal,
        &mut visited,
    )?;
    if visited != object_count {
        return Err(CoreError::MissingClosureEdge);
    }
    if derive_version_v1(logical_root) != version.version_id {
        return Err(CoreError::IdMismatch);
    }

    Ok(AdmittedClosureV1 {
        version_record: expected_version_record,
        object_count,
        created_count,
        reused_count,
    })
}

fn validate_canonical_typed_ids<C>(closure: &mut C, count: u64) -> CoreResult<()>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
{
    let mut previous = None;
    for ordinal in 0..count {
        let current = closure.object_id_at(ordinal).map_err(map_source_port)?;
        if previous.is_some_and(|left| {
            compare_closure_object_ids_v1(left, current) != core::cmp::Ordering::Less
        }) {
            return Err(CoreError::NonCanonicalOrder);
        }
        previous = Some(current);
    }
    Ok(())
}

struct ClosureObjectReadV1<'a, C: CompleteImmutableClosureReadPortV1 + ?Sized> {
    closure: &'a mut C,
    ordinal: u64,
    len: u64,
    counters: &'a mut OperationCountersV1,
}

impl<'a, C: CompleteImmutableClosureReadPortV1 + ?Sized> ClosureObjectReadV1<'a, C> {
    const fn new(
        closure: &'a mut C,
        ordinal: u64,
        len: u64,
        counters: &'a mut OperationCountersV1,
    ) -> Self {
        Self {
            closure,
            ordinal,
            len,
            counters,
        }
    }
}

impl<C: CompleteImmutableClosureReadPortV1 + ?Sized> PhysicalObjectReadPortV1
    for ClosureObjectReadV1<'_, C>
{
    fn len(&mut self) -> CoreResult<u64> {
        Ok(self.len)
    }

    fn read_exact_at(&mut self, offset: u64, destination: &mut [u8]) -> CoreResult<()> {
        let amount = u64::try_from(destination.len()).map_err(|_| CoreError::IntegerOverflow)?;
        let end = offset
            .checked_add(amount)
            .ok_or(CoreError::IntegerOverflow)?;
        if end > self.len {
            return Err(CoreError::Truncated);
        }
        self.closure
            .read_object_exact_at(self.ordinal, offset, destination)
            .map_err(map_source_port)?;
        self.counters.add(CounterFieldV1::BytesRead, amount)
    }
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
        let amount = u64::try_from(destination.len()).map_err(|_| CoreError::IntegerOverflow)?;
        let end = offset
            .checked_add(amount)
            .ok_or(CoreError::IntegerOverflow)?;
        if end > self.len {
            return Err(CoreError::Truncated);
        }
        self.port
            .read_occupied_exact_at(self.id, offset, destination)
            .map_err(map_source_port)?;
        self.counters.add(CounterFieldV1::BytesRead, amount)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct ClosureSummariesV1 {
    entry_count: u64,
    tree_count: u64,
    file_count: u64,
    symlink_count: u64,
    chunk_count: u64,
    logical_file_bytes: u64,
    extent_count: u64,
    chunk_ref_count: u64,
    physical_chunk_bytes: u64,
}

impl ClosureSummariesV1 {
    fn add(&mut self, payload: PhysicalObjectPayloadV1) -> CoreResult<()> {
        match payload {
            PhysicalObjectPayloadV1::VersionRecord(_) => {}
            PhysicalObjectPayloadV1::Tree(tree) => {
                checked_increment(&mut self.tree_count, 1)?;
                if let TreeRecordV1::Directory(directory) = tree {
                    checked_increment(&mut self.entry_count, u64::from(directory.entry_count))?;
                }
            }
            PhysicalObjectPayloadV1::File(file) => {
                checked_increment(&mut self.file_count, 1)?;
                checked_increment(&mut self.logical_file_bytes, file.logical_len)?;
                checked_increment(&mut self.extent_count, u64::from(file.extent_count))?;
                checked_increment(&mut self.chunk_ref_count, file.chunk_ref_count)?;
            }
            PhysicalObjectPayloadV1::Symlink(_) => {
                checked_increment(&mut self.symlink_count, 1)?;
            }
            PhysicalObjectPayloadV1::Chunk(chunk) => {
                checked_increment(&mut self.chunk_count, 1)?;
                checked_increment(&mut self.physical_chunk_bytes, u64::from(chunk.payload_len))?;
            }
        }
        Ok(())
    }
}

fn checked_increment(target: &mut u64, amount: u64) -> CoreResult<()> {
    *target = target
        .checked_add(amount)
        .ok_or(CoreError::IntegerOverflow)?;
    Ok(())
}

fn validate_version_summary(
    version: VersionRecordV1,
    actual: ClosureSummariesV1,
    object_count: u64,
) -> CoreResult<()> {
    if version.chunker_spec_id != ChunkerSpecV1::frozen().id()
        || version.digest_spec_id != DigestSpecV1::frozen().id()
        || u64::from(version.tree_count) != actual.tree_count
        || u64::from(version.entry_count) != actual.entry_count
        || u64::from(version.file_count) != actual.file_count
        || u64::from(version.symlink_count) != actual.symlink_count
        || u64::from(version.chunk_count) != actual.chunk_count
        || u64::from(version.extent_count) != actual.extent_count
        || u64::from(version.chunk_ref_count) != actual.chunk_ref_count
        || u64::from(version.total_object_count) != object_count
        || version.logical_file_bytes != actual.logical_file_bytes
        || version.physical_chunk_bytes != actual.physical_chunk_bytes
    {
        return Err(CoreError::IdMismatch);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_and_compare_occupied<C, O>(
    closure: &mut C,
    ordinal: u64,
    incoming_len: u64,
    expected_id: TypedPhysicalObjectIdV1,
    occupied: &mut O,
    occupied_len: u64,
    counters: &mut OperationCountersV1,
    incoming_scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    occupied_scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
) -> CoreResult<ValidatedOccupiedObjectV1>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
    O: OccupiedImmutableReadPortV1 + ?Sized,
{
    {
        let mut reader = OccupiedObjectReadV1 {
            port: occupied,
            id: expected_id,
            len: occupied_len,
            counters,
        };
        let decoded = decode_physical_object_from_port_v1(
            &mut reader,
            &mut DiscardStrongEdgesV1,
            occupied_scratch,
        )
        .map_err(map_occupant_validation)?;
        if decoded.header().kind() != expected_id.kind() {
            return Err(CoreError::MalformedOccupant);
        }
    }

    let mut equal = incoming_len == occupied_len;
    let mut offset = 0_u64;
    let comparison_len = incoming_len.max(occupied_len);
    while offset < comparison_len {
        let take = usize::try_from((comparison_len - offset).min(COMPARISON_WINDOW_BYTES as u64))
            .map_err(|_| CoreError::IntegerOverflow)?;
        let amount = u64::try_from(take).map_err(|_| CoreError::IntegerOverflow)?;
        let incoming_take = usize::try_from(incoming_len.saturating_sub(offset).min(amount))
            .map_err(|_| CoreError::IntegerOverflow)?;
        let occupied_take = usize::try_from(occupied_len.saturating_sub(offset).min(amount))
            .map_err(|_| CoreError::IntegerOverflow)?;
        if incoming_take != 0 {
            closure
                .read_object_exact_at(ordinal, offset, &mut incoming_scratch[..incoming_take])
                .map_err(map_source_port)?;
            counters.add(
                CounterFieldV1::BytesRead,
                u64::try_from(incoming_take).map_err(|_| CoreError::IntegerOverflow)?,
            )?;
        }
        if occupied_take != 0 {
            occupied
                .read_occupied_exact_at(expected_id, offset, &mut occupied_scratch[..occupied_take])
                .map_err(map_source_port)?;
            counters.add(
                CounterFieldV1::BytesRead,
                u64::try_from(occupied_take).map_err(|_| CoreError::IntegerOverflow)?,
            )?;
        }
        if incoming_take != occupied_take
            || incoming_scratch[..incoming_take] != occupied_scratch[..occupied_take]
        {
            equal = false;
        }
        offset = offset
            .checked_add(amount)
            .ok_or(CoreError::IntegerOverflow)?;
    }
    if equal {
        Ok(ValidatedOccupiedObjectV1::new(
            ordinal,
            expected_id,
            occupied_len,
        ))
    } else {
        Err(CoreError::OccupiedSameIdDifferentBytes)
    }
}

fn stage_private_object_bounded<C, S>(
    closure: &mut C,
    ordinal: u64,
    len: u64,
    id: TypedPhysicalObjectIdV1,
    sink: &mut S,
    counters: &mut OperationCountersV1,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
) -> CoreResult<()>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
    S: PreparedImmutableClosurePortV1 + ?Sized,
{
    sink.begin_private_object(id, len).map_err(map_sink_port)?;
    let mut offset = 0_u64;
    while offset < len {
        let take = usize::try_from((len - offset).min(COMPARISON_WINDOW_BYTES as u64))
            .map_err(|_| CoreError::IntegerOverflow)?;
        closure
            .read_object_exact_at(ordinal, offset, &mut scratch[..take])
            .map_err(map_source_port)?;
        sink.write_private_object(&scratch[..take])
            .map_err(map_sink_port)?;
        let amount = u64::try_from(take).map_err(|_| CoreError::IntegerOverflow)?;
        counters.add(CounterFieldV1::BytesRead, amount)?;
        counters.add(CounterFieldV1::BytesCopied, amount)?;
        counters.add(CounterFieldV1::BytesWritten, amount)?;
        offset = offset
            .checked_add(amount)
            .ok_or(CoreError::IntegerOverflow)?;
    }
    sink.finish_private_object(id).map_err(map_sink_port)
}

fn map_source_port(ImmutablePortErrorV1::Failure: ImmutablePortErrorV1) -> CoreError {
    CoreError::SourceFailure
}

fn map_sink_port(ImmutablePortErrorV1::Failure: ImmutablePortErrorV1) -> CoreError {
    CoreError::SinkRefused
}

const fn map_occupant_validation(error: CoreError) -> CoreError {
    match error {
        CoreError::SourceFailure
        | CoreError::ResourceRefused
        | CoreError::Cancelled
        | CoreError::Deadline => error,
        _ => CoreError::MalformedOccupant,
    }
}

fn validate_object_edges<C>(
    closure: &mut C,
    object_count: u64,
    ordinal: u64,
    payload: PhysicalObjectPayloadV1,
    counters: &mut OperationCountersV1,
) -> CoreResult<()>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
{
    match payload {
        PhysicalObjectPayloadV1::VersionRecord(version) => {
            require_edge(
                closure,
                object_count,
                TypedPhysicalObjectIdV1::Tree(version.root_tree_id),
            )?;
        }
        PhysicalObjectPayloadV1::Tree(TreeRecordV1::Directory(directory)) => {
            if let Some(root) = directory.root_page_id {
                require_edge(closure, object_count, TypedPhysicalObjectIdV1::Tree(root))?;
            }
        }
        PhysicalObjectPayloadV1::Tree(TreeRecordV1::Leaf(leaf)) => {
            let mut cursor = ObjectCursorV1::payload(closure, ordinal)?;
            let _subtype = cursor.read_u8(closure, counters)?;
            let _depth = cursor.read_u8(closure, counters)?;
            let count = cursor.read_u16_be(closure, counters)?;
            if count != leaf.count {
                return Err(CoreError::IdMismatch);
            }
            for _ in 0..count {
                let mut name = [0_u8; MAX_COMPONENT_BYTES];
                let _name_len = read_component(&mut cursor, closure, counters, &mut name)?;
                let kind = PhysicalTreeChildKindV1::try_from(cursor.read_u8(closure, counters)?)?;
                let raw = cursor.read_array::<32, _>(closure, counters)?;
                let id = match kind {
                    PhysicalTreeChildKindV1::Tree => {
                        TypedPhysicalObjectIdV1::Tree(PhysicalTreeIdV1::from_digest(raw))
                    }
                    PhysicalTreeChildKindV1::File => {
                        TypedPhysicalObjectIdV1::File(PhysicalFileIdV1::from_digest(raw))
                    }
                    PhysicalTreeChildKindV1::Symlink => {
                        TypedPhysicalObjectIdV1::Symlink(PhysicalSymlinkIdV1::from_digest(raw))
                    }
                };
                require_edge(closure, object_count, id)?;
            }
            cursor.finish()?;
        }
        PhysicalObjectPayloadV1::Tree(TreeRecordV1::Index(index)) => {
            let mut cursor = ObjectCursorV1::payload(closure, ordinal)?;
            let _subtype = cursor.read_u8(closure, counters)?;
            let _depth = cursor.read_u8(closure, counters)?;
            let count = cursor.read_u16_be(closure, counters)?;
            if count != index.count {
                return Err(CoreError::IdMismatch);
            }
            for _ in 0..count {
                let _subtree_count = cursor.read_u32_be(closure, counters)?;
                let mut first = [0_u8; MAX_COMPONENT_BYTES];
                let _first_len = read_component(&mut cursor, closure, counters, &mut first)?;
                let mut last = [0_u8; MAX_COMPONENT_BYTES];
                let _last_len = read_component(&mut cursor, closure, counters, &mut last)?;
                let child =
                    PhysicalTreeIdV1::from_digest(cursor.read_array::<32, _>(closure, counters)?);
                require_edge(closure, object_count, TypedPhysicalObjectIdV1::Tree(child))?;
            }
            cursor.finish()?;
        }
        PhysicalObjectPayloadV1::File(file) => {
            let mut cursor = ObjectCursorV1::payload(closure, ordinal)?;
            let _mode = cursor.read_u16_be(closure, counters)?;
            let _logical_len = cursor.read_u64_be(closure, counters)?;
            let extent_count = cursor.read_u32_be(closure, counters)?;
            if extent_count != file.extent_count {
                return Err(CoreError::IdMismatch);
            }
            for _ in 0..extent_count {
                let tag = ExtentTagV1::try_from(cursor.read_u8(closure, counters)?)?;
                let _extent_len = cursor.read_u64_be(closure, counters)?;
                if tag == ExtentTagV1::Data {
                    let count = cursor.read_u32_be(closure, counters)?;
                    for _ in 0..count {
                        let _chunk_len = cursor.read_u32_be(closure, counters)?;
                        let chunk = PhysicalChunkIdV1::from_digest(
                            cursor.read_array::<32, _>(closure, counters)?,
                        );
                        require_edge(closure, object_count, TypedPhysicalObjectIdV1::Chunk(chunk))?;
                    }
                }
            }
            cursor.finish()?;
        }
        PhysicalObjectPayloadV1::Symlink(_) | PhysicalObjectPayloadV1::Chunk(_) => {}
    }
    Ok(())
}

fn find_ordinal<C>(
    closure: &mut C,
    count: u64,
    id: TypedPhysicalObjectIdV1,
) -> CoreResult<Option<u64>>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
{
    let mut low = 0_u64;
    let mut high = count;
    while low < high {
        let middle = low + (high - low) / 2;
        let current = closure.object_id_at(middle).map_err(map_source_port)?;
        match compare_closure_object_ids_v1(current, id) {
            core::cmp::Ordering::Less => low = middle + 1,
            core::cmp::Ordering::Greater => high = middle,
            core::cmp::Ordering::Equal => return Ok(Some(middle)),
        }
    }
    Ok(None)
}

fn contains_raw_id<C>(closure: &mut C, count: u64, raw: [u8; 32]) -> CoreResult<bool>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
{
    for candidate in [
        TypedPhysicalObjectIdV1::VersionRecord(PhysicalVersionRecordIdV1::from_digest(raw)),
        TypedPhysicalObjectIdV1::Tree(PhysicalTreeIdV1::from_digest(raw)),
        TypedPhysicalObjectIdV1::File(PhysicalFileIdV1::from_digest(raw)),
        TypedPhysicalObjectIdV1::Symlink(PhysicalSymlinkIdV1::from_digest(raw)),
        TypedPhysicalObjectIdV1::Chunk(PhysicalChunkIdV1::from_digest(raw)),
    ] {
        if find_ordinal(closure, count, candidate)?.is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn require_edge<C>(closure: &mut C, count: u64, id: TypedPhysicalObjectIdV1) -> CoreResult<u64>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
{
    if let Some(ordinal) = find_ordinal(closure, count, id)? {
        Ok(ordinal)
    } else if contains_raw_id(closure, count, *id.as_bytes())? {
        Err(CoreError::TypedEdge)
    } else {
        Err(CoreError::MissingClosureEdge)
    }
}

fn traversal_get(states: &[u8], ordinal: u64) -> CoreResult<u8> {
    let ordinal = usize::try_from(ordinal).map_err(|_| CoreError::IntegerOverflow)?;
    let bit = ordinal.checked_mul(2).ok_or(CoreError::IntegerOverflow)?;
    let byte = *states.get(bit / 8).ok_or(CoreError::ResourceRefused)?;
    Ok((byte >> (bit % 8)) & 0b11)
}

fn traversal_set(states: &mut [u8], ordinal: u64, state: u8) -> CoreResult<()> {
    let ordinal = usize::try_from(ordinal).map_err(|_| CoreError::IntegerOverflow)?;
    let bit = ordinal.checked_mul(2).ok_or(CoreError::IntegerOverflow)?;
    let target = states.get_mut(bit / 8).ok_or(CoreError::ResourceRefused)?;
    let shift = bit % 8;
    *target = (*target & !(0b11 << shift)) | ((state & 0b11) << shift);
    Ok(())
}

/// Returns `true` when this is the first traversal of the object. A completed
/// shared object may be reconstructed again to recover its logical identity,
/// but is counted as reachable only once.
fn traversal_enter(states: &mut [u8], ordinal: u64) -> CoreResult<bool> {
    match traversal_get(states, ordinal)? {
        0 => {
            traversal_set(states, ordinal, 1)?;
            Ok(true)
        }
        1 => Err(CoreError::Cycle),
        2 => Ok(false),
        _ => Err(CoreError::IdMismatch),
    }
}

fn traversal_finish(states: &mut [u8], ordinal: u64, visited: &mut u64) -> CoreResult<()> {
    if traversal_get(states, ordinal)? == 1 {
        traversal_set(states, ordinal, 2)?;
        *visited = visited.checked_add(1).ok_or(CoreError::IntegerOverflow)?;
    }
    Ok(())
}

#[cfg(test)]
mod traversal_tests {
    use super::{traversal_enter, traversal_finish};
    use crate::CoreError;

    #[test]
    fn active_object_reentry_is_a_cycle_but_completed_sharing_is_valid() {
        let mut states = [0_u8; 1];
        let mut visited = 0_u64;

        assert_eq!(traversal_enter(&mut states, 0), Ok(true));
        assert_eq!(traversal_enter(&mut states, 0), Err(CoreError::Cycle));
        traversal_finish(&mut states, 0, &mut visited).unwrap();
        assert_eq!(visited, 1);
        assert_eq!(traversal_enter(&mut states, 0), Ok(false));
        traversal_finish(&mut states, 0, &mut visited).unwrap();
        assert_eq!(visited, 1);
    }
}

#[derive(Clone, Copy)]
struct ObjectCursorV1 {
    ordinal: u64,
    offset: u64,
    end: u64,
}

impl ObjectCursorV1 {
    fn payload<C>(closure: &mut C, ordinal: u64) -> CoreResult<Self>
    where
        C: CompleteImmutableClosureReadPortV1 + ?Sized,
    {
        let len = closure.object_len_at(ordinal).map_err(map_source_port)?;
        if len < OBJECT_HEADER_BYTES {
            return Err(CoreError::Truncated);
        }
        Ok(Self {
            ordinal,
            offset: OBJECT_HEADER_BYTES,
            end: len,
        })
    }

    fn remaining(self) -> u64 {
        self.end - self.offset
    }

    fn read_into<C>(
        &mut self,
        closure: &mut C,
        counters: &mut OperationCountersV1,
        destination: &mut [u8],
    ) -> CoreResult<()>
    where
        C: CompleteImmutableClosureReadPortV1 + ?Sized,
    {
        let amount = u64::try_from(destination.len()).map_err(|_| CoreError::IntegerOverflow)?;
        let end = self
            .offset
            .checked_add(amount)
            .ok_or(CoreError::IntegerOverflow)?;
        if end > self.end {
            return Err(CoreError::Truncated);
        }
        closure
            .read_object_exact_at(self.ordinal, self.offset, destination)
            .map_err(map_source_port)?;
        counters.add(CounterFieldV1::BytesRead, amount)?;
        self.offset = end;
        Ok(())
    }

    fn read_array<const N: usize, C>(
        &mut self,
        closure: &mut C,
        counters: &mut OperationCountersV1,
    ) -> CoreResult<[u8; N]>
    where
        C: CompleteImmutableClosureReadPortV1 + ?Sized,
    {
        let mut result = [0_u8; N];
        self.read_into(closure, counters, &mut result)?;
        Ok(result)
    }

    fn read_u8<C>(&mut self, closure: &mut C, counters: &mut OperationCountersV1) -> CoreResult<u8>
    where
        C: CompleteImmutableClosureReadPortV1 + ?Sized,
    {
        Ok(self.read_array::<1, _>(closure, counters)?[0])
    }

    fn read_u16_be<C>(
        &mut self,
        closure: &mut C,
        counters: &mut OperationCountersV1,
    ) -> CoreResult<u16>
    where
        C: CompleteImmutableClosureReadPortV1 + ?Sized,
    {
        Ok(u16::from_be_bytes(self.read_array(closure, counters)?))
    }

    fn read_u32_be<C>(
        &mut self,
        closure: &mut C,
        counters: &mut OperationCountersV1,
    ) -> CoreResult<u32>
    where
        C: CompleteImmutableClosureReadPortV1 + ?Sized,
    {
        Ok(u32::from_be_bytes(self.read_array(closure, counters)?))
    }

    fn read_u64_be<C>(
        &mut self,
        closure: &mut C,
        counters: &mut OperationCountersV1,
    ) -> CoreResult<u64>
    where
        C: CompleteImmutableClosureReadPortV1 + ?Sized,
    {
        Ok(u64::from_be_bytes(self.read_array(closure, counters)?))
    }

    fn finish(self) -> CoreResult<()> {
        if self.offset == self.end {
            Ok(())
        } else {
            Err(CoreError::TrailingBytes)
        }
    }
}

fn read_component<C>(
    cursor: &mut ObjectCursorV1,
    closure: &mut C,
    counters: &mut OperationCountersV1,
    destination: &mut [u8; MAX_COMPONENT_BYTES],
) -> CoreResult<usize>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
{
    let len = usize::from(cursor.read_u16_be(closure, counters)?);
    if len == 0 || len > destination.len() {
        return Err(CoreError::Name);
    }
    cursor.read_into(closure, counters, &mut destination[..len])?;
    ValidatedComponent::new(&destination[..len])?;
    Ok(len)
}

enum ReconstructedDirectoryV1 {
    ImplicitRoot(ImplicitRootDirectoryV1),
    Explicit(ExplicitDirectoryNodeV1),
}

#[allow(clippy::too_many_arguments)]
fn reconstruct_root_directory<C>(
    closure: &mut C,
    object_count: u64,
    id: PhysicalTreeIdV1,
    counters: &mut OperationCountersV1,
    source_window: &mut [u8; MAXIMUM_CHUNK_BYTES],
    cdc_ring: &mut [u8; MAXIMUM_CHUNK_BYTES],
    states: &mut [u8],
    visited: &mut u64,
    depth: usize,
    validation_cdc: ClosureCdcV1,
) -> CoreResult<ImplicitRootDirectoryV1>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
{
    match reconstruct_directory(
        closure,
        object_count,
        id,
        DirectoryModeContext::ImplicitRoot,
        counters,
        source_window,
        cdc_ring,
        states,
        visited,
        depth,
        validation_cdc,
    )? {
        ReconstructedDirectoryV1::ImplicitRoot(root) => Ok(root),
        ReconstructedDirectoryV1::Explicit(_) => Err(CoreError::RootSentinel),
    }
}

#[allow(clippy::too_many_arguments)]
fn reconstruct_explicit_directory<C>(
    closure: &mut C,
    object_count: u64,
    id: PhysicalTreeIdV1,
    counters: &mut OperationCountersV1,
    source_window: &mut [u8; MAXIMUM_CHUNK_BYTES],
    cdc_ring: &mut [u8; MAXIMUM_CHUNK_BYTES],
    states: &mut [u8],
    visited: &mut u64,
    depth: usize,
    validation_cdc: ClosureCdcV1,
) -> CoreResult<ExplicitDirectoryNodeV1>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
{
    match reconstruct_directory(
        closure,
        object_count,
        id,
        DirectoryModeContext::Explicit,
        counters,
        source_window,
        cdc_ring,
        states,
        visited,
        depth,
        validation_cdc,
    )? {
        ReconstructedDirectoryV1::Explicit(directory) => Ok(directory),
        ReconstructedDirectoryV1::ImplicitRoot(_) => Err(CoreError::ChildMode),
    }
}

#[allow(clippy::too_many_arguments)]
fn reconstruct_directory<C>(
    closure: &mut C,
    object_count: u64,
    id: PhysicalTreeIdV1,
    context: DirectoryModeContext,
    counters: &mut OperationCountersV1,
    source_window: &mut [u8; MAXIMUM_CHUNK_BYTES],
    cdc_ring: &mut [u8; MAXIMUM_CHUNK_BYTES],
    states: &mut [u8],
    visited: &mut u64,
    depth: usize,
    validation_cdc: ClosureCdcV1,
) -> CoreResult<ReconstructedDirectoryV1>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
{
    require_depth(depth)?;
    let typed = TypedPhysicalObjectIdV1::Tree(id);
    let ordinal = require_edge(closure, object_count, typed)?;
    let first_visit = traversal_enter(states, ordinal)?;
    let mut cursor = ObjectCursorV1::payload(closure, ordinal)?;
    if TreeSubtypeV1::try_from(cursor.read_u8(closure, counters)?)? != TreeSubtypeV1::Directory {
        return Err(CoreError::TypedEdge);
    }
    let mode = cursor.read_u16_be(closure, counters)?;
    let entry_count = cursor.read_u32_be(closure, counters)?;
    let page_depth = cursor.read_u8(closure, counters)?;
    let presence = cursor.read_u8(closure, counters)?;
    let root_page = match presence {
        0 if entry_count == 0 => None,
        1 if entry_count != 0 => Some(PhysicalTreeIdV1::from_digest(
            cursor.read_array::<32, _>(closure, counters)?,
        )),
        _ => return Err(CoreError::TypedEdge),
    };
    cursor.finish()?;
    let mut hasher = LogicalDirectoryHasherV1::new(mode, context, u64::from(entry_count))?;
    if let Some(root_page) = root_page {
        let facts = stream_page_entries(
            closure,
            object_count,
            root_page,
            page_depth,
            &mut hasher,
            counters,
            source_window,
            cdc_ring,
            states,
            visited,
            depth + 1,
            validation_cdc,
        )?;
        if facts.entry_count != entry_count || facts.depth != page_depth {
            return Err(CoreError::IdMismatch);
        }
    }
    if first_visit {
        traversal_finish(states, ordinal, visited)?;
    }
    match context {
        DirectoryModeContext::ImplicitRoot => hasher
            .finish_implicit_root()
            .map(ReconstructedDirectoryV1::ImplicitRoot),
        DirectoryModeContext::Explicit => hasher
            .finish_explicit()
            .map(ReconstructedDirectoryV1::Explicit),
    }
}

#[derive(Clone, Copy)]
struct PageFactsV1 {
    depth: u8,
    entry_count: u32,
    first_name: [u8; MAX_COMPONENT_BYTES],
    first_name_len: usize,
    last_name: [u8; MAX_COMPONENT_BYTES],
    last_name_len: usize,
}

#[allow(clippy::too_many_arguments)]
fn stream_page_entries<C>(
    closure: &mut C,
    object_count: u64,
    id: PhysicalTreeIdV1,
    expected_depth: u8,
    hasher: &mut LogicalDirectoryHasherV1,
    counters: &mut OperationCountersV1,
    source_window: &mut [u8; MAXIMUM_CHUNK_BYTES],
    cdc_ring: &mut [u8; MAXIMUM_CHUNK_BYTES],
    states: &mut [u8],
    visited: &mut u64,
    traversal_depth: usize,
    validation_cdc: ClosureCdcV1,
) -> CoreResult<PageFactsV1>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
{
    require_depth(traversal_depth)?;
    let ordinal = require_edge(closure, object_count, TypedPhysicalObjectIdV1::Tree(id))?;
    let first_visit = traversal_enter(states, ordinal)?;
    let mut cursor = ObjectCursorV1::payload(closure, ordinal)?;
    let subtype = TreeSubtypeV1::try_from(cursor.read_u8(closure, counters)?)?;
    let result = match subtype {
        TreeSubtypeV1::Leaf => {
            let depth = cursor.read_u8(closure, counters)?;
            let count = cursor.read_u16_be(closure, counters)?;
            if expected_depth != 0 || depth != 0 || count == 0 {
                return Err(CoreError::IdMismatch);
            }
            let mut first_name = [0_u8; MAX_COMPONENT_BYTES];
            let mut first_name_len = 0_usize;
            let mut last_name = [0_u8; MAX_COMPONENT_BYTES];
            let mut last_name_len = 0_usize;
            for index in 0..count {
                let mut name = [0_u8; MAX_COMPONENT_BYTES];
                let name_len = read_component(&mut cursor, closure, counters, &mut name)?;
                let kind = PhysicalTreeChildKindV1::try_from(cursor.read_u8(closure, counters)?)?;
                let raw_id = cursor.read_array::<32, _>(closure, counters)?;
                let child = reconstruct_child(
                    closure,
                    object_count,
                    kind,
                    raw_id,
                    counters,
                    source_window,
                    cdc_ring,
                    states,
                    visited,
                    traversal_depth + 1,
                    validation_cdc,
                )?;
                let component = ValidatedComponent::new(&name[..name_len])?;
                hasher.push(LogicalDirectoryEntryV1::new(component, child))?;
                if index == 0 {
                    first_name[..name_len].copy_from_slice(&name[..name_len]);
                    first_name_len = name_len;
                }
                last_name[..name_len].copy_from_slice(&name[..name_len]);
                last_name_len = name_len;
            }
            PageFactsV1 {
                depth,
                entry_count: u32::from(count),
                first_name,
                first_name_len,
                last_name,
                last_name_len,
            }
        }
        TreeSubtypeV1::Index => {
            let depth = cursor.read_u8(closure, counters)?;
            let count = cursor.read_u16_be(closure, counters)?;
            if expected_depth == 0 || depth != expected_depth || count == 0 {
                return Err(CoreError::IdMismatch);
            }
            let mut total = 0_u32;
            let mut first_name = [0_u8; MAX_COMPONENT_BYTES];
            let mut first_name_len = 0_usize;
            let mut last_name = [0_u8; MAX_COMPONENT_BYTES];
            let mut last_name_len = 0_usize;
            for index in 0..count {
                let declared_count = cursor.read_u32_be(closure, counters)?;
                let mut declared_first = [0_u8; MAX_COMPONENT_BYTES];
                let declared_first_len =
                    read_component(&mut cursor, closure, counters, &mut declared_first)?;
                let mut declared_last = [0_u8; MAX_COMPONENT_BYTES];
                let declared_last_len =
                    read_component(&mut cursor, closure, counters, &mut declared_last)?;
                let child =
                    PhysicalTreeIdV1::from_digest(cursor.read_array::<32, _>(closure, counters)?);
                let facts = stream_page_entries(
                    closure,
                    object_count,
                    child,
                    expected_depth - 1,
                    hasher,
                    counters,
                    source_window,
                    cdc_ring,
                    states,
                    visited,
                    traversal_depth + 1,
                    validation_cdc,
                )?;
                if facts.depth.checked_add(1) != Some(expected_depth)
                    || facts.entry_count != declared_count
                    || facts.first_name[..facts.first_name_len]
                        != declared_first[..declared_first_len]
                    || facts.last_name[..facts.last_name_len] != declared_last[..declared_last_len]
                {
                    return Err(CoreError::IdMismatch);
                }
                total = total
                    .checked_add(facts.entry_count)
                    .ok_or(CoreError::IntegerOverflow)?;
                if index == 0 {
                    first_name[..facts.first_name_len]
                        .copy_from_slice(&facts.first_name[..facts.first_name_len]);
                    first_name_len = facts.first_name_len;
                }
                last_name[..facts.last_name_len]
                    .copy_from_slice(&facts.last_name[..facts.last_name_len]);
                last_name_len = facts.last_name_len;
            }
            PageFactsV1 {
                depth,
                entry_count: total,
                first_name,
                first_name_len,
                last_name,
                last_name_len,
            }
        }
        TreeSubtypeV1::Directory => return Err(CoreError::TypedEdge),
    };
    cursor.finish()?;
    if first_visit {
        traversal_finish(states, ordinal, visited)?;
    }
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn reconstruct_child<C>(
    closure: &mut C,
    object_count: u64,
    kind: PhysicalTreeChildKindV1,
    raw_id: [u8; 32],
    counters: &mut OperationCountersV1,
    source_window: &mut [u8; MAXIMUM_CHUNK_BYTES],
    cdc_ring: &mut [u8; MAXIMUM_CHUNK_BYTES],
    states: &mut [u8],
    visited: &mut u64,
    depth: usize,
    validation_cdc: ClosureCdcV1,
) -> CoreResult<LogicalChildIdV1>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
{
    match kind {
        PhysicalTreeChildKindV1::Tree => reconstruct_explicit_directory(
            closure,
            object_count,
            PhysicalTreeIdV1::from_digest(raw_id),
            counters,
            source_window,
            cdc_ring,
            states,
            visited,
            depth,
            validation_cdc,
        )
        .map(LogicalChildIdV1::Directory),
        PhysicalTreeChildKindV1::File => reconstruct_file(
            closure,
            object_count,
            PhysicalFileIdV1::from_digest(raw_id),
            counters,
            source_window,
            cdc_ring,
            states,
            visited,
            depth,
            validation_cdc,
        )
        .map(LogicalChildIdV1::File),
        PhysicalTreeChildKindV1::Symlink => reconstruct_symlink(
            closure,
            object_count,
            PhysicalSymlinkIdV1::from_digest(raw_id),
            counters,
            source_window,
            states,
            visited,
            depth,
        )
        .map(LogicalChildIdV1::Symlink),
    }
}

fn require_depth(depth: usize) -> CoreResult<()> {
    if depth >= MAX_CLOSURE_TRAVERSAL_DEPTH {
        Err(CoreError::CountCap)
    } else {
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn reconstruct_symlink<C>(
    closure: &mut C,
    object_count: u64,
    id: PhysicalSymlinkIdV1,
    counters: &mut OperationCountersV1,
    source_window: &mut [u8; MAXIMUM_CHUNK_BYTES],
    states: &mut [u8],
    visited: &mut u64,
    depth: usize,
) -> CoreResult<SymlinkNodeIdV1>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
{
    require_depth(depth)?;
    let ordinal = require_edge(closure, object_count, TypedPhysicalObjectIdV1::Symlink(id))?;
    let first_visit = traversal_enter(states, ordinal)?;
    let mut cursor = ObjectCursorV1::payload(closure, ordinal)?;
    let target_len = usize::try_from(cursor.read_u32_be(closure, counters)?)
        .map_err(|_| CoreError::IntegerOverflow)?;
    if target_len == 0 || target_len > 4_096 {
        return Err(CoreError::Target);
    }
    cursor.read_into(closure, counters, &mut source_window[..target_len])?;
    cursor.finish()?;
    let target = ValidatedSymlinkTarget::new(&source_window[..target_len])?;
    let result = derive_symlink_node_v1(target)?;
    if first_visit {
        traversal_finish(states, ordinal, visited)?;
    }
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn reconstruct_file<C>(
    closure: &mut C,
    object_count: u64,
    id: PhysicalFileIdV1,
    counters: &mut OperationCountersV1,
    source_window: &mut [u8; MAXIMUM_CHUNK_BYTES],
    cdc_ring: &mut [u8; MAXIMUM_CHUNK_BYTES],
    states: &mut [u8],
    visited: &mut u64,
    depth: usize,
    validation_cdc: ClosureCdcV1,
) -> CoreResult<FileNodeIdV1>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
{
    require_depth(depth)?;
    let ordinal = require_edge(closure, object_count, TypedPhysicalObjectIdV1::File(id))?;
    let first_visit = traversal_enter(states, ordinal)?;
    let mut count_control = ContinueCdcControlV1;
    let mut count_consumer = ChunkCountConsumerV1 { count: 0 };
    let mut count_stream = validation_cdc.stream(cdc_ring, &mut count_control)?;
    let (mode, logical_len) = stream_file_bytes(
        closure,
        object_count,
        ordinal,
        counters,
        source_window,
        states,
        visited,
        depth + 1,
        |bytes| count_stream.push(Ok(bytes), &mut count_control, &mut count_consumer),
    )?;
    count_stream.finish(&mut count_control, &mut count_consumer)?;

    let mut hash_control = ContinueCdcControlV1;
    let mut hash_consumer = LogicalFileConsumerV1 {
        hasher: LogicalFileHasherV1::new(logical_len, count_consumer.count)?,
        failure: None,
    };
    let mut hash_stream = validation_cdc.stream(cdc_ring, &mut hash_control)?;
    let (second_mode, second_len) = stream_file_bytes(
        closure,
        object_count,
        ordinal,
        counters,
        source_window,
        states,
        visited,
        depth + 1,
        |bytes| hash_stream.push(Ok(bytes), &mut hash_control, &mut hash_consumer),
    )?;
    if second_mode != mode || second_len != logical_len {
        return Err(CoreError::IdMismatch);
    }
    if let Err(error) = hash_stream.finish(&mut hash_control, &mut hash_consumer) {
        return Err(hash_consumer.failure.take().unwrap_or(error));
    }
    let logical_file = hash_consumer.hasher.finish()?;
    if first_visit {
        traversal_finish(states, ordinal, visited)?;
    }
    derive_file_node_v1(mode, logical_file)
}

#[allow(clippy::too_many_arguments)]
fn stream_file_bytes<C, F>(
    closure: &mut C,
    object_count: u64,
    file_ordinal: u64,
    counters: &mut OperationCountersV1,
    source_window: &mut [u8; MAXIMUM_CHUNK_BYTES],
    states: &mut [u8],
    visited: &mut u64,
    depth: usize,
    mut consume: F,
) -> CoreResult<(u16, u64)>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
    F: FnMut(&[u8]) -> CoreResult<()>,
{
    let mut cursor = ObjectCursorV1::payload(closure, file_ordinal)?;
    let mode = cursor.read_u16_be(closure, counters)?;
    let logical_len = cursor.read_u64_be(closure, counters)?;
    let extent_count = cursor.read_u32_be(closure, counters)?;
    for _ in 0..extent_count {
        let tag = ExtentTagV1::try_from(cursor.read_u8(closure, counters)?)?;
        let length = cursor.read_u64_be(closure, counters)?;
        match tag {
            ExtentTagV1::Hole => {
                source_window.fill(0);
                let mut remaining = length;
                while remaining != 0 {
                    let take = usize::try_from(remaining.min(MAXIMUM_CHUNK_BYTES as u64))
                        .map_err(|_| CoreError::IntegerOverflow)?;
                    consume(&source_window[..take])?;
                    remaining -= u64::try_from(take).map_err(|_| CoreError::IntegerOverflow)?;
                }
            }
            ExtentTagV1::Data => {
                let count = cursor.read_u32_be(closure, counters)?;
                let mut reconstructed = 0_u64;
                for _ in 0..count {
                    let chunk_len = cursor.read_u32_be(closure, counters)?;
                    let chunk_id = PhysicalChunkIdV1::from_digest(
                        cursor.read_array::<32, _>(closure, counters)?,
                    );
                    stream_chunk_payload(
                        closure,
                        object_count,
                        chunk_id,
                        chunk_len,
                        counters,
                        source_window,
                        states,
                        visited,
                        depth,
                        &mut consume,
                    )?;
                    reconstructed = reconstructed
                        .checked_add(u64::from(chunk_len))
                        .ok_or(CoreError::IntegerOverflow)?;
                }
                if reconstructed != length {
                    return Err(CoreError::LogicalLength);
                }
            }
        }
    }
    cursor.finish()?;
    Ok((mode, logical_len))
}

#[allow(clippy::too_many_arguments)]
fn stream_chunk_payload<C, F>(
    closure: &mut C,
    object_count: u64,
    id: PhysicalChunkIdV1,
    declared_len: u32,
    counters: &mut OperationCountersV1,
    source_window: &mut [u8; MAXIMUM_CHUNK_BYTES],
    states: &mut [u8],
    visited: &mut u64,
    depth: usize,
    consume: &mut F,
) -> CoreResult<()>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
    F: FnMut(&[u8]) -> CoreResult<()>,
{
    require_depth(depth)?;
    let ordinal = require_edge(closure, object_count, TypedPhysicalObjectIdV1::Chunk(id))?;
    let first_visit = traversal_enter(states, ordinal)?;
    let mut cursor = ObjectCursorV1::payload(closure, ordinal)?;
    let actual_len = cursor.remaining();
    if actual_len != u64::from(declared_len) {
        return Err(CoreError::IdMismatch);
    }
    let take = usize::try_from(actual_len).map_err(|_| CoreError::IntegerOverflow)?;
    if take > source_window.len() {
        return Err(CoreError::ResourceRefused);
    }
    cursor.read_into(closure, counters, &mut source_window[..take])?;
    cursor.finish()?;
    consume(&source_window[..take])?;
    if first_visit {
        traversal_finish(states, ordinal, visited)?;
    }
    Ok(())
}

struct ChunkCountConsumerV1 {
    count: u64,
}

impl BoundaryConsumerV1 for ChunkCountConsumerV1 {
    fn accept(
        &mut self,
        _boundary: ChunkBoundaryV1,
        _chunk: BorrowedChunkV1<'_>,
    ) -> Result<(), CdcBoundaryConsumerErrorV1> {
        self.count = self
            .count
            .checked_add(1)
            .ok_or(CdcBoundaryConsumerErrorV1::Refused)?;
        Ok(())
    }
}

struct LogicalFileConsumerV1 {
    hasher: LogicalFileHasherV1,
    failure: Option<CoreError>,
}

impl BoundaryConsumerV1 for LogicalFileConsumerV1 {
    fn accept(
        &mut self,
        _boundary: ChunkBoundaryV1,
        chunk: BorrowedChunkV1<'_>,
    ) -> Result<(), CdcBoundaryConsumerErrorV1> {
        let result = (|| {
            let logical = derive_logical_chunk_spans_v1(chunk.first(), chunk.second())?;
            self.hasher.push(LogicalChunkRefV1::from_identity(logical))
        })();
        if let Err(error) = result {
            self.failure = Some(error);
            Err(CdcBoundaryConsumerErrorV1::Refused)
        } else {
            Ok(())
        }
    }
}
