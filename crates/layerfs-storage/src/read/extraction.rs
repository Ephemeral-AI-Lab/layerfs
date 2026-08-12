//! Portable private root traversal and extraction through real FsCas locators.
//!
//! The retained complete-closure marker is authenticated before a sink is
//! opened. Every delivered chunk is then completely content-address
//! validated before any of its payload reaches the caller-owned sink. Exact
//! ranges resolve only the canonical metadata and chunks needed for the
//! requested logical interval; they never reconstruct a full file.

use core::cmp::Ordering;

use crate::cas::{
    FsCasBoundaryV1, FsCasControlV1, FsCasErrorV1, FsCasOccupiedV1, FsCasV1, FsOperationKindV1,
    FsOperationObservedControlV1, FsStorageEnvelopeV1, ImmutablePortErrorV1,
};
#[cfg(test)]
use crate::cas::{
    FsCasCleanupTargetV1, FsCasFailureCauseV1, FsCasFilesystemBoundaryV1, FsCasFilesystemFailureV1,
    CATALOG_MARKER_BYTES, PERSISTENT_LOCATOR_BYTES_V1,
};
use crate::content::{
    stream_verified_file_range_v1, VerifiedFileBytesConsumerV1, VerifiedFileRangePortV1,
    VerifiedFileSegmentV1,
};
use crate::format::{
    ExtentTagV1, PhysicalTreeChildKindV1, ValidatedComponent, MAX_PATH_BYTES, MAX_PATH_DEPTH,
    MAX_TREE_PAGE_DEPTH,
};
use crate::identity::{
    PhysicalChunkIdV1, PhysicalFileIdV1, PhysicalSymlinkIdV1, PhysicalTreeIdV1,
    PhysicalVersionRecordIdV1, COMPARISON_WINDOW_BYTES, IDENTITY_HASHER_BYTES_V1,
};
use crate::limits::{
    CounterFieldV1, MemoryComponentV1, OperationCountersV1, OperationMemoryPlanV1,
};
use crate::object::{
    decode_physical_object_from_port_v1, CanonicalTraversalBudgetV1, DiscardStrongEdgesV1,
    PhysicalObjectPayloadV1, TreeRecordV1, TypedPhysicalObjectIdV1,
};
use crate::{CoreError, CoreResult};

use super::object_reader::OccupiedObjectReaderV1;
#[cfg(test)]
use super::range::read_file_range_v1;
use super::range::{
    begin_exact_range_digest_v1, execute_exact_range_v1, ExactRangeExecutorV1, ExactRangePlanV1,
    ExactRangeRequestV1,
};

const FULL_DIGEST_DOMAIN: &[u8; 8] = b"L155EXT1";
const CLOSURE_MARKER_BYTES: u64 = 120;

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
    const fn into_fscas_v1(self) -> FsCasErrorV1 {
        match self {
            Self::Core(error) => FsCasErrorV1::Core(error),
            Self::FsCas(error) => error,
            Self::Sink(ReadSinkErrorV1::Refused) => FsCasErrorV1::Core(CoreError::SinkRefused),
        }
    }

    fn dominated_by_fscas_v1(self, dominant: FsCasErrorV1) -> Self {
        Self::FsCas(self.into_fscas_v1().dominated_by_v1(dominant))
    }

    fn retain_terminal_v1(current: Option<Self>, candidate: Self) -> Option<Self> {
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

#[allow(clippy::too_many_arguments)]
pub(crate) fn extract_root_v1<S, C>(
    cas: &FsCasV1,
    cancellation_key: u64,
    version_record: PhysicalVersionRecordIdV1,
    requested_root: PhysicalTreeIdV1,
    sink: &mut S,
    counters: &mut OperationCountersV1,
    buffers: ReadBuffersV1<'_>,
    control: &mut C,
) -> Result<ReadResultV1, ReadOperationErrorV1>
where
    S: ReadSinkV1 + ?Sized,
    C: FsCasControlV1 + ?Sized,
{
    run_read_v1(
        cas,
        FsOperationKindV1::RootExtraction,
        cancellation_key,
        version_record,
        requested_root,
        ReadRequestV1::Full,
        sink,
        counters,
        buffers,
        control,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn read_file_range_impl_v1<S, C>(
    cas: &FsCasV1,
    cancellation_key: u64,
    version_record: PhysicalVersionRecordIdV1,
    requested_root: PhysicalTreeIdV1,
    path: &[u8],
    offset: u64,
    len: u64,
    sink: &mut S,
    counters: &mut OperationCountersV1,
    buffers: ReadBuffersV1<'_>,
    control: &mut C,
) -> Result<ReadResultV1, ReadOperationErrorV1>
where
    S: ReadSinkV1 + ?Sized,
    C: FsCasControlV1 + ?Sized,
{
    run_read_v1(
        cas,
        FsOperationKindV1::ExactFileRangeRead,
        cancellation_key,
        version_record,
        requested_root,
        ReadRequestV1::RangeInput(ExactRangeRequestV1::new(path, offset, len)),
        sink,
        counters,
        buffers,
        control,
    )
}

#[derive(Clone, Copy)]
enum ReadRequestV1<'a> {
    Full,
    RangeInput(ExactRangeRequestV1<'a>),
    Range(ExactRangePlanV1<'a>),
}

#[derive(Clone, Copy)]
enum ValidatedReadRequestV1<'a> {
    Full,
    Range(ExactRangePlanV1<'a>),
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ReadSinkTransactionStateV1 {
    NotStarted,
    Active,
    Finished,
    AbortAttempted,
}

#[allow(clippy::too_many_arguments)]
fn run_read_v1<S, C>(
    cas: &FsCasV1,
    operation_kind: FsOperationKindV1,
    cancellation_key: u64,
    version_record: PhysicalVersionRecordIdV1,
    requested_root: PhysicalTreeIdV1,
    request: ReadRequestV1<'_>,
    sink: &mut S,
    counters: &mut OperationCountersV1,
    buffers: ReadBuffersV1<'_>,
    control: &mut C,
) -> Result<ReadResultV1, ReadOperationErrorV1>
where
    S: ReadSinkV1 + ?Sized,
    C: FsCasControlV1 + ?Sized,
{
    // Root admission is the first operation action. In particular, do not
    // validate the typed request, query a sink declaration, borrow buffer
    // dimensions, or open occupied storage before this opaque capability is
    // held. The non-cloneable capability remains live through terminal
    // accounting and is released explicitly immediately before outer return.
    control.boundary_reached(FsCasBoundaryV1::BeforeOperationSlotReservationRequest);
    let mut operation = cas
        .begin_operation_capability_v1(operation_kind, cancellation_key, counters, control)
        .map_err(ReadOperationErrorV1::FsCas)?;
    let mut observed_control = FsOperationObservedControlV1::new(control);
    let control = &mut observed_control;
    let mut sink_transaction = ReadSinkTransactionStateV1::NotStarted;
    let terminal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> Result<ReadResultV1, ReadOperationErrorV1> {
            // Reads reserve a directly observed zero-write storage equation
            // before request/path/sink inspection. The resulting token binds
            // every occupied-object and closure read to this exact root
            // operation without granting mutation or publication authority.
            operation
                .declare_storage_envelope_v1(
                    FsStorageEnvelopeV1::new(0, 0, 0, 0).map_err(ReadOperationErrorV1::Core)?,
                )
                .map_err(ReadOperationErrorV1::FsCas)?;
            check_control(control).map_err(ReadOperationErrorV1::Core)?;

            let request = match request {
                ReadRequestV1::Full => ValidatedReadRequestV1::Full,
                ReadRequestV1::RangeInput(request) => ValidatedReadRequestV1::Range(
                    request.validate().map_err(ReadOperationErrorV1::Core)?,
                ),
                ReadRequestV1::Range(plan) => ValidatedReadRequestV1::Range(plan),
            };
            let metadata_resident = cas
                .occupied_resident_memory_bound_v1()
                .map_err(ReadOperationErrorV1::Core)?
                .checked_add(
                    sink.resident_memory_bound_bytes()
                        .map_err(ReadOperationErrorV1::Core)?,
                )
                .ok_or(ReadOperationErrorV1::Core(CoreError::IntegerOverflow))?;
            let plan = OperationMemoryPlanV1::empty()
                .charge(
                    MemoryComponentV1::ComparisonWindow,
                    buffers.comparison.len() as u64,
                )
                .map_err(ReadOperationErrorV1::Core)?
                .charge(
                    MemoryComponentV1::TraversalState,
                    u64::try_from(core::mem::size_of::<BoundedFullTraversalStackV1>())
                        .map_err(|_| ReadOperationErrorV1::Core(CoreError::IntegerOverflow))?
                        .checked_add(buffers.path.len() as u64)
                        .ok_or(CoreError::IntegerOverflow)?,
                )
                .map_err(ReadOperationErrorV1::Core)?
                .charge(MemoryComponentV1::MetadataWindow, metadata_resident)
                .map_err(ReadOperationErrorV1::Core)?
                .charge(MemoryComponentV1::HashState, IDENTITY_HASHER_BYTES_V1)
                .map_err(ReadOperationErrorV1::Core)?;
            operation
                .declare_plan_v1(plan)
                .map_err(ReadOperationErrorV1::Core)?;
            counters.memory_high_water = counters
                .memory_high_water
                .max(operation.memory_high_water_bytes_v1());
            let storage_token = operation
                .storage_token_v1()
                .map_err(ReadOperationErrorV1::FsCas)?;
            check_control(control).map_err(ReadOperationErrorV1::Core)?;

            // Opening an occupied reader is private storage participation and must
            // occur only after the one orchestrator-owned operation slot is held.
            let mut occupied = cas
                .occupied_private_controlled_borrowed_v1(storage_token, control)
                .map_err(ReadOperationErrorV1::FsCas)?;

            let closure = cas
                .validate_closure_for_read_controlled_borrowed_v1(
                    storage_token,
                    version_record,
                    control,
                )
                .map_err(ReadOperationErrorV1::FsCas)?;
            if closure.version_record() != version_record {
                return Err(ReadOperationErrorV1::Core(CoreError::IdMismatch));
            }
            counters
                .add(CounterFieldV1::BytesRead, CLOSURE_MARKER_BYTES)
                .map_err(ReadOperationErrorV1::Core)?;
            counters
                .record_fscas_read(CLOSURE_MARKER_BYTES, 1)
                .map_err(ReadOperationErrorV1::Core)?;

            let result = (|| {
                let mut reader = ReaderV1 {
                    occupied: &mut occupied,
                    sink,
                    counters,
                    comparison: buffers.comparison,
                    path: buffers.path,
                    path_len: 0,
                    path_depth: 0,
                    control,
                    payload_bytes: 0,
                    files: 0,
                    directories: 0,
                    symlinks: 0,
                    objects_traversed: 0,
                    payload_direct_bytes: 0,
                    payload_direct_calls: 0,
                };
                let version = reader
                    .validate_object(TypedPhysicalObjectIdV1::VersionRecord(version_record))?;
                let PhysicalObjectPayloadV1::VersionRecord(version) = version.payload else {
                    return Err(CoreError::TypeDomain);
                };
                if version.root_tree_id != requested_root
                    || u64::from(version.total_object_count) != closure.object_count()
                {
                    return Err(CoreError::IdMismatch);
                }
                check_control(reader.control)?;

                let (kind, digest, ranges) = match request {
                    ValidatedReadRequestV1::Full => {
                        reader
                            .sink
                            .begin_read(ReadKindV1::FullExtraction)
                            .map_err(map_sink)?;
                        sink_transaction = ReadSinkTransactionStateV1::Active;
                        let mut hasher =
                            begin_digest(FULL_DIGEST_DOMAIN, version_record, requested_root);
                        let operation = reader.walk_root_full(requested_root, &mut hasher);
                        let digest = match operation {
                            Ok(()) => finish_digest(hasher),
                            Err(error) => return Err(error),
                        };
                        (ReadKindV1::FullExtraction, digest, 0)
                    }
                    ValidatedReadRequestV1::Range(plan) => {
                        reader
                            .sink
                            .begin_read(ReadKindV1::ExactRange)
                            .map_err(map_sink)?;
                        sink_transaction = ReadSinkTransactionStateV1::Active;
                        let mut hasher =
                            begin_exact_range_digest_v1(version_record, requested_root, plan);
                        let operation =
                            execute_exact_range_v1(&mut reader, requested_root, plan, &mut hasher);
                        let digest = match operation {
                            Ok(()) => finish_digest(hasher),
                            Err(error) => return Err(error),
                        };
                        (ReadKindV1::ExactRange, digest, 1)
                    }
                };
                Ok(ReadResultV1 {
                    kind,
                    verification_digest: digest,
                    payload_bytes: reader.payload_bytes,
                    files: reader.files,
                    directories: reader.directories,
                    symlinks: reader.symlinks,
                    ranges,
                    objects_traversed: reader.objects_traversed,
                    closure_direct_bytes: CLOSURE_MARKER_BYTES,
                    closure_direct_calls: 1,
                    metadata_direct_bytes: 0,
                    metadata_direct_calls: 0,
                    payload_direct_bytes: reader.payload_direct_bytes,
                    payload_direct_calls: reader.payload_direct_calls,
                })
            })();

            let first_fscas_error = occupied.first_error_typed_v1();
            let terminal = result.map_err(|error| {
                first_fscas_error.map_or_else(
                    || {
                        if error == CoreError::SinkRefused {
                            ReadOperationErrorV1::Sink(ReadSinkErrorV1::Refused)
                        } else {
                            ReadOperationErrorV1::Core(error)
                        }
                    },
                    ReadOperationErrorV1::FsCas,
                )
            });
            let (direct_bytes, direct_calls) =
                match occupied.direct_storage_read_observation_typed_v1() {
                    Ok(observation) => observation,
                    Err(error) => {
                        return Err(ReadOperationErrorV1::retain_terminal_v1(
                            terminal.err(),
                            ReadOperationErrorV1::FsCas(error),
                        )
                        .expect("direct observation retains a terminal failure"));
                    }
                };
            if let Err(error) = counters.record_fscas_read(direct_bytes, direct_calls) {
                return Err(ReadOperationErrorV1::retain_terminal_v1(
                    terminal.err(),
                    ReadOperationErrorV1::Core(error),
                )
                .expect("direct counter transfer retains a terminal failure"));
            }
            let terminal = match terminal {
                Ok(mut value) => {
                    value.metadata_direct_bytes = direct_bytes
                        .checked_sub(value.payload_direct_bytes)
                        .ok_or(ReadOperationErrorV1::Core(CoreError::IntegerOverflow))?;
                    value.metadata_direct_calls = direct_calls
                        .checked_sub(value.payload_direct_calls)
                        .ok_or(ReadOperationErrorV1::Core(CoreError::IntegerOverflow))?;
                    Ok(value)
                }
                Err(error) => Err(error),
            }?;

            // `finish_read` is the caller-owned transaction's only success
            // boundary. All fallible LayerFS validation and direct read
            // attribution above must complete first, so a later internal
            // failure can still be paired with exactly one explicit abort.
            sink.finish_read(terminal.verification_digest)
                .map_err(ReadOperationErrorV1::Sink)?;
            sink_transaction = ReadSinkTransactionStateV1::Finished;
            Ok(terminal)
        },
    ));
    let sink_abort = if sink_transaction == ReadSinkTransactionStateV1::Active {
        sink_transaction = ReadSinkTransactionStateV1::AbortAttempted;
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sink.abort_read()))
    } else {
        Ok(())
    };
    let commit = matches!(terminal, Ok(Ok(_)))
        && sink_transaction == ReadSinkTransactionStateV1::Finished
        && sink_abort.is_ok();
    // Read terminalization uses the same one-capability boundary as complete
    // mutation. A controlled invalidation callback may unwind while storage
    // is being terminalized; contain that payload so queue/memory authority
    // is still released explicitly before `Drop` becomes only a backstop.
    let operation_terminal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        operation.finish_terminal_v1(commit, counters, control)
    }));
    let observation_terminal = observed_control.finish_v1(counters);
    let (operation_terminal, operation_unwind) = match operation_terminal {
        Ok(terminal) => (terminal, None),
        Err(payload) => (Ok(()), Some(payload)),
    };
    // Preserve the chronological body cause first. Only the existing typed
    // cleanup/invalidation representation may replace the dominant half;
    // observation and other later terminals remain secondary.
    let mut terminal_failure = match &terminal {
        Ok(Err(error)) => Some(*error),
        Ok(Ok(_)) | Err(_) => None,
    };
    if let Err(error) = operation_terminal {
        terminal_failure = ReadOperationErrorV1::retain_terminal_v1(
            terminal_failure,
            ReadOperationErrorV1::FsCas(error),
        );
    }
    if let Err(error) = observation_terminal {
        terminal_failure = ReadOperationErrorV1::retain_terminal_v1(
            terminal_failure,
            ReadOperationErrorV1::Core(error),
        );
    }
    if let Some(failure) = terminal_failure {
        if let Err(payload) = terminal {
            drop(payload);
        }
        if let Err(payload) = sink_abort {
            drop(payload);
        }
        drop(operation_unwind);
        return Err(failure);
    }

    // With no typed terminal, preserve the established caller-abort unwind
    // behavior, then the body unwind, then operation terminalization.
    if let Err(payload) = sink_abort {
        drop(terminal.err());
        drop(operation_unwind);
        std::panic::resume_unwind(payload);
    }
    let value = match terminal {
        Ok(Ok(value)) => value,
        Ok(Err(_)) => unreachable!("typed read failure was returned above"),
        Err(payload) => {
            drop(operation_unwind);
            std::panic::resume_unwind(payload)
        }
    };
    if let Some(payload) = operation_unwind {
        std::panic::resume_unwind(payload);
    }
    Ok(value)
}

struct ValidatedObjectV1 {
    len: u64,
    payload: PhysicalObjectPayloadV1,
}

#[derive(Clone, Copy)]
enum FullTraversalFrameV1 {
    Directory {
        id: PhysicalTreeIdV1,
        path_len: usize,
        path_depth: usize,
    },
    Page {
        id: PhysicalTreeIdV1,
        expected_depth: u8,
        path_len: usize,
        path_depth: usize,
    },
    LeafEntries {
        cursor: ObjectCursorV1,
        remaining: u16,
        path_len: usize,
        path_depth: usize,
    },
    IndexEntries {
        cursor: ObjectCursorV1,
        remaining: u16,
        child_depth: u8,
        path_len: usize,
        path_depth: usize,
    },
    File {
        id: PhysicalFileIdV1,
        path_len: usize,
        path_depth: usize,
    },
    Symlink {
        id: PhysicalSymlinkIdV1,
        path_len: usize,
        path_depth: usize,
    },
}

struct BoundedFullTraversalStackV1 {
    frames: [Option<FullTraversalFrameV1>; crate::object::MAX_CANONICAL_TRAVERSAL_FRAMES_V1],
    budget: CanonicalTraversalBudgetV1,
}

impl BoundedFullTraversalStackV1 {
    fn new() -> Self {
        Self {
            frames: [None; crate::object::MAX_CANONICAL_TRAVERSAL_FRAMES_V1],
            budget: CanonicalTraversalBudgetV1::new(),
        }
    }

    fn push(&mut self, frame: FullTraversalFrameV1) -> CoreResult<()> {
        self.budget.push()?;
        let index = self.budget.len() - 1;
        self.frames[index] = Some(frame);
        Ok(())
    }

    fn pop(&mut self) -> Option<FullTraversalFrameV1> {
        let index = self.budget.len().checked_sub(1)?;
        let frame = self.frames[index].take();
        let _ = self.budget.pop();
        frame
    }
}

struct ReaderV1<'a, S: ReadSinkV1 + ?Sized, C: FsCasControlV1 + ?Sized> {
    occupied: &'a mut FsCasOccupiedV1,
    sink: &'a mut S,
    counters: &'a mut OperationCountersV1,
    comparison: &'a mut [u8; COMPARISON_WINDOW_BYTES],
    path: &'a mut [u8; MAX_PATH_BYTES],
    path_len: usize,
    path_depth: usize,
    control: &'a mut C,
    payload_bytes: u64,
    files: u64,
    directories: u64,
    symlinks: u64,
    objects_traversed: u64,
    payload_direct_bytes: u64,
    payload_direct_calls: u64,
}

impl<S: ReadSinkV1 + ?Sized, C: FsCasControlV1 + ?Sized> ReaderV1<'_, S, C> {
    fn required_occupied_len(&mut self, id: TypedPhysicalObjectIdV1) -> CoreResult<u64> {
        match self
            .occupied
            .occupied_len_typed_controlled_v1(id, self.control)
        {
            Ok(Some(len)) => Ok(len),
            Ok(None) => {
                self.occupied
                    .retain_first_error_typed_v1(FsCasErrorV1::MissingOccupant);
                Err(CoreError::SourceFailure)
            }
            Err(error) => {
                self.occupied.retain_first_error_typed_v1(error);
                Err(CoreError::SourceFailure)
            }
        }
    }

    fn validate_object(&mut self, id: TypedPhysicalObjectIdV1) -> CoreResult<ValidatedObjectV1> {
        check_control(self.control)?;
        let len = self.required_occupied_len(id)?;
        let mut source =
            OccupiedObjectReaderV1::new(self.occupied, self.counters, self.control, id, len);
        let decoded = decode_physical_object_from_port_v1(
            &mut source,
            &mut DiscardStrongEdgesV1,
            self.comparison,
        )?;
        if decoded.physical_id() != id || decoded.header().kind() != id.kind() {
            return Err(CoreError::IdMismatch);
        }
        self.objects_traversed = self
            .objects_traversed
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(ValidatedObjectV1 {
            len,
            payload: decoded.payload(),
        })
    }

    fn cursor(&mut self, id: TypedPhysicalObjectIdV1, len: u64) -> ObjectCursorV1 {
        ObjectCursorV1 {
            id,
            offset: crate::object::OBJECT_HEADER_BYTES,
            end: len,
        }
    }

    fn walk_root_full(
        &mut self,
        id: PhysicalTreeIdV1,
        hasher: &mut blake3::Hasher,
    ) -> CoreResult<()> {
        let mut stack = BoundedFullTraversalStackV1::new();
        stack.push(FullTraversalFrameV1::Directory {
            id,
            path_len: 0,
            path_depth: 0,
        })?;
        while let Some(frame) = stack.pop() {
            check_control(self.control)?;
            match frame {
                FullTraversalFrameV1::Directory {
                    id,
                    path_len,
                    path_depth,
                } => {
                    self.restore_path(path_len, path_depth)?;
                    let object = self.validate_object(TypedPhysicalObjectIdV1::Tree(id))?;
                    let PhysicalObjectPayloadV1::Tree(TreeRecordV1::Directory(directory)) =
                        object.payload
                    else {
                        return Err(CoreError::TypedEdge);
                    };
                    self.directories = self
                        .directories
                        .checked_add(1)
                        .ok_or(CoreError::IntegerOverflow)?;
                    digest_entry_prefix(
                        hasher,
                        0x10,
                        &self.path[..self.path_len],
                        directory.mode,
                        u64::from(directory.entry_count),
                    );
                    if let Some(page) = directory.root_page_id {
                        stack.push(FullTraversalFrameV1::Page {
                            id: page,
                            expected_depth: directory.page_depth,
                            path_len,
                            path_depth,
                        })?;
                    }
                }
                FullTraversalFrameV1::Page {
                    id,
                    expected_depth,
                    path_len,
                    path_depth,
                } => {
                    self.restore_path(path_len, path_depth)?;
                    let object = self.validate_object(TypedPhysicalObjectIdV1::Tree(id))?;
                    let mut cursor = self.cursor(TypedPhysicalObjectIdV1::Tree(id), object.len);
                    match object.payload {
                        PhysicalObjectPayloadV1::Tree(TreeRecordV1::Leaf(leaf)) => {
                            if expected_depth != 0
                                || cursor.read_u8(self.occupied, self.counters, self.control)?
                                    != 0x02
                                || cursor.read_u8(self.occupied, self.counters, self.control)?
                                    != leaf.depth
                                || cursor.read_u16(self.occupied, self.counters, self.control)?
                                    != leaf.count
                            {
                                return Err(CoreError::TypeDomain);
                            }
                            stack.push(FullTraversalFrameV1::LeafEntries {
                                cursor,
                                remaining: leaf.count,
                                path_len,
                                path_depth,
                            })?;
                        }
                        PhysicalObjectPayloadV1::Tree(TreeRecordV1::Index(index)) => {
                            if expected_depth == 0
                                || index.depth != expected_depth
                                || cursor.read_u8(self.occupied, self.counters, self.control)?
                                    != 0x03
                                || cursor.read_u8(self.occupied, self.counters, self.control)?
                                    != index.depth
                                || cursor.read_u16(self.occupied, self.counters, self.control)?
                                    != index.count
                            {
                                return Err(CoreError::TypeDomain);
                            }
                            stack.push(FullTraversalFrameV1::IndexEntries {
                                cursor,
                                remaining: index.count,
                                child_depth: expected_depth - 1,
                                path_len,
                                path_depth,
                            })?;
                        }
                        _ => return Err(CoreError::TypedEdge),
                    }
                }
                FullTraversalFrameV1::LeafEntries {
                    mut cursor,
                    remaining,
                    path_len,
                    path_depth,
                } => {
                    self.restore_path(path_len, path_depth)?;
                    if remaining == 0 {
                        cursor.finish()?;
                        continue;
                    }
                    let mut component = [0_u8; 255];
                    let component_len = cursor.read_component(
                        self.occupied,
                        self.counters,
                        self.control,
                        &mut component,
                    )?;
                    let kind = PhysicalTreeChildKindV1::try_from(cursor.read_u8(
                        self.occupied,
                        self.counters,
                        self.control,
                    )?)?;
                    let digest =
                        cursor.read_array::<32>(self.occupied, self.counters, self.control)?;
                    stack.push(FullTraversalFrameV1::LeafEntries {
                        cursor,
                        remaining: remaining - 1,
                        path_len,
                        path_depth,
                    })?;
                    self.push_component(&component[..component_len])?;
                    let child_path_len = self.path_len;
                    let child_path_depth = self.path_depth;
                    let child = match kind {
                        PhysicalTreeChildKindV1::Tree => FullTraversalFrameV1::Directory {
                            id: PhysicalTreeIdV1::from_digest(digest),
                            path_len: child_path_len,
                            path_depth: child_path_depth,
                        },
                        PhysicalTreeChildKindV1::File => FullTraversalFrameV1::File {
                            id: PhysicalFileIdV1::from_digest(digest),
                            path_len: child_path_len,
                            path_depth: child_path_depth,
                        },
                        PhysicalTreeChildKindV1::Symlink => FullTraversalFrameV1::Symlink {
                            id: PhysicalSymlinkIdV1::from_digest(digest),
                            path_len: child_path_len,
                            path_depth: child_path_depth,
                        },
                    };
                    stack.push(child)?;
                }
                FullTraversalFrameV1::IndexEntries {
                    mut cursor,
                    remaining,
                    child_depth,
                    path_len,
                    path_depth,
                } => {
                    self.restore_path(path_len, path_depth)?;
                    if remaining == 0 {
                        cursor.finish()?;
                        continue;
                    }
                    let _subtree = cursor.read_u32(self.occupied, self.counters, self.control)?;
                    cursor.skip_component(self.occupied, self.counters, self.control)?;
                    cursor.skip_component(self.occupied, self.counters, self.control)?;
                    let child = PhysicalTreeIdV1::from_digest(cursor.read_array::<32>(
                        self.occupied,
                        self.counters,
                        self.control,
                    )?);
                    stack.push(FullTraversalFrameV1::IndexEntries {
                        cursor,
                        remaining: remaining - 1,
                        child_depth,
                        path_len,
                        path_depth,
                    })?;
                    stack.push(FullTraversalFrameV1::Page {
                        id: child,
                        expected_depth: child_depth,
                        path_len,
                        path_depth,
                    })?;
                }
                FullTraversalFrameV1::File {
                    id,
                    path_len,
                    path_depth,
                } => {
                    self.restore_path(path_len, path_depth)?;
                    self.walk_file_full(id, hasher)?;
                }
                FullTraversalFrameV1::Symlink {
                    id,
                    path_len,
                    path_depth,
                } => {
                    self.restore_path(path_len, path_depth)?;
                    self.walk_symlink_full(id, hasher)?;
                }
            }
        }
        Ok(())
    }

    fn walk_file_full(
        &mut self,
        id: PhysicalFileIdV1,
        hasher: &mut blake3::Hasher,
    ) -> CoreResult<()> {
        let object = self.validate_object(TypedPhysicalObjectIdV1::File(id))?;
        let PhysicalObjectPayloadV1::File(file) = object.payload else {
            return Err(CoreError::TypedEdge);
        };
        self.files = self
            .files
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        digest_entry_prefix(
            hasher,
            0x11,
            &self.path[..self.path_len],
            file.mode,
            file.logical_len,
        );
        digest_stream_prefix(hasher, 0x31, file.logical_len);
        self.sink
            .begin_file(
                &self.path[..self.path_len],
                file.mode,
                file.logical_len,
                0,
                file.logical_len,
            )
            .map_err(map_sink)?;
        let result = self.stream_verified_file(
            id,
            object.len,
            file.extent_count,
            file.logical_len,
            0,
            file.logical_len,
            hasher,
        );
        result?;
        self.sink.finish_file().map_err(map_sink)
    }

    #[allow(clippy::too_many_arguments)]
    fn stream_verified_file(
        &mut self,
        id: PhysicalFileIdV1,
        object_len: u64,
        extent_count: u32,
        logical_len: u64,
        selected_start: u64,
        selected_end: u64,
        hasher: &mut blake3::Hasher,
    ) -> CoreResult<()> {
        let mut cursor = self.cursor(TypedPhysicalObjectIdV1::File(id), object_len);
        let _mode = cursor.read_u16(self.occupied, self.counters, self.control)?;
        if cursor.read_u64(self.occupied, self.counters, self.control)? != logical_len {
            return Err(CoreError::LogicalLength);
        }
        if cursor.read_u32(self.occupied, self.counters, self.control)? != extent_count {
            return Err(CoreError::CountCap);
        }
        let expected = selected_end
            .checked_sub(selected_start)
            .ok_or(CoreError::LogicalLength)?;
        let stream_result = {
            let mut port = FsCasVerifiedFileRangeV1 {
                occupied: self.occupied,
                counters: self.counters,
                control: self.control,
                cursor,
                file_logical_len: logical_len,
                selected_start,
                selected_end,
                extents_remaining: extent_count,
                logical_offset: 0,
                active_data_chunks: 0,
                active_data_end: 0,
                next_token: 1,
                current_data: None,
                validated_chunks: 0,
            };
            let mut consumer = ExtractionFileConsumerV1 {
                sink: self.sink,
                hasher,
            };
            let result =
                stream_verified_file_range_v1(expected, &mut port, &mut consumer, self.comparison)?;
            (result, port.validated_chunks)
        };
        self.payload_bytes = self
            .payload_bytes
            .checked_add(stream_result.0.logical_bytes)
            .ok_or(CoreError::IntegerOverflow)?;
        self.payload_direct_bytes = self
            .payload_direct_bytes
            .checked_add(stream_result.0.payload_direct_bytes)
            .ok_or(CoreError::IntegerOverflow)?;
        self.payload_direct_calls = self
            .payload_direct_calls
            .checked_add(stream_result.0.payload_direct_calls)
            .ok_or(CoreError::IntegerOverflow)?;
        self.objects_traversed = self
            .objects_traversed
            .checked_add(stream_result.1)
            .ok_or(CoreError::IntegerOverflow)?;
        self.counters
            .add(CounterFieldV1::BytesWritten, stream_result.0.logical_bytes)?;
        if selected_end > logical_len {
            return Err(CoreError::LogicalLength);
        }
        Ok(())
    }

    fn walk_symlink_full(
        &mut self,
        id: PhysicalSymlinkIdV1,
        hasher: &mut blake3::Hasher,
    ) -> CoreResult<()> {
        let object = self.validate_object(TypedPhysicalObjectIdV1::Symlink(id))?;
        let PhysicalObjectPayloadV1::Symlink(link) = object.payload else {
            return Err(CoreError::TypedEdge);
        };
        self.symlinks = self
            .symlinks
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        digest_entry_prefix(
            hasher,
            0x12,
            &self.path[..self.path_len],
            0,
            u64::from(link.target_len),
        );
        digest_stream_prefix(hasher, 0x32, u64::from(link.target_len));
        let len = usize::try_from(link.target_len).map_err(|_| CoreError::IntegerOverflow)?;
        read_occupied_exact_accounted_v1(
            self.occupied,
            self.counters,
            self.control,
            TypedPhysicalObjectIdV1::Symlink(id),
            crate::object::OBJECT_HEADER_BYTES + 4,
            &mut self.comparison[..len],
        )?;
        self.payload_direct_bytes = self
            .payload_direct_bytes
            .checked_add(len as u64)
            .ok_or(CoreError::IntegerOverflow)?;
        self.payload_direct_calls = self
            .payload_direct_calls
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        hasher.update(&self.comparison[..len]);
        Ok(())
    }

    fn read_exact_range(
        &mut self,
        root: PhysicalTreeIdV1,
        path: &[u8],
        offset: u64,
        end: u64,
        hasher: &mut blake3::Hasher,
    ) -> CoreResult<()> {
        let mut directory = root;
        let mut components = path.split(|byte| *byte == b'/').peekable();
        while let Some(component) = components.next() {
            let (kind, digest) = self.lookup_directory_component(directory, component)?;
            if components.peek().is_some() {
                if kind != PhysicalTreeChildKindV1::Tree {
                    return Err(CoreError::TypedEdge);
                }
                directory = PhysicalTreeIdV1::from_digest(digest);
                continue;
            }
            if kind != PhysicalTreeChildKindV1::File {
                return Err(CoreError::TypedEdge);
            }
            return self.stream_file_range(
                PhysicalFileIdV1::from_digest(digest),
                path,
                offset,
                end,
                hasher,
            );
        }
        Err(CoreError::Path)
    }

    fn lookup_directory_component(
        &mut self,
        directory: PhysicalTreeIdV1,
        component: &[u8],
    ) -> CoreResult<(PhysicalTreeChildKindV1, [u8; 32])> {
        let object = self.validate_object(TypedPhysicalObjectIdV1::Tree(directory))?;
        let PhysicalObjectPayloadV1::Tree(TreeRecordV1::Directory(directory)) = object.payload
        else {
            return Err(CoreError::TypedEdge);
        };
        let page = directory
            .root_page_id
            .ok_or(CoreError::MissingClosureEdge)?;
        self.lookup_page_component(page, directory.page_depth, component)
    }

    fn lookup_page_component(
        &mut self,
        mut page: PhysicalTreeIdV1,
        mut expected_depth: u8,
        component: &[u8],
    ) -> CoreResult<(PhysicalTreeChildKindV1, [u8; 32])> {
        for _ in 0..=MAX_TREE_PAGE_DEPTH {
            check_control(self.control)?;
            let object = self.validate_object(TypedPhysicalObjectIdV1::Tree(page))?;
            let mut cursor = self.cursor(TypedPhysicalObjectIdV1::Tree(page), object.len);
            match object.payload {
                PhysicalObjectPayloadV1::Tree(TreeRecordV1::Leaf(leaf)) => {
                    if expected_depth != 0
                        || cursor.read_u8(self.occupied, self.counters, self.control)? != 0x02
                        || cursor.read_u8(self.occupied, self.counters, self.control)? != leaf.depth
                        || cursor.read_u16(self.occupied, self.counters, self.control)?
                            != leaf.count
                    {
                        return Err(CoreError::TypeDomain);
                    }
                    for _ in 0..leaf.count {
                        let mut name = [0_u8; 255];
                        let name_len = cursor.read_component(
                            self.occupied,
                            self.counters,
                            self.control,
                            &mut name,
                        )?;
                        let kind = PhysicalTreeChildKindV1::try_from(cursor.read_u8(
                            self.occupied,
                            self.counters,
                            self.control,
                        )?)?;
                        let digest =
                            cursor.read_array::<32>(self.occupied, self.counters, self.control)?;
                        match name[..name_len].cmp(component) {
                            Ordering::Less => {}
                            Ordering::Equal => return Ok((kind, digest)),
                            Ordering::Greater => return Err(CoreError::MissingClosureEdge),
                        }
                    }
                    return Err(CoreError::MissingClosureEdge);
                }
                PhysicalObjectPayloadV1::Tree(TreeRecordV1::Index(index)) => {
                    if expected_depth == 0
                        || index.depth != expected_depth
                        || cursor.read_u8(self.occupied, self.counters, self.control)? != 0x03
                        || cursor.read_u8(self.occupied, self.counters, self.control)?
                            != index.depth
                        || cursor.read_u16(self.occupied, self.counters, self.control)?
                            != index.count
                    {
                        return Err(CoreError::TypeDomain);
                    }
                    let mut selected = None;
                    for _ in 0..index.count {
                        let _subtree =
                            cursor.read_u32(self.occupied, self.counters, self.control)?;
                        let mut first = [0_u8; 255];
                        let first_len = cursor.read_component(
                            self.occupied,
                            self.counters,
                            self.control,
                            &mut first,
                        )?;
                        let mut last = [0_u8; 255];
                        let last_len = cursor.read_component(
                            self.occupied,
                            self.counters,
                            self.control,
                            &mut last,
                        )?;
                        let child = PhysicalTreeIdV1::from_digest(cursor.read_array::<32>(
                            self.occupied,
                            self.counters,
                            self.control,
                        )?);
                        if component < &first[..first_len] {
                            return Err(CoreError::MissingClosureEdge);
                        }
                        if component <= &last[..last_len] {
                            selected = Some(child);
                            break;
                        }
                    }
                    page = selected.ok_or(CoreError::MissingClosureEdge)?;
                    expected_depth -= 1;
                }
                _ => return Err(CoreError::TypedEdge),
            }
        }
        Err(CoreError::CountCap)
    }

    fn stream_file_range(
        &mut self,
        id: PhysicalFileIdV1,
        path: &[u8],
        selected_start: u64,
        selected_end: u64,
        hasher: &mut blake3::Hasher,
    ) -> CoreResult<()> {
        let object = self.validate_object(TypedPhysicalObjectIdV1::File(id))?;
        let PhysicalObjectPayloadV1::File(file) = object.payload else {
            return Err(CoreError::TypedEdge);
        };
        if selected_end > file.logical_len {
            return Err(CoreError::LogicalLength);
        }
        let selected_len = selected_end - selected_start;
        digest_entry_prefix(hasher, 0x11, path, file.mode, file.logical_len);
        digest_stream_prefix(hasher, 0x33, selected_len);
        self.sink
            .begin_file(
                path,
                file.mode,
                file.logical_len,
                selected_start,
                selected_len,
            )
            .map_err(map_sink)?;
        self.stream_verified_file(
            id,
            object.len,
            file.extent_count,
            file.logical_len,
            selected_start,
            selected_end,
            hasher,
        )?;
        self.files = 1;
        self.sink.finish_file().map_err(map_sink)
    }

    fn push_component(&mut self, component: &[u8]) -> CoreResult<()> {
        ValidatedComponent::new(component)?;
        let next_depth = self
            .path_depth
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        if next_depth > MAX_PATH_DEPTH {
            return Err(CoreError::Path);
        }
        let separator = usize::from(self.path_len != 0);
        let next = self
            .path_len
            .checked_add(separator)
            .and_then(|value| value.checked_add(component.len()))
            .ok_or(CoreError::IntegerOverflow)?;
        if next > self.path.len() {
            return Err(CoreError::Path);
        }
        if separator != 0 {
            self.path[self.path_len] = b'/';
            self.path_len += 1;
        }
        self.path[self.path_len..next].copy_from_slice(component);
        self.path_len = next;
        self.path_depth = next_depth;
        Ok(())
    }

    fn restore_path(&mut self, path_len: usize, path_depth: usize) -> CoreResult<()> {
        if path_len > self.path.len() || path_depth > MAX_PATH_DEPTH {
            return Err(CoreError::Path);
        }
        self.path_len = path_len;
        self.path_depth = path_depth;
        Ok(())
    }
}

impl<S: ReadSinkV1 + ?Sized, C: FsCasControlV1 + ?Sized> ExactRangeExecutorV1
    for ReaderV1<'_, S, C>
{
    fn execute_exact_range_v1(
        &mut self,
        root: PhysicalTreeIdV1,
        plan: ExactRangePlanV1<'_>,
        hasher: &mut blake3::Hasher,
    ) -> CoreResult<()> {
        self.read_exact_range(root, plan.path(), plan.offset(), plan.end(), hasher)
    }
}

fn read_occupied_exact_accounted_v1<C>(
    occupied: &mut FsCasOccupiedV1,
    counters: &mut OperationCountersV1,
    control: &mut C,
    id: TypedPhysicalObjectIdV1,
    offset: u64,
    destination: &mut [u8],
) -> CoreResult<()>
where
    C: FsCasControlV1 + ?Sized,
{
    // Traversal frames intentionally retain only canonical IDs and byte
    // offsets. A descended child can evict its parent's resolved carrier from
    // the occupied reader's two-entry locality cache, so every resumed frame
    // must resolve its own ID again before reading. Treating the cache as an
    // implicit lifetime capability made otherwise valid depth-first reads
    // fail closed after visiting a child with more than one object.
    let occupied_len = match occupied.occupied_len_typed_controlled_v1(id, control) {
        Ok(Some(len)) => len,
        Ok(None) => {
            occupied.retain_first_error_typed_v1(FsCasErrorV1::MissingOccupant);
            return Err(CoreError::SourceFailure);
        }
        Err(error) => {
            occupied.retain_first_error_typed_v1(error);
            return Err(CoreError::SourceFailure);
        }
    };
    let end = offset
        .checked_add(destination.len() as u64)
        .ok_or(CoreError::IntegerOverflow)?;
    if end > occupied_len {
        return Err(CoreError::Truncated);
    }
    occupied
        .read_occupied_exact_at_typed_controlled_v1(id, offset, destination, control)
        .map_err(|error| {
            occupied.retain_first_error_typed_v1(error);
            CoreError::SourceFailure
        })?;
    counters.add(CounterFieldV1::BytesRead, destination.len() as u64)
}

struct ExtractionFileConsumerV1<'a, S: ReadSinkV1 + ?Sized> {
    sink: &'a mut S,
    hasher: &'a mut blake3::Hasher,
}

impl<S: ReadSinkV1 + ?Sized> VerifiedFileBytesConsumerV1 for ExtractionFileConsumerV1<'_, S> {
    fn write_verified_bytes(&mut self, bytes: &[u8]) -> CoreResult<()> {
        self.hasher.update(bytes);
        self.sink.write_file_bytes(bytes).map_err(map_sink)
    }
}

#[derive(Clone, Copy)]
struct CurrentVerifiedDataV1 {
    token: u64,
    id: PhysicalChunkIdV1,
    allowed_start: u64,
    allowed_end: u64,
}

/// FsCas-backed implementation of the narrow verified-file port. Concrete
/// namespace and locator behavior remains here in the extraction owner;
/// `content::read` sees only opaque, single-use data tokens.
struct FsCasVerifiedFileRangeV1<'a, C: FsCasControlV1 + ?Sized> {
    occupied: &'a mut FsCasOccupiedV1,
    counters: &'a mut OperationCountersV1,
    control: &'a mut C,
    cursor: ObjectCursorV1,
    file_logical_len: u64,
    selected_start: u64,
    selected_end: u64,
    extents_remaining: u32,
    logical_offset: u64,
    active_data_chunks: u32,
    active_data_end: u64,
    next_token: u64,
    current_data: Option<CurrentVerifiedDataV1>,
    validated_chunks: u64,
}

impl<C: FsCasControlV1 + ?Sized> FsCasVerifiedFileRangeV1<'_, C> {
    fn validate_chunk(
        &mut self,
        id: PhysicalChunkIdV1,
        expected_len: u32,
        scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    ) -> CoreResult<()> {
        let typed = TypedPhysicalObjectIdV1::Chunk(id);
        let len = match self
            .occupied
            .occupied_len_typed_controlled_v1(typed, self.control)
        {
            Ok(Some(len)) => len,
            Ok(None) => {
                self.occupied
                    .retain_first_error_typed_v1(FsCasErrorV1::MissingOccupant);
                return Err(CoreError::SourceFailure);
            }
            Err(error) => {
                self.occupied.retain_first_error_typed_v1(error);
                return Err(CoreError::SourceFailure);
            }
        };
        let mut source =
            OccupiedObjectReaderV1::new(self.occupied, self.counters, self.control, typed, len);
        let decoded =
            decode_physical_object_from_port_v1(&mut source, &mut DiscardStrongEdgesV1, scratch)?;
        let PhysicalObjectPayloadV1::Chunk(chunk) = decoded.payload() else {
            return Err(CoreError::TypedEdge);
        };
        if decoded.physical_id() != typed
            || decoded.header().kind() != typed.kind()
            || chunk.payload_len != expected_len
        {
            return Err(CoreError::IdMismatch);
        }
        self.validated_chunks = self
            .validated_chunks
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(())
    }

    fn finish_if_complete(&mut self) -> CoreResult<Option<VerifiedFileSegmentV1>> {
        if self.active_data_chunks != 0 || self.extents_remaining != 0 {
            return Err(CoreError::CountCap);
        }
        if self.logical_offset != self.file_logical_len {
            return Err(CoreError::LogicalLength);
        }
        self.cursor.finish()?;
        Ok(None)
    }
}

impl<C: FsCasControlV1 + ?Sized> VerifiedFileRangePortV1 for FsCasVerifiedFileRangeV1<'_, C> {
    fn check_control(&mut self) -> CoreResult<()> {
        check_control(self.control)
    }

    fn next_intersection(
        &mut self,
        verification_scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    ) -> CoreResult<Option<VerifiedFileSegmentV1>> {
        self.current_data = None;
        loop {
            self.check_control()?;
            if self.active_data_chunks != 0 {
                let chunk_len = self
                    .cursor
                    .read_u32(self.occupied, self.counters, self.control)?;
                let chunk = PhysicalChunkIdV1::from_digest(self.cursor.read_array::<32>(
                    self.occupied,
                    self.counters,
                    self.control,
                )?);
                self.active_data_chunks -= 1;
                let chunk_start = self.logical_offset;
                let chunk_end = chunk_start
                    .checked_add(u64::from(chunk_len))
                    .ok_or(CoreError::IntegerOverflow)?;
                if chunk_end > self.active_data_end
                    || (self.active_data_chunks == 0 && chunk_end != self.active_data_end)
                {
                    return Err(CoreError::LogicalLength);
                }
                self.logical_offset = chunk_end;
                if let Some((start, end)) = overlap(
                    chunk_start,
                    chunk_end,
                    self.selected_start,
                    self.selected_end,
                ) {
                    self.validate_chunk(chunk, chunk_len, verification_scratch)?;
                    let token = self.next_token;
                    self.next_token = self
                        .next_token
                        .checked_add(1)
                        .ok_or(CoreError::IntegerOverflow)?;
                    let source_start = start - chunk_start;
                    let source_end = end - chunk_start;
                    self.current_data = Some(CurrentVerifiedDataV1 {
                        token,
                        id: chunk,
                        allowed_start: source_start,
                        allowed_end: source_end,
                    });
                    return Ok(Some(VerifiedFileSegmentV1::data(
                        token,
                        source_start,
                        source_end - source_start,
                    )));
                }
                continue;
            }

            if self.extents_remaining == 0 {
                return self.finish_if_complete();
            }
            let tag = ExtentTagV1::try_from(self.cursor.read_u8(
                self.occupied,
                self.counters,
                self.control,
            )?)?;
            let extent_len = self
                .cursor
                .read_u64(self.occupied, self.counters, self.control)?;
            if extent_len == 0 {
                return Err(CoreError::LogicalLength);
            }
            self.extents_remaining -= 1;
            let extent_start = self.logical_offset;
            let extent_end = extent_start
                .checked_add(extent_len)
                .ok_or(CoreError::IntegerOverflow)?;
            if extent_end > self.file_logical_len {
                return Err(CoreError::LogicalLength);
            }
            match tag {
                ExtentTagV1::Hole => {
                    self.logical_offset = extent_end;
                    if let Some((start, end)) = overlap(
                        extent_start,
                        extent_end,
                        self.selected_start,
                        self.selected_end,
                    ) {
                        return Ok(Some(VerifiedFileSegmentV1::hole(end - start)));
                    }
                }
                ExtentTagV1::Data => {
                    let count = self
                        .cursor
                        .read_u32(self.occupied, self.counters, self.control)?;
                    if count == 0 {
                        return Err(CoreError::CountCap);
                    }
                    self.active_data_chunks = count;
                    self.active_data_end = extent_end;
                }
            }
        }
    }

    fn read_data_exact(
        &mut self,
        token: u64,
        source_offset: u64,
        destination: &mut [u8],
    ) -> CoreResult<()> {
        let current = self.current_data.ok_or(CoreError::TypedEdge)?;
        let end = source_offset
            .checked_add(destination.len() as u64)
            .ok_or(CoreError::IntegerOverflow)?;
        if token != current.token
            || source_offset < current.allowed_start
            || end > current.allowed_end
        {
            return Err(CoreError::TypedEdge);
        }
        read_occupied_exact_accounted_v1(
            self.occupied,
            self.counters,
            self.control,
            TypedPhysicalObjectIdV1::Chunk(current.id),
            crate::object::OBJECT_HEADER_BYTES
                .checked_add(source_offset)
                .ok_or(CoreError::IntegerOverflow)?,
            destination,
        )
    }
}

#[derive(Clone, Copy)]
struct ObjectCursorV1 {
    id: TypedPhysicalObjectIdV1,
    offset: u64,
    end: u64,
}

impl ObjectCursorV1 {
    fn read_array<const N: usize>(
        &mut self,
        occupied: &mut FsCasOccupiedV1,
        counters: &mut OperationCountersV1,
        control: &mut (impl FsCasControlV1 + ?Sized),
    ) -> CoreResult<[u8; N]> {
        let next = self
            .offset
            .checked_add(N as u64)
            .ok_or(CoreError::IntegerOverflow)?;
        if next > self.end {
            return Err(CoreError::Truncated);
        }
        let mut bytes = [0_u8; N];
        read_occupied_exact_accounted_v1(
            occupied,
            counters,
            control,
            self.id,
            self.offset,
            &mut bytes,
        )?;
        self.offset = next;
        Ok(bytes)
    }

    fn read_u8(
        &mut self,
        occupied: &mut FsCasOccupiedV1,
        counters: &mut OperationCountersV1,
        control: &mut (impl FsCasControlV1 + ?Sized),
    ) -> CoreResult<u8> {
        Ok(self.read_array::<1>(occupied, counters, control)?[0])
    }

    fn read_u16(
        &mut self,
        occupied: &mut FsCasOccupiedV1,
        counters: &mut OperationCountersV1,
        control: &mut (impl FsCasControlV1 + ?Sized),
    ) -> CoreResult<u16> {
        Ok(u16::from_be_bytes(
            self.read_array::<2>(occupied, counters, control)?,
        ))
    }

    fn read_u32(
        &mut self,
        occupied: &mut FsCasOccupiedV1,
        counters: &mut OperationCountersV1,
        control: &mut (impl FsCasControlV1 + ?Sized),
    ) -> CoreResult<u32> {
        Ok(u32::from_be_bytes(
            self.read_array::<4>(occupied, counters, control)?,
        ))
    }

    fn read_u64(
        &mut self,
        occupied: &mut FsCasOccupiedV1,
        counters: &mut OperationCountersV1,
        control: &mut (impl FsCasControlV1 + ?Sized),
    ) -> CoreResult<u64> {
        Ok(u64::from_be_bytes(
            self.read_array::<8>(occupied, counters, control)?,
        ))
    }

    fn read_component(
        &mut self,
        occupied: &mut FsCasOccupiedV1,
        counters: &mut OperationCountersV1,
        control: &mut (impl FsCasControlV1 + ?Sized),
        destination: &mut [u8; 255],
    ) -> CoreResult<usize> {
        let len = usize::from(self.read_u16(occupied, counters, control)?);
        if len == 0 || len > destination.len() {
            return Err(CoreError::Name);
        }
        let next = self
            .offset
            .checked_add(len as u64)
            .ok_or(CoreError::IntegerOverflow)?;
        if next > self.end {
            return Err(CoreError::Truncated);
        }
        read_occupied_exact_accounted_v1(
            occupied,
            counters,
            control,
            self.id,
            self.offset,
            &mut destination[..len],
        )?;
        self.offset = next;
        Ok(len)
    }

    fn skip_component(
        &mut self,
        occupied: &mut FsCasOccupiedV1,
        counters: &mut OperationCountersV1,
        control: &mut (impl FsCasControlV1 + ?Sized),
    ) -> CoreResult<()> {
        let mut bytes = [0_u8; 255];
        self.read_component(occupied, counters, control, &mut bytes)
            .map(|_| ())
    }

    fn finish(self) -> CoreResult<()> {
        if self.offset == self.end {
            Ok(())
        } else {
            Err(CoreError::TrailingBytes)
        }
    }
}

fn begin_digest(
    domain: &[u8; 8],
    version: PhysicalVersionRecordIdV1,
    root: PhysicalTreeIdV1,
) -> blake3::Hasher {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    digest_frame(&mut hasher, 0x01, version.as_bytes());
    digest_frame(&mut hasher, 0x02, root.as_bytes());
    hasher
}

fn digest_entry_prefix(
    hasher: &mut blake3::Hasher,
    tag: u8,
    path: &[u8],
    mode: u16,
    logical_len: u64,
) {
    digest_frame(hasher, tag, path);
    digest_frame(hasher, 0x41, &mode.to_be_bytes());
    digest_frame(hasher, 0x42, &logical_len.to_be_bytes());
}

fn digest_stream_prefix(hasher: &mut blake3::Hasher, tag: u8, len: u64) {
    hasher.update(&[tag]);
    hasher.update(&len.to_be_bytes());
}

fn digest_frame(hasher: &mut blake3::Hasher, tag: u8, bytes: &[u8]) {
    hasher.update(&[tag]);
    hasher.update(&(bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn finish_digest(hasher: blake3::Hasher) -> [u8; 32] {
    *hasher.finalize().as_bytes()
}

fn overlap(a_start: u64, a_end: u64, b_start: u64, b_end: u64) -> Option<(u64, u64)> {
    let start = a_start.max(b_start);
    let end = a_end.min(b_end);
    (start < end).then_some((start, end))
}

fn check_control<C: FsCasControlV1 + ?Sized>(control: &mut C) -> CoreResult<()> {
    if control.cancellation_requested() {
        Err(CoreError::Cancelled)
    } else if control.deadline_exceeded() {
        Err(CoreError::Deadline)
    } else {
        Ok(())
    }
}

fn map_immutable(ImmutablePortErrorV1::Failure: ImmutablePortErrorV1) -> CoreError {
    CoreError::SourceFailure
}

fn map_sink(ReadSinkErrorV1::Refused: ReadSinkErrorV1) -> CoreError {
    CoreError::SinkRefused
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use crate::cdc::{CdcAlgorithmV1, CdcControlV1, MAXIMUM_CHUNK_BYTES};
    use crate::content::{
        request_tree_operation_v1, run_create_tree_v1, ContentSourceErrorV1, ContentSourceV1,
        OperationBuffersV1, SourceSupplierV1, TreeFileV1,
    };
    use crate::cow::{TreePageSummaryV1, MAX_TREE_OBJECT_BYTES, MAX_TREE_PAGE_SUMMARIES};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(1);

    struct TestRoot(PathBuf);

    impl TestRoot {
        fn new(label: &str) -> Self {
            let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
            let parent = fs::canonicalize(std::env::temp_dir()).expect("canonical temp directory");
            Self(parent.join(format!(
                "layerfs-private-read-{label}-{}-{sequence:016x}",
                std::process::id()
            )))
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    struct SliceSource<'a> {
        bytes: &'a [u8],
        offset: usize,
        maximum_read: usize,
    }

    impl ContentSourceV1 for SliceSource<'_> {
        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            Ok(core::mem::size_of::<Self>() as u64)
        }

        fn read(&mut self, destination: &mut [u8]) -> Result<usize, ContentSourceErrorV1> {
            let take = destination
                .len()
                .min(self.maximum_read)
                .min(self.bytes.len() - self.offset);
            destination[..take].copy_from_slice(&self.bytes[self.offset..self.offset + take]);
            self.offset += take;
            Ok(take)
        }
    }

    struct SliceSupplier<'a> {
        bytes: &'a [u8],
        maximum_read: usize,
        cas: &'a FsCasV1,
    }

    impl<'a> SourceSupplierV1 for SliceSupplier<'a> {
        type Source = SliceSource<'a>;

        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            Ok(core::mem::size_of::<SliceSource<'_>>() as u64)
        }

        fn supply(self) -> CoreResult<Self::Source> {
            assert_eq!(self.cas.operation_admitted_slots_v1(), 1);
            Ok(SliceSource {
                bytes: self.bytes,
                offset: 0,
                maximum_read: self.maximum_read,
            })
        }
    }

    #[derive(Default)]
    struct ContinueControl;

    impl CdcControlV1 for ContinueControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for ContinueControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    #[derive(Default)]
    struct PanicDuringReadTerminalInvalidation {
        panicked: bool,
    }

    impl FsCasControlV1 for PanicDuringReadTerminalInvalidation {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if !self.panicked && target == FsCasCleanupTargetV1::RootInvalidation {
                self.panicked = true;
                panic!("injected read-terminal invalidation unwind")
            }
            false
        }
    }

    struct PanicAfterReadTerminalRelease {
        terminal_unwind_pending: bool,
        observation_failure: Option<CoreError>,
        terminal_hook_calls: u64,
        observation_hook_calls: u64,
    }

    impl FsCasControlV1 for PanicAfterReadTerminalRelease {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_operation_terminal_unwind_after_release(&mut self) -> bool {
            self.terminal_hook_calls += 1;
            core::mem::take(&mut self.terminal_unwind_pending)
        }

        fn inject_root_lock_observation_failure(&mut self) -> Option<CoreError> {
            self.observation_hook_calls += 1;
            self.observation_failure.take()
        }
    }

    #[derive(Clone, Copy)]
    enum StopKind {
        Cancel,
        Deadline,
    }

    struct StopControl {
        kind: StopKind,
        polls: u64,
        stop_at: u64,
    }

    impl FsCasControlV1 for StopControl {
        fn cancellation_requested(&mut self) -> bool {
            self.polls += 1;
            matches!(self.kind, StopKind::Cancel) && self.polls >= self.stop_at
        }

        fn deadline_exceeded(&mut self) -> bool {
            matches!(self.kind, StopKind::Deadline) && self.polls >= self.stop_at
        }
    }

    struct StopAfterBeginControl {
        kind: StopKind,
        stopped: Rc<Cell<bool>>,
    }

    impl FsCasControlV1 for StopAfterBeginControl {
        fn cancellation_requested(&mut self) -> bool {
            matches!(self.kind, StopKind::Cancel) && self.stopped.get()
        }

        fn deadline_exceeded(&mut self) -> bool {
            matches!(self.kind, StopKind::Deadline) && self.stopped.get()
        }
    }

    struct FilesystemFaultAfterBeginControl {
        activated: Rc<Cell<bool>>,
        boundary: FsCasFilesystemBoundaryV1,
        fault: FsCasErrorV1,
        injected: bool,
    }

    struct FilesystemFaultControl {
        boundary: FsCasFilesystemBoundaryV1,
        fault: FsCasErrorV1,
        injected: bool,
    }

    impl FsCasControlV1 for FilesystemFaultControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_filesystem_failure(
            &mut self,
            boundary: FsCasFilesystemBoundaryV1,
        ) -> Option<FsCasErrorV1> {
            if !self.injected && boundary == self.boundary {
                self.injected = true;
                return Some(self.fault);
            }
            None
        }
    }

    impl FsCasControlV1 for FilesystemFaultAfterBeginControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_filesystem_failure(
            &mut self,
            boundary: FsCasFilesystemBoundaryV1,
        ) -> Option<FsCasErrorV1> {
            if self.activated.get() && !self.injected && boundary == self.boundary {
                self.injected = true;
                return Some(self.fault);
            }
            None
        }
    }

    struct FilesystemFaultAfterBeginInvalidationControl {
        activated: Rc<Cell<bool>>,
        boundary: FsCasFilesystemBoundaryV1,
        fault: FsCasErrorV1,
        injected: bool,
        invalidation_attempts: u64,
    }

    impl FsCasControlV1 for FilesystemFaultAfterBeginInvalidationControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::RootInvalidation {
                self.invalidation_attempts += 1;
                return true;
            }
            false
        }

        fn inject_filesystem_failure(
            &mut self,
            boundary: FsCasFilesystemBoundaryV1,
        ) -> Option<FsCasErrorV1> {
            if self.activated.get() && !self.injected && boundary == self.boundary {
                self.injected = true;
                return Some(self.fault);
            }
            None
        }
    }

    struct FilesystemFaultBeforeSinkInvalidationControl {
        cas: FsCasV1,
        boundary: FsCasFilesystemBoundaryV1,
        fault: FsCasErrorV1,
        injected: bool,
        invalidation_attempts: u64,
    }

    impl FsCasControlV1 for FilesystemFaultBeforeSinkInvalidationControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::RootInvalidation {
                self.invalidation_attempts += 1;
                return true;
            }
            false
        }

        fn inject_filesystem_failure(
            &mut self,
            boundary: FsCasFilesystemBoundaryV1,
        ) -> Option<FsCasErrorV1> {
            if !self.injected && boundary == self.boundary {
                self.injected = true;
                self.cas.poison_storage_admission_for_test_v1();
                return Some(self.fault);
            }
            None
        }
    }

    struct StopAtVisibilityContention {
        kind: StopKind,
        current: Option<FsCasBoundaryV1>,
    }

    impl FsCasControlV1 for StopAtVisibilityContention {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            self.current = Some(boundary);
        }

        fn cancellation_requested(&mut self) -> bool {
            matches!(self.kind, StopKind::Cancel)
                && self.current == Some(FsCasBoundaryV1::VisibilityLockContended)
        }

        fn deadline_exceeded(&mut self) -> bool {
            matches!(self.kind, StopKind::Deadline)
                && self.current == Some(FsCasBoundaryV1::VisibilityLockContended)
        }
    }

    #[derive(Debug, Eq, PartialEq)]
    struct CapturedFile {
        path: Vec<u8>,
        mode: u16,
        logical_len: u64,
        selected_offset: u64,
        selected_len: u64,
        bytes: Vec<u8>,
    }

    struct CaptureSink {
        declared_bound: u64,
        bound_calls: Cell<u64>,
        panic_on_bound: bool,
        panic_on_write: bool,
        panic_on_abort: bool,
        remove_objects_on_begin: Option<PathBuf>,
        stop_after_begin: Option<Rc<Cell<bool>>>,
        poison_storage_on_finish: Option<FsCasV1>,
        poison_storage_on_abort: Option<FsCasV1>,
        began: u64,
        finished: u64,
        aborted: u64,
        files: Vec<CapturedFile>,
        current: Option<CapturedFile>,
    }

    impl CaptureSink {
        fn new(declared_bound: u64) -> Self {
            Self {
                declared_bound,
                bound_calls: Cell::new(0),
                panic_on_bound: false,
                panic_on_write: false,
                panic_on_abort: false,
                remove_objects_on_begin: None,
                stop_after_begin: None,
                poison_storage_on_finish: None,
                poison_storage_on_abort: None,
                began: 0,
                finished: 0,
                aborted: 0,
                files: Vec::new(),
                current: None,
            }
        }
    }

    impl ReadSinkV1 for CaptureSink {
        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            self.bound_calls.set(self.bound_calls.get() + 1);
            assert!(!self.panic_on_bound, "injected sink-bound unwind");
            Ok(self.declared_bound)
        }

        fn begin_read(&mut self, _kind: ReadKindV1) -> Result<(), ReadSinkErrorV1> {
            self.began += 1;
            if let Some(objects) = self.remove_objects_on_begin.take() {
                for entry in fs::read_dir(objects).expect("read test object locator directory") {
                    fs::remove_file(entry.expect("test object locator entry").path())
                        .expect("remove test object locator after read begin");
                }
            }
            if let Some(stopped) = self.stop_after_begin.take() {
                stopped.set(true);
            }
            Ok(())
        }

        fn begin_file(
            &mut self,
            path: &[u8],
            mode: u16,
            logical_len: u64,
            selected_offset: u64,
            selected_len: u64,
        ) -> Result<(), ReadSinkErrorV1> {
            if self.current.is_some() {
                return Err(ReadSinkErrorV1::Refused);
            }
            self.current = Some(CapturedFile {
                path: path.to_vec(),
                mode,
                logical_len,
                selected_offset,
                selected_len,
                bytes: Vec::with_capacity(selected_len as usize),
            });
            Ok(())
        }

        fn write_file_bytes(&mut self, bytes: &[u8]) -> Result<(), ReadSinkErrorV1> {
            if self.panic_on_write {
                self.panic_on_write = false;
                panic!("injected post-begin sink write unwind");
            }
            self.current
                .as_mut()
                .ok_or(ReadSinkErrorV1::Refused)?
                .bytes
                .extend_from_slice(bytes);
            Ok(())
        }

        fn finish_file(&mut self) -> Result<(), ReadSinkErrorV1> {
            let file = self.current.take().ok_or(ReadSinkErrorV1::Refused)?;
            if file.bytes.len() as u64 != file.selected_len {
                return Err(ReadSinkErrorV1::Refused);
            }
            self.files.push(file);
            Ok(())
        }

        fn finish_read(&mut self, _digest: [u8; 32]) -> Result<(), ReadSinkErrorV1> {
            if self.current.is_some() {
                return Err(ReadSinkErrorV1::Refused);
            }
            if let Some(cas) = self.poison_storage_on_finish.take() {
                cas.poison_storage_admission_for_test_v1();
            }
            self.finished += 1;
            Ok(())
        }

        fn abort_read(&mut self) {
            self.aborted += 1;
            self.current = None;
            if let Some(cas) = self.poison_storage_on_abort.take() {
                cas.poison_storage_admission_for_test_v1();
            }
            if self.panic_on_abort {
                self.panic_on_abort = false;
                panic!("injected sink abort unwind");
            }
        }
    }

    struct CreatedFixture {
        _root: TestRoot,
        cas: FsCasV1,
        version: PhysicalVersionRecordIdV1,
        root_tree: PhysicalTreeIdV1,
        expected: Vec<(Vec<u8>, u16, Vec<u8>)>,
    }

    fn deterministic_bytes(len: usize, mut state: u64) -> Vec<u8> {
        let mut bytes = vec![0_u8; len];
        for destination in bytes.chunks_mut(8) {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            destination.copy_from_slice(&state.to_le_bytes()[..destination.len()]);
        }
        bytes
    }

    fn create_fixture_from_expected(
        label: &str,
        expected: Vec<(Vec<u8>, u16, Vec<u8>)>,
    ) -> CreatedFixture {
        let root = TestRoot::new(label);
        let cas = FsCasV1::create_new(root.path()).expect("create FsCas");
        let mut files = expected
            .iter()
            .map(|entry| {
                TreeFileV1::new(
                    &entry.0,
                    entry.1,
                    entry.2.len() as u64,
                    SliceSupplier {
                        bytes: &entry.2,
                        maximum_read: 997,
                        cas: &cas,
                    },
                )
            })
            .collect::<Vec<_>>();
        let mut counters = OperationCountersV1::default();
        let mut source: Box<[u8; MAXIMUM_CHUNK_BYTES]> = vec![0; MAXIMUM_CHUNK_BYTES]
            .into_boxed_slice()
            .try_into()
            .expect("source array");
        let mut ring: Box<[u8; MAXIMUM_CHUNK_BYTES]> = vec![0; MAXIMUM_CHUNK_BYTES]
            .into_boxed_slice()
            .try_into()
            .expect("ring array");
        let mut incoming: Box<[u8; COMPARISON_WINDOW_BYTES]> = vec![0; COMPARISON_WINDOW_BYTES]
            .into_boxed_slice()
            .try_into()
            .expect("comparison array");
        let mut occupied: Box<[u8; COMPARISON_WINDOW_BYTES]> = vec![0; COMPARISON_WINDOW_BYTES]
            .into_boxed_slice()
            .try_into()
            .expect("comparison array");
        let mut tree_object: Box<[u8; MAX_TREE_OBJECT_BYTES]> = vec![0; MAX_TREE_OBJECT_BYTES]
            .into_boxed_slice()
            .try_into()
            .expect("tree object array");
        let mut tree_pages = [None::<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES];
        let mut traversal = vec![0_u8; 64 * 1024];
        let mut create_control = ContinueControl;
        let operation =
            request_tree_operation_v1(&cas, 5, &mut counters, &mut create_control).unwrap();
        let handoff = run_create_tree_v1(
            operation,
            CdcAlgorithmV1::FastCdc,
            &mut files,
            OperationBuffersV1 {
                source: &mut source,
                cdc_ring: &mut ring,
                incoming_comparison: &mut incoming,
                occupied_comparison: &mut occupied,
                tree_object: &mut tree_object,
                tree_pages: &mut tree_pages,
                traversal_state: &mut traversal,
            },
            &mut create_control,
            &mut counters,
        )
        .unwrap_or_else(|error| panic!("{error:?}; {counters:#?}"));
        assert_eq!(cas.operation_admitted_slots_v1(), 0);
        drop(files);
        CreatedFixture {
            _root: root,
            cas,
            version: handoff.version_record(),
            root_tree: handoff.root_tree(),
            expected,
        }
    }

    fn create_fixture(label: &str) -> CreatedFixture {
        create_fixture_from_expected(
            label,
            vec![
                (
                    b"a.bin".to_vec(),
                    0o640,
                    deterministic_bytes(96 * 1024 + 31, 0x93ac_7ee1_4df0_2219),
                ),
                (
                    b"d/b.bin".to_vec(),
                    0o644,
                    deterministic_bytes(33 * 1024 + 7, 0xa114_5cc9_732e_f011),
                ),
                (b"d/e/c.bin".to_vec(), 0o600, Vec::new()),
            ],
        )
    }

    fn read_buffers() -> (
        Box<[u8; COMPARISON_WINDOW_BYTES]>,
        Box<[u8; MAX_PATH_BYTES]>,
    ) {
        (
            vec![0; COMPARISON_WINDOW_BYTES]
                .into_boxed_slice()
                .try_into()
                .expect("comparison array"),
            vec![0; MAX_PATH_BYTES]
                .into_boxed_slice()
                .try_into()
                .expect("path array"),
        )
    }

    fn preparation_entry_count(fixture: &CreatedFixture) -> usize {
        fs::read_dir(fixture._root.path().join("preparation"))
            .expect("read preparation directory")
            .count()
    }

    fn slash_join(component_lengths: &[usize]) -> Vec<u8> {
        let mut path = Vec::new();
        for (index, len) in component_lengths.iter().copied().enumerate() {
            if index != 0 {
                path.push(b'/');
            }
            path.extend(core::iter::repeat_n(b'a', len));
        }
        path
    }

    fn assert_single_path_extracts(path: Vec<u8>, label: &str) {
        let fixture = create_fixture_from_expected(label, vec![(path.clone(), 0o640, vec![0x5a])]);
        let (mut comparison, mut path_buffer) = read_buffers();
        let mut counters = OperationCountersV1::default();
        let mut sink = CaptureSink::new((path.len() + 1) as u64);
        let result = extract_root_v1(
            &fixture.cas,
            101,
            fixture.version,
            fixture.root_tree,
            &mut sink,
            &mut counters,
            ReadBuffersV1 {
                comparison: &mut comparison,
                path: &mut path_buffer,
            },
            &mut ContinueControl,
        )
        .expect("boundary extraction");
        assert_eq!(fixture.cas.operation_admitted_slots_v1(), 0);
        assert_eq!(result.files(), 1);
        assert_eq!(sink.files.len(), 1);
        assert_eq!(sink.files[0].path, path);
        assert_eq!(sink.files[0].bytes, [0x5a]);
    }

    struct CountSink {
        files: u64,
        previous_path: Vec<u8>,
        began: bool,
        finished: bool,
    }

    impl CountSink {
        fn new() -> Self {
            Self {
                files: 0,
                previous_path: Vec::with_capacity(MAX_PATH_BYTES),
                began: false,
                finished: false,
            }
        }
    }

    impl ReadSinkV1 for CountSink {
        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            Ok((core::mem::size_of::<Self>() + self.previous_path.capacity()) as u64)
        }

        fn begin_read(&mut self, _kind: ReadKindV1) -> Result<(), ReadSinkErrorV1> {
            self.began = true;
            Ok(())
        }

        fn begin_file(
            &mut self,
            path: &[u8],
            _mode: u16,
            _logical_len: u64,
            _selected_offset: u64,
            _selected_len: u64,
        ) -> Result<(), ReadSinkErrorV1> {
            if !self.previous_path.is_empty() && self.previous_path.as_slice() >= path {
                return Err(ReadSinkErrorV1::Refused);
            }
            self.previous_path.clear();
            self.previous_path.extend_from_slice(path);
            self.files += 1;
            Ok(())
        }

        fn write_file_bytes(&mut self, bytes: &[u8]) -> Result<(), ReadSinkErrorV1> {
            if bytes.is_empty() {
                Ok(())
            } else {
                Err(ReadSinkErrorV1::Refused)
            }
        }

        fn finish_file(&mut self) -> Result<(), ReadSinkErrorV1> {
            Ok(())
        }

        fn finish_read(&mut self, _digest: [u8; 32]) -> Result<(), ReadSinkErrorV1> {
            self.finished = true;
            Ok(())
        }

        fn abort_read(&mut self) {
            self.finished = false;
        }
    }

    #[test]
    fn real_fscas_full_and_exact_range_stream_from_a_reopened_handle() {
        let fixture = create_fixture("success");
        let reopened = FsCasV1::open_existing(fixture._root.path()).expect("reopen FsCas");
        let (mut comparison, mut path) = read_buffers();
        let mut counters = OperationCountersV1::default();
        let expected_bytes = fixture
            .expected
            .iter()
            .map(|entry| entry.2.len() as u64)
            .sum();
        let mut sink = CaptureSink::new(expected_bytes + 4 * 1024);
        let result = extract_root_v1(
            &reopened,
            102,
            fixture.version,
            fixture.root_tree,
            &mut sink,
            &mut counters,
            ReadBuffersV1 {
                comparison: &mut comparison,
                path: &mut path,
            },
            &mut ContinueControl,
        )
        .expect("full extraction");
        assert_eq!(reopened.operation_admitted_slots_v1(), 0);
        assert_eq!(result.kind(), ReadKindV1::FullExtraction);
        assert_eq!(result.files(), 3);
        assert_eq!(result.direct_fscas_bytes(), counters.fscas_bytes_read);
        assert_eq!(result.direct_fscas_calls(), counters.fscas_read_calls);
        assert_eq!(result.payload_bytes(), expected_bytes);
        assert_eq!(sink.began, 1);
        assert_eq!(sink.finished, 1);
        assert_eq!(sink.aborted, 0);
        for (captured, expected) in sink.files.iter().zip(&fixture.expected) {
            assert_eq!(&captured.path, &expected.0);
            assert_eq!(captured.mode, expected.1);
            assert_eq!(&captured.bytes, &expected.2);
            assert_eq!(captured.selected_offset, 0);
            assert_eq!(captured.selected_len, expected.2.len() as u64);
        }

        let selected_offset = 817_u64;
        let selected_len = 17_777_u64;
        let (mut comparison, mut path) = read_buffers();
        let mut counters = OperationCountersV1::default();
        let mut sink = CaptureSink::new(selected_len + 1024);
        let result = read_file_range_v1(
            &fixture.cas,
            103,
            fixture.version,
            fixture.root_tree,
            b"d/b.bin",
            selected_offset,
            selected_len,
            &mut sink,
            &mut counters,
            ReadBuffersV1 {
                comparison: &mut comparison,
                path: &mut path,
            },
            &mut ContinueControl,
        )
        .expect("exact range");
        assert_eq!(fixture.cas.operation_admitted_slots_v1(), 0);
        assert_eq!(result.kind(), ReadKindV1::ExactRange);
        assert_eq!(result.ranges(), 1);
        assert_eq!(result.payload_bytes(), selected_len);
        assert_eq!(result.direct_fscas_bytes(), counters.fscas_bytes_read);
        assert_eq!(result.direct_fscas_calls(), counters.fscas_read_calls);
        assert_eq!(sink.files.len(), 1);
        assert_eq!(sink.files[0].path, b"d/b.bin");
        assert_eq!(sink.files[0].selected_offset, selected_offset);
        assert_eq!(sink.files[0].selected_len, selected_len);
        assert_eq!(
            sink.files[0].bytes,
            fixture.expected[1].2
                [selected_offset as usize..(selected_offset + selected_len) as usize]
        );
    }

    #[test]
    fn full_read_occupied_observation_overflow_is_exact_and_transactional() {
        const SEEDED_BYTES: u64 = 79;
        let metadata_bytes = u64::try_from(PERSISTENT_LOCATOR_BYTES_V1).unwrap()
            + u64::try_from(CATALOG_MARKER_BYTES).unwrap();
        for (
            case_index,
            case,
            occupied_seeded_calls,
            payload_seeded_calls,
            expected_bytes,
            expected_calls,
        ) in [
            (
                0_u64,
                "metadata",
                Some(u64::MAX - 1),
                None,
                CLOSURE_MARKER_BYTES + SEEDED_BYTES,
                u64::MAX,
            ),
            (
                1_u64,
                "pack",
                Some(u64::MAX - 3),
                None,
                CLOSURE_MARKER_BYTES + SEEDED_BYTES + metadata_bytes,
                u64::MAX,
            ),
            (
                2_u64,
                "payload",
                None,
                Some(u64::MAX),
                CLOSURE_MARKER_BYTES,
                1,
            ),
        ] {
            let fixture = create_fixture(&format!(
                "occupied-{case}-observation-overflow-{case_index}"
            ));
            let stale = FsCasV1::open_existing(fixture._root.path()).expect("open stale handle");
            let preparation_before = preparation_entry_count(&fixture);
            let (mut comparison, mut path) = read_buffers();
            let mut counters = OperationCountersV1::default();
            let mut sink = CaptureSink::new(256 * 1024);
            if let Some(seeded_calls) = occupied_seeded_calls {
                fixture
                    .cas
                    .seed_next_occupied_read_observation_for_test_v1(SEEDED_BYTES, seeded_calls);
            }
            if let Some(seeded_calls) = payload_seeded_calls {
                fixture
                    .cas
                    .seed_next_occupied_payload_read_observation_for_test_v1(
                        SEEDED_BYTES,
                        seeded_calls,
                    );
            }

            assert_eq!(
                extract_root_v1(
                    &fixture.cas,
                    0x3_900 + case_index,
                    fixture.version,
                    fixture.root_tree,
                    &mut sink,
                    &mut counters,
                    ReadBuffersV1 {
                        comparison: &mut comparison,
                        path: &mut path,
                    },
                    &mut ContinueControl,
                ),
                Err(ReadOperationErrorV1::FsCas(FsCasErrorV1::Core(
                    CoreError::IntegerOverflow
                ))),
                "{case}"
            );

            // The closure read completes first. The first two rows reject the
            // metadata and pack observation pairs independently. The payload
            // row completes a real file read, rejects its bytes+call pair, and
            // then rejects the saturated occupied tuple as a whole when the
            // operation attempts to merge it with the closure observation.
            assert_eq!(counters.fscas_bytes_read, expected_bytes, "{case}");
            assert_eq!(counters.fscas_read_calls, expected_calls, "{case}");
            assert_eq!(sink.began, 0, "{case}");
            assert_eq!(sink.finished, 0, "{case}");
            assert_eq!(sink.aborted, 0, "{case}");
            assert_eq!(fixture.cas.operation_admitted_slots_v1(), 0, "{case}");
            assert_eq!(
                fixture.cas.operation_admission_active_for_test_v1(),
                0,
                "{case}"
            );
            assert_eq!(
                fixture.cas.operation_admission_queue_for_test_v1(),
                (0, 0, 0),
                "{case}"
            );
            assert_eq!(
                fixture.cas.storage_admission_active_for_test_v1(),
                (0, 0, 0),
                "{case}"
            );
            assert_eq!(
                preparation_entry_count(&fixture),
                preparation_before,
                "{case}"
            );
            assert_eq!(counters.storage_bytes_requested, 0, "{case}");
            assert_eq!(
                counters.storage_bytes_requested, counters.storage_bytes_reserved,
                "{case}"
            );
            assert_eq!(
                counters.storage_bytes_reserved,
                counters.storage_bytes_released
                    + counters.storage_bytes_committed
                    + counters.storage_bytes_retained,
                "{case}"
            );
            assert_eq!(counters.storage_inodes_requested, 0, "{case}");
            assert_eq!(
                counters.storage_inodes_requested, counters.storage_inodes_reserved,
                "{case}"
            );
            assert_eq!(
                counters.storage_inodes_reserved,
                counters.storage_inodes_released
                    + counters.storage_inodes_committed
                    + counters.storage_inodes_retained,
                "{case}"
            );
            assert_eq!(
                counters.storage_preparation_bytes_current_after_cleanup, 0,
                "{case}"
            );
            assert_eq!(
                counters.storage_preparation_inodes_current_after_cleanup, 0,
                "{case}"
            );
            assert_eq!(counters.mutable_preparation_residue_bytes, 0, "{case}");
            assert_eq!(counters.mutable_preparation_residue_inodes, 0, "{case}");
            assert_eq!(counters.immutable_residue_bytes, 0, "{case}");
            assert_eq!(counters.immutable_residue_inodes, 0, "{case}");
            assert!(counters.has_zero_forbidden_work(), "{case}");

            // Direct-observation arithmetic exhaustion is not storage damage.
            assert!(fixture.cas.occupied_private_v1().is_ok(), "{case}");
            assert!(stale.occupied_private_v1().is_ok(), "{case}");
            assert!(
                FsCasV1::open_existing(fixture._root.path()).is_ok(),
                "{case}"
            );
        }
    }

    #[test]
    fn root_read_admission_precedes_request_sink_and_storage_work_and_releases_on_all_prework_stops(
    ) {
        let fixture = create_fixture("root-read-admission");
        let preparation_before = preparation_entry_count(&fixture);

        // Fill the fixed root-owned queue without granting an operation. The
        // production range call is the 1,025th ticket and therefore must fail
        // before validating its deliberately malformed path, querying the
        // sink, opening occupied storage, or creating preparation state.
        let mut pending = Vec::with_capacity(1_024);
        for cancellation_key in 0..1_024_u64 {
            pending.push(
                fixture
                    .cas
                    .issue_pending_admission_for_test_v1(cancellation_key)
                    .expect("one fixed pending ticket"),
            );
        }
        let (mut comparison, mut path) = read_buffers();
        let mut counters = OperationCountersV1::default();
        let mut sink = CaptureSink::new(1);
        let error = read_file_range_v1(
            &fixture.cas,
            0x1_025,
            fixture.version,
            fixture.root_tree,
            b"invalid//path",
            0,
            1,
            &mut sink,
            &mut counters,
            ReadBuffersV1 {
                comparison: &mut comparison,
                path: &mut path,
            },
            &mut ContinueControl,
        )
        .expect_err("the 1,025th root ticket must be refused");
        assert!(matches!(
            error,
            ReadOperationErrorV1::FsCas(FsCasErrorV1::ResourceExhausted(_))
        ));
        assert_eq!(sink.bound_calls.get(), 0);
        assert_eq!(sink.began, 0);
        assert_eq!(counters.fscas_read_calls, 0);
        assert_eq!(counters.fscas_bytes_read, 0);
        assert_eq!(counters.root_admission_queue_entries, 0);
        assert_eq!(counters.root_admission_queue_refusals, 1);
        assert_eq!(counters.root_admission_queue_depth_high_water, 0);
        assert_eq!(counters.root_admission_active_slots_high_water, 0);
        assert_eq!(preparation_entry_count(&fixture), preparation_before);
        assert_eq!(fixture.cas.operation_admitted_slots_v1(), 0);
        drop(pending);

        // Saturate all sixteen shared slots. Cancellation and deadline are
        // observed while the new root ticket waits, still ahead of typed path
        // validation and every sink/storage callback.
        let mut held = Vec::with_capacity(16);
        let mut held_counters = OperationCountersV1::default();
        for cancellation_key in 0..16_u64 {
            held.push(
                fixture
                    .cas
                    .begin_operation_capability_v1(
                        FsOperationKindV1::RootExtraction,
                        0x2_000 + cancellation_key,
                        &mut held_counters,
                        &mut ContinueControl,
                    )
                    .expect("one of sixteen root slots"),
            );
        }
        assert_eq!(fixture.cas.operation_admitted_slots_v1(), 16);
        assert_eq!(held_counters.root_admission_queue_entries, 16);
        assert_eq!(held_counters.root_admission_queue_refusals, 0);
        assert_eq!(held_counters.root_admission_queue_depth_high_water, 1);
        assert_eq!(held_counters.root_admission_active_slots_high_water, 16);
        for (kind, expected) in [
            (StopKind::Cancel, CoreError::Cancelled),
            (StopKind::Deadline, CoreError::Deadline),
        ] {
            let (mut comparison, mut path) = read_buffers();
            let mut counters = OperationCountersV1::default();
            let mut sink = CaptureSink::new(1);
            let error = read_file_range_v1(
                &fixture.cas,
                0x3_000 + expected as u64,
                fixture.version,
                fixture.root_tree,
                b"invalid//path",
                0,
                1,
                &mut sink,
                &mut counters,
                ReadBuffersV1 {
                    comparison: &mut comparison,
                    path: &mut path,
                },
                &mut StopControl {
                    kind,
                    polls: 0,
                    stop_at: 1,
                },
            )
            .expect_err("waiting root read must stop");
            assert_eq!(
                error,
                ReadOperationErrorV1::FsCas(FsCasErrorV1::Core(expected))
            );
            assert_eq!(sink.bound_calls.get(), 0);
            assert_eq!(sink.began, 0);
            assert_eq!(counters.fscas_read_calls, 0);
            assert_eq!(counters.fscas_bytes_read, 0);
            assert_eq!(counters.root_admission_queue_entries, 1);
            assert_eq!(counters.root_admission_queue_refusals, 0);
            assert_eq!(counters.root_admission_queue_depth_high_water, 1);
            assert_eq!(counters.root_admission_active_slots_high_water, 0);
            assert_eq!(counters.root_admission_wait_polls, 0);
            assert_eq!(preparation_entry_count(&fixture), preparation_before);
            assert_eq!(fixture.cas.operation_admitted_slots_v1(), 16);
        }
        drop(held);
        assert_eq!(fixture.cas.operation_admitted_slots_v1(), 0);

        // A panic in the first post-grant sink observation exercises the
        // read operation's explicit unwind terminalizer. No occupied reader
        // or preparation artifact has been opened at this point.
        let (mut comparison, mut path) = read_buffers();
        let mut counters = OperationCountersV1::default();
        let mut sink = CaptureSink::new(1);
        sink.panic_on_bound = true;
        let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = extract_root_v1(
                &fixture.cas,
                0x4_000,
                fixture.version,
                fixture.root_tree,
                &mut sink,
                &mut counters,
                ReadBuffersV1 {
                    comparison: &mut comparison,
                    path: &mut path,
                },
                &mut ContinueControl,
            );
        }));
        assert!(unwind.is_err());
        assert_eq!(sink.bound_calls.get(), 1);
        assert_eq!(sink.began, 0);
        assert_eq!(counters.fscas_read_calls, 0);
        assert_eq!(counters.fscas_bytes_read, 0);
        assert_eq!(counters.root_admission_queue_entries, 1);
        assert_eq!(counters.root_admission_active_slots_high_water, 1);
        assert_eq!(counters.storage_bytes_requested, 0);
        assert_eq!(
            counters.storage_bytes_requested,
            counters.storage_bytes_reserved
        );
        assert_eq!(
            counters.storage_bytes_reserved,
            counters.storage_bytes_released
                + counters.storage_bytes_committed
                + counters.storage_bytes_retained
        );
        assert_eq!(counters.storage_inodes_requested, 0);
        assert_eq!(
            counters.storage_inodes_requested,
            counters.storage_inodes_reserved
        );
        assert_eq!(
            counters.storage_inodes_reserved,
            counters.storage_inodes_released
                + counters.storage_inodes_committed
                + counters.storage_inodes_retained
        );
        assert_eq!(counters.storage_preparation_bytes_high_water, 0);
        assert_eq!(counters.storage_preparation_inodes_high_water, 0);
        assert_eq!(counters.storage_preparation_bytes_current_after_cleanup, 0);
        assert_eq!(counters.storage_preparation_inodes_current_after_cleanup, 0);
        assert_eq!(counters.mutable_preparation_residue_bytes, 0);
        assert_eq!(counters.mutable_preparation_residue_inodes, 0);
        assert_eq!(counters.immutable_residue_bytes, 0);
        assert_eq!(counters.immutable_residue_inodes, 0);
        assert_eq!(preparation_entry_count(&fixture), preparation_before);
        assert_eq!(fixture.cas.operation_admitted_slots_v1(), 0);
    }

    #[test]
    fn read_storage_terminal_invalidation_unwind_releases_authority_and_finishes_observation() {
        let fixture = create_fixture("read-terminal-invalidation-unwind");
        let stale = FsCasV1::open_existing(fixture._root.path()).expect("open stale read handle");
        let preparation_before = preparation_entry_count(&fixture);
        let (mut comparison, mut path) = read_buffers();
        let mut counters = OperationCountersV1::default();
        let mut sink = CaptureSink::new(256 * 1024);
        sink.poison_storage_on_finish = Some(fixture.cas.clone());
        let mut control = PanicDuringReadTerminalInvalidation::default();

        assert_eq!(
            extract_root_v1(
                &fixture.cas,
                0x4_100,
                fixture.version,
                fixture.root_tree,
                &mut sink,
                &mut counters,
                ReadBuffersV1 {
                    comparison: &mut comparison,
                    path: &mut path,
                },
                &mut control,
            ),
            Err(ReadOperationErrorV1::FsCas(
                FsCasErrorV1::SynchronizationPoisoned
            ))
        );
        assert!(control.panicked);
        assert_eq!(sink.finished, 1);
        assert_eq!(sink.aborted, 0);
        assert_eq!(fixture.cas.operation_admitted_slots_v1(), 0);
        assert_eq!(fixture.cas.operation_admission_active_for_test_v1(), 0);
        assert_eq!(
            fixture.cas.operation_admission_queue_for_test_v1(),
            (0, 0, 0)
        );
        assert_eq!(
            fixture.cas.storage_admission_active_for_test_v1(),
            (0, 0, 0)
        );
        assert_eq!(preparation_entry_count(&fixture), preparation_before);
        assert_eq!(counters.storage_bytes_requested, 0);
        assert_eq!(
            counters.storage_bytes_requested,
            counters.storage_bytes_reserved
        );
        assert_eq!(counters.storage_inodes_requested, 0);
        assert_eq!(
            counters.storage_inodes_requested,
            counters.storage_inodes_reserved
        );
        assert_eq!(
            counters.storage_bytes_reserved,
            counters.storage_bytes_released
                + counters.storage_bytes_committed
                + counters.storage_bytes_retained
        );
        assert_eq!(
            counters.storage_inodes_reserved,
            counters.storage_inodes_released
                + counters.storage_inodes_committed
                + counters.storage_inodes_retained
        );
        assert_eq!(counters.storage_preparation_bytes_current_after_cleanup, 0);
        assert_eq!(counters.storage_preparation_inodes_current_after_cleanup, 0);
        assert_eq!(counters.mutable_preparation_residue_bytes, 0);
        assert_eq!(counters.mutable_preparation_residue_inodes, 0);
        assert_eq!(counters.immutable_residue_bytes, 0);
        assert_eq!(counters.immutable_residue_inodes, 0);
        assert!(counters.visibility_lock_acquisitions > 0);
        assert_eq!(counters.publication_lock_acquisitions, 0);
        assert!(counters.has_zero_forbidden_work());
        assert!(matches!(
            fixture.cas.occupied(),
            Err(FsCasErrorV1::Invalidated)
        ));
        assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
        assert!(matches!(
            FsCasV1::open_existing(fixture._root.path()),
            Err(FsCasErrorV1::Invalidated)
        ));
    }

    #[test]
    fn read_terminal_unwind_preserves_typed_observation_failure_after_explicit_release() {
        for (case_index, case, injected_observation) in [
            (0_u64, "clean-observation", None),
            (
                1_u64,
                "invalid-observation-state",
                Some(CoreError::PackInvalid),
            ),
            (2_u64, "observation-counter-overflow", None),
        ] {
            let fixture = create_fixture(&format!(
                "read-terminal-unwind-observation-{case_index}-{case}"
            ));
            let stale =
                FsCasV1::open_existing(fixture._root.path()).expect("open stale read handle");
            let preparation_before = preparation_entry_count(&fixture);
            let (mut comparison, mut path) = read_buffers();
            let mut counters = OperationCountersV1::default();
            if case == "observation-counter-overflow" {
                counters.visibility_lock_acquisitions = u64::MAX;
            }
            let mut sink = CaptureSink::new(256 * 1024);
            let mut control = PanicAfterReadTerminalRelease {
                terminal_unwind_pending: true,
                observation_failure: injected_observation,
                terminal_hook_calls: 0,
                observation_hook_calls: 0,
            };

            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                extract_root_v1(
                    &fixture.cas,
                    0x4_180 + case_index,
                    fixture.version,
                    fixture.root_tree,
                    &mut sink,
                    &mut counters,
                    ReadBuffersV1 {
                        comparison: &mut comparison,
                        path: &mut path,
                    },
                    &mut control,
                )
            }));

            match case {
                "clean-observation" => {
                    let payload = outcome.expect_err(
                        "a terminal unwind with a valid observation must remain an unwind",
                    );
                    assert_eq!(
                        payload.downcast_ref::<&'static str>().copied(),
                        Some("injected operation-terminal unwind after explicit release")
                    );
                }
                "invalid-observation-state" => assert_eq!(
                    outcome.expect("a typed observation failure must consume the unwind"),
                    Err(ReadOperationErrorV1::Core(CoreError::PackInvalid))
                ),
                "observation-counter-overflow" => assert_eq!(
                    outcome.expect("a typed observation failure must consume the unwind"),
                    Err(ReadOperationErrorV1::Core(CoreError::IntegerOverflow))
                ),
                _ => unreachable!(),
            }

            assert_eq!(control.terminal_hook_calls, 1, "{case}");
            assert_eq!(control.observation_hook_calls, 1, "{case}");
            assert_eq!(sink.began, 1, "{case}");
            assert_eq!(sink.finished, 1, "{case}");
            assert_eq!(sink.aborted, 0, "{case}");
            assert!(sink.current.is_none(), "{case}");
            assert_eq!(fixture.cas.operation_admitted_slots_v1(), 0, "{case}");
            assert_eq!(
                fixture.cas.operation_admission_active_for_test_v1(),
                0,
                "{case}"
            );
            assert_eq!(
                fixture.cas.operation_admission_queue_for_test_v1(),
                (0, 0, 0),
                "{case}"
            );
            assert_eq!(
                fixture.cas.storage_admission_active_for_test_v1(),
                (0, 0, 0),
                "{case}"
            );
            assert_eq!(
                preparation_entry_count(&fixture),
                preparation_before,
                "{case}"
            );
            assert_eq!(counters.storage_bytes_requested, 0, "{case}");
            assert_eq!(
                counters.storage_bytes_requested, counters.storage_bytes_reserved,
                "{case}"
            );
            assert_eq!(
                counters.storage_bytes_reserved,
                counters.storage_bytes_released
                    + counters.storage_bytes_committed
                    + counters.storage_bytes_retained,
                "{case}"
            );
            assert_eq!(counters.storage_inodes_requested, 0, "{case}");
            assert_eq!(
                counters.storage_inodes_requested, counters.storage_inodes_reserved,
                "{case}"
            );
            assert_eq!(
                counters.storage_inodes_reserved,
                counters.storage_inodes_released
                    + counters.storage_inodes_committed
                    + counters.storage_inodes_retained,
                "{case}"
            );
            assert_eq!(counters.storage_preparation_bytes_high_water, 0, "{case}");
            assert_eq!(counters.storage_preparation_inodes_high_water, 0, "{case}");
            assert_eq!(
                counters.storage_preparation_bytes_current_after_cleanup, 0,
                "{case}"
            );
            assert_eq!(
                counters.storage_preparation_inodes_current_after_cleanup, 0,
                "{case}"
            );
            assert_eq!(counters.mutable_preparation_residue_bytes, 0, "{case}");
            assert_eq!(counters.mutable_preparation_residue_inodes, 0, "{case}");
            assert_eq!(counters.immutable_residue_bytes, 0, "{case}");
            assert_eq!(counters.immutable_residue_inodes, 0, "{case}");
            assert!(counters.has_zero_forbidden_work(), "{case}");
            assert!(
                fixture.cas.visibility_lock_available_for_test_v1(),
                "{case}"
            );
            assert!(
                fixture.cas.publication_lock_available_for_test_v1(),
                "{case}"
            );
            assert!(fixture.cas.occupied().is_ok(), "{case}");
            assert!(stale.occupied().is_ok(), "{case}");
            assert!(
                FsCasV1::open_existing(fixture._root.path()).is_ok(),
                "{case}"
            );
        }
    }

    #[test]
    fn typed_read_failure_survives_operation_terminal_unwind_and_later_observation() {
        for (case_index, operation_name, range, observation_failure) in [
            (0_u64, "full-clean-observation", false, None),
            (
                1_u64,
                "full-failed-observation",
                false,
                Some(CoreError::PackInvalid),
            ),
            (2_u64, "exact-range-clean-observation", true, None),
            (
                3_u64,
                "exact-range-failed-observation",
                true,
                Some(CoreError::PackInvalid),
            ),
        ] {
            let fixture = create_fixture(&format!(
                "typed-read-failure-terminal-unwind-{operation_name}"
            ));
            let stale =
                FsCasV1::open_existing(fixture._root.path()).expect("open stale read handle");
            let preparation_before = preparation_entry_count(&fixture);
            let (mut comparison, mut path) = read_buffers();
            let mut counters = OperationCountersV1::default();
            let mut sink = CaptureSink::new(256 * 1024);
            sink.remove_objects_on_begin = Some(fixture._root.path().join("objects"));
            let mut control = PanicAfterReadTerminalRelease {
                terminal_unwind_pending: true,
                observation_failure,
                terminal_hook_calls: 0,
                observation_hook_calls: 0,
            };

            let result = if range {
                read_file_range_v1(
                    &fixture.cas,
                    0x4_190 + case_index,
                    fixture.version,
                    fixture.root_tree,
                    b"d/b.bin",
                    817,
                    17_777,
                    &mut sink,
                    &mut counters,
                    ReadBuffersV1 {
                        comparison: &mut comparison,
                        path: &mut path,
                    },
                    &mut control,
                )
            } else {
                extract_root_v1(
                    &fixture.cas,
                    0x4_190 + case_index,
                    fixture.version,
                    fixture.root_tree,
                    &mut sink,
                    &mut counters,
                    ReadBuffersV1 {
                        comparison: &mut comparison,
                        path: &mut path,
                    },
                    &mut control,
                )
            };

            assert_eq!(
                result,
                Err(ReadOperationErrorV1::FsCas(FsCasErrorV1::MissingOccupant)),
                "{operation_name}"
            );
            assert_eq!(control.terminal_hook_calls, 1, "{operation_name}");
            assert_eq!(control.observation_hook_calls, 1, "{operation_name}");
            assert_eq!(sink.began, 1, "{operation_name}");
            assert_eq!(sink.finished, 0, "{operation_name}");
            assert_eq!(sink.aborted, 1, "{operation_name}");
            assert!(sink.current.is_none(), "{operation_name}");
            assert_eq!(
                fixture.cas.operation_admitted_slots_v1(),
                0,
                "{operation_name}"
            );
            assert_eq!(
                fixture.cas.operation_admission_active_for_test_v1(),
                0,
                "{operation_name}"
            );
            assert_eq!(
                fixture.cas.operation_admission_queue_for_test_v1(),
                (0, 0, 0),
                "{operation_name}"
            );
            assert_eq!(
                fixture.cas.storage_admission_active_for_test_v1(),
                (0, 0, 0),
                "{operation_name}"
            );
            assert_eq!(
                preparation_entry_count(&fixture),
                preparation_before,
                "{operation_name}"
            );
            assert_eq!(counters.storage_bytes_requested, 0, "{operation_name}");
            assert_eq!(
                counters.storage_bytes_requested, counters.storage_bytes_reserved,
                "{operation_name}"
            );
            assert_eq!(
                counters.storage_bytes_reserved,
                counters.storage_bytes_released
                    + counters.storage_bytes_committed
                    + counters.storage_bytes_retained,
                "{operation_name}"
            );
            assert_eq!(counters.storage_inodes_requested, 0, "{operation_name}");
            assert_eq!(
                counters.storage_inodes_requested, counters.storage_inodes_reserved,
                "{operation_name}"
            );
            assert_eq!(
                counters.storage_inodes_reserved,
                counters.storage_inodes_released
                    + counters.storage_inodes_committed
                    + counters.storage_inodes_retained,
                "{operation_name}"
            );
            assert_eq!(
                counters.storage_preparation_bytes_current_after_cleanup, 0,
                "{operation_name}"
            );
            assert_eq!(
                counters.storage_preparation_inodes_current_after_cleanup, 0,
                "{operation_name}"
            );
            assert_eq!(
                counters.mutable_preparation_residue_bytes, 0,
                "{operation_name}"
            );
            assert_eq!(
                counters.mutable_preparation_residue_inodes, 0,
                "{operation_name}"
            );
            assert_eq!(counters.immutable_residue_bytes, 0, "{operation_name}");
            assert_eq!(counters.immutable_residue_inodes, 0, "{operation_name}");
            assert!(counters.has_zero_forbidden_work(), "{operation_name}");
            assert!(
                fixture.cas.visibility_lock_available_for_test_v1(),
                "{operation_name}"
            );
            assert!(
                fixture.cas.publication_lock_available_for_test_v1(),
                "{operation_name}"
            );
            assert!(fixture.cas.occupied().is_ok(), "{operation_name}");
            assert!(stale.occupied().is_ok(), "{operation_name}");
            assert!(
                FsCasV1::open_existing(fixture._root.path()).is_ok(),
                "{operation_name}"
            );
        }
    }

    #[test]
    fn typed_post_begin_read_failure_survives_sink_abort_unwind() {
        let fixture = create_fixture("typed-post-begin-read-failure-abort-unwind");
        let preparation_before = preparation_entry_count(&fixture);
        let (mut comparison, mut path) = read_buffers();
        let mut counters = OperationCountersV1::default();
        let mut sink = CaptureSink::new(256 * 1024);
        sink.remove_objects_on_begin = Some(fixture._root.path().join("objects"));
        sink.panic_on_abort = true;

        let error = extract_root_v1(
            &fixture.cas,
            0x4_180,
            fixture.version,
            fixture.root_tree,
            &mut sink,
            &mut counters,
            ReadBuffersV1 {
                comparison: &mut comparison,
                path: &mut path,
            },
            &mut ContinueControl,
        )
        .expect_err("the typed read failure must survive the abort unwind");

        assert_eq!(
            error,
            ReadOperationErrorV1::FsCas(FsCasErrorV1::MissingOccupant)
        );
        assert_eq!(sink.began, 1);
        assert_eq!(sink.finished, 0);
        assert_eq!(sink.aborted, 1);
        assert!(sink.current.is_none());
        assert_eq!(fixture.cas.operation_admitted_slots_v1(), 0);
        assert_eq!(fixture.cas.operation_admission_active_for_test_v1(), 0);
        assert_eq!(
            fixture.cas.operation_admission_queue_for_test_v1(),
            (0, 0, 0)
        );
        assert_eq!(
            fixture.cas.storage_admission_active_for_test_v1(),
            (0, 0, 0)
        );
        assert_eq!(preparation_entry_count(&fixture), preparation_before);
        assert_eq!(counters.storage_bytes_requested, 0);
        assert_eq!(
            counters.storage_bytes_requested,
            counters.storage_bytes_reserved
        );
        assert_eq!(
            counters.storage_bytes_reserved,
            counters.storage_bytes_released
                + counters.storage_bytes_committed
                + counters.storage_bytes_retained
        );
        assert_eq!(counters.storage_inodes_requested, 0);
        assert_eq!(
            counters.storage_inodes_requested,
            counters.storage_inodes_reserved
        );
        assert_eq!(
            counters.storage_inodes_reserved,
            counters.storage_inodes_released
                + counters.storage_inodes_committed
                + counters.storage_inodes_retained
        );
        assert_eq!(counters.mutable_preparation_residue_bytes, 0);
        assert_eq!(counters.mutable_preparation_residue_inodes, 0);
        assert_eq!(counters.immutable_residue_bytes, 0);
        assert_eq!(counters.immutable_residue_inodes, 0);
        assert!(counters.has_zero_forbidden_work());
        assert!(fixture.cas.visibility_lock_available_for_test_v1());
        assert!(fixture.cas.publication_lock_available_for_test_v1());
    }

    #[test]
    fn typed_post_begin_stop_survives_sink_abort_unwind() {
        for (case, kind, expected) in [
            ("cancel", StopKind::Cancel, CoreError::Cancelled),
            ("deadline", StopKind::Deadline, CoreError::Deadline),
        ] {
            let fixture = create_fixture(&format!("typed-post-begin-{case}-abort-unwind"));
            let preparation_before = preparation_entry_count(&fixture);
            let stopped = Rc::new(Cell::new(false));
            let (mut comparison, mut path) = read_buffers();
            let mut counters = OperationCountersV1::default();
            let mut sink = CaptureSink::new(256 * 1024);
            sink.stop_after_begin = Some(Rc::clone(&stopped));
            sink.panic_on_abort = true;
            let mut control = StopAfterBeginControl { kind, stopped };

            let error = extract_root_v1(
                &fixture.cas,
                0x4_200,
                fixture.version,
                fixture.root_tree,
                &mut sink,
                &mut counters,
                ReadBuffersV1 {
                    comparison: &mut comparison,
                    path: &mut path,
                },
                &mut control,
            )
            .expect_err("the typed stop must survive the abort unwind");

            assert_eq!(error, ReadOperationErrorV1::Core(expected), "{case}");
            assert_eq!(sink.began, 1, "{case}");
            assert_eq!(sink.finished, 0, "{case}");
            assert_eq!(sink.aborted, 1, "{case}");
            assert!(sink.current.is_none(), "{case}");
            assert_eq!(fixture.cas.operation_admitted_slots_v1(), 0, "{case}");
            assert_eq!(
                fixture.cas.operation_admission_active_for_test_v1(),
                0,
                "{case}"
            );
            assert_eq!(
                fixture.cas.operation_admission_queue_for_test_v1(),
                (0, 0, 0),
                "{case}"
            );
            assert_eq!(
                fixture.cas.storage_admission_active_for_test_v1(),
                (0, 0, 0),
                "{case}"
            );
            assert_eq!(
                preparation_entry_count(&fixture),
                preparation_before,
                "{case}"
            );
            assert_eq!(counters.storage_bytes_requested, 0, "{case}");
            assert_eq!(
                counters.storage_bytes_requested, counters.storage_bytes_reserved,
                "{case}"
            );
            assert_eq!(
                counters.storage_bytes_reserved,
                counters.storage_bytes_released
                    + counters.storage_bytes_committed
                    + counters.storage_bytes_retained,
                "{case}"
            );
            assert_eq!(counters.storage_inodes_requested, 0, "{case}");
            assert_eq!(
                counters.storage_inodes_requested, counters.storage_inodes_reserved,
                "{case}"
            );
            assert_eq!(
                counters.storage_inodes_reserved,
                counters.storage_inodes_released
                    + counters.storage_inodes_committed
                    + counters.storage_inodes_retained,
                "{case}"
            );
            assert_eq!(counters.mutable_preparation_residue_bytes, 0, "{case}");
            assert_eq!(counters.mutable_preparation_residue_inodes, 0, "{case}");
            assert_eq!(counters.immutable_residue_bytes, 0, "{case}");
            assert_eq!(counters.immutable_residue_inodes, 0, "{case}");
            assert!(counters.has_zero_forbidden_work(), "{case}");
            assert!(
                fixture.cas.visibility_lock_available_for_test_v1(),
                "{case}"
            );
            assert!(
                fixture.cas.publication_lock_available_for_test_v1(),
                "{case}"
            );
            assert!(fixture.cas.occupied().is_ok(), "{case}");
            assert!(
                FsCasV1::open_existing(fixture._root.path()).is_ok(),
                "{case}"
            );
        }
    }

    #[test]
    fn typed_post_begin_filesystem_failure_survives_sink_abort_unwind() {
        for (case, fault) in [
            (
                "permission-denied",
                FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
            ),
            (
                "read-failure",
                FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ReadFailure),
            ),
            (
                "short-read",
                FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ShortRead),
            ),
        ] {
            let fixture = create_fixture(&format!("typed-post-begin-{case}-abort-unwind"));
            let preparation_before = preparation_entry_count(&fixture);
            let activated = Rc::new(Cell::new(false));
            let (mut comparison, mut path) = read_buffers();
            let mut counters = OperationCountersV1::default();
            let mut sink = CaptureSink::new(256 * 1024);
            sink.stop_after_begin = Some(Rc::clone(&activated));
            sink.panic_on_abort = true;
            let mut control = FilesystemFaultAfterBeginControl {
                activated,
                boundary: FsCasFilesystemBoundaryV1::ObjectLocatorRead,
                fault,
                injected: false,
            };

            let error = extract_root_v1(
                &fixture.cas,
                0x4_300,
                fixture.version,
                fixture.root_tree,
                &mut sink,
                &mut counters,
                ReadBuffersV1 {
                    comparison: &mut comparison,
                    path: &mut path,
                },
                &mut control,
            )
            .expect_err("the typed filesystem read failure must survive the abort unwind");

            assert_eq!(error, ReadOperationErrorV1::FsCas(fault), "{case}");
            assert!(control.injected, "{case}");
            assert_eq!(sink.began, 1, "{case}");
            assert_eq!(sink.finished, 0, "{case}");
            assert_eq!(sink.aborted, 1, "{case}");
            assert!(sink.current.is_none(), "{case}");
            assert_eq!(fixture.cas.operation_admitted_slots_v1(), 0, "{case}");
            assert_eq!(
                fixture.cas.operation_admission_active_for_test_v1(),
                0,
                "{case}"
            );
            assert_eq!(
                fixture.cas.operation_admission_queue_for_test_v1(),
                (0, 0, 0),
                "{case}"
            );
            assert_eq!(
                fixture.cas.storage_admission_active_for_test_v1(),
                (0, 0, 0),
                "{case}"
            );
            assert_eq!(
                preparation_entry_count(&fixture),
                preparation_before,
                "{case}"
            );
            assert_eq!(counters.storage_bytes_requested, 0, "{case}");
            assert_eq!(
                counters.storage_bytes_requested, counters.storage_bytes_reserved,
                "{case}"
            );
            assert_eq!(
                counters.storage_bytes_reserved,
                counters.storage_bytes_released
                    + counters.storage_bytes_committed
                    + counters.storage_bytes_retained,
                "{case}"
            );
            assert_eq!(counters.storage_inodes_requested, 0, "{case}");
            assert_eq!(
                counters.storage_inodes_requested, counters.storage_inodes_reserved,
                "{case}"
            );
            assert_eq!(
                counters.storage_inodes_reserved,
                counters.storage_inodes_released
                    + counters.storage_inodes_committed
                    + counters.storage_inodes_retained,
                "{case}"
            );
            assert_eq!(counters.mutable_preparation_residue_bytes, 0, "{case}");
            assert_eq!(counters.mutable_preparation_residue_inodes, 0, "{case}");
            assert_eq!(counters.immutable_residue_bytes, 0, "{case}");
            assert_eq!(counters.immutable_residue_inodes, 0, "{case}");
            assert!(counters.has_zero_forbidden_work(), "{case}");
            assert!(
                fixture.cas.visibility_lock_available_for_test_v1(),
                "{case}"
            );
            assert!(
                fixture.cas.publication_lock_available_for_test_v1(),
                "{case}"
            );
            assert!(fixture.cas.occupied().is_ok(), "{case}");
            assert!(
                FsCasV1::open_existing(fixture._root.path()).is_ok(),
                "{case}"
            );
        }
    }

    #[test]
    fn occupied_read_failures_are_exact_for_full_and_range_reads() {
        for (boundary_name, boundary) in [
            ("locator", FsCasFilesystemBoundaryV1::ObjectLocatorRead),
            ("catalog", FsCasFilesystemBoundaryV1::CatalogMarkerRead),
            (
                "carrier-metadata",
                FsCasFilesystemBoundaryV1::CarrierMetadataRead,
            ),
            ("carrier-index", FsCasFilesystemBoundaryV1::CarrierIndexRead),
            (
                "carrier-object",
                FsCasFilesystemBoundaryV1::CarrierObjectRead,
            ),
            (
                "carrier-payload",
                FsCasFilesystemBoundaryV1::CarrierPayloadRead,
            ),
        ] {
            for (fault_name, fault) in [
                ("missing-occupant", FsCasErrorV1::MissingOccupant),
                (
                    "permission-denied",
                    FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
                ),
                (
                    "read-failure",
                    FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ReadFailure),
                ),
                (
                    "short-read",
                    FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ShortRead),
                ),
            ] {
                for range in [false, true] {
                    let operation_name = if range { "range" } else { "full" };
                    let fixture = create_fixture(&format!(
                        "occupied-{boundary_name}-{fault_name}-{operation_name}"
                    ));
                    let preparation_before = preparation_entry_count(&fixture);
                    let activated = Rc::new(Cell::new(false));
                    let (mut comparison, mut path) = read_buffers();
                    let mut counters = OperationCountersV1::default();
                    let mut sink = CaptureSink::new(256 * 1024);
                    sink.stop_after_begin = Some(Rc::clone(&activated));
                    let mut control = FilesystemFaultAfterBeginControl {
                        activated,
                        boundary,
                        fault,
                        injected: false,
                    };

                    let result = if range {
                        read_file_range_v1(
                            &fixture.cas,
                            0x4_340,
                            fixture.version,
                            fixture.root_tree,
                            b"d/b.bin",
                            817,
                            17_777,
                            &mut sink,
                            &mut counters,
                            ReadBuffersV1 {
                                comparison: &mut comparison,
                                path: &mut path,
                            },
                            &mut control,
                        )
                    } else {
                        extract_root_v1(
                            &fixture.cas,
                            0x4_341,
                            fixture.version,
                            fixture.root_tree,
                            &mut sink,
                            &mut counters,
                            ReadBuffersV1 {
                                comparison: &mut comparison,
                                path: &mut path,
                            },
                            &mut control,
                        )
                    };
                    let error = result.expect_err(
                        "the semantic occupied-read failure must terminate the authenticated read",
                    );

                    assert_eq!(error, ReadOperationErrorV1::FsCas(fault));
                    assert!(control.injected, "{fault_name}/{operation_name}");
                    assert_eq!(sink.began, 1, "{fault_name}/{operation_name}");
                    assert_eq!(sink.finished, 0, "{fault_name}/{operation_name}");
                    assert_eq!(sink.aborted, 1, "{fault_name}/{operation_name}");
                    assert!(sink.current.is_none(), "{fault_name}/{operation_name}");
                    assert_eq!(fixture.cas.operation_admitted_slots_v1(), 0);
                    assert_eq!(fixture.cas.operation_admission_active_for_test_v1(), 0);
                    assert_eq!(
                        fixture.cas.operation_admission_queue_for_test_v1(),
                        (0, 0, 0)
                    );
                    assert_eq!(
                        fixture.cas.storage_admission_active_for_test_v1(),
                        (0, 0, 0)
                    );
                    assert_eq!(preparation_entry_count(&fixture), preparation_before);
                    assert_eq!(
                        counters.storage_bytes_requested,
                        counters.storage_bytes_reserved
                    );
                    assert_eq!(
                        counters.storage_bytes_reserved,
                        counters.storage_bytes_released
                            + counters.storage_bytes_committed
                            + counters.storage_bytes_retained
                    );
                    assert_eq!(
                        counters.storage_inodes_requested,
                        counters.storage_inodes_reserved
                    );
                    assert_eq!(
                        counters.storage_inodes_reserved,
                        counters.storage_inodes_released
                            + counters.storage_inodes_committed
                            + counters.storage_inodes_retained
                    );
                    assert_eq!(counters.mutable_preparation_residue_bytes, 0);
                    assert_eq!(counters.mutable_preparation_residue_inodes, 0);
                    assert_eq!(counters.immutable_residue_bytes, 0);
                    assert_eq!(counters.immutable_residue_inodes, 0);
                    assert!(fixture.cas.visibility_lock_available_for_test_v1());
                    assert!(fixture.cas.publication_lock_available_for_test_v1());
                    assert!(fixture.cas.occupied().is_ok());
                    assert!(FsCasV1::open_existing(fixture._root.path()).is_ok());
                }
            }
        }
    }

    #[test]
    fn closure_marker_read_failures_are_exact_for_full_and_range_reads() {
        for (fault_name, fault) in [
            ("missing-occupant", FsCasErrorV1::MissingOccupant),
            (
                "permission-denied",
                FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
            ),
            (
                "read-failure",
                FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ReadFailure),
            ),
            (
                "short-read",
                FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ShortRead),
            ),
        ] {
            for range in [false, true] {
                let operation_name = if range { "range" } else { "full" };
                let fixture =
                    create_fixture(&format!("closure-marker-{fault_name}-{operation_name}"));
                let preparation_before = preparation_entry_count(&fixture);
                let (mut comparison, mut path) = read_buffers();
                let mut counters = OperationCountersV1::default();
                let mut sink = CaptureSink::new(256 * 1024);
                let mut control = FilesystemFaultControl {
                    boundary: FsCasFilesystemBoundaryV1::ClosureMarkerRead,
                    fault,
                    injected: false,
                };

                let result = if range {
                    read_file_range_v1(
                        &fixture.cas,
                        0x4_350,
                        fixture.version,
                        fixture.root_tree,
                        b"d/b.bin",
                        817,
                        17_777,
                        &mut sink,
                        &mut counters,
                        ReadBuffersV1 {
                            comparison: &mut comparison,
                            path: &mut path,
                        },
                        &mut control,
                    )
                } else {
                    extract_root_v1(
                        &fixture.cas,
                        0x4_351,
                        fixture.version,
                        fixture.root_tree,
                        &mut sink,
                        &mut counters,
                        ReadBuffersV1 {
                            comparison: &mut comparison,
                            path: &mut path,
                        },
                        &mut control,
                    )
                };
                let error = result.expect_err(
                    "the semantic closure-marker failure must terminate before the sink begins",
                );

                assert_eq!(error, ReadOperationErrorV1::FsCas(fault));
                assert!(control.injected, "{fault_name}/{operation_name}");
                assert_eq!(sink.began, 0, "{fault_name}/{operation_name}");
                assert_eq!(sink.finished, 0, "{fault_name}/{operation_name}");
                assert_eq!(sink.aborted, 0, "{fault_name}/{operation_name}");
                assert_eq!(fixture.cas.operation_admitted_slots_v1(), 0);
                assert_eq!(fixture.cas.operation_admission_active_for_test_v1(), 0);
                assert_eq!(
                    fixture.cas.operation_admission_queue_for_test_v1(),
                    (0, 0, 0)
                );
                assert_eq!(
                    fixture.cas.storage_admission_active_for_test_v1(),
                    (0, 0, 0)
                );
                assert_eq!(preparation_entry_count(&fixture), preparation_before);
                assert_eq!(counters.storage_bytes_requested, 0);
                assert_eq!(counters.storage_inodes_requested, 0);
                assert!(counters.has_zero_forbidden_work());
                assert!(fixture.cas.visibility_lock_available_for_test_v1());
                assert!(fixture.cas.publication_lock_available_for_test_v1());
                assert!(fixture.cas.occupied().is_ok());
                assert!(FsCasV1::open_existing(fixture._root.path()).is_ok());
            }
        }
    }

    #[test]
    fn closure_marker_read_failure_retains_invalidation_dominance_before_sink_begin() {
        for (operation_name, range) in [("full", false), ("exact-range", true)] {
            let fixture = create_fixture(&format!(
                "typed-closure-marker-read-failure-invalidation-{operation_name}"
            ));
            let stale = fixture.cas.clone();
            let preparation_before = preparation_entry_count(&fixture);
            let (mut comparison, mut path) = read_buffers();
            let mut counters = OperationCountersV1::default();
            let mut sink = CaptureSink::new(256 * 1024);
            let primary = FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied);
            let mut control = FilesystemFaultBeforeSinkInvalidationControl {
                cas: fixture.cas.clone(),
                boundary: FsCasFilesystemBoundaryV1::ClosureMarkerRead,
                fault: primary,
                injected: false,
                invalidation_attempts: 0,
            };

            let error = if range {
                read_file_range_v1(
                    &fixture.cas,
                    0x4_370,
                    fixture.version,
                    fixture.root_tree,
                    b"d/b.bin",
                    817,
                    17_777,
                    &mut sink,
                    &mut counters,
                    ReadBuffersV1 {
                        comparison: &mut comparison,
                        path: &mut path,
                    },
                    &mut control,
                )
            } else {
                extract_root_v1(
                    &fixture.cas,
                    0x4_370,
                    fixture.version,
                    fixture.root_tree,
                    &mut sink,
                    &mut counters,
                    ReadBuffersV1 {
                        comparison: &mut comparison,
                        path: &mut path,
                    },
                    &mut control,
                )
            }
            .expect_err("the pre-sink closure-marker error must survive terminal invalidation");

            assert_eq!(
                error,
                ReadOperationErrorV1::FsCas(FsCasErrorV1::TerminalFailure {
                    first: FsCasFailureCauseV1::Filesystem(
                        FsCasFilesystemFailureV1::PermissionDenied
                    ),
                    dominant: FsCasFailureCauseV1::InvalidationFailed,
                }),
                "{operation_name}"
            );
            assert!(control.injected, "{operation_name}");
            assert_eq!(control.invalidation_attempts, 1, "{operation_name}");
            assert_eq!(sink.began, 0, "{operation_name}");
            assert_eq!(sink.finished, 0, "{operation_name}");
            assert_eq!(sink.aborted, 0, "{operation_name}");
            assert!(sink.current.is_none(), "{operation_name}");
            assert_eq!(
                fixture.cas.operation_admitted_slots_v1(),
                0,
                "{operation_name}"
            );
            assert_eq!(
                fixture.cas.operation_admission_active_for_test_v1(),
                0,
                "{operation_name}"
            );
            assert_eq!(
                fixture.cas.operation_admission_queue_for_test_v1(),
                (0, 0, 0),
                "{operation_name}"
            );
            assert_eq!(
                fixture.cas.storage_admission_active_for_test_v1(),
                (0, 0, 0),
                "{operation_name}"
            );
            assert_eq!(
                preparation_entry_count(&fixture),
                preparation_before,
                "{operation_name}"
            );
            assert_eq!(
                counters.storage_bytes_requested, counters.storage_bytes_reserved,
                "{operation_name}"
            );
            assert_eq!(
                counters.storage_bytes_reserved,
                counters.storage_bytes_released
                    + counters.storage_bytes_committed
                    + counters.storage_bytes_retained,
                "{operation_name}"
            );
            assert_eq!(
                counters.storage_inodes_requested, counters.storage_inodes_reserved,
                "{operation_name}"
            );
            assert_eq!(
                counters.storage_inodes_reserved,
                counters.storage_inodes_released
                    + counters.storage_inodes_committed
                    + counters.storage_inodes_retained,
                "{operation_name}"
            );
            assert_eq!(
                counters.mutable_preparation_residue_bytes, 0,
                "{operation_name}"
            );
            assert_eq!(
                counters.mutable_preparation_residue_inodes, 0,
                "{operation_name}"
            );
            assert_eq!(counters.immutable_residue_bytes, 0, "{operation_name}");
            assert_eq!(counters.immutable_residue_inodes, 0, "{operation_name}");
            assert!(counters.has_zero_forbidden_work(), "{operation_name}");
            assert!(
                fixture.cas.visibility_lock_available_for_test_v1(),
                "{operation_name}"
            );
            assert!(
                fixture.cas.publication_lock_available_for_test_v1(),
                "{operation_name}"
            );
            assert!(
                matches!(fixture.cas.occupied(), Err(FsCasErrorV1::Invalidated)),
                "{operation_name}"
            );
            assert!(
                matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
                "{operation_name}"
            );
            assert!(
                matches!(
                    FsCasV1::open_existing(fixture._root.path()),
                    Err(FsCasErrorV1::Busy | FsCasErrorV1::Invalidated)
                ),
                "{operation_name}"
            );
        }
    }

    #[test]
    fn typed_post_begin_read_failure_retains_invalidation_dominance_after_abort_unwind() {
        for (boundary_name, boundary) in [
            ("locator", FsCasFilesystemBoundaryV1::ObjectLocatorRead),
            ("catalog", FsCasFilesystemBoundaryV1::CatalogMarkerRead),
            (
                "carrier-metadata",
                FsCasFilesystemBoundaryV1::CarrierMetadataRead,
            ),
            ("carrier-index", FsCasFilesystemBoundaryV1::CarrierIndexRead),
            (
                "carrier-object",
                FsCasFilesystemBoundaryV1::CarrierObjectRead,
            ),
            (
                "carrier-payload",
                FsCasFilesystemBoundaryV1::CarrierPayloadRead,
            ),
        ] {
            for (operation_name, range) in [("full", false), ("exact-range", true)] {
                let fixture = create_fixture(&format!(
                    "typed-{boundary_name}-read-failure-invalidation-abort-unwind-{operation_name}"
                ));
                let stale = fixture.cas.clone();
                let preparation_before = preparation_entry_count(&fixture);
                let activated = Rc::new(Cell::new(false));
                let (mut comparison, mut path) = read_buffers();
                let mut counters = OperationCountersV1::default();
                let mut sink = CaptureSink::new(256 * 1024);
                sink.stop_after_begin = Some(Rc::clone(&activated));
                sink.poison_storage_on_abort = Some(fixture.cas.clone());
                sink.panic_on_abort = true;
                let primary = FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied);
                let mut control = FilesystemFaultAfterBeginInvalidationControl {
                    activated,
                    boundary,
                    fault: primary,
                    injected: false,
                    invalidation_attempts: 0,
                };

                let error = if range {
                    read_file_range_v1(
                        &fixture.cas,
                        0x4_380,
                        fixture.version,
                        fixture.root_tree,
                        b"d/b.bin",
                        817,
                        17_777,
                        &mut sink,
                        &mut counters,
                        ReadBuffersV1 {
                            comparison: &mut comparison,
                            path: &mut path,
                        },
                        &mut control,
                    )
                } else {
                    extract_root_v1(
                        &fixture.cas,
                        0x4_380,
                        fixture.version,
                        fixture.root_tree,
                        &mut sink,
                        &mut counters,
                        ReadBuffersV1 {
                            comparison: &mut comparison,
                            path: &mut path,
                        },
                        &mut control,
                    )
                }
                .expect_err(
                    "the typed read failure must survive the abort and terminal double fault",
                );

                assert_eq!(
                    error,
                    ReadOperationErrorV1::FsCas(FsCasErrorV1::TerminalFailure {
                        first: FsCasFailureCauseV1::Filesystem(
                            FsCasFilesystemFailureV1::PermissionDenied
                        ),
                        dominant: FsCasFailureCauseV1::InvalidationFailed,
                    }),
                    "{operation_name}"
                );
                assert!(control.injected, "{operation_name}");
                assert_eq!(control.invalidation_attempts, 1, "{operation_name}");
                assert_eq!(sink.began, 1, "{operation_name}");
                assert_eq!(sink.finished, 0, "{operation_name}");
                assert_eq!(sink.aborted, 1, "{operation_name}");
                assert!(sink.current.is_none(), "{operation_name}");
                assert_eq!(
                    fixture.cas.operation_admitted_slots_v1(),
                    0,
                    "{operation_name}"
                );
                assert_eq!(
                    fixture.cas.operation_admission_active_for_test_v1(),
                    0,
                    "{operation_name}"
                );
                assert_eq!(
                    fixture.cas.operation_admission_queue_for_test_v1(),
                    (0, 0, 0),
                    "{operation_name}"
                );
                assert_eq!(
                    fixture.cas.storage_admission_active_for_test_v1(),
                    (0, 0, 0),
                    "{operation_name}"
                );
                assert_eq!(
                    preparation_entry_count(&fixture),
                    preparation_before,
                    "{operation_name}"
                );
                assert_eq!(counters.storage_bytes_requested, 0, "{operation_name}");
                assert_eq!(
                    counters.storage_bytes_requested, counters.storage_bytes_reserved,
                    "{operation_name}"
                );
                assert_eq!(
                    counters.storage_bytes_reserved,
                    counters.storage_bytes_released
                        + counters.storage_bytes_committed
                        + counters.storage_bytes_retained,
                    "{operation_name}"
                );
                assert_eq!(counters.storage_inodes_requested, 0, "{operation_name}");
                assert_eq!(
                    counters.storage_inodes_requested, counters.storage_inodes_reserved,
                    "{operation_name}"
                );
                assert_eq!(
                    counters.storage_inodes_reserved,
                    counters.storage_inodes_released
                        + counters.storage_inodes_committed
                        + counters.storage_inodes_retained,
                    "{operation_name}"
                );
                assert_eq!(
                    counters.mutable_preparation_residue_bytes, 0,
                    "{operation_name}"
                );
                assert_eq!(
                    counters.mutable_preparation_residue_inodes, 0,
                    "{operation_name}"
                );
                assert_eq!(counters.immutable_residue_bytes, 0, "{operation_name}");
                assert_eq!(counters.immutable_residue_inodes, 0, "{operation_name}");
                assert!(counters.has_zero_forbidden_work(), "{operation_name}");
                assert!(
                    matches!(fixture.cas.occupied(), Err(FsCasErrorV1::Invalidated)),
                    "{operation_name}"
                );
                assert!(
                    matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
                    "{operation_name}"
                );
                assert!(
                    matches!(
                        FsCasV1::open_existing(fixture._root.path()),
                        Err(FsCasErrorV1::Busy | FsCasErrorV1::Invalidated)
                    ),
                    "{operation_name}"
                );
            }
        }
    }

    #[test]
    fn post_begin_sink_unwind_aborts_once_and_contains_abort_double_unwind() {
        for (case_index, panic_on_abort, expected_payload) in [
            (0_u64, false, "injected post-begin sink write unwind"),
            (1_u64, true, "injected sink abort unwind"),
        ] {
            let fixture = create_fixture(&format!("post-begin-sink-unwind-{case_index}"));
            let stale =
                FsCasV1::open_existing(fixture._root.path()).expect("open stale read handle");
            let preparation_before = preparation_entry_count(&fixture);
            let (mut comparison, mut path) = read_buffers();
            let mut counters = OperationCountersV1::default();
            let mut sink = CaptureSink::new(256 * 1024);
            sink.panic_on_write = true;
            sink.panic_on_abort = panic_on_abort;

            let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = extract_root_v1(
                    &fixture.cas,
                    0x4_200 + case_index,
                    fixture.version,
                    fixture.root_tree,
                    &mut sink,
                    &mut counters,
                    ReadBuffersV1 {
                        comparison: &mut comparison,
                        path: &mut path,
                    },
                    &mut ContinueControl,
                );
            }))
            .expect_err("the post-begin sink callback must unwind");

            assert_eq!(
                unwind.downcast_ref::<&'static str>().copied(),
                Some(expected_payload)
            );
            assert_eq!(sink.began, 1);
            assert_eq!(sink.finished, 0);
            assert_eq!(sink.aborted, 1);
            assert!(sink.current.is_none());
            assert_eq!(fixture.cas.operation_admitted_slots_v1(), 0);
            assert_eq!(fixture.cas.operation_admission_active_for_test_v1(), 0);
            assert_eq!(
                fixture.cas.operation_admission_queue_for_test_v1(),
                (0, 0, 0)
            );
            assert_eq!(
                fixture.cas.storage_admission_active_for_test_v1(),
                (0, 0, 0)
            );
            assert_eq!(preparation_entry_count(&fixture), preparation_before);
            assert_eq!(counters.storage_bytes_requested, 0);
            assert_eq!(
                counters.storage_bytes_requested,
                counters.storage_bytes_reserved
            );
            assert_eq!(
                counters.storage_bytes_reserved,
                counters.storage_bytes_released
                    + counters.storage_bytes_committed
                    + counters.storage_bytes_retained
            );
            assert_eq!(counters.storage_inodes_requested, 0);
            assert_eq!(
                counters.storage_inodes_requested,
                counters.storage_inodes_reserved
            );
            assert_eq!(
                counters.storage_inodes_reserved,
                counters.storage_inodes_released
                    + counters.storage_inodes_committed
                    + counters.storage_inodes_retained
            );
            assert_eq!(counters.storage_preparation_bytes_high_water, 0);
            assert_eq!(counters.storage_preparation_inodes_high_water, 0);
            assert_eq!(counters.storage_preparation_bytes_current_after_cleanup, 0);
            assert_eq!(counters.storage_preparation_inodes_current_after_cleanup, 0);
            assert_eq!(counters.mutable_preparation_residue_bytes, 0);
            assert_eq!(counters.mutable_preparation_residue_inodes, 0);
            assert_eq!(counters.immutable_residue_bytes, 0);
            assert_eq!(counters.immutable_residue_inodes, 0);
            assert!(counters.visibility_lock_acquisitions > 0);
            assert_eq!(counters.publication_lock_acquisitions, 0);
            assert!(counters.has_zero_forbidden_work());
            assert!(fixture.cas.visibility_lock_available_for_test_v1());
            assert!(fixture.cas.publication_lock_available_for_test_v1());
            assert!(fixture.cas.occupied().is_ok());
            assert!(stale.occupied().is_ok());
            assert!(FsCasV1::open_existing(fixture._root.path()).is_ok());

            // Both injected panics are one-shot. Reusing the same sink proves
            // that the explicit abort left no active caller transaction and
            // that the root has no latent poisoned coordination state.
            let (mut comparison, mut path) = read_buffers();
            let mut followup_counters = OperationCountersV1::default();
            extract_root_v1(
                &stale,
                0x4_300 + case_index,
                fixture.version,
                fixture.root_tree,
                &mut sink,
                &mut followup_counters,
                ReadBuffersV1 {
                    comparison: &mut comparison,
                    path: &mut path,
                },
                &mut ContinueControl,
            )
            .expect("a read after explicit sink abort must succeed");
            assert_eq!(sink.began, 2);
            assert_eq!(sink.finished, 1);
            assert_eq!(sink.aborted, 1);
            assert_eq!(fixture.cas.operation_admitted_slots_v1(), 0);
            assert_eq!(fixture.cas.operation_admission_active_for_test_v1(), 0);
            assert_eq!(
                fixture.cas.operation_admission_queue_for_test_v1(),
                (0, 0, 0)
            );
            assert_eq!(
                fixture.cas.storage_admission_active_for_test_v1(),
                (0, 0, 0)
            );
            assert_eq!(preparation_entry_count(&fixture), preparation_before);
            assert!(followup_counters.has_zero_forbidden_work());
        }
    }

    #[test]
    fn admitted_read_visibility_wait_is_cancellable_and_deadline_aware() {
        let fixture = create_fixture("read-visibility-contention");
        let preparation_before = preparation_entry_count(&fixture);
        let visibility = fixture.cas.hold_visibility_lock_for_test_v1();

        for (kind, expected) in [
            (StopKind::Cancel, CoreError::Cancelled),
            (StopKind::Deadline, CoreError::Deadline),
        ] {
            let (mut comparison, mut path) = read_buffers();
            let mut counters = OperationCountersV1::default();
            let mut sink = CaptureSink::new(1);
            let error = extract_root_v1(
                &fixture.cas,
                0x5_000 + expected as u64,
                fixture.version,
                fixture.root_tree,
                &mut sink,
                &mut counters,
                ReadBuffersV1 {
                    comparison: &mut comparison,
                    path: &mut path,
                },
                &mut StopAtVisibilityContention {
                    kind,
                    current: None,
                },
            )
            .expect_err("an admitted read must stop while waiting for visibility");

            assert_eq!(
                error,
                ReadOperationErrorV1::FsCas(FsCasErrorV1::Core(expected))
            );
            assert_eq!(sink.bound_calls.get(), 1);
            assert_eq!(sink.began, 0);
            assert_eq!(counters.fscas_read_calls, 0);
            assert_eq!(counters.fscas_bytes_read, 0);
            assert_eq!(preparation_entry_count(&fixture), preparation_before);
            assert_eq!(fixture.cas.operation_admitted_slots_v1(), 0);
        }

        drop(visibility);
    }

    #[test]
    fn exact_path_component_and_stack_bounds_pass_and_one_over_releases_root_slot() {
        let exact_depth = slash_join(&vec![1; MAX_PATH_DEPTH]);
        assert_eq!(exact_depth.len(), MAX_PATH_DEPTH * 2 - 1);
        assert_single_path_extracts(exact_depth, "depth-boundary");

        let mut lengths = vec![255; 15];
        lengths.extend([254, 1]);
        let exact_bytes = slash_join(&lengths);
        assert_eq!(exact_bytes.len(), MAX_PATH_BYTES);
        assert_single_path_extracts(exact_bytes.clone(), "path-byte-boundary");

        let fixture = create_fixture("path-one-over");
        let invalid_paths = [vec![b'a'; 256], slash_join(&vec![1; MAX_PATH_DEPTH + 1]), {
            let mut one_over = exact_bytes;
            one_over.push(b'a');
            one_over
        }];
        for invalid in invalid_paths {
            let (mut comparison, mut path) = read_buffers();
            let mut counters = OperationCountersV1::default();
            let mut sink = CaptureSink::new(1);
            assert_eq!(
                read_file_range_v1(
                    &fixture.cas,
                    104,
                    fixture.version,
                    fixture.root_tree,
                    &invalid,
                    0,
                    1,
                    &mut sink,
                    &mut counters,
                    ReadBuffersV1 {
                        comparison: &mut comparison,
                        path: &mut path,
                    },
                    &mut ContinueControl,
                ),
                Err(ReadOperationErrorV1::Core(CoreError::Path))
            );
            assert_eq!(fixture.cas.operation_admitted_slots_v1(), 0);
            assert_eq!(sink.began, 0);
        }

        let mut stack = BoundedFullTraversalStackV1::new();
        let frame = FullTraversalFrameV1::File {
            id: PhysicalFileIdV1::from_digest([0; 32]),
            path_len: 0,
            path_depth: 0,
        };
        for _ in 0..crate::object::MAX_CANONICAL_TRAVERSAL_FRAMES_V1 {
            stack.push(frame).expect("exact bounded stack capacity");
        }
        assert_eq!(stack.push(frame), Err(CoreError::CountCap));
    }

    #[test]
    fn first_two_level_index_shape_extracts_in_canonical_order() {
        let entry_count = 18_433;
        let expected = (0..entry_count)
            .map(|index| (format!("f{index:05}").into_bytes(), 0o640, Vec::new()))
            .collect();
        let fixture = create_fixture_from_expected("index-depth-two", expected);
        let (mut comparison, mut path) = read_buffers();
        let mut counters = OperationCountersV1::default();
        let mut sink = CountSink::new();
        let result = extract_root_v1(
            &fixture.cas,
            105,
            fixture.version,
            fixture.root_tree,
            &mut sink,
            &mut counters,
            ReadBuffersV1 {
                comparison: &mut comparison,
                path: &mut path,
            },
            &mut ContinueControl,
        )
        .expect("depth-two index extraction");
        assert_eq!(fixture.cas.operation_admitted_slots_v1(), 0);
        assert_eq!(result.files(), entry_count);
        assert_eq!(sink.files, entry_count);
        assert!(sink.began);
        assert!(sink.finished);
    }

    fn assert_stopped_read(fixture: &CreatedFixture, kind: StopKind, expected: CoreError) {
        let (mut comparison, mut path) = read_buffers();
        let mut counters = OperationCountersV1::default();
        let mut sink = CaptureSink::new(256 * 1024);
        let stopped = Rc::new(Cell::new(false));
        sink.stop_after_begin = Some(Rc::clone(&stopped));
        let error = extract_root_v1(
            &fixture.cas,
            106,
            fixture.version,
            fixture.root_tree,
            &mut sink,
            &mut counters,
            ReadBuffersV1 {
                comparison: &mut comparison,
                path: &mut path,
            },
            &mut StopAfterBeginControl { kind, stopped },
        )
        .expect_err("read must stop");
        assert_eq!(error, ReadOperationErrorV1::Core(expected));
        assert_eq!(fixture.cas.operation_admitted_slots_v1(), 0);
        assert_eq!(sink.began, 1);
        assert_eq!(sink.finished, 0);
        assert_eq!(sink.aborted, 1);
    }

    #[test]
    fn cancellation_deadline_and_corrupt_occupant_are_exact_and_release_the_slot() {
        let fixture = create_fixture("failure");
        assert_stopped_read(&fixture, StopKind::Cancel, CoreError::Cancelled);
        assert_stopped_read(&fixture, StopKind::Deadline, CoreError::Deadline);

        for entry in fs::read_dir(fixture._root.path().join("objects")).expect("object locators") {
            let path = entry.expect("locator entry").path();
            let mut bytes = fs::read(&path).expect("read locator");
            bytes[..8].copy_from_slice(b"CORRUPT!");
            fs::remove_file(&path).expect("remove immutable locator");
            let mut replacement = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)
                .expect("replace locator");
            replacement
                .write_all(&bytes)
                .expect("write corrupt locator");
            let mut permissions = replacement
                .metadata()
                .expect("locator metadata")
                .permissions();
            permissions.set_readonly(true);
            replacement
                .set_permissions(permissions)
                .expect("restore immutable permission");
        }
        let (mut comparison, mut path) = read_buffers();
        let mut counters = OperationCountersV1::default();
        let mut sink = CaptureSink::new(256 * 1024);
        let error = extract_root_v1(
            &fixture.cas,
            107,
            fixture.version,
            fixture.root_tree,
            &mut sink,
            &mut counters,
            ReadBuffersV1 {
                comparison: &mut comparison,
                path: &mut path,
            },
            &mut ContinueControl,
        )
        .expect_err("corrupt occupant must fail");
        assert_eq!(
            error,
            ReadOperationErrorV1::FsCas(FsCasErrorV1::MalformedOccupant)
        );
        assert_eq!(fixture.cas.operation_admitted_slots_v1(), 0);
        assert_eq!(sink.began, 0);
        assert_eq!(sink.finished, 0);
        assert_eq!(sink.aborted, 0);
    }

    #[test]
    fn missing_required_occupant_is_exact_and_releases_the_slot() {
        let fixture = create_fixture("missing-occupant");
        for entry in fs::read_dir(fixture._root.path().join("objects")).expect("object locators") {
            fs::remove_file(entry.expect("locator entry").path()).expect("remove locator");
        }

        let (mut comparison, mut path) = read_buffers();
        let mut counters = OperationCountersV1::default();
        let mut sink = CaptureSink::new(256 * 1024);
        let error = extract_root_v1(
            &fixture.cas,
            108,
            fixture.version,
            fixture.root_tree,
            &mut sink,
            &mut counters,
            ReadBuffersV1 {
                comparison: &mut comparison,
                path: &mut path,
            },
            &mut ContinueControl,
        )
        .expect_err("missing required occupant must fail");
        assert_eq!(
            error,
            ReadOperationErrorV1::FsCas(FsCasErrorV1::MissingOccupant)
        );
        assert_eq!(fixture.cas.operation_admitted_slots_v1(), 0);
        assert_eq!(sink.began, 0);
        assert_eq!(sink.finished, 0);
        assert_eq!(sink.aborted, 0);
    }
}
