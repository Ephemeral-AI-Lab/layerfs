//! Shared complete-content operation lifecycle.
//!
//! This coordinator is the only owner of the preparation-to-handoff state
//! machine used by both one-file and multi-entry Create. Content builders run
//! only inside its borrowed semantic storage session; concrete filesystem,
//! pack, locator, and closure implementations remain below their private
//! ports.

#[cfg(feature = "operation-polymorphism")]
mod preparation;

#[cfg(feature = "operation-polymorphism")]
pub(crate) use preparation::{
    FileBuiltDirectorySpoolV1, FileBuiltFileSpoolV1, FileChunkReferenceSpoolV1,
    OperationPreparationV1, PreparationErrorV1,
};

use std::cell::RefCell;

use crate::cas::{
    authenticate_base_root_storage_v1, begin_storage_session_v1, complete_closure_fence_storage_v1,
    locator_publication_receipt_preparation_bytes_bound_v1, AdmissionBuffersV1,
    ClosureFenceStorageOutcomeV1, FsCasBoundaryV1, FsCasCleanupTargetV1, FsCasControlV1,
    FsCasErrorV1, FsCasV1, FsClosureAdmissionErrorV1, FsOperationCapabilityV1, FsOperationKindV1,
    FsOperationObservedControlV1, FsPackAdmissionOutcomeV1, FsStorageEnvelopeV1,
    FsStorageOperationTokenV1, StorageSessionV1, CATALOG_MARKER_BYTES, CLOSURE_MARKER_BYTES,
    GLOBAL_SEEN_RECORD_BYTES, PERSISTENT_LOCATOR_BYTES_V1,
};
use crate::cdc::{CdcAlgorithmV1, CdcControlV1, MAXIMUM_CHUNK_BYTES};
use crate::content::update::{
    authenticate_base_file_evidence_v1, reencode_file_metadata_borrowed_v1,
    update_file_borrowed_v1, AuthenticatedBaseByteReaderV1, BaseChunkEvidenceSourceV1,
    UpdateBuffersV1, MAX_UPDATE_RESYNCHRONIZATION_BYTES,
};
use crate::content::{
    replace_file_borrowed_v1, ChunkReferenceSpoolV1, ContentBuffersV1, ContentSourceV1,
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
    OptionalU64ObservationV1, TerminalOptionalObservationsV1,
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
// The preparation namespace has five base spools that remain open for the
// operation lifetime. A storage-session phase adds one private carrier path;
// the later marker-publication phase uses one private marker path instead of
// that carrier path. The two optional tree spools remain open whenever tree
// storage is requested. Keep these phase names explicit: the fixed count is
// not a guess about how many prefixes happen to exist in one test.
const NAMED_NON_TREE_PREPARATION_SPOOLS_V1: u64 = 5;
const NAMED_PRIVATE_PACK_PREPARATION_PATH_V1: u64 = 1;
const NAMED_MARKER_PREPARATION_PATH_V1: u64 = 1;
const NAMED_TREE_PREPARATION_SPOOLS_V1: u64 = 2;
const FIXED_PREPARATION_NAMESPACE_ENTRIES_V1: u64 =
    NAMED_NON_TREE_PREPARATION_SPOOLS_V1 + NAMED_PRIVATE_PACK_PREPARATION_PATH_V1;

fn maximum_simultaneous_preparation_names_v1(require_tree_storage: bool) -> CoreResult<u64> {
    let tree_names = if require_tree_storage {
        NAMED_TREE_PREPARATION_SPOOLS_V1
    } else {
        0
    };
    let long_lived = NAMED_NON_TREE_PREPARATION_SPOOLS_V1
        .checked_add(tree_names)
        .ok_or(CoreError::IntegerOverflow)?;
    let storage_session = long_lived
        .checked_add(NAMED_PRIVATE_PACK_PREPARATION_PATH_V1)
        .ok_or(CoreError::IntegerOverflow)?;
    let marker_publication = long_lived
        .checked_add(NAMED_MARKER_PREPARATION_PATH_V1)
        .ok_or(CoreError::IntegerOverflow)?;
    Ok(storage_session.max(marker_publication))
}

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
    fn locator_receipt_spool_high_water_is_checked_and_charged() {
        let record_bytes = (PERSISTENT_LOCATOR_BYTES_V1 + 24) as u64;
        assert_eq!(record_bytes, 184);
        assert_eq!(
            locator_publication_receipt_preparation_bytes_bound_v1(1),
            Ok(record_bytes)
        );
        assert_eq!(
            locator_publication_receipt_preparation_bytes_bound_v1(25_600),
            Ok(25_600 * 184)
        );
        assert_eq!(
            locator_publication_receipt_preparation_bytes_bound_v1(MAX_PACK_RECORDS as u64),
            Ok(466_032 * 184)
        );
        assert_eq!(
            locator_publication_receipt_preparation_bytes_bound_v1(u64::MAX),
            Err(CoreError::IntegerOverflow)
        );
    }

    #[test]
    fn preparation_namespace_entry_envelope_uses_the_phase_maximum() {
        assert_eq!(
            NAMED_NON_TREE_PREPARATION_SPOOLS_V1, 5,
            "references, pack-index, closure, global-seen, and locator receipts"
        );
        assert_eq!(NAMED_PRIVATE_PACK_PREPARATION_PATH_V1, 1);
        assert_eq!(NAMED_MARKER_PREPARATION_PATH_V1, 1);
        assert_eq!(NAMED_TREE_PREPARATION_SPOOLS_V1, 2);
        assert_eq!(FIXED_PREPARATION_NAMESPACE_ENTRIES_V1, 6);
        assert_eq!(maximum_simultaneous_preparation_names_v1(false), Ok(6));
        assert_eq!(maximum_simultaneous_preparation_names_v1(true), Ok(8));
        assert_eq!(
            FIXED_PREPARATION_NAMESPACE_ENTRIES_V1 + NAMED_TREE_PREPARATION_SPOOLS_V1,
            maximum_simultaneous_preparation_names_v1(true).unwrap()
        );
    }

    #[test]
    fn total_storage_envelope_decomposes_receipts_and_all_other_components() {
        let maximum_candidate_objects = 7_u64;
        let maximum_new_objects = 5_u64;
        let maximum_chunk_references = 11_u64;
        let maximum_files = 2_u64;
        let maximum_tree_objects = 3_u64;
        let maximum_logical_payload_bytes = 1_000_u64;
        let global_seen_capacity = 16_u32;

        let receipts = locator_publication_receipt_preparation_bytes_bound_v1(5).unwrap();
        let references = maximum_chunk_references * CHUNK_REFERENCE_PREPARATION_BYTES_V1;
        let index = maximum_new_objects * PACK_INDEX_ENTRY_BYTES;
        let closure = maximum_candidate_objects * CLOSURE_OBJECT_PREPARATION_BYTES_V1;
        let seen = u64::from(global_seen_capacity) * GLOBAL_SEEN_RECORD_BYTES;
        let tree = maximum_files * BUILT_FILE_PREPARATION_BYTES_V1
            + maximum_tree_objects * BUILT_DIRECTORY_PREPARATION_BYTES_V1;
        let marker = PERSISTENT_LOCATOR_BYTES_V1
            .max(CATALOG_MARKER_BYTES)
            .max(CLOSURE_MARKER_BYTES) as u64;
        let preparation_bytes =
            references + index + closure + seen + tree + receipts + MAX_PACK_BYTES + marker;

        let file_metadata = maximum_chunk_references * FILE_EXTENT_CANONICAL_BYTES_V1
            + maximum_files * (OBJECT_HEADER_BYTES + FILE_FIXED_CANONICAL_BYTES_V1);
        let tree_metadata = maximum_tree_objects * MAX_TREE_OBJECT_BYTES as u64;
        let canonical_bytes = maximum_logical_payload_bytes
            + file_metadata
            + tree_metadata
            + OBJECT_HEADER_BYTES
            + VERSION_RECORD_PAYLOAD_BYTES;
        let pack_content_bytes = canonical_bytes
            + maximum_new_objects * (PACK_OBJECT_RECORD_BYTES_V1 + PACK_INDEX_ENTRY_BYTES);
        let maximum_carriers = 2_u64;
        let immutable_bytes = pack_content_bytes
            + maximum_carriers * (PACK_HEADER_BYTES + PACK_TRAILER_BYTES)
            + maximum_new_objects * PERSISTENT_LOCATOR_BYTES_V1 as u64
            + maximum_carriers * CATALOG_MARKER_BYTES as u64
            + CLOSURE_MARKER_BYTES as u64;
        let immutable_namespace_entries = maximum_new_objects + maximum_carriers * 2 + 2;

        assert_eq!(receipts, 5 * 184);
        assert_eq!(
            storage_envelope_v1(
                maximum_candidate_objects,
                maximum_new_objects,
                maximum_chunk_references,
                maximum_files,
                maximum_tree_objects,
                maximum_logical_payload_bytes,
                global_seen_capacity,
                true,
            ),
            FsStorageEnvelopeV1::new(
                preparation_bytes,
                immutable_bytes,
                8,
                immutable_namespace_entries,
            )
        );
    }

    #[test]
    fn total_storage_envelope_rejects_checked_aggregate_overflow_at_receipts() {
        let index = PACK_INDEX_ENTRY_BYTES;
        let maximum_candidate_objects = (u64::MAX - index) / CLOSURE_OBJECT_PREPARATION_BYTES_V1;
        let closure = maximum_candidate_objects * CLOSURE_OBJECT_PREPARATION_BYTES_V1;
        let before_receipts = index.checked_add(closure).unwrap();
        let receipts = locator_publication_receipt_preparation_bytes_bound_v1(1).unwrap();
        assert_eq!(receipts, 184);
        assert!(before_receipts.checked_add(receipts).is_none());
        assert_eq!(
            storage_envelope_v1(maximum_candidate_objects, 1, 0, 0, 0, 0, 0, false,),
            Err(CoreError::IntegerOverflow)
        );
    }

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
pub(crate) fn storage_envelope_v1(
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
    let locator_receipt_preparation =
        locator_publication_receipt_preparation_bytes_bound_v1(maximum_pack_records)?;
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
        .and_then(|bytes| bytes.checked_add(locator_receipt_preparation))
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
    let preparation_inodes = maximum_simultaneous_preparation_names_v1(require_tree_storage)?;
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

fn check_lifecycle_control_v1<C: LifecycleControlV1 + ?Sized>(control: &mut C) -> CoreResult<()> {
    if FsCasControlV1::cancellation_requested(control) {
        Err(CoreError::Cancelled)
    } else if FsCasControlV1::deadline_exceeded(control) {
        Err(CoreError::Deadline)
    } else {
        Ok(())
    }
}

pub(crate) const MAX_STORAGE_RECORDS_V1: u64 = crate::pack::MAX_PACK_RECORDS;

pub(crate) fn admission_traversal_resident_bytes_v1() -> CoreResult<u64> {
    crate::cas::admission_traversal_resident_bytes_v1()
}

/// Lifecycle-level control contract. Content code does not depend on the
/// concrete filesystem CAS control surface; the root lifecycle adapter is the
/// sole bridge to both CDC and durable storage cancellation/fault boundaries.
pub(crate) trait LifecycleControlV1: CdcControlV1 + FsCasControlV1 {}

impl<T> LifecycleControlV1 for T where T: CdcControlV1 + FsCasControlV1 + ?Sized {}

pub(crate) struct SharedOperationControlV1<'cell, 'control, C: ?Sized> {
    inner: &'cell RefCell<&'control mut C>,
}

impl<'cell, 'control, C: ?Sized> SharedOperationControlV1<'cell, 'control, C> {
    pub(crate) fn new(inner: &'cell RefCell<&'control mut C>) -> Self {
        Self { inner }
    }
}

impl<C: CdcControlV1 + ?Sized> CdcControlV1 for SharedOperationControlV1<'_, '_, C> {
    fn cancellation_requested(&mut self) -> bool {
        (**self.inner.borrow_mut()).cancellation_requested()
    }

    fn deadline_exceeded(&mut self) -> bool {
        (**self.inner.borrow_mut()).deadline_exceeded()
    }
}

impl<C: FsCasControlV1 + ?Sized> FsCasControlV1 for SharedOperationControlV1<'_, '_, C> {
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
    fn inject_carrier_receipt_transition_failure_v1(
        &mut self,
        check: crate::cas::CarrierReceiptTransitionCheckV1,
    ) -> Option<crate::cas::FsCasErrorV1> {
        (**self.inner.borrow_mut()).inject_carrier_receipt_transition_failure_v1(check)
    }

    #[cfg(test)]
    fn inject_operation_terminal_unwind_after_release(&mut self) -> bool {
        (**self.inner.borrow_mut()).inject_operation_terminal_unwind_after_release()
    }

    #[cfg(test)]
    fn inject_root_lock_observation_failure(&mut self) -> Option<CoreError> {
        (**self.inner.borrow_mut()).inject_root_lock_observation_failure()
    }

    #[cfg(test)]
    fn inject_carrier_counter_accumulation_overflow(&mut self) -> bool {
        (**self.inner.borrow_mut()).inject_carrier_counter_accumulation_overflow()
    }

    #[cfg(test)]
    fn inject_global_seen_counter_accumulation_overflow(&mut self) -> bool {
        (**self.inner.borrow_mut()).inject_global_seen_counter_accumulation_overflow()
    }

    #[cfg(test)]
    fn inject_pack_object_disposition_overflow(&mut self, created: bool) -> bool {
        (**self.inner.borrow_mut()).inject_pack_object_disposition_overflow(created)
    }

    #[cfg(test)]
    fn inject_operation_spool_write_observation_overflow(&mut self) -> bool {
        (**self.inner.borrow_mut()).inject_operation_spool_write_observation_overflow()
    }

    #[cfg(test)]
    fn inject_operation_spool_precharge_failure_v1(&mut self) -> Option<crate::cas::FsCasErrorV1> {
        (**self.inner.borrow_mut()).inject_operation_spool_precharge_failure_v1()
    }

    #[cfg(test)]
    fn inject_counted_pack_read_observation_overflow(&mut self) -> bool {
        (**self.inner.borrow_mut()).inject_counted_pack_read_observation_overflow()
    }

    #[cfg(test)]
    fn inject_same_carrier_comparison_observation_overflow(&mut self) -> bool {
        (**self.inner.borrow_mut()).inject_same_carrier_comparison_observation_overflow()
    }

    #[cfg(test)]
    fn inject_pending_unwind_retirement_failure(&mut self) -> Option<crate::cas::FsCasErrorV1> {
        (**self.inner.borrow_mut()).inject_pending_unwind_retirement_failure()
    }

    #[cfg(test)]
    fn inject_root_lock_post_acquire_validation_failure(
        &mut self,
    ) -> Option<crate::cas::FsCasErrorV1> {
        (**self.inner.borrow_mut()).inject_root_lock_post_acquire_validation_failure()
    }
}

#[derive(Clone, Copy)]
pub(crate) struct PreparationResidentBoundsV1 {
    pub(crate) references: u64,
    pub(crate) metadata: u64,
    pub(crate) closure_objects: u64,
    pub(crate) global_seen: u64,
    pub(crate) locator_receipts: u64,
    pub(crate) built_files: Option<u64>,
    pub(crate) built_directories: Option<u64>,
}

/// Opaque lower-storage residency declaration consumed by the shared
/// lifecycle. Content orchestration may charge the aggregate, but cannot name
/// or construct concrete preparation, carrier, locator, or closure adapters.
#[derive(Clone, Copy)]
pub(crate) struct StorageResidentPlanV1 {
    preparation: PreparationResidentBoundsV1,
    private_storage: u64,
    locator_receipts: u64,
    total: u64,
}

impl StorageResidentPlanV1 {
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
pub(crate) trait StorageSessionPortV1 {
    fn content_parts_v1(
        &mut self,
    ) -> (
        &mut (dyn ChunkReferenceSpoolV1 + '_),
        &mut (dyn PreparedObjectSinkV1 + '_),
    );
    fn tree_sink_v1(&mut self) -> &mut (dyn PreparedTreeSinkV1 + '_);
    fn reference_storage_bytes_v1(&self) -> CoreResult<OptionalU64ObservationV1>;
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
    ) -> Result<VersionSummaryInputV1, OperationErrorV1>;
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
    fn take_first_core_error_v1(&mut self) -> Option<CoreError>;
    fn take_first_fscas_error_v1(&mut self) -> Option<FsCasErrorV1>;
    fn record_global_seen_observation_v1(&mut self) -> CoreResult<()>;
    fn take_storage_counters_v1(&mut self) -> OperationCountersV1;
}

/// Opaque, non-cloneable root-owned operation capability. Lifecycle is the
/// only owner that can turn an admitted ticket into preparation and handoff.
pub(crate) struct StorageOperationV1<'root> {
    capability: FsOperationCapabilityV1<'root>,
}

pub(crate) struct CreateOperationGrantV1<'root> {
    operation: StorageOperationV1<'root>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MutationOperationKindV1 {
    Replace,
    Update,
    Add,
    Remove,
    Move,
    Metadata,
}

impl MutationOperationKindV1 {
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
pub(crate) fn request_mutation_operation_v1<'root, C>(
    cas: &'root FsCasV1,
    kind: MutationOperationKindV1,
    cancellation_key: u64,
    counters: &mut OperationCountersV1,
    control: &mut C,
) -> Result<StorageOperationV1<'root>, FsCasErrorV1>
where
    C: FsCasControlV1 + ?Sized,
{
    control.boundary_reached(FsCasBoundaryV1::BeforeOperationSlotReservationRequest);
    Ok(StorageOperationV1 {
        capability: cas.begin_operation_capability_v1(
            kind.storage_kind_v1(),
            cancellation_key,
            counters,
            control,
        )?,
    })
}

pub(crate) fn request_create_operation_v1<'root, C>(
    cas: &'root FsCasV1,
    cancellation_key: u64,
    counters: &mut OperationCountersV1,
    control: &mut C,
) -> Result<CreateOperationGrantV1<'root>, FsCasErrorV1>
where
    C: FsCasControlV1 + ?Sized,
{
    control.boundary_reached(FsCasBoundaryV1::BeforeOperationSlotReservationRequest);
    Ok(CreateOperationGrantV1 {
        operation: StorageOperationV1 {
            capability: cas.begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                cancellation_key,
                counters,
                control,
            )?,
        },
    })
}

pub(crate) fn request_tree_operation_v1<'root, C>(
    cas: &'root FsCasV1,
    cancellation_key: u64,
    counters: &mut OperationCountersV1,
    control: &mut C,
) -> Result<StorageOperationV1<'root>, FsCasErrorV1>
where
    C: FsCasControlV1 + ?Sized,
{
    control.boundary_reached(FsCasBoundaryV1::BeforeOperationSlotReservationRequest);
    Ok(StorageOperationV1 {
        capability: cas.begin_operation_capability_v1(
            FsOperationKindV1::CompleteC3Tree,
            cancellation_key,
            counters,
            control,
        )?,
    })
}

impl<'root> CreateOperationGrantV1<'root> {
    pub(crate) fn into_operation(self) -> StorageOperationV1<'root> {
        self.operation
    }
}

impl StorageOperationV1<'_> {
    pub(crate) fn require_operation_kind_v1(
        &self,
        expected: FsOperationKindV1,
    ) -> Result<(), FsCasErrorV1> {
        self.capability.require_operation_kind_v1(expected)
    }

    pub(crate) fn require_complete_file_kind_v1(&self) -> Result<(), FsCasErrorV1> {
        self.require_operation_kind_v1(FsOperationKindV1::CompleteC3File)
    }

    pub(crate) fn require_complete_tree_kind_v1(&self) -> Result<(), FsCasErrorV1> {
        self.require_operation_kind_v1(FsOperationKindV1::CompleteC3Tree)
    }

    pub(crate) fn declare_empty_storage_envelope_v1(&mut self) -> Result<(), FsCasErrorV1> {
        self.declare_storage_envelope_v1(FsStorageEnvelopeV1::new(0, 0, 0, 0)?)
    }

    pub(crate) fn declare_plan_v1(&mut self, plan: OperationMemoryPlanV1) -> Result<(), CoreError> {
        self.capability.declare_plan_v1(plan)
    }

    pub(crate) fn declare_storage_envelope_v1(
        &mut self,
        envelope: FsStorageEnvelopeV1,
    ) -> Result<(), FsCasErrorV1> {
        self.capability.declare_storage_envelope_v1(envelope)
    }

    /// Run the preparation-free portion of one already-admitted operation.
    /// Any typed error or unwind after the root grant is balanced through the
    /// same explicit storage/admission terminal path used by full lifecycle;
    /// the capability remains live on success and is then consumed by
    /// `run_lifecycle_v1`.
    pub(crate) fn run_preparation_free_stage_v1<T, C, F>(
        &mut self,
        counters: &mut OperationCountersV1,
        control: &mut C,
        body: F,
    ) -> Result<T, OperationErrorV1>
    where
        C: LifecycleControlV1 + ?Sized,
        F: FnOnce(&mut Self, &mut OperationCountersV1, &mut C) -> Result<T, OperationErrorV1>,
    {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            body(self, counters, control)
        })) {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => match self.finish_operation_caught_v1(false, counters, control) {
                Ok(Ok(())) => Err(error),
                Ok(Err(terminal)) => Err(error.dominated_by_fscas_v1(terminal)),
                Err(terminal_payload) => {
                    drop(terminal_payload);
                    Err(error)
                }
            },
            Err(payload) => {
                match self.finish_operation_caught_v1(false, counters, control) {
                    Ok(Ok(())) => std::panic::resume_unwind(payload),
                    Ok(Err(terminal)) => {
                        // A typed terminal accounting, release, cleanup, or
                        // invalidation failure is the machine-readable
                        // operation outcome. Consume the initiating callback
                        // payload only after both terminal halves have been
                        // attempted; do not replace this cause with a string
                        // panic.
                        drop(payload);
                        Err(OperationErrorV1::FsCas(terminal))
                    }
                    Err(terminal_payload) => {
                        // Both payloads are non-typed. Resume the initiating
                        // callback unwind because it happened first.
                        drop(terminal_payload);
                        std::panic::resume_unwind(payload)
                    }
                }
            }
        }
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
        self.capability
            .finish_terminal_v1(commit, counters, control)
    }

    fn finish_operation_caught_v1<C>(
        &mut self,
        commit: bool,
        counters: &mut OperationCountersV1,
        control: &mut C,
    ) -> std::thread::Result<Result<(), FsCasErrorV1>>
    where
        C: FsCasControlV1 + ?Sized,
    {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.finish_operation_v1(commit, counters, control)
        }))
    }

    fn storage_token_v1(&self) -> Result<FsStorageOperationTokenV1, FsCasErrorV1> {
        self.capability.storage_token_v1()
    }

    pub(crate) fn memory_high_water_bytes_v1(&self) -> u64 {
        self.capability.memory_high_water_bytes_v1()
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
    ) -> Result<u64, OperationErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        authenticate_base_root_storage_v1(
            self.capability.owner_ref_v1(),
            self.storage_token_v1()?,
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
        _maximum_records: u32,
    ) -> Result<StorageResidentPlanV1, CoreError> {
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
        let locator_receipts =
            owner.operation_spool_resident_memory_bound_v1("locator-receipts")?;
        let preparation = PreparationResidentBoundsV1 {
            references,
            metadata,
            closure_objects,
            global_seen,
            locator_receipts,
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
            .and_then(|bytes| bytes.checked_add(locator_receipts))
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(StorageResidentPlanV1 {
            preparation,
            private_storage,
            locator_receipts,
            total,
        })
    }

    fn begin_preparation_v1<C>(
        &self,
        global_seen_capacity: u32,
        bounds: PreparationResidentBoundsV1,
        control: &mut C,
    ) -> Result<OperationPreparationV1, PreparationErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        OperationPreparationV1::begin(
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
        begin_storage_session_v1(
            self.capability.owner_ref_v1(),
            self.storage_token_v1()?,
            preparation,
            require_tree_storage,
            left,
            right,
            maximum_records,
            private_pack_resident_bound,
            reservation,
            control,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn complete_closure_fence_v1<C>(
        &self,
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
pub(crate) enum OperationErrorV1 {
    Core(CoreError),
    FsCas(FsCasErrorV1),
}

impl OperationErrorV1 {
    fn into_fscas_v1(self) -> FsCasErrorV1 {
        match self {
            Self::Core(error) => FsCasErrorV1::Core(error),
            Self::FsCas(error) => error,
        }
    }

    fn dominated_by_fscas_v1(self, dominant: FsCasErrorV1) -> Self {
        if dominant.has_cleanup_or_invalidation_dominance_v1() {
            Self::FsCas(self.into_fscas_v1().dominated_by_v1(dominant))
        } else {
            self
        }
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

    fn reconcile_unwind_terminal_v1<T>(
        current: Option<Self>,
        terminal: Result<T, Self>,
    ) -> Option<Self> {
        match terminal {
            Err(Self::Core(CoreError::SourceFailure)) => current,
            Err(error) => Self::retain_terminal_v1(current, error),
            Ok(_) => Self::retain_terminal_v1(current, Self::Core(CoreError::PackInvalid)),
        }
    }
}

impl From<CoreError> for OperationErrorV1 {
    fn from(error: CoreError) -> Self {
        Self::Core(error)
    }
}

impl From<FsCasErrorV1> for OperationErrorV1 {
    fn from(error: FsCasErrorV1) -> Self {
        Self::FsCas(error)
    }
}

impl From<PreparationErrorV1> for OperationErrorV1 {
    fn from(error: PreparationErrorV1) -> Self {
        match error {
            PreparationErrorV1::Core(error) => Self::Core(error),
            PreparationErrorV1::FsCas(error) => Self::FsCas(error),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct OperationHandoffV1 {
    algorithm: CdcAlgorithmV1,
    version_record: PhysicalVersionRecordIdV1,
    root_tree: PhysicalTreeIdV1,
    pack: SealedPackV1,
    pack_outcome: FsPackAdmissionOutcomeV1,
    carrier_count: u32,
    carrier_rollovers: u32,
    carriers_installed: u32,
    carriers_reused: u32,
    object_count: u64,
    reference_spool_bytes: OptionalU64ObservationV1,
    index_spool_bytes: OptionalU64ObservationV1,
    terminal_optional_observations: TerminalOptionalObservationsV1,
}

impl OperationHandoffV1 {
    pub(crate) const fn algorithm(self) -> CdcAlgorithmV1 {
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

    pub(crate) const fn reference_spool_bytes(self) -> OptionalU64ObservationV1 {
        self.reference_spool_bytes
    }

    pub(crate) const fn index_spool_bytes(self) -> OptionalU64ObservationV1 {
        self.index_spool_bytes
    }

    pub(crate) const fn terminal_optional_observations(self) -> TerminalOptionalObservationsV1 {
        self.terminal_optional_observations
    }
}

pub(crate) struct OperationBuffersV1<'a> {
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
pub(crate) struct LifecycleBuildBuffersV1<'a> {
    pub(crate) source: &'a mut [u8; MAXIMUM_CHUNK_BYTES],
    pub(crate) cdc_ring: &'a mut [u8; MAXIMUM_CHUNK_BYTES],
    pub(crate) tree_object: &'a mut [u8; MAX_TREE_OBJECT_BYTES],
    pub(crate) tree_pages: &'a mut [Option<TreePageSummaryV1>],
}

pub(crate) struct LifecyclePlanV1 {
    pub(crate) global_seen_capacity: u32,
    pub(crate) storage_resident: StorageResidentPlanV1,
    pub(crate) require_tree_storage: bool,
    pub(crate) maximum_records: u32,
    pub(crate) algorithm: CdcAlgorithmV1,
}

pub(crate) struct PreparedCandidateV1 {
    version_record: PhysicalVersionRecordIdV1,
    root_tree: PhysicalTreeIdV1,
    completed: CompletedPackSetV1,
    reference_spool_bytes: OptionalU64ObservationV1,
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
    let maximum_records = u32::try_from(maximum_objects.min(MAX_STORAGE_RECORDS_V1))
        .map_err(|_| CoreError::IntegerOverflow)?;
    Ok((maximum_objects, global_seen_capacity, maximum_records))
}

fn ensure_mutation_buffers_v1(
    buffers: &OperationBuffersV1<'_>,
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
    buffers: &OperationBuffersV1<'_>,
    storage_resident: StorageResidentPlanV1,
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
            admission_traversal_resident_bytes_v1()?
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
    storage: &mut dyn StorageSessionPortV1,
    control_cell: &RefCell<&mut C>,
    candidate: CanonicalDirectoryTreeV1,
    counters: &mut OperationCountersV1,
) -> Result<PreparedCandidateV1, OperationErrorV1>
where
    C: LifecycleControlV1 + ?Sized,
{
    let DirectoryLogicalIdentityV1::ImplicitRoot(logical_root) = candidate.logical() else {
        return Err(CoreError::TypeDomain.into());
    };
    let summary = storage.rebuild_candidate_closure_v1(
        candidate.physical(),
        counters,
        &mut SharedOperationControlV1::new(control_cell),
    )?;
    let version = storage.write_version_v1(
        derive_version_v1(logical_root),
        candidate.physical(),
        summary,
        counters,
    )?;
    let completed = storage.complete_v1(version)?;
    let reference_spool_bytes = storage.reference_storage_bytes_v1()?;
    Ok(PreparedCandidateV1::new(
        version,
        candidate.physical(),
        completed,
        reference_spool_bytes,
    ))
}

impl PreparedCandidateV1 {
    pub(crate) const fn new(
        version_record: PhysicalVersionRecordIdV1,
        root_tree: PhysicalTreeIdV1,
        completed: CompletedPackSetV1,
        reference_spool_bytes: OptionalU64ObservationV1,
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
pub(crate) fn run_complete_replace_v1<S, C>(
    cas: &FsCasV1,
    cancellation_key: u64,
    algorithm: CdcAlgorithmV1,
    version_record: PhysicalVersionRecordIdV1,
    base_root: CanonicalDirectoryTreeV1,
    replacement_evidence: AuthenticatedTreeReplacementEvidenceV1<'_>,
    replacement_index: usize,
    name: &[u8],
    mode: u16,
    declared_len: u64,
    source: &mut S,
    buffers: OperationBuffersV1<'_>,
    cow_logical: &mut [u8; COMPARISON_WINDOW_BYTES],
    control: &mut C,
    counters: &mut OperationCountersV1,
) -> Result<OperationHandoffV1, OperationErrorV1>
where
    S: ContentSourceV1 + ?Sized,
    C: LifecycleControlV1 + ?Sized,
{
    let mut operation = request_mutation_operation_v1(
        cas,
        MutationOperationKindV1::Replace,
        cancellation_key,
        counters,
        control,
    )?;
    let (component, global_seen_capacity, maximum_records, source_resident, storage_resident) =
        operation.run_preparation_free_stage_v1(
            counters,
            control,
            |operation, counters, control| {
                operation.declare_storage_envelope_v1(FsStorageEnvelopeV1::new(0, 0, 0, 0)?)?;
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
                let (maximum_objects, global_seen_capacity, maximum_records) =
                    mutation_candidate_bounds_v1(
                        base_objects,
                        maximum_refs
                            .checked_add(1)
                            .ok_or(CoreError::IntegerOverflow)?,
                        u64::from(tree_shape.tree_object_count()),
                    )?;
                ensure_mutation_buffers_v1(
                    &buffers,
                    maximum_objects,
                    tree_shape.page_summary_count(),
                )?;
                operation.declare_storage_envelope_v1(storage_envelope_v1(
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
                let storage_resident =
                    operation.storage_resident_plan_v1(false, maximum_records)?;
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
                    .max(operation.memory_high_water_bytes_v1());
                Ok((
                    component,
                    global_seen_capacity,
                    maximum_records,
                    source_resident,
                    storage_resident,
                ))
            },
        )?;

    run_lifecycle_v1(
        operation,
        LifecyclePlanV1 {
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
                replace_file_borrowed_v1(
                    name,
                    mode,
                    declared_len,
                    source,
                    sink,
                    references,
                    ContentBuffersV1::new(buffers.source, buffers.cdc_ring),
                    &mut SharedOperationControlV1::new(control_cell),
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
pub(crate) fn run_complete_update_v1<S, B, E, C>(
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
    buffers: OperationBuffersV1<'_>,
    cow_logical: &mut [u8; COMPARISON_WINDOW_BYTES],
    control: &mut C,
    counters: &mut OperationCountersV1,
) -> Result<OperationHandoffV1, OperationErrorV1>
where
    S: ContentSourceV1 + ?Sized,
    B: AuthenticatedBaseByteReaderV1 + ?Sized,
    E: BaseChunkEvidenceSourceV1 + ?Sized,
    C: LifecycleControlV1 + ?Sized,
{
    let mut operation = request_mutation_operation_v1(
        cas,
        MutationOperationKindV1::Update,
        cancellation_key,
        counters,
        control,
    )?;
    let (
        component,
        global_seen_capacity,
        maximum_records,
        source_resident,
        base_reader_resident,
        chunk_evidence_resident,
        storage_resident,
    ) = operation.run_preparation_free_stage_v1(
        counters,
        control,
        |operation, counters, control| {
            operation.declare_storage_envelope_v1(FsStorageEnvelopeV1::new(0, 0, 0, 0)?)?;
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
            let (maximum_objects, global_seen_capacity, maximum_records) =
                mutation_candidate_bounds_v1(
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
            operation.declare_storage_envelope_v1(storage_envelope_v1(
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
            let storage_resident = operation.storage_resident_plan_v1(false, maximum_records)?;
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
                .max(operation.memory_high_water_bytes_v1());
            Ok((
                component,
                global_seen_capacity,
                maximum_records,
                source_resident,
                base_reader_resident,
                chunk_evidence_resident,
                storage_resident,
            ))
        },
    )?;

    run_lifecycle_v1(
        operation,
        LifecyclePlanV1 {
            global_seen_capacity,
            storage_resident,
            require_tree_storage: false,
            maximum_records,
            algorithm: CdcAlgorithmV1::FastCdc,
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
                update_file_borrowed_v1(
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
                    &mut SharedOperationControlV1::new(control_cell),
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
pub(crate) fn run_complete_add_v1<S, T, C>(
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
    buffers: OperationBuffersV1<'_>,
    cow_logical: &mut [u8; COMPARISON_WINDOW_BYTES],
    control: &mut C,
    counters: &mut OperationCountersV1,
) -> Result<OperationHandoffV1, OperationErrorV1>
where
    S: ContentSourceV1 + ?Sized,
    T: CanonicalTreeMutationSourceV1 + ?Sized,
    C: LifecycleControlV1 + ?Sized,
{
    let mut operation = request_mutation_operation_v1(
        cas,
        MutationOperationKindV1::Add,
        cancellation_key,
        counters,
        control,
    )?;
    let (
        component,
        global_seen_capacity,
        maximum_records,
        source_resident,
        tree_source_resident,
        storage_resident,
    ) = operation.run_preparation_free_stage_v1(
        counters,
        control,
        |operation, counters, control| {
            operation.declare_storage_envelope_v1(FsStorageEnvelopeV1::new(0, 0, 0, 0)?)?;
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
            let base_entry_count = base_root.entry_count();
            let result_entry_count = base_entry_count
                .checked_add(1)
                .ok_or(CoreError::IntegerOverflow)?;
            if insertion_index > base_entry_count as usize {
                return Err(CoreError::Path.into());
            }
            let maximum_refs = declared_len
                .checked_add(8_191)
                .ok_or(CoreError::IntegerOverflow)?
                / 8_192;
            validate_chunk_refs_per_file(maximum_refs)?;
            let result_shape = preflight_canonical_tree_v1(u64::from(result_entry_count))?;
            let (maximum_objects, global_seen_capacity, maximum_records) =
                mutation_candidate_bounds_v1(
                    base_objects,
                    maximum_refs
                        .checked_add(1)
                        .ok_or(CoreError::IntegerOverflow)?,
                    u64::from(result_shape.tree_object_count()),
                )?;
            ensure_mutation_buffers_v1(
                &buffers,
                maximum_objects,
                result_shape.page_summary_count(),
            )?;
            operation.declare_storage_envelope_v1(storage_envelope_v1(
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

            // Declared tree counts are semantic source callbacks. Validate
            // them only after the result shape has reserved its final
            // conservative storage envelope.
            if tree_source.declared_base_entry_count()? != base_entry_count
                || tree_source.declared_result_entry_count()? != result_entry_count
            {
                return Err(CoreError::Path.into());
            }
            let source_resident = source.resident_memory_bound_bytes()?;
            let tree_source_resident = tree_source.resident_memory_bound_bytes()?;
            let storage_resident = operation.storage_resident_plan_v1(false, maximum_records)?;
            let evidence_resident =
                u64::try_from(mutation_evidence_resident_bytes_v1(mutation_evidence)?)
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
                .max(operation.memory_high_water_bytes_v1());
            Ok((
                component,
                global_seen_capacity,
                maximum_records,
                source_resident,
                tree_source_resident,
                storage_resident,
            ))
        },
    )?;

    run_lifecycle_v1(
        operation,
        LifecyclePlanV1 {
            global_seen_capacity,
            storage_resident,
            require_tree_storage: false,
            maximum_records,
            algorithm: CdcAlgorithmV1::FastCdc,
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
                replace_file_borrowed_v1(
                    name,
                    mode,
                    declared_len,
                    source,
                    sink,
                    references,
                    ContentBuffersV1::new(buffers.source, buffers.cdc_ring),
                    &mut SharedOperationControlV1::new(control_cell),
                    reservation,
                    CdcAlgorithmV1::FastCdc,
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
pub(crate) fn run_complete_remove_v1<T, C>(
    cas: &FsCasV1,
    cancellation_key: u64,
    version_record: PhysicalVersionRecordIdV1,
    base_root: CanonicalDirectoryTreeV1,
    mutation_evidence: AuthenticatedTreeMutationEvidenceV1<'_>,
    removal_index: usize,
    expected_removed: CanonicalTreeEntryV1<'_>,
    tree_source: &mut T,
    buffers: OperationBuffersV1<'_>,
    cow_logical: &mut [u8; COMPARISON_WINDOW_BYTES],
    control: &mut C,
    counters: &mut OperationCountersV1,
) -> Result<OperationHandoffV1, OperationErrorV1>
where
    T: CanonicalTreeMutationSourceV1 + ?Sized,
    C: LifecycleControlV1 + ?Sized,
{
    let mut operation = request_mutation_operation_v1(
        cas,
        MutationOperationKindV1::Remove,
        cancellation_key,
        counters,
        control,
    )?;
    let (global_seen_capacity, maximum_records, tree_source_resident, storage_resident) = operation
        .run_preparation_free_stage_v1(counters, control, |operation, counters, control| {
            operation.declare_storage_envelope_v1(FsStorageEnvelopeV1::new(0, 0, 0, 0)?)?;
            check_lifecycle_control_v1(control)?;
            let base_objects = operation.authenticate_base_root_v1(
                version_record,
                base_root.physical(),
                counters,
                cow_logical,
                control,
            )?;
            check_lifecycle_control_v1(control)?;

            let base_entry_count = base_root.entry_count();
            let result_entry_count = base_entry_count.checked_sub(1).ok_or(CoreError::Path)?;
            if removal_index >= base_entry_count as usize {
                return Err(CoreError::Path.into());
            }
            let result_shape = preflight_canonical_tree_v1(u64::from(result_entry_count))?;
            let (maximum_objects, global_seen_capacity, maximum_records) =
                mutation_candidate_bounds_v1(
                    base_objects,
                    0,
                    u64::from(result_shape.tree_object_count()),
                )?;
            ensure_mutation_buffers_v1(
                &buffers,
                maximum_objects,
                result_shape.page_summary_count(),
            )?;
            operation.declare_storage_envelope_v1(storage_envelope_v1(
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
            if tree_source.declared_base_entry_count()? != base_entry_count
                || tree_source.declared_result_entry_count()? != result_entry_count
            {
                return Err(CoreError::Path.into());
            }
            let tree_source_resident = tree_source.resident_memory_bound_bytes()?;
            let storage_resident = operation.storage_resident_plan_v1(false, maximum_records)?;
            let evidence_resident =
                u64::try_from(mutation_evidence_resident_bytes_v1(mutation_evidence)?)
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
                .max(operation.memory_high_water_bytes_v1());
            Ok((
                global_seen_capacity,
                maximum_records,
                tree_source_resident,
                storage_resident,
            ))
        })?;

    run_lifecycle_v1(
        operation,
        LifecyclePlanV1 {
            global_seen_capacity,
            storage_resident,
            require_tree_storage: false,
            maximum_records,
            algorithm: CdcAlgorithmV1::FastCdc,
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
pub(crate) fn run_complete_metadata_v1<E, C>(
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
    buffers: OperationBuffersV1<'_>,
    cow_logical: &mut [u8; COMPARISON_WINDOW_BYTES],
    control: &mut C,
    counters: &mut OperationCountersV1,
) -> Result<OperationHandoffV1, OperationErrorV1>
where
    E: BaseChunkEvidenceSourceV1 + ?Sized,
    C: LifecycleControlV1 + ?Sized,
{
    let mut operation = request_mutation_operation_v1(
        cas,
        MutationOperationKindV1::Metadata,
        cancellation_key,
        counters,
        control,
    )?;
    let (component, global_seen_capacity, maximum_records, storage_resident) = operation
        .run_preparation_free_stage_v1(counters, control, |operation, counters, control| {
            operation.declare_storage_envelope_v1(FsStorageEnvelopeV1::new(0, 0, 0, 0)?)?;
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
                mutation_candidate_bounds_v1(
                    base_objects,
                    0,
                    u64::from(tree_shape.tree_object_count()),
                )?;
            ensure_mutation_buffers_v1(&buffers, maximum_objects, tree_shape.page_summary_count())?;
            operation.declare_storage_envelope_v1(storage_envelope_v1(
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
            let storage_resident = operation.storage_resident_plan_v1(false, maximum_records)?;
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
                .max(operation.memory_high_water_bytes_v1());

            // This bounded evidence authentication remains preparation-free.
            // Its error/unwind terminal now balances the same root lease.
            authenticate_base_file_evidence_v1(base_file, chunk_evidence, counters)?;
            check_lifecycle_control_v1(control)?;
            Ok((
                component,
                global_seen_capacity,
                maximum_records,
                storage_resident,
            ))
        })?;

    run_lifecycle_v1(
        operation,
        LifecyclePlanV1 {
            global_seen_capacity,
            storage_resident,
            require_tree_storage: false,
            maximum_records,
            algorithm: CdcAlgorithmV1::FastCdc,
        },
        buffers,
        control,
        counters,
        move |storage, control_cell, reservation, buffers, counters| {
            let file = {
                let (references, sink) = storage.content_parts_v1();
                reencode_file_metadata_borrowed_v1(
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
pub(crate) fn run_complete_move_v1<T, C>(
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
    buffers: OperationBuffersV1<'_>,
    cow_logical: &mut [u8; COMPARISON_WINDOW_BYTES],
    control: &mut C,
    counters: &mut OperationCountersV1,
) -> Result<OperationHandoffV1, OperationErrorV1>
where
    T: CanonicalTreeMutationSourceV1 + ?Sized,
    C: LifecycleControlV1 + ?Sized,
{
    let mut operation = request_mutation_operation_v1(
        cas,
        MutationOperationKindV1::Move,
        cancellation_key,
        counters,
        control,
    )?;
    let (component, global_seen_capacity, maximum_records, tree_source_resident, storage_resident) =
        operation.run_preparation_free_stage_v1(
            counters,
            control,
            |operation, counters, control| {
                operation.declare_storage_envelope_v1(FsStorageEnvelopeV1::new(0, 0, 0, 0)?)?;
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
                let base_entry_count = base_root.entry_count();
                if base_entry_count == 0
                    || removal_index >= base_entry_count as usize
                    || insertion_index >= base_entry_count as usize
                {
                    return Err(CoreError::Path.into());
                }
                let tree_shape = preflight_canonical_tree_v1(u64::from(base_entry_count))?;
                let (maximum_objects, global_seen_capacity, maximum_records) =
                    mutation_candidate_bounds_v1(
                        base_objects,
                        0,
                        u64::from(tree_shape.tree_object_count()),
                    )?;
                ensure_mutation_buffers_v1(
                    &buffers,
                    maximum_objects,
                    tree_shape.page_summary_count(),
                )?;
                operation.declare_storage_envelope_v1(storage_envelope_v1(
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
                if tree_source.declared_base_entry_count()? != base_entry_count
                    || tree_source.declared_result_entry_count()? != base_entry_count
                {
                    return Err(CoreError::Path.into());
                }
                let tree_source_resident = tree_source.resident_memory_bound_bytes()?;
                let storage_resident =
                    operation.storage_resident_plan_v1(false, maximum_records)?;
                let evidence_resident =
                    u64::try_from(mutation_evidence_resident_bytes_v1(mutation_evidence)?)
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
                    .max(operation.memory_high_water_bytes_v1());
                Ok((
                    component,
                    global_seen_capacity,
                    maximum_records,
                    tree_source_resident,
                    storage_resident,
                ))
            },
        )?;

    let moved = CanonicalTreeEntryV1::new(component, expected_removed.child());
    run_lifecycle_v1(
        operation,
        LifecyclePlanV1 {
            global_seen_capacity,
            storage_resident,
            require_tree_storage: false,
            maximum_records,
            algorithm: CdcAlgorithmV1::FastCdc,
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
    buffers: OperationBuffersV1<'_>,
    cow_logical: &mut [u8; COMPARISON_WINDOW_BYTES],
    control: &mut C,
    counters: &mut OperationCountersV1,
) -> Result<OperationHandoffV1, OperationErrorV1>
where
    ST: CanonicalTreeMutationSourceV1 + ?Sized,
    DT: CanonicalTreeMutationSourceV1 + ?Sized,
    RT: CanonicalTreeMutationSourceV1 + ?Sized,
    C: LifecycleControlV1 + ?Sized,
{
    let mut operation = request_mutation_operation_v1(
        cas,
        MutationOperationKindV1::Move,
        cancellation_key,
        counters,
        control,
    )?;
    let (
        moved_name,
        global_seen_capacity,
        maximum_records,
        source_tree_resident,
        destination_tree_resident,
        root_tree_resident,
        storage_resident,
    ) = operation.run_preparation_free_stage_v1(
        counters,
        control,
        |operation, counters, control| {
            operation.declare_storage_envelope_v1(FsStorageEnvelopeV1::new(0, 0, 0, 0)?)?;
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
                || expected_source_root_entry.child()
                    != explicit_directory_child_v1(source_directory)?
                || expected_destination_root_entry.child()
                    != explicit_directory_child_v1(destination_directory)?
            {
                return Err(CoreError::Path.into());
            }
            let moved_name = ValidatedComponent::new(new_name)?;
            let source_base_count = source_directory.entry_count();
            let source_result_count = source_base_count.checked_sub(1).ok_or(CoreError::Path)?;
            let destination_base_count = destination_directory.entry_count();
            let destination_result_count = destination_base_count
                .checked_add(1)
                .ok_or(CoreError::CountCap)?;
            if source_removal_index >= source_base_count as usize
                || destination_insertion_index > destination_base_count as usize
            {
                return Err(CoreError::Path.into());
            }

            let source_shape = preflight_canonical_tree_v1(u64::from(source_result_count))?;
            let destination_shape =
                preflight_canonical_tree_v1(u64::from(destination_result_count))?;
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
            operation.declare_storage_envelope_v1(storage_envelope_v1(
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

            if source_tree_source.declared_base_entry_count()? != source_base_count
                || source_tree_source.declared_result_entry_count()? != source_result_count
                || destination_tree_source.declared_base_entry_count()? != destination_base_count
                || destination_tree_source.declared_result_entry_count()?
                    != destination_result_count
                || root_tree_source.declared_base_entry_count()? != base_root.entry_count()
                || root_tree_source.declared_result_entry_count()? != base_root.entry_count()
            {
                return Err(CoreError::Path.into());
            }
            let source_tree_resident = source_tree_source.resident_memory_bound_bytes()?;
            let destination_tree_resident =
                destination_tree_source.resident_memory_bound_bytes()?;
            let root_tree_resident = root_tree_source.resident_memory_bound_bytes()?;
            let tree_source_resident = source_tree_resident
                .checked_add(destination_tree_resident)
                .and_then(|bytes| bytes.checked_add(root_tree_resident))
                .ok_or(CoreError::IntegerOverflow)?;
            let source_evidence_resident = mutation_evidence_resident_bytes_v1(source_evidence)?;
            let destination_evidence_resident =
                mutation_evidence_resident_bytes_v1(destination_evidence)?;
            let root_evidence_resident = mutation_evidence_resident_bytes_v1(root_evidence)?;
            let evidence_resident = source_evidence_resident
                .checked_add(destination_evidence_resident)
                .and_then(|bytes| bytes.checked_add(root_evidence_resident))
                .ok_or(CoreError::IntegerOverflow)?;
            let evidence_resident =
                u64::try_from(evidence_resident).map_err(|_| CoreError::IntegerOverflow)?;
            let storage_resident = operation.storage_resident_plan_v1(false, maximum_records)?;
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
                .max(operation.memory_high_water_bytes_v1());
            Ok((
                moved_name,
                global_seen_capacity,
                maximum_records,
                source_tree_resident,
                destination_tree_resident,
                root_tree_resident,
                storage_resident,
            ))
        },
    )?;

    let moved = CanonicalTreeEntryV1::new(moved_name, expected_removed.child());
    run_lifecycle_v1(
        operation,
        LifecyclePlanV1 {
            global_seen_capacity,
            storage_resident,
            require_tree_storage: false,
            maximum_records,
            algorithm: CdcAlgorithmV1::FastCdc,
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
pub(crate) fn run_lifecycle_v1<C, B>(
    operation: StorageOperationV1<'_>,
    plan: LifecyclePlanV1,
    buffers: OperationBuffersV1<'_>,
    control: &mut C,
    counters: &mut OperationCountersV1,
    build: B,
) -> Result<OperationHandoffV1, OperationErrorV1>
where
    C: CdcControlV1 + FsCasControlV1 + ?Sized,
    B: FnOnce(
        &mut dyn StorageSessionPortV1,
        &RefCell<&mut FsOperationObservedControlV1<'_, C>>,
        &OperationReservationV1<'_>,
        &mut LifecycleBuildBuffersV1<'_>,
        &mut OperationCountersV1,
    ) -> Result<PreparedCandidateV1, OperationErrorV1>,
{
    let mut observed_control = FsOperationObservedControlV1::new(control);
    let terminal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_lifecycle_observed_body_v1(
            operation,
            plan,
            buffers,
            &mut observed_control,
            counters,
            build,
        )
    }));
    let observation = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        observed_control.finish_v1(counters)
    }));
    let observation = match observation {
        Ok(observation) => observation,
        Err(observation_payload) => match terminal {
            Ok(_) => std::panic::resume_unwind(observation_payload),
            Err(initiating_payload) => {
                drop(observation_payload);
                std::panic::resume_unwind(initiating_payload);
            }
        },
    };
    match terminal {
        Ok(result) => match observation {
            Ok(()) => result,
            Err(error) => Err(OperationErrorV1::retain_terminal_v1(
                result.err(),
                OperationErrorV1::Core(error),
            )
            .expect("direct lock observation failure")),
        },
        Err(payload) => match observation {
            Ok(()) => std::panic::resume_unwind(payload),
            Err(error) => {
                // The initiating callback payload remains primary only when
                // the complete operation-owned observation terminal is
                // balanced. A typed observation failure is returned after
                // lifecycle has already completed cleanup and capability
                // terminalization inside the caught body.
                drop(payload);
                Err(OperationErrorV1::Core(error))
            }
        },
    }
}

#[allow(clippy::too_many_arguments, clippy::drop_non_drop)]
fn run_lifecycle_observed_body_v1<C, B>(
    mut operation: StorageOperationV1<'_>,
    plan: LifecyclePlanV1,
    buffers: OperationBuffersV1<'_>,
    control: &mut C,
    counters: &mut OperationCountersV1,
    build: B,
) -> Result<OperationHandoffV1, OperationErrorV1>
where
    C: CdcControlV1 + FsCasControlV1 + ?Sized,
    B: FnOnce(
        &mut dyn StorageSessionPortV1,
        &RefCell<&mut C>,
        &OperationReservationV1<'_>,
        &mut LifecycleBuildBuffersV1<'_>,
        &mut OperationCountersV1,
    ) -> Result<PreparedCandidateV1, OperationErrorV1>,
{
    // The complete variant owns the already-bounded operation buffers; boxing
    // it would add a fallible allocation to terminal unwind reconciliation.
    #[allow(clippy::large_enum_variant)]
    enum BuildTerminalV1 {
        Complete(Result<PreparedCandidateV1, OperationErrorV1>),
        Unwind {
            payload: Box<dyn core::any::Any + Send>,
            failure: Option<OperationErrorV1>,
        },
    }

    let OperationBuffersV1 {
        source,
        cdc_ring,
        incoming_comparison,
        occupied_comparison,
        tree_object,
        tree_pages,
        traversal_state,
    } = buffers;
    let reservation = operation.reservation_v1();
    let preparation_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        operation.begin_preparation_v1(
            plan.global_seen_capacity,
            plan.storage_resident.preparation,
            control,
        )
    }));
    let mut preparation = match preparation_result {
        Ok(Ok(preparation)) => preparation,
        Ok(Err(error)) => {
            let original = OperationErrorV1::from(error);
            return match operation.finish_operation_caught_v1(false, counters, control) {
                Ok(Ok(())) => Err(original),
                Ok(Err(terminal)) => Err(original.dominated_by_fscas_v1(terminal)),
                Err(terminal_payload) => {
                    drop(terminal_payload);
                    Err(original)
                }
            };
        }
        Err(payload) => {
            match operation.finish_operation_caught_v1(false, counters, control) {
                Ok(Ok(())) => std::panic::resume_unwind(payload),
                Ok(Err(terminal)) => {
                    // The initiating callback payload remains primary only
                    // while the owned storage/admission terminal is clean.
                    // Once that terminal fails, its typed cause is the
                    // machine-readable operation outcome.
                    drop(payload);
                    return Err(OperationErrorV1::FsCas(terminal));
                }
                Err(terminal_payload) => {
                    drop(terminal_payload);
                    std::panic::resume_unwind(payload);
                }
            }
        }
    };
    let mut build_buffers = LifecycleBuildBuffersV1 {
        source,
        cdc_ring,
        tree_object,
        tree_pages,
    };

    let built = (|| -> BuildTerminalV1 {
        let control_cell = RefCell::new(&mut *control);
        let storage_result = operation.begin_session_v1(
            &mut preparation,
            plan.require_tree_storage,
            incoming_comparison,
            occupied_comparison,
            plan.maximum_records,
            plan.storage_resident.private_storage,
            reservation,
            &control_cell,
        );
        let mut storage = match storage_result {
            Ok(storage) => storage,
            Err(error) => {
                drop(control_cell);
                return BuildTerminalV1::Complete(Err(error.into()));
            }
        };
        // Catch only while every operation-owned storage object and the
        // outer capability are still live. This lets lifecycle perform the
        // same explicit, fallible cleanup and terminal accounting as a typed
        // error before resuming the caller's original panic. Drop remains a
        // last-resort backstop for a second panic in cleanup itself.
        let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            build(
                &mut storage,
                &control_cell,
                reservation,
                &mut build_buffers,
                counters,
            )
        }));

        let mut build_unwind = None;
        let built = match built {
            Ok(built) => Some(built),
            Err(payload) => {
                build_unwind = Some(payload);
                None
            }
        };
        // Terminal observation and private-pack cleanup are themselves
        // fault-control boundaries. Keep the owned session live while this
        // whole block is caught so a panic cannot bypass explicit cleanup.
        // This value is owned outside the catch so a later cleanup callback
        // unwind cannot destroy the already-classified body/storage cause.
        let terminal_first_failure = std::cell::Cell::new(None);
        let terminal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if let Some(built) = built {
                match built {
                    Ok(candidate) => {
                        let global_seen = storage.record_global_seen_observation_v1();
                        let counter_result =
                            counters.accumulate(storage.take_storage_counters_v1());
                        match (global_seen, counter_result) {
                            (Ok(()), Ok(())) => Ok(candidate),
                            (global_seen, counter_result) => {
                                // A completed pack set may already own fresh
                                // immutable carriers, locators, and a catalog
                                // marker.  If a terminal observation cannot
                                // be transferred, dropping `candidate` must
                                // not lose that operation-relative custody.
                                // The first counter batch was taken before
                                // the merge, so the storage session now owns
                                // only the exact one-shot residue observation
                                // recorded below.
                                let original = global_seen
                                    .err()
                                    .or_else(|| counter_result.err())
                                    .map(OperationErrorV1::Core)
                                    .expect("completed candidate observation failure");
                                terminal_first_failure.set(Some(original));
                                let residue = storage.record_incomplete_residue_v1();
                                let mut failure = Some(original);
                                if let Err(error) = residue {
                                    failure = OperationErrorV1::retain_terminal_v1(
                                        failure,
                                        OperationErrorV1::Core(error),
                                    );
                                }
                                terminal_first_failure.set(failure);
                                let private_cleanup = storage.cleanup_private_pack_controlled_v1();
                                if let Err(error) = private_cleanup {
                                    failure = OperationErrorV1::retain_terminal_v1(
                                        failure,
                                        OperationErrorV1::FsCas(error),
                                    );
                                }
                                terminal_first_failure.set(failure);
                                let residue_counters =
                                    counters.accumulate(storage.take_storage_counters_v1());
                                if let Err(error) = residue_counters {
                                    failure = OperationErrorV1::retain_terminal_v1(
                                        failure,
                                        OperationErrorV1::Core(error),
                                    );
                                }
                                terminal_first_failure.set(failure);
                                Err(failure.expect("completed candidate terminal failure"))
                            }
                        }
                    }
                    Err(error) => {
                        // Capture the body/storage cause before cleanup can
                        // retain its own terminally dominant failure in the
                        // same adapter side channel.
                        let core_error = storage.take_first_core_error_v1();
                        let fscas_error = storage.take_first_fscas_error_v1();
                        let original = fscas_error.map_or_else(
                            || core_error.map_or(error, OperationErrorV1::Core),
                            OperationErrorV1::FsCas,
                        );
                        let mut failure = Some(original);
                        terminal_first_failure.set(failure);
                        let residue_result = storage.record_incomplete_residue_v1();
                        if let Err(error) = residue_result {
                            failure = OperationErrorV1::retain_terminal_v1(
                                failure,
                                OperationErrorV1::Core(error),
                            );
                        }
                        terminal_first_failure.set(failure);
                        let private_cleanup = storage.cleanup_private_pack_controlled_v1();
                        if let Err(error) = private_cleanup {
                            failure = OperationErrorV1::retain_terminal_v1(
                                failure,
                                OperationErrorV1::FsCas(error),
                            );
                        }
                        terminal_first_failure.set(failure);
                        let global_seen = storage.record_global_seen_observation_v1();
                        if let Err(error) = global_seen {
                            failure = OperationErrorV1::retain_terminal_v1(
                                failure,
                                OperationErrorV1::Core(error),
                            );
                        }
                        terminal_first_failure.set(failure);
                        let counter_result =
                            counters.accumulate(storage.take_storage_counters_v1());
                        if let Err(error) = counter_result {
                            failure = OperationErrorV1::retain_terminal_v1(
                                failure,
                                OperationErrorV1::Core(error),
                            );
                        }
                        terminal_first_failure.set(failure);
                        Err(failure.expect("typed body terminal failure"))
                    }
                }
            } else {
                let core_error = storage.take_first_core_error_v1();
                let fscas_error = storage.take_first_fscas_error_v1();
                let mut failure = fscas_error
                    .map(OperationErrorV1::FsCas)
                    .or_else(|| core_error.map(OperationErrorV1::Core));
                terminal_first_failure.set(failure);
                let residue = storage.record_incomplete_residue_v1();
                if let Err(error) = residue {
                    failure = OperationErrorV1::retain_terminal_v1(
                        failure,
                        OperationErrorV1::Core(error),
                    );
                }
                terminal_first_failure.set(failure);
                let private_cleanup = storage.cleanup_private_pack_controlled_v1();
                if let Err(error) = private_cleanup {
                    failure = OperationErrorV1::retain_terminal_v1(
                        failure,
                        OperationErrorV1::FsCas(error),
                    );
                }
                terminal_first_failure.set(failure);
                let global_seen = storage.record_global_seen_observation_v1();
                if let Err(error) = global_seen {
                    failure = OperationErrorV1::retain_terminal_v1(
                        failure,
                        OperationErrorV1::Core(error),
                    );
                }
                terminal_first_failure.set(failure);
                let counter_result = counters.accumulate(storage.take_storage_counters_v1());
                if let Err(error) = counter_result {
                    failure = OperationErrorV1::retain_terminal_v1(
                        failure,
                        OperationErrorV1::Core(error),
                    );
                }
                terminal_first_failure.set(failure);
                Err(failure.unwrap_or(OperationErrorV1::Core(CoreError::SourceFailure)))
            }
        }));
        let terminal = match terminal {
            Ok(terminal) => terminal,
            Err(terminal_payload) => {
                // A private-pack cleanup panic has already left the pack in a
                // typed fail-closed state. Re-entering is observation-only and
                // returns that retained error; it cannot retry filesystem work.
                let residue = storage.record_incomplete_residue_v1();
                let core_error = storage.take_first_core_error_v1();
                let fscas_error = storage.take_first_fscas_error_v1();
                let private_cleanup = storage.cleanup_private_pack_controlled_v1();
                let global_seen = storage.record_global_seen_observation_v1();
                let counter_result = counters.accumulate(storage.take_storage_counters_v1());
                // Preserve chronological cause ownership: the body/storage
                // failure happened before residue observation and cleanup.
                // A later cleanup/invalidation failure may dominate, but it
                // must be paired with—not replace—the first typed cause.
                let mut failure = terminal_first_failure.take();
                if let Some(error) = core_error {
                    failure = OperationErrorV1::retain_terminal_v1(
                        failure,
                        OperationErrorV1::Core(error),
                    );
                }
                if let Some(error) = fscas_error {
                    failure = OperationErrorV1::retain_terminal_v1(
                        failure,
                        OperationErrorV1::FsCas(error),
                    );
                }
                if let Err(error) = residue {
                    failure = OperationErrorV1::retain_terminal_v1(
                        failure,
                        OperationErrorV1::Core(error),
                    );
                }
                if let Err(error) = private_cleanup {
                    failure = OperationErrorV1::retain_terminal_v1(
                        failure,
                        OperationErrorV1::FsCas(error),
                    );
                }
                if let Err(error) = global_seen {
                    failure = OperationErrorV1::retain_terminal_v1(
                        failure,
                        OperationErrorV1::Core(error),
                    );
                }
                if let Err(error) = counter_result {
                    failure = OperationErrorV1::retain_terminal_v1(
                        failure,
                        OperationErrorV1::Core(error),
                    );
                }
                drop(storage);
                drop(control_cell);
                return BuildTerminalV1::Unwind {
                    payload: build_unwind.unwrap_or(terminal_payload),
                    failure,
                };
            }
        };
        if let Some(payload) = build_unwind {
            let failure = OperationErrorV1::reconcile_unwind_terminal_v1(
                terminal_first_failure.take(),
                terminal,
            );
            drop(storage);
            drop(control_cell);
            return BuildTerminalV1::Unwind { payload, failure };
        }
        drop(storage);
        drop(control_cell);
        BuildTerminalV1::Complete(terminal)
    })();

    let built = match built {
        BuildTerminalV1::Complete(built) => built,
        BuildTerminalV1::Unwind {
            payload,
            failure: storage_failure,
        } => {
            // Preparation owns six independent files and attempts every one
            // even when an earlier removal fails. Finish the root storage
            // equation and release the capability synchronously before the
            // original panic crosses the public operation boundary.
            let mut cleanup_terminal = preparation.finish_after_unwind_v1(control, payload);
            let (operation_failure, operation_unwind) =
                match operation.finish_operation_caught_v1(false, counters, control) {
                    Ok(result) => (result.err().map(OperationErrorV1::FsCas), None),
                    Err(payload) => (None, Some(payload)),
                };
            let mut failure = storage_failure;
            if let Some(error) = cleanup_terminal.first_error_v1() {
                failure =
                    OperationErrorV1::retain_terminal_v1(failure, OperationErrorV1::FsCas(error));
            }
            if let Some(error) = operation_failure {
                failure = OperationErrorV1::retain_terminal_v1(failure, error);
            }
            if let Some(failure) = failure {
                // Once cleanup or terminalization has produced a typed
                // failure, it is the operation's machine-readable outcome.
                // The initiating callback payload remains bounded here, but
                // must not replace that classified terminal with a string
                // panic at this Result-returning boundary.
                drop(cleanup_terminal.take_unwind_v1());
                drop(operation_unwind);
                return Err(failure);
            }
            let payload = cleanup_terminal
                .take_unwind_v1()
                .expect("operation unwind retained through cleanup");
            drop(operation_unwind);
            std::panic::resume_unwind(payload)
        }
    };

    let mut unreturned_installed_residue_bytes = 0_u64;
    let handoff_terminal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match built {
        Ok(candidate) => {
            // Until the closure fence returns a terminal result, every
            // carrier installed by this candidate is operation-relative
            // residue if a callback unwinds. Preserve that exact amount
            // outside the catch so the unwind path records the same direct
            // observation as the typed-error path.
            unreturned_installed_residue_bytes = candidate.completed.installed_residue_bytes();
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
                Ok(closure) => {
                    unreturned_installed_residue_bytes = unreturned_installed_residue_bytes
                        .checked_add(closure.installed_residue_bytes_v1())
                        .ok_or(CoreError::IntegerOverflow)?;
                    Ok(OperationHandoffV1 {
                        algorithm: plan.algorithm,
                        version_record: candidate.version_record,
                        root_tree: candidate.root_tree,
                        pack: candidate.completed.last_sealed(),
                        pack_outcome: candidate.completed.last_outcome(),
                        carrier_count: candidate.completed.carrier_count(),
                        carrier_rollovers: candidate.completed.carrier_count().saturating_sub(1),
                        carriers_installed: candidate.completed.carriers_installed(),
                        carriers_reused: candidate.completed.carriers_reused(),
                        object_count: closure.object_count_v1(),
                        reference_spool_bytes: candidate.reference_spool_bytes,
                        index_spool_bytes: candidate.completed.index_spool_bytes(),
                        terminal_optional_observations: counters
                            .terminal_optional_observations_v1(),
                    })
                }
                Err(error) => {
                    counters
                        .record_unreachable_installed_residue(unreturned_installed_residue_bytes)?;
                    unreturned_installed_residue_bytes = 0;
                    Err(match error {
                        FsClosureAdmissionErrorV1::Core(error) => OperationErrorV1::Core(error),
                        FsClosureAdmissionErrorV1::FsCas(error) => OperationErrorV1::FsCas(error),
                    })
                }
            }
        }
        Err(error) => Err(error),
    }));

    let handoff = match handoff_terminal {
        Ok(handoff) => handoff,
        Err(payload) => {
            // Closure construction/fencing is still inside the same outer
            // operation. A panic here must clean every preparation name and
            // balance storage/admission before it can cross the boundary.
            let residue_failure = counters
                .record_unreachable_installed_residue(unreturned_installed_residue_bytes)
                .err()
                .map(OperationErrorV1::Core);
            let mut cleanup_terminal = preparation.finish_after_unwind_v1(control, payload);
            let (operation_failure, operation_unwind) =
                match operation.finish_operation_caught_v1(false, counters, control) {
                    Ok(result) => (result.err().map(OperationErrorV1::FsCas), None),
                    Err(payload) => (None, Some(payload)),
                };
            let mut failure = residue_failure;
            if let Some(error) = cleanup_terminal.first_error_v1() {
                failure =
                    OperationErrorV1::retain_terminal_v1(failure, OperationErrorV1::FsCas(error));
            }
            if let Some(error) = operation_failure {
                failure = OperationErrorV1::retain_terminal_v1(failure, error);
            }
            if let Some(failure) = failure {
                // The closure payload is resumed only when residue
                // attribution, preparation cleanup, and both capability
                // terminal halves completed cleanly. A classified terminal
                // must remain a typed operation result rather than being
                // flattened into a formatted panic.
                drop(cleanup_terminal.take_unwind_v1());
                drop(operation_unwind);
                return Err(failure);
            }
            let payload = cleanup_terminal
                .take_unwind_v1()
                .expect("closure unwind retained through cleanup");
            drop(operation_unwind);
            std::panic::resume_unwind(payload)
        }
    };

    // Cleanup itself is user-control/fault-injection reachable. Catch its
    // unwind separately so root storage and the admission slot still receive
    // an explicit terminal record before the cleanup panic is resumed.
    let mut cleanup_terminal = preparation.finish(control);
    let cleanup_complete =
        cleanup_terminal.first_error_v1().is_none() && !cleanup_terminal.has_unwind_v1();
    let mut handoff_unwind = None;
    if handoff.is_ok() && cleanup_complete {
        if let Err(payload) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            control.boundary_reached(FsCasBoundaryV1::AfterCompleteValidatedHandoff);
        })) {
            handoff_unwind = Some(payload);
        }
    }
    let commit_storage = handoff.is_ok() && cleanup_complete && handoff_unwind.is_none();
    let residue_failure = if commit_storage || unreturned_installed_residue_bytes == 0 {
        None
    } else {
        counters
            .record_unreachable_installed_residue(unreturned_installed_residue_bytes)
            .err()
            .map(OperationErrorV1::Core)
    };
    let invalidation_failure = if handoff_unwind.is_some() {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            operation.capability.invalidate_owner_controlled_v1(control)
        })) {
            Ok(result) => result.err().map(OperationErrorV1::FsCas),
            Err(_secondary_payload) => operation
                .capability
                .invalidate_owner_backstop_v1()
                .err()
                .map(OperationErrorV1::FsCas),
        }
    } else {
        None
    };
    let (operation_terminal, operation_unwind) =
        match operation.finish_operation_caught_v1(commit_storage, counters, control) {
            Ok(result) => (result, None),
            Err(payload) => (Ok(()), Some(payload)),
        };
    let mut terminal_error = handoff.as_ref().err().copied();
    if let Some(cleanup) = cleanup_terminal.first_error_v1() {
        terminal_error =
            OperationErrorV1::retain_terminal_v1(terminal_error, OperationErrorV1::FsCas(cleanup));
    }
    if let Some(residue) = residue_failure {
        terminal_error = OperationErrorV1::retain_terminal_v1(terminal_error, residue);
    }
    if let Some(invalidation) = invalidation_failure {
        terminal_error = OperationErrorV1::retain_terminal_v1(terminal_error, invalidation);
    }
    if let Err(operation) = operation_terminal {
        terminal_error = OperationErrorV1::retain_terminal_v1(
            terminal_error,
            OperationErrorV1::FsCas(operation),
        );
    }
    if let Some(error) = terminal_error {
        drop(cleanup_terminal.take_unwind_v1());
        drop(handoff_unwind);
        drop(operation_unwind);
        return Err(error);
    }
    if let Some(payload) = cleanup_terminal.take_unwind_v1() {
        drop(handoff_unwind);
        drop(operation_unwind);
        std::panic::resume_unwind(payload);
    }
    if let Some(payload) = handoff_unwind {
        drop(operation_unwind);
        std::panic::resume_unwind(payload);
    }
    if let Some(payload) = operation_unwind {
        std::panic::resume_unwind(payload);
    }
    handoff
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_dominant_fscas_candidate_preserves_exact_core_wrapper() {
        let first = OperationErrorV1::Core(CoreError::ResourceRefused);
        let later = FsCasErrorV1::Core(CoreError::IntegerOverflow);

        assert_eq!(first.dominated_by_fscas_v1(later), first);
    }

    #[test]
    fn build_unwind_reconciliation_resumes_only_without_a_typed_terminal() {
        let first = OperationErrorV1::Core(CoreError::ResourceRefused);

        assert_eq!(
            OperationErrorV1::reconcile_unwind_terminal_v1(
                Some(first),
                Err::<(), _>(OperationErrorV1::Core(CoreError::SourceFailure)),
            ),
            Some(first)
        );
        assert_eq!(
            OperationErrorV1::reconcile_unwind_terminal_v1(
                Some(first),
                Err::<(), _>(OperationErrorV1::Core(CoreError::IntegerOverflow)),
            ),
            Some(first)
        );
        assert_eq!(
            OperationErrorV1::reconcile_unwind_terminal_v1(
                None,
                Err::<(), _>(OperationErrorV1::Core(CoreError::SourceFailure)),
            ),
            None
        );

        let dominant = FsCasErrorV1::CleanupFailed(FsCasCleanupTargetV1::PrivatePack);
        assert_eq!(
            OperationErrorV1::reconcile_unwind_terminal_v1(
                Some(first),
                Err::<(), _>(OperationErrorV1::FsCas(dominant)),
            ),
            Some(OperationErrorV1::FsCas(FsCasErrorV1::TerminalFailure {
                first: crate::cas::FsCasFailureCauseV1::Core(CoreError::ResourceRefused),
                dominant: crate::cas::FsCasFailureCauseV1::CleanupFailed(
                    FsCasCleanupTargetV1::PrivatePack,
                ),
            }))
        );
    }
}
