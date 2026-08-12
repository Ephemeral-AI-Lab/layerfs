//! CAS admission session for a granted complete-content operation.
//!
//! Lifecycle supplies the already-granted root and preparation ports. This
//! module assembles CAS-owned admission and closure-fence behavior around the
//! pack-owned carrier writer; it cannot reserve slots or publish product state.

use std::cell::RefCell;

use crate::cas::{
    AdmissionBuffersV1, ClosureObjectRecordV1, FileClosureObjectSpoolV1, FileGlobalSeenSpoolV1,
    FsCasClosureSpoolV1, FsCasControlV1, FsCasErrorV1, FsCasOccupiedV1, FsCasV1,
    FsClosureAdmissionErrorV1, FsStorageOperationTokenV1, GlobalSeenErrorV1, GlobalSeenRecordV1,
    OccupiedImmutableReadPortV1, CLOSURE_MARKER_BYTES,
};
use crate::cdc::{CdcAlgorithmV1, CdcControlV1};
use crate::content::{ChunkReferenceSpoolV1, PreparedObjectSinkV1};
use crate::cow::PreparedTreeSinkV1;
use crate::identity::{PhysicalTreeIdV1, PhysicalVersionRecordIdV1, COMPARISON_WINDOW_BYTES};
use crate::lifecycle::{
    BuiltDirectoryRecordV1, BuiltFileRecordV1, FileBuiltDirectorySpoolV1, FileBuiltFileSpoolV1,
    FileChunkReferenceSpoolV1, OperationErrorV1, OperationPreparationV1, SharedOperationControlV1,
    StorageSessionPortV1, VersionSummaryInputV1,
};
use crate::limits::{OperationCountersV1, OperationReservationV1, OptionalU64ObservationV1};
use crate::object::{
    decode_physical_object_from_port_v1, traverse_strong_edges_v1, DiscardStrongEdgesV1,
    PhysicalObjectPayloadV1, PhysicalObjectReadPortV1, StrongEdgeTraversalQueueV1, TreeRecordV1,
    TypedPhysicalObjectIdV1,
};
use crate::pack::{CompletedPackSetV1, DirectPackSinkV1, FilePackIndexSpoolV1, PackReadPortV1};
use crate::{CoreError, CoreResult};

/// Exact storage custody transferred from the authenticated closure fence to
/// the outer lifecycle. A fresh marker remains operation-relative until the
/// synchronous handoff boundary returns; an equal incumbent contributes zero
/// newly installed bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ClosureFenceStorageOutcomeV1 {
    object_count: u64,
    installed_residue_bytes: u64,
}

impl ClosureFenceStorageOutcomeV1 {
    pub(crate) const fn object_count_v1(self) -> u64 {
        self.object_count
    }

    pub(crate) const fn installed_residue_bytes_v1(self) -> u64 {
        self.installed_residue_bytes
    }
}

pub(crate) fn terminalize_failed_closure_marker_v1<C>(
    operation: &mut crate::cas::fs::FsClosureOperationV1,
    counters: &mut OperationCountersV1,
    control: &mut C,
) -> Result<(), FsCasErrorV1>
where
    C: FsCasControlV1 + ?Sized,
{
    let retention = FsCasV1::retain_closure_marker_residue_v1(operation, counters);
    let requires_invalidation = !matches!(retention, Ok(false));
    let invalidation = if requires_invalidation {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            FsCasV1::invalidate_closure_operation_controlled_v1(operation, control)
        })) {
            Ok(result) => result,
            Err(_) => FsCasV1::invalidate_closure_operation_backstop_v1(operation),
        }
    } else {
        Ok(())
    };
    match (retention, invalidation) {
        (Ok(_), Ok(())) => Ok(()),
        (Err(first), Ok(())) => Err(first),
        (Ok(_), Err(dominant)) => Err(dominant),
        (Err(first), Err(dominant)) => Err(first.dominated_by_v1(dominant)),
    }
}

/// Preserve the initiating unwind only when every owned closure-marker
/// terminal action completed successfully. A typed retention or invalidation
/// failure is the operation terminal and must cross the storage boundary as a
/// typed result instead of being replaced by a fabricated panic.
pub(crate) fn terminalize_closure_unwind_v1<C>(
    operation: &mut crate::cas::fs::FsClosureOperationV1,
    counters: &mut OperationCountersV1,
    control: &mut C,
    payload: Box<dyn std::any::Any + Send>,
) -> FsClosureAdmissionErrorV1
where
    C: FsCasControlV1 + ?Sized,
{
    match terminalize_failed_closure_marker_v1(operation, counters, control) {
        Ok(()) => std::panic::resume_unwind(payload),
        Err(terminal) => FsClosureAdmissionErrorV1::FsCas(terminal),
    }
}

/// Authenticate an accepted version/root pair through the real closure marker
/// and occupied-object path while the caller retains the sole root operation
/// capability. This performs no preparation and preserves the first typed
/// FsCas failure instead of flattening it through a content adapter.
pub(crate) fn authenticate_base_root_storage_v1<C>(
    cas: &FsCasV1,
    storage_token: FsStorageOperationTokenV1,
    version_record: PhysicalVersionRecordIdV1,
    expected_root: PhysicalTreeIdV1,
    counters: &mut OperationCountersV1,
    comparison: &mut [u8; COMPARISON_WINDOW_BYTES],
    control: &mut C,
) -> Result<u64, OperationErrorV1>
where
    C: FsCasControlV1 + ?Sized,
{
    let closure = cas.validate_closure_for_read_controlled_borrowed_v1(
        storage_token,
        version_record,
        control,
    )?;
    if closure.version_record() != version_record {
        return Err(CoreError::IdMismatch.into());
    }
    counters.add(
        crate::limits::CounterFieldV1::BytesRead,
        CLOSURE_MARKER_BYTES as u64,
    )?;
    counters.record_fscas_read(CLOSURE_MARKER_BYTES as u64, 1)?;

    let mut occupied = cas.occupied_private_controlled_borrowed_v1(storage_token, control)?;
    let typed = TypedPhysicalObjectIdV1::VersionRecord(version_record);
    let len = match occupied.occupied_len_typed_controlled_v1(typed, control) {
        Ok(Some(len)) => len,
        Ok(None) => {
            occupied.retain_first_error_typed_v1(FsCasErrorV1::MissingOccupant);
            return Err(FsCasErrorV1::MissingOccupant.into());
        }
        Err(error) => {
            occupied.retain_first_error_typed_v1(error);
            return Err(error.into());
        }
    };
    let before = occupied.direct_storage_read_observation_typed_v1()?;
    let decoded = {
        let mut reader = BaseRootOccupiedReaderV1 {
            occupied: &mut occupied,
            counters,
            control,
            id: typed,
            len,
        };
        let mut visitor = DiscardStrongEdgesV1;
        let result = decode_physical_object_from_port_v1(&mut reader, &mut visitor, comparison);
        match result {
            Ok(decoded) => decoded,
            Err(error) => {
                if let Some(storage) = reader.occupied.first_error_typed_v1() {
                    return Err(storage.into());
                }
                return Err(error.into());
            }
        }
    };
    let after = occupied.direct_storage_read_observation_typed_v1()?;
    counters.record_fscas_read(
        after
            .0
            .checked_sub(before.0)
            .ok_or(CoreError::IntegerOverflow)?,
        after
            .1
            .checked_sub(before.1)
            .ok_or(CoreError::IntegerOverflow)?,
    )?;
    if decoded.physical_id() != typed || decoded.header().kind() != typed.kind() {
        return Err(CoreError::IdMismatch.into());
    }
    let PhysicalObjectPayloadV1::VersionRecord(version) = decoded.payload() else {
        return Err(CoreError::TypeDomain.into());
    };
    if version.root_tree_id != expected_root
        || u64::from(version.total_object_count) != closure.object_count()
    {
        return Err(CoreError::IdMismatch.into());
    }
    Ok(closure.object_count())
}

struct BaseRootOccupiedReaderV1<'occupied, 'counters, 'control, C: ?Sized> {
    occupied: &'occupied mut FsCasOccupiedV1,
    counters: &'counters mut OperationCountersV1,
    control: &'control mut C,
    id: TypedPhysicalObjectIdV1,
    len: u64,
}

impl<C: FsCasControlV1 + ?Sized> PhysicalObjectReadPortV1
    for BaseRootOccupiedReaderV1<'_, '_, '_, C>
{
    fn len(&mut self) -> CoreResult<u64> {
        Ok(self.len)
    }

    fn read_exact_at(&mut self, offset: u64, destination: &mut [u8]) -> CoreResult<()> {
        let end = offset
            .checked_add(destination.len() as u64)
            .ok_or(CoreError::IntegerOverflow)?;
        if end > self.len {
            return Err(CoreError::Truncated);
        }
        self.occupied
            .read_occupied_exact_at_typed_controlled_v1(self.id, offset, destination, self.control)
            .map_err(|error| {
                self.occupied.retain_first_error_typed_v1(error);
                CoreError::SourceFailure
            })?;
        self.counters.add(
            crate::limits::CounterFieldV1::BytesRead,
            destination.len() as u64,
        )
    }
}

/// CAS-owned construction of the concrete storage session. The lifecycle
/// owner passes an already-granted root and already-created preparation; this
/// function cannot mint admission or create outer operation state.
#[allow(clippy::too_many_arguments)]
pub(crate) fn begin_storage_session_v1<'operation, 'ledger, 'control, C>(
    cas: &'operation FsCasV1,
    storage_token: FsStorageOperationTokenV1,
    preparation: &'operation mut OperationPreparationV1,
    require_tree_storage: bool,
    left: &'operation mut [u8; COMPARISON_WINDOW_BYTES],
    right: &'operation mut [u8; COMPARISON_WINDOW_BYTES],
    maximum_records: u32,
    private_pack_resident_bound: u64,
    reservation: &'operation OperationReservationV1<'ledger>,
    control: &'operation RefCell<&'control mut C>,
) -> Result<StorageSessionV1<'operation, 'ledger, 'control, C>, FsCasErrorV1>
where
    C: CdcControlV1 + FsCasControlV1 + ?Sized,
{
    let (
        references,
        metadata,
        closure_objects,
        global_seen,
        locator_receipts,
        built_files,
        built_directories,
    ) = preparation.parts_mut();
    if require_tree_storage != (built_files.is_some() && built_directories.is_some()) {
        return Err(FsCasErrorV1::Core(CoreError::Schema));
    }
    let occupied_resident = cas.occupied_resident_memory_bound_v1()?;
    let occupied = cas.occupied_private_borrowed_v1(storage_token)?;
    if occupied.resident_memory_bound_bytes()? > occupied_resident {
        return Err(FsCasErrorV1::Core(CoreError::ResourceRefused));
    }
    let mut shared_control = SharedOperationControlV1::new(control);
    let mut private_pack =
        cas.begin_private_pack_borrowed_controlled_v1(storage_token, &mut shared_control)?;
    if private_pack.resident_memory_bound_bytes()? > private_pack_resident_bound {
        let cleanup = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            private_pack.cleanup_controlled_v1(&mut shared_control)
        }));
        match cleanup {
            Ok(result) => result?,
            Err(_) => {
                // FsPrivatePack retains the complete cleanup/invalidation
                // terminal before rethrowing. At this construction boundary
                // there is no session value to return to lifecycle, so surface
                // that stable terminal instead of synthesizing a cleanup-only
                // result or allowing the partial session to escape through
                // Drop.
                return Err(private_pack.retained_cleanup_terminal_v1().unwrap_or(
                    FsCasErrorV1::CleanupFailed(crate::cas::FsCasCleanupTargetV1::PrivatePack),
                ));
            }
        }
        return Err(FsCasErrorV1::Core(CoreError::ResourceRefused));
    }
    let sink = DirectPackSinkV1::new(
        cas,
        storage_token,
        private_pack,
        metadata,
        closure_objects,
        global_seen,
        locator_receipts,
        occupied,
        left,
        right,
        maximum_records,
        private_pack_resident_bound,
        reservation,
        control,
    );
    Ok(StorageSessionV1 {
        references,
        built_files,
        built_directories,
        sink,
    })
}

/// CAS-owned complete-closure validation and consumed-handoff fence. The
/// lifecycle owner retains the sole outer operation capability throughout.
#[allow(clippy::too_many_arguments)]
pub(crate) fn complete_closure_fence_storage_v1<C>(
    cas: &FsCasV1,
    storage_token: FsStorageOperationTokenV1,
    preparation: &mut OperationPreparationV1,
    root: TypedPhysicalObjectIdV1,
    reservation: &OperationReservationV1<'_>,
    counters: &mut OperationCountersV1,
    buffers: AdmissionBuffersV1<'_>,
    algorithm: CdcAlgorithmV1,
    control: &mut C,
) -> Result<ClosureFenceStorageOutcomeV1, FsClosureAdmissionErrorV1>
where
    C: FsCasControlV1 + ?Sized,
{
    let closure_objects = preparation.closure_objects_for_fence_mut();
    let occupied = cas
        .occupied_private_borrowed_v1(storage_token)
        .map_err(FsClosureAdmissionErrorV1::FsCas)?;
    let mut closure = FsCasClosureSpoolV1::new(closure_objects, occupied);
    let mut closure_operation = cas
        .begin_closure_operation_borrowed_controlled_v1(storage_token, control)
        .map_err(FsClosureAdmissionErrorV1::FsCas)?;
    let closure_terminal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let (admitted, mut capability) = cas.admit_complete_closure_borrowed_v1(
            &mut closure_operation,
            &mut closure,
            storage_token,
            root,
            reservation,
            counters,
            buffers,
            algorithm,
            control,
        )?;
        cas.consume_validated_closure_for_handoff_controlled_v1(
            &mut closure_operation,
            &mut capability,
            control,
        )
        .map_err(FsClosureAdmissionErrorV1::FsCas)?;
        Ok::<u64, FsClosureAdmissionErrorV1>(admitted.object_count())
    }));
    match closure_terminal {
        Ok(Ok(object_count)) => Ok(ClosureFenceStorageOutcomeV1 {
            object_count,
            installed_residue_bytes: FsCasV1::take_closure_marker_residue_bytes_v1(
                &mut closure_operation,
            ),
        }),
        Ok(Err(error)) => {
            let original = closure
                .take_first_error_typed_v1()
                .map(FsClosureAdmissionErrorV1::FsCas)
                .unwrap_or(error);
            let terminalization =
                terminalize_failed_closure_marker_v1(&mut closure_operation, counters, control);
            match terminalization {
                Ok(()) => Err(original),
                Err(terminal) => Err(FsClosureAdmissionErrorV1::FsCas(match original {
                    FsClosureAdmissionErrorV1::Core(error) => {
                        FsCasErrorV1::Core(error).dominated_by_v1(terminal)
                    }
                    FsClosureAdmissionErrorV1::FsCas(error) => error.dominated_by_v1(terminal),
                })),
            }
        }
        Err(payload) => Err(terminalize_closure_unwind_v1(
            &mut closure_operation,
            counters,
            control,
            payload,
        )),
    }
}

pub(crate) struct StorageSessionV1<'operation, 'ledger, 'control, C: ?Sized> {
    references: &'operation mut FileChunkReferenceSpoolV1,
    built_files: Option<&'operation mut FileBuiltFileSpoolV1>,
    built_directories: Option<&'operation mut FileBuiltDirectorySpoolV1>,
    sink: DirectPackSinkV1<'operation, 'ledger, 'control, FilePackIndexSpoolV1, C>,
}

impl<C> StorageSessionPortV1 for StorageSessionV1<'_, '_, '_, C>
where
    C: CdcControlV1 + FsCasControlV1 + ?Sized,
{
    fn content_parts_v1(
        &mut self,
    ) -> (
        &mut (dyn ChunkReferenceSpoolV1 + '_),
        &mut (dyn PreparedObjectSinkV1 + '_),
    ) {
        (self.references, &mut self.sink)
    }

    fn tree_sink_v1(&mut self) -> &mut (dyn PreparedTreeSinkV1 + '_) {
        &mut self.sink
    }

    fn reference_storage_bytes_v1(&self) -> CoreResult<OptionalU64ObservationV1> {
        self.references.storage_bytes_observation()
    }

    fn push_built_file_v1(&mut self, record: BuiltFileRecordV1) -> CoreResult<()> {
        self.built_files
            .as_deref_mut()
            .ok_or(CoreError::Schema)?
            .push(record)
    }

    fn read_built_file_v1(&mut self, ordinal: u32) -> CoreResult<BuiltFileRecordV1> {
        self.built_files
            .as_deref_mut()
            .ok_or(CoreError::Schema)?
            .read(ordinal)
    }

    fn push_built_directory_v1(&mut self, record: BuiltDirectoryRecordV1) -> CoreResult<()> {
        self.built_directories
            .as_deref_mut()
            .ok_or(CoreError::Schema)?
            .push(record)
    }

    fn built_version_summary_v1(
        &mut self,
        canonical_len: u64,
        counters: &mut OperationCountersV1,
        control: &mut dyn FsCasControlV1,
    ) -> CoreResult<VersionSummaryInputV1> {
        let file_stats = self
            .built_files
            .as_deref_mut()
            .ok_or(CoreError::Schema)?
            .sort_unique_stats(control, counters)?;
        let entry_count = self
            .built_directories
            .as_deref_mut()
            .ok_or(CoreError::Schema)?
            .sort_unique_entry_count(control, counters)?;
        Ok(VersionSummaryInputV1::new(
            canonical_len,
            file_stats.logical_file_bytes,
            entry_count,
            file_stats.extent_count,
            file_stats.chunk_ref_count,
        ))
    }

    fn rebuild_candidate_closure_v1(
        &mut self,
        root_tree: PhysicalTreeIdV1,
        counters: &mut OperationCountersV1,
        control: &mut dyn FsCasControlV1,
    ) -> Result<VersionSummaryInputV1, OperationErrorV1> {
        self.sink.flush_changed_objects_for_candidate_v1()?;
        let (closure, seen, occupied, comparison) = self.sink.candidate_graph_parts_v1();
        rebuild_candidate_graph_v1(
            closure, seen, occupied, comparison, root_tree, counters, control,
        )
    }

    fn write_version_v1(
        &mut self,
        version_id: crate::identity::VersionIdV1,
        root_tree: PhysicalTreeIdV1,
        summary: VersionSummaryInputV1,
        counters: &mut OperationCountersV1,
    ) -> CoreResult<PhysicalVersionRecordIdV1> {
        self.sink
            .write_version_v1(version_id, root_tree, summary, counters)
    }

    fn complete_v1(
        &mut self,
        expected_version: PhysicalVersionRecordIdV1,
    ) -> CoreResult<CompletedPackSetV1> {
        self.sink.complete_v1(expected_version)
    }

    fn record_incomplete_residue_v1(&mut self) -> CoreResult<()> {
        self.sink.record_incomplete_residue()
    }

    fn cleanup_private_pack_controlled_v1(&mut self) -> Result<(), FsCasErrorV1> {
        self.sink.cleanup_private_pack_controlled_v1()
    }

    fn take_first_core_error_v1(&mut self) -> Option<CoreError> {
        self.sink.take_first_core_error()
    }

    fn take_first_fscas_error_v1(&mut self) -> Option<FsCasErrorV1> {
        self.sink.take_first_fscas_error()
    }

    fn record_global_seen_observation_v1(&mut self) -> CoreResult<()> {
        self.sink.record_global_seen_observation()
    }

    fn take_storage_counters_v1(&mut self) -> OperationCountersV1 {
        self.sink.take_storage_counters()
    }
}

#[derive(Clone, Copy, Default)]
struct CandidateGraphSummaryV1 {
    canonical_len: u64,
    logical_file_bytes: u64,
    entry_count: u64,
    extent_count: u64,
    chunk_ref_count: u64,
}

impl CandidateGraphSummaryV1 {
    fn observe(&mut self, payload: PhysicalObjectPayloadV1) -> CoreResult<()> {
        match payload {
            PhysicalObjectPayloadV1::VersionRecord(_) => return Err(CoreError::TypedEdge),
            PhysicalObjectPayloadV1::Tree(TreeRecordV1::Directory(directory)) => {
                self.entry_count = self
                    .entry_count
                    .checked_add(u64::from(directory.entry_count))
                    .ok_or(CoreError::IntegerOverflow)?;
                crate::format::validate_entry_count(self.entry_count)?;
            }
            PhysicalObjectPayloadV1::Tree(_) => {}
            PhysicalObjectPayloadV1::File(file) => {
                self.canonical_len = self
                    .canonical_len
                    .checked_add(file.logical_len)
                    .ok_or(CoreError::IntegerOverflow)?;
                self.logical_file_bytes = self
                    .logical_file_bytes
                    .checked_add(file.logical_len)
                    .ok_or(CoreError::IntegerOverflow)?;
                self.extent_count = self
                    .extent_count
                    .checked_add(u64::from(file.extent_count))
                    .ok_or(CoreError::IntegerOverflow)?;
                self.chunk_ref_count = self
                    .chunk_ref_count
                    .checked_add(file.chunk_ref_count)
                    .ok_or(CoreError::IntegerOverflow)?;
                crate::format::validate_logical_length(self.canonical_len)?;
                crate::format::validate_logical_length(self.logical_file_bytes)?;
                crate::format::validate_extents_per_version(self.extent_count)?;
                crate::format::validate_chunk_refs_per_version(self.chunk_ref_count)?;
            }
            PhysicalObjectPayloadV1::Symlink(_) | PhysicalObjectPayloadV1::Chunk(_) => {}
        }
        Ok(())
    }

    fn finish(self) -> CoreResult<VersionSummaryInputV1> {
        Ok(VersionSummaryInputV1::new(
            self.canonical_len,
            self.logical_file_bytes,
            u32::try_from(self.entry_count).map_err(|_| CoreError::IntegerOverflow)?,
            u32::try_from(self.extent_count).map_err(|_| CoreError::IntegerOverflow)?,
            u32::try_from(self.chunk_ref_count).map_err(|_| CoreError::IntegerOverflow)?,
        ))
    }
}

struct CandidateOccupiedReaderV1<'occupied, 'cell, 'control, 'counters> {
    occupied: &'occupied mut FsCasOccupiedV1,
    control: &'cell RefCell<&'control mut dyn FsCasControlV1>,
    counters: &'counters mut OperationCountersV1,
    id: TypedPhysicalObjectIdV1,
    len: u64,
}

impl PhysicalObjectReadPortV1 for CandidateOccupiedReaderV1<'_, '_, '_, '_> {
    fn len(&mut self) -> CoreResult<u64> {
        Ok(self.len)
    }

    fn read_exact_at(&mut self, offset: u64, destination: &mut [u8]) -> CoreResult<()> {
        let mut shared_control = CandidateSharedControlV1 {
            inner: self.control,
        };
        self.occupied
            .read_occupied_exact_at_typed_controlled_v1(
                self.id,
                offset,
                destination,
                &mut shared_control,
            )
            .map_err(|error| {
                self.occupied.retain_first_error_typed_v1(error);
                CoreError::SourceFailure
            })?;
        self.counters.add(
            crate::limits::CounterFieldV1::BytesRead,
            destination.len() as u64,
        )
    }
}

struct CandidateTraversalQueueV1<'closure, 'seen, 'cell, 'control, 'errors> {
    closure: &'closure mut FileClosureObjectSpoolV1,
    seen: &'seen mut FileGlobalSeenSpoolV1,
    control: &'cell RefCell<&'control mut dyn FsCasControlV1>,
    first_error: &'errors RefCell<Option<OperationErrorV1>>,
}

impl CandidateTraversalQueueV1<'_, '_, '_, '_, '_> {
    fn retain_operation_error(&self, terminal: OperationErrorV1) -> CoreError {
        self.first_error.borrow_mut().get_or_insert(terminal);
        map_operation_to_core_v1(terminal)
    }

    fn retain_closure_error(&mut self, error: CoreError) -> CoreError {
        let terminal = self
            .closure
            .take_first_error()
            .map(OperationErrorV1::FsCas)
            .unwrap_or(OperationErrorV1::Core(error));
        self.retain_operation_error(terminal)
    }
}

impl StrongEdgeTraversalQueueV1 for CandidateTraversalQueueV1<'_, '_, '_, '_, '_> {
    fn enqueue_if_new_v1(&mut self, id: TypedPhysicalObjectIdV1) -> CoreResult<()> {
        let mut shared_control = CandidateSharedControlV1 {
            inner: self.control,
        };
        let lookup = self.seen.lookup(id, &mut shared_control).map_err(|error| {
            self.retain_operation_error(map_global_seen_operation_error_v1(error))
        })?;
        if lookup.record.is_some() {
            return Ok(());
        }
        self.seen
            .insert_controlled_v1(
                lookup.vacant_slot,
                id,
                GlobalSeenRecordV1 {
                    complete_len: crate::object::OBJECT_HEADER_BYTES,
                    private_payload_offset: 0,
                    carrier_ordinal: u32::MAX,
                },
                &mut shared_control,
            )
            .map_err(|error| {
                self.retain_operation_error(map_global_seen_operation_error_v1(error))
            })?;
        let next_count = u64::from(self.closure.count)
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        crate::format::validate_total_object_count(next_count)?;
        self.closure.push(ClosureObjectRecordV1::pending(id))
    }

    fn pending_count_v1(&mut self) -> CoreResult<u32> {
        Ok(self.closure.count)
    }

    fn pending_id_v1(&mut self, ordinal: u32) -> CoreResult<TypedPhysicalObjectIdV1> {
        let pending = self
            .closure
            .read(ordinal)
            .map_err(|error| self.retain_closure_error(error))?;
        if !pending.is_pending() {
            return Err(CoreError::IdMismatch);
        }
        Ok(pending.id)
    }

    fn complete_pending_v1(&mut self, ordinal: u32, complete_len: u64) -> CoreResult<()> {
        self.closure
            .complete_pending(ordinal, complete_len)
            .map_err(|error| self.retain_closure_error(error))
    }
}

struct CandidateSharedControlV1<'cell, 'control> {
    inner: &'cell RefCell<&'control mut dyn FsCasControlV1>,
}

impl FsCasControlV1 for CandidateSharedControlV1<'_, '_> {
    fn boundary_reached(&mut self, boundary: crate::cas::FsCasBoundaryV1) {
        (**self.inner.borrow_mut()).boundary_reached(boundary);
    }

    fn cancellation_requested(&mut self) -> bool {
        (**self.inner.borrow_mut()).cancellation_requested()
    }

    fn deadline_exceeded(&mut self) -> bool {
        (**self.inner.borrow_mut()).deadline_exceeded()
    }

    fn inject_cleanup_failure(&mut self, target: crate::cas::FsCasCleanupTargetV1) -> bool {
        (**self.inner.borrow_mut()).inject_cleanup_failure(target)
    }

    fn inject_filesystem_failure(
        &mut self,
        boundary: crate::cas::FsCasFilesystemBoundaryV1,
    ) -> Option<crate::cas::FsCasErrorV1> {
        (**self.inner.borrow_mut()).inject_filesystem_failure(boundary)
    }

    #[cfg(test)]
    fn inject_residue_accounting_failure(
        &mut self,
        boundary: crate::cas::FsCasResidueAccountingBoundaryV1,
    ) -> bool {
        (**self.inner.borrow_mut()).inject_residue_accounting_failure(boundary)
    }

    #[cfg(test)]
    fn before_carrier_no_replace_transition_for_test_v1(&mut self) {
        (**self.inner.borrow_mut()).before_carrier_no_replace_transition_for_test_v1();
    }

    #[cfg(test)]
    fn inject_operation_terminal_unwind_after_release(&mut self) -> bool {
        (**self.inner.borrow_mut()).inject_operation_terminal_unwind_after_release()
    }

    #[cfg(test)]
    fn inject_root_lock_observation_failure(&mut self) -> Option<CoreError> {
        (**self.inner.borrow_mut()).inject_root_lock_observation_failure()
    }
}

#[allow(clippy::too_many_arguments)]
fn rebuild_candidate_graph_v1(
    closure: &mut FileClosureObjectSpoolV1,
    seen: &mut FileGlobalSeenSpoolV1,
    occupied: &mut FsCasOccupiedV1,
    comparison: &mut [u8; COMPARISON_WINDOW_BYTES],
    root_tree: PhysicalTreeIdV1,
    counters: &mut OperationCountersV1,
    control: &mut dyn FsCasControlV1,
) -> Result<VersionSummaryInputV1, OperationErrorV1> {
    closure.clear_for_candidate_graph_v1().map_err(|error| {
        closure
            .take_first_error()
            .map(OperationErrorV1::FsCas)
            .unwrap_or(OperationErrorV1::Core(error))
    })?;
    seen.reset_for_candidate_graph_controlled_v1(control)
        .map_err(map_global_seen_operation_error_v1)?;

    let control_cell = RefCell::new(control);
    let before = occupied.direct_storage_read_observation_typed_v1()?;
    let mut summary = CandidateGraphSummaryV1::default();
    let first_error = RefCell::new(None);
    let mut queue = CandidateTraversalQueueV1 {
        closure,
        seen,
        control: &control_cell,
        first_error: &first_error,
    };
    let traversal = traverse_strong_edges_v1(
        &mut queue,
        TypedPhysicalObjectIdV1::Tree(root_tree),
        |id, visitor| {
            let len = match occupied.occupied_len_typed_controlled_v1(
                id,
                &mut CandidateSharedControlV1 {
                    inner: &control_cell,
                },
            ) {
                Ok(Some(len)) => len,
                Ok(None) => {
                    let terminal = OperationErrorV1::FsCas(FsCasErrorV1::MissingOccupant);
                    occupied.retain_first_error_typed_v1(FsCasErrorV1::MissingOccupant);
                    first_error.borrow_mut().get_or_insert(terminal);
                    return Err(map_operation_to_core_v1(terminal));
                }
                Err(error) => {
                    occupied.retain_first_error_typed_v1(error);
                    let terminal = OperationErrorV1::FsCas(error);
                    first_error.borrow_mut().get_or_insert(terminal);
                    return Err(map_operation_to_core_v1(terminal));
                }
            };
            let mut reader = CandidateOccupiedReaderV1 {
                occupied,
                control: &control_cell,
                counters,
                id,
                len,
            };
            let result = decode_physical_object_from_port_v1(&mut reader, visitor, comparison);
            match result {
                Ok(decoded) => {
                    if let Some(terminal) = first_error.borrow().as_ref().copied() {
                        return Err(map_operation_to_core_v1(terminal));
                    }
                    if let Some(error) = reader.occupied.first_error_typed_v1() {
                        let terminal = OperationErrorV1::FsCas(error);
                        first_error.borrow_mut().get_or_insert(terminal);
                        return Err(map_operation_to_core_v1(terminal));
                    }
                    if decoded.physical_id() != id || decoded.header().kind() != id.kind() {
                        return Err(CoreError::IdMismatch);
                    }
                    Ok((len, decoded))
                }
                Err(error) => {
                    if let Some(terminal) = first_error.borrow().as_ref().copied() {
                        return Err(map_operation_to_core_v1(terminal));
                    }
                    if let Some(error) = reader.occupied.first_error_typed_v1() {
                        let terminal = OperationErrorV1::FsCas(error);
                        first_error.borrow_mut().get_or_insert(terminal);
                        return Err(map_operation_to_core_v1(terminal));
                    }
                    Err(error)
                }
            }
        },
        |_ordinal, _id, _len, decoded| summary.observe(decoded.payload()),
        || {
            if (**control_cell.borrow_mut()).cancellation_requested() {
                return Err(CoreError::Cancelled);
            }
            if (**control_cell.borrow_mut()).deadline_exceeded() {
                return Err(CoreError::Deadline);
            }
            Ok(())
        },
    );
    if let Some(terminal) = first_error.into_inner() {
        return Err(terminal);
    }
    traversal.map_err(OperationErrorV1::Core)?;
    let after = occupied.direct_storage_read_observation_typed_v1()?;
    counters.record_fscas_read(
        after
            .0
            .checked_sub(before.0)
            .ok_or(CoreError::IntegerOverflow)?,
        after
            .1
            .checked_sub(before.1)
            .ok_or(CoreError::IntegerOverflow)?,
    )?;
    summary.finish().map_err(OperationErrorV1::Core)
}

fn map_global_seen_operation_error_v1(error: GlobalSeenErrorV1) -> OperationErrorV1 {
    match error {
        GlobalSeenErrorV1::Core(error) => OperationErrorV1::Core(error),
        GlobalSeenErrorV1::FsCas(error) => OperationErrorV1::FsCas(error),
    }
}

fn map_operation_to_core_v1(error: OperationErrorV1) -> CoreError {
    match error {
        OperationErrorV1::Core(error) => error,
        OperationErrorV1::FsCas(FsCasErrorV1::Core(error)) => error,
        OperationErrorV1::FsCas(
            FsCasErrorV1::Unsupported | FsCasErrorV1::Busy | FsCasErrorV1::ResourceExhausted(_),
        ) => CoreError::ResourceRefused,
        OperationErrorV1::FsCas(_) => CoreError::SourceFailure,
    }
}
