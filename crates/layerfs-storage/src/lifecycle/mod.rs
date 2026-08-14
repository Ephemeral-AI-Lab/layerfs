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
    create_file_borrowed_v1, replace_file_borrowed_v1, ChunkReferenceSpoolV1, ContentBuffersV1,
    ContentSourceV1, PreparedObjectSinkV1, SourceSupplierV1, TreeFileV1,
};
use crate::cow::file::{AuthenticatedBaseFileV1, UpdateRangeV1};
use crate::cow::{
    add_directory_entry_cow_borrowed_v1, build_canonical_directory_borrowed_v1,
    move_directory_entry_cow_borrowed_v1, mutation_evidence_resident_bytes_v1,
    mutation_hash_state_bytes_v1, preflight_canonical_tree_v1,
    remove_directory_entry_cow_borrowed_v1, replace_directory_entry_cow_borrowed_v1,
    replace_two_directory_entries_cow_borrowed_v1, replacement_evidence_resident_bytes_v1,
    AuthenticatedTreeMutationEvidenceV1, AuthenticatedTreeReplacementEvidenceV1,
    CanonicalDirectoryTreeV1, CanonicalTreeChildV1, CanonicalTreeEntryV1,
    CanonicalTreeMutationSourceV1, DirectoryBuildModeV1, DirectoryLogicalIdentityV1,
    PreparedTreeSinkV1, TreePageSummaryV1, MAX_TREE_OBJECT_BYTES,
};
use crate::format::{
    require_strictly_increasing_paths, validate_chunk_refs_per_file,
    validate_chunk_refs_per_version, validate_entry_count, validate_file_mode,
    validate_logical_length, validate_total_object_count, validate_tree_object_count,
    ValidatedComponent, ValidatedPath, MAX_PATH_DEPTH,
};
use crate::identity::{
    derive_file_node_v1, derive_version_v1, FileNodeIdV1, PhysicalFileIdV1, PhysicalTreeIdV1,
    PhysicalVersionRecordIdV1, VersionIdV1, COMPARISON_WINDOW_BYTES, IDENTITY_HASHER_BYTES_V1,
};
use crate::limits::{
    MemoryComponentV1, ObservationScopeV1, OperationCountersV1, OperationMemoryPlanV1,
    OperationReservationV1, OptionalU64ObservationV1, TerminalOptionalObservationsV1,
};
use crate::object::{TypedPhysicalObjectIdV1, OBJECT_HEADER_BYTES, VERSION_RECORD_PAYLOAD_BYTES};
use crate::pack::{
    CompletedPackSetV1, SealedPackV1, MAX_PACK_BYTES, MAX_PACK_RECORDS, PACK_HEADER_BYTES,
    PACK_INDEX_ENTRY_BYTES, PACK_TRAILER_BYTES,
};
use crate::{CoreError, CoreResult};

/// The only cross-crate lifecycle observation needed by the subprocess
/// ownership check.  The opened CAS handle never crosses this boundary.
#[cfg(feature = "operation-polymorphism")]
pub mod semantic {
    use std::fs;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{mpsc, Arc, Condvar, Mutex};
    use std::time::{Duration, Instant};

    use super::{
        complete_cross_directory_move_operation_v1, request_create_operation_v1,
        request_tree_operation_v1, run_complete_add_v1, run_complete_metadata_v1,
        run_complete_move_v1, run_complete_remove_v1, run_complete_replace_v1,
        run_complete_update_v1, run_create_tree_v1, run_create_v1, FsCasBoundaryV1,
        FsCasCleanupTargetV1, FsCasControlV1, FsCasErrorV1, FsCasV1, FsOperationKindV1,
        FsStorageEnvelopeV1, LifecycleControlV1, OperationBuffersV1, OperationErrorV1,
    };
    use crate::cas::semantic::{
        publication_causes_v1, publication_error_v1, PublicationCauseV1,
        PublicationCleanupTargetV1, PublicationErrorV1,
    };
    use crate::cas::{
        FsCasFailureCauseV1, FsCasFilesystemBoundaryV1, FsCasFilesystemFailureV1,
        FsCasResidueAccountingBoundaryV1, FsCasResourceV1, ROOT_LOGICAL_STORAGE_BUDGET_V1,
        ROOT_NAMESPACE_ENTRY_BUDGET_V1,
    };
    use crate::cdc::{CdcAlgorithmV1, CdcControlV1, FastCdcV1, MAXIMUM_CHUNK_BYTES};
    use crate::content::update::{
        AuthenticatedBaseByteReaderV1, BaseChunkEvidenceSourceV1, BaseChunkEvidenceV1,
        BaseReadErrorV1,
    };
    use crate::content::{
        ContentSourceErrorV1, ContentSourceV1, PreparedSinkErrorV1, SourceSupplierV1, TreeFileV1,
    };
    use crate::cow::file::{AuthenticatedBaseFileV1, UpdateRangeV1};
    use crate::cow::semantic::{with_mutation_evidence_v1, with_replacement_evidence_v1};
    use crate::cow::{
        CanonicalDirectoryTreeV1, CanonicalTreeChildV1, CanonicalTreeEntryV1, DirectoryBuildModeV1,
        DirectoryLogicalIdentityV1, TreePageSummaryV1, MAX_TREE_OBJECT_BYTES,
        MAX_TREE_PAGE_SUMMARIES,
    };
    use crate::format::{ValidatedComponent, MAX_PATH_BYTES};
    use crate::identity::{
        derive_file_node_v1, derive_logical_chunk_v1, derive_logical_file_v1,
        derive_physical_chunk_id_v1, derive_physical_file_id_v1, LogicalChunkRefV1,
        LogicalFileIdentityV1, PhysicalFileIdV1, PhysicalTreeIdV1, PhysicalVersionRecordIdV1,
        COMPARISON_WINDOW_BYTES,
    };
    use crate::limits::{ObservationScopeV1, OperationCountersV1, OptionalObservationStatusV1};
    use crate::pack::PACK_HEADER_BYTES;
    use crate::profile::ProfileSpecV1;
    use crate::read::extraction::{extract_root_v1, read_file_range_impl_v1};
    use crate::read::{
        ReadBuffersV1, ReadKindV1, ReadOperationErrorV1, ReadSinkErrorV1, ReadSinkV1,
    };
    use crate::CoreError;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum OpenExistingObservationV1 {
        Opened,
        Busy,
        Invalidated,
        Rejected,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct SubprocessObservationV1 {
        child_succeeded: bool,
        child_reports: u32,
        child_busy_reports: u32,
    }

    impl SubprocessObservationV1 {
        pub const fn child_succeeded(self) -> bool {
            self.child_succeeded
        }

        pub const fn child_reports(self) -> u32 {
            self.child_reports
        }

        pub const fn child_busy_reports(self) -> u32 {
            self.child_busy_reports
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ExclusiveOwnerTransferObservationV1 {
        busy_with_alias: bool,
        busy_after_alias_drop: bool,
        opened_after_owner_drop: bool,
    }

    impl ExclusiveOwnerTransferObservationV1 {
        pub const fn busy_with_alias(self) -> bool {
            self.busy_with_alias
        }

        pub const fn busy_after_alias_drop(self) -> bool {
            self.busy_after_alias_drop
        }

        pub const fn opened_after_owner_drop(self) -> bool {
            self.opened_after_owner_drop
        }

        pub const fn transferred_cleanly(self) -> bool {
            self.busy_with_alias && self.busy_after_alias_drop && self.opened_after_owner_drop
        }
    }

    /// The named filesystem fault points retained by the historical fault
    /// owner.  The concrete boundary and failure enums stay below this
    /// feature-gated semantic port.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum FilesystemFaultCaseV1 {
        PreparationCreateNoSpace,
        PreparationResizeQuota,
        PermissionChangeDenied,
        PreparationWriteShortWrite,
        PrivatePackCreateInodeExhaustion,
        PrivatePackWriteShortWrite,
        PrivatePackFlushWriteFailure,
        CarrierHardLinkUnsupported,
        MarkerCreateInodeExhaustion,
        MarkerWriteNoSpace,
        MarkerFlushWriteFailure,
        MarkerHardLinkUnsupported,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum FilesystemFaultFailureV1 {
        NoSpace,
        Quota,
        InodeExhaustion,
        ReadFailure,
        WriteFailure,
        ShortRead,
        ShortWrite,
        PermissionDenied,
        Unsupported,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum FilesystemFaultErrorV1 {
        Filesystem(FilesystemFaultFailureV1),
        Unsupported,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct FilesystemFaultObservationV1 {
        error: Option<FilesystemFaultErrorV1>,
        fired: bool,
        bound_invoked: bool,
        supply_invoked: bool,
        source_read_calls: u64,
        preparation_entries: u64,
        immutable_entries: u64,
        storage_bytes_requested: u64,
        storage_bytes_reserved: u64,
        storage_bytes_released: u64,
        storage_bytes_committed: u64,
        storage_bytes_retained: u64,
        storage_inodes_requested: u64,
        storage_inodes_reserved: u64,
        storage_inodes_released: u64,
        storage_inodes_committed: u64,
        storage_inodes_retained: u64,
        operation_slots: u64,
        operation_active: u64,
        storage_active_operations: u64,
        storage_active_bytes: u64,
        storage_active_inodes: u64,
        root_usable: bool,
        stale_usable: bool,
        zero_forbidden_work: bool,
    }

    impl FilesystemFaultObservationV1 {
        pub const fn error(self) -> Option<FilesystemFaultErrorV1> {
            self.error
        }

        pub const fn fired(self) -> bool {
            self.fired
        }

        pub const fn bound_invoked(self) -> bool {
            self.bound_invoked
        }

        pub const fn supply_invoked(self) -> bool {
            self.supply_invoked
        }

        pub const fn source_read_calls(self) -> u64 {
            self.source_read_calls
        }

        pub const fn preparation_entries(self) -> u64 {
            self.preparation_entries
        }

        pub const fn immutable_entries(self) -> u64 {
            self.immutable_entries
        }

        pub const fn storage_bytes_requested(self) -> u64 {
            self.storage_bytes_requested
        }

        pub const fn storage_bytes_reserved(self) -> u64 {
            self.storage_bytes_reserved
        }

        pub const fn storage_bytes_released(self) -> u64 {
            self.storage_bytes_released
        }

        pub const fn storage_bytes_committed(self) -> u64 {
            self.storage_bytes_committed
        }

        pub const fn storage_bytes_retained(self) -> u64 {
            self.storage_bytes_retained
        }

        pub const fn storage_inodes_requested(self) -> u64 {
            self.storage_inodes_requested
        }

        pub const fn storage_inodes_reserved(self) -> u64 {
            self.storage_inodes_reserved
        }

        pub const fn storage_inodes_released(self) -> u64 {
            self.storage_inodes_released
        }

        pub const fn storage_inodes_committed(self) -> u64 {
            self.storage_inodes_committed
        }

        pub const fn storage_inodes_retained(self) -> u64 {
            self.storage_inodes_retained
        }

        pub const fn operation_slots(self) -> u64 {
            self.operation_slots
        }

        pub const fn operation_active(self) -> u64 {
            self.operation_active
        }

        pub const fn storage_active_operations(self) -> u64 {
            self.storage_active_operations
        }

        pub const fn storage_active_bytes(self) -> u64 {
            self.storage_active_bytes
        }

        pub const fn storage_active_inodes(self) -> u64 {
            self.storage_active_inodes
        }

        pub const fn root_usable(self) -> bool {
            self.root_usable
        }

        pub const fn stale_usable(self) -> bool {
            self.stale_usable
        }

        pub const fn zero_forbidden_work(self) -> bool {
            self.zero_forbidden_work
        }
    }

    static NEXT_SUBPROCESS_ROOT: AtomicU64 = AtomicU64::new(1);
    const CHILD_ROOT_ENV: &str = "LAYERFS_LIFECYCLE_OPEN_EXISTING_ROOT";
    const CHILD_SENTINEL_ENV: &str = "LAYERFS_LIFECYCLE_OPEN_EXISTING_CHILD";
    const CHILD_EXPECT_ENV: &str = "LAYERFS_LIFECYCLE_OPEN_EXISTING_EXPECT";

    fn run_open_probe(
        root: &Path,
        selector: &str,
        expected: OpenExistingObservationV1,
    ) -> (bool, u32, u32, u32) {
        let output =
            Command::new(std::env::current_exe().expect("current integration-test executable"))
                .args(["--exact", selector, "--nocapture"])
                .env(CHILD_ROOT_ENV, root)
                .env(CHILD_SENTINEL_ENV, "1")
                .env(CHILD_EXPECT_ENV, format!("{expected:?}"))
                .output()
                .expect("spawn exact open-existing child");
        let output_lines = [
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        ]
        .into_iter()
        .flat_map(|stream| stream.lines().map(str::to_owned).collect::<Vec<_>>())
        .collect::<Vec<_>>();
        (
            output.status.success(),
            output_lines
                .iter()
                .filter(|line| line.starts_with("LAYERFS_CHILD_RESULT="))
                .count() as u32,
            output_lines
                .iter()
                .filter(|line| line.as_str() == "LAYERFS_CHILD_RESULT=Busy")
                .count() as u32,
            output_lines
                .iter()
                .filter(|line| line.as_str() == "LAYERFS_CHILD_RESULT=Opened")
                .count() as u32,
        )
    }

    /// Hold a real root owner while an exact integration-test child probes it.
    /// Only the process result crosses the facade; the FsCAS owner never does.
    pub fn run_open_existing_subprocess_v1(selector: &str) -> SubprocessObservationV1 {
        let sequence = NEXT_SUBPROCESS_ROOT.fetch_add(1, Ordering::Relaxed);
        let parent = fs::canonicalize(std::env::temp_dir()).expect("canonical temporary root");
        let root: PathBuf = parent.join(format!(
            "layerfs-lifecycle-subprocess-{}-{sequence:016x}",
            std::process::id()
        ));
        let owner = FsCasV1::create_new(&root).expect("create subprocess ownership fixture");
        let (child_succeeded, reports, busy_reports, _) =
            run_open_probe(&root, selector, OpenExistingObservationV1::Busy);
        drop(owner);
        let _ = fs::remove_dir_all(&root);
        SubprocessObservationV1 {
            child_succeeded,
            child_reports: reports,
            child_busy_reports: busy_reports,
        }
    }

    pub fn probe_open_existing_subprocess_v1(
        root: &Path,
        selector: &str,
        expected: OpenExistingObservationV1,
    ) -> bool {
        let (succeeded, reports, _, _) = run_open_probe(root, selector, expected);
        succeeded && reports == 1
    }

    pub fn run_exclusive_owner_transfer_v1(selector: &str) -> ExclusiveOwnerTransferObservationV1 {
        let sequence = NEXT_SUBPROCESS_ROOT.fetch_add(1, Ordering::Relaxed);
        let parent = fs::canonicalize(std::env::temp_dir()).expect("canonical temporary root");
        let root = parent.join(format!(
            "layerfs-lifecycle-owner-transfer-{}-{sequence:016x}",
            std::process::id()
        ));
        let owner = FsCasV1::create_new(&root).expect("create ownership-transfer fixture");
        let alias = FsCasV1::open_existing(&root).expect("open owner alias");
        let first = run_open_probe(&root, selector, OpenExistingObservationV1::Busy);
        drop(alias);
        let second = run_open_probe(&root, selector, OpenExistingObservationV1::Busy);
        drop(owner);
        let third = run_open_probe(&root, selector, OpenExistingObservationV1::Opened);
        let _ = fs::remove_dir_all(&root);
        ExclusiveOwnerTransferObservationV1 {
            busy_with_alias: first == (true, 1, 1, 0),
            busy_after_alias_drop: second == (true, 1, 1, 0),
            opened_after_owner_drop: third == (true, 1, 0, 1),
        }
    }

    pub fn open_existing_subprocess_child_v1(
    ) -> Option<(OpenExistingObservationV1, OpenExistingObservationV1)> {
        let root = std::env::var_os(CHILD_ROOT_ENV)?;
        if std::env::var_os(CHILD_SENTINEL_ENV).as_deref() != Some(std::ffi::OsStr::new("1")) {
            return None;
        }
        let expected = match std::env::var(CHILD_EXPECT_ENV).ok()?.as_str() {
            "Busy" => OpenExistingObservationV1::Busy,
            "Opened" => OpenExistingObservationV1::Opened,
            "Invalidated" => OpenExistingObservationV1::Invalidated,
            _ => return None,
        };
        let observation = open_existing_v1(Path::new(&root));
        println!("LAYERFS_CHILD_RESULT={observation:?}");
        Some((observation, expected))
    }

    pub fn open_existing_v1(root: &Path) -> OpenExistingObservationV1 {
        match FsCasV1::open_existing(root) {
            Ok(_) => OpenExistingObservationV1::Opened,
            Err(FsCasErrorV1::Busy) => OpenExistingObservationV1::Busy,
            Err(FsCasErrorV1::Invalidated) => OpenExistingObservationV1::Invalidated,
            Err(_) => OpenExistingObservationV1::Rejected,
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct CreateFaultObservationV1 {
        error: Option<PublicationErrorV1>,
        operation_error: Option<PublicationErrorV1>,
        terminal_error: Option<PublicationErrorV1>,
        marker_fault_boundaries: (bool, bool, bool, bool, bool),
        marker_cleanup_observation: (Option<u64>, Option<u64>, bool, bool, bool, bool),
        setup_storage: (u64, u64, u64, u64, u64, u64, u64, u64, u64, u64),
        incumbent_marker_bytes: (Option<[u8; 8]>, Option<[u8; 8]>),
        filesystem_failure: Option<FilesystemFaultFailureV1>,
        first_cause: Option<PublicationCauseV1>,
        dominant_cause: Option<PublicationCauseV1>,
        panicked: bool,
        panic_payload: Option<&'static str>,
        control_fired: bool,
        alias_injected: bool,
        cleanup_calls: u32,
        carrier_installed: bool,
        poisoned: bool,
        bound_invoked: bool,
        supply_invoked: bool,
        followup_succeeded: bool,
        terminal_hook_calls: u32,
        invalidation_attempts: u32,
        global_seen_injected: bool,
        preparation_bytes: u64,
        preparation_entries: u64,
        preparation_residue: PreparationResidueV1,
        immutable_bytes: u64,
        immutable_entries: u64,
        immutable_residue_bytes: u64,
        immutable_residue_inodes: u64,
        carrier_bytes: u64,
        carrier_entries: u64,
        locator_bytes: u64,
        locator_entries: u64,
        catalog_bytes: u64,
        catalog_entries: u64,
        closure_bytes: u64,
        closure_entries: u64,
        residue_bytes: u64,
        mutable_preparation_residue_bytes: u64,
        mutable_preparation_residue_inodes: u64,
        source_read_calls: u64,
        catalog_operations: u64,
        source_bytes_read: u64,
        global_seen_lookups: u64,
        global_seen_probes: u64,
        global_seen_maximum_probe: u64,
        global_seen_entries: u64,
        global_seen_table_bytes: u64,
        global_seen_metadata_bytes_read: u64,
        global_seen_metadata_read_calls: u64,
        global_seen_metadata_bytes_written: u64,
        storage_bytes_requested: u64,
        storage_bytes_reserved: u64,
        storage_bytes_released: u64,
        storage_bytes_committed: u64,
        storage_bytes_retained: u64,
        storage_inodes_requested: u64,
        storage_inodes_reserved: u64,
        storage_inodes_released: u64,
        storage_inodes_committed: u64,
        storage_inodes_retained: u64,
        operation_slots: u64,
        operation_active: u64,
        operation_queue: (u64, u64, u64),
        root_admission_queue: (u64, u64, u64),
        storage_active_operations: u64,
        storage_active_bytes: u64,
        storage_active_inodes: u64,
        unwind_authority: (u64, u64, (u64, u64, u64), (u64, u64, u64), bool),
        followup_bound_invoked: bool,
        followup_supply_invoked: bool,
        followup_preparation_entries: u64,
        followup_storage: (u64, u64, u64, u64, u64, u64, u64, u64, u64, u64),
        followup_zero_forbidden_work: bool,
        usable_handles: (bool, bool, bool),
        invalidated: bool,
        stale_invalidated: bool,
        reopen_invalidated: bool,
        reopen_rejected: bool,
        persistent_invalidation: bool,
        visibility_lock_available: bool,
        publication_lock_available: bool,
        zero_forbidden_work: bool,
    }

    /// Immutable scalar custody for the historical complete-Create owners.
    /// Concrete CAS, preparation, writer, and handoff types remain private.
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct CompleteCreateCountersV1 {
        pub physical_carrier_object_writes: u64,
        pub pack_entries: u64,
        pub pack_bytes: u64,
        pub carrier_bytes_total: u64,
        pub ring_fills: u64,
        pub ring_wrap_spans: u64,
        pub cdc_scan_calls: u64,
        pub cdc_scan_bytes: u64,
        pub bytes_boundary_inspected: u64,
        pub seqcdc_comparisons: u64,
        pub seqcdc_equal_absorptions: u64,
        pub seqcdc_opposing_slopes: u64,
        pub seqcdc_jumps: u64,
        pub seqcdc_jump_bytes: u64,
        pub global_seen_lookups: u64,
        pub global_seen_probes: u64,
        pub global_seen_metadata_bytes_read: u64,
        pub global_seen_metadata_read_calls: u64,
        pub global_seen_metadata_bytes_written: u64,
        pub global_seen_maximum_probe: u64,
        pub global_seen_entries: u64,
        pub global_seen_table_bytes: u64,
        pub version_objects_created: u64,
        pub tree_objects_created: u64,
        pub file_objects_created: u64,
        pub symlink_objects_created: u64,
        pub chunk_objects_created: u64,
        pub version_objects_reused: u64,
        pub tree_objects_reused: u64,
        pub file_objects_reused: u64,
        pub symlink_objects_reused: u64,
        pub chunk_objects_reused: u64,
        pub pack_local_objects_created: u64,
        pub pack_local_objects_reused: u64,
        pub source_read_calls: u64,
        pub source_bytes_read: u64,
        pub fscas_read_calls: u64,
        pub fscas_bytes_read: u64,
        pub fscas_bytes_written: u64,
        pub closure_fences: u64,
        pub visibility_lock_acquisitions: u64,
        pub publication_lock_acquisitions: u64,
        pub file_sort_comparisons: u64,
        pub file_sort_record_reads: u64,
        pub file_sort_record_writes: u64,
        pub file_sort_passes: u64,
        pub file_sort_control_polls: u64,
        pub file_sort_work_units: u64,
        pub file_sort_maximum_work_budget: u64,
        pub file_sort_temporary_bytes_high_water: u64,
        pub storage_bytes_requested: u64,
        pub storage_bytes_reserved: u64,
        pub storage_bytes_released: u64,
        pub storage_bytes_committed: u64,
        pub storage_bytes_retained: u64,
        pub storage_inodes_requested: u64,
        pub storage_inodes_reserved: u64,
        pub storage_inodes_released: u64,
        pub storage_inodes_committed: u64,
        pub storage_inodes_retained: u64,
        pub root_reserved_bytes_high_water: u64,
        pub root_reserved_inodes_high_water: u64,
        pub mutable_preparation_residue_bytes: u64,
        pub mutable_preparation_residue_inodes: u64,
        pub immutable_residue_bytes: u64,
        pub immutable_residue_inodes: u64,
        pub unreachable_installed_residue_bytes: u64,
        pub zero_forbidden_work: bool,
        pub storage_equations_hold: bool,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct CompleteCreateObservationV1 {
        pub error: Option<PublicationErrorV1>,
        pub error_from_storage: bool,
        pub control_fired: bool,
        pub algorithm: Option<CdcAlgorithmV1>,
        pub pack_installed: bool,
        pub object_count: u64,
        pub carrier_count: u32,
        pub carrier_rollovers: u32,
        pub carriers_installed: u32,
        pub carriers_reused: u32,
        pub reference_spool_observed: bool,
        pub reference_spool_bytes: Option<u64>,
        pub reference_spool_operation_scoped: bool,
        pub reference_spool_method: &'static str,
        pub index_spool_observed: bool,
        pub index_spool_bytes: Option<u64>,
        pub index_spool_operation_scoped: bool,
        pub index_spool_method: &'static str,
        pub terminal_optional_observations_match_counters: bool,
        pub terminal_optional_observations_empty: bool,
        pub preparation_usage: (u64, u64),
        pub immutable_usage: (u64, u64),
        pub operation_authority_clean: bool,
        pub operation_admitted_slots: u64,
        pub operation_admission_active: u64,
        pub operation_admission_queue: (u64, u64, u64),
        pub storage_admission_active: (u64, u64, u64),
        pub preparation_entries: u64,
        pub root_usable: bool,
        pub stale_usable: bool,
        pub closure_marker_observed: bool,
        pub visibility_lock_available: bool,
        pub publication_lock_available: bool,
        pub closure_publication_acquisitions: u64,
        pub closure_publication_releases: u64,
        pub observed_visibility_acquisitions: u64,
        pub observed_visibility_releases: u64,
        pub observed_publication_acquisitions: u64,
        pub observed_publication_releases: u64,
        pub counters: CompleteCreateCountersV1,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum CompleteCreateCaseV1 {
        StorageCounterMergeOverflow,
        FastCdcCounterOverflow,
        SeqCdcCounterOverflow,
        GlobalSeenCounterOverflow,
        OperationSpoolWriteOverflow,
        OperationSpoolReadOverflow,
        CountedPackReadOverflow,
        SameCarrierComparisonOverflow,
        PostAdmissionCarrierTallyOverflow,
        CreatedDispositionOverflow,
        TreeReusedDispositionOverflow,
        Algorithm(CdcAlgorithmV1),
        Exact100MiB,
        ClosureMarkerLockScope,
        WriterDirectLockObservations,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum CompleteMutationCaseV1 {
        ReplaceAndMetadata,
        AddMoveRemove,
        CrossDirectoryMove,
        Update,
        UpdateReferenceMetadataOverflow,
        UpdateExactRejoinOverflow,
        UnauthenticatedBase,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum CompleteMutationTerminalV1 {
        Succeeded,
        IntegerOverflow,
        FsCas,
        IdMismatch,
    }

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct CompleteMutationCountersV1 {
        pub update_reference_metadata_records: u64,
        pub update_reference_metadata_bytes: u64,
        pub update_base_payload_bytes: u64,
        pub update_inserted_bytes: u64,
        pub update_resynchronization_bytes: u64,
        pub exact_rejoin_bytes: u64,
        pub rejoin_successes: u64,
        pub rejoin_failures: u64,
        pub anchor_attempts: u64,
        pub bytes_read: u64,
        pub source_read_calls: u64,
        pub source_bytes_read: u64,
        pub fscas_read_calls: u64,
        pub fscas_bytes_read: u64,
        pub update_failures: u64,
        pub storage_bytes_requested: u64,
        pub storage_bytes_reserved: u64,
        pub storage_bytes_released: u64,
        pub storage_bytes_committed: u64,
        pub storage_bytes_retained: u64,
        pub storage_inodes_requested: u64,
        pub storage_inodes_reserved: u64,
        pub storage_inodes_released: u64,
        pub storage_inodes_committed: u64,
        pub storage_inodes_retained: u64,
        pub root_storage_active_reserved_bytes_lifetime_high_water: u64,
        pub root_storage_active_reserved_inodes_lifetime_high_water: u64,
        pub mutable_preparation_residue_bytes: u64,
        pub mutable_preparation_residue_inodes: u64,
        pub unreachable_installed_residue_bytes: u64,
        pub storage_preparation_bytes_current_after_cleanup: u64,
        pub storage_preparation_inodes_current_after_cleanup: u64,
        pub zero_forbidden_work: bool,
        pub storage_equations_hold: bool,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct CompleteMutationObservationV1 {
        pub terminal: CompleteMutationTerminalV1,
        pub accepted_root: Option<PhysicalTreeIdV1>,
        pub base_tree: Option<PhysicalTreeIdV1>,
        pub replaced_root: Option<PhysicalTreeIdV1>,
        pub replacement_tree: Option<PhysicalTreeIdV1>,
        pub metadata_file: Option<PhysicalFileIdV1>,
        pub replacement_file: Option<PhysicalFileIdV1>,
        pub metadata_root: Option<PhysicalTreeIdV1>,
        pub metadata_tree: Option<PhysicalTreeIdV1>,
        pub added_root: Option<PhysicalTreeIdV1>,
        pub added_tree: Option<PhysicalTreeIdV1>,
        pub moved_root: Option<PhysicalTreeIdV1>,
        pub moved_tree: Option<PhysicalTreeIdV1>,
        pub removed_root: Option<PhysicalTreeIdV1>,
        pub removed_tree: Option<PhysicalTreeIdV1>,
        pub updated_root: Option<PhysicalTreeIdV1>,
        pub update_tree: Option<PhysicalTreeIdV1>,
        pub completed_operations: u32,
        pub final_root_returns_to_base: bool,
        pub algorithm_is_fastcdc: bool,
        pub update_algorithm: Option<CdcAlgorithmV1>,
        pub validated_handoffs: u32,
        pub storage_terminals: u32,
        pub source_offset: u64,
        pub namespace_before: Option<((u64, u64), (u64, u64))>,
        pub exact_operation_namespace_usage: Option<((u64, u64), (u64, u64))>,
        pub authority_clean: bool,
        pub namespace_entries_are_regular: bool,
        pub root_usable: bool,
        pub stale_usable: bool,
        pub accepted_version: Option<PhysicalVersionRecordIdV1>,
        pub wrong_version: Option<PhysicalVersionRecordIdV1>,
        pub counters: CompleteMutationCountersV1,
        pub operation_counters: [CompleteMutationCountersV1; 3],
        pub operation_counter_count: u32,
        pub operation_admitted_slots: u64,
        pub operation_admission_active: u64,
        pub storage_admission_active: (u64, u64, u64),
        pub preparation_entries: u64,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct EquivalentCreateLifecycleObservationV1 {
        pub success_traces_equal: bool,
        pub success_starts_at_slot_reservation: bool,
        pub success_ends_at_validated_handoff: bool,
        pub failed_one_control_fired: bool,
        pub failed_tree_control_fired: bool,
        pub failure_errors_equal: bool,
        pub failure_traces_equal: bool,
        pub failure_trace_has_no_handoff: bool,
        pub failed_one_clean: bool,
        pub failed_tree_clean: bool,
        pub success_one_counters: CompleteCreateCountersV1,
        pub success_tree_counters: CompleteCreateCountersV1,
        pub failed_one_counters: CompleteCreateCountersV1,
        pub failed_tree_counters: CompleteCreateCountersV1,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum PreparationResidueV1 {
        None,
        BuiltDirectories,
        BuiltFiles,
        GlobalSeen,
        ClosureObjects,
        PackIndex,
        ChunkReferences,
        LocatorReceipts,
        PrivatePack,
        Other,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum PreparationConstructionBoundaryV1 {
        CreateBuiltDirectories,
        CreateBuiltFiles,
        CreateGlobalSeen,
        CreateClosureObjects,
        CreatePackIndex,
        CreateChunkReferences,
        InitializeGlobalSeen,
        SetPermissions,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum PreparationCleanupBoundaryV1 {
        BuiltDirectories,
        BuiltFiles,
        GlobalSeen,
        ClosureObjects,
        PackIndex,
        ChunkReferences,
        LocatorReceipts,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct OperationBoundaryObservationV1 {
        completed: bool,
        starts: u32,
        ends: u32,
        preparation_empty_at_start: bool,
        preparation_empty_at_end: bool,
    }

    impl OperationBoundaryObservationV1 {
        pub const fn completed(self) -> bool {
            self.completed
        }

        pub const fn starts(self) -> u32 {
            self.starts
        }

        pub const fn ends(self) -> u32 {
            self.ends
        }

        pub const fn preparation_empty_at_start(self) -> bool {
            self.preparation_empty_at_start
        }

        pub const fn preparation_empty_at_end(self) -> bool {
            self.preparation_empty_at_end
        }
    }

    macro_rules! create_fault_getters {
        ($($name:ident: $field:ident => $ty:ty),* $(,)?) => {
            impl CreateFaultObservationV1 {
                $(pub const fn $name(self) -> $ty { self.$field })*
            }
        };
    }

    create_fault_getters! {
        error: error => Option<PublicationErrorV1>,
        operation_error: operation_error => Option<PublicationErrorV1>,
        finish_terminal_v1: terminal_error => Option<PublicationErrorV1>,
        marker_fault_boundaries: marker_fault_boundaries => (bool, bool, bool, bool, bool),
        marker_cleanup_observation: marker_cleanup_observation => (Option<u64>, Option<u64>, bool, bool, bool, bool),
        setup_storage: setup_storage => (u64, u64, u64, u64, u64, u64, u64, u64, u64, u64),
        incumbent_marker_bytes: incumbent_marker_bytes => (Option<[u8; 8]>, Option<[u8; 8]>),
        filesystem_failure: filesystem_failure => Option<FilesystemFaultFailureV1>,
        first_cause: first_cause => Option<PublicationCauseV1>,
        dominant_cause: dominant_cause => Option<PublicationCauseV1>,
        panicked: panicked => bool,
        panic_payload: panic_payload => Option<&'static str>,
        control_fired: control_fired => bool,
        alias_injected: alias_injected => bool,
        cleanup_calls: cleanup_calls => u32,
        carrier_installed: carrier_installed => bool,
        poisoned: poisoned => bool,
        bound_invoked: bound_invoked => bool,
        supply_invoked: supply_invoked => bool,
        followup_succeeded: followup_succeeded => bool,
        terminal_hook_calls: terminal_hook_calls => u32,
        invalidation_attempts: invalidation_attempts => u32,
        global_seen_injected: global_seen_injected => bool,
        preparation_bytes: preparation_bytes => u64,
        preparation_entries: preparation_entries => u64,
        preparation_residue: preparation_residue => PreparationResidueV1,
        immutable_bytes: immutable_bytes => u64,
        immutable_entries: immutable_entries => u64,
        immutable_residue_bytes: immutable_residue_bytes => u64,
        immutable_residue_inodes: immutable_residue_inodes => u64,
        carrier_bytes: carrier_bytes => u64,
        carrier_entries: carrier_entries => u64,
        locator_bytes: locator_bytes => u64,
        locator_entries: locator_entries => u64,
        catalog_bytes: catalog_bytes => u64,
        catalog_entries: catalog_entries => u64,
        closure_bytes: closure_bytes => u64,
        closure_entries: closure_entries => u64,
        residue_bytes: residue_bytes => u64,
        mutable_preparation_residue_bytes: mutable_preparation_residue_bytes => u64,
        mutable_preparation_residue_inodes: mutable_preparation_residue_inodes => u64,
        source_read_calls: source_read_calls => u64,
        catalog_operations: catalog_operations => u64,
        source_bytes_read: source_bytes_read => u64,
        global_seen_lookups: global_seen_lookups => u64,
        global_seen_probes: global_seen_probes => u64,
        global_seen_maximum_probe: global_seen_maximum_probe => u64,
        global_seen_entries: global_seen_entries => u64,
        global_seen_table_bytes: global_seen_table_bytes => u64,
        global_seen_metadata_bytes_read: global_seen_metadata_bytes_read => u64,
        global_seen_metadata_read_calls: global_seen_metadata_read_calls => u64,
        global_seen_metadata_bytes_written: global_seen_metadata_bytes_written => u64,
        storage_bytes_requested: storage_bytes_requested => u64,
        storage_bytes_reserved: storage_bytes_reserved => u64,
        storage_bytes_released: storage_bytes_released => u64,
        storage_bytes_committed: storage_bytes_committed => u64,
        storage_bytes_retained: storage_bytes_retained => u64,
        storage_inodes_requested: storage_inodes_requested => u64,
        storage_inodes_reserved: storage_inodes_reserved => u64,
        storage_inodes_released: storage_inodes_released => u64,
        storage_inodes_committed: storage_inodes_committed => u64,
        storage_inodes_retained: storage_inodes_retained => u64,
        operation_slots: operation_slots => u64,
        operation_active: operation_active => u64,
        operation_queue: operation_queue => (u64, u64, u64),
        root_admission_queue: root_admission_queue => (u64, u64, u64),
        storage_active_operations: storage_active_operations => u64,
        storage_active_bytes: storage_active_bytes => u64,
        storage_active_inodes: storage_active_inodes => u64,
        usable_handles: usable_handles => (bool, bool, bool),
        invalidated: invalidated => bool,
        stale_invalidated: stale_invalidated => bool,
        reopen_invalidated: reopen_invalidated => bool,
        reopen_rejected: reopen_rejected => bool,
        persistent_invalidation: persistent_invalidation => bool,
        visibility_lock_available: visibility_lock_available => bool,
        publication_lock_available: publication_lock_available => bool,
        zero_forbidden_work: zero_forbidden_work => bool,
    }

    impl CreateFaultObservationV1 {
        const fn with_alias_injected(mut self, alias_injected: bool) -> Self {
            self.alias_injected = alias_injected;
            self
        }

        pub const fn unwind_authority(self) -> (u64, u64, (u64, u64, u64), (u64, u64, u64), bool) {
            self.unwind_authority
        }

        pub const fn followup_observation(
            self,
        ) -> (
            bool,
            bool,
            u64,
            (u64, u64, u64, u64, u64, u64, u64, u64, u64, u64),
            bool,
        ) {
            (
                self.followup_bound_invoked,
                self.followup_supply_invoked,
                self.followup_preparation_entries,
                self.followup_storage,
                self.followup_zero_forbidden_work,
            )
        }
    }

    struct CreateFaultAttempt {
        error: Option<FsCasErrorV1>,
        panicked: bool,
        panic_payload: Option<&'static str>,
    }

    #[derive(Clone, Copy, Default)]
    struct CreateFaultControlObservation {
        control_fired: bool,
        cleanup_calls: u32,
        carrier_installed: bool,
        poisoned: bool,
    }

    struct CounterSource {
        len: u64,
        offset: u64,
    }

    impl ContentSourceV1 for CounterSource {
        fn resident_memory_bound_bytes(&self) -> crate::CoreResult<u64> {
            Ok(core::mem::size_of::<Self>() as u64)
        }

        fn read(&mut self, destination: &mut [u8]) -> Result<usize, ContentSourceErrorV1> {
            let remaining = usize::try_from(self.len - self.offset).unwrap_or(usize::MAX);
            let take = destination.len().min(remaining);
            for (relative, byte) in destination[..take].iter_mut().enumerate() {
                let position = self.offset + relative as u64;
                let block = position / 8;
                let lane = usize::try_from(position % 8).unwrap();
                let mut mixed = block ^ 0x6a09_e667_f3bc_c909;
                mixed ^= mixed >> 30;
                mixed = mixed.wrapping_mul(0xbf58_476d_1ce4_e5b9);
                mixed ^= mixed >> 27;
                mixed = mixed.wrapping_mul(0x94d0_49bb_1331_11eb);
                mixed ^= mixed >> 31;
                *byte = mixed.to_le_bytes()[lane];
            }
            self.offset += take as u64;
            Ok(take)
        }
    }

    struct CallbackSupplier {
        bound_invoked: Arc<AtomicBool>,
        supply_invoked: Arc<AtomicBool>,
        len: u64,
    }

    impl SourceSupplierV1 for CallbackSupplier {
        type Source = CounterSource;

        fn resident_memory_bound_bytes(&self) -> crate::CoreResult<u64> {
            self.bound_invoked.store(true, Ordering::Release);
            Ok(core::mem::size_of::<CounterSource>() as u64)
        }

        fn supply(self) -> crate::CoreResult<Self::Source> {
            self.supply_invoked.store(true, Ordering::Release);
            Ok(CounterSource {
                len: self.len,
                offset: 0,
            })
        }
    }

    struct PanicDuringPreparationFreeSupplier {
        cas_to_poison: Option<FsCasV1>,
        bound_invoked: Arc<AtomicBool>,
        supply_invoked: Arc<AtomicBool>,
    }

    impl SourceSupplierV1 for PanicDuringPreparationFreeSupplier {
        type Source = CounterSource;

        fn resident_memory_bound_bytes(&self) -> crate::CoreResult<u64> {
            self.bound_invoked.store(true, Ordering::Release);
            if let Some(cas) = self.cas_to_poison.as_ref() {
                cas.poison_storage_admission_for_test_v1();
            }
            panic!("injected preparation-free supplier-bound unwind");
        }

        fn supply(self) -> crate::CoreResult<Self::Source> {
            self.supply_invoked.store(true, Ordering::Release);
            Ok(CounterSource { len: 1, offset: 0 })
        }
    }

    struct FailingPreparationFreeSupplier {
        bound_invoked: Arc<AtomicBool>,
        supply_invoked: Arc<AtomicBool>,
    }

    impl SourceSupplierV1 for FailingPreparationFreeSupplier {
        type Source = CounterSource;

        fn resident_memory_bound_bytes(&self) -> crate::CoreResult<u64> {
            self.bound_invoked.store(true, Ordering::Release);
            Err(CoreError::ResourceRefused)
        }

        fn supply(self) -> crate::CoreResult<Self::Source> {
            self.supply_invoked.store(true, Ordering::Release);
            Ok(CounterSource { len: 1, offset: 0 })
        }
    }

    struct FailingBodySource;

    impl ContentSourceV1 for FailingBodySource {
        fn resident_memory_bound_bytes(&self) -> crate::CoreResult<u64> {
            Ok(core::mem::size_of::<Self>() as u64)
        }

        fn read(&mut self, _destination: &mut [u8]) -> Result<usize, ContentSourceErrorV1> {
            Err(ContentSourceErrorV1::Failure)
        }
    }

    struct FailingBodySupplier;

    impl SourceSupplierV1 for FailingBodySupplier {
        type Source = FailingBodySource;

        fn resident_memory_bound_bytes(&self) -> crate::CoreResult<u64> {
            Ok(core::mem::size_of::<FailingBodySource>() as u64)
        }

        fn supply(self) -> crate::CoreResult<Self::Source> {
            Ok(FailingBodySource)
        }
    }

    struct FailingAfterBytesSource {
        remaining: u64,
    }

    impl ContentSourceV1 for FailingAfterBytesSource {
        fn resident_memory_bound_bytes(&self) -> crate::CoreResult<u64> {
            Ok(core::mem::size_of::<Self>() as u64)
        }

        fn read(&mut self, destination: &mut [u8]) -> Result<usize, ContentSourceErrorV1> {
            if self.remaining == 0 {
                return Err(ContentSourceErrorV1::Failure);
            }
            let take = usize::try_from(self.remaining.min(destination.len() as u64))
                .map_err(|_| ContentSourceErrorV1::Failure)?;
            destination[..take].fill(0x5a);
            self.remaining -= take as u64;
            Ok(take)
        }
    }

    struct FailingAfterBytesSupplier {
        bytes_before_failure: u64,
    }

    impl SourceSupplierV1 for FailingAfterBytesSupplier {
        type Source = FailingAfterBytesSource;

        fn resident_memory_bound_bytes(&self) -> crate::CoreResult<u64> {
            Ok(core::mem::size_of::<FailingAfterBytesSource>() as u64)
        }

        fn supply(self) -> crate::CoreResult<Self::Source> {
            Ok(FailingAfterBytesSource {
                remaining: self.bytes_before_failure,
            })
        }
    }

    #[derive(Default)]
    struct ContinueFaultControl;

    impl CdcControlV1 for ContinueFaultControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for ContinueFaultControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    struct ExactOperationBoundaryControl<'a> {
        root: &'a Path,
        starts: u32,
        ends: u32,
        preparation_empty_at_start: bool,
        preparation_empty_at_end: bool,
    }

    impl CdcControlV1 for ExactOperationBoundaryControl<'_> {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for ExactOperationBoundaryControl<'_> {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            let preparation_empty = || {
                fs::read_dir(self.root.join("preparation"))
                    .map(|entries| entries.count() == 0)
                    .unwrap_or(false)
            };
            match boundary {
                FsCasBoundaryV1::BeforeOperationSlotReservationRequest => {
                    self.starts += 1;
                    self.preparation_empty_at_start &= preparation_empty();
                }
                FsCasBoundaryV1::AfterCompleteValidatedHandoff => {
                    self.ends += 1;
                    self.preparation_empty_at_end &= preparation_empty();
                }
                _ => {}
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    #[derive(Clone, Copy)]
    enum FinalHandoffFault {
        AdmissionPoison,
        StoragePoison,
        Unwind,
    }

    struct FinalHandoffControl {
        cas: FsCasV1,
        fault: FinalHandoffFault,
        panic_during_invalidation: bool,
        fired: bool,
        invalidation_panicked: bool,
    }

    impl CdcControlV1 for FinalHandoffControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for FinalHandoffControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if self.fired || boundary != FsCasBoundaryV1::AfterCompleteValidatedHandoff {
                return;
            }
            self.fired = true;
            match self.fault {
                FinalHandoffFault::AdmissionPoison => {
                    self.cas.poison_operation_admission_for_test_v1()
                }
                FinalHandoffFault::StoragePoison => self.cas.poison_storage_admission_for_test_v1(),
                FinalHandoffFault::Unwind => panic!("injected final handoff unwind"),
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if self.panic_during_invalidation
                && !self.invalidation_panicked
                && target == FsCasCleanupTargetV1::RootInvalidation
            {
                self.invalidation_panicked = true;
                panic!("injected final-handoff invalidation unwind")
            }
            false
        }
    }

    struct FailFilesystemBoundaryOnce {
        boundary: FsCasFilesystemBoundaryV1,
        error: FsCasErrorV1,
        fired: bool,
    }

    impl CdcControlV1 for FailFilesystemBoundaryOnce {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for FailFilesystemBoundaryOnce {
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
            if !self.fired && boundary == self.boundary {
                self.fired = true;
                Some(self.error)
            } else {
                None
            }
        }
    }

    fn filesystem_fault_spec(
        case: FilesystemFaultCaseV1,
    ) -> (FsCasFilesystemBoundaryV1, FsCasErrorV1, bool, u64) {
        match case {
            FilesystemFaultCaseV1::PreparationCreateNoSpace => (
                FsCasFilesystemBoundaryV1::PreparationCreate,
                FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::NoSpace),
                true,
                0x800,
            ),
            FilesystemFaultCaseV1::PreparationResizeQuota => (
                FsCasFilesystemBoundaryV1::PreparationResize,
                FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::Quota),
                true,
                0x801,
            ),
            FilesystemFaultCaseV1::PermissionChangeDenied => (
                FsCasFilesystemBoundaryV1::PermissionChange,
                FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
                true,
                0x802,
            ),
            FilesystemFaultCaseV1::PreparationWriteShortWrite => (
                FsCasFilesystemBoundaryV1::PreparationWrite,
                FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ShortWrite),
                false,
                0x803,
            ),
            FilesystemFaultCaseV1::PrivatePackCreateInodeExhaustion => (
                FsCasFilesystemBoundaryV1::PrivatePackCreate,
                FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::InodeExhaustion),
                false,
                0x804,
            ),
            FilesystemFaultCaseV1::PrivatePackWriteShortWrite => (
                FsCasFilesystemBoundaryV1::PrivatePackWrite,
                FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ShortWrite),
                false,
                0x805,
            ),
            FilesystemFaultCaseV1::PrivatePackFlushWriteFailure => (
                FsCasFilesystemBoundaryV1::PrivatePackFlush,
                FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure),
                false,
                0x806,
            ),
            FilesystemFaultCaseV1::CarrierHardLinkUnsupported => (
                FsCasFilesystemBoundaryV1::CarrierHardLink,
                FsCasErrorV1::Unsupported,
                false,
                0x807,
            ),
            FilesystemFaultCaseV1::MarkerCreateInodeExhaustion => (
                FsCasFilesystemBoundaryV1::MarkerCreate,
                FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::InodeExhaustion),
                false,
                0x808,
            ),
            FilesystemFaultCaseV1::MarkerWriteNoSpace => (
                FsCasFilesystemBoundaryV1::MarkerWrite,
                FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::NoSpace),
                false,
                0x809,
            ),
            FilesystemFaultCaseV1::MarkerFlushWriteFailure => (
                FsCasFilesystemBoundaryV1::MarkerFlush,
                FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure),
                false,
                0x80a,
            ),
            FilesystemFaultCaseV1::MarkerHardLinkUnsupported => (
                FsCasFilesystemBoundaryV1::MarkerHardLink,
                FsCasErrorV1::Unsupported,
                false,
                0x80b,
            ),
        }
    }

    fn map_filesystem_fault_error(error: Option<FsCasErrorV1>) -> Option<FilesystemFaultErrorV1> {
        error.map(|error| match error {
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::NoSpace) => {
                FilesystemFaultErrorV1::Filesystem(FilesystemFaultFailureV1::NoSpace)
            }
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::Quota) => {
                FilesystemFaultErrorV1::Filesystem(FilesystemFaultFailureV1::Quota)
            }
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::InodeExhaustion) => {
                FilesystemFaultErrorV1::Filesystem(FilesystemFaultFailureV1::InodeExhaustion)
            }
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure) => {
                FilesystemFaultErrorV1::Filesystem(FilesystemFaultFailureV1::WriteFailure)
            }
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ShortWrite) => {
                FilesystemFaultErrorV1::Filesystem(FilesystemFaultFailureV1::ShortWrite)
            }
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied) => {
                FilesystemFaultErrorV1::Filesystem(FilesystemFaultFailureV1::PermissionDenied)
            }
            FsCasErrorV1::Unsupported => FilesystemFaultErrorV1::Unsupported,
            unexpected => panic!("unexpected filesystem fault result: {unexpected:?}"),
        })
    }

    pub fn filesystem_fault_v1(
        root: &Path,
        case: FilesystemFaultCaseV1,
    ) -> FilesystemFaultObservationV1 {
        let (boundary, expected_error, before_supply, cancellation_key) =
            filesystem_fault_spec(case);
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = FailFilesystemBoundaryOnce {
            boundary,
            error: expected_error,
            fired: false,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            cancellation_key,
            1,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: 1,
            },
            &mut control,
            &mut counters,
        );
        let (preparation_bytes, preparation_entries) = directory_usage(&root.join("preparation"));
        let (_, immutable_entries) = immutable_usage(root);
        let (storage_active_operations, storage_active_bytes, storage_active_inodes) =
            cas.storage_admission_active_for_test_v1();
        let _ = preparation_bytes;
        FilesystemFaultObservationV1 {
            error: map_filesystem_fault_error(attempt.error),
            fired: control.fired,
            bound_invoked: bound_invoked.load(Ordering::Acquire),
            supply_invoked: supply_invoked.load(Ordering::Acquire),
            source_read_calls: counters.source_read_calls,
            preparation_entries,
            immutable_entries,
            storage_bytes_requested: counters.storage_bytes_requested,
            storage_bytes_reserved: counters.storage_bytes_reserved,
            storage_bytes_released: counters.storage_bytes_released,
            storage_bytes_committed: counters.storage_bytes_committed,
            storage_bytes_retained: counters.storage_bytes_retained,
            storage_inodes_requested: counters.storage_inodes_requested,
            storage_inodes_reserved: counters.storage_inodes_reserved,
            storage_inodes_released: counters.storage_inodes_released,
            storage_inodes_committed: counters.storage_inodes_committed,
            storage_inodes_retained: counters.storage_inodes_retained,
            operation_slots: cas.operation_admitted_slots_v1(),
            operation_active: cas.operation_admission_active_for_test_v1(),
            storage_active_operations,
            storage_active_bytes,
            storage_active_inodes,
            root_usable: cas.occupied().is_ok(),
            stale_usable: stale.occupied().is_ok(),
            zero_forbidden_work: counters.has_zero_forbidden_work()
                && (before_supply || supply_invoked.load(Ordering::Acquire)),
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum CarrierLinkFaultFailureV1 {
        Unsupported,
        WriteFailure,
    }

    struct PoisonStorageAtCarrierLink {
        cas: FsCasV1,
        error: FsCasErrorV1,
        fired: bool,
        invalidation_attempts: u32,
        fail_invalidation: bool,
    }

    impl CdcControlV1 for PoisonStorageAtCarrierLink {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for PoisonStorageAtCarrierLink {
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
            if !self.fired && boundary == FsCasFilesystemBoundaryV1::CarrierHardLink {
                self.fired = true;
                self.cas.poison_storage_admission_for_test_v1();
                Some(self.error)
            } else {
                None
            }
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::RootInvalidation {
                self.invalidation_attempts += 1;
                return self.fail_invalidation;
            }
            false
        }
    }

    fn carrier_link_fault_error(failure: CarrierLinkFaultFailureV1) -> FsCasErrorV1 {
        match failure {
            CarrierLinkFaultFailureV1::Unsupported => FsCasErrorV1::Unsupported,
            CarrierLinkFaultFailureV1::WriteFailure => {
                FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure)
            }
        }
    }

    pub fn carrier_link_fault_v1(
        root: &Path,
        failure: CarrierLinkFaultFailureV1,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = PoisonStorageAtCarrierLink {
            cas: cas.clone(),
            error: carrier_link_fault_error(failure),
            fired: false,
            invalidation_attempts: 0,
            fail_invalidation,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x820,
            1,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: 1,
            },
            &mut control,
            &mut counters,
        );
        observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            control.invalidation_attempts,
            false,
            CreateFaultControlObservation {
                control_fired: control.fired,
                cleanup_calls: 0,
                carrier_installed: false,
                poisoned: control.fired,
            },
            &counters,
        )
    }

    struct InstallCarrierAndPoisonStorage {
        cas: FsCasV1,
        installed: bool,
        invalidation_attempts: u32,
        fail_invalidation: bool,
    }

    impl CdcControlV1 for InstallCarrierAndPoisonStorage {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for InstallCarrierAndPoisonStorage {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if !self.installed && boundary == FsCasBoundaryV1::BeforeCarrierInstall {
                self.cas
                    .install_single_prepared_carrier_for_test_v1()
                    .expect("independent carrier install must win the test race");
                self.installed = true;
                self.cas.poison_storage_admission_for_test_v1();
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::RootInvalidation {
                self.invalidation_attempts += 1;
                return self.fail_invalidation;
            }
            false
        }
    }

    pub fn carrier_exists_fault_v1(
        root: &Path,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = InstallCarrierAndPoisonStorage {
            cas: cas.clone(),
            installed: false,
            invalidation_attempts: 0,
            fail_invalidation,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x821,
            1,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: 1,
            },
            &mut control,
            &mut counters,
        );
        observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            control.invalidation_attempts,
            false,
            CreateFaultControlObservation {
                control_fired: control.installed,
                cleanup_calls: 0,
                carrier_installed: control.installed,
                poisoned: control.installed,
            },
            &counters,
        )
    }

    #[derive(Default)]
    struct FailPreparationCreateAndCleanup {
        preparation_creates: u32,
        create_failed: bool,
        cleanup_failed: bool,
    }

    impl CdcControlV1 for FailPreparationCreateAndCleanup {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for FailPreparationCreateAndCleanup {
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
            if boundary != FsCasFilesystemBoundaryV1::PreparationCreate {
                return None;
            }
            self.preparation_creates += 1;
            if self.preparation_creates == 2 {
                self.create_failed = true;
                Some(FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::NoSpace))
            } else {
                None
            }
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::PreparationSpool && !self.cleanup_failed {
                self.cleanup_failed = true;
                true
            } else {
                false
            }
        }
    }

    pub fn preparation_create_cleanup_fault_v1(root: &Path) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = FailPreparationCreateAndCleanup::default();
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x8ff,
            1,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: 1,
            },
            &mut control,
            &mut counters,
        );
        observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            0,
            false,
            CreateFaultControlObservation {
                control_fired: control.create_failed,
                cleanup_calls: u32::from(control.cleanup_failed),
                carrier_installed: false,
                poisoned: false,
            },
            &counters,
        )
    }

    fn lifecycle_filesystem_error(failure: FilesystemFaultFailureV1) -> FsCasErrorV1 {
        let failure = match failure {
            FilesystemFaultFailureV1::NoSpace => FsCasFilesystemFailureV1::NoSpace,
            FilesystemFaultFailureV1::Quota => FsCasFilesystemFailureV1::Quota,
            FilesystemFaultFailureV1::InodeExhaustion => FsCasFilesystemFailureV1::InodeExhaustion,
            FilesystemFaultFailureV1::ReadFailure => FsCasFilesystemFailureV1::ReadFailure,
            FilesystemFaultFailureV1::WriteFailure => FsCasFilesystemFailureV1::WriteFailure,
            FilesystemFaultFailureV1::ShortRead => FsCasFilesystemFailureV1::ShortRead,
            FilesystemFaultFailureV1::ShortWrite => FsCasFilesystemFailureV1::ShortWrite,
            FilesystemFaultFailureV1::PermissionDenied => {
                FsCasFilesystemFailureV1::PermissionDenied
            }
            FilesystemFaultFailureV1::Unsupported => {
                return FsCasErrorV1::Unsupported;
            }
        };
        FsCasErrorV1::Filesystem(failure)
    }

    struct PreparationPermissionCleanupControl {
        first_error: FsCasErrorV1,
        permission_failed: bool,
        cleanup_calls: u32,
        cleanup_panicked: bool,
        invalidation_attempts: u32,
        fail_invalidation: bool,
    }

    impl CdcControlV1 for PreparationPermissionCleanupControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for PreparationPermissionCleanupControl {
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
            if boundary == FsCasFilesystemBoundaryV1::PermissionChange && !self.permission_failed {
                self.permission_failed = true;
                Some(self.first_error)
            } else {
                None
            }
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            match target {
                FsCasCleanupTargetV1::PreparationSpool => {
                    self.cleanup_calls += 1;
                    if !self.cleanup_panicked {
                        self.cleanup_panicked = true;
                        panic!("injected partial preparation cleanup unwind");
                    }
                    false
                }
                FsCasCleanupTargetV1::RootInvalidation => {
                    self.invalidation_attempts += 1;
                    self.fail_invalidation
                }
                _ => false,
            }
        }
    }

    pub fn preparation_permission_cleanup_fault_v1(
        root: &Path,
        failure: FilesystemFaultFailureV1,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = PreparationPermissionCleanupControl {
            first_error: lifecycle_filesystem_error(failure),
            permission_failed: false,
            cleanup_calls: 0,
            cleanup_panicked: false,
            invalidation_attempts: 0,
            fail_invalidation,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x900 + u64::from(fail_invalidation),
            1,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: 1,
            },
            &mut control,
            &mut counters,
        );
        observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            control.invalidation_attempts,
            false,
            CreateFaultControlObservation {
                control_fired: control.permission_failed,
                cleanup_calls: control.cleanup_calls,
                carrier_installed: false,
                poisoned: false,
            },
            &counters,
        )
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum PreparationConstructionCaseV1 {
        CleanupFails,
        CleanupUnwinds,
        PreCreateAccountingReleaseFails,
    }

    struct PreparationConstructionControl {
        cas: FsCasV1,
        case: PreparationConstructionCaseV1,
        construction_panicked: bool,
        cleanup_calls: u32,
        invalidation_attempts: u32,
        fail_invalidation: bool,
    }

    impl CdcControlV1 for PreparationConstructionControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for PreparationConstructionControl {
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
            let target = match self.case {
                PreparationConstructionCaseV1::CleanupFails
                | PreparationConstructionCaseV1::CleanupUnwinds => {
                    FsCasFilesystemBoundaryV1::PermissionChange
                }
                PreparationConstructionCaseV1::PreCreateAccountingReleaseFails => {
                    FsCasFilesystemBoundaryV1::PreparationCreate
                }
            };
            if boundary == target && !self.construction_panicked {
                self.construction_panicked = true;
                if self.case == PreparationConstructionCaseV1::PreCreateAccountingReleaseFails {
                    self.cas.fail_next_preparation_remove_for_test_v1();
                }
                panic!("injected partial preparation construction unwind");
            }
            None
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            match target {
                FsCasCleanupTargetV1::PreparationSpool
                    if self.case
                        != PreparationConstructionCaseV1::PreCreateAccountingReleaseFails =>
                {
                    self.cleanup_calls += 1;
                    match self.case {
                        PreparationConstructionCaseV1::CleanupFails => true,
                        PreparationConstructionCaseV1::CleanupUnwinds => {
                            panic!("injected partial preparation construction cleanup unwind");
                        }
                        PreparationConstructionCaseV1::PreCreateAccountingReleaseFails => {
                            unreachable!()
                        }
                    }
                }
                FsCasCleanupTargetV1::RootInvalidation => {
                    self.invalidation_attempts += 1;
                    self.fail_invalidation
                }
                _ => false,
            }
        }
    }

    pub fn preparation_construction_unwind_fault_v1(
        root: &Path,
        case: PreparationConstructionCaseV1,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = PreparationConstructionControl {
            cas: cas.clone(),
            case,
            construction_panicked: false,
            cleanup_calls: 0,
            invalidation_attempts: 0,
            fail_invalidation,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x901 + u64::from(fail_invalidation),
            1,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: 1,
            },
            &mut control,
            &mut counters,
        );
        observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            control.invalidation_attempts,
            false,
            CreateFaultControlObservation {
                control_fired: control.construction_panicked,
                cleanup_calls: control.cleanup_calls,
                carrier_installed: false,
                poisoned: false,
            },
            &counters,
        )
    }

    struct PreparationInitializationUnwindControl {
        construction_panicked: bool,
        cleanup_calls: u32,
        invalidation_attempts: u32,
        fail_invalidation: bool,
    }

    impl CdcControlV1 for PreparationInitializationUnwindControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for PreparationInitializationUnwindControl {
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
            if boundary == FsCasFilesystemBoundaryV1::PreparationResize
                && !self.construction_panicked
            {
                self.construction_panicked = true;
                panic!("injected preparation initialization unwind");
            }
            None
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            match target {
                FsCasCleanupTargetV1::PreparationSpool if self.cleanup_calls < 4 => {
                    self.cleanup_calls += 1;
                    self.cleanup_calls == 1
                }
                FsCasCleanupTargetV1::RootInvalidation => {
                    self.invalidation_attempts += 1;
                    self.fail_invalidation
                }
                _ => false,
            }
        }
    }

    pub fn preparation_initialization_cleanup_fault_v1(
        root: &Path,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = PreparationInitializationUnwindControl {
            construction_panicked: false,
            cleanup_calls: 0,
            invalidation_attempts: 0,
            fail_invalidation,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x902 + u64::from(fail_invalidation),
            1,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: 1,
            },
            &mut control,
            &mut counters,
        );
        observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            control.invalidation_attempts,
            false,
            CreateFaultControlObservation {
                control_fired: control.construction_panicked,
                cleanup_calls: control.cleanup_calls,
                carrier_installed: false,
                poisoned: false,
            },
            &counters,
        )
    }

    struct PreparationInitializationPoisonControl {
        cas: FsCasV1,
        construction_panicked: bool,
        cleanup_calls: u32,
        invalidation_attempts: u32,
        fail_invalidation: bool,
        poison_terminal: bool,
    }

    impl CdcControlV1 for PreparationInitializationPoisonControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for PreparationInitializationPoisonControl {
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
            if boundary == FsCasFilesystemBoundaryV1::PreparationResize
                && !self.construction_panicked
            {
                self.construction_panicked = true;
                if self.poison_terminal {
                    self.cas.poison_storage_admission_for_test_v1();
                }
                panic!("injected preparation initialization unwind before outer terminal");
            }
            None
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            match target {
                FsCasCleanupTargetV1::PreparationSpool => {
                    self.cleanup_calls += 1;
                    false
                }
                FsCasCleanupTargetV1::RootInvalidation => {
                    self.invalidation_attempts += 1;
                    self.fail_invalidation
                }
                _ => false,
            }
        }
    }

    pub fn preparation_initialization_unwind_fault_v1(
        root: &Path,
        poison_terminal: bool,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = PreparationInitializationPoisonControl {
            cas: cas.clone(),
            construction_panicked: false,
            cleanup_calls: 0,
            invalidation_attempts: 0,
            fail_invalidation,
            poison_terminal,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x903 + u64::from(poison_terminal) + u64::from(fail_invalidation),
            1,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: 1,
            },
            &mut control,
            &mut counters,
        );
        let fault_usage = (
            directory_usage(&root.join("preparation")),
            immutable_usage(root),
        );
        let unwind_authority = (
            cas.operation_admitted_slots_v1(),
            cas.operation_admission_active_for_test_v1(),
            cas.operation_admission_queue_for_test_v1(),
            cas.storage_admission_active_for_test_v1(),
            cas.occupied().is_ok(),
        );
        let mut followup_observation = None;
        let (attempt, followup_succeeded) = if poison_terminal {
            (attempt, false)
        } else {
            let followup_bound = Arc::new(AtomicBool::new(false));
            let followup_supply = Arc::new(AtomicBool::new(false));
            let mut followup_counters = OperationCountersV1::default();
            let mut followup_control = ContinueFaultControl;
            let followup = run_create_fault_attempt(
                &cas,
                0x904,
                1,
                CallbackSupplier {
                    bound_invoked: Arc::clone(&followup_bound),
                    supply_invoked: Arc::clone(&followup_supply),
                    len: 1,
                },
                &mut followup_control,
                &mut followup_counters,
            );
            let followup_succeeded = followup.error.is_none() && !followup.panicked;
            followup_observation = Some((
                followup_bound.load(Ordering::Acquire),
                followup_supply.load(Ordering::Acquire),
                followup_counters,
            ));
            let error = attempt.error.or(followup.error);
            (
                CreateFaultAttempt {
                    error,
                    panicked: attempt.panicked,
                    panic_payload: attempt.panic_payload,
                },
                followup_succeeded,
            )
        };
        let mut observation = observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            followup_succeeded,
            0,
            control.invalidation_attempts,
            false,
            CreateFaultControlObservation {
                control_fired: control.construction_panicked,
                cleanup_calls: control.cleanup_calls,
                carrier_installed: false,
                poisoned: poison_terminal,
            },
            &counters,
        );
        if !poison_terminal {
            (
                (
                    observation.preparation_bytes,
                    observation.preparation_entries,
                ),
                (observation.immutable_bytes, observation.immutable_entries),
            ) = fault_usage;
            observation.unwind_authority = unwind_authority;
            let (followup_bound, followup_supply, followup_counters) =
                followup_observation.expect("clean unwind performs a followup");
            observation.followup_bound_invoked = followup_bound;
            observation.followup_supply_invoked = followup_supply;
            observation.followup_preparation_entries = directory_usage(&root.join("preparation")).1;
            observation.followup_storage = (
                followup_counters.storage_bytes_requested,
                followup_counters.storage_bytes_reserved,
                followup_counters.storage_bytes_released,
                followup_counters.storage_bytes_committed,
                followup_counters.storage_bytes_retained,
                followup_counters.storage_inodes_requested,
                followup_counters.storage_inodes_reserved,
                followup_counters.storage_inodes_released,
                followup_counters.storage_inodes_committed,
                followup_counters.storage_inodes_retained,
            );
            observation.followup_zero_forbidden_work = followup_counters.has_zero_forbidden_work();
        }
        observation
    }

    struct ClosureUnwindControl {
        cas: FsCasV1,
        closure_panicked: bool,
        cleanup_calls: u32,
        invalidation_attempts: u32,
        fail_invalidation: bool,
        poison_terminal: bool,
    }

    impl CdcControlV1 for ClosureUnwindControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for ClosureUnwindControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary == FsCasBoundaryV1::BeforeClosureMarkerPublication && !self.closure_panicked
            {
                self.closure_panicked = true;
                if self.poison_terminal {
                    self.cas.poison_storage_admission_for_test_v1();
                }
                panic!("injected closure-fence unwind before outer terminal");
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            match target {
                FsCasCleanupTargetV1::PreparationSpool => {
                    self.cleanup_calls += 1;
                    false
                }
                FsCasCleanupTargetV1::RootInvalidation => {
                    self.invalidation_attempts += 1;
                    self.fail_invalidation
                }
                _ => false,
            }
        }
    }

    pub fn closure_unwind_fault_v1(
        root: &Path,
        poison_terminal: bool,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = ClosureUnwindControl {
            cas: cas.clone(),
            closure_panicked: false,
            cleanup_calls: 0,
            invalidation_attempts: 0,
            fail_invalidation,
            poison_terminal,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x905 + u64::from(poison_terminal) + u64::from(fail_invalidation),
            1,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: 1,
            },
            &mut control,
            &mut counters,
        );
        observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            control.invalidation_attempts,
            false,
            CreateFaultControlObservation {
                control_fired: control.closure_panicked,
                cleanup_calls: control.cleanup_calls,
                carrier_installed: false,
                poisoned: poison_terminal,
            },
            &counters,
        )
    }

    struct PreparationAccountingPoisonControl {
        cas: FsCasV1,
        poisoned: bool,
        invalidation_attempts: u32,
        fail_invalidation: bool,
    }

    impl CdcControlV1 for PreparationAccountingPoisonControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for PreparationAccountingPoisonControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary == FsCasBoundaryV1::VisibilityLockAcquired && !self.poisoned {
                self.poisoned = true;
                self.cas.poison_storage_admission_for_test_v1();
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::RootInvalidation {
                self.invalidation_attempts += 1;
                self.fail_invalidation
            } else {
                false
            }
        }
    }

    pub fn preparation_accounting_poison_fault_v1(
        root: &Path,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = PreparationAccountingPoisonControl {
            cas: cas.clone(),
            poisoned: false,
            invalidation_attempts: 0,
            fail_invalidation,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x906 + u64::from(fail_invalidation),
            1,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: 1,
            },
            &mut control,
            &mut counters,
        );
        observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            control.invalidation_attempts,
            false,
            CreateFaultControlObservation {
                control_fired: control.poisoned,
                cleanup_calls: 0,
                carrier_installed: false,
                poisoned: control.poisoned,
            },
            &counters,
        )
    }

    struct PreparationOpenAccountingControl {
        cas: FsCasV1,
        fired: bool,
    }

    impl CdcControlV1 for PreparationOpenAccountingControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for PreparationOpenAccountingControl {
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
            if boundary == FsCasFilesystemBoundaryV1::PreparationCreate && !self.fired {
                self.fired = true;
                self.cas.remove_active_preparation_inode_for_test_v1();
                Some(FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::NoSpace))
            } else {
                None
            }
        }
    }

    pub fn preparation_open_accounting_fault_v1(root: &Path) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = PreparationOpenAccountingControl {
            cas: cas.clone(),
            fired: false,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x907,
            1,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: 1,
            },
            &mut control,
            &mut counters,
        );
        observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            0,
            false,
            CreateFaultControlObservation {
                control_fired: control.fired,
                cleanup_calls: 0,
                carrier_installed: false,
                poisoned: false,
            },
            &counters,
        )
    }

    struct PreparationFreeTerminalControl {
        fail_invalidation: bool,
        invalidation_attempts: u32,
    }

    impl CdcControlV1 for PreparationFreeTerminalControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for PreparationFreeTerminalControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::RootInvalidation {
                self.invalidation_attempts += 1;
                return self.fail_invalidation;
            }
            false
        }
    }

    struct PanicAfterOperationTerminalReleaseControl {
        unwind_pending: bool,
        terminal_hook_calls: u32,
    }

    impl CdcControlV1 for PanicAfterOperationTerminalReleaseControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for PanicAfterOperationTerminalReleaseControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_operation_terminal_unwind_after_release(&mut self) -> bool {
            self.terminal_hook_calls += 1;
            core::mem::take(&mut self.unwind_pending)
        }
    }

    #[derive(Default)]
    struct GlobalSeenCounterOverflowControl {
        injected: bool,
    }

    impl CdcControlV1 for GlobalSeenCounterOverflowControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for GlobalSeenCounterOverflowControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_global_seen_counter_accumulation_overflow(&mut self) -> bool {
            if self.injected {
                false
            } else {
                self.injected = true;
                true
            }
        }
    }

    struct FailBodyCleanupTerminalControl {
        preparation_cleanup_injected: bool,
        fail_invalidation: bool,
        invalidation_attempts: u32,
    }

    impl CdcControlV1 for FailBodyCleanupTerminalControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for FailBodyCleanupTerminalControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::PreparationSpool
                && !self.preparation_cleanup_injected
            {
                self.preparation_cleanup_injected = true;
                return true;
            }
            if target == FsCasCleanupTargetV1::RootInvalidation {
                self.invalidation_attempts += 1;
                return self.fail_invalidation;
            }
            false
        }
    }

    struct CancelBeforeCandidateValidationAndFailPrivatePackCleanup {
        cancelled: bool,
        cleanup_calls: u32,
        invalidation_attempts: u32,
    }

    impl CdcControlV1 for CancelBeforeCandidateValidationAndFailPrivatePackCleanup {
        fn cancellation_requested(&mut self) -> bool {
            self.cancelled
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for CancelBeforeCandidateValidationAndFailPrivatePackCleanup {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary == FsCasBoundaryV1::BeforeCandidateValidation {
                self.cancelled = true;
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            self.cancelled
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            match target {
                FsCasCleanupTargetV1::PrivatePack if self.cleanup_calls == 0 => {
                    self.cleanup_calls = 1;
                    true
                }
                FsCasCleanupTargetV1::RootInvalidation => {
                    self.invalidation_attempts += 1;
                    false
                }
                _ => false,
            }
        }
    }

    struct CancelAfterCarrierInstallAndFailCleanup {
        cancelled: bool,
        carrier_installed: bool,
        cleanup_calls: u32,
        invalidation_attempts: u32,
    }

    impl CdcControlV1 for CancelAfterCarrierInstallAndFailCleanup {
        fn cancellation_requested(&mut self) -> bool {
            self.cancelled
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for CancelAfterCarrierInstallAndFailCleanup {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary == FsCasBoundaryV1::AfterCarrierInstall {
                self.carrier_installed = true;
                self.cancelled = true;
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            self.cancelled
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            match target {
                FsCasCleanupTargetV1::Carrier if self.cleanup_calls == 0 => {
                    self.cleanup_calls = 1;
                    true
                }
                FsCasCleanupTargetV1::RootInvalidation => {
                    self.invalidation_attempts += 1;
                    false
                }
                _ => false,
            }
        }
    }

    struct PoisonStorageAndCancelAfterCarrierInstall {
        cas: FsCasV1,
        cancelled: bool,
        carrier_installed: bool,
        poisoned: bool,
        invalidation_attempts: u32,
        fail_invalidation: bool,
    }

    impl CdcControlV1 for PoisonStorageAndCancelAfterCarrierInstall {
        fn cancellation_requested(&mut self) -> bool {
            self.cancelled
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for PoisonStorageAndCancelAfterCarrierInstall {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary == FsCasBoundaryV1::AfterCarrierInstall {
                self.carrier_installed = true;
                self.cancelled = true;
                self.cas.poison_storage_admission_for_test_v1();
                self.poisoned = true;
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            self.cancelled
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::RootInvalidation {
                self.invalidation_attempts += 1;
                return self.fail_invalidation;
            }
            false
        }
    }

    struct FailCarrierAliasPreparationAccountingControl {
        cas: FsCasV1,
        armed: bool,
        invalidation_attempts: u32,
        fail_invalidation: bool,
    }

    impl CdcControlV1 for FailCarrierAliasPreparationAccountingControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for FailCarrierAliasPreparationAccountingControl {
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
            if boundary == FsCasFilesystemBoundaryV1::CarrierAliasUnlink && !self.armed {
                self.armed = true;
                self.cas.fail_next_preparation_remove_for_test_v1();
            }
            None
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::RootInvalidation {
                self.invalidation_attempts += 1;
                return self.fail_invalidation;
            }
            false
        }
    }

    fn run_create_fault_attempt<C, S>(
        cas: &FsCasV1,
        cancellation_key: u64,
        declared_len: u64,
        supplier: S,
        control: &mut C,
        counters: &mut OperationCountersV1,
    ) -> CreateFaultAttempt
    where
        C: LifecycleControlV1 + ?Sized,
        S: SourceSupplierV1,
    {
        let mut scratch = OperationScratch::new();
        let terminal = catch_unwind(AssertUnwindSafe(|| {
            let grant = request_create_operation_v1(cas, cancellation_key, counters, control)
                .map_err(OperationErrorV1::FsCas)?;
            run_create_v1(
                grant,
                CdcAlgorithmV1::FastCdc,
                b"payload.bin",
                0o644,
                declared_len,
                supplier,
                scratch.borrow(),
                control,
                counters,
            )
        }));
        match terminal {
            Ok(Ok(_)) => CreateFaultAttempt {
                error: None,
                panicked: false,
                panic_payload: None,
            },
            Ok(Err(error)) => CreateFaultAttempt {
                error: Some(match error {
                    OperationErrorV1::Core(error) => FsCasErrorV1::Core(error),
                    OperationErrorV1::FsCas(error) => error,
                }),
                panicked: false,
                panic_payload: None,
            },
            Err(payload) => CreateFaultAttempt {
                error: None,
                panicked: true,
                panic_payload: payload.downcast_ref::<&'static str>().copied(),
            },
        }
    }

    fn run_tree_fault_attempt<C, S>(
        cas: &FsCasV1,
        cancellation_key: u64,
        files: &mut [TreeFileV1<'_, S>],
        control: &mut C,
        counters: &mut OperationCountersV1,
    ) -> CreateFaultAttempt
    where
        C: LifecycleControlV1 + ?Sized,
        S: SourceSupplierV1,
    {
        let operation = match request_tree_operation_v1(cas, cancellation_key, counters, control) {
            Ok(operation) => operation,
            Err(error) => {
                return CreateFaultAttempt {
                    error: Some(error),
                    panicked: false,
                    panic_payload: None,
                };
            }
        };
        let mut scratch = OperationScratch::new();
        let terminal = catch_unwind(AssertUnwindSafe(|| {
            run_create_tree_v1(
                operation,
                CdcAlgorithmV1::FastCdc,
                files,
                scratch.borrow(),
                control,
                counters,
            )
        }));
        match terminal {
            Ok(Ok(_)) => CreateFaultAttempt {
                error: None,
                panicked: false,
                panic_payload: None,
            },
            Ok(Err(error)) => CreateFaultAttempt {
                error: Some(match error {
                    OperationErrorV1::Core(error) => FsCasErrorV1::Core(error),
                    OperationErrorV1::FsCas(error) => error,
                }),
                panicked: false,
                panic_payload: None,
            },
            Err(payload) => CreateFaultAttempt {
                error: None,
                panicked: true,
                panic_payload: payload.downcast_ref::<&'static str>().copied(),
            },
        }
    }

    fn observe_create_fault(
        root: &Path,
        cas: &FsCasV1,
        stale: &FsCasV1,
        attempt: CreateFaultAttempt,
        bound_invoked: bool,
        supply_invoked: bool,
        followup_succeeded: bool,
        terminal_hook_calls: u32,
        invalidation_attempts: u32,
        global_seen_injected: bool,
        counters: &OperationCountersV1,
    ) -> CreateFaultObservationV1 {
        observe_create_fault_with_control(
            root,
            cas,
            stale,
            attempt,
            bound_invoked,
            supply_invoked,
            followup_succeeded,
            terminal_hook_calls,
            invalidation_attempts,
            global_seen_injected,
            CreateFaultControlObservation::default(),
            counters,
        )
    }

    fn filesystem_failure_v1(error: Option<FsCasErrorV1>) -> Option<FilesystemFaultFailureV1> {
        let error = error?;
        let (first, _) = error.failure_causes_v1();
        match first {
            FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::NoSpace) => {
                Some(FilesystemFaultFailureV1::NoSpace)
            }
            FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::Quota) => {
                Some(FilesystemFaultFailureV1::Quota)
            }
            FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::InodeExhaustion) => {
                Some(FilesystemFaultFailureV1::InodeExhaustion)
            }
            FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::ReadFailure) => {
                Some(FilesystemFaultFailureV1::ReadFailure)
            }
            FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure) => {
                Some(FilesystemFaultFailureV1::WriteFailure)
            }
            FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::ShortRead) => {
                Some(FilesystemFaultFailureV1::ShortRead)
            }
            FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::ShortWrite) => {
                Some(FilesystemFaultFailureV1::ShortWrite)
            }
            FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied) => {
                Some(FilesystemFaultFailureV1::PermissionDenied)
            }
            _ => None,
        }
    }

    fn observe_create_fault_with_control(
        root: &Path,
        cas: &FsCasV1,
        stale: &FsCasV1,
        attempt: CreateFaultAttempt,
        bound_invoked: bool,
        supply_invoked: bool,
        followup_succeeded: bool,
        terminal_hook_calls: u32,
        invalidation_attempts: u32,
        global_seen_injected: bool,
        control: CreateFaultControlObservation,
        counters: &OperationCountersV1,
    ) -> CreateFaultObservationV1 {
        let (preparation_bytes, preparation_entries) = directory_usage(&root.join("preparation"));
        let (immutable_bytes, immutable_entries) = immutable_usage(root);
        let (carrier_bytes, carrier_entries) = directory_usage(&root.join("carriers"));
        let (locator_bytes, locator_entries) = directory_usage(&root.join("objects"));
        let (catalog_bytes, catalog_entries) = directory_usage(&root.join("catalog"));
        let (closure_bytes, closure_entries) = directory_usage(&root.join("closures"));
        let (first_cause, dominant_cause) = attempt
            .error
            .map(publication_causes_v1)
            .map(|(first, dominant)| (Some(first), Some(dominant)))
            .unwrap_or((None, None));
        let (storage_active_operations, storage_active_bytes, storage_active_inodes) =
            cas.storage_admission_active_for_test_v1();
        CreateFaultObservationV1 {
            error: attempt.error.map(publication_error_v1),
            operation_error: attempt.error.map(publication_error_v1),
            terminal_error: None,
            marker_fault_boundaries: (false, false, false, false, false),
            marker_cleanup_observation: (None, None, false, false, false, false),
            setup_storage: (0, 0, 0, 0, 0, 0, 0, 0, 0, 0),
            incumbent_marker_bytes: (None, None),
            filesystem_failure: filesystem_failure_v1(attempt.error),
            first_cause,
            dominant_cause,
            panicked: attempt.panicked,
            panic_payload: attempt.panic_payload,
            control_fired: control.control_fired,
            alias_injected: false,
            cleanup_calls: control.cleanup_calls,
            carrier_installed: control.carrier_installed,
            poisoned: control.poisoned,
            bound_invoked,
            supply_invoked,
            followup_succeeded,
            terminal_hook_calls,
            invalidation_attempts,
            global_seen_injected,
            preparation_bytes,
            preparation_entries,
            preparation_residue: preparation_residue_v1(root),
            immutable_bytes,
            immutable_entries,
            immutable_residue_bytes: counters.immutable_residue_bytes,
            immutable_residue_inodes: counters.immutable_residue_inodes,
            carrier_bytes,
            carrier_entries,
            locator_bytes,
            locator_entries,
            catalog_bytes,
            catalog_entries,
            closure_bytes,
            closure_entries,
            residue_bytes: counters.unreachable_installed_residue_bytes,
            mutable_preparation_residue_bytes: counters.mutable_preparation_residue_bytes,
            mutable_preparation_residue_inodes: counters.mutable_preparation_residue_inodes,
            source_read_calls: counters.source_read_calls,
            catalog_operations: counters.fscas_catalog_operations,
            source_bytes_read: counters.source_bytes_read,
            global_seen_lookups: counters.global_seen_lookups,
            global_seen_probes: counters.global_seen_probes,
            global_seen_maximum_probe: counters.global_seen_maximum_probe,
            global_seen_entries: counters.global_seen_entries,
            global_seen_table_bytes: counters.global_seen_table_bytes,
            global_seen_metadata_bytes_read: counters.global_seen_metadata_bytes_read,
            global_seen_metadata_read_calls: counters.global_seen_metadata_read_calls,
            global_seen_metadata_bytes_written: counters.global_seen_metadata_bytes_written,
            storage_bytes_requested: counters.storage_bytes_requested,
            storage_bytes_reserved: counters.storage_bytes_reserved,
            storage_bytes_released: counters.storage_bytes_released,
            storage_bytes_committed: counters.storage_bytes_committed,
            storage_bytes_retained: counters.storage_bytes_retained,
            storage_inodes_requested: counters.storage_inodes_requested,
            storage_inodes_reserved: counters.storage_inodes_reserved,
            storage_inodes_released: counters.storage_inodes_released,
            storage_inodes_committed: counters.storage_inodes_committed,
            storage_inodes_retained: counters.storage_inodes_retained,
            operation_slots: cas.operation_admitted_slots_v1(),
            operation_active: cas.operation_admission_active_for_test_v1(),
            operation_queue: cas.operation_admission_queue_for_test_v1(),
            root_admission_queue: (
                counters.root_admission_queue_entries,
                counters.root_admission_queue_refusals,
                counters.root_admission_release_failures,
            ),
            storage_active_operations,
            storage_active_bytes,
            storage_active_inodes,
            unwind_authority: (0, 0, (0, 0, 0), (0, 0, 0), false),
            followup_bound_invoked: false,
            followup_supply_invoked: false,
            followup_preparation_entries: 0,
            followup_storage: (0, 0, 0, 0, 0, 0, 0, 0, 0, 0),
            followup_zero_forbidden_work: false,
            usable_handles: (
                cas.occupied().is_ok(),
                stale.occupied().is_ok(),
                FsCasV1::open_existing(root)
                    .and_then(|reopened| reopened.occupied())
                    .is_ok(),
            ),
            invalidated: matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            stale_invalidated: matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            reopen_invalidated: matches!(
                FsCasV1::open_existing(root),
                Err(FsCasErrorV1::Invalidated)
            ),
            reopen_rejected: matches!(
                FsCasV1::open_existing(root),
                Err(FsCasErrorV1::Busy | FsCasErrorV1::Invalidated)
            ),
            persistent_invalidation: root.join("invalidated").is_dir(),
            visibility_lock_available: cas.visibility_lock_available_for_test_v1(),
            publication_lock_available: cas.publication_lock_available_for_test_v1(),
            zero_forbidden_work: counters.has_zero_forbidden_work(),
        }
    }

    fn preparation_residue_v1(root: &Path) -> PreparationResidueV1 {
        let mut entries = fs::read_dir(root.join("preparation"))
            .expect("preparation namespace")
            .map(|entry| entry.expect("preparation entry"));
        let Some(entry) = entries.next() else {
            return PreparationResidueV1::None;
        };
        if entries.next().is_some() {
            return PreparationResidueV1::Other;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("built-directories-") {
            PreparationResidueV1::BuiltDirectories
        } else if name.starts_with("built-files-") {
            PreparationResidueV1::BuiltFiles
        } else if name.starts_with("global-seen-") {
            PreparationResidueV1::GlobalSeen
        } else if name.starts_with("closure-objects-") {
            PreparationResidueV1::ClosureObjects
        } else if name.starts_with("pack-index-") {
            PreparationResidueV1::PackIndex
        } else if name.starts_with("chunk-references-") {
            PreparationResidueV1::ChunkReferences
        } else if name.starts_with("locator-receipts-") {
            PreparationResidueV1::LocatorReceipts
        } else if name.starts_with("pack-") {
            PreparationResidueV1::PrivatePack
        } else {
            PreparationResidueV1::Other
        }
    }

    fn new_fault_root(root: &Path) -> (FsCasV1, FsCasV1) {
        let cas = FsCasV1::create_new(root).expect("create lifecycle fault root");
        let stale = FsCasV1::open_existing(root).expect("open lifecycle fault stale owner");
        (cas, stale)
    }

    pub fn exact_complete_operation_boundary_v1(root: &Path) -> OperationBoundaryObservationV1 {
        let (cas, _stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut counters = OperationCountersV1::default();
        let mut control = ExactOperationBoundaryControl {
            root,
            starts: 0,
            ends: 0,
            preparation_empty_at_start: true,
            preparation_empty_at_end: true,
        };
        let attempt = run_create_fault_attempt(
            &cas,
            100,
            96 * 1024 + 31,
            CallbackSupplier {
                bound_invoked,
                supply_invoked,
                len: 96 * 1024 + 31,
            },
            &mut control,
            &mut counters,
        );
        OperationBoundaryObservationV1 {
            completed: !attempt.panicked && attempt.error.is_none(),
            starts: control.starts,
            ends: control.ends,
            preparation_empty_at_start: control.preparation_empty_at_start,
            preparation_empty_at_end: control.preparation_empty_at_end,
        }
    }

    impl PreparationConstructionBoundaryV1 {
        const fn fault(self) -> (FsCasFilesystemBoundaryV1, u32) {
            match self {
                Self::CreateBuiltDirectories => (FsCasFilesystemBoundaryV1::PreparationCreate, 1),
                Self::CreateBuiltFiles => (FsCasFilesystemBoundaryV1::PreparationCreate, 2),
                Self::CreateGlobalSeen => (FsCasFilesystemBoundaryV1::PreparationCreate, 3),
                Self::CreateClosureObjects => (FsCasFilesystemBoundaryV1::PreparationCreate, 4),
                Self::CreatePackIndex => (FsCasFilesystemBoundaryV1::PreparationCreate, 5),
                Self::CreateChunkReferences => (FsCasFilesystemBoundaryV1::PreparationCreate, 6),
                Self::InitializeGlobalSeen => (FsCasFilesystemBoundaryV1::PreparationResize, 1),
                Self::SetPermissions => (FsCasFilesystemBoundaryV1::PermissionChange, 1),
            }
        }
    }

    impl PreparationCleanupBoundaryV1 {
        const fn ordinal(self) -> u32 {
            match self {
                Self::BuiltDirectories => 1,
                Self::BuiltFiles => 2,
                Self::GlobalSeen => 3,
                Self::ClosureObjects => 4,
                Self::PackIndex => 5,
                Self::ChunkReferences => 6,
                Self::LocatorReceipts => 7,
            }
        }
    }

    struct PanicPreparationBoundary {
        boundary: FsCasFilesystemBoundaryV1,
        target_call: u32,
        observed_calls: u32,
        injected: bool,
    }

    impl CdcControlV1 for PanicPreparationBoundary {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for PanicPreparationBoundary {
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
            if boundary != self.boundary {
                return None;
            }
            self.observed_calls += 1;
            if !self.injected && self.observed_calls == self.target_call {
                self.injected = true;
                panic!("injected preparation boundary unwind");
            }
            None
        }
    }

    pub fn preparation_construction_boundary_unwind_v1(
        root: &Path,
        boundary: PreparationConstructionBoundaryV1,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let (filesystem_boundary, target_call) = boundary.fault();
        let mut control = PanicPreparationBoundary {
            boundary: filesystem_boundary,
            target_call,
            observed_calls: 0,
            injected: false,
        };
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut files = [TreeFileV1::new(
            b"a.bin",
            0o644,
            1,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: 1,
            },
        )];
        let mut counters = OperationCountersV1::default();
        let attempt = run_tree_fault_attempt(
            &cas,
            0x400 + u64::from(target_call),
            &mut files,
            &mut control,
            &mut counters,
        );

        let followup_bound = Arc::new(AtomicBool::new(false));
        let followup_supply = Arc::new(AtomicBool::new(false));
        let mut followup_control = ContinueFaultControl;
        let mut followup_counters = OperationCountersV1::default();
        let followup = run_create_fault_attempt(
            &cas,
            0x500 + u64::from(target_call),
            1,
            CallbackSupplier {
                bound_invoked: Arc::clone(&followup_bound),
                supply_invoked: Arc::clone(&followup_supply),
                len: 1,
            },
            &mut followup_control,
            &mut followup_counters,
        );
        let followup_succeeded = !followup.panicked
            && followup.error.is_none()
            && followup_bound.load(Ordering::Acquire)
            && followup_supply.load(Ordering::Acquire)
            && followup_counters.storage_bytes_requested
                == followup_counters.storage_bytes_reserved
            && followup_counters.storage_bytes_reserved
                == followup_counters.storage_bytes_released
                    + followup_counters.storage_bytes_committed
                    + followup_counters.storage_bytes_retained
            && followup_counters.storage_inodes_requested
                == followup_counters.storage_inodes_reserved
            && followup_counters.storage_inodes_reserved
                == followup_counters.storage_inodes_released
                    + followup_counters.storage_inodes_committed
                    + followup_counters.storage_inodes_retained
            && followup_counters.has_zero_forbidden_work();

        observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            followup_bound.load(Ordering::Acquire),
            followup_supply.load(Ordering::Acquire),
            followup_succeeded,
            0,
            0,
            false,
            CreateFaultControlObservation {
                control_fired: control.injected,
                cleanup_calls: 0,
                carrier_installed: false,
                poisoned: false,
            },
            &counters,
        )
    }

    struct PreparationCleanupControl {
        target_call: u32,
        observed_calls: u32,
        injected: bool,
        unwind: bool,
        fail_invalidation: bool,
        invalidation_attempts: u32,
    }

    impl CdcControlV1 for PreparationCleanupControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for PreparationCleanupControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::RootInvalidation && self.fail_invalidation {
                self.invalidation_attempts += 1;
                return true;
            }
            if target != FsCasCleanupTargetV1::PreparationSpool {
                return false;
            }
            self.observed_calls += 1;
            if !self.injected && self.observed_calls == self.target_call {
                self.injected = true;
                if self.unwind {
                    panic!("injected preparation cleanup unwind");
                }
                return true;
            }
            false
        }
    }

    pub fn preparation_cleanup_unwind_v1(
        root: &Path,
        boundary: PreparationCleanupBoundaryV1,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut files = [TreeFileV1::new(
            b"a.bin",
            0o644,
            1,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: 1,
            },
        )];
        let mut control = PreparationCleanupControl {
            target_call: boundary.ordinal(),
            observed_calls: 0,
            injected: false,
            unwind: true,
            fail_invalidation,
            invalidation_attempts: 0,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_tree_fault_attempt(
            &cas,
            0x500 + u64::from(boundary.ordinal()),
            &mut files,
            &mut control,
            &mut counters,
        );
        observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            control.invalidation_attempts,
            false,
            CreateFaultControlObservation {
                control_fired: control.injected,
                cleanup_calls: control.observed_calls,
                carrier_installed: false,
                poisoned: false,
            },
            &counters,
        )
    }

    pub fn preparation_cleanup_boundary_failure_v1(
        root: &Path,
        boundary: PreparationCleanupBoundaryV1,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut files = [
            TreeFileV1::new(
                b"a.bin",
                0o644,
                64 * 1024 + 17,
                CallbackSupplier {
                    bound_invoked: Arc::clone(&bound_invoked),
                    supply_invoked: Arc::clone(&supply_invoked),
                    len: 64 * 1024 + 17,
                },
            ),
            TreeFileV1::new(
                b"nested/b.bin",
                0o600,
                72 * 1024 + 29,
                CallbackSupplier {
                    bound_invoked: Arc::clone(&bound_invoked),
                    supply_invoked: Arc::clone(&supply_invoked),
                    len: 72 * 1024 + 29,
                },
            ),
        ];
        let mut control = PreparationCleanupControl {
            target_call: boundary.ordinal(),
            observed_calls: 0,
            injected: false,
            unwind: false,
            fail_invalidation: false,
            invalidation_attempts: 0,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_tree_fault_attempt(
            &cas,
            0x300 + u64::from(boundary.ordinal()),
            &mut files,
            &mut control,
            &mut counters,
        );
        observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            0,
            false,
            CreateFaultControlObservation {
                control_fired: control.injected,
                cleanup_calls: control.observed_calls,
                carrier_installed: false,
                poisoned: false,
            },
            &counters,
        )
    }

    #[derive(Default)]
    struct FailFirstPreparationCleanup {
        injected: bool,
        fail_invalidation: bool,
        invalidation_attempts: u32,
    }

    impl CdcControlV1 for FailFirstPreparationCleanup {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for FailFirstPreparationCleanup {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            match target {
                FsCasCleanupTargetV1::PreparationSpool if !self.injected => {
                    self.injected = true;
                    true
                }
                FsCasCleanupTargetV1::RootInvalidation if self.fail_invalidation => {
                    self.invalidation_attempts += 1;
                    true
                }
                _ => false,
            }
        }
    }

    pub fn preparation_cleanup_failure_lifecycle_v1(root: &Path) -> CreateFaultObservationV1 {
        preparation_cleanup_and_invalidation_failure_v1(root, false)
    }

    pub fn preparation_cleanup_and_invalidation_failure_v1(
        root: &Path,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = FailFirstPreparationCleanup {
            fail_invalidation,
            ..FailFirstPreparationCleanup::default()
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            109,
            64 * 1024 + 17,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: 64 * 1024 + 17,
            },
            &mut control,
            &mut counters,
        );
        observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            control.invalidation_attempts,
            false,
            CreateFaultControlObservation {
                control_fired: control.injected,
                cleanup_calls: u32::from(control.injected),
                carrier_installed: false,
                poisoned: false,
            },
            &counters,
        )
    }

    #[derive(Default)]
    struct PanicPrivatePackCleanupAfterWriteFailure {
        write_injected: bool,
        cleanup_panicked: bool,
        fail_invalidation: bool,
        invalidation_attempts: u32,
    }

    impl CdcControlV1 for PanicPrivatePackCleanupAfterWriteFailure {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for PanicPrivatePackCleanupAfterWriteFailure {
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
            if boundary == FsCasFilesystemBoundaryV1::PrivatePackWrite && !self.write_injected {
                self.write_injected = true;
                Some(FsCasErrorV1::Filesystem(
                    FsCasFilesystemFailureV1::ShortWrite,
                ))
            } else {
                None
            }
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::PrivatePack && !self.cleanup_panicked {
                self.cleanup_panicked = true;
                panic!("injected private-pack cleanup unwind");
            }
            if target == FsCasCleanupTargetV1::RootInvalidation && self.fail_invalidation {
                self.invalidation_attempts += 1;
                return true;
            }
            false
        }
    }

    pub fn private_pack_cleanup_unwind_v1(
        root: &Path,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = PanicPrivatePackCleanupAfterWriteFailure {
            fail_invalidation,
            ..PanicPrivatePackCleanupAfterWriteFailure::default()
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x600,
            1,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: 1,
            },
            &mut control,
            &mut counters,
        );
        observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            control.invalidation_attempts,
            false,
            CreateFaultControlObservation {
                control_fired: control.write_injected && control.cleanup_panicked,
                cleanup_calls: u32::from(control.cleanup_panicked),
                carrier_installed: false,
                poisoned: false,
            },
            &counters,
        )
    }

    fn final_handoff_fault_v1(
        root: &Path,
        fault: FinalHandoffFault,
        panic_during_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let declared_len = match (fault, panic_during_invalidation) {
            (FinalHandoffFault::AdmissionPoison, false) => 64 * 1024 + 37,
            (FinalHandoffFault::AdmissionPoison, true) => 64 * 1024 + 47,
            (FinalHandoffFault::StoragePoison, false) => 64 * 1024 + 41,
            (FinalHandoffFault::StoragePoison, true) => 64 * 1024 + 43,
            (FinalHandoffFault::Unwind, false) => 64 * 1024 + 19,
            (FinalHandoffFault::Unwind, true) => 64 * 1024 + 23,
        };
        let mut counters = OperationCountersV1::default();
        let mut control = FinalHandoffControl {
            cas: cas.clone(),
            fault,
            panic_during_invalidation,
            fired: false,
            invalidation_panicked: false,
        };
        let attempt = run_create_fault_attempt(
            &cas,
            0x740 + u64::from(panic_during_invalidation),
            declared_len,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: declared_len,
            },
            &mut control,
            &mut counters,
        );
        observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            u32::from(control.invalidation_panicked),
            false,
            CreateFaultControlObservation {
                control_fired: control.fired,
                carrier_installed: immutable_usage(root).1 > 0,
                poisoned: !matches!(fault, FinalHandoffFault::Unwind),
                ..CreateFaultControlObservation::default()
            },
            &counters,
        )
    }

    pub fn final_handoff_admission_poison_v1(
        root: &Path,
        panic_during_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        final_handoff_fault_v1(
            root,
            FinalHandoffFault::AdmissionPoison,
            panic_during_invalidation,
        )
    }

    pub fn final_handoff_storage_poison_v1(
        root: &Path,
        panic_during_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        final_handoff_fault_v1(
            root,
            FinalHandoffFault::StoragePoison,
            panic_during_invalidation,
        )
    }

    pub fn final_handoff_unwind_v1(
        root: &Path,
        panic_during_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        final_handoff_fault_v1(root, FinalHandoffFault::Unwind, panic_during_invalidation)
    }

    #[derive(Default)]
    struct MarkerWriteAndCleanupControl {
        marker_write_failed: bool,
        cleanup_failed: bool,
    }

    impl CdcControlV1 for MarkerWriteAndCleanupControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for MarkerWriteAndCleanupControl {
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
            if boundary == FsCasFilesystemBoundaryV1::MarkerWrite && !self.marker_write_failed {
                self.marker_write_failed = true;
                return Some(FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::NoSpace));
            }
            None
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::PreparationSpool && !self.cleanup_failed {
                self.cleanup_failed = true;
                return true;
            }
            false
        }
    }

    struct PreLinkTerminalCleanupControl {
        first_error: Option<FsCasErrorV1>,
        cleanup_calls: u32,
        invalidation_calls: u32,
        fail_invalidation: bool,
    }

    impl FsCasControlV1 for PreLinkTerminalCleanupControl {
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
            (boundary == FsCasFilesystemBoundaryV1::MarkerWrite)
                .then(|| self.first_error.take())
                .flatten()
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            match target {
                FsCasCleanupTargetV1::PreparationSpool => {
                    self.cleanup_calls += 1;
                    panic!("injected pre-link marker terminal cleanup unwind")
                }
                FsCasCleanupTargetV1::RootInvalidation => {
                    self.invalidation_calls += 1;
                    self.fail_invalidation
                }
                _ => false,
            }
        }
    }

    struct PreLinkCallbackCleanupControl {
        cleanup_unwinds: bool,
        preparation_panicked: bool,
        cleanup_calls: u32,
        invalidation_calls: u32,
        fail_invalidation: bool,
    }

    impl FsCasControlV1 for PreLinkCallbackCleanupControl {
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
            if boundary == FsCasFilesystemBoundaryV1::MarkerWrite && !self.preparation_panicked {
                self.preparation_panicked = true;
                panic!("injected pre-link marker preparation unwind")
            }
            None
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            match target {
                FsCasCleanupTargetV1::PreparationSpool => {
                    self.cleanup_calls += 1;
                    if self.cleanup_unwinds {
                        panic!("injected pre-link marker cleanup unwind")
                    }
                    true
                }
                FsCasCleanupTargetV1::RootInvalidation => {
                    self.invalidation_calls += 1;
                    self.fail_invalidation
                }
                _ => false,
            }
        }
    }

    struct BoundaryFailureControl {
        boundary: FsCasFilesystemBoundaryV1,
        error: FsCasErrorV1,
        fired: bool,
    }

    impl CdcControlV1 for BoundaryFailureControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for BoundaryFailureControl {
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
            if !self.fired && boundary == self.boundary {
                self.fired = true;
                return Some(self.error.clone());
            }
            None
        }
    }

    struct CarrierAliasInvalidationControl {
        alias_failed: bool,
        invalidation_write_failed: bool,
        invalidation_marker_failed: bool,
    }

    impl CdcControlV1 for CarrierAliasInvalidationControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for CarrierAliasInvalidationControl {
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
            match boundary {
                FsCasFilesystemBoundaryV1::CarrierAliasUnlink if !self.alias_failed => {
                    self.alias_failed = true;
                    Some(FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::NoSpace))
                }
                FsCasFilesystemBoundaryV1::InvalidationWrite if !self.invalidation_write_failed => {
                    self.invalidation_write_failed = true;
                    Some(FsCasErrorV1::Filesystem(
                        FsCasFilesystemFailureV1::WriteFailure,
                    ))
                }
                FsCasFilesystemBoundaryV1::InvalidationMarkerCreate
                    if !self.invalidation_marker_failed =>
                {
                    self.invalidation_marker_failed = true;
                    Some(FsCasErrorV1::Filesystem(
                        FsCasFilesystemFailureV1::InodeExhaustion,
                    ))
                }
                _ => None,
            }
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum PostLinkMarkerTargetV1 {
        ObjectLocator,
        Catalog,
        Closure,
    }

    impl PostLinkMarkerTargetV1 {
        fn boundary(self) -> FsCasBoundaryV1 {
            match self {
                Self::ObjectLocator => FsCasBoundaryV1::AfterObjectLocatorMarkerLink,
                Self::Catalog => FsCasBoundaryV1::AfterCatalogMarkerLink,
                Self::Closure => FsCasBoundaryV1::AfterClosureMarkerLink,
            }
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum PostLinkAliasCleanupV1 {
        Succeeds,
        Fails,
        Unwinds,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum CarrierCleanupAfterUnwindV1 {
        Succeeds,
        Fails,
        Unwinds,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum LocatorRollbackUnlinkFaultModeV1 {
        SampledUnsupported,
        SampledWriteFailure,
        PermissionDenied,
        WriteFailure,
        InjectedCleanup,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum RollbackCleanupTargetV1 {
        ObjectLocator,
        Carrier,
    }

    impl RollbackCleanupTargetV1 {
        const fn cleanup_target(self) -> FsCasCleanupTargetV1 {
            match self {
                Self::ObjectLocator => FsCasCleanupTargetV1::ObjectLocator,
                Self::Carrier => FsCasCleanupTargetV1::Carrier,
            }
        }

        const fn accounting_boundary(self) -> FsCasResidueAccountingBoundaryV1 {
            match self {
                Self::ObjectLocator => FsCasResidueAccountingBoundaryV1::ObjectLocator,
                Self::Carrier => FsCasResidueAccountingBoundaryV1::Carrier,
            }
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum ResidueAccountingBoundaryV1 {
        CatalogMarker,
        ObjectLocator,
        Carrier,
    }

    impl ResidueAccountingBoundaryV1 {
        const fn boundary(self) -> FsCasResidueAccountingBoundaryV1 {
            match self {
                Self::CatalogMarker => FsCasResidueAccountingBoundaryV1::CatalogMarker,
                Self::ObjectLocator => FsCasResidueAccountingBoundaryV1::ObjectLocator,
                Self::Carrier => FsCasResidueAccountingBoundaryV1::Carrier,
            }
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum PostCatalogControlFailureV1 {
        Cancelled,
        Deadline,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum AdmissionUnwindPrivateCleanupV1 {
        Clean,
        Fails,
        Unwinds,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum AdmissionPanicBoundaryV1 {
        PublicationLockAcquired,
        AfterCatalogPublication,
    }

    impl AdmissionPanicBoundaryV1 {
        const fn boundary(self) -> FsCasBoundaryV1 {
            match self {
                Self::PublicationLockAcquired => FsCasBoundaryV1::PublicationLockAcquired,
                Self::AfterCatalogPublication => FsCasBoundaryV1::AfterCatalogPublication,
            }
        }
    }

    struct PostLinkMarkerCleanupControl {
        target: FsCasBoundaryV1,
        boundary_unwind: bool,
        alias_cleanup: PostLinkAliasCleanupV1,
        fail_invalidation: bool,
        current: Option<FsCasBoundaryV1>,
        boundary_panicked: bool,
        alias_cleanup_calls: u32,
        invalidation_calls: u32,
    }

    impl CdcControlV1 for PostLinkMarkerCleanupControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for PostLinkMarkerCleanupControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            self.current = Some(boundary);
            if self.boundary_unwind && !self.boundary_panicked && boundary == self.target {
                self.boundary_panicked = true;
                panic!("injected post-link marker boundary unwind")
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::RootInvalidation {
                self.invalidation_calls += 1;
                return self.fail_invalidation;
            }
            if target != FsCasCleanupTargetV1::PublishedMarkerAlias
                || self.current != Some(self.target)
            {
                return false;
            }
            self.alias_cleanup_calls += 1;
            match self.alias_cleanup {
                PostLinkAliasCleanupV1::Succeeds => false,
                PostLinkAliasCleanupV1::Fails => true,
                PostLinkAliasCleanupV1::Unwinds => {
                    panic!("injected post-link alias cleanup unwind")
                }
            }
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum PreLinkMarkerPanicPointV1 {
        MarkerWrite,
        MarkerFlush,
        VisibilityRequest,
        MarkerHardLink,
    }

    struct PreLinkMarkerUnwindControl {
        target: PreLinkMarkerPanicPointV1,
        marker_started: bool,
        injected: bool,
        retain_marker: bool,
        cleanup_injected: bool,
    }

    impl CdcControlV1 for PreLinkMarkerUnwindControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for PreLinkMarkerUnwindControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if !self.injected
                && self.marker_started
                && self.target == PreLinkMarkerPanicPointV1::VisibilityRequest
                && boundary == FsCasBoundaryV1::VisibilityLockRequested
            {
                self.injected = true;
                panic!("injected pre-link marker visibility unwind")
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if self.retain_marker
                && !self.cleanup_injected
                && target == FsCasCleanupTargetV1::PreparationSpool
            {
                self.cleanup_injected = true;
                return true;
            }
            false
        }

        fn inject_filesystem_failure(
            &mut self,
            boundary: FsCasFilesystemBoundaryV1,
        ) -> Option<FsCasErrorV1> {
            if boundary == FsCasFilesystemBoundaryV1::MarkerWrite {
                self.marker_started = true;
            }
            let matches = matches!(
                (self.target, boundary),
                (
                    PreLinkMarkerPanicPointV1::MarkerWrite,
                    FsCasFilesystemBoundaryV1::MarkerWrite
                ) | (
                    PreLinkMarkerPanicPointV1::MarkerFlush,
                    FsCasFilesystemBoundaryV1::MarkerFlush
                ) | (
                    PreLinkMarkerPanicPointV1::MarkerHardLink,
                    FsCasFilesystemBoundaryV1::MarkerHardLink
                )
            );
            if !self.injected && matches {
                self.injected = true;
                panic!("injected pre-link marker filesystem unwind")
            }
            None
        }
    }

    #[derive(Default)]
    struct CarrierPreLinkUnwindControl {
        injected: bool,
    }

    impl CdcControlV1 for CarrierPreLinkUnwindControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for CarrierPreLinkUnwindControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if !self.injected && boundary == FsCasBoundaryV1::BeforeCarrierInstall {
                self.injected = true;
                panic!("injected carrier pre-link unwind")
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    struct CarrierPostLinkUnwindControl {
        carrier_cleanup: CarrierCleanupAfterUnwindV1,
        fail_invalidation: bool,
        overflow_carrier_counter_transfer: bool,
        boundary_panicked: bool,
        carrier_counter_overflow_injected: bool,
        carrier_cleanup_calls: u32,
        private_cleanup_calls: u32,
        invalidation_calls: u32,
    }

    impl CdcControlV1 for CarrierPostLinkUnwindControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for CarrierPostLinkUnwindControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if !self.boundary_panicked && boundary == FsCasBoundaryV1::AfterCarrierInstall {
                self.boundary_panicked = true;
                panic!("injected carrier post-link unwind")
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            match target {
                FsCasCleanupTargetV1::Carrier if self.carrier_cleanup_calls == 0 => {
                    self.carrier_cleanup_calls += 1;
                    match self.carrier_cleanup {
                        CarrierCleanupAfterUnwindV1::Succeeds => false,
                        CarrierCleanupAfterUnwindV1::Fails => true,
                        CarrierCleanupAfterUnwindV1::Unwinds => {
                            panic!("injected carrier cleanup unwind")
                        }
                    }
                }
                FsCasCleanupTargetV1::PrivatePack => {
                    self.private_cleanup_calls += 1;
                    false
                }
                FsCasCleanupTargetV1::RootInvalidation => {
                    self.invalidation_calls += 1;
                    self.fail_invalidation
                }
                _ => false,
            }
        }

        fn inject_carrier_counter_accumulation_overflow(&mut self) -> bool {
            if self.overflow_carrier_counter_transfer
                && self.boundary_panicked
                && !self.carrier_counter_overflow_injected
            {
                self.carrier_counter_overflow_injected = true;
                true
            } else {
                false
            }
        }
    }

    #[derive(Default)]
    struct LocatorResidueControl {
        cancel: bool,
        locator_retained: bool,
        carrier_cleanup_attempted: bool,
        invalidation_calls: u32,
    }

    impl CdcControlV1 for LocatorResidueControl {
        fn cancellation_requested(&mut self) -> bool {
            self.cancel
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for LocatorResidueControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary == FsCasBoundaryV1::BeforeCatalogPublication {
                self.cancel = true;
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            self.cancel
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            match target {
                FsCasCleanupTargetV1::ObjectLocator if !self.locator_retained => {
                    self.locator_retained = true;
                    true
                }
                FsCasCleanupTargetV1::Carrier => {
                    self.carrier_cleanup_attempted = true;
                    false
                }
                FsCasCleanupTargetV1::RootInvalidation => {
                    self.invalidation_calls += 1;
                    false
                }
                _ => false,
            }
        }
    }

    #[cfg(unix)]
    struct LocatorDirectionalControl {
        mode: LocatorRollbackUnlinkFaultModeV1,
        objects: PathBuf,
        held_objects: PathBuf,
        cancel: bool,
        armed: bool,
        fault_reached: bool,
        restored: bool,
        fail_invalidation: bool,
        carrier_cleanup_attempted: bool,
        invalidation_calls: u32,
    }

    #[cfg(unix)]
    impl LocatorDirectionalControl {
        fn restore_objects(&mut self) {
            if self.restored {
                return;
            }
            match self.mode {
                LocatorRollbackUnlinkFaultModeV1::PermissionDenied => {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&self.objects, fs::Permissions::from_mode(0o700))
                        .expect("restore locator directory permissions");
                }
                LocatorRollbackUnlinkFaultModeV1::WriteFailure => {
                    fs::remove_file(&self.objects).expect("remove locator fault file");
                    fs::rename(&self.held_objects, &self.objects)
                        .expect("restore locator directory");
                }
                LocatorRollbackUnlinkFaultModeV1::SampledUnsupported
                | LocatorRollbackUnlinkFaultModeV1::SampledWriteFailure
                | LocatorRollbackUnlinkFaultModeV1::InjectedCleanup => {}
            }
            self.restored = true;
        }
    }

    #[cfg(unix)]
    impl CdcControlV1 for LocatorDirectionalControl {
        fn cancellation_requested(&mut self) -> bool {
            self.cancel
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    #[cfg(unix)]
    impl FsCasControlV1 for LocatorDirectionalControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary == FsCasBoundaryV1::BeforeCatalogPublication {
                self.cancel = true;
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            self.cancel
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::ObjectLocator && !self.armed {
                self.armed = true;
                match self.mode {
                    LocatorRollbackUnlinkFaultModeV1::PermissionDenied => {
                        use std::os::unix::fs::PermissionsExt;
                        fs::set_permissions(&self.objects, fs::Permissions::from_mode(0o500))
                            .expect("restrict locator directory");
                    }
                    LocatorRollbackUnlinkFaultModeV1::WriteFailure => {
                        fs::rename(&self.objects, &self.held_objects)
                            .expect("hold locator directory");
                        fs::write(&self.objects, b"not-a-directory")
                            .expect("install locator fault file");
                    }
                    LocatorRollbackUnlinkFaultModeV1::InjectedCleanup => {
                        self.fault_reached = true;
                        return true;
                    }
                    LocatorRollbackUnlinkFaultModeV1::SampledUnsupported
                    | LocatorRollbackUnlinkFaultModeV1::SampledWriteFailure => {}
                }
            }
            if target == FsCasCleanupTargetV1::Carrier {
                self.carrier_cleanup_attempted = true;
            }
            if target == FsCasCleanupTargetV1::RootInvalidation {
                self.invalidation_calls += 1;
                self.restore_objects();
                return self.fail_invalidation;
            }
            false
        }

        fn inject_filesystem_failure(
            &mut self,
            boundary: FsCasFilesystemBoundaryV1,
        ) -> Option<FsCasErrorV1> {
            if boundary != FsCasFilesystemBoundaryV1::LocatorUnlink || self.fault_reached {
                return None;
            }
            match self.mode {
                LocatorRollbackUnlinkFaultModeV1::SampledUnsupported => {
                    self.fault_reached = true;
                    Some(FsCasErrorV1::Unsupported)
                }
                LocatorRollbackUnlinkFaultModeV1::SampledWriteFailure => {
                    self.fault_reached = true;
                    Some(FsCasErrorV1::Filesystem(
                        FsCasFilesystemFailureV1::WriteFailure,
                    ))
                }
                LocatorRollbackUnlinkFaultModeV1::PermissionDenied
                | LocatorRollbackUnlinkFaultModeV1::WriteFailure => {
                    self.fault_reached = true;
                    None
                }
                LocatorRollbackUnlinkFaultModeV1::InjectedCleanup => None,
            }
        }
    }

    struct PoisonLocatorAccountingControl {
        cas: FsCasV1,
        cancel: bool,
        armed: bool,
        fail_invalidation: bool,
        carrier_cleanup_attempted: bool,
        invalidation_calls: u32,
    }

    impl CdcControlV1 for PoisonLocatorAccountingControl {
        fn cancellation_requested(&mut self) -> bool {
            self.cancel
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for PoisonLocatorAccountingControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary == FsCasBoundaryV1::BeforeCatalogPublication && !self.armed {
                self.cas.poison_next_immutable_remove_for_test_v1();
                self.armed = true;
                self.cancel = true;
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            self.cancel
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::Carrier {
                self.carrier_cleanup_attempted = true;
            }
            if target == FsCasCleanupTargetV1::RootInvalidation {
                self.invalidation_calls += 1;
                return self.fail_invalidation;
            }
            false
        }
    }

    struct RollbackCleanupUnwindControl {
        cleanup_target: FsCasCleanupTargetV1,
        accounting_boundary: Option<FsCasResidueAccountingBoundaryV1>,
        fail_invalidation: bool,
        cancel: bool,
        locator_cleanup_calls: u32,
        carrier_cleanup_calls: u32,
        cleanup_panicked: bool,
        accounting_injected: bool,
        invalidation_calls: u32,
    }

    impl CdcControlV1 for RollbackCleanupUnwindControl {
        fn cancellation_requested(&mut self) -> bool {
            self.cancel
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for RollbackCleanupUnwindControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary == FsCasBoundaryV1::BeforeCatalogPublication {
                self.cancel = true;
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            self.cancel
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::ObjectLocator {
                self.locator_cleanup_calls += 1;
            }
            if target == FsCasCleanupTargetV1::Carrier {
                self.carrier_cleanup_calls += 1;
            }
            if target == FsCasCleanupTargetV1::RootInvalidation {
                self.invalidation_calls += 1;
                return self.fail_invalidation;
            }
            if target == self.cleanup_target && !self.cleanup_panicked {
                self.cleanup_panicked = true;
                panic!("injected rollback cleanup unwind at {target:?}")
            }
            false
        }

        fn inject_residue_accounting_failure(
            &mut self,
            boundary: FsCasResidueAccountingBoundaryV1,
        ) -> bool {
            if !self.accounting_injected && self.accounting_boundary == Some(boundary) {
                self.accounting_injected = true;
                true
            } else {
                false
            }
        }
    }

    struct VisibleCatalogControl {
        current: Option<FsCasBoundaryV1>,
        fail_alias: bool,
        first_error: Option<FsCasErrorV1>,
        accounting_boundary: Option<FsCasResidueAccountingBoundaryV1>,
        post_catalog_control_failure: Option<PostCatalogControlFailureV1>,
        fail_invalidation: bool,
        alias_injected: bool,
        accounting_injected: bool,
        root_invalidation_calls: u32,
    }

    impl VisibleCatalogControl {
        fn cancellation_active(&self) -> bool {
            self.current == Some(FsCasBoundaryV1::AfterCatalogPublication)
                && self.post_catalog_control_failure == Some(PostCatalogControlFailureV1::Cancelled)
        }

        fn deadline_active(&self) -> bool {
            self.current == Some(FsCasBoundaryV1::AfterCatalogPublication)
                && self.post_catalog_control_failure == Some(PostCatalogControlFailureV1::Deadline)
        }
    }

    impl CdcControlV1 for VisibleCatalogControl {
        fn cancellation_requested(&mut self) -> bool {
            self.cancellation_active()
        }

        fn deadline_exceeded(&mut self) -> bool {
            self.deadline_active()
        }
    }

    impl FsCasControlV1 for VisibleCatalogControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            self.current = Some(boundary);
        }

        fn cancellation_requested(&mut self) -> bool {
            self.cancellation_active()
        }

        fn deadline_exceeded(&mut self) -> bool {
            self.deadline_active()
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            match target {
                FsCasCleanupTargetV1::PublishedMarkerAlias
                    if !self.alias_injected
                        && self.fail_alias
                        && self.current == Some(FsCasBoundaryV1::AfterCatalogMarkerLink) =>
                {
                    self.alias_injected = true;
                    true
                }
                FsCasCleanupTargetV1::RootInvalidation => {
                    self.root_invalidation_calls += 1;
                    self.fail_invalidation
                }
                _ => false,
            }
        }

        fn inject_filesystem_failure(
            &mut self,
            boundary: FsCasFilesystemBoundaryV1,
        ) -> Option<FsCasErrorV1> {
            if !self.alias_injected
                && self.fail_alias
                && boundary == FsCasFilesystemBoundaryV1::MarkerAliasUnlink
                && self.current == Some(FsCasBoundaryV1::AfterCatalogMarkerLink)
            {
                if let Some(error) = self.first_error.clone() {
                    self.alias_injected = true;
                    return Some(error);
                }
            }
            None
        }

        fn inject_residue_accounting_failure(
            &mut self,
            boundary: FsCasResidueAccountingBoundaryV1,
        ) -> bool {
            if !self.accounting_injected && self.accounting_boundary == Some(boundary) {
                self.accounting_injected = true;
                true
            } else {
                false
            }
        }
    }

    struct AdmissionUnwindControl {
        panic_boundary: FsCasBoundaryV1,
        accounting_boundary: Option<FsCasResidueAccountingBoundaryV1>,
        private_cleanup: AdmissionUnwindPrivateCleanupV1,
        fail_invalidation: bool,
        boundary_panicked: bool,
        accounting_injected: bool,
        private_cleanup_calls: u32,
        root_invalidation_calls: u32,
    }

    impl CdcControlV1 for AdmissionUnwindControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for AdmissionUnwindControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if !self.boundary_panicked && boundary == self.panic_boundary {
                self.boundary_panicked = true;
                panic!("injected admission callback unwind")
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            match target {
                FsCasCleanupTargetV1::PrivatePack => {
                    self.private_cleanup_calls += 1;
                    if self.private_cleanup_calls != 1 {
                        return false;
                    }
                    match self.private_cleanup {
                        AdmissionUnwindPrivateCleanupV1::Clean => false,
                        AdmissionUnwindPrivateCleanupV1::Fails => true,
                        AdmissionUnwindPrivateCleanupV1::Unwinds => {
                            panic!("injected admission private-pack cleanup unwind")
                        }
                    }
                }
                FsCasCleanupTargetV1::RootInvalidation => {
                    self.root_invalidation_calls += 1;
                    self.fail_invalidation
                }
                _ => false,
            }
        }

        fn inject_residue_accounting_failure(
            &mut self,
            boundary: FsCasResidueAccountingBoundaryV1,
        ) -> bool {
            if !self.accounting_injected && self.accounting_boundary == Some(boundary) {
                self.accounting_injected = true;
                true
            } else {
                false
            }
        }
    }

    struct PublishedAliasFailureControl {
        target: FsCasBoundaryV1,
        current: Option<FsCasBoundaryV1>,
        first_error: Option<FsCasErrorV1>,
        fail_invalidation: bool,
        injected: bool,
    }

    impl CdcControlV1 for PublishedAliasFailureControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for PublishedAliasFailureControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            self.current = Some(boundary);
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::RootInvalidation && self.fail_invalidation {
                return true;
            }
            if self.first_error.is_none()
                && !self.injected
                && target == FsCasCleanupTargetV1::PublishedMarkerAlias
                && self.current == Some(self.target)
            {
                self.injected = true;
                return true;
            }
            false
        }

        fn inject_filesystem_failure(
            &mut self,
            boundary: FsCasFilesystemBoundaryV1,
        ) -> Option<FsCasErrorV1> {
            if !self.injected
                && boundary == FsCasFilesystemBoundaryV1::MarkerAliasUnlink
                && self.current == Some(self.target)
            {
                self.injected = true;
                return self.first_error.take();
            }
            None
        }
    }

    struct MalformedClosureControl {
        destination: PathBuf,
        malformed_installed: bool,
        preparation_cleanup_calls: u32,
        preparation_cleanup_injected: bool,
        root_invalidation_calls: u32,
        fail_invalidation: bool,
    }

    impl CdcControlV1 for MalformedClosureControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for MalformedClosureControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary == FsCasBoundaryV1::BeforeClosureMarkerPublication
                && !self.malformed_installed
            {
                fs::write(&self.destination, [0_u8; 120])
                    .expect("install deterministic malformed closure occupant");
                self.malformed_installed = true;
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            match target {
                FsCasCleanupTargetV1::PreparationSpool => {
                    self.preparation_cleanup_calls += 1;
                    if !self.preparation_cleanup_injected {
                        self.preparation_cleanup_injected = true;
                        true
                    } else {
                        false
                    }
                }
                FsCasCleanupTargetV1::RootInvalidation => {
                    self.root_invalidation_calls += 1;
                    self.fail_invalidation
                }
                _ => false,
            }
        }
    }

    pub fn marker_write_cleanup_terminal_v1(root: &Path) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = MarkerWriteAndCleanupControl::default();
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x8fe,
            8,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: 8,
            },
            &mut control,
            &mut counters,
        );
        observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            0,
            false,
            CreateFaultControlObservation {
                control_fired: control.marker_write_failed && control.cleanup_failed,
                cleanup_calls: u32::from(control.cleanup_failed),
                carrier_installed: false,
                poisoned: false,
            },
            &counters,
        )
    }

    pub fn pre_link_marker_terminal_cleanup_v1(
        root: &Path,
        equal_incumbent: bool,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let cas = FsCasV1::create_new(root).expect("pre-link marker terminal root");
        let mut setup_counters = OperationCountersV1::default();
        let mut setup_control = ContinueFaultControl;
        if equal_incumbent {
            let mut setup = cas
                .begin_operation_capability_v1(
                    FsOperationKindV1::CompleteC3File,
                    0x8ff,
                    &mut setup_counters,
                    &mut setup_control,
                )
                .expect("pre-link marker incumbent capability");
            setup
                .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(8, 8, 1, 1).unwrap())
                .expect("pre-link marker incumbent envelope");
            let token = setup
                .storage_token_v1()
                .expect("pre-link marker incumbent token");
            cas.publish_test_marker_borrowed_v1(token, &mut setup_control)
                .expect("pre-link marker incumbent publication");
            setup
                .finish_terminal_v1(true, &mut setup_counters, &mut setup_control)
                .expect("pre-link marker incumbent terminal");
        }

        let stale = FsCasV1::open_existing(root).expect("pre-link marker stale owner");
        let mut counters = OperationCountersV1::default();
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x900,
                &mut counters,
                &mut setup_control,
            )
            .expect("pre-link marker capability");
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(8, 8, 1, 1).unwrap())
            .expect("pre-link marker envelope");
        let token = capability
            .storage_token_v1()
            .expect("pre-link marker token");
        let mut control = PreLinkTerminalCleanupControl {
            first_error: (!equal_incumbent)
                .then_some(FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::NoSpace)),
            cleanup_calls: 0,
            invalidation_calls: 0,
            fail_invalidation,
        };
        let operation_error = cas
            .publish_test_marker_borrowed_v1(token, &mut control)
            .err();
        let terminal_error = capability
            .finish_terminal_v1(false, &mut counters, &mut setup_control)
            .err();
        assert_eq!(cas.operation_admission_active_for_test_v1(), 0);
        assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
        let (preparation_bytes, preparation_inodes) = directory_usage(&root.join("preparation"));
        assert_eq!(preparation_inodes, 1);
        assert_eq!(preparation_bytes, u64::from(equal_incumbent) * 8);
        let (immutable_bytes, immutable_inodes) = immutable_usage(root);
        assert_eq!(
            (immutable_bytes, immutable_inodes),
            if equal_incumbent { (8, 1) } else { (0, 0) }
        );
        assert_eq!(
            (&counters).storage_bytes_requested,
            (&counters).storage_bytes_reserved
        );
        assert_eq!(
            (&counters).storage_inodes_requested,
            (&counters).storage_inodes_reserved
        );
        assert_eq!(
            (&counters).storage_bytes_reserved,
            (&counters).storage_bytes_released
                + (&counters).storage_bytes_committed
                + (&counters).storage_bytes_retained
        );
        assert_eq!(
            (&counters).storage_inodes_reserved,
            (&counters).storage_inodes_released
                + (&counters).storage_inodes_committed
                + (&counters).storage_inodes_retained
        );
        let mut observation = observe_direct_fault(
            root,
            &cas,
            &stale,
            operation_error,
            terminal_error,
            CreateFaultControlObservation {
                control_fired: control.cleanup_calls > 0,
                cleanup_calls: control.cleanup_calls,
                carrier_installed: false,
                poisoned: false,
            },
            control.invalidation_calls,
            &counters,
        );
        observation.setup_storage = (
            setup_counters.storage_bytes_requested,
            setup_counters.storage_bytes_reserved,
            setup_counters.storage_bytes_released,
            setup_counters.storage_bytes_committed,
            setup_counters.storage_bytes_retained,
            setup_counters.storage_inodes_requested,
            setup_counters.storage_inodes_reserved,
            setup_counters.storage_inodes_released,
            setup_counters.storage_inodes_committed,
            setup_counters.storage_inodes_retained,
        );
        observation
    }

    pub fn pre_link_marker_callback_cleanup_v1(
        root: &Path,
        cleanup_unwinds: bool,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let mut counters = OperationCountersV1::default();
        let mut admission_control = ContinueFaultControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x901,
                &mut counters,
                &mut admission_control,
            )
            .expect("pre-link callback capability");
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(8, 8, 1, 1).unwrap())
            .expect("pre-link callback envelope");
        let token = capability
            .storage_token_v1()
            .expect("pre-link callback token");
        let mut control = PreLinkCallbackCleanupControl {
            cleanup_unwinds,
            preparation_panicked: false,
            cleanup_calls: 0,
            invalidation_calls: 0,
            fail_invalidation,
        };
        let operation_error = match catch_unwind(AssertUnwindSafe(|| {
            cas.publish_test_marker_borrowed_v1(token, &mut control)
        })) {
            Ok(result) => result.err(),
            Err(_) => panic!("pre-link callback unwind escaped cleanup reconciliation"),
        };
        let terminal_error = capability
            .finish_terminal_v1(false, &mut counters, &mut admission_control)
            .err();
        assert_eq!(
            (&counters).storage_inodes_reserved,
            (&counters).storage_inodes_released
                + (&counters).storage_inodes_committed
                + (&counters).storage_inodes_retained
        );
        observe_direct_fault(
            root,
            &cas,
            &stale,
            operation_error,
            terminal_error,
            CreateFaultControlObservation {
                control_fired: control.preparation_panicked,
                cleanup_calls: control.cleanup_calls,
                carrier_installed: false,
                poisoned: false,
            },
            control.invalidation_calls,
            &counters,
        )
    }

    pub fn carrier_alias_unlink_cleanup_v1(root: &Path) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = BoundaryFailureControl {
            boundary: FsCasFilesystemBoundaryV1::CarrierAliasUnlink,
            error: FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::NoSpace),
            fired: false,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x900,
            64 * 1024 + 17,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: 64 * 1024 + 17,
            },
            &mut control,
            &mut counters,
        );
        observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            0,
            false,
            CreateFaultControlObservation {
                control_fired: control.fired,
                cleanup_calls: 0,
                carrier_installed: false,
                poisoned: false,
            },
            &counters,
        )
    }

    pub fn published_locator_alias_unlink_v1(root: &Path) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = BoundaryFailureControl {
            boundary: FsCasFilesystemBoundaryV1::MarkerAliasUnlink,
            error: FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::Quota),
            fired: false,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x901,
            64 * 1024 + 17,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: 64 * 1024 + 17,
            },
            &mut control,
            &mut counters,
        );
        let observation = observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            0,
            false,
            CreateFaultControlObservation {
                control_fired: control.fired,
                cleanup_calls: 0,
                carrier_installed: false,
                poisoned: false,
            },
            &counters,
        );
        assert_eq!(
            (&counters).storage_bytes_reserved,
            (&counters).storage_bytes_released
                + (&counters).storage_bytes_committed
                + (&counters).storage_bytes_retained
        );
        assert_eq!(
            (&counters).storage_inodes_reserved,
            (&counters).storage_inodes_released
                + (&counters).storage_inodes_committed
                + (&counters).storage_inodes_retained
        );
        observation
    }

    pub fn alias_cleanup_invalidation_double_fault_v1(root: &Path) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = CarrierAliasInvalidationControl {
            alias_failed: false,
            invalidation_write_failed: false,
            invalidation_marker_failed: false,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x902,
            64 * 1024 + 17,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: 64 * 1024 + 17,
            },
            &mut control,
            &mut counters,
        );
        let observation = observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            0,
            false,
            CreateFaultControlObservation {
                control_fired: control.alias_failed
                    && control.invalidation_write_failed
                    && control.invalidation_marker_failed,
                cleanup_calls: 0,
                carrier_installed: false,
                poisoned: false,
            },
            &counters,
        );
        assert_eq!(fs::read(root.join("owner")).unwrap()[8], 1);
        observation
    }

    pub fn carrier_pre_link_unwind_v1(root: &Path) -> CreateFaultObservationV1 {
        const BODY_BYTES: u64 = 64 * 1024 + 17;
        let (cas, stale) = new_fault_root(root);
        let first_bound = Arc::new(AtomicBool::new(false));
        let first_supply = Arc::new(AtomicBool::new(false));
        let mut control = CarrierPreLinkUnwindControl::default();
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x71f,
            BODY_BYTES,
            CallbackSupplier {
                bound_invoked: Arc::clone(&first_bound),
                supply_invoked: Arc::clone(&first_supply),
                len: BODY_BYTES,
            },
            &mut control,
            &mut counters,
        );
        let mut observation = observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            first_bound.load(Ordering::Acquire),
            first_supply.load(Ordering::Acquire),
            false,
            0,
            0,
            false,
            CreateFaultControlObservation {
                control_fired: control.injected,
                cleanup_calls: 0,
                carrier_installed: false,
                poisoned: false,
            },
            &counters,
        );

        let followup_bound = Arc::new(AtomicBool::new(false));
        let followup_supply = Arc::new(AtomicBool::new(false));
        let mut followup_control = ContinueFaultControl;
        let mut followup_counters = OperationCountersV1::default();
        let followup = run_create_fault_attempt(
            &cas,
            0x720,
            1,
            CallbackSupplier {
                bound_invoked: Arc::clone(&followup_bound),
                supply_invoked: Arc::clone(&followup_supply),
                len: 1,
            },
            &mut followup_control,
            &mut followup_counters,
        );
        let followup_succeeded = !followup.panicked
            && followup.error.is_none()
            && followup_bound.load(Ordering::Acquire)
            && followup_supply.load(Ordering::Acquire)
            && followup_counters.storage_bytes_requested
                == followup_counters.storage_bytes_reserved
            && followup_counters.storage_bytes_reserved
                == followup_counters.storage_bytes_released
                    + followup_counters.storage_bytes_committed
                    + followup_counters.storage_bytes_retained
            && followup_counters.storage_inodes_requested
                == followup_counters.storage_inodes_reserved
            && followup_counters.storage_inodes_reserved
                == followup_counters.storage_inodes_released
                    + followup_counters.storage_inodes_committed
                    + followup_counters.storage_inodes_retained
            && followup_counters.has_zero_forbidden_work();
        observation.followup_succeeded = followup_succeeded;
        observation
    }

    pub fn carrier_post_link_unwind_v1(
        root: &Path,
        carrier_cleanup: CarrierCleanupAfterUnwindV1,
        fail_invalidation: bool,
        overflow_counter_transfer: bool,
    ) -> CreateFaultObservationV1 {
        const BODY_BYTES: u64 = 64 * 1024 + 17;
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = CarrierPostLinkUnwindControl {
            carrier_cleanup,
            fail_invalidation,
            overflow_carrier_counter_transfer: overflow_counter_transfer,
            boundary_panicked: false,
            carrier_counter_overflow_injected: false,
            carrier_cleanup_calls: 0,
            private_cleanup_calls: 0,
            invalidation_calls: 0,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x720,
            BODY_BYTES,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: BODY_BYTES,
            },
            &mut control,
            &mut counters,
        );
        observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            control.private_cleanup_calls,
            control.invalidation_calls,
            false,
            CreateFaultControlObservation {
                control_fired: control.boundary_panicked,
                cleanup_calls: control.carrier_cleanup_calls,
                carrier_installed: control.boundary_panicked,
                poisoned: control.carrier_counter_overflow_injected,
            },
            &counters,
        )
    }

    pub fn locator_cleanup_residue_v1(root: &Path) -> CreateFaultObservationV1 {
        const BODY_BYTES: u64 = 64 * 1024 + 17;
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = LocatorResidueControl::default();
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x728,
            BODY_BYTES,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: BODY_BYTES,
            },
            &mut control,
            &mut counters,
        );
        observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            u32::from(control.carrier_cleanup_attempted),
            control.invalidation_calls,
            false,
            CreateFaultControlObservation {
                control_fired: control.locator_retained,
                cleanup_calls: u32::from(control.locator_retained),
                carrier_installed: false,
                poisoned: false,
            },
            &counters,
        )
    }

    #[cfg(unix)]
    pub fn locator_rollback_directional_fault_v1(
        root: &Path,
        mode: LocatorRollbackUnlinkFaultModeV1,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        const BODY_BYTES: u64 = 64 * 1024 + 41;
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = LocatorDirectionalControl {
            mode,
            objects: root.join("objects"),
            held_objects: root.join("objects-held-for-fault"),
            cancel: false,
            armed: false,
            fault_reached: false,
            restored: false,
            fail_invalidation,
            carrier_cleanup_attempted: false,
            invalidation_calls: 0,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x72a,
            BODY_BYTES,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: BODY_BYTES,
            },
            &mut control,
            &mut counters,
        );
        observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            control.invalidation_calls,
            false,
            CreateFaultControlObservation {
                control_fired: control.armed && control.fault_reached && control.restored,
                cleanup_calls: u32::from(control.carrier_cleanup_attempted),
                carrier_installed: false,
                poisoned: false,
            },
            &counters,
        )
    }

    pub fn locator_rollback_accounting_poison_v1(
        root: &Path,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        const BODY_BYTES: u64 = 64 * 1024 + 53;
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = PoisonLocatorAccountingControl {
            cas: cas.clone(),
            cancel: false,
            armed: false,
            fail_invalidation,
            carrier_cleanup_attempted: false,
            invalidation_calls: 0,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x72b,
            BODY_BYTES,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: BODY_BYTES,
            },
            &mut control,
            &mut counters,
        );
        observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            control.invalidation_calls,
            false,
            CreateFaultControlObservation {
                control_fired: control.armed,
                cleanup_calls: u32::from(control.carrier_cleanup_attempted),
                carrier_installed: false,
                poisoned: control.armed,
            },
            &counters,
        )
    }

    pub fn locator_cleanup_unwind_v1(
        root: &Path,
        cleanup_target: RollbackCleanupTargetV1,
        inject_accounting: bool,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        const BODY_BYTES: u64 = 64 * 1024 + 29;
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = RollbackCleanupUnwindControl {
            cleanup_target: cleanup_target.cleanup_target(),
            accounting_boundary: inject_accounting.then(|| cleanup_target.accounting_boundary()),
            fail_invalidation,
            cancel: false,
            locator_cleanup_calls: 0,
            carrier_cleanup_calls: 0,
            cleanup_panicked: false,
            accounting_injected: false,
            invalidation_calls: 0,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x729,
            BODY_BYTES,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: BODY_BYTES,
            },
            &mut control,
            &mut counters,
        );
        observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            control.carrier_cleanup_calls,
            control.invalidation_calls,
            false,
            CreateFaultControlObservation {
                control_fired: control.cleanup_panicked,
                cleanup_calls: control.locator_cleanup_calls,
                carrier_installed: false,
                poisoned: control.accounting_injected,
            },
            &counters,
        )
    }

    pub fn visible_catalog_terminal_v1(
        root: &Path,
        accounting_boundary: ResidueAccountingBoundaryV1,
        directional_first_error: bool,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = VisibleCatalogControl {
            current: None,
            fail_alias: true,
            first_error: directional_first_error.then_some(FsCasErrorV1::Filesystem(
                FsCasFilesystemFailureV1::PermissionDenied,
            )),
            accounting_boundary: Some(accounting_boundary.boundary()),
            post_catalog_control_failure: None,
            fail_invalidation,
            alias_injected: false,
            accounting_injected: false,
            root_invalidation_calls: 0,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x92b,
            1,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: 1,
            },
            &mut control,
            &mut counters,
        );
        let alias_injected = control.alias_injected;
        observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            control.root_invalidation_calls,
            false,
            CreateFaultControlObservation {
                control_fired: control.alias_injected,
                cleanup_calls: u32::from(control.alias_injected),
                carrier_installed: false,
                poisoned: control.accounting_injected,
            },
            &counters,
        )
        .with_alias_injected(alias_injected)
    }

    pub fn post_catalog_control_terminal_v1(
        root: &Path,
        control_failure: PostCatalogControlFailureV1,
        accounting_boundary: Option<ResidueAccountingBoundaryV1>,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = VisibleCatalogControl {
            current: None,
            fail_alias: false,
            first_error: None,
            accounting_boundary: accounting_boundary.map(ResidueAccountingBoundaryV1::boundary),
            post_catalog_control_failure: Some(control_failure),
            fail_invalidation,
            alias_injected: false,
            accounting_injected: false,
            root_invalidation_calls: 0,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x92c,
            1,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: 1,
            },
            &mut control,
            &mut counters,
        );
        let alias_injected = control.alias_injected;
        observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            control.root_invalidation_calls,
            false,
            CreateFaultControlObservation {
                control_fired: true,
                cleanup_calls: 0,
                carrier_installed: false,
                poisoned: control.accounting_injected,
            },
            &counters,
        )
        .with_alias_injected(alias_injected)
    }

    pub fn admission_callback_unwind_v1(
        root: &Path,
        panic_boundary: AdmissionPanicBoundaryV1,
        accounting_boundary: Option<ResidueAccountingBoundaryV1>,
        private_cleanup: AdmissionUnwindPrivateCleanupV1,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        const BODY_BYTES: u64 = 64 * 1024 + 17;
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = AdmissionUnwindControl {
            panic_boundary: panic_boundary.boundary(),
            accounting_boundary: accounting_boundary.map(ResidueAccountingBoundaryV1::boundary),
            private_cleanup,
            fail_invalidation,
            boundary_panicked: false,
            accounting_injected: false,
            private_cleanup_calls: 0,
            root_invalidation_calls: 0,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x92d,
            BODY_BYTES,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: BODY_BYTES,
            },
            &mut control,
            &mut counters,
        );
        observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            control.private_cleanup_calls,
            control.root_invalidation_calls,
            false,
            CreateFaultControlObservation {
                control_fired: control.boundary_panicked,
                cleanup_calls: control.private_cleanup_calls,
                carrier_installed: false,
                poisoned: control.accounting_injected,
            },
            &counters,
        )
    }

    pub fn post_link_marker_unwind_v1(
        root: &Path,
        target: PostLinkMarkerTargetV1,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = PostLinkMarkerCleanupControl {
            target: target.boundary(),
            boundary_unwind: true,
            alias_cleanup: PostLinkAliasCleanupV1::Succeeds,
            fail_invalidation: false,
            current: None,
            boundary_panicked: false,
            alias_cleanup_calls: 0,
            invalidation_calls: 0,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x710 + target as u64,
            64 * 1024 + 17,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: 64 * 1024 + 17,
            },
            &mut control,
            &mut counters,
        );
        let observation = observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            control.invalidation_calls,
            false,
            CreateFaultControlObservation {
                control_fired: control.boundary_panicked,
                cleanup_calls: control.alias_cleanup_calls,
                carrier_installed: false,
                poisoned: false,
            },
            &counters,
        );
        assert_storage_equations(&counters);
        observation
    }

    pub fn post_link_marker_secondary_v1(
        root: &Path,
        target: PostLinkMarkerTargetV1,
        boundary_unwind: bool,
        alias_cleanup: PostLinkAliasCleanupV1,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = PostLinkMarkerCleanupControl {
            target: target.boundary(),
            boundary_unwind,
            alias_cleanup,
            fail_invalidation,
            current: None,
            boundary_panicked: false,
            alias_cleanup_calls: 0,
            invalidation_calls: 0,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x711,
            1,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: 1,
            },
            &mut control,
            &mut counters,
        );
        let observation = observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            control.invalidation_calls,
            false,
            CreateFaultControlObservation {
                control_fired: control.boundary_panicked,
                cleanup_calls: control.alias_cleanup_calls,
                carrier_installed: false,
                poisoned: false,
            },
            &counters,
        );
        assert_eq!(control.invalidation_calls, 1);
        assert_eq!(cas.operation_admitted_slots_v1(), 0);
        assert_eq!(cas.operation_admission_active_for_test_v1(), 0);
        assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
        assert_storage_equations(&counters);
        observation
    }

    pub fn pre_link_marker_unwind_v1(
        root: &Path,
        point: PreLinkMarkerPanicPointV1,
        retain_marker: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = PreLinkMarkerUnwindControl {
            target: point,
            marker_started: false,
            injected: false,
            retain_marker,
            cleanup_injected: false,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x718,
            64 * 1024 + 17,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: 64 * 1024 + 17,
            },
            &mut control,
            &mut counters,
        );
        let observation = observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            0,
            false,
            CreateFaultControlObservation {
                control_fired: control.injected,
                cleanup_calls: u32::from(control.cleanup_injected),
                carrier_installed: false,
                poisoned: false,
            },
            &counters,
        );
        assert_eq!(
            counters.unreachable_installed_residue_bytes,
            observation.carrier_bytes
        );
        observation
    }

    pub fn post_link_alias_directional_failure_v1(
        root: &Path,
        target: PostLinkMarkerTargetV1,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = PublishedAliasFailureControl {
            target: target.boundary(),
            current: None,
            first_error: Some(FsCasErrorV1::Filesystem(
                FsCasFilesystemFailureV1::PermissionDenied,
            )),
            fail_invalidation,
            injected: false,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x719 + target as u64,
            64 * 1024 + 17,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: 64 * 1024 + 17,
            },
            &mut control,
            &mut counters,
        );
        let observation = observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            0,
            false,
            CreateFaultControlObservation {
                control_fired: control.injected,
                cleanup_calls: 1,
                carrier_installed: false,
                poisoned: false,
            },
            &counters,
        );
        assert_storage_equations(&counters);
        observation
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct MalformedClosureObservationV1 {
        error: Option<PublicationErrorV1>,
        first_cause: Option<PublicationCauseV1>,
        dominant_cause: Option<PublicationCauseV1>,
        malformed_closure_installed: bool,
        malformed_closure_preserved: bool,
        closure_bytes: u64,
        carrier_entries_preserved: bool,
        catalog_entries_preserved: bool,
        object_entries_preserved: bool,
        closure_fences: u64,
        residue_bytes: u64,
        cleanup_calls: u32,
        invalidation_attempts: u32,
        preparation_bytes: u64,
        preparation_entries: u64,
        immutable_bytes: u64,
        immutable_entries: u64,
        storage_bytes_committed: u64,
        storage_inodes_committed: u64,
        storage_bytes_retained: u64,
        storage_inodes_retained: u64,
        operation_slots: u64,
        invalidated: bool,
        stale_invalidated: bool,
        reopen_invalidated: bool,
        zero_forbidden_work: bool,
    }

    impl MalformedClosureObservationV1 {
        pub const fn error(self) -> Option<PublicationErrorV1> {
            self.error
        }

        pub const fn first_cause(self) -> Option<PublicationCauseV1> {
            self.first_cause
        }

        pub const fn dominant_cause(self) -> Option<PublicationCauseV1> {
            self.dominant_cause
        }

        pub const fn malformed_closure_installed(self) -> bool {
            self.malformed_closure_installed
        }

        pub const fn malformed_closure_preserved(self) -> bool {
            self.malformed_closure_preserved
        }

        pub const fn closure_bytes(self) -> u64 {
            self.closure_bytes
        }

        pub const fn carrier_entries_preserved(self) -> bool {
            self.carrier_entries_preserved
        }

        pub const fn catalog_entries_preserved(self) -> bool {
            self.catalog_entries_preserved
        }

        pub const fn object_entries_preserved(self) -> bool {
            self.object_entries_preserved
        }

        pub const fn closure_fences(self) -> u64 {
            self.closure_fences
        }

        pub const fn residue_bytes(self) -> u64 {
            self.residue_bytes
        }

        pub const fn cleanup_calls(self) -> u32 {
            self.cleanup_calls
        }

        pub const fn invalidation_attempts(self) -> u32 {
            self.invalidation_attempts
        }

        pub const fn preparation_bytes(self) -> u64 {
            self.preparation_bytes
        }

        pub const fn preparation_entries(self) -> u64 {
            self.preparation_entries
        }

        pub const fn immutable_bytes(self) -> u64 {
            self.immutable_bytes
        }

        pub const fn immutable_entries(self) -> u64 {
            self.immutable_entries
        }

        pub const fn storage_bytes_committed(self) -> u64 {
            self.storage_bytes_committed
        }

        pub const fn storage_inodes_committed(self) -> u64 {
            self.storage_inodes_committed
        }

        pub const fn storage_bytes_retained(self) -> u64 {
            self.storage_bytes_retained
        }

        pub const fn storage_inodes_retained(self) -> u64 {
            self.storage_inodes_retained
        }

        pub const fn operation_slots(self) -> u64 {
            self.operation_slots
        }

        pub const fn invalidated(self) -> bool {
            self.invalidated
        }

        pub const fn stale_invalidated(self) -> bool {
            self.stale_invalidated
        }

        pub const fn reopen_invalidated(self) -> bool {
            self.reopen_invalidated
        }

        pub const fn zero_forbidden_work(self) -> bool {
            self.zero_forbidden_work
        }
    }

    fn malformed_closure_attempt(
        root: &Path,
        fail_cleanup: bool,
        fail_invalidation: bool,
    ) -> MalformedClosureObservationV1 {
        const BODY_BYTES: u64 = 64 * 1024 + 29;
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut first_counters = OperationCountersV1::default();
        let mut first_control = ContinueFaultControl;
        let first = run_create_fault_attempt(
            &cas,
            0x410,
            BODY_BYTES,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: BODY_BYTES,
            },
            &mut first_control,
            &mut first_counters,
        );
        assert!(!first.panicked, "malformed closure seed create unwound");
        assert!(
            first.error.is_none(),
            "malformed closure seed create failed"
        );
        let mut closures = fs::read_dir(root.join("closures")).expect("read seeded closures");
        let closure = closures
            .next()
            .expect("seeded closure")
            .expect("seeded closure entry")
            .path();
        assert!(
            closures.next().is_none(),
            "seeded create produced extra closures"
        );
        let carrier_entries = directory_usage(&root.join("carriers")).1;
        let catalog_entries = directory_usage(&root.join("catalog")).1;
        let object_entries = directory_usage(&root.join("objects")).1;
        fs::remove_file(&closure).expect("remove seeded closure for race");
        let (before_preparation_bytes, before_preparation_inodes) =
            directory_usage(&root.join("preparation"));
        let (before_immutable_bytes, before_immutable_inodes) = immutable_usage(root);

        let mut control = MalformedClosureControl {
            destination: closure.clone(),
            malformed_installed: false,
            preparation_cleanup_calls: 0,
            preparation_cleanup_injected: !fail_cleanup,
            root_invalidation_calls: 0,
            fail_invalidation,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x411,
            BODY_BYTES,
            CallbackSupplier {
                bound_invoked,
                supply_invoked,
                len: BODY_BYTES,
            },
            &mut control,
            &mut counters,
        );
        let (first_cause, dominant_cause) = attempt
            .error
            .map(publication_causes_v1)
            .map(|(first, dominant)| (Some(first), Some(dominant)))
            .unwrap_or((None, None));
        let (after_preparation_bytes, after_preparation_inodes) =
            directory_usage(&root.join("preparation"));
        let (after_immutable_bytes, after_immutable_inodes) = immutable_usage(root);
        if fail_cleanup {
            assert_eq!(
                (before_preparation_bytes, before_preparation_inodes),
                (0, 0)
            );
            assert!(control.preparation_cleanup_injected);
            assert_eq!(control.preparation_cleanup_calls, 6);
            assert_eq!(control.root_invalidation_calls, 1);
            assert_eq!(fs::read(&closure).unwrap(), [0_u8; 120]);

            let preparation_bytes = after_preparation_bytes - before_preparation_bytes;
            let preparation_inodes = after_preparation_inodes - before_preparation_inodes;
            assert!(preparation_bytes > 0);
            assert_eq!(preparation_inodes, 1);
            assert_eq!(after_immutable_bytes, before_immutable_bytes + 120);
            assert_eq!(after_immutable_inodes, before_immutable_inodes + 1);
            assert_eq!(counters.storage_bytes_retained, preparation_bytes);
            assert_eq!(counters.storage_inodes_retained, preparation_inodes);
            assert_eq!(
                counters.mutable_preparation_residue_bytes,
                preparation_bytes
            );
            assert_eq!(
                counters.mutable_preparation_residue_inodes,
                preparation_inodes
            );
            assert_eq!(counters.immutable_residue_bytes, 0);
            assert_eq!(counters.immutable_residue_inodes, 0);
            assert_eq!(counters.unreachable_installed_residue_bytes, 0);
            assert_eq!(cas.operation_admitted_slots_v1(), 0);
            assert_eq!(cas.operation_admission_active_for_test_v1(), 0);
            assert_eq!(cas.operation_admission_queue_for_test_v1(), (0, 0, 0));
            assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
            assert_storage_equations(&counters);
        }
        MalformedClosureObservationV1 {
            error: attempt.error.map(publication_error_v1),
            first_cause,
            dominant_cause,
            malformed_closure_installed: control.malformed_installed,
            malformed_closure_preserved: fs::read(&closure).ok().as_deref() == Some(&[0_u8; 120]),
            closure_bytes: fs::metadata(&closure)
                .map(|metadata| metadata.len())
                .unwrap_or(0),
            carrier_entries_preserved: directory_usage(&root.join("carriers")).1 == carrier_entries,
            catalog_entries_preserved: directory_usage(&root.join("catalog")).1 == catalog_entries,
            object_entries_preserved: directory_usage(&root.join("objects")).1 == object_entries,
            closure_fences: counters.closure_fences,
            residue_bytes: counters.unreachable_installed_residue_bytes,
            cleanup_calls: control.preparation_cleanup_calls,
            invalidation_attempts: control.root_invalidation_calls,
            preparation_bytes: after_preparation_bytes,
            preparation_entries: after_preparation_inodes,
            immutable_bytes: after_immutable_bytes,
            immutable_entries: after_immutable_inodes,
            storage_bytes_committed: counters.storage_bytes_committed,
            storage_inodes_committed: counters.storage_inodes_committed,
            storage_bytes_retained: counters.storage_bytes_retained,
            storage_inodes_retained: counters.storage_inodes_retained,
            operation_slots: cas.operation_admitted_slots_v1(),
            invalidated: matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            stale_invalidated: matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            reopen_invalidated: matches!(
                FsCasV1::open_existing(root),
                Err(FsCasErrorV1::Invalidated)
            ),
            zero_forbidden_work: counters.has_zero_forbidden_work(),
        }
    }

    pub fn atomic_closure_malformed_occupant_v1(root: &Path) -> MalformedClosureObservationV1 {
        malformed_closure_attempt(root, false, false)
    }

    pub fn malformed_closure_cleanup_terminal_v1(
        root: &Path,
        fail_invalidation: bool,
    ) -> MalformedClosureObservationV1 {
        malformed_closure_attempt(root, true, fail_invalidation)
    }

    #[derive(Default)]
    struct DirectInvalidationControl {
        fail: bool,
        attempts: u32,
    }

    impl FsCasControlV1 for DirectInvalidationControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::RootInvalidation {
                self.attempts += 1;
                return self.fail;
            }
            false
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum CandidateValidationFailureV1 {
        PermissionDenied,
        ReadFailure,
    }

    struct CandidateValidationFailureControl {
        cas: FsCasV1,
        failure: Option<FsCasErrorV1>,
        injected: bool,
    }

    impl CdcControlV1 for CandidateValidationFailureControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for CandidateValidationFailureControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary == FsCasBoundaryV1::BeforeCandidateValidation && !self.injected {
                self.injected = true;
                self.cas.fail_next_invalidation_probe_for_test_v1(
                    self.failure.take().expect("candidate validation failure"),
                );
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    pub fn invalidation_probe_failure_before_candidate_validation_v1(
        root: &Path,
        failure: CandidateValidationFailureV1,
    ) -> CreateFaultObservationV1 {
        const BODY_BYTES: u64 = 64 * 1024 + 17;
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let expected = FsCasErrorV1::Filesystem(match failure {
            CandidateValidationFailureV1::PermissionDenied => {
                FsCasFilesystemFailureV1::PermissionDenied
            }
            CandidateValidationFailureV1::ReadFailure => FsCasFilesystemFailureV1::ReadFailure,
        });
        let mut control = CandidateValidationFailureControl {
            cas: cas.clone(),
            failure: Some(expected),
            injected: false,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x7c0 + u64::from(failure == CandidateValidationFailureV1::ReadFailure),
            BODY_BYTES,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: BODY_BYTES,
            },
            &mut control,
            &mut counters,
        );
        let observation = observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            0,
            false,
            CreateFaultControlObservation {
                control_fired: control.injected,
                ..CreateFaultControlObservation::default()
            },
            &counters,
        );
        assert_eq!(cas.operation_admitted_slots_v1(), 0);
        assert_eq!(cas.operation_admission_active_for_test_v1(), 0);
        assert_eq!(cas.operation_admission_queue_for_test_v1(), (0, 0, 0));
        assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
        assert_eq!(fs::read_dir(root.join("preparation")).unwrap().count(), 0);
        assert_storage_equations(&counters);
        assert_eq!(
            counters.storage_inodes_reserved,
            counters.storage_inodes_released
                + counters.storage_inodes_committed
                + counters.storage_inodes_retained
        );
        observation
    }

    struct DirectPrivatePackCreateControl {
        cas: FsCasV1,
        error: FsCasErrorV1,
        fired: bool,
        fail_invalidation: bool,
        invalidation_attempts: u32,
    }

    impl FsCasControlV1 for DirectPrivatePackCreateControl {
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
            if !self.fired && boundary == FsCasFilesystemBoundaryV1::PrivatePackCreate {
                self.fired = true;
                self.cas.remove_active_preparation_inode_for_test_v1();
                Some(self.error)
            } else {
                None
            }
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::RootInvalidation {
                self.invalidation_attempts += 1;
                return self.fail_invalidation;
            }
            false
        }
    }

    struct DirectMarkerCreateControl {
        cas: FsCasV1,
        error: FsCasErrorV1,
        break_accounting: bool,
        fired: bool,
        fail_invalidation: bool,
        invalidation_attempts: u32,
    }

    impl FsCasControlV1 for DirectMarkerCreateControl {
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
            if !self.fired && boundary == FsCasFilesystemBoundaryV1::MarkerCreate {
                self.fired = true;
                if self.break_accounting {
                    self.cas.remove_active_preparation_inode_for_test_v1();
                }
                Some(self.error)
            } else {
                None
            }
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::RootInvalidation {
                self.invalidation_attempts += 1;
                return self.fail_invalidation;
            }
            false
        }
    }

    struct DirectMarkerLengthControl {
        cas: FsCasV1,
        corrupted: bool,
        restored_for_cleanup: bool,
        payload_or_link_seen: bool,
        fail_invalidation: bool,
        invalidation_attempts: u32,
    }

    impl FsCasControlV1 for DirectMarkerLengthControl {
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
            if !self.corrupted && boundary == FsCasFilesystemBoundaryV1::PermissionChange {
                self.corrupted = true;
                self.cas.inject_active_preparation_byte_for_test_v1();
            } else if matches!(
                boundary,
                FsCasFilesystemBoundaryV1::MarkerWrite | FsCasFilesystemBoundaryV1::MarkerHardLink
            ) {
                self.payload_or_link_seen = true;
            }
            None
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::RootInvalidation {
                if !self.restored_for_cleanup {
                    self.restored_for_cleanup = true;
                    self.cas.clear_active_preparation_bytes_for_test_v1();
                }
                self.invalidation_attempts += 1;
                return self.fail_invalidation;
            }
            false
        }
    }

    struct DirectMarkerImmutableControl {
        marker_write_seen: bool,
        marker_link_boundary_seen: bool,
        fail_invalidation: bool,
        invalidation_attempts: u32,
    }

    impl FsCasControlV1 for DirectMarkerImmutableControl {
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
            match boundary {
                FsCasFilesystemBoundaryV1::MarkerWrite => self.marker_write_seen = true,
                FsCasFilesystemBoundaryV1::MarkerHardLink => self.marker_link_boundary_seen = true,
                _ => {}
            }
            None
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::RootInvalidation {
                self.invalidation_attempts += 1;
                return self.fail_invalidation;
            }
            false
        }
    }

    struct RestoreMarkerCleanupAccountingControl {
        cas: FsCasV1,
        accounting_restored: bool,
        fail_invalidation: bool,
    }

    impl FsCasControlV1 for RestoreMarkerCleanupAccountingControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::RootInvalidation && !self.accounting_restored {
                self.cas.restore_active_preparation_bytes_for_test_v1(9);
                self.accounting_restored = true;
            }
            self.fail_invalidation && target == FsCasCleanupTargetV1::RootInvalidation
        }
    }

    #[cfg(unix)]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum MarkerCleanupUnlinkModeV1 {
        PermissionDenied,
        NonDirectory,
        Injected,
    }

    #[cfg(unix)]
    struct MarkerCleanupUnlinkControl {
        preparation: PathBuf,
        held_preparation: PathBuf,
        mode: MarkerCleanupUnlinkModeV1,
        armed: bool,
        restored: bool,
        fail_invalidation: bool,
    }

    #[cfg(unix)]
    impl MarkerCleanupUnlinkControl {
        fn restore_preparation(&mut self) {
            if self.restored {
                return;
            }
            match self.mode {
                MarkerCleanupUnlinkModeV1::PermissionDenied => {
                    use std::os::unix::fs::PermissionsExt;

                    fs::set_permissions(&self.preparation, fs::Permissions::from_mode(0o700))
                        .expect("restore marker cleanup permissions");
                }
                MarkerCleanupUnlinkModeV1::NonDirectory => {
                    fs::remove_file(&self.preparation).expect("remove marker cleanup stand-in");
                    fs::rename(&self.held_preparation, &self.preparation)
                        .expect("restore marker cleanup directory");
                }
                MarkerCleanupUnlinkModeV1::Injected => {}
            }
            self.restored = true;
        }
    }

    #[cfg(unix)]
    impl FsCasControlV1 for MarkerCleanupUnlinkControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::PreparationSpool && !self.armed {
                self.armed = true;
                match self.mode {
                    MarkerCleanupUnlinkModeV1::PermissionDenied => {
                        use std::os::unix::fs::PermissionsExt;

                        fs::set_permissions(&self.preparation, fs::Permissions::from_mode(0o500))
                            .expect("arm marker cleanup permissions");
                        return false;
                    }
                    MarkerCleanupUnlinkModeV1::NonDirectory => {
                        fs::rename(&self.preparation, &self.held_preparation)
                            .expect("hold marker cleanup directory");
                        fs::write(&self.preparation, b"not-a-directory")
                            .expect("install marker cleanup stand-in");
                        return false;
                    }
                    MarkerCleanupUnlinkModeV1::Injected => return true,
                }
            }
            if target == FsCasCleanupTargetV1::RootInvalidation {
                self.restore_preparation();
                return self.fail_invalidation;
            }
            false
        }
    }

    #[cfg(unix)]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum PreparationMetadataFaultModeV1 {
        WrongType,
        Missing,
        PermissionDenied,
        ReadFailure,
    }

    #[cfg(unix)]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum PreparationUnlinkFaultModeV1 {
        Missing,
        PermissionDenied,
        WriteFailure,
        Injected,
    }

    #[cfg(unix)]
    struct RestorePreparationMetadataAuthorityV1 {
        preparation: PathBuf,
        held_preparation: PathBuf,
        mode: PreparationMetadataFaultModeV1,
        restored: bool,
        fail_invalidation: bool,
    }

    #[cfg(unix)]
    impl RestorePreparationMetadataAuthorityV1 {
        fn restore_v1(&mut self) {
            if self.restored {
                return;
            }
            match self.mode {
                PreparationMetadataFaultModeV1::PermissionDenied => {
                    use std::os::unix::fs::PermissionsExt;

                    fs::set_permissions(&self.preparation, fs::Permissions::from_mode(0o700))
                        .expect("restore preparation permissions");
                }
                PreparationMetadataFaultModeV1::ReadFailure => {
                    fs::remove_file(&self.preparation).expect("remove preparation stand-in");
                    fs::rename(&self.held_preparation, &self.preparation)
                        .expect("restore preparation directory");
                }
                PreparationMetadataFaultModeV1::WrongType
                | PreparationMetadataFaultModeV1::Missing => {}
            }
            self.restored = true;
        }
    }

    #[cfg(unix)]
    impl FsCasControlV1 for RestorePreparationMetadataAuthorityV1 {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::RootInvalidation {
                self.restore_v1();
                return self.fail_invalidation;
            }
            false
        }
    }

    #[cfg(unix)]
    struct FailPreparationUnlinkV1 {
        preparation: PathBuf,
        held_preparation: PathBuf,
        spool_path: PathBuf,
        mode: PreparationUnlinkFaultModeV1,
        target: FsCasCleanupTargetV1,
        armed: bool,
        restored: bool,
        fail_invalidation: bool,
    }

    #[cfg(unix)]
    impl FailPreparationUnlinkV1 {
        fn restore_v1(&mut self) {
            if self.restored {
                return;
            }
            match self.mode {
                PreparationUnlinkFaultModeV1::PermissionDenied => {
                    use std::os::unix::fs::PermissionsExt;

                    fs::set_permissions(&self.preparation, fs::Permissions::from_mode(0o700))
                        .expect("restore unlink permissions");
                }
                PreparationUnlinkFaultModeV1::WriteFailure => {
                    fs::remove_file(&self.preparation).expect("remove unlink stand-in");
                    fs::rename(&self.held_preparation, &self.preparation)
                        .expect("restore unlink directory");
                }
                PreparationUnlinkFaultModeV1::Missing | PreparationUnlinkFaultModeV1::Injected => {}
            }
            self.restored = true;
        }
    }

    #[cfg(unix)]
    impl FsCasControlV1 for FailPreparationUnlinkV1 {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == self.target && !self.armed {
                self.armed = true;
                match self.mode {
                    PreparationUnlinkFaultModeV1::Missing => {
                        fs::remove_file(&self.spool_path).expect("remove unlink target");
                    }
                    PreparationUnlinkFaultModeV1::PermissionDenied => {
                        use std::os::unix::fs::PermissionsExt;

                        fs::set_permissions(&self.preparation, fs::Permissions::from_mode(0o500))
                            .expect("arm unlink permissions");
                    }
                    PreparationUnlinkFaultModeV1::WriteFailure => {
                        fs::rename(&self.preparation, &self.held_preparation)
                            .expect("hold unlink directory");
                        fs::write(&self.preparation, b"not-a-directory")
                            .expect("install unlink stand-in");
                    }
                    PreparationUnlinkFaultModeV1::Injected => return true,
                }
                return false;
            }
            if target == FsCasCleanupTargetV1::RootInvalidation {
                self.restore_v1();
                return self.fail_invalidation;
            }
            false
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct PackFaultObservationV1 {
        operation_error: Option<PublicationErrorV1>,
        cleanup_error: Option<PublicationErrorV1>,
        cleanup_retry_error: Option<PublicationErrorV1>,
        operation_first_cause: Option<PublicationCauseV1>,
        operation_dominant_cause: Option<PublicationCauseV1>,
        cleanup_first_cause: Option<PublicationCauseV1>,
        cleanup_dominant_cause: Option<PublicationCauseV1>,
        logical_length: u64,
        physical_length: Option<u64>,
        accounted_length: u64,
        physical_is_directory: bool,
        physical_is_missing: bool,
        preparation_bytes: u64,
        preparation_entries: u64,
        immutable_bytes: u64,
        immutable_entries: u64,
        storage_bytes_requested: u64,
        storage_bytes_reserved: u64,
        storage_bytes_released: u64,
        storage_bytes_committed: u64,
        storage_bytes_retained: u64,
        storage_inodes_requested: u64,
        storage_inodes_reserved: u64,
        storage_inodes_released: u64,
        storage_inodes_committed: u64,
        storage_inodes_retained: u64,
        mutable_preparation_residue_bytes: u64,
        mutable_preparation_residue_inodes: u64,
        unreachable_installed_residue_bytes: u64,
        operation_slots: u64,
        operation_active: u64,
        operation_queue: (u64, u64, u64),
        storage_active: (u64, u64, u64),
        invalidated: bool,
        stale_invalidated: bool,
        reopen_invalidated: bool,
        root_usable: bool,
        stale_usable: bool,
        reopen_usable: bool,
        zero_forbidden_work: bool,
    }

    macro_rules! pack_fault_getters {
        ($($name:ident: $field:ident => $ty:ty),* $(,)?) => {
            impl PackFaultObservationV1 {
                $(pub const fn $name(self) -> $ty { self.$field })*
            }
        };
    }

    pack_fault_getters! {
        operation_error: operation_error => Option<PublicationErrorV1>,
        cleanup_error: cleanup_error => Option<PublicationErrorV1>,
        cleanup_retry_error: cleanup_retry_error => Option<PublicationErrorV1>,
        operation_first_cause: operation_first_cause => Option<PublicationCauseV1>,
        operation_dominant_cause: operation_dominant_cause => Option<PublicationCauseV1>,
        cleanup_first_cause: cleanup_first_cause => Option<PublicationCauseV1>,
        cleanup_dominant_cause: cleanup_dominant_cause => Option<PublicationCauseV1>,
        logical_length: logical_length => u64,
        physical_length: physical_length => Option<u64>,
        accounted_length: accounted_length => u64,
        physical_is_directory: physical_is_directory => bool,
        physical_is_missing: physical_is_missing => bool,
        preparation_bytes: preparation_bytes => u64,
        preparation_entries: preparation_entries => u64,
        immutable_bytes: immutable_bytes => u64,
        immutable_entries: immutable_entries => u64,
        storage_bytes_requested: storage_bytes_requested => u64,
        storage_bytes_reserved: storage_bytes_reserved => u64,
        storage_bytes_released: storage_bytes_released => u64,
        storage_bytes_committed: storage_bytes_committed => u64,
        storage_bytes_retained: storage_bytes_retained => u64,
        storage_inodes_requested: storage_inodes_requested => u64,
        storage_inodes_reserved: storage_inodes_reserved => u64,
        storage_inodes_released: storage_inodes_released => u64,
        storage_inodes_committed: storage_inodes_committed => u64,
        storage_inodes_retained: storage_inodes_retained => u64,
        mutable_preparation_residue_bytes: mutable_preparation_residue_bytes => u64,
        mutable_preparation_residue_inodes: mutable_preparation_residue_inodes => u64,
        unreachable_installed_residue_bytes: unreachable_installed_residue_bytes => u64,
        operation_slots: operation_slots => u64,
        operation_active: operation_active => u64,
        operation_queue: operation_queue => (u64, u64, u64),
        storage_active: storage_active => (u64, u64, u64),
        invalidated: invalidated => bool,
        stale_invalidated: stale_invalidated => bool,
        reopen_invalidated: reopen_invalidated => bool,
        root_usable: root_usable => bool,
        stale_usable: stale_usable => bool,
        reopen_usable: reopen_usable => bool,
        zero_forbidden_work: zero_forbidden_work => bool,
    }

    fn lossy_preparation_usage(path: &Path) -> (u64, u64) {
        let Ok(entries) = fs::read_dir(path) else {
            return (0, 0);
        };
        entries
            .filter_map(Result::ok)
            .fold((0, 0), |(bytes, entries), entry| {
                let next_entries = entries + 1;
                let next_bytes = entry
                    .metadata()
                    .ok()
                    .filter(|metadata| metadata.file_type().is_file())
                    .map_or(bytes, |metadata| bytes.saturating_add(metadata.len()));
                (next_bytes, next_entries)
            })
    }

    fn regular_file_length(path: &Path) -> Option<u64> {
        fs::symlink_metadata(path)
            .ok()
            .filter(|metadata| metadata.file_type().is_file())
            .map(|metadata| metadata.len())
    }

    fn preparation_path_kind(path: &Path) -> (bool, bool) {
        match fs::symlink_metadata(path) {
            Ok(metadata) => (metadata.file_type().is_dir(), false),
            Err(error) => (false, error.kind() == std::io::ErrorKind::NotFound),
        }
    }

    #[cfg(unix)]
    fn apply_preparation_metadata_fault(
        preparation: &Path,
        path: &Path,
        held_preparation: &Path,
        mode: PreparationMetadataFaultModeV1,
    ) {
        match mode {
            PreparationMetadataFaultModeV1::WrongType => {
                fs::remove_file(path).expect("remove metadata fixture");
                fs::create_dir(path).expect("replace metadata fixture");
            }
            PreparationMetadataFaultModeV1::Missing => {
                fs::remove_file(path).expect("remove metadata fixture");
            }
            PreparationMetadataFaultModeV1::PermissionDenied => {
                use std::os::unix::fs::PermissionsExt;

                fs::set_permissions(preparation, fs::Permissions::from_mode(0o000))
                    .expect("arm metadata permissions");
            }
            PreparationMetadataFaultModeV1::ReadFailure => {
                fs::rename(preparation, held_preparation).expect("hold metadata directory");
                fs::write(preparation, b"not-a-directory").expect("install metadata stand-in");
            }
        }
    }

    fn observe_pack_fault(
        root: &Path,
        cas: &FsCasV1,
        stale: &FsCasV1,
        operation_error: Option<FsCasErrorV1>,
        cleanup_error: Option<FsCasErrorV1>,
        cleanup_retry_error: Option<FsCasErrorV1>,
        logical_length: u64,
        physical_length: Option<u64>,
        physical_is_directory: bool,
        physical_is_missing: bool,
        accounted_length: u64,
        counters: &OperationCountersV1,
    ) -> PackFaultObservationV1 {
        let (operation_first_cause, operation_dominant_cause) = operation_error
            .map(publication_causes_v1)
            .map(|(first, dominant)| (Some(first), Some(dominant)))
            .unwrap_or((None, None));
        let (cleanup_first_cause, cleanup_dominant_cause) = cleanup_error
            .map(publication_causes_v1)
            .map(|(first, dominant)| (Some(first), Some(dominant)))
            .unwrap_or((None, None));
        let (preparation_bytes, preparation_entries) =
            lossy_preparation_usage(&root.join("preparation"));
        let (immutable_bytes, immutable_entries) = immutable_usage(root);
        let reopen = FsCasV1::open_existing(root);
        PackFaultObservationV1 {
            operation_error: operation_error.map(publication_error_v1),
            cleanup_error: cleanup_error.map(publication_error_v1),
            cleanup_retry_error: cleanup_retry_error.map(publication_error_v1),
            operation_first_cause,
            operation_dominant_cause,
            cleanup_first_cause,
            cleanup_dominant_cause,
            logical_length,
            physical_length,
            accounted_length,
            physical_is_directory,
            physical_is_missing,
            preparation_bytes,
            preparation_entries,
            immutable_bytes,
            immutable_entries,
            storage_bytes_requested: counters.storage_bytes_requested,
            storage_bytes_reserved: counters.storage_bytes_reserved,
            storage_bytes_released: counters.storage_bytes_released,
            storage_bytes_committed: counters.storage_bytes_committed,
            storage_bytes_retained: counters.storage_bytes_retained,
            storage_inodes_requested: counters.storage_inodes_requested,
            storage_inodes_reserved: counters.storage_inodes_reserved,
            storage_inodes_released: counters.storage_inodes_released,
            storage_inodes_committed: counters.storage_inodes_committed,
            storage_inodes_retained: counters.storage_inodes_retained,
            mutable_preparation_residue_bytes: counters.mutable_preparation_residue_bytes,
            mutable_preparation_residue_inodes: counters.mutable_preparation_residue_inodes,
            unreachable_installed_residue_bytes: counters.unreachable_installed_residue_bytes,
            operation_slots: cas.operation_admitted_slots_v1(),
            operation_active: cas.operation_admission_active_for_test_v1(),
            operation_queue: cas.operation_admission_queue_for_test_v1(),
            storage_active: cas.storage_admission_active_for_test_v1(),
            invalidated: matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            stale_invalidated: matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            reopen_invalidated: matches!(
                &reopen,
                Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy)
            ),
            root_usable: cas.occupied().is_ok(),
            stale_usable: stale.occupied().is_ok(),
            reopen_usable: reopen.is_ok(),
            zero_forbidden_work: counters.has_zero_forbidden_work(),
        }
    }

    #[cfg(unix)]
    pub fn operation_spool_cleanup_accounting_fault_v1(
        root: &Path,
        before_unlink: bool,
        fail_invalidation: bool,
    ) -> PackFaultObservationV1 {
        const SPOOL_BYTES: u64 = 17;
        let (cas, stale) = new_fault_root(root);
        let mut counters = OperationCountersV1::default();
        let mut setup_control = ContinueFaultControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x8fc,
                &mut counters,
                &mut setup_control,
            )
            .expect("operation-spool cleanup accounting capability");
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(SPOOL_BYTES, 0, 1, 0).unwrap())
            .expect("operation-spool cleanup accounting envelope");
        let token = capability
            .storage_token_v1()
            .expect("operation-spool cleanup accounting token");
        let mut spool = cas
            .begin_operation_spool_borrowed_v1("cleanup-accounting", token, &mut setup_control)
            .expect("operation-spool cleanup accounting handle");
        spool
            .initialize_zeroed_len_controlled_v1(SPOOL_BYTES, &mut setup_control)
            .expect("operation-spool cleanup accounting initialization");
        let spool_path = fs::read_dir(root.join("preparation"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        if before_unlink {
            cas.clear_active_preparation_bytes_for_test_v1();
        } else {
            cas.remove_active_preparation_inode_for_test_v1();
        }
        let mut control = DirectInvalidationControl {
            fail: fail_invalidation,
            attempts: 0,
        };
        let cleanup_error = spool.cleanup_controlled_v1(&mut control).err();
        let cleanup_retry_error = spool.cleanup_controlled_v1(&mut control).err();
        assert_eq!(
            cleanup_retry_error, cleanup_error,
            "operation-spool accounting cleanup changed on retry"
        );
        let physical_length = regular_file_length(&spool_path);
        let (physical_is_directory, physical_is_missing) = preparation_path_kind(&spool_path);
        drop(spool);
        capability
            .finish_terminal_v1(false, &mut counters, &mut setup_control)
            .expect("operation-spool cleanup accounting terminal");
        observe_pack_fault(
            root,
            &cas,
            &stale,
            None,
            cleanup_error,
            cleanup_retry_error,
            SPOOL_BYTES,
            physical_length,
            physical_is_directory,
            physical_is_missing,
            SPOOL_BYTES,
            &counters,
        )
    }

    #[cfg(unix)]
    pub fn operation_spool_cleanup_metadata_fault_v1(
        root: &Path,
        mode: PreparationMetadataFaultModeV1,
        fail_invalidation: bool,
    ) -> PackFaultObservationV1 {
        const SPOOL_BYTES: u64 = 19;
        let (cas, stale) = new_fault_root(root);
        let mut counters = OperationCountersV1::default();
        let mut setup_control = ContinueFaultControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x8f7,
                &mut counters,
                &mut setup_control,
            )
            .expect("operation-spool cleanup metadata capability");
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(SPOOL_BYTES, 0, 1, 0).unwrap())
            .expect("operation-spool cleanup metadata envelope");
        let token = capability
            .storage_token_v1()
            .expect("operation-spool cleanup metadata token");
        let mut spool = cas
            .begin_operation_spool_borrowed_v1("cleanup-metadata", token, &mut setup_control)
            .expect("operation-spool cleanup metadata handle");
        spool
            .initialize_zeroed_len_controlled_v1(SPOOL_BYTES, &mut setup_control)
            .expect("operation-spool cleanup metadata initialization");
        let preparation = root.join("preparation");
        let spool_path = fs::read_dir(&preparation)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let held_preparation = root.join("preparation-held-for-read-failure");
        apply_preparation_metadata_fault(&preparation, &spool_path, &held_preparation, mode);
        let mut control = RestorePreparationMetadataAuthorityV1 {
            preparation,
            held_preparation,
            mode,
            restored: false,
            fail_invalidation,
        };
        let cleanup_error = spool.cleanup_controlled_v1(&mut control).err();
        let cleanup_retry_error = spool.cleanup_controlled_v1(&mut control).err();
        assert_eq!(
            cleanup_retry_error, cleanup_error,
            "operation-spool metadata cleanup changed on retry"
        );
        if matches!(mode, PreparationMetadataFaultModeV1::Missing) {
            assert_eq!(
                fs::symlink_metadata(&spool_path).unwrap_err().kind(),
                std::io::ErrorKind::NotFound,
            );
        }
        let physical_length = regular_file_length(&spool_path);
        let (physical_is_directory, physical_is_missing) = preparation_path_kind(&spool_path);
        drop(spool);
        capability
            .finish_terminal_v1(false, &mut counters, &mut setup_control)
            .expect("operation-spool cleanup metadata terminal");
        observe_pack_fault(
            root,
            &cas,
            &stale,
            None,
            cleanup_error,
            cleanup_retry_error,
            SPOOL_BYTES,
            physical_length,
            physical_is_directory,
            physical_is_missing,
            SPOOL_BYTES,
            &counters,
        )
    }

    #[cfg(unix)]
    pub fn operation_spool_drop_metadata_fault_v1(
        root: &Path,
        mode: Option<PreparationMetadataFaultModeV1>,
    ) -> PackFaultObservationV1 {
        const LOGICAL_BYTES: u64 = 23;
        const PHYSICAL_BYTES: u64 = 7;
        let (cas, stale) = new_fault_root(root);
        let mut counters = OperationCountersV1::default();
        let mut setup_control = ContinueFaultControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x907,
                &mut counters,
                &mut setup_control,
            )
            .expect("operation-spool drop metadata capability");
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(LOGICAL_BYTES, 0, 1, 0).unwrap())
            .expect("operation-spool drop metadata envelope");
        let token = capability
            .storage_token_v1()
            .expect("operation-spool drop metadata token");
        let mut spool = cas
            .begin_operation_spool_borrowed_v1("drop-metadata", token, &mut setup_control)
            .expect("operation-spool drop metadata handle");
        spool
            .initialize_zeroed_len_controlled_v1(LOGICAL_BYTES, &mut setup_control)
            .expect("operation-spool drop metadata initialization");
        let preparation = root.join("preparation");
        let spool_path = fs::read_dir(&preparation)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        fs::OpenOptions::new()
            .write(true)
            .open(&spool_path)
            .unwrap()
            .set_len(PHYSICAL_BYTES)
            .unwrap();
        let held_preparation = root.join("preparation-held-for-drop-read");
        if let Some(mode) = mode {
            apply_preparation_metadata_fault(&preparation, &spool_path, &held_preparation, mode);
        }
        drop(spool);
        if let Some(mode) = mode {
            match mode {
                PreparationMetadataFaultModeV1::PermissionDenied => {
                    use std::os::unix::fs::PermissionsExt;

                    fs::set_permissions(&preparation, fs::Permissions::from_mode(0o700)).unwrap();
                }
                PreparationMetadataFaultModeV1::ReadFailure => {
                    fs::remove_file(&preparation).unwrap();
                    fs::rename(&held_preparation, &preparation).unwrap();
                }
                PreparationMetadataFaultModeV1::WrongType
                | PreparationMetadataFaultModeV1::Missing => {}
            }
        }
        match mode {
            Some(PreparationMetadataFaultModeV1::WrongType) => {
                assert_eq!(fs::read_dir(&preparation).unwrap().count(), 1);
            }
            Some(PreparationMetadataFaultModeV1::Missing) => {
                assert_eq!(
                    fs::symlink_metadata(&spool_path).unwrap_err().kind(),
                    std::io::ErrorKind::NotFound,
                );
            }
            None
            | Some(PreparationMetadataFaultModeV1::PermissionDenied)
            | Some(PreparationMetadataFaultModeV1::ReadFailure) => {}
        }
        let physical_length = regular_file_length(&spool_path);
        let (physical_is_directory, physical_is_missing) = preparation_path_kind(&spool_path);
        capability
            .finish_terminal_v1(false, &mut counters, &mut setup_control)
            .expect("operation-spool drop metadata terminal");
        observe_pack_fault(
            root,
            &cas,
            &stale,
            None,
            None,
            None,
            LOGICAL_BYTES,
            physical_length,
            physical_is_directory,
            physical_is_missing,
            if mode.is_some() { LOGICAL_BYTES } else { 0 },
            &counters,
        )
    }

    #[cfg(unix)]
    pub fn operation_spool_unlink_fault_v1(
        root: &Path,
        mode: PreparationUnlinkFaultModeV1,
        fail_invalidation: bool,
    ) -> PackFaultObservationV1 {
        const SPOOL_BYTES: u64 = 23;
        let (cas, stale) = new_fault_root(root);
        let mut counters = OperationCountersV1::default();
        let mut setup_control = ContinueFaultControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x8f8,
                &mut counters,
                &mut setup_control,
            )
            .expect("operation-spool unlink capability");
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(SPOOL_BYTES, 0, 1, 0).unwrap())
            .expect("operation-spool unlink envelope");
        let token = capability
            .storage_token_v1()
            .expect("operation-spool unlink token");
        let mut spool = cas
            .begin_operation_spool_borrowed_v1("cleanup-unlink", token, &mut setup_control)
            .expect("operation-spool unlink handle");
        spool
            .initialize_zeroed_len_controlled_v1(SPOOL_BYTES, &mut setup_control)
            .expect("operation-spool unlink initialization");
        let preparation = root.join("preparation");
        let spool_path = fs::read_dir(&preparation)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let mut control = FailPreparationUnlinkV1 {
            preparation: preparation.clone(),
            held_preparation: root.join("preparation-held-for-unlink"),
            spool_path: spool_path.clone(),
            mode,
            target: FsCasCleanupTargetV1::PreparationSpool,
            armed: false,
            restored: false,
            fail_invalidation,
        };
        let cleanup_error = spool.cleanup_controlled_v1(&mut control).err();
        let cleanup_retry_error = spool.cleanup_controlled_v1(&mut control).err();
        assert_eq!(
            cleanup_retry_error, cleanup_error,
            "operation-spool unlink cleanup changed on retry"
        );
        if matches!(mode, PreparationUnlinkFaultModeV1::Missing) {
            assert_eq!(
                fs::symlink_metadata(&spool_path).unwrap_err().kind(),
                std::io::ErrorKind::NotFound,
            );
        } else {
            assert_eq!(fs::read_dir(&preparation).unwrap().count(), 1);
        }
        let physical_length = regular_file_length(&spool_path);
        let (physical_is_directory, physical_is_missing) = preparation_path_kind(&spool_path);
        drop(spool);
        capability
            .finish_terminal_v1(false, &mut counters, &mut setup_control)
            .expect("operation-spool unlink terminal");
        observe_pack_fault(
            root,
            &cas,
            &stale,
            None,
            cleanup_error,
            cleanup_retry_error,
            SPOOL_BYTES,
            physical_length,
            physical_is_directory,
            physical_is_missing,
            SPOOL_BYTES,
            &counters,
        )
    }

    #[cfg(unix)]
    pub fn private_pack_cleanup_metadata_fault_v1(
        root: &Path,
        mode: PreparationMetadataFaultModeV1,
        fail_invalidation: bool,
    ) -> PackFaultObservationV1 {
        const PACK_CEILING: u64 = 128;
        let (cas, stale) = new_fault_root(root);
        let mut counters = OperationCountersV1::default();
        let mut setup_control = ContinueFaultControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x8f9,
                &mut counters,
                &mut setup_control,
            )
            .expect("private-pack cleanup metadata capability");
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(PACK_CEILING, 0, 1, 0).unwrap())
            .expect("private-pack cleanup metadata envelope");
        let token = capability
            .storage_token_v1()
            .expect("private-pack cleanup metadata token");
        let mut private_pack = cas
            .begin_private_pack_borrowed_v1(token)
            .expect("private-pack cleanup metadata handle");
        private_pack
            .begin_direct_controlled_v1(PACK_CEILING, &mut setup_control)
            .expect("private-pack cleanup metadata initialization");
        let preparation = root.join("preparation");
        let pack_path = fs::read_dir(&preparation)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let held_preparation = root.join("preparation-held-for-private-read");
        apply_preparation_metadata_fault(&preparation, &pack_path, &held_preparation, mode);
        let mut control = RestorePreparationMetadataAuthorityV1 {
            preparation: preparation.clone(),
            held_preparation,
            mode,
            restored: false,
            fail_invalidation,
        };
        let cleanup_error = private_pack.cleanup_controlled_v1(&mut control).err();
        let cleanup_retry_error = private_pack.cleanup_controlled_v1(&mut control).err();
        assert_eq!(
            cleanup_retry_error, cleanup_error,
            "private-pack metadata cleanup changed on retry"
        );
        assert_eq!(
            fs::read_dir(&preparation).unwrap().count(),
            usize::from(!matches!(mode, PreparationMetadataFaultModeV1::Missing)),
        );
        match mode {
            PreparationMetadataFaultModeV1::WrongType => {
                assert!(fs::symlink_metadata(&pack_path)
                    .unwrap()
                    .file_type()
                    .is_dir());
            }
            PreparationMetadataFaultModeV1::Missing => {
                assert_eq!(
                    fs::symlink_metadata(&pack_path).unwrap_err().kind(),
                    std::io::ErrorKind::NotFound,
                );
            }
            PreparationMetadataFaultModeV1::PermissionDenied
            | PreparationMetadataFaultModeV1::ReadFailure => {
                assert_eq!(fs::metadata(&pack_path).unwrap().len(), PACK_HEADER_BYTES);
            }
        }
        for immutable in ["carriers", "objects", "catalog", "closures"] {
            assert_eq!(fs::read_dir(root.join(immutable)).unwrap().count(), 0);
        }
        let physical_length = regular_file_length(&pack_path);
        let (physical_is_directory, physical_is_missing) = preparation_path_kind(&pack_path);
        drop(private_pack);
        capability
            .finish_terminal_v1(false, &mut counters, &mut setup_control)
            .expect("private-pack cleanup metadata terminal");
        observe_pack_fault(
            root,
            &cas,
            &stale,
            None,
            cleanup_error,
            cleanup_retry_error,
            PACK_CEILING,
            physical_length,
            physical_is_directory,
            physical_is_missing,
            PACK_HEADER_BYTES,
            &counters,
        )
    }

    #[cfg(unix)]
    pub fn private_pack_drop_metadata_fault_v1(
        root: &Path,
        mode: Option<PreparationMetadataFaultModeV1>,
    ) -> PackFaultObservationV1 {
        const PACK_CEILING: u64 = 128;
        const PHYSICAL_BYTES: u64 = 7;
        let (cas, stale) = new_fault_root(root);
        let mut counters = OperationCountersV1::default();
        let mut setup_control = ContinueFaultControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x909,
                &mut counters,
                &mut setup_control,
            )
            .expect("private-pack drop metadata capability");
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(PACK_CEILING, 0, 1, 0).unwrap())
            .expect("private-pack drop metadata envelope");
        let token = capability
            .storage_token_v1()
            .expect("private-pack drop metadata token");
        let mut private_pack = cas
            .begin_private_pack_borrowed_v1(token)
            .expect("private-pack drop metadata handle");
        private_pack
            .begin_direct_controlled_v1(PACK_CEILING, &mut setup_control)
            .expect("private-pack drop metadata initialization");
        let preparation = root.join("preparation");
        let pack_path = fs::read_dir(&preparation)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        fs::OpenOptions::new()
            .write(true)
            .open(&pack_path)
            .unwrap()
            .set_len(PHYSICAL_BYTES)
            .unwrap();
        let held_preparation = root.join("preparation-held-for-private-drop");
        if let Some(mode) = mode {
            apply_preparation_metadata_fault(&preparation, &pack_path, &held_preparation, mode);
        }
        drop(private_pack);
        if let Some(mode) = mode {
            match mode {
                PreparationMetadataFaultModeV1::PermissionDenied => {
                    use std::os::unix::fs::PermissionsExt;

                    fs::set_permissions(&preparation, fs::Permissions::from_mode(0o700)).unwrap();
                }
                PreparationMetadataFaultModeV1::ReadFailure => {
                    fs::remove_file(&preparation).unwrap();
                    fs::rename(&held_preparation, &preparation).unwrap();
                }
                PreparationMetadataFaultModeV1::WrongType
                | PreparationMetadataFaultModeV1::Missing => {}
            }
        }
        for immutable in ["carriers", "objects", "catalog", "closures"] {
            assert_eq!(fs::read_dir(root.join(immutable)).unwrap().count(), 0);
        }
        match mode {
            Some(PreparationMetadataFaultModeV1::WrongType) => {
                assert_eq!(fs::read_dir(&preparation).unwrap().count(), 1);
                assert!(fs::symlink_metadata(&pack_path)
                    .unwrap()
                    .file_type()
                    .is_dir());
            }
            Some(PreparationMetadataFaultModeV1::Missing) => {
                assert_eq!(fs::read_dir(&preparation).unwrap().count(), 0);
                assert_eq!(
                    fs::symlink_metadata(&pack_path).unwrap_err().kind(),
                    std::io::ErrorKind::NotFound,
                );
            }
            Some(
                PreparationMetadataFaultModeV1::PermissionDenied
                | PreparationMetadataFaultModeV1::ReadFailure,
            ) => {
                assert_eq!(fs::read_dir(&preparation).unwrap().count(), 1);
                assert_eq!(fs::metadata(&pack_path).unwrap().len(), PHYSICAL_BYTES);
            }
            None => assert_eq!(fs::read_dir(&preparation).unwrap().count(), 0),
        }
        let physical_length = regular_file_length(&pack_path);
        let (physical_is_directory, physical_is_missing) = preparation_path_kind(&pack_path);
        capability
            .finish_terminal_v1(false, &mut counters, &mut setup_control)
            .expect("private-pack drop metadata terminal");
        observe_pack_fault(
            root,
            &cas,
            &stale,
            None,
            None,
            None,
            PACK_CEILING,
            physical_length,
            physical_is_directory,
            physical_is_missing,
            if mode.is_some() { PACK_HEADER_BYTES } else { 0 },
            &counters,
        )
    }

    #[cfg(unix)]
    pub fn private_pack_unlink_fault_v1(
        root: &Path,
        mode: PreparationUnlinkFaultModeV1,
        fail_invalidation: bool,
    ) -> PackFaultObservationV1 {
        const PACK_CEILING: u64 = 128;
        let (cas, stale) = new_fault_root(root);
        let mut counters = OperationCountersV1::default();
        let mut setup_control = ContinueFaultControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x8fa,
                &mut counters,
                &mut setup_control,
            )
            .expect("private-pack unlink capability");
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(PACK_CEILING, 0, 1, 0).unwrap())
            .expect("private-pack unlink envelope");
        let token = capability
            .storage_token_v1()
            .expect("private-pack unlink token");
        let mut private_pack = cas
            .begin_private_pack_borrowed_v1(token)
            .expect("private-pack unlink handle");
        private_pack
            .begin_direct_controlled_v1(PACK_CEILING, &mut setup_control)
            .expect("private-pack unlink initialization");
        let preparation = root.join("preparation");
        let pack_path = fs::read_dir(&preparation)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let mut control = FailPreparationUnlinkV1 {
            preparation: preparation.clone(),
            held_preparation: root.join("preparation-held-for-private-unlink"),
            spool_path: pack_path.clone(),
            mode,
            target: FsCasCleanupTargetV1::PrivatePack,
            armed: false,
            restored: false,
            fail_invalidation,
        };
        let cleanup_error = private_pack.cleanup_controlled_v1(&mut control).err();
        let cleanup_retry_error = private_pack.cleanup_controlled_v1(&mut control).err();
        assert_eq!(
            cleanup_retry_error, cleanup_error,
            "private-pack unlink cleanup changed on retry"
        );
        if matches!(mode, PreparationUnlinkFaultModeV1::Missing) {
            assert_eq!(
                fs::symlink_metadata(&pack_path).unwrap_err().kind(),
                std::io::ErrorKind::NotFound,
            );
            assert_eq!(fs::read_dir(&preparation).unwrap().count(), 0);
        } else {
            assert_eq!(fs::metadata(&pack_path).unwrap().len(), PACK_HEADER_BYTES);
            assert_eq!(fs::read_dir(&preparation).unwrap().count(), 1);
        }
        for immutable in ["carriers", "objects", "catalog", "closures"] {
            assert_eq!(fs::read_dir(root.join(immutable)).unwrap().count(), 0);
        }
        let physical_length = regular_file_length(&pack_path);
        let (physical_is_directory, physical_is_missing) = preparation_path_kind(&pack_path);
        drop(private_pack);
        capability
            .finish_terminal_v1(false, &mut counters, &mut setup_control)
            .expect("private-pack unlink terminal");
        observe_pack_fault(
            root,
            &cas,
            &stale,
            None,
            cleanup_error,
            cleanup_retry_error,
            PACK_CEILING,
            physical_length,
            physical_is_directory,
            physical_is_missing,
            PACK_HEADER_BYTES,
            &counters,
        )
    }

    pub fn private_pack_truncate_accounting_fault_v1(
        root: &Path,
        truncate: bool,
        fail_invalidation: bool,
    ) -> PackFaultObservationV1 {
        const PACK_CEILING: u64 = 128;
        const APPEND_BYTES: u64 = 16;
        const TRUNCATED_BYTES: u64 = PACK_HEADER_BYTES + 6;
        let (cas, stale) = new_fault_root(root);
        let mut counters = OperationCountersV1::default();
        let mut setup_control = ContinueFaultControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x8ff,
                &mut counters,
                &mut setup_control,
            )
            .expect("private-pack accounting capability");
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(PACK_CEILING, 0, 1, 0).unwrap())
            .expect("private-pack accounting envelope");
        let token = capability
            .storage_token_v1()
            .expect("private-pack accounting token");
        let mut private_pack = cas
            .begin_private_pack_borrowed_v1(token)
            .expect("private-pack accounting handle");
        private_pack
            .begin_direct_controlled_v1(PACK_CEILING, &mut setup_control)
            .expect("private-pack accounting initialization");
        let pack_path = fs::read_dir(root.join("preparation"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        if truncate {
            private_pack
                .append_controlled_v1(&[0x5a; APPEND_BYTES as usize], &mut setup_control)
                .expect("private-pack accounting seed");
        }
        cas.clear_active_preparation_bytes_for_test_v1();
        let mut control = DirectInvalidationControl {
            fail: fail_invalidation,
            attempts: 0,
        };
        let operation = if truncate {
            private_pack.truncate_direct_controlled_v1(TRUNCATED_BYTES, &mut control)
        } else {
            private_pack.append_controlled_v1(&[0x6b; 8], &mut control)
        };
        assert!(
            operation.is_err(),
            "private-pack accounting operation succeeded"
        );
        let operation_error = private_pack.take_first_error_typed_v1();
        let (physical_length, accounted_length) = private_pack.direct_lengths_for_test_v1();
        let cleanup_error = private_pack.cleanup_controlled_v1(&mut setup_control).err();
        let cleanup_retry_error = private_pack.cleanup_controlled_v1(&mut setup_control).err();
        assert_eq!(
            cleanup_retry_error, cleanup_error,
            "private-pack accounting cleanup changed on retry"
        );
        for immutable in ["carriers", "objects", "catalog", "closures"] {
            assert_eq!(fs::read_dir(root.join(immutable)).unwrap().count(), 0);
        }
        let (physical_is_directory, physical_is_missing) = preparation_path_kind(&pack_path);
        drop(private_pack);
        capability
            .finish_terminal_v1(false, &mut counters, &mut setup_control)
            .expect("private-pack accounting terminal");
        observe_pack_fault(
            root,
            &cas,
            &stale,
            operation_error,
            cleanup_error,
            cleanup_retry_error,
            physical_length.unwrap_or_default(),
            physical_length,
            physical_is_directory,
            physical_is_missing,
            accounted_length,
            &counters,
        )
    }

    pub fn private_pack_cleanup_accounting_fault_v1(
        root: &Path,
        before_unlink: bool,
        fail_invalidation: bool,
    ) -> PackFaultObservationV1 {
        const PACK_CEILING: u64 = 128;
        let (cas, stale) = new_fault_root(root);
        let mut counters = OperationCountersV1::default();
        let mut setup_control = ContinueFaultControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x8fd,
                &mut counters,
                &mut setup_control,
            )
            .expect("private-pack cleanup accounting capability");
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(PACK_CEILING, 0, 1, 0).unwrap())
            .expect("private-pack cleanup accounting envelope");
        let token = capability
            .storage_token_v1()
            .expect("private-pack cleanup accounting token");
        let mut private_pack = cas
            .begin_private_pack_borrowed_v1(token)
            .expect("private-pack cleanup accounting handle");
        private_pack
            .begin_direct_controlled_v1(PACK_CEILING, &mut setup_control)
            .expect("private-pack cleanup accounting initialization");
        let pack_path = fs::read_dir(root.join("preparation"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        if before_unlink {
            cas.clear_active_preparation_bytes_for_test_v1();
        } else {
            cas.remove_active_preparation_inode_for_test_v1();
        }
        let mut control = DirectInvalidationControl {
            fail: fail_invalidation,
            attempts: 0,
        };
        let cleanup_error = private_pack.cleanup_controlled_v1(&mut control).err();
        let cleanup_retry_error = private_pack.cleanup_controlled_v1(&mut control).err();
        assert_eq!(
            cleanup_retry_error, cleanup_error,
            "private-pack accounting cleanup changed on retry"
        );
        for immutable in ["carriers", "objects", "catalog", "closures"] {
            assert_eq!(fs::read_dir(root.join(immutable)).unwrap().count(), 0);
        }
        let physical_length = regular_file_length(&pack_path);
        let (physical_is_directory, physical_is_missing) = preparation_path_kind(&pack_path);
        drop(private_pack);
        capability
            .finish_terminal_v1(false, &mut counters, &mut setup_control)
            .expect("private-pack cleanup accounting terminal");
        observe_pack_fault(
            root,
            &cas,
            &stale,
            None,
            cleanup_error,
            cleanup_retry_error,
            PACK_CEILING,
            physical_length,
            physical_is_directory,
            physical_is_missing,
            PACK_HEADER_BYTES,
            &counters,
        )
    }

    fn observe_direct_fault(
        root: &Path,
        cas: &FsCasV1,
        stale: &FsCasV1,
        operation_error: Option<FsCasErrorV1>,
        terminal_error: Option<FsCasErrorV1>,
        control: CreateFaultControlObservation,
        invalidation_attempts: u32,
        counters: &OperationCountersV1,
    ) -> CreateFaultObservationV1 {
        let error = match (operation_error, terminal_error) {
            (Some(first), Some(later)) => Some(first.dominated_by_v1(later)),
            (Some(error), None) | (None, Some(error)) => Some(error),
            (None, None) => None,
        };
        let mut observation = observe_create_fault_with_control(
            root,
            cas,
            stale,
            CreateFaultAttempt {
                error,
                panicked: false,
                panic_payload: None,
            },
            false,
            false,
            false,
            0,
            invalidation_attempts,
            false,
            control,
            counters,
        );
        observation.operation_error = operation_error.map(publication_error_v1);
        observation.terminal_error = terminal_error.map(publication_error_v1);
        observation
    }

    /// Scalar custody for the file-backed operation-spool fault owner.  The
    /// spool handle and its control never cross the feature boundary.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct OperationSpoolFaultObservationV1 {
        operation_error: Option<PublicationErrorV1>,
        cleanup_error: Option<PublicationErrorV1>,
        cleanup_retry_error: Option<PublicationErrorV1>,
        operation_first_cause: Option<PublicationCauseV1>,
        operation_dominant_cause: Option<PublicationCauseV1>,
        cleanup_first_cause: Option<PublicationCauseV1>,
        cleanup_dominant_cause: Option<PublicationCauseV1>,
        logical_length: u64,
        physical_length: u64,
        physical_first_byte: Option<u8>,
        bytes_read: u64,
        read_calls: u64,
        bytes_written: u64,
        preparation_bytes: u64,
        preparation_entries: u64,
        immutable_bytes: u64,
        immutable_entries: u64,
        storage_bytes_requested: u64,
        storage_bytes_reserved: u64,
        storage_bytes_released: u64,
        storage_bytes_committed: u64,
        storage_bytes_retained: u64,
        storage_inodes_requested: u64,
        storage_inodes_reserved: u64,
        storage_inodes_released: u64,
        storage_inodes_committed: u64,
        storage_inodes_retained: u64,
        operation_slots: u64,
        operation_active: u64,
        operation_queue: (u64, u64, u64),
        storage_active: (u64, u64, u64),
        invalidated: bool,
        stale_invalidated: bool,
        reopen_invalidated: bool,
        root_usable: bool,
        stale_usable: bool,
        reopen_usable: bool,
        zero_forbidden_work: bool,
    }

    impl OperationSpoolFaultObservationV1 {
        pub const fn operation_error(self) -> Option<PublicationErrorV1> {
            self.operation_error
        }

        pub const fn cleanup_error(self) -> Option<PublicationErrorV1> {
            self.cleanup_error
        }

        pub const fn cleanup_retry_error(self) -> Option<PublicationErrorV1> {
            self.cleanup_retry_error
        }

        pub const fn operation_first_cause(self) -> Option<PublicationCauseV1> {
            self.operation_first_cause
        }

        pub const fn operation_dominant_cause(self) -> Option<PublicationCauseV1> {
            self.operation_dominant_cause
        }

        pub const fn cleanup_first_cause(self) -> Option<PublicationCauseV1> {
            self.cleanup_first_cause
        }

        pub const fn cleanup_dominant_cause(self) -> Option<PublicationCauseV1> {
            self.cleanup_dominant_cause
        }

        pub const fn logical_length(self) -> u64 {
            self.logical_length
        }

        pub const fn physical_length(self) -> u64 {
            self.physical_length
        }

        pub const fn physical_first_byte(self) -> Option<u8> {
            self.physical_first_byte
        }

        pub const fn direct_storage_observation(self) -> (u64, u64, u64) {
            (self.bytes_read, self.read_calls, self.bytes_written)
        }

        pub const fn preparation_bytes(self) -> u64 {
            self.preparation_bytes
        }

        pub const fn preparation_entries(self) -> u64 {
            self.preparation_entries
        }

        pub const fn immutable_bytes(self) -> u64 {
            self.immutable_bytes
        }

        pub const fn immutable_entries(self) -> u64 {
            self.immutable_entries
        }

        pub const fn storage_bytes_requested(self) -> u64 {
            self.storage_bytes_requested
        }

        pub const fn storage_bytes_reserved(self) -> u64 {
            self.storage_bytes_reserved
        }

        pub const fn storage_bytes_released(self) -> u64 {
            self.storage_bytes_released
        }

        pub const fn storage_bytes_committed(self) -> u64 {
            self.storage_bytes_committed
        }

        pub const fn storage_bytes_retained(self) -> u64 {
            self.storage_bytes_retained
        }

        pub const fn storage_inodes_requested(self) -> u64 {
            self.storage_inodes_requested
        }

        pub const fn storage_inodes_reserved(self) -> u64 {
            self.storage_inodes_reserved
        }

        pub const fn storage_inodes_released(self) -> u64 {
            self.storage_inodes_released
        }

        pub const fn storage_inodes_committed(self) -> u64 {
            self.storage_inodes_committed
        }

        pub const fn storage_inodes_retained(self) -> u64 {
            self.storage_inodes_retained
        }

        pub const fn operation_slots(self) -> u64 {
            self.operation_slots
        }

        pub const fn operation_active(self) -> u64 {
            self.operation_active
        }

        pub const fn operation_queue(self) -> (u64, u64, u64) {
            self.operation_queue
        }

        pub const fn storage_active(self) -> (u64, u64, u64) {
            self.storage_active
        }

        pub const fn invalidated(self) -> bool {
            self.invalidated
        }

        pub const fn stale_invalidated(self) -> bool {
            self.stale_invalidated
        }

        pub const fn reopen_invalidated(self) -> bool {
            self.reopen_invalidated
        }

        pub const fn usable_handles(self) -> (bool, bool, bool) {
            (self.root_usable, self.stale_usable, self.reopen_usable)
        }

        pub const fn root_usable(self) -> bool {
            self.root_usable
        }

        pub const fn stale_usable(self) -> bool {
            self.stale_usable
        }

        pub const fn reopen_usable(self) -> bool {
            self.reopen_usable
        }

        pub const fn zero_forbidden_work(self) -> bool {
            self.zero_forbidden_work
        }
    }

    fn preparation_file_length(root: &Path) -> u64 {
        fs::read_dir(root.join("preparation"))
            .expect("operation-spool preparation directory")
            .next()
            .expect("operation-spool preparation entry")
            .expect("operation-spool preparation entry metadata")
            .metadata()
            .expect("operation-spool preparation metadata")
            .len()
    }

    fn preparation_file_first_byte(root: &Path) -> Option<u8> {
        let path = fs::read_dir(root.join("preparation"))
            .ok()?
            .next()?
            .ok()?
            .path();
        fs::read(path).ok()?.first().copied()
    }

    fn observe_operation_spool_fault(
        root: &Path,
        cas: &FsCasV1,
        stale: &FsCasV1,
        operation_error: Option<FsCasErrorV1>,
        cleanup_error: Option<FsCasErrorV1>,
        cleanup_retry_error: Option<FsCasErrorV1>,
        logical_length: u64,
        physical_length: u64,
        physical_first_byte: Option<u8>,
        direct_storage: (u64, u64, u64),
        counters: &OperationCountersV1,
    ) -> OperationSpoolFaultObservationV1 {
        let (operation_first_cause, operation_dominant_cause) = operation_error
            .map(publication_causes_v1)
            .map(|(first, dominant)| (Some(first), Some(dominant)))
            .unwrap_or((None, None));
        let (cleanup_first_cause, cleanup_dominant_cause) = cleanup_error
            .map(publication_causes_v1)
            .map(|(first, dominant)| (Some(first), Some(dominant)))
            .unwrap_or((None, None));
        let (preparation_bytes, preparation_entries) = directory_usage(&root.join("preparation"));
        let (immutable_bytes, immutable_entries) = immutable_usage(root);
        let reopen = FsCasV1::open_existing(root);
        OperationSpoolFaultObservationV1 {
            operation_error: operation_error.map(publication_error_v1),
            cleanup_error: cleanup_error.map(publication_error_v1),
            cleanup_retry_error: cleanup_retry_error.map(publication_error_v1),
            operation_first_cause,
            operation_dominant_cause,
            cleanup_first_cause,
            cleanup_dominant_cause,
            logical_length,
            physical_length,
            physical_first_byte,
            bytes_read: direct_storage.0,
            read_calls: direct_storage.1,
            bytes_written: direct_storage.2,
            preparation_bytes,
            preparation_entries,
            immutable_bytes,
            immutable_entries,
            storage_bytes_requested: counters.storage_bytes_requested,
            storage_bytes_reserved: counters.storage_bytes_reserved,
            storage_bytes_released: counters.storage_bytes_released,
            storage_bytes_committed: counters.storage_bytes_committed,
            storage_bytes_retained: counters.storage_bytes_retained,
            storage_inodes_requested: counters.storage_inodes_requested,
            storage_inodes_reserved: counters.storage_inodes_reserved,
            storage_inodes_released: counters.storage_inodes_released,
            storage_inodes_committed: counters.storage_inodes_committed,
            storage_inodes_retained: counters.storage_inodes_retained,
            operation_slots: cas.operation_admitted_slots_v1(),
            operation_active: cas.operation_admission_active_for_test_v1(),
            operation_queue: cas.operation_admission_queue_for_test_v1(),
            storage_active: cas.storage_admission_active_for_test_v1(),
            invalidated: matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            stale_invalidated: matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            reopen_invalidated: matches!(
                &reopen,
                Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy)
            ),
            root_usable: cas.occupied().is_ok(),
            stale_usable: stale.occupied().is_ok(),
            reopen_usable: reopen.is_ok(),
            zero_forbidden_work: counters.has_zero_forbidden_work(),
        }
    }

    #[derive(Default)]
    struct OperationSpoolWriteObservationOverflowControl {
        injected: bool,
    }

    impl CdcControlV1 for OperationSpoolWriteObservationOverflowControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for OperationSpoolWriteObservationOverflowControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_operation_spool_write_observation_overflow(&mut self) -> bool {
            if self.injected {
                false
            } else {
                self.injected = true;
                true
            }
        }
    }

    #[derive(Default)]
    struct CountedPackReadObservationOverflowControl {
        injected: bool,
    }

    impl CdcControlV1 for CountedPackReadObservationOverflowControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for CountedPackReadObservationOverflowControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_counted_pack_read_observation_overflow(&mut self) -> bool {
            if self.injected {
                false
            } else {
                self.injected = true;
                true
            }
        }
    }

    #[derive(Default)]
    struct SameCarrierComparisonObservationOverflowControl {
        injected: bool,
    }

    impl CdcControlV1 for SameCarrierComparisonObservationOverflowControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for SameCarrierComparisonObservationOverflowControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_same_carrier_comparison_observation_overflow(&mut self) -> bool {
            if self.injected {
                false
            } else {
                self.injected = true;
                true
            }
        }
    }

    #[derive(Default)]
    struct PostAdmissionCarrierTallyOverflowControl {
        injected: bool,
    }

    impl CdcControlV1 for PostAdmissionCarrierTallyOverflowControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for PostAdmissionCarrierTallyOverflowControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_post_admission_carrier_tally_overflow(&mut self) -> bool {
            if self.injected {
                false
            } else {
                self.injected = true;
                true
            }
        }
    }

    struct PackObjectDispositionOverflowControl {
        target_created: bool,
        injected: bool,
    }

    impl CdcControlV1 for PackObjectDispositionOverflowControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for PackObjectDispositionOverflowControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_pack_object_disposition_overflow(&mut self, created: bool) -> bool {
            if self.injected || created != self.target_created {
                false
            } else {
                self.injected = true;
                true
            }
        }
    }

    #[derive(Default)]
    struct EquivalentCreateTraceControl {
        boundaries: Vec<FsCasBoundaryV1>,
        fail_marker_hard_link: bool,
        marker_hard_link_failed: bool,
    }

    impl CdcControlV1 for EquivalentCreateTraceControl {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for EquivalentCreateTraceControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            self.boundaries.push(boundary);
        }

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
            if self.fail_marker_hard_link
                && !self.marker_hard_link_failed
                && boundary == FsCasFilesystemBoundaryV1::MarkerHardLink
            {
                self.marker_hard_link_failed = true;
                Some(FsCasErrorV1::Filesystem(
                    crate::cas::FsCasFilesystemFailureV1::NoSpace,
                ))
            } else {
                None
            }
        }
    }

    struct ObserveClosureMarkerLockScope {
        cas: FsCasV1,
        observed: bool,
        visibility_available: bool,
        publication_available: bool,
        closure_phase: bool,
        visibility_acquisitions: u64,
        visibility_releases: u64,
        publication_acquisitions: u64,
        publication_releases: u64,
        closure_publication_acquisitions: u64,
        closure_publication_releases: u64,
    }

    impl CdcControlV1 for ObserveClosureMarkerLockScope {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for ObserveClosureMarkerLockScope {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary == FsCasBoundaryV1::BeforeClosureMarkerPublication {
                self.observed = true;
                self.closure_phase = true;
                self.visibility_available = self.cas.visibility_lock_available_for_test_v1();
                self.publication_available = self.cas.publication_lock_available_for_test_v1();
            }
            match boundary {
                FsCasBoundaryV1::VisibilityLockAcquired => self.visibility_acquisitions += 1,
                FsCasBoundaryV1::VisibilityLockReleased => self.visibility_releases += 1,
                FsCasBoundaryV1::PublicationLockAcquired => {
                    self.publication_acquisitions += 1;
                    if self.closure_phase {
                        self.closure_publication_acquisitions += 1;
                    }
                }
                FsCasBoundaryV1::PublicationLockReleased => {
                    self.publication_releases += 1;
                    if self.closure_phase {
                        self.closure_publication_releases += 1;
                    }
                }
                _ => {}
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    pub fn operation_spool_resize_fault_v1(
        root: &Path,
        fail_invalidation: bool,
    ) -> OperationSpoolFaultObservationV1 {
        const ORIGINAL_BYTES: u64 = 17;
        const TRUNCATED_BYTES: u64 = 9;

        let (cas, stale) = new_fault_root(root);
        let mut counters = OperationCountersV1::default();
        let mut admission_control = ContinueFaultControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x8fe,
                &mut counters,
                &mut admission_control,
            )
            .expect("operation-spool resize capability");
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(ORIGINAL_BYTES, 0, 1, 0).unwrap())
            .expect("operation-spool resize envelope");
        let token = capability
            .storage_token_v1()
            .expect("operation-spool resize token");
        let mut spool = cas
            .begin_operation_spool_borrowed_v1("resize-accounting", token, &mut admission_control)
            .expect("operation-spool resize handle");
        spool
            .initialize_zeroed_len_controlled_v1(ORIGINAL_BYTES, &mut admission_control)
            .expect("operation-spool resize initialization");
        cas.clear_active_preparation_bytes_for_test_v1();

        let mut control = DirectInvalidationControl {
            fail: fail_invalidation,
            attempts: 0,
        };
        let operation_error = spool
            .set_len_controlled_v1(TRUNCATED_BYTES, &mut control)
            .err();
        let logical_length = spool.logical_len_for_test_v1();
        let physical_length = preparation_file_length(root);
        let physical_first_byte = preparation_file_first_byte(root);
        let cleanup_error = spool.cleanup_controlled_v1(&mut admission_control).err();
        let cleanup_retry_error = spool.cleanup_controlled_v1(&mut admission_control).err();
        drop(spool);
        capability
            .finish_terminal_v1(false, &mut counters, &mut admission_control)
            .expect("operation-spool resize terminal");
        observe_operation_spool_fault(
            root,
            &cas,
            &stale,
            operation_error,
            cleanup_error,
            cleanup_retry_error,
            logical_length,
            physical_length,
            physical_first_byte,
            (0, 0, 0),
            &counters,
        )
    }

    pub fn operation_spool_write_observation_overflow_v1(
        root: &Path,
    ) -> OperationSpoolFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let mut counters = OperationCountersV1::default();
        let mut admission_control = ContinueFaultControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x8ff,
                &mut counters,
                &mut admission_control,
            )
            .expect("operation-spool write capability");
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(1, 0, 1, 0).unwrap())
            .expect("operation-spool write envelope");
        let token = capability
            .storage_token_v1()
            .expect("operation-spool write token");
        let mut spool = cas
            .begin_operation_spool_borrowed_v1("write-observation", token, &mut admission_control)
            .expect("operation-spool write handle");
        spool
            .initialize_zeroed_len_controlled_v1(1, &mut admission_control)
            .expect("operation-spool write initialization");
        let mut control = OperationSpoolWriteObservationOverflowControl::default();
        let operation_error = spool
            .write_exact_at_controlled_v1(0, &[0x5a], &mut control)
            .err();
        assert!(
            control.injected,
            "operation-spool write control did not fire"
        );
        let physical_length = preparation_file_length(root);
        let physical_first_byte = preparation_file_first_byte(root);
        let direct_storage = spool.direct_storage_observation();
        let cleanup_error = spool.cleanup_controlled_v1(&mut admission_control).err();
        let cleanup_retry_error = spool.cleanup_controlled_v1(&mut admission_control).err();
        drop(spool);
        capability
            .finish_terminal_v1(false, &mut counters, &mut admission_control)
            .expect("operation-spool write terminal");
        observe_operation_spool_fault(
            root,
            &cas,
            &stale,
            operation_error,
            cleanup_error,
            cleanup_retry_error,
            1,
            physical_length,
            physical_first_byte,
            direct_storage,
            &counters,
        )
    }

    pub fn operation_spool_read_observation_overflow_v1(
        root: &Path,
    ) -> OperationSpoolFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let mut counters = OperationCountersV1::default();
        let mut admission_control = ContinueFaultControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x900,
                &mut counters,
                &mut admission_control,
            )
            .expect("operation-spool read capability");
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(1, 0, 1, 0).unwrap())
            .expect("operation-spool read envelope");
        let token = capability
            .storage_token_v1()
            .expect("operation-spool read token");
        let mut spool = cas
            .begin_operation_spool_borrowed_v1("read-observation", token, &mut admission_control)
            .expect("operation-spool read handle");
        spool
            .initialize_zeroed_len_controlled_v1(1, &mut admission_control)
            .expect("operation-spool read initialization");
        spool
            .write_exact_at_controlled_v1(0, &[0x5a], &mut admission_control)
            .expect("operation-spool read seed");
        cas.seed_next_operation_spool_read_observation_for_test_v1(73, u64::MAX);
        let mut destination = [0_u8; 1];
        let operation_error = spool.read_exact_at(0, &mut destination).err();
        assert_eq!(
            destination,
            [0x5a],
            "operation-spool read lost committed byte"
        );
        let direct_storage = spool.direct_storage_observation();
        let physical_length = preparation_file_length(root);
        let physical_first_byte = preparation_file_first_byte(root);
        let cleanup_error = spool.cleanup_controlled_v1(&mut admission_control).err();
        let cleanup_retry_error = spool.cleanup_controlled_v1(&mut admission_control).err();
        drop(spool);
        capability
            .finish_terminal_v1(false, &mut counters, &mut admission_control)
            .expect("operation-spool read terminal");
        observe_operation_spool_fault(
            root,
            &cas,
            &stale,
            operation_error,
            cleanup_error,
            cleanup_retry_error,
            1,
            physical_length,
            physical_first_byte,
            direct_storage,
            &counters,
        )
    }

    pub fn marker_cleanup_length_fault_v1(
        root: &Path,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let mut counters = OperationCountersV1::default();
        let mut admission_control = ContinueFaultControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x906,
                &mut counters,
                &mut admission_control,
            )
            .expect("marker cleanup length capability");
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(9, 0, 1, 0).unwrap())
            .expect("marker cleanup length envelope");
        let token = capability
            .storage_token_v1()
            .expect("marker cleanup length token");
        let temporary = cas
            .prepare_test_marker_cleanup_mismatch_v1(token)
            .expect("marker cleanup length fixture");
        let before_length = fs::metadata(&temporary).ok().map(|metadata| metadata.len());
        cas.clear_active_preparation_bytes_for_test_v1();
        let mut control = RestoreMarkerCleanupAccountingControl {
            cas: cas.clone(),
            accounting_restored: false,
            fail_invalidation,
        };
        let cleanup_error = cas
            .cleanup_test_marker_mismatch_borrowed_v1(token, &mut control)
            .err();
        let after_length = fs::metadata(&temporary).ok().map(|metadata| metadata.len());
        let terminal_error = capability
            .finish_terminal_v1(false, &mut counters, &mut admission_control)
            .err();
        let mut observation = observe_direct_fault(
            root,
            &cas,
            &stale,
            cleanup_error,
            terminal_error,
            CreateFaultControlObservation {
                control_fired: control.accounting_restored,
                cleanup_calls: u32::from(control.accounting_restored),
                carrier_installed: false,
                poisoned: false,
            },
            u32::from(control.accounting_restored),
            &counters,
        );
        observation.marker_cleanup_observation = (
            before_length,
            after_length,
            false,
            false,
            false,
            control.accounting_restored,
        );
        observation
    }

    pub fn marker_cleanup_metadata_fault_v1(
        root: &Path,
        wrong_type: bool,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let mut counters = OperationCountersV1::default();
        let mut admission_control = ContinueFaultControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x907,
                &mut counters,
                &mut admission_control,
            )
            .expect("marker cleanup metadata capability");
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(8, 0, 1, 0).unwrap())
            .expect("marker cleanup metadata envelope");
        let token = capability
            .storage_token_v1()
            .expect("marker cleanup metadata token");
        let temporary = cas
            .prepare_test_marker_cleanup_file_v1(token, 8)
            .expect("marker cleanup metadata fixture");
        fs::remove_file(&temporary).expect("remove marker cleanup metadata fixture");
        if wrong_type {
            fs::create_dir(&temporary).expect("replace marker cleanup metadata fixture");
        }
        let mut control = DirectInvalidationControl {
            fail: fail_invalidation,
            attempts: 0,
        };
        let cleanup_error = cas
            .cleanup_test_marker_mismatch_borrowed_v1(token, &mut control)
            .err();
        let physical = fs::symlink_metadata(&temporary);
        let is_directory = physical
            .as_ref()
            .is_ok_and(|metadata| metadata.file_type().is_dir());
        let is_missing = physical
            .as_ref()
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound);
        let terminal_error = capability
            .finish_terminal_v1(false, &mut counters, &mut admission_control)
            .err();
        let mut observation = observe_direct_fault(
            root,
            &cas,
            &stale,
            cleanup_error,
            terminal_error,
            CreateFaultControlObservation {
                control_fired: true,
                cleanup_calls: 1,
                carrier_installed: false,
                poisoned: false,
            },
            control.attempts,
            &counters,
        );
        observation.marker_cleanup_observation =
            (None, None, is_directory, is_missing, false, false);
        observation
    }

    #[cfg(unix)]
    pub fn marker_cleanup_unlink_fault_v1(
        root: &Path,
        mode: MarkerCleanupUnlinkModeV1,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let mut counters = OperationCountersV1::default();
        let mut admission_control = ContinueFaultControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x908,
                &mut counters,
                &mut admission_control,
            )
            .expect("marker cleanup unlink capability");
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(8, 0, 1, 0).unwrap())
            .expect("marker cleanup unlink envelope");
        let token = capability
            .storage_token_v1()
            .expect("marker cleanup unlink token");
        let temporary = cas
            .prepare_test_marker_cleanup_file_v1(token, 8)
            .expect("marker cleanup unlink fixture");
        let preparation = root.join("preparation");
        let mut control = MarkerCleanupUnlinkControl {
            held_preparation: root.join("preparation-unlink-held"),
            preparation,
            mode,
            armed: false,
            restored: false,
            fail_invalidation,
        };
        let cleanup_error = cas
            .cleanup_test_marker_mismatch_borrowed_v1(token, &mut control)
            .err();
        let after_length = fs::metadata(&temporary).ok().map(|metadata| metadata.len());
        let terminal_error = capability
            .finish_terminal_v1(false, &mut counters, &mut admission_control)
            .err();
        let mut observation = observe_direct_fault(
            root,
            &cas,
            &stale,
            cleanup_error,
            terminal_error,
            CreateFaultControlObservation {
                control_fired: control.armed,
                cleanup_calls: u32::from(control.armed),
                carrier_installed: false,
                poisoned: false,
            },
            u32::from(control.fail_invalidation && control.armed),
            &counters,
        );
        observation.marker_cleanup_observation = (
            None,
            after_length,
            false,
            false,
            control.armed,
            control.restored,
        );
        observation
    }

    pub fn marker_cleanup_post_unlink_fault_v1(
        root: &Path,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let mut counters = OperationCountersV1::default();
        let mut admission_control = ContinueFaultControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x909,
                &mut counters,
                &mut admission_control,
            )
            .expect("marker cleanup post-unlink capability");
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(8, 0, 1, 0).unwrap())
            .expect("marker cleanup post-unlink envelope");
        let token = capability
            .storage_token_v1()
            .expect("marker cleanup post-unlink token");
        let temporary = cas
            .prepare_test_marker_cleanup_file_v1(token, 8)
            .expect("marker cleanup post-unlink fixture");
        cas.fail_next_preparation_remove_for_test_v1();
        let mut control = DirectInvalidationControl {
            fail: fail_invalidation,
            attempts: 0,
        };
        let cleanup_error = cas
            .cleanup_test_marker_mismatch_borrowed_v1(token, &mut control)
            .err();
        let is_missing = fs::symlink_metadata(&temporary)
            .as_ref()
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound);
        let terminal_error = capability
            .finish_terminal_v1(false, &mut counters, &mut admission_control)
            .err();
        let mut observation = observe_direct_fault(
            root,
            &cas,
            &stale,
            cleanup_error,
            terminal_error,
            CreateFaultControlObservation {
                control_fired: true,
                cleanup_calls: 1,
                carrier_installed: false,
                poisoned: false,
            },
            control.attempts,
            &counters,
        );
        observation.marker_cleanup_observation = (None, None, false, is_missing, false, false);
        observation
    }

    pub fn private_pack_precharge_poison_v1(
        root: &Path,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let mut counters = OperationCountersV1::default();
        let mut admission_control = ContinueFaultControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x8fa,
                &mut counters,
                &mut admission_control,
            )
            .expect("private-pack precharge capability");
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(0, 0, 1, 0).unwrap())
            .expect("private-pack precharge envelope");
        let token = capability.storage_token_v1().expect("private-pack token");
        let mut private_pack = cas
            .begin_private_pack_borrowed_v1(token)
            .expect("private-pack handle");
        cas.poison_storage_admission_for_test_v1();
        let mut control = DirectInvalidationControl {
            fail: fail_invalidation,
            attempts: 0,
        };
        let operation_error = private_pack
            .begin_direct_controlled_v1(128, &mut control)
            .err()
            .and_then(|_| private_pack.take_first_error_typed_v1());
        let operation_error = operation_error.or_else(|| private_pack.take_first_error_typed_v1());
        let _ = private_pack.cleanup_controlled_v1(&mut control);
        drop(private_pack);
        let terminal_error = capability
            .finish_terminal_v1(false, &mut counters, &mut control)
            .err();
        observe_direct_fault(
            root,
            &cas,
            &stale,
            operation_error,
            terminal_error,
            CreateFaultControlObservation {
                control_fired: fail_invalidation,
                cleanup_calls: 0,
                carrier_installed: false,
                poisoned: true,
            },
            control.attempts,
            &counters,
        )
    }

    pub fn private_pack_create_failure_v1(
        root: &Path,
        failure: FilesystemFaultFailureV1,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let error = FsCasErrorV1::Filesystem(match failure {
            FilesystemFaultFailureV1::WriteFailure => FsCasFilesystemFailureV1::WriteFailure,
            FilesystemFaultFailureV1::PermissionDenied => {
                FsCasFilesystemFailureV1::PermissionDenied
            }
            _ => panic!("private-pack adapter accepts only write or permission failures"),
        });
        let (cas, stale) = new_fault_root(root);
        let mut counters = OperationCountersV1::default();
        let mut admission_control = ContinueFaultControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x8fb,
                &mut counters,
                &mut admission_control,
            )
            .expect("private-pack create capability");
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(0, 0, 1, 0).unwrap())
            .expect("private-pack create envelope");
        let token = capability
            .storage_token_v1()
            .expect("private-pack create token");
        let mut private_pack = cas
            .begin_private_pack_borrowed_v1(token)
            .expect("private-pack create handle");
        let mut control = DirectPrivatePackCreateControl {
            cas: cas.clone(),
            error,
            fired: false,
            fail_invalidation,
            invalidation_attempts: 0,
        };
        let operation_error = private_pack
            .begin_direct_controlled_v1(128, &mut control)
            .err()
            .and_then(|_| private_pack.take_first_error_typed_v1());
        let operation_error = operation_error.or_else(|| private_pack.take_first_error_typed_v1());
        let _ = private_pack.cleanup_controlled_v1(&mut control);
        drop(private_pack);
        let terminal_error = capability
            .finish_terminal_v1(false, &mut counters, &mut control)
            .err();
        observe_direct_fault(
            root,
            &cas,
            &stale,
            operation_error,
            terminal_error,
            CreateFaultControlObservation {
                control_fired: control.fired,
                cleanup_calls: 1,
                carrier_installed: false,
                poisoned: false,
            },
            control.invalidation_attempts,
            &counters,
        )
    }

    pub fn marker_create_fault_v1(
        root: &Path,
        failure: FilesystemFaultFailureV1,
        break_accounting: bool,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let error = FsCasErrorV1::Filesystem(match failure {
            FilesystemFaultFailureV1::WriteFailure => FsCasFilesystemFailureV1::WriteFailure,
            FilesystemFaultFailureV1::PermissionDenied => {
                FsCasFilesystemFailureV1::PermissionDenied
            }
            _ => panic!("marker adapter accepts only write or permission failures"),
        });
        let (cas, stale) = new_fault_root(root);
        let mut counters = OperationCountersV1::default();
        let mut admission_control = ContinueFaultControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x900,
                &mut counters,
                &mut admission_control,
            )
            .expect("marker create capability");
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(0, 0, 1, 0).unwrap())
            .expect("marker create envelope");
        let token = capability.storage_token_v1().expect("marker create token");
        let mut control = DirectMarkerCreateControl {
            cas: cas.clone(),
            error,
            break_accounting,
            fired: false,
            fail_invalidation,
            invalidation_attempts: 0,
        };
        let operation_error = cas
            .publish_test_marker_borrowed_v1(token, &mut control)
            .err();
        let terminal_error = capability
            .finish_terminal_v1(false, &mut counters, &mut admission_control)
            .err();
        observe_direct_fault(
            root,
            &cas,
            &stale,
            operation_error,
            terminal_error,
            CreateFaultControlObservation {
                control_fired: control.fired,
                cleanup_calls: u32::from(break_accounting),
                carrier_installed: false,
                poisoned: break_accounting,
            },
            control.invalidation_attempts,
            &counters,
        )
    }

    pub fn marker_length_precharge_v1(
        root: &Path,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let mut counters = OperationCountersV1::default();
        let mut admission_control = ContinueFaultControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x901,
                &mut counters,
                &mut admission_control,
            )
            .expect("marker length capability");
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(8, 0, 1, 0).unwrap())
            .expect("marker length envelope");
        let token = capability.storage_token_v1().expect("marker length token");
        let mut control = DirectMarkerLengthControl {
            cas: cas.clone(),
            corrupted: false,
            restored_for_cleanup: false,
            payload_or_link_seen: false,
            fail_invalidation,
            invalidation_attempts: 0,
        };
        let operation_error = cas
            .publish_test_marker_borrowed_v1(token, &mut control)
            .err();
        let terminal_error = capability
            .finish_terminal_v1(false, &mut counters, &mut admission_control)
            .err();
        let mut observation = observe_direct_fault(
            root,
            &cas,
            &stale,
            operation_error,
            terminal_error,
            CreateFaultControlObservation {
                control_fired: control.corrupted && control.restored_for_cleanup,
                cleanup_calls: u32::from(control.restored_for_cleanup),
                carrier_installed: false,
                poisoned: false,
            },
            control.invalidation_attempts,
            &counters,
        );
        observation.marker_fault_boundaries = (
            control.corrupted,
            control.restored_for_cleanup,
            control.payload_or_link_seen,
            false,
            false,
        );
        observation
    }

    pub fn marker_immutable_precharge_v1(
        root: &Path,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let mut counters = OperationCountersV1::default();
        let mut admission_control = ContinueFaultControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x902,
                &mut counters,
                &mut admission_control,
            )
            .expect("marker immutable capability");
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(8, 0, 1, 0).unwrap())
            .expect("marker immutable envelope");
        let token = capability
            .storage_token_v1()
            .expect("marker immutable token");
        let mut control = DirectMarkerImmutableControl {
            marker_write_seen: false,
            marker_link_boundary_seen: false,
            fail_invalidation,
            invalidation_attempts: 0,
        };
        let operation_error = cas
            .publish_test_marker_borrowed_v1(token, &mut control)
            .err();
        let terminal_error = capability
            .finish_terminal_v1(false, &mut counters, &mut admission_control)
            .err();
        let mut observation = observe_direct_fault(
            root,
            &cas,
            &stale,
            operation_error,
            terminal_error,
            CreateFaultControlObservation {
                control_fired: control.marker_write_seen && control.marker_link_boundary_seen,
                cleanup_calls: 0,
                carrier_installed: false,
                poisoned: false,
            },
            control.invalidation_attempts,
            &counters,
        );
        observation.marker_fault_boundaries = (
            false,
            false,
            false,
            control.marker_write_seen,
            control.marker_link_boundary_seen,
        );
        observation
    }

    pub fn equal_marker_incumbent_rollback_v1(
        root: &Path,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let cas = FsCasV1::create_new(root).expect("marker incumbent root");
        let mut setup_counters = OperationCountersV1::default();
        let mut setup_control = ContinueFaultControl;
        let mut setup = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x903,
                &mut setup_counters,
                &mut setup_control,
            )
            .expect("marker incumbent setup capability");
        setup
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(8, 8, 1, 1).unwrap())
            .expect("marker incumbent setup envelope");
        let setup_token = setup
            .storage_token_v1()
            .expect("marker incumbent setup token");
        cas.publish_test_marker_borrowed_v1(setup_token, &mut setup_control)
            .expect("marker incumbent setup publication");
        setup
            .finish_terminal_v1(true, &mut setup_counters, &mut setup_control)
            .expect("marker incumbent setup terminal");
        let marker_path = root.join("closures/test-marker");
        let incumbent_before = fs::read(&marker_path)
            .ok()
            .and_then(|bytes| bytes.try_into().ok());

        let stale = FsCasV1::open_existing(root).expect("marker incumbent stale owner");
        let mut counters = OperationCountersV1::default();
        let mut admission_control = ContinueFaultControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x904,
                &mut counters,
                &mut admission_control,
            )
            .expect("marker incumbent capability");
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(8, 8, 1, 1).unwrap())
            .expect("marker incumbent envelope");
        let token = capability
            .storage_token_v1()
            .expect("marker incumbent token");
        cas.poison_next_immutable_remove_for_test_v1();
        let mut control = DirectInvalidationControl {
            fail: fail_invalidation,
            attempts: 0,
        };
        let operation_error = cas
            .publish_test_marker_borrowed_v1(token, &mut control)
            .err();
        let incumbent_after = fs::read(&marker_path)
            .ok()
            .and_then(|bytes| bytes.try_into().ok());
        let terminal_error = capability
            .finish_terminal_v1(false, &mut counters, &mut admission_control)
            .err();
        let mut observation = observe_direct_fault(
            root,
            &cas,
            &stale,
            operation_error,
            terminal_error,
            CreateFaultControlObservation {
                control_fired: true,
                cleanup_calls: 0,
                carrier_installed: false,
                poisoned: true,
            },
            control.attempts,
            &counters,
        );
        observation.setup_storage = (
            setup_counters.storage_bytes_requested,
            setup_counters.storage_bytes_reserved,
            setup_counters.storage_bytes_released,
            setup_counters.storage_bytes_committed,
            setup_counters.storage_bytes_retained,
            setup_counters.storage_inodes_requested,
            setup_counters.storage_inodes_reserved,
            setup_counters.storage_inodes_released,
            setup_counters.storage_inodes_committed,
            setup_counters.storage_inodes_retained,
        );
        observation.incumbent_marker_bytes = (incumbent_before, incumbent_after);
        observation
    }

    pub fn marker_hard_link_fault_v1(
        root: &Path,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let mut counters = OperationCountersV1::default();
        let mut admission_control = ContinueFaultControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x905,
                &mut counters,
                &mut admission_control,
            )
            .expect("marker hard-link capability");
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(8, 8, 1, 1).unwrap())
            .expect("marker hard-link envelope");
        let token = capability
            .storage_token_v1()
            .expect("marker hard-link token");
        let closures = root.join("closures");
        fs::remove_dir(&closures).expect("remove closures directory");
        fs::write(&closures, b"not-a-directory").expect("replace closures directory");
        cas.poison_next_immutable_remove_for_test_v1();
        let mut control = DirectInvalidationControl {
            fail: fail_invalidation,
            attempts: 0,
        };
        let operation_error = cas
            .publish_test_marker_borrowed_v1(token, &mut control)
            .err();
        fs::remove_file(&closures).expect("restore closures directory");
        fs::create_dir(&closures).expect("recreate closures directory");
        let terminal_error = capability
            .finish_terminal_v1(false, &mut counters, &mut admission_control)
            .err();
        observe_direct_fault(
            root,
            &cas,
            &stale,
            operation_error,
            terminal_error,
            CreateFaultControlObservation {
                control_fired: true,
                cleanup_calls: 1,
                carrier_installed: false,
                poisoned: true,
            },
            control.attempts,
            &counters,
        )
    }

    pub fn preparation_free_unwind_v1(root: &Path) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = ContinueFaultControl;
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x8fd,
            1,
            PanicDuringPreparationFreeSupplier {
                cas_to_poison: None,
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
            },
            &mut control,
            &mut counters,
        );
        let fault_usage = (
            directory_usage(&root.join("preparation")),
            immutable_usage(root),
        );
        let unwind_authority = (
            cas.operation_admitted_slots_v1(),
            cas.operation_admission_active_for_test_v1(),
            cas.operation_admission_queue_for_test_v1(),
            cas.storage_admission_active_for_test_v1(),
            cas.occupied().is_ok(),
        );
        let followup_bound = Arc::new(AtomicBool::new(false));
        let followup_supply = Arc::new(AtomicBool::new(false));
        let mut followup_counters = OperationCountersV1::default();
        let followup = run_create_fault_attempt(
            &cas,
            0x8fe,
            1,
            CallbackSupplier {
                bound_invoked: Arc::clone(&followup_bound),
                supply_invoked: Arc::clone(&followup_supply),
                len: 1,
            },
            &mut control,
            &mut followup_counters,
        );
        let followup_succeeded = followup.error.is_none() && !followup.panicked;
        let error = attempt.error.or(followup.error);
        let mut observation = observe_create_fault(
            root,
            &cas,
            &stale,
            CreateFaultAttempt {
                error,
                panicked: attempt.panicked,
                panic_payload: attempt.panic_payload,
            },
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            followup_succeeded,
            0,
            0,
            false,
            &counters,
        );
        (
            (
                observation.preparation_bytes,
                observation.preparation_entries,
            ),
            (observation.immutable_bytes, observation.immutable_entries),
        ) = fault_usage;
        observation.unwind_authority = unwind_authority;
        observation.followup_bound_invoked = followup_bound.load(Ordering::Acquire);
        observation.followup_supply_invoked = followup_supply.load(Ordering::Acquire);
        observation.followup_preparation_entries = directory_usage(&root.join("preparation")).1;
        observation.followup_storage = (
            followup_counters.storage_bytes_requested,
            followup_counters.storage_bytes_reserved,
            followup_counters.storage_bytes_released,
            followup_counters.storage_bytes_committed,
            followup_counters.storage_bytes_retained,
            followup_counters.storage_inodes_requested,
            followup_counters.storage_inodes_reserved,
            followup_counters.storage_inodes_released,
            followup_counters.storage_inodes_committed,
            followup_counters.storage_inodes_retained,
        );
        observation.followup_zero_forbidden_work = followup_counters.has_zero_forbidden_work();
        observation
    }

    pub fn preparation_free_terminalization_v1(
        root: &Path,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = PreparationFreeTerminalControl {
            fail_invalidation,
            invalidation_attempts: 0,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x8ff + u64::from(fail_invalidation),
            1,
            PanicDuringPreparationFreeSupplier {
                cas_to_poison: Some(cas.clone()),
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
            },
            &mut control,
            &mut counters,
        );
        observe_create_fault(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            control.invalidation_attempts,
            false,
            &counters,
        )
    }

    pub fn typed_preparation_free_error_v1(root: &Path) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = PanicAfterOperationTerminalReleaseControl {
            unwind_pending: true,
            terminal_hook_calls: 0,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x0011_5520,
            1,
            FailingPreparationFreeSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
            },
            &mut control,
            &mut counters,
        );
        observe_create_fault(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            control.terminal_hook_calls,
            0,
            false,
            &counters,
        )
    }

    pub fn typed_complete_body_error_v1(root: &Path) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let mut control = PanicAfterOperationTerminalReleaseControl {
            unwind_pending: true,
            terminal_hook_calls: 0,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x0011_5521,
            1,
            FailingBodySupplier,
            &mut control,
            &mut counters,
        );
        observe_create_fault(
            root,
            &cas,
            &stale,
            attempt,
            false,
            false,
            false,
            control.terminal_hook_calls,
            0,
            false,
            &counters,
        )
    }

    pub fn typed_complete_global_seen_error_v1(root: &Path) -> CreateFaultObservationV1 {
        const BODY_BYTES: u64 = 64 * 1024;
        let (cas, stale) = new_fault_root(root);
        let mut control = GlobalSeenCounterOverflowControl::default();
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x0011_5524,
            BODY_BYTES + 1,
            FailingAfterBytesSupplier {
                bytes_before_failure: BODY_BYTES,
            },
            &mut control,
            &mut counters,
        );
        observe_create_fault(
            root,
            &cas,
            &stale,
            attempt,
            false,
            false,
            false,
            0,
            0,
            control.injected,
            &counters,
        )
    }

    pub fn typed_complete_storage_counter_error_v1(root: &Path) -> CreateFaultObservationV1 {
        const BODY_BYTES: u64 = 64 * 1024;
        let (cas, stale) = new_fault_root(root);
        let mut control = ContinueFaultControl;
        let mut counters = OperationCountersV1 {
            global_seen_metadata_bytes_written: u64::MAX,
            ..OperationCountersV1::default()
        };
        let attempt = run_create_fault_attempt(
            &cas,
            0x0011_5525,
            BODY_BYTES + 1,
            FailingAfterBytesSupplier {
                bytes_before_failure: BODY_BYTES,
            },
            &mut control,
            &mut counters,
        );
        observe_create_fault(
            root, &cas, &stale, attempt, false, false, false, 0, 0, false, &counters,
        )
    }

    pub fn typed_body_cleanup_dominance_v1(
        root: &Path,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let mut control = FailBodyCleanupTerminalControl {
            preparation_cleanup_injected: false,
            fail_invalidation,
            invalidation_attempts: 0,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x0011_5526 + u64::from(fail_invalidation),
            1,
            FailingBodySupplier,
            &mut control,
            &mut counters,
        );
        observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            false,
            false,
            false,
            0,
            control.invalidation_attempts,
            false,
            CreateFaultControlObservation {
                control_fired: control.preparation_cleanup_injected,
                ..CreateFaultControlObservation::default()
            },
            &counters,
        )
    }

    pub fn private_pack_cleanup_failure_v1(root: &Path) -> CreateFaultObservationV1 {
        const BODY_BYTES: u64 = 64 * 1024 + 17;
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = CancelBeforeCandidateValidationAndFailPrivatePackCleanup {
            cancelled: false,
            cleanup_calls: 0,
            invalidation_attempts: 0,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x0011_5571,
            BODY_BYTES,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: BODY_BYTES,
            },
            &mut control,
            &mut counters,
        );
        observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            control.invalidation_attempts,
            false,
            CreateFaultControlObservation {
                control_fired: control.cancelled,
                cleanup_calls: control.cleanup_calls,
                carrier_installed: false,
                poisoned: false,
            },
            &counters,
        )
    }

    pub fn lifecycle_carrier_cleanup_failure_v1(root: &Path) -> CreateFaultObservationV1 {
        const BODY_BYTES: u64 = 64 * 1024 + 17;
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = CancelAfterCarrierInstallAndFailCleanup {
            cancelled: false,
            carrier_installed: false,
            cleanup_calls: 0,
            invalidation_attempts: 0,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x0011_5572,
            BODY_BYTES,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: BODY_BYTES,
            },
            &mut control,
            &mut counters,
        );
        let observation = observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            control.invalidation_attempts,
            false,
            CreateFaultControlObservation {
                control_fired: control.carrier_installed,
                cleanup_calls: control.cleanup_calls,
                carrier_installed: control.carrier_installed,
                poisoned: false,
            },
            &counters,
        );
        assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
        assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
        assert!(matches!(
            FsCasV1::open_existing(root),
            Err(FsCasErrorV1::Invalidated)
        ));
        let exact_residue_bytes = observation.carrier_bytes();
        assert_eq!(
            counters.unreachable_installed_residue_bytes,
            exact_residue_bytes
        );
        observation
    }

    pub fn carrier_alias_post_unlink_accounting_v1(
        root: &Path,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        const BODY_BYTES: u64 = 64 * 1024 + 17;
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = FailCarrierAliasPreparationAccountingControl {
            cas: cas.clone(),
            armed: false,
            invalidation_attempts: 0,
            fail_invalidation,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x0011_5573 + u64::from(fail_invalidation),
            BODY_BYTES,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: BODY_BYTES,
            },
            &mut control,
            &mut counters,
        );
        let observation = observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            control.invalidation_attempts,
            false,
            CreateFaultControlObservation {
                control_fired: control.armed,
                cleanup_calls: 0,
                carrier_installed: false,
                poisoned: false,
            },
            &counters,
        );
        assert_eq!((&cas).operation_admitted_slots_v1(), 0);
        assert_eq!((&cas).operation_admission_active_for_test_v1(), 0);
        assert_eq!((&cas).operation_admission_queue_for_test_v1(), (0, 0, 0));
        assert_eq!((&cas).storage_admission_active_for_test_v1(), (0, 0, 0));
        assert_eq!(fs::read_dir(root.join("preparation")).unwrap().count(), 0);
        assert_storage_equations(&counters);
        assert_eq!(
            counters.storage_bytes_requested,
            counters.storage_bytes_reserved
        );
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
        observation
    }

    pub fn carrier_accounting_poison_v1(
        root: &Path,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        const BODY_BYTES: u64 = 64 * 1024 + 17;
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = PoisonStorageAndCancelAfterCarrierInstall {
            cas: cas.clone(),
            cancelled: false,
            carrier_installed: false,
            poisoned: false,
            invalidation_attempts: 0,
            fail_invalidation,
        };
        let mut counters = OperationCountersV1::default();
        let attempt = run_create_fault_attempt(
            &cas,
            0x0011_5573 + u64::from(fail_invalidation),
            BODY_BYTES,
            CallbackSupplier {
                bound_invoked: Arc::clone(&bound_invoked),
                supply_invoked: Arc::clone(&supply_invoked),
                len: BODY_BYTES,
            },
            &mut control,
            &mut counters,
        );
        let observation = observe_create_fault_with_control(
            root,
            &cas,
            &stale,
            attempt,
            bound_invoked.load(Ordering::Acquire),
            supply_invoked.load(Ordering::Acquire),
            false,
            0,
            control.invalidation_attempts,
            false,
            CreateFaultControlObservation {
                control_fired: control.carrier_installed,
                cleanup_calls: 0,
                carrier_installed: control.carrier_installed,
                poisoned: control.poisoned,
            },
            &counters,
        );
        assert_eq!((&cas).operation_admitted_slots_v1(), 0);
        assert_eq!((&cas).operation_admission_active_for_test_v1(), 0);
        assert_eq!((&cas).operation_admission_queue_for_test_v1(), (0, 0, 0));
        assert_eq!((&cas).storage_admission_active_for_test_v1(), (0, 0, 0));
        assert_eq!(fs::read_dir(root.join("preparation")).unwrap().count(), 0);
        assert_storage_equations(&counters);
        assert_eq!(
            counters.storage_bytes_requested,
            counters.storage_bytes_reserved
        );
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
        observation
    }

    #[derive(Clone, Copy)]
    pub struct PostInstallCleanupRequestV1<'a> {
        root: &'a Path,
        name: &'a [u8],
        base: &'a [u8],
        replacement: &'a [u8],
    }

    impl<'a> PostInstallCleanupRequestV1<'a> {
        pub const fn new(
            root: &'a Path,
            name: &'a [u8],
            base: &'a [u8],
            replacement: &'a [u8],
        ) -> Self {
            Self {
                root,
                name,
                base,
                replacement,
            }
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct PostInstallCleanupObservationV1 {
        error: Option<PublicationErrorV1>,
        first_cause: Option<PublicationCauseV1>,
        dominant_cause: Option<PublicationCauseV1>,
        after_catalog_publication: bool,
        publication_poll_passed: bool,
        cleanup_panicked: bool,
        operation_slots: u64,
        operation_active: u64,
        storage_active_operations: u64,
        storage_active_bytes: u64,
        storage_active_inodes: u64,
        new_carrier_entries: u64,
        new_carrier_bytes: u64,
        preparation_bytes: u64,
        preparation_inodes: u64,
        locator_delta_bytes: u64,
        locator_delta_inodes: u64,
        catalog_delta_bytes: u64,
        catalog_delta_inodes: u64,
        closure_delta_bytes: u64,
        closure_delta_inodes: u64,
        immutable_delta_bytes: u64,
        immutable_delta_inodes: u64,
        residue_bytes: u64,
        storage_bytes_requested: u64,
        storage_bytes_reserved: u64,
        storage_bytes_released: u64,
        storage_bytes_committed: u64,
        storage_bytes_retained: u64,
        storage_inodes_requested: u64,
        storage_inodes_reserved: u64,
        storage_inodes_released: u64,
        storage_inodes_committed: u64,
        storage_inodes_retained: u64,
        invalidated: bool,
        stale_invalidated: bool,
        reopen_invalidated: bool,
        zero_forbidden_work: bool,
    }

    macro_rules! post_install_getters {
        ($($name:ident: $field:ident => $ty:ty),* $(,)?) => {
            impl PostInstallCleanupObservationV1 {
                $(pub const fn $name(&self) -> $ty { self.$field })*
            }
        };
    }

    post_install_getters! {
        error: error => Option<PublicationErrorV1>,
        first_cause: first_cause => Option<PublicationCauseV1>,
        dominant_cause: dominant_cause => Option<PublicationCauseV1>,
        after_catalog_publication: after_catalog_publication => bool,
        publication_poll_passed: publication_poll_passed => bool,
        cleanup_panicked: cleanup_panicked => bool,
        operation_slots: operation_slots => u64,
        operation_active: operation_active => u64,
        storage_active_operations: storage_active_operations => u64,
        storage_active_bytes: storage_active_bytes => u64,
        storage_active_inodes: storage_active_inodes => u64,
        new_carrier_entries: new_carrier_entries => u64,
        new_carrier_bytes: new_carrier_bytes => u64,
        preparation_bytes: preparation_bytes => u64,
        preparation_inodes: preparation_inodes => u64,
        locator_delta_bytes: locator_delta_bytes => u64,
        locator_delta_inodes: locator_delta_inodes => u64,
        catalog_delta_bytes: catalog_delta_bytes => u64,
        catalog_delta_inodes: catalog_delta_inodes => u64,
        closure_delta_bytes: closure_delta_bytes => u64,
        closure_delta_inodes: closure_delta_inodes => u64,
        immutable_delta_bytes: immutable_delta_bytes => u64,
        immutable_delta_inodes: immutable_delta_inodes => u64,
        residue_bytes: residue_bytes => u64,
        storage_bytes_requested: storage_bytes_requested => u64,
        storage_bytes_reserved: storage_bytes_reserved => u64,
        storage_bytes_released: storage_bytes_released => u64,
        storage_bytes_committed: storage_bytes_committed => u64,
        storage_bytes_retained: storage_bytes_retained => u64,
        storage_inodes_requested: storage_inodes_requested => u64,
        storage_inodes_reserved: storage_inodes_reserved => u64,
        storage_inodes_released: storage_inodes_released => u64,
        storage_inodes_committed: storage_inodes_committed => u64,
        storage_inodes_retained: storage_inodes_retained => u64,
        invalidated: invalidated => bool,
        stale_invalidated: stale_invalidated => bool,
        reopen_invalidated: reopen_invalidated => bool,
        zero_forbidden_work: zero_forbidden_work => bool,
    }

    impl PostInstallCleanupObservationV1 {
        pub const fn terminal(
            &self,
        ) -> (
            Option<PublicationErrorV1>,
            Option<PublicationCauseV1>,
            Option<PublicationCauseV1>,
        ) {
            (self.error, self.first_cause, self.dominant_cause)
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
        fn boundary_reached(&mut self, _boundary: FsCasBoundaryV1) {}

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    #[derive(Default)]
    struct PanicPrivatePackCleanupControl {
        after_catalog_publication: bool,
        publication_poll_passed: bool,
        cleanup_panicked: bool,
    }

    impl PanicPrivatePackCleanupControl {
        fn cancellation_requested_v1(&mut self) -> bool {
            if !self.after_catalog_publication {
                return false;
            }
            if !self.publication_poll_passed {
                self.publication_poll_passed = true;
                return false;
            }
            true
        }
    }

    impl CdcControlV1 for PanicPrivatePackCleanupControl {
        fn cancellation_requested(&mut self) -> bool {
            self.cancellation_requested_v1()
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for PanicPrivatePackCleanupControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary == FsCasBoundaryV1::AfterCatalogPublication {
                self.after_catalog_publication = true;
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            self.cancellation_requested_v1()
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::PrivatePack && !self.cleanup_panicked {
                self.cleanup_panicked = true;
                panic!("injected post-install private-pack cleanup unwind");
            }
            false
        }
    }

    struct SliceSource<'a> {
        bytes: &'a [u8],
        offset: usize,
    }

    impl<'a> SliceSource<'a> {
        const fn new(bytes: &'a [u8]) -> Self {
            Self { bytes, offset: 0 }
        }
    }

    impl ContentSourceV1 for SliceSource<'_> {
        fn resident_memory_bound_bytes(&self) -> crate::CoreResult<u64> {
            Ok(core::mem::size_of::<Self>() as u64)
        }

        fn read(&mut self, destination: &mut [u8]) -> Result<usize, ContentSourceErrorV1> {
            let amount = destination.len().min(self.bytes.len() - self.offset);
            destination[..amount].copy_from_slice(&self.bytes[self.offset..self.offset + amount]);
            self.offset += amount;
            Ok(amount)
        }
    }

    struct SliceSupplier<'a> {
        bytes: &'a [u8],
    }

    impl<'a> SourceSupplierV1 for SliceSupplier<'a> {
        type Source = SliceSource<'a>;

        fn resident_memory_bound_bytes(&self) -> crate::CoreResult<u64> {
            Ok(core::mem::size_of::<SliceSource<'_>>() as u64)
        }

        fn supply(self) -> crate::CoreResult<Self::Source> {
            Ok(SliceSource::new(self.bytes))
        }
    }

    struct OperationScratch {
        source: Box<[u8; MAXIMUM_CHUNK_BYTES]>,
        cdc_ring: Box<[u8; MAXIMUM_CHUNK_BYTES]>,
        incoming: Box<[u8; COMPARISON_WINDOW_BYTES]>,
        occupied: Box<[u8; COMPARISON_WINDOW_BYTES]>,
        tree_object: Box<[u8; MAX_TREE_OBJECT_BYTES]>,
        tree_pages: Box<[Option<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES]>,
        traversal: Vec<u8>,
    }

    impl OperationScratch {
        fn new() -> Self {
            Self {
                source: boxed_zeroes(),
                cdc_ring: boxed_zeroes(),
                incoming: boxed_zeroes(),
                occupied: boxed_zeroes(),
                tree_object: boxed_zeroes(),
                tree_pages: vec![None; MAX_TREE_PAGE_SUMMARIES]
                    .into_boxed_slice()
                    .try_into()
                    .unwrap_or_else(|_| unreachable!("exact tree-page scratch length")),
                traversal: vec![0; 4_096],
            }
        }

        fn borrow(&mut self) -> OperationBuffersV1<'_> {
            OperationBuffersV1 {
                source: &mut self.source,
                cdc_ring: &mut self.cdc_ring,
                incoming_comparison: &mut self.incoming,
                occupied_comparison: &mut self.occupied,
                tree_object: &mut self.tree_object,
                tree_pages: &mut self.tree_pages[..],
                traversal_state: &mut self.traversal,
            }
        }
    }

    fn boxed_zeroes<const N: usize>() -> Box<[u8; N]> {
        vec![0_u8; N]
            .into_boxed_slice()
            .try_into()
            .unwrap_or_else(|_| unreachable!("exact boxed scratch length"))
    }

    fn canonical_object(kind: u8, payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(52 + payload.len());
        bytes.extend_from_slice(b"ELSOBJ01");
        bytes.extend_from_slice(&1_u16.to_be_bytes());
        bytes.push(kind);
        bytes.push(0);
        bytes.extend_from_slice(ProfileSpecV1::frozen().id().as_bytes());
        bytes.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    #[derive(Clone)]
    struct ExpectedFileV1 {
        logical: LogicalFileIdentityV1,
        physical: PhysicalFileIdV1,
        chunks: Vec<BaseChunkEvidenceV1>,
    }

    impl ExpectedFileV1 {
        fn child(&self, mode: u16) -> crate::CoreResult<CanonicalTreeChildV1> {
            Ok(CanonicalTreeChildV1::File {
                logical: derive_file_node_v1(mode, self.logical)?,
                physical: self.physical,
            })
        }

        fn authenticated(&self, mode: u16) -> AuthenticatedBaseFileV1 {
            AuthenticatedBaseFileV1::new(
                self.logical,
                self.physical,
                mode,
                self.chunks.len() as u32,
            )
        }

        fn evidence(&self) -> ExpectedFileEvidenceV1 {
            ExpectedFileEvidenceV1 {
                chunks: self.chunks.clone(),
                cursor: 0,
            }
        }
    }

    #[derive(Clone)]
    struct ExpectedFileEvidenceV1 {
        chunks: Vec<BaseChunkEvidenceV1>,
        cursor: usize,
    }

    impl ExpectedFileEvidenceV1 {
        fn total_len(&self) -> u64 {
            self.chunks
                .last()
                .and_then(|chunk| chunk.end().ok())
                .unwrap_or(0)
        }
    }

    impl BaseChunkEvidenceSourceV1 for ExpectedFileEvidenceV1 {
        fn resident_memory_bound_bytes(&self) -> crate::CoreResult<u64> {
            Ok(core::mem::size_of::<Self>() as u64)
        }

        fn rewind(&mut self) -> Result<(), PreparedSinkErrorV1> {
            self.cursor = 0;
            Ok(())
        }

        fn next(&mut self) -> Result<Option<BaseChunkEvidenceV1>, PreparedSinkErrorV1> {
            let result = self.chunks.get(self.cursor).copied();
            self.cursor += usize::from(result.is_some());
            Ok(result)
        }

        fn containing(
            &mut self,
            offset: u64,
            include_end: bool,
        ) -> Result<Option<BaseChunkEvidenceV1>, PreparedSinkErrorV1> {
            let total_len = self.total_len();
            Ok(self.chunks.iter().copied().find(|chunk| {
                let end = chunk.end().expect("bounded semantic chunk");
                (chunk.start() <= offset && offset < end)
                    || (include_end && offset == end && end == total_len)
            }))
        }

        fn at_start(
            &mut self,
            offset: u64,
        ) -> Result<Option<BaseChunkEvidenceV1>, PreparedSinkErrorV1> {
            Ok(self
                .chunks
                .iter()
                .copied()
                .find(|chunk| chunk.start() == offset))
        }
    }

    struct BaseBytesV1<'a> {
        bytes: &'a [u8],
    }

    impl AuthenticatedBaseByteReaderV1 for BaseBytesV1<'_> {
        fn resident_memory_bound_bytes(&self) -> crate::CoreResult<u64> {
            Ok(core::mem::size_of::<Self>() as u64)
        }

        fn read_exact_at(
            &mut self,
            offset: u64,
            destination: &mut [u8],
        ) -> Result<(), BaseReadErrorV1> {
            let start = usize::try_from(offset).map_err(|_| BaseReadErrorV1::Missing)?;
            let end = start
                .checked_add(destination.len())
                .ok_or(BaseReadErrorV1::Missing)?;
            destination
                .copy_from_slice(self.bytes.get(start..end).ok_or(BaseReadErrorV1::Missing)?);
            Ok(())
        }

        fn compare_exact_at(
            &mut self,
            offset: u64,
            first: &[u8],
            second: &[u8],
        ) -> Result<bool, BaseReadErrorV1> {
            let start = usize::try_from(offset).map_err(|_| BaseReadErrorV1::Missing)?;
            let first_end = start
                .checked_add(first.len())
                .ok_or(BaseReadErrorV1::Missing)?;
            let end = first_end
                .checked_add(second.len())
                .ok_or(BaseReadErrorV1::Missing)?;
            Ok(self.bytes.get(start..first_end) == Some(first)
                && self.bytes.get(first_end..end) == Some(second))
        }
    }

    fn expected_file_details(data: &[u8], mode: u16) -> crate::CoreResult<ExpectedFileV1> {
        let mut chunks = Vec::new();
        let mut logical_refs = Vec::new();
        let mut physical_refs = Vec::new();
        let mut offset = 0_usize;
        while offset < data.len() {
            let cut = FastCdcV1::new().cut(&data[offset..])?;
            let payload = &data[offset..offset + cut];
            let logical = derive_logical_chunk_v1(payload)?;
            let physical = derive_physical_chunk_id_v1(&canonical_object(0x05, payload))?;
            chunks.push(BaseChunkEvidenceV1::new(
                offset as u64,
                logical.id(),
                physical,
                cut as u32,
            ));
            logical_refs.push(LogicalChunkRefV1::from_identity(logical));
            physical_refs.push((cut as u32, physical));
            offset += cut;
        }
        let logical = derive_logical_file_v1(data.len() as u64, &logical_refs)?;
        let mut payload = Vec::new();
        payload.extend_from_slice(&mode.to_be_bytes());
        payload.extend_from_slice(&(data.len() as u64).to_be_bytes());
        payload.extend_from_slice(&u32::from(!data.is_empty()).to_be_bytes());
        if !data.is_empty() {
            payload.push(0x02);
            payload.extend_from_slice(&(data.len() as u64).to_be_bytes());
            payload.extend_from_slice(&(physical_refs.len() as u32).to_be_bytes());
            for (length, id) in physical_refs {
                payload.extend_from_slice(&length.to_be_bytes());
                payload.extend_from_slice(id.as_bytes());
            }
        }
        let physical = derive_physical_file_id_v1(&canonical_object(0x03, &payload))?;
        Ok(ExpectedFileV1 {
            logical,
            physical,
            chunks,
        })
    }

    fn expected_file(
        data: &[u8],
        mode: u16,
    ) -> crate::CoreResult<(LogicalFileIdentityV1, PhysicalFileIdV1)> {
        let file = expected_file_details(data, mode)?;
        Ok((file.logical, file.physical))
    }

    fn directory_usage(path: &Path) -> (u64, u64) {
        fs::read_dir(path)
            .expect("semantic namespace directory")
            .map(|entry| {
                let entry = entry.expect("semantic namespace entry");
                let metadata =
                    fs::symlink_metadata(entry.path()).expect("semantic namespace metadata");
                (metadata.len(), 1_u64)
            })
            .fold((0, 0), |(bytes, inodes), (length, one)| {
                (bytes + length, inodes + one)
            })
    }

    fn clean_preparation_usage(root: &Path) -> (u64, u64) {
        let (preparation_bytes, preparation_inodes) = directory_usage(&root.join("preparation"));
        assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
        (preparation_bytes, preparation_inodes)
    }

    fn assert_operation_authority_baseline(cas: &FsCasV1, root: &Path) {
        assert_eq!(cas.operation_admitted_slots_v1(), 0);
        assert_eq!(cas.operation_admission_active_for_test_v1(), 0);
        assert_eq!(cas.operation_admission_queue_for_test_v1(), (0, 0, 0));
        assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
        assert_eq!(
            fs::read_dir(root.join("preparation"))
                .expect("operation preparation namespace")
                .count(),
            0
        );
    }

    fn immutable_usage(root: &Path) -> (u64, u64) {
        ["carriers", "objects", "catalog", "closures"]
            .into_iter()
            .map(|name| directory_usage(&root.join(name)))
            .fold((0, 0), |(bytes, inodes), (next_bytes, next_inodes)| {
                (bytes + next_bytes, inodes + next_inodes)
            })
    }

    /// Execute the historical two-phase replacement until the installed
    /// carrier is followed by cancellation and private-pack cleanup failure.
    /// Only scalar custody facts cross the qualification seam.
    pub fn post_install_cleanup_v1(
        request: PostInstallCleanupRequestV1<'_>,
    ) -> PostInstallCleanupObservationV1 {
        let cas = FsCasV1::create_new(request.root).expect("post-install semantic root");
        let stale = FsCasV1::open_existing(request.root).expect("post-install stale owner");
        let (base_logical, base_physical) =
            expected_file(request.base, 0o644).expect("post-install base identity");
        let base_component = ValidatedComponent::new(request.name).expect("post-install name");
        let base_entry = CanonicalTreeEntryV1::new(
            base_component,
            CanonicalTreeChildV1::File {
                logical: derive_file_node_v1(0o644, base_logical).expect("post-install file node"),
                physical: base_physical,
            },
        );
        let expected_root = with_replacement_evidence_v1(
            DirectoryBuildModeV1::ImplicitRoot,
            std::slice::from_ref(&base_entry),
            0,
            |tree, _| tree.physical(),
        )
        .expect("post-install base tree");

        let mut manifest = [TreeFileV1::new(
            request.name,
            0o644,
            request.base.len() as u64,
            SliceSupplier {
                bytes: request.base,
            },
        )];
        let mut scratch = OperationScratch::new();
        let mut base_control = ContinueControl;
        let mut base_counters = OperationCountersV1::default();
        let base_operation =
            request_tree_operation_v1(&cas, 0x515, &mut base_counters, &mut base_control)
                .expect("post-install base grant");
        let base_handoff = run_create_tree_v1(
            base_operation,
            CdcAlgorithmV1::FastCdc,
            &mut manifest,
            scratch.borrow(),
            &mut base_control,
            &mut base_counters,
        )
        .expect("post-install base handoff");
        assert_eq!(base_handoff.root_tree(), expected_root);
        let base_version = base_handoff.version_record();
        let before_immutable = immutable_usage(request.root);
        let before_carriers = directory_usage(&request.root.join("carriers"));
        let before_objects = directory_usage(&request.root.join("objects"));
        let before_catalog = directory_usage(&request.root.join("catalog"));
        let before_closures = directory_usage(&request.root.join("closures"));

        let mut replacement_source = SliceSource::new(request.replacement);
        let mut replacement_scratch = OperationScratch::new();
        let mut cow_logical = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
        let mut control = PanicPrivatePackCleanupControl::default();
        let mut counters = OperationCountersV1::default();
        let terminal = catch_unwind(AssertUnwindSafe(|| {
            with_replacement_evidence_v1(
                DirectoryBuildModeV1::ImplicitRoot,
                std::slice::from_ref(&base_entry),
                0,
                |base_tree, evidence| {
                    run_complete_replace_v1(
                        &cas,
                        0x516,
                        CdcAlgorithmV1::FastCdc,
                        base_version,
                        base_tree,
                        evidence,
                        0,
                        request.name,
                        0o600,
                        request.replacement.len() as u64,
                        &mut replacement_source,
                        replacement_scratch.borrow(),
                        &mut cow_logical,
                        &mut control,
                        &mut counters,
                    )
                },
            )
        }));
        let raw_error = match terminal {
            Ok(Ok(Err(OperationErrorV1::FsCas(error)))) => Some(error),
            Ok(Ok(Err(OperationErrorV1::Core(error)))) => Some(FsCasErrorV1::Core(error)),
            Ok(Ok(Ok(_))) | Ok(Err(_)) => None,
            Err(_) => None,
        };
        let after_preparation = directory_usage(&request.root.join("preparation"));
        let after_immutable = immutable_usage(request.root);
        let after_carriers = directory_usage(&request.root.join("carriers"));
        let after_objects = directory_usage(&request.root.join("objects"));
        let after_catalog = directory_usage(&request.root.join("catalog"));
        let after_closures = directory_usage(&request.root.join("closures"));
        let (storage_active_operations, storage_active_bytes, storage_active_inodes) =
            cas.storage_admission_active_for_test_v1();
        let (first_cause, dominant_cause) = raw_error
            .map(publication_causes_v1)
            .map(|(first, dominant)| (Some(first), Some(dominant)))
            .unwrap_or((None, None));
        PostInstallCleanupObservationV1 {
            error: raw_error.map(publication_error_v1),
            first_cause,
            dominant_cause,
            after_catalog_publication: control.after_catalog_publication,
            publication_poll_passed: control.publication_poll_passed,
            cleanup_panicked: control.cleanup_panicked,
            operation_slots: cas.operation_admitted_slots_v1(),
            operation_active: cas.operation_admission_active_for_test_v1(),
            storage_active_operations,
            storage_active_bytes,
            storage_active_inodes,
            new_carrier_entries: after_carriers.1.saturating_sub(before_carriers.1),
            new_carrier_bytes: after_carriers.0.saturating_sub(before_carriers.0),
            preparation_bytes: after_preparation.0,
            preparation_inodes: after_preparation.1,
            locator_delta_bytes: after_objects.0.saturating_sub(before_objects.0),
            locator_delta_inodes: after_objects.1.saturating_sub(before_objects.1),
            catalog_delta_bytes: after_catalog.0.saturating_sub(before_catalog.0),
            catalog_delta_inodes: after_catalog.1.saturating_sub(before_catalog.1),
            closure_delta_bytes: after_closures.0.saturating_sub(before_closures.0),
            closure_delta_inodes: after_closures.1.saturating_sub(before_closures.1),
            immutable_delta_bytes: after_immutable.0.saturating_sub(before_immutable.0),
            immutable_delta_inodes: after_immutable.1.saturating_sub(before_immutable.1),
            residue_bytes: counters.unreachable_installed_residue_bytes,
            storage_bytes_requested: counters.storage_bytes_requested,
            storage_bytes_reserved: counters.storage_bytes_reserved,
            storage_bytes_released: counters.storage_bytes_released,
            storage_bytes_committed: counters.storage_bytes_committed,
            storage_bytes_retained: counters.storage_bytes_retained,
            storage_inodes_requested: counters.storage_inodes_requested,
            storage_inodes_reserved: counters.storage_inodes_reserved,
            storage_inodes_released: counters.storage_inodes_released,
            storage_inodes_committed: counters.storage_inodes_committed,
            storage_inodes_retained: counters.storage_inodes_retained,
            invalidated: matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            stale_invalidated: matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            reopen_invalidated: matches!(
                FsCasV1::open_existing(request.root),
                Err(FsCasErrorV1::Invalidated)
            ),
            zero_forbidden_work: counters.has_zero_forbidden_work(),
        }
    }

    struct WatchdogGateV1 {
        released: Mutex<bool>,
        wake: Condvar,
    }

    impl WatchdogGateV1 {
        fn new() -> Self {
            Self {
                released: Mutex::new(false),
                wake: Condvar::new(),
            }
        }

        fn wait(&self) {
            let released = self.released.lock().expect("semantic watchdog gate");
            let (released, timeout) = self
                .wake
                .wait_timeout_while(released, Duration::from_secs(5), |released| !*released)
                .expect("semantic watchdog gate");
            let timed_out = !*released;
            drop(released);
            if timed_out {
                panic!("semantic watchdog gate timed out: {timeout:?}");
            }
        }

        fn release(&self) {
            *self.released.lock().expect("semantic watchdog gate") = true;
            self.wake.notify_all();
        }
    }

    struct WatchdogGateReleaseV1 {
        gate: Arc<WatchdogGateV1>,
        released: bool,
    }

    impl WatchdogGateReleaseV1 {
        fn new(gate: Arc<WatchdogGateV1>) -> Self {
            Self {
                gate,
                released: false,
            }
        }

        fn release(&mut self) {
            self.gate.release();
            self.released = true;
        }
    }

    impl Drop for WatchdogGateReleaseV1 {
        fn drop(&mut self) {
            if !self.released {
                self.gate.release();
            }
        }
    }

    struct BarrierSliceSupplier<'a> {
        bytes: &'a [u8],
        ready: mpsc::SyncSender<()>,
        start: Arc<WatchdogGateV1>,
    }

    impl<'a> SourceSupplierV1 for BarrierSliceSupplier<'a> {
        type Source = SliceSource<'a>;

        fn resident_memory_bound_bytes(&self) -> crate::CoreResult<u64> {
            Ok(core::mem::size_of::<SliceSource<'_>>() as u64)
        }

        fn supply(self) -> crate::CoreResult<Self::Source> {
            self.ready.send(()).expect("barrier supplier receiver");
            self.start.wait();
            Ok(SliceSource::new(self.bytes))
        }
    }

    struct BarrierFailingSupplier {
        ready: mpsc::SyncSender<()>,
        start: Arc<WatchdogGateV1>,
    }

    impl SourceSupplierV1 for BarrierFailingSupplier {
        type Source = SliceSource<'static>;

        fn resident_memory_bound_bytes(&self) -> crate::CoreResult<u64> {
            Ok(core::mem::size_of::<SliceSource<'static>>() as u64)
        }

        fn supply(self) -> crate::CoreResult<Self::Source> {
            self.ready.send(()).expect("failing supplier receiver");
            self.start.wait();
            Err(CoreError::CountCap)
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum CandidateValidationStopV1 {
        Cancelled,
        Deadline,
    }

    struct StopBeforeCandidateValidationV1 {
        stop: CandidateValidationStopV1,
        armed: bool,
    }

    impl StopBeforeCandidateValidationV1 {
        const fn new(stop: CandidateValidationStopV1) -> Self {
            Self { stop, armed: false }
        }

        fn cancelled(&self) -> bool {
            self.armed && self.stop == CandidateValidationStopV1::Cancelled
        }

        fn deadline(&self) -> bool {
            self.armed && self.stop == CandidateValidationStopV1::Deadline
        }
    }

    impl CdcControlV1 for StopBeforeCandidateValidationV1 {
        fn cancellation_requested(&mut self) -> bool {
            self.cancelled()
        }

        fn deadline_exceeded(&mut self) -> bool {
            self.deadline()
        }
    }

    impl FsCasControlV1 for StopBeforeCandidateValidationV1 {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary == FsCasBoundaryV1::BeforeCandidateValidation {
                self.armed = true;
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            self.cancelled()
        }

        fn deadline_exceeded(&mut self) -> bool {
            self.deadline()
        }
    }

    fn run_semantic_create<C, S>(
        cas: &FsCasV1,
        key: u64,
        declared_len: u64,
        supplier: S,
        control: &mut C,
        counters: &mut OperationCountersV1,
    ) -> Result<super::OperationHandoffV1, OperationErrorV1>
    where
        C: CdcControlV1 + FsCasControlV1,
        S: SourceSupplierV1,
    {
        run_semantic_create_with_algorithm(
            cas,
            key,
            CdcAlgorithmV1::FastCdc,
            declared_len,
            supplier,
            control,
            counters,
        )
    }

    fn run_semantic_create_with_algorithm<C, S>(
        cas: &FsCasV1,
        key: u64,
        algorithm: CdcAlgorithmV1,
        declared_len: u64,
        supplier: S,
        control: &mut C,
        counters: &mut OperationCountersV1,
    ) -> Result<super::OperationHandoffV1, OperationErrorV1>
    where
        C: CdcControlV1 + FsCasControlV1,
        S: SourceSupplierV1,
    {
        let mut scratch = OperationScratch::new();
        let grant = request_create_operation_v1(cas, key, counters, control)
            .map_err(OperationErrorV1::FsCas)?;
        run_create_v1(
            grant,
            algorithm,
            b"payload.bin",
            0o644,
            declared_len,
            supplier,
            scratch.borrow(),
            control,
            counters,
        )
    }

    struct SemanticCounterSupplier {
        len: u64,
    }

    impl SourceSupplierV1 for SemanticCounterSupplier {
        type Source = CounterSource;

        fn resident_memory_bound_bytes(&self) -> crate::CoreResult<u64> {
            Ok(core::mem::size_of::<CounterSource>() as u64)
        }

        fn supply(self) -> crate::CoreResult<Self::Source> {
            Ok(CounterSource {
                len: self.len,
                offset: 0,
            })
        }
    }

    fn complete_create_counters(counters: &OperationCountersV1) -> CompleteCreateCountersV1 {
        CompleteCreateCountersV1 {
            physical_carrier_object_writes: counters.physical_carrier_object_writes,
            pack_entries: counters.pack_entries,
            pack_bytes: counters.pack_bytes,
            carrier_bytes_total: counters.carrier_bytes_total,
            ring_fills: counters.ring_fills,
            ring_wrap_spans: counters.ring_wrap_spans,
            cdc_scan_calls: counters.cdc_scan_calls,
            cdc_scan_bytes: counters.cdc_scan_bytes,
            bytes_boundary_inspected: counters.bytes_boundary_inspected,
            seqcdc_comparisons: counters.seqcdc_comparisons,
            seqcdc_equal_absorptions: counters.seqcdc_equal_absorptions,
            seqcdc_opposing_slopes: counters.seqcdc_opposing_slopes,
            seqcdc_jumps: counters.seqcdc_jumps,
            seqcdc_jump_bytes: counters.seqcdc_jump_bytes,
            global_seen_lookups: counters.global_seen_lookups,
            global_seen_probes: counters.global_seen_probes,
            global_seen_metadata_bytes_read: counters.global_seen_metadata_bytes_read,
            global_seen_metadata_read_calls: counters.global_seen_metadata_read_calls,
            global_seen_metadata_bytes_written: counters.global_seen_metadata_bytes_written,
            global_seen_maximum_probe: counters.global_seen_maximum_probe,
            global_seen_entries: counters.global_seen_entries,
            global_seen_table_bytes: counters.global_seen_table_bytes,
            version_objects_created: counters.version_objects_created,
            tree_objects_created: counters.tree_objects_created,
            file_objects_created: counters.file_objects_created,
            symlink_objects_created: counters.symlink_objects_created,
            chunk_objects_created: counters.chunk_objects_created,
            version_objects_reused: counters.version_objects_reused,
            tree_objects_reused: counters.tree_objects_reused,
            file_objects_reused: counters.file_objects_reused,
            symlink_objects_reused: counters.symlink_objects_reused,
            chunk_objects_reused: counters.chunk_objects_reused,
            pack_local_objects_created: counters.pack_local_objects_created,
            pack_local_objects_reused: counters.pack_local_objects_reused,
            source_read_calls: counters.source_read_calls,
            source_bytes_read: counters.source_bytes_read,
            fscas_read_calls: counters.fscas_read_calls,
            fscas_bytes_read: counters.fscas_bytes_read,
            fscas_bytes_written: counters.fscas_bytes_written,
            closure_fences: counters.closure_fences,
            visibility_lock_acquisitions: counters.visibility_lock_acquisitions,
            publication_lock_acquisitions: counters.publication_lock_acquisitions,
            file_sort_comparisons: counters.file_sort_comparisons,
            file_sort_record_reads: counters.file_sort_record_reads,
            file_sort_record_writes: counters.file_sort_record_writes,
            file_sort_passes: counters.file_sort_passes,
            file_sort_control_polls: counters.file_sort_control_polls,
            file_sort_work_units: counters.file_sort_work_units,
            file_sort_maximum_work_budget: counters.file_sort_maximum_work_budget,
            file_sort_temporary_bytes_high_water: counters.file_sort_temporary_bytes_high_water,
            storage_bytes_requested: counters.storage_bytes_requested,
            storage_bytes_reserved: counters.storage_bytes_reserved,
            storage_bytes_released: counters.storage_bytes_released,
            storage_bytes_committed: counters.storage_bytes_committed,
            storage_bytes_retained: counters.storage_bytes_retained,
            storage_inodes_requested: counters.storage_inodes_requested,
            storage_inodes_reserved: counters.storage_inodes_reserved,
            storage_inodes_released: counters.storage_inodes_released,
            storage_inodes_committed: counters.storage_inodes_committed,
            storage_inodes_retained: counters.storage_inodes_retained,
            root_reserved_bytes_high_water: counters
                .root_storage_active_reserved_bytes_lifetime_high_water,
            root_reserved_inodes_high_water: counters
                .root_storage_active_reserved_inodes_lifetime_high_water,
            mutable_preparation_residue_bytes: counters.mutable_preparation_residue_bytes,
            mutable_preparation_residue_inodes: counters.mutable_preparation_residue_inodes,
            immutable_residue_bytes: counters.immutable_residue_bytes,
            immutable_residue_inodes: counters.immutable_residue_inodes,
            unreachable_installed_residue_bytes: counters.unreachable_installed_residue_bytes,
            zero_forbidden_work: counters.has_zero_forbidden_work(),
            storage_equations_hold: storage_equations_hold(counters),
        }
    }

    #[derive(Clone, Copy, Default)]
    struct CompleteCreateLockObservation {
        closure_marker_observed: bool,
        visibility_lock_available: bool,
        publication_lock_available: bool,
        closure_publication_acquisitions: u64,
        closure_publication_releases: u64,
        observed_visibility_acquisitions: u64,
        observed_visibility_releases: u64,
        observed_publication_acquisitions: u64,
        observed_publication_releases: u64,
    }

    fn observe_complete_create(
        root: &Path,
        cas: &FsCasV1,
        stale: &FsCasV1,
        terminal: Result<super::OperationHandoffV1, OperationErrorV1>,
        control_fired: bool,
        counters: &OperationCountersV1,
        lock: CompleteCreateLockObservation,
    ) -> CompleteCreateObservationV1 {
        let error_from_storage = matches!(terminal, Err(OperationErrorV1::FsCas(_)));
        let (error, handoff) = match terminal {
            Ok(handoff) => (None, Some(handoff)),
            Err(OperationErrorV1::Core(error)) => {
                (Some(publication_error_v1(FsCasErrorV1::Core(error))), None)
            }
            Err(OperationErrorV1::FsCas(error)) => (Some(publication_error_v1(error)), None),
        };
        let reference = handoff.map(|value| value.reference_spool_bytes());
        let index = handoff.map(|value| value.index_spool_bytes());
        let (preparation_bytes, preparation_inodes) = directory_usage(&root.join("preparation"));
        let (immutable_bytes, immutable_inodes) = immutable_usage(root);
        let operation_admitted_slots = cas.operation_admitted_slots_v1();
        let operation_admission_active = cas.operation_admission_active_for_test_v1();
        let operation_admission_queue = cas.operation_admission_queue_for_test_v1();
        let storage_admission_active = cas.storage_admission_active_for_test_v1();
        let preparation_entries = fs::read_dir(root.join("preparation"))
            .expect("semantic preparation namespace")
            .count() as u64;
        CompleteCreateObservationV1 {
            error,
            error_from_storage,
            control_fired,
            algorithm: handoff.map(|value| value.algorithm()),
            pack_installed: handoff
                .map(|value| value.pack_outcome() == super::FsPackAdmissionOutcomeV1::Installed)
                .unwrap_or(false),
            object_count: handoff.map(|value| value.object_count()).unwrap_or(0),
            carrier_count: handoff.map(|value| value.carrier_count()).unwrap_or(0),
            carrier_rollovers: handoff.map(|value| value.carrier_rollovers()).unwrap_or(0),
            carriers_installed: handoff.map(|value| value.carriers_installed()).unwrap_or(0),
            carriers_reused: handoff.map(|value| value.carriers_reused()).unwrap_or(0),
            reference_spool_observed: reference
                .map(|value| value.status() == OptionalObservationStatusV1::Observed)
                .unwrap_or(false),
            reference_spool_bytes: reference.and_then(|value| value.value()),
            reference_spool_operation_scoped: reference
                .map(|value| value.scope() == ObservationScopeV1::Operation)
                .unwrap_or(false),
            reference_spool_method: reference.map(|value| value.method()).unwrap_or(""),
            index_spool_observed: index
                .map(|value| value.status() == OptionalObservationStatusV1::Observed)
                .unwrap_or(false),
            index_spool_bytes: index.and_then(|value| value.value()),
            index_spool_operation_scoped: index
                .map(|value| value.scope() == ObservationScopeV1::Operation)
                .unwrap_or(false),
            index_spool_method: index.map(|value| value.method()).unwrap_or(""),
            terminal_optional_observations_match_counters: handoff
                .map(|value| {
                    value.terminal_optional_observations()
                        == counters.terminal_optional_observations_v1()
                })
                .unwrap_or(false),
            terminal_optional_observations_empty: handoff
                .map(|value| {
                    value
                        .terminal_optional_observations()
                        .all()
                        .into_iter()
                        .all(|observation| observation.value().is_none())
                })
                .unwrap_or(false),
            preparation_usage: (preparation_bytes, preparation_inodes),
            immutable_usage: (immutable_bytes, immutable_inodes),
            operation_authority_clean: operation_admitted_slots == 0
                && operation_admission_active == 0
                && operation_admission_queue == (0, 0, 0)
                && storage_admission_active == (0, 0, 0)
                && preparation_entries == 0,
            operation_admitted_slots,
            operation_admission_active,
            operation_admission_queue,
            storage_admission_active,
            preparation_entries,
            root_usable: cas.occupied().is_ok(),
            stale_usable: stale.occupied().is_ok(),
            closure_marker_observed: lock.closure_marker_observed,
            visibility_lock_available: lock.visibility_lock_available,
            publication_lock_available: lock.publication_lock_available,
            closure_publication_acquisitions: lock.closure_publication_acquisitions,
            closure_publication_releases: lock.closure_publication_releases,
            observed_visibility_acquisitions: lock.observed_visibility_acquisitions,
            observed_visibility_releases: lock.observed_visibility_releases,
            observed_publication_acquisitions: lock.observed_publication_acquisitions,
            observed_publication_releases: lock.observed_publication_releases,
            counters: complete_create_counters(counters),
        }
    }

    pub fn complete_create_case_v1(
        root: &Path,
        case: CompleteCreateCaseV1,
    ) -> CompleteCreateObservationV1 {
        const LOGICAL_BYTES: u64 = 64 * 1024;
        let (cas, stale) = new_fault_root(root);

        macro_rules! observed {
            ($terminal:expr, $fired:expr, $counters:expr) => {
                observe_complete_create(
                    root,
                    &cas,
                    &stale,
                    $terminal,
                    $fired,
                    &$counters,
                    CompleteCreateLockObservation::default(),
                )
            };
        }

        match case {
            CompleteCreateCaseV1::StorageCounterMergeOverflow => {
                let mut counters = OperationCountersV1 {
                    physical_carrier_object_writes: 41,
                    pack_entries: 43,
                    pack_bytes: 47,
                    carrier_bytes_total: u64::MAX,
                    ..OperationCountersV1::default()
                };
                let mut control = ContinueFaultControl;
                let terminal = run_semantic_create(
                    &cas,
                    0x0011_5504,
                    1,
                    SliceSupplier { bytes: &[0x5a] },
                    &mut control,
                    &mut counters,
                );
                observed!(terminal, false, counters)
            }
            CompleteCreateCaseV1::FastCdcCounterOverflow => {
                let mut counters = OperationCountersV1 {
                    ring_fills: 41,
                    ring_wrap_spans: 43,
                    cdc_scan_calls: 47,
                    cdc_scan_bytes: 53,
                    bytes_boundary_inspected: u64::MAX,
                    ..OperationCountersV1::default()
                };
                let mut control = ContinueFaultControl;
                let terminal = run_semantic_create(
                    &cas,
                    0x0011_5505,
                    LOGICAL_BYTES,
                    SemanticCounterSupplier { len: LOGICAL_BYTES },
                    &mut control,
                    &mut counters,
                );
                observed!(terminal, false, counters)
            }
            CompleteCreateCaseV1::SeqCdcCounterOverflow => {
                let input = (0..LOGICAL_BYTES as usize)
                    .map(|index| if index % 2 == 0 { 0xff } else { 0x00 })
                    .collect::<Vec<_>>();
                let mut counters = OperationCountersV1 {
                    seqcdc_comparisons: 41,
                    seqcdc_equal_absorptions: 43,
                    seqcdc_opposing_slopes: 47,
                    seqcdc_jumps: 53,
                    seqcdc_jump_bytes: u64::MAX,
                    ..OperationCountersV1::default()
                };
                let mut control = ContinueFaultControl;
                let terminal = run_semantic_create_with_algorithm(
                    &cas,
                    0x0011_5506,
                    CdcAlgorithmV1::SeqCdc,
                    LOGICAL_BYTES,
                    SliceSupplier { bytes: &input },
                    &mut control,
                    &mut counters,
                );
                observed!(terminal, false, counters)
            }
            CompleteCreateCaseV1::GlobalSeenCounterOverflow => {
                let mut counters = OperationCountersV1::default();
                let mut control = GlobalSeenCounterOverflowControl::default();
                let terminal = run_semantic_create(
                    &cas,
                    0x0011_5507,
                    LOGICAL_BYTES,
                    SemanticCounterSupplier { len: LOGICAL_BYTES },
                    &mut control,
                    &mut counters,
                );
                observed!(terminal, control.injected, counters)
            }
            CompleteCreateCaseV1::OperationSpoolWriteOverflow => {
                let mut counters = OperationCountersV1::default();
                let mut control = OperationSpoolWriteObservationOverflowControl::default();
                let terminal = run_semantic_create(
                    &cas,
                    0x0011_550d,
                    LOGICAL_BYTES,
                    SemanticCounterSupplier { len: LOGICAL_BYTES },
                    &mut control,
                    &mut counters,
                );
                observed!(terminal, control.injected, counters)
            }
            CompleteCreateCaseV1::OperationSpoolReadOverflow => {
                let mut counters = OperationCountersV1::default();
                let mut control = ContinueFaultControl;
                cas.seed_next_operation_spool_read_observation_for_test_v1(71, u64::MAX);
                let terminal = run_semantic_create(
                    &cas,
                    0x0011_550e,
                    LOGICAL_BYTES,
                    SemanticCounterSupplier { len: LOGICAL_BYTES },
                    &mut control,
                    &mut counters,
                );
                observed!(terminal, true, counters)
            }
            CompleteCreateCaseV1::CountedPackReadOverflow => {
                let mut counters = OperationCountersV1::default();
                let mut control = CountedPackReadObservationOverflowControl::default();
                let terminal = run_semantic_create(
                    &cas,
                    0x0011_550f,
                    LOGICAL_BYTES,
                    SemanticCounterSupplier { len: LOGICAL_BYTES },
                    &mut control,
                    &mut counters,
                );
                observed!(terminal, control.injected, counters)
            }
            CompleteCreateCaseV1::SameCarrierComparisonOverflow => {
                let input = vec![0x5a; LOGICAL_BYTES as usize];
                let mut counters = OperationCountersV1::default();
                let mut control = SameCarrierComparisonObservationOverflowControl::default();
                let terminal = run_semantic_create(
                    &cas,
                    0x0011_5510,
                    LOGICAL_BYTES,
                    SliceSupplier { bytes: &input },
                    &mut control,
                    &mut counters,
                );
                observed!(terminal, control.injected, counters)
            }
            CompleteCreateCaseV1::PostAdmissionCarrierTallyOverflow => {
                let mut counters = OperationCountersV1::default();
                let mut control = PostAdmissionCarrierTallyOverflowControl::default();
                let terminal = run_semantic_create(
                    &cas,
                    0x0011_5511,
                    LOGICAL_BYTES,
                    SemanticCounterSupplier { len: LOGICAL_BYTES },
                    &mut control,
                    &mut counters,
                );
                observed!(terminal, control.injected, counters)
            }
            CompleteCreateCaseV1::CreatedDispositionOverflow => {
                let mut counters = OperationCountersV1::default();
                let mut control = PackObjectDispositionOverflowControl {
                    target_created: true,
                    injected: false,
                };
                let terminal = run_semantic_create(
                    &cas,
                    0x0011_5508,
                    1,
                    SliceSupplier { bytes: &[0x5a] },
                    &mut control,
                    &mut counters,
                );
                observed!(terminal, control.injected, counters)
            }
            CompleteCreateCaseV1::TreeReusedDispositionOverflow => {
                let payload = [0x6b];
                let mut files = [
                    TreeFileV1::new(b"a.bin", 0o644, 1, SliceSupplier { bytes: &payload }),
                    TreeFileV1::new(b"b.bin", 0o644, 1, SliceSupplier { bytes: &payload }),
                ];
                let mut counters = OperationCountersV1::default();
                let mut control = PackObjectDispositionOverflowControl {
                    target_created: false,
                    injected: false,
                };
                let terminal =
                    request_tree_operation_v1(&cas, 0x0011_5509, &mut counters, &mut control)
                        .map_err(OperationErrorV1::FsCas)
                        .and_then(|operation| {
                            let mut scratch = OperationScratch::new();
                            run_create_tree_v1(
                                operation,
                                CdcAlgorithmV1::FastCdc,
                                &mut files,
                                scratch.borrow(),
                                &mut control,
                                &mut counters,
                            )
                        });
                observed!(terminal, control.injected, counters)
            }
            CompleteCreateCaseV1::Algorithm(algorithm) => {
                let mut input = vec![0; 384 * 1024 + 73];
                let mut state = 0x9e37_79b9_u32;
                for (index, byte) in input.iter_mut().enumerate() {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    *byte = (state as u8) ^ (index as u8).wrapping_mul(17);
                }
                let mut counters = OperationCountersV1::default();
                let mut control = ContinueFaultControl;
                let terminal = run_semantic_create_with_algorithm(
                    &cas,
                    106,
                    algorithm,
                    input.len() as u64,
                    SliceSupplier { bytes: &input },
                    &mut control,
                    &mut counters,
                );
                observed!(terminal, false, counters)
            }
            CompleteCreateCaseV1::Exact100MiB => {
                const EXACT_BYTES: u64 = 100 * 1024 * 1024;
                let mut counters = OperationCountersV1::default();
                let mut control = ContinueFaultControl;
                let terminal = run_semantic_create(
                    &cas,
                    107,
                    EXACT_BYTES,
                    SemanticCounterSupplier { len: EXACT_BYTES },
                    &mut control,
                    &mut counters,
                );
                observed!(terminal, false, counters)
            }
            CompleteCreateCaseV1::ClosureMarkerLockScope => {
                let mut counters = OperationCountersV1::default();
                let mut control = ObserveClosureMarkerLockScope {
                    cas: cas.clone(),
                    observed: false,
                    visibility_available: false,
                    publication_available: false,
                    closure_phase: false,
                    visibility_acquisitions: 0,
                    visibility_releases: 0,
                    publication_acquisitions: 0,
                    publication_releases: 0,
                    closure_publication_acquisitions: 0,
                    closure_publication_releases: 0,
                };
                let terminal = run_semantic_create(
                    &cas,
                    0x0011_5500,
                    1,
                    SliceSupplier { bytes: &[0x5a] },
                    &mut control,
                    &mut counters,
                );
                let lock = CompleteCreateLockObservation {
                    closure_marker_observed: control.observed,
                    visibility_lock_available: control.visibility_available,
                    publication_lock_available: control.publication_available,
                    closure_publication_acquisitions: control.closure_publication_acquisitions,
                    closure_publication_releases: control.closure_publication_releases,
                    observed_visibility_acquisitions: control.visibility_acquisitions,
                    observed_visibility_releases: control.visibility_releases,
                    observed_publication_acquisitions: control.publication_acquisitions,
                    observed_publication_releases: control.publication_releases,
                };
                observe_complete_create(root, &cas, &stale, terminal, false, &counters, lock)
            }
            CompleteCreateCaseV1::WriterDirectLockObservations => {
                let mut counters = OperationCountersV1::default();
                let mut control = ContinueFaultControl;
                let terminal = run_semantic_create(
                    &cas,
                    0x0011_5501,
                    1,
                    SliceSupplier { bytes: &[0x5b] },
                    &mut control,
                    &mut counters,
                );
                observed!(terminal, false, counters)
            }
        }
    }

    fn outer_create_trace(boundaries: &[FsCasBoundaryV1]) -> Vec<FsCasBoundaryV1> {
        boundaries
            .iter()
            .copied()
            .filter(|boundary| {
                matches!(
                    boundary,
                    FsCasBoundaryV1::BeforeOperationSlotReservationRequest
                        | FsCasBoundaryV1::BeforeClosureMarkerPublication
                        | FsCasBoundaryV1::AfterClosureMarkerLink
                        | FsCasBoundaryV1::AfterCompleteValidatedHandoff
                )
            })
            .collect()
    }

    fn run_equivalent_single_create(
        cas: &FsCasV1,
        key: u64,
        control: &mut EquivalentCreateTraceControl,
        counters: &mut OperationCountersV1,
    ) -> Result<super::OperationHandoffV1, OperationErrorV1> {
        run_semantic_create(
            cas,
            key,
            b"one-file-create".len() as u64,
            SliceSupplier {
                bytes: b"one-file-create",
            },
            control,
            counters,
        )
    }

    fn run_equivalent_tree_create(
        cas: &FsCasV1,
        key: u64,
        control: &mut EquivalentCreateTraceControl,
        counters: &mut OperationCountersV1,
    ) -> Result<super::OperationHandoffV1, OperationErrorV1> {
        let one = b"one-file-create";
        let two = b"second-entry-create";
        let mut files = [
            TreeFileV1::new(
                b"one.txt",
                0o644,
                one.len() as u64,
                SliceSupplier { bytes: one },
            ),
            TreeFileV1::new(
                b"two.txt",
                0o644,
                two.len() as u64,
                SliceSupplier { bytes: two },
            ),
        ];
        let operation = request_tree_operation_v1(cas, key, counters, control)
            .map_err(OperationErrorV1::FsCas)?;
        let mut scratch = OperationScratch::new();
        run_create_tree_v1(
            operation,
            CdcAlgorithmV1::FastCdc,
            &mut files,
            scratch.borrow(),
            control,
            counters,
        )
    }

    pub fn equivalent_create_lifecycle_v1(root: &Path) -> EquivalentCreateLifecycleObservationV1 {
        fs::create_dir_all(root).expect("equivalent Create parent root");

        let success_one_root = root.join("success-one");
        let (success_one_cas, _success_one_stale) = new_fault_root(&success_one_root);
        let mut success_one_control = EquivalentCreateTraceControl::default();
        let mut success_one_counters = OperationCountersV1::default();
        run_equivalent_single_create(
            &success_one_cas,
            0x601,
            &mut success_one_control,
            &mut success_one_counters,
        )
        .expect("one-file Create succeeds");

        let success_tree_root = root.join("success-tree");
        let (success_tree_cas, _success_tree_stale) = new_fault_root(&success_tree_root);
        let mut success_tree_control = EquivalentCreateTraceControl::default();
        let mut success_tree_counters = OperationCountersV1::default();
        run_equivalent_tree_create(
            &success_tree_cas,
            0x602,
            &mut success_tree_control,
            &mut success_tree_counters,
        )
        .expect("multi-entry Create succeeds");

        let failed_one_root = root.join("failed-one");
        let (failed_one_cas, _failed_one_stale) = new_fault_root(&failed_one_root);
        let mut failed_one_control = EquivalentCreateTraceControl {
            fail_marker_hard_link: true,
            ..EquivalentCreateTraceControl::default()
        };
        let mut failed_one_counters = OperationCountersV1::default();
        let failed_one = run_equivalent_single_create(
            &failed_one_cas,
            0x603,
            &mut failed_one_control,
            &mut failed_one_counters,
        )
        .expect_err("one-file Create surfaces marker failure");

        let failed_tree_root = root.join("failed-tree");
        let (failed_tree_cas, _failed_tree_stale) = new_fault_root(&failed_tree_root);
        let mut failed_tree_control = EquivalentCreateTraceControl {
            fail_marker_hard_link: true,
            ..EquivalentCreateTraceControl::default()
        };
        let mut failed_tree_counters = OperationCountersV1::default();
        let failed_tree = run_equivalent_tree_create(
            &failed_tree_cas,
            0x604,
            &mut failed_tree_control,
            &mut failed_tree_counters,
        )
        .expect_err("multi-entry Create surfaces marker failure");

        let success_one_trace = outer_create_trace(&success_one_control.boundaries);
        let success_tree_trace = outer_create_trace(&success_tree_control.boundaries);
        let failed_one_trace = outer_create_trace(&failed_one_control.boundaries);
        let failed_tree_trace = outer_create_trace(&failed_tree_control.boundaries);
        EquivalentCreateLifecycleObservationV1 {
            success_traces_equal: success_one_trace == success_tree_trace,
            success_starts_at_slot_reservation: success_one_trace.first()
                == Some(&FsCasBoundaryV1::BeforeOperationSlotReservationRequest),
            success_ends_at_validated_handoff: success_one_trace.last()
                == Some(&FsCasBoundaryV1::AfterCompleteValidatedHandoff),
            failed_one_control_fired: failed_one_control.marker_hard_link_failed,
            failed_tree_control_fired: failed_tree_control.marker_hard_link_failed,
            failure_errors_equal: failed_one == failed_tree,
            failure_traces_equal: failed_one_trace == failed_tree_trace,
            failure_trace_has_no_handoff: !failed_one_trace
                .contains(&FsCasBoundaryV1::AfterCompleteValidatedHandoff),
            failed_one_clean: operation_authority_is_clean(&failed_one_cas, &failed_one_root),
            failed_tree_clean: operation_authority_is_clean(&failed_tree_cas, &failed_tree_root),
            success_one_counters: complete_create_counters(&success_one_counters),
            success_tree_counters: complete_create_counters(&success_tree_counters),
            failed_one_counters: complete_create_counters(&failed_one_counters),
            failed_tree_counters: complete_create_counters(&failed_tree_counters),
        }
    }

    fn storage_equations_hold(counters: &OperationCountersV1) -> bool {
        counters.storage_bytes_requested == counters.storage_bytes_reserved
            && counters.storage_inodes_requested == counters.storage_inodes_reserved
            && counters.storage_bytes_reserved
                == counters
                    .storage_bytes_released
                    .checked_add(counters.storage_bytes_committed)
                    .and_then(|value| value.checked_add(counters.storage_bytes_retained))
                    .unwrap_or(u64::MAX)
            && counters.storage_inodes_reserved
                == counters
                    .storage_inodes_released
                    .checked_add(counters.storage_inodes_committed)
                    .and_then(|value| value.checked_add(counters.storage_inodes_retained))
                    .unwrap_or(u64::MAX)
    }

    fn operation_authority_is_clean(cas: &FsCasV1, root: &Path) -> bool {
        cas.operation_admitted_slots_v1() == 0
            && cas.operation_admission_active_for_test_v1() == 0
            && cas.operation_admission_queue_for_test_v1() == (0, 0, 0)
            && cas.storage_admission_active_for_test_v1() == (0, 0, 0)
            && fs::read_dir(root.join("preparation"))
                .expect("semantic preparation namespace")
                .count()
                == 0
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum PackAdmissionObservationV1 {
        Installed,
        ExistingComplete,
    }

    fn pack_admission_observation(
        outcome: crate::cas::FsPackAdmissionOutcomeV1,
    ) -> PackAdmissionObservationV1 {
        match outcome {
            crate::cas::FsPackAdmissionOutcomeV1::Installed => {
                PackAdmissionObservationV1::Installed
            }
            crate::cas::FsPackAdmissionOutcomeV1::ExistingComplete => {
                PackAdmissionObservationV1::ExistingComplete
            }
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ConcurrentOperationCountersObservationV1 {
        pub storage_equations_hold: bool,
        pub zero_forbidden_work: bool,
        pub storage_bytes_requested: u64,
        pub storage_bytes_released: u64,
        pub storage_inodes_requested: u64,
        pub storage_inodes_released: u64,
        pub visibility_lock_acquisitions: u64,
        pub visibility_lock_wait_nanoseconds: u64,
        pub visibility_lock_hold_nanoseconds: u64,
        pub visibility_lock_hold_nanoseconds_high_water: u64,
        pub publication_lock_acquisitions: u64,
        pub publication_lock_wait_nanoseconds: u64,
        pub publication_lock_hold_nanoseconds: u64,
        pub publication_lock_hold_nanoseconds_high_water: u64,
        pub preparation_bytes_after_cleanup: u64,
        pub preparation_inodes_after_cleanup: u64,
        pub mutable_residue_bytes: u64,
        pub mutable_residue_inodes: u64,
        pub storage_bytes_retained: u64,
        pub storage_inodes_retained: u64,
        pub storage_bytes_committed: u64,
        pub storage_inodes_committed: u64,
        pub storage_bytes_reserved: u64,
        pub storage_inodes_reserved: u64,
        pub preparation_bytes_high_water: u64,
        pub preparation_inodes_high_water: u64,
        pub open_handles_high_water: u64,
        pub memory_high_water: u64,
        pub active_slots_high_water: u64,
        pub root_admission_wait_polls: u64,
        pub root_admission_wait_nanoseconds: u64,
        pub root_admission_queue_entries: u64,
        pub root_admission_queue_refusals: u64,
        pub root_reserved_bytes_high_water: u64,
        pub root_reserved_inodes_high_water: u64,
        pub source_bytes_read: u64,
        pub file_sort_control_polls: u64,
        pub active_pack_publication_wait_polls: u64,
        pub active_pack_publication_wait_nanoseconds: u64,
        pub locator_owner_publication_wait_polls: u64,
        pub locator_owner_publication_wait_nanoseconds: u64,
        pub incumbent_comparison_windows: u64,
        pub maximum_active_carrier_bytes: u64,
        pub unreachable_installed_residue_bytes: u64,
    }

    fn concurrent_counters(
        counters: &OperationCountersV1,
    ) -> ConcurrentOperationCountersObservationV1 {
        assert_eq!(
            counters.storage_bytes_requested,
            counters.storage_bytes_reserved
        );
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
        ConcurrentOperationCountersObservationV1 {
            storage_equations_hold: storage_equations_hold(counters),
            zero_forbidden_work: counters.has_zero_forbidden_work(),
            storage_bytes_requested: counters.storage_bytes_requested,
            storage_bytes_released: counters.storage_bytes_released,
            storage_inodes_requested: counters.storage_inodes_requested,
            storage_inodes_released: counters.storage_inodes_released,
            visibility_lock_acquisitions: counters.visibility_lock_acquisitions,
            visibility_lock_wait_nanoseconds: counters.visibility_lock_wait_nanoseconds,
            visibility_lock_hold_nanoseconds: counters.visibility_lock_hold_nanoseconds,
            visibility_lock_hold_nanoseconds_high_water: counters
                .visibility_lock_hold_nanoseconds_high_water,
            publication_lock_acquisitions: counters.publication_lock_acquisitions,
            publication_lock_wait_nanoseconds: counters.publication_lock_wait_nanoseconds,
            publication_lock_hold_nanoseconds: counters.publication_lock_hold_nanoseconds,
            publication_lock_hold_nanoseconds_high_water: counters
                .publication_lock_hold_nanoseconds_high_water,
            preparation_bytes_after_cleanup: counters
                .storage_preparation_bytes_current_after_cleanup,
            preparation_inodes_after_cleanup: counters
                .storage_preparation_inodes_current_after_cleanup,
            mutable_residue_bytes: counters.mutable_preparation_residue_bytes,
            mutable_residue_inodes: counters.mutable_preparation_residue_inodes,
            storage_bytes_retained: counters.storage_bytes_retained,
            storage_inodes_retained: counters.storage_inodes_retained,
            storage_bytes_committed: counters.storage_bytes_committed,
            storage_inodes_committed: counters.storage_inodes_committed,
            storage_bytes_reserved: counters.storage_bytes_reserved,
            storage_inodes_reserved: counters.storage_inodes_reserved,
            preparation_bytes_high_water: counters.storage_preparation_bytes_high_water,
            preparation_inodes_high_water: counters.storage_preparation_inodes_high_water,
            open_handles_high_water: counters.layerfs_open_file_handles_high_water,
            memory_high_water: counters.memory_high_water,
            active_slots_high_water: counters.root_admission_active_slots_high_water,
            root_admission_wait_polls: counters.root_admission_wait_polls,
            root_admission_wait_nanoseconds: counters.root_admission_wait_nanoseconds,
            root_admission_queue_entries: counters.root_admission_queue_entries,
            root_admission_queue_refusals: counters.root_admission_queue_refusals,
            root_reserved_bytes_high_water: counters
                .root_storage_active_reserved_bytes_lifetime_high_water,
            root_reserved_inodes_high_water: counters
                .root_storage_active_reserved_inodes_lifetime_high_water,
            source_bytes_read: counters.source_bytes_read,
            file_sort_control_polls: counters.file_sort_control_polls,
            active_pack_publication_wait_polls: counters.active_pack_publication_wait_polls,
            active_pack_publication_wait_nanoseconds: counters
                .active_pack_publication_wait_nanoseconds,
            locator_owner_publication_wait_polls: counters.locator_owner_publication_wait_polls,
            locator_owner_publication_wait_nanoseconds: counters
                .locator_owner_publication_wait_nanoseconds,
            incumbent_comparison_windows: counters.incumbent_comparison_windows,
            maximum_active_carrier_bytes: counters.maximum_active_carrier_bytes,
            unreachable_installed_residue_bytes: counters.unreachable_installed_residue_bytes,
        }
    }

    fn assert_storage_equations(counters: &OperationCountersV1) {
        assert_eq!(
            counters.storage_bytes_requested,
            counters.storage_bytes_reserved
        );
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
    }

    fn assert_balanced_storage_terminal(counters: &OperationCountersV1) {
        assert!(counters.storage_bytes_requested > 0);
        assert_storage_equations(counters);
        assert!(
            counters.root_storage_active_reserved_bytes_lifetime_high_water
                >= counters.storage_bytes_reserved
        );
        assert!(
            counters.root_storage_active_reserved_inodes_lifetime_high_water
                >= counters.storage_inodes_reserved
        );
        assert_eq!(counters.storage_bytes_retained, 0);
        assert_eq!(counters.storage_inodes_retained, 0);
        assert_eq!(counters.storage_preparation_bytes_current_after_cleanup, 0);
        assert_eq!(counters.storage_preparation_inodes_current_after_cleanup, 0);
        assert_eq!(counters.mutable_preparation_residue_bytes, 0);
        assert_eq!(counters.mutable_preparation_residue_inodes, 0);
        assert_eq!(counters.unreachable_installed_residue_bytes, 0);
        assert!(counters.has_zero_forbidden_work());
    }

    fn assert_read_storage_terminal(counters: &OperationCountersV1) {
        assert_eq!(counters.storage_bytes_requested, 0);
        assert_eq!(counters.storage_bytes_reserved, 0);
        assert_eq!(counters.storage_bytes_released, 0);
        assert_eq!(counters.storage_bytes_committed, 0);
        assert_eq!(counters.storage_bytes_retained, 0);
        assert_eq!(counters.storage_inodes_requested, 0);
        assert_eq!(counters.storage_inodes_reserved, 0);
        assert_eq!(counters.storage_inodes_released, 0);
        assert_eq!(counters.storage_inodes_committed, 0);
        assert_eq!(counters.storage_inodes_retained, 0);
        assert_eq!(counters.storage_preparation_bytes_current_after_cleanup, 0);
        assert_eq!(counters.storage_preparation_inodes_current_after_cleanup, 0);
        assert_eq!(counters.mutable_preparation_residue_bytes, 0);
        assert_eq!(counters.mutable_preparation_residue_inodes, 0);
        assert!(counters.visibility_lock_acquisitions > 0);
        assert_eq!(counters.publication_lock_acquisitions, 0);
        assert!(counters.has_zero_forbidden_work());
    }

    struct SignalActivePackPublicationWaitV1 {
        reached: Option<mpsc::SyncSender<()>>,
    }

    impl CdcControlV1 for SignalActivePackPublicationWaitV1 {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for SignalActivePackPublicationWaitV1 {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary == FsCasBoundaryV1::ActivePackPublicationWait {
                if let Some(reached) = self.reached.take() {
                    reached.send(()).expect("active-publication wait receiver");
                }
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum CarrierAlreadyExistsTerminalObservationV1 {
        Success,
        CallbackUnwind,
        CleanupFailure,
    }

    struct CarrierAlreadyExistsRaceControlV1 {
        restore_requested: Option<mpsc::SyncSender<()>>,
        restore_completed: mpsc::Receiver<Result<(), String>>,
        comparison_entered: Option<mpsc::SyncSender<()>>,
        comparison_release: mpsc::Receiver<()>,
        terminal: CarrierAlreadyExistsTerminalObservationV1,
        no_replace_injected: bool,
        comparison_gated: bool,
        cleanup_failed: bool,
    }

    impl CdcControlV1 for CarrierAlreadyExistsRaceControlV1 {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for CarrierAlreadyExistsRaceControlV1 {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary != FsCasBoundaryV1::BeforeIncumbentComparisonWindow || self.comparison_gated
            {
                return;
            }
            self.comparison_gated = true;
            self.comparison_entered
                .take()
                .expect("comparison entry signal")
                .send(())
                .expect("comparison entry receiver");
            self.comparison_release
                .recv_timeout(Duration::from_secs(5))
                .expect("comparison release gate");
            if self.terminal == CarrierAlreadyExistsTerminalObservationV1::CallbackUnwind {
                panic!("injected incumbent-comparison callback unwind");
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if self.terminal == CarrierAlreadyExistsTerminalObservationV1::CleanupFailure
                && target == FsCasCleanupTargetV1::PrivatePack
                && !self.cleanup_failed
            {
                self.cleanup_failed = true;
                true
            } else {
                false
            }
        }

        fn before_carrier_no_replace_transition_for_test_v1(&mut self) {
            if self.no_replace_injected {
                panic!("carrier no-replace transition repeated");
            }
            self.restore_requested
                .take()
                .expect("carrier restore request")
                .send(())
                .expect("independent winner receiver");
            self.restore_completed
                .recv_timeout(Duration::from_secs(5))
                .expect("independent winner response")
                .unwrap_or_else(|error| panic!("independent winner failed: {error}"));
            self.no_replace_injected = true;
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum ContenderProgressObservationV1 {
        Blocked,
        Completed,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum ConcurrentWriterTerminalObservationV1 {
        Succeeded,
        CallbackUnwind,
        CleanupFailed,
        Invalidated,
        OtherFailure,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct CarrierAlreadyExistsObservationV1 {
        pub terminal: CarrierAlreadyExistsTerminalObservationV1,
        pub contender_progress: ContenderProgressObservationV1,
        pub no_replace_injected: bool,
        pub comparison_gated: bool,
        pub counters: [ConcurrentOperationCountersObservationV1; 2],
        pub first_result: ConcurrentWriterTerminalObservationV1,
        pub contender_result: ConcurrentWriterTerminalObservationV1,
        pub cleanup_failed: bool,
        pub preparation_usage: (u64, u64),
        pub operation_slots: u64,
        pub operation_active: u64,
        pub storage_active: (u64, u64, u64),
        pub queue_after: (u64, u64, u64),
        pub seed_state: RootStateObservationV1,
        pub stale_state: RootStateObservationV1,
        pub reopen_state: OpenExistingObservationV1,
        pub authority_clean: bool,
    }

    pub fn carrier_already_exists_owner_v1(
        rows: [(&Path, &Path); 3],
    ) -> [CarrierAlreadyExistsObservationV1; 3] {
        let terminals = [
            CarrierAlreadyExistsTerminalObservationV1::Success,
            CarrierAlreadyExistsTerminalObservationV1::CallbackUnwind,
            CarrierAlreadyExistsTerminalObservationV1::CleanupFailure,
        ];
        std::array::from_fn(|index| {
            let (root, held) = rows[index];
            let terminal = terminals[index];
            let seed = FsCasV1::create_new(root).expect("carrier-race semantic root");
            let mut seed_control = ContinueControl;
            let mut seed_counters = OperationCountersV1::default();
            run_semantic_create(
                &seed,
                0x0011_5570,
                1,
                SliceSupplier { bytes: &[0x5a] },
                &mut seed_control,
                &mut seed_counters,
            )
            .expect("carrier-race seed");
            assert_storage_equations(&seed_counters);

            let first_cas = FsCasV1::open_existing(root).expect("first carrier-race writer");
            let contender_cas =
                FsCasV1::open_existing(root).expect("contender carrier-race writer");
            let stale = FsCasV1::open_existing(root).expect("stale carrier-race owner");
            fs::create_dir_all(held).expect("carrier-race held directory");
            let carrier = fs::read_dir(root.join("carriers"))
                .expect("seed carrier directory")
                .next()
                .expect("seed carrier")
                .expect("seed carrier entry")
                .path();
            let catalog = fs::read_dir(root.join("catalog"))
                .expect("seed catalog directory")
                .next()
                .expect("seed catalog")
                .expect("seed catalog entry")
                .path();
            let held_carrier = held.join("carrier");
            let held_catalog = held.join("catalog");
            fs::rename(&carrier, &held_carrier).expect("hold carrier");
            fs::rename(&catalog, &held_catalog).expect("hold catalog");

            let (restore_request_tx, restore_request_rx) = mpsc::sync_channel(0);
            let (restore_complete_tx, restore_complete_rx) = mpsc::sync_channel(0);
            let (comparison_entered_tx, comparison_entered_rx) = mpsc::sync_channel(1);
            let (comparison_release_tx, comparison_release_rx) = mpsc::sync_channel(0);
            let (wait_tx, wait_rx) = mpsc::sync_channel(1);
            let (contender_done_tx, contender_done_rx) = mpsc::sync_channel(1);

            let (
                contender_progress,
                first_result,
                first_counters,
                control,
                contender_result,
                contender_counters,
            ) = std::thread::scope(|scope| {
                let installer = scope.spawn(move || {
                    let result = restore_request_rx
                        .recv_timeout(Duration::from_secs(5))
                        .map_err(|error| format!("restore request: {error}"))
                        .and_then(|()| {
                            fs::rename(&held_carrier, &carrier)
                                .map_err(|error| format!("carrier restore: {error}"))?;
                            fs::rename(&held_catalog, &catalog)
                                .map_err(|error| format!("catalog restore: {error}"))?;
                            Ok(())
                        });
                    let _ = restore_complete_tx.send(result.clone());
                    result
                });
                let first = scope.spawn(move || {
                    let mut control = CarrierAlreadyExistsRaceControlV1 {
                        restore_requested: Some(restore_request_tx),
                        restore_completed: restore_complete_rx,
                        comparison_entered: Some(comparison_entered_tx),
                        comparison_release: comparison_release_rx,
                        terminal,
                        no_replace_injected: false,
                        comparison_gated: false,
                        cleanup_failed: false,
                    };
                    let mut counters = OperationCountersV1::default();
                    let terminal = catch_unwind(AssertUnwindSafe(|| {
                        run_semantic_create(
                            &first_cas,
                            0x0011_5571,
                            1,
                            SliceSupplier { bytes: &[0x5a] },
                            &mut control,
                            &mut counters,
                        )
                    }));
                    (terminal, counters, control)
                });

                comparison_entered_rx
                    .recv_timeout(Duration::from_secs(5))
                    .expect("carrier-race comparison entry");
                let contender = scope.spawn(move || {
                    let mut control = SignalActivePackPublicationWaitV1 {
                        reached: Some(wait_tx),
                    };
                    let mut counters = OperationCountersV1::default();
                    let terminal = run_semantic_create(
                        &contender_cas,
                        0x0011_5572,
                        1,
                        SliceSupplier { bytes: &[0x5a] },
                        &mut control,
                        &mut counters,
                    );
                    let _ = contender_done_tx.send(());
                    (terminal, counters)
                });
                wait_rx
                    .recv_timeout(Duration::from_secs(5))
                    .expect("carrier-race active-owner wait");
                let contender_progress = match contender_done_rx
                    .recv_timeout(Duration::from_millis(100))
                {
                    Err(mpsc::RecvTimeoutError::Timeout) => ContenderProgressObservationV1::Blocked,
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                        ContenderProgressObservationV1::Completed
                    }
                };
                let _ = comparison_release_tx.send(());
                installer
                    .join()
                    .expect("carrier-race installer thread")
                    .unwrap_or_else(|error| panic!("carrier-race installer failed: {error}"));
                let (first_result, first_counters, control) =
                    first.join().expect("first carrier-race writer thread");
                let (contender_result, contender_counters) = contender
                    .join()
                    .expect("contender carrier-race writer thread");
                (
                    contender_progress,
                    first_result,
                    first_counters,
                    control,
                    contender_result,
                    contender_counters,
                )
            });

            if terminal == CarrierAlreadyExistsTerminalObservationV1::CallbackUnwind {
                assert!(first_result.is_err());
            }
            if terminal == CarrierAlreadyExistsTerminalObservationV1::CleanupFailure {
                assert!(matches!(
                    &first_result,
                    Ok(Err(OperationErrorV1::FsCas(FsCasErrorV1::CleanupFailed(
                        FsCasCleanupTargetV1::PrivatePack
                    ))))
                ));
                assert!(control.cleanup_failed);
            }
            assert_storage_equations(&first_counters);
            assert_storage_equations(&contender_counters);
            let first_result = match first_result {
                Ok(Ok(_)) => ConcurrentWriterTerminalObservationV1::Succeeded,
                Err(_) => ConcurrentWriterTerminalObservationV1::CallbackUnwind,
                Ok(Err(OperationErrorV1::FsCas(FsCasErrorV1::CleanupFailed(
                    FsCasCleanupTargetV1::PrivatePack,
                )))) => ConcurrentWriterTerminalObservationV1::CleanupFailed,
                Ok(Err(_)) => ConcurrentWriterTerminalObservationV1::OtherFailure,
            };
            let contender_result = match contender_result {
                Ok(_) => ConcurrentWriterTerminalObservationV1::Succeeded,
                Err(OperationErrorV1::FsCas(FsCasErrorV1::Invalidated)) => {
                    ConcurrentWriterTerminalObservationV1::Invalidated
                }
                Err(_) => ConcurrentWriterTerminalObservationV1::OtherFailure,
            };
            CarrierAlreadyExistsObservationV1 {
                terminal,
                contender_progress,
                no_replace_injected: control.no_replace_injected,
                comparison_gated: control.comparison_gated,
                counters: [
                    concurrent_counters(&first_counters),
                    concurrent_counters(&contender_counters),
                ],
                first_result,
                contender_result,
                cleanup_failed: control.cleanup_failed,
                preparation_usage: directory_usage(&root.join("preparation")),
                operation_slots: seed.operation_admitted_slots_v1(),
                operation_active: seed.operation_admission_active_for_test_v1(),
                storage_active: seed.storage_admission_active_for_test_v1(),
                queue_after: seed.operation_admission_queue_for_test_v1(),
                seed_state: root_state(&seed),
                stale_state: root_state(&stale),
                reopen_state: open_existing_v1(root),
                authority_clean: operation_authority_is_clean(&seed, root),
            }
        })
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum PreCatalogUnwindBoundaryObservationV1 {
        AfterCarrierInstall,
        AfterObjectLocatorPublication,
    }

    struct BarrierPanicAtPackPublicationV1 {
        target: FsCasBoundaryV1,
        entered_signal: mpsc::SyncSender<()>,
        release: Arc<WatchdogGateV1>,
        injected: bool,
    }

    impl CdcControlV1 for BarrierPanicAtPackPublicationV1 {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for BarrierPanicAtPackPublicationV1 {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if !self.injected && boundary == self.target {
                self.injected = true;
                self.entered_signal
                    .send(())
                    .expect("pre-catalog publication barrier receiver");
                self.release.wait();
                panic!("injected pre-catalog publication unwind at {boundary:?}");
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct PreCatalogUnwindObservationV1 {
        pub target: PreCatalogUnwindBoundaryObservationV1,
        pub contender_progress: ContenderProgressObservationV1,
        pub injected: bool,
        pub first_result: ConcurrentWriterTerminalObservationV1,
        pub counters: [ConcurrentOperationCountersObservationV1; 2],
        pub contender_result: ConcurrentWriterTerminalObservationV1,
        pub carrier_entries: usize,
        pub catalog_entries: usize,
        pub seed_state: RootStateObservationV1,
        pub stale_state: RootStateObservationV1,
        pub reopen_state: OpenExistingObservationV1,
        pub authority_clean: bool,
    }

    pub fn same_pack_pre_catalog_unwind_v1(
        roots: [&Path; 2],
    ) -> [PreCatalogUnwindObservationV1; 2] {
        let targets = [
            (
                FsCasBoundaryV1::AfterCarrierInstall,
                PreCatalogUnwindBoundaryObservationV1::AfterCarrierInstall,
            ),
            (
                FsCasBoundaryV1::AfterObjectLocatorPublication,
                PreCatalogUnwindBoundaryObservationV1::AfterObjectLocatorPublication,
            ),
        ];
        std::array::from_fn(|index| {
            let root = roots[index];
            let (target, target_observation) = targets[index];
            let seed = FsCasV1::create_new(root).expect("pre-catalog semantic root");
            let first_cas = FsCasV1::open_existing(root).expect("first pre-catalog writer");
            let contender_cas = FsCasV1::open_existing(root).expect("contender pre-catalog writer");
            let stale = FsCasV1::open_existing(root).expect("stale pre-catalog owner");
            let release = Arc::new(WatchdogGateV1::new());
            let mut release_guard = WatchdogGateReleaseV1::new(Arc::clone(&release));
            let (entered_tx, entered_rx) = mpsc::sync_channel(1);
            let (wait_tx, wait_rx) = mpsc::sync_channel(1);
            let (contender_done_tx, contender_done_rx) = mpsc::sync_channel(1);

            let (
                contender_progress,
                first_terminal,
                first_counters,
                injected,
                contender_terminal,
                contender_counters,
            ) = std::thread::scope(|scope| {
                let first_release = Arc::clone(&release);
                let first = scope.spawn(move || {
                    let mut control = BarrierPanicAtPackPublicationV1 {
                        target,
                        entered_signal: entered_tx,
                        release: first_release,
                        injected: false,
                    };
                    let mut counters = OperationCountersV1::default();
                    let terminal = catch_unwind(AssertUnwindSafe(|| {
                        run_semantic_create(
                            &first_cas,
                            0x0011_5580,
                            1,
                            SliceSupplier { bytes: &[0x5b] },
                            &mut control,
                            &mut counters,
                        )
                    }));
                    (terminal, counters, control.injected)
                });
                entered_rx
                    .recv_timeout(Duration::from_secs(5))
                    .expect("pre-catalog target boundary");
                let contender = scope.spawn(move || {
                    let mut control = SignalActivePackPublicationWaitV1 {
                        reached: Some(wait_tx),
                    };
                    let mut counters = OperationCountersV1::default();
                    let terminal = run_semantic_create(
                        &contender_cas,
                        0x0011_5581,
                        1,
                        SliceSupplier { bytes: &[0x5b] },
                        &mut control,
                        &mut counters,
                    );
                    let _ = contender_done_tx.send(());
                    (terminal, counters)
                });
                wait_rx
                    .recv_timeout(Duration::from_secs(5))
                    .expect("pre-catalog active publication wait");
                let contender_progress = match contender_done_rx
                    .recv_timeout(Duration::from_millis(100))
                {
                    Err(mpsc::RecvTimeoutError::Timeout) => ContenderProgressObservationV1::Blocked,
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                        ContenderProgressObservationV1::Completed
                    }
                };
                release_guard.release();
                let (first_terminal, first_counters, injected) =
                    first.join().expect("first pre-catalog writer thread");
                let (contender_terminal, contender_counters) = contender
                    .join()
                    .expect("contender pre-catalog writer thread");
                (
                    contender_progress,
                    first_terminal,
                    first_counters,
                    injected,
                    contender_terminal,
                    contender_counters,
                )
            });
            assert!(first_terminal.is_err());
            assert_storage_equations(&first_counters);
            assert_storage_equations(&contender_counters);
            assert!(first_counters.has_zero_forbidden_work());
            assert!(contender_counters.has_zero_forbidden_work());
            if target == FsCasBoundaryV1::AfterCarrierInstall {
                assert!(stale.occupied().is_ok());
            }
            PreCatalogUnwindObservationV1 {
                target: target_observation,
                contender_progress,
                injected,
                first_result: if first_terminal.is_err() {
                    ConcurrentWriterTerminalObservationV1::CallbackUnwind
                } else {
                    ConcurrentWriterTerminalObservationV1::OtherFailure
                },
                counters: [
                    concurrent_counters(&first_counters),
                    concurrent_counters(&contender_counters),
                ],
                contender_result: match contender_terminal {
                    Ok(_) => ConcurrentWriterTerminalObservationV1::Succeeded,
                    Err(OperationErrorV1::FsCas(FsCasErrorV1::Invalidated)) => {
                        ConcurrentWriterTerminalObservationV1::Invalidated
                    }
                    Err(_) => ConcurrentWriterTerminalObservationV1::OtherFailure,
                },
                carrier_entries: fs::read_dir(root.join("carriers"))
                    .expect("pre-catalog carriers")
                    .count(),
                catalog_entries: fs::read_dir(root.join("catalog"))
                    .expect("pre-catalog catalog")
                    .count(),
                seed_state: root_state(&seed),
                stale_state: root_state(&stale),
                reopen_state: open_existing_v1(root),
                authority_clean: operation_authority_is_clean(&seed, root),
            }
        })
    }

    #[derive(Debug, Eq, PartialEq)]
    pub struct ReopenedCompleteWritersObservationV1 {
        pub equal: bool,
        pub counters: [ConcurrentOperationCountersObservationV1; 2],
        pub version_identity_equal: bool,
        pub root_identity_equal: bool,
        pub pack_identity_equal: bool,
        pub installed_outcomes: usize,
        pub existing_outcomes: usize,
        pub carriers_installed: u32,
        pub carriers_reused: u32,
        pub carrier_entries: usize,
        pub preparation_usage: (u64, u64),
        pub committed_usage: (u64, u64),
        pub immutable_usage: (u64, u64),
        pub installer_pack_bytes_owned: bool,
        pub installer_pack_inodes_owned: bool,
        pub adopter_committed_bytes: u64,
        pub adopter_committed_inodes: u64,
        pub closure_usage: (u64, u64),
        pub adopter_byte_equation_holds: bool,
        pub adopter_inode_equation_holds: bool,
        pub catalog_entries: usize,
        pub closure_entries: usize,
        pub left_outcome: PackAdmissionObservationV1,
        pub right_outcome: PackAdmissionObservationV1,
        pub left_carriers_installed: u32,
        pub right_carriers_installed: u32,
        pub left_carriers_reused: u32,
        pub right_carriers_reused: u32,
        pub root_clean: bool,
        pub queue_after: (u64, u64, u64),
    }

    pub fn simultaneous_reopened_complete_writers_v1(
        roots: [&Path; 2],
    ) -> [ReopenedCompleteWritersObservationV1; 2] {
        let rows = [(0x61_u8, 0x61_u8, true), (0x62_u8, 0x63_u8, false)];
        std::array::from_fn(|index| {
            let root = roots[index];
            let (left_byte, right_byte, equal) = rows[index];
            let seed = FsCasV1::create_new(root).expect("reopened-writer semantic root");
            let left_cas = FsCasV1::open_existing(root).expect("left reopened writer");
            let right_cas = FsCasV1::open_existing(root).expect("right reopened writer");
            let start = Arc::new(WatchdogGateV1::new());
            let (ready_tx, ready_rx) = mpsc::sync_channel(2);
            let ((left_terminal, left_counters), (right_terminal, right_counters)) =
                std::thread::scope(|scope| {
                    let mut release = WatchdogGateReleaseV1::new(Arc::clone(&start));
                    let left_start = Arc::clone(&start);
                    let left_ready = ready_tx.clone();
                    let left = scope.spawn(move || {
                        let input = [left_byte];
                        let mut control = ContinueControl;
                        let mut counters = OperationCountersV1::default();
                        let terminal = run_semantic_create(
                            &left_cas,
                            0x0011_5590,
                            1,
                            BarrierSliceSupplier {
                                bytes: &input,
                                ready: left_ready,
                                start: left_start,
                            },
                            &mut control,
                            &mut counters,
                        );
                        (terminal, counters)
                    });
                    let right_start = Arc::clone(&start);
                    let right = scope.spawn(move || {
                        let input = [right_byte];
                        let mut control = ContinueControl;
                        let mut counters = OperationCountersV1::default();
                        let terminal = run_semantic_create(
                            &right_cas,
                            0x0011_5591,
                            1,
                            BarrierSliceSupplier {
                                bytes: &input,
                                ready: ready_tx,
                                start: right_start,
                            },
                            &mut control,
                            &mut counters,
                        );
                        (terminal, counters)
                    });
                    ready_rx
                        .recv_timeout(Duration::from_secs(5))
                        .expect("left reopened-writer rendezvous");
                    ready_rx
                        .recv_timeout(Duration::from_secs(5))
                        .expect("right reopened-writer rendezvous");
                    release.release();
                    (
                        left.join().expect("left reopened writer thread"),
                        right.join().expect("right reopened writer thread"),
                    )
                });
            let left = left_terminal.expect("left reopened writer terminal");
            let right = right_terminal.expect("right reopened writer terminal");
            let left_outcome = pack_admission_observation(left.pack_outcome());
            let right_outcome = pack_admission_observation(right.pack_outcome());
            let outcomes = [left_outcome, right_outcome];
            let preparation_usage = clean_preparation_usage(root);
            let (preparation_bytes, preparation_inodes) = preparation_usage;
            let immutable_usage = immutable_usage(root);
            let immutable_bytes = immutable_usage.0;
            let immutable_inodes = immutable_usage.1;
            if equal {
                assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
                assert_eq!(
                    left_counters.storage_bytes_committed + right_counters.storage_bytes_committed,
                    immutable_bytes
                );
                assert_eq!(
                    left_counters.storage_inodes_committed
                        + right_counters.storage_inodes_committed,
                    immutable_inodes
                );
            } else {
                assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
                assert_eq!(
                    left_counters.storage_bytes_committed + right_counters.storage_bytes_committed,
                    immutable_bytes
                );
                assert_eq!(
                    left_counters.storage_inodes_committed
                        + right_counters.storage_inodes_committed,
                    immutable_inodes
                );
            }
            let committed_usage = (
                left_counters.storage_bytes_committed + right_counters.storage_bytes_committed,
                left_counters.storage_inodes_committed + right_counters.storage_inodes_committed,
            );
            let (installer, adopter) = if left_outcome == PackAdmissionObservationV1::Installed {
                (&left_counters, &right_counters)
            } else {
                (&right_counters, &left_counters)
            };
            let pack_usage = ["carriers", "objects", "catalog"]
                .into_iter()
                .map(|name| directory_usage(&root.join(name)))
                .fold((0, 0), |(bytes, inodes), next| {
                    (bytes + next.0, inodes + next.1)
                });
            let closure_usage = directory_usage(&root.join("closures"));
            let root_clean = operation_authority_is_clean(&seed, root)
                && root_state(&seed) == RootStateObservationV1::Usable;
            ReopenedCompleteWritersObservationV1 {
                equal,
                counters: [
                    concurrent_counters(&left_counters),
                    concurrent_counters(&right_counters),
                ],
                version_identity_equal: left.version_record() == right.version_record(),
                root_identity_equal: left.root_tree() == right.root_tree(),
                pack_identity_equal: left.pack() == right.pack(),
                installed_outcomes: outcomes
                    .iter()
                    .filter(|outcome| **outcome == PackAdmissionObservationV1::Installed)
                    .count(),
                existing_outcomes: outcomes
                    .iter()
                    .filter(|outcome| **outcome == PackAdmissionObservationV1::ExistingComplete)
                    .count(),
                carriers_installed: left.carriers_installed() + right.carriers_installed(),
                carriers_reused: left.carriers_reused() + right.carriers_reused(),
                carrier_entries: fs::read_dir(root.join("carriers"))
                    .expect("reopened-writer carriers")
                    .count(),
                preparation_usage,
                committed_usage,
                immutable_usage,
                installer_pack_bytes_owned: installer
                    .storage_bytes_committed
                    .checked_sub(pack_usage.0)
                    == closure_usage.0.checked_sub(adopter.storage_bytes_committed),
                installer_pack_inodes_owned: installer
                    .storage_inodes_committed
                    .checked_sub(pack_usage.1)
                    == closure_usage
                        .1
                        .checked_sub(adopter.storage_inodes_committed),
                adopter_committed_bytes: adopter.storage_bytes_committed,
                adopter_committed_inodes: adopter.storage_inodes_committed,
                closure_usage,
                adopter_byte_equation_holds: adopter.storage_bytes_reserved
                    == adopter.storage_bytes_released + adopter.storage_bytes_committed,
                adopter_inode_equation_holds: adopter.storage_inodes_reserved
                    == adopter.storage_inodes_released + adopter.storage_inodes_committed,
                catalog_entries: fs::read_dir(root.join("catalog"))
                    .expect("reopened-writer catalog")
                    .count(),
                closure_entries: fs::read_dir(root.join("closures"))
                    .expect("reopened-writer closures")
                    .count(),
                left_outcome,
                right_outcome,
                left_carriers_installed: left.carriers_installed(),
                right_carriers_installed: right.carriers_installed(),
                left_carriers_reused: left.carriers_reused(),
                right_carriers_reused: right.carriers_reused(),
                root_clean,
                queue_after: seed.operation_admission_queue_for_test_v1(),
            }
        })
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum ConcurrentFailureObservationV1 {
        CountCap,
        Cancelled,
        Deadline,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct SuccessAcrossFailureObservationV1 {
        pub failure: ConcurrentFailureObservationV1,
        pub failure_terminal: ConcurrentFailureObservationV1,
        pub counters: [ConcurrentOperationCountersObservationV1; 2],
        pub success_outcome: PackAdmissionObservationV1,
        pub preparation_usage: (u64, u64),
        pub immutable_usage: (u64, u64),
        pub root_clean: bool,
        pub root_usable: bool,
        pub reopened_usable: bool,
    }

    pub fn simultaneous_success_across_failure_v1(
        roots: [&Path; 3],
    ) -> [SuccessAcrossFailureObservationV1; 3] {
        let failures = [
            ConcurrentFailureObservationV1::CountCap,
            ConcurrentFailureObservationV1::Cancelled,
            ConcurrentFailureObservationV1::Deadline,
        ];
        std::array::from_fn(|index| {
            let root = roots[index];
            let failure = failures[index];
            let seed = FsCasV1::create_new(root).expect("success/failure semantic root");
            let success_cas = FsCasV1::open_existing(root).expect("successful reopened writer");
            let failure_cas = FsCasV1::open_existing(root).expect("failing reopened writer");
            let start = Arc::new(WatchdogGateV1::new());
            let (ready_tx, ready_rx) = mpsc::sync_channel(2);
            let ((success_terminal, success_counters), (failure_terminal, failure_counters)) =
                std::thread::scope(|scope| {
                    let mut release = WatchdogGateReleaseV1::new(Arc::clone(&start));
                    let success_start = Arc::clone(&start);
                    let success_ready = ready_tx.clone();
                    let success = scope.spawn(move || {
                        let input = [0x71_u8];
                        let mut control = ContinueControl;
                        let mut counters = OperationCountersV1::default();
                        let terminal = run_semantic_create(
                            &success_cas,
                            0x0011_55a0,
                            1,
                            BarrierSliceSupplier {
                                bytes: &input,
                                ready: success_ready,
                                start: success_start,
                            },
                            &mut control,
                            &mut counters,
                        );
                        (terminal, counters)
                    });
                    let failure_start = Arc::clone(&start);
                    let failed = scope.spawn(move || {
                        let mut counters = OperationCountersV1::default();
                        let terminal = match failure {
                            ConcurrentFailureObservationV1::CountCap => {
                                let mut control = ContinueControl;
                                run_semantic_create(
                                    &failure_cas,
                                    0x0011_55a1,
                                    1,
                                    BarrierFailingSupplier {
                                        ready: ready_tx,
                                        start: failure_start,
                                    },
                                    &mut control,
                                    &mut counters,
                                )
                            }
                            ConcurrentFailureObservationV1::Cancelled
                            | ConcurrentFailureObservationV1::Deadline => {
                                let input = [0x72_u8];
                                let stop = if failure == ConcurrentFailureObservationV1::Cancelled {
                                    CandidateValidationStopV1::Cancelled
                                } else {
                                    CandidateValidationStopV1::Deadline
                                };
                                let mut control = StopBeforeCandidateValidationV1::new(stop);
                                run_semantic_create(
                                    &failure_cas,
                                    0x0011_55a2,
                                    1,
                                    BarrierSliceSupplier {
                                        bytes: &input,
                                        ready: ready_tx,
                                        start: failure_start,
                                    },
                                    &mut control,
                                    &mut counters,
                                )
                            }
                        };
                        (terminal, counters)
                    });
                    ready_rx
                        .recv_timeout(Duration::from_secs(5))
                        .expect("success/failure first rendezvous");
                    ready_rx
                        .recv_timeout(Duration::from_secs(5))
                        .expect("success/failure second rendezvous");
                    release.release();
                    (
                        success.join().expect("successful writer thread"),
                        failed.join().expect("failing writer thread"),
                    )
                });
            let success = success_terminal.expect("successful concurrent writer");
            let failure_terminal = match failure_terminal.expect_err("concurrent failure") {
                OperationErrorV1::Core(CoreError::CountCap) => {
                    ConcurrentFailureObservationV1::CountCap
                }
                OperationErrorV1::FsCas(FsCasErrorV1::Core(CoreError::Cancelled)) => {
                    ConcurrentFailureObservationV1::Cancelled
                }
                OperationErrorV1::FsCas(FsCasErrorV1::Core(CoreError::Deadline)) => {
                    ConcurrentFailureObservationV1::Deadline
                }
                other => panic!("unexpected concurrent failure terminal: {other:?}"),
            };
            let immutable_usage = immutable_usage(root);
            let immutable_bytes = immutable_usage.0;
            let immutable_inodes = immutable_usage.1;
            assert_eq!(success_counters.storage_bytes_committed, immutable_bytes);
            assert_eq!(success_counters.storage_inodes_committed, immutable_inodes);
            SuccessAcrossFailureObservationV1 {
                failure,
                failure_terminal,
                counters: [
                    concurrent_counters(&success_counters),
                    concurrent_counters(&failure_counters),
                ],
                success_outcome: pack_admission_observation(success.pack_outcome()),
                preparation_usage: clean_preparation_usage(root),
                immutable_usage,
                root_clean: operation_authority_is_clean(&seed, root),
                root_usable: seed.occupied().is_ok(),
                reopened_usable: FsCasV1::open_existing(root)
                    .is_ok_and(|cas| cas.occupied().is_ok()),
            }
        })
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct WriterAdmissionOperationObservationV1 {
        pub outcome: PackAdmissionObservationV1,
        pub carriers_installed: u32,
        pub carriers_reused: u32,
        pub counters: ConcurrentOperationCountersObservationV1,
    }

    #[derive(Debug, Eq, PartialEq)]
    pub struct WriterAdmissionLevelObservationV1 {
        pub level: usize,
        pub operations: Vec<WriterAdmissionOperationObservationV1>,
        pub observed_admission_high_water: u64,
        pub observed_root_bytes_high_water: u64,
        pub observed_root_inodes_high_water: u64,
        pub total_reserved_bytes: u64,
        pub total_reserved_inodes: u64,
        pub preparation_usage: (u64, u64),
        pub committed_usage: (u64, u64),
        pub immutable_usage: (u64, u64),
        pub carrier_entries: usize,
        pub catalog_entries: usize,
        pub closure_entries: usize,
        pub root_clean: bool,
        pub root_usable: bool,
    }

    pub fn reopened_writer_admission_levels_v1(
        roots: [&Path; 5],
    ) -> [WriterAdmissionLevelObservationV1; 5] {
        let levels = [1_usize, 2, 4, 8, 16];
        std::array::from_fn(|row| {
            let root = roots[row];
            let level = levels[row];
            let seed = FsCasV1::create_new(root).expect("writer-level semantic root");
            let callers = (0..level)
                .map(|_| FsCasV1::open_existing(root).expect("reopened level writer"))
                .collect::<Vec<_>>();
            let start = Arc::new(WatchdogGateV1::new());
            let (ready_tx, ready_rx) = mpsc::sync_channel(level);
            let results = std::thread::scope(|scope| {
                let mut release = WatchdogGateReleaseV1::new(Arc::clone(&start));
                let joins = callers
                    .into_iter()
                    .enumerate()
                    .map(|(index, cas)| {
                        let start = Arc::clone(&start);
                        let ready = ready_tx.clone();
                        scope.spawn(move || {
                            let input = [0x90_u8 + index as u8];
                            let mut control = ContinueControl;
                            let mut counters = OperationCountersV1::default();
                            let terminal = run_semantic_create(
                                &cas,
                                0x0011_5600 + index as u64,
                                1,
                                BarrierSliceSupplier {
                                    bytes: &input,
                                    ready,
                                    start,
                                },
                                &mut control,
                                &mut counters,
                            );
                            (terminal, counters)
                        })
                    })
                    .collect::<Vec<_>>();
                for _ in 0..level {
                    ready_rx
                        .recv_timeout(Duration::from_secs(5))
                        .expect("writer-level rendezvous");
                }
                release.release();
                joins
                    .into_iter()
                    .map(|join| join.join().expect("writer-level thread"))
                    .collect::<Vec<_>>()
            });
            let mut operations = Vec::with_capacity(level);
            let mut total_reserved_bytes = 0;
            let mut total_reserved_inodes = 0;
            let mut committed_usage = (0, 0);
            let mut observed_admission_high_water = 0;
            let mut observed_root_bytes_high_water = 0;
            let mut observed_root_inodes_high_water = 0;
            for (terminal, counters) in results {
                let handoff = terminal.expect("writer-level terminal");
                total_reserved_bytes += counters.storage_bytes_reserved;
                total_reserved_inodes += counters.storage_inodes_reserved;
                committed_usage.0 += counters.storage_bytes_committed;
                committed_usage.1 += counters.storage_inodes_committed;
                observed_admission_high_water = observed_admission_high_water
                    .max(counters.root_admission_active_slots_high_water);
                observed_root_bytes_high_water = observed_root_bytes_high_water
                    .max(counters.root_storage_active_reserved_bytes_lifetime_high_water);
                observed_root_inodes_high_water = observed_root_inodes_high_water
                    .max(counters.root_storage_active_reserved_inodes_lifetime_high_water);
                operations.push(WriterAdmissionOperationObservationV1 {
                    outcome: pack_admission_observation(handoff.pack_outcome()),
                    carriers_installed: handoff.carriers_installed(),
                    carriers_reused: handoff.carriers_reused(),
                    counters: concurrent_counters(&counters),
                });
            }
            let immutable_usage = immutable_usage(root);
            let immutable_bytes = immutable_usage.0;
            let immutable_inodes = immutable_usage.1;
            let total_committed_bytes = committed_usage.0;
            let total_committed_inodes = committed_usage.1;
            assert_eq!(total_committed_bytes, immutable_bytes);
            assert_eq!(total_committed_inodes, immutable_inodes);
            assert_operation_authority_baseline(&seed, root);
            WriterAdmissionLevelObservationV1 {
                level,
                operations,
                observed_admission_high_water,
                observed_root_bytes_high_water,
                observed_root_inodes_high_water,
                total_reserved_bytes,
                total_reserved_inodes,
                preparation_usage: clean_preparation_usage(root),
                committed_usage,
                immutable_usage,
                carrier_entries: fs::read_dir(root.join("carriers"))
                    .expect("writer-level carriers")
                    .count(),
                catalog_entries: fs::read_dir(root.join("catalog"))
                    .expect("writer-level catalog")
                    .count(),
                closure_entries: fs::read_dir(root.join("closures"))
                    .expect("writer-level closures")
                    .count(),
                root_clean: operation_authority_is_clean(&seed, root),
                root_usable: seed.occupied().is_ok(),
            }
        })
    }

    struct BarrierCounterSupplier {
        len: u64,
        ready: mpsc::SyncSender<()>,
        start: Arc<WatchdogGateV1>,
    }

    impl SourceSupplierV1 for BarrierCounterSupplier {
        type Source = CounterSource;

        fn resident_memory_bound_bytes(&self) -> crate::CoreResult<u64> {
            Ok(core::mem::size_of::<CounterSource>() as u64)
        }

        fn supply(self) -> crate::CoreResult<Self::Source> {
            self.ready.send(()).expect("counter supplier receiver");
            self.start.wait();
            Ok(CounterSource {
                len: self.len,
                offset: 0,
            })
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct MultiPackWriterObservationV1 {
        pub multi_carrier_count: u32,
        pub multi_carrier_rollovers: u32,
        pub multi_carriers_installed: u32,
        pub multi_carriers_reused: u32,
        pub disjoint_carrier_count: u32,
        pub disjoint_carriers_installed: u32,
        pub disjoint_carriers_reused: u32,
        pub counters: [ConcurrentOperationCountersObservationV1; 2],
        pub total_reserved_bytes: u64,
        pub total_reserved_inodes: u64,
        pub preparation_usage: (u64, u64),
        pub committed_usage: (u64, u64),
        pub immutable_usage: (u64, u64),
        pub carrier_entries: usize,
        pub catalog_entries: usize,
        pub closure_entries: usize,
        pub root_clean: bool,
    }

    pub fn reopened_multi_pack_writer_v1(root: &Path) -> MultiPackWriterObservationV1 {
        const MULTI_PACK_BYTES: u64 = 65 * 1_024 * 1_024;
        let seed = FsCasV1::create_new(root).expect("multi-pack semantic root");
        let multi_cas = FsCasV1::open_existing(root).expect("multi-pack reopened writer");
        let disjoint_cas = FsCasV1::open_existing(root).expect("disjoint reopened writer");
        let start = Arc::new(WatchdogGateV1::new());
        let (ready_tx, ready_rx) = mpsc::sync_channel(2);
        let ((multi_terminal, multi_counters), (disjoint_terminal, disjoint_counters)) =
            std::thread::scope(|scope| {
                let mut release = WatchdogGateReleaseV1::new(Arc::clone(&start));
                let multi_start = Arc::clone(&start);
                let multi_ready = ready_tx.clone();
                let multi = scope.spawn(move || {
                    let mut control = ContinueControl;
                    let mut counters = OperationCountersV1::default();
                    let terminal = run_semantic_create(
                        &multi_cas,
                        0x0011_5700,
                        MULTI_PACK_BYTES,
                        BarrierCounterSupplier {
                            len: MULTI_PACK_BYTES,
                            ready: multi_ready,
                            start: multi_start,
                        },
                        &mut control,
                        &mut counters,
                    );
                    (terminal, counters)
                });
                let disjoint_start = Arc::clone(&start);
                let disjoint = scope.spawn(move || {
                    let input = [0xd1_u8];
                    let mut control = ContinueControl;
                    let mut counters = OperationCountersV1::default();
                    let terminal = run_semantic_create(
                        &disjoint_cas,
                        0x0011_5701,
                        1,
                        BarrierSliceSupplier {
                            bytes: &input,
                            ready: ready_tx,
                            start: disjoint_start,
                        },
                        &mut control,
                        &mut counters,
                    );
                    (terminal, counters)
                });
                ready_rx
                    .recv_timeout(Duration::from_secs(5))
                    .expect("multi-pack writer rendezvous");
                ready_rx
                    .recv_timeout(Duration::from_secs(5))
                    .expect("disjoint writer rendezvous");
                release.release();
                (
                    multi.join().expect("multi-pack writer thread"),
                    disjoint.join().expect("disjoint writer thread"),
                )
            });
        let multi = multi_terminal.expect("multi-pack writer terminal");
        let disjoint = disjoint_terminal.expect("disjoint writer terminal");
        let immutable_usage = immutable_usage(root);
        let immutable_bytes = immutable_usage.0;
        let immutable_inodes = immutable_usage.1;
        assert_eq!(
            multi_counters.storage_bytes_committed + disjoint_counters.storage_bytes_committed,
            immutable_bytes
        );
        assert_eq!(
            multi_counters.storage_inodes_committed + disjoint_counters.storage_inodes_committed,
            immutable_inodes
        );
        assert_operation_authority_baseline(&seed, root);
        MultiPackWriterObservationV1 {
            multi_carrier_count: multi.carrier_count(),
            multi_carrier_rollovers: multi.carrier_rollovers(),
            multi_carriers_installed: multi.carriers_installed(),
            multi_carriers_reused: multi.carriers_reused(),
            disjoint_carrier_count: disjoint.carrier_count(),
            disjoint_carriers_installed: disjoint.carriers_installed(),
            disjoint_carriers_reused: disjoint.carriers_reused(),
            counters: [
                concurrent_counters(&multi_counters),
                concurrent_counters(&disjoint_counters),
            ],
            total_reserved_bytes: multi_counters.storage_bytes_reserved
                + disjoint_counters.storage_bytes_reserved,
            total_reserved_inodes: multi_counters.storage_inodes_reserved
                + disjoint_counters.storage_inodes_reserved,
            preparation_usage: clean_preparation_usage(root),
            committed_usage: (
                multi_counters.storage_bytes_committed + disjoint_counters.storage_bytes_committed,
                multi_counters.storage_inodes_committed
                    + disjoint_counters.storage_inodes_committed,
            ),
            immutable_usage,
            carrier_entries: fs::read_dir(root.join("carriers"))
                .expect("multi-pack carriers")
                .count(),
            catalog_entries: fs::read_dir(root.join("catalog"))
                .expect("multi-pack catalog")
                .count(),
            closure_entries: fs::read_dir(root.join("closures"))
                .expect("multi-pack closures")
                .count(),
            root_clean: operation_authority_is_clean(&seed, root),
        }
    }

    struct LoadAbortOnDropV1 {
        abort: Arc<AtomicBool>,
        armed: bool,
    }

    impl LoadAbortOnDropV1 {
        fn new(abort: Arc<AtomicBool>) -> Self {
            Self { abort, armed: true }
        }

        fn disarm(&mut self) {
            self.armed = false;
        }
    }

    impl Drop for LoadAbortOnDropV1 {
        fn drop(&mut self) {
            if self.armed {
                self.abort.store(true, Ordering::Release);
            }
        }
    }

    struct LoadReadSinkV1 {
        entered: Option<mpsc::SyncSender<()>>,
        gate: Arc<WatchdogGateV1>,
        bytes: Vec<u8>,
        selected_offset: u64,
        selected_len: u64,
        finished: bool,
        aborted: bool,
    }

    impl LoadReadSinkV1 {
        fn new(capacity: usize, entered: mpsc::SyncSender<()>, gate: Arc<WatchdogGateV1>) -> Self {
            Self {
                entered: Some(entered),
                gate,
                bytes: Vec::with_capacity(capacity),
                selected_offset: 0,
                selected_len: 0,
                finished: false,
                aborted: false,
            }
        }
    }

    impl ReadSinkV1 for LoadReadSinkV1 {
        fn resident_memory_bound_bytes(&self) -> crate::CoreResult<u64> {
            u64::try_from(core::mem::size_of::<Self>() + self.bytes.capacity())
                .map_err(|_| CoreError::IntegerOverflow)
        }

        fn begin_read(&mut self, _kind: ReadKindV1) -> Result<(), ReadSinkErrorV1> {
            Ok(())
        }

        fn begin_file(
            &mut self,
            _path: &[u8],
            _mode: u16,
            _logical_len: u64,
            selected_offset: u64,
            selected_len: u64,
        ) -> Result<(), ReadSinkErrorV1> {
            self.selected_offset = selected_offset;
            self.selected_len = selected_len;
            Ok(())
        }

        fn write_file_bytes(&mut self, bytes: &[u8]) -> Result<(), ReadSinkErrorV1> {
            if let Some(entered) = self.entered.take() {
                entered.send(()).map_err(|_| ReadSinkErrorV1::Refused)?;
                self.gate.wait();
            }
            self.bytes.extend_from_slice(bytes);
            Ok(())
        }

        fn finish_file(&mut self) -> Result<(), ReadSinkErrorV1> {
            Ok(())
        }

        fn finish_read(&mut self, _verification_digest: [u8; 32]) -> Result<(), ReadSinkErrorV1> {
            self.finished = true;
            Ok(())
        }

        fn abort_read(&mut self) {
            self.aborted = true;
            self.finished = false;
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum MutationReadKindObservationV1 {
        FullExtraction,
        ExactRange,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct MutationReadCrossingObservationV1 {
        pub kind: MutationReadKindObservationV1,
        pub initial_root_matches: bool,
        pub payload_bytes: u64,
        pub payload_matches: bool,
        pub selected_offset: u64,
        pub selected_len: u64,
        pub sink_finished: bool,
        pub sink_aborted: bool,
        pub mutation_completed_while_read_blocked: bool,
        pub mutation_root_changed: bool,
        pub read_counters: ConcurrentOperationCountersObservationV1,
        pub mutation_counters: ConcurrentOperationCountersObservationV1,
        pub read_storage_terminal: bool,
        pub mutation_storage_terminal: bool,
        pub overlap_high_water: u64,
        pub operation_admitted_slots: u64,
        pub operation_active: u64,
        pub storage_active: (u64, u64, u64),
        pub preparation_entries: usize,
        pub authority_clean: bool,
        pub root_usable: bool,
        pub reopened_usable: bool,
        pub namespace_entries_are_regular: bool,
    }

    fn read_storage_terminal(counters: &OperationCountersV1) -> bool {
        counters.storage_bytes_requested == 0
            && counters.storage_bytes_reserved == 0
            && counters.storage_bytes_released == 0
            && counters.storage_bytes_committed == 0
            && counters.storage_bytes_retained == 0
            && counters.storage_inodes_requested == 0
            && counters.storage_inodes_reserved == 0
            && counters.storage_inodes_released == 0
            && counters.storage_inodes_committed == 0
            && counters.storage_inodes_retained == 0
            && counters.storage_preparation_bytes_current_after_cleanup == 0
            && counters.storage_preparation_inodes_current_after_cleanup == 0
            && counters.mutable_preparation_residue_bytes == 0
            && counters.mutable_preparation_residue_inodes == 0
            && counters.visibility_lock_acquisitions > 0
            && counters.publication_lock_acquisitions == 0
            && counters.has_zero_forbidden_work()
    }

    fn mutation_storage_terminal(counters: &OperationCountersV1) -> bool {
        storage_equations_hold(counters)
            && counters.storage_bytes_requested > 0
            && counters.root_storage_active_reserved_bytes_lifetime_high_water
                >= counters.storage_bytes_reserved
            && counters.root_storage_active_reserved_inodes_lifetime_high_water
                >= counters.storage_inodes_reserved
            && counters.storage_bytes_committed > 0
            && counters.storage_inodes_committed > 0
            && counters.storage_bytes_retained == 0
            && counters.storage_inodes_retained == 0
            && counters.mutable_preparation_residue_bytes == 0
            && counters.mutable_preparation_residue_inodes == 0
            && counters.unreachable_installed_residue_bytes == 0
            && counters.has_zero_forbidden_work()
    }

    fn namespace_entries_are_regular(root: &Path) -> bool {
        ["preparation", "carriers", "objects", "catalog", "closures"]
            .into_iter()
            .flat_map(|name| fs::read_dir(root.join(name)).expect("semantic namespace"))
            .all(|entry| {
                entry
                    .ok()
                    .and_then(|entry| fs::symlink_metadata(entry.path()).ok())
                    .is_some_and(|metadata| metadata.file_type().is_file())
            })
    }

    /// Cross a real complete replacement with both real reopened read paths
    /// while payload delivery is blocked outside the storage engine. Only
    /// immutable facts about the two executions cross this semantic seam.
    pub fn reopened_mutation_read_crossings_v1(
        root: &Path,
    ) -> [MutationReadCrossingObservationV1; 2] {
        std::array::from_fn(|index| {
            let range = index == 1;
            let scenario_root = root.join(if range {
                "mutation-range-read"
            } else {
                "mutation-full-read"
            });
            let seed = FsCasV1::create_new(&scenario_root).expect("create read crossing FsCas");
            let reader_cas =
                FsCasV1::open_existing(&scenario_root).expect("reopen crossing reader");
            let mutation_cas =
                FsCasV1::open_existing(&scenario_root).expect("reopen crossing mutator");
            let base_data = (0..48_123)
                .map(|index| (index * 37) as u8)
                .collect::<Vec<_>>();
            let replacement_data = (0..57_321)
                .map(|index| (index * 19 + 7) as u8)
                .collect::<Vec<_>>();
            let name = b"b.bin";
            let (base_logical, base_physical) =
                expected_file(&base_data, 0o644).expect("crossing base identity");
            let base_entry = CanonicalTreeEntryV1::new(
                ValidatedComponent::new(name).expect("crossing component"),
                CanonicalTreeChildV1::File {
                    logical: derive_file_node_v1(0o644, base_logical).expect("crossing file node"),
                    physical: base_physical,
                },
            );
            with_replacement_evidence_v1(
                DirectoryBuildModeV1::ImplicitRoot,
                std::slice::from_ref(&base_entry),
                0,
                |base_tree, replacement_evidence| {
                    let mut manifest = [TreeFileV1::new(
                        name,
                        0o644,
                        base_data.len() as u64,
                        SliceSupplier { bytes: &base_data },
                    )];
                    let mut scratch = OperationScratch::new();
                    let mut control = ContinueControl;
                    let mut counters = OperationCountersV1::default();
                    let operation =
                        request_tree_operation_v1(&seed, 0x520, &mut counters, &mut control)
                            .expect("crossing base grant");
                    let base_handoff = run_create_tree_v1(
                        operation,
                        CdcAlgorithmV1::FastCdc,
                        &mut manifest,
                        scratch.borrow(),
                        &mut control,
                        &mut counters,
                    )
                    .expect("crossing base handoff");
                    let base_version = base_handoff.version_record();
                    let accepted_root = base_handoff.root_tree();
                    let selected_offset = if range { 817 } else { 0 };
                    let selected_len = if range {
                        17_777
                    } else {
                        base_data.len() as u64
                    };
                    let expected = &base_data
                        [selected_offset as usize..(selected_offset + selected_len) as usize];
                    let release = Arc::new(WatchdogGateV1::new());
                    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
                    let (mutation_done_tx, mutation_done_rx) = mpsc::sync_channel(1);

                    let (read_terminal, mutation_terminal, completed_while_blocked) =
                        std::thread::scope(|scope| {
                            let mut release_guard =
                                WatchdogGateReleaseV1::new(Arc::clone(&release));
                            let read_release = Arc::clone(&release);
                            let reader = scope.spawn(move || {
                                let mut comparison = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
                                let mut path = boxed_zeroes::<MAX_PATH_BYTES>();
                                let mut counters = OperationCountersV1::default();
                                let mut control = ContinueControl;
                                let mut sink = LoadReadSinkV1::new(
                                    selected_len as usize,
                                    ready_tx,
                                    read_release,
                                );
                                let result = if range {
                                    read_file_range_impl_v1(
                                        &reader_cas,
                                        0x521,
                                        base_version,
                                        base_tree.physical(),
                                        name,
                                        selected_offset,
                                        selected_len,
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
                                        &reader_cas,
                                        0x521,
                                        base_version,
                                        base_tree.physical(),
                                        &mut sink,
                                        &mut counters,
                                        ReadBuffersV1 {
                                            comparison: &mut comparison,
                                            path: &mut path,
                                        },
                                        &mut control,
                                    )
                                };
                                (result, sink, counters)
                            });

                            ready_rx
                                .recv_timeout(Duration::from_secs(5))
                                .expect("read crossing reached payload sink");
                            let mutator = scope.spawn(move || {
                                let mut source = SliceSource::new(&replacement_data);
                                let mut scratch = OperationScratch::new();
                                let mut cow_logical = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
                                let mut control = ContinueControl;
                                let mut counters = OperationCountersV1::default();
                                let result = run_complete_replace_v1(
                                    &mutation_cas,
                                    0x522,
                                    CdcAlgorithmV1::FastCdc,
                                    base_version,
                                    base_tree,
                                    replacement_evidence,
                                    0,
                                    name,
                                    0o600,
                                    replacement_data.len() as u64,
                                    &mut source,
                                    scratch.borrow(),
                                    &mut cow_logical,
                                    &mut control,
                                    &mut counters,
                                );
                                mutation_done_tx
                                    .send(())
                                    .expect("mutation crossing completion receiver");
                                (result, counters)
                            });
                            let completed_while_blocked = mutation_done_rx
                                .recv_timeout(Duration::from_secs(5))
                                .is_ok();
                            release_guard.release();
                            (
                                reader.join().expect("crossing reader thread"),
                                mutator.join().expect("crossing mutator thread"),
                                completed_while_blocked,
                            )
                        });

                    let (read_result, sink, read_counters) = read_terminal;
                    let read_result = read_result.expect("real reopened read succeeds");
                    let (mutation_result, mutation_counters) = mutation_terminal;
                    let mutation_result =
                        mutation_result.expect("real complete replacement succeeds");
                    MutationReadCrossingObservationV1 {
                        kind: match read_result.kind() {
                            ReadKindV1::FullExtraction => {
                                MutationReadKindObservationV1::FullExtraction
                            }
                            ReadKindV1::ExactRange => MutationReadKindObservationV1::ExactRange,
                        },
                        initial_root_matches: accepted_root == base_tree.physical(),
                        payload_bytes: read_result.payload_bytes(),
                        payload_matches: sink.bytes == expected,
                        selected_offset: sink.selected_offset,
                        selected_len: sink.selected_len,
                        sink_finished: sink.finished,
                        sink_aborted: sink.aborted,
                        mutation_completed_while_read_blocked: completed_while_blocked,
                        mutation_root_changed: mutation_result.root_tree() != base_tree.physical(),
                        read_counters: concurrent_counters(&read_counters),
                        mutation_counters: concurrent_counters(&mutation_counters),
                        read_storage_terminal: read_storage_terminal(&read_counters),
                        mutation_storage_terminal: mutation_storage_terminal(&mutation_counters),
                        overlap_high_water: read_counters
                            .root_admission_active_slots_high_water
                            .max(mutation_counters.root_admission_active_slots_high_water),
                        operation_admitted_slots: seed.operation_admitted_slots_v1(),
                        operation_active: seed.operation_admission_active_for_test_v1(),
                        storage_active: seed.storage_admission_active_for_test_v1(),
                        preparation_entries: fs::read_dir(scenario_root.join("preparation"))
                            .expect("crossing preparation namespace")
                            .count(),
                        authority_clean: operation_authority_is_clean(&seed, &scenario_root),
                        root_usable: seed.occupied().is_ok(),
                        reopened_usable: FsCasV1::open_existing(&scenario_root)
                            .is_ok_and(|cas| cas.occupied().is_ok()),
                        namespace_entries_are_regular: namespace_entries_are_regular(
                            &scenario_root,
                        ),
                    }
                },
            )
            .expect("crossing replacement evidence")
        })
    }

    #[derive(Default)]
    struct MutationTraceControlV1 {
        boundaries: Vec<FsCasBoundaryV1>,
    }

    impl CdcControlV1 for MutationTraceControlV1 {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for MutationTraceControlV1 {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            self.boundaries.push(boundary);
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    fn mutation_entry<'a>(
        name: &'a [u8],
        file: &ExpectedFileV1,
        mode: u16,
    ) -> CanonicalTreeEntryV1<'a> {
        CanonicalTreeEntryV1::new(
            ValidatedComponent::new(name).expect("mutation semantic component"),
            file.child(mode).expect("mutation semantic file child"),
        )
    }

    fn mutation_directory_entry(
        name: &[u8],
        directory: CanonicalDirectoryTreeV1,
    ) -> CanonicalTreeEntryV1<'_> {
        let DirectoryLogicalIdentityV1::Explicit(logical) = directory.logical() else {
            panic!("mutation semantic nested directory must be explicit");
        };
        CanonicalTreeEntryV1::new(
            ValidatedComponent::new(name).expect("mutation semantic directory component"),
            CanonicalTreeChildV1::Directory {
                logical,
                physical: directory.physical(),
            },
        )
    }

    fn accept_mutation_files_v1(
        cas: &FsCasV1,
        key: u64,
        files: &[(&[u8], u16, &[u8])],
    ) -> (PhysicalVersionRecordIdV1, PhysicalTreeIdV1) {
        let mut manifest = files
            .iter()
            .map(|(path, mode, bytes)| {
                TreeFileV1::new(path, *mode, bytes.len() as u64, SliceSupplier { bytes })
            })
            .collect::<Vec<_>>();
        let mut scratch = OperationScratch::new();
        let mut control = MutationTraceControlV1::default();
        let mut counters = OperationCountersV1::default();
        let operation = request_tree_operation_v1(cas, key, &mut counters, &mut control)
            .expect("mutation semantic root grant");
        let handoff = run_create_tree_v1(
            operation,
            CdcAlgorithmV1::FastCdc,
            &mut manifest,
            scratch.borrow(),
            &mut control,
            &mut counters,
        )
        .expect("mutation semantic accepted base root");
        (handoff.version_record(), handoff.root_tree())
    }

    fn complete_mutation_counters(counters: &OperationCountersV1) -> CompleteMutationCountersV1 {
        CompleteMutationCountersV1 {
            update_reference_metadata_records: counters.update_reference_metadata_records,
            update_reference_metadata_bytes: counters.update_reference_metadata_bytes,
            update_base_payload_bytes: counters.update_base_payload_bytes,
            update_inserted_bytes: counters.update_inserted_bytes,
            update_resynchronization_bytes: counters.update_resynchronization_bytes,
            exact_rejoin_bytes: counters.exact_rejoin_bytes,
            rejoin_successes: counters.rejoin_successes,
            rejoin_failures: counters.rejoin_failures,
            anchor_attempts: counters.anchor_attempts,
            bytes_read: counters.bytes_read,
            source_read_calls: counters.source_read_calls,
            source_bytes_read: counters.source_bytes_read,
            fscas_read_calls: counters.fscas_read_calls,
            fscas_bytes_read: counters.fscas_bytes_read,
            update_failures: counters.update_failures,
            storage_bytes_requested: counters.storage_bytes_requested,
            storage_bytes_reserved: counters.storage_bytes_reserved,
            storage_bytes_released: counters.storage_bytes_released,
            storage_bytes_committed: counters.storage_bytes_committed,
            storage_bytes_retained: counters.storage_bytes_retained,
            storage_inodes_requested: counters.storage_inodes_requested,
            storage_inodes_reserved: counters.storage_inodes_reserved,
            storage_inodes_released: counters.storage_inodes_released,
            storage_inodes_committed: counters.storage_inodes_committed,
            storage_inodes_retained: counters.storage_inodes_retained,
            root_storage_active_reserved_bytes_lifetime_high_water: counters
                .root_storage_active_reserved_bytes_lifetime_high_water,
            root_storage_active_reserved_inodes_lifetime_high_water: counters
                .root_storage_active_reserved_inodes_lifetime_high_water,
            mutable_preparation_residue_bytes: counters.mutable_preparation_residue_bytes,
            mutable_preparation_residue_inodes: counters.mutable_preparation_residue_inodes,
            unreachable_installed_residue_bytes: counters.unreachable_installed_residue_bytes,
            storage_preparation_bytes_current_after_cleanup: counters
                .storage_preparation_bytes_current_after_cleanup,
            storage_preparation_inodes_current_after_cleanup: counters
                .storage_preparation_inodes_current_after_cleanup,
            zero_forbidden_work: counters.has_zero_forbidden_work(),
            storage_equations_hold: storage_equations_hold(counters),
        }
    }

    fn mutation_observation(
        terminal: CompleteMutationTerminalV1,
        cas: &FsCasV1,
        stale: Option<&FsCasV1>,
        root: &Path,
        counters: &OperationCountersV1,
    ) -> CompleteMutationObservationV1 {
        let preparation_entries = fs::read_dir(root.join("preparation"))
            .expect("mutation semantic preparation namespace")
            .count() as u64;
        let root_usable = cas.occupied().is_ok();
        let stale_usable = stale.is_none_or(|stale| stale.occupied().is_ok());
        CompleteMutationObservationV1 {
            terminal,
            accepted_root: None,
            base_tree: None,
            replaced_root: None,
            replacement_tree: None,
            metadata_file: None,
            replacement_file: None,
            metadata_root: None,
            metadata_tree: None,
            added_root: None,
            added_tree: None,
            moved_root: None,
            moved_tree: None,
            removed_root: None,
            removed_tree: None,
            updated_root: None,
            update_tree: None,
            completed_operations: 0,
            final_root_returns_to_base: false,
            algorithm_is_fastcdc: false,
            update_algorithm: None,
            validated_handoffs: 0,
            storage_terminals: 0,
            source_offset: 0,
            namespace_before: None,
            exact_operation_namespace_usage: None,
            authority_clean: operation_authority_is_clean(cas, root),
            namespace_entries_are_regular: namespace_entries_are_regular(root),
            root_usable,
            stale_usable,
            accepted_version: None,
            wrong_version: None,
            counters: complete_mutation_counters(counters),
            operation_counters: [CompleteMutationCountersV1::default(); 3],
            operation_counter_count: 0,
            operation_admitted_slots: cas.operation_admitted_slots_v1(),
            operation_admission_active: cas.operation_admission_active_for_test_v1(),
            storage_admission_active: cas.storage_admission_active_for_test_v1(),
            preparation_entries,
        }
    }

    /// Execute one historically named complete mutation through the real
    /// root-owned lifecycle. The facade exposes only immutable custody facts.
    pub fn complete_mutation_case_v1(
        root: &Path,
        case: CompleteMutationCaseV1,
        update_base: &[u8],
    ) -> CompleteMutationObservationV1 {
        let case_root = root.join(match case {
            CompleteMutationCaseV1::ReplaceAndMetadata => "complete-replace-metadata",
            CompleteMutationCaseV1::AddMoveRemove => "complete-add-move-remove",
            CompleteMutationCaseV1::CrossDirectoryMove => "complete-cross-directory-move",
            CompleteMutationCaseV1::Update => "complete-update",
            CompleteMutationCaseV1::UpdateReferenceMetadataOverflow => {
                "complete-update-reference-overflow"
            }
            CompleteMutationCaseV1::UpdateExactRejoinOverflow => "complete-update-rejoin-overflow",
            CompleteMutationCaseV1::UnauthenticatedBase => "complete-unauthenticated-base",
        });
        match case {
            CompleteMutationCaseV1::ReplaceAndMetadata => complete_replace_metadata_v1(&case_root),
            CompleteMutationCaseV1::AddMoveRemove => complete_add_move_remove_v1(&case_root),
            CompleteMutationCaseV1::CrossDirectoryMove => {
                complete_cross_directory_move_v1(&case_root)
            }
            CompleteMutationCaseV1::Update => {
                complete_update_semantic_v1(&case_root, update_base, false, false)
            }
            CompleteMutationCaseV1::UpdateReferenceMetadataOverflow => {
                complete_update_semantic_v1(&case_root, update_base, true, false)
            }
            CompleteMutationCaseV1::UpdateExactRejoinOverflow => {
                complete_update_semantic_v1(&case_root, update_base, false, true)
            }
            CompleteMutationCaseV1::UnauthenticatedBase => {
                complete_unauthenticated_base_v1(&case_root)
            }
        }
    }

    fn complete_replace_metadata_v1(root: &Path) -> CompleteMutationObservationV1 {
        let cas = FsCasV1::create_new(root).expect("replace/metadata semantic FsCas");
        let base_data = (0..48_123)
            .map(|index| (index * 37) as u8)
            .collect::<Vec<_>>();
        let replacement_data = (0..57_321)
            .map(|index| (index * 19 + 7) as u8)
            .collect::<Vec<_>>();
        let name = b"b.bin";
        let base_file = expected_file_details(&base_data, 0o644).expect("base file");
        let replacement_file =
            expected_file_details(&replacement_data, 0o600).expect("replacement file");
        let metadata_file = expected_file_details(&replacement_data, 0o640).expect("metadata file");
        let base_entries = [mutation_entry(name, &base_file, 0o644)];
        let replacement_entries = [mutation_entry(name, &replacement_file, 0o600)];
        let metadata_entries = [mutation_entry(name, &metadata_file, 0o640)];

        with_replacement_evidence_v1(
            DirectoryBuildModeV1::ImplicitRoot,
            &base_entries,
            0,
            |base_tree, base_evidence| {
                with_replacement_evidence_v1(
                    DirectoryBuildModeV1::ImplicitRoot,
                    &replacement_entries,
                    0,
                    |replacement_tree, metadata_evidence| {
                        with_replacement_evidence_v1(
                            DirectoryBuildModeV1::ImplicitRoot,
                            &metadata_entries,
                            0,
                            |metadata_tree, _| {
                                let (base_version, accepted_root) = accept_mutation_files_v1(
                                    &cas,
                                    0x510,
                                    &[(name, 0o644, &base_data)],
                                );
                                let mut control = MutationTraceControlV1::default();
                                let mut cow_logical = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
                                let mut source = SliceSource::new(&replacement_data);
                                let mut replace_scratch = OperationScratch::new();
                                let mut replace_counters = OperationCountersV1::default();
                                let replaced = run_complete_replace_v1(
                                    &cas,
                                    0x511,
                                    CdcAlgorithmV1::FastCdc,
                                    base_version,
                                    base_tree,
                                    base_evidence,
                                    0,
                                    name,
                                    0o600,
                                    replacement_data.len() as u64,
                                    &mut source,
                                    replace_scratch.borrow(),
                                    &mut cow_logical,
                                    &mut control,
                                    &mut replace_counters,
                                )
                                .expect("real complete Replace handoff");
                                let mut evidence = replacement_file.evidence();
                                let mut metadata_scratch = OperationScratch::new();
                                let mut metadata_counters = OperationCountersV1::default();
                                let metadata = run_complete_metadata_v1(
                                    &cas,
                                    0x512,
                                    replaced.version_record(),
                                    replacement_tree,
                                    metadata_evidence,
                                    0,
                                    name,
                                    0o640,
                                    replacement_file.authenticated(0o600),
                                    &mut evidence,
                                    metadata_scratch.borrow(),
                                    &mut cow_logical,
                                    &mut control,
                                    &mut metadata_counters,
                                )
                                .expect("real complete Metadata handoff");
                                let mut observation = mutation_observation(
                                    CompleteMutationTerminalV1::Succeeded,
                                    &cas,
                                    None,
                                    root,
                                    &metadata_counters,
                                );
                                observation.accepted_root = Some(accepted_root);
                                observation.base_tree = Some(base_tree.physical());
                                observation.replaced_root = Some(replaced.root_tree());
                                observation.replacement_tree = Some(replacement_tree.physical());
                                observation.metadata_file = Some(metadata_file.physical);
                                observation.replacement_file = Some(replacement_file.physical);
                                observation.metadata_root = Some(metadata.root_tree());
                                observation.metadata_tree = Some(metadata_tree.physical());
                                observation.completed_operations = 2;
                                observation.algorithm_is_fastcdc = replaced.algorithm()
                                    == CdcAlgorithmV1::FastCdc
                                    && metadata.algorithm() == CdcAlgorithmV1::FastCdc;
                                observation.validated_handoffs = control
                                    .boundaries
                                    .iter()
                                    .filter(|boundary| {
                                        **boundary == FsCasBoundaryV1::AfterCompleteValidatedHandoff
                                    })
                                    .count()
                                    as u32;
                                observation.storage_terminals =
                                    u32::from(mutation_storage_terminal(&replace_counters))
                                        + u32::from(mutation_storage_terminal(&metadata_counters));
                                observation.operation_counters = [
                                    complete_mutation_counters(&replace_counters),
                                    complete_mutation_counters(&metadata_counters),
                                    CompleteMutationCountersV1::default(),
                                ];
                                observation.operation_counter_count = 2;
                                observation
                            },
                        )
                        .expect("metadata expected tree")
                    },
                )
                .expect("replacement expected tree")
            },
        )
        .expect("base replacement evidence")
    }

    fn complete_add_move_remove_v1(root: &Path) -> CompleteMutationObservationV1 {
        let cas = FsCasV1::create_new(root).expect("add/move/remove semantic FsCas");
        let b_data = (0..24_321)
            .map(|index| (index * 11) as u8)
            .collect::<Vec<_>>();
        let d_data = (0..31_777)
            .map(|index| (index * 29 + 3) as u8)
            .collect::<Vec<_>>();
        let b_file = expected_file_details(&b_data, 0o644).expect("base file");
        let d_file = expected_file_details(&d_data, 0o600).expect("added file");
        let base_entries = [mutation_entry(b"b.bin", &b_file, 0o644)];
        let added_entries = [
            mutation_entry(b"b.bin", &b_file, 0o644),
            mutation_entry(b"d.bin", &d_file, 0o600),
        ];
        let moved_entries = [
            mutation_entry(b"a.bin", &d_file, 0o600),
            mutation_entry(b"b.bin", &b_file, 0o644),
        ];
        let removed_entries = [mutation_entry(b"b.bin", &b_file, 0o644)];

        with_mutation_evidence_v1(
            DirectoryBuildModeV1::ImplicitRoot,
            &base_entries,
            &added_entries,
            1,
            |base_tree, added_tree, add_evidence, add_tree_source| {
                let (base_version, accepted_root) =
                    accept_mutation_files_v1(&cas, 0x520, &[(b"b.bin", 0o644, &b_data)]);
                let mut control = MutationTraceControlV1::default();
                let mut cow_logical = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
                let mut source = SliceSource::new(&d_data);
                let mut scratch = OperationScratch::new();
                let mut add_counters = OperationCountersV1::default();
                let added = run_complete_add_v1(
                    &cas,
                    0x521,
                    base_version,
                    base_tree,
                    add_evidence,
                    1,
                    b"d.bin",
                    0o600,
                    d_data.len() as u64,
                    &mut source,
                    add_tree_source,
                    scratch.borrow(),
                    &mut cow_logical,
                    &mut control,
                    &mut add_counters,
                )
                .expect("real complete Add handoff");
                with_mutation_evidence_v1(
                    DirectoryBuildModeV1::ImplicitRoot,
                    &added_entries,
                    &moved_entries,
                    0,
                    |_, moved_tree, move_evidence, move_tree_source| {
                        let mut scratch = OperationScratch::new();
                        let mut move_counters = OperationCountersV1::default();
                        let moved = run_complete_move_v1(
                            &cas,
                            0x522,
                            added.version_record(),
                            added_tree,
                            move_evidence,
                            1,
                            0,
                            added_entries[1],
                            b"a.bin",
                            move_tree_source,
                            scratch.borrow(),
                            &mut cow_logical,
                            &mut control,
                            &mut move_counters,
                        )
                        .expect("real complete Move handoff");
                        with_mutation_evidence_v1(
                            DirectoryBuildModeV1::ImplicitRoot,
                            &moved_entries,
                            &removed_entries,
                            0,
                            |_, removed_tree, remove_evidence, remove_tree_source| {
                                let mut scratch = OperationScratch::new();
                                let mut remove_counters = OperationCountersV1::default();
                                let removed = run_complete_remove_v1(
                                    &cas,
                                    0x523,
                                    moved.version_record(),
                                    moved_tree,
                                    remove_evidence,
                                    0,
                                    moved_entries[0],
                                    remove_tree_source,
                                    scratch.borrow(),
                                    &mut cow_logical,
                                    &mut control,
                                    &mut remove_counters,
                                )
                                .expect("real complete Remove handoff");
                                let mut observation = mutation_observation(
                                    CompleteMutationTerminalV1::Succeeded,
                                    &cas,
                                    None,
                                    root,
                                    &remove_counters,
                                );
                                observation.accepted_root = Some(accepted_root);
                                observation.base_tree = Some(base_tree.physical());
                                observation.added_root = Some(added.root_tree());
                                observation.added_tree = Some(added_tree.physical());
                                observation.moved_root = Some(moved.root_tree());
                                observation.moved_tree = Some(moved_tree.physical());
                                observation.removed_root = Some(removed.root_tree());
                                observation.removed_tree = Some(removed_tree.physical());
                                observation.completed_operations = 3;
                                observation.final_root_returns_to_base =
                                    removed.root_tree() == base_tree.physical();
                                observation.algorithm_is_fastcdc = added.algorithm()
                                    == CdcAlgorithmV1::FastCdc
                                    && moved.algorithm() == CdcAlgorithmV1::FastCdc
                                    && removed.algorithm() == CdcAlgorithmV1::FastCdc;
                                observation.validated_handoffs = control
                                    .boundaries
                                    .iter()
                                    .filter(|boundary| {
                                        **boundary == FsCasBoundaryV1::AfterCompleteValidatedHandoff
                                    })
                                    .count()
                                    as u32;
                                observation.storage_terminals =
                                    u32::from(mutation_storage_terminal(&add_counters))
                                        + u32::from(mutation_storage_terminal(&move_counters))
                                        + u32::from(mutation_storage_terminal(&remove_counters));
                                observation.operation_counters = [
                                    complete_mutation_counters(&add_counters),
                                    complete_mutation_counters(&move_counters),
                                    complete_mutation_counters(&remove_counters),
                                ];
                                observation.operation_counter_count = 3;
                                observation
                            },
                        )
                        .expect("remove mutation evidence")
                    },
                )
                .expect("move mutation evidence")
            },
        )
        .expect("add mutation evidence")
    }

    fn complete_cross_directory_move_v1(root: &Path) -> CompleteMutationObservationV1 {
        let cas = FsCasV1::create_new(root).expect("cross-directory semantic FsCas");
        let moved_data = (0..28_417)
            .map(|index| (index * 17 + 5) as u8)
            .collect::<Vec<_>>();
        let resident_data = (0..19_007)
            .map(|index| (index * 31 + 9) as u8)
            .collect::<Vec<_>>();
        let moved_file = expected_file_details(&moved_data, 0o644).expect("moved file");
        let resident_file = expected_file_details(&resident_data, 0o600).expect("resident file");
        let source_base_entries = [mutation_entry(b"old.bin", &moved_file, 0o644)];
        let source_result_entries = [];
        let destination_base_entries = [mutation_entry(b"z.bin", &resident_file, 0o600)];
        let destination_result_entries = [
            mutation_entry(b"moved.bin", &moved_file, 0o644),
            mutation_entry(b"z.bin", &resident_file, 0o600),
        ];

        with_mutation_evidence_v1(
            DirectoryBuildModeV1::Explicit(0o755),
            &source_base_entries,
            &source_result_entries,
            0,
            |source_base, source_result, source_evidence, source_view| {
                with_mutation_evidence_v1(
                    DirectoryBuildModeV1::Explicit(0o755),
                    &destination_base_entries,
                    &destination_result_entries,
                    0,
                    |destination_base,
                     destination_result,
                     destination_evidence,
                     destination_view| {
                        let root_base_entries = [
                            mutation_directory_entry(b"left", source_base),
                            mutation_directory_entry(b"right", destination_base),
                        ];
                        let root_result_entries = [
                            mutation_directory_entry(b"left", source_result),
                            mutation_directory_entry(b"right", destination_result),
                        ];
                        with_mutation_evidence_v1(
                            DirectoryBuildModeV1::ImplicitRoot,
                            &root_base_entries,
                            &root_result_entries,
                            0,
                            |root_base, root_result, root_evidence, root_view| {
                                let (base_version, accepted_root) = accept_mutation_files_v1(
                                    &cas,
                                    0x524,
                                    &[
                                        (b"left/old.bin", 0o644, moved_data.as_slice()),
                                        (b"right/z.bin", 0o600, resident_data.as_slice()),
                                    ],
                                );
                                let mut scratch = OperationScratch::new();
                                let mut cow_logical = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
                                let mut control = MutationTraceControlV1::default();
                                let mut counters = OperationCountersV1::default();
                                let moved = complete_cross_directory_move_operation_v1(
                                    &cas,
                                    0x525,
                                    base_version,
                                    root_base,
                                    root_evidence,
                                    0,
                                    root_base_entries[0],
                                    source_base,
                                    source_evidence,
                                    0,
                                    source_base_entries[0],
                                    source_view,
                                    1,
                                    root_base_entries[1],
                                    destination_base,
                                    destination_evidence,
                                    0,
                                    b"moved.bin",
                                    destination_view,
                                    root_view,
                                    scratch.borrow(),
                                    &mut cow_logical,
                                    &mut control,
                                    &mut counters,
                                )
                                .expect("real cross-directory Move handoff");
                                let mut observation = mutation_observation(
                                    CompleteMutationTerminalV1::Succeeded,
                                    &cas,
                                    None,
                                    root,
                                    &counters,
                                );
                                observation.accepted_root = Some(accepted_root);
                                observation.base_tree = Some(root_base.physical());
                                observation.moved_root = Some(moved.root_tree());
                                observation.moved_tree = Some(root_result.physical());
                                observation.completed_operations = 1;
                                observation.algorithm_is_fastcdc =
                                    moved.algorithm() == CdcAlgorithmV1::FastCdc;
                                observation.validated_handoffs = control
                                    .boundaries
                                    .iter()
                                    .filter(|boundary| {
                                        **boundary == FsCasBoundaryV1::AfterCompleteValidatedHandoff
                                    })
                                    .count()
                                    as u32;
                                observation.storage_terminals =
                                    u32::from(mutation_storage_terminal(&counters));
                                observation.operation_counters[0] =
                                    complete_mutation_counters(&counters);
                                observation.operation_counter_count = 1;
                                observation
                            },
                        )
                        .expect("root mutation evidence")
                    },
                )
                .expect("destination mutation evidence")
            },
        )
        .expect("source mutation evidence")
    }

    fn complete_update_semantic_v1(
        root: &Path,
        base_data: &[u8],
        reference_overflow: bool,
        rejoin_overflow: bool,
    ) -> CompleteMutationObservationV1 {
        assert_eq!(base_data.len(), 300_000, "historical Update fixture length");
        let cas = FsCasV1::create_new(root).expect("update semantic FsCas");
        let stale = FsCasV1::open_existing(root).expect("update semantic stale handle");
        let inserted = b"changed";
        let range = UpdateRangeV1::new(120_000, 120_010, base_data.len() as u64)
            .expect("historical update range");
        let mut result_data = Vec::with_capacity(base_data.len() - 3);
        result_data.extend_from_slice(&base_data[..range.start() as usize]);
        result_data.extend_from_slice(inserted);
        result_data.extend_from_slice(&base_data[range.end() as usize..]);
        let name = b"b.bin";
        let base_file = expected_file_details(base_data, 0o644).expect("update base file");
        let result_file = expected_file_details(&result_data, 0o644).expect("update result file");
        let base_entries = [mutation_entry(name, &base_file, 0o644)];
        let result_entries = [mutation_entry(name, &result_file, 0o644)];

        with_replacement_evidence_v1(
            DirectoryBuildModeV1::ImplicitRoot,
            &base_entries,
            0,
            |base_tree, evidence| {
                with_replacement_evidence_v1(
                    DirectoryBuildModeV1::ImplicitRoot,
                    &result_entries,
                    0,
                    |result_tree, _| {
                        let (base_version, accepted_root) =
                            accept_mutation_files_v1(&cas, 0x530, &[(name, 0o644, base_data)]);
                        let namespace_before = (
                            directory_usage(&root.join("preparation")),
                            immutable_usage(root),
                        );
                        let mut inserted_source = SliceSource::new(inserted);
                        let mut base_reader = BaseBytesV1 { bytes: base_data };
                        let mut chunk_evidence = base_file.evidence();
                        let mut scratch = OperationScratch::new();
                        let mut cow_logical = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
                        let mut control = MutationTraceControlV1::default();
                        let mut counters = if reference_overflow {
                            OperationCountersV1 {
                                update_reference_metadata_records: 7,
                                update_reference_metadata_bytes: u64::MAX,
                                ..OperationCountersV1::default()
                            }
                        } else if rejoin_overflow {
                            OperationCountersV1 {
                                exact_rejoin_bytes: 7,
                                rejoin_successes: u64::MAX,
                                rejoin_failures: 11,
                                ..OperationCountersV1::default()
                            }
                        } else {
                            OperationCountersV1::default()
                        };
                        let terminal = run_complete_update_v1(
                            &cas,
                            if reference_overflow {
                                0x533
                            } else if rejoin_overflow {
                                0x535
                            } else {
                                0x531
                            },
                            base_version,
                            base_tree,
                            evidence,
                            0,
                            name,
                            0o644,
                            base_file.authenticated(0o644),
                            range,
                            inserted.len() as u64,
                            &mut inserted_source,
                            &mut base_reader,
                            &mut chunk_evidence,
                            scratch.borrow(),
                            &mut cow_logical,
                            &mut control,
                            &mut counters,
                        );
                        let expected_terminal = if reference_overflow || rejoin_overflow {
                            assert_eq!(
                                terminal,
                                Err(OperationErrorV1::Core(CoreError::IntegerOverflow))
                            );
                            CompleteMutationTerminalV1::IntegerOverflow
                        } else {
                            CompleteMutationTerminalV1::Succeeded
                        };
                        let exact_operation_namespace_usage = (
                            directory_usage(&root.join("preparation")),
                            immutable_usage(root),
                        );
                        let mut observation = mutation_observation(
                            expected_terminal,
                            &cas,
                            Some(&stale),
                            root,
                            &counters,
                        );
                        observation.accepted_root = Some(accepted_root);
                        observation.base_tree = Some(base_tree.physical());
                        if let Ok(updated) = terminal {
                            observation.updated_root = Some(updated.root_tree());
                            observation.update_tree = Some(result_tree.physical());
                            observation.completed_operations = 1;
                            observation.algorithm_is_fastcdc =
                                updated.algorithm() == CdcAlgorithmV1::FastCdc;
                            observation.update_algorithm = Some(updated.algorithm());
                            observation.storage_terminals =
                                u32::from(mutation_storage_terminal(&counters));
                        }
                        observation.validated_handoffs = control
                            .boundaries
                            .iter()
                            .filter(|boundary| {
                                **boundary == FsCasBoundaryV1::AfterCompleteValidatedHandoff
                            })
                            .count()
                            as u32;
                        observation.source_offset = inserted_source.offset as u64;
                        observation.operation_counters[0] = complete_mutation_counters(&counters);
                        observation.operation_counter_count = 1;
                        observation.namespace_before = Some(namespace_before);
                        observation.exact_operation_namespace_usage =
                            Some(exact_operation_namespace_usage);
                        observation
                    },
                )
                .expect("update result tree")
            },
        )
        .expect("update replacement evidence")
    }

    fn complete_unauthenticated_base_v1(root: &Path) -> CompleteMutationObservationV1 {
        let cas = FsCasV1::create_new(root).expect("unauthenticated semantic FsCas");
        let data = b"authenticated base";
        let file = expected_file_details(data, 0o644).expect("unauthenticated base file");
        let entries = [mutation_entry(b"b.bin", &file, 0o644)];
        with_replacement_evidence_v1(
            DirectoryBuildModeV1::ImplicitRoot,
            &entries,
            0,
            |tree, evidence| {
                let (version, accepted_root) =
                    accept_mutation_files_v1(&cas, 0x540, &[(b"b.bin", 0o644, data)]);
                let wrong_version = PhysicalVersionRecordIdV1::from_digest([0x5a; 32]);
                let namespace_before = (
                    directory_usage(&root.join("preparation")),
                    immutable_usage(root),
                );
                let mut source = SliceSource::new(b"replacement");
                let mut scratch = OperationScratch::new();
                let mut cow_logical = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
                let mut control = MutationTraceControlV1::default();
                let mut counters = OperationCountersV1::default();
                let error = run_complete_replace_v1(
                    &cas,
                    0x541,
                    CdcAlgorithmV1::FastCdc,
                    wrong_version,
                    tree,
                    evidence,
                    0,
                    b"b.bin",
                    0o644,
                    11,
                    &mut source,
                    scratch.borrow(),
                    &mut cow_logical,
                    &mut control,
                    &mut counters,
                )
                .expect_err("unaccepted version must fail closed");
                let terminal = match error {
                    OperationErrorV1::FsCas(_) => CompleteMutationTerminalV1::FsCas,
                    OperationErrorV1::Core(CoreError::IdMismatch) => {
                        CompleteMutationTerminalV1::IdMismatch
                    }
                    other => panic!("unexpected unauthenticated base error: {other:?}"),
                };
                let mut observation = mutation_observation(terminal, &cas, None, root, &counters);
                observation.accepted_root = Some(accepted_root);
                observation.base_tree = Some(tree.physical());
                observation.source_offset = source.offset as u64;
                observation.namespace_before = Some(namespace_before);
                observation.exact_operation_namespace_usage = Some((
                    directory_usage(&root.join("preparation")),
                    immutable_usage(root),
                ));
                observation.accepted_version = Some(version);
                observation.wrong_version = Some(wrong_version);
                observation
            },
        )
        .expect("unauthenticated replacement evidence")
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum LoadReaderStopV1 {
        Continue,
        Cancelled,
        Deadline,
    }

    struct LoadReaderControlV1 {
        stop: LoadReaderStopV1,
        armed: Arc<AtomicU64>,
        observed_polls: Arc<AtomicU64>,
        occupied_read_entered: Option<mpsc::SyncSender<()>>,
        occupied_read_gate: Option<Arc<WatchdogGateV1>>,
        abort: Arc<AtomicBool>,
    }

    impl LoadReaderControlV1 {
        fn cancellation_requested_v1(&self) -> bool {
            self.observed_polls.fetch_add(1, Ordering::AcqRel);
            self.abort.load(Ordering::Acquire)
                || (self.stop == LoadReaderStopV1::Cancelled
                    && self.armed.load(Ordering::Acquire) != 0)
        }
    }

    impl CdcControlV1 for LoadReaderControlV1 {
        fn cancellation_requested(&mut self) -> bool {
            self.cancellation_requested_v1()
        }

        fn deadline_exceeded(&mut self) -> bool {
            self.stop == LoadReaderStopV1::Deadline && self.armed.load(Ordering::Acquire) != 0
        }
    }

    impl FsCasControlV1 for LoadReaderControlV1 {
        fn cancellation_requested(&mut self) -> bool {
            self.cancellation_requested_v1()
        }

        fn deadline_exceeded(&mut self) -> bool {
            self.stop == LoadReaderStopV1::Deadline && self.armed.load(Ordering::Acquire) != 0
        }

        fn inject_filesystem_failure(
            &mut self,
            boundary: FsCasFilesystemBoundaryV1,
        ) -> Option<FsCasErrorV1> {
            if boundary == FsCasFilesystemBoundaryV1::CarrierPayloadRead {
                if let Some(entered) = self.occupied_read_entered.take() {
                    entered.send(()).expect("occupied-read gate receiver");
                    self.occupied_read_gate
                        .as_ref()
                        .expect("occupied reader gate")
                        .wait();
                }
            }
            None
        }
    }

    struct LoadWriterControlV1 {
        carrier_winner_entered: mpsc::SyncSender<()>,
        carrier_winner_gate: Arc<WatchdogGateV1>,
        active_wait_entered: mpsc::SyncSender<()>,
        carrier_winner_reported: bool,
        active_wait_reported: bool,
        delayed_comparison_windows: u64,
        catalog_fault_claim: Arc<AtomicU64>,
        comparison_delay_claim: Arc<AtomicU64>,
        comparison_entered: mpsc::SyncSender<()>,
        comparison_gate: Arc<WatchdogGateV1>,
        abort: Arc<AtomicBool>,
        fault_catalog_commit: bool,
        catalog_phase: bool,
        catalog_commit_failed: bool,
    }

    impl CdcControlV1 for LoadWriterControlV1 {
        fn cancellation_requested(&mut self) -> bool {
            self.abort.load(Ordering::Acquire)
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for LoadWriterControlV1 {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            match boundary {
                FsCasBoundaryV1::AfterCarrierInstall if !self.carrier_winner_reported => {
                    self.carrier_winner_reported = true;
                    self.fault_catalog_commit = self
                        .catalog_fault_claim
                        .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok();
                    if self.fault_catalog_commit {
                        self.carrier_winner_entered
                            .send(())
                            .expect("load carrier winner receiver");
                        self.carrier_winner_gate.wait();
                    }
                }
                FsCasBoundaryV1::ActivePackPublicationWait if !self.active_wait_reported => {
                    self.active_wait_reported = true;
                    self.active_wait_entered
                        .send(())
                        .expect("load active-publication receiver");
                }
                FsCasBoundaryV1::BeforeIncumbentComparisonWindow
                | FsCasBoundaryV1::BeforeObjectComparisonWindow => {
                    self.delayed_comparison_windows += 1;
                    if self
                        .comparison_delay_claim
                        .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                    {
                        self.comparison_entered
                            .send(())
                            .expect("load comparison receiver");
                        self.comparison_gate.wait();
                    }
                }
                FsCasBoundaryV1::BeforeCatalogPublication => self.catalog_phase = true,
                _ => {}
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            self.abort.load(Ordering::Acquire)
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_filesystem_failure(
            &mut self,
            boundary: FsCasFilesystemBoundaryV1,
        ) -> Option<FsCasErrorV1> {
            if self.fault_catalog_commit
                && self.catalog_phase
                && !self.catalog_commit_failed
                && boundary == FsCasFilesystemBoundaryV1::MarkerHardLink
            {
                self.catalog_commit_failed = true;
                Some(FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::NoSpace))
            } else {
                None
            }
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum LoadReaderTerminalObservationV1 {
        Succeeded,
        Cancelled,
        Deadline,
        OtherFailure,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct LoadReaderObservationV1 {
        pub terminal: LoadReaderTerminalObservationV1,
        pub full_extraction: bool,
        pub payload_bytes: u64,
        pub payload_matches: bool,
        pub sink_empty: bool,
        pub sink_finished: bool,
        pub sink_aborted: bool,
        pub counters: ConcurrentOperationCountersObservationV1,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum LoadWriterTerminalObservationV1 {
        Succeeded,
        NoSpace,
        OtherFailure,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct LoadWriterObservationV1 {
        pub terminal: LoadWriterTerminalObservationV1,
        pub delayed_comparison_windows: u64,
        pub carrier_winner_reported: bool,
        pub catalog_phase: bool,
        pub catalog_commit_failed: bool,
        pub canonical_version_matches: bool,
        pub canonical_root_matches: bool,
        pub canonical_carrier_count_matches: bool,
        pub carrier_count: u32,
        pub carriers_installed: u32,
        pub carriers_reused: u32,
        pub counters: ConcurrentOperationCountersObservationV1,
    }

    #[derive(Clone, Copy, Debug)]
    pub struct LoadContentionReportObservationV1 {
        pub reader_successes: usize,
        pub reader_cancelled: usize,
        pub reader_deadlines: usize,
        pub writer_successes: usize,
        pub writer_faults: usize,
        pub total_terminals: usize,
        pub cancellation_terminal_latency: Duration,
        pub deadline_terminal_latency: Duration,
        pub elapsed: Duration,
        pub throughput_numerator: usize,
        pub terminals_per_second: f64,
        pub admission_wait_tokens: usize,
        pub admission_wait_nanoseconds: u64,
        pub active_publication_wait_tokens: usize,
        pub active_publication_wait_nanoseconds: u64,
        pub visibility_wait_nanoseconds: u64,
        pub visibility_hold_nanoseconds: u64,
        pub publication_wait_nanoseconds: u64,
        pub publication_hold_nanoseconds: u64,
        pub final_preparation_bytes: u64,
        pub final_preparation_inodes: u64,
    }

    #[derive(Debug)]
    pub struct LoadContentionObservationV1 {
        pub initial_root_matches: bool,
        pub queue_before_waiters: (u64, u64, u64),
        pub stopped_readers: [usize; 2],
        pub stopped_queue: (u64, u64, u64),
        pub winner_queue: (u64, u64, u64),
        pub full_queue: (u64, u64, u64),
        pub readers: Vec<LoadReaderObservationV1>,
        pub writers: Vec<LoadWriterObservationV1>,
        pub queued_reader_tokens: usize,
        pub observed_admission_high_water: u64,
        pub active_wait_tokens: usize,
        pub faulted_writers: usize,
        pub installed_carriers: u32,
        pub reused_carriers: u32,
        pub canonical_carrier_count: u32,
        pub delayed_comparison_windows: u64,
        pub recorded_comparison_windows: u64,
        pub observed_root_bytes_high_water: u64,
        pub observed_root_inodes_high_water: u64,
        pub total_reserved_bytes: u64,
        pub total_reserved_inodes: u64,
        pub before_preparation: (u64, u64),
        pub after_preparation: (u64, u64),
        pub immutable_delta: (u64, u64),
        pub committed_total: (u64, u64),
        pub carrier_delta: usize,
        pub closure_entries: usize,
        pub report: LoadContentionReportObservationV1,
        pub authority_clean: bool,
        pub final_queue: (u64, u64, u64),
        pub root_usable: bool,
        pub reopened_usable: bool,
        pub namespace_entries_are_regular: bool,
    }

    pub fn reopened_reader_writer_contention_v1(root: &Path) -> LoadContentionObservationV1 {
        const READERS: usize = 32;
        const WRITERS: usize = 8;
        const ROOT_CAPACITY: u64 = 16;

        let seed = FsCasV1::create_new(root).expect("load semantic root");
        let reader_handles = (0..READERS)
            .map(|_| FsCasV1::open_existing(root).expect("load reopened reader"))
            .collect::<Vec<_>>();
        let writer_handles = (0..WRITERS)
            .map(|_| FsCasV1::open_existing(root).expect("load reopened writer"))
            .collect::<Vec<_>>();
        let base_data = (0..64_321)
            .map(|index| (index * 37 + 11) as u8)
            .collect::<Vec<_>>();
        let replacement_data = (0..72_119)
            .map(|index| (index * 19 + 7) as u8)
            .collect::<Vec<_>>();
        let name = b"load.bin";
        let (base_logical, base_physical) =
            expected_file(&base_data, 0o644).expect("load base identity");
        let base_entry = CanonicalTreeEntryV1::new(
            ValidatedComponent::new(name).expect("load component"),
            CanonicalTreeChildV1::File {
                logical: derive_file_node_v1(0o644, base_logical).expect("load file node"),
                physical: base_physical,
            },
        );
        let expected_root = with_replacement_evidence_v1(
            DirectoryBuildModeV1::ImplicitRoot,
            std::slice::from_ref(&base_entry),
            0,
            |tree, _| tree.physical(),
        )
        .expect("load expected root");
        let mut manifest = [TreeFileV1::new(
            name,
            0o644,
            base_data.len() as u64,
            SliceSupplier { bytes: &base_data },
        )];
        let mut scratch = OperationScratch::new();
        let mut control = ContinueControl;
        let mut counters = OperationCountersV1::default();
        let operation = request_tree_operation_v1(&seed, 0x580, &mut counters, &mut control)
            .expect("load base grant");
        let base_handoff = run_create_tree_v1(
            operation,
            CdcAlgorithmV1::FastCdc,
            &mut manifest,
            scratch.borrow(),
            &mut control,
            &mut counters,
        )
        .expect("load base handoff");
        let base_version = base_handoff.version_record();
        let accepted_root = base_handoff.root_tree();
        let before_preparation = directory_usage(&root.join("preparation"));
        let before_immutable = immutable_usage(root);
        let before_carriers = fs::read_dir(root.join("carriers"))
            .expect("load seed carriers")
            .count();

        let active_read_start = Arc::new(WatchdogGateV1::new());
        let waiting_read_start = Arc::new(WatchdogGateV1::new());
        let reader_delivery_gate = Arc::new(WatchdogGateV1::new());
        let occupied_read_gate = Arc::new(WatchdogGateV1::new());
        let (read_ready_tx, read_ready_rx) = mpsc::sync_channel(READERS);
        let (reader_delivery_tx, reader_delivery_rx) = mpsc::sync_channel(READERS);
        let (occupied_read_tx, occupied_read_rx) = mpsc::sync_channel(1);
        let (read_done_tx, read_done_rx) = mpsc::sync_channel(READERS);
        let cancelled_reader_armed = Arc::new(AtomicU64::new(0));
        let cancelled_reader_polls = Arc::new(AtomicU64::new(0));
        let deadline_reader_armed = Arc::new(AtomicU64::new(0));
        let deadline_reader_polls = Arc::new(AtomicU64::new(0));
        let winner_writer_start = Arc::new(WatchdogGateV1::new());
        let adopting_writer_start = Arc::new(WatchdogGateV1::new());
        let (writer_ready_tx, writer_ready_rx) = mpsc::sync_channel(WRITERS);
        let carrier_winner_gate = Arc::new(WatchdogGateV1::new());
        let (carrier_winner_tx, carrier_winner_rx) = mpsc::sync_channel(1);
        let (active_wait_tx, active_wait_rx) = mpsc::sync_channel(WRITERS - 1);
        let catalog_fault_claim = Arc::new(AtomicU64::new(0));
        let comparison_delay_claim = Arc::new(AtomicU64::new(0));
        let comparison_gate = Arc::new(WatchdogGateV1::new());
        let (comparison_tx, comparison_rx) = mpsc::sync_channel(1);
        let (writer_done_tx, writer_done_rx) = mpsc::sync_channel(WRITERS);
        let abort = Arc::new(AtomicBool::new(false));

        let (
            reader_results,
            writer_results,
            queue_before_waiters,
            stopped_readers,
            stopped_queue,
            winner_queue,
            full_queue,
            contention_elapsed,
            cancellation_terminal_latency,
            deadline_terminal_latency,
        ) = std::thread::scope(|scope| {
            let mut abort_on_drop = LoadAbortOnDropV1::new(Arc::clone(&abort));
            let mut active_read_release =
                WatchdogGateReleaseV1::new(Arc::clone(&active_read_start));
            let mut waiting_read_release =
                WatchdogGateReleaseV1::new(Arc::clone(&waiting_read_start));
            let mut reader_delivery_release =
                WatchdogGateReleaseV1::new(Arc::clone(&reader_delivery_gate));
            let mut occupied_read_release =
                WatchdogGateReleaseV1::new(Arc::clone(&occupied_read_gate));
            let mut winner_writer_release =
                WatchdogGateReleaseV1::new(Arc::clone(&winner_writer_start));
            let mut adopting_writer_release =
                WatchdogGateReleaseV1::new(Arc::clone(&adopting_writer_start));
            let mut carrier_winner_release =
                WatchdogGateReleaseV1::new(Arc::clone(&carrier_winner_gate));
            let mut comparison_release = WatchdogGateReleaseV1::new(Arc::clone(&comparison_gate));
            let reader_joins = reader_handles
                .into_iter()
                .enumerate()
                .map(|(index, cas)| {
                    let start = if index < ROOT_CAPACITY as usize {
                        Arc::clone(&active_read_start)
                    } else {
                        Arc::clone(&waiting_read_start)
                    };
                    let ready = read_ready_tx.clone();
                    let delivery_entered = reader_delivery_tx.clone();
                    let done = read_done_tx.clone();
                    let occupied_read_entered = (index == 0).then(|| occupied_read_tx.clone());
                    let selected_occupied_read_gate =
                        (index == 0).then(|| Arc::clone(&occupied_read_gate));
                    let abort = Arc::clone(&abort);
                    let (stop, armed, observed_polls) = match index {
                        30 => (
                            LoadReaderStopV1::Cancelled,
                            Arc::clone(&cancelled_reader_armed),
                            Arc::clone(&cancelled_reader_polls),
                        ),
                        31 => (
                            LoadReaderStopV1::Deadline,
                            Arc::clone(&deadline_reader_armed),
                            Arc::clone(&deadline_reader_polls),
                        ),
                        _ => (
                            LoadReaderStopV1::Continue,
                            Arc::new(AtomicU64::new(0)),
                            Arc::new(AtomicU64::new(0)),
                        ),
                    };
                    let base_data = &base_data;
                    let delivery_gate = Arc::clone(&reader_delivery_gate);
                    scope.spawn(move || {
                        ready.send(()).expect("load reader readiness");
                        start.wait();
                        let mut comparison = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
                        let mut path = boxed_zeroes::<MAX_PATH_BYTES>();
                        let mut counters = OperationCountersV1::default();
                        let mut control = LoadReaderControlV1 {
                            stop,
                            armed,
                            observed_polls,
                            occupied_read_entered,
                            occupied_read_gate: selected_occupied_read_gate,
                            abort,
                        };
                        let mut sink =
                            LoadReadSinkV1::new(base_data.len(), delivery_entered, delivery_gate);
                        let terminal = extract_root_v1(
                            &cas,
                            0x581 + index as u64,
                            base_version,
                            accepted_root,
                            &mut sink,
                            &mut counters,
                            ReadBuffersV1 {
                                comparison: &mut comparison,
                                path: &mut path,
                            },
                            &mut control,
                        );
                        done.send((index, Instant::now()))
                            .expect("load reader completion");
                        (terminal, sink, counters)
                    })
                })
                .collect::<Vec<_>>();

            for _ in 0..READERS {
                read_ready_rx
                    .recv_timeout(Duration::from_secs(5))
                    .expect("load reader rendezvous");
            }
            let contention_started_at = Instant::now();
            active_read_release.release();
            occupied_read_rx
                .recv_timeout(Duration::from_secs(10))
                .expect("load occupied-reader boundary");
            for _ in 0..ROOT_CAPACITY - 1 {
                reader_delivery_rx
                    .recv_timeout(Duration::from_secs(10))
                    .expect("load admitted-reader hold");
            }
            let queue_before_waiters = seed.operation_admission_queue_for_test_v1();
            waiting_read_release.release();
            let deadline = Instant::now() + Duration::from_secs(5);
            while seed.operation_admission_active_for_test_v1() != ROOT_CAPACITY
                || seed.operation_admission_queue_for_test_v1() != (16, 16, 0)
                || cancelled_reader_polls.load(Ordering::Acquire) < 2
                || deadline_reader_polls.load(Ordering::Acquire) < 2
            {
                if Instant::now() >= deadline {
                    panic!("load readers missed exact admission state");
                }
                std::thread::yield_now();
            }

            let cancellation_armed_at = Instant::now();
            cancelled_reader_armed.store(1, Ordering::Release);
            let deadline_armed_at = Instant::now();
            deadline_reader_armed.store(1, Ordering::Release);
            let mut stopped_readers = Vec::with_capacity(2);
            let mut cancellation_terminal_at = None;
            let mut deadline_terminal_at = None;
            for _ in 0..2 {
                let (index, terminal_at) = read_done_rx
                    .recv_timeout(Duration::from_secs(5))
                    .expect("load stopped reader terminal");
                match index {
                    30 => cancellation_terminal_at = Some(terminal_at),
                    31 => deadline_terminal_at = Some(terminal_at),
                    other => panic!("reader {other} terminalized before stopped waiters"),
                }
                stopped_readers.push(index);
            }
            stopped_readers.sort_unstable();
            let stopped_readers = stopped_readers
                .try_into()
                .unwrap_or_else(|_| unreachable!("exact stopped reader count"));
            let cancellation_terminal_latency = cancellation_terminal_at
                .expect("cancelled reader timestamp")
                .duration_since(cancellation_armed_at);
            let deadline_terminal_latency = deadline_terminal_at
                .expect("deadline reader timestamp")
                .duration_since(deadline_armed_at);
            let deadline = Instant::now() + Duration::from_secs(5);
            while seed.operation_admission_queue_for_test_v1() != (16, 14, 2) {
                if Instant::now() >= deadline {
                    panic!("load stopped reader tickets did not retire");
                }
                std::thread::yield_now();
            }
            let stopped_queue = seed.operation_admission_queue_for_test_v1();

            let writer_joins = writer_handles
                .into_iter()
                .enumerate()
                .map(|(index, cas)| {
                    let start = if index == 0 {
                        Arc::clone(&winner_writer_start)
                    } else {
                        Arc::clone(&adopting_writer_start)
                    };
                    let ready = writer_ready_tx.clone();
                    let carrier_winner_entered = carrier_winner_tx.clone();
                    let carrier_winner_gate = Arc::clone(&carrier_winner_gate);
                    let active_wait_entered = active_wait_tx.clone();
                    let catalog_fault_claim = Arc::clone(&catalog_fault_claim);
                    let comparison_delay_claim = Arc::clone(&comparison_delay_claim);
                    let comparison_entered = comparison_tx.clone();
                    let comparison_gate = Arc::clone(&comparison_gate);
                    let done = writer_done_tx.clone();
                    let abort = Arc::clone(&abort);
                    let replacement_data = &replacement_data;
                    let base_entry = &base_entry;
                    scope.spawn(move || {
                        ready.send(()).expect("load writer readiness");
                        start.wait();
                        let mut source = SliceSource::new(replacement_data);
                        let mut scratch = OperationScratch::new();
                        let mut cow_logical = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
                        let mut counters = OperationCountersV1::default();
                        let mut control = LoadWriterControlV1 {
                            carrier_winner_entered,
                            carrier_winner_gate,
                            active_wait_entered,
                            carrier_winner_reported: false,
                            active_wait_reported: false,
                            delayed_comparison_windows: 0,
                            catalog_fault_claim,
                            comparison_delay_claim,
                            comparison_entered,
                            comparison_gate,
                            abort,
                            fault_catalog_commit: false,
                            catalog_phase: false,
                            catalog_commit_failed: false,
                        };
                        let terminal = with_replacement_evidence_v1(
                            DirectoryBuildModeV1::ImplicitRoot,
                            std::slice::from_ref(base_entry),
                            0,
                            |base_tree, evidence| {
                                run_complete_replace_v1(
                                    &cas,
                                    0x5a1 + index as u64,
                                    CdcAlgorithmV1::FastCdc,
                                    base_version,
                                    base_tree,
                                    evidence,
                                    0,
                                    name,
                                    0o600,
                                    replacement_data.len() as u64,
                                    &mut source,
                                    scratch.borrow(),
                                    &mut cow_logical,
                                    &mut control,
                                    &mut counters,
                                )
                            },
                        )
                        .expect("load replacement evidence");
                        done.send((index, Instant::now()))
                            .expect("load writer completion");
                        (
                            terminal,
                            counters,
                            control.delayed_comparison_windows,
                            control.carrier_winner_reported,
                            control.catalog_phase,
                            control.catalog_commit_failed,
                        )
                    })
                })
                .collect::<Vec<_>>();

            for _ in 0..WRITERS {
                writer_ready_rx
                    .recv_timeout(Duration::from_secs(5))
                    .expect("load writer rendezvous");
            }
            winner_writer_release.release();
            let deadline = Instant::now() + Duration::from_secs(5);
            while seed.operation_admission_queue_for_test_v1() != (17, 15, 2) {
                if Instant::now() >= deadline {
                    panic!("load winner did not queue behind readers");
                }
                std::thread::yield_now();
            }
            let winner_queue = seed.operation_admission_queue_for_test_v1();
            adopting_writer_release.release();
            let deadline = Instant::now() + Duration::from_secs(5);
            while seed.operation_admission_active_for_test_v1() != ROOT_CAPACITY
                || seed.operation_admission_queue_for_test_v1() != (24, 22, 2)
            {
                if Instant::now() >= deadline {
                    panic!("load readers and writers missed exact overlap");
                }
                std::thread::yield_now();
            }
            let full_queue = seed.operation_admission_queue_for_test_v1();

            reader_delivery_release.release();
            carrier_winner_rx
                .recv_timeout(Duration::from_secs(10))
                .expect("load carrier winner gate");
            for _ in 0..WRITERS - 1 {
                active_wait_rx
                    .recv_timeout(Duration::from_secs(10))
                    .expect("load active-owner wait");
            }
            carrier_winner_release.release();
            comparison_rx
                .recv_timeout(Duration::from_secs(10))
                .expect("load comparison gate");
            comparison_release.release();
            occupied_read_release.release();

            let completion_deadline = Instant::now() + Duration::from_secs(15);
            for _ in 0..READERS - 2 {
                read_done_rx
                    .recv_timeout(completion_deadline.saturating_duration_since(Instant::now()))
                    .expect("load reader completion");
            }
            for _ in 0..WRITERS {
                writer_done_rx
                    .recv_timeout(completion_deadline.saturating_duration_since(Instant::now()))
                    .expect("load writer completion");
            }
            let readers = reader_joins
                .into_iter()
                .map(|join| join.join().expect("load reader thread"))
                .collect::<Vec<_>>();
            let writers = writer_joins
                .into_iter()
                .map(|join| join.join().expect("load writer thread"))
                .collect::<Vec<_>>();
            let elapsed = contention_started_at.elapsed();
            abort_on_drop.disarm();
            (
                readers,
                writers,
                queue_before_waiters,
                stopped_readers,
                stopped_queue,
                winner_queue,
                full_queue,
                elapsed,
                cancellation_terminal_latency,
                deadline_terminal_latency,
            )
        });

        let mut observed_admission_high_water = 0;
        let mut queued_reader_tokens = 0;
        let mut reader_successes = 0;
        let mut reader_cancelled = 0;
        let mut reader_deadlines = 0;
        let mut visibility_wait_nanoseconds = 0_u64;
        let mut visibility_hold_nanoseconds = 0_u64;
        let mut publication_wait_nanoseconds = 0_u64;
        let mut publication_hold_nanoseconds = 0_u64;
        let mut admission_wait_nanoseconds = 0_u64;
        let mut active_publication_wait_nanoseconds = 0_u64;
        let readers = reader_results
            .into_iter()
            .map(|(terminal, sink, counters)| {
                let (terminal, full_extraction, payload_bytes) = match terminal {
                    Ok(result) => {
                        assert_read_storage_terminal(&counters);
                        reader_successes += 1;
                        (
                            LoadReaderTerminalObservationV1::Succeeded,
                            result.kind() == ReadKindV1::FullExtraction,
                            result.payload_bytes(),
                        )
                    }
                    Err(ReadOperationErrorV1::FsCas(FsCasErrorV1::Core(CoreError::Cancelled))) => {
                        reader_cancelled += 1;
                        (LoadReaderTerminalObservationV1::Cancelled, false, 0)
                    }
                    Err(ReadOperationErrorV1::FsCas(FsCasErrorV1::Core(CoreError::Deadline))) => {
                        reader_deadlines += 1;
                        (LoadReaderTerminalObservationV1::Deadline, false, 0)
                    }
                    Err(_) => (LoadReaderTerminalObservationV1::OtherFailure, false, 0),
                };
                visibility_wait_nanoseconds += counters.visibility_lock_wait_nanoseconds;
                visibility_hold_nanoseconds += counters.visibility_lock_hold_nanoseconds;
                publication_wait_nanoseconds += counters.publication_lock_wait_nanoseconds;
                publication_hold_nanoseconds += counters.publication_lock_hold_nanoseconds;
                admission_wait_nanoseconds += counters.root_admission_wait_nanoseconds;
                active_publication_wait_nanoseconds +=
                    counters.active_pack_publication_wait_nanoseconds;
                observed_admission_high_water = observed_admission_high_water
                    .max(counters.root_admission_active_slots_high_water);
                queued_reader_tokens += usize::from(counters.root_admission_wait_polls > 0);
                LoadReaderObservationV1 {
                    terminal,
                    full_extraction,
                    payload_bytes,
                    payload_matches: sink.bytes == base_data,
                    sink_empty: sink.bytes.is_empty(),
                    sink_finished: sink.finished,
                    sink_aborted: sink.aborted,
                    counters: concurrent_counters(&counters),
                }
            })
            .collect::<Vec<_>>();

        let mut canonical_version = None;
        let mut canonical_root = None;
        let mut canonical_carrier_count = None;
        let mut installed_carriers = 0;
        let mut reused_carriers = 0;
        let mut total_committed_bytes = 0;
        let mut total_committed_inodes = 0;
        let mut total_reserved_bytes = 0;
        let mut total_reserved_inodes = 0;
        let mut active_wait_tokens = 0;
        let mut delayed_comparison_windows = 0;
        let mut recorded_comparison_windows = 0;
        let mut observed_root_bytes_high_water = 0;
        let mut observed_root_inodes_high_water = 0;
        let mut faulted_writers = 0;
        let mut successful_writers = 0;
        let writers = writer_results
            .into_iter()
            .map(
                |(
                    terminal,
                    counters,
                    delayed_windows,
                    carrier_winner_reported,
                    catalog_phase,
                    catalog_commit_failed,
                )| {
                    assert_balanced_storage_terminal(&counters);
                    let (
                        terminal,
                        canonical_version_matches,
                        canonical_root_matches,
                        canonical_carrier_count_matches,
                        carrier_count,
                        carriers_installed,
                        carriers_reused,
                    ) = match terminal {
                        Ok(handoff) => {
                            let version_matches = canonical_version
                                .map_or(true, |value| value == handoff.version_record());
                            let root_matches =
                                canonical_root.map_or(true, |value| value == handoff.root_tree());
                            let carrier_matches = canonical_carrier_count
                                .map_or(true, |value| value == handoff.carrier_count());
                            canonical_version.get_or_insert(handoff.version_record());
                            canonical_root.get_or_insert(handoff.root_tree());
                            canonical_carrier_count.get_or_insert(handoff.carrier_count());
                            installed_carriers += handoff.carriers_installed();
                            reused_carriers += handoff.carriers_reused();
                            successful_writers += 1;
                            (
                                LoadWriterTerminalObservationV1::Succeeded,
                                version_matches,
                                root_matches,
                                carrier_matches,
                                handoff.carrier_count(),
                                handoff.carriers_installed(),
                                handoff.carriers_reused(),
                            )
                        }
                        Err(OperationErrorV1::FsCas(FsCasErrorV1::Filesystem(
                            FsCasFilesystemFailureV1::NoSpace,
                        ))) => {
                            faulted_writers += 1;
                            (
                                LoadWriterTerminalObservationV1::NoSpace,
                                false,
                                false,
                                false,
                                0,
                                0,
                                0,
                            )
                        }
                        Err(_) => (
                            LoadWriterTerminalObservationV1::OtherFailure,
                            false,
                            false,
                            false,
                            0,
                            0,
                            0,
                        ),
                    };
                    active_wait_tokens +=
                        usize::from(counters.active_pack_publication_wait_polls > 0);
                    delayed_comparison_windows += delayed_windows;
                    recorded_comparison_windows += counters.incumbent_comparison_windows;
                    total_committed_bytes += counters.storage_bytes_committed;
                    total_committed_inodes += counters.storage_inodes_committed;
                    total_reserved_bytes += counters.storage_bytes_reserved;
                    total_reserved_inodes += counters.storage_inodes_reserved;
                    observed_admission_high_water = observed_admission_high_water
                        .max(counters.root_admission_active_slots_high_water);
                    observed_root_bytes_high_water = observed_root_bytes_high_water
                        .max(counters.root_storage_active_reserved_bytes_lifetime_high_water);
                    observed_root_inodes_high_water = observed_root_inodes_high_water
                        .max(counters.root_storage_active_reserved_inodes_lifetime_high_water);
                    visibility_wait_nanoseconds += counters.visibility_lock_wait_nanoseconds;
                    visibility_hold_nanoseconds += counters.visibility_lock_hold_nanoseconds;
                    publication_wait_nanoseconds += counters.publication_lock_wait_nanoseconds;
                    publication_hold_nanoseconds += counters.publication_lock_hold_nanoseconds;
                    admission_wait_nanoseconds += counters.root_admission_wait_nanoseconds;
                    active_publication_wait_nanoseconds +=
                        counters.active_pack_publication_wait_nanoseconds;
                    LoadWriterObservationV1 {
                        terminal,
                        delayed_comparison_windows: delayed_windows,
                        carrier_winner_reported,
                        catalog_phase,
                        catalog_commit_failed,
                        canonical_version_matches,
                        canonical_root_matches,
                        canonical_carrier_count_matches,
                        carrier_count,
                        carriers_installed,
                        carriers_reused,
                        counters: concurrent_counters(&counters),
                    }
                },
            )
            .collect::<Vec<_>>();

        let after_preparation = directory_usage(&root.join("preparation"));
        let after_immutable = immutable_usage(root);
        let after_carriers = fs::read_dir(root.join("carriers"))
            .expect("load carriers")
            .count();
        let closure_entries = fs::read_dir(root.join("closures"))
            .expect("load closures")
            .count();
        let immutable_delta = (
            after_immutable
                .0
                .checked_sub(before_immutable.0)
                .expect("load immutable byte delta"),
            after_immutable
                .1
                .checked_sub(before_immutable.1)
                .expect("load immutable inode delta"),
        );
        let total_terminals = reader_successes
            + reader_cancelled
            + reader_deadlines
            + successful_writers
            + faulted_writers;
        let throughput_numerator = READERS + WRITERS;
        let report = LoadContentionReportObservationV1 {
            reader_successes,
            reader_cancelled,
            reader_deadlines,
            writer_successes: successful_writers,
            writer_faults: faulted_writers,
            total_terminals,
            cancellation_terminal_latency,
            deadline_terminal_latency,
            elapsed: contention_elapsed,
            throughput_numerator,
            terminals_per_second: throughput_numerator as f64 / contention_elapsed.as_secs_f64(),
            admission_wait_tokens: queued_reader_tokens + WRITERS,
            admission_wait_nanoseconds,
            active_publication_wait_tokens: active_wait_tokens,
            active_publication_wait_nanoseconds,
            visibility_wait_nanoseconds,
            visibility_hold_nanoseconds,
            publication_wait_nanoseconds,
            publication_hold_nanoseconds,
            final_preparation_bytes: after_preparation.0,
            final_preparation_inodes: after_preparation.1,
        };
        let namespace_entries_are_regular =
            ["preparation", "carriers", "objects", "catalog", "closures"]
                .into_iter()
                .flat_map(|name| fs::read_dir(root.join(name)).expect("load namespace"))
                .all(|entry| {
                    entry
                        .ok()
                        .and_then(|entry| fs::symlink_metadata(entry.path()).ok())
                        .is_some_and(|metadata| metadata.file_type().is_file())
                });
        assert!(seed.occupied().is_ok());
        let reopened_usable = FsCasV1::open_existing(root)
            .expect("post-load reopened root")
            .occupied()
            .is_ok();
        assert!(reopened_usable);
        LoadContentionObservationV1 {
            initial_root_matches: accepted_root == expected_root,
            queue_before_waiters,
            stopped_readers,
            stopped_queue,
            winner_queue,
            full_queue,
            readers,
            writers,
            queued_reader_tokens,
            observed_admission_high_water,
            active_wait_tokens,
            faulted_writers,
            installed_carriers,
            reused_carriers,
            canonical_carrier_count: canonical_carrier_count.unwrap_or(0),
            delayed_comparison_windows,
            recorded_comparison_windows,
            observed_root_bytes_high_water,
            observed_root_inodes_high_water,
            total_reserved_bytes,
            total_reserved_inodes,
            before_preparation,
            after_preparation,
            immutable_delta,
            committed_total: (total_committed_bytes, total_committed_inodes),
            carrier_delta: after_carriers - before_carriers,
            closure_entries,
            report,
            authority_clean: operation_authority_is_clean(&seed, root),
            final_queue: seed.operation_admission_queue_for_test_v1(),
            root_usable: true,
            reopened_usable,
            namespace_entries_are_regular,
        }
    }

    #[derive(Default)]
    struct PanicWhileQueuedControlV1 {
        panicked: bool,
    }

    impl FsCasControlV1 for PanicWhileQueuedControlV1 {
        fn cancellation_requested(&mut self) -> bool {
            if !self.panicked {
                self.panicked = true;
                panic!("injected queued cancellation observation unwind");
            }
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    #[derive(Debug, Eq, PartialEq)]
    pub struct QueuedControlUnwindObservationV1 {
        pub panic_payload: Option<&'static str>,
        pub control_panicked: bool,
        pub active_before_release: u64,
        pub queue_before_release: (u64, u64, u64),
        pub clean_after_followup: bool,
    }

    pub fn queued_control_unwind_v1(root: &Path) -> QueuedControlUnwindObservationV1 {
        let cas = FsCasV1::create_new(root).expect("queued-unwind semantic root");
        let mut control = ContinueControl;
        let mut counters = OperationCountersV1::default();
        let mut active = Vec::with_capacity(16);
        for cancellation_key in 0..16 {
            active.push(
                request_create_operation_v1(&cas, cancellation_key, &mut counters, &mut control)
                    .expect("saturate root operation admission"),
            );
        }

        let mut panic_control = PanicWhileQueuedControlV1::default();
        let unwind = catch_unwind(AssertUnwindSafe(|| {
            let _ = request_create_operation_v1(&cas, 16, &mut counters, &mut panic_control);
        }))
        .expect_err("queued cancellation observation must unwind");
        let panic_payload = unwind.downcast_ref::<&'static str>().copied();
        let active_before_release = cas.operation_admission_active_for_test_v1();
        let queue_before_release = cas.operation_admission_queue_for_test_v1();

        drop(active);
        assert_operation_authority_baseline(&cas, root);
        let clean_after_unwind = operation_authority_is_clean(&cas, root);
        let mut terminal = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                17,
                &mut counters,
                &mut control,
            )
            .expect("post-unwind operation capability");
        terminal
            .finish_terminal_v1(false, &mut counters, &mut control)
            .expect("post-unwind operation terminal");
        assert_operation_authority_baseline(&cas, root);
        assert_storage_equations(&counters);

        QueuedControlUnwindObservationV1 {
            panic_payload,
            control_panicked: panic_control.panicked,
            active_before_release,
            queue_before_release,
            clean_after_followup: clean_after_unwind
                && operation_authority_is_clean(&cas, root)
                && storage_equations_hold(&counters)
                && counters.has_zero_forbidden_work(),
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum RootLockBoundaryObservationV1 {
        VisibilityAcquired,
        PublicationAcquired,
        VisibilityReleased,
        PublicationReleased,
    }

    impl RootLockBoundaryObservationV1 {
        const fn boundary(self) -> FsCasBoundaryV1 {
            match self {
                Self::VisibilityAcquired => FsCasBoundaryV1::VisibilityLockAcquired,
                Self::PublicationAcquired => FsCasBoundaryV1::PublicationLockAcquired,
                Self::VisibilityReleased => FsCasBoundaryV1::VisibilityLockReleased,
                Self::PublicationReleased => FsCasBoundaryV1::PublicationLockReleased,
            }
        }
    }

    struct PanicAtRootLockBoundaryV1 {
        target: FsCasBoundaryV1,
        target_occurrence: usize,
        matching_boundaries: usize,
        panicked: bool,
    }

    impl CdcControlV1 for PanicAtRootLockBoundaryV1 {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    impl FsCasControlV1 for PanicAtRootLockBoundaryV1 {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary == self.target {
                self.matching_boundaries += 1;
                if !self.panicked && self.matching_boundaries == self.target_occurrence {
                    self.panicked = true;
                    panic!("injected root-lock boundary unwind");
                }
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum RootStateObservationV1 {
        Usable,
        Invalidated,
        Rejected,
    }

    fn root_state(cas: &FsCasV1) -> RootStateObservationV1 {
        match cas.occupied() {
            Ok(_) => RootStateObservationV1::Usable,
            Err(FsCasErrorV1::Invalidated) => RootStateObservationV1::Invalidated,
            Err(_) => RootStateObservationV1::Rejected,
        }
    }

    fn reopened_root_state(root: &Path) -> RootStateObservationV1 {
        match FsCasV1::open_existing(root) {
            Ok(cas) if cas.occupied().is_ok() => RootStateObservationV1::Usable,
            Err(FsCasErrorV1::Invalidated) => RootStateObservationV1::Invalidated,
            Ok(_) | Err(_) => RootStateObservationV1::Rejected,
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct RootLockUnwindCaseObservationV1 {
        pub target: RootLockBoundaryObservationV1,
        pub panic_payload: Option<&'static str>,
        pub control_panicked: bool,
        pub storage_bytes_committed: u64,
        pub storage_inodes_committed: u64,
        pub zero_forbidden_work: bool,
        pub visibility_available: bool,
        pub publication_available: bool,
        pub storage_bytes_retained: u64,
        pub storage_inodes_retained: u64,
        pub immutable_residue_bytes: u64,
        pub immutable_residue_inodes: u64,
        pub root_state: RootStateObservationV1,
        pub stale_state: RootStateObservationV1,
        pub reopened_state: RootStateObservationV1,
        pub followup_storage_bytes_retained: u64,
        pub followup_storage_inodes_retained: u64,
        pub followup_zero_forbidden_work: bool,
    }

    pub fn root_lock_callback_unwind_v1(roots: [&Path; 4]) -> [RootLockUnwindCaseObservationV1; 4] {
        let targets = [
            RootLockBoundaryObservationV1::VisibilityAcquired,
            RootLockBoundaryObservationV1::PublicationAcquired,
            RootLockBoundaryObservationV1::VisibilityReleased,
            RootLockBoundaryObservationV1::PublicationReleased,
        ];
        std::array::from_fn(|index| {
            let root = roots[index];
            let target = targets[index];
            let cas = FsCasV1::create_new(root).expect("root-lock semantic root");
            let stale = FsCasV1::open_existing(root).expect("root-lock stale handle");
            let input = [0x39_u8; 64 * 1024 + 17];
            let mut counters = OperationCountersV1::default();
            let mut scratch = OperationScratch::new();
            let mut control = PanicAtRootLockBoundaryV1 {
                target: target.boundary(),
                target_occurrence: usize::from(
                    target == RootLockBoundaryObservationV1::PublicationReleased,
                ) + 1,
                matching_boundaries: 0,
                panicked: false,
            };
            let grant = request_create_operation_v1(&cas, 0x180, &mut counters, &mut control)
                .expect("root-lock operation grant");
            let unwind = catch_unwind(AssertUnwindSafe(|| {
                let _ = run_create_v1(
                    grant,
                    CdcAlgorithmV1::FastCdc,
                    b"first.bin",
                    0o644,
                    input.len() as u64,
                    SliceSupplier { bytes: &input },
                    scratch.borrow(),
                    &mut control,
                    &mut counters,
                );
            }))
            .expect_err("root-lock callback must unwind");
            let panic_payload = unwind.downcast_ref::<&'static str>().copied();
            if !operation_authority_is_clean(&cas, root) || !storage_equations_hold(&counters) {
                panic!("root-lock unwind leaked authority or storage accounting");
            }
            assert_operation_authority_baseline(&cas, root);
            assert_storage_equations(&counters);
            let visibility_available = cas.visibility_lock_available_for_test_v1();
            let publication_available = cas.publication_lock_available_for_test_v1();
            let stale_state = root_state(&stale);
            let root_state = root_state(&cas);
            let reopened_state = reopened_root_state(root);

            let mut followup_storage_bytes_retained = 0;
            let mut followup_storage_inodes_retained = 0;
            let mut followup_zero_forbidden_work = true;
            if target != RootLockBoundaryObservationV1::PublicationReleased {
                if root_state != RootStateObservationV1::Usable
                    || stale_state != RootStateObservationV1::Usable
                {
                    panic!("prepublication root-lock unwind poisoned the root");
                }
                let mut followup_counters = OperationCountersV1::default();
                let mut followup_control = ContinueControl;
                let followup = request_create_operation_v1(
                    &cas,
                    0x181,
                    &mut followup_counters,
                    &mut followup_control,
                )
                .expect("root-lock followup grant");
                run_create_v1(
                    followup,
                    CdcAlgorithmV1::FastCdc,
                    b"second.bin",
                    0o644,
                    input.len() as u64,
                    SliceSupplier { bytes: &input },
                    scratch.borrow(),
                    &mut followup_control,
                    &mut followup_counters,
                )
                .expect("root-lock followup create");
                if !operation_authority_is_clean(&cas, root)
                    || !storage_equations_hold(&followup_counters)
                {
                    panic!("root-lock followup leaked authority or accounting");
                }
                assert_operation_authority_baseline(&cas, root);
                assert_storage_equations(&followup_counters);
                followup_storage_bytes_retained = followup_counters.storage_bytes_retained;
                followup_storage_inodes_retained = followup_counters.storage_inodes_retained;
                followup_zero_forbidden_work = followup_counters.has_zero_forbidden_work();
            }

            RootLockUnwindCaseObservationV1 {
                target,
                panic_payload,
                control_panicked: control.panicked,
                storage_bytes_committed: counters.storage_bytes_committed,
                storage_inodes_committed: counters.storage_inodes_committed,
                zero_forbidden_work: counters.has_zero_forbidden_work(),
                visibility_available,
                publication_available,
                storage_bytes_retained: counters.storage_bytes_retained,
                storage_inodes_retained: counters.storage_inodes_retained,
                immutable_residue_bytes: counters.immutable_residue_bytes,
                immutable_residue_inodes: counters.immutable_residue_inodes,
                root_state,
                stale_state,
                reopened_state,
                followup_storage_bytes_retained,
                followup_storage_inodes_retained,
                followup_zero_forbidden_work,
            }
        })
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum QueuedTransitionObservationV1 {
        Grant,
        Cancelled,
        Deadline,
    }

    struct ArmableQueuedControlV1 {
        transition: QueuedTransitionObservationV1,
        armed: Arc<AtomicBool>,
        observed_polls: Arc<AtomicU64>,
    }

    impl FsCasControlV1 for ArmableQueuedControlV1 {
        fn cancellation_requested(&mut self) -> bool {
            self.observed_polls.fetch_add(1, Ordering::AcqRel);
            self.transition == QueuedTransitionObservationV1::Cancelled
                && self.armed.load(Ordering::Acquire)
        }

        fn deadline_exceeded(&mut self) -> bool {
            self.transition == QueuedTransitionObservationV1::Deadline
                && self.armed.load(Ordering::Acquire)
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct QueuedTransitionCaseObservationV1 {
        pub transition: QueuedTransitionObservationV1,
        pub active_at_capacity: u64,
        pub queue_deadline_preserved: bool,
        pub active_while_queued: u64,
        pub terminal_was_early: bool,
        pub setup_zero_forbidden_work: bool,
        pub terminal: QueuedTransitionObservationV1,
        pub queue_entries: u64,
        pub queue_refusals: u64,
        pub queue_depth_high_water: u64,
        pub active_slots_high_water: u64,
        pub wait_polls: u64,
        pub wait_nanoseconds: u64,
        pub release_failures: u64,
        pub zero_forbidden_work: bool,
        pub queue_after: (u64, u64, u64),
    }

    pub fn seventeenth_operation_queue_v1(
        roots: [&Path; 3],
    ) -> [QueuedTransitionCaseObservationV1; 3] {
        let transitions = [
            QueuedTransitionObservationV1::Grant,
            QueuedTransitionObservationV1::Cancelled,
            QueuedTransitionObservationV1::Deadline,
        ];
        std::array::from_fn(|index| {
            let root = roots[index];
            let transition = transitions[index];
            let cas = FsCasV1::create_new(root).expect("C+1 semantic root");
            let waiter_cas = FsCasV1::open_existing(root).expect("C+1 waiter handle");
            let armed = Arc::new(AtomicBool::new(false));
            let observed_polls = Arc::new(AtomicU64::new(0));
            let (terminal_tx, terminal_rx) = mpsc::sync_channel(1);

            let (
                terminal,
                counters,
                active_at_capacity,
                queue_deadline_preserved,
                active_while_queued,
                terminal_was_early,
                setup_zero_forbidden_work,
            ) = std::thread::scope(|scope| {
                let mut setup_control = ContinueControl;
                let mut setup_counters = OperationCountersV1::default();
                let mut active = Vec::with_capacity(16);
                for key in 0..16_u64 {
                    active.push(
                        request_create_operation_v1(
                            &cas,
                            0x20_000 + key,
                            &mut setup_counters,
                            &mut setup_control,
                        )
                        .expect("saturate C+1 root"),
                    );
                }
                let active_at_capacity = cas.operation_admission_active_for_test_v1();
                let waiter_armed = Arc::clone(&armed);
                let waiter_polls = Arc::clone(&observed_polls);
                let waiter = scope.spawn(move || {
                    let mut control = ArmableQueuedControlV1 {
                        transition,
                        armed: waiter_armed,
                        observed_polls: waiter_polls,
                    };
                    let mut counters = OperationCountersV1::default();
                    let terminal = request_create_operation_v1(
                        &waiter_cas,
                        0x20_100,
                        &mut counters,
                        &mut control,
                    )
                    .map(|capability| {
                        drop(capability);
                        QueuedTransitionObservationV1::Grant
                    })
                    .unwrap_or_else(|error| match error {
                        FsCasErrorV1::Core(CoreError::Cancelled) => {
                            QueuedTransitionObservationV1::Cancelled
                        }
                        FsCasErrorV1::Core(CoreError::Deadline) => {
                            QueuedTransitionObservationV1::Deadline
                        }
                        other => panic!("unexpected C+1 terminal: {other:?}"),
                    });
                    terminal_tx
                        .send((terminal, counters))
                        .expect("C+1 terminal receiver");
                });

                let deadline = Instant::now() + Duration::from_secs(5);
                let queue_deadline_preserved = true;
                while cas.operation_admission_queue_for_test_v1() != (1, 1, 0)
                    || observed_polls.load(Ordering::Acquire) < 2
                {
                    assert!(Instant::now() < deadline, "C+1 queue rendezvous timed out");
                    std::thread::yield_now();
                }
                let active_while_queued = cas.operation_admission_active_for_test_v1();
                let terminal_was_early =
                    !matches!(terminal_rx.try_recv(), Err(mpsc::TryRecvError::Empty));
                match transition {
                    QueuedTransitionObservationV1::Grant => {
                        drop(active.pop().expect("one saturated capability"));
                    }
                    QueuedTransitionObservationV1::Cancelled
                    | QueuedTransitionObservationV1::Deadline => {
                        armed.store(true, Ordering::Release);
                    }
                }
                let terminal = terminal_rx
                    .recv_timeout(Duration::from_secs(5))
                    .expect("C+1 terminal timeout");
                waiter.join().expect("C+1 waiter thread");
                drop(active);
                assert_storage_equations(&setup_counters);
                let setup_zero_forbidden_work = storage_equations_hold(&setup_counters)
                    && setup_counters.has_zero_forbidden_work();
                (
                    terminal.0,
                    terminal.1,
                    active_at_capacity,
                    queue_deadline_preserved,
                    active_while_queued,
                    terminal_was_early,
                    setup_zero_forbidden_work,
                )
            });

            if !operation_authority_is_clean(&cas, root) || !storage_equations_hold(&counters) {
                panic!("C+1 schedule leaked authority or accounting");
            }
            assert_operation_authority_baseline(&cas, root);
            assert_storage_equations(&counters);
            QueuedTransitionCaseObservationV1 {
                transition,
                active_at_capacity,
                queue_deadline_preserved,
                active_while_queued,
                terminal_was_early,
                setup_zero_forbidden_work,
                terminal,
                queue_entries: counters.root_admission_queue_entries,
                queue_refusals: counters.root_admission_queue_refusals,
                queue_depth_high_water: counters.root_admission_queue_depth_high_water,
                active_slots_high_water: counters.root_admission_active_slots_high_water,
                wait_polls: counters.root_admission_wait_polls,
                wait_nanoseconds: counters.root_admission_wait_nanoseconds,
                release_failures: counters.root_admission_release_failures,
                zero_forbidden_work: counters.has_zero_forbidden_work(),
                queue_after: cas.operation_admission_queue_for_test_v1(),
            }
        })
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum AdmissionRefusalObservationV1 {
        Cancelled,
        Deadline,
        Queue,
        StorageBytes,
        StorageInodes,
    }

    struct StopWhileQueuedControlV1(AdmissionRefusalObservationV1);

    impl FsCasControlV1 for StopWhileQueuedControlV1 {
        fn cancellation_requested(&mut self) -> bool {
            self.0 == AdmissionRefusalObservationV1::Cancelled
        }

        fn deadline_exceeded(&mut self) -> bool {
            self.0 == AdmissionRefusalObservationV1::Deadline
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct QueuedStopCaseObservationV1 {
        pub expected: AdmissionRefusalObservationV1,
        pub terminal: AdmissionRefusalObservationV1,
        pub supplier_invoked: bool,
        pub preparation_entries: u64,
        pub queue_entries: u64,
        pub queue_refusals: u64,
        pub queue_depth_high_water: u64,
        pub active_slots_high_water: u64,
        pub wait_polls: u64,
        pub memory_refusals: u64,
        pub release_failures: u64,
        pub zero_forbidden_work: bool,
    }

    pub fn queued_stop_before_supplier_v1(roots: [&Path; 2]) -> [QueuedStopCaseObservationV1; 2] {
        let stops = [
            AdmissionRefusalObservationV1::Cancelled,
            AdmissionRefusalObservationV1::Deadline,
        ];
        std::array::from_fn(|index| {
            let root = roots[index];
            let expected = stops[index];
            let cas = FsCasV1::create_new(root).expect("queued-stop semantic root");
            let mut control = ContinueControl;
            let mut counters = OperationCountersV1::default();
            let mut active = Vec::with_capacity(16);
            for key in 0..16 {
                active.push(
                    request_create_operation_v1(&cas, key, &mut counters, &mut control)
                        .expect("saturate queued-stop root"),
                );
            }
            let supplier_invoked = AtomicBool::new(false);
            let mut stop_control = StopWhileQueuedControlV1(expected);
            let terminal = request_create_operation_v1(&cas, 16, &mut counters, &mut stop_control)
                .map(|_| panic!("stopped queued request was admitted"))
                .unwrap_or_else(|error| match error {
                    FsCasErrorV1::Core(CoreError::Cancelled) => {
                        AdmissionRefusalObservationV1::Cancelled
                    }
                    FsCasErrorV1::Core(CoreError::Deadline) => {
                        AdmissionRefusalObservationV1::Deadline
                    }
                    other => panic!("unexpected queued-stop terminal: {other:?}"),
                });
            let preparation_entries = fs::read_dir(root.join("preparation"))
                .expect("queued-stop preparation namespace")
                .count() as u64;
            assert_eq!(preparation_entries, 0);
            drop(active);
            assert_storage_equations(&counters);
            if !operation_authority_is_clean(&cas, root) || !storage_equations_hold(&counters) {
                panic!("queued-stop schedule leaked authority or accounting");
            }
            assert_operation_authority_baseline(&cas, root);
            QueuedStopCaseObservationV1 {
                expected,
                terminal,
                supplier_invoked: supplier_invoked.load(Ordering::Acquire),
                preparation_entries,
                queue_entries: counters.root_admission_queue_entries,
                queue_refusals: counters.root_admission_queue_refusals,
                queue_depth_high_water: counters.root_admission_queue_depth_high_water,
                active_slots_high_water: counters.root_admission_active_slots_high_water,
                wait_polls: counters.root_admission_wait_polls,
                memory_refusals: counters.root_admission_memory_refusals,
                release_failures: counters.root_admission_release_failures,
                zero_forbidden_work: counters.has_zero_forbidden_work(),
            }
        })
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct QueueCapacityObservationV1 {
        pub terminal: AdmissionRefusalObservationV1,
        pub supplier_invoked: bool,
        pub queue_entries: u64,
        pub queue_refusals: u64,
        pub source_read_calls: u64,
        pub source_bytes_read: u64,
        pub preparation_bytes_high_water: u64,
        pub preparation_inodes_high_water: u64,
        pub open_handles_high_water: u64,
        pub preparation_entries: u64,
        pub pending_tickets: usize,
    }

    pub fn queue_capacity_refusal_v1(root: &Path) -> QueueCapacityObservationV1 {
        let cas = FsCasV1::create_new(root).expect("queue-capacity semantic root");
        let pending = (0_u64..1_024)
            .map(|key| {
                cas.issue_pending_admission_for_test_v1(key)
                    .expect("fill admission queue")
            })
            .collect::<Vec<_>>();
        let supplier_invoked = AtomicBool::new(false);
        let mut control = ContinueControl;
        let mut counters = OperationCountersV1::default();
        let result = request_create_operation_v1(&cas, 1_024, &mut counters, &mut control);
        assert!(matches!(
            &result,
            Err(FsCasErrorV1::ResourceExhausted(FsCasResourceV1::Queue))
        ));
        let terminal = result
            .map(|_| panic!("queue-capacity request was admitted"))
            .unwrap_or_else(|error| match error {
                FsCasErrorV1::ResourceExhausted(FsCasResourceV1::Queue) => {
                    AdmissionRefusalObservationV1::Queue
                }
                other => panic!("unexpected queue-capacity terminal: {other:?}"),
            });
        let observation = QueueCapacityObservationV1 {
            terminal,
            supplier_invoked: supplier_invoked.load(Ordering::Acquire),
            queue_entries: counters.root_admission_queue_entries,
            queue_refusals: counters.root_admission_queue_refusals,
            source_read_calls: counters.source_read_calls,
            source_bytes_read: counters.source_bytes_read,
            preparation_bytes_high_water: counters.storage_preparation_bytes_high_water,
            preparation_inodes_high_water: counters.storage_preparation_inodes_high_water,
            open_handles_high_water: counters.layerfs_open_file_handles_high_water,
            preparation_entries: fs::read_dir(root.join("preparation"))
                .expect("queue-capacity preparation namespace")
                .count() as u64,
            pending_tickets: pending.len(),
        };
        drop(pending);
        observation
    }

    struct CallbackCheckedSupplier<'a> {
        bound_invoked: &'a AtomicBool,
        supply_invoked: &'a AtomicBool,
    }

    impl SourceSupplierV1 for CallbackCheckedSupplier<'_> {
        type Source = SliceSource<'static>;

        fn resident_memory_bound_bytes(&self) -> crate::CoreResult<u64> {
            self.bound_invoked.store(true, Ordering::Release);
            Ok(core::mem::size_of::<SliceSource<'static>>() as u64)
        }

        fn supply(self) -> crate::CoreResult<Self::Source> {
            self.supply_invoked.store(true, Ordering::Release);
            Ok(SliceSource::new(&[0]))
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct StorageRefusalCaseObservationV1 {
        pub resource: AdmissionRefusalObservationV1,
        pub terminal: AdmissionRefusalObservationV1,
        pub bound_invoked: bool,
        pub supply_invoked: bool,
        pub source_read_calls: u64,
        pub source_bytes_read: u64,
        pub operation_slots: u64,
        pub operation_active: u64,
        pub storage_active_operations: u64,
        pub preparation_entries: u64,
        pub zero_forbidden_work: bool,
        pub blocker_byte_equation_holds: bool,
        pub blocker_inode_equation_holds: bool,
        pub blocker_zero_forbidden_work: bool,
        pub authority_clean: bool,
        pub blocker_storage_equations_hold: bool,
    }

    pub fn storage_refusal_before_supplier_v1(
        roots: [&Path; 2],
    ) -> [StorageRefusalCaseObservationV1; 2] {
        let rows = [
            (
                AdmissionRefusalObservationV1::StorageBytes,
                FsCasResourceV1::StorageBytes,
                FsStorageEnvelopeV1::new(
                    ROOT_LOGICAL_STORAGE_BUDGET_V1 - 16 * 1_024 * 1_024,
                    0,
                    1,
                    0,
                )
                .expect("storage-byte blocker envelope"),
            ),
            (
                AdmissionRefusalObservationV1::StorageInodes,
                FsCasResourceV1::StorageInodes,
                FsStorageEnvelopeV1::new(1, 0, ROOT_NAMESPACE_ENTRY_BUDGET_V1 - 16, 0)
                    .expect("storage-inode blocker envelope"),
            ),
        ];
        std::array::from_fn(|index| {
            let root = roots[index];
            let (resource, expected_resource, envelope) = rows[index];
            let cas = FsCasV1::create_new(root).expect("storage-refusal semantic root");
            let mut control = ContinueControl;
            let mut blocker_counters = OperationCountersV1::default();
            let mut blocker = cas
                .begin_operation_capability_v1(
                    FsOperationKindV1::CompleteC3File,
                    0x51,
                    &mut blocker_counters,
                    &mut control,
                )
                .expect("storage-refusal blocker operation");
            blocker
                .declare_storage_envelope_v1(envelope)
                .expect("storage-refusal blocker reservation");

            let bound_invoked = AtomicBool::new(false);
            let supply_invoked = AtomicBool::new(false);
            let mut counters = OperationCountersV1::default();
            let mut scratch = OperationScratch::new();
            let grant = request_create_operation_v1(&cas, 0x52, &mut counters, &mut control)
                .expect("storage-refusal contender operation");
            let result = run_create_v1(
                grant,
                CdcAlgorithmV1::FastCdc,
                b"payload.bin",
                0o644,
                1,
                CallbackCheckedSupplier {
                    bound_invoked: &bound_invoked,
                    supply_invoked: &supply_invoked,
                },
                scratch.borrow(),
                &mut control,
                &mut counters,
            );
            assert!(matches!(
                &result,
                Err(OperationErrorV1::FsCas(FsCasErrorV1::ResourceExhausted(observed)))
                    if *observed == expected_resource
            ));
            let terminal = result
                .map(|_| panic!("storage-refused request completed"))
                .unwrap_or_else(|error| match error {
                    OperationErrorV1::FsCas(FsCasErrorV1::ResourceExhausted(
                        FsCasResourceV1::StorageBytes,
                    )) => AdmissionRefusalObservationV1::StorageBytes,
                    OperationErrorV1::FsCas(FsCasErrorV1::ResourceExhausted(
                        FsCasResourceV1::StorageInodes,
                    )) => AdmissionRefusalObservationV1::StorageInodes,
                    other => panic!("unexpected storage-refusal terminal: {other:?}"),
                });
            let (storage_active_operations, _, _) = cas.storage_admission_active_for_test_v1();
            let operation_slots = cas.operation_admitted_slots_v1();
            let operation_active = cas.operation_admission_active_for_test_v1();
            let preparation_entries = fs::read_dir(root.join("preparation"))
                .expect("storage-refusal preparation namespace")
                .count() as u64;
            let zero_forbidden_work =
                storage_equations_hold(&counters) && counters.has_zero_forbidden_work();

            blocker
                .finish_storage_admission_v1(false, &mut blocker_counters, &mut control)
                .expect("release storage-refusal reservation");
            blocker
                .finish_operation_admission_v1(&mut blocker_counters, &mut control)
                .expect("release storage-refusal operation");
            assert_storage_equations(&counters);
            assert_storage_equations(&blocker_counters);
            let blocker_byte_equation_holds = blocker_counters.storage_bytes_requested
                == blocker_counters
                    .storage_bytes_released
                    .checked_add(blocker_counters.storage_bytes_committed)
                    .and_then(|value| value.checked_add(blocker_counters.storage_bytes_retained))
                    .unwrap_or(u64::MAX);
            let blocker_inode_equation_holds = blocker_counters.storage_inodes_requested
                == blocker_counters
                    .storage_inodes_released
                    .checked_add(blocker_counters.storage_inodes_committed)
                    .and_then(|value| value.checked_add(blocker_counters.storage_inodes_retained))
                    .unwrap_or(u64::MAX);
            let authority_clean = operation_authority_is_clean(&cas, root);
            let blocker_storage_equations_hold = storage_equations_hold(&blocker_counters);
            if !authority_clean || !blocker_storage_equations_hold {
                panic!("storage-refusal blocker leaked authority or accounting");
            }
            assert_operation_authority_baseline(&cas, root);
            StorageRefusalCaseObservationV1 {
                resource,
                terminal,
                bound_invoked: bound_invoked.load(Ordering::Acquire),
                supply_invoked: supply_invoked.load(Ordering::Acquire),
                source_read_calls: counters.source_read_calls,
                source_bytes_read: counters.source_bytes_read,
                operation_slots,
                operation_active,
                storage_active_operations,
                preparation_entries,
                zero_forbidden_work,
                blocker_byte_equation_holds,
                blocker_inode_equation_holds,
                blocker_zero_forbidden_work: blocker_counters.has_zero_forbidden_work(),
                authority_clean,
                blocker_storage_equations_hold,
            }
        })
    }
}

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
            locator_publication_receipt_preparation_bytes_bound_v1(MAX_PACK_RECORDS),
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
pub trait LifecycleControlV1: CdcControlV1 + FsCasControlV1 {}

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

    #[cfg(any(test, feature = "operation-polymorphism"))]
    fn inject_residue_accounting_failure(
        &mut self,
        boundary: crate::cas::FsCasResidueAccountingBoundaryV1,
    ) -> bool {
        (**self.inner.borrow_mut()).inject_residue_accounting_failure(boundary)
    }

    #[cfg(any(test, feature = "operation-polymorphism"))]
    fn before_carrier_no_replace_transition_for_test_v1(&mut self) {
        (**self.inner.borrow_mut()).before_carrier_no_replace_transition_for_test_v1();
    }

    #[cfg(any(test, feature = "operation-polymorphism"))]
    fn inject_carrier_receipt_transition_failure_v1(
        &mut self,
        check: crate::cas::CarrierReceiptTransitionCheckV1,
    ) -> Option<crate::cas::FsCasErrorV1> {
        (**self.inner.borrow_mut()).inject_carrier_receipt_transition_failure_v1(check)
    }

    #[cfg(any(test, feature = "operation-polymorphism"))]
    fn inject_operation_terminal_unwind_after_release(&mut self) -> bool {
        (**self.inner.borrow_mut()).inject_operation_terminal_unwind_after_release()
    }

    #[cfg(any(test, feature = "operation-polymorphism"))]
    fn inject_root_lock_observation_failure(&mut self) -> Option<CoreError> {
        (**self.inner.borrow_mut()).inject_root_lock_observation_failure()
    }

    #[cfg(any(test, feature = "operation-polymorphism"))]
    fn inject_carrier_counter_accumulation_overflow(&mut self) -> bool {
        (**self.inner.borrow_mut()).inject_carrier_counter_accumulation_overflow()
    }

    #[cfg(any(test, feature = "operation-polymorphism"))]
    fn inject_global_seen_counter_accumulation_overflow(&mut self) -> bool {
        (**self.inner.borrow_mut()).inject_global_seen_counter_accumulation_overflow()
    }

    #[cfg(any(test, feature = "operation-polymorphism"))]
    fn inject_pack_object_disposition_overflow(&mut self, created: bool) -> bool {
        (**self.inner.borrow_mut()).inject_pack_object_disposition_overflow(created)
    }

    #[cfg(any(test, feature = "operation-polymorphism"))]
    fn inject_operation_spool_write_observation_overflow(&mut self) -> bool {
        (**self.inner.borrow_mut()).inject_operation_spool_write_observation_overflow()
    }

    #[cfg(any(test, feature = "operation-polymorphism"))]
    fn inject_operation_spool_precharge_failure_v1(&mut self) -> Option<crate::cas::FsCasErrorV1> {
        (**self.inner.borrow_mut()).inject_operation_spool_precharge_failure_v1()
    }

    #[cfg(any(test, feature = "operation-polymorphism"))]
    fn inject_counted_pack_read_observation_overflow(&mut self) -> bool {
        (**self.inner.borrow_mut()).inject_counted_pack_read_observation_overflow()
    }

    #[cfg(any(test, feature = "operation-polymorphism"))]
    fn inject_same_carrier_comparison_observation_overflow(&mut self) -> bool {
        (**self.inner.borrow_mut()).inject_same_carrier_comparison_observation_overflow()
    }

    #[cfg(any(test, feature = "operation-polymorphism"))]
    fn inject_pending_unwind_retirement_failure(&mut self) -> Option<crate::cas::FsCasErrorV1> {
        (**self.inner.borrow_mut()).inject_pending_unwind_retirement_failure()
    }

    #[cfg(any(test, feature = "operation-polymorphism"))]
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
pub struct StorageOperationV1<'root> {
    capability: FsOperationCapabilityV1<'root>,
}

pub struct CreateOperationGrantV1<'root> {
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

pub fn request_create_operation_v1<'root, C>(
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

pub fn request_tree_operation_v1<'root, C>(
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
pub enum OperationErrorV1 {
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
pub struct OperationHandoffV1 {
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
    pub const fn algorithm(self) -> CdcAlgorithmV1 {
        self.algorithm
    }

    pub const fn version_record(self) -> PhysicalVersionRecordIdV1 {
        self.version_record
    }

    pub const fn root_tree(self) -> PhysicalTreeIdV1 {
        self.root_tree
    }

    pub const fn pack(self) -> SealedPackV1 {
        self.pack
    }

    pub const fn pack_outcome(self) -> FsPackAdmissionOutcomeV1 {
        self.pack_outcome
    }

    pub const fn carrier_count(self) -> u32 {
        self.carrier_count
    }

    pub const fn carrier_rollovers(self) -> u32 {
        self.carrier_rollovers
    }

    pub const fn carriers_installed(self) -> u32 {
        self.carriers_installed
    }

    pub const fn carriers_reused(self) -> u32 {
        self.carriers_reused
    }

    pub const fn object_count(self) -> u64 {
        self.object_count
    }

    pub const fn reference_spool_bytes(self) -> OptionalU64ObservationV1 {
        self.reference_spool_bytes
    }

    pub const fn index_spool_bytes(self) -> OptionalU64ObservationV1 {
        self.index_spool_bytes
    }

    pub const fn terminal_optional_observations(self) -> TerminalOptionalObservationsV1 {
        self.terminal_optional_observations
    }
}

pub struct OperationBuffersV1<'a> {
    pub source: &'a mut [u8; MAXIMUM_CHUNK_BYTES],
    pub cdc_ring: &'a mut [u8; MAXIMUM_CHUNK_BYTES],
    pub incoming_comparison: &'a mut [u8; COMPARISON_WINDOW_BYTES],
    pub occupied_comparison: &'a mut [u8; COMPARISON_WINDOW_BYTES],
    pub tree_object: &'a mut [u8; MAX_TREE_OBJECT_BYTES],
    pub tree_pages: &'a mut [Option<TreePageSummaryV1>],
    pub traversal_state: &'a mut [u8],
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

const DEFAULT_METADATA_RESERVATION_BYTES: u64 = 1_048_576;
const DEFAULT_EXPLICIT_DIRECTORY_MODE: u16 = 0o755;
const MANIFEST_BUILD_STACK_CAPACITY_V1: usize = MAX_PATH_DEPTH + 1;

fn closure_traversal_bytes_v1(maximum_objects: u64) -> CoreResult<usize> {
    usize::try_from(maximum_objects)
        .map_err(|_| CoreError::IntegerOverflow)?
        .checked_mul(2)
        .and_then(|bits| bits.checked_add(7))
        .map(|bits| bits / 8)
        .ok_or(CoreError::IntegerOverflow)
}

fn global_seen_capacity_v1(maximum_objects: u64) -> CoreResult<u32> {
    let required = maximum_objects
        .checked_mul(2)
        .ok_or(CoreError::IntegerOverflow)?
        .max(8);
    let capacity = required
        .checked_next_power_of_two()
        .ok_or(CoreError::IntegerOverflow)?;
    u32::try_from(capacity).map_err(|_| CoreError::CountCap)
}

#[allow(clippy::too_many_arguments)]
pub fn run_create_v1<S, C>(
    grant: CreateOperationGrantV1<'_>,
    algorithm: CdcAlgorithmV1,
    name: &[u8],
    mode: u16,
    declared_len: u64,
    supplier: S,
    buffers: OperationBuffersV1<'_>,
    control: &mut C,
    counters: &mut OperationCountersV1,
) -> Result<OperationHandoffV1, OperationErrorV1>
where
    S: SourceSupplierV1,
    C: LifecycleControlV1 + ?Sized,
{
    let mut operation = grant.into_operation();
    let (component, maximum_records_u32, global_seen_capacity, supplier_resident, storage_resident) =
        operation.run_preparation_free_stage_v1(
            counters,
            control,
            |operation, counters, _control| {
                operation.require_complete_file_kind_v1()?;
                operation.declare_empty_storage_envelope_v1()?;

                let component = ValidatedComponent::new(name)?;
                let maximum_refs = declared_len
                    .checked_add(8_191)
                    .ok_or(CoreError::IntegerOverflow)?
                    / 8_192;
                validate_chunk_refs_per_file(maximum_refs)?;
                let maximum_records = maximum_refs
                    .checked_add(4)
                    .ok_or(CoreError::IntegerOverflow)?;
                if maximum_records > MAX_STORAGE_RECORDS_V1 {
                    return Err(CoreError::CountCap.into());
                }
                let maximum_records_u32 =
                    u32::try_from(maximum_records).map_err(|_| CoreError::IntegerOverflow)?;
                let global_seen_capacity = global_seen_capacity_v1(maximum_records)?;
                let root_shape = preflight_canonical_tree_v1(1)?;
                let required_traversal_bytes = closure_traversal_bytes_v1(maximum_records)?;
                if buffers.traversal_state.len() < required_traversal_bytes
                    || buffers.tree_pages.len()
                        < usize::try_from(root_shape.page_summary_count())
                            .map_err(|_| CoreError::IntegerOverflow)?
                {
                    return Err(CoreError::ResourceRefused.into());
                }

                operation.declare_storage_envelope_v1(storage_envelope_v1(
                    maximum_records,
                    maximum_records,
                    maximum_refs,
                    1,
                    u64::from(root_shape.tree_object_count()),
                    declared_len,
                    global_seen_capacity,
                    false,
                )?)?;

                // The final conservative envelope is live before the first
                // supplier callback. No preparation path exists in this stage.
                let supplier_resident = supplier.resident_memory_bound_bytes()?;
                let storage_resident =
                    operation.storage_resident_plan_v1(false, maximum_records_u32)?;
                let port_resident = supplier_resident
                    .checked_add(storage_resident.total_resident_bytes_v1())
                    .ok_or(CoreError::IntegerOverflow)?;
                let metadata_reservation = port_resident.max(DEFAULT_METADATA_RESERVATION_BYTES);
                let plan = OperationMemoryPlanV1::empty()
                    .charge(MemoryComponentV1::SourceWindow, buffers.source.len() as u64)?
                    .charge(MemoryComponentV1::CdcRing, buffers.cdc_ring.len() as u64)?
                    .charge(
                        MemoryComponentV1::ComparisonWindow,
                        (2 * COMPARISON_WINDOW_BYTES) as u64,
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
                    .charge(MemoryComponentV1::MetadataWindow, metadata_reservation)?
                    .charge(
                        MemoryComponentV1::HashState,
                        IDENTITY_HASHER_BYTES_V1
                            .checked_mul(2)
                            .ok_or(CoreError::IntegerOverflow)?,
                    )?;
                operation.declare_plan_v1(plan)?;
                counters.memory_high_water = counters
                    .memory_high_water
                    .max(operation.memory_high_water_bytes_v1());
                Ok((
                    component,
                    maximum_records_u32,
                    global_seen_capacity,
                    supplier_resident,
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
            maximum_records: maximum_records_u32,
            algorithm,
        },
        buffers,
        control,
        counters,
        move |storage, control_cell, reservation, buffers, counters| {
            let mut source = supplier.supply()?;
            if source.resident_memory_bound_bytes()? > supplier_resident {
                return Err(CoreError::ResourceRefused.into());
            }
            let mut cdc_control = SharedOperationControlV1::new(control_cell);
            let (references, sink) = storage.content_parts_v1();
            let file = create_file_borrowed_v1(
                name,
                mode,
                declared_len,
                &mut source,
                sink,
                references,
                ContentBuffersV1::new(buffers.source, buffers.cdc_ring),
                &mut cdc_control,
                reservation,
                algorithm,
                counters,
            )?;
            let file_node = derive_file_node_v1(mode, file.logical_file())?;
            let entry = CanonicalTreeEntryV1::new(
                component,
                CanonicalTreeChildV1::File {
                    logical: file_node,
                    physical: file.physical_file(),
                },
            );
            let tree = build_canonical_directory_borrowed_v1(
                DirectoryBuildModeV1::ImplicitRoot,
                &[entry],
                storage.tree_sink_v1(),
                reservation,
                counters,
                buffers.tree_object,
                buffers.tree_pages,
            )?;
            let DirectoryLogicalIdentityV1::ImplicitRoot(logical_root) = tree.logical() else {
                return Err(CoreError::TypeDomain.into());
            };
            let version = storage.write_version_v1(
                derive_version_v1(logical_root),
                tree.physical(),
                VersionSummaryInputV1::new(
                    declared_len,
                    declared_len,
                    tree.entry_count(),
                    u32::from(declared_len != 0),
                    file.chunk_count(),
                ),
                counters,
            )?;
            let completed = storage.complete_v1(version)?;
            let reference_spool_bytes = storage.reference_storage_bytes_v1()?;
            Ok(PreparedCandidateV1::new(
                version,
                tree.physical(),
                completed,
                reference_spool_bytes,
            ))
        },
    )
}

#[derive(Clone, Copy, Default)]
struct TreePreflightV1 {
    directory_entry_count: u64,
    tree_object_count: u64,
    directory_count: u64,
    peak_entry_memory: u64,
    maximum_page_summary_count: u32,
}

impl TreePreflightV1 {
    fn add_child(&mut self, child: Self) -> CoreResult<()> {
        self.directory_entry_count = self
            .directory_entry_count
            .checked_add(child.directory_entry_count)
            .ok_or(CoreError::IntegerOverflow)?;
        self.tree_object_count = self
            .tree_object_count
            .checked_add(child.tree_object_count)
            .ok_or(CoreError::IntegerOverflow)?;
        self.directory_count = self
            .directory_count
            .checked_add(child.directory_count)
            .ok_or(CoreError::IntegerOverflow)?;
        self.peak_entry_memory = self.peak_entry_memory.max(child.peak_entry_memory);
        self.maximum_page_summary_count = self
            .maximum_page_summary_count
            .max(child.maximum_page_summary_count);
        Ok(())
    }
}

fn path_component_at(path: &[u8], prefix_len: usize) -> CoreResult<(&[u8], bool)> {
    let tail = path.get(prefix_len..).ok_or(CoreError::Path)?;
    if tail.is_empty() {
        return Err(CoreError::Path);
    }
    match tail.iter().position(|&byte| byte == b'/') {
        Some(end) => Ok((&tail[..end], true)),
        None => Ok((tail, false)),
    }
}

fn directory_group_end<S>(
    files: &[TreeFileV1<'_, S>],
    start: usize,
    end: usize,
    prefix_len: usize,
) -> CoreResult<usize> {
    let (component, _) = path_component_at(files[start].path(), prefix_len)?;
    let mut cursor = start + 1;
    while cursor < end {
        let (candidate, _) = path_component_at(files[cursor].path(), prefix_len)?;
        if candidate != component {
            break;
        }
        cursor += 1;
    }
    Ok(cursor)
}

#[derive(Clone, Copy)]
struct ManifestPreflightFrameV1 {
    start: usize,
    end: usize,
    prefix_len: usize,
    cursor: usize,
    entry_count: u64,
    result: TreePreflightV1,
}

impl ManifestPreflightFrameV1 {
    const fn new(start: usize, end: usize, prefix_len: usize) -> Self {
        Self {
            start,
            end,
            prefix_len,
            cursor: start,
            entry_count: 0,
            result: TreePreflightV1 {
                directory_entry_count: 0,
                tree_object_count: 0,
                directory_count: 0,
                peak_entry_memory: 0,
                maximum_page_summary_count: 0,
            },
        }
    }

    fn finish(mut self) -> CoreResult<TreePreflightV1> {
        if self.cursor != self.end || self.start > self.end {
            return Err(CoreError::Truncated);
        }
        let shape = preflight_canonical_tree_v1(self.entry_count)?;
        self.result.directory_entry_count = self
            .result
            .directory_entry_count
            .checked_add(self.entry_count)
            .ok_or(CoreError::IntegerOverflow)?;
        self.result.tree_object_count = self
            .result
            .tree_object_count
            .checked_add(u64::from(shape.tree_object_count()))
            .ok_or(CoreError::IntegerOverflow)?;
        self.result.directory_count = self
            .result
            .directory_count
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        let entry_bytes = self
            .entry_count
            .checked_mul(
                u64::try_from(core::mem::size_of::<CanonicalTreeEntryV1<'static>>())
                    .map_err(|_| CoreError::IntegerOverflow)?,
            )
            .ok_or(CoreError::IntegerOverflow)?;
        self.result.peak_entry_memory = entry_bytes
            .checked_add(self.result.peak_entry_memory)
            .ok_or(CoreError::IntegerOverflow)?;
        self.result.maximum_page_summary_count = self
            .result
            .maximum_page_summary_count
            .max(shape.page_summary_count());
        Ok(self.result)
    }
}

fn preflight_manifest_directory_v1<S>(
    files: &[TreeFileV1<'_, S>],
    start: usize,
    end: usize,
    prefix_len: usize,
) -> CoreResult<TreePreflightV1> {
    // This pass intentionally uses an explicit fixed-capacity stack. A legal
    // 256-component path must not depend on the platform's native call-stack
    // size, and an invalid 257th component has already been rejected by
    // `ValidatedPath` before this function is entered.
    let mut stack = [None::<ManifestPreflightFrameV1>; MAX_PATH_DEPTH.saturating_add(1)];
    stack[0] = Some(ManifestPreflightFrameV1::new(start, end, prefix_len));
    let mut depth = 0_usize;
    loop {
        let frame = stack[depth].ok_or(CoreError::Truncated)?;
        if frame.cursor == frame.end {
            let completed = frame.finish()?;
            stack[depth] = None;
            if depth == 0 {
                return Ok(completed);
            }
            depth -= 1;
            stack[depth]
                .as_mut()
                .ok_or(CoreError::Truncated)?
                .result
                .add_child(completed)?;
            continue;
        }
        if frame.cursor > frame.end {
            return Err(CoreError::Truncated);
        }
        let cursor = frame.cursor;
        let (component, has_descendants) =
            path_component_at(files[cursor].path(), frame.prefix_len)?;
        ValidatedComponent::new(component)?;
        let group_end = directory_group_end(files, cursor, frame.end, frame.prefix_len)?;
        let current = stack[depth].as_mut().ok_or(CoreError::Truncated)?;
        current.entry_count = current
            .entry_count
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        current.cursor = group_end;
        if has_descendants {
            let child_prefix = frame
                .prefix_len
                .checked_add(component.len())
                .and_then(|value| value.checked_add(1))
                .ok_or(CoreError::IntegerOverflow)?;
            let child_depth = depth.checked_add(1).ok_or(CoreError::IntegerOverflow)?;
            if child_depth >= stack.len() {
                return Err(CoreError::CountCap);
            }
            stack[child_depth] = Some(ManifestPreflightFrameV1::new(
                cursor,
                group_end,
                child_prefix,
            ));
            depth = child_depth;
        } else if group_end != cursor + 1 {
            return Err(CoreError::Path);
        }
    }
}

struct ManifestBuildFrameV1<'path> {
    end: usize,
    prefix_len: usize,
    cursor: usize,
    mode: DirectoryBuildModeV1,
    component_in_parent: Option<ValidatedComponent<'path>>,
    entries: Vec<CanonicalTreeEntryV1<'path>>,
}

fn manifest_build_stack_resident_bytes_v1() -> CoreResult<u64> {
    let frame_bytes = u64::try_from(core::mem::size_of::<ManifestBuildFrameV1<'static>>())
        .map_err(|_| CoreError::IntegerOverflow)?;
    let capacity =
        u64::try_from(MANIFEST_BUILD_STACK_CAPACITY_V1).map_err(|_| CoreError::IntegerOverflow)?;
    frame_bytes
        .checked_mul(capacity)
        .and_then(|bytes| {
            bytes.checked_add(core::mem::size_of::<Vec<ManifestBuildFrameV1<'static>>>() as u64)
        })
        .ok_or(CoreError::IntegerOverflow)
}

fn manifest_directory_entry_count_v1<S>(
    files: &[TreeFileV1<'_, S>],
    start: usize,
    end: usize,
    prefix_len: usize,
) -> CoreResult<usize> {
    let mut cursor = start;
    let mut entry_count = 0_usize;
    while cursor < end {
        entry_count = entry_count
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        cursor = directory_group_end(files, cursor, end, prefix_len)?;
    }
    Ok(entry_count)
}

fn new_manifest_build_frame_v1<'path, S>(
    files: &[TreeFileV1<'path, S>],
    start: usize,
    end: usize,
    prefix_len: usize,
    mode: DirectoryBuildModeV1,
    component_in_parent: Option<ValidatedComponent<'path>>,
) -> CoreResult<ManifestBuildFrameV1<'path>> {
    let entry_count = manifest_directory_entry_count_v1(files, start, end, prefix_len)?;
    let mut entries = Vec::new();
    entries
        .try_reserve_exact(entry_count)
        .map_err(|_| CoreError::ResourceRefused)?;
    if entries.capacity() > entry_count {
        return Err(CoreError::ResourceRefused);
    }
    Ok(ManifestBuildFrameV1 {
        end,
        prefix_len,
        cursor: start,
        mode,
        component_in_parent,
        entries,
    })
}

#[allow(clippy::too_many_arguments)]
fn build_manifest_directory_v1<'path, S, P>(
    files: &[TreeFileV1<'path, S>],
    storage: &mut P,
    start: usize,
    end: usize,
    prefix_len: usize,
    mode: DirectoryBuildModeV1,
    reservation: &crate::limits::OperationReservationV1<'_>,
    counters: &mut OperationCountersV1,
    object_scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
    page_scratch: &mut [Option<TreePageSummaryV1>],
) -> CoreResult<crate::cow::CanonicalDirectoryTreeV1>
where
    P: StorageSessionPortV1 + ?Sized,
{
    let mut frames = Vec::new();
    frames
        .try_reserve_exact(MANIFEST_BUILD_STACK_CAPACITY_V1)
        .map_err(|_| CoreError::ResourceRefused)?;
    if frames.capacity() > MANIFEST_BUILD_STACK_CAPACITY_V1 {
        return Err(CoreError::ResourceRefused);
    }
    frames.push(new_manifest_build_frame_v1(
        files, start, end, prefix_len, mode, None,
    )?);
    loop {
        let frame_index = frames.len().checked_sub(1).ok_or(CoreError::Truncated)?;
        let cursor = frames[frame_index].cursor;
        let frame_end = frames[frame_index].end;
        let frame_prefix_len = frames[frame_index].prefix_len;
        if cursor == frame_end {
            let frame = frames.pop().ok_or(CoreError::Truncated)?;
            let directory = build_canonical_directory_borrowed_v1(
                frame.mode,
                &frame.entries,
                storage.tree_sink_v1(),
                reservation,
                counters,
                object_scratch,
                page_scratch,
            )?;
            storage.push_built_directory_v1(BuiltDirectoryRecordV1 {
                physical: directory.physical(),
                entry_count: directory.entry_count(),
            })?;
            let Some(parent_component) = frame.component_in_parent else {
                if frames.is_empty() {
                    return Ok(directory);
                }
                return Err(CoreError::Truncated);
            };
            let DirectoryLogicalIdentityV1::Explicit(logical) = directory.logical() else {
                return Err(CoreError::TypeDomain);
            };
            frames
                .last_mut()
                .ok_or(CoreError::Truncated)?
                .entries
                .push(CanonicalTreeEntryV1::new(
                    parent_component,
                    CanonicalTreeChildV1::Directory {
                        logical,
                        physical: directory.physical(),
                    },
                ));
            continue;
        }
        if cursor > frame_end {
            return Err(CoreError::Truncated);
        }
        let (component_bytes, has_descendants) =
            path_component_at(files[cursor].path(), frame_prefix_len)?;
        let component = ValidatedComponent::new(component_bytes)?;
        let group_end = directory_group_end(files, cursor, frame_end, frame_prefix_len)?;
        frames[frame_index].cursor = group_end;
        if has_descendants {
            let child_prefix = frame_prefix_len
                .checked_add(component_bytes.len())
                .and_then(|value| value.checked_add(1))
                .ok_or(CoreError::IntegerOverflow)?;
            if frames.len() >= MAX_PATH_DEPTH.saturating_add(1) {
                return Err(CoreError::CountCap);
            }
            frames.push(new_manifest_build_frame_v1(
                files,
                cursor,
                group_end,
                child_prefix,
                DirectoryBuildModeV1::Explicit(DEFAULT_EXPLICIT_DIRECTORY_MODE),
                Some(component),
            )?);
        } else {
            if group_end != cursor + 1 {
                return Err(CoreError::Path);
            }
            let ordinal = u32::try_from(cursor).map_err(|_| CoreError::IntegerOverflow)?;
            let file = storage.read_built_file_v1(ordinal)?;
            frames[frame_index].entries.push(CanonicalTreeEntryV1::new(
                component,
                CanonicalTreeChildV1::File {
                    logical: file.logical,
                    physical: file.physical,
                },
            ));
        }
    }
}

/// Build one complete, bounded, canonically ordered candidate root containing
/// zero or more files. All manifest validation and the sole operation-slot
/// reservation happen before the first supplier is invoked.
#[allow(clippy::too_many_arguments)]
pub fn run_create_tree_v1<S, C>(
    mut operation: StorageOperationV1<'_>,
    algorithm: CdcAlgorithmV1,
    files: &mut [TreeFileV1<'_, S>],
    buffers: OperationBuffersV1<'_>,
    control: &mut C,
    counters: &mut OperationCountersV1,
) -> Result<OperationHandoffV1, OperationErrorV1>
where
    S: SourceSupplierV1,
    C: LifecycleControlV1 + ?Sized,
{
    let (
        canonical_len,
        global_seen_capacity,
        maximum_records_u32,
        maximum_source_resident,
        storage_resident,
    ) = operation.run_preparation_free_stage_v1(
        counters,
        control,
        |operation, counters, _control| {
            // The split request/run API must not let an in-crate caller
            // replay a live grant for another kind. The zero-write lease
            // gives every later terminal path a directly observed
            // storage equation before request/manifest inspection.
            operation.require_complete_tree_kind_v1()?;
            operation.declare_empty_storage_envelope_v1()?;
            validate_entry_count(files.len() as u64)?;
            validate_file_mode(DEFAULT_EXPLICIT_DIRECTORY_MODE)?;
            let mut canonical_len = 0_u64;
            let mut maximum_refs_per_version = 0_u64;
            let mut previous = None;
            for file in files.iter() {
                let path = ValidatedPath::new(file.path())?;
                if let Some(left) = previous {
                    require_strictly_increasing_paths(left, path)?;
                    if file.path().len() > left.as_bytes().len()
                        && file.path().starts_with(left.as_bytes())
                        && file.path()[left.as_bytes().len()] == b'/'
                    {
                        return Err(CoreError::Path.into());
                    }
                }
                previous = Some(path);
                validate_file_mode(file.mode())?;
                validate_logical_length(file.declared_len())?;
                canonical_len = canonical_len
                    .checked_add(file.declared_len())
                    .ok_or(CoreError::IntegerOverflow)?;
                validate_logical_length(canonical_len)?;
                let refs = file
                    .declared_len()
                    .checked_add(8_191)
                    .ok_or(CoreError::IntegerOverflow)?
                    / 8_192;
                validate_chunk_refs_per_file(refs)?;
                maximum_refs_per_version = maximum_refs_per_version
                    .checked_add(refs)
                    .ok_or(CoreError::IntegerOverflow)?;
                validate_chunk_refs_per_version(maximum_refs_per_version)?;
            }
            let tree_preflight = preflight_manifest_directory_v1(files, 0, files.len(), 0)?;
            validate_entry_count(tree_preflight.directory_entry_count)?;
            validate_tree_object_count(tree_preflight.tree_object_count)?;
            let maximum_objects = maximum_refs_per_version
                .checked_add(files.len() as u64)
                .and_then(|count| count.checked_add(tree_preflight.tree_object_count))
                .and_then(|count| count.checked_add(1))
                .ok_or(CoreError::IntegerOverflow)?;
            validate_total_object_count(maximum_objects)?;
            let global_seen_capacity = global_seen_capacity_v1(maximum_objects)?;
            let maximum_records = maximum_objects.min(MAX_STORAGE_RECORDS_V1);
            let maximum_records_u32 =
                u32::try_from(maximum_records).map_err(|_| CoreError::IntegerOverflow)?;
            let required_traversal_bytes = closure_traversal_bytes_v1(maximum_objects)?;
            if buffers.traversal_state.len() < required_traversal_bytes
                || buffers.tree_pages.len()
                    < usize::try_from(tree_preflight.maximum_page_summary_count)
                        .map_err(|_| CoreError::IntegerOverflow)?
            {
                return Err(CoreError::ResourceRefused.into());
            }

            operation.declare_storage_envelope_v1(storage_envelope_v1(
                maximum_objects,
                maximum_objects,
                maximum_refs_per_version,
                files.len() as u64,
                tree_preflight.tree_object_count,
                canonical_len,
                global_seen_capacity,
                true,
            )?)?;

            // Query suppliers only after the final conservative envelope
            // is admitted. These callbacks create no preparation state.
            let mut maximum_source_resident = 0_u64;
            for file in files.iter() {
                let supplier = file.supplier_ref().ok_or(CoreError::SourceFailure)?;
                maximum_source_resident =
                    maximum_source_resident.max(supplier.resident_memory_bound_bytes()?);
            }
            let storage_resident = operation.storage_resident_plan_v1(true, maximum_records_u32)?;
            // `files` and its path bytes are caller-owned immutable
            // manifest input. Charge LayerFS-created views and borrowed
            // ports without relabelling caller storage as slot allocation.
            let port_resident = maximum_source_resident
                .checked_add(storage_resident.total_resident_bytes_v1())
                .ok_or(CoreError::IntegerOverflow)?;
            // Manifest entry construction and authenticated closure
            // reconstruction are sequential phases. The manifest vectors,
            // their bounded directory stack, and the page-summary buffer are
            // all gone from active tree-building work before the closure
            // admission stack is created. Charge the exact larger phase peak
            // in their shared traversal component instead of adding both
            // mutually exclusive peaks. Persistent storage adapters remain in
            // `port_resident` and are therefore still charged across phases.
            let tree_build_resident = tree_preflight
                .peak_entry_memory
                .checked_add(manifest_build_stack_resident_bytes_v1()?)
                .and_then(|bytes| {
                    bytes.checked_add(core::mem::size_of_val(buffers.tree_pages) as u64)
                })
                .ok_or(CoreError::IntegerOverflow)?;
            let traversal_phase_resident =
                admission_traversal_resident_bytes_v1()?.max(tree_build_resident);
            let plan = OperationMemoryPlanV1::empty()
                .charge(MemoryComponentV1::SourceWindow, buffers.source.len() as u64)?
                .charge(MemoryComponentV1::CdcRing, buffers.cdc_ring.len() as u64)?
                .charge(
                    MemoryComponentV1::ComparisonWindow,
                    (2 * COMPARISON_WINDOW_BYTES) as u64,
                )?
                .charge(
                    MemoryComponentV1::ObjectScratch,
                    buffers.tree_object.len() as u64,
                )?
                .charge(MemoryComponentV1::PageSummaries, traversal_phase_resident)?
                .charge(
                    MemoryComponentV1::TraversalState,
                    buffers.traversal_state.len() as u64,
                )?
                .charge(
                    MemoryComponentV1::MetadataWindow,
                    port_resident.max(DEFAULT_METADATA_RESERVATION_BYTES),
                )?
                .charge(
                    MemoryComponentV1::HashState,
                    IDENTITY_HASHER_BYTES_V1
                        .checked_mul(2)
                        .ok_or(CoreError::IntegerOverflow)?,
                )?;
            operation.declare_plan_v1(plan)?;
            counters.memory_high_water = counters
                .memory_high_water
                .max(operation.memory_high_water_bytes_v1());
            Ok((
                canonical_len,
                global_seen_capacity,
                maximum_records_u32,
                maximum_source_resident,
                storage_resident,
            ))
        },
    )?;

    run_lifecycle_v1(
        operation,
        LifecyclePlanV1 {
            global_seen_capacity,
            storage_resident,
            require_tree_storage: true,
            maximum_records: maximum_records_u32,
            algorithm,
        },
        buffers,
        control,
        counters,
        move |storage, control_cell, reservation, buffers, counters| {
            let mut reference_spool_bytes = OptionalU64ObservationV1::observed(
                0,
                "direct cumulative chunk-reference spool logical length",
                ObservationScopeV1::Operation,
            );
            let (_, sink) = storage.content_parts_v1();
            sink.begin_closure().map_err(|_| CoreError::SinkRefused)?;
            for file in files.iter_mut() {
                let supplier = file.take_supplier()?;
                let declared_resident = supplier.resident_memory_bound_bytes()?;
                let mut source = supplier.supply()?;
                if source.resident_memory_bound_bytes()? > declared_resident
                    || declared_resident > maximum_source_resident
                {
                    return Err(CoreError::ResourceRefused.into());
                }
                let mut cdc_control = SharedOperationControlV1::new(control_cell);
                let (references, sink) = storage.content_parts_v1();
                let prepared = create_file_borrowed_v1(
                    file.path(),
                    file.mode(),
                    file.declared_len(),
                    &mut source,
                    sink,
                    references,
                    ContentBuffersV1::new(buffers.source, buffers.cdc_ring),
                    &mut cdc_control,
                    reservation,
                    algorithm,
                    counters,
                )?;
                reference_spool_bytes = reference_spool_bytes.checked_add_operation_v1(
                    storage.reference_storage_bytes_v1()?,
                    "direct cumulative chunk-reference spool logical length",
                )?;
                storage.push_built_file_v1(BuiltFileRecordV1 {
                    logical: derive_file_node_v1(file.mode(), prepared.logical_file())?,
                    physical: prepared.physical_file(),
                    logical_len: file.declared_len(),
                    chunk_count: prepared.chunk_count(),
                    extent_count: u8::from(file.declared_len() != 0),
                })?;
            }
            let tree = build_manifest_directory_v1(
                files,
                storage,
                0,
                files.len(),
                0,
                DirectoryBuildModeV1::ImplicitRoot,
                reservation,
                counters,
                buffers.tree_object,
                buffers.tree_pages,
            )?;
            let DirectoryLogicalIdentityV1::ImplicitRoot(logical_root) = tree.logical() else {
                return Err(CoreError::TypeDomain.into());
            };
            let summary = storage.built_version_summary_v1(
                canonical_len,
                counters,
                &mut SharedOperationControlV1::new(control_cell),
            )?;
            let version = storage.write_version_v1(
                derive_version_v1(logical_root),
                tree.physical(),
                summary,
                counters,
            )?;
            let completed = storage.complete_v1(version)?;
            Ok(PreparedCandidateV1::new(
                version,
                tree.physical(),
                completed,
                reference_spool_bytes,
            ))
        },
    )
}

/// Complete root-owned whole-file Replace. The one opaque root capability is
/// acquired before the base root, path, source declaration, policy, or any
/// request bound is inspected, and is borrowed through candidate closure,
/// exact closure fencing, explicit cleanup, and handoff.
#[allow(clippy::too_many_arguments)]
pub fn run_complete_replace_v1<S, C>(
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
pub fn run_complete_update_v1<S, B, E, C>(
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
pub fn run_complete_add_v1<S, T, C>(
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
pub fn run_complete_remove_v1<T, C>(
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
pub fn run_complete_metadata_v1<E, C>(
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
pub fn run_complete_move_v1<T, C>(
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
pub fn complete_cross_directory_move_operation_v1<ST, DT, RT, C>(
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
