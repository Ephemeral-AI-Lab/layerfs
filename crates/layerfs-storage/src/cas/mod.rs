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
        read_complete_immutable_v1, AdmissionBuffersV1, BoundedImmutableReadSinkV1,
        CompleteImmutableClosureReadPortV1, ImmutablePortErrorV1,
        FsCasBoundaryV1, FsCasCleanupTargetV1, FsCasControlV1, FsCasErrorV1,
        FsCasFailureCauseV1, FsCasFilesystemBoundaryV1, FsCasFilesystemFailureV1, FsCasV1,
        FsPrivatePackV1,
        OccupiedImmutableReadPortV1,
    };
    use crate::object::{decode_physical_object_v1, DiscardStrongEdgesV1};
    use crate::pack::{
        build_dense_pack_v1, PackIndexEntryV1, PackIndexSpoolV1, PackObjectSourceV1,
        PackPortErrorV1, PrivatePackPortV1,
    };
    use crate::identity::{
        derive_implicit_root_directory_v1, derive_physical_chunk_id_v1,
        derive_physical_file_id_v1, derive_physical_symlink_id_v1, derive_physical_tree_id_v1,
        derive_physical_version_record_id_v1, derive_version_v1, COMPARISON_WINDOW_BYTES,
    };
    use crate::limits::{OperationCountersV1, ResourceLedgerV1};
    use crate::object::TypedPhysicalObjectIdV1;
    use crate::profile::{ChunkerSpecV1, DigestSpecV1, ProfileSpecV1};
    use crate::{CoreError, CoreResult};
    use std::fs;
    use std::path::{Path, PathBuf};

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
        Cleanup,
        Invalidation,
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
        Cleanup(PublicationCleanupTargetV1),
        Invalidation,
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
    }

    /// The immutable facts retained by the three CAS fault-boundary owners.
    /// This is deliberately smaller than the filesystem engine: callers can
    /// assert typed failure, cleanup, incumbent custody, and counter
    /// invariants without receiving a CAS handle or a control object.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct FaultObservationV1 {
        error: Option<PublicationErrorV1>,
        preparation_entries: u64,
        carrier_entries: u64,
        object_entries: u64,
        catalog_entries: u64,
        residue_bytes: u64,
        bytes_written: u64,
        admitted_slots: u64,
        catalog_operations: u64,
        zero_forbidden_work: bool,
        source_bytes_read: u64,
        incumbent_comparison_bytes: u64,
        incumbent_comparison_windows: u64,
        incumbent_preserved: bool,
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
    /// counts let the integration owner assert every typed pair without
    /// receiving the private filesystem control or CAS handle.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ReadFaultObservationV1 {
        cases: u32,
        injected_cases: u32,
        expected_error_cases: u32,
        missing_occupant_cases: u32,
        permission_denied_cases: u32,
        read_failure_cases: u32,
        short_read_cases: u32,
        preparation_clean_cases: u32,
        carrier_preserved_cases: u32,
        catalog_preserved_cases: u32,
        object_cleanup_cases: u32,
        residue_free_cases: u32,
        slots_released_cases: u32,
        incumbent_usable_cases: u32,
        zero_forbidden_cases: u32,
    }

    impl ReadFaultObservationV1 {
        pub const fn cases(&self) -> u32 {
            self.cases
        }

        pub const fn injected_cases(&self) -> u32 {
            self.injected_cases
        }

        pub const fn expected_error_cases(&self) -> u32 {
            self.expected_error_cases
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

        pub const fn preparation_clean_cases(&self) -> u32 {
            self.preparation_clean_cases
        }

        pub const fn carrier_preserved_cases(&self) -> u32 {
            self.carrier_preserved_cases
        }

        pub const fn catalog_preserved_cases(&self) -> u32 {
            self.catalog_preserved_cases
        }

        pub const fn object_cleanup_cases(&self) -> u32 {
            self.object_cleanup_cases
        }

        pub const fn residue_free_cases(&self) -> u32 {
            self.residue_free_cases
        }

        pub const fn slots_released_cases(&self) -> u32 {
            self.slots_released_cases
        }

        pub const fn incumbent_usable_cases(&self) -> u32 {
            self.incumbent_usable_cases
        }

        pub const fn zero_forbidden_cases(&self) -> u32 {
            self.zero_forbidden_cases
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
        incumbent_preserved: bool,
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

        pub const fn incumbent_preserved(&self) -> bool {
            self.incumbent_preserved
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

        pub const fn closure_entries(&self) -> u64 {
            self.closure_entries
        }

        pub const fn closure_fences(&self) -> u64 {
            self.closure_fences
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
        metadata_error: Option<PublicationErrorV1>,
        metadata_bytes: u64,
        metadata_calls: u64,
        pack_error: Option<PublicationErrorV1>,
        pack_bytes: u64,
        pack_calls: u64,
        payload_error: Option<PublicationErrorV1>,
        payload_bytes: u64,
        payload_calls: u64,
        payload_prefix_preserved: bool,
        payload_object_cached: bool,
        root_clean: bool,
    }

    impl OccupiedOverflowObservationV1 {
        pub const fn metadata_error(&self) -> Option<PublicationErrorV1> {
            self.metadata_error
        }

        pub const fn metadata_bytes(&self) -> u64 {
            self.metadata_bytes
        }

        pub const fn metadata_calls(&self) -> u64 {
            self.metadata_calls
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

        pub const fn root_clean(&self) -> bool {
            self.root_clean
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
    }

    struct PublicationControl {
        target: u32,
        publications: u32,
        cancelled: bool,
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

    impl FsCasControlV1 for CandidateValidationFailureControl {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary == FsCasBoundaryV1::BeforeCandidateValidation && !self.injected {
                self.injected = true;
                self.cas
                    .fail_next_invalidation_probe_for_test_v1(
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

    fn candidate_validation_failure_v1(
        failure: CandidateValidationFailureV1,
    ) -> FsCasErrorV1 {
        FsCasErrorV1::Filesystem(match failure {
            CandidateValidationFailureV1::PermissionDenied => {
                FsCasFilesystemFailureV1::PermissionDenied
            }
            CandidateValidationFailureV1::ReadFailure => FsCasFilesystemFailureV1::ReadFailure,
        })
    }

    /// Fail the real invalidation probe immediately before candidate
    /// validation. The adapter exposes only the typed result and namespace
    /// custody; the probe control remains private to the CAS boundary.
    pub fn invalidation_probe_failure_before_candidate_validation_v1(
        request: PublicationRequestV1<'_>,
        failure: CandidateValidationFailureV1,
    ) -> FaultObservationV1 {
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
            let mut control = CandidateValidationFailureControl {
                cas: cas.clone(),
                failure: Some(candidate_validation_failure_v1(failure)),
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
        let version = closure_object(1, &payload);
        let version_id = TypedPhysicalObjectIdV1::VersionRecord(
            derive_physical_version_record_id_v1(&version)
                .expect("semantic closure version id"),
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
            destination.copy_from_slice(
                bytes
                    .get(start..end)
                    .ok_or(PackPortErrorV1::Failure)?,
            );
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
            FsCasErrorV1::CleanupFailed(_) => PublicationErrorV1::Cleanup,
            FsCasErrorV1::InvalidationFailed => PublicationErrorV1::Invalidation,
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
                PublicationCauseV1::Cleanup(publication_cleanup_target(target))
            }
            FsCasFailureCauseV1::InvalidationFailed => PublicationCauseV1::Invalidation,
        }
    }

    const fn publication_causes(
        error: FsCasErrorV1,
    ) -> (PublicationCauseV1, PublicationCauseV1) {
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

    /// Run the real filesystem admission path and stop after a requested
    /// locator publication. The returned observations are immutable scalar
    /// facts suitable for integration assertions.
    pub fn cancel_after_locator_publication_v1(
        request: PublicationRequestV1<'_>,
    ) -> PublicationObservationV1 {
        let mut counters = OperationCountersV1::default();
        let mut publications = 0;
        let error = (|| {
            let cas = FsCasV1::create_new(request.root).map_err(publication_error)?;
            let mut source = PublicationSource::new(request.objects)
                .map_err(PublicationErrorV1::Core)?;
            let mut pack: FsPrivatePackV1 = cas
                .begin_private_pack()
                .map_err(publication_error)?;
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
            residue_bytes: counters.unreachable_installed_residue_bytes,
            bytes_written: counters.fscas_bytes_written,
            admitted_slots: ledger.admitted_slots(),
            catalog_operations: counters.fscas_catalog_operations,
            zero_forbidden_work: counters.has_zero_forbidden_work(),
            source_bytes_read,
            incumbent_comparison_bytes,
            incumbent_comparison_windows,
            incumbent_preserved,
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
            injected_cases: 0,
            expected_error_cases: 0,
            missing_occupant_cases: 0,
            permission_denied_cases: 0,
            read_failure_cases: 0,
            short_read_cases: 0,
            preparation_clean_cases: 0,
            carrier_preserved_cases: 0,
            catalog_preserved_cases: 0,
            object_cleanup_cases: 0,
            residue_free_cases: 0,
            slots_released_cases: 0,
            incumbent_usable_cases: 0,
            zero_forbidden_cases: 0,
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

                if control.injected {
                    observation.injected_cases += 1;
                }
                if result == Err(error) {
                    observation.expected_error_cases += 1;
                }
                if directory_entries(request.root, "preparation") == Some(0) {
                    observation.preparation_clean_cases += 1;
                }
                if directory_entries(request.root, "carriers") == Some(1) {
                    observation.carrier_preserved_cases += 1;
                }
                if directory_entries(request.root, "catalog") == Some(1) {
                    observation.catalog_preserved_cases += 1;
                }
                if directory_entries(request.root, "objects") == Some(1) {
                    observation.object_cleanup_cases += 1;
                }
                if counters.unreachable_installed_residue_bytes == 0 {
                    observation.residue_free_cases += 1;
                }
                if ledger.admitted_slots() == 0 {
                    observation.slots_released_cases += 1;
                }
                if cas.occupied().is_ok() {
                    observation.incumbent_usable_cases += 1;
                }
                if counters.has_zero_forbidden_work() {
                    observation.zero_forbidden_cases += 1;
                }
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
        incumbent_preserved: bool,
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
            incumbent_preserved,
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
        let mut incumbent_preserved = false;
        let mut read_bytes_before = 0;
        let mut read_calls_before = 0;
        let result = (|| -> Result<(), FsCasErrorV1> {
            let cas = FsCasV1::create_new(request.root)?;
            let stale = FsCasV1::open_existing(request.root)?;
            let (mut incumbent, _) =
                build_publication_pack_raw(&cas, request.objects, &ledger, &mut incumbent_counters, &mut scratch)?;
            let mut spool = PublicationSpool::default();
            let installed = cas.admit_pack(
                &mut incumbent,
                &mut spool,
                &ledger,
                &mut incumbent_counters,
                &mut scratch,
            )?;
            installed_pack_len = installed.sealed().pack_len();

            let (mut candidate, _) =
                build_publication_pack_raw(&cas, request.objects, &ledger, &mut candidate_counters, &mut scratch)?;
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
            incumbent_preserved = directory_entries(request.root, "carriers") == Some(1)
                && directory_entries(request.root, "catalog") == Some(1)
                && cas.occupied().is_ok()
                && stale.occupied().is_ok();
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
            incumbent_preserved,
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
        let mut incumbent_preserved = false;
        let mut read_bytes_before = 0;
        let mut read_calls_before = 0;
        let result = (|| -> Result<(), FsCasErrorV1> {
            let cas = FsCasV1::create_new(request.root)?;
            let stale = FsCasV1::open_existing(request.root)?;
            let (mut incumbent, _) =
                build_publication_pack_raw(&cas, request.objects, &ledger, &mut incumbent_counters, &mut scratch)?;
            let mut spool = PublicationSpool::default();
            let installed = cas.admit_pack(
                &mut incumbent,
                &mut spool,
                &ledger,
                &mut incumbent_counters,
                &mut scratch,
            )?;
            installed_pack_len = installed.sealed().pack_len();
            let (mut candidate, _) =
                build_publication_pack_raw(&cas, request.objects, &ledger, &mut candidate_counters, &mut scratch)?;
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
            incumbent_preserved = directory_entries(request.root, "carriers") == Some(1)
                && directory_entries(request.root, "catalog") == Some(1)
                && cas.occupied().is_ok()
                && stale.occupied().is_ok();
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
            incumbent_preserved,
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
            let (mut incumbent, _) =
                build_publication_pack_raw(&cas, request.objects, &ledger, &mut incumbent_counters, &mut scratch)?;
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

            let (mut candidate, _) =
                build_publication_pack_raw(&cas, request.objects, &ledger, &mut candidate_counters, &mut scratch)?;
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
                .admit_pack(
                    &mut pack,
                    &mut spool,
                    &ledger,
                    &mut counters,
                    &mut scratch,
                )
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
            Ok(failure)
        })()
        .err();

        ClosureFailureObservationV1 {
            error,
            pack_len,
            record_count,
            residue_bytes: counters.unreachable_installed_residue_bytes,
            closure_entries: directory_entries(request.root, "closures").unwrap_or(0),
            closure_fences: counters.closure_fences,
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
                .map(|_| ())
                ;
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
    ) -> FaultObservationV1 {
        let mut counters = OperationCountersV1::default();
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut source_bytes_read = 0;
        let mut incumbent_preserved = false;
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

            let (mut candidate, candidate_read) = build_publication_pack_raw(
                &cas,
                request.objects,
                &ledger,
                &mut counters,
                &mut scratch,
            )?;
            source_bytes_read = source_bytes_read.saturating_add(candidate_read);
            let mut control =
                FaultControl::cancellation(FsCasBoundaryV1::AfterIncumbentComparisonWindow);
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
            incumbent_preserved = directory_entries(request.root, "carriers") == Some(1)
                && directory_entries(request.root, "catalog") == Some(1);
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
            incumbent_preserved,
        )
    }

    fn one_entry(root: &Path, name: &str) -> PathBuf {
        let mut entries = fs::read_dir(root.join(name))
            .expect("semantic fault fixture directory")
            .map(|entry| entry.expect("semantic fault fixture entry").path())
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 1, "semantic fault fixture entry count");
        entries.pop().expect("semantic fault fixture entry")
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

    fn publication_locator_path(
        root: &Path,
        id: TypedPhysicalObjectIdV1,
    ) -> PathBuf {
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
            residue_bytes: 0,
            bytes_written: 0,
            admitted_slots: 0,
            catalog_operations: 0,
            zero_forbidden_work: true,
            source_bytes_read: 0,
            incumbent_comparison_bytes: 0,
            incumbent_comparison_windows: 0,
            incumbent_preserved: false,
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

    /// Repeat the malformed-occupant race while injecting preparation-spool
    /// cleanup failure, preserving both causes in the bounded observation.
    pub fn atomic_locator_cleanup_failure_v1(
        request: PublicationRequestV1<'_>,
    ) -> FaultObservationV1 {
        let mut counters = OperationCountersV1::default();
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut source_bytes_read = 0;
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

    /// Preserve the three checked byte/call tuples from the occupied-reader
    /// overflow owner while returning only immutable semantic observations.
    pub fn occupied_locator_catalog_observation_overflow_v1(
        request: PublicationRequestV1<'_>,
    ) -> OccupiedOverflowObservationV1 {
        let object = match request.objects.first().copied() {
            Some(object) => object,
            None => {
                return OccupiedOverflowObservationV1 {
                    metadata_error: Some(PublicationErrorV1::Core(CoreError::Schema)),
                    metadata_bytes: 0,
                    metadata_calls: 0,
                    pack_error: None,
                    pack_bytes: 0,
                    pack_calls: 0,
                    payload_error: None,
                    payload_bytes: 0,
                    payload_calls: 0,
                    payload_prefix_preserved: false,
                    payload_object_cached: false,
                    root_clean: false,
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
            cas.admit_pack(
                &mut pack,
                &mut spool,
                &ledger,
                &mut counters,
                &mut scratch,
            )?;
            let id = publication_object_id(object)?;

            cas.seed_next_occupied_read_observation_for_test_v1(37, u64::MAX - 1);
            let mut metadata = cas.occupied_private_v1()?;
            let metadata_error = metadata
                .occupied_len_typed_v1(id)
                .err()
                .map(publication_error);
            let (metadata_bytes, metadata_calls) = metadata
                .direct_storage_read_observation_typed_v1()?;
            drop(metadata);

            cas.seed_next_occupied_read_observation_for_test_v1(53, u64::MAX - 2);
            let mut pack_observation = cas.occupied_private_v1()?;
            let pack_error = pack_observation
                .occupied_len_typed_v1(id)
                .err()
                .map(publication_error);
            let (pack_bytes, pack_calls) = pack_observation
                .direct_storage_read_observation_typed_v1()?;
            drop(pack_observation);

            cas.seed_next_occupied_payload_read_observation_for_test_v1(71, u64::MAX);
            let mut payload = cas.occupied_private_v1()?;
            let payload_cached_before_read = payload
                .occupied_len_typed_v1(id)?
                .is_some()
                && payload.resolved_object_cached_for_test_v1(id);
            let mut prefix = [0_u8; 11];
            let payload_error = payload
                .read_occupied_exact_at_typed_v1(id, 0, &mut prefix)
                .err()
                .map(publication_error);
            let (payload_bytes, payload_calls) = payload
                .direct_storage_read_observation_typed_v1()?;
            let payload_object_cached = payload.resolved_object_cached_for_test_v1(id);
            let payload_prefix_preserved = payload_cached_before_read && prefix == object[..11];
            drop(payload);

            let root_clean = FsCasV1::open_existing(request.root).is_ok()
                && directory_entries(request.root, "preparation") == Some(0)
                && ledger.admitted_slots() == 0;
            Ok(OccupiedOverflowObservationV1 {
                metadata_error,
                metadata_bytes,
                metadata_calls,
                pack_error,
                pack_bytes,
                pack_calls,
                payload_error,
                payload_bytes,
                payload_calls,
                payload_prefix_preserved,
                payload_object_cached,
                root_clean,
            })
        })();
        match result {
            Ok(observation) => observation,
            Err(error) => OccupiedOverflowObservationV1 {
                metadata_error: Some(publication_error(error)),
                metadata_bytes: 0,
                metadata_calls: 0,
                pack_error: None,
                pack_bytes: 0,
                pack_calls: 0,
                payload_error: None,
                payload_bytes: 0,
                payload_calls: 0,
                payload_prefix_preserved: false,
                payload_object_cached: false,
                root_clean: false,
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
                .admit_pack(
                    &mut pack,
                    &mut spool,
                    &ledger,
                    &mut counters,
                    &mut scratch,
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

    pub fn carrier_cleanup_failure_v1(request: PublicationRequestV1<'_>) -> FaultObservationV1 {
        controlled_cleanup_failure_v1(
            request,
            FsCasBoundaryV1::AfterCarrierInstall,
            FsCasCleanupTargetV1::Carrier,
        )
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
                bytes[56..64].try_into().expect("semantic carrier index offset"),
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

    #[derive(Debug, Default)]
    struct Sink {
        resident_bytes: u64,
        begins: u64,
        aborts: u64,
        writes: u64,
        maximum_write: usize,
        expected_id: Option<TypedPhysicalObjectIdV1>,
        expected_len: u64,
        bytes: Vec<u8>,
        finished: bool,
    }

    impl BoundedImmutableReadSinkV1 for Sink {
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

    impl Sink {
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
