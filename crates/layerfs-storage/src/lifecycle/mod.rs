//! Shared complete-content operation lifecycle.
//!
//! This coordinator is the only owner of the preparation-to-handoff state
//! machine used by both one-file and multi-entry Create. Content builders run
//! only inside its borrowed semantic storage session; concrete filesystem,
//! pack, locator, and closure implementations remain below their private
//! ports.

#[cfg(feature = "c3-polymorphism")]
mod preparation;

#[cfg(feature = "c3-polymorphism")]
pub(crate) use preparation::{
    C3OperationPreparationV1, C3PreparationErrorV1, FileBuiltDirectorySpoolV1,
    FileBuiltFileSpoolV1, FileChunkReferenceSpoolV1,
};

use std::cell::RefCell;

use crate::cas::{
    authenticate_base_root_storage_v1, begin_storage_session_v1, complete_closure_fence_storage_v1,
    AdmissionBuffersV1, C3StorageSessionV1, FsCasBoundaryV1, FsCasCleanupTargetV1, FsCasControlV1,
    FsCasErrorV1, FsCasV1, FsClosureAdmissionErrorV1, FsOperationCapabilityV1, FsOperationKindV1,
    FsPackAdmissionOutcomeV1, FsStorageEnvelopeV1, FsStorageOperationTokenV1, CATALOG_MARKER_BYTES,
    CLOSURE_MARKER_BYTES, GLOBAL_SEEN_RECORD_BYTES, PERSISTENT_LOCATOR_BYTES_V1,
};
use crate::cdc::{C3CdcAlgorithmV1, CdcControlV1, MAXIMUM_CHUNK_BYTES};
use crate::content::update::{
    authenticate_base_file_evidence_v1, reencode_file_metadata_c3_borrowed_v1,
    update_file_c3_borrowed_v1, AuthenticatedBaseByteReaderV1, BaseChunkEvidenceSourceV1,
    UpdateBuffersV1, MAX_UPDATE_RESYNCHRONIZATION_BYTES,
};
use crate::content::{
    replace_file_c3_borrowed_v1, ChunkReferenceSpoolV1, ContentBuffersV1, ContentSourceV1,
    PreparedObjectSinkV1,
};
use crate::cow::file::{AuthenticatedBaseFileV1, UpdateRangeV1};
use crate::cow::{
    add_directory_entry_cow_borrowed_v1, move_directory_entry_cow_borrowed_v1,
    mutation_evidence_resident_bytes_v1, mutation_hash_state_bytes_v1, preflight_canonical_tree_v1,
    remove_directory_entry_cow_borrowed_v1, replace_directory_entry_cow_borrowed_v1,
    replace_two_directory_entries_cow_borrowed_v1, replacement_evidence_resident_bytes_v1,
    AuthenticatedTreeMutationEvidenceV1, AuthenticatedTreeReplacementEvidenceV1,
    CanonicalDirectoryTreeV1, CanonicalTreeChildV1, CanonicalTreeEntryV1,
    CanonicalTreeMutationSourceV1, DirectoryLogicalIdentityV1, PreparedTreeSinkV1,
    TreePageSummaryV1, MAX_TREE_OBJECT_BYTES,
};
use crate::format::{
    validate_chunk_refs_per_file, validate_file_mode, validate_logical_length,
    validate_total_object_count, ValidatedComponent,
};
use crate::identity::{
    derive_file_node_v1, derive_version_v1, FileNodeIdV1, PhysicalFileIdV1, PhysicalTreeIdV1,
    PhysicalVersionRecordIdV1, VersionIdV1, COMPARISON_WINDOW_BYTES, IDENTITY_HASHER_BYTES_V1,
};
use crate::limits::{
    MemoryComponentV1, OperationCountersV1, OperationMemoryPlanV1, OperationReservationV1,
    ResourceLedgerV1,
};
use crate::object::{TypedPhysicalObjectIdV1, OBJECT_HEADER_BYTES, VERSION_RECORD_PAYLOAD_BYTES};
use crate::pack::{
    CompletedPackSetV1, SealedPackV1, MAX_PACK_BYTES, MAX_PACK_RECORDS, PACK_HEADER_BYTES,
    PACK_INDEX_ENTRY_BYTES, PACK_TRAILER_BYTES,
};
use crate::{CoreError, CoreResult};

const MUTATION_METADATA_RESERVATION_BYTES_V1: u64 = 1_048_576;
const CHUNK_REFERENCE_PREPARATION_BYTES_V1: u64 = 68;
const CLOSURE_OBJECT_PREPARATION_BYTES_V1: u64 = 48;
const BUILT_FILE_PREPARATION_BYTES_V1: u64 = 80;
const BUILT_DIRECTORY_PREPARATION_BYTES_V1: u64 = 40;
const FILE_EXTENT_CANONICAL_BYTES_V1: u64 = 36;
const FILE_FIXED_CANONICAL_BYTES_V1: u64 = 23;
const PACK_OBJECT_RECORD_BYTES_V1: u64 = 52;
const FIXED_PREPARATION_INODES_V1: u64 = 6;

fn checked_ceil_div_v1(numerator: u64, denominator: u64) -> CoreResult<u64> {
    if denominator == 0 {
        return Err(CoreError::IntegerOverflow);
    }
    numerator
        .checked_add(denominator - 1)
        .map(|value| value / denominator)
        .ok_or(CoreError::IntegerOverflow)
}

/// Conservative canonical payload that one bounded Update may newly stage.
/// The changed CDC stream begins at the predecessor chunk, contains every
/// inserted byte, and may consume the complete frozen resynchronization
/// window before a verified suffix rejoin. It can never exceed the resulting
/// file length. This is storage admission only; it does not authorize a
/// whole-base read or an Update-to-Replace fallback.
fn update_maximum_new_payload_bytes_v1(
    base_len: u64,
    new_len: u64,
    inserted_len: u64,
) -> CoreResult<u64> {
    let predecessor_bytes = base_len.min(MAXIMUM_CHUNK_BYTES as u64);
    inserted_len
        .checked_add(predecessor_bytes)
        .and_then(|bytes| bytes.checked_add(MAX_UPDATE_RESYNCHRONIZATION_BYTES))
        .map(|bytes| bytes.min(new_len))
        .ok_or(CoreError::IntegerOverflow)
}

#[cfg(test)]
mod storage_envelope_tests {
    use super::*;

    #[test]
    fn update_payload_envelope_covers_predecessor_insert_and_rejoin_window() {
        let window = MAXIMUM_CHUNK_BYTES as u64 + MAX_UPDATE_RESYNCHRONIZATION_BYTES;
        assert_eq!(
            update_maximum_new_payload_bytes_v1(1_000_000, 1_000_000, 7),
            Ok(window + 7)
        );
        assert_eq!(
            update_maximum_new_payload_bytes_v1(1_000_000, 64 * 1_024, 7),
            Ok(64 * 1_024)
        );
        assert_eq!(
            update_maximum_new_payload_bytes_v1(0, 4_096, 4_096),
            Ok(4_096)
        );
        assert_eq!(
            update_maximum_new_payload_bytes_v1(u64::MAX, u64::MAX, u64::MAX),
            Err(CoreError::IntegerOverflow)
        );
    }
}

/// Checked root-wide logical namespace envelope for one complete content
/// operation. This is deliberately conservative language-level accounting:
/// it is neither allocated filesystem blocks nor free-space/quota discovery.
/// The operation-wide closure population may include occupied base objects,
/// while only `maximum_new_objects` and their canonical bytes can become new
/// immutable namespace state.
#[allow(clippy::too_many_arguments)]
pub(crate) fn c3_storage_envelope_v1(
    maximum_candidate_objects: u64,
    maximum_new_objects: u64,
    maximum_chunk_references: u64,
    maximum_files: u64,
    maximum_tree_objects: u64,
    maximum_logical_payload_bytes: u64,
    global_seen_capacity: u32,
    require_tree_storage: bool,
) -> CoreResult<FsStorageEnvelopeV1> {
    if maximum_new_objects > maximum_candidate_objects {
        return Err(CoreError::CountCap);
    }

    let maximum_pack_records = maximum_new_objects.min(MAX_PACK_RECORDS);
    let reference_preparation = maximum_chunk_references
        .checked_mul(CHUNK_REFERENCE_PREPARATION_BYTES_V1)
        .ok_or(CoreError::IntegerOverflow)?;
    let index_preparation = maximum_pack_records
        .checked_mul(PACK_INDEX_ENTRY_BYTES)
        .ok_or(CoreError::IntegerOverflow)?;
    let closure_preparation = maximum_candidate_objects
        .checked_mul(CLOSURE_OBJECT_PREPARATION_BYTES_V1)
        .ok_or(CoreError::IntegerOverflow)?;
    let seen_preparation = u64::from(global_seen_capacity)
        .checked_mul(GLOBAL_SEEN_RECORD_BYTES)
        .ok_or(CoreError::IntegerOverflow)?;
    let tree_preparation = if require_tree_storage {
        maximum_files
            .checked_mul(BUILT_FILE_PREPARATION_BYTES_V1)
            .and_then(|bytes| {
                maximum_tree_objects
                    .checked_mul(BUILT_DIRECTORY_PREPARATION_BYTES_V1)
                    .and_then(|directories| bytes.checked_add(directories))
            })
            .ok_or(CoreError::IntegerOverflow)?
    } else {
        0
    };
    let marker_preparation = u64::try_from(
        PERSISTENT_LOCATOR_BYTES_V1
            .max(CATALOG_MARKER_BYTES)
            .max(CLOSURE_MARKER_BYTES),
    )
    .map_err(|_| CoreError::IntegerOverflow)?;
    let preparation_bytes = reference_preparation
        .checked_add(index_preparation)
        .and_then(|bytes| bytes.checked_add(closure_preparation))
        .and_then(|bytes| bytes.checked_add(seen_preparation))
        .and_then(|bytes| bytes.checked_add(tree_preparation))
        .and_then(|bytes| bytes.checked_add(MAX_PACK_BYTES))
        .and_then(|bytes| bytes.checked_add(marker_preparation))
        .ok_or(CoreError::IntegerOverflow)?;

    let file_metadata = maximum_chunk_references
        .checked_mul(FILE_EXTENT_CANONICAL_BYTES_V1)
        .and_then(|bytes| {
            maximum_files
                .checked_mul(OBJECT_HEADER_BYTES + FILE_FIXED_CANONICAL_BYTES_V1)
                .and_then(|files| bytes.checked_add(files))
        })
        .ok_or(CoreError::IntegerOverflow)?;
    let tree_metadata = maximum_tree_objects
        .checked_mul(MAX_TREE_OBJECT_BYTES as u64)
        .ok_or(CoreError::IntegerOverflow)?;
    let version_metadata = OBJECT_HEADER_BYTES
        .checked_add(VERSION_RECORD_PAYLOAD_BYTES)
        .ok_or(CoreError::IntegerOverflow)?;
    let canonical_bytes = maximum_logical_payload_bytes
        .checked_add(file_metadata)
        .and_then(|bytes| bytes.checked_add(tree_metadata))
        .and_then(|bytes| bytes.checked_add(version_metadata))
        .ok_or(CoreError::IntegerOverflow)?;
    let pack_records = maximum_new_objects
        .checked_mul(PACK_OBJECT_RECORD_BYTES_V1 + PACK_INDEX_ENTRY_BYTES)
        .ok_or(CoreError::IntegerOverflow)?;
    let pack_content_bytes = canonical_bytes
        .checked_add(pack_records)
        .ok_or(CoreError::IntegerOverflow)?;
    let carriers_by_bytes = checked_ceil_div_v1(pack_content_bytes, MAX_PACK_BYTES / 2)?.max(1);
    let carriers_by_records = checked_ceil_div_v1(maximum_new_objects, MAX_PACK_RECORDS)?.max(1);
    // Complete mutations make changed candidate objects readable before
    // rebuilding the candidate closure, then write the version record into a
    // fresh carrier.  That mandatory visibility boundary can add one carrier
    // beyond aggregate byte/record packing whenever a non-version object may
    // be new.  Keep the bound conservative without inventing an empty extra
    // carrier for the version-only case.
    let forced_version_carrier = u64::from(maximum_new_objects > 1);
    let maximum_carriers = carriers_by_bytes
        .max(carriers_by_records)
        .checked_add(forced_version_carrier)
        .ok_or(CoreError::IntegerOverflow)?
        .min(maximum_new_objects.max(1));
    let carrier_framing = maximum_carriers
        .checked_mul(PACK_HEADER_BYTES + PACK_TRAILER_BYTES)
        .ok_or(CoreError::IntegerOverflow)?;
    let locator_bytes = maximum_new_objects
        .checked_mul(
            u64::try_from(PERSISTENT_LOCATOR_BYTES_V1).map_err(|_| CoreError::IntegerOverflow)?,
        )
        .ok_or(CoreError::IntegerOverflow)?;
    let catalog_bytes = maximum_carriers
        .checked_mul(u64::try_from(CATALOG_MARKER_BYTES).map_err(|_| CoreError::IntegerOverflow)?)
        .ok_or(CoreError::IntegerOverflow)?;
    let immutable_bytes = pack_content_bytes
        .checked_add(carrier_framing)
        .and_then(|bytes| bytes.checked_add(locator_bytes))
        .and_then(|bytes| bytes.checked_add(catalog_bytes))
        .and_then(|bytes| bytes.checked_add(CLOSURE_MARKER_BYTES as u64))
        .ok_or(CoreError::IntegerOverflow)?;
    let preparation_inodes = FIXED_PREPARATION_INODES_V1
        .checked_add(u64::from(require_tree_storage) * 2)
        .ok_or(CoreError::IntegerOverflow)?;
    let immutable_inodes = maximum_new_objects
        .checked_add(
            maximum_carriers
                .checked_mul(2)
                .ok_or(CoreError::IntegerOverflow)?,
        )
        .and_then(|inodes| inodes.checked_add(2))
        .ok_or(CoreError::IntegerOverflow)?;

    FsStorageEnvelopeV1::new(
        preparation_bytes,
        immutable_bytes,
        preparation_inodes,
        immutable_inodes,
    )
}

fn candidate_traversal_bytes_v1(maximum_objects: u64) -> CoreResult<usize> {
    usize::try_from(maximum_objects)
        .map_err(|_| CoreError::IntegerOverflow)?
        .checked_mul(2)
        .and_then(|bits| bits.checked_add(7))
        .map(|bits| bits / 8)
        .ok_or(CoreError::IntegerOverflow)
}

fn candidate_global_seen_capacity_v1(maximum_objects: u64) -> CoreResult<u32> {
    let required = maximum_objects
        .checked_mul(2)
        .ok_or(CoreError::IntegerOverflow)?
        .max(8);
    let capacity = required
        .checked_next_power_of_two()
        .ok_or(CoreError::IntegerOverflow)?;
    u32::try_from(capacity).map_err(|_| CoreError::CountCap)
}

fn check_lifecycle_control_v1<C: C3LifecycleControlV1 + ?Sized>(control: &mut C) -> CoreResult<()> {
    if FsCasControlV1::cancellation_requested(control) {
        Err(CoreError::Cancelled)
    } else if FsCasControlV1::deadline_exceeded(control) {
        Err(CoreError::Deadline)
    } else {
        Ok(())
    }
}

pub(crate) const C3_MAX_STORAGE_RECORDS_V1: u64 = crate::pack::MAX_PACK_RECORDS;

pub(crate) fn c3_admission_traversal_resident_bytes_v1() -> CoreResult<u64> {
    crate::cas::admission_traversal_resident_bytes_v1()
}

/// Lifecycle-level control contract. Content code does not depend on the
/// concrete filesystem CAS control surface; the root lifecycle adapter is the
/// sole bridge to both CDC and durable storage cancellation/fault boundaries.
pub(crate) trait C3LifecycleControlV1: CdcControlV1 + FsCasControlV1 {}

impl<T> C3LifecycleControlV1 for T where T: CdcControlV1 + FsCasControlV1 + ?Sized {}

pub(crate) struct SharedC3ControlV1<'cell, 'control, C: ?Sized> {
    inner: &'cell RefCell<&'control mut C>,
}

impl<'cell, 'control, C: ?Sized> SharedC3ControlV1<'cell, 'control, C> {
    pub(crate) fn new(inner: &'cell RefCell<&'control mut C>) -> Self {
        Self { inner }
    }
}

impl<C: CdcControlV1 + ?Sized> CdcControlV1 for SharedC3ControlV1<'_, '_, C> {
    fn cancellation_requested(&mut self) -> bool {
        (**self.inner.borrow_mut()).cancellation_requested()
    }

    fn deadline_exceeded(&mut self) -> bool {
        (**self.inner.borrow_mut()).deadline_exceeded()
    }
}

impl<C: FsCasControlV1 + ?Sized> FsCasControlV1 for SharedC3ControlV1<'_, '_, C> {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        (**self.inner.borrow_mut()).boundary_reached(boundary);
    }

    fn cancellation_requested(&mut self) -> bool {
        (**self.inner.borrow_mut()).cancellation_requested()
    }

    fn deadline_exceeded(&mut self) -> bool {
        (**self.inner.borrow_mut()).deadline_exceeded()
    }

    fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
        (**self.inner.borrow_mut()).inject_cleanup_failure(target)
    }

    fn inject_filesystem_failure(
        &mut self,
        boundary: crate::cas::FsCasFilesystemBoundaryV1,
    ) -> Option<crate::cas::FsCasErrorV1> {
        (**self.inner.borrow_mut()).inject_filesystem_failure(boundary)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct C3PreparationResidentBoundsV1 {
    pub(crate) references: u64,
    pub(crate) metadata: u64,
    pub(crate) closure_objects: u64,
    pub(crate) global_seen: u64,
    pub(crate) built_files: Option<u64>,
    pub(crate) built_directories: Option<u64>,
}

/// Opaque lower-storage residency declaration consumed by the shared
/// lifecycle. Content orchestration may charge the aggregate, but cannot name
/// or construct concrete preparation, carrier, locator, or closure adapters.
#[derive(Clone, Copy)]
pub(crate) struct C3StorageResidentPlanV1 {
    preparation: C3PreparationResidentBoundsV1,
    private_storage: u64,
    total: u64,
}

impl C3StorageResidentPlanV1 {
    pub(crate) const fn total_resident_bytes_v1(self) -> u64 {
        self.total
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BuiltFileRecordV1 {
    pub(crate) logical: FileNodeIdV1,
    pub(crate) physical: PhysicalFileIdV1,
    pub(crate) logical_len: u64,
    pub(crate) chunk_count: u32,
    pub(crate) extent_count: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BuiltDirectoryRecordV1 {
    pub(crate) physical: PhysicalTreeIdV1,
    pub(crate) entry_count: u32,
}

#[derive(Clone, Copy)]
pub(crate) struct VersionSummaryInputV1 {
    pub(crate) canonical_len: u64,
    pub(crate) logical_file_bytes: u64,
    pub(crate) entry_count: u32,
    pub(crate) extent_count: u32,
    pub(crate) chunk_ref_count: u32,
}

impl VersionSummaryInputV1 {
    pub(crate) const fn new(
        canonical_len: u64,
        logical_file_bytes: u64,
        entry_count: u32,
        extent_count: u32,
        chunk_ref_count: u32,
    ) -> Self {
        Self {
            canonical_len,
            logical_file_bytes,
            entry_count,
            extent_count,
            chunk_ref_count,
        }
    }
}

/// Narrow lifecycle storage port borrowed by content orchestration. It
/// deliberately exposes no filesystem, carrier, locator, or closure types.
pub(crate) trait C3StorageSessionPortV1 {
    fn content_parts_v1(
        &mut self,
    ) -> (
        &mut (dyn ChunkReferenceSpoolV1 + '_),
        &mut (dyn PreparedObjectSinkV1 + '_),
    );
    fn tree_sink_v1(&mut self) -> &mut (dyn PreparedTreeSinkV1 + '_);
    fn reference_storage_bytes_v1(&self) -> CoreResult<Option<u64>>;
    fn push_built_file_v1(&mut self, record: BuiltFileRecordV1) -> CoreResult<()>;
    fn read_built_file_v1(&mut self, ordinal: u32) -> CoreResult<BuiltFileRecordV1>;
    fn push_built_directory_v1(&mut self, record: BuiltDirectoryRecordV1) -> CoreResult<()>;
    fn built_version_summary_v1(
        &mut self,
        canonical_len: u64,
        counters: &mut OperationCountersV1,
        control: &mut dyn FsCasControlV1,
    ) -> CoreResult<VersionSummaryInputV1>;
    fn rebuild_candidate_closure_v1(
        &mut self,
        root_tree: PhysicalTreeIdV1,
        counters: &mut OperationCountersV1,
        control: &mut dyn FsCasControlV1,
    ) -> Result<VersionSummaryInputV1, C3OperationErrorV1>;
    fn write_version_v1(
        &mut self,
        version_id: VersionIdV1,
        root_tree: PhysicalTreeIdV1,
        summary: VersionSummaryInputV1,
        counters: &mut OperationCountersV1,
    ) -> CoreResult<PhysicalVersionRecordIdV1>;
    fn complete_v1(
        &mut self,
        expected_version: PhysicalVersionRecordIdV1,
    ) -> CoreResult<CompletedPackSetV1>;
    fn record_incomplete_residue_v1(&mut self) -> CoreResult<()>;
    fn cleanup_private_pack_controlled_v1(&mut self) -> Result<(), FsCasErrorV1>;
    fn take_first_fscas_error_v1(&mut self) -> Option<FsCasErrorV1>;
    fn record_global_seen_observation_v1(&mut self) -> CoreResult<()>;
    fn take_storage_counters_v1(&mut self) -> OperationCountersV1;
}

/// Opaque, non-cloneable root-owned operation capability. Lifecycle is the
/// only owner that can turn an admitted ticket into preparation and handoff.
pub(crate) struct C3StorageOperationV1<'root> {
    capability: FsOperationCapabilityV1<'root>,
}

pub(crate) struct C3QualificationCreateGrantV1<'root> {
    operation: C3StorageOperationV1<'root>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum C3MutationOperationKindV1 {
    Replace,
    Update,
    Add,
    Remove,
    Move,
    Metadata,
}

impl C3MutationOperationKindV1 {
    const fn storage_kind_v1(self) -> FsOperationKindV1 {
        match self {
            Self::Replace => FsOperationKindV1::CompleteReplace,
            Self::Update => FsOperationKindV1::CompleteUpdate,
            Self::Add => FsOperationKindV1::CompleteAdd,
            Self::Remove => FsOperationKindV1::CompleteRemove,
            Self::Move => FsOperationKindV1::CompleteMove,
            Self::Metadata => FsOperationKindV1::CompleteMetadata,
        }
    }
}

/// Request one distinct root-owned mutation authority. The operation kind and
/// cancellation key are the complete phase-one ticket; no typed mutation
/// request, base root, edit path, source, sink, bound, or policy is inspected
/// until this function succeeds.
pub(crate) fn request_c3_mutation_operation_v1<'root, C>(
    cas: &'root FsCasV1,
    kind: C3MutationOperationKindV1,
    cancellation_key: u64,
    counters: &mut OperationCountersV1,
    control: &mut C,
) -> Result<C3StorageOperationV1<'root>, FsCasErrorV1>
where
    C: FsCasControlV1 + ?Sized,
{
    control.boundary_reached(FsCasBoundaryV1::BeforeOperationSlotReservationRequest);
    Ok(C3StorageOperationV1 {
        capability: cas.begin_operation_capability_v1(
            kind.storage_kind_v1(),
            cancellation_key,
            counters,
            control,
        )?,
    })
}

pub(crate) fn request_c3_create_qualification_v1<'root, C>(
    cas: &'root FsCasV1,
    cancellation_key: u64,
    counters: &mut OperationCountersV1,
    control: &mut C,
) -> Result<C3QualificationCreateGrantV1<'root>, FsCasErrorV1>
where
    C: FsCasControlV1 + ?Sized,
{
    control.boundary_reached(FsCasBoundaryV1::BeforeOperationSlotReservationRequest);
    Ok(C3QualificationCreateGrantV1 {
        operation: C3StorageOperationV1 {
            capability: cas.begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                cancellation_key,
                counters,
                control,
            )?,
        },
    })
}

pub(crate) fn request_c3_tree_operation_v1<'root, C>(
    cas: &'root FsCasV1,
    cancellation_key: u64,
    counters: &mut OperationCountersV1,
    control: &mut C,
) -> Result<C3StorageOperationV1<'root>, FsCasErrorV1>
where
    C: FsCasControlV1 + ?Sized,
{
    control.boundary_reached(FsCasBoundaryV1::BeforeOperationSlotReservationRequest);
    Ok(C3StorageOperationV1 {
        capability: cas.begin_operation_capability_v1(
            FsOperationKindV1::CompleteC3Tree,
            cancellation_key,
            counters,
            control,
        )?,
    })
}

impl<'root> C3QualificationCreateGrantV1<'root> {
    pub(crate) fn into_operation(self) -> C3StorageOperationV1<'root> {
        self.operation
    }
}

impl C3StorageOperationV1<'_> {
    pub(crate) fn declare_plan_v1(&mut self, plan: OperationMemoryPlanV1) -> Result<(), CoreError> {
        self.capability.declare_plan_v1(plan)
    }

    pub(crate) fn declare_storage_envelope_v1(
        &mut self,
        envelope: FsStorageEnvelopeV1,
    ) -> Result<(), FsCasErrorV1> {
        self.capability.declare_storage_envelope_v1(envelope)
    }

    fn finish_operation_v1<C>(
        &mut self,
        commit: bool,
        counters: &mut OperationCountersV1,
        control: &mut C,
    ) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        let storage = self
            .capability
            .finish_storage_admission_v1(commit, counters, control);
        let admission = self
            .capability
            .finish_operation_admission_v1(counters, control);
        match storage {
            Err(error) => Err(error),
            Ok(()) => admission,
        }
    }

    fn storage_token_v1(&self) -> Result<FsStorageOperationTokenV1, FsCasErrorV1> {
        self.capability.storage_token_v1()
    }

    pub(crate) fn ledger_v1(&self) -> &ResourceLedgerV1 {
        self.capability.ledger_v1()
    }

    pub(crate) fn reservation_v1(&self) -> &OperationReservationV1<'_> {
        self.capability.reservation_v1()
    }

    pub(crate) fn authenticate_base_root_v1<C>(
        &self,
        version_record: PhysicalVersionRecordIdV1,
        expected_root: PhysicalTreeIdV1,
        counters: &mut OperationCountersV1,
        comparison: &mut [u8; COMPARISON_WINDOW_BYTES],
        control: &mut C,
    ) -> Result<u64, C3OperationErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        authenticate_base_root_storage_v1(
            self.capability.owner_ref_v1(),
            version_record,
            expected_root,
            counters,
            comparison,
            control,
        )
    }

    pub(crate) fn storage_resident_plan_v1(
        &self,
        require_tree_storage: bool,
    ) -> Result<C3StorageResidentPlanV1, CoreError> {
        let owner = self.capability.owner_ref_v1();
        let references = owner.operation_spool_resident_memory_bound_v1("chunk-references")?;
        let metadata = owner.operation_spool_resident_memory_bound_v1("pack-index")?;
        let closure_objects = owner.operation_spool_resident_memory_bound_v1("closure-objects")?;
        let global_seen = owner.operation_spool_resident_memory_bound_v1("global-seen")?;
        let built_files = require_tree_storage
            .then(|| owner.operation_spool_resident_memory_bound_v1("built-files"))
            .transpose()?;
        let built_directories = require_tree_storage
            .then(|| owner.operation_spool_resident_memory_bound_v1("built-directories"))
            .transpose()?;
        let private_storage = owner.private_pack_resident_memory_bound_v1()?;
        let occupied = owner.occupied_resident_memory_bound_v1()?;
        let preparation = C3PreparationResidentBoundsV1 {
            references,
            metadata,
            closure_objects,
            global_seen,
            built_files,
            built_directories,
        };
        let total = references
            .checked_add(metadata)
            .and_then(|bytes| bytes.checked_add(closure_objects))
            .and_then(|bytes| bytes.checked_add(global_seen))
            .and_then(|bytes| bytes.checked_add(built_files.unwrap_or(0)))
            .and_then(|bytes| bytes.checked_add(built_directories.unwrap_or(0)))
            .and_then(|bytes| bytes.checked_add(private_storage))
            .and_then(|bytes| bytes.checked_add(occupied))
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(C3StorageResidentPlanV1 {
            preparation,
            private_storage,
            total,
        })
    }

    fn begin_preparation_v1<C>(
        &self,
        global_seen_capacity: u32,
        bounds: C3PreparationResidentBoundsV1,
        control: &mut C,
    ) -> Result<C3OperationPreparationV1, C3PreparationErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        C3OperationPreparationV1::begin(
            self.capability.owner_ref_v1(),
            self.storage_token_v1()?,
            global_seen_capacity,
            bounds,
            control,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn begin_session_v1<'operation, 'ledger, 'control, C>(
        &'operation self,
        preparation: &'operation mut C3OperationPreparationV1,
        require_tree_storage: bool,
        left: &'operation mut [u8; COMPARISON_WINDOW_BYTES],
        right: &'operation mut [u8; COMPARISON_WINDOW_BYTES],
        maximum_records: u32,
        private_pack_resident_bound: u64,
        ledger: &'operation ResourceLedgerV1,
        reservation: &'operation OperationReservationV1<'ledger>,
        control: &'operation RefCell<&'control mut C>,
    ) -> Result<C3StorageSessionV1<'operation, 'ledger, 'control, C>, FsCasErrorV1>
    where
        C: CdcControlV1 + FsCasControlV1 + ?Sized,
    {
        begin_storage_session_v1(
            self.capability.owner_ref_v1(),
            self.storage_token_v1()?,
            preparation,
            require_tree_storage,
            left,
            right,
            maximum_records,
            private_pack_resident_bound,
            ledger,
            reservation,
            control,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn complete_closure_fence_v1<C>(
        &self,
        preparation: &mut C3OperationPreparationV1,
        root: TypedPhysicalObjectIdV1,
        reservation: &OperationReservationV1<'_>,
        counters: &mut OperationCountersV1,
        buffers: AdmissionBuffersV1<'_>,
        algorithm: C3CdcAlgorithmV1,
        control: &mut C,
    ) -> Result<u64, FsClosureAdmissionErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        complete_closure_fence_storage_v1(
            self.capability.owner_ref_v1(),
            self.storage_token_v1()
                .map_err(FsClosureAdmissionErrorV1::FsCas)?,
            preparation,
            root,
            reservation,
            counters,
            buffers,
            algorithm,
            control,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum C3OperationErrorV1 {
    Core(CoreError),
    FsCas(FsCasErrorV1),
}

impl From<CoreError> for C3OperationErrorV1 {
    fn from(error: CoreError) -> Self {
        Self::Core(error)
    }
}

impl From<FsCasErrorV1> for C3OperationErrorV1 {
    fn from(error: FsCasErrorV1) -> Self {
        Self::FsCas(error)
    }
}

impl From<C3PreparationErrorV1> for C3OperationErrorV1 {
    fn from(error: C3PreparationErrorV1) -> Self {
        match error {
            C3PreparationErrorV1::Core(error) => Self::Core(error),
            C3PreparationErrorV1::FsCas(error) => Self::FsCas(error),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct C3HandoffV1 {
    algorithm: C3CdcAlgorithmV1,
    version_record: PhysicalVersionRecordIdV1,
    root_tree: PhysicalTreeIdV1,
    pack: SealedPackV1,
    pack_outcome: FsPackAdmissionOutcomeV1,
    carrier_count: u32,
    carrier_rollovers: u32,
    carriers_installed: u32,
    carriers_reused: u32,
    object_count: u64,
    reference_spool_bytes: Option<u64>,
    index_spool_bytes: Option<u64>,
}

impl C3HandoffV1 {
    pub(crate) const fn algorithm(self) -> C3CdcAlgorithmV1 {
        self.algorithm
    }

    pub(crate) const fn version_record(self) -> PhysicalVersionRecordIdV1 {
        self.version_record
    }

    pub(crate) const fn root_tree(self) -> PhysicalTreeIdV1 {
        self.root_tree
    }

    pub(crate) const fn pack(self) -> SealedPackV1 {
        self.pack
    }

    pub(crate) const fn pack_outcome(self) -> FsPackAdmissionOutcomeV1 {
        self.pack_outcome
    }

    pub(crate) const fn carrier_count(self) -> u32 {
        self.carrier_count
    }

    pub(crate) const fn carrier_rollovers(self) -> u32 {
        self.carrier_rollovers
    }

    pub(crate) const fn carriers_installed(self) -> u32 {
        self.carriers_installed
    }

    pub(crate) const fn carriers_reused(self) -> u32 {
        self.carriers_reused
    }

    pub(crate) const fn object_count(self) -> u64 {
        self.object_count
    }

    pub(crate) const fn reference_spool_bytes(self) -> Option<u64> {
        self.reference_spool_bytes
    }

    pub(crate) const fn index_spool_bytes(self) -> Option<u64> {
        self.index_spool_bytes
    }
}

pub(crate) struct C3OperationBuffersV1<'a> {
    pub(crate) source: &'a mut [u8; MAXIMUM_CHUNK_BYTES],
    pub(crate) cdc_ring: &'a mut [u8; MAXIMUM_CHUNK_BYTES],
    pub(crate) incoming_comparison: &'a mut [u8; COMPARISON_WINDOW_BYTES],
    pub(crate) occupied_comparison: &'a mut [u8; COMPARISON_WINDOW_BYTES],
    pub(crate) tree_object: &'a mut [u8; MAX_TREE_OBJECT_BYTES],
    pub(crate) tree_pages: &'a mut [Option<TreePageSummaryV1>],
    pub(crate) traversal_state: &'a mut [u8],
}

/// Builder-visible buffers exclude the comparison and traversal storage held
/// by the storage session and the later closure fence. This split makes it
/// impossible for a content builder to alias those lifecycle-owned regions.
pub(crate) struct C3LifecycleBuildBuffersV1<'a> {
    pub(crate) source: &'a mut [u8; MAXIMUM_CHUNK_BYTES],
    pub(crate) cdc_ring: &'a mut [u8; MAXIMUM_CHUNK_BYTES],
    pub(crate) tree_object: &'a mut [u8; MAX_TREE_OBJECT_BYTES],
    pub(crate) tree_pages: &'a mut [Option<TreePageSummaryV1>],
}

pub(crate) struct C3LifecyclePlanV1 {
    pub(crate) global_seen_capacity: u32,
    pub(crate) storage_resident: C3StorageResidentPlanV1,
    pub(crate) require_tree_storage: bool,
    pub(crate) maximum_records: u32,
    pub(crate) algorithm: C3CdcAlgorithmV1,
}

pub(crate) struct C3PreparedCandidateV1 {
    version_record: PhysicalVersionRecordIdV1,
    root_tree: PhysicalTreeIdV1,
    completed: CompletedPackSetV1,
    reference_spool_bytes: Option<u64>,
}

fn mutation_candidate_bounds_v1(
    base_objects: u64,
    maximum_file_objects: u64,
    maximum_tree_objects: u64,
) -> CoreResult<(u64, u32, u32)> {
    let maximum_objects = base_objects
        .checked_add(maximum_file_objects)
        .and_then(|count| count.checked_add(maximum_tree_objects))
        .and_then(|count| count.checked_add(1))
        .ok_or(CoreError::IntegerOverflow)?;
    validate_total_object_count(maximum_objects)?;
    let global_seen_capacity = candidate_global_seen_capacity_v1(maximum_objects)?;
    let maximum_records = u32::try_from(maximum_objects.min(C3_MAX_STORAGE_RECORDS_V1))
        .map_err(|_| CoreError::IntegerOverflow)?;
    Ok((maximum_objects, global_seen_capacity, maximum_records))
}

fn ensure_mutation_buffers_v1(
    buffers: &C3OperationBuffersV1<'_>,
    maximum_objects: u64,
    maximum_page_summaries: u32,
) -> CoreResult<()> {
    if buffers.traversal_state.len() < candidate_traversal_bytes_v1(maximum_objects)?
        || buffers.tree_pages.len()
            < usize::try_from(maximum_page_summaries).map_err(|_| CoreError::IntegerOverflow)?
    {
        return Err(CoreError::ResourceRefused);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn mutation_memory_plan_v1(
    buffers: &C3OperationBuffersV1<'_>,
    storage_resident: C3StorageResidentPlanV1,
    source_resident: u64,
    other_port_resident: u64,
    evidence_resident: u64,
    cow_logical_bytes: u64,
    hash_state_bytes: u64,
) -> CoreResult<OperationMemoryPlanV1> {
    let metadata = storage_resident
        .total_resident_bytes_v1()
        .checked_add(source_resident)
        .and_then(|bytes| bytes.checked_add(other_port_resident))
        .ok_or(CoreError::IntegerOverflow)?
        .max(MUTATION_METADATA_RESERVATION_BYTES_V1);
    OperationMemoryPlanV1::empty()
        .charge(MemoryComponentV1::SourceWindow, buffers.source.len() as u64)?
        .charge(MemoryComponentV1::CdcRing, buffers.cdc_ring.len() as u64)?
        .charge(
            MemoryComponentV1::ComparisonWindow,
            (2 * COMPARISON_WINDOW_BYTES) as u64 + cow_logical_bytes,
        )?
        .charge(
            MemoryComponentV1::ObjectScratch,
            buffers.tree_object.len() as u64,
        )?
        .charge(
            MemoryComponentV1::PageSummaries,
            c3_admission_traversal_resident_bytes_v1()?
                .max(core::mem::size_of_val(buffers.tree_pages) as u64),
        )?
        .charge(
            MemoryComponentV1::TraversalState,
            buffers.traversal_state.len() as u64,
        )?
        .charge(MemoryComponentV1::EvidenceWindow, evidence_resident)?
        .charge(MemoryComponentV1::MetadataWindow, metadata)?
        .charge(MemoryComponentV1::HashState, hash_state_bytes)
}

fn complete_mutation_candidate_v1<C>(
    storage: &mut dyn C3StorageSessionPortV1,
    control_cell: &RefCell<&mut C>,
    candidate: CanonicalDirectoryTreeV1,
    counters: &mut OperationCountersV1,
) -> Result<C3PreparedCandidateV1, C3OperationErrorV1>
where
    C: C3LifecycleControlV1 + ?Sized,
{
    let DirectoryLogicalIdentityV1::ImplicitRoot(logical_root) = candidate.logical() else {
        return Err(CoreError::TypeDomain.into());
    };
    let summary = storage.rebuild_candidate_closure_v1(
        candidate.physical(),
        counters,
        &mut SharedC3ControlV1::new(control_cell),
    )?;
    let version = storage.write_version_v1(
        derive_version_v1(logical_root),
        candidate.physical(),
        summary,
        counters,
    )?;
    let completed = storage.complete_v1(version)?;
    let reference_spool_bytes = storage.reference_storage_bytes_v1()?;
    Ok(C3PreparedCandidateV1::new(
        version,
        candidate.physical(),
        completed,
        reference_spool_bytes,
    ))
}

impl C3PreparedCandidateV1 {
    pub(crate) const fn new(
        version_record: PhysicalVersionRecordIdV1,
        root_tree: PhysicalTreeIdV1,
        completed: CompletedPackSetV1,
        reference_spool_bytes: Option<u64>,
    ) -> Self {
        Self {
            version_record,
            root_tree,
            completed,
            reference_spool_bytes,
        }
    }
}

/// Complete root-owned whole-file Replace. The one opaque root capability is
/// acquired before the base root, path, source declaration, policy, or any
/// request bound is inspected, and is borrowed through candidate closure,
/// exact closure fencing, explicit cleanup, and handoff.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_c3_complete_replace_v1<S, C>(
    cas: &FsCasV1,
    cancellation_key: u64,
    algorithm: C3CdcAlgorithmV1,
    version_record: PhysicalVersionRecordIdV1,
    base_root: CanonicalDirectoryTreeV1,
    replacement_evidence: AuthenticatedTreeReplacementEvidenceV1<'_>,
    replacement_index: usize,
    name: &[u8],
    mode: u16,
    declared_len: u64,
    source: &mut S,
    buffers: C3OperationBuffersV1<'_>,
    cow_logical: &mut [u8; COMPARISON_WINDOW_BYTES],
    control: &mut C,
    counters: &mut OperationCountersV1,
) -> Result<C3HandoffV1, C3OperationErrorV1>
where
    S: ContentSourceV1 + ?Sized,
    C: C3LifecycleControlV1 + ?Sized,
{
    let mut operation = request_c3_mutation_operation_v1(
        cas,
        C3MutationOperationKindV1::Replace,
        cancellation_key,
        counters,
        control,
    )?;
    check_lifecycle_control_v1(control)?;

    let base_objects = operation.authenticate_base_root_v1(
        version_record,
        base_root.physical(),
        counters,
        cow_logical,
        control,
    )?;
    check_lifecycle_control_v1(control)?;

    let component = ValidatedComponent::new(name)?;
    validate_file_mode(mode)?;
    validate_logical_length(declared_len)?;
    if replacement_index >= base_root.entry_count() as usize {
        return Err(CoreError::CountCap.into());
    }
    let maximum_refs = declared_len
        .checked_add(8_191)
        .ok_or(CoreError::IntegerOverflow)?
        / 8_192;
    validate_chunk_refs_per_file(maximum_refs)?;
    let tree_shape = preflight_canonical_tree_v1(u64::from(base_root.entry_count()))?;
    let (maximum_objects, global_seen_capacity, maximum_records) = mutation_candidate_bounds_v1(
        base_objects,
        maximum_refs
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?,
        u64::from(tree_shape.tree_object_count()),
    )?;
    ensure_mutation_buffers_v1(&buffers, maximum_objects, tree_shape.page_summary_count())?;

    operation.declare_storage_envelope_v1(c3_storage_envelope_v1(
        maximum_objects,
        maximum_refs
            .checked_add(u64::from(tree_shape.tree_object_count()))
            .and_then(|count| count.checked_add(2))
            .ok_or(CoreError::IntegerOverflow)?,
        maximum_refs,
        1,
        u64::from(tree_shape.tree_object_count()),
        declared_len,
        global_seen_capacity,
        false,
    )?)?;

    let source_resident = source.resident_memory_bound_bytes()?;
    let storage_resident = operation.storage_resident_plan_v1(false)?;
    let evidence_resident = u64::try_from(replacement_evidence_resident_bytes_v1(
        replacement_evidence,
    )?)
    .map_err(|_| CoreError::IntegerOverflow)?;
    let plan = mutation_memory_plan_v1(
        &buffers,
        storage_resident,
        source_resident,
        0,
        evidence_resident,
        COMPARISON_WINDOW_BYTES as u64,
        IDENTITY_HASHER_BYTES_V1
            .checked_mul(4)
            .ok_or(CoreError::IntegerOverflow)?,
    )?;
    operation.declare_plan_v1(plan)?;
    counters.memory_high_water = counters
        .memory_high_water
        .max(operation.ledger_v1().high_water_bytes());

    run_c3_lifecycle_v1(
        operation,
        C3LifecyclePlanV1 {
            global_seen_capacity,
            storage_resident,
            require_tree_storage: false,
            maximum_records,
            algorithm,
        },
        buffers,
        control,
        counters,
        move |storage, control_cell, reservation, buffers, counters| {
            if source.resident_memory_bound_bytes()? > source_resident {
                return Err(CoreError::ResourceRefused.into());
            }
            let file = {
                let (references, sink) = storage.content_parts_v1();
                replace_file_c3_borrowed_v1(
                    name,
                    mode,
                    declared_len,
                    source,
                    sink,
                    references,
                    ContentBuffersV1::new(buffers.source, buffers.cdc_ring),
                    &mut SharedC3ControlV1::new(control_cell),
                    reservation,
                    algorithm,
                    counters,
                )?
            };
            let replacement = CanonicalTreeEntryV1::new(
                component,
                CanonicalTreeChildV1::File {
                    logical: derive_file_node_v1(mode, file.logical_file())?,
                    physical: file.physical_file(),
                },
            );
            let candidate = replace_directory_entry_cow_borrowed_v1(
                base_root,
                replacement_evidence,
                replacement_index,
                replacement,
                storage.tree_sink_v1(),
                reservation,
                counters,
                buffers.tree_object,
                cow_logical,
            )?
            .directory();
            complete_mutation_candidate_v1(storage, control_cell, candidate, counters)
        },
    )
}

/// Complete authenticated bounded Update/rejoin. Update owns a distinct root
/// operation kind and can only reach the FastCDC implementation frozen by the
/// accepted Phase-1 format; it has no Replace redispatch or full-base payload
/// fallback path.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_c3_complete_update_v1<S, B, E, C>(
    cas: &FsCasV1,
    cancellation_key: u64,
    version_record: PhysicalVersionRecordIdV1,
    base_root: CanonicalDirectoryTreeV1,
    replacement_evidence: AuthenticatedTreeReplacementEvidenceV1<'_>,
    replacement_index: usize,
    name: &[u8],
    mode: u16,
    base_file: AuthenticatedBaseFileV1,
    range: UpdateRangeV1,
    inserted_len: u64,
    inserted: &mut S,
    base_bytes: &mut B,
    chunk_evidence: &mut E,
    buffers: C3OperationBuffersV1<'_>,
    cow_logical: &mut [u8; COMPARISON_WINDOW_BYTES],
    control: &mut C,
    counters: &mut OperationCountersV1,
) -> Result<C3HandoffV1, C3OperationErrorV1>
where
    S: ContentSourceV1 + ?Sized,
    B: AuthenticatedBaseByteReaderV1 + ?Sized,
    E: BaseChunkEvidenceSourceV1 + ?Sized,
    C: C3LifecycleControlV1 + ?Sized,
{
    let mut operation = request_c3_mutation_operation_v1(
        cas,
        C3MutationOperationKindV1::Update,
        cancellation_key,
        counters,
        control,
    )?;
    check_lifecycle_control_v1(control)?;

    let base_objects = operation.authenticate_base_root_v1(
        version_record,
        base_root.physical(),
        counters,
        cow_logical,
        control,
    )?;
    check_lifecycle_control_v1(control)?;

    let component = ValidatedComponent::new(name)?;
    validate_file_mode(mode)?;
    if replacement_index >= base_root.entry_count() as usize {
        return Err(CoreError::CountCap.into());
    }
    let expected_base_entry = CanonicalTreeEntryV1::new(
        component,
        CanonicalTreeChildV1::File {
            logical: derive_file_node_v1(base_file.mode(), base_file.identity())?,
            physical: base_file.physical_file(),
        },
    );
    if replacement_evidence.expected_entry_v1(replacement_index)? != expected_base_entry {
        return Err(CoreError::IdMismatch.into());
    }
    let new_len = base_file
        .identity()
        .logical_len()
        .checked_sub(range.len())
        .and_then(|len| len.checked_add(inserted_len))
        .ok_or(CoreError::RangeResyncFailed)?;
    validate_logical_length(new_len).map_err(|_| CoreError::RangeResyncFailed)?;
    let maximum_refs = new_len
        .checked_add(8_191)
        .ok_or(CoreError::RangeResyncFailed)?
        / 8_192;
    validate_chunk_refs_per_file(maximum_refs).map_err(|_| CoreError::RangeResyncFailed)?;
    let tree_shape = preflight_canonical_tree_v1(u64::from(base_root.entry_count()))?;
    let (maximum_objects, global_seen_capacity, maximum_records) = mutation_candidate_bounds_v1(
        base_objects,
        maximum_refs
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?,
        u64::from(tree_shape.tree_object_count()),
    )?;
    ensure_mutation_buffers_v1(&buffers, maximum_objects, tree_shape.page_summary_count())?;

    let maximum_new_payload_bytes = update_maximum_new_payload_bytes_v1(
        base_file.identity().logical_len(),
        new_len,
        inserted_len,
    )?;

    operation.declare_storage_envelope_v1(c3_storage_envelope_v1(
        maximum_objects,
        maximum_refs
            .checked_add(u64::from(tree_shape.tree_object_count()))
            .and_then(|count| count.checked_add(2))
            .ok_or(CoreError::IntegerOverflow)?,
        maximum_refs,
        1,
        u64::from(tree_shape.tree_object_count()),
        maximum_new_payload_bytes,
        global_seen_capacity,
        false,
    )?)?;

    let source_resident = inserted.resident_memory_bound_bytes()?;
    let base_reader_resident = base_bytes.resident_memory_bound_bytes()?;
    let chunk_evidence_resident = chunk_evidence.resident_memory_bound_bytes()?;
    let other_port_resident = base_reader_resident
        .checked_add(chunk_evidence_resident)
        .ok_or(CoreError::IntegerOverflow)?;
    let storage_resident = operation.storage_resident_plan_v1(false)?;
    let evidence_resident = u64::try_from(replacement_evidence_resident_bytes_v1(
        replacement_evidence,
    )?)
    .map_err(|_| CoreError::IntegerOverflow)?;
    let plan = mutation_memory_plan_v1(
        &buffers,
        storage_resident,
        source_resident,
        other_port_resident,
        evidence_resident,
        COMPARISON_WINDOW_BYTES as u64,
        IDENTITY_HASHER_BYTES_V1
            .checked_mul(4)
            .ok_or(CoreError::IntegerOverflow)?,
    )?;
    operation.declare_plan_v1(plan)?;
    counters.memory_high_water = counters
        .memory_high_water
        .max(operation.ledger_v1().high_water_bytes());

    run_c3_lifecycle_v1(
        operation,
        C3LifecyclePlanV1 {
            global_seen_capacity,
            storage_resident,
            require_tree_storage: false,
            maximum_records,
            algorithm: C3CdcAlgorithmV1::FastCdc,
        },
        buffers,
        control,
        counters,
        move |storage, control_cell, reservation, buffers, counters| {
            if inserted.resident_memory_bound_bytes()? > source_resident
                || base_bytes.resident_memory_bound_bytes()? > base_reader_resident
                || chunk_evidence.resident_memory_bound_bytes()? > chunk_evidence_resident
            {
                return Err(CoreError::ResourceRefused.into());
            }
            let file = {
                let (references, sink) = storage.content_parts_v1();
                update_file_c3_borrowed_v1(
                    name,
                    mode,
                    base_file,
                    range,
                    inserted_len,
                    inserted,
                    base_bytes,
                    chunk_evidence,
                    sink,
                    references,
                    UpdateBuffersV1::new(buffers.source, buffers.cdc_ring),
                    &mut SharedC3ControlV1::new(control_cell),
                    reservation,
                    counters,
                )?
            };
            let replacement = CanonicalTreeEntryV1::new(
                component,
                CanonicalTreeChildV1::File {
                    logical: derive_file_node_v1(mode, file.logical_file())?,
                    physical: file.physical_file(),
                },
            );
            let candidate = replace_directory_entry_cow_borrowed_v1(
                base_root,
                replacement_evidence,
                replacement_index,
                replacement,
                storage.tree_sink_v1(),
                reservation,
                counters,
                buffers.tree_object,
                cow_logical,
            )?
            .directory();
            complete_mutation_candidate_v1(storage, control_cell, candidate, counters)
        },
    )
}

/// Complete root-directory Add, including new file construction, structural
/// COW, candidate-graph authentication, complete closure fencing, and one
/// synchronous handoff under the same root-owned grant.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_c3_complete_add_v1<S, T, C>(
    cas: &FsCasV1,
    cancellation_key: u64,
    version_record: PhysicalVersionRecordIdV1,
    base_root: CanonicalDirectoryTreeV1,
    mutation_evidence: AuthenticatedTreeMutationEvidenceV1<'_>,
    insertion_index: usize,
    name: &[u8],
    mode: u16,
    declared_len: u64,
    source: &mut S,
    tree_source: &mut T,
    buffers: C3OperationBuffersV1<'_>,
    cow_logical: &mut [u8; COMPARISON_WINDOW_BYTES],
    control: &mut C,
    counters: &mut OperationCountersV1,
) -> Result<C3HandoffV1, C3OperationErrorV1>
where
    S: ContentSourceV1 + ?Sized,
    T: CanonicalTreeMutationSourceV1 + ?Sized,
    C: C3LifecycleControlV1 + ?Sized,
{
    let mut operation = request_c3_mutation_operation_v1(
        cas,
        C3MutationOperationKindV1::Add,
        cancellation_key,
        counters,
        control,
    )?;
    check_lifecycle_control_v1(control)?;
    let base_objects = operation.authenticate_base_root_v1(
        version_record,
        base_root.physical(),
        counters,
        cow_logical,
        control,
    )?;
    check_lifecycle_control_v1(control)?;

    let component = ValidatedComponent::new(name)?;
    validate_file_mode(mode)?;
    validate_logical_length(declared_len)?;
    let base_entry_count = tree_source.declared_base_entry_count()?;
    let result_entry_count = tree_source.declared_result_entry_count()?;
    if base_entry_count != base_root.entry_count()
        || result_entry_count
            != base_entry_count
                .checked_add(1)
                .ok_or(CoreError::IntegerOverflow)?
        || insertion_index > base_entry_count as usize
    {
        return Err(CoreError::Path.into());
    }
    let maximum_refs = declared_len
        .checked_add(8_191)
        .ok_or(CoreError::IntegerOverflow)?
        / 8_192;
    validate_chunk_refs_per_file(maximum_refs)?;
    let result_shape = preflight_canonical_tree_v1(u64::from(result_entry_count))?;
    let (maximum_objects, global_seen_capacity, maximum_records) = mutation_candidate_bounds_v1(
        base_objects,
        maximum_refs
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?,
        u64::from(result_shape.tree_object_count()),
    )?;
    ensure_mutation_buffers_v1(&buffers, maximum_objects, result_shape.page_summary_count())?;

    operation.declare_storage_envelope_v1(c3_storage_envelope_v1(
        maximum_objects,
        maximum_refs
            .checked_add(u64::from(result_shape.tree_object_count()))
            .and_then(|count| count.checked_add(2))
            .ok_or(CoreError::IntegerOverflow)?,
        maximum_refs,
        1,
        u64::from(result_shape.tree_object_count()),
        declared_len,
        global_seen_capacity,
        false,
    )?)?;

    let source_resident = source.resident_memory_bound_bytes()?;
    let tree_source_resident = tree_source.resident_memory_bound_bytes()?;
    let storage_resident = operation.storage_resident_plan_v1(false)?;
    let evidence_resident = u64::try_from(mutation_evidence_resident_bytes_v1(mutation_evidence)?)
        .map_err(|_| CoreError::IntegerOverflow)?;
    let hash_state_bytes = mutation_hash_state_bytes_v1()?.max(
        IDENTITY_HASHER_BYTES_V1
            .checked_mul(4)
            .ok_or(CoreError::IntegerOverflow)?,
    );
    let plan = mutation_memory_plan_v1(
        &buffers,
        storage_resident,
        source_resident,
        tree_source_resident,
        evidence_resident,
        COMPARISON_WINDOW_BYTES as u64,
        hash_state_bytes,
    )?;
    operation.declare_plan_v1(plan)?;
    counters.memory_high_water = counters
        .memory_high_water
        .max(operation.ledger_v1().high_water_bytes());

    run_c3_lifecycle_v1(
        operation,
        C3LifecyclePlanV1 {
            global_seen_capacity,
            storage_resident,
            require_tree_storage: false,
            maximum_records,
            algorithm: C3CdcAlgorithmV1::FastCdc,
        },
        buffers,
        control,
        counters,
        move |storage, control_cell, reservation, buffers, counters| {
            if source.resident_memory_bound_bytes()? > source_resident
                || tree_source.resident_memory_bound_bytes()? > tree_source_resident
            {
                return Err(CoreError::ResourceRefused.into());
            }
            let file = {
                let (references, sink) = storage.content_parts_v1();
                replace_file_c3_borrowed_v1(
                    name,
                    mode,
                    declared_len,
                    source,
                    sink,
                    references,
                    ContentBuffersV1::new(buffers.source, buffers.cdc_ring),
                    &mut SharedC3ControlV1::new(control_cell),
                    reservation,
                    C3CdcAlgorithmV1::FastCdc,
                    counters,
                )?
            };
            let added = CanonicalTreeEntryV1::new(
                component,
                CanonicalTreeChildV1::File {
                    logical: derive_file_node_v1(mode, file.logical_file())?,
                    physical: file.physical_file(),
                },
            );
            let candidate = add_directory_entry_cow_borrowed_v1(
                base_root,
                mutation_evidence,
                insertion_index,
                added,
                tree_source,
                storage.tree_sink_v1(),
                reservation,
                counters,
                buffers.tree_object,
                cow_logical,
                buffers.tree_pages,
            )?
            .directory();
            complete_mutation_candidate_v1(storage, control_cell, candidate, counters)
        },
    )
}

/// Complete root-directory Remove. No content source is present, but the
/// accepted root, exact removed entry, result snapshot, new tree objects,
/// candidate closure, cleanup, and handoff remain one indivisible operation.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_c3_complete_remove_v1<T, C>(
    cas: &FsCasV1,
    cancellation_key: u64,
    version_record: PhysicalVersionRecordIdV1,
    base_root: CanonicalDirectoryTreeV1,
    mutation_evidence: AuthenticatedTreeMutationEvidenceV1<'_>,
    removal_index: usize,
    expected_removed: CanonicalTreeEntryV1<'_>,
    tree_source: &mut T,
    buffers: C3OperationBuffersV1<'_>,
    cow_logical: &mut [u8; COMPARISON_WINDOW_BYTES],
    control: &mut C,
    counters: &mut OperationCountersV1,
) -> Result<C3HandoffV1, C3OperationErrorV1>
where
    T: CanonicalTreeMutationSourceV1 + ?Sized,
    C: C3LifecycleControlV1 + ?Sized,
{
    let mut operation = request_c3_mutation_operation_v1(
        cas,
        C3MutationOperationKindV1::Remove,
        cancellation_key,
        counters,
        control,
    )?;
    check_lifecycle_control_v1(control)?;
    let base_objects = operation.authenticate_base_root_v1(
        version_record,
        base_root.physical(),
        counters,
        cow_logical,
        control,
    )?;
    check_lifecycle_control_v1(control)?;

    let base_entry_count = tree_source.declared_base_entry_count()?;
    let result_entry_count = tree_source.declared_result_entry_count()?;
    if base_entry_count != base_root.entry_count()
        || result_entry_count.checked_add(1) != Some(base_entry_count)
        || removal_index >= base_entry_count as usize
    {
        return Err(CoreError::Path.into());
    }
    let result_shape = preflight_canonical_tree_v1(u64::from(result_entry_count))?;
    let (maximum_objects, global_seen_capacity, maximum_records) =
        mutation_candidate_bounds_v1(base_objects, 0, u64::from(result_shape.tree_object_count()))?;
    ensure_mutation_buffers_v1(&buffers, maximum_objects, result_shape.page_summary_count())?;

    operation.declare_storage_envelope_v1(c3_storage_envelope_v1(
        maximum_objects,
        u64::from(result_shape.tree_object_count())
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?,
        0,
        0,
        u64::from(result_shape.tree_object_count()),
        0,
        global_seen_capacity,
        false,
    )?)?;

    let tree_source_resident = tree_source.resident_memory_bound_bytes()?;
    let storage_resident = operation.storage_resident_plan_v1(false)?;
    let evidence_resident = u64::try_from(mutation_evidence_resident_bytes_v1(mutation_evidence)?)
        .map_err(|_| CoreError::IntegerOverflow)?;
    let plan = mutation_memory_plan_v1(
        &buffers,
        storage_resident,
        0,
        tree_source_resident,
        evidence_resident,
        COMPARISON_WINDOW_BYTES as u64,
        mutation_hash_state_bytes_v1()?,
    )?;
    operation.declare_plan_v1(plan)?;
    counters.memory_high_water = counters
        .memory_high_water
        .max(operation.ledger_v1().high_water_bytes());

    run_c3_lifecycle_v1(
        operation,
        C3LifecyclePlanV1 {
            global_seen_capacity,
            storage_resident,
            require_tree_storage: false,
            maximum_records,
            algorithm: C3CdcAlgorithmV1::FastCdc,
        },
        buffers,
        control,
        counters,
        move |storage, control_cell, reservation, buffers, counters| {
            if tree_source.resident_memory_bound_bytes()? > tree_source_resident {
                return Err(CoreError::ResourceRefused.into());
            }
            let candidate = remove_directory_entry_cow_borrowed_v1(
                base_root,
                mutation_evidence,
                removal_index,
                expected_removed,
                tree_source,
                storage.tree_sink_v1(),
                reservation,
                counters,
                buffers.tree_object,
                cow_logical,
                buffers.tree_pages,
            )?
            .directory();
            complete_mutation_candidate_v1(storage, control_cell, candidate, counters)
        },
    )
}

/// Complete metadata-only file replacement. The old file identity and exact
/// chunk-reference stream are authenticated before any preparation artifact
/// is created; only the file-node mode and affected directory spine change.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_c3_complete_metadata_v1<E, C>(
    cas: &FsCasV1,
    cancellation_key: u64,
    version_record: PhysicalVersionRecordIdV1,
    base_root: CanonicalDirectoryTreeV1,
    replacement_evidence: AuthenticatedTreeReplacementEvidenceV1<'_>,
    replacement_index: usize,
    name: &[u8],
    new_mode: u16,
    base_file: AuthenticatedBaseFileV1,
    chunk_evidence: &mut E,
    buffers: C3OperationBuffersV1<'_>,
    cow_logical: &mut [u8; COMPARISON_WINDOW_BYTES],
    control: &mut C,
    counters: &mut OperationCountersV1,
) -> Result<C3HandoffV1, C3OperationErrorV1>
where
    E: BaseChunkEvidenceSourceV1 + ?Sized,
    C: C3LifecycleControlV1 + ?Sized,
{
    let mut operation = request_c3_mutation_operation_v1(
        cas,
        C3MutationOperationKindV1::Metadata,
        cancellation_key,
        counters,
        control,
    )?;
    check_lifecycle_control_v1(control)?;
    let base_objects = operation.authenticate_base_root_v1(
        version_record,
        base_root.physical(),
        counters,
        cow_logical,
        control,
    )?;
    check_lifecycle_control_v1(control)?;

    let component = ValidatedComponent::new(name)?;
    validate_file_mode(new_mode)?;
    if replacement_index >= base_root.entry_count() as usize {
        return Err(CoreError::CountCap.into());
    }
    let expected_base_entry = CanonicalTreeEntryV1::new(
        component,
        CanonicalTreeChildV1::File {
            logical: derive_file_node_v1(base_file.mode(), base_file.identity())?,
            physical: base_file.physical_file(),
        },
    );
    if replacement_evidence.expected_entry_v1(replacement_index)? != expected_base_entry {
        return Err(CoreError::IdMismatch.into());
    }
    let tree_shape = preflight_canonical_tree_v1(u64::from(base_root.entry_count()))?;
    let (maximum_objects, global_seen_capacity, maximum_records) =
        mutation_candidate_bounds_v1(base_objects, 0, u64::from(tree_shape.tree_object_count()))?;
    ensure_mutation_buffers_v1(&buffers, maximum_objects, tree_shape.page_summary_count())?;

    operation.declare_storage_envelope_v1(c3_storage_envelope_v1(
        maximum_objects,
        u64::from(tree_shape.tree_object_count())
            .checked_add(2)
            .ok_or(CoreError::IntegerOverflow)?,
        u64::from(base_file.chunk_count()),
        1,
        u64::from(tree_shape.tree_object_count()),
        0,
        global_seen_capacity,
        false,
    )?)?;

    let chunk_evidence_resident = chunk_evidence.resident_memory_bound_bytes()?;
    let storage_resident = operation.storage_resident_plan_v1(false)?;
    let evidence_resident = u64::try_from(replacement_evidence_resident_bytes_v1(
        replacement_evidence,
    )?)
    .map_err(|_| CoreError::IntegerOverflow)?;
    let plan = mutation_memory_plan_v1(
        &buffers,
        storage_resident,
        0,
        chunk_evidence_resident,
        evidence_resident,
        COMPARISON_WINDOW_BYTES as u64,
        IDENTITY_HASHER_BYTES_V1
            .checked_mul(4)
            .ok_or(CoreError::IntegerOverflow)?,
    )?;
    operation.declare_plan_v1(plan)?;
    counters.memory_high_water = counters
        .memory_high_water
        .max(operation.ledger_v1().high_water_bytes());

    // This authentication consumes only bounded reference metadata and runs
    // before preparation begins, so a malformed/mismatched base leaves no
    // private carrier, index, locator, or closure artifact.
    authenticate_base_file_evidence_v1(base_file, chunk_evidence, counters)?;
    check_lifecycle_control_v1(control)?;

    run_c3_lifecycle_v1(
        operation,
        C3LifecyclePlanV1 {
            global_seen_capacity,
            storage_resident,
            require_tree_storage: false,
            maximum_records,
            algorithm: C3CdcAlgorithmV1::FastCdc,
        },
        buffers,
        control,
        counters,
        move |storage, control_cell, reservation, buffers, counters| {
            let file = {
                let (references, sink) = storage.content_parts_v1();
                reencode_file_metadata_c3_borrowed_v1(
                    new_mode,
                    base_file,
                    chunk_evidence,
                    sink,
                    references,
                    reservation,
                    counters,
                )?
            };
            let replacement = CanonicalTreeEntryV1::new(
                component,
                CanonicalTreeChildV1::File {
                    logical: derive_file_node_v1(new_mode, file.logical_file())?,
                    physical: file.physical_file(),
                },
            );
            let candidate = replace_directory_entry_cow_borrowed_v1(
                base_root,
                replacement_evidence,
                replacement_index,
                replacement,
                storage.tree_sink_v1(),
                reservation,
                counters,
                buffers.tree_object,
                cow_logical,
            )?
            .directory();
            complete_mutation_candidate_v1(storage, control_cell, candidate, counters)
        },
    )
}

/// Complete same-directory Move/rename as one authenticated same-count COW
/// transformation. No intermediate removed tree is admitted, so every object
/// written by a successful operation belongs to the final candidate graph.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_c3_complete_move_v1<T, C>(
    cas: &FsCasV1,
    cancellation_key: u64,
    version_record: PhysicalVersionRecordIdV1,
    base_root: CanonicalDirectoryTreeV1,
    mutation_evidence: AuthenticatedTreeMutationEvidenceV1<'_>,
    removal_index: usize,
    insertion_index: usize,
    expected_removed: CanonicalTreeEntryV1<'_>,
    new_name: &[u8],
    tree_source: &mut T,
    buffers: C3OperationBuffersV1<'_>,
    cow_logical: &mut [u8; COMPARISON_WINDOW_BYTES],
    control: &mut C,
    counters: &mut OperationCountersV1,
) -> Result<C3HandoffV1, C3OperationErrorV1>
where
    T: CanonicalTreeMutationSourceV1 + ?Sized,
    C: C3LifecycleControlV1 + ?Sized,
{
    let mut operation = request_c3_mutation_operation_v1(
        cas,
        C3MutationOperationKindV1::Move,
        cancellation_key,
        counters,
        control,
    )?;
    check_lifecycle_control_v1(control)?;
    let base_objects = operation.authenticate_base_root_v1(
        version_record,
        base_root.physical(),
        counters,
        cow_logical,
        control,
    )?;
    check_lifecycle_control_v1(control)?;

    let component = ValidatedComponent::new(new_name)?;
    let base_entry_count = tree_source.declared_base_entry_count()?;
    let result_entry_count = tree_source.declared_result_entry_count()?;
    if base_entry_count == 0
        || base_entry_count != base_root.entry_count()
        || result_entry_count != base_entry_count
        || removal_index >= base_entry_count as usize
        || insertion_index >= result_entry_count as usize
    {
        return Err(CoreError::Path.into());
    }
    let tree_shape = preflight_canonical_tree_v1(u64::from(result_entry_count))?;
    let (maximum_objects, global_seen_capacity, maximum_records) =
        mutation_candidate_bounds_v1(base_objects, 0, u64::from(tree_shape.tree_object_count()))?;
    ensure_mutation_buffers_v1(&buffers, maximum_objects, tree_shape.page_summary_count())?;

    operation.declare_storage_envelope_v1(c3_storage_envelope_v1(
        maximum_objects,
        u64::from(tree_shape.tree_object_count())
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?,
        0,
        0,
        u64::from(tree_shape.tree_object_count()),
        0,
        global_seen_capacity,
        false,
    )?)?;

    let tree_source_resident = tree_source.resident_memory_bound_bytes()?;
    let storage_resident = operation.storage_resident_plan_v1(false)?;
    let evidence_resident = u64::try_from(mutation_evidence_resident_bytes_v1(mutation_evidence)?)
        .map_err(|_| CoreError::IntegerOverflow)?;
    let plan = mutation_memory_plan_v1(
        &buffers,
        storage_resident,
        0,
        tree_source_resident,
        evidence_resident,
        COMPARISON_WINDOW_BYTES as u64,
        mutation_hash_state_bytes_v1()?,
    )?;
    operation.declare_plan_v1(plan)?;
    counters.memory_high_water = counters
        .memory_high_water
        .max(operation.ledger_v1().high_water_bytes());

    let moved = CanonicalTreeEntryV1::new(component, expected_removed.child());
    run_c3_lifecycle_v1(
        operation,
        C3LifecyclePlanV1 {
            global_seen_capacity,
            storage_resident,
            require_tree_storage: false,
            maximum_records,
            algorithm: C3CdcAlgorithmV1::FastCdc,
        },
        buffers,
        control,
        counters,
        move |storage, control_cell, reservation, buffers, counters| {
            if tree_source.resident_memory_bound_bytes()? > tree_source_resident {
                return Err(CoreError::ResourceRefused.into());
            }
            let candidate = move_directory_entry_cow_borrowed_v1(
                base_root,
                mutation_evidence,
                removal_index,
                insertion_index,
                expected_removed,
                moved,
                tree_source,
                storage.tree_sink_v1(),
                reservation,
                counters,
                buffers.tree_object,
                cow_logical,
                buffers.tree_pages,
            )?
            .directory();
            complete_mutation_candidate_v1(storage, control_cell, candidate, counters)
        },
    )
}

fn explicit_directory_child_v1(
    directory: CanonicalDirectoryTreeV1,
) -> CoreResult<CanonicalTreeChildV1> {
    let DirectoryLogicalIdentityV1::Explicit(logical) = directory.logical() else {
        return Err(CoreError::TypeDomain);
    };
    Ok(CanonicalTreeChildV1::Directory {
        logical,
        physical: directory.physical(),
    })
}

/// Complete one authenticated Move between two sibling directories. Source
/// detach, destination attach, and both root-spine replacements share one
/// root-owned capability and one private storage session. Only the final root
/// receives a closure fence and handoff; no intermediate remove/add root can
/// escape or acquire publication authority.
#[allow(clippy::too_many_arguments)]
pub(crate) fn complete_cross_directory_move_operation_v1<ST, DT, RT, C>(
    cas: &FsCasV1,
    cancellation_key: u64,
    version_record: PhysicalVersionRecordIdV1,
    base_root: CanonicalDirectoryTreeV1,
    root_evidence: AuthenticatedTreeMutationEvidenceV1<'_>,
    source_root_index: usize,
    expected_source_root_entry: CanonicalTreeEntryV1<'_>,
    source_directory: CanonicalDirectoryTreeV1,
    source_evidence: AuthenticatedTreeMutationEvidenceV1<'_>,
    source_removal_index: usize,
    expected_removed: CanonicalTreeEntryV1<'_>,
    source_tree_source: &mut ST,
    destination_root_index: usize,
    expected_destination_root_entry: CanonicalTreeEntryV1<'_>,
    destination_directory: CanonicalDirectoryTreeV1,
    destination_evidence: AuthenticatedTreeMutationEvidenceV1<'_>,
    destination_insertion_index: usize,
    new_name: &[u8],
    destination_tree_source: &mut DT,
    root_tree_source: &mut RT,
    buffers: C3OperationBuffersV1<'_>,
    cow_logical: &mut [u8; COMPARISON_WINDOW_BYTES],
    control: &mut C,
    counters: &mut OperationCountersV1,
) -> Result<C3HandoffV1, C3OperationErrorV1>
where
    ST: CanonicalTreeMutationSourceV1 + ?Sized,
    DT: CanonicalTreeMutationSourceV1 + ?Sized,
    RT: CanonicalTreeMutationSourceV1 + ?Sized,
    C: C3LifecycleControlV1 + ?Sized,
{
    let mut operation = request_c3_mutation_operation_v1(
        cas,
        C3MutationOperationKindV1::Move,
        cancellation_key,
        counters,
        control,
    )?;
    check_lifecycle_control_v1(control)?;
    let base_objects = operation.authenticate_base_root_v1(
        version_record,
        base_root.physical(),
        counters,
        cow_logical,
        control,
    )?;
    check_lifecycle_control_v1(control)?;

    if !matches!(
        base_root.logical(),
        DirectoryLogicalIdentityV1::ImplicitRoot(_)
    ) || source_root_index == destination_root_index
        || source_root_index >= base_root.entry_count() as usize
        || destination_root_index >= base_root.entry_count() as usize
        || expected_source_root_entry.child() != explicit_directory_child_v1(source_directory)?
        || expected_destination_root_entry.child()
            != explicit_directory_child_v1(destination_directory)?
    {
        return Err(CoreError::Path.into());
    }
    let moved_name = ValidatedComponent::new(new_name)?;

    let source_base_count = source_tree_source.declared_base_entry_count()?;
    let source_result_count = source_tree_source.declared_result_entry_count()?;
    if source_base_count != source_directory.entry_count()
        || source_result_count.checked_add(1) != Some(source_base_count)
        || source_removal_index >= source_base_count as usize
    {
        return Err(CoreError::Path.into());
    }
    let destination_base_count = destination_tree_source.declared_base_entry_count()?;
    let destination_result_count = destination_tree_source.declared_result_entry_count()?;
    if destination_base_count != destination_directory.entry_count()
        || destination_result_count
            != destination_base_count
                .checked_add(1)
                .ok_or(CoreError::CountCap)?
        || destination_insertion_index > destination_base_count as usize
    {
        return Err(CoreError::Path.into());
    }
    if root_tree_source.declared_base_entry_count()? != base_root.entry_count()
        || root_tree_source.declared_result_entry_count()? != base_root.entry_count()
    {
        return Err(CoreError::Path.into());
    }

    let source_shape = preflight_canonical_tree_v1(u64::from(source_result_count))?;
    let destination_shape = preflight_canonical_tree_v1(u64::from(destination_result_count))?;
    let root_shape = preflight_canonical_tree_v1(u64::from(base_root.entry_count()))?;
    let maximum_tree_objects = u64::from(source_shape.tree_object_count())
        .checked_add(u64::from(destination_shape.tree_object_count()))
        .and_then(|count| count.checked_add(u64::from(root_shape.tree_object_count())))
        .ok_or(CoreError::IntegerOverflow)?;
    let maximum_page_summaries = source_shape
        .page_summary_count()
        .max(destination_shape.page_summary_count())
        .max(root_shape.page_summary_count());
    let (maximum_objects, global_seen_capacity, maximum_records) =
        mutation_candidate_bounds_v1(base_objects, 0, maximum_tree_objects)?;
    ensure_mutation_buffers_v1(&buffers, maximum_objects, maximum_page_summaries)?;

    operation.declare_storage_envelope_v1(c3_storage_envelope_v1(
        maximum_objects,
        maximum_tree_objects
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?,
        0,
        0,
        maximum_tree_objects,
        0,
        global_seen_capacity,
        false,
    )?)?;

    let source_tree_resident = source_tree_source.resident_memory_bound_bytes()?;
    let destination_tree_resident = destination_tree_source.resident_memory_bound_bytes()?;
    let root_tree_resident = root_tree_source.resident_memory_bound_bytes()?;
    let tree_source_resident = source_tree_resident
        .checked_add(destination_tree_resident)
        .and_then(|bytes| bytes.checked_add(root_tree_resident))
        .ok_or(CoreError::IntegerOverflow)?;
    let source_evidence_resident = mutation_evidence_resident_bytes_v1(source_evidence)?;
    let destination_evidence_resident = mutation_evidence_resident_bytes_v1(destination_evidence)?;
    let root_evidence_resident = mutation_evidence_resident_bytes_v1(root_evidence)?;
    let evidence_resident = source_evidence_resident
        .checked_add(destination_evidence_resident)
        .and_then(|bytes| bytes.checked_add(root_evidence_resident))
        .ok_or(CoreError::IntegerOverflow)?;
    let evidence_resident =
        u64::try_from(evidence_resident).map_err(|_| CoreError::IntegerOverflow)?;
    let storage_resident = operation.storage_resident_plan_v1(false)?;
    let plan = mutation_memory_plan_v1(
        &buffers,
        storage_resident,
        0,
        tree_source_resident,
        evidence_resident,
        COMPARISON_WINDOW_BYTES as u64,
        mutation_hash_state_bytes_v1()?,
    )?;
    operation.declare_plan_v1(plan)?;
    counters.memory_high_water = counters
        .memory_high_water
        .max(operation.ledger_v1().high_water_bytes());

    let moved = CanonicalTreeEntryV1::new(moved_name, expected_removed.child());
    run_c3_lifecycle_v1(
        operation,
        C3LifecyclePlanV1 {
            global_seen_capacity,
            storage_resident,
            require_tree_storage: false,
            maximum_records,
            algorithm: C3CdcAlgorithmV1::FastCdc,
        },
        buffers,
        control,
        counters,
        move |storage, control_cell, reservation, buffers, counters| {
            if source_tree_source.resident_memory_bound_bytes()? > source_tree_resident
                || destination_tree_source.resident_memory_bound_bytes()?
                    > destination_tree_resident
                || root_tree_source.resident_memory_bound_bytes()? > root_tree_resident
            {
                return Err(CoreError::ResourceRefused.into());
            }
            let source_candidate = remove_directory_entry_cow_borrowed_v1(
                source_directory,
                source_evidence,
                source_removal_index,
                expected_removed,
                source_tree_source,
                storage.tree_sink_v1(),
                reservation,
                counters,
                buffers.tree_object,
                cow_logical,
                buffers.tree_pages,
            )?
            .directory();
            let destination_candidate = add_directory_entry_cow_borrowed_v1(
                destination_directory,
                destination_evidence,
                destination_insertion_index,
                moved,
                destination_tree_source,
                storage.tree_sink_v1(),
                reservation,
                counters,
                buffers.tree_object,
                cow_logical,
                buffers.tree_pages,
            )?
            .directory();
            let source_replacement = CanonicalTreeEntryV1::new(
                expected_source_root_entry.name(),
                explicit_directory_child_v1(source_candidate)?,
            );
            let destination_replacement = CanonicalTreeEntryV1::new(
                expected_destination_root_entry.name(),
                explicit_directory_child_v1(destination_candidate)?,
            );
            let candidate = replace_two_directory_entries_cow_borrowed_v1(
                base_root,
                root_evidence,
                source_root_index,
                expected_source_root_entry,
                source_replacement,
                destination_root_index,
                expected_destination_root_entry,
                destination_replacement,
                root_tree_source,
                storage.tree_sink_v1(),
                reservation,
                counters,
                buffers.tree_object,
                cow_logical,
                buffers.tree_pages,
            )?
            .directory();
            complete_mutation_candidate_v1(storage, control_cell, candidate, counters)
        },
    )
}

/// Run the one shared post-grant state machine. The root-owned capability is
/// borrowed continuously from preparation creation through closure handoff;
/// every explicit cleanup attempt completes before this function releases it.
// The explicit drops end the RefCell-held mutable control borrow before the
// same control is borrowed for fallible preparation cleanup below.
#[allow(clippy::too_many_arguments, clippy::drop_non_drop)]
pub(crate) fn run_c3_lifecycle_v1<C, B>(
    mut operation: C3StorageOperationV1<'_>,
    plan: C3LifecyclePlanV1,
    buffers: C3OperationBuffersV1<'_>,
    control: &mut C,
    counters: &mut OperationCountersV1,
    build: B,
) -> Result<C3HandoffV1, C3OperationErrorV1>
where
    C: CdcControlV1 + FsCasControlV1 + ?Sized,
    B: FnOnce(
        &mut dyn C3StorageSessionPortV1,
        &RefCell<&mut C>,
        &OperationReservationV1<'_>,
        &mut C3LifecycleBuildBuffersV1<'_>,
        &mut OperationCountersV1,
    ) -> Result<C3PreparedCandidateV1, C3OperationErrorV1>,
{
    let C3OperationBuffersV1 {
        source,
        cdc_ring,
        incoming_comparison,
        occupied_comparison,
        tree_object,
        tree_pages,
        traversal_state,
    } = buffers;
    let ledger = operation.ledger_v1();
    let reservation = operation.reservation_v1();
    let preparation_result = operation.begin_preparation_v1(
        plan.global_seen_capacity,
        plan.storage_resident.preparation,
        control,
    );
    let mut preparation = match preparation_result {
        Ok(preparation) => preparation,
        Err(error) => {
            let operation_terminal = operation.finish_operation_v1(false, counters, control);
            return Err(operation_terminal
                .err()
                .map_or_else(|| error.into(), C3OperationErrorV1::FsCas));
        }
    };
    let mut build_buffers = C3LifecycleBuildBuffersV1 {
        source,
        cdc_ring,
        tree_object,
        tree_pages,
    };

    let built = (|| -> Result<C3PreparedCandidateV1, C3OperationErrorV1> {
        let control_cell = RefCell::new(&mut *control);
        let storage_result = operation.begin_session_v1(
            &mut preparation,
            plan.require_tree_storage,
            incoming_comparison,
            occupied_comparison,
            plan.maximum_records,
            plan.storage_resident.private_storage,
            ledger,
            reservation,
            &control_cell,
        );
        let mut storage = match storage_result {
            Ok(storage) => storage,
            Err(error) => {
                drop(control_cell);
                return Err(error.into());
            }
        };
        let built = build(
            &mut storage,
            &control_cell,
            reservation,
            &mut build_buffers,
            counters,
        );

        let terminal = match built {
            Ok(candidate) => {
                let global_seen = storage.record_global_seen_observation_v1();
                let counter_result = counters.accumulate(storage.take_storage_counters_v1());
                global_seen?;
                counter_result?;
                Ok(candidate)
            }
            Err(error) => {
                let residue_result = storage.record_incomplete_residue_v1();
                let private_cleanup = storage.cleanup_private_pack_controlled_v1();
                let fscas_error = storage.take_first_fscas_error_v1();
                let global_seen = storage.record_global_seen_observation_v1();
                let counter_result = counters.accumulate(storage.take_storage_counters_v1());
                if let Err(cleanup) = private_cleanup {
                    Err(cleanup.into())
                } else {
                    residue_result?;
                    global_seen?;
                    counter_result?;
                    Err(fscas_error.map_or(error, C3OperationErrorV1::FsCas))
                }
            }
        };
        drop(storage);
        drop(control_cell);
        terminal
    })();

    let handoff = match built {
        Ok(candidate) => {
            let closure = operation.complete_closure_fence_v1(
                &mut preparation,
                TypedPhysicalObjectIdV1::VersionRecord(candidate.version_record),
                reservation,
                counters,
                AdmissionBuffersV1::new(
                    incoming_comparison,
                    occupied_comparison,
                    source,
                    cdc_ring,
                    traversal_state,
                ),
                plan.algorithm,
                control,
            );
            match closure {
                Ok(object_count) => Ok(C3HandoffV1 {
                    algorithm: plan.algorithm,
                    version_record: candidate.version_record,
                    root_tree: candidate.root_tree,
                    pack: candidate.completed.last_sealed(),
                    pack_outcome: candidate.completed.last_outcome(),
                    carrier_count: candidate.completed.carrier_count(),
                    carrier_rollovers: candidate.completed.carrier_count().saturating_sub(1),
                    carriers_installed: candidate.completed.carriers_installed(),
                    carriers_reused: candidate.completed.carriers_reused(),
                    object_count,
                    reference_spool_bytes: candidate.reference_spool_bytes,
                    index_spool_bytes: candidate.completed.index_spool_bytes(),
                }),
                Err(error) => {
                    counters.record_unreachable_installed_residue(
                        candidate.completed.installed_residue_bytes(),
                    )?;
                    Err(match error {
                        FsClosureAdmissionErrorV1::Core(error) => C3OperationErrorV1::Core(error),
                        FsClosureAdmissionErrorV1::FsCas(error) => C3OperationErrorV1::FsCas(error),
                    })
                }
            }
        }
        Err(error) => Err(error),
    };

    let cleanup = preparation.finish(control);
    let commit_storage = handoff.is_ok() && cleanup.is_ok();
    let operation_terminal = operation.finish_operation_v1(commit_storage, counters, control);
    if let Err(cleanup) = cleanup {
        return Err(cleanup.into());
    }
    if let Err(operation) = operation_terminal {
        return Err(operation.into());
    }
    if handoff.is_ok() {
        control.boundary_reached(FsCasBoundaryV1::AfterCompleteValidatedHandoff);
    }
    handoff
}
