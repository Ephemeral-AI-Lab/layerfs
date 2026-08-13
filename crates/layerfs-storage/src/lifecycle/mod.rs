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
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Arc;

    use super::{
        request_create_operation_v1, run_create_tree_v1, run_complete_replace_v1, run_create_v1,
        request_tree_operation_v1, LifecycleControlV1,
        FsCasBoundaryV1, FsCasCleanupTargetV1, FsCasControlV1, FsCasErrorV1,
        FsCasV1, FsOperationKindV1, FsStorageEnvelopeV1,
        OperationBuffersV1, OperationErrorV1,
    };
    use crate::cas::semantic::{
        publication_causes_v1, publication_error_v1, PublicationCauseV1,
        PublicationCleanupTargetV1, PublicationErrorV1,
    };
    use crate::cas::{
        FsCasFailureCauseV1, FsCasFilesystemBoundaryV1, FsCasFilesystemFailureV1,
    };
    use crate::cdc::{CdcAlgorithmV1, CdcControlV1, FastCdcV1, MAXIMUM_CHUNK_BYTES};
    use crate::content::{ContentSourceErrorV1, ContentSourceV1, SourceSupplierV1, TreeFileV1};
    use crate::cow::semantic::with_replacement_evidence_v1;
    use crate::cow::{
        CanonicalTreeChildV1, CanonicalTreeEntryV1, DirectoryBuildModeV1, TreePageSummaryV1,
        MAX_TREE_OBJECT_BYTES, MAX_TREE_PAGE_SUMMARIES,
    };
    use crate::format::ValidatedComponent;
    use crate::identity::{
        derive_file_node_v1, derive_logical_chunk_v1, derive_logical_file_v1,
        derive_physical_chunk_id_v1, derive_physical_file_id_v1, LogicalChunkRefV1,
        LogicalFileIdentityV1, PhysicalFileIdV1, COMPARISON_WINDOW_BYTES,
    };
    use crate::limits::OperationCountersV1;
    use crate::pack::PACK_HEADER_BYTES;
    use crate::profile::ProfileSpecV1;
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
        storage_bytes_committed: u64,
        storage_bytes_retained: u64,
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

        pub const fn storage_bytes_committed(self) -> u64 {
            self.storage_bytes_committed
        }

        pub const fn storage_bytes_retained(self) -> u64 {
            self.storage_bytes_retained
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
        let output = Command::new(
            std::env::current_exe().expect("current integration-test executable"),
        )
        .args(["--exact", selector, "--nocapture"])
        .env(CHILD_ROOT_ENV, &root)
        .env(CHILD_SENTINEL_ENV, "1")
        .output()
        .expect("spawn exact open-existing child");
        let output_lines = [
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        ]
        .into_iter()
        .flat_map(|stream| stream.lines().map(str::to_owned).collect::<Vec<_>>())
        .collect::<Vec<_>>();
        let reports = output_lines
            .iter()
            .filter(|line| line.starts_with("LAYERFS_CHILD_RESULT="))
            .count() as u32;
        let busy_reports = output_lines
            .iter()
            .filter(|line| line.as_str() == "LAYERFS_CHILD_RESULT=Busy")
            .count() as u32;
        drop(owner);
        let _ = fs::remove_dir_all(&root);
        SubprocessObservationV1 {
            child_succeeded: output.status.success(),
            child_reports: reports,
            child_busy_reports: busy_reports,
        }
    }

    pub fn open_existing_subprocess_child_v1() -> Option<OpenExistingObservationV1> {
        let root = std::env::var_os(CHILD_ROOT_ENV)?;
        if std::env::var_os(CHILD_SENTINEL_ENV).as_deref() != Some(std::ffi::OsStr::new("1")) {
            return None;
        }
        let observation = open_existing_v1(Path::new(&root));
        println!("LAYERFS_CHILD_RESULT={observation:?}");
        Some(observation)
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
        filesystem_failure: Option<FilesystemFaultFailureV1>,
        first_cause: Option<PublicationCauseV1>,
        dominant_cause: Option<PublicationCauseV1>,
        panicked: bool,
        panic_payload: Option<&'static str>,
        control_fired: bool,
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
        immutable_bytes: u64,
        immutable_entries: u64,
        residue_bytes: u64,
        mutable_preparation_residue_bytes: u64,
        mutable_preparation_residue_inodes: u64,
        source_read_calls: u64,
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
        storage_active_operations: u64,
        storage_active_bytes: u64,
        storage_active_inodes: u64,
        invalidated: bool,
        stale_invalidated: bool,
        reopen_invalidated: bool,
        zero_forbidden_work: bool,
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
        filesystem_failure: filesystem_failure => Option<FilesystemFaultFailureV1>,
        first_cause: first_cause => Option<PublicationCauseV1>,
        dominant_cause: dominant_cause => Option<PublicationCauseV1>,
        panicked: panicked => bool,
        panic_payload: panic_payload => Option<&'static str>,
        control_fired: control_fired => bool,
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
        immutable_bytes: immutable_bytes => u64,
        immutable_entries: immutable_entries => u64,
        residue_bytes: residue_bytes => u64,
        mutable_preparation_residue_bytes: mutable_preparation_residue_bytes => u64,
        mutable_preparation_residue_inodes: mutable_preparation_residue_inodes => u64,
        source_read_calls: source_read_calls => u64,
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
        storage_active_operations: storage_active_operations => u64,
        storage_active_bytes: storage_active_bytes => u64,
        storage_active_inodes: storage_active_inodes => u64,
        invalidated: invalidated => bool,
        stale_invalidated: stale_invalidated => bool,
        reopen_invalidated: reopen_invalidated => bool,
        zero_forbidden_work: zero_forbidden_work => bool,
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
        let (preparation_bytes, preparation_entries) =
            directory_usage(&root.join("preparation"));
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
            storage_bytes_committed: counters.storage_bytes_committed,
            storage_bytes_retained: counters.storage_bytes_retained,
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
            FilesystemFaultFailureV1::InodeExhaustion => {
                FsCasFilesystemFailureV1::InodeExhaustion
            }
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
                    if self.case != PreparationConstructionCaseV1::PreCreateAccountingReleaseFails =>
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
        let (attempt, followup_succeeded) = if poison_terminal {
            (attempt, false)
        } else {
            let mut followup_counters = OperationCountersV1::default();
            let followup = run_create_fault_attempt(
                &cas,
                0x904,
                1,
                CallbackSupplier {
                    bound_invoked: Arc::clone(&bound_invoked),
                    supply_invoked: Arc::clone(&supply_invoked),
                    len: 1,
                },
                &mut control,
                &mut followup_counters,
            );
            let followup_succeeded = followup.error.is_none() && !followup.panicked;
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
        observe_create_fault_with_control(
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
        )
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
            if boundary == FsCasBoundaryV1::BeforeClosureMarkerPublication
                && !self.closure_panicked
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
        let (first_cause, dominant_cause) = attempt
            .error
            .map(publication_causes_v1)
            .map(|(first, dominant)| (Some(first), Some(dominant)))
            .unwrap_or((None, None));
        let (storage_active_operations, storage_active_bytes, storage_active_inodes) =
            cas.storage_admission_active_for_test_v1();
        CreateFaultObservationV1 {
            error: attempt.error.map(publication_error_v1),
            filesystem_failure: filesystem_failure_v1(attempt.error),
            first_cause,
            dominant_cause,
            panicked: attempt.panicked,
            panic_payload: attempt.panic_payload,
            control_fired: control.control_fired,
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
            immutable_bytes,
            immutable_entries,
            residue_bytes: counters.unreachable_installed_residue_bytes,
            mutable_preparation_residue_bytes: counters.mutable_preparation_residue_bytes,
            mutable_preparation_residue_inodes: counters.mutable_preparation_residue_inodes,
            source_read_calls: counters.source_read_calls,
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
            storage_active_operations,
            storage_active_bytes,
            storage_active_inodes,
            invalidated: matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            stale_invalidated: matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            reopen_invalidated: matches!(
                FsCasV1::open_existing(root),
                Err(FsCasErrorV1::Invalidated)
            ),
            zero_forbidden_work: counters.has_zero_forbidden_work(),
        }
    }

    fn new_fault_root(root: &Path) -> (FsCasV1, FsCasV1) {
        let cas = FsCasV1::create_new(root).expect("create lifecycle fault root");
        let stale = FsCasV1::open_existing(root).expect("open lifecycle fault stale owner");
        (cas, stale)
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
                return Some(FsCasErrorV1::Filesystem(
                    FsCasFilesystemFailureV1::NoSpace,
                ));
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
                    Some(FsCasErrorV1::Filesystem(
                        FsCasFilesystemFailureV1::NoSpace,
                    ))
                }
                FsCasFilesystemBoundaryV1::InvalidationWrite
                    if !self.invalidation_write_failed =>
                {
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
        cleanup_calls: u32,
        cleanup_injected: bool,
        invalidation_attempts: u32,
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
                    self.cleanup_calls += 1;
                    if !self.cleanup_injected {
                        self.cleanup_injected = true;
                        true
                    } else {
                        false
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
        observe_direct_fault(
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
        )
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

    pub fn alias_cleanup_invalidation_double_fault_v1(
        root: &Path,
    ) -> CreateFaultObservationV1 {
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
                control_fired: control.alias_failed
                    && control.invalidation_write_failed
                    && control.invalidation_marker_failed,
                cleanup_calls: 0,
                carrier_installed: false,
                poisoned: false,
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
                control_fired: control.boundary_panicked,
                cleanup_calls: control.alias_cleanup_calls,
                carrier_installed: false,
                poisoned: false,
            },
            &counters,
        )
    }

    pub fn post_link_marker_secondary_v1(
        root: &Path,
        boundary_unwind: bool,
        alias_cleanup: PostLinkAliasCleanupV1,
        fail_invalidation: bool,
    ) -> CreateFaultObservationV1 {
        let (cas, stale) = new_fault_root(root);
        let bound_invoked = Arc::new(AtomicBool::new(false));
        let supply_invoked = Arc::new(AtomicBool::new(false));
        let mut control = PostLinkMarkerCleanupControl {
            target: FsCasBoundaryV1::AfterClosureMarkerLink,
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
                control_fired: control.boundary_panicked,
                cleanup_calls: control.alias_cleanup_calls,
                carrier_installed: false,
                poisoned: false,
            },
            &counters,
        )
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
                cleanup_calls: u32::from(control.cleanup_injected),
                carrier_installed: false,
                poisoned: false,
            },
            &counters,
        )
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
                cleanup_calls: 1,
                carrier_installed: false,
                poisoned: false,
            },
            &counters,
        )
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct MalformedClosureObservationV1 {
        error: Option<PublicationErrorV1>,
        first_cause: Option<PublicationCauseV1>,
        dominant_cause: Option<PublicationCauseV1>,
        malformed_closure_installed: bool,
        closure_bytes: u64,
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

        pub const fn closure_bytes(self) -> u64 {
            self.closure_bytes
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
        assert!(first.error.is_none(), "malformed closure seed create failed");
        let mut closures = fs::read_dir(root.join("closures"))
            .expect("read seeded closures");
        let closure = closures
            .next()
            .expect("seeded closure")
            .expect("seeded closure entry")
            .path();
        assert!(closures.next().is_none(), "seeded create produced extra closures");
        fs::remove_file(&closure).expect("remove seeded closure for race");

        let mut control = MalformedClosureControl {
            destination: closure.clone(),
            malformed_installed: false,
            cleanup_calls: 0,
            cleanup_injected: !fail_cleanup,
            invalidation_attempts: 0,
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
        let (preparation_bytes, preparation_entries) = directory_usage(&root.join("preparation"));
        let (immutable_bytes, immutable_entries) = immutable_usage(root);
        MalformedClosureObservationV1 {
            error: attempt.error.map(publication_error_v1),
            first_cause,
            dominant_cause,
            malformed_closure_installed: control.malformed_installed,
            closure_bytes: fs::metadata(&closure).map(|metadata| metadata.len()).unwrap_or(0),
            cleanup_calls: control.cleanup_calls,
            invalidation_attempts: control.invalidation_attempts,
            preparation_bytes,
            preparation_entries,
            immutable_bytes,
            immutable_entries,
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
                FsCasFilesystemBoundaryV1::MarkerHardLink => {
                    self.marker_link_boundary_seen = true
                }
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
                PreparationUnlinkFaultModeV1::Missing
                | PreparationUnlinkFaultModeV1::Injected => {}
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
        operation_first_cause: Option<PublicationCauseV1>,
        operation_dominant_cause: Option<PublicationCauseV1>,
        cleanup_first_cause: Option<PublicationCauseV1>,
        cleanup_dominant_cause: Option<PublicationCauseV1>,
        logical_length: u64,
        physical_length: Option<u64>,
        accounted_length: u64,
        preparation_bytes: u64,
        preparation_entries: u64,
        storage_bytes_released: u64,
        storage_bytes_committed: u64,
        storage_bytes_retained: u64,
        storage_inodes_released: u64,
        storage_inodes_committed: u64,
        storage_inodes_retained: u64,
        invalidated: bool,
        stale_invalidated: bool,
        reopen_invalidated: bool,
        root_usable: bool,
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
        operation_first_cause: operation_first_cause => Option<PublicationCauseV1>,
        operation_dominant_cause: operation_dominant_cause => Option<PublicationCauseV1>,
        cleanup_first_cause: cleanup_first_cause => Option<PublicationCauseV1>,
        cleanup_dominant_cause: cleanup_dominant_cause => Option<PublicationCauseV1>,
        logical_length: logical_length => u64,
        physical_length: physical_length => Option<u64>,
        accounted_length: accounted_length => u64,
        preparation_bytes: preparation_bytes => u64,
        preparation_entries: preparation_entries => u64,
        storage_bytes_released: storage_bytes_released => u64,
        storage_bytes_committed: storage_bytes_committed => u64,
        storage_bytes_retained: storage_bytes_retained => u64,
        storage_inodes_released: storage_inodes_released => u64,
        storage_inodes_committed: storage_inodes_committed => u64,
        storage_inodes_retained: storage_inodes_retained => u64,
        invalidated: invalidated => bool,
        stale_invalidated: stale_invalidated => bool,
        reopen_invalidated: reopen_invalidated => bool,
        root_usable: root_usable => bool,
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
        logical_length: u64,
        physical_length: Option<u64>,
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
        PackFaultObservationV1 {
            operation_error: operation_error.map(publication_error_v1),
            cleanup_error: cleanup_error.map(publication_error_v1),
            operation_first_cause,
            operation_dominant_cause,
            cleanup_first_cause,
            cleanup_dominant_cause,
            logical_length,
            physical_length,
            accounted_length,
            preparation_bytes,
            preparation_entries,
            storage_bytes_released: counters.storage_bytes_released,
            storage_bytes_committed: counters.storage_bytes_committed,
            storage_bytes_retained: counters.storage_bytes_retained,
            storage_inodes_released: counters.storage_inodes_released,
            storage_inodes_committed: counters.storage_inodes_committed,
            storage_inodes_retained: counters.storage_inodes_retained,
            invalidated: matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            stale_invalidated: matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            reopen_invalidated: matches!(
                FsCasV1::open_existing(root),
                Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy)
            ),
            root_usable: cas.occupied().is_ok(),
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
            .declare_storage_envelope_v1(
                FsStorageEnvelopeV1::new(SPOOL_BYTES, 0, 1, 0).unwrap(),
            )
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
        assert_eq!(
            spool.cleanup_controlled_v1(&mut control).err(),
            cleanup_error,
            "operation-spool accounting cleanup changed on retry"
        );
        let physical_length = regular_file_length(&spool_path);
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
            SPOOL_BYTES,
            physical_length,
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
            .declare_storage_envelope_v1(
                FsStorageEnvelopeV1::new(SPOOL_BYTES, 0, 1, 0).unwrap(),
            )
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
        assert_eq!(
            spool.cleanup_controlled_v1(&mut control).err(),
            cleanup_error,
            "operation-spool metadata cleanup changed on retry"
        );
        let physical_length = regular_file_length(&spool_path);
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
            SPOOL_BYTES,
            physical_length,
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
            .declare_storage_envelope_v1(
                FsStorageEnvelopeV1::new(LOGICAL_BYTES, 0, 1, 0).unwrap(),
            )
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

                    fs::set_permissions(&preparation, fs::Permissions::from_mode(0o700))
                        .unwrap();
                }
                PreparationMetadataFaultModeV1::ReadFailure => {
                    fs::remove_file(&preparation).unwrap();
                    fs::rename(&held_preparation, &preparation).unwrap();
                }
                PreparationMetadataFaultModeV1::WrongType
                | PreparationMetadataFaultModeV1::Missing => {}
            }
        }
        let physical_length = regular_file_length(&spool_path);
        capability
            .finish_terminal_v1(false, &mut counters, &mut setup_control)
            .expect("operation-spool drop metadata terminal");
        observe_pack_fault(
            root,
            &cas,
            &stale,
            None,
            None,
            LOGICAL_BYTES,
            physical_length,
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
            .declare_storage_envelope_v1(
                FsStorageEnvelopeV1::new(SPOOL_BYTES, 0, 1, 0).unwrap(),
            )
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
        assert_eq!(
            spool.cleanup_controlled_v1(&mut control).err(),
            cleanup_error,
            "operation-spool unlink cleanup changed on retry"
        );
        let physical_length = regular_file_length(&spool_path);
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
            SPOOL_BYTES,
            physical_length,
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
            .declare_storage_envelope_v1(
                FsStorageEnvelopeV1::new(PACK_CEILING, 0, 1, 0).unwrap(),
            )
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
            preparation,
            held_preparation,
            mode,
            restored: false,
            fail_invalidation,
        };
        let cleanup_error = private_pack.cleanup_controlled_v1(&mut control).err();
        assert_eq!(
            private_pack.cleanup_controlled_v1(&mut control).err(),
            cleanup_error,
            "private-pack metadata cleanup changed on retry"
        );
        let physical_length = regular_file_length(&pack_path);
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
            PACK_CEILING,
            physical_length,
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
            .declare_storage_envelope_v1(
                FsStorageEnvelopeV1::new(PACK_CEILING, 0, 1, 0).unwrap(),
            )
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

                    fs::set_permissions(&preparation, fs::Permissions::from_mode(0o700))
                        .unwrap();
                }
                PreparationMetadataFaultModeV1::ReadFailure => {
                    fs::remove_file(&preparation).unwrap();
                    fs::rename(&held_preparation, &preparation).unwrap();
                }
                PreparationMetadataFaultModeV1::WrongType
                | PreparationMetadataFaultModeV1::Missing => {}
            }
        }
        let physical_length = regular_file_length(&pack_path);
        capability
            .finish_terminal_v1(false, &mut counters, &mut setup_control)
            .expect("private-pack drop metadata terminal");
        observe_pack_fault(
            root,
            &cas,
            &stale,
            None,
            None,
            PACK_CEILING,
            physical_length,
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
            .declare_storage_envelope_v1(
                FsStorageEnvelopeV1::new(PACK_CEILING, 0, 1, 0).unwrap(),
            )
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
        assert_eq!(
            private_pack.cleanup_controlled_v1(&mut control).err(),
            cleanup_error,
            "private-pack unlink cleanup changed on retry"
        );
        let physical_length = regular_file_length(&pack_path);
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
            PACK_CEILING,
            physical_length,
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
            .declare_storage_envelope_v1(
                FsStorageEnvelopeV1::new(PACK_CEILING, 0, 1, 0).unwrap(),
            )
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
        assert!(operation.is_err(), "private-pack accounting operation succeeded");
        let operation_error = private_pack.take_first_error_typed_v1();
        let (physical_length, accounted_length) = private_pack.direct_lengths_for_test_v1();
        let cleanup_error = private_pack.cleanup_controlled_v1(&mut setup_control).err();
        assert_eq!(
            private_pack.cleanup_controlled_v1(&mut setup_control).err(),
            cleanup_error,
            "private-pack accounting cleanup changed on retry"
        );
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
            physical_length.unwrap_or_default(),
            physical_length,
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
            .declare_storage_envelope_v1(
                FsStorageEnvelopeV1::new(PACK_CEILING, 0, 1, 0).unwrap(),
            )
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
        assert_eq!(
            private_pack.cleanup_controlled_v1(&mut control).err(),
            cleanup_error,
            "private-pack accounting cleanup changed on retry"
        );
        let physical_length = regular_file_length(&pack_path);
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
            PACK_CEILING,
            physical_length,
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
        observe_create_fault_with_control(
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
        )
    }

    /// Scalar custody for the file-backed operation-spool fault owner.  The
    /// spool handle and its control never cross the feature boundary.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct OperationSpoolFaultObservationV1 {
        operation_error: Option<PublicationErrorV1>,
        cleanup_error: Option<PublicationErrorV1>,
        logical_length: u64,
        physical_length: u64,
        bytes_read: u64,
        read_calls: u64,
        bytes_written: u64,
        preparation_bytes: u64,
        preparation_entries: u64,
        storage_bytes_released: u64,
        storage_bytes_committed: u64,
        storage_bytes_retained: u64,
        storage_inodes_released: u64,
        storage_inodes_committed: u64,
        storage_inodes_retained: u64,
        invalidated: bool,
        stale_invalidated: bool,
        reopen_invalidated: bool,
        zero_forbidden_work: bool,
    }

    impl OperationSpoolFaultObservationV1 {
        pub const fn operation_error(self) -> Option<PublicationErrorV1> {
            self.operation_error
        }

        pub const fn cleanup_error(self) -> Option<PublicationErrorV1> {
            self.cleanup_error
        }

        pub const fn logical_length(self) -> u64 {
            self.logical_length
        }

        pub const fn physical_length(self) -> u64 {
            self.physical_length
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

        pub const fn storage_bytes_released(self) -> u64 {
            self.storage_bytes_released
        }

        pub const fn storage_bytes_committed(self) -> u64 {
            self.storage_bytes_committed
        }

        pub const fn storage_bytes_retained(self) -> u64 {
            self.storage_bytes_retained
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

    fn observe_operation_spool_fault(
        root: &Path,
        cas: &FsCasV1,
        stale: &FsCasV1,
        operation_error: Option<FsCasErrorV1>,
        cleanup_error: Option<FsCasErrorV1>,
        logical_length: u64,
        physical_length: u64,
        direct_storage: (u64, u64, u64),
        counters: &OperationCountersV1,
    ) -> OperationSpoolFaultObservationV1 {
        let (preparation_bytes, preparation_entries) = directory_usage(&root.join("preparation"));
        OperationSpoolFaultObservationV1 {
            operation_error: operation_error.map(publication_error_v1),
            cleanup_error: cleanup_error.map(publication_error_v1),
            logical_length,
            physical_length,
            bytes_read: direct_storage.0,
            read_calls: direct_storage.1,
            bytes_written: direct_storage.2,
            preparation_bytes,
            preparation_entries,
            storage_bytes_released: counters.storage_bytes_released,
            storage_bytes_committed: counters.storage_bytes_committed,
            storage_bytes_retained: counters.storage_bytes_retained,
            storage_inodes_released: counters.storage_inodes_released,
            storage_inodes_committed: counters.storage_inodes_committed,
            storage_inodes_retained: counters.storage_inodes_retained,
            invalidated: matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            stale_invalidated: matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            reopen_invalidated: matches!(
                FsCasV1::open_existing(root),
                Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy)
            ),
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
        let cleanup_error = spool.cleanup_controlled_v1(&mut admission_control).err();
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
            logical_length,
            physical_length,
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
        assert!(control.injected, "operation-spool write control did not fire");
        let physical_length = preparation_file_length(root);
        let direct_storage = spool.direct_storage_observation();
        let cleanup_error = spool.cleanup_controlled_v1(&mut admission_control).err();
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
            1,
            physical_length,
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
        assert_eq!(destination, [0x5a], "operation-spool read lost committed byte");
        let direct_storage = spool.direct_storage_observation();
        let physical_length = preparation_file_length(root);
        let cleanup_error = spool.cleanup_controlled_v1(&mut admission_control).err();
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
            1,
            physical_length,
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
        cas.prepare_test_marker_cleanup_mismatch_v1(token)
            .expect("marker cleanup length fixture");
        cas.clear_active_preparation_bytes_for_test_v1();
        let mut control = RestoreMarkerCleanupAccountingControl {
            cas: cas.clone(),
            accounting_restored: false,
            fail_invalidation,
        };
        let cleanup_error = cas
            .cleanup_test_marker_mismatch_borrowed_v1(token, &mut control)
            .err();
        let terminal_error = capability
            .finish_terminal_v1(false, &mut counters, &mut admission_control)
            .err();
        observe_direct_fault(
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
        )
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
        let terminal_error = capability
            .finish_terminal_v1(false, &mut counters, &mut admission_control)
            .err();
        observe_direct_fault(
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
        )
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
        let terminal_error = capability
            .finish_terminal_v1(false, &mut counters, &mut admission_control)
            .err();
        let _ = fs::metadata(&temporary);
        observe_direct_fault(
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
        )
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
        let terminal_error = capability
            .finish_terminal_v1(false, &mut counters, &mut admission_control)
            .err();
        let _ = fs::symlink_metadata(&temporary);
        observe_direct_fault(
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
        )
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
            FilesystemFaultFailureV1::PermissionDenied => FsCasFilesystemFailureV1::PermissionDenied,
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
        let token = capability.storage_token_v1().expect("private-pack create token");
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
            FilesystemFaultFailureV1::PermissionDenied => FsCasFilesystemFailureV1::PermissionDenied,
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
        observe_direct_fault(
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
        )
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
        let token = capability.storage_token_v1().expect("marker immutable token");
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
        observe_direct_fault(
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
        )
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
        let setup_token = setup.storage_token_v1().expect("marker incumbent setup token");
        cas.publish_test_marker_borrowed_v1(setup_token, &mut setup_control)
            .expect("marker incumbent setup publication");
        setup
            .finish_terminal_v1(true, &mut setup_counters, &mut setup_control)
            .expect("marker incumbent setup terminal");

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
        let token = capability.storage_token_v1().expect("marker incumbent token");
        cas.poison_next_immutable_remove_for_test_v1();
        let mut control = DirectInvalidationControl {
            fail: fail_invalidation,
            attempts: 0,
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
                control_fired: true,
                cleanup_calls: 0,
                carrier_installed: false,
                poisoned: true,
            },
            control.attempts,
            &counters,
        )
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
        let token = capability.storage_token_v1().expect("marker hard-link token");
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
        let followup_bound = Arc::new(AtomicBool::new(false));
        let followup_supply = Arc::new(AtomicBool::new(false));
        let mut followup_counters = OperationCountersV1::default();
        let followup = run_create_fault_attempt(
            &cas,
            0x8fe,
            1,
            CallbackSupplier {
                bound_invoked: followup_bound,
                supply_invoked: followup_supply,
                len: 1,
            },
            &mut control,
            &mut followup_counters,
        );
        let followup_succeeded = followup.error.is_none() && !followup.panicked;
        let error = attempt.error.or(followup.error);
        observe_create_fault(
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
        )
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
            root,
            &cas,
            &stale,
            attempt,
            false,
            false,
            false,
            0,
            0,
            false,
            &counters,
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
        observe_create_fault(
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

    pub fn carrier_cleanup_failure_v1(root: &Path) -> CreateFaultObservationV1 {
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
                control_fired: control.carrier_installed,
                cleanup_calls: control.cleanup_calls,
                carrier_installed: control.carrier_installed,
                poisoned: false,
            },
            &counters,
        )
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
                control_fired: control.carrier_installed,
                cleanup_calls: 0,
                carrier_installed: control.carrier_installed,
                poisoned: control.poisoned,
            },
            &counters,
        )
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

    fn expected_file(data: &[u8], mode: u16) -> crate::CoreResult<(LogicalFileIdentityV1, PhysicalFileIdV1)> {
        let mut logical_refs = Vec::new();
        let mut physical_refs = Vec::new();
        let mut offset = 0_usize;
        while offset < data.len() {
            let cut = FastCdcV1::new().cut(&data[offset..])?;
            let payload = &data[offset..offset + cut];
            let logical = derive_logical_chunk_v1(payload)?;
            let physical = derive_physical_chunk_id_v1(&canonical_object(0x05, payload))?;
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
        Ok((logical, physical))
    }

    fn directory_usage(path: &Path) -> (u64, u64) {
        fs::read_dir(path)
            .expect("semantic namespace directory")
            .map(|entry| {
                let entry = entry.expect("semantic namespace entry");
                let metadata = fs::symlink_metadata(entry.path()).expect("semantic namespace metadata");
                assert!(metadata.file_type().is_file());
                (metadata.len(), 1_u64)
            })
            .fold((0, 0), |(bytes, inodes), (length, one)| {
                (bytes + length, inodes + one)
            })
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
            SliceSupplier { bytes: request.base },
        )];
        let mut scratch = OperationScratch::new();
        let mut base_control = ContinueControl;
        let mut base_counters = OperationCountersV1::default();
        let base_operation = request_tree_operation_v1(
            &cas,
            0x515,
            &mut base_counters,
            &mut base_control,
        )
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
        let before_preparation = directory_usage(&request.root.join("preparation"));
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
