//! Bounded, transaction-private immutable admission over a random-readable
//! closure spool. The spool may be backed by a pack or temporary file; this
//! module never requires all object bytes or the complete object index to be
//! resident at once.

use super::closure::{
    compare_closure_object_ids_v1, AdmittedClosureV1, CompleteImmutableClosureReadPortV1,
};
use crate::cas::{
    ImmutablePortErrorV1, OccupiedImmutableReadPortV1, PreparedImmutableClosurePortV1,
    ValidatedOccupiedObjectV1,
};
use crate::cdc::{
    BorrowedChunkV1, BoundaryConsumerV1, CdcBoundaryConsumerErrorV1, CdcControlV1,
    CdcSourceErrorV1, ChunkBoundaryV1, FastCdcV1, FastCdcV1Stream, MAXIMUM_CHUNK_BYTES,
};
#[cfg(feature = "operation-polymorphism")]
use crate::cdc::{CdcAlgorithmV1, CdcStreamV1};
use crate::format::{
    DirectoryModeContext, ExtentTagV1, PhysicalTreeChildKindV1, TreeSubtypeV1, ValidatedComponent,
    ValidatedSymlinkTarget, MAX_PATH_DEPTH, MAX_TREE_PAGE_DEPTH,
};
use crate::identity::{
    derive_file_node_v1, derive_logical_chunk_spans_v1, derive_symlink_node_v1, derive_version_v1,
    DeferredCountLogicalFileHasherV1, ExplicitDirectoryNodeV1, FileNodeIdV1,
    ImplicitRootDirectoryV1, LogicalChildIdV1, LogicalChunkRefV1, LogicalDirectoryEntryV1,
    LogicalDirectoryHasherV1, PhysicalChunkIdV1, PhysicalFileIdV1, PhysicalSymlinkIdV1,
    PhysicalTreeIdV1, PhysicalVersionRecordIdV1, SymlinkNodeIdV1, COMPARISON_WINDOW_BYTES,
    DEFERRED_COUNT_LOGICAL_FILE_HASHER_BYTES_V1,
};
#[cfg(any(test, feature = "operation-polymorphism"))]
use crate::limits::ResourceLedgerV1;
use crate::limits::{
    CounterFieldV1, MemoryComponentV1, OperationCountersV1, OperationMemoryPlanV1,
    OperationReservationV1,
};
use crate::object::require_canonical_traversal_depth_v1;
use crate::object::{
    decode_physical_object_from_port_v1, DiscardStrongEdgesV1, PhysicalObjectPayloadV1,
    PhysicalObjectReadPortV1, TreeRecordV1, TypedPhysicalObjectIdV1, VersionRecordV1,
    OBJECT_HEADER_BYTES,
};
use crate::profile::{ChunkerSpecV1, DigestSpecV1};
use crate::{CoreError, CoreResult};

const MAX_COMPONENT_BYTES: usize = 255;
const ADMISSION_DIRECTORY_STACK_CAPACITY_V1: usize = MAX_PATH_DEPTH + 1;

#[derive(Clone, Copy)]
enum ClosureCdcV1 {
    FastCdc,
    #[cfg(feature = "operation-polymorphism")]
    Selected(CdcAlgorithmV1),
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
            #[cfg(feature = "operation-polymorphism")]
            Self::Selected(algorithm) => algorithm
                .stream(ring, control)
                .map(ClosureCdcStreamV1::Selected),
        }
    }
}

enum ClosureCdcStreamV1<'ring> {
    FastCdc(FastCdcV1Stream<'ring>),
    #[cfg(feature = "operation-polymorphism")]
    Selected(CdcStreamV1<'ring>),
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
            #[cfg(feature = "operation-polymorphism")]
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
            #[cfg(feature = "operation-polymorphism")]
            Self::Selected(stream) => stream.finish(control, consumer),
        }
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

#[derive(Clone, Copy)]
enum AdmissionControlStageV1 {
    ClosureValidation,
    CandidateGraph,
}

/// One borrow of the caller-owned operation control plus the direct stage
/// observation that makes each bounded interval independently auditable.
struct AdmissionWorkControlV1<'a, S>
where
    S: PreparedImmutableClosurePortV1 + ?Sized,
{
    sink: &'a mut S,
    counters: &'a mut OperationCountersV1,
    stage: AdmissionControlStageV1,
}

impl<'a, S> AdmissionWorkControlV1<'a, S>
where
    S: PreparedImmutableClosurePortV1 + ?Sized,
{
    const fn new(sink: &'a mut S, counters: &'a mut OperationCountersV1) -> Self {
        Self {
            sink,
            counters,
            stage: AdmissionControlStageV1::ClosureValidation,
        }
    }

    fn poll(&mut self, work_since_poll: u64) -> CoreResult<()> {
        match self.stage {
            AdmissionControlStageV1::ClosureValidation => self
                .counters
                .record_closure_validation_control_poll_v1(work_since_poll)?,
            AdmissionControlStageV1::CandidateGraph => self
                .counters
                .record_candidate_graph_control_poll_v1(work_since_poll)?,
        }
        if self.sink.cancellation_requested_v1() {
            Err(CoreError::Cancelled)
        } else if self.sink.deadline_exceeded_v1() {
            Err(CoreError::Deadline)
        } else {
            Ok(())
        }
    }

    fn begin_candidate_graph(&mut self) -> CoreResult<()> {
        self.poll(0)?;
        self.stage = AdmissionControlStageV1::CandidateGraph;
        self.poll(0)
    }
}

#[cfg(any(test, feature = "operation-polymorphism"))]
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

#[cfg(feature = "operation-polymorphism")]
#[allow(clippy::too_many_arguments)]
pub(crate) fn admit_complete_immutable_borrowed_v1<C, O, S>(
    closure: &mut C,
    expected_version_record: TypedPhysicalObjectIdV1,
    occupied: &mut O,
    sink: &mut S,
    reservation: &OperationReservationV1<'_>,
    counters: &mut OperationCountersV1,
    buffers: AdmissionBuffersV1<'_>,
    algorithm: CdcAlgorithmV1,
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
            admission_traversal_resident_bytes_v1()?,
        )?
        .charge(MemoryComponentV1::MetadataWindow, source_resident)?
        .charge(
            MemoryComponentV1::HashState,
            DEFERRED_COUNT_LOGICAL_FILE_HASHER_BYTES_V1,
        )
}

pub(crate) fn admission_traversal_resident_bytes_v1() -> CoreResult<u64> {
    let frame_bytes = u64::try_from(core::mem::size_of::<AdmissionDirectoryFrameV1>())
        .map_err(|_| CoreError::IntegerOverflow)?;
    let capacity = u64::try_from(ADMISSION_DIRECTORY_STACK_CAPACITY_V1)
        .map_err(|_| CoreError::IntegerOverflow)?;
    frame_bytes
        .checked_mul(capacity)
        .and_then(|bytes| {
            bytes.checked_add(core::mem::size_of::<Vec<AdmissionDirectoryFrameV1>>() as u64)
        })
        .ok_or(CoreError::IntegerOverflow)
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
    let mut control = AdmissionWorkControlV1::new(sink, counters);
    control.poll(0)?;
    control.poll(0)?;
    let object_count = closure.object_count().map_err(map_source_port)?;
    control.poll(1)?;
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

    validate_canonical_typed_ids(closure, object_count, &mut control)?;
    buffers.traversal_state[..required_traversal_bytes].fill(0);
    control.poll(0)?;
    control
        .sink
        .begin_private_closure(object_count)
        .map_err(map_sink_port)?;
    let validation = control.poll(1).and_then(|()| {
        admit_inner(
            closure,
            object_count,
            expected_version_record,
            occupied,
            &mut control,
            buffers,
            required_traversal_bytes,
            validation_cdc,
        )
    });
    let source_poll = control.poll(0);
    let source_read_accounting = closure
        .direct_storage_read_observation()
        .map_err(map_source_port)
        .and_then(|(bytes, calls)| {
            control.counters.record_fscas_read(bytes, calls)?;
            control.poll(calls)
        });
    let source_read_accounting = source_poll.and(source_read_accounting);
    let occupied_poll = control.poll(0);
    let occupied_read_accounting = occupied
        .direct_storage_read_observation()
        .map_err(map_source_port)
        .and_then(|(bytes, calls)| {
            control.counters.record_fscas_read(bytes, calls)?;
            control.poll(calls)
        });
    let occupied_read_accounting = occupied_poll.and(occupied_read_accounting);
    let result = match (validation, source_read_accounting, occupied_read_accounting) {
        (Err(error), _, _) => Err(error),
        (Ok(_), Err(error), _) | (Ok(_), Ok(()), Err(error)) => Err(error),
        (Ok(admitted), Ok(()), Ok(())) => {
            // All fallible accounting precedes the only visibility
            // transition. Assigning the checked snapshot afterward cannot
            // turn a visible complete closure into an accounting failure.
            let visibility = (|| {
                control.poll(0)?;
                let mut visible_counters = *control.counters;
                visible_counters.record_closure_fence()?;
                control
                    .sink
                    .make_closure_visible(expected_version_record)
                    .map_err(map_sink_port)?;
                *control.counters = visible_counters;
                Ok(())
            })();
            visibility.map(|()| admitted)
        }
    };
    if result.is_err() {
        control.sink.abort_private_closure();
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn admit_inner<C, O, S>(
    closure: &mut C,
    object_count: u64,
    expected_version_record: TypedPhysicalObjectIdV1,
    occupied: &mut O,
    control: &mut AdmissionWorkControlV1<'_, S>,
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
        control.poll(0)?;
        let expected_id = closure.object_id_at(ordinal).map_err(map_source_port)?;
        control.poll(1)?;
        control.poll(0)?;
        let len = closure.object_len_at(ordinal).map_err(map_source_port)?;
        control.poll(1)?;
        let decoded = {
            let mut source = ClosureObjectReadV1::new(closure, ordinal, len, control);
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
        validate_object_edges(closure, object_count, ordinal, decoded.payload(), control)?;
        summaries.add(decoded.payload())?;
        if expected_id == expected_version_record {
            let PhysicalObjectPayloadV1::VersionRecord(record) = decoded.payload() else {
                return Err(CoreError::TypeDomain);
            };
            version = Some(record);
        }

        control.poll(0)?;
        let occupied_len = occupied
            .occupied_len(expected_id)
            .map_err(map_source_port)?;
        control.poll(1)?;
        match occupied_len {
            Some(occupied_len) => {
                let validated = validate_and_compare_occupied(
                    closure,
                    ordinal,
                    len,
                    expected_id,
                    occupied,
                    occupied_len,
                    control,
                    buffers.incoming_comparison,
                    buffers.occupied_comparison,
                )?;
                control.poll(0)?;
                control
                    .sink
                    .note_reused_object(validated)
                    .map_err(map_sink_port)?;
                control.poll(1)?;
                reused_count = reused_count
                    .checked_add(1)
                    .ok_or(CoreError::IntegerOverflow)?;
                control
                    .counters
                    .add(CounterFieldV1::ClosureObjectsOccupiedValidated, 1)?;
            }
            None => {
                stage_private_object_bounded(
                    closure,
                    ordinal,
                    len,
                    expected_id,
                    control,
                    buffers.incoming_comparison,
                )?;
                created_count = created_count
                    .checked_add(1)
                    .ok_or(CoreError::IntegerOverflow)?;
                control
                    .counters
                    .add(CounterFieldV1::ClosureObjectsMissing, 1)?;
            }
        }
    }

    let version = version.ok_or(CoreError::MissingClosureEdge)?;
    validate_version_summary(version, summaries, object_count)?;
    control.begin_candidate_graph()?;
    let mut visited = 0_u64;
    let version_ordinal = find_ordinal(closure, object_count, expected_version_record, control)?
        .ok_or(CoreError::MissingClosureEdge)?;
    traversal_enter(
        &mut buffers.traversal_state[..required_traversal_bytes],
        version_ordinal,
    )?;
    let logical_root = reconstruct_root_directory(
        closure,
        object_count,
        version.root_tree_id,
        control,
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
    control.poll(0)?;

    Ok(AdmittedClosureV1 {
        version_record: expected_version_record,
        object_count,
        created_count,
        reused_count,
    })
}

fn validate_canonical_typed_ids<C, S>(
    closure: &mut C,
    count: u64,
    control: &mut AdmissionWorkControlV1<'_, S>,
) -> CoreResult<()>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
    S: PreparedImmutableClosurePortV1 + ?Sized,
{
    let mut previous = None;
    for ordinal in 0..count {
        control.poll(0)?;
        let current = closure.object_id_at(ordinal).map_err(map_source_port)?;
        control.poll(1)?;
        if previous.is_some_and(|left| {
            compare_closure_object_ids_v1(left, current) != core::cmp::Ordering::Less
        }) {
            return Err(CoreError::NonCanonicalOrder);
        }
        previous = Some(current);
    }
    Ok(())
}

struct ClosureObjectReadV1<
    'a,
    'operation,
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
    S: PreparedImmutableClosurePortV1 + ?Sized,
> {
    closure: &'a mut C,
    ordinal: u64,
    len: u64,
    control: &'a mut AdmissionWorkControlV1<'operation, S>,
}

impl<
        'a,
        'operation,
        C: CompleteImmutableClosureReadPortV1 + ?Sized,
        S: PreparedImmutableClosurePortV1 + ?Sized,
    > ClosureObjectReadV1<'a, 'operation, C, S>
{
    const fn new(
        closure: &'a mut C,
        ordinal: u64,
        len: u64,
        control: &'a mut AdmissionWorkControlV1<'operation, S>,
    ) -> Self {
        Self {
            closure,
            ordinal,
            len,
            control,
        }
    }
}

impl<
        C: CompleteImmutableClosureReadPortV1 + ?Sized,
        S: PreparedImmutableClosurePortV1 + ?Sized,
    > PhysicalObjectReadPortV1 for ClosureObjectReadV1<'_, '_, C, S>
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
        self.control.poll(0)?;
        self.closure
            .read_object_exact_at(self.ordinal, offset, destination)
            .map_err(map_source_port)?;
        self.control
            .counters
            .add(CounterFieldV1::BytesRead, amount)?;
        self.control.poll(amount)
    }
}

struct OccupiedObjectReadV1<
    'a,
    'operation,
    O: OccupiedImmutableReadPortV1 + ?Sized,
    S: PreparedImmutableClosurePortV1 + ?Sized,
> {
    port: &'a mut O,
    id: TypedPhysicalObjectIdV1,
    len: u64,
    control: &'a mut AdmissionWorkControlV1<'operation, S>,
}

impl<O: OccupiedImmutableReadPortV1 + ?Sized, S: PreparedImmutableClosurePortV1 + ?Sized>
    PhysicalObjectReadPortV1 for OccupiedObjectReadV1<'_, '_, O, S>
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
        self.control.poll(0)?;
        self.port
            .read_occupied_exact_at(self.id, offset, destination)
            .map_err(map_source_port)?;
        self.control
            .counters
            .add(CounterFieldV1::BytesRead, amount)?;
        self.control.poll(amount)
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
fn validate_and_compare_occupied<C, O, S>(
    closure: &mut C,
    ordinal: u64,
    incoming_len: u64,
    expected_id: TypedPhysicalObjectIdV1,
    occupied: &mut O,
    occupied_len: u64,
    control: &mut AdmissionWorkControlV1<'_, S>,
    incoming_scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    occupied_scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
) -> CoreResult<ValidatedOccupiedObjectV1>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
    O: OccupiedImmutableReadPortV1 + ?Sized,
    S: PreparedImmutableClosurePortV1 + ?Sized,
{
    {
        let mut reader = OccupiedObjectReadV1 {
            port: occupied,
            id: expected_id,
            len: occupied_len,
            control,
        };
        let decoded = decode_physical_object_from_port_v1(
            &mut reader,
            &mut DiscardStrongEdgesV1,
            occupied_scratch,
        )
        .map_err(map_occupant_validation)?;
        if decoded.header().kind() != expected_id.kind() {
            return Err(CoreError::TypeDomain);
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
            control.poll(0)?;
            closure
                .read_object_exact_at(ordinal, offset, &mut incoming_scratch[..incoming_take])
                .map_err(map_source_port)?;
            let incoming_amount =
                u64::try_from(incoming_take).map_err(|_| CoreError::IntegerOverflow)?;
            control
                .counters
                .add(CounterFieldV1::BytesRead, incoming_amount)?;
            control.poll(incoming_amount)?;
        }
        if occupied_take != 0 {
            control.poll(0)?;
            occupied
                .read_occupied_exact_at(expected_id, offset, &mut occupied_scratch[..occupied_take])
                .map_err(map_source_port)?;
            let occupied_amount =
                u64::try_from(occupied_take).map_err(|_| CoreError::IntegerOverflow)?;
            control
                .counters
                .add(CounterFieldV1::BytesRead, occupied_amount)?;
            control.poll(occupied_amount)?;
        }
        if incoming_take != occupied_take
            || incoming_scratch[..incoming_take] != occupied_scratch[..occupied_take]
        {
            equal = false;
        }
        control.poll(amount)?;
        offset = offset
            .checked_add(amount)
            .ok_or(CoreError::IntegerOverflow)?;
    }
    control.poll(0)?;
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
    control: &mut AdmissionWorkControlV1<'_, S>,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
) -> CoreResult<()>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
    S: PreparedImmutableClosurePortV1 + ?Sized,
{
    control.poll(0)?;
    control
        .sink
        .begin_private_object(id, len)
        .map_err(map_sink_port)?;
    control.poll(1)?;
    let mut offset = 0_u64;
    while offset < len {
        let take = usize::try_from((len - offset).min(COMPARISON_WINDOW_BYTES as u64))
            .map_err(|_| CoreError::IntegerOverflow)?;
        control.poll(0)?;
        closure
            .read_object_exact_at(ordinal, offset, &mut scratch[..take])
            .map_err(map_source_port)?;
        let amount = u64::try_from(take).map_err(|_| CoreError::IntegerOverflow)?;
        control.counters.add(CounterFieldV1::BytesRead, amount)?;
        control.poll(amount)?;
        control.poll(0)?;
        control
            .sink
            .write_private_object(&scratch[..take])
            .map_err(map_sink_port)?;
        let mut checked = *control.counters;
        checked.add(CounterFieldV1::BytesCopied, amount)?;
        checked.add(CounterFieldV1::BytesWritten, amount)?;
        *control.counters = checked;
        control.poll(amount)?;
        offset = offset
            .checked_add(amount)
            .ok_or(CoreError::IntegerOverflow)?;
    }
    control.poll(0)?;
    control
        .sink
        .finish_private_object(id)
        .map_err(map_sink_port)?;
    control.poll(1)
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
        | CoreError::Deadline
        | CoreError::Schema
        | CoreError::TypeDomain
        | CoreError::UnknownKind
        | CoreError::IdMismatch => error,
        _ => CoreError::MalformedOccupant,
    }
}

fn validate_object_edges<C, S>(
    closure: &mut C,
    object_count: u64,
    ordinal: u64,
    payload: PhysicalObjectPayloadV1,
    control: &mut AdmissionWorkControlV1<'_, S>,
) -> CoreResult<()>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
    S: PreparedImmutableClosurePortV1 + ?Sized,
{
    match payload {
        PhysicalObjectPayloadV1::VersionRecord(version) => {
            require_edge(
                closure,
                object_count,
                TypedPhysicalObjectIdV1::Tree(version.root_tree_id),
                control,
            )?;
        }
        PhysicalObjectPayloadV1::Tree(TreeRecordV1::Directory(directory)) => {
            if let Some(root) = directory.root_page_id {
                require_edge(
                    closure,
                    object_count,
                    TypedPhysicalObjectIdV1::Tree(root),
                    control,
                )?;
            }
        }
        PhysicalObjectPayloadV1::Tree(TreeRecordV1::Leaf(leaf)) => {
            let mut cursor = ObjectCursorV1::payload(closure, ordinal, control)?;
            let _subtype = cursor.read_u8(closure, control)?;
            let _depth = cursor.read_u8(closure, control)?;
            let count = cursor.read_u16_be(closure, control)?;
            if count != leaf.count {
                return Err(CoreError::IdMismatch);
            }
            for _ in 0..count {
                let mut name = [0_u8; MAX_COMPONENT_BYTES];
                let _name_len = read_component(&mut cursor, closure, control, &mut name)?;
                let kind = PhysicalTreeChildKindV1::try_from(cursor.read_u8(closure, control)?)?;
                let raw = cursor.read_array::<32, _, _>(closure, control)?;
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
                require_edge(closure, object_count, id, control)?;
            }
            cursor.finish()?;
        }
        PhysicalObjectPayloadV1::Tree(TreeRecordV1::Index(index)) => {
            let mut cursor = ObjectCursorV1::payload(closure, ordinal, control)?;
            let _subtype = cursor.read_u8(closure, control)?;
            let _depth = cursor.read_u8(closure, control)?;
            let count = cursor.read_u16_be(closure, control)?;
            if count != index.count {
                return Err(CoreError::IdMismatch);
            }
            for _ in 0..count {
                let _subtree_count = cursor.read_u32_be(closure, control)?;
                let mut first = [0_u8; MAX_COMPONENT_BYTES];
                let _first_len = read_component(&mut cursor, closure, control, &mut first)?;
                let mut last = [0_u8; MAX_COMPONENT_BYTES];
                let _last_len = read_component(&mut cursor, closure, control, &mut last)?;
                let child =
                    PhysicalTreeIdV1::from_digest(cursor.read_array::<32, _, _>(closure, control)?);
                require_edge(
                    closure,
                    object_count,
                    TypedPhysicalObjectIdV1::Tree(child),
                    control,
                )?;
            }
            cursor.finish()?;
        }
        PhysicalObjectPayloadV1::File(file) => {
            let mut cursor = ObjectCursorV1::payload(closure, ordinal, control)?;
            let _mode = cursor.read_u16_be(closure, control)?;
            let _logical_len = cursor.read_u64_be(closure, control)?;
            let extent_count = cursor.read_u32_be(closure, control)?;
            if extent_count != file.extent_count {
                return Err(CoreError::IdMismatch);
            }
            for _ in 0..extent_count {
                let tag = ExtentTagV1::try_from(cursor.read_u8(closure, control)?)?;
                let _extent_len = cursor.read_u64_be(closure, control)?;
                if tag == ExtentTagV1::Data {
                    let count = cursor.read_u32_be(closure, control)?;
                    for _ in 0..count {
                        let _chunk_len = cursor.read_u32_be(closure, control)?;
                        let chunk = PhysicalChunkIdV1::from_digest(
                            cursor.read_array::<32, _, _>(closure, control)?,
                        );
                        require_edge(
                            closure,
                            object_count,
                            TypedPhysicalObjectIdV1::Chunk(chunk),
                            control,
                        )?;
                    }
                }
            }
            cursor.finish()?;
        }
        PhysicalObjectPayloadV1::Symlink(_) | PhysicalObjectPayloadV1::Chunk(_) => {}
    }
    Ok(())
}

fn find_ordinal<C, S>(
    closure: &mut C,
    count: u64,
    id: TypedPhysicalObjectIdV1,
    control: &mut AdmissionWorkControlV1<'_, S>,
) -> CoreResult<Option<u64>>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
    S: PreparedImmutableClosurePortV1 + ?Sized,
{
    let mut low = 0_u64;
    let mut high = count;
    while low < high {
        let middle = low + (high - low) / 2;
        control.poll(0)?;
        let current = closure.object_id_at(middle).map_err(map_source_port)?;
        control.poll(1)?;
        match compare_closure_object_ids_v1(current, id) {
            core::cmp::Ordering::Less => low = middle + 1,
            core::cmp::Ordering::Greater => high = middle,
            core::cmp::Ordering::Equal => return Ok(Some(middle)),
        }
    }
    Ok(None)
}

fn contains_raw_id<C, S>(
    closure: &mut C,
    count: u64,
    raw: [u8; 32],
    control: &mut AdmissionWorkControlV1<'_, S>,
) -> CoreResult<bool>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
    S: PreparedImmutableClosurePortV1 + ?Sized,
{
    for candidate in [
        TypedPhysicalObjectIdV1::VersionRecord(PhysicalVersionRecordIdV1::from_digest(raw)),
        TypedPhysicalObjectIdV1::Tree(PhysicalTreeIdV1::from_digest(raw)),
        TypedPhysicalObjectIdV1::File(PhysicalFileIdV1::from_digest(raw)),
        TypedPhysicalObjectIdV1::Symlink(PhysicalSymlinkIdV1::from_digest(raw)),
        TypedPhysicalObjectIdV1::Chunk(PhysicalChunkIdV1::from_digest(raw)),
    ] {
        if find_ordinal(closure, count, candidate, control)?.is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn require_edge<C, S>(
    closure: &mut C,
    count: u64,
    id: TypedPhysicalObjectIdV1,
    control: &mut AdmissionWorkControlV1<'_, S>,
) -> CoreResult<u64>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
    S: PreparedImmutableClosurePortV1 + ?Sized,
{
    if let Some(ordinal) = find_ordinal(closure, count, id, control)? {
        Ok(ordinal)
    } else if contains_raw_id(closure, count, *id.as_bytes(), control)? {
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
    fn payload<C, S>(
        closure: &mut C,
        ordinal: u64,
        control: &mut AdmissionWorkControlV1<'_, S>,
    ) -> CoreResult<Self>
    where
        C: CompleteImmutableClosureReadPortV1 + ?Sized,
        S: PreparedImmutableClosurePortV1 + ?Sized,
    {
        control.poll(0)?;
        let len = closure.object_len_at(ordinal).map_err(map_source_port)?;
        control.poll(1)?;
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

    fn read_into<C, S>(
        &mut self,
        closure: &mut C,
        control: &mut AdmissionWorkControlV1<'_, S>,
        destination: &mut [u8],
    ) -> CoreResult<()>
    where
        C: CompleteImmutableClosureReadPortV1 + ?Sized,
        S: PreparedImmutableClosurePortV1 + ?Sized,
    {
        let amount = u64::try_from(destination.len()).map_err(|_| CoreError::IntegerOverflow)?;
        let end = self
            .offset
            .checked_add(amount)
            .ok_or(CoreError::IntegerOverflow)?;
        if end > self.end {
            return Err(CoreError::Truncated);
        }
        control.poll(0)?;
        closure
            .read_object_exact_at(self.ordinal, self.offset, destination)
            .map_err(map_source_port)?;
        control.counters.add(CounterFieldV1::BytesRead, amount)?;
        control.poll(amount)?;
        self.offset = end;
        Ok(())
    }

    fn read_array<const N: usize, C, S>(
        &mut self,
        closure: &mut C,
        control: &mut AdmissionWorkControlV1<'_, S>,
    ) -> CoreResult<[u8; N]>
    where
        C: CompleteImmutableClosureReadPortV1 + ?Sized,
        S: PreparedImmutableClosurePortV1 + ?Sized,
    {
        let mut result = [0_u8; N];
        self.read_into(closure, control, &mut result)?;
        Ok(result)
    }

    fn read_u8<C, S>(
        &mut self,
        closure: &mut C,
        control: &mut AdmissionWorkControlV1<'_, S>,
    ) -> CoreResult<u8>
    where
        C: CompleteImmutableClosureReadPortV1 + ?Sized,
        S: PreparedImmutableClosurePortV1 + ?Sized,
    {
        Ok(self.read_array::<1, _, _>(closure, control)?[0])
    }

    fn read_u16_be<C, S>(
        &mut self,
        closure: &mut C,
        control: &mut AdmissionWorkControlV1<'_, S>,
    ) -> CoreResult<u16>
    where
        C: CompleteImmutableClosureReadPortV1 + ?Sized,
        S: PreparedImmutableClosurePortV1 + ?Sized,
    {
        Ok(u16::from_be_bytes(self.read_array(closure, control)?))
    }

    fn read_u32_be<C, S>(
        &mut self,
        closure: &mut C,
        control: &mut AdmissionWorkControlV1<'_, S>,
    ) -> CoreResult<u32>
    where
        C: CompleteImmutableClosureReadPortV1 + ?Sized,
        S: PreparedImmutableClosurePortV1 + ?Sized,
    {
        Ok(u32::from_be_bytes(self.read_array(closure, control)?))
    }

    fn read_u64_be<C, S>(
        &mut self,
        closure: &mut C,
        control: &mut AdmissionWorkControlV1<'_, S>,
    ) -> CoreResult<u64>
    where
        C: CompleteImmutableClosureReadPortV1 + ?Sized,
        S: PreparedImmutableClosurePortV1 + ?Sized,
    {
        Ok(u64::from_be_bytes(self.read_array(closure, control)?))
    }

    fn finish(self) -> CoreResult<()> {
        if self.offset == self.end {
            Ok(())
        } else {
            Err(CoreError::TrailingBytes)
        }
    }
}

fn read_component<C, S>(
    cursor: &mut ObjectCursorV1,
    closure: &mut C,
    control: &mut AdmissionWorkControlV1<'_, S>,
    destination: &mut [u8; MAX_COMPONENT_BYTES],
) -> CoreResult<usize>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
    S: PreparedImmutableClosurePortV1 + ?Sized,
{
    let len = usize::from(cursor.read_u16_be(closure, control)?);
    if len == 0 || len > destination.len() {
        return Err(CoreError::Name);
    }
    cursor.read_into(closure, control, &mut destination[..len])?;
    ValidatedComponent::new(&destination[..len])?;
    Ok(len)
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

impl PageFactsV1 {
    const fn empty(depth: u8) -> Self {
        Self {
            depth,
            entry_count: 0,
            first_name: [0; MAX_COMPONENT_BYTES],
            first_name_len: 0,
            last_name: [0; MAX_COMPONENT_BYTES],
            last_name_len: 0,
        }
    }

    fn add_entry(&mut self, name: &[u8]) -> CoreResult<()> {
        if self.entry_count == 0 {
            self.first_name[..name.len()].copy_from_slice(name);
            self.first_name_len = name.len();
        }
        self.last_name[..name.len()].copy_from_slice(name);
        self.last_name_len = name.len();
        self.entry_count = self
            .entry_count
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(())
    }

    fn add_child(&mut self, child: Self) -> CoreResult<()> {
        if self.entry_count == 0 {
            self.first_name[..child.first_name_len]
                .copy_from_slice(&child.first_name[..child.first_name_len]);
            self.first_name_len = child.first_name_len;
        }
        self.last_name[..child.last_name_len]
            .copy_from_slice(&child.last_name[..child.last_name_len]);
        self.last_name_len = child.last_name_len;
        self.entry_count = self
            .entry_count
            .checked_add(child.entry_count)
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct OwnedComponentV1 {
    bytes: [u8; MAX_COMPONENT_BYTES],
    len: usize,
}

impl OwnedComponentV1 {
    fn read<C, S>(
        cursor: &mut ObjectCursorV1,
        closure: &mut C,
        control: &mut AdmissionWorkControlV1<'_, S>,
    ) -> CoreResult<Self>
    where
        C: CompleteImmutableClosureReadPortV1 + ?Sized,
        S: PreparedImmutableClosurePortV1 + ?Sized,
    {
        let mut bytes = [0; MAX_COMPONENT_BYTES];
        let len = read_component(cursor, closure, control, &mut bytes)?;
        Ok(Self { bytes, len })
    }

    fn validated(&self) -> CoreResult<ValidatedComponent<'_>> {
        ValidatedComponent::new(&self.bytes[..self.len])
    }
}

#[derive(Clone, Copy)]
struct PhysicalTreeEntryOwnedV1 {
    name: OwnedComponentV1,
    kind: PhysicalTreeChildKindV1,
    raw_id: [u8; 32],
}

#[derive(Clone, Copy)]
struct IndexExpectationV1 {
    count: u32,
    first: OwnedComponentV1,
    last: OwnedComponentV1,
}

#[derive(Clone, Copy)]
// This fixed inline state is part of the explicitly charged traversal stack.
// Indirection would add a heap allocation outside that resident-byte proof.
#[allow(clippy::large_enum_variant)]
enum PageFrameStateV1 {
    Leaf {
        remaining: u16,
    },
    Index {
        remaining: u16,
        pending: Option<IndexExpectationV1>,
    },
}

#[derive(Clone, Copy)]
struct PageFrameV1 {
    ordinal: u64,
    first_visit: bool,
    cursor: ObjectCursorV1,
    facts: PageFactsV1,
    state: PageFrameStateV1,
}

const PAGE_TRAVERSAL_CAPACITY_V1: usize = MAX_TREE_PAGE_DEPTH as usize + 1;

#[derive(Clone, Copy)]
struct PageTraversalV1 {
    frames: [Option<PageFrameV1>; PAGE_TRAVERSAL_CAPACITY_V1],
    len: usize,
    finished: Option<PageFactsV1>,
}

impl PageTraversalV1 {
    fn new<C, S>(
        closure: &mut C,
        object_count: u64,
        id: PhysicalTreeIdV1,
        expected_depth: u8,
        control: &mut AdmissionWorkControlV1<'_, S>,
        states: &mut [u8],
    ) -> CoreResult<Self>
    where
        C: CompleteImmutableClosureReadPortV1 + ?Sized,
        S: PreparedImmutableClosurePortV1 + ?Sized,
    {
        let mut result = Self {
            frames: [None; PAGE_TRAVERSAL_CAPACITY_V1],
            len: 0,
            finished: None,
        };
        result.push_page(closure, object_count, id, expected_depth, control, states)?;
        Ok(result)
    }

    fn push_page<C, S>(
        &mut self,
        closure: &mut C,
        object_count: u64,
        id: PhysicalTreeIdV1,
        expected_depth: u8,
        control: &mut AdmissionWorkControlV1<'_, S>,
        states: &mut [u8],
    ) -> CoreResult<()>
    where
        C: CompleteImmutableClosureReadPortV1 + ?Sized,
        S: PreparedImmutableClosurePortV1 + ?Sized,
    {
        if self.len >= self.frames.len() {
            return Err(CoreError::CountCap);
        }
        let ordinal = require_edge(
            closure,
            object_count,
            TypedPhysicalObjectIdV1::Tree(id),
            control,
        )?;
        let first_visit = traversal_enter(states, ordinal)?;
        let mut cursor = ObjectCursorV1::payload(closure, ordinal, control)?;
        let subtype = TreeSubtypeV1::try_from(cursor.read_u8(closure, control)?)?;
        let depth = cursor.read_u8(closure, control)?;
        let count = cursor.read_u16_be(closure, control)?;
        if depth != expected_depth || count == 0 {
            return Err(CoreError::IdMismatch);
        }
        let state = match subtype {
            TreeSubtypeV1::Leaf if expected_depth == 0 => {
                PageFrameStateV1::Leaf { remaining: count }
            }
            TreeSubtypeV1::Index if expected_depth != 0 => PageFrameStateV1::Index {
                remaining: count,
                pending: None,
            },
            _ => return Err(CoreError::TypedEdge),
        };
        self.frames[self.len] = Some(PageFrameV1 {
            ordinal,
            first_visit,
            cursor,
            facts: PageFactsV1::empty(depth),
            state,
        });
        self.len += 1;
        Ok(())
    }

    fn next_entry<C, S>(
        &mut self,
        closure: &mut C,
        object_count: u64,
        control: &mut AdmissionWorkControlV1<'_, S>,
        states: &mut [u8],
        visited: &mut u64,
    ) -> CoreResult<Option<PhysicalTreeEntryOwnedV1>>
    where
        C: CompleteImmutableClosureReadPortV1 + ?Sized,
        S: PreparedImmutableClosurePortV1 + ?Sized,
    {
        loop {
            let index = self.len.checked_sub(1).ok_or(CoreError::Truncated)?;
            let state = self.frames[index].ok_or(CoreError::Truncated)?.state;
            match state {
                PageFrameStateV1::Leaf { remaining } if remaining != 0 => {
                    let frame = self.frames[index].as_mut().ok_or(CoreError::Truncated)?;
                    let name = OwnedComponentV1::read(&mut frame.cursor, closure, control)?;
                    let kind =
                        PhysicalTreeChildKindV1::try_from(frame.cursor.read_u8(closure, control)?)?;
                    let raw_id = frame.cursor.read_array::<32, _, _>(closure, control)?;
                    frame.facts.add_entry(&name.bytes[..name.len])?;
                    frame.state = PageFrameStateV1::Leaf {
                        remaining: remaining - 1,
                    };
                    return Ok(Some(PhysicalTreeEntryOwnedV1 { name, kind, raw_id }));
                }
                PageFrameStateV1::Index {
                    remaining,
                    pending: None,
                } if remaining != 0 => {
                    let (expectation, child, child_depth) = {
                        let frame = self.frames[index].as_mut().ok_or(CoreError::Truncated)?;
                        let count = frame.cursor.read_u32_be(closure, control)?;
                        let first = OwnedComponentV1::read(&mut frame.cursor, closure, control)?;
                        let last = OwnedComponentV1::read(&mut frame.cursor, closure, control)?;
                        let child = PhysicalTreeIdV1::from_digest(
                            frame.cursor.read_array::<32, _, _>(closure, control)?,
                        );
                        let child_depth = frame
                            .facts
                            .depth
                            .checked_sub(1)
                            .ok_or(CoreError::IdMismatch)?;
                        frame.state = PageFrameStateV1::Index {
                            remaining: remaining - 1,
                            pending: Some(IndexExpectationV1 { count, first, last }),
                        };
                        (
                            IndexExpectationV1 { count, first, last },
                            child,
                            child_depth,
                        )
                    };
                    let _ = expectation;
                    self.push_page(closure, object_count, child, child_depth, control, states)?;
                }
                PageFrameStateV1::Index {
                    pending: Some(_), ..
                } => return Err(CoreError::Truncated),
                PageFrameStateV1::Leaf { remaining: 0 }
                | PageFrameStateV1::Index {
                    remaining: 0,
                    pending: None,
                } => {
                    let frame = self.frames[index].take().ok_or(CoreError::Truncated)?;
                    self.len -= 1;
                    frame.cursor.finish()?;
                    if frame.first_visit {
                        traversal_finish(states, frame.ordinal, visited)?;
                    }
                    if self.len == 0 {
                        self.finished = Some(frame.facts);
                        return Ok(None);
                    }
                    let parent = self.frames[self.len - 1]
                        .as_mut()
                        .ok_or(CoreError::Truncated)?;
                    let PageFrameStateV1::Index {
                        remaining,
                        pending: Some(expectation),
                    } = parent.state
                    else {
                        return Err(CoreError::Truncated);
                    };
                    if frame.facts.depth.checked_add(1) != Some(parent.facts.depth)
                        || frame.facts.entry_count != expectation.count
                        || frame.facts.first_name[..frame.facts.first_name_len]
                            != expectation.first.bytes[..expectation.first.len]
                        || frame.facts.last_name[..frame.facts.last_name_len]
                            != expectation.last.bytes[..expectation.last.len]
                    {
                        return Err(CoreError::IdMismatch);
                    }
                    parent.facts.add_child(frame.facts)?;
                    parent.state = PageFrameStateV1::Index {
                        remaining,
                        pending: None,
                    };
                }
                PageFrameStateV1::Leaf { .. } | PageFrameStateV1::Index { .. } => {
                    return Err(CoreError::Truncated);
                }
            }
        }
    }

    fn finish(self) -> CoreResult<PageFactsV1> {
        if self.len == 0 {
            self.finished.ok_or(CoreError::Truncated)
        } else {
            Err(CoreError::Truncated)
        }
    }
}

struct AdmissionDirectoryFrameV1 {
    ordinal: u64,
    first_visit: bool,
    context: DirectoryModeContext,
    entry_count: u32,
    page_depth: u8,
    hasher: LogicalDirectoryHasherV1,
    pages: Option<PageTraversalV1>,
    page_facts: Option<PageFactsV1>,
    pending_name: Option<OwnedComponentV1>,
}

impl AdmissionDirectoryFrameV1 {
    fn new<C, S>(
        closure: &mut C,
        object_count: u64,
        id: PhysicalTreeIdV1,
        context: DirectoryModeContext,
        control: &mut AdmissionWorkControlV1<'_, S>,
        states: &mut [u8],
    ) -> CoreResult<Self>
    where
        C: CompleteImmutableClosureReadPortV1 + ?Sized,
        S: PreparedImmutableClosurePortV1 + ?Sized,
    {
        let ordinal = require_edge(
            closure,
            object_count,
            TypedPhysicalObjectIdV1::Tree(id),
            control,
        )?;
        let first_visit = traversal_enter(states, ordinal)?;
        let mut cursor = ObjectCursorV1::payload(closure, ordinal, control)?;
        if TreeSubtypeV1::try_from(cursor.read_u8(closure, control)?)? != TreeSubtypeV1::Directory {
            return Err(CoreError::TypedEdge);
        }
        let mode = cursor.read_u16_be(closure, control)?;
        let entry_count = cursor.read_u32_be(closure, control)?;
        let page_depth = cursor.read_u8(closure, control)?;
        if u64::from(page_depth) > MAX_TREE_PAGE_DEPTH {
            return Err(CoreError::CountCap);
        }
        let presence = cursor.read_u8(closure, control)?;
        let root_page = match presence {
            0 if entry_count == 0 => None,
            1 if entry_count != 0 => Some(PhysicalTreeIdV1::from_digest(
                cursor.read_array::<32, _, _>(closure, control)?,
            )),
            _ => return Err(CoreError::TypedEdge),
        };
        cursor.finish()?;
        let pages = match root_page {
            Some(root_page) => Some(PageTraversalV1::new(
                closure,
                object_count,
                root_page,
                page_depth,
                control,
                states,
            )?),
            None => None,
        };
        Ok(Self {
            ordinal,
            first_visit,
            context,
            entry_count,
            page_depth,
            hasher: LogicalDirectoryHasherV1::new(mode, context, u64::from(entry_count))?,
            pages,
            page_facts: None,
            pending_name: None,
        })
    }

    fn push_child(&mut self, name: &OwnedComponentV1, child: LogicalChildIdV1) -> CoreResult<()> {
        self.hasher
            .push(LogicalDirectoryEntryV1::new(name.validated()?, child))
    }

    fn is_complete(&self) -> bool {
        self.pages.is_none()
    }

    fn validate_completed_pages(&self) -> CoreResult<()> {
        match (self.entry_count, self.page_facts) {
            (0, None) => Ok(()),
            (count, Some(facts))
                if facts.entry_count == count && facts.depth == self.page_depth =>
            {
                Ok(())
            }
            _ => Err(CoreError::IdMismatch),
        }
    }
}

enum ReconstructedDirectoryV1 {
    ImplicitRoot(ImplicitRootDirectoryV1),
    Explicit(ExplicitDirectoryNodeV1),
}

fn directory_depth_charge_v1(directory_frames: usize) -> CoreResult<usize> {
    let stride = (MAX_TREE_PAGE_DEPTH as usize)
        .checked_add(2)
        .ok_or(CoreError::IntegerOverflow)?;
    let depth = directory_frames
        .checked_sub(1)
        .and_then(|value| value.checked_mul(stride))
        .and_then(|value| value.checked_add(1))
        .ok_or(CoreError::IntegerOverflow)?;
    require_depth(depth)?;
    Ok(depth)
}

#[allow(clippy::too_many_arguments)]
fn reconstruct_root_directory<C, S>(
    closure: &mut C,
    object_count: u64,
    id: PhysicalTreeIdV1,
    control: &mut AdmissionWorkControlV1<'_, S>,
    source_window: &mut [u8; MAXIMUM_CHUNK_BYTES],
    cdc_ring: &mut [u8; MAXIMUM_CHUNK_BYTES],
    states: &mut [u8],
    visited: &mut u64,
    _depth: usize,
    validation_cdc: ClosureCdcV1,
) -> CoreResult<ImplicitRootDirectoryV1>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
    S: PreparedImmutableClosurePortV1 + ?Sized,
{
    let mut frames = Vec::new();
    frames
        .try_reserve_exact(ADMISSION_DIRECTORY_STACK_CAPACITY_V1)
        .map_err(|_| CoreError::ResourceRefused)?;
    if frames.capacity() > ADMISSION_DIRECTORY_STACK_CAPACITY_V1 {
        return Err(CoreError::ResourceRefused);
    }
    frames.push(AdmissionDirectoryFrameV1::new(
        closure,
        object_count,
        id,
        DirectoryModeContext::ImplicitRoot,
        control,
        states,
    )?);
    loop {
        let depth = directory_depth_charge_v1(frames.len())?;
        if frames.last().ok_or(CoreError::Truncated)?.is_complete() {
            let frame = frames.pop().ok_or(CoreError::Truncated)?;
            frame.validate_completed_pages()?;
            if frame.first_visit {
                traversal_finish(states, frame.ordinal, visited)?;
            }
            let logical = match frame.context {
                DirectoryModeContext::ImplicitRoot => frame
                    .hasher
                    .finish_implicit_root()
                    .map(ReconstructedDirectoryV1::ImplicitRoot)?,
                DirectoryModeContext::Explicit => frame
                    .hasher
                    .finish_explicit()
                    .map(ReconstructedDirectoryV1::Explicit)?,
            };
            if let Some(parent) = frames.last_mut() {
                let ReconstructedDirectoryV1::Explicit(directory) = logical else {
                    return Err(CoreError::RootSentinel);
                };
                let name = parent.pending_name.take().ok_or(CoreError::Truncated)?;
                parent.push_child(&name, LogicalChildIdV1::Directory(directory))?;
                continue;
            }
            return match logical {
                ReconstructedDirectoryV1::ImplicitRoot(root) => Ok(root),
                ReconstructedDirectoryV1::Explicit(_) => Err(CoreError::RootSentinel),
            };
        }

        let entry = {
            let frame = frames.last_mut().ok_or(CoreError::Truncated)?;
            frame
                .pages
                .as_mut()
                .ok_or(CoreError::Truncated)?
                .next_entry(closure, object_count, control, states, visited)?
        };
        let Some(entry) = entry else {
            let frame = frames.last_mut().ok_or(CoreError::Truncated)?;
            let pages = frame.pages.take().ok_or(CoreError::Truncated)?;
            frame.page_facts = Some(pages.finish()?);
            continue;
        };
        match entry.kind {
            PhysicalTreeChildKindV1::Tree => {
                if frames
                    .last()
                    .ok_or(CoreError::Truncated)?
                    .pending_name
                    .is_some()
                {
                    return Err(CoreError::Truncated);
                }
                frames.last_mut().ok_or(CoreError::Truncated)?.pending_name = Some(entry.name);
                let next_count = frames
                    .len()
                    .checked_add(1)
                    .ok_or(CoreError::IntegerOverflow)?;
                directory_depth_charge_v1(next_count)?;
                frames.push(AdmissionDirectoryFrameV1::new(
                    closure,
                    object_count,
                    PhysicalTreeIdV1::from_digest(entry.raw_id),
                    DirectoryModeContext::Explicit,
                    control,
                    states,
                )?);
            }
            PhysicalTreeChildKindV1::File => {
                let logical = reconstruct_file(
                    closure,
                    object_count,
                    PhysicalFileIdV1::from_digest(entry.raw_id),
                    control,
                    source_window,
                    cdc_ring,
                    states,
                    visited,
                    depth,
                    validation_cdc,
                )?;
                frames
                    .last_mut()
                    .ok_or(CoreError::Truncated)?
                    .push_child(&entry.name, LogicalChildIdV1::File(logical))?;
            }
            PhysicalTreeChildKindV1::Symlink => {
                let logical = reconstruct_symlink(
                    closure,
                    object_count,
                    PhysicalSymlinkIdV1::from_digest(entry.raw_id),
                    control,
                    source_window,
                    states,
                    visited,
                    depth,
                )?;
                frames
                    .last_mut()
                    .ok_or(CoreError::Truncated)?
                    .push_child(&entry.name, LogicalChildIdV1::Symlink(logical))?;
            }
        }
    }
}

fn require_depth(depth: usize) -> CoreResult<()> {
    require_canonical_traversal_depth_v1(depth)
}

#[allow(clippy::too_many_arguments)]
fn reconstruct_symlink<C, S>(
    closure: &mut C,
    object_count: u64,
    id: PhysicalSymlinkIdV1,
    control: &mut AdmissionWorkControlV1<'_, S>,
    source_window: &mut [u8; MAXIMUM_CHUNK_BYTES],
    states: &mut [u8],
    visited: &mut u64,
    depth: usize,
) -> CoreResult<SymlinkNodeIdV1>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
    S: PreparedImmutableClosurePortV1 + ?Sized,
{
    require_depth(depth)?;
    let ordinal = require_edge(
        closure,
        object_count,
        TypedPhysicalObjectIdV1::Symlink(id),
        control,
    )?;
    let first_visit = traversal_enter(states, ordinal)?;
    let mut cursor = ObjectCursorV1::payload(closure, ordinal, control)?;
    let target_len = usize::try_from(cursor.read_u32_be(closure, control)?)
        .map_err(|_| CoreError::IntegerOverflow)?;
    if target_len == 0 || target_len > 4_096 {
        return Err(CoreError::Target);
    }
    cursor.read_into(closure, control, &mut source_window[..target_len])?;
    cursor.finish()?;
    let target = ValidatedSymlinkTarget::new(&source_window[..target_len])?;
    let result = derive_symlink_node_v1(target)?;
    if first_visit {
        traversal_finish(states, ordinal, visited)?;
    }
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn reconstruct_file<C, S>(
    closure: &mut C,
    object_count: u64,
    id: PhysicalFileIdV1,
    control: &mut AdmissionWorkControlV1<'_, S>,
    source_window: &mut [u8; MAXIMUM_CHUNK_BYTES],
    cdc_ring: &mut [u8; MAXIMUM_CHUNK_BYTES],
    states: &mut [u8],
    visited: &mut u64,
    depth: usize,
    validation_cdc: ClosureCdcV1,
) -> CoreResult<FileNodeIdV1>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
    S: PreparedImmutableClosurePortV1 + ?Sized,
{
    let mut direct = OperationCountersV1::default();
    let result = (|| {
        require_depth(depth)?;
        let ordinal = require_edge(
            closure,
            object_count,
            TypedPhysicalObjectIdV1::File(id),
            control,
        )?;
        let first_visit = traversal_enter(states, ordinal)?;
        let (mode, logical_file) = stream_file_bytes(
            closure,
            object_count,
            ordinal,
            control,
            &mut direct,
            source_window,
            cdc_ring,
            states,
            visited,
            depth + 1,
            validation_cdc,
        )?;
        if first_visit {
            traversal_finish(states, ordinal, visited)?;
        }
        derive_file_node_v1(mode, logical_file)
    })();
    let observation = control.counters.accumulate(direct);
    match (result, observation) {
        (Err(error), _) => Err(error),
        (Ok(value), Ok(())) => Ok(value),
        (Ok(_), Err(error)) => Err(error),
    }
}

#[allow(clippy::too_many_arguments)]
fn stream_file_bytes<C, S>(
    closure: &mut C,
    object_count: u64,
    file_ordinal: u64,
    admission: &mut AdmissionWorkControlV1<'_, S>,
    direct: &mut OperationCountersV1,
    source_window: &mut [u8; MAXIMUM_CHUNK_BYTES],
    cdc_ring: &mut [u8; MAXIMUM_CHUNK_BYTES],
    states: &mut [u8],
    visited: &mut u64,
    depth: usize,
    validation_cdc: ClosureCdcV1,
) -> CoreResult<(u16, crate::identity::LogicalFileIdentityV1)>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
    S: PreparedImmutableClosurePortV1 + ?Sized,
{
    let mut cursor = ObjectCursorV1::payload(closure, file_ordinal, admission)?;
    let mode = cursor.read_u16_be(closure, admission)?;
    let logical_len = cursor.read_u64_be(closure, admission)?;
    let extent_count = cursor.read_u32_be(closure, admission)?;
    let mut control = LogicalReconstructionControlV1::new(admission, direct);
    control.record_pass()?;
    let mut stream = match validation_cdc.stream(cdc_ring, &mut control) {
        Ok(stream) => stream,
        Err(error) => return Err(control.take_failure().unwrap_or(error)),
    };
    let mut consumer = LogicalFileConsumerV1 {
        hasher: DeferredCountLogicalFileHasherV1::new(logical_len)?,
        failure: None,
    };
    for _ in 0..extent_count {
        let tag = ExtentTagV1::try_from(cursor.read_u8(closure, control.admission)?)?;
        let length = cursor.read_u64_be(closure, control.admission)?;
        match tag {
            ExtentTagV1::Hole => {
                source_window.fill(0);
                let mut remaining = length;
                while remaining != 0 {
                    let take = usize::try_from(remaining.min(MAXIMUM_CHUNK_BYTES as u64))
                        .map_err(|_| CoreError::IntegerOverflow)?;
                    push_reconstructed_bytes(
                        &mut stream,
                        &mut control,
                        &mut consumer,
                        &source_window[..take],
                    )?;
                    remaining -= u64::try_from(take).map_err(|_| CoreError::IntegerOverflow)?;
                }
            }
            ExtentTagV1::Data => {
                let count = cursor.read_u32_be(closure, control.admission)?;
                let mut reconstructed = 0_u64;
                for _ in 0..count {
                    let chunk_len = cursor.read_u32_be(closure, control.admission)?;
                    let chunk_id = PhysicalChunkIdV1::from_digest(
                        cursor.read_array::<32, _, _>(closure, control.admission)?,
                    );
                    stream_chunk_payload(
                        closure,
                        object_count,
                        chunk_id,
                        chunk_len,
                        &mut control,
                        &mut stream,
                        &mut consumer,
                        source_window,
                        states,
                        visited,
                        depth,
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
    if let Err(error) = stream.finish(&mut control, &mut consumer) {
        return Err(consumer
            .failure
            .take()
            .or_else(|| control.take_failure())
            .unwrap_or(error));
    }
    Ok((mode, consumer.hasher.finish()?))
}

#[allow(clippy::too_many_arguments)]
fn stream_chunk_payload<C, S>(
    closure: &mut C,
    object_count: u64,
    id: PhysicalChunkIdV1,
    declared_len: u32,
    control: &mut LogicalReconstructionControlV1<'_, '_, S>,
    stream: &mut ClosureCdcStreamV1<'_>,
    consumer: &mut LogicalFileConsumerV1,
    source_window: &mut [u8; MAXIMUM_CHUNK_BYTES],
    states: &mut [u8],
    visited: &mut u64,
    depth: usize,
) -> CoreResult<()>
where
    C: CompleteImmutableClosureReadPortV1 + ?Sized,
    S: PreparedImmutableClosurePortV1 + ?Sized,
{
    require_depth(depth)?;
    let ordinal = require_edge(
        closure,
        object_count,
        TypedPhysicalObjectIdV1::Chunk(id),
        control.admission,
    )?;
    let first_visit = traversal_enter(states, ordinal)?;
    let mut cursor = ObjectCursorV1::payload(closure, ordinal, control.admission)?;
    let actual_len = cursor.remaining();
    if actual_len != u64::from(declared_len) {
        return Err(CoreError::IdMismatch);
    }
    let take = usize::try_from(actual_len).map_err(|_| CoreError::IntegerOverflow)?;
    if take > source_window.len() {
        return Err(CoreError::ResourceRefused);
    }
    control.poll_now()?;
    control.record_payload_call()?;
    let end = cursor
        .offset
        .checked_add(actual_len)
        .ok_or(CoreError::IntegerOverflow)?;
    if end > cursor.end {
        return Err(CoreError::Truncated);
    }
    closure
        .read_object_exact_at(cursor.ordinal, cursor.offset, &mut source_window[..take])
        .map_err(map_source_port)?;
    control
        .admission
        .counters
        .add(CounterFieldV1::BytesRead, actual_len)?;
    cursor.offset = end;
    control.record_payload_bytes(actual_len)?;
    control.poll_now()?;
    cursor.finish()?;
    push_reconstructed_bytes(stream, control, consumer, &source_window[..take])?;
    if first_visit {
        traversal_finish(states, ordinal, visited)?;
    }
    Ok(())
}

fn push_reconstructed_bytes<S>(
    stream: &mut ClosureCdcStreamV1<'_>,
    control: &mut LogicalReconstructionControlV1<'_, '_, S>,
    consumer: &mut LogicalFileConsumerV1,
    bytes: &[u8],
) -> CoreResult<()>
where
    S: PreparedImmutableClosurePortV1 + ?Sized,
{
    control.record_logical_bytes(
        u64::try_from(bytes.len()).map_err(|_| CoreError::IntegerOverflow)?,
    )?;
    if let Err(error) = stream.push(Ok(bytes), control, consumer) {
        return Err(consumer
            .failure
            .take()
            .or_else(|| control.take_failure())
            .unwrap_or(error));
    }
    Ok(())
}

struct LogicalReconstructionControlV1<'a, 'operation, S>
where
    S: PreparedImmutableClosurePortV1 + ?Sized,
{
    admission: &'a mut AdmissionWorkControlV1<'operation, S>,
    direct: &'a mut OperationCountersV1,
    work_since_poll: u64,
    failure: Option<CoreError>,
}

impl<'a, 'operation, S> LogicalReconstructionControlV1<'a, 'operation, S>
where
    S: PreparedImmutableClosurePortV1 + ?Sized,
{
    fn new(
        admission: &'a mut AdmissionWorkControlV1<'operation, S>,
        direct: &'a mut OperationCountersV1,
    ) -> Self {
        Self {
            admission,
            direct,
            work_since_poll: 0,
            failure: None,
        }
    }

    fn record_pass(&mut self) -> CoreResult<()> {
        self.direct.record_logical_reconstruction_pass_v1()
    }

    fn record_payload_call(&mut self) -> CoreResult<()> {
        self.direct.record_logical_reconstruction_payload_call_v1()
    }

    fn record_payload_bytes(&mut self, bytes: u64) -> CoreResult<()> {
        let mut checked = *self.direct;
        checked.record_logical_reconstruction_payload_bytes_v1(bytes)?;
        let work = self
            .work_since_poll
            .checked_add(bytes)
            .ok_or(CoreError::IntegerOverflow)?;
        *self.direct = checked;
        self.work_since_poll = work;
        Ok(())
    }

    fn record_logical_bytes(&mut self, bytes: u64) -> CoreResult<()> {
        let mut checked = *self.direct;
        checked.record_logical_reconstruction_bytes_v1(bytes)?;
        let work = self
            .work_since_poll
            .checked_add(bytes)
            .ok_or(CoreError::IntegerOverflow)?;
        *self.direct = checked;
        self.work_since_poll = work;
        Ok(())
    }

    fn record_poll(&mut self) -> CoreResult<()> {
        self.direct
            .record_logical_reconstruction_poll_v1(self.work_since_poll)?;
        self.work_since_poll = 0;
        Ok(())
    }

    fn poll_now(&mut self) -> CoreResult<()> {
        self.record_poll()?;
        if self.admission.sink.cancellation_requested_v1() {
            Err(CoreError::Cancelled)
        } else if self.admission.sink.deadline_exceeded_v1() {
            Err(CoreError::Deadline)
        } else {
            Ok(())
        }
    }

    fn take_failure(&mut self) -> Option<CoreError> {
        self.failure.take()
    }
}

impl<S> CdcControlV1 for LogicalReconstructionControlV1<'_, '_, S>
where
    S: PreparedImmutableClosurePortV1 + ?Sized,
{
    fn cancellation_requested(&mut self) -> bool {
        if let Err(error) = self.record_poll() {
            self.failure.get_or_insert(error);
            true
        } else {
            self.admission.sink.cancellation_requested_v1()
        }
    }

    fn deadline_exceeded(&mut self) -> bool {
        self.admission.sink.deadline_exceeded_v1()
    }
}

struct LogicalFileConsumerV1 {
    hasher: DeferredCountLogicalFileHasherV1,
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
