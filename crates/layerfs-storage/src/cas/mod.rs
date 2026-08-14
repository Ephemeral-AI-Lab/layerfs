//! Immutable content-addressed storage ports and filesystem implementation.

mod admission;
mod catalog;
mod closure;
#[cfg(feature = "operation-polymorphism")]
mod closure_storage;
mod fs;
mod locator;
#[cfg(feature = "operation-polymorphism")]
mod locator_index;
#[cfg(feature = "operation-polymorphism")]
mod operation_admission;
mod port;

#[cfg(feature = "operation-polymorphism")]
pub(crate) use admission::admission_traversal_resident_bytes_v1;
#[cfg(any(test, feature = "operation-polymorphism"))]
pub use admission::admit_complete_immutable_v1;
pub use admission::AdmissionBuffersV1;
pub use catalog::CATALOG_MARKER_BYTES;
#[cfg(feature = "operation-polymorphism")]
pub(crate) use closure::FsCasClosureSpoolV1;
pub use closure::{
    compare_closure_object_ids_v1, AdmittedClosureV1, CompleteImmutableClosureReadPortV1,
};
#[cfg(feature = "operation-polymorphism")]
pub(crate) use closure_storage::{ClosureObjectRecordV1, FileClosureObjectSpoolV1};
pub(crate) use fs::locator_publication_receipt_preparation_bytes_bound_v1;
pub use fs::FsCasOccupiedV1;
#[cfg(any(test, feature = "operation-polymorphism"))]
pub use fs::{CarrierReceiptTransitionCheckV1, FsCasFailureCauseV1};
pub use fs::{
    FsCasBoundaryV1, FsCasCleanupTargetV1, FsCasControlV1, FsCasErrorV1, FsCasFilesystemBoundaryV1,
    FsCasV1, FsOperationObservedControlV1, FsPackAdmissionOutcomeV1, FsPrivatePackV1,
    CLOSURE_MARKER_BYTES,
};
#[cfg(feature = "operation-polymorphism")]
pub use fs::{
    FsCasFilesystemFailureV1, FsCasResidueAccountingBoundaryV1, FsCasResourceV1,
    ROOT_LOGICAL_STORAGE_BUDGET_V1, ROOT_NAMESPACE_ENTRY_BUDGET_V1,
};
#[cfg(feature = "operation-polymorphism")]
pub(crate) use fs::{
    FsClosureAdmissionErrorV1, FsOperationCapabilityV1, FsOperationSpoolConstructionUnwindV1,
    FsOperationSpoolV1, FsStorageOperationTokenV1,
};
#[cfg(feature = "operation-polymorphism")]
pub use fs::{FsOperationKindV1, FsStorageEnvelopeV1};
pub use locator::PERSISTENT_LOCATOR_BYTES_V1;
#[cfg(all(test, feature = "operation-polymorphism"))]
pub(crate) use locator_index::{global_seen_hash_v1, GLOBAL_SEEN_MAXIMUM_PROBES_PER_LOOKUP_V1};
#[cfg(feature = "operation-polymorphism")]
pub(crate) use locator_index::{
    FileGlobalSeenSpoolV1, GlobalSeenErrorV1, GlobalSeenLookupV1, GlobalSeenRecordV1,
    GLOBAL_SEEN_RECORD_BYTES,
};
#[cfg(feature = "operation-polymorphism")]
pub(crate) use operation_admission::{
    authenticate_base_root_storage_v1, begin_storage_session_v1, complete_closure_fence_storage_v1,
    ClosureFenceStorageOutcomeV1, StorageSessionV1,
};
#[cfg(any(test, feature = "operation-polymorphism"))]
pub use port::{read_complete_immutable_v1, BoundedImmutableReadSinkV1, ClosureObjectV1};
pub use port::{
    ImmutablePortErrorV1, OccupiedImmutableReadPortV1, PreparedImmutableClosurePortV1,
    ValidatedOccupiedObjectV1,
};

#[cfg(feature = "operation-polymorphism")]
pub mod semantic {
    use super::{
        admit_complete_immutable_v1, read_complete_immutable_v1, AdmissionBuffersV1,
        BoundedImmutableReadSinkV1, CompleteImmutableClosureReadPortV1, FsCasBoundaryV1,
        FsCasCleanupTargetV1, FsCasControlV1, FsCasErrorV1, FsCasFailureCauseV1,
        FsCasFilesystemBoundaryV1, FsCasFilesystemFailureV1, FsCasV1, FsOperationObservedControlV1,
        FsPackAdmissionOutcomeV1, FsPrivatePackV1, ImmutablePortErrorV1,
        OccupiedImmutableReadPortV1, PreparedImmutableClosurePortV1, ValidatedOccupiedObjectV1,
        CATALOG_MARKER_BYTES, PERSISTENT_LOCATOR_BYTES_V1,
    };
    use crate::identity::{
        derive_implicit_root_directory_v1, derive_physical_chunk_id_v1, derive_physical_file_id_v1,
        derive_physical_symlink_id_v1, derive_physical_tree_id_v1,
        derive_physical_version_record_id_v1, derive_version_v1, COMPARISON_WINDOW_BYTES,
    };
    use crate::limits::{OperationCountersV1, ResourceLedgerV1};
    use crate::object::{decode_physical_object_v1, DiscardStrongEdgesV1};
    use crate::object::{TypedPhysicalObjectIdV1, OBJECT_HEADER_BYTES};
    use crate::pack::{
        build_dense_pack_v1, PackIndexEntryV1, PackIndexSpoolV1, PackObjectSourceV1,
        PackPortErrorV1, PackReadPortV1, PrivatePackPortV1,
    };
    use crate::profile::{ChunkerSpecV1, DigestSpecV1, ProfileSpecV1};
    use crate::{CoreError, CoreResult};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{mpsc, Arc, Condvar, Mutex};
    use std::time::Duration;

    /// A bounded filesystem-publication request. The request carries only the
    /// caller-owned canonical object bytes and a root path; no CAS handle,
    /// pack, ledger, or control object crosses the semantic boundary.
    #[derive(Clone, Copy)]
    pub struct PublicationRequestV1<'a> {
        root: &'a Path,
        objects: &'a [&'a [u8]],
        cancel_after_locator_publication: u32,
    }

    impl<'a> PublicationRequestV1<'a> {
        pub const fn new(root: &'a Path, objects: &'a [&'a [u8]]) -> Self {
            Self {
                root,
                objects,
                cancel_after_locator_publication: 1,
            }
        }

        pub const fn with_cancel_after_locator_publication(mut self, count: u32) -> Self {
            self.cancel_after_locator_publication = count;
            self
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum PublicationErrorV1 {
        Core(CoreError),
        Busy,
        Invalidated,
        Unsupported,
        SynchronizationPoisoned,
        CrossOwner,
        WrongOperationKind,
        Integrity,
        Collision,
        MalformedOccupant,
        MissingOccupant,
        UnequalOccupant,
        Io,
        ResourceRefused,
        Filesystem,
        CleanupFailed,
        InvalidationFailed,
        TerminalFailure,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum PublicationCleanupTargetV1 {
        RootInitialization,
        ObjectLocator,
        Carrier,
        PrivatePack,
        PreparationSpool,
        PublishedMarkerAlias,
        RootInvalidation,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum PublicationCauseV1 {
        Core(CoreError),
        Busy,
        Invalidated,
        Unsupported,
        SynchronizationPoisoned,
        CrossOwner,
        WrongOperationKind,
        Integrity,
        Collision,
        MalformedOccupant,
        MissingOccupant,
        UnequalOccupant,
        Io,
        ResourceRefused,
        Filesystem,
        CleanupFailed(PublicationCleanupTargetV1),
        InvalidationFailed,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct PublicationObservationV1 {
        error: Option<PublicationErrorV1>,
        directories_observed: bool,
        preparation_entries: u64,
        carrier_entries: u64,
        object_entries: u64,
        catalog_entries: u64,
        residue_bytes: u64,
        bytes_written: u64,
        admitted_slots: u64,
        zero_forbidden_work: bool,
        locator_publications: u32,
        source_payload_bytes_read: u64,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum PublicationOutcomeV1 {
        Installed,
        ExistingComplete,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct OverlappingPackObservationV1 {
        pub outcomes: [PublicationOutcomeV1; 2],
        pub shared_locator_canonical: bool,
        pub object_entries: u64,
        pub occupied_lengths_match: bool,
        pub occupied_bytes_match: bool,
        pub closure_admitted: bool,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct PublicationLockScopeObservationV1 {
        pub outcome: PublicationOutcomeV1,
        pub observed: bool,
        pub visibility_available: bool,
        pub publication_available: bool,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct SharedObjectCancellationObservationV1 {
        pub error: Option<PublicationErrorV1>,
        pub preparation_entries: u64,
        pub carrier_entries: u64,
        pub catalog_entries: u64,
        pub object_entries: u64,
        pub winner_locator_present: bool,
        pub unreachable_residue_bytes: u64,
        pub admitted_slots: u64,
        pub zero_forbidden_work: bool,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ReopenedPublicationObservationV1 {
        pub outcomes: [PublicationOutcomeV1; 2],
        pub bytes_written: [u64; 2],
        pub zero_forbidden_work: [bool; 2],
        pub shared_id_matches: bool,
        pub carrier_entries: u64,
        pub catalog_entries: u64,
        pub object_entries: u64,
        pub occupied_lengths_match: bool,
        pub admitted_slots: u64,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct LocatorOwnerWaitObservationV1 {
        pub locator_wait_observed: bool,
        pub completed_before_owner: bool,
        pub first_blocked: bool,
        pub control_observations_clean: [bool; 2],
        pub outcomes: [PublicationOutcomeV1; 2],
        pub publication_lock_acquisitions: [u64; 2],
        pub active_publication_wait_polls: u64,
        pub active_publication_wait_nanoseconds: u64,
        pub locator_owner_wait_polls: u64,
        pub locator_owner_wait_nanoseconds: u64,
        pub zero_forbidden_work: [bool; 2],
        pub admitted_slots: u64,
        pub carrier_entries: u64,
        pub catalog_entries: u64,
        pub object_entries: u64,
        pub occupied_lengths_match: bool,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct PreparationLockScopeObservationV1 {
        pub visibility_available_while_blocked: bool,
        pub publication_available_while_blocked: bool,
        pub boundary_blocked: bool,
        pub preparation_entries: u64,
        pub visibility_available_after_cleanup: bool,
        pub publication_available_after_cleanup: bool,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct DisjointPublicationObservationV1 {
        pub first_blocked: bool,
        pub outcomes: [PublicationOutcomeV1; 2],
        pub second_completed_before_release: bool,
        pub carrier_entries: u64,
        pub catalog_entries: u64,
        pub admitted_slots: u64,
        pub zero_forbidden_work: [bool; 2],
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct SamePackRaceObservationV1 {
        pub pack_len: u64,
        pub comparison_observed: bool,
        pub visibility_available: bool,
        pub publication_available: bool,
        pub outcome: PublicationOutcomeV1,
        pub incumbent_comparison_bytes: u64,
        pub incumbent_comparison_windows: u64,
        pub carrier_entries: u64,
        pub catalog_entries: u64,
        pub preparation_entries: u64,
        pub carrier_identity_preserved: bool,
        pub zero_forbidden_work: bool,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum ConcurrentIncumbentFailureV1 {
        UnequalCompleteBytes,
        Malformed,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ConcurrentIncumbentCaseObservationV1 {
        pub failure: ConcurrentIncumbentFailureV1,
        pub failure_error: Option<PublicationErrorV1>,
        pub failure_control_clean: bool,
        pub success_outcome: PublicationOutcomeV1,
        pub success_control_clean: bool,
        pub storage_equations_hold: [bool; 2],
        pub zero_forbidden_work: [bool; 2],
        pub unreachable_residue_bytes: [u64; 2],
        pub publication_lock_acquisitions: [u64; 2],
        pub publication_lock_hold_nanoseconds: [u64; 2],
        pub success_visibility_lock_acquisitions: u64,
        pub success_visibility_lock_hold_nanoseconds: u64,
        pub incumbent_carrier_preserved: bool,
        pub incumbent_locator_preserved: bool,
        pub incumbent_carrier_identity_preserved: bool,
        pub preparation_entries: u64,
        pub carrier_entries: u64,
        pub catalog_entries: u64,
        pub object_entries: u64,
        pub admitted_slots: u64,
        pub disjoint_object_length_matches: bool,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ConcurrentIncumbentObservationV1 {
        pub seed_installed: bool,
        pub cases: [ConcurrentIncumbentCaseObservationV1; 2],
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct PackTransferObservationV1 {
        fixed_handles_within_budget: bool,
        installed: bool,
        pack_len: u64,
        preparation_entries: u64,
        carrier_entries: u64,
        catalog_entries: u64,
        bytes_written: u64,
        bytes_read: u64,
        read_calls: u64,
        catalog_operations: u64,
        installed_carrier_logical_bytes: u64,
        zero_forbidden_work: bool,
        admitted_slots: u64,
        reopened_lengths_match: bool,
        reopened_bytes_match: bool,
        reopened_read_calls: u64,
        reopened_bytes_read: u64,
        expected_object_bytes: u64,
        source_payload_bytes_read: u64,
        expected_source_payload_bytes: u64,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum ExistingCatalogCaseV1 {
        BindingMismatch,
        SameIdUnequal,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum AtomicCatalogCaseV1 {
        BindingMismatch,
        SameIdUnequal,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct NamespaceCreationObservationV1 {
        error: Option<PublicationErrorV1>,
        namespace_absent: bool,
    }

    impl NamespaceCreationObservationV1 {
        pub const fn error(self) -> Option<PublicationErrorV1> {
            self.error
        }
        pub const fn namespace_absent(self) -> bool {
            self.namespace_absent
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ExistingCatalogObservationV1 {
        incumbent_installed: bool,
        error: Option<PublicationErrorV1>,
        preparation_entries: u64,
        carrier_entries: u64,
        object_entries: u64,
        catalog_entries: u64,
        marker_preserved: bool,
        unreachable_installed_residue_bytes: u64,
        admitted_slots: u64,
        zero_forbidden_work: bool,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum ClosureCapabilityFailureCaseV1 {
        NonexistentObjects,
        SpoofedBytes,
        DuplicateTypedIds,
        WrongVersionRecord,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ClosureCapabilityFailureObservationV1 {
        error: Option<CoreError>,
        closure_entries: u64,
        closure_fences: u64,
        fscas_bytes_read: u64,
        fscas_read_calls: u64,
        admitted_slots: u64,
        zero_forbidden_work: bool,
    }

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct LocatorBindingCaseObservationV1 {
        read_error: Option<PublicationErrorV1>,
        admission_error: Option<PublicationErrorV1>,
        admitted_slots: u64,
        zero_forbidden_work: bool,
    }

    impl LocatorBindingCaseObservationV1 {
        pub const fn read_error(self) -> Option<PublicationErrorV1> {
            self.read_error
        }
        pub const fn admission_error(self) -> Option<PublicationErrorV1> {
            self.admission_error
        }
        pub const fn admitted_slots(self) -> u64 {
            self.admitted_slots
        }
        pub const fn zero_forbidden_work(self) -> bool {
            self.zero_forbidden_work
        }
    }

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct LocatorBindingObservationV1 {
        binding_cases: [LocatorBindingCaseObservationV1; 2],
        reuse_error: Option<PublicationErrorV1>,
        reuse_admitted_slots: u64,
        reuse_zero_forbidden_work: bool,
    }

    impl LocatorBindingObservationV1 {
        pub const fn binding_cases(&self) -> &[LocatorBindingCaseObservationV1; 2] {
            &self.binding_cases
        }
        pub const fn reuse_error(self) -> Option<PublicationErrorV1> {
            self.reuse_error
        }
        pub const fn reuse_admitted_slots(self) -> u64 {
            self.reuse_admitted_slots
        }
        pub const fn reuse_zero_forbidden_work(self) -> bool {
            self.reuse_zero_forbidden_work
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct CompleteClosureObservationV1 {
        installed: bool,
        pack_len: u64,
        invisible_before_validation: bool,
        version_record_matches: bool,
        object_count: u64,
        created_count: u64,
        reused_count: u64,
        capability_version_matches: bool,
        capability_object_count: u64,
        closure_entries: u64,
        closure_fences: u64,
        bytes_read: u64,
        fscas_bytes_read_delta: u64,
        fscas_read_calls_delta: u64,
        admitted_slots: u64,
        zero_forbidden_work: bool,
    }

    impl CompleteClosureObservationV1 {
        pub const fn installed(self) -> bool {
            self.installed
        }
        pub const fn pack_len(self) -> u64 {
            self.pack_len
        }
        pub const fn invisible_before_validation(self) -> bool {
            self.invisible_before_validation
        }
        pub const fn version_record_matches(self) -> bool {
            self.version_record_matches
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
        pub const fn capability_version_matches(self) -> bool {
            self.capability_version_matches
        }
        pub const fn capability_object_count(self) -> u64 {
            self.capability_object_count
        }
        pub const fn closure_entries(self) -> u64 {
            self.closure_entries
        }
        pub const fn closure_fences(self) -> u64 {
            self.closure_fences
        }
        pub const fn bytes_read(self) -> u64 {
            self.bytes_read
        }
        pub const fn fscas_bytes_read_delta(self) -> u64 {
            self.fscas_bytes_read_delta
        }
        pub const fn fscas_read_calls_delta(self) -> u64 {
            self.fscas_read_calls_delta
        }
        pub const fn admitted_slots(self) -> u64 {
            self.admitted_slots
        }
        pub const fn zero_forbidden_work(self) -> bool {
            self.zero_forbidden_work
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ClosureBindingObservationV1 {
        cross_fscas_error: Option<PublicationErrorV1>,
        cross_operation_error: Option<PublicationErrorV1>,
        replay_error: Option<PublicationErrorV1>,
        primary_closure_entries: u64,
        other_closure_entries: u64,
        closure_fences: u64,
        invalidation_terminal_failure: bool,
        invalidated_version_record: bool,
        invalidated_object_count: bool,
        invalidated_handoff: bool,
        admitted_slots: u64,
        zero_forbidden_work: bool,
    }

    impl ClosureBindingObservationV1 {
        pub const fn cross_fscas_error(self) -> Option<PublicationErrorV1> {
            self.cross_fscas_error
        }
        pub const fn cross_operation_error(self) -> Option<PublicationErrorV1> {
            self.cross_operation_error
        }
        pub const fn replay_error(self) -> Option<PublicationErrorV1> {
            self.replay_error
        }
        pub const fn primary_closure_entries(self) -> u64 {
            self.primary_closure_entries
        }
        pub const fn other_closure_entries(self) -> u64 {
            self.other_closure_entries
        }
        pub const fn closure_fences(self) -> u64 {
            self.closure_fences
        }
        pub const fn invalidation_terminal_failure(self) -> bool {
            self.invalidation_terminal_failure
        }
        pub const fn invalidated_version_record(self) -> bool {
            self.invalidated_version_record
        }
        pub const fn invalidated_object_count(self) -> bool {
            self.invalidated_object_count
        }
        pub const fn invalidated_handoff(self) -> bool {
            self.invalidated_handoff
        }
        pub const fn admitted_slots(self) -> u64 {
            self.admitted_slots
        }
        pub const fn zero_forbidden_work(self) -> bool {
            self.zero_forbidden_work
        }
    }

    impl ClosureCapabilityFailureObservationV1 {
        pub const fn error(self) -> Option<CoreError> {
            self.error
        }

        pub const fn closure_entries(self) -> u64 {
            self.closure_entries
        }

        pub const fn closure_fences(self) -> u64 {
            self.closure_fences
        }

        pub const fn fscas_bytes_read(self) -> u64 {
            self.fscas_bytes_read
        }

        pub const fn fscas_read_calls(self) -> u64 {
            self.fscas_read_calls
        }

        pub const fn admitted_slots(self) -> u64 {
            self.admitted_slots
        }

        pub const fn zero_forbidden_work(self) -> bool {
            self.zero_forbidden_work
        }
    }

    impl ExistingCatalogObservationV1 {
        pub const fn incumbent_installed(self) -> bool {
            self.incumbent_installed
        }

        pub const fn error(self) -> Option<PublicationErrorV1> {
            self.error
        }

        pub const fn preparation_entries(self) -> u64 {
            self.preparation_entries
        }

        pub const fn carrier_entries(self) -> u64 {
            self.carrier_entries
        }

        pub const fn object_entries(self) -> u64 {
            self.object_entries
        }

        pub const fn catalog_entries(self) -> u64 {
            self.catalog_entries
        }

        pub const fn marker_preserved(self) -> bool {
            self.marker_preserved
        }

        pub const fn unreachable_installed_residue_bytes(self) -> u64 {
            self.unreachable_installed_residue_bytes
        }

        pub const fn admitted_slots(self) -> u64 {
            self.admitted_slots
        }

        pub const fn zero_forbidden_work(self) -> bool {
            self.zero_forbidden_work
        }
    }

    impl PackTransferObservationV1 {
        pub const fn fixed_handles_within_budget(self) -> bool {
            self.fixed_handles_within_budget
        }
        pub const fn installed(self) -> bool {
            self.installed
        }
        pub const fn pack_len(self) -> u64 {
            self.pack_len
        }
        pub const fn preparation_entries(self) -> u64 {
            self.preparation_entries
        }
        pub const fn carrier_entries(self) -> u64 {
            self.carrier_entries
        }
        pub const fn catalog_entries(self) -> u64 {
            self.catalog_entries
        }
        pub const fn bytes_written(self) -> u64 {
            self.bytes_written
        }
        pub const fn bytes_read(self) -> u64 {
            self.bytes_read
        }
        pub const fn read_calls(self) -> u64 {
            self.read_calls
        }
        pub const fn catalog_operations(self) -> u64 {
            self.catalog_operations
        }
        pub const fn installed_carrier_logical_bytes(self) -> u64 {
            self.installed_carrier_logical_bytes
        }
        pub const fn zero_forbidden_work(self) -> bool {
            self.zero_forbidden_work
        }
        pub const fn admitted_slots(self) -> u64 {
            self.admitted_slots
        }
        pub const fn reopened_lengths_match(self) -> bool {
            self.reopened_lengths_match
        }
        pub const fn reopened_bytes_match(self) -> bool {
            self.reopened_bytes_match
        }
        pub const fn reopened_read_calls(self) -> u64 {
            self.reopened_read_calls
        }
        pub const fn reopened_bytes_read(self) -> u64 {
            self.reopened_bytes_read
        }
        pub const fn expected_object_bytes(self) -> u64 {
            self.expected_object_bytes
        }
        pub const fn source_payload_bytes_read(self) -> u64 {
            self.source_payload_bytes_read
        }
        pub const fn expected_source_payload_bytes(self) -> u64 {
            self.expected_source_payload_bytes
        }
    }

    /// Canonical bytes and bounded resource declarations for one immutable
    /// admission. Expected bytes may differ from source bytes only when a
    /// caller is qualifying identity rejection of a mutated source object.
    #[derive(Clone, Copy)]
    pub struct AdmissionRequestV1<'a> {
        objects: &'a [&'a [u8]],
        expected_objects: Option<&'a [&'a [u8]]>,
        version_ordinal: usize,
        occupied_ordinal: Option<usize>,
        occupied_bytes: Option<&'a [u8]>,
        source_resident_bytes: u64,
        occupied_resident_bytes: u64,
        sink_resident_bytes: u64,
        ledger_budget_bytes: u64,
    }

    impl<'a> AdmissionRequestV1<'a> {
        pub const fn new(objects: &'a [&'a [u8]]) -> Self {
            Self {
                objects,
                expected_objects: None,
                version_ordinal: 0,
                occupied_ordinal: None,
                occupied_bytes: None,
                source_resident_bytes: 0,
                occupied_resident_bytes: 0,
                sink_resident_bytes: 0,
                ledger_budget_bytes: 32 * 1024 * 1024,
            }
        }

        pub const fn with_expected_objects(mut self, objects: &'a [&'a [u8]]) -> Self {
            self.expected_objects = Some(objects);
            self
        }

        pub const fn with_version_ordinal(mut self, ordinal: usize) -> Self {
            self.version_ordinal = ordinal;
            self
        }

        pub const fn with_occupied(mut self, ordinal: usize, bytes: &'a [u8]) -> Self {
            self.occupied_ordinal = Some(ordinal);
            self.occupied_bytes = Some(bytes);
            self
        }

        pub const fn with_source_residency(mut self, bytes: u64) -> Self {
            self.source_resident_bytes = bytes;
            self
        }

        pub const fn with_occupied_residency(mut self, bytes: u64) -> Self {
            self.occupied_resident_bytes = bytes;
            self
        }

        pub const fn with_sink_residency(mut self, bytes: u64) -> Self {
            self.sink_resident_bytes = bytes;
            self
        }

        pub const fn with_ledger_budget(mut self, bytes: u64) -> Self {
            self.ledger_budget_bytes = bytes;
            self
        }
    }

    /// Immutable admission outcome facts; no storage engine authority or
    /// concrete port identity crosses the qualification boundary.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct AdmissionObservationV1 {
        error: Option<CoreError>,
        object_count: u64,
        created_count: u64,
        reused_count: u64,
        sink_begun: u64,
        staged_count: u64,
        staged_in_source_order: bool,
        reused_occupied: bool,
        visible_expected: bool,
        sink_aborts: u64,
        admitted_slots: u64,
        physical_objects_created: u64,
        physical_objects_reused: u64,
        closure_objects_missing: u64,
        closure_objects_occupied_validated: u64,
        publication_authority_dispatches: u64,
        bytes_read: u64,
        bytes_copied: u64,
        bytes_written: u64,
        memory_high_water: u64,
        planned_high_water: u64,
        source_count_calls: u64,
        source_reads: u64,
        source_maximum_read: u64,
        occupied_lookups: u64,
        occupied_reads: u64,
        occupied_maximum_read: u64,
    }

    impl AdmissionObservationV1 {
        pub const fn error(&self) -> Option<CoreError> {
            self.error
        }
        pub const fn object_count(&self) -> u64 {
            self.object_count
        }
        pub const fn created_count(&self) -> u64 {
            self.created_count
        }
        pub const fn reused_count(&self) -> u64 {
            self.reused_count
        }
        pub const fn sink_begun(&self) -> u64 {
            self.sink_begun
        }
        pub const fn staged_count(&self) -> u64 {
            self.staged_count
        }
        pub const fn staged_in_source_order(&self) -> bool {
            self.staged_in_source_order
        }
        pub const fn reused_occupied(&self) -> bool {
            self.reused_occupied
        }
        pub const fn visible_expected(&self) -> bool {
            self.visible_expected
        }
        pub const fn sink_aborts(&self) -> u64 {
            self.sink_aborts
        }
        pub const fn admitted_slots(&self) -> u64 {
            self.admitted_slots
        }
        pub const fn physical_objects_created(&self) -> u64 {
            self.physical_objects_created
        }
        pub const fn physical_objects_reused(&self) -> u64 {
            self.physical_objects_reused
        }
        pub const fn closure_objects_missing(&self) -> u64 {
            self.closure_objects_missing
        }
        pub const fn closure_objects_occupied_validated(&self) -> u64 {
            self.closure_objects_occupied_validated
        }
        pub const fn publication_authority_dispatches(&self) -> u64 {
            self.publication_authority_dispatches
        }
        pub const fn bytes_read(&self) -> u64 {
            self.bytes_read
        }
        pub const fn bytes_copied(&self) -> u64 {
            self.bytes_copied
        }
        pub const fn bytes_written(&self) -> u64 {
            self.bytes_written
        }
        pub const fn memory_high_water(&self) -> u64 {
            self.memory_high_water
        }
        pub const fn planned_high_water(&self) -> u64 {
            self.planned_high_water
        }
        pub const fn source_count_calls(&self) -> u64 {
            self.source_count_calls
        }
        pub const fn source_reads(&self) -> u64 {
            self.source_reads
        }
        pub const fn source_maximum_read(&self) -> u64 {
            self.source_maximum_read
        }
        pub const fn occupied_lookups(&self) -> u64 {
            self.occupied_lookups
        }
        pub const fn occupied_reads(&self) -> u64 {
            self.occupied_reads
        }
        pub const fn occupied_maximum_read(&self) -> u64 {
            self.occupied_maximum_read
        }
    }

    /// The immutable facts retained by the three CAS fault-boundary owners.
    /// This is deliberately smaller than the filesystem engine: callers can
    /// assert typed failure, cleanup, incumbent custody, and counter
    /// invariants without receiving a CAS handle or a control object.
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub enum IncumbentIdentityObservationV1 {
        Preserved,
        Changed,
        #[default]
        Unavailable,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct FaultObservationV1 {
        error: Option<PublicationErrorV1>,
        preparation_entries: u64,
        carrier_entries: u64,
        object_entries: u64,
        catalog_entries: u64,
        closure_entries: u64,
        residue_bytes: u64,
        bytes_written: u64,
        admitted_slots: u64,
        catalog_operations: u64,
        closure_fences: u64,
        zero_forbidden_work: bool,
        source_bytes_read: u64,
        incumbent_comparison_bytes: u64,
        incumbent_comparison_windows: u64,
        incumbent_preserved: bool,
        incumbent_identity: IncumbentIdentityObservationV1,
        fault_injected: bool,
        cleanup_fault_injected: bool,
        storage_bytes_request_matches_reservation: bool,
        storage_inodes_request_matches_reservation: bool,
        storage_bytes_terminal_sum_matches_reservation: bool,
        storage_inodes_terminal_sum_matches_reservation: bool,
        storage_bytes_retained: u64,
        storage_inodes_retained: u64,
        loser_locator_absent: bool,
        owner_handle_invalidated: bool,
        owner_private_invalidated: bool,
        owner_occupied_invalidated: bool,
        owner_closure_invalidated: bool,
        stale_handle_invalidated: bool,
        stale_private_invalidated: bool,
        stale_occupied_invalidated: bool,
        stale_closure_refused: bool,
        reopen_invalidated: bool,
        donor_installed: bool,
        incumbent_marker_has_canonical_size: bool,
        incumbent_pack_len: u64,
        candidate_pack_len: u64,
        expected_residue_bytes: u64,
        closure_payload_len: u64,
        filesystem_write_failure: bool,
        catalog_path_is_file: bool,
        carrier_path_is_file: bool,
        malformed_locator_preserved: bool,
        invalidated: bool,
        first_cause: Option<PublicationCauseV1>,
        dominant_cause: Option<PublicationCauseV1>,
    }

    impl FaultObservationV1 {
        pub const fn error(&self) -> Option<PublicationErrorV1> {
            self.error
        }

        pub const fn preparation_entries(&self) -> u64 {
            self.preparation_entries
        }

        pub const fn carrier_entries(&self) -> u64 {
            self.carrier_entries
        }

        pub const fn object_entries(&self) -> u64 {
            self.object_entries
        }

        pub const fn catalog_entries(&self) -> u64 {
            self.catalog_entries
        }

        pub const fn closure_entries(&self) -> u64 {
            self.closure_entries
        }

        pub const fn residue_bytes(&self) -> u64 {
            self.residue_bytes
        }

        pub const fn bytes_written(&self) -> u64 {
            self.bytes_written
        }

        pub const fn admitted_slots(&self) -> u64 {
            self.admitted_slots
        }

        pub const fn catalog_operations(&self) -> u64 {
            self.catalog_operations
        }

        pub const fn closure_fences(&self) -> u64 {
            self.closure_fences
        }

        pub const fn zero_forbidden_work(&self) -> bool {
            self.zero_forbidden_work
        }

        pub const fn source_bytes_read(&self) -> u64 {
            self.source_bytes_read
        }

        pub const fn incumbent_comparison_bytes(&self) -> u64 {
            self.incumbent_comparison_bytes
        }

        pub const fn incumbent_comparison_windows(&self) -> u64 {
            self.incumbent_comparison_windows
        }

        pub const fn incumbent_preserved(&self) -> bool {
            self.incumbent_preserved
        }

        pub const fn incumbent_identity(&self) -> IncumbentIdentityObservationV1 {
            self.incumbent_identity
        }

        pub const fn fault_injected(&self) -> bool {
            self.fault_injected
        }

        pub const fn cleanup_fault_injected(&self) -> bool {
            self.cleanup_fault_injected
        }

        pub const fn storage_bytes_request_matches_reservation(&self) -> bool {
            self.storage_bytes_request_matches_reservation
        }

        pub const fn storage_inodes_request_matches_reservation(&self) -> bool {
            self.storage_inodes_request_matches_reservation
        }

        pub const fn storage_bytes_terminal_sum_matches_reservation(&self) -> bool {
            self.storage_bytes_terminal_sum_matches_reservation
        }

        pub const fn storage_inodes_terminal_sum_matches_reservation(&self) -> bool {
            self.storage_inodes_terminal_sum_matches_reservation
        }

        pub const fn storage_bytes_retained(&self) -> u64 {
            self.storage_bytes_retained
        }

        pub const fn storage_inodes_retained(&self) -> u64 {
            self.storage_inodes_retained
        }

        pub const fn loser_locator_absent(&self) -> bool {
            self.loser_locator_absent
        }

        pub const fn owner_handle_invalidated(&self) -> bool {
            self.owner_handle_invalidated
        }

        pub const fn owner_private_invalidated(&self) -> bool {
            self.owner_private_invalidated
        }

        pub const fn owner_occupied_invalidated(&self) -> bool {
            self.owner_occupied_invalidated
        }

        pub const fn owner_closure_invalidated(&self) -> bool {
            self.owner_closure_invalidated
        }

        pub const fn stale_handle_invalidated(&self) -> bool {
            self.stale_handle_invalidated
        }

        pub const fn stale_private_invalidated(&self) -> bool {
            self.stale_private_invalidated
        }

        pub const fn stale_occupied_invalidated(&self) -> bool {
            self.stale_occupied_invalidated
        }

        pub const fn stale_closure_refused(&self) -> bool {
            self.stale_closure_refused
        }

        pub const fn reopen_invalidated(&self) -> bool {
            self.reopen_invalidated
        }

        pub const fn donor_installed(&self) -> bool {
            self.donor_installed
        }

        pub const fn incumbent_marker_has_canonical_size(&self) -> bool {
            self.incumbent_marker_has_canonical_size
        }

        pub const fn incumbent_pack_len(&self) -> u64 {
            self.incumbent_pack_len
        }

        pub const fn candidate_pack_len(&self) -> u64 {
            self.candidate_pack_len
        }

        pub const fn expected_residue_bytes(&self) -> u64 {
            self.expected_residue_bytes
        }

        pub const fn closure_payload_len(&self) -> u64 {
            self.closure_payload_len
        }

        pub const fn filesystem_write_failure(&self) -> bool {
            self.filesystem_write_failure
        }

        pub const fn catalog_path_is_file(&self) -> bool {
            self.catalog_path_is_file
        }

        pub const fn carrier_path_is_file(&self) -> bool {
            self.carrier_path_is_file
        }

        pub const fn malformed_locator_preserved(&self) -> bool {
            self.malformed_locator_preserved
        }

        pub const fn invalidated(&self) -> bool {
            self.invalidated
        }

        pub const fn first_cause(&self) -> Option<PublicationCauseV1> {
            self.first_cause
        }

        pub const fn dominant_cause(&self) -> Option<PublicationCauseV1> {
            self.dominant_cause
        }
    }

    /// Aggregate immutable facts for the explicit incumbent-read fault
    /// matrices. Each public operation below owns a fixed boundary set; the
    /// facts let the integration owner assert every typed pair without
    /// receiving the private filesystem control or CAS handle.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ReadFaultObservationV1 {
        cases: u32,
        all_injected: bool,
        all_errors_expected: bool,
        missing_occupant_cases: u32,
        permission_denied_cases: u32,
        read_failure_cases: u32,
        short_read_cases: u32,
        all_preparations_clean: bool,
        all_carriers_preserved: bool,
        all_catalogs_preserved: bool,
        all_objects_cleaned: bool,
        all_residue_free: bool,
        all_slots_released: bool,
        all_incumbents_usable: bool,
        all_forbidden_work_zero: bool,
    }

    impl ReadFaultObservationV1 {
        pub const fn cases(&self) -> u32 {
            self.cases
        }

        pub const fn all_injected(&self) -> bool {
            self.all_injected
        }

        pub const fn all_errors_expected(&self) -> bool {
            self.all_errors_expected
        }

        pub const fn missing_occupant_cases(&self) -> u32 {
            self.missing_occupant_cases
        }

        pub const fn permission_denied_cases(&self) -> u32 {
            self.permission_denied_cases
        }

        pub const fn read_failure_cases(&self) -> u32 {
            self.read_failure_cases
        }

        pub const fn short_read_cases(&self) -> u32 {
            self.short_read_cases
        }

        pub const fn all_preparations_clean(&self) -> bool {
            self.all_preparations_clean
        }

        pub const fn all_carriers_preserved(&self) -> bool {
            self.all_carriers_preserved
        }

        pub const fn all_catalogs_preserved(&self) -> bool {
            self.all_catalogs_preserved
        }

        pub const fn all_objects_cleaned(&self) -> bool {
            self.all_objects_cleaned
        }

        pub const fn all_residue_free(&self) -> bool {
            self.all_residue_free
        }

        pub const fn all_slots_released(&self) -> bool {
            self.all_slots_released
        }

        pub const fn all_incumbents_usable(&self) -> bool {
            self.all_incumbents_usable
        }

        pub const fn all_forbidden_work_zero(&self) -> bool {
            self.all_forbidden_work_zero
        }
    }

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct BoundaryCaseObservationV1 {
        error: Option<PublicationErrorV1>,
        after_catalog_publication: bool,
        pack_len: u64,
        preparation_entries: u64,
        closure_entries: u64,
        carrier_entries: u64,
        catalog_entries: u64,
        object_entries: u64,
        residue_bytes: u64,
        expected_residue_bytes: u64,
        bytes_written: u64,
        admitted_slots: u64,
        closure_fences: u64,
        open_file_handles_high_water: u64,
        incumbent_preserved: bool,
        zero_forbidden_work: bool,
    }

    impl BoundaryCaseObservationV1 {
        pub const fn error(self) -> Option<PublicationErrorV1> {
            self.error
        }
        pub const fn after_catalog_publication(self) -> bool {
            self.after_catalog_publication
        }
        pub const fn pack_len(self) -> u64 {
            self.pack_len
        }
        pub const fn preparation_entries(self) -> u64 {
            self.preparation_entries
        }
        pub const fn closure_entries(self) -> u64 {
            self.closure_entries
        }
        pub const fn carrier_entries(self) -> u64 {
            self.carrier_entries
        }
        pub const fn catalog_entries(self) -> u64 {
            self.catalog_entries
        }
        pub const fn object_entries(self) -> u64 {
            self.object_entries
        }
        pub const fn residue_bytes(self) -> u64 {
            self.residue_bytes
        }
        pub const fn expected_residue_bytes(self) -> u64 {
            self.expected_residue_bytes
        }
        pub const fn bytes_written(self) -> u64 {
            self.bytes_written
        }
        pub const fn admitted_slots(self) -> u64 {
            self.admitted_slots
        }
        pub const fn closure_fences(self) -> u64 {
            self.closure_fences
        }
        pub const fn open_file_handles_high_water(self) -> u64 {
            self.open_file_handles_high_water
        }
        pub const fn incumbent_preserved(self) -> bool {
            self.incumbent_preserved
        }
        pub const fn zero_forbidden_work(self) -> bool {
            self.zero_forbidden_work
        }
    }

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct BoundaryMatrixObservationV1 {
        cases: [BoundaryCaseObservationV1; 10],
        case_count: usize,
    }

    impl BoundaryMatrixObservationV1 {
        pub fn cases(&self) -> &[BoundaryCaseObservationV1] {
            &self.cases[..self.case_count]
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ComparisonOverflowObservationV1 {
        error: Option<PublicationErrorV1>,
        comparison_bytes: u64,
        comparison_windows: u64,
        read_bytes_delta: u64,
        read_calls_delta: u64,
        preparation_entries: u64,
        carrier_entries: u64,
        catalog_entries: u64,
        residue_bytes: u64,
        admitted_slots: u64,
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
        installed_pack_len: u64,
        incumbent_identity: IncumbentIdentityObservationV1,
        owner_usable: bool,
        stale_usable: bool,
        zero_forbidden_work: bool,
    }

    impl ComparisonOverflowObservationV1 {
        pub const fn error(&self) -> Option<PublicationErrorV1> {
            self.error
        }

        pub const fn comparison_bytes(&self) -> u64 {
            self.comparison_bytes
        }

        pub const fn comparison_windows(&self) -> u64 {
            self.comparison_windows
        }

        pub const fn read_bytes_delta(&self) -> u64 {
            self.read_bytes_delta
        }

        pub const fn read_calls_delta(&self) -> u64 {
            self.read_calls_delta
        }

        pub const fn preparation_entries(&self) -> u64 {
            self.preparation_entries
        }

        pub const fn carrier_entries(&self) -> u64 {
            self.carrier_entries
        }

        pub const fn catalog_entries(&self) -> u64 {
            self.catalog_entries
        }

        pub const fn residue_bytes(&self) -> u64 {
            self.residue_bytes
        }

        pub const fn admitted_slots(&self) -> u64 {
            self.admitted_slots
        }

        pub const fn storage_bytes_requested(&self) -> u64 {
            self.storage_bytes_requested
        }

        pub const fn storage_bytes_reserved(&self) -> u64 {
            self.storage_bytes_reserved
        }

        pub const fn storage_bytes_released(&self) -> u64 {
            self.storage_bytes_released
        }

        pub const fn storage_bytes_committed(&self) -> u64 {
            self.storage_bytes_committed
        }

        pub const fn storage_bytes_retained(&self) -> u64 {
            self.storage_bytes_retained
        }

        pub const fn storage_inodes_requested(&self) -> u64 {
            self.storage_inodes_requested
        }

        pub const fn storage_inodes_reserved(&self) -> u64 {
            self.storage_inodes_reserved
        }

        pub const fn storage_inodes_released(&self) -> u64 {
            self.storage_inodes_released
        }

        pub const fn storage_inodes_committed(&self) -> u64 {
            self.storage_inodes_committed
        }

        pub const fn storage_inodes_retained(&self) -> u64 {
            self.storage_inodes_retained
        }

        pub const fn installed_pack_len(&self) -> u64 {
            self.installed_pack_len
        }

        pub const fn incumbent_identity(&self) -> IncumbentIdentityObservationV1 {
            self.incumbent_identity
        }

        pub const fn owner_usable(&self) -> bool {
            self.owner_usable
        }

        pub const fn stale_usable(&self) -> bool {
            self.stale_usable
        }

        pub const fn zero_forbidden_work(&self) -> bool {
            self.zero_forbidden_work
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct IncumbentObservationV1 {
        error: Option<PublicationErrorV1>,
        preparation_entries: u64,
        carrier_entries: u64,
        catalog_entries: u64,
        residue_bytes: u64,
        admitted_slots: u64,
        installed_pack_len: u64,
        incumbent_preserved: bool,
        zero_forbidden_work: bool,
    }

    impl IncumbentObservationV1 {
        pub const fn error(&self) -> Option<PublicationErrorV1> {
            self.error
        }

        pub const fn preparation_entries(&self) -> u64 {
            self.preparation_entries
        }

        pub const fn carrier_entries(&self) -> u64 {
            self.carrier_entries
        }

        pub const fn catalog_entries(&self) -> u64 {
            self.catalog_entries
        }

        pub const fn residue_bytes(&self) -> u64 {
            self.residue_bytes
        }

        pub const fn admitted_slots(&self) -> u64 {
            self.admitted_slots
        }

        pub const fn installed_pack_len(&self) -> u64 {
            self.installed_pack_len
        }

        pub const fn incumbent_preserved(&self) -> bool {
            self.incumbent_preserved
        }

        pub const fn zero_forbidden_work(&self) -> bool {
            self.zero_forbidden_work
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ClosureFailureObservationV1 {
        error: Option<CoreError>,
        pack_len: u64,
        record_count: u32,
        residue_bytes: u64,
        closure_entries: u64,
        closure_fences: u64,
        bytes_read: u64,
        publication_authority_dispatches: u64,
        closure_path_is_file: bool,
        admitted_slots: u64,
        zero_forbidden_work: bool,
    }

    impl ClosureFailureObservationV1 {
        pub const fn error(&self) -> Option<CoreError> {
            self.error
        }

        pub const fn pack_len(&self) -> u64 {
            self.pack_len
        }

        pub const fn record_count(&self) -> u32 {
            self.record_count
        }

        pub const fn residue_bytes(&self) -> u64 {
            self.residue_bytes
        }

        pub const fn expected_residue_bytes(&self) -> u64 {
            self.pack_len
                + self.record_count as u64 * PERSISTENT_LOCATOR_BYTES_V1 as u64
                + CATALOG_MARKER_BYTES as u64
        }

        pub const fn closure_entries(&self) -> u64 {
            self.closure_entries
        }

        pub const fn closure_fences(&self) -> u64 {
            self.closure_fences
        }

        pub const fn bytes_read(&self) -> u64 {
            self.bytes_read
        }

        pub const fn publication_authority_dispatches(&self) -> u64 {
            self.publication_authority_dispatches
        }

        pub const fn closure_path_is_file(&self) -> bool {
            self.closure_path_is_file
        }

        pub const fn admitted_slots(&self) -> u64 {
            self.admitted_slots
        }

        pub const fn zero_forbidden_work(&self) -> bool {
            self.zero_forbidden_work
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct OccupiedOverflowObservationV1 {
        initial_admitted_slots: u64,
        initial_preparation_entries: u64,
        metadata_error: Option<PublicationErrorV1>,
        metadata_bytes: u64,
        metadata_calls: u64,
        metadata_object_cached: bool,
        pack_error: Option<PublicationErrorV1>,
        pack_bytes: u64,
        pack_calls: u64,
        pack_object_cached: bool,
        payload_len: Option<u64>,
        payload_object_cached_before_read: bool,
        payload_error: Option<PublicationErrorV1>,
        payload_bytes: u64,
        payload_calls: u64,
        payload_prefix_preserved: bool,
        payload_object_cached: bool,
        current_handle_usable: bool,
        reopen_usable: bool,
        admitted_slots: u64,
        preparation_entries: u64,
    }

    impl OccupiedOverflowObservationV1 {
        pub const fn initial_admitted_slots(&self) -> u64 {
            self.initial_admitted_slots
        }

        pub const fn initial_preparation_entries(&self) -> u64 {
            self.initial_preparation_entries
        }

        pub const fn metadata_error(&self) -> Option<PublicationErrorV1> {
            self.metadata_error
        }

        pub const fn metadata_bytes(&self) -> u64 {
            self.metadata_bytes
        }

        pub const fn metadata_calls(&self) -> u64 {
            self.metadata_calls
        }

        pub const fn metadata_object_cached(&self) -> bool {
            self.metadata_object_cached
        }

        pub const fn pack_error(&self) -> Option<PublicationErrorV1> {
            self.pack_error
        }

        pub const fn pack_bytes(&self) -> u64 {
            self.pack_bytes
        }

        pub const fn pack_calls(&self) -> u64 {
            self.pack_calls
        }

        pub const fn pack_object_cached(&self) -> bool {
            self.pack_object_cached
        }

        pub const fn payload_len(&self) -> Option<u64> {
            self.payload_len
        }

        pub const fn payload_object_cached_before_read(&self) -> bool {
            self.payload_object_cached_before_read
        }

        pub const fn payload_error(&self) -> Option<PublicationErrorV1> {
            self.payload_error
        }

        pub const fn payload_bytes(&self) -> u64 {
            self.payload_bytes
        }

        pub const fn payload_calls(&self) -> u64 {
            self.payload_calls
        }

        pub const fn payload_prefix_preserved(&self) -> bool {
            self.payload_prefix_preserved
        }

        pub const fn payload_object_cached(&self) -> bool {
            self.payload_object_cached
        }

        pub const fn current_handle_usable(&self) -> bool {
            self.current_handle_usable
        }

        pub const fn reopen_usable(&self) -> bool {
            self.reopen_usable
        }

        pub const fn admitted_slots(&self) -> u64 {
            self.admitted_slots
        }

        pub const fn preparation_entries(&self) -> u64 {
            self.preparation_entries
        }
    }

    impl PublicationObservationV1 {
        pub const fn error(&self) -> Option<PublicationErrorV1> {
            self.error
        }

        pub const fn directories_observed(&self) -> bool {
            self.directories_observed
        }

        pub const fn preparation_entries(&self) -> u64 {
            self.preparation_entries
        }

        pub const fn carrier_entries(&self) -> u64 {
            self.carrier_entries
        }

        pub const fn object_entries(&self) -> u64 {
            self.object_entries
        }

        pub const fn catalog_entries(&self) -> u64 {
            self.catalog_entries
        }

        pub const fn residue_bytes(&self) -> u64 {
            self.residue_bytes
        }

        pub const fn bytes_written(&self) -> u64 {
            self.bytes_written
        }

        pub const fn admitted_slots(&self) -> u64 {
            self.admitted_slots
        }

        pub const fn zero_forbidden_work(&self) -> bool {
            self.zero_forbidden_work
        }

        pub const fn locator_publications(&self) -> u32 {
            self.locator_publications
        }

        pub const fn source_payload_bytes_read(&self) -> u64 {
            self.source_payload_bytes_read
        }
    }

    struct PublicationControl {
        target: u32,
        publications: u32,
        cancelled: bool,
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
            let released = self.released.lock().expect("watchdog gate is healthy");
            let (released, timeout) = self
                .wake
                .wait_timeout_while(released, Duration::from_secs(5), |released| !*released)
                .expect("watchdog gate is healthy");
            assert!(*released, "watchdog gate timed out: {timeout:?}");
        }

        fn release(&self) {
            *self.released.lock().expect("watchdog gate is healthy") = true;
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

    struct ObservePublicationLockScopeV1 {
        cas: FsCasV1,
        fresh_carrier: bool,
        observed: bool,
        visibility_available: bool,
        publication_available: bool,
    }

    impl FsCasControlV1 for ObservePublicationLockScopeV1 {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            let target = if self.fresh_carrier {
                boundary == FsCasBoundaryV1::AfterCarrierInstall
            } else {
                matches!(
                    boundary,
                    FsCasBoundaryV1::BeforeIncumbentComparisonWindow
                        | FsCasBoundaryV1::BeforeObjectComparisonWindow
                )
            };
            if target {
                self.observed = true;
                self.visibility_available = self.cas.visibility_lock_available_for_test_v1();
                self.publication_available = self.cas.publication_lock_available_for_test_v1();
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    struct BlockPreparationCreateV1 {
        entered: mpsc::SyncSender<()>,
        release: mpsc::Receiver<()>,
        blocked: bool,
    }

    impl FsCasControlV1 for BlockPreparationCreateV1 {
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
            if !self.blocked && boundary == FsCasFilesystemBoundaryV1::PreparationCreate {
                self.blocked = true;
                self.entered
                    .send(())
                    .expect("preparation-create watchdog receiver remains live");
                self.release
                    .recv_timeout(Duration::from_secs(5))
                    .expect("preparation-create release watchdog expired");
            }
            None
        }
    }

    struct BlockCatalogMarkerWriteV1 {
        entered: mpsc::SyncSender<()>,
        release: mpsc::Receiver<()>,
        catalog_phase: bool,
        blocked: bool,
    }

    impl FsCasControlV1 for BlockCatalogMarkerWriteV1 {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary == FsCasBoundaryV1::BeforeCatalogPublication {
                self.catalog_phase = true;
            }
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
            if self.catalog_phase
                && !self.blocked
                && boundary == FsCasFilesystemBoundaryV1::MarkerWrite
            {
                self.blocked = true;
                self.entered
                    .send(())
                    .expect("catalog preparation watchdog receiver remains live");
                self.release
                    .recv_timeout(Duration::from_secs(5))
                    .expect("catalog preparation release watchdog expired");
            }
            None
        }
    }

    struct BlockAfterLocatorPublicationV1 {
        release: Arc<WatchdogGateV1>,
        entered: mpsc::SyncSender<()>,
        blocked: bool,
    }

    impl FsCasControlV1 for BlockAfterLocatorPublicationV1 {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if !self.blocked && boundary == FsCasBoundaryV1::AfterObjectLocatorPublication {
                self.blocked = true;
                self.entered
                    .send(())
                    .expect("locator-publication watchdog receiver remains live");
                self.release.wait();
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    struct SignalLocatorOwnerWaitV1 {
        entered: Option<mpsc::SyncSender<()>>,
    }

    impl FsCasControlV1 for SignalLocatorOwnerWaitV1 {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary == FsCasBoundaryV1::LocatorOwnerPublicationWait {
                if let Some(entered) = self.entered.take() {
                    entered
                        .send(())
                        .expect("locator-owner watchdog receiver remains live");
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

    struct BlockAtIncumbentAuthorityV1 {
        release: Arc<WatchdogGateV1>,
        entered: Option<mpsc::SyncSender<()>>,
    }

    impl FsCasControlV1 for BlockAtIncumbentAuthorityV1 {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary == FsCasBoundaryV1::BeforeIncumbentMarkerRead {
                if let Some(entered) = self.entered.take() {
                    entered
                        .send(())
                        .expect("incumbent-authority watchdog receiver remains live");
                    self.release.wait();
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

    struct ContinueControlV1;

    impl FsCasControlV1 for ContinueControlV1 {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    #[derive(Clone, Copy)]
    enum FaultStopV1 {
        Cancellation(FsCasBoundaryV1),
        Deadline(FsCasBoundaryV1),
    }

    struct FaultControl {
        stop: FaultStopV1,
        current: Option<FsCasBoundaryV1>,
        cleanup_target: Option<FsCasCleanupTargetV1>,
        cleanup_injected: bool,
    }

    impl FaultControl {
        const fn cancellation(boundary: FsCasBoundaryV1) -> Self {
            Self {
                stop: FaultStopV1::Cancellation(boundary),
                current: None,
                cleanup_target: None,
                cleanup_injected: false,
            }
        }

        const fn deadline(boundary: FsCasBoundaryV1) -> Self {
            Self {
                stop: FaultStopV1::Deadline(boundary),
                current: None,
                cleanup_target: None,
                cleanup_injected: false,
            }
        }

        const fn with_cleanup_failure(mut self, target: FsCasCleanupTargetV1) -> Self {
            self.cleanup_target = Some(target);
            self
        }
    }

    impl FsCasControlV1 for FaultControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            self.current = Some(boundary);
        }

        fn cancellation_requested(&mut self) -> bool {
            matches!(self.stop, FaultStopV1::Cancellation(target) if self.current == Some(target))
        }

        fn deadline_exceeded(&mut self) -> bool {
            matches!(self.stop, FaultStopV1::Deadline(target) if self.current == Some(target))
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if !self.cleanup_injected && self.cleanup_target == Some(target) {
                self.cleanup_injected = true;
                true
            } else {
                false
            }
        }
    }

    struct ReadFaultControl {
        boundary: FsCasFilesystemBoundaryV1,
        error: FsCasErrorV1,
        injected: bool,
    }

    impl FsCasControlV1 for ReadFaultControl {
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
                Some(self.error)
            } else {
                None
            }
        }
    }

    struct InstallMalformedLocatorControl {
        locator: PathBuf,
        injected: bool,
    }

    impl FsCasControlV1 for InstallMalformedLocatorControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary == FsCasBoundaryV1::BeforeObjectLocatorPublication && !self.injected {
                fs::write(&self.locator, [0_u8; 160]).expect("semantic malformed locator");
                self.injected = true;
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    struct InstallLocatorAndFailPreparationCleanupControl {
        locator: PathBuf,
        occupant_injected: bool,
        cleanup_injected: bool,
    }

    impl FsCasControlV1 for InstallLocatorAndFailPreparationCleanupControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary == FsCasBoundaryV1::BeforeObjectLocatorPublication
                && !self.occupant_injected
            {
                fs::write(&self.locator, [0_u8; 160]).expect("semantic malformed locator");
                self.occupant_injected = true;
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::PreparationSpool && !self.cleanup_injected {
                self.cleanup_injected = true;
                true
            } else {
                false
            }
        }
    }

    impl PublicationControl {
        const fn new(target: u32) -> Self {
            Self {
                target,
                publications: 0,
                cancelled: false,
            }
        }
    }

    impl FsCasControlV1 for PublicationControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary == FsCasBoundaryV1::AfterObjectLocatorPublication {
                self.publications = self.publications.saturating_add(1);
                if self.publications == self.target {
                    self.cancelled = true;
                }
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            self.cancelled
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    struct PublicationSource<'a> {
        bytes: &'a [&'a [u8]],
        ids: Vec<TypedPhysicalObjectIdV1>,
        fail_payload: bool,
        payload_bytes_read: u64,
    }

    fn closure_object(kind: u8, payload: &[u8]) -> Vec<u8> {
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

    fn empty_closure_objects() -> (
        Vec<u8>,
        Vec<u8>,
        TypedPhysicalObjectIdV1,
        TypedPhysicalObjectIdV1,
    ) {
        let root = closure_object(2, &[1, 0x10, 0, 0, 0, 0, 0, 0, 0]);
        let root_id = derive_physical_tree_id_v1(&root).expect("semantic closure root id");
        let logical_root = derive_implicit_root_directory_v1(&[]).expect("semantic logical root");
        let version_id = derive_version_v1(logical_root);
        let mut payload = Vec::with_capacity(184);
        payload.extend_from_slice(version_id.as_bytes());
        payload.extend_from_slice(ChunkerSpecV1::frozen().id().as_bytes());
        payload.extend_from_slice(DigestSpecV1::frozen().id().as_bytes());
        payload.extend_from_slice(root_id.as_bytes());
        payload.extend_from_slice(&0_u64.to_be_bytes());
        payload.extend_from_slice(&0_u64.to_be_bytes());
        for count in [0_u32, 1, 0, 0, 0, 0, 0, 2] {
            payload.extend_from_slice(&count.to_be_bytes());
        }
        payload.extend_from_slice(&0_u64.to_be_bytes());
        assert_eq!(payload.len(), 184);
        let version = closure_object(1, &payload);
        let version_id = TypedPhysicalObjectIdV1::VersionRecord(
            derive_physical_version_record_id_v1(&version).expect("semantic closure version id"),
        );
        (
            version,
            root,
            version_id,
            TypedPhysicalObjectIdV1::Tree(root_id),
        )
    }

    struct ClosureSource<'a> {
        objects: &'a [(TypedPhysicalObjectIdV1, Vec<u8>)],
    }

    impl CompleteImmutableClosureReadPortV1 for ClosureSource<'_> {
        fn object_count(&mut self) -> Result<u64, ImmutablePortErrorV1> {
            u64::try_from(self.objects.len()).map_err(|_| ImmutablePortErrorV1::Failure)
        }

        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            Ok(0)
        }

        fn object_id_at(
            &mut self,
            ordinal: u64,
        ) -> Result<TypedPhysicalObjectIdV1, ImmutablePortErrorV1> {
            let ordinal = usize::try_from(ordinal).map_err(|_| ImmutablePortErrorV1::Failure)?;
            self.objects
                .get(ordinal)
                .map(|(id, _)| *id)
                .ok_or(ImmutablePortErrorV1::Failure)
        }

        fn object_len_at(&mut self, ordinal: u64) -> Result<u64, ImmutablePortErrorV1> {
            let ordinal = usize::try_from(ordinal).map_err(|_| ImmutablePortErrorV1::Failure)?;
            self.objects
                .get(ordinal)
                .ok_or(ImmutablePortErrorV1::Failure)
                .and_then(|(_, bytes)| {
                    u64::try_from(bytes.len()).map_err(|_| ImmutablePortErrorV1::Failure)
                })
        }

        fn read_object_exact_at(
            &mut self,
            ordinal: u64,
            offset: u64,
            destination: &mut [u8],
        ) -> Result<(), ImmutablePortErrorV1> {
            let ordinal = usize::try_from(ordinal).map_err(|_| ImmutablePortErrorV1::Failure)?;
            let offset = usize::try_from(offset).map_err(|_| ImmutablePortErrorV1::Failure)?;
            let end = offset
                .checked_add(destination.len())
                .ok_or(ImmutablePortErrorV1::Failure)?;
            let bytes = &self
                .objects
                .get(ordinal)
                .ok_or(ImmutablePortErrorV1::Failure)?
                .1;
            destination.copy_from_slice(
                bytes
                    .get(offset..end)
                    .ok_or(ImmutablePortErrorV1::Failure)?,
            );
            Ok(())
        }
    }

    struct AdmissionSource<'a> {
        objects: Vec<(TypedPhysicalObjectIdV1, &'a [u8])>,
        resident_memory: u64,
        maximum_read: usize,
        count_calls: u64,
        reads: u64,
    }

    impl CompleteImmutableClosureReadPortV1 for AdmissionSource<'_> {
        fn object_count(&mut self) -> Result<u64, ImmutablePortErrorV1> {
            self.count_calls = self
                .count_calls
                .checked_add(1)
                .ok_or(ImmutablePortErrorV1::Failure)?;
            u64::try_from(self.objects.len()).map_err(|_| ImmutablePortErrorV1::Failure)
        }

        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            Ok(self.resident_memory)
        }

        fn object_id_at(
            &mut self,
            ordinal: u64,
        ) -> Result<TypedPhysicalObjectIdV1, ImmutablePortErrorV1> {
            self.objects
                .get(usize::try_from(ordinal).map_err(|_| ImmutablePortErrorV1::Failure)?)
                .map(|(id, _)| *id)
                .ok_or(ImmutablePortErrorV1::Failure)
        }

        fn object_len_at(&mut self, ordinal: u64) -> Result<u64, ImmutablePortErrorV1> {
            self.objects
                .get(usize::try_from(ordinal).map_err(|_| ImmutablePortErrorV1::Failure)?)
                .ok_or(ImmutablePortErrorV1::Failure)
                .and_then(|(_, bytes)| {
                    u64::try_from(bytes.len()).map_err(|_| ImmutablePortErrorV1::Failure)
                })
        }

        fn read_object_exact_at(
            &mut self,
            ordinal: u64,
            offset: u64,
            destination: &mut [u8],
        ) -> Result<(), ImmutablePortErrorV1> {
            self.maximum_read = self.maximum_read.max(destination.len());
            self.reads = self
                .reads
                .checked_add(1)
                .ok_or(ImmutablePortErrorV1::Failure)?;
            let bytes = self
                .objects
                .get(usize::try_from(ordinal).map_err(|_| ImmutablePortErrorV1::Failure)?)
                .map(|(_, bytes)| *bytes)
                .ok_or(ImmutablePortErrorV1::Failure)?;
            let start = usize::try_from(offset).map_err(|_| ImmutablePortErrorV1::Failure)?;
            let end = start
                .checked_add(destination.len())
                .ok_or(ImmutablePortErrorV1::Failure)?;
            destination
                .copy_from_slice(bytes.get(start..end).ok_or(ImmutablePortErrorV1::Failure)?);
            Ok(())
        }
    }

    struct AdmissionOccupied<'a> {
        entry: Option<(TypedPhysicalObjectIdV1, &'a [u8])>,
        resident_memory: u64,
        maximum_read: usize,
        lookups: u64,
        reads: u64,
    }

    impl OccupiedImmutableReadPortV1 for AdmissionOccupied<'_> {
        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            Ok(self.resident_memory)
        }

        fn occupied_len(
            &mut self,
            id: TypedPhysicalObjectIdV1,
        ) -> Result<Option<u64>, ImmutablePortErrorV1> {
            self.lookups = self
                .lookups
                .checked_add(1)
                .ok_or(ImmutablePortErrorV1::Failure)?;
            self.entry
                .filter(|(occupied_id, _)| occupied_id == &id)
                .map_or(Ok(None), |(_, bytes)| {
                    u64::try_from(bytes.len())
                        .map(Some)
                        .map_err(|_| ImmutablePortErrorV1::Failure)
                })
        }

        fn read_occupied_exact_at(
            &mut self,
            id: TypedPhysicalObjectIdV1,
            offset: u64,
            destination: &mut [u8],
        ) -> Result<(), ImmutablePortErrorV1> {
            self.maximum_read = self.maximum_read.max(destination.len());
            self.reads = self
                .reads
                .checked_add(1)
                .ok_or(ImmutablePortErrorV1::Failure)?;
            let (_, bytes) = self
                .entry
                .filter(|(occupied_id, _)| occupied_id == &id)
                .ok_or(ImmutablePortErrorV1::Failure)?;
            let start = usize::try_from(offset).map_err(|_| ImmutablePortErrorV1::Failure)?;
            let end = start
                .checked_add(destination.len())
                .ok_or(ImmutablePortErrorV1::Failure)?;
            destination
                .copy_from_slice(bytes.get(start..end).ok_or(ImmutablePortErrorV1::Failure)?);
            Ok(())
        }
    }

    struct AdmissionSink {
        resident_memory: u64,
        begun: u64,
        staged: Vec<TypedPhysicalObjectIdV1>,
        active: Option<(TypedPhysicalObjectIdV1, u64, u64)>,
        reused: Vec<TypedPhysicalObjectIdV1>,
        visible: Option<TypedPhysicalObjectIdV1>,
        aborts: u64,
    }

    impl PreparedImmutableClosurePortV1 for AdmissionSink {
        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            Ok(self.resident_memory)
        }

        fn begin_private_closure(&mut self, object_count: u64) -> Result<(), ImmutablePortErrorV1> {
            self.begun = object_count;
            Ok(())
        }

        fn begin_private_object(
            &mut self,
            id: TypedPhysicalObjectIdV1,
            exact_len: u64,
        ) -> Result<(), ImmutablePortErrorV1> {
            if self.active.replace((id, exact_len, 0)).is_some() {
                return Err(ImmutablePortErrorV1::Failure);
            }
            Ok(())
        }

        fn write_private_object(
            &mut self,
            canonical_fragment: &[u8],
        ) -> Result<(), ImmutablePortErrorV1> {
            let (_, exact_len, written) =
                self.active.as_mut().ok_or(ImmutablePortErrorV1::Failure)?;
            *written = written
                .checked_add(canonical_fragment.len() as u64)
                .ok_or(ImmutablePortErrorV1::Failure)?;
            if *written > *exact_len {
                return Err(ImmutablePortErrorV1::Failure);
            }
            Ok(())
        }

        fn finish_private_object(
            &mut self,
            id: TypedPhysicalObjectIdV1,
        ) -> Result<(), ImmutablePortErrorV1> {
            let (active_id, exact_len, written) =
                self.active.take().ok_or(ImmutablePortErrorV1::Failure)?;
            if active_id != id || exact_len != written {
                return Err(ImmutablePortErrorV1::Failure);
            }
            self.staged.push(id);
            Ok(())
        }

        fn note_reused_object(
            &mut self,
            validated: ValidatedOccupiedObjectV1,
        ) -> Result<(), ImmutablePortErrorV1> {
            self.reused.push(validated.id());
            Ok(())
        }

        fn make_closure_visible(
            &mut self,
            version_record: TypedPhysicalObjectIdV1,
        ) -> Result<(), ImmutablePortErrorV1> {
            self.visible = Some(version_record);
            Ok(())
        }

        fn abort_private_closure(&mut self) {
            self.aborts = self.aborts.saturating_add(1);
            self.staged.clear();
            self.active = None;
            self.reused.clear();
            self.visible = None;
        }
    }

    fn semantic_object_id(bytes: &[u8]) -> TypedPhysicalObjectIdV1 {
        decode_physical_object_v1(bytes, &mut DiscardStrongEdgesV1)
            .expect("semantic admission request contains decodable expected objects")
            .physical_id()
            .expect("semantic admission expected object has a physical id")
    }

    pub fn admit_v1(request: AdmissionRequestV1<'_>) -> AdmissionObservationV1 {
        let expected = request.expected_objects.unwrap_or(request.objects);
        assert_eq!(request.objects.len(), expected.len());
        let mut source = AdmissionSource {
            objects: request
                .objects
                .iter()
                .zip(expected)
                .map(|(bytes, expected)| (semantic_object_id(expected), *bytes))
                .collect(),
            resident_memory: request.source_resident_bytes,
            maximum_read: 0,
            count_calls: 0,
            reads: 0,
        };
        let expected_ids: Vec<_> = source.objects.iter().map(|(id, _)| *id).collect();
        let version_id = expected_ids[request.version_ordinal];
        let occupied_id = request
            .occupied_ordinal
            .map(|ordinal| expected_ids[ordinal]);
        let mut occupied = AdmissionOccupied {
            entry: occupied_id.zip(request.occupied_bytes),
            resident_memory: request.occupied_resident_bytes,
            maximum_read: 0,
            lookups: 0,
            reads: 0,
        };
        let mut sink = AdmissionSink {
            resident_memory: request.sink_resident_bytes,
            begun: 0,
            staged: Vec::new(),
            active: None,
            reused: Vec::new(),
            visible: None,
            aborts: 0,
        };
        let ledger = ResourceLedgerV1::new(request.ledger_budget_bytes);
        let mut counters = OperationCountersV1::default();
        let mut closure_bitmap = vec![0_u8; request.objects.len().div_ceil(4)];
        let result = admit_complete_immutable_v1(
            &mut source,
            version_id,
            &mut occupied,
            &mut sink,
            &ledger,
            &mut counters,
            AdmissionBuffersV1::new(
                &mut [0_u8; 65_536],
                &mut [0_u8; 65_536],
                &mut [0_u8; 32_768],
                &mut [0_u8; 32_768],
                &mut closure_bitmap,
            ),
        );
        let (error, object_count, created_count, reused_count) = match result {
            Ok(admitted) => (
                None,
                admitted.object_count(),
                admitted.created_count(),
                admitted.reused_count(),
            ),
            Err(error) => (Some(error), 0, 0, 0),
        };
        AdmissionObservationV1 {
            error,
            object_count,
            created_count,
            reused_count,
            sink_begun: sink.begun,
            staged_count: sink.staged.len() as u64,
            staged_in_source_order: sink.staged == expected_ids,
            reused_occupied: occupied_id.is_some_and(|id| sink.reused == [id]),
            visible_expected: sink.visible == Some(version_id),
            sink_aborts: sink.aborts,
            admitted_slots: ledger.admitted_slots(),
            physical_objects_created: counters.physical_objects_created,
            physical_objects_reused: counters.physical_objects_reused,
            closure_objects_missing: counters.closure_objects_missing,
            closure_objects_occupied_validated: counters.closure_objects_occupied_validated,
            publication_authority_dispatches: counters.publication_authority_dispatches,
            bytes_read: counters.bytes_read,
            bytes_copied: counters.bytes_copied,
            bytes_written: counters.bytes_written,
            memory_high_water: counters.memory_high_water,
            planned_high_water: ledger.planned_high_water_bytes(),
            source_count_calls: source.count_calls,
            source_reads: source.reads,
            source_maximum_read: source.maximum_read as u64,
            occupied_lookups: occupied.lookups,
            occupied_reads: occupied.reads,
            occupied_maximum_read: occupied.maximum_read as u64,
        }
    }

    impl<'a> PublicationSource<'a> {
        fn new(bytes: &'a [&'a [u8]]) -> CoreResult<Self> {
            let ids = bytes
                .iter()
                .map(|bytes| {
                    decode_physical_object_v1(bytes, &mut DiscardStrongEdgesV1)
                        .map_err(|_| CoreError::Schema)?
                        .physical_id()
                        .map_err(|_| CoreError::IdMismatch)
                })
                .collect::<CoreResult<Vec<_>>>()?;
            Ok(Self {
                bytes,
                ids,
                fail_payload: false,
                payload_bytes_read: 0,
            })
        }

        fn with_payload_failure(mut self) -> Self {
            self.fail_payload = true;
            self
        }
    }

    impl PackObjectSourceV1 for PublicationSource<'_> {
        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            u64::try_from(self.ids.capacity())
                .map_err(|_| CoreError::IntegerOverflow)?
                .checked_mul(core::mem::size_of::<TypedPhysicalObjectIdV1>() as u64)
                .ok_or(CoreError::IntegerOverflow)
        }

        fn declared_object_count(&self) -> CoreResult<u32> {
            u32::try_from(self.bytes.len()).map_err(|_| CoreError::IntegerOverflow)
        }

        fn object_id(&mut self, ordinal: u32) -> Result<TypedPhysicalObjectIdV1, PackPortErrorV1> {
            self.ids
                .get(ordinal as usize)
                .copied()
                .ok_or(PackPortErrorV1::Failure)
        }

        fn object_len(&mut self, ordinal: u32) -> Result<u64, PackPortErrorV1> {
            self.bytes
                .get(ordinal as usize)
                .ok_or(PackPortErrorV1::Failure)
                .and_then(|bytes| u64::try_from(bytes.len()).map_err(|_| PackPortErrorV1::Failure))
        }

        fn read_object_exact_at(
            &mut self,
            ordinal: u32,
            offset: u64,
            destination: &mut [u8],
        ) -> Result<(), PackPortErrorV1> {
            if self.fail_payload {
                return Err(PackPortErrorV1::Failure);
            }
            let bytes = self
                .bytes
                .get(ordinal as usize)
                .ok_or(PackPortErrorV1::Failure)?;
            let start = usize::try_from(offset).map_err(|_| PackPortErrorV1::Failure)?;
            let end = start
                .checked_add(destination.len())
                .ok_or(PackPortErrorV1::Failure)?;
            destination.copy_from_slice(bytes.get(start..end).ok_or(PackPortErrorV1::Failure)?);
            self.payload_bytes_read = self
                .payload_bytes_read
                .checked_add(destination.len() as u64)
                .ok_or(PackPortErrorV1::Failure)?;
            Ok(())
        }
    }

    #[derive(Default)]
    struct PublicationSpool {
        entries: Vec<PackIndexEntryV1>,
        cursor: usize,
        maximum: usize,
    }

    impl PackIndexSpoolV1 for PublicationSpool {
        fn resident_memory_bound_bytes(&self, maximum_entries: u32) -> CoreResult<u64> {
            u64::from(maximum_entries)
                .checked_mul(core::mem::size_of::<PackIndexEntryV1>() as u64)
                .ok_or(CoreError::IntegerOverflow)
        }

        fn reset(&mut self, maximum_entries: u32) -> Result<(), PackPortErrorV1> {
            self.entries.clear();
            self.cursor = 0;
            self.maximum = maximum_entries as usize;
            Ok(())
        }

        fn push(&mut self, entry: PackIndexEntryV1) -> Result<(), PackPortErrorV1> {
            if self.entries.len() >= self.maximum {
                return Err(PackPortErrorV1::Failure);
            }
            self.entries.push(entry);
            Ok(())
        }

        fn sort_by_key(&mut self) -> Result<(), PackPortErrorV1> {
            self.entries.sort_by(PackIndexEntryV1::compare_key);
            Ok(())
        }

        fn sort_by_offset(&mut self) -> Result<(), PackPortErrorV1> {
            self.entries.sort_by(PackIndexEntryV1::compare_offset);
            Ok(())
        }

        fn rewind(&mut self) -> Result<(), PackPortErrorV1> {
            self.cursor = 0;
            Ok(())
        }

        fn next(&mut self) -> Result<Option<PackIndexEntryV1>, PackPortErrorV1> {
            let entry = self.entries.get(self.cursor).copied();
            self.cursor = self
                .cursor
                .checked_add(usize::from(entry.is_some()))
                .ok_or(PackPortErrorV1::Failure)?;
            Ok(entry)
        }

        fn abort(&mut self) {
            self.entries.clear();
            self.cursor = 0;
        }
    }

    fn publication_error(error: FsCasErrorV1) -> PublicationErrorV1 {
        match error {
            FsCasErrorV1::Core(error) => PublicationErrorV1::Core(error),
            FsCasErrorV1::Busy => PublicationErrorV1::Busy,
            FsCasErrorV1::Invalidated => PublicationErrorV1::Invalidated,
            FsCasErrorV1::Unsupported => PublicationErrorV1::Unsupported,
            FsCasErrorV1::SynchronizationPoisoned => PublicationErrorV1::SynchronizationPoisoned,
            FsCasErrorV1::CrossOwner => PublicationErrorV1::CrossOwner,
            FsCasErrorV1::WrongOperationKind => PublicationErrorV1::WrongOperationKind,
            FsCasErrorV1::Integrity => PublicationErrorV1::Integrity,
            FsCasErrorV1::Collision => PublicationErrorV1::Collision,
            FsCasErrorV1::MalformedOccupant => PublicationErrorV1::MalformedOccupant,
            FsCasErrorV1::MissingOccupant => PublicationErrorV1::MissingOccupant,
            FsCasErrorV1::UnequalOccupant => PublicationErrorV1::UnequalOccupant,
            FsCasErrorV1::Io => PublicationErrorV1::Io,
            FsCasErrorV1::ResourceExhausted(_) => PublicationErrorV1::ResourceRefused,
            FsCasErrorV1::Filesystem(_) => PublicationErrorV1::Filesystem,
            FsCasErrorV1::CleanupFailed(_) => PublicationErrorV1::CleanupFailed,
            FsCasErrorV1::InvalidationFailed => PublicationErrorV1::InvalidationFailed,
            FsCasErrorV1::TerminalFailure { .. } => PublicationErrorV1::TerminalFailure,
        }
    }

    pub(crate) fn publication_error_v1(error: FsCasErrorV1) -> PublicationErrorV1 {
        publication_error(error)
    }

    const fn publication_cleanup_target(
        target: FsCasCleanupTargetV1,
    ) -> PublicationCleanupTargetV1 {
        match target {
            FsCasCleanupTargetV1::RootInitialization => {
                PublicationCleanupTargetV1::RootInitialization
            }
            FsCasCleanupTargetV1::ObjectLocator => PublicationCleanupTargetV1::ObjectLocator,
            FsCasCleanupTargetV1::Carrier => PublicationCleanupTargetV1::Carrier,
            FsCasCleanupTargetV1::PrivatePack => PublicationCleanupTargetV1::PrivatePack,
            FsCasCleanupTargetV1::PreparationSpool => PublicationCleanupTargetV1::PreparationSpool,
            FsCasCleanupTargetV1::PublishedMarkerAlias => {
                PublicationCleanupTargetV1::PublishedMarkerAlias
            }
            FsCasCleanupTargetV1::RootInvalidation => PublicationCleanupTargetV1::RootInvalidation,
        }
    }

    const fn publication_cause(cause: FsCasFailureCauseV1) -> PublicationCauseV1 {
        match cause {
            FsCasFailureCauseV1::Core(error) => PublicationCauseV1::Core(error),
            FsCasFailureCauseV1::Busy => PublicationCauseV1::Busy,
            FsCasFailureCauseV1::Invalidated => PublicationCauseV1::Invalidated,
            FsCasFailureCauseV1::Unsupported => PublicationCauseV1::Unsupported,
            FsCasFailureCauseV1::SynchronizationPoisoned => {
                PublicationCauseV1::SynchronizationPoisoned
            }
            FsCasFailureCauseV1::CrossOwner => PublicationCauseV1::CrossOwner,
            FsCasFailureCauseV1::WrongOperationKind => PublicationCauseV1::WrongOperationKind,
            FsCasFailureCauseV1::Integrity => PublicationCauseV1::Integrity,
            FsCasFailureCauseV1::Collision => PublicationCauseV1::Collision,
            FsCasFailureCauseV1::MalformedOccupant => PublicationCauseV1::MalformedOccupant,
            FsCasFailureCauseV1::MissingOccupant => PublicationCauseV1::MissingOccupant,
            FsCasFailureCauseV1::UnequalOccupant => PublicationCauseV1::UnequalOccupant,
            FsCasFailureCauseV1::Io => PublicationCauseV1::Io,
            FsCasFailureCauseV1::ResourceExhausted(_) => PublicationCauseV1::ResourceRefused,
            FsCasFailureCauseV1::Filesystem(_) => PublicationCauseV1::Filesystem,
            FsCasFailureCauseV1::CleanupFailed(target) => {
                PublicationCauseV1::CleanupFailed(publication_cleanup_target(target))
            }
            FsCasFailureCauseV1::InvalidationFailed => PublicationCauseV1::InvalidationFailed,
        }
    }

    const fn publication_causes(error: FsCasErrorV1) -> (PublicationCauseV1, PublicationCauseV1) {
        let (first, dominant) = error.failure_causes_v1();
        (publication_cause(first), publication_cause(dominant))
    }

    pub(crate) const fn publication_causes_v1(
        error: FsCasErrorV1,
    ) -> (PublicationCauseV1, PublicationCauseV1) {
        publication_causes(error)
    }

    fn directory_entries(root: &Path, name: &str) -> Option<u64> {
        fs::read_dir(root.join(name))
            .ok()
            .map(|entries| entries.count() as u64)
    }

    pub fn transfer_pack_then_reopen_v1(
        request: PublicationRequestV1<'_>,
    ) -> PackTransferObservationV1 {
        let cas = FsCasV1::create_new(request.root).expect("create pack-transfer root");
        let fixed_handles_within_budget = cas
            .fixed_handle_ledger_charge_bytes()
            .is_ok_and(|bytes| bytes <= crate::limits::BASE_LEDGER_BYTES);
        let ids = request
            .objects
            .iter()
            .map(|bytes| semantic_object_id(bytes))
            .collect::<Vec<_>>();
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut counters = OperationCountersV1::default();
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        let (mut pack, source_payload_bytes_read) =
            build_publication_pack_raw(&cas, request.objects, &ledger, &mut counters, &mut scratch)
                .expect("build pack-transfer pack");
        let mut spool = PublicationSpool::default();
        let admission = cas
            .admit_pack(&mut pack, &mut spool, &ledger, &mut counters, &mut scratch)
            .expect("admit pack-transfer pack");
        let pack_len = admission.sealed().pack_len();
        let installed = admission.outcome() == super::FsPackAdmissionOutcomeV1::Installed;
        drop(cas);

        let reopened = FsCasV1::open_existing(request.root).expect("reopen installed pack");
        let mut occupied = reopened.occupied().expect("open occupied reader");
        let mut reopened_lengths_match = true;
        let mut reopened_bytes_match = true;
        for (id, expected) in ids.into_iter().zip(request.objects) {
            reopened_lengths_match &=
                occupied.occupied_len(id).ok().flatten() == Some(expected.len() as u64);
            let mut actual = vec![0_u8; expected.len()];
            for (offset, block) in actual.chunks_mut(7).enumerate() {
                if occupied
                    .read_occupied_exact_at(id, (offset * 7) as u64, block)
                    .is_err()
                {
                    reopened_bytes_match = false;
                    break;
                }
            }
            reopened_bytes_match &= actual == *expected;
        }
        let (reopened_bytes_read, reopened_read_calls) = occupied
            .direct_storage_read_observation()
            .expect("read reopened storage observation");
        PackTransferObservationV1 {
            fixed_handles_within_budget,
            installed,
            pack_len,
            preparation_entries: directory_entries(request.root, "preparation").unwrap_or(0),
            carrier_entries: directory_entries(request.root, "carriers").unwrap_or(0),
            catalog_entries: directory_entries(request.root, "catalog").unwrap_or(0),
            bytes_written: counters.fscas_bytes_written,
            bytes_read: counters.fscas_bytes_read,
            read_calls: counters.fscas_read_calls,
            catalog_operations: counters.fscas_catalog_operations,
            installed_carrier_logical_bytes: counters.installed_carrier_logical_bytes,
            zero_forbidden_work: counters.has_zero_forbidden_work(),
            admitted_slots: ledger.admitted_slots(),
            reopened_lengths_match,
            reopened_bytes_match,
            reopened_read_calls,
            reopened_bytes_read,
            expected_object_bytes: request.objects.iter().map(|bytes| bytes.len() as u64).sum(),
            source_payload_bytes_read,
            expected_source_payload_bytes: request
                .objects
                .iter()
                .map(|bytes| bytes.len() as u64)
                .sum(),
        }
    }

    pub fn existing_catalog_classification_v1(
        request: PublicationRequestV1<'_>,
        case: ExistingCatalogCaseV1,
    ) -> ExistingCatalogObservationV1 {
        fs::create_dir_all(request.root).expect("create existing-catalog fixture parent");
        let root = request.root.join(match case {
            ExistingCatalogCaseV1::BindingMismatch => "binding",
            ExistingCatalogCaseV1::SameIdUnequal => "same-id-unequal",
        });
        let cas = FsCasV1::create_new(&root).expect("create existing-catalog root");
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut installed_counters = OperationCountersV1::default();
        let (mut incumbent, _) = build_publication_pack_raw(
            &cas,
            request.objects,
            &ledger,
            &mut installed_counters,
            &mut scratch,
        )
        .expect("build existing-catalog incumbent");
        let mut spool = PublicationSpool::default();
        let incumbent_installed = cas
            .admit_pack(
                &mut incumbent,
                &mut spool,
                &ledger,
                &mut installed_counters,
                &mut scratch,
            )
            .is_ok_and(|admission| {
                admission.outcome() == super::FsPackAdmissionOutcomeV1::Installed
            });

        let marker_path = one_entry(&root, "catalog");
        let original_permissions = make_owner_writable(&marker_path);
        let mut marker = fs::read(&marker_path).expect("read installed catalog marker");
        match case {
            ExistingCatalogCaseV1::BindingMismatch => marker[8] ^= 1,
            ExistingCatalogCaseV1::SameIdUnequal => {
                let pack_len = u64::from_be_bytes(
                    marker[40..48]
                        .try_into()
                        .expect("catalog pack length field"),
                );
                marker[40..48].copy_from_slice(
                    &pack_len
                        .checked_add(1)
                        .expect("mutated pack length")
                        .to_be_bytes(),
                );
            }
        }
        fs::write(&marker_path, &marker).expect("write mutated catalog marker");
        fs::set_permissions(&marker_path, original_permissions)
            .expect("restore catalog marker permissions");

        let mut candidate_counters = OperationCountersV1::default();
        let (mut candidate, _) = build_publication_pack_raw(
            &cas,
            request.objects,
            &ledger,
            &mut candidate_counters,
            &mut scratch,
        )
        .expect("build existing-catalog candidate");
        let error = cas
            .admit_pack(
                &mut candidate,
                &mut spool,
                &ledger,
                &mut candidate_counters,
                &mut scratch,
            )
            .err()
            .map(publication_error);

        ExistingCatalogObservationV1 {
            incumbent_installed,
            error,
            preparation_entries: directory_entries(&root, "preparation").unwrap_or(0),
            carrier_entries: directory_entries(&root, "carriers").unwrap_or(0),
            object_entries: directory_entries(&root, "objects").unwrap_or(0),
            catalog_entries: directory_entries(&root, "catalog").unwrap_or(0),
            marker_preserved: fs::read(&marker_path).is_ok_and(|bytes| bytes == marker),
            unreachable_installed_residue_bytes: candidate_counters
                .unreachable_installed_residue_bytes,
            admitted_slots: ledger.admitted_slots(),
            zero_forbidden_work: candidate_counters.has_zero_forbidden_work(),
        }
    }

    pub fn closure_capability_failure_v1(
        root: &Path,
        case: ClosureCapabilityFailureCaseV1,
    ) -> ClosureCapabilityFailureObservationV1 {
        let cas = FsCasV1::create_new(root).expect("create closure-capability root");
        let (version, root_object, version_id, root_id) = empty_closure_objects();
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut counters = OperationCountersV1::default();
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        if matches!(
            case,
            ClosureCapabilityFailureCaseV1::SpoofedBytes
                | ClosureCapabilityFailureCaseV1::WrongVersionRecord
        ) {
            let objects = [version.as_slice(), root_object.as_slice()];
            let (mut pack, _) =
                build_publication_pack_raw(&cas, &objects, &ledger, &mut counters, &mut scratch)
                    .expect("build closure-capability incumbent");
            let mut spool = PublicationSpool::default();
            cas.admit_pack(&mut pack, &mut spool, &ledger, &mut counters, &mut scratch)
                .expect("admit closure-capability incumbent");
        }

        let wrong_version = TypedPhysicalObjectIdV1::VersionRecord(
            derive_physical_version_record_id_v1(&closure_object(1, b"wrong-version"))
                .expect("derive wrong version id"),
        );
        let mut spoofed_version = version.clone();
        spoofed_version[52] ^= 1;
        let objects = match case {
            ClosureCapabilityFailureCaseV1::NonexistentObjects
            | ClosureCapabilityFailureCaseV1::WrongVersionRecord => {
                vec![(version_id, version), (root_id, root_object)]
            }
            ClosureCapabilityFailureCaseV1::SpoofedBytes => {
                vec![(version_id, spoofed_version), (root_id, root_object)]
            }
            ClosureCapabilityFailureCaseV1::DuplicateTypedIds => vec![
                (version_id, version.clone()),
                (version_id, version),
                (root_id, root_object),
            ],
        };
        let requested_version = if case == ClosureCapabilityFailureCaseV1::WrongVersionRecord {
            wrong_version
        } else {
            version_id
        };
        let mut closure = ClosureSource { objects: &objects };
        let mut operation = cas
            .begin_closure_operation()
            .expect("begin closure-capability operation");
        let mut incoming_comparison = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut occupied_comparison = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut source_window = [0_u8; 32_768];
        let mut cdc_ring = [0_u8; 32_768];
        let mut traversal = [0_u8; 1];
        let error = cas
            .admit_complete_closure(
                &mut operation,
                &mut closure,
                requested_version,
                &ledger,
                &mut counters,
                AdmissionBuffersV1::new(
                    &mut incoming_comparison,
                    &mut occupied_comparison,
                    &mut source_window,
                    &mut cdc_ring,
                    &mut traversal,
                ),
            )
            .err();
        ClosureCapabilityFailureObservationV1 {
            error,
            closure_entries: directory_entries(root, "closures").unwrap_or(0),
            closure_fences: counters.closure_fences,
            fscas_bytes_read: counters.fscas_bytes_read,
            fscas_read_calls: counters.fscas_read_calls,
            admitted_slots: ledger.admitted_slots(),
            zero_forbidden_work: counters.has_zero_forbidden_work(),
        }
    }

    pub fn valid_locator_binding_mismatches_v1(root: &Path) -> LocatorBindingObservationV1 {
        fs::create_dir_all(root).expect("create locator-binding fixture parent");
        let mut observation = LocatorBindingObservationV1::default();

        for (index, (name, field)) in [("catalog-binding", 80..88), ("entry-binding", 112..116)]
            .into_iter()
            .enumerate()
        {
            let case_root = root.join(name);
            let cas = FsCasV1::create_new(&case_root).expect("create locator-binding root");
            let shared = closure_object(5, b"authenticated-locator-binding");
            let shared_id = publication_object_id(&shared).expect("derive locator object id");
            let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
            let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
            let mut counters = OperationCountersV1::default();
            let (mut winner, _) = build_publication_pack_raw(
                &cas,
                &[shared.as_slice()],
                &ledger,
                &mut counters,
                &mut scratch,
            )
            .expect("build locator-binding incumbent");
            let mut spool = PublicationSpool::default();
            cas.admit_pack(
                &mut winner,
                &mut spool,
                &ledger,
                &mut counters,
                &mut scratch,
            )
            .expect("install locator-binding incumbent");

            let locator = publication_locator_path(&case_root, shared_id);
            let original = make_owner_writable(&locator);
            let mut bytes = fs::read(&locator).expect("read locator binding");
            if field.end - field.start == 8 {
                let value = u64::from_be_bytes(
                    bytes[field.clone()]
                        .try_into()
                        .expect("locator u64 binding"),
                );
                bytes[field].copy_from_slice(&value.checked_add(1).unwrap().to_be_bytes());
            } else {
                let value = u32::from_be_bytes(
                    bytes[field.clone()]
                        .try_into()
                        .expect("locator u32 binding"),
                );
                bytes[field].copy_from_slice(&value.checked_add(1).unwrap().to_be_bytes());
            }
            fs::write(&locator, bytes).expect("write locator binding");
            fs::set_permissions(&locator, original).expect("restore locator permissions");

            let read_error = cas
                .occupied_private_v1()
                .and_then(|mut occupied| occupied.occupied_len_typed_v1(shared_id))
                .err();
            let extra = closure_object(5, b"new-candidate-object");
            let mut candidate_counters = OperationCountersV1::default();
            let (mut candidate, _) = build_publication_pack_raw(
                &cas,
                &[shared.as_slice(), extra.as_slice()],
                &ledger,
                &mut candidate_counters,
                &mut scratch,
            )
            .expect("build locator-binding candidate");
            let admission_error = cas
                .admit_pack(
                    &mut candidate,
                    &mut spool,
                    &ledger,
                    &mut candidate_counters,
                    &mut scratch,
                )
                .err();

            observation.binding_cases[index] = LocatorBindingCaseObservationV1 {
                read_error: read_error.map(publication_error),
                admission_error: admission_error.map(publication_error),
                admitted_slots: ledger.admitted_slots(),
                zero_forbidden_work: candidate_counters.has_zero_forbidden_work(),
            };
        }

        let case_root = root.join("equal-carrier-binding");
        let cas = FsCasV1::create_new(&case_root).expect("create equal-carrier root");
        let shared = closure_object(5, b"equal-carrier-locator-binding");
        let shared_id = publication_object_id(&shared).expect("derive equal-carrier object id");
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut counters = OperationCountersV1::default();
        let mut spool = PublicationSpool::default();
        let (mut winner, _) = build_publication_pack_raw(
            &cas,
            &[shared.as_slice()],
            &ledger,
            &mut counters,
            &mut scratch,
        )
        .expect("build equal-carrier incumbent");
        cas.admit_pack(
            &mut winner,
            &mut spool,
            &ledger,
            &mut counters,
            &mut scratch,
        )
        .expect("install equal-carrier incumbent");
        let locator = publication_locator_path(&case_root, shared_id);
        let original = make_owner_writable(&locator);
        let mut bytes = fs::read(&locator).expect("read equal-carrier locator");
        let object_len = u32::from_be_bytes(bytes[112..116].try_into().unwrap());
        bytes[112..116].copy_from_slice(&object_len.checked_add(1).unwrap().to_be_bytes());
        fs::write(&locator, bytes).expect("write equal-carrier locator");
        fs::set_permissions(&locator, original).expect("restore equal-carrier permissions");
        let mut candidate_counters = OperationCountersV1::default();
        let (mut candidate, _) = build_publication_pack_raw(
            &cas,
            &[shared.as_slice()],
            &ledger,
            &mut candidate_counters,
            &mut scratch,
        )
        .expect("build equal-carrier candidate");
        let admission_error = cas
            .admit_pack(
                &mut candidate,
                &mut spool,
                &ledger,
                &mut candidate_counters,
                &mut scratch,
            )
            .err();
        observation.reuse_error = admission_error.map(publication_error);
        observation.reuse_admitted_slots = ledger.admitted_slots();
        observation.reuse_zero_forbidden_work = candidate_counters.has_zero_forbidden_work();
        observation
    }

    pub fn unequal_incumbent_bytes_v1(root: &Path) -> FaultObservationV1 {
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut counters = OperationCountersV1::default();
        let mut source_bytes_read = 0;
        let mut incumbent_preserved = false;
        let mut loser_locator_absent = false;
        let error = (|| -> Result<(), FsCasErrorV1> {
            let cas = FsCasV1::create_new(root)?;
            let shared = closure_object(5, &[0x71; 4096]);
            let shared_id = publication_object_id(&shared)?;
            let winner_only = closure_object(5, b"winner-only");
            let loser_only = closure_object(5, b"loser-only");
            let loser_id = publication_object_id(&loser_only)?;
            let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
            let mut winner_counters = OperationCountersV1::default();
            let (mut winner, _) = build_publication_pack_raw(
                &cas,
                &[shared.as_slice(), winner_only.as_slice()],
                &ledger,
                &mut winner_counters,
                &mut scratch,
            )?;
            let mut spool = PublicationSpool::default();
            cas.admit_pack(
                &mut winner,
                &mut spool,
                &ledger,
                &mut winner_counters,
                &mut scratch,
            )?;

            let marker = fs::read(publication_locator_path(root, shared_id))
                .map_err(|_| FsCasErrorV1::Io)?;
            let object_offset = u64::from_be_bytes(marker[104..112].try_into().unwrap()) + 4;
            let object_len = u32::from_be_bytes(marker[112..116].try_into().unwrap());
            let carrier = one_entry(root, "carriers");
            let original = make_owner_writable(&carrier);
            let mut bytes = fs::read(&carrier).map_err(|_| FsCasErrorV1::Io)?;
            let corrupt_at = usize::try_from(object_offset + u64::from(object_len) - 1)
                .map_err(|_| FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
            bytes[corrupt_at] ^= 0xff;
            fs::write(&carrier, bytes).map_err(|_| FsCasErrorV1::Io)?;
            fs::set_permissions(&carrier, original).map_err(|_| FsCasErrorV1::Io)?;

            let (mut candidate, source_read) = build_publication_pack_raw(
                &cas,
                &[shared.as_slice(), loser_only.as_slice()],
                &ledger,
                &mut counters,
                &mut scratch,
            )?;
            source_bytes_read = source_read;
            let result = cas
                .admit_pack(
                    &mut candidate,
                    &mut spool,
                    &ledger,
                    &mut counters,
                    &mut scratch,
                )
                .map(|_| ());
            loser_locator_absent = !publication_locator_path(root, loser_id).exists();
            incumbent_preserved = directory_entries(root, "preparation") == Some(0)
                && directory_entries(root, "carriers") == Some(1)
                && directory_entries(root, "catalog") == Some(1)
                && directory_entries(root, "objects") == Some(2)
                && loser_locator_absent;
            result
        })()
        .err();
        let mut observation = fault_observation(
            root,
            error,
            &counters,
            &ledger,
            source_bytes_read,
            0,
            0,
            incumbent_preserved,
        );
        observation.loser_locator_absent = loser_locator_absent;
        observation
    }

    pub fn malformed_object_locator_v1(root: &Path) -> FaultObservationV1 {
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut counters = OperationCountersV1::default();
        let mut source_bytes_read = 0;
        let mut incumbent_preserved = false;
        let mut owner_handle_invalidated = false;
        let mut reopen_invalidated = false;
        let error = (|| -> Result<(), FsCasErrorV1> {
            let cas = FsCasV1::create_new(root)?;
            let (version, root_object, version_id, _) = empty_closure_objects();
            let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
            let mut winner_counters = OperationCountersV1::default();
            let (mut winner, _) = build_publication_pack_raw(
                &cas,
                &[version.as_slice(), root_object.as_slice()],
                &ledger,
                &mut winner_counters,
                &mut scratch,
            )?;
            let mut spool = PublicationSpool::default();
            cas.admit_pack(
                &mut winner,
                &mut spool,
                &ledger,
                &mut winner_counters,
                &mut scratch,
            )?;
            let locator = publication_locator_path(root, version_id);
            let original = make_owner_writable(&locator);
            fs::write(&locator, b"truncated").map_err(|_| FsCasErrorV1::Io)?;
            fs::set_permissions(&locator, original).map_err(|_| FsCasErrorV1::Io)?;

            let loser = closure_object(5, b"loser");
            let loser_id = publication_object_id(&loser)?;
            let (mut candidate, source_read) = build_publication_pack_raw(
                &cas,
                &[version.as_slice(), loser.as_slice()],
                &ledger,
                &mut counters,
                &mut scratch,
            )?;
            source_bytes_read = source_read;
            let result = cas
                .admit_pack(
                    &mut candidate,
                    &mut spool,
                    &ledger,
                    &mut counters,
                    &mut scratch,
                )
                .map(|_| ());
            incumbent_preserved = directory_entries(root, "preparation") == Some(0)
                && directory_entries(root, "carriers") == Some(1)
                && directory_entries(root, "catalog") == Some(1)
                && !publication_locator_path(root, loser_id).exists();
            owner_handle_invalidated =
                matches!(cas.begin_private_pack(), Err(FsCasErrorV1::Invalidated));
            reopen_invalidated =
                matches!(FsCasV1::open_existing(root), Err(FsCasErrorV1::Invalidated));
            result
        })()
        .err();
        let mut observation = fault_observation(
            root,
            error,
            &counters,
            &ledger,
            source_bytes_read,
            0,
            0,
            incumbent_preserved,
        );
        observation.owner_handle_invalidated = owner_handle_invalidated;
        observation.reopen_invalidated = reopen_invalidated;
        observation
    }

    #[cfg(unix)]
    struct ReplaceLocatorAfterComparisonControl {
        locator: PathBuf,
        displaced: PathBuf,
        injected: bool,
    }

    #[cfg(unix)]
    impl FsCasControlV1 for ReplaceLocatorAfterComparisonControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary == FsCasBoundaryV1::AfterObjectComparisonWindow && !self.injected {
                fs::rename(&self.locator, &self.displaced)
                    .expect("displace compared semantic locator");
                fs::write(&self.locator, [0_u8; PERSISTENT_LOCATOR_BYTES_V1])
                    .expect("replace compared semantic locator");
                self.injected = true;
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    struct InstallMalformedCatalogControl {
        root: PathBuf,
        injected: bool,
    }

    impl FsCasControlV1 for InstallMalformedCatalogControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary == FsCasBoundaryV1::BeforeCatalogPublication && !self.injected {
                let carrier = one_entry(&self.root, "carriers");
                fs::write(
                    self.root.join("catalog").join(carrier.file_name().unwrap()),
                    [0_u8; CATALOG_MARKER_BYTES],
                )
                .expect("install malformed semantic catalog marker");
                self.injected = true;
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    struct BreakCatalogAtPublicationControl {
        catalog: PathBuf,
        injected: bool,
    }

    impl FsCasControlV1 for BreakCatalogAtPublicationControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary == FsCasBoundaryV1::BeforeCatalogPublication && !self.injected {
                fs::remove_dir(&self.catalog).expect("remove semantic catalog directory");
                fs::write(&self.catalog, b"injected-not-a-directory")
                    .expect("replace semantic catalog directory");
                self.injected = true;
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    struct InstallUnequalCatalogControl {
        root: PathBuf,
        bytes: Vec<u8>,
        bind_candidate_id: bool,
        injected: bool,
    }

    impl FsCasControlV1 for InstallUnequalCatalogControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary != FsCasBoundaryV1::BeforeCatalogPublication || self.injected {
                return;
            }
            let carrier = one_entry(&self.root, "carriers");
            if self.bind_candidate_id {
                let name = carrier.file_name().unwrap().to_str().unwrap();
                for (index, slot) in self.bytes[8..40].iter_mut().enumerate() {
                    *slot = u8::from_str_radix(&name[index * 2..index * 2 + 2], 16)
                        .expect("semantic carrier id");
                }
            }
            fs::write(
                self.root.join("catalog").join(carrier.file_name().unwrap()),
                &self.bytes,
            )
            .expect("install unequal semantic catalog marker");
            self.injected = true;
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    #[cfg(unix)]
    pub fn post_comparison_locator_replacement_v1(root: &Path) -> FaultObservationV1 {
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut counters = OperationCountersV1::default();
        let mut source_bytes_read = 0;
        let mut injected = false;
        let mut owner_handle_invalidated = false;
        let mut stale_handle_invalidated = false;
        let mut reopen_invalidated = false;
        let error = (|| -> Result<(), FsCasErrorV1> {
            let cas = FsCasV1::create_new(root)?;
            let stale = FsCasV1::open_existing(root)?;
            let shared = closure_object(5, b"shared-complete-object");
            let winner_only = closure_object(5, b"winner-only-object");
            let candidate_only = closure_object(5, b"candidate-only-object");
            let shared_locator = publication_locator_path(root, publication_object_id(&shared)?);
            let displaced = root.join("displaced-shared-locator");
            let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
            let mut spool = PublicationSpool::default();
            let mut winner_counters = OperationCountersV1::default();
            let (mut winner, _) = build_publication_pack_raw(
                &cas,
                &[shared.as_slice(), winner_only.as_slice()],
                &ledger,
                &mut winner_counters,
                &mut scratch,
            )?;
            cas.admit_pack(
                &mut winner,
                &mut spool,
                &ledger,
                &mut winner_counters,
                &mut scratch,
            )?;
            let (mut candidate, source_read) = build_publication_pack_raw(
                &cas,
                &[shared.as_slice(), candidate_only.as_slice()],
                &ledger,
                &mut counters,
                &mut scratch,
            )?;
            source_bytes_read = source_read;
            let mut control = ReplaceLocatorAfterComparisonControl {
                locator: shared_locator.clone(),
                displaced: displaced.clone(),
                injected: false,
            };
            let result = cas
                .admit_pack_controlled(
                    &mut candidate,
                    &mut spool,
                    &ledger,
                    &mut counters,
                    &mut scratch,
                    &mut control,
                )
                .map(|_| ());
            injected = control.injected;
            if injected {
                fs::remove_file(&shared_locator).map_err(|_| FsCasErrorV1::Io)?;
                fs::rename(displaced, shared_locator).map_err(|_| FsCasErrorV1::Io)?;
            }
            owner_handle_invalidated = matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated));
            stale_handle_invalidated = matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated));
            reopen_invalidated =
                matches!(FsCasV1::open_existing(root), Err(FsCasErrorV1::Invalidated));
            result
        })()
        .err();
        let mut observation = fault_observation(
            root,
            error,
            &counters,
            &ledger,
            source_bytes_read,
            counters.incumbent_comparison_bytes,
            counters.incumbent_comparison_windows,
            directory_entries(root, "catalog") == Some(1)
                && directory_entries(root, "carriers") == Some(1),
        );
        observation.fault_injected = injected;
        observation.owner_handle_invalidated = owner_handle_invalidated;
        observation.stale_handle_invalidated = stale_handle_invalidated;
        observation.reopen_invalidated = reopen_invalidated;
        observation
    }

    pub fn atomic_catalog_malformed_occupant_v1(root: &Path) -> FaultObservationV1 {
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut counters = OperationCountersV1::default();
        let mut source_bytes_read = 0;
        let mut injected = false;
        let error = (|| -> Result<(), FsCasErrorV1> {
            let cas = FsCasV1::create_new(root)?;
            let object = closure_object(5, b"atomic-catalog");
            let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
            let (mut pack, source_read) = build_publication_pack_raw(
                &cas,
                &[object.as_slice()],
                &ledger,
                &mut counters,
                &mut scratch,
            )?;
            source_bytes_read = source_read;
            let mut spool = PublicationSpool::default();
            let mut control = InstallMalformedCatalogControl {
                root: root.to_path_buf(),
                injected: false,
            };
            let result = cas
                .admit_pack_controlled(
                    &mut pack,
                    &mut spool,
                    &ledger,
                    &mut counters,
                    &mut scratch,
                    &mut control,
                )
                .map(|_| ());
            injected = control.injected;
            result
        })()
        .err();
        let marker_preserved = directory_entries(root, "catalog") == Some(1)
            && fs::read(one_entry(root, "catalog")).ok().as_deref()
                == Some(&[0_u8; CATALOG_MARKER_BYTES]);
        let mut observation = fault_observation(
            root,
            error,
            &counters,
            &ledger,
            source_bytes_read,
            0,
            0,
            marker_preserved,
        );
        observation.fault_injected = injected;
        observation
    }

    pub fn atomic_catalog_classification_v1(
        root: &Path,
        case: AtomicCatalogCaseV1,
    ) -> FaultObservationV1 {
        fs::create_dir_all(root).expect("create atomic-catalog semantic root");
        let donor_root = root.join("donor");
        let candidate_root = root.join("candidate");
        let donor = FsCasV1::create_new(&donor_root).expect("create atomic-catalog donor");
        let donor_object = closure_object(5, b"canonical unequal catalog donor");
        let donor_ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut donor_counters = OperationCountersV1::default();
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        let (mut donor_pack, _) = build_publication_pack_raw(
            &donor,
            &[donor_object.as_slice()],
            &donor_ledger,
            &mut donor_counters,
            &mut scratch,
        )
        .expect("build atomic-catalog donor");
        let mut spool = PublicationSpool::default();
        let donor_installed = donor
            .admit_pack(
                &mut donor_pack,
                &mut spool,
                &donor_ledger,
                &mut donor_counters,
                &mut scratch,
            )
            .expect("install atomic-catalog donor")
            .outcome()
            == FsPackAdmissionOutcomeV1::Installed;
        let unequal_marker =
            fs::read(one_entry(&donor_root, "catalog")).expect("read atomic-catalog donor marker");
        let incumbent_marker_has_canonical_size = unequal_marker.len() == CATALOG_MARKER_BYTES;
        let incumbent_pack_len = u64::from_be_bytes(
            unequal_marker[40..48]
                .try_into()
                .expect("atomic-catalog donor pack length"),
        );

        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut counters = OperationCountersV1::default();
        let mut source_bytes_read = 0;
        let mut injected = false;
        let mut preserved = false;
        let mut candidate_pack_len = 0;
        let error = (|| -> Result<(), FsCasErrorV1> {
            let cas = FsCasV1::create_new(&candidate_root)?;
            let object = closure_object(5, b"candidate with a distinct sealed pack");
            let (mut pack, source_read) = build_publication_pack_raw(
                &cas,
                &[object.as_slice()],
                &ledger,
                &mut counters,
                &mut scratch,
            )?;
            source_bytes_read = source_read;
            candidate_pack_len = pack.len().expect("atomic-catalog candidate pack length");
            let mut control = InstallUnequalCatalogControl {
                root: candidate_root.clone(),
                bytes: unequal_marker,
                bind_candidate_id: case == AtomicCatalogCaseV1::SameIdUnequal,
                injected: false,
            };
            let result = cas
                .admit_pack_controlled(
                    &mut pack,
                    &mut spool,
                    &ledger,
                    &mut counters,
                    &mut scratch,
                    &mut control,
                )
                .map(|_| ());
            injected = control.injected;
            preserved = fs::read(one_entry(&candidate_root, "catalog")).ok() == Some(control.bytes);
            result
        })()
        .err();
        let mut observation = fault_observation(
            &candidate_root,
            error,
            &counters,
            &ledger,
            source_bytes_read,
            0,
            0,
            preserved,
        );
        observation.fault_injected = injected;
        observation.donor_installed = donor_installed;
        observation.incumbent_marker_has_canonical_size = incumbent_marker_has_canonical_size;
        observation.incumbent_pack_len = incumbent_pack_len;
        observation.candidate_pack_len = candidate_pack_len;
        observation
    }

    pub fn every_fresh_admission_boundary_v1(root: &Path) -> BoundaryMatrixObservationV1 {
        fs::create_dir_all(root).expect("create fresh-boundary semantic root");
        let mut observation = BoundaryMatrixObservationV1::default();
        for (index, boundary) in [
            FsCasBoundaryV1::BeforeCandidateValidation,
            FsCasBoundaryV1::AfterCandidateValidation,
            FsCasBoundaryV1::BeforeCarrierInstall,
            FsCasBoundaryV1::AfterCarrierInstall,
            FsCasBoundaryV1::AfterCarrierValidation,
            FsCasBoundaryV1::AfterCarrierMadeImmutable,
            FsCasBoundaryV1::BeforeObjectLocatorPublication,
            FsCasBoundaryV1::AfterObjectLocatorPublication,
            FsCasBoundaryV1::BeforeCatalogPublication,
            FsCasBoundaryV1::AfterCatalogPublication,
        ]
        .into_iter()
        .enumerate()
        {
            let case_root = root.join(format!("boundary-{index}"));
            let cas = FsCasV1::create_new(&case_root).expect("create fresh-boundary case");
            let object = closure_object(5, b"boundary-fault");
            let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
            let mut counters = OperationCountersV1::default();
            let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
            let (mut pack, _) = build_publication_pack_raw(
                &cas,
                &[object.as_slice()],
                &ledger,
                &mut counters,
                &mut scratch,
            )
            .expect("build fresh-boundary pack");
            let pack_len = pack.len().expect("fresh-boundary pack length");
            let mut spool = PublicationSpool::default();
            let mut control = FaultControl::cancellation(boundary);
            let error = cas
                .admit_pack_controlled(
                    &mut pack,
                    &mut spool,
                    &ledger,
                    &mut counters,
                    &mut scratch,
                    &mut control,
                )
                .err();
            let after_catalog = boundary == FsCasBoundaryV1::AfterCatalogPublication;
            let expected_namespace = directory_entries(&case_root, "preparation") == Some(0)
                && directory_entries(&case_root, "closures") == Some(0)
                && directory_entries(&case_root, "carriers") == Some(u64::from(after_catalog))
                && directory_entries(&case_root, "catalog") == Some(u64::from(after_catalog))
                && directory_entries(&case_root, "objects") == Some(u64::from(after_catalog));
            let expected_residue = if after_catalog {
                pack_len + PERSISTENT_LOCATOR_BYTES_V1 as u64 + CATALOG_MARKER_BYTES as u64
            } else {
                0
            };
            observation.cases[index] = BoundaryCaseObservationV1 {
                error: error.map(publication_error),
                after_catalog_publication: after_catalog,
                pack_len,
                preparation_entries: directory_entries(&case_root, "preparation").unwrap_or(0),
                closure_entries: directory_entries(&case_root, "closures").unwrap_or(0),
                carrier_entries: directory_entries(&case_root, "carriers").unwrap_or(0),
                catalog_entries: directory_entries(&case_root, "catalog").unwrap_or(0),
                object_entries: directory_entries(&case_root, "objects").unwrap_or(0),
                residue_bytes: counters.unreachable_installed_residue_bytes,
                expected_residue_bytes: expected_residue,
                bytes_written: counters.fscas_bytes_written,
                admitted_slots: ledger.admitted_slots(),
                closure_fences: counters.closure_fences,
                open_file_handles_high_water: counters.layerfs_open_file_handles_high_water,
                incumbent_preserved: expected_namespace,
                zero_forbidden_work: counters.has_zero_forbidden_work(),
            };
            observation.case_count += 1;
        }
        observation
    }

    pub fn every_incumbent_boundary_v1(root: &Path) -> BoundaryMatrixObservationV1 {
        let cas = FsCasV1::create_new(root).expect("create incumbent-boundary root");
        let object = closure_object(5, &[0x3c; 32_768]);
        let objects = [object.as_slice()];
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut spool = PublicationSpool::default();
        let mut incumbent_counters = OperationCountersV1::default();
        let (mut incumbent, _) = build_publication_pack_raw(
            &cas,
            &objects,
            &ledger,
            &mut incumbent_counters,
            &mut scratch,
        )
        .expect("build incumbent-boundary winner");
        cas.admit_pack(
            &mut incumbent,
            &mut spool,
            &ledger,
            &mut incumbent_counters,
            &mut scratch,
        )
        .expect("install incumbent-boundary winner");
        let carrier = one_entry(root, "carriers");
        let original = fs::read(&carrier).expect("read incumbent-boundary carrier");
        let mut observation = BoundaryMatrixObservationV1::default();
        for (index, boundary) in [
            FsCasBoundaryV1::BeforeIncumbentMarkerRead,
            FsCasBoundaryV1::AfterIncumbentMarkerRead,
            FsCasBoundaryV1::AfterIncumbentValidation,
            FsCasBoundaryV1::BeforeIncumbentComparisonWindow,
            FsCasBoundaryV1::AfterIncumbentComparisonWindow,
            FsCasBoundaryV1::BeforeObjectLocatorRead,
            FsCasBoundaryV1::AfterObjectLocatorRead,
            FsCasBoundaryV1::AfterObjectIncumbentValidation,
        ]
        .into_iter()
        .enumerate()
        {
            let mut counters = OperationCountersV1::default();
            let (mut candidate, _) =
                build_publication_pack_raw(&cas, &objects, &ledger, &mut counters, &mut scratch)
                    .expect("build incumbent-boundary candidate");
            let mut control = FaultControl::cancellation(boundary);
            let error = cas
                .admit_pack_controlled(
                    &mut candidate,
                    &mut spool,
                    &ledger,
                    &mut counters,
                    &mut scratch,
                    &mut control,
                )
                .err();
            observation.cases[index] = BoundaryCaseObservationV1 {
                error: error.map(publication_error),
                after_catalog_publication: false,
                pack_len: 0,
                preparation_entries: directory_entries(root, "preparation").unwrap_or(0),
                closure_entries: directory_entries(root, "closures").unwrap_or(0),
                carrier_entries: directory_entries(root, "carriers").unwrap_or(0),
                catalog_entries: directory_entries(root, "catalog").unwrap_or(0),
                object_entries: directory_entries(root, "objects").unwrap_or(0),
                residue_bytes: counters.unreachable_installed_residue_bytes,
                expected_residue_bytes: 0,
                bytes_written: counters.fscas_bytes_written,
                admitted_slots: ledger.admitted_slots(),
                closure_fences: counters.closure_fences,
                open_file_handles_high_water: counters.layerfs_open_file_handles_high_water,
                incumbent_preserved: fs::read(&carrier).ok().as_deref()
                    == Some(original.as_slice()),
                zero_forbidden_work: counters.has_zero_forbidden_work(),
            };
            observation.case_count += 1;
        }
        observation
    }

    pub fn catalog_publication_io_failure_v1(root: &Path) -> FaultObservationV1 {
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut counters = OperationCountersV1::default();
        let mut source_bytes_read = 0;
        let mut injected = false;
        let error = (|| -> Result<(), FsCasErrorV1> {
            let cas = FsCasV1::create_new(root)?;
            let object = closure_object(5, b"catalog-fault");
            let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
            let (mut pack, source_read) = build_publication_pack_raw(
                &cas,
                &[object.as_slice()],
                &ledger,
                &mut counters,
                &mut scratch,
            )?;
            source_bytes_read = source_read;
            let mut spool = PublicationSpool::default();
            let mut control = BreakCatalogAtPublicationControl {
                catalog: root.join("catalog"),
                injected: false,
            };
            let result = cas
                .admit_pack_controlled(
                    &mut pack,
                    &mut spool,
                    &ledger,
                    &mut counters,
                    &mut scratch,
                    &mut control,
                )
                .map(|_| ());
            injected = control.injected;
            result
        })()
        .err();
        let mut observation = fault_observation(
            root,
            error,
            &counters,
            &ledger,
            source_bytes_read,
            0,
            0,
            false,
        );
        observation.fault_injected = injected;
        observation
    }

    pub fn malformed_carrier_directory_v1(root: &Path) -> FaultObservationV1 {
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut counters = OperationCountersV1::default();
        let mut source_bytes_read = 0;
        let error = (|| -> Result<(), FsCasErrorV1> {
            let cas = FsCasV1::create_new(root)?;
            let object = closure_object(5, b"unsupported");
            let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
            let (mut pack, source_read) = build_publication_pack_raw(
                &cas,
                &[object.as_slice()],
                &ledger,
                &mut counters,
                &mut scratch,
            )?;
            source_bytes_read = source_read;
            fs::remove_dir(root.join("carriers")).map_err(|_| FsCasErrorV1::Io)?;
            fs::write(root.join("carriers"), b"not-a-private-directory")
                .map_err(|_| FsCasErrorV1::Io)?;
            let mut spool = PublicationSpool::default();
            cas.admit_pack(&mut pack, &mut spool, &ledger, &mut counters, &mut scratch)
                .map(|_| ())
        })()
        .err();
        fault_observation(
            root,
            error,
            &counters,
            &ledger,
            source_bytes_read,
            0,
            0,
            false,
        )
    }

    #[cfg(unix)]
    pub fn symlinked_parent_namespace_creation_v1(root: &Path) -> NamespaceCreationObservationV1 {
        use std::os::unix::fs::symlink;

        fs::create_dir(root).expect("create symlink-parent semantic root");
        let actual = root.join("actual");
        fs::create_dir(&actual).expect("create symlink-parent actual directory");
        let linked = root.join("linked");
        symlink(&actual, &linked).expect("create symlink-parent link");
        let error = FsCasV1::create_new(&linked.join("cas"))
            .err()
            .map(publication_error);
        NamespaceCreationObservationV1 {
            error,
            namespace_absent: !actual.join("cas").exists(),
        }
    }

    pub fn complete_carrier_backed_closure_v1(root: &Path) -> CompleteClosureObservationV1 {
        let cas = FsCasV1::create_new(root).expect("create complete-closure root");
        let (version, root_object, version_id, root_id) = empty_closure_objects();
        let objects = [version.as_slice(), root_object.as_slice()];
        let closure_objects = [
            (version_id, version.clone()),
            (root_id, root_object.clone()),
        ];
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut counters = OperationCountersV1::default();
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        let (mut pack, _) =
            build_publication_pack_raw(&cas, &objects, &ledger, &mut counters, &mut scratch)
                .expect("build complete-closure pack");
        let mut spool = PublicationSpool::default();
        let admission = cas
            .admit_pack(&mut pack, &mut spool, &ledger, &mut counters, &mut scratch)
            .expect("install complete-closure pack");
        let installed = admission.outcome() == super::FsPackAdmissionOutcomeV1::Installed;
        let pack_len = admission.sealed().pack_len();
        let invisible_before_validation = directory_entries(root, "closures") == Some(0);
        let before_bytes = counters.fscas_bytes_read;
        let before_calls = counters.fscas_read_calls;

        let mut closure = ClosureSource {
            objects: &closure_objects,
        };
        let mut operation = cas
            .begin_closure_operation()
            .expect("begin complete-closure operation");
        let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut source = [0_u8; 32_768];
        let mut ring = [0_u8; 32_768];
        let mut traversal = [0_u8; 1];
        let (admitted, mut capability) = cas
            .admit_complete_closure(
                &mut operation,
                &mut closure,
                version_id,
                &ledger,
                &mut counters,
                AdmissionBuffersV1::new(
                    &mut incoming,
                    &mut occupied,
                    &mut source,
                    &mut ring,
                    &mut traversal,
                ),
            )
            .expect("admit complete closure");
        let version_record_matches = admitted.version_record() == version_id;
        let object_count = admitted.object_count();
        let created_count = admitted.created_count();
        let reused_count = admitted.reused_count();
        let capability_version_matches = capability.version_record() == Ok(version_id);
        let capability_object_count = capability.object_count().unwrap_or(0);
        cas.consume_validated_closure_for_handoff(&mut operation, &mut capability)
            .expect("consume complete closure");

        CompleteClosureObservationV1 {
            installed,
            pack_len,
            invisible_before_validation,
            version_record_matches,
            object_count,
            created_count,
            reused_count,
            capability_version_matches,
            capability_object_count,
            closure_entries: directory_entries(root, "closures").unwrap_or(0),
            closure_fences: counters.closure_fences,
            bytes_read: counters.bytes_read,
            fscas_bytes_read_delta: counters.fscas_bytes_read.saturating_sub(before_bytes),
            fscas_read_calls_delta: counters.fscas_read_calls.saturating_sub(before_calls),
            admitted_slots: ledger.admitted_slots(),
            zero_forbidden_work: counters.has_zero_forbidden_work(),
        }
    }

    fn closure_counter_failure_v1(
        root: &Path,
        read_counter: bool,
    ) -> ClosureCapabilityFailureObservationV1 {
        let cas = FsCasV1::create_new(root).expect("create closure-counter root");
        let (version, root_object, version_id, root_id) = empty_closure_objects();
        let objects = [version.as_slice(), root_object.as_slice()];
        let closure_objects = [
            (version_id, version.clone()),
            (root_id, root_object.clone()),
        ];
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut install_counters = OperationCountersV1::default();
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        let (mut pack, _) = build_publication_pack_raw(
            &cas,
            &objects,
            &ledger,
            &mut install_counters,
            &mut scratch,
        )
        .expect("build closure-counter pack");
        let mut spool = PublicationSpool::default();
        cas.admit_pack(
            &mut pack,
            &mut spool,
            &ledger,
            &mut install_counters,
            &mut scratch,
        )
        .expect("install closure-counter pack");
        let mut counters = if read_counter {
            OperationCountersV1 {
                fscas_bytes_read: u64::MAX,
                ..OperationCountersV1::default()
            }
        } else {
            install_counters
        };
        if !read_counter {
            counters.closure_fences = u64::MAX;
        }

        let mut closure = ClosureSource {
            objects: &closure_objects,
        };
        let mut operation = cas
            .begin_closure_operation()
            .expect("begin closure-counter operation");
        let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut source = [0_u8; 32_768];
        let mut ring = [0_u8; 32_768];
        let mut traversal = [0_u8; 1];
        let error = cas
            .admit_complete_closure(
                &mut operation,
                &mut closure,
                version_id,
                &ledger,
                &mut counters,
                AdmissionBuffersV1::new(
                    &mut incoming,
                    &mut occupied,
                    &mut source,
                    &mut ring,
                    &mut traversal,
                ),
            )
            .err();
        ClosureCapabilityFailureObservationV1 {
            error,
            closure_entries: directory_entries(root, "closures").unwrap_or(0),
            closure_fences: counters.closure_fences,
            fscas_bytes_read: counters.fscas_bytes_read,
            fscas_read_calls: counters.fscas_read_calls,
            admitted_slots: ledger.admitted_slots(),
            zero_forbidden_work: counters.has_zero_forbidden_work(),
        }
    }

    pub fn closure_fence_counter_overflow_v1(root: &Path) -> ClosureCapabilityFailureObservationV1 {
        closure_counter_failure_v1(root, false)
    }

    pub fn closure_read_counter_overflow_v1(root: &Path) -> ClosureCapabilityFailureObservationV1 {
        closure_counter_failure_v1(root, true)
    }

    pub fn closure_capability_binding_v1(root: &Path) -> ClosureBindingObservationV1 {
        let other_root = root.with_extension("other-fscas");
        let cas = FsCasV1::create_new(root).expect("create closure-binding root");
        let other = FsCasV1::create_new(&other_root).expect("create other closure-binding root");
        let (version, root_object, version_id, root_id) = empty_closure_objects();
        let closure_objects = [
            (version_id, version.clone()),
            (root_id, root_object.clone()),
        ];
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut counters = OperationCountersV1::default();
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        let (mut pack, _) = build_publication_pack_raw(
            &cas,
            &[version.as_slice(), root_object.as_slice()],
            &ledger,
            &mut counters,
            &mut scratch,
        )
        .expect("build closure-binding pack");
        let mut spool = PublicationSpool::default();
        cas.admit_pack(&mut pack, &mut spool, &ledger, &mut counters, &mut scratch)
            .expect("install closure-binding pack");

        let mut operation_a = cas
            .begin_closure_operation()
            .expect("begin closure-binding operation a");
        let mut closure_a = ClosureSource {
            objects: &closure_objects,
        };
        let mut incoming_a = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut occupied_a = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut source_a = [0_u8; 32_768];
        let mut ring_a = [0_u8; 32_768];
        let mut traversal_a = [0_u8; 1];
        let (_, mut capability_a) = cas
            .admit_complete_closure(
                &mut operation_a,
                &mut closure_a,
                version_id,
                &ledger,
                &mut counters,
                AdmissionBuffersV1::new(
                    &mut incoming_a,
                    &mut occupied_a,
                    &mut source_a,
                    &mut ring_a,
                    &mut traversal_a,
                ),
            )
            .expect("admit closure-binding operation a");

        let mut operation_b = cas
            .begin_closure_operation()
            .expect("begin closure-binding operation b");
        let mut closure_b = ClosureSource {
            objects: &closure_objects,
        };
        let mut incoming_b = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut occupied_b = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut source_b = [0_u8; 32_768];
        let mut ring_b = [0_u8; 32_768];
        let mut traversal_b = [0_u8; 1];
        let (_, mut capability_b) = cas
            .admit_complete_closure(
                &mut operation_b,
                &mut closure_b,
                version_id,
                &ledger,
                &mut counters,
                AdmissionBuffersV1::new(
                    &mut incoming_b,
                    &mut occupied_b,
                    &mut source_b,
                    &mut ring_b,
                    &mut traversal_b,
                ),
            )
            .expect("admit closure-binding operation b");

        let cross_fscas_error = other
            .consume_validated_closure_for_handoff(&mut operation_a, &mut capability_a)
            .err()
            .map(publication_error);
        let cross_operation_error = cas
            .consume_validated_closure_for_handoff(&mut operation_b, &mut capability_a)
            .err()
            .map(publication_error);
        cas.consume_validated_closure_for_handoff(&mut operation_a, &mut capability_a)
            .expect("consume closure-binding capability a");
        let replay_error = cas
            .consume_validated_closure_for_handoff(&mut operation_a, &mut capability_a)
            .err()
            .map(publication_error);
        cas.consume_validated_closure_for_handoff(&mut operation_b, &mut capability_b)
            .expect("consume closure-binding capability b");

        let doomed = closure_object(5, b"invalidate-issued-closure");
        let (mut doomed_pack, _) = build_publication_pack_raw(
            &cas,
            &[doomed.as_slice()],
            &ledger,
            &mut counters,
            &mut scratch,
        )
        .expect("build closure invalidation pack");
        let mut control = FaultControl::cancellation(FsCasBoundaryV1::AfterCarrierInstall)
            .with_cleanup_failure(FsCasCleanupTargetV1::Carrier);
        let raw_invalidation_error = cas
            .admit_pack_controlled(
                &mut doomed_pack,
                &mut spool,
                &ledger,
                &mut counters,
                &mut scratch,
                &mut control,
            )
            .err();
        let invalidation_terminal_failure = matches!(
            raw_invalidation_error,
            Some(FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::Core(CoreError::Cancelled),
                dominant: FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::Carrier),
            })
        );
        let invalidated_version_record =
            capability_b.version_record() == Err(FsCasErrorV1::Invalidated);
        let invalidated_object_count =
            capability_b.object_count() == Err(FsCasErrorV1::Invalidated);
        let invalidated_handoff = cas
            .consume_validated_closure_for_handoff(&mut operation_b, &mut capability_b)
            == Err(FsCasErrorV1::Invalidated);

        ClosureBindingObservationV1 {
            cross_fscas_error,
            cross_operation_error,
            replay_error,
            primary_closure_entries: directory_entries(root, "closures").unwrap_or(0),
            other_closure_entries: directory_entries(&other_root, "closures").unwrap_or(0),
            closure_fences: counters.closure_fences,
            invalidation_terminal_failure,
            invalidated_version_record,
            invalidated_object_count,
            invalidated_handoff,
            admitted_slots: ledger.admitted_slots(),
            zero_forbidden_work: counters.has_zero_forbidden_work(),
        }
    }

    fn installed_closure_failure_v1(root: &Path, fence_io: bool) -> ClosureFailureObservationV1 {
        let cas = FsCasV1::create_new(root).expect("create closure-failure root");
        let (version, root_object, version_id, root_id) = empty_closure_objects();
        let objects = [version.as_slice(), root_object.as_slice()];
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut counters = OperationCountersV1::default();
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        let (mut pack, _) =
            build_publication_pack_raw(&cas, &objects, &ledger, &mut counters, &mut scratch)
                .expect("build closure-failure pack");
        let mut spool = PublicationSpool::default();
        let admission = cas
            .admit_pack(&mut pack, &mut spool, &ledger, &mut counters, &mut scratch)
            .expect("install closure-failure pack");
        let pack_len = admission.sealed().pack_len();
        let record_count = admission.sealed().record_count();
        let closure_root = if fence_io {
            fs::remove_dir(root.join("closures")).expect("remove closure directory");
            fs::write(root.join("closures"), b"injected-not-a-directory")
                .expect("replace closure directory");
            root_object.clone()
        } else {
            let mut malformed = root_object.clone();
            *malformed.last_mut().expect("closure root byte") ^= 1;
            malformed
        };
        let closure_objects = [(version_id, version.clone()), (root_id, closure_root)];
        let mut closure = ClosureSource {
            objects: &closure_objects,
        };
        let mut operation = cas
            .begin_closure_operation()
            .expect("begin closure-failure operation");
        let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut source = [0_u8; 32_768];
        let mut ring = [0_u8; 32_768];
        let mut traversal = [0_u8; 1];
        let error = cas
            .admit_complete_closure(
                &mut operation,
                &mut closure,
                version_id,
                &ledger,
                &mut counters,
                AdmissionBuffersV1::new(
                    &mut incoming,
                    &mut occupied,
                    &mut source,
                    &mut ring,
                    &mut traversal,
                ),
            )
            .err();
        admission
            .record_later_unreachable_residue(&mut counters)
            .expect("record closure-failure residue");
        ClosureFailureObservationV1 {
            error,
            pack_len,
            record_count,
            residue_bytes: counters.unreachable_installed_residue_bytes,
            closure_entries: directory_entries(root, "closures").unwrap_or(0),
            closure_fences: counters.closure_fences,
            bytes_read: counters.bytes_read,
            publication_authority_dispatches: counters.publication_authority_dispatches,
            closure_path_is_file: root.join("closures").is_file(),
            admitted_slots: ledger.admitted_slots(),
            zero_forbidden_work: counters.has_zero_forbidden_work(),
        }
    }

    pub fn closure_validation_failure_v1(root: &Path) -> ClosureFailureObservationV1 {
        installed_closure_failure_v1(root, false)
    }

    pub fn closure_fence_io_failure_v1(root: &Path) -> ClosureFailureObservationV1 {
        installed_closure_failure_v1(root, true)
    }

    /// Run the real filesystem admission path and stop after a requested
    /// locator publication. The returned observations are immutable scalar
    /// facts suitable for integration assertions.
    pub fn cancel_after_locator_publication_v1(
        request: PublicationRequestV1<'_>,
    ) -> PublicationObservationV1 {
        let mut counters = OperationCountersV1::default();
        let mut publications = 0;
        let mut source_payload_bytes_read = 0;
        let error = (|| {
            let cas = FsCasV1::create_new(request.root).map_err(publication_error)?;
            let mut source =
                PublicationSource::new(request.objects).map_err(PublicationErrorV1::Core)?;
            let mut pack: FsPrivatePackV1 = cas.begin_private_pack().map_err(publication_error)?;
            let mut spool = PublicationSpool::default();
            let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
            let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
            build_dense_pack_v1(
                &mut source,
                &mut pack,
                &mut spool,
                &ledger,
                &mut counters,
                &mut scratch,
            )
            .map_err(PublicationErrorV1::Core)?;
            source_payload_bytes_read = source.payload_bytes_read;
            let mut control = PublicationControl::new(request.cancel_after_locator_publication);
            let result = cas
                .admit_pack_controlled(
                    &mut pack,
                    &mut spool,
                    &ledger,
                    &mut counters,
                    &mut scratch,
                    &mut control,
                )
                .map_err(publication_error);
            publications = control.publications;
            result.map(|_| ())
        })()
        .err();

        let preparation_entries = directory_entries(request.root, "preparation");
        let carrier_entries = directory_entries(request.root, "carriers");
        let object_entries = directory_entries(request.root, "objects");
        let catalog_entries = directory_entries(request.root, "catalog");
        PublicationObservationV1 {
            error,
            directories_observed: preparation_entries.is_some()
                && carrier_entries.is_some()
                && object_entries.is_some()
                && catalog_entries.is_some(),
            preparation_entries: preparation_entries.unwrap_or(0),
            carrier_entries: carrier_entries.unwrap_or(0),
            object_entries: object_entries.unwrap_or(0),
            catalog_entries: catalog_entries.unwrap_or(0),
            residue_bytes: counters.unreachable_installed_residue_bytes,
            bytes_written: counters.fscas_bytes_written,
            admitted_slots: counters.root_admission_active_slots_high_water,
            zero_forbidden_work: counters.has_zero_forbidden_work(),
            locator_publications: publications,
            source_payload_bytes_read,
        }
    }

    fn fault_observation(
        root: &Path,
        raw_error: Option<FsCasErrorV1>,
        counters: &OperationCountersV1,
        ledger: &ResourceLedgerV1,
        source_bytes_read: u64,
        incumbent_comparison_bytes: u64,
        incumbent_comparison_windows: u64,
        incumbent_preserved: bool,
    ) -> FaultObservationV1 {
        let error = raw_error.map(publication_error);
        FaultObservationV1 {
            error,
            preparation_entries: directory_entries(root, "preparation").unwrap_or(0),
            carrier_entries: directory_entries(root, "carriers").unwrap_or(0),
            object_entries: directory_entries(root, "objects").unwrap_or(0),
            catalog_entries: directory_entries(root, "catalog").unwrap_or(0),
            closure_entries: directory_entries(root, "closures").unwrap_or(0),
            residue_bytes: counters.unreachable_installed_residue_bytes,
            bytes_written: counters.fscas_bytes_written,
            admitted_slots: ledger.admitted_slots(),
            catalog_operations: counters.fscas_catalog_operations,
            closure_fences: counters.closure_fences,
            zero_forbidden_work: counters.has_zero_forbidden_work(),
            source_bytes_read,
            incumbent_comparison_bytes,
            incumbent_comparison_windows,
            incumbent_preserved,
            incumbent_identity: IncumbentIdentityObservationV1::Unavailable,
            fault_injected: false,
            cleanup_fault_injected: false,
            storage_bytes_request_matches_reservation: counters.storage_bytes_requested
                == counters.storage_bytes_reserved,
            storage_inodes_request_matches_reservation: counters.storage_inodes_requested
                == counters.storage_inodes_reserved,
            storage_bytes_terminal_sum_matches_reservation: counters
                .storage_bytes_released
                .checked_add(counters.storage_bytes_committed)
                .and_then(|value| value.checked_add(counters.storage_bytes_retained))
                == Some(counters.storage_bytes_reserved),
            storage_inodes_terminal_sum_matches_reservation: counters
                .storage_inodes_released
                .checked_add(counters.storage_inodes_committed)
                .and_then(|value| value.checked_add(counters.storage_inodes_retained))
                == Some(counters.storage_inodes_reserved),
            storage_bytes_retained: counters.storage_bytes_retained,
            storage_inodes_retained: counters.storage_inodes_retained,
            loser_locator_absent: false,
            owner_handle_invalidated: false,
            owner_private_invalidated: false,
            owner_occupied_invalidated: false,
            owner_closure_invalidated: false,
            stale_handle_invalidated: false,
            stale_private_invalidated: false,
            stale_occupied_invalidated: false,
            stale_closure_refused: false,
            reopen_invalidated: false,
            donor_installed: false,
            incumbent_marker_has_canonical_size: false,
            incumbent_pack_len: 0,
            candidate_pack_len: 0,
            expected_residue_bytes: 0,
            closure_payload_len: 0,
            filesystem_write_failure: matches!(
                raw_error,
                Some(FsCasErrorV1::Filesystem(
                    FsCasFilesystemFailureV1::WriteFailure
                ))
            ),
            catalog_path_is_file: root.join("catalog").is_file(),
            carrier_path_is_file: root.join("carriers").is_file(),
            malformed_locator_preserved: false,
            invalidated: root.join("invalidated").is_dir(),
            first_cause: raw_error.map(|error| publication_causes(error).0),
            dominant_cause: raw_error.map(|error| publication_causes(error).1),
        }
    }

    fn build_publication_pack(
        cas: &FsCasV1,
        objects: &[&[u8]],
        ledger: &ResourceLedgerV1,
        counters: &mut OperationCountersV1,
        scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    ) -> Result<(FsPrivatePackV1, u64), PublicationErrorV1> {
        let mut source = PublicationSource::new(objects).map_err(PublicationErrorV1::Core)?;
        let mut pack = cas.begin_private_pack().map_err(publication_error)?;
        let mut spool = PublicationSpool::default();
        build_dense_pack_v1(
            &mut source,
            &mut pack,
            &mut spool,
            ledger,
            counters,
            scratch,
        )
        .map_err(PublicationErrorV1::Core)?;
        assert_eq!(
            source.payload_bytes_read,
            objects.iter().map(|bytes| bytes.len() as u64).sum::<u64>()
        );
        Ok((pack, source.payload_bytes_read))
    }

    fn build_publication_pack_raw(
        cas: &FsCasV1,
        objects: &[&[u8]],
        ledger: &ResourceLedgerV1,
        counters: &mut OperationCountersV1,
        scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    ) -> Result<(FsPrivatePackV1, u64), FsCasErrorV1> {
        let mut source = PublicationSource::new(objects).map_err(FsCasErrorV1::Core)?;
        let mut pack = cas.begin_private_pack()?;
        let mut spool = PublicationSpool::default();
        build_dense_pack_v1(
            &mut source,
            &mut pack,
            &mut spool,
            ledger,
            counters,
            scratch,
        )
        .map_err(FsCasErrorV1::Core)?;
        assert_eq!(
            source.payload_bytes_read,
            objects.iter().map(|bytes| bytes.len() as u64).sum::<u64>()
        );
        Ok((pack, source.payload_bytes_read))
    }

    fn read_fault_matrix(
        request: PublicationRequestV1<'_>,
        incumbent_objects: &[&[u8]],
        candidate_objects: &[&[u8]],
        boundaries: &[FsCasFilesystemBoundaryV1],
    ) -> ReadFaultObservationV1 {
        const ERRORS: [FsCasErrorV1; 4] = [
            FsCasErrorV1::MissingOccupant,
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ReadFailure),
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ShortRead),
        ];

        let cas = FsCasV1::create_new(request.root).expect("semantic read-fault root");
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut incumbent_counters = OperationCountersV1::default();
        let (mut incumbent, _) = build_publication_pack_raw(
            &cas,
            incumbent_objects,
            &ledger,
            &mut incumbent_counters,
            &mut scratch,
        )
        .expect("semantic read-fault incumbent pack");
        let mut spool = PublicationSpool::default();
        cas.admit_pack(
            &mut incumbent,
            &mut spool,
            &ledger,
            &mut incumbent_counters,
            &mut scratch,
        )
        .expect("semantic read-fault incumbent admission");

        let mut observation = ReadFaultObservationV1 {
            cases: 0,
            all_injected: true,
            all_errors_expected: true,
            missing_occupant_cases: 0,
            permission_denied_cases: 0,
            read_failure_cases: 0,
            short_read_cases: 0,
            all_preparations_clean: true,
            all_carriers_preserved: true,
            all_catalogs_preserved: true,
            all_objects_cleaned: true,
            all_residue_free: true,
            all_slots_released: true,
            all_incumbents_usable: true,
            all_forbidden_work_zero: true,
        };

        for boundary in boundaries {
            for error in ERRORS {
                observation.cases += 1;
                match error {
                    FsCasErrorV1::MissingOccupant => observation.missing_occupant_cases += 1,
                    FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied) => {
                        observation.permission_denied_cases += 1
                    }
                    FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ReadFailure) => {
                        observation.read_failure_cases += 1
                    }
                    FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ShortRead) => {
                        observation.short_read_cases += 1
                    }
                    _ => unreachable!(),
                }

                let mut counters = OperationCountersV1::default();
                let (mut candidate, _) = build_publication_pack_raw(
                    &cas,
                    candidate_objects,
                    &ledger,
                    &mut counters,
                    &mut scratch,
                )
                .expect("semantic read-fault candidate pack");
                let mut control = ReadFaultControl {
                    boundary: *boundary,
                    error,
                    injected: false,
                };
                let result = cas
                    .admit_pack_controlled(
                        &mut candidate,
                        &mut spool,
                        &ledger,
                        &mut counters,
                        &mut scratch,
                        &mut control,
                    )
                    .map(|_| ());
                drop(candidate);

                observation.all_injected &= control.injected;
                observation.all_errors_expected &= result == Err(error);
                observation.all_preparations_clean &=
                    directory_entries(request.root, "preparation") == Some(0);
                observation.all_carriers_preserved &=
                    directory_entries(request.root, "carriers") == Some(1);
                observation.all_catalogs_preserved &=
                    directory_entries(request.root, "catalog") == Some(1);
                observation.all_objects_cleaned &=
                    directory_entries(request.root, "objects") == Some(1);
                observation.all_residue_free &= counters.unreachable_installed_residue_bytes == 0;
                observation.all_slots_released &= ledger.admitted_slots() == 0;
                observation.all_incumbents_usable &= cas.occupied().is_ok();
                observation.all_forbidden_work_zero &= counters.has_zero_forbidden_work();
            }
        }
        observation
    }

    fn comparison_observation(
        root: &Path,
        raw_error: Option<FsCasErrorV1>,
        counters: &OperationCountersV1,
        ledger: &ResourceLedgerV1,
        read_bytes_before: u64,
        read_calls_before: u64,
        installed_pack_len: u64,
        incumbent_identity: IncumbentIdentityObservationV1,
        owner_usable: bool,
        stale_usable: bool,
    ) -> ComparisonOverflowObservationV1 {
        ComparisonOverflowObservationV1 {
            error: raw_error.map(publication_error),
            comparison_bytes: counters.incumbent_comparison_bytes,
            comparison_windows: counters.incumbent_comparison_windows,
            read_bytes_delta: counters.fscas_bytes_read.saturating_sub(read_bytes_before),
            read_calls_delta: counters.fscas_read_calls.saturating_sub(read_calls_before),
            preparation_entries: directory_entries(root, "preparation").unwrap_or(0),
            carrier_entries: directory_entries(root, "carriers").unwrap_or(0),
            catalog_entries: directory_entries(root, "catalog").unwrap_or(0),
            residue_bytes: counters.unreachable_installed_residue_bytes,
            admitted_slots: ledger.admitted_slots(),
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
            installed_pack_len,
            incumbent_identity,
            owner_usable,
            stale_usable,
            zero_forbidden_work: counters.has_zero_forbidden_work(),
        }
    }

    fn incumbent_observation(
        root: &Path,
        raw_error: Option<FsCasErrorV1>,
        counters: &OperationCountersV1,
        ledger: &ResourceLedgerV1,
        installed_pack_len: u64,
        incumbent_preserved: bool,
    ) -> IncumbentObservationV1 {
        IncumbentObservationV1 {
            error: raw_error.map(publication_error),
            preparation_entries: directory_entries(root, "preparation").unwrap_or(0),
            carrier_entries: directory_entries(root, "carriers").unwrap_or(0),
            catalog_entries: directory_entries(root, "catalog").unwrap_or(0),
            residue_bytes: counters.unreachable_installed_residue_bytes,
            admitted_slots: ledger.admitted_slots(),
            installed_pack_len,
            incumbent_preserved,
            zero_forbidden_work: counters.has_zero_forbidden_work(),
        }
    }

    /// Preserve the incumbent-comparison overflow contract with the real
    /// admission path. Only scalar counter and custody observations cross the
    /// feature-gated boundary.
    pub fn equal_incumbent_comparison_overflow_v1(
        request: PublicationRequestV1<'_>,
    ) -> ComparisonOverflowObservationV1 {
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut incumbent_counters = OperationCountersV1::default();
        let mut candidate_counters = OperationCountersV1::default();
        let mut installed_pack_len = 0;
        let mut incumbent_identity = IncumbentIdentityObservationV1::Unavailable;
        let mut owner_usable = false;
        let mut stale_usable = false;
        let mut read_bytes_before = 0;
        let mut read_calls_before = 0;
        let result = (|| -> Result<(), FsCasErrorV1> {
            let cas = FsCasV1::create_new(request.root)?;
            let stale = FsCasV1::open_existing(request.root)?;
            let (mut incumbent, _) = build_publication_pack_raw(
                &cas,
                request.objects,
                &ledger,
                &mut incumbent_counters,
                &mut scratch,
            )?;
            let mut spool = PublicationSpool::default();
            let installed = cas.admit_pack(
                &mut incumbent,
                &mut spool,
                &ledger,
                &mut incumbent_counters,
                &mut scratch,
            )?;
            installed_pack_len = installed.sealed().pack_len();
            let incumbent_path = one_entry(request.root, "carriers");
            let incumbent_before = fs::metadata(&incumbent_path).ok();

            let (mut candidate, _) = build_publication_pack_raw(
                &cas,
                request.objects,
                &ledger,
                &mut candidate_counters,
                &mut scratch,
            )?;
            candidate_counters.incumbent_comparison_bytes = 7;
            candidate_counters.incumbent_comparison_windows = u64::MAX;
            read_bytes_before = candidate_counters.fscas_bytes_read;
            read_calls_before = candidate_counters.fscas_read_calls;
            let error = cas
                .admit_pack(
                    &mut candidate,
                    &mut spool,
                    &ledger,
                    &mut candidate_counters,
                    &mut scratch,
                )
                .err();
            drop(candidate);
            incumbent_identity = incumbent_identity_observation(
                incumbent_before.as_ref(),
                fs::metadata(&incumbent_path).ok().as_ref(),
            );
            owner_usable = cas.occupied().is_ok();
            stale_usable = stale.occupied().is_ok();
            Err(error.unwrap_or(FsCasErrorV1::Core(CoreError::Schema)))
        })();
        let error = result.err();
        comparison_observation(
            request.root,
            error,
            &candidate_counters,
            &ledger,
            read_bytes_before,
            read_calls_before,
            installed_pack_len,
            incumbent_identity,
            owner_usable,
            stale_usable,
        )
    }

    /// Preserve the counted incumbent-pack-read overflow boundary.
    pub fn incumbent_pack_read_observation_overflow_v1(
        request: PublicationRequestV1<'_>,
    ) -> ComparisonOverflowObservationV1 {
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut incumbent_counters = OperationCountersV1::default();
        let mut candidate_counters = OperationCountersV1::default();
        let mut installed_pack_len = 0;
        let mut incumbent_identity = IncumbentIdentityObservationV1::Unavailable;
        let mut owner_usable = false;
        let mut stale_usable = false;
        let mut read_bytes_before = 0;
        let mut read_calls_before = 0;
        let result = (|| -> Result<(), FsCasErrorV1> {
            let cas = FsCasV1::create_new(request.root)?;
            let stale = FsCasV1::open_existing(request.root)?;
            let (mut incumbent, _) = build_publication_pack_raw(
                &cas,
                request.objects,
                &ledger,
                &mut incumbent_counters,
                &mut scratch,
            )?;
            let mut spool = PublicationSpool::default();
            let installed = cas.admit_pack(
                &mut incumbent,
                &mut spool,
                &ledger,
                &mut incumbent_counters,
                &mut scratch,
            )?;
            installed_pack_len = installed.sealed().pack_len();
            let incumbent_path = one_entry(request.root, "carriers");
            let incumbent_before = fs::metadata(&incumbent_path).ok();
            let (mut candidate, _) = build_publication_pack_raw(
                &cas,
                request.objects,
                &ledger,
                &mut candidate_counters,
                &mut scratch,
            )?;
            read_bytes_before = candidate_counters.fscas_bytes_read;
            read_calls_before = candidate_counters.fscas_read_calls;
            cas.saturate_next_occupant_pack_read_calls_for_test_v1();
            let error = cas
                .admit_pack(
                    &mut candidate,
                    &mut spool,
                    &ledger,
                    &mut candidate_counters,
                    &mut scratch,
                )
                .err();
            drop(candidate);
            incumbent_identity = incumbent_identity_observation(
                incumbent_before.as_ref(),
                fs::metadata(&incumbent_path).ok().as_ref(),
            );
            owner_usable = cas.occupied().is_ok();
            stale_usable = stale.occupied().is_ok();
            Err(error.unwrap_or(FsCasErrorV1::Core(CoreError::Schema)))
        })();
        let error = result.err();
        comparison_observation(
            request.root,
            error,
            &candidate_counters,
            &ledger,
            read_bytes_before,
            read_calls_before,
            installed_pack_len,
            incumbent_identity,
            owner_usable,
            stale_usable,
        )
    }

    /// Corrupt the installed incumbent carrier, then exercise candidate
    /// validation and report the preserved physical custody facts.
    pub fn malformed_incumbent_v1(request: PublicationRequestV1<'_>) -> IncumbentObservationV1 {
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut incumbent_counters = OperationCountersV1::default();
        let mut candidate_counters = OperationCountersV1::default();
        let mut installed_pack_len = 0;
        let mut incumbent_preserved = false;
        let result = (|| -> Result<(), FsCasErrorV1> {
            let cas = FsCasV1::create_new(request.root)?;
            let (mut incumbent, _) = build_publication_pack_raw(
                &cas,
                request.objects,
                &ledger,
                &mut incumbent_counters,
                &mut scratch,
            )?;
            let mut spool = PublicationSpool::default();
            let installed = cas.admit_pack(
                &mut incumbent,
                &mut spool,
                &ledger,
                &mut incumbent_counters,
                &mut scratch,
            )?;
            installed_pack_len = installed.sealed().pack_len();
            let carrier = one_entry(request.root, "carriers");
            let original = make_owner_writable(&carrier);
            let mut bytes = fs::read(&carrier).map_err(|_| FsCasErrorV1::Io)?;
            if let Some(first) = bytes.first_mut() {
                *first ^= 0xff;
            }
            fs::write(&carrier, bytes).map_err(|_| FsCasErrorV1::Io)?;
            fs::set_permissions(&carrier, original).map_err(|_| FsCasErrorV1::Io)?;

            let (mut candidate, _) = build_publication_pack_raw(
                &cas,
                request.objects,
                &ledger,
                &mut candidate_counters,
                &mut scratch,
            )?;
            let error = cas
                .admit_pack(
                    &mut candidate,
                    &mut spool,
                    &ledger,
                    &mut candidate_counters,
                    &mut scratch,
                )
                .err();
            drop(candidate);
            incumbent_preserved = directory_entries(request.root, "carriers") == Some(1)
                && directory_entries(request.root, "catalog") == Some(1);
            Err(error.unwrap_or(FsCasErrorV1::Core(CoreError::Schema)))
        })();
        incumbent_observation(
            request.root,
            result.err(),
            &candidate_counters,
            &ledger,
            installed_pack_len,
            incumbent_preserved,
        )
    }

    /// Preserve the closure-admission failure after the immutable carrier is
    /// already installed. The carrier residue is recorded through the real
    /// admission receipt; no closure handle or marker escapes this adapter.
    pub fn later_closure_failure_v1(
        request: PublicationRequestV1<'_>,
    ) -> ClosureFailureObservationV1 {
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut counters = OperationCountersV1::default();
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut pack_len = 0;
        let mut record_count = 0;
        let error = (|| -> Result<CoreError, CoreError> {
            let cas = FsCasV1::create_new(request.root).map_err(|_| CoreError::SinkRefused)?;
            let carrier_object = closure_object(5, b"carrier-only");
            let (mut pack, _) = build_publication_pack_raw(
                &cas,
                &[carrier_object.as_slice()],
                &ledger,
                &mut counters,
                &mut scratch,
            )
            .map_err(|_| CoreError::SinkRefused)?;
            let mut spool = PublicationSpool::default();
            let admission = cas
                .admit_pack(&mut pack, &mut spool, &ledger, &mut counters, &mut scratch)
                .map_err(|_| CoreError::SinkRefused)?;
            pack_len = admission.sealed().pack_len();
            record_count = admission.sealed().record_count();

            let (version, root, version_id, root_id) = empty_closure_objects();
            let closure_objects = [(version_id, version), (root_id, root)];
            let mut closure = ClosureSource {
                objects: &closure_objects,
            };
            let mut operation = cas
                .begin_closure_operation()
                .map_err(|_| CoreError::SinkRefused)?;
            let mut incoming_comparison = [0_u8; COMPARISON_WINDOW_BYTES];
            let mut occupied_comparison = [0_u8; COMPARISON_WINDOW_BYTES];
            let mut source_window = [0_u8; 32_768];
            let mut cdc_ring = [0_u8; 32_768];
            let mut traversal = [0_u8; 1];
            let failure = cas
                .admit_complete_closure(
                    &mut operation,
                    &mut closure,
                    version_id,
                    &ledger,
                    &mut counters,
                    AdmissionBuffersV1::new(
                        &mut incoming_comparison,
                        &mut occupied_comparison,
                        &mut source_window,
                        &mut cdc_ring,
                        &mut traversal,
                    ),
                )
                .err()
                .unwrap_or(CoreError::Schema);
            admission
                .record_later_unreachable_residue(&mut counters)
                .map_err(|_| CoreError::IntegerOverflow)?;
            Err(failure)
        })()
        .err();

        ClosureFailureObservationV1 {
            error,
            pack_len,
            record_count,
            residue_bytes: counters.unreachable_installed_residue_bytes,
            closure_entries: directory_entries(request.root, "closures").unwrap_or(0),
            closure_fences: counters.closure_fences,
            bytes_read: counters.bytes_read,
            publication_authority_dispatches: counters.publication_authority_dispatches,
            closure_path_is_file: request.root.join("closures").is_file(),
            admitted_slots: ledger.admitted_slots(),
            zero_forbidden_work: counters.has_zero_forbidden_work(),
        }
    }

    /// Exercise every same-carrier incumbent read boundary against every
    /// frozen typed read failure.
    pub fn same_carrier_incumbent_read_failures_v1(
        request: PublicationRequestV1<'_>,
    ) -> ReadFaultObservationV1 {
        read_fault_matrix(
            request,
            request.objects,
            request.objects,
            &[
                FsCasFilesystemBoundaryV1::CatalogMarkerRead,
                FsCasFilesystemBoundaryV1::CatalogMarkerRevalidationRead,
                FsCasFilesystemBoundaryV1::CarrierMetadataRead,
                FsCasFilesystemBoundaryV1::IncumbentComparisonRead,
            ],
        )
    }

    /// Exercise every cross-carrier object-validation boundary against every
    /// frozen typed read failure.
    pub fn cross_carrier_object_validation_read_failures_v1(
        request: PublicationRequestV1<'_>,
    ) -> ReadFaultObservationV1 {
        let shared = request.objects.first().copied().unwrap_or_default();
        let additional = request.objects.get(1).copied().unwrap_or(shared);
        read_fault_matrix(
            request,
            std::slice::from_ref(&shared),
            &[shared, additional],
            &[
                FsCasFilesystemBoundaryV1::ObjectLocatorRead,
                FsCasFilesystemBoundaryV1::CatalogMarkerRead,
                FsCasFilesystemBoundaryV1::CarrierMetadataRead,
                FsCasFilesystemBoundaryV1::CarrierIndexRead,
                FsCasFilesystemBoundaryV1::CarrierObjectRead,
                FsCasFilesystemBoundaryV1::IncumbentComparisonRead,
            ],
        )
    }

    /// Exercise a source-side read failure before immutable admission. The
    /// source is still caller-owned bytes, while the real pack builder and
    /// private preparation cleanup remain engine-owned.
    pub fn source_failure_v1(request: PublicationRequestV1<'_>) -> FaultObservationV1 {
        let mut counters = OperationCountersV1::default();
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut source_bytes_read = 0;
        let error = (|| {
            let cas = FsCasV1::create_new(request.root)?;
            let mut source = PublicationSource::new(request.objects)
                .map_err(FsCasErrorV1::Core)?
                .with_payload_failure();
            let mut pack = cas.begin_private_pack()?;
            let mut spool = PublicationSpool::default();
            let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
            let result = build_dense_pack_v1(
                &mut source,
                &mut pack,
                &mut spool,
                &ledger,
                &mut counters,
                &mut scratch,
            )
            .map_err(FsCasErrorV1::Core);
            source_bytes_read = source.payload_bytes_read;
            drop(pack);
            result
        })()
        .err();
        fault_observation(
            request.root,
            error,
            &counters,
            &ledger,
            source_bytes_read,
            0,
            0,
            false,
        )
    }

    /// Stop admission at the pre-install control boundary and expose the
    /// resulting cleanup/counter facts as immutable scalar observations.
    pub fn deadline_before_install_v1(request: PublicationRequestV1<'_>) -> FaultObservationV1 {
        let mut counters = OperationCountersV1::default();
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut source_bytes_read = 0;
        let error = (|| {
            let cas = FsCasV1::create_new(request.root)?;
            let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
            let (mut pack, source_read) = build_publication_pack_raw(
                &cas,
                request.objects,
                &ledger,
                &mut counters,
                &mut scratch,
            )?;
            source_bytes_read = source_read;
            let mut spool = PublicationSpool::default();
            let mut control = FaultControl::deadline(FsCasBoundaryV1::BeforeCarrierInstall);
            let result = cas
                .admit_pack_controlled(
                    &mut pack,
                    &mut spool,
                    &ledger,
                    &mut counters,
                    &mut scratch,
                    &mut control,
                )
                .map(|_| ());
            drop(pack);
            result
        })()
        .err();
        fault_observation(
            request.root,
            error,
            &counters,
            &ledger,
            source_bytes_read,
            counters.incumbent_comparison_bytes,
            counters.incumbent_comparison_windows,
            false,
        )
    }

    /// Admit one real incumbent, then cancel a matching candidate after the
    /// bounded incumbent-comparison window. The surviving directory counts
    /// are the ownership proof that the incumbent, not the candidate, won.
    pub fn cancellation_during_loser_readback_v1(
        request: PublicationRequestV1<'_>,
        should_cancel: &mut impl FnMut() -> bool,
    ) -> FaultObservationV1 {
        let mut counters = OperationCountersV1::default();
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut source_bytes_read = 0;
        let mut incumbent_preserved = false;
        let mut incumbent_identity = IncumbentIdentityObservationV1::Unavailable;
        let error = (|| {
            let cas = FsCasV1::create_new(request.root)?;
            let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
            let (mut incumbent, incumbent_read) = build_publication_pack_raw(
                &cas,
                request.objects,
                &ledger,
                &mut counters,
                &mut scratch,
            )?;
            source_bytes_read = incumbent_read;
            let mut spool = PublicationSpool::default();
            cas.admit_pack(
                &mut incumbent,
                &mut spool,
                &ledger,
                &mut counters,
                &mut scratch,
            )?;

            let incumbent_path = one_entry(request.root, "carriers");
            let incumbent_before = fs::metadata(&incumbent_path).ok();

            let (mut candidate, candidate_read) = build_publication_pack_raw(
                &cas,
                request.objects,
                &ledger,
                &mut counters,
                &mut scratch,
            )?;
            source_bytes_read = source_bytes_read.saturating_add(candidate_read);
            struct LoserReadbackControl<'a, F> {
                at_boundary: bool,
                should_cancel: &'a mut F,
            }

            impl<F: FnMut() -> bool> FsCasControlV1 for LoserReadbackControl<'_, F> {
                fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
                    self.at_boundary = boundary == FsCasBoundaryV1::AfterIncumbentComparisonWindow;
                }

                fn cancellation_requested(&mut self) -> bool {
                    self.at_boundary && (self.should_cancel)()
                }

                fn deadline_exceeded(&mut self) -> bool {
                    false
                }
            }

            let mut control = LoserReadbackControl {
                at_boundary: false,
                should_cancel,
            };
            let result = cas
                .admit_pack_controlled(
                    &mut candidate,
                    &mut spool,
                    &ledger,
                    &mut counters,
                    &mut scratch,
                    &mut control,
                )
                .map(|_| ());
            drop(candidate);
            let incumbent_after = fs::metadata(&incumbent_path).ok();
            incumbent_identity =
                incumbent_identity_observation(incumbent_before.as_ref(), incumbent_after.as_ref());
            incumbent_preserved = directory_entries(request.root, "carriers") == Some(1)
                && directory_entries(request.root, "catalog") == Some(1)
                && incumbent_identity != IncumbentIdentityObservationV1::Changed;
            result
        })()
        .err();
        let mut observation = fault_observation(
            request.root,
            error,
            &counters,
            &ledger,
            source_bytes_read,
            counters.incumbent_comparison_bytes,
            counters.incumbent_comparison_windows,
            incumbent_preserved,
        );
        observation.incumbent_identity = incumbent_identity;
        observation
    }

    fn one_entry(root: &Path, name: &str) -> PathBuf {
        let mut entries = fs::read_dir(root.join(name))
            .expect("semantic fault fixture directory")
            .map(|entry| entry.expect("semantic fault fixture entry").path())
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 1, "semantic fault fixture entry count");
        entries.pop().expect("semantic fault fixture entry")
    }

    fn incumbent_identity_observation(
        before: Option<&fs::Metadata>,
        after: Option<&fs::Metadata>,
    ) -> IncumbentIdentityObservationV1 {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;

            return match (before, after) {
                (Some(before), Some(after))
                    if before.dev() == after.dev() && before.ino() == after.ino() =>
                {
                    IncumbentIdentityObservationV1::Preserved
                }
                (Some(_), Some(_)) => IncumbentIdentityObservationV1::Changed,
                _ => IncumbentIdentityObservationV1::Unavailable,
            };
        }
        #[cfg(not(unix))]
        {
            let _ = (before, after);
            IncumbentIdentityObservationV1::Unavailable
        }
    }

    fn make_owner_writable(path: &Path) -> fs::Permissions {
        let original = fs::metadata(path)
            .expect("semantic fault fixture metadata")
            .permissions();
        let mut writable = original.clone();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            writable.set_mode(writable.mode() | 0o200);
        }
        #[cfg(windows)]
        writable.set_readonly(false);
        fs::set_permissions(path, writable).expect("semantic fault fixture writable");
        original
    }

    fn publication_object_id(bytes: &[u8]) -> Result<TypedPhysicalObjectIdV1, FsCasErrorV1> {
        decode_physical_object_v1(bytes, &mut DiscardStrongEdgesV1)
            .map_err(|_| FsCasErrorV1::Core(CoreError::Schema))?
            .physical_id()
            .map_err(|_| FsCasErrorV1::Core(CoreError::IdMismatch))
    }

    fn publication_locator_path(root: &Path, id: TypedPhysicalObjectIdV1) -> PathBuf {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let prefix = match id {
            TypedPhysicalObjectIdV1::VersionRecord(_) => "01-",
            TypedPhysicalObjectIdV1::Tree(_) => "02-",
            TypedPhysicalObjectIdV1::File(_) => "03-",
            TypedPhysicalObjectIdV1::Symlink(_) => "04-",
            TypedPhysicalObjectIdV1::Chunk(_) => "05-",
        };
        let mut name = String::with_capacity(prefix.len() + 64);
        name.push_str(prefix);
        for byte in id.as_bytes() {
            name.push(char::from(HEX[usize::from(byte >> 4)]));
            name.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        root.join("objects").join(name)
    }

    fn empty_fault_observation(error: FsCasErrorV1) -> FaultObservationV1 {
        FaultObservationV1 {
            error: Some(publication_error(error)),
            preparation_entries: 0,
            carrier_entries: 0,
            object_entries: 0,
            catalog_entries: 0,
            closure_entries: 0,
            residue_bytes: 0,
            bytes_written: 0,
            admitted_slots: 0,
            catalog_operations: 0,
            closure_fences: 0,
            zero_forbidden_work: true,
            source_bytes_read: 0,
            incumbent_comparison_bytes: 0,
            incumbent_comparison_windows: 0,
            incumbent_preserved: false,
            incumbent_identity: IncumbentIdentityObservationV1::Unavailable,
            fault_injected: false,
            cleanup_fault_injected: false,
            storage_bytes_request_matches_reservation: true,
            storage_inodes_request_matches_reservation: true,
            storage_bytes_terminal_sum_matches_reservation: true,
            storage_inodes_terminal_sum_matches_reservation: true,
            storage_bytes_retained: 0,
            storage_inodes_retained: 0,
            loser_locator_absent: false,
            owner_handle_invalidated: false,
            owner_private_invalidated: false,
            owner_occupied_invalidated: false,
            owner_closure_invalidated: false,
            stale_handle_invalidated: false,
            stale_private_invalidated: false,
            stale_occupied_invalidated: false,
            stale_closure_refused: false,
            reopen_invalidated: false,
            donor_installed: false,
            incumbent_marker_has_canonical_size: false,
            incumbent_pack_len: 0,
            candidate_pack_len: 0,
            expected_residue_bytes: 0,
            closure_payload_len: 0,
            filesystem_write_failure: false,
            catalog_path_is_file: false,
            carrier_path_is_file: false,
            malformed_locator_preserved: false,
            invalidated: false,
            first_cause: Some(publication_causes(error).0),
            dominant_cause: Some(publication_causes(error).1),
        }
    }

    /// Install a malformed object locator at the exact no-replace boundary.
    /// The adapter returns only the typed outcome and resulting custody facts;
    /// the filesystem control seam remains private to this operation.
    pub fn atomic_locator_malformed_occupant_v1(
        request: PublicationRequestV1<'_>,
    ) -> FaultObservationV1 {
        let mut counters = OperationCountersV1::default();
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut source_bytes_read = 0;
        let mut malformed_locator_preserved = false;
        let error = (|| {
            let cas = FsCasV1::create_new(request.root)?;
            let object = request
                .objects
                .first()
                .copied()
                .ok_or(FsCasErrorV1::Core(CoreError::Schema))?;
            let locator = publication_locator_path(request.root, publication_object_id(object)?);
            let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
            let (mut pack, source_read) = build_publication_pack_raw(
                &cas,
                request.objects,
                &ledger,
                &mut counters,
                &mut scratch,
            )?;
            source_bytes_read = source_read;
            let mut spool = PublicationSpool::default();
            let mut control = InstallMalformedLocatorControl {
                locator,
                injected: false,
            };
            let result = cas
                .admit_pack_controlled(
                    &mut pack,
                    &mut spool,
                    &ledger,
                    &mut counters,
                    &mut scratch,
                    &mut control,
                )
                .map(|_| ());
            malformed_locator_preserved =
                fs::read(&control.locator).ok().as_deref() == Some(&[0_u8; 160]);
            drop(pack);
            result
        })()
        .err();
        let mut observation = fault_observation(
            request.root,
            error,
            &counters,
            &ledger,
            source_bytes_read,
            0,
            0,
            false,
        );
        observation.malformed_locator_preserved = malformed_locator_preserved;
        observation.fault_injected = malformed_locator_preserved;
        observation.reopen_invalidated = matches!(
            FsCasV1::open_existing(request.root),
            Err(FsCasErrorV1::Invalidated)
        );
        observation
    }

    /// Repeat the malformed-occupant race while injecting preparation-spool
    /// cleanup failure, preserving both causes in the bounded observation.
    pub fn atomic_locator_cleanup_failure_v1(
        request: PublicationRequestV1<'_>,
    ) -> FaultObservationV1 {
        let mut counters = OperationCountersV1::default();
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut source_bytes_read = 0;
        let mut occupant_injected = false;
        let mut cleanup_injected = false;
        let mut owner_occupied_invalidated = false;
        let mut stale_occupied_invalidated = false;
        let error = (|| {
            let cas = FsCasV1::create_new(request.root)?;
            let stale = FsCasV1::open_existing(request.root)?;
            let object = request
                .objects
                .first()
                .copied()
                .ok_or(FsCasErrorV1::Core(CoreError::Schema))?;
            let locator = publication_locator_path(request.root, publication_object_id(object)?);
            let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
            let (mut pack, source_read) = build_publication_pack_raw(
                &cas,
                request.objects,
                &ledger,
                &mut counters,
                &mut scratch,
            )?;
            source_bytes_read = source_read;
            let mut spool = PublicationSpool::default();
            let mut control = InstallLocatorAndFailPreparationCleanupControl {
                locator,
                occupant_injected: false,
                cleanup_injected: false,
            };
            let result = cas
                .admit_pack_controlled(
                    &mut pack,
                    &mut spool,
                    &ledger,
                    &mut counters,
                    &mut scratch,
                    &mut control,
                )
                .map(|_| ());
            occupant_injected = control.occupant_injected;
            cleanup_injected = control.cleanup_injected;
            owner_occupied_invalidated = matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated));
            stale_occupied_invalidated = matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated));
            drop(pack);
            result
        })()
        .err();
        let mut observation = fault_observation(
            request.root,
            error,
            &counters,
            &ledger,
            source_bytes_read,
            0,
            0,
            false,
        );
        observation.fault_injected = occupant_injected;
        observation.cleanup_fault_injected = cleanup_injected;
        observation.owner_occupied_invalidated = owner_occupied_invalidated;
        observation.stale_occupied_invalidated = stale_occupied_invalidated;
        observation.reopen_invalidated = matches!(
            FsCasV1::open_existing(request.root),
            Err(FsCasErrorV1::Invalidated)
        );
        observation
    }

    /// Preserve the three checked byte/call tuples from the occupied-reader
    /// overflow owner while returning only immutable semantic observations.
    pub fn occupied_locator_catalog_observation_overflow_v1(
        request: PublicationRequestV1<'_>,
    ) -> OccupiedOverflowObservationV1 {
        let object = match request.objects.first().copied() {
            Some(object) => object,
            None => {
                return OccupiedOverflowObservationV1 {
                    initial_admitted_slots: 0,
                    initial_preparation_entries: 0,
                    metadata_error: Some(PublicationErrorV1::Core(CoreError::Schema)),
                    metadata_bytes: 0,
                    metadata_calls: 0,
                    metadata_object_cached: false,
                    pack_error: None,
                    pack_bytes: 0,
                    pack_calls: 0,
                    pack_object_cached: false,
                    payload_len: None,
                    payload_object_cached_before_read: false,
                    payload_error: None,
                    payload_bytes: 0,
                    payload_calls: 0,
                    payload_prefix_preserved: false,
                    payload_object_cached: false,
                    current_handle_usable: false,
                    reopen_usable: false,
                    admitted_slots: 0,
                    preparation_entries: 0,
                };
            }
        };
        let result = (|| -> Result<OccupiedOverflowObservationV1, FsCasErrorV1> {
            let cas = FsCasV1::create_new(request.root)?;
            let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
            let mut counters = OperationCountersV1::default();
            let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
            let (mut pack, _) = build_publication_pack_raw(
                &cas,
                request.objects,
                &ledger,
                &mut counters,
                &mut scratch,
            )?;
            let mut spool = PublicationSpool::default();
            cas.admit_pack(&mut pack, &mut spool, &ledger, &mut counters, &mut scratch)?;
            let initial_admitted_slots = ledger.admitted_slots();
            let initial_preparation_entries =
                directory_entries(request.root, "preparation").unwrap_or(0);
            let id = publication_object_id(object)?;

            cas.seed_next_occupied_read_observation_for_test_v1(37, u64::MAX - 1);
            let mut metadata = cas.occupied_private_v1()?;
            let metadata_error = metadata
                .occupied_len_typed_v1(id)
                .err()
                .map(publication_error);
            let (metadata_bytes, metadata_calls) =
                metadata.direct_storage_read_observation_typed_v1()?;
            let metadata_object_cached = metadata.resolved_object_cached_for_test_v1(id);
            drop(metadata);

            cas.seed_next_occupied_read_observation_for_test_v1(53, u64::MAX - 2);
            let mut pack_observation = cas.occupied_private_v1()?;
            let pack_error = pack_observation
                .occupied_len_typed_v1(id)
                .err()
                .map(publication_error);
            let (pack_bytes, pack_calls) =
                pack_observation.direct_storage_read_observation_typed_v1()?;
            let pack_object_cached = pack_observation.resolved_object_cached_for_test_v1(id);
            drop(pack_observation);

            cas.seed_next_occupied_payload_read_observation_for_test_v1(71, u64::MAX);
            let mut payload = cas.occupied_private_v1()?;
            let payload_len = payload.occupied_len_typed_v1(id)?;
            let payload_object_cached_before_read = payload.resolved_object_cached_for_test_v1(id);
            let mut prefix = [0_u8; 11];
            let payload_error = payload
                .read_occupied_exact_at_typed_v1(id, 0, &mut prefix)
                .err()
                .map(publication_error);
            let (payload_bytes, payload_calls) =
                payload.direct_storage_read_observation_typed_v1()?;
            let payload_object_cached = payload.resolved_object_cached_for_test_v1(id);
            let payload_prefix_preserved = prefix == object[..11];
            drop(payload);

            let current_handle_usable = cas.occupied_private_v1().is_ok();
            let reopen_usable = FsCasV1::open_existing(request.root).is_ok();
            Ok(OccupiedOverflowObservationV1 {
                initial_admitted_slots,
                initial_preparation_entries,
                metadata_error,
                metadata_bytes,
                metadata_calls,
                metadata_object_cached,
                pack_error,
                pack_bytes,
                pack_calls,
                pack_object_cached,
                payload_len,
                payload_object_cached_before_read,
                payload_error,
                payload_bytes,
                payload_calls,
                payload_prefix_preserved,
                payload_object_cached,
                current_handle_usable,
                reopen_usable,
                admitted_slots: ledger.admitted_slots(),
                preparation_entries: directory_entries(request.root, "preparation").unwrap_or(0),
            })
        })();
        match result {
            Ok(observation) => observation,
            Err(error) => OccupiedOverflowObservationV1 {
                initial_admitted_slots: 0,
                initial_preparation_entries: 0,
                metadata_error: Some(publication_error(error)),
                metadata_bytes: 0,
                metadata_calls: 0,
                metadata_object_cached: false,
                pack_error: None,
                pack_bytes: 0,
                pack_calls: 0,
                pack_object_cached: false,
                payload_len: None,
                payload_object_cached_before_read: false,
                payload_error: None,
                payload_bytes: 0,
                payload_calls: 0,
                payload_prefix_preserved: false,
                payload_object_cached: false,
                current_handle_usable: false,
                reopen_usable: false,
                admitted_slots: 0,
                preparation_entries: 0,
            },
        }
    }

    /// Preserve the catalog-counter preflight boundary and its no-visibility
    /// result without exposing the mutable counter engine.
    pub fn catalog_counter_overflow_v1(request: PublicationRequestV1<'_>) -> FaultObservationV1 {
        let mut counters = OperationCountersV1::default();
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut source_bytes_read = 0;
        let error = (|| {
            let cas = FsCasV1::create_new(request.root)?;
            let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
            let (mut pack, source_read) = build_publication_pack_raw(
                &cas,
                request.objects,
                &ledger,
                &mut counters,
                &mut scratch,
            )?;
            source_bytes_read = source_read;
            counters.fscas_catalog_operations = u64::MAX;
            let mut spool = PublicationSpool::default();
            let result = cas
                .admit_pack(&mut pack, &mut spool, &ledger, &mut counters, &mut scratch)
                .map(|_| ());
            drop(pack);
            result
        })()
        .err();
        fault_observation(
            request.root,
            error,
            &counters,
            &ledger,
            source_bytes_read,
            0,
            0,
            false,
        )
    }

    fn controlled_cleanup_failure_v1(
        request: PublicationRequestV1<'_>,
        boundary: FsCasBoundaryV1,
        target: FsCasCleanupTargetV1,
    ) -> FaultObservationV1 {
        let mut counters = OperationCountersV1::default();
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut source_bytes_read = 0;
        let mut expected_residue_bytes = 0;
        let mut fault_injected = false;
        let mut owner_private_invalidated = false;
        let mut reopen_invalidated = false;
        let error = (|| {
            let cas = FsCasV1::create_new(request.root)?;
            let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
            let (mut pack, source_read) = build_publication_pack_raw(
                &cas,
                request.objects,
                &ledger,
                &mut counters,
                &mut scratch,
            )?;
            source_bytes_read = source_read;
            expected_residue_bytes = pack
                .len()
                .expect("locator-cleanup pack length")
                .checked_add(PERSISTENT_LOCATOR_BYTES_V1 as u64)
                .expect("locator-cleanup residue length");
            let mut spool = PublicationSpool::default();
            let mut control = FaultControl::cancellation(boundary).with_cleanup_failure(target);
            let result = cas
                .admit_pack_controlled(
                    &mut pack,
                    &mut spool,
                    &ledger,
                    &mut counters,
                    &mut scratch,
                    &mut control,
                )
                .map(|_| ());
            drop(pack);
            fault_injected = control.cleanup_injected;
            owner_private_invalidated =
                matches!(cas.begin_private_pack(), Err(FsCasErrorV1::Invalidated));
            reopen_invalidated = matches!(
                FsCasV1::open_existing(request.root),
                Err(FsCasErrorV1::Invalidated)
            );
            result
        })()
        .err();
        let mut observation = fault_observation(
            request.root,
            error,
            &counters,
            &ledger,
            source_bytes_read,
            0,
            0,
            false,
        );
        observation.expected_residue_bytes = expected_residue_bytes;
        observation.fault_injected = fault_injected;
        observation.owner_private_invalidated = owner_private_invalidated;
        observation.reopen_invalidated = reopen_invalidated;
        observation
    }

    pub fn carrier_cleanup_failure_v1(request: PublicationRequestV1<'_>) -> FaultObservationV1 {
        let mut counters = OperationCountersV1::default();
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut source_bytes_read = 0;
        let mut fault_injected = false;
        let mut stale_handle_invalidated = false;
        let mut stale_private_invalidated = false;
        let mut stale_occupied_invalidated = false;
        let mut stale_closure_refused = false;
        let mut owner_handle_invalidated = false;
        let mut owner_private_invalidated = false;
        let mut owner_occupied_invalidated = false;
        let mut owner_closure_invalidated = false;
        let mut reopen_invalidated = false;
        let mut carrier_pack_len = 0;
        let mut closure_payload_len = 0;
        let error = (|| {
            let cas = FsCasV1::create_new(request.root)?;
            let object = request
                .objects
                .first()
                .copied()
                .ok_or(FsCasErrorV1::Core(CoreError::Schema))?;
            let object_id = publication_object_id(object)?;
            let mut stale_occupied = cas.occupied()?;
            let mut stale_private = cas.begin_private_pack()?;
            stale_private
                .begin_private(1)
                .map_err(|_| FsCasErrorV1::Core(CoreError::SinkRefused))?;
            let mut stale_operation = cas.begin_closure_operation()?;
            let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
            let (mut pack, source_read) = build_publication_pack_raw(
                &cas,
                request.objects,
                &ledger,
                &mut counters,
                &mut scratch,
            )?;
            source_bytes_read = source_read;
            carrier_pack_len = pack.len().expect("carrier-cleanup pack length");
            let mut spool = PublicationSpool::default();
            let mut control = FaultControl::cancellation(FsCasBoundaryV1::AfterCarrierInstall)
                .with_cleanup_failure(FsCasCleanupTargetV1::Carrier);
            let result = cas
                .admit_pack_controlled(
                    &mut pack,
                    &mut spool,
                    &ledger,
                    &mut counters,
                    &mut scratch,
                    &mut control,
                )
                .map(|_| ());
            drop(pack);
            fault_injected = control.cleanup_injected;

            stale_private_invalidated = stale_private.append(b"x") == Err(PackPortErrorV1::Failure);
            drop(stale_private);
            stale_occupied_invalidated =
                stale_occupied.occupied_len(object_id) == Err(ImmutablePortErrorV1::Failure);
            let no_objects: [(TypedPhysicalObjectIdV1, Vec<u8>); 0] = [];
            let mut stale_closure = ClosureSource {
                objects: &no_objects,
            };
            let mut incoming_comparison = [0_u8; COMPARISON_WINDOW_BYTES];
            let mut occupied_comparison = [0_u8; COMPARISON_WINDOW_BYTES];
            let mut source_window = [0_u8; 32_768];
            let mut cdc_ring = [0_u8; 32_768];
            let mut traversal = [0_u8; 1];
            let (version, _, version_id, _) = empty_closure_objects();
            closure_payload_len = u64::try_from(
                version.len() - usize::try_from(OBJECT_HEADER_BYTES).expect("object header length"),
            )
            .expect("closure payload length");
            let stale_closure_invalidated = cas.admit_complete_closure(
                &mut stale_operation,
                &mut stale_closure,
                version_id,
                &ledger,
                &mut counters,
                AdmissionBuffersV1::new(
                    &mut incoming_comparison,
                    &mut occupied_comparison,
                    &mut source_window,
                    &mut cdc_ring,
                    &mut traversal,
                ),
            ) == Err(CoreError::SinkRefused);
            stale_closure_refused = stale_closure_invalidated;
            stale_handle_invalidated = stale_private_invalidated
                && stale_occupied_invalidated
                && stale_closure_invalidated;
            owner_private_invalidated =
                matches!(cas.begin_private_pack(), Err(FsCasErrorV1::Invalidated));
            owner_occupied_invalidated = matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated));
            owner_closure_invalidated = matches!(
                cas.begin_closure_operation(),
                Err(FsCasErrorV1::Invalidated)
            );
            owner_handle_invalidated = owner_private_invalidated
                && owner_occupied_invalidated
                && owner_closure_invalidated;
            reopen_invalidated = matches!(
                FsCasV1::open_existing(request.root),
                Err(FsCasErrorV1::Invalidated)
            );
            result
        })()
        .err();
        let mut observation = fault_observation(
            request.root,
            error,
            &counters,
            &ledger,
            source_bytes_read,
            0,
            0,
            false,
        );
        observation.fault_injected = fault_injected;
        observation.stale_handle_invalidated = stale_handle_invalidated;
        observation.stale_private_invalidated = stale_private_invalidated;
        observation.stale_occupied_invalidated = stale_occupied_invalidated;
        observation.stale_closure_refused = stale_closure_refused;
        observation.owner_handle_invalidated = owner_handle_invalidated;
        observation.owner_private_invalidated = owner_private_invalidated;
        observation.owner_occupied_invalidated = owner_occupied_invalidated;
        observation.owner_closure_invalidated = owner_closure_invalidated;
        observation.reopen_invalidated = reopen_invalidated;
        observation.candidate_pack_len = carrier_pack_len;
        observation.closure_payload_len = closure_payload_len;
        observation
    }

    pub fn locator_cleanup_failure_v1(request: PublicationRequestV1<'_>) -> FaultObservationV1 {
        controlled_cleanup_failure_v1(
            request,
            FsCasBoundaryV1::AfterObjectLocatorPublication,
            FsCasCleanupTargetV1::ObjectLocator,
        )
    }

    struct CarrierRollbackControl {
        root: PathBuf,
        injected: bool,
    }

    impl FsCasControlV1 for CarrierRollbackControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary != FsCasBoundaryV1::BeforeCatalogPublication || self.injected {
                return;
            }
            let carrier = one_entry(&self.root, "carriers");
            let original_permissions = make_owner_writable(&carrier);
            let mut bytes = fs::read(&carrier).expect("semantic carrier bytes");
            let index_offset = usize::try_from(u64::from_be_bytes(
                bytes[56..64]
                    .try_into()
                    .expect("semantic carrier index offset"),
            ))
            .expect("semantic carrier index offset");
            bytes[index_offset + 1] = 1;
            fs::write(&carrier, bytes).expect("semantic carrier corruption");
            fs::set_permissions(&carrier, original_permissions)
                .expect("semantic carrier permissions");
            self.injected = true;
        }

        fn cancellation_requested(&mut self) -> bool {
            self.injected
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    /// Corrupt only the authenticated carrier index after installation. The
    /// real rollback path must retain cancellation as first cause and locator
    /// cleanup as the dominant terminal cause.
    pub fn rollback_carrier_authentication_failure_v1(
        request: PublicationRequestV1<'_>,
    ) -> FaultObservationV1 {
        let mut counters = OperationCountersV1::default();
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut source_bytes_read = 0;
        let mut expected_residue_bytes = 0;
        let mut fault_injected = false;
        let mut owner_occupied_invalidated = false;
        let mut reopen_invalidated = false;
        let error = (|| {
            let cas = FsCasV1::create_new(request.root)?;
            let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
            let (mut pack, source_read) = build_publication_pack_raw(
                &cas,
                request.objects,
                &ledger,
                &mut counters,
                &mut scratch,
            )?;
            source_bytes_read = source_read;
            expected_residue_bytes = pack
                .len()
                .expect("rollback-carrier pack length")
                .checked_add(PERSISTENT_LOCATOR_BYTES_V1 as u64)
                .expect("rollback-carrier residue length");
            let mut spool = PublicationSpool::default();
            let mut control = CarrierRollbackControl {
                root: request.root.to_path_buf(),
                injected: false,
            };
            let result = cas
                .admit_pack_controlled(
                    &mut pack,
                    &mut spool,
                    &ledger,
                    &mut counters,
                    &mut scratch,
                    &mut control,
                )
                .map(|_| ());
            drop(pack);
            fault_injected = control.injected;
            owner_occupied_invalidated = matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated));
            reopen_invalidated = matches!(
                FsCasV1::open_existing(request.root),
                Err(FsCasErrorV1::Invalidated)
            );
            result
        })()
        .err();
        let mut observation = fault_observation(
            request.root,
            error,
            &counters,
            &ledger,
            source_bytes_read,
            0,
            0,
            false,
        );
        observation.expected_residue_bytes = expected_residue_bytes;
        observation.fault_injected = fault_injected;
        observation.owner_occupied_invalidated = owner_occupied_invalidated;
        observation.reopen_invalidated = reopen_invalidated;
        observation
    }

    fn publication_outcome(outcome: FsPackAdmissionOutcomeV1) -> PublicationOutcomeV1 {
        match outcome {
            FsPackAdmissionOutcomeV1::Installed => PublicationOutcomeV1::Installed,
            FsPackAdmissionOutcomeV1::ExistingComplete => PublicationOutcomeV1::ExistingComplete,
        }
    }

    fn storage_equations_hold(counters: &OperationCountersV1) -> bool {
        counters.storage_bytes_requested == counters.storage_bytes_reserved
            && counters.storage_inodes_requested == counters.storage_inodes_reserved
            && counters.storage_bytes_reserved
                == counters
                    .storage_bytes_released
                    .saturating_add(counters.storage_bytes_committed)
                    .saturating_add(counters.storage_bytes_retained)
            && counters.storage_inodes_reserved
                == counters
                    .storage_inodes_released
                    .saturating_add(counters.storage_inodes_committed)
                    .saturating_add(counters.storage_inodes_retained)
    }

    pub fn overlapping_packs_v1(root: &Path) -> OverlappingPackObservationV1 {
        let cas = FsCasV1::create_new(root).expect("create overlapping-pack root");
        let (version, closure_root, version_id, root_id) = empty_closure_objects();
        let extra = closure_object(5, b"pack-b-only");
        let extra_id = semantic_object_id(&extra);
        let packs = [
            [version.as_slice(), closure_root.as_slice()],
            [version.as_slice(), extra.as_slice()],
        ];
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut counters = OperationCountersV1::default();
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut spool = PublicationSpool::default();
        let mut outcomes = [PublicationOutcomeV1::ExistingComplete; 2];
        let mut canonical_locator = None;
        let mut shared_locator_canonical = true;
        for (index, objects) in packs.iter().enumerate() {
            let (mut pack, _) =
                build_publication_pack_raw(&cas, objects, &ledger, &mut counters, &mut scratch)
                    .expect("build overlapping pack");
            outcomes[index] = publication_outcome(
                cas.admit_pack(&mut pack, &mut spool, &ledger, &mut counters, &mut scratch)
                    .expect("admit overlapping pack")
                    .outcome(),
            );
            let observed =
                fs::read(publication_locator_path(root, version_id)).expect("read shared locator");
            match &canonical_locator {
                None => canonical_locator = Some(observed),
                Some(canonical) => shared_locator_canonical &= canonical == &observed,
            }
        }

        let mut occupied = cas.occupied().expect("open occupied reader");
        let expected = [
            (version_id, version.as_slice()),
            (root_id, closure_root.as_slice()),
            (extra_id, extra.as_slice()),
        ];
        let mut occupied_lengths_match = true;
        let mut occupied_bytes_match = true;
        for (id, expected) in expected {
            occupied_lengths_match &= occupied.occupied_len(id).expect("observe occupied length")
                == Some(expected.len() as u64);
            let mut actual = vec![0_u8; expected.len()];
            occupied
                .read_occupied_exact_at(id, 0, &mut actual)
                .expect("read occupied object");
            assert_eq!(actual, *expected);
            occupied_bytes_match &= actual == *expected;
        }

        let closure_objects = [(version_id, version), (root_id, closure_root)];
        let mut closure = ClosureSource {
            objects: &closure_objects,
        };
        let mut operation = cas
            .begin_closure_operation()
            .expect("begin closure operation");
        let mut incoming_comparison = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut occupied_comparison = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut source_window = [0_u8; 32_768];
        let mut cdc_ring = [0_u8; 32_768];
        let mut traversal = [0_u8; 1];
        let closure_admitted = cas
            .admit_complete_closure(
                &mut operation,
                &mut closure,
                version_id,
                &ledger,
                &mut counters,
                AdmissionBuffersV1::new(
                    &mut incoming_comparison,
                    &mut occupied_comparison,
                    &mut source_window,
                    &mut cdc_ring,
                    &mut traversal,
                ),
            )
            .is_ok();

        OverlappingPackObservationV1 {
            outcomes,
            shared_locator_canonical,
            object_entries: directory_entries(root, "objects").unwrap_or(0),
            occupied_lengths_match,
            occupied_bytes_match,
            closure_admitted,
        }
    }

    pub fn overlapping_incumbent_lock_scope_v1(root: &Path) -> PublicationLockScopeObservationV1 {
        let cas = FsCasV1::create_new(root).expect("create incumbent lock-scope root");
        let shared = closure_object(5, &[0x4d; 16_384]);
        let additional = closure_object(5, &[0x9e; 16_384]);
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut spool = PublicationSpool::default();
        let mut incumbent_counters = OperationCountersV1::default();
        let (mut incumbent, _) = build_publication_pack_raw(
            &cas,
            &[shared.as_slice()],
            &ledger,
            &mut incumbent_counters,
            &mut scratch,
        )
        .expect("build incumbent pack");
        cas.admit_pack(
            &mut incumbent,
            &mut spool,
            &ledger,
            &mut incumbent_counters,
            &mut scratch,
        )
        .expect("admit incumbent pack");

        let mut candidate_counters = OperationCountersV1::default();
        let (mut candidate, _) = build_publication_pack_raw(
            &cas,
            &[shared.as_slice(), additional.as_slice()],
            &ledger,
            &mut candidate_counters,
            &mut scratch,
        )
        .expect("build overlapping candidate");
        let mut control = ObservePublicationLockScopeV1 {
            cas: cas.clone(),
            fresh_carrier: false,
            observed: false,
            visibility_available: false,
            publication_available: false,
        };
        let outcome = publication_outcome(
            cas.admit_pack_controlled(
                &mut candidate,
                &mut spool,
                &ledger,
                &mut candidate_counters,
                &mut scratch,
                &mut control,
            )
            .expect("admit overlapping candidate")
            .outcome(),
        );
        PublicationLockScopeObservationV1 {
            outcome,
            observed: control.observed,
            visibility_available: control.visibility_available,
            publication_available: control.publication_available,
        }
    }

    pub fn cancel_shared_object_validation_v1(
        root: &Path,
    ) -> SharedObjectCancellationObservationV1 {
        let cas = FsCasV1::create_new(root).expect("create overlap-cancel root");
        let (version, closure_root, version_id, _) = empty_closure_objects();
        let extra = closure_object(5, b"loser-only");
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut spool = PublicationSpool::default();
        let mut winner_counters = OperationCountersV1::default();
        let (mut winner, _) = build_publication_pack_raw(
            &cas,
            &[version.as_slice(), closure_root.as_slice()],
            &ledger,
            &mut winner_counters,
            &mut scratch,
        )
        .expect("build winner pack");
        cas.admit_pack(
            &mut winner,
            &mut spool,
            &ledger,
            &mut winner_counters,
            &mut scratch,
        )
        .expect("admit winner pack");

        let mut loser_counters = OperationCountersV1::default();
        let (mut loser, _) = build_publication_pack_raw(
            &cas,
            &[version.as_slice(), extra.as_slice()],
            &ledger,
            &mut loser_counters,
            &mut scratch,
        )
        .expect("build loser pack");
        let mut control = FaultControl::cancellation(FsCasBoundaryV1::AfterObjectComparisonWindow);
        let error = cas
            .admit_pack_controlled(
                &mut loser,
                &mut spool,
                &ledger,
                &mut loser_counters,
                &mut scratch,
                &mut control,
            )
            .err()
            .map(publication_error);
        SharedObjectCancellationObservationV1 {
            error,
            preparation_entries: directory_entries(root, "preparation").unwrap_or(0),
            carrier_entries: directory_entries(root, "carriers").unwrap_or(0),
            catalog_entries: directory_entries(root, "catalog").unwrap_or(0),
            object_entries: directory_entries(root, "objects").unwrap_or(0),
            winner_locator_present: publication_locator_path(root, version_id).is_file(),
            unreachable_residue_bytes: loser_counters.unreachable_installed_residue_bytes,
            admitted_slots: ledger.admitted_slots(),
            zero_forbidden_work: loser_counters.has_zero_forbidden_work(),
        }
    }

    pub fn simultaneous_reopened_publication_v1(root: &Path) -> ReopenedPublicationObservationV1 {
        let seed = FsCasV1::create_new(root).expect("create reopened-publication root");
        let left_cas = FsCasV1::open_existing(root).expect("open left CAS");
        let right_cas = FsCasV1::open_existing(root).expect("open right CAS");
        let shared = closure_object(5, &[0x6d; 4_096]);
        let left = closure_object(5, b"left-only");
        let right = closure_object(5, b"right-only");
        let expected = [
            (semantic_object_id(&shared), shared.as_slice()),
            (semantic_object_id(&left), left.as_slice()),
            (semantic_object_id(&right), right.as_slice()),
        ];
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut left_counters = OperationCountersV1::default();
        let mut right_counters = OperationCountersV1::default();
        let (mut left_pack, _) = build_publication_pack_raw(
            &left_cas,
            &[shared.as_slice(), left.as_slice()],
            &ledger,
            &mut left_counters,
            &mut scratch,
        )
        .expect("build left pack");
        let (mut right_pack, _) = build_publication_pack_raw(
            &right_cas,
            &[shared.as_slice(), right.as_slice()],
            &ledger,
            &mut right_counters,
            &mut scratch,
        )
        .expect("build right pack");
        let start = Arc::new(WatchdogGateV1::new());
        let (ready_tx, ready_rx) = mpsc::sync_channel(2);
        let (left_result, right_result) = std::thread::scope(|scope| {
            let mut start_release = WatchdogGateReleaseV1::new(Arc::clone(&start));
            let left_start = Arc::clone(&start);
            let left_ready = ready_tx.clone();
            let left_ledger = &ledger;
            let left = scope.spawn(move || {
                let mut spool = PublicationSpool::default();
                let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
                left_ready.send(()).expect("left readiness receiver");
                left_start.wait();
                let result = left_cas.admit_pack(
                    &mut left_pack,
                    &mut spool,
                    left_ledger,
                    &mut left_counters,
                    &mut scratch,
                );
                (result, left_counters)
            });
            let right_start = Arc::clone(&start);
            let right_ledger = &ledger;
            let right = scope.spawn(move || {
                let mut spool = PublicationSpool::default();
                let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
                ready_tx.send(()).expect("right readiness receiver");
                right_start.wait();
                let result = right_cas.admit_pack(
                    &mut right_pack,
                    &mut spool,
                    right_ledger,
                    &mut right_counters,
                    &mut scratch,
                );
                (result, right_counters)
            });
            for _ in 0..2 {
                ready_rx
                    .recv_timeout(Duration::from_secs(5))
                    .expect("publication caller readiness");
            }
            start_release.release();
            (
                left.join().expect("left publication"),
                right.join().expect("right publication"),
            )
        });
        let (left_admission, left_counters) = left_result;
        let (right_admission, right_counters) = right_result;
        let mut occupied = seed.occupied().expect("open concurrent occupied reader");
        let shared_id = semantic_object_id(&shared);
        let mut shared_bytes = vec![0_u8; shared.len()];
        let shared_id_matches = occupied.occupied_len(shared_id).ok().flatten()
            == Some(shared.len() as u64)
            && occupied
                .read_occupied_exact_at(shared_id, 0, &mut shared_bytes)
                .is_ok()
            && shared_bytes == shared;
        let occupied_lengths_match = expected.iter().all(|(id, bytes)| {
            occupied.occupied_len(*id).expect("occupied length") == Some(bytes.len() as u64)
        });
        ReopenedPublicationObservationV1 {
            outcomes: [
                publication_outcome(left_admission.expect("left admission").outcome()),
                publication_outcome(right_admission.expect("right admission").outcome()),
            ],
            bytes_written: [
                left_counters.fscas_bytes_written,
                right_counters.fscas_bytes_written,
            ],
            zero_forbidden_work: [
                left_counters.has_zero_forbidden_work(),
                right_counters.has_zero_forbidden_work(),
            ],
            shared_id_matches,
            carrier_entries: directory_entries(root, "carriers").unwrap_or(0),
            catalog_entries: directory_entries(root, "catalog").unwrap_or(0),
            object_entries: directory_entries(root, "objects").unwrap_or(0),
            occupied_lengths_match,
            admitted_slots: ledger.admitted_slots(),
        }
    }

    pub fn locator_owner_wait_v1(root: &Path) -> LocatorOwnerWaitObservationV1 {
        let seed = FsCasV1::create_new(root).expect("create locator-owner root");
        let first_cas = FsCasV1::open_existing(root).expect("open first CAS");
        let second_cas = FsCasV1::open_existing(root).expect("open second CAS");
        let shared = closure_object(5, &[0x6e; 4_096]);
        let second_only = closure_object(5, b"second-pack-only");
        let expected = [
            (semantic_object_id(&shared), shared.as_slice()),
            (semantic_object_id(&second_only), second_only.as_slice()),
        ];
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut first_counters = OperationCountersV1::default();
        let mut second_counters = OperationCountersV1::default();
        let (mut first_pack, _) = build_publication_pack_raw(
            &first_cas,
            &[shared.as_slice()],
            &ledger,
            &mut first_counters,
            &mut scratch,
        )
        .expect("build first locator-owner pack");
        let (mut second_pack, _) = build_publication_pack_raw(
            &second_cas,
            &[shared.as_slice(), second_only.as_slice()],
            &ledger,
            &mut second_counters,
            &mut scratch,
        )
        .expect("build second locator-owner pack");
        let release = Arc::new(WatchdogGateV1::new());
        let (first_entered_tx, first_entered_rx) = mpsc::sync_channel(1);
        let (locator_wait_tx, locator_wait_rx) = mpsc::sync_channel(1);
        let (second_done_tx, second_done_rx) = mpsc::sync_channel(1);
        let (first_result, second_result, locator_wait_observed, completed_before_owner) =
            std::thread::scope(|scope| {
                let mut release_guard = WatchdogGateReleaseV1::new(Arc::clone(&release));
                let first_release = Arc::clone(&release);
                let first = scope.spawn(|| {
                    let mut spool = PublicationSpool::default();
                    let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
                    let mut control = BlockAfterLocatorPublicationV1 {
                        release: first_release,
                        entered: first_entered_tx,
                        blocked: false,
                    };
                    let (admission, observation) = {
                        let mut observed = FsOperationObservedControlV1::new(&mut control);
                        let admission = first_cas.admit_pack_controlled(
                            &mut first_pack,
                            &mut spool,
                            &ledger,
                            &mut first_counters,
                            &mut scratch,
                            &mut observed,
                        );
                        let observation = observed.finish_v1(&mut first_counters);
                        (admission, observation)
                    };
                    (admission, observation, control.blocked, first_counters)
                });
                first_entered_rx
                    .recv_timeout(Duration::from_secs(5))
                    .expect("first locator publication boundary");
                let second = scope.spawn(|| {
                    let mut spool = PublicationSpool::default();
                    let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
                    let mut control = SignalLocatorOwnerWaitV1 {
                        entered: Some(locator_wait_tx),
                    };
                    let (admission, observation) = {
                        let mut observed = FsOperationObservedControlV1::new(&mut control);
                        let admission = second_cas.admit_pack_controlled(
                            &mut second_pack,
                            &mut spool,
                            &ledger,
                            &mut second_counters,
                            &mut scratch,
                            &mut observed,
                        );
                        let observation = observed.finish_v1(&mut second_counters);
                        (admission, observation)
                    };
                    second_done_tx.send(()).expect("second completion receiver");
                    (admission, observation, second_counters)
                });
                let locator_wait_observed =
                    locator_wait_rx.recv_timeout(Duration::from_secs(5)).is_ok();
                let completed_before_owner =
                    second_done_rx.recv_timeout(Duration::from_millis(100));
                assert!(
                    completed_before_owner.is_err(),
                    "second pack completed before the locator owner made its catalog visible"
                );
                release_guard.release();
                (
                    first.join().expect("first locator-owner caller"),
                    second.join().expect("second locator-owner caller"),
                    locator_wait_observed,
                    completed_before_owner.is_ok(),
                )
            });
        let (first_admission, first_observation, first_blocked, first_counters) = first_result;
        let (second_admission, second_observation, second_counters) = second_result;
        let mut occupied = seed.occupied().expect("open locator-owner occupied reader");
        let occupied_lengths_match = expected.iter().all(|(id, bytes)| {
            occupied.occupied_len(*id).expect("occupied length") == Some(bytes.len() as u64)
        });
        LocatorOwnerWaitObservationV1 {
            locator_wait_observed,
            completed_before_owner,
            first_blocked,
            control_observations_clean: [first_observation.is_ok(), second_observation.is_ok()],
            outcomes: [
                publication_outcome(first_admission.expect("first admission").outcome()),
                publication_outcome(second_admission.expect("second admission").outcome()),
            ],
            publication_lock_acquisitions: [
                first_counters.publication_lock_acquisitions,
                second_counters.publication_lock_acquisitions,
            ],
            active_publication_wait_polls: second_counters.active_pack_publication_wait_polls,
            active_publication_wait_nanoseconds: second_counters
                .active_pack_publication_wait_nanoseconds,
            locator_owner_wait_polls: second_counters.locator_owner_publication_wait_polls,
            locator_owner_wait_nanoseconds: second_counters
                .locator_owner_publication_wait_nanoseconds,
            zero_forbidden_work: [
                first_counters.has_zero_forbidden_work(),
                second_counters.has_zero_forbidden_work(),
            ],
            admitted_slots: ledger.admitted_slots(),
            carrier_entries: directory_entries(root, "carriers").unwrap_or(0),
            catalog_entries: directory_entries(root, "catalog").unwrap_or(0),
            object_entries: directory_entries(root, "objects").unwrap_or(0),
            occupied_lengths_match,
        }
    }

    pub fn fresh_carrier_lock_scope_v1(root: &Path) -> PublicationLockScopeObservationV1 {
        let cas = FsCasV1::create_new(root).expect("create fresh-carrier root");
        let object = closure_object(5, &[0x4d; 32_768]);
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut counters = OperationCountersV1::default();
        let (mut pack, _) = build_publication_pack_raw(
            &cas,
            &[object.as_slice()],
            &ledger,
            &mut counters,
            &mut scratch,
        )
        .expect("build fresh-carrier pack");
        let mut spool = PublicationSpool::default();
        let mut control = ObservePublicationLockScopeV1 {
            cas: cas.clone(),
            fresh_carrier: true,
            observed: false,
            visibility_available: false,
            publication_available: false,
        };
        let outcome = publication_outcome(
            cas.admit_pack_controlled(
                &mut pack,
                &mut spool,
                &ledger,
                &mut counters,
                &mut scratch,
                &mut control,
            )
            .expect("admit fresh-carrier pack")
            .outcome(),
        );
        PublicationLockScopeObservationV1 {
            outcome,
            observed: control.observed,
            visibility_available: control.visibility_available,
            publication_available: control.publication_available,
        }
    }

    pub fn preparation_spool_lock_scope_v1(root: &Path) -> PreparationLockScopeObservationV1 {
        let cas = FsCasV1::create_new(root).expect("create preparation-lock root");
        let worker_cas = cas.clone();
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::channel();
        let (visibility_available_while_blocked, publication_available_while_blocked, blocked) =
            std::thread::scope(|scope| {
                let worker = scope.spawn(move || {
                    let mut control = BlockPreparationCreateV1 {
                        entered: entered_tx,
                        release: release_rx,
                        blocked: false,
                    };
                    let spool =
                        worker_cas.begin_operation_spool_v1("preparation-lock-scope", &mut control);
                    (spool, control)
                });
                entered_rx
                    .recv_timeout(Duration::from_secs(5))
                    .expect("preparation-create boundary");
                let visibility = cas.visibility_lock_available_for_test_v1();
                let publication = cas.publication_lock_available_for_test_v1();
                release_tx
                    .send(())
                    .expect("preparation worker remains live");
                let (spool, mut control) = worker.join().expect("preparation worker");
                let mut spool = spool.expect("begin operation spool");
                let blocked = control.blocked;
                spool
                    .cleanup_controlled_v1(&mut control)
                    .expect("cleanup operation spool");
                (visibility, publication, blocked)
            });
        PreparationLockScopeObservationV1 {
            visibility_available_while_blocked,
            publication_available_while_blocked,
            boundary_blocked: blocked,
            preparation_entries: directory_entries(root, "preparation").unwrap_or(0),
            visibility_available_after_cleanup: cas.visibility_lock_available_for_test_v1(),
            publication_available_after_cleanup: cas.publication_lock_available_for_test_v1(),
        }
    }

    pub fn disjoint_catalog_preparation_v1(root: &Path) -> DisjointPublicationObservationV1 {
        let seed = FsCasV1::create_new(root).expect("create catalog-preparation root");
        let first_cas = FsCasV1::open_existing(root).expect("open first catalog CAS");
        let second_cas = FsCasV1::open_existing(root).expect("open second catalog CAS");
        let first_object = closure_object(5, &[0x51; 8_192]);
        let second_object = closure_object(5, &[0x52; 8_192]);
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut first_counters = OperationCountersV1::default();
        let mut second_counters = OperationCountersV1::default();
        let (mut first_pack, _) = build_publication_pack_raw(
            &first_cas,
            &[first_object.as_slice()],
            &ledger,
            &mut first_counters,
            &mut scratch,
        )
        .expect("build first catalog pack");
        let (mut second_pack, _) = build_publication_pack_raw(
            &second_cas,
            &[second_object.as_slice()],
            &ledger,
            &mut second_counters,
            &mut scratch,
        )
        .expect("build second catalog pack");
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::channel();
        let (second_done_tx, second_done_rx) = mpsc::sync_channel(1);
        let (first_result, first_blocked, second_result, second_completed_before_release) =
            std::thread::scope(|scope| {
                let first = scope.spawn(|| {
                    let mut spool = PublicationSpool::default();
                    let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
                    let mut control = BlockCatalogMarkerWriteV1 {
                        entered: entered_tx,
                        release: release_rx,
                        catalog_phase: false,
                        blocked: false,
                    };
                    let result = first_cas.admit_pack_controlled(
                        &mut first_pack,
                        &mut spool,
                        &ledger,
                        &mut first_counters,
                        &mut scratch,
                        &mut control,
                    );
                    (result, control.blocked)
                });
                entered_rx
                    .recv_timeout(Duration::from_secs(5))
                    .expect("catalog marker preparation boundary");
                let second = scope.spawn(|| {
                    let mut spool = PublicationSpool::default();
                    let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
                    let result = second_cas.admit_pack(
                        &mut second_pack,
                        &mut spool,
                        &ledger,
                        &mut second_counters,
                        &mut scratch,
                    );
                    second_done_tx
                        .send(result)
                        .expect("second publication receiver");
                });
                let second_result = second_done_rx.recv_timeout(Duration::from_secs(5));
                let second_completed_before_release = second_result.is_ok();
                release_tx.send(()).expect("catalog worker remains live");
                let (first_result, first_blocked) = first.join().expect("first catalog caller");
                second.join().expect("second catalog caller");
                (
                    first_result,
                    first_blocked,
                    second_result.expect("disjoint publication completed"),
                    second_completed_before_release,
                )
            });
        let observation = DisjointPublicationObservationV1 {
            first_blocked,
            outcomes: [
                publication_outcome(first_result.expect("first catalog admission").outcome()),
                publication_outcome(second_result.expect("second catalog admission").outcome()),
            ],
            second_completed_before_release,
            carrier_entries: directory_entries(root, "carriers").unwrap_or(0),
            catalog_entries: directory_entries(root, "catalog").unwrap_or(0),
            admitted_slots: ledger.admitted_slots(),
            zero_forbidden_work: [
                first_counters.has_zero_forbidden_work(),
                second_counters.has_zero_forbidden_work(),
            ],
        };
        drop(seed);
        observation
    }

    pub fn same_pack_no_replace_v1(root: &Path) -> SamePackRaceObservationV1 {
        let cas = FsCasV1::create_new(root).expect("create same-pack root");
        let object = closure_object(5, &[0x5a; 32_768]);
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut first_counters = OperationCountersV1::default();
        let (mut first, _) = build_publication_pack_raw(
            &cas,
            &[object.as_slice()],
            &ledger,
            &mut first_counters,
            &mut scratch,
        )
        .expect("build first same-pack candidate");
        let mut spool = PublicationSpool::default();
        let installed = cas
            .admit_pack(
                &mut first,
                &mut spool,
                &ledger,
                &mut first_counters,
                &mut scratch,
            )
            .expect("admit first same-pack candidate");
        let pack_len = installed.sealed().pack_len();
        let carrier = one_entry(root, "carriers");
        #[cfg(unix)]
        let original_inode = {
            use std::os::unix::fs::MetadataExt;
            fs::metadata(&carrier).expect("carrier metadata").ino()
        };
        let mut second_counters = OperationCountersV1::default();
        let (mut second, _) = build_publication_pack_raw(
            &cas,
            &[object.as_slice()],
            &ledger,
            &mut second_counters,
            &mut scratch,
        )
        .expect("build second same-pack candidate");
        let mut control = ObservePublicationLockScopeV1 {
            cas: cas.clone(),
            fresh_carrier: false,
            observed: false,
            visibility_available: false,
            publication_available: false,
        };
        let outcome = publication_outcome(
            cas.admit_pack_controlled(
                &mut second,
                &mut spool,
                &ledger,
                &mut second_counters,
                &mut scratch,
                &mut control,
            )
            .expect("admit second same-pack candidate")
            .outcome(),
        );
        #[cfg(unix)]
        let carrier_identity_preserved = {
            use std::os::unix::fs::MetadataExt;
            fs::metadata(&carrier).expect("carrier metadata").ino() == original_inode
        };
        #[cfg(not(unix))]
        let carrier_identity_preserved = true;
        SamePackRaceObservationV1 {
            pack_len,
            comparison_observed: control.observed,
            visibility_available: control.visibility_available,
            publication_available: control.publication_available,
            outcome,
            incumbent_comparison_bytes: second_counters.incumbent_comparison_bytes,
            incumbent_comparison_windows: second_counters.incumbent_comparison_windows,
            carrier_entries: directory_entries(root, "carriers").unwrap_or(0),
            catalog_entries: directory_entries(root, "catalog").unwrap_or(0),
            preparation_entries: directory_entries(root, "preparation").unwrap_or(0),
            carrier_identity_preserved,
            zero_forbidden_work: second_counters.has_zero_forbidden_work(),
        }
    }

    fn concurrent_incumbent_case_v1(
        root: &Path,
        failure: ConcurrentIncumbentFailureV1,
    ) -> (bool, ConcurrentIncumbentCaseObservationV1) {
        let seed = FsCasV1::create_new(root).expect("create concurrent-incumbent root");
        let failing_cas = FsCasV1::open_existing(root).expect("open failing CAS");
        let success_cas = FsCasV1::open_existing(root).expect("open success CAS");
        let shared = closure_object(5, &[0x81; 4_096]);
        let shared_id = semantic_object_id(&shared);
        let disjoint = closure_object(5, &[0x82; 4_097]);
        let disjoint_id = semantic_object_id(&disjoint);
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut seed_counters = OperationCountersV1::default();
        let (mut seed_pack, _) = build_publication_pack_raw(
            &seed,
            &[shared.as_slice()],
            &ledger,
            &mut seed_counters,
            &mut scratch,
        )
        .expect("build incumbent seed");
        let mut seed_spool = PublicationSpool::default();
        let seed_installed = seed
            .admit_pack(
                &mut seed_pack,
                &mut seed_spool,
                &ledger,
                &mut seed_counters,
                &mut scratch,
            )
            .is_ok_and(|admission| {
                publication_outcome(admission.outcome()) == PublicationOutcomeV1::Installed
            });
        let incumbent_locator_path = publication_locator_path(root, shared_id);
        let incumbent_locator = fs::read(&incumbent_locator_path).expect("read incumbent locator");
        let incumbent_carrier = one_entry(root, "carriers");
        let original_permissions = make_owner_writable(&incumbent_carrier);
        let mut corrupted_carrier = fs::read(&incumbent_carrier).expect("read incumbent carrier");
        match failure {
            ConcurrentIncumbentFailureV1::UnequalCompleteBytes => {
                let object_offset = u64::from_be_bytes(
                    incumbent_locator[104..112]
                        .try_into()
                        .expect("locator object offset"),
                ) + 4;
                let object_len = u32::from_be_bytes(
                    incumbent_locator[112..116]
                        .try_into()
                        .expect("locator object length"),
                );
                let corrupt_at = usize::try_from(object_offset + u64::from(object_len) - 1)
                    .expect("carrier corruption offset");
                corrupted_carrier[corrupt_at] ^= 0xff;
            }
            ConcurrentIncumbentFailureV1::Malformed => corrupted_carrier[0] ^= 0xff,
        }
        fs::write(&incumbent_carrier, &corrupted_carrier).expect("corrupt incumbent carrier");
        fs::set_permissions(&incumbent_carrier, original_permissions)
            .expect("restore incumbent permissions");
        #[cfg(unix)]
        let incumbent_inode = {
            use std::os::unix::fs::MetadataExt;
            fs::metadata(&incumbent_carrier)
                .expect("incumbent carrier metadata")
                .ino()
        };
        let mut failure_counters = OperationCountersV1::default();
        let (mut failure_pack, _) = build_publication_pack_raw(
            &failing_cas,
            &[shared.as_slice()],
            &ledger,
            &mut failure_counters,
            &mut scratch,
        )
        .expect("build failing candidate");
        let mut success_counters = OperationCountersV1::default();
        let (mut success_pack, _) = build_publication_pack_raw(
            &success_cas,
            &[disjoint.as_slice()],
            &ledger,
            &mut success_counters,
            &mut scratch,
        )
        .expect("build disjoint candidate");
        let incumbent_gate = Arc::new(WatchdogGateV1::new());
        let (incumbent_entered_tx, incumbent_entered_rx) = mpsc::sync_channel(1);
        let (success_done_tx, success_done_rx) = mpsc::sync_channel(1);
        let (failure_result, success_result) = std::thread::scope(|scope| {
            let mut incumbent_release = WatchdogGateReleaseV1::new(Arc::clone(&incumbent_gate));
            let failure_gate = Arc::clone(&incumbent_gate);
            let failure = scope.spawn(|| {
                let mut spool = PublicationSpool::default();
                let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
                let mut control = BlockAtIncumbentAuthorityV1 {
                    release: failure_gate,
                    entered: Some(incumbent_entered_tx),
                };
                let (terminal, observation) = {
                    let mut observed = FsOperationObservedControlV1::new(&mut control);
                    let terminal = failing_cas.admit_pack_controlled(
                        &mut failure_pack,
                        &mut spool,
                        &ledger,
                        &mut failure_counters,
                        &mut scratch,
                        &mut observed,
                    );
                    let observation = observed.finish_v1(&mut failure_counters);
                    (terminal, observation)
                };
                (terminal, observation, failure_counters)
            });
            incumbent_entered_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("failing caller incumbent boundary");
            let success = scope.spawn(|| {
                let mut spool = PublicationSpool::default();
                let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
                let mut control = ContinueControlV1;
                let (terminal, observation) = {
                    let mut observed = FsOperationObservedControlV1::new(&mut control);
                    let terminal = success_cas.admit_pack_controlled(
                        &mut success_pack,
                        &mut spool,
                        &ledger,
                        &mut success_counters,
                        &mut scratch,
                        &mut observed,
                    );
                    let observation = observed.finish_v1(&mut success_counters);
                    (terminal, observation)
                };
                success_done_tx.send(()).expect("disjoint-success receiver");
                (terminal, observation, success_counters)
            });
            success_done_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("disjoint publication crossed incumbent validation");
            incumbent_release.release();
            (
                failure.join().expect("failing incumbent caller"),
                success.join().expect("successful disjoint caller"),
            )
        });
        let (failure_terminal, failure_observation, failure_counters) = failure_result;
        let (success_terminal, success_observation, success_counters) = success_result;
        for counters in [&failure_counters, &success_counters] {
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
        #[cfg(unix)]
        let incumbent_carrier_identity_preserved = {
            use std::os::unix::fs::MetadataExt;
            fs::metadata(&incumbent_carrier)
                .expect("incumbent carrier metadata")
                .ino()
                == incumbent_inode
        };
        #[cfg(not(unix))]
        let incumbent_carrier_identity_preserved = true;
        let disjoint_object_length_matches = seed
            .occupied()
            .expect("open disjoint occupied reader")
            .occupied_len(disjoint_id)
            .expect("disjoint occupied length")
            == Some(disjoint.len() as u64);
        (
            seed_installed,
            ConcurrentIncumbentCaseObservationV1 {
                failure,
                failure_error: failure_terminal.err().map(publication_error),
                failure_control_clean: failure_observation.is_ok(),
                success_outcome: publication_outcome(
                    success_terminal
                        .expect("successful disjoint admission")
                        .outcome(),
                ),
                success_control_clean: success_observation.is_ok(),
                storage_equations_hold: [
                    storage_equations_hold(&failure_counters),
                    storage_equations_hold(&success_counters),
                ],
                zero_forbidden_work: [
                    failure_counters.has_zero_forbidden_work(),
                    success_counters.has_zero_forbidden_work(),
                ],
                unreachable_residue_bytes: [
                    failure_counters.unreachable_installed_residue_bytes,
                    success_counters.unreachable_installed_residue_bytes,
                ],
                publication_lock_acquisitions: [
                    failure_counters.publication_lock_acquisitions,
                    success_counters.publication_lock_acquisitions,
                ],
                publication_lock_hold_nanoseconds: [
                    failure_counters.publication_lock_hold_nanoseconds,
                    success_counters.publication_lock_hold_nanoseconds,
                ],
                success_visibility_lock_acquisitions: success_counters.visibility_lock_acquisitions,
                success_visibility_lock_hold_nanoseconds: success_counters
                    .visibility_lock_hold_nanoseconds,
                incumbent_carrier_preserved: fs::read(&incumbent_carrier)
                    .is_ok_and(|bytes| bytes == corrupted_carrier),
                incumbent_locator_preserved: fs::read(&incumbent_locator_path)
                    .is_ok_and(|bytes| bytes == incumbent_locator),
                incumbent_carrier_identity_preserved,
                preparation_entries: directory_entries(root, "preparation").unwrap_or(0),
                carrier_entries: directory_entries(root, "carriers").unwrap_or(0),
                catalog_entries: directory_entries(root, "catalog").unwrap_or(0),
                object_entries: directory_entries(root, "objects").unwrap_or(0),
                admitted_slots: ledger.admitted_slots(),
                disjoint_object_length_matches,
            },
        )
    }

    pub fn simultaneous_disjoint_incumbents_v1(
        roots: [&Path; 2],
    ) -> ConcurrentIncumbentObservationV1 {
        let (unequal_seed, unequal) = concurrent_incumbent_case_v1(
            roots[0],
            ConcurrentIncumbentFailureV1::UnequalCompleteBytes,
        );
        let (malformed_seed, malformed) =
            concurrent_incumbent_case_v1(roots[1], ConcurrentIncumbentFailureV1::Malformed);
        ConcurrentIncumbentObservationV1 {
            seed_installed: unequal_seed && malformed_seed,
            cases: [unequal, malformed],
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum ReadObjectKindV1 {
        Chunk,
        File,
        Tree,
        Symlink,
        VersionRecord,
    }

    #[derive(Clone, Copy, Debug)]
    pub struct ReadRequestV1<'a> {
        kind: ReadObjectKindV1,
        expected: &'a [u8],
        occupied: &'a [u8],
        occupied_resident_bytes: u64,
        sink_resident_bytes: u64,
    }

    impl<'a> ReadRequestV1<'a> {
        pub const fn new(kind: ReadObjectKindV1, expected: &'a [u8]) -> Self {
            Self {
                kind,
                expected,
                occupied: expected,
                occupied_resident_bytes: 0,
                sink_resident_bytes: 0,
            }
        }

        pub const fn with_occupied(mut self, occupied: &'a [u8]) -> Self {
            self.occupied = occupied;
            self
        }

        pub const fn with_residency(mut self, occupied: u64, sink: u64) -> Self {
            self.occupied_resident_bytes = occupied;
            self.sink_resident_bytes = sink;
            self
        }
    }

    #[derive(Debug, Eq, PartialEq)]
    pub struct ReadObservationV1 {
        error: Option<CoreError>,
        id_matches_expected: bool,
        canonical_len: u64,
        output_len: u64,
        output_digest: [u8; 32],
        sink_finished: bool,
        sink_begins: u64,
        sink_aborts: u64,
        sink_writes: u64,
        sink_max_write: u64,
        occupied_lookups: u64,
        occupied_reads: u64,
        occupied_max_read: u64,
        bytes_read: u64,
        bytes_written: u64,
    }

    impl ReadObservationV1 {
        pub const fn error(&self) -> Option<CoreError> {
            self.error
        }

        pub const fn id_matches_expected(&self) -> bool {
            self.id_matches_expected
        }

        pub const fn canonical_len(&self) -> u64 {
            self.canonical_len
        }

        pub const fn output_len(&self) -> u64 {
            self.output_len
        }

        pub const fn output_digest(&self) -> [u8; 32] {
            self.output_digest
        }

        pub const fn sink_finished(&self) -> bool {
            self.sink_finished
        }

        pub const fn sink_begins(&self) -> u64 {
            self.sink_begins
        }

        pub const fn sink_aborts(&self) -> u64 {
            self.sink_aborts
        }

        pub const fn sink_writes(&self) -> u64 {
            self.sink_writes
        }

        pub const fn sink_max_write(&self) -> u64 {
            self.sink_max_write
        }

        pub const fn occupied_lookups(&self) -> u64 {
            self.occupied_lookups
        }

        pub const fn occupied_reads(&self) -> u64 {
            self.occupied_reads
        }

        pub const fn occupied_max_read(&self) -> u64 {
            self.occupied_max_read
        }

        pub const fn bytes_read(&self) -> u64 {
            self.bytes_read
        }

        pub const fn bytes_written(&self) -> u64 {
            self.bytes_written
        }
    }

    fn expected_id(kind: ReadObjectKindV1, bytes: &[u8]) -> CoreResult<TypedPhysicalObjectIdV1> {
        Ok(match kind {
            ReadObjectKindV1::Chunk => {
                TypedPhysicalObjectIdV1::Chunk(derive_physical_chunk_id_v1(bytes)?)
            }
            ReadObjectKindV1::File => {
                TypedPhysicalObjectIdV1::File(derive_physical_file_id_v1(bytes)?)
            }
            ReadObjectKindV1::Tree => {
                TypedPhysicalObjectIdV1::Tree(derive_physical_tree_id_v1(bytes)?)
            }
            ReadObjectKindV1::Symlink => {
                TypedPhysicalObjectIdV1::Symlink(derive_physical_symlink_id_v1(bytes)?)
            }
            ReadObjectKindV1::VersionRecord => {
                TypedPhysicalObjectIdV1::VersionRecord(derive_physical_version_record_id_v1(bytes)?)
            }
        })
    }

    #[derive(Debug)]
    struct Occupied<'a> {
        expected_id: TypedPhysicalObjectIdV1,
        bytes: &'a [u8],
        resident_bytes: u64,
        lookups: u64,
        reads: u64,
        maximum_read: usize,
    }

    impl Occupied<'_> {
        fn failure() -> ImmutablePortErrorV1 {
            ImmutablePortErrorV1::Failure
        }
    }

    impl OccupiedImmutableReadPortV1 for Occupied<'_> {
        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            Ok(self.resident_bytes)
        }

        fn occupied_len(
            &mut self,
            id: TypedPhysicalObjectIdV1,
        ) -> Result<Option<u64>, ImmutablePortErrorV1> {
            self.lookups = self.lookups.checked_add(1).ok_or_else(Self::failure)?;
            Ok((id == self.expected_id).then_some(self.bytes.len() as u64))
        }

        fn read_occupied_exact_at(
            &mut self,
            id: TypedPhysicalObjectIdV1,
            offset: u64,
            destination: &mut [u8],
        ) -> Result<(), ImmutablePortErrorV1> {
            if id != self.expected_id {
                return Err(Self::failure());
            }
            self.maximum_read = self.maximum_read.max(destination.len());
            self.reads = self.reads.checked_add(1).ok_or_else(Self::failure)?;
            let start = usize::try_from(offset).map_err(|_| Self::failure())?;
            let end = start
                .checked_add(destination.len())
                .ok_or_else(Self::failure)?;
            destination.copy_from_slice(self.bytes.get(start..end).ok_or_else(Self::failure)?);
            Ok(())
        }
    }

    #[derive(Default)]
    struct Sink<'a> {
        resident_bytes: u64,
        begins: u64,
        aborts: u64,
        writes: u64,
        maximum_write: usize,
        expected_id: Option<TypedPhysicalObjectIdV1>,
        expected_len: u64,
        bytes: Vec<u8>,
        finished: bool,
        writer: Option<&'a mut dyn std::io::Write>,
    }

    impl BoundedImmutableReadSinkV1 for Sink<'_> {
        fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
            Ok(self.resident_bytes)
        }

        fn begin_complete_immutable(
            &mut self,
            id: TypedPhysicalObjectIdV1,
            exact_len: u64,
        ) -> Result<(), ImmutablePortErrorV1> {
            self.begins = self.begins.checked_add(1).ok_or(Self::failure())?;
            self.expected_id = Some(id);
            self.expected_len = exact_len;
            Ok(())
        }

        fn write_complete_immutable(
            &mut self,
            fragment: &[u8],
        ) -> Result<(), ImmutablePortErrorV1> {
            if let Some(writer) = self.writer.as_mut() {
                writer.write_all(fragment).map_err(|_| Self::failure())?;
            }
            self.maximum_write = self.maximum_write.max(fragment.len());
            self.writes = self.writes.checked_add(1).ok_or(Self::failure())?;
            self.bytes.extend_from_slice(fragment);
            Ok(())
        }

        fn finish_complete_immutable(
            &mut self,
            id: TypedPhysicalObjectIdV1,
        ) -> Result<(), ImmutablePortErrorV1> {
            if self.expected_id != Some(id) || self.bytes.len() as u64 != self.expected_len {
                return Err(Self::failure());
            }
            self.finished = true;
            Ok(())
        }

        fn abort_complete_immutable(&mut self) {
            self.aborts += 1;
            self.bytes.clear();
            self.finished = false;
        }
    }

    impl Sink<'_> {
        fn failure() -> ImmutablePortErrorV1 {
            ImmutablePortErrorV1::Failure
        }
    }

    fn empty(error: Option<CoreError>) -> ReadObservationV1 {
        ReadObservationV1 {
            error,
            id_matches_expected: false,
            canonical_len: 0,
            output_len: 0,
            output_digest: [0; 32],
            sink_finished: false,
            sink_begins: 0,
            sink_aborts: 0,
            sink_writes: 0,
            sink_max_write: 0,
            occupied_lookups: 0,
            occupied_reads: 0,
            occupied_max_read: 0,
            bytes_read: 0,
            bytes_written: 0,
        }
    }

    pub fn read_v1(request: ReadRequestV1<'_>) -> ReadObservationV1 {
        read_v1_with_writer(request, None)
    }

    pub fn read_v1_to_writer(
        request: ReadRequestV1<'_>,
        writer: &mut dyn std::io::Write,
    ) -> ReadObservationV1 {
        read_v1_with_writer(request, Some(writer))
    }

    fn read_v1_with_writer(
        request: ReadRequestV1<'_>,
        writer: Option<&mut dyn std::io::Write>,
    ) -> ReadObservationV1 {
        let expected_id = match expected_id(request.kind, request.expected) {
            Ok(id) => id,
            Err(error) => return empty(Some(error)),
        };
        let mut occupied = Occupied {
            expected_id,
            bytes: request.occupied,
            resident_bytes: request.occupied_resident_bytes,
            lookups: 0,
            reads: 0,
            maximum_read: 0,
        };
        let mut sink = Sink {
            resident_bytes: request.sink_resident_bytes,
            writer,
            ..Sink::default()
        };
        let mut ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut counters = OperationCountersV1::default();
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        let result = read_complete_immutable_v1(
            expected_id,
            &mut occupied,
            &mut sink,
            &ledger,
            &mut counters,
            &mut scratch,
        );
        let (error, canonical_len, id_matches_expected) = match result {
            Ok(read) => (None, read.canonical_len(), true),
            Err(error) => (Some(error), 0, false),
        };
        let output_len = sink.bytes.len() as u64;
        let output_digest = if sink.bytes.is_empty() {
            [0; 32]
        } else {
            *blake3::hash(&sink.bytes).as_bytes()
        };
        let _ = &mut ledger;
        ReadObservationV1 {
            error,
            id_matches_expected,
            canonical_len,
            output_len,
            output_digest,
            sink_finished: sink.finished,
            sink_begins: sink.begins,
            sink_aborts: sink.aborts,
            sink_writes: sink.writes,
            sink_max_write: sink.maximum_write as u64,
            occupied_lookups: occupied.lookups,
            occupied_reads: occupied.reads,
            occupied_max_read: occupied.maximum_read as u64,
            bytes_read: counters.bytes_read,
            bytes_written: counters.bytes_written,
        }
    }
}
