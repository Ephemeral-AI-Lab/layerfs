//! Filesystem-backed immutable pack carrier and catalog fence.
//!
//! `FsCasV1` owns one private local-filesystem namespace. Pack payload bytes
//! are written once to an operation-private file and are installed by a
//! same-filesystem hard-link transfer; no whole-pack copy or memory payload
//! backend exists here. A small catalog marker is the visibility point after
//! the installed carrier has been reopened and completely validated.

use std::alloc::Layout;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, TryLockError, Weak};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::catalog::{decode_catalog_marker, encode_catalog_marker, CATALOG_MARKER_BYTES};
use super::locator::{
    decode_persistent_locator_v1, encode_persistent_locator_v1, PersistentLocatorCodecErrorV1,
    PersistentObjectLocatorV1, PERSISTENT_LOCATOR_BYTES_V1,
};
#[cfg(test)]
use crate::cas::admit_complete_immutable_v1;
use crate::cas::{
    AdmissionBuffersV1, AdmittedClosureV1, CompleteImmutableClosureReadPortV1,
    ImmutablePortErrorV1, OccupiedImmutableReadPortV1, PreparedImmutableClosurePortV1,
    ValidatedOccupiedObjectV1,
};
use crate::identity::{PackIdV1, PhysicalVersionRecordIdV1, COMPARISON_WINDOW_BYTES};
use crate::limits::{
    admitted_slots_for_budget, OperationCountersV1, OperationMemoryPlanV1, OperationReservationV1,
    ResourceLedgerV1, BASE_LEDGER_BYTES, MEMORY_PROFILE_72_MIB,
};
use crate::object::TypedPhysicalObjectIdV1;
#[cfg(test)]
use crate::pack::validate_pack_v1;
use crate::pack::{
    locate_validated_pack_index_entry_v1, read_validated_pack_index_entry_v1,
    validate_validated_pack_object_v1, PackIndexEntryV1, PackIndexSpoolV1, PackObjectLocationV1,
    PackPortErrorV1, PackReadPortV1, PrivatePackPortV1, SealedPackV1, MAX_PACK_BYTES,
};
use crate::{CoreError, CoreResult};

const GENERATION_MAGIC: &[u8; 8] = b"LFSGEN01";
const GENERATION_MARKER_BYTES: usize = 40;
const CLOSURE_MAGIC: &[u8; 8] = b"LFSCLO01";
pub(crate) const CLOSURE_MARKER_BYTES: usize = 120;
const INVALIDATED_ROOT_NAME: &str = "invalidated";
const ROOT_OWNER_NAME: &str = "owner";
const ROOT_OWNER_MAGIC: &[u8; 8] = b"LFSOWN01";
const ROOT_OWNER_BYTES: usize = 48;
const ROOT_OWNER_STATE_ACTIVE: u8 = 1;
const ROOT_OWNER_STATE_INVALIDATED: u8 = 2;
const MAX_ADMISSION_TICKETS: usize = 1_024;
const ADMISSION_CONTROL_POLL: Duration = Duration::from_millis(2);
/// Separate root-owned Phase-1 logical namespace budgets. These fixed values
/// bound LayerFS' own admitted growth; they are deliberately not claims about
/// filesystem free blocks or per-user quota, whose availability remains a
/// separately typed observation.
pub(crate) const ROOT_LOGICAL_STORAGE_BUDGET_V1: u64 = 512 * 1_024 * 1_024 * 1_024;
pub(crate) const ROOT_NAMESPACE_ENTRY_BUDGET_V1: u64 = 256 * 1_024 * 1_024;
const ROOT_STORAGE_OPERATION_SLOTS_V1: usize = 16;
const PERSISTENT_LOCATOR_BYTES_U64_V1: u64 = PERSISTENT_LOCATOR_BYTES_V1 as u64;

static NEXT_PRIVATE_NAME: AtomicU64 = AtomicU64::new(1);
static NEXT_CLOSURE_OPERATION: AtomicU64 = AtomicU64::new(1);
static NEXT_STORAGE_OWNER_INSTANCE: AtomicU64 = AtomicU64::new(1);
static OPEN_ROOTS: OnceLock<Mutex<HashMap<PathBuf, Weak<FsCasInnerV1>>>> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FsCasErrorV1 {
    Unsupported,
    /// Another process owns the root, or a failed invalidation retained the
    /// exclusive owner token so the damaged root cannot reopen as valid.
    Busy,
    Invalidated,
    /// An authoritative root-owned synchronization primitive was poisoned.
    /// The operation that discovers the poison returns this initiating cause
    /// after fail-closed invalidation; later operations observe `Invalidated`.
    SynchronizationPoisoned,
    /// An operation-local storage authority was presented to a different
    /// shared root owner or to a later owner generation.  This is distinct
    /// from a stale/replayed nonce within the correct owner.
    CrossOwner,
    /// A live operation authority was presented to a boundary owned by a
    /// different operation kind. Owner, generation, slot, and nonce may all
    /// match; kind is an independent part of the authority identity.
    WrongOperationKind,
    /// Explicit cleanup failed at the named owned lifecycle boundary. The
    /// shared root has already entered the fail-closed invalid state.
    CleanupFailed(FsCasCleanupTargetV1),
    /// Neither the pre-existing owner record nor the descriptive invalidation
    /// marker could be transitioned and verified. The retained owner record
    /// still prevents adoption, but the persistence transition itself failed.
    InvalidationFailed,
    /// An existing locator, catalog, carrier, or closure occupant is
    /// truncated or fails its canonical structural validation.
    MalformedOccupant,
    /// A locator, carrier, catalog entry, or closure object required by an
    /// already accepted root is absent.
    MissingOccupant,
    /// An existing complete occupant has the requested typed identity but
    /// different canonical bytes.
    UnequalOccupant,
    Io,
    Integrity,
    Collision,
    /// The shared root's fixed-profile resource domain refused admission
    /// before any operation-owned state was created.
    ResourceExhausted(FsCasResourceV1),
    /// A filesystem operation failed with a distinction that is required for
    /// private lifecycle/resource qualification. Generic platform errors use
    /// `Io`; these variants are never inferred from logical byte counters.
    Filesystem(FsCasFilesystemFailureV1),
    Core(CoreError),
    /// A later cleanup or invalidation failure is terminally dominant, while
    /// the first meaningful typed cause remains available for diagnosis.
    /// This is a bounded two-cause record, not a retry/error stack.
    TerminalFailure {
        first: FsCasFailureCauseV1,
        dominant: FsCasFailureCauseV1,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FsCasResourceV1 {
    Memory,
    Queue,
    StorageBytes,
    StorageInodes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FsCasFilesystemFailureV1 {
    NoSpace,
    Quota,
    InodeExhaustion,
    ReadFailure,
    WriteFailure,
    ShortRead,
    ShortWrite,
    PermissionDenied,
}

/// Non-recursive typed cause stored by [`FsCasErrorV1::TerminalFailure`].
/// It mirrors every scalar FsCas error so a cleanup/invalidation double fault
/// never erases the original provenance or allocates an unbounded cause chain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FsCasFailureCauseV1 {
    Unsupported,
    Busy,
    Invalidated,
    SynchronizationPoisoned,
    CrossOwner,
    WrongOperationKind,
    CleanupFailed(FsCasCleanupTargetV1),
    InvalidationFailed,
    MalformedOccupant,
    MissingOccupant,
    UnequalOccupant,
    Io,
    Integrity,
    Collision,
    ResourceExhausted(FsCasResourceV1),
    Filesystem(FsCasFilesystemFailureV1),
    Core(CoreError),
}

impl FsCasErrorV1 {
    const fn first_cause_v1(self) -> FsCasFailureCauseV1 {
        match self {
            Self::TerminalFailure { first, .. } => first,
            Self::Unsupported => FsCasFailureCauseV1::Unsupported,
            Self::Busy => FsCasFailureCauseV1::Busy,
            Self::Invalidated => FsCasFailureCauseV1::Invalidated,
            Self::SynchronizationPoisoned => FsCasFailureCauseV1::SynchronizationPoisoned,
            Self::CrossOwner => FsCasFailureCauseV1::CrossOwner,
            Self::WrongOperationKind => FsCasFailureCauseV1::WrongOperationKind,
            Self::CleanupFailed(target) => FsCasFailureCauseV1::CleanupFailed(target),
            Self::InvalidationFailed => FsCasFailureCauseV1::InvalidationFailed,
            Self::MalformedOccupant => FsCasFailureCauseV1::MalformedOccupant,
            Self::MissingOccupant => FsCasFailureCauseV1::MissingOccupant,
            Self::UnequalOccupant => FsCasFailureCauseV1::UnequalOccupant,
            Self::Io => FsCasFailureCauseV1::Io,
            Self::Integrity => FsCasFailureCauseV1::Integrity,
            Self::Collision => FsCasFailureCauseV1::Collision,
            Self::ResourceExhausted(resource) => FsCasFailureCauseV1::ResourceExhausted(resource),
            Self::Filesystem(failure) => FsCasFailureCauseV1::Filesystem(failure),
            Self::Core(error) => FsCasFailureCauseV1::Core(error),
        }
    }

    pub(crate) const fn dominant_cause_v1(self) -> FsCasFailureCauseV1 {
        match self {
            Self::TerminalFailure { dominant, .. } => dominant,
            scalar => scalar.first_cause_v1(),
        }
    }

    pub const fn failure_causes_v1(self) -> (FsCasFailureCauseV1, FsCasFailureCauseV1) {
        (self.first_cause_v1(), self.dominant_cause_v1())
    }

    pub(crate) fn dominated_by_v1(self, dominant: Self) -> Self {
        // Only explicit cleanup and persistent-invalidation failure may
        // replace the terminal classification while retaining the initiating
        // cause. Counter/observation, I/O, integrity, and resource failures
        // that happen later remain secondary diagnostics and cannot be
        // promoted into the dominant slot by a careless caller.
        if dominant.has_cleanup_or_invalidation_dominance_v1()
            && self.dominant_cause_v1() != dominant.dominant_cause_v1()
        {
            Self::TerminalFailure {
                first: self.first_cause_v1(),
                dominant: dominant.dominant_cause_v1(),
            }
        } else {
            self
        }
    }

    pub(crate) const fn has_cleanup_or_invalidation_dominance_v1(self) -> bool {
        matches!(
            self.dominant_cause_v1(),
            FsCasFailureCauseV1::CleanupFailed(_) | FsCasFailureCauseV1::InvalidationFailed
        )
    }

    pub(crate) const fn has_invalidation_dominance_v1(self) -> bool {
        matches!(
            self.dominant_cause_v1(),
            FsCasFailureCauseV1::InvalidationFailed
        )
    }
}

/// Concrete filesystem event at which a deterministic qualification fault may
/// be injected. The hook is private with the rest of FsCas; production control
/// implementations return `None` and execute the real portable operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FsCasFilesystemBoundaryV1 {
    PreparationCreate,
    PreparationResize,
    PreparationWrite,
    PrivatePackCreate,
    PrivatePackResize,
    PrivatePackWrite,
    PrivatePackFlush,
    PermissionChange,
    CarrierHardLink,
    CarrierAliasUnlink,
    MarkerCreate,
    MarkerWrite,
    MarkerFlush,
    MarkerHardLink,
    MarkerAliasUnlink,
    /// Validate the root namespace before reopening an existing CAS. This is
    /// a semantic root-authority read boundary, not a native syscall counter.
    RootValidationRead,
    /// Read and authenticate the root generation marker before reopening an
    /// existing CAS. This is a semantic storage-path boundary, not a native
    /// syscall counter.
    GenerationMarkerRead,
    /// Read the canonical persistent object locator required to resolve an
    /// occupied immutable object. This is a semantic storage-path boundary,
    /// not a native syscall counter.
    ObjectLocatorRead,
    /// Read the canonical catalog marker that binds an object locator to its
    /// immutable carrier. This is a semantic storage-path boundary, not a
    /// native syscall counter.
    CatalogMarkerRead,
    /// Re-read and authenticate the canonical catalog marker after the
    /// visibility fence is reacquired for an equal-incumbent reuse result.
    /// This is a distinct authority revalidation from the earlier optimistic
    /// catalog read, not a native syscall counter.
    CatalogMarkerRevalidationRead,
    /// Open and inspect the immutable carrier metadata after locator/catalog
    /// authentication. This is a semantic storage-path boundary, not a
    /// native syscall counter.
    CarrierMetadataRead,
    /// Validate the immutable carrier index entry for an occupied object.
    /// This is a semantic storage-path boundary, not a native syscall
    /// counter.
    CarrierIndexRead,
    /// Validate the immutable carrier object record after its index lookup.
    /// This is a semantic storage-path boundary, not a native syscall
    /// counter.
    CarrierObjectRead,
    /// Read and authenticate the complete-closure marker before admitting a
    /// read operation. This is a semantic storage-path boundary, not a
    /// native syscall counter.
    ClosureMarkerRead,
    /// Read authenticated object payload bytes from an already validated
    /// immutable carrier. This is a semantic storage-path boundary, not a
    /// native syscall counter.
    CarrierPayloadRead,
    /// Read one bounded candidate/incumbent pair while proving canonical
    /// immutable equality. This is a semantic comparison-read event, not a
    /// native syscall counter.
    IncumbentComparisonRead,
    CarrierUnlink,
    LocatorUnlink,
    InvalidationWrite,
    InvalidationFlush,
    InvalidationMarkerCreate,
}

/// Test-only arithmetic boundary for proving that visible immutable custody
/// remains dependency-safe when a direct residue observation cannot be
/// recorded. Production has no injectable counter path.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FsCasResidueAccountingBoundaryV1 {
    CatalogMarker,
    ObjectLocator,
    Carrier,
}

impl From<CoreError> for FsCasErrorV1 {
    fn from(error: CoreError) -> Self {
        Self::Core(error)
    }
}

/// Operation-local cooperative stop signal for filesystem admission.
///
/// The interface is statically dispatched and carries no scheduler, retry, or
/// fallback policy into the storage adapter.
pub trait FsCasControlV1 {
    fn boundary_reached(&mut self, _boundary: FsCasBoundaryV1) {}
    fn cancellation_requested(&mut self) -> bool;
    fn deadline_exceeded(&mut self) -> bool;
    fn inject_cleanup_failure(&mut self, _target: FsCasCleanupTargetV1) -> bool {
        false
    }
    fn inject_filesystem_failure(
        &mut self,
        _boundary: FsCasFilesystemBoundaryV1,
    ) -> Option<FsCasErrorV1> {
        None
    }
    #[cfg(test)]
    fn inject_residue_accounting_failure(
        &mut self,
        _boundary: FsCasResidueAccountingBoundaryV1,
    ) -> bool {
        false
    }
    /// Test-only proof hook for the otherwise-unreachable case where an
    /// operation terminal callback unwinds after both owned capability halves
    /// have been released successfully. Production controls have no such
    /// callback and compile without this method.
    #[cfg(test)]
    fn inject_operation_terminal_unwind_after_release(&mut self) -> bool {
        false
    }
    /// Test-only direct-observation failure. It proves that a terminal unwind
    /// cannot replace the machine-readable observation result with a string
    /// panic. Production observations always come from `RootLockObservationV1`.
    #[cfg(test)]
    fn inject_root_lock_observation_failure(&mut self) -> Option<CoreError> {
        None
    }
    /// Test-only proof hook for the checked per-carrier counter transfer that
    /// follows an FsCas admission callback unwind. Production controls cannot
    /// alter the accumulator and compile without this method.
    #[cfg(test)]
    fn inject_carrier_counter_accumulation_overflow(&mut self) -> bool {
        false
    }
    /// Test-only proof hook for a checked tally failure *after* immutable
    /// admission has made a carrier, its locators, and its catalog marker
    /// visible. Production controls cannot alter the tally; the hook proves
    /// that exact operation-relative residue is transferred before the typed
    /// arithmetic terminal leaves the pack sink.
    #[cfg(test)]
    fn inject_post_admission_carrier_tally_overflow(&mut self) -> bool {
        false
    }
    /// Test-only proof hook for the checked global-seen observation transfer.
    /// Production controls cannot alter the accumulator and compile without
    /// this method.
    #[cfg(test)]
    fn inject_global_seen_counter_accumulation_overflow(&mut self) -> bool {
        false
    }
    /// Test-only proof hook for the checked aggregate/physical/kind object
    /// disposition transaction. Production controls cannot alter counters and
    /// compile without this method.
    #[cfg(test)]
    fn inject_pack_object_disposition_overflow(&mut self, _created: bool) -> bool {
        false
    }
    /// Test-only proof hook for the checked direct write-byte observation on
    /// an operation preparation spool. The hook fires only after the real
    /// write succeeds, so tests can prove that the typed observation failure
    /// does not masquerade as structural corruption or disturb cleanup.
    #[cfg(test)]
    fn inject_operation_spool_write_observation_overflow(&mut self) -> bool {
        false
    }
    /// Test-only proof hook for the checksum reader wrapped around a private
    /// pack. The hook fires immediately before the real checksum read, so a
    /// late call-count overflow can prove that the byte/call observation is
    /// indivisible and its exact checked-arithmetic cause survives the neutral
    /// pack-port error.
    #[cfg(test)]
    fn inject_counted_pack_read_observation_overflow(&mut self) -> bool {
        false
    }
    /// Test-only proof hook for the direct private-pack reads used to compare
    /// a same-carrier incumbent with a new candidate. The hook fires only
    /// after both real reads complete, so a late call-count overflow proves
    /// that their byte/call observation commits as one transaction.
    #[cfg(test)]
    fn inject_same_carrier_comparison_observation_overflow(&mut self) -> bool {
        false
    }
    /// Test-only proof hook for the otherwise-unreachable checked transition
    /// that advances a cancelled queue ticket after a control callback
    /// unwinds. Production controls cannot alter root queue state.
    #[cfg(test)]
    fn inject_pending_unwind_retirement_failure(&mut self) -> Option<FsCasErrorV1> {
        None
    }
    /// Test-only proof hook for invalidation discovered after the root mutex
    /// has been acquired but before the acquisition is exposed to its caller.
    /// Production validation always comes from `FsCasV1::ensure_valid`.
    #[cfg(test)]
    fn inject_root_lock_post_acquire_validation_failure(&mut self) -> Option<FsCasErrorV1> {
        None
    }
}

impl<T: FsCasControlV1 + ?Sized> crate::limits::OperationWorkControlV1 for T {
    fn cancellation_requested_v1(&mut self) -> bool {
        self.cancellation_requested()
    }

    fn deadline_exceeded_v1(&mut self) -> bool {
        self.deadline_exceeded()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FsCasCleanupTargetV1 {
    /// Rollback of a newly created root or its not-yet-accepted owner token.
    /// This precedes any usable FsCas handle or operation admission.
    RootInitialization,
    ObjectLocator,
    Carrier,
    PrivatePack,
    PreparationSpool,
    /// The private hard-link alias after its immutable destination became
    /// visible. Failure here is post-publication and must never be treated as
    /// an unpublished marker.
    PublishedMarkerAlias,
    /// The pre-existing root owner record and its descriptive invalidation
    /// marker. Fault injection here models failure of the first persistent
    /// invalidation action plus allocation failure for the marker fallback.
    RootInvalidation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FsCasBoundaryV1 {
    /// Exact complete-C3 benchmark start: immediately before the one
    /// orchestrator-owned operation slot is requested.
    BeforeOperationSlotReservationRequest,
    /// Exact complete-C3 benchmark end: the validated handoff is complete,
    /// all private preparation cleanup succeeded, and the slot is still held.
    AfterCompleteValidatedHandoff,
    /// The operation requested the short shared-root visibility fence.
    VisibilityLockRequested,
    /// The caller thread observed the shared-root visibility mutex busy and
    /// is about to poll cancellation/deadline before another bounded wait.
    VisibilityLockContended,
    /// The caller thread acquired the shared-root visibility mutex and
    /// revalidated root state. This is an observation boundary only.
    VisibilityLockAcquired,
    /// A visibility-fence wait ended without acquiring the mutex because the
    /// operation was cancelled, exceeded its deadline, observed invalidation,
    /// or found poisoned coordination state.
    VisibilityLockWaitTerminated,
    /// The short visibility fence was explicitly released.
    VisibilityLockReleased,
    /// The operation requested the writer publication transaction mutex.
    PublicationLockRequested,
    /// The caller thread observed the shared-root writer transaction mutex
    /// busy and is about to poll cancellation/deadline.
    PublicationLockContended,
    /// The caller thread acquired the writer transaction mutex and
    /// revalidated root state.
    PublicationLockAcquired,
    /// A publication-lock wait ended without an acquisition.
    PublicationLockWaitTerminated,
    /// The writer publication transaction mutex was explicitly released.
    PublicationLockReleased,
    BeforeCandidateValidation,
    AfterCandidateValidation,
    BeforeCarrierInstall,
    AfterCarrierInstall,
    AfterCarrierValidation,
    AfterCarrierMadeImmutable,
    BeforeObjectLocatorRead,
    AfterObjectLocatorRead,
    AfterObjectIncumbentValidation,
    BeforeObjectComparisonWindow,
    AfterObjectComparisonWindow,
    BeforeObjectLocatorPublication,
    /// An object-locator destination exists while its preparation alias is
    /// still present. Cancellation is not sampled after this visibility
    /// transition; the boundary exists for deterministic cleanup injection.
    AfterObjectLocatorMarkerLink,
    AfterObjectLocatorPublication,
    BeforeCatalogPublication,
    /// The catalog destination exists, while its preparation alias is still
    /// present. This boundary exists only for deterministic lifecycle fault
    /// injection; cancellation is not sampled after visibility.
    AfterCatalogMarkerLink,
    AfterCatalogPublication,
    /// The complete closure was absent from the authenticated snapshot and
    /// the implementation is about to attempt the atomic no-replace link.
    /// Tests use this boundary to install a racing incumbent; production must
    /// classify that occupant from the hard-link result and authenticate it.
    BeforeClosureMarkerPublication,
    /// The closure destination exists, while its preparation alias is still
    /// present. This boundary exists only for deterministic lifecycle fault
    /// injection; cancellation is not sampled after visibility.
    AfterClosureMarkerLink,
    BeforeIncumbentMarkerRead,
    AfterIncumbentMarkerRead,
    AfterIncumbentValidation,
    BeforeIncumbentComparisonWindow,
    AfterIncumbentComparisonWindow,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ContinueFsCasControlV1;

impl FsCasControlV1 for ContinueFsCasControlV1 {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

#[derive(Default)]
struct RootLockObservationV1 {
    visibility_requested: Option<Instant>,
    visibility_acquired: Option<Instant>,
    visibility_acquisitions: u64,
    visibility_contended_polls: u64,
    visibility_wait_nanoseconds: u64,
    visibility_hold_nanoseconds: u64,
    visibility_maximum_hold_nanoseconds: u64,
    publication_requested: Option<Instant>,
    publication_acquired: Option<Instant>,
    publication_acquisitions: u64,
    publication_contended_polls: u64,
    publication_wait_nanoseconds: u64,
    publication_hold_nanoseconds: u64,
    publication_maximum_hold_nanoseconds: u64,
    invalid: bool,
}

impl RootLockObservationV1 {
    fn duration_nanoseconds_v1(started: Instant) -> Option<u64> {
        u64::try_from(started.elapsed().as_nanos()).ok()
    }

    fn add_v1(target: &mut u64, value: Option<u64>, invalid: &mut bool) {
        let Some(value) = value else {
            *invalid = true;
            return;
        };
        let Some(total) = target.checked_add(value) else {
            *invalid = true;
            return;
        };
        *target = total;
    }

    fn increment_v1(target: &mut u64, invalid: &mut bool) {
        let Some(total) = target.checked_add(1) else {
            *invalid = true;
            return;
        };
        *target = total;
    }

    fn boundary_reached_v1(&mut self, boundary: FsCasBoundaryV1) {
        match boundary {
            FsCasBoundaryV1::VisibilityLockRequested => {
                if self.visibility_requested.replace(Instant::now()).is_some()
                    || self.visibility_acquired.is_some()
                {
                    self.invalid = true;
                }
            }
            FsCasBoundaryV1::VisibilityLockContended => {
                Self::increment_v1(&mut self.visibility_contended_polls, &mut self.invalid);
            }
            FsCasBoundaryV1::VisibilityLockAcquired => {
                let requested = self.visibility_requested.take();
                if requested.is_none() || self.visibility_acquired.is_some() {
                    self.invalid = true;
                }
                if let Some(requested) = requested {
                    Self::add_v1(
                        &mut self.visibility_wait_nanoseconds,
                        Self::duration_nanoseconds_v1(requested),
                        &mut self.invalid,
                    );
                }
                Self::increment_v1(&mut self.visibility_acquisitions, &mut self.invalid);
                self.visibility_acquired = Some(Instant::now());
            }
            FsCasBoundaryV1::VisibilityLockWaitTerminated => {
                let requested = self.visibility_requested.take();
                if requested.is_none() || self.visibility_acquired.is_some() {
                    self.invalid = true;
                }
                if let Some(requested) = requested {
                    Self::add_v1(
                        &mut self.visibility_wait_nanoseconds,
                        Self::duration_nanoseconds_v1(requested),
                        &mut self.invalid,
                    );
                }
            }
            FsCasBoundaryV1::VisibilityLockReleased => {
                let acquired = self.visibility_acquired.take();
                if acquired.is_none() || self.visibility_requested.is_some() {
                    self.invalid = true;
                }
                if let Some(acquired) = acquired {
                    let elapsed = Self::duration_nanoseconds_v1(acquired);
                    Self::add_v1(
                        &mut self.visibility_hold_nanoseconds,
                        elapsed,
                        &mut self.invalid,
                    );
                    if let Some(elapsed) = elapsed {
                        self.visibility_maximum_hold_nanoseconds =
                            self.visibility_maximum_hold_nanoseconds.max(elapsed);
                    }
                }
            }
            FsCasBoundaryV1::PublicationLockRequested => {
                if self.publication_requested.replace(Instant::now()).is_some()
                    || self.publication_acquired.is_some()
                {
                    self.invalid = true;
                }
            }
            FsCasBoundaryV1::PublicationLockContended => {
                Self::increment_v1(&mut self.publication_contended_polls, &mut self.invalid);
            }
            FsCasBoundaryV1::PublicationLockAcquired => {
                let requested = self.publication_requested.take();
                if requested.is_none() || self.publication_acquired.is_some() {
                    self.invalid = true;
                }
                if let Some(requested) = requested {
                    Self::add_v1(
                        &mut self.publication_wait_nanoseconds,
                        Self::duration_nanoseconds_v1(requested),
                        &mut self.invalid,
                    );
                }
                Self::increment_v1(&mut self.publication_acquisitions, &mut self.invalid);
                self.publication_acquired = Some(Instant::now());
            }
            FsCasBoundaryV1::PublicationLockWaitTerminated => {
                let requested = self.publication_requested.take();
                if requested.is_none() || self.publication_acquired.is_some() {
                    self.invalid = true;
                }
                if let Some(requested) = requested {
                    Self::add_v1(
                        &mut self.publication_wait_nanoseconds,
                        Self::duration_nanoseconds_v1(requested),
                        &mut self.invalid,
                    );
                }
            }
            FsCasBoundaryV1::PublicationLockReleased => {
                let acquired = self.publication_acquired.take();
                if acquired.is_none() || self.publication_requested.is_some() {
                    self.invalid = true;
                }
                if let Some(acquired) = acquired {
                    let elapsed = Self::duration_nanoseconds_v1(acquired);
                    Self::add_v1(
                        &mut self.publication_hold_nanoseconds,
                        elapsed,
                        &mut self.invalid,
                    );
                    if let Some(elapsed) = elapsed {
                        self.publication_maximum_hold_nanoseconds =
                            self.publication_maximum_hold_nanoseconds.max(elapsed);
                    }
                }
            }
            _ => {}
        }
    }

    fn finish_v1(self, counters: &mut OperationCountersV1) -> CoreResult<()> {
        if self.invalid
            || self.visibility_requested.is_some()
            || self.visibility_acquired.is_some()
            || self.publication_requested.is_some()
            || self.publication_acquired.is_some()
        {
            return Err(CoreError::PackInvalid);
        }
        counters.record_root_lock_observations_v1(
            self.visibility_acquisitions,
            self.visibility_contended_polls,
            self.visibility_wait_nanoseconds,
            self.visibility_hold_nanoseconds,
            self.visibility_maximum_hold_nanoseconds,
            self.publication_acquisitions,
            self.publication_contended_polls,
            self.publication_wait_nanoseconds,
            self.publication_hold_nanoseconds,
            self.publication_maximum_hold_nanoseconds,
        )
    }
}

/// Operation-local control adapter for direct lock provenance. It allocates
/// no heap state and delegates every stop/fault decision to the caller's
/// existing control. The adapter is private and cannot mint an operation.
pub(crate) struct FsOperationObservedControlV1<'control, C: ?Sized> {
    inner: &'control mut C,
    locks: RootLockObservationV1,
}

impl<'control, C: ?Sized> FsOperationObservedControlV1<'control, C> {
    pub(crate) fn new(inner: &'control mut C) -> Self {
        Self {
            inner,
            locks: RootLockObservationV1::default(),
        }
    }

    pub(crate) fn inner_mut_v1(&mut self) -> &mut C {
        self.inner
    }

    pub(crate) fn finish_v1(self, counters: &mut OperationCountersV1) -> CoreResult<()>
    where
        C: FsCasControlV1,
    {
        #[cfg(test)]
        if let Some(error) = self.inner.inject_root_lock_observation_failure() {
            return Err(error);
        }
        self.locks.finish_v1(counters)
    }
}

impl<C: FsCasControlV1 + ?Sized> FsCasControlV1 for FsOperationObservedControlV1<'_, C> {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        self.locks.boundary_reached_v1(boundary);
        self.inner.boundary_reached(boundary);
    }

    fn cancellation_requested(&mut self) -> bool {
        self.inner.cancellation_requested()
    }

    fn deadline_exceeded(&mut self) -> bool {
        self.inner.deadline_exceeded()
    }

    fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
        self.inner.inject_cleanup_failure(target)
    }

    fn inject_filesystem_failure(
        &mut self,
        boundary: FsCasFilesystemBoundaryV1,
    ) -> Option<FsCasErrorV1> {
        self.inner.inject_filesystem_failure(boundary)
    }

    #[cfg(test)]
    fn inject_residue_accounting_failure(
        &mut self,
        boundary: FsCasResidueAccountingBoundaryV1,
    ) -> bool {
        self.inner.inject_residue_accounting_failure(boundary)
    }

    #[cfg(test)]
    fn inject_operation_terminal_unwind_after_release(&mut self) -> bool {
        self.inner.inject_operation_terminal_unwind_after_release()
    }

    #[cfg(test)]
    fn inject_root_lock_observation_failure(&mut self) -> Option<CoreError> {
        self.inner.inject_root_lock_observation_failure()
    }

    #[cfg(test)]
    fn inject_pending_unwind_retirement_failure(&mut self) -> Option<FsCasErrorV1> {
        self.inner.inject_pending_unwind_retirement_failure()
    }

    #[cfg(test)]
    fn inject_root_lock_post_acquire_validation_failure(&mut self) -> Option<FsCasErrorV1> {
        self.inner
            .inject_root_lock_post_acquire_validation_failure()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FsPackAdmissionOutcomeV1 {
    Installed,
    ExistingComplete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FsPackAdmissionV1 {
    outcome: FsPackAdmissionOutcomeV1,
    sealed: SealedPackV1,
    installed_residue_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CarrierPublicationCustodyV1 {
    Absent,
    InstalledUnreported,
    RetainedAndRecorded,
    ReturnedInstalled,
}

/// Exact custody for object-locator names made visible by one carrier
/// publication transaction. Visibility is recorded at the successful
/// no-replace link, before any fallible observation counter or callback can
/// return control to the semantic owner.
#[derive(Default)]
struct LocatorPublicationCustodyV1 {
    live_unclassified: u64,
    retained_and_recorded: u64,
}

impl LocatorPublicationCustodyV1 {
    fn mark_visible_v1(&mut self) {
        // A pack contains at most `u32::MAX` records and each publication loop
        // visits an ordinal once, so this `u64` increment is preflighted by the
        // validated pack shape and cannot overflow.
        self.live_unclassified += 1;
    }

    fn mark_removed_v1(&mut self) -> Result<(), FsCasErrorV1> {
        self.live_unclassified = self
            .live_unclassified
            .checked_sub(1)
            .ok_or(FsCasErrorV1::Integrity)?;
        Ok(())
    }

    fn retain_one_v1(&mut self, counters: &mut OperationCountersV1) -> Result<(), FsCasErrorV1> {
        counters.record_unreachable_installed_residue(PERSISTENT_LOCATOR_BYTES_U64_V1)?;
        self.live_unclassified = self
            .live_unclassified
            .checked_sub(1)
            .ok_or(FsCasErrorV1::Integrity)?;
        self.retained_and_recorded = self
            .retained_and_recorded
            .checked_add(1)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        Ok(())
    }

    fn retain_all_live_v1(
        &mut self,
        counters: &mut OperationCountersV1,
    ) -> Result<(), FsCasErrorV1> {
        if self.live_unclassified == 0 {
            return Ok(());
        }
        let bytes = self
            .live_unclassified
            .checked_mul(PERSISTENT_LOCATOR_BYTES_U64_V1)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        counters.record_unreachable_installed_residue(bytes)?;
        self.retained_and_recorded = self
            .retained_and_recorded
            .checked_add(self.live_unclassified)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        self.live_unclassified = 0;
        Ok(())
    }

    const fn has_retained_v1(&self) -> bool {
        self.retained_and_recorded != 0
    }

    const fn requires_carrier_retention_v1(&self) -> bool {
        self.has_retained_v1() || self.live_unclassified != 0
    }

    fn take_live_bytes_v1(&mut self) -> Result<u64, FsCasErrorV1> {
        let bytes = self
            .live_unclassified
            .checked_mul(PERSISTENT_LOCATOR_BYTES_U64_V1)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        self.live_unclassified = 0;
        Ok(bytes)
    }
}

/// Exact custody for a small immutable marker after its no-replace link is
/// visible and before its semantic owner has returned a terminal result.  The
/// transition is recorded before any post-link callback can unwind.
#[derive(Default)]
struct ImmutableMarkerCustodyV1 {
    live_unclassified_bytes: u64,
    retained_and_recorded_bytes: u64,
}

impl ImmutableMarkerCustodyV1 {
    fn mark_visible_v1(&mut self, bytes: u64) {
        // One custody value owns at most one fixed-size marker. The caller
        // derives `bytes` from a validated array length before publication.
        debug_assert_eq!(self.live_unclassified_bytes, 0);
        self.live_unclassified_bytes = bytes;
    }

    fn retain_live_v1(&mut self, counters: &mut OperationCountersV1) -> Result<bool, FsCasErrorV1> {
        if self.live_unclassified_bytes == 0 {
            return Ok(false);
        }
        counters.record_unreachable_installed_residue(self.live_unclassified_bytes)?;
        self.retained_and_recorded_bytes = self
            .retained_and_recorded_bytes
            .checked_add(self.live_unclassified_bytes)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        self.live_unclassified_bytes = 0;
        Ok(true)
    }

    fn take_live_bytes_v1(&mut self) -> u64 {
        core::mem::take(&mut self.live_unclassified_bytes)
    }
}

#[cfg(feature = "c3-polymorphism")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FsClosureAdmissionErrorV1 {
    Core(CoreError),
    FsCas(FsCasErrorV1),
}

impl FsPackAdmissionV1 {
    pub const fn outcome(self) -> FsPackAdmissionOutcomeV1 {
        self.outcome
    }

    pub const fn sealed(self) -> SealedPackV1 {
        self.sealed
    }

    /// Account immutable carrier bytes that became unreachable only after a
    /// successful install and a later closure or authority failure.
    pub fn record_later_unreachable_residue(
        self,
        counters: &mut OperationCountersV1,
    ) -> CoreResult<()> {
        counters.record_unreachable_installed_residue(self.installed_residue_bytes)?;
        Ok(())
    }

    pub(crate) const fn installed_residue_bytes_v1(self) -> u64 {
        self.installed_residue_bytes
    }
}

/// Storage-local proof that one typed closure passed complete validation.
///
/// This is neither a Workspace Version nor a publication/authority decision;
/// only the later owner-controlled authority compare-and-swap can make the
/// candidate a committed/published Version.
pub struct CompleteValidatedClosureV1 {
    owner: FsCasV1,
    generation: [u8; 32],
    operation_nonce: u64,
    version_record: TypedPhysicalObjectIdV1,
    object_count: u64,
    transcript: [u8; 32],
    consumed: bool,
}

impl CompleteValidatedClosureV1 {
    pub fn version_record(&self) -> Result<TypedPhysicalObjectIdV1, FsCasErrorV1> {
        self.owner.ensure_valid()?;
        Ok(self.version_record)
    }

    pub fn object_count(&self) -> Result<u64, FsCasErrorV1> {
        self.owner.ensure_valid()?;
        Ok(self.object_count)
    }
}

impl core::fmt::Debug for CompleteValidatedClosureV1 {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("CompleteValidatedClosureV1")
            .field("generation", &self.generation)
            .field("operation_nonce", &self.operation_nonce)
            .field("version_record", &self.version_record)
            .field("object_count", &self.object_count)
            .field("transcript", &self.transcript)
            .field("consumed", &self.consumed)
            .finish()
    }
}

impl PartialEq for CompleteValidatedClosureV1 {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.owner.inner, &other.owner.inner)
            && self.generation == other.generation
            && self.operation_nonce == other.operation_nonce
            && self.version_record == other.version_record
            && self.object_count == other.object_count
            && self.transcript == other.transcript
            && self.consumed == other.consumed
    }
}

impl Eq for CompleteValidatedClosureV1 {}

/// One-shot storage-local operation context. It is opaque and non-cloneable;
/// a closure capability minted for one context cannot cross into another.
pub struct FsClosureOperationV1 {
    owner: FsCasV1,
    generation: [u8; 32],
    nonce: u64,
    storage_token: Option<FsStorageOperationTokenV1>,
    marker_custody: ImmutableMarkerCustodyV1,
    admission_started: bool,
    admitted: bool,
    consumed: bool,
}

#[derive(Clone)]
pub struct FsCasV1 {
    inner: Arc<FsCasInnerV1>,
}

struct FsCasInnerV1 {
    root: PathBuf,
    generation: [u8; 32],
    invalidated: AtomicBool,
    #[cfg(test)]
    invalidation_probe_failure: Mutex<Option<FsCasErrorV1>>,
    /// A pre-created, pre-opened cross-process ownership token. Invalidation
    /// mutates this existing allocation in place and retains its pathname;
    /// therefore ENOSPC while creating the optional descriptive marker can
    /// never make the root reopen as valid.
    ownership: Mutex<Option<File>>,
    operation_ledger: ResourceLedgerV1,
    operation_admission: OperationAdmissionQueueV1,
    /// Logical file-length and namespace-entry admission is independent from
    /// the 72 MiB userspace ledger. It is shared by every alias/reopen of this
    /// root and has one fixed state cell per admitted operation.
    storage_admission: RootStorageAdmissionV1,
    /// Short namespace snapshot/commit fence. No carrier validation,
    /// incumbent comparison, or payload read may run while this is held.
    visibility: Mutex<()>,
    /// Same-process writer transaction serialization for the filesystem's
    /// multi-name carrier/locator/catalog install. This is deliberately
    /// separate from `visibility`, so readers and namespace snapshots are not
    /// monopolized by validation or comparison work. Its scope is narrowed
    /// further as publication-state coordination becomes independently
    /// representable.
    publication: Mutex<()>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct RootStorageUsageV1 {
    bytes: u64,
    inodes: u64,
}

/// Allocation-independent logical namespace headroom for the durable root
/// invalidation barrier. The owner record is created and opened before the
/// root becomes usable; this additional name covers the secondary
/// `invalidated` marker without consulting host free-inode or quota state.
/// It is fixed root state, never attributed to an operation as residue.
const ROOT_INVALIDATION_BARRIER_RESERVATION_V1: RootStorageUsageV1 = RootStorageUsageV1 {
    bytes: 0,
    inodes: 1,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FsStorageEnvelopeV1 {
    preparation_bytes: u64,
    immutable_bytes: u64,
    preparation_inodes: u64,
    immutable_inodes: u64,
}

impl FsStorageEnvelopeV1 {
    pub(crate) fn new(
        preparation_bytes: u64,
        immutable_bytes: u64,
        preparation_inodes: u64,
        immutable_inodes: u64,
    ) -> Result<Self, CoreError> {
        preparation_bytes
            .checked_add(immutable_bytes)
            .ok_or(CoreError::IntegerOverflow)?;
        preparation_inodes
            .checked_add(immutable_inodes)
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(Self {
            preparation_bytes,
            immutable_bytes,
            preparation_inodes,
            immutable_inodes,
        })
    }

    const fn requested_bytes(self) -> u64 {
        self.preparation_bytes + self.immutable_bytes
    }

    const fn requested_inodes(self) -> u64 {
        self.preparation_inodes + self.immutable_inodes
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct RootStorageOperationStateV1 {
    active: bool,
    nonce: u64,
    operation_kind: u64,
    envelope: Option<FsStorageEnvelopeV1>,
    preparation_current: RootStorageUsageV1,
    preparation_high_water: RootStorageUsageV1,
    immutable_pending: RootStorageUsageV1,
}

/// Bounded custody retained after a generation-matched operation cell is
/// retired but its contribution cannot be added to one or both root-wide
/// `u64` aggregates.  The root is invalidated before control returns, so this
/// record is diagnostic fail-closed custody rather than reusable authority.
/// Keeping one fixed cell per storage slot avoids fabricating a numeric total
/// and prevents arithmetic failure from leaving the active operation live.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct RootStorageUnclassifiedTerminalV1 {
    occupied: bool,
    nonce: u64,
    reservation: RootStorageUsageV1,
    immutable: RootStorageUsageV1,
    preparation: RootStorageUsageV1,
    reservation_known: bool,
    active_reserved_rebuilt: bool,
    immutable_aggregated: bool,
    preparation_aggregated: bool,
}

struct RootStorageAdmissionStateV1 {
    immutable: RootStorageUsageV1,
    preparation: RootStorageUsageV1,
    active_reserved: RootStorageUsageV1,
    reserved_high_water: RootStorageUsageV1,
    next_nonce: u64,
    operations: [RootStorageOperationStateV1; ROOT_STORAGE_OPERATION_SLOTS_V1],
    unclassified_terminals: [RootStorageUnclassifiedTerminalV1; ROOT_STORAGE_OPERATION_SLOTS_V1],
}

struct RootStorageAdmissionV1 {
    identity: FsStorageOwnerIdentityV1,
    byte_capacity: u64,
    inode_capacity: u64,
    fixed_reservation: RootStorageUsageV1,
    state: Mutex<RootStorageAdmissionStateV1>,
    #[cfg(test)]
    poison_next_immutable_remove: AtomicBool,
    #[cfg(test)]
    fail_next_preparation_remove: AtomicBool,
}

enum RootStorageFinishV1 {
    Complete,
    TerminalizedError(FsCasErrorV1),
}

/// Exact shared-owner identity for operation-local storage authority.  The
/// persistent generation rejects cross-generation use; the process-local
/// instance rejects a stale authority after close-all/reopen even when the
/// on-disk generation is unchanged.  Neither value is caller-mintable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FsStorageOwnerIdentityV1 {
    generation: [u8; 32],
    instance: u64,
}

/// Unforgeable, generation-checked identity for the storage half of one
/// already-admitted root operation. It is never exposed outside the crate and
/// carries no authority to reserve another operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FsStorageOperationTokenV1 {
    owner: FsStorageOwnerIdentityV1,
    slot: u8,
    nonce: u64,
    operation_kind: FsOperationKindV1,
}

impl RootStorageAdmissionV1 {
    fn new(
        immutable: RootStorageUsageV1,
        preparation: RootStorageUsageV1,
        generation: [u8; 32],
    ) -> Result<Self, FsCasErrorV1> {
        Self::new_with_capacities_for_generation(
            immutable,
            preparation,
            ROOT_LOGICAL_STORAGE_BUDGET_V1,
            ROOT_NAMESPACE_ENTRY_BUDGET_V1,
            generation,
        )
    }

    #[cfg(test)]
    fn new_with_capacities(
        immutable: RootStorageUsageV1,
        preparation: RootStorageUsageV1,
        byte_capacity: u64,
        inode_capacity: u64,
    ) -> Result<Self, FsCasErrorV1> {
        Self::new_with_capacities_for_generation(
            immutable,
            preparation,
            byte_capacity,
            inode_capacity,
            [0; 32],
        )
    }

    fn new_with_capacities_for_generation(
        immutable: RootStorageUsageV1,
        preparation: RootStorageUsageV1,
        byte_capacity: u64,
        inode_capacity: u64,
        generation: [u8; 32],
    ) -> Result<Self, FsCasErrorV1> {
        let used_bytes = immutable
            .bytes
            .checked_add(preparation.bytes)
            .and_then(|value| value.checked_add(ROOT_INVALIDATION_BARRIER_RESERVATION_V1.bytes))
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        let used_inodes = immutable
            .inodes
            .checked_add(preparation.inodes)
            .and_then(|value| value.checked_add(ROOT_INVALIDATION_BARRIER_RESERVATION_V1.inodes))
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        if used_bytes > byte_capacity {
            return Err(FsCasErrorV1::ResourceExhausted(
                FsCasResourceV1::StorageBytes,
            ));
        }
        if used_inodes > inode_capacity {
            return Err(FsCasErrorV1::ResourceExhausted(
                FsCasResourceV1::StorageInodes,
            ));
        }
        let instance = NEXT_STORAGE_OWNER_INSTANCE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        Ok(Self {
            identity: FsStorageOwnerIdentityV1 {
                generation,
                instance,
            },
            byte_capacity,
            inode_capacity,
            fixed_reservation: ROOT_INVALIDATION_BARRIER_RESERVATION_V1,
            state: Mutex::new(RootStorageAdmissionStateV1 {
                immutable,
                preparation,
                active_reserved: RootStorageUsageV1::default(),
                reserved_high_water: RootStorageUsageV1::default(),
                next_nonce: 1,
                operations: [RootStorageOperationStateV1::default();
                    ROOT_STORAGE_OPERATION_SLOTS_V1],
                unclassified_terminals: [RootStorageUnclassifiedTerminalV1::default();
                    ROOT_STORAGE_OPERATION_SLOTS_V1],
            }),
            #[cfg(test)]
            poison_next_immutable_remove: AtomicBool::new(false),
            #[cfg(test)]
            fail_next_preparation_remove: AtomicBool::new(false),
        })
    }

    #[cfg(test)]
    fn reserve(
        &self,
        envelope: FsStorageEnvelopeV1,
    ) -> Result<RootStorageLeaseV1<'_>, FsCasErrorV1> {
        self.reserve_for_operation_v1(envelope, FsOperationKindV1::CompleteC3File)
    }

    fn reserve_for_operation_v1(
        &self,
        envelope: FsStorageEnvelopeV1,
        operation_kind: FsOperationKindV1,
    ) -> Result<RootStorageLeaseV1<'_>, FsCasErrorV1> {
        let requested = RootStorageUsageV1 {
            bytes: envelope.requested_bytes(),
            inodes: envelope.requested_inodes(),
        };
        let mut state = self
            .state
            .lock()
            .map_err(|_| FsCasErrorV1::SynchronizationPoisoned)?;
        if state
            .unclassified_terminals
            .iter()
            .any(|terminal| terminal.occupied)
        {
            return Err(FsCasErrorV1::Integrity);
        }
        let next_bytes = state
            .immutable
            .bytes
            .checked_add(state.preparation.bytes)
            .and_then(|value| value.checked_add(self.fixed_reservation.bytes))
            .and_then(|value| value.checked_add(state.active_reserved.bytes))
            .and_then(|value| value.checked_add(requested.bytes))
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        if next_bytes > self.byte_capacity {
            return Err(FsCasErrorV1::ResourceExhausted(
                FsCasResourceV1::StorageBytes,
            ));
        }
        let next_inodes = state
            .immutable
            .inodes
            .checked_add(state.preparation.inodes)
            .and_then(|value| value.checked_add(self.fixed_reservation.inodes))
            .and_then(|value| value.checked_add(state.active_reserved.inodes))
            .and_then(|value| value.checked_add(requested.inodes))
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        if next_inodes > self.inode_capacity {
            return Err(FsCasErrorV1::ResourceExhausted(
                FsCasResourceV1::StorageInodes,
            ));
        }
        let slot = state
            .operations
            .iter()
            .position(|operation| !operation.active)
            .ok_or(FsCasErrorV1::Integrity)?;
        let nonce = state.next_nonce;
        state.next_nonce = state
            .next_nonce
            .checked_add(1)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        state.operations[slot] = RootStorageOperationStateV1 {
            active: true,
            nonce,
            operation_kind: operation_kind as u64,
            envelope: Some(envelope),
            preparation_current: RootStorageUsageV1::default(),
            preparation_high_water: RootStorageUsageV1::default(),
            immutable_pending: RootStorageUsageV1::default(),
        };
        state.active_reserved.bytes = state
            .active_reserved
            .bytes
            .checked_add(requested.bytes)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        state.active_reserved.inodes = state
            .active_reserved
            .inodes
            .checked_add(requested.inodes)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        state.reserved_high_water.bytes = state
            .reserved_high_water
            .bytes
            .max(state.active_reserved.bytes);
        state.reserved_high_water.inodes = state
            .reserved_high_water
            .inodes
            .max(state.active_reserved.inodes);
        Ok(RootStorageLeaseV1 {
            admission: self,
            slot,
            nonce,
            operation_kind,
            released: false,
        })
    }

    fn operation_mut_v1<'a>(
        &self,
        state: &'a mut RootStorageAdmissionStateV1,
        token: FsStorageOperationTokenV1,
    ) -> Result<&'a mut RootStorageOperationStateV1, FsCasErrorV1> {
        if token.owner != self.identity {
            return Err(FsCasErrorV1::CrossOwner);
        }
        let operation = state
            .operations
            .get_mut(usize::from(token.slot))
            .ok_or(FsCasErrorV1::Integrity)?;
        if !operation.active || operation.nonce != token.nonce {
            return Err(FsCasErrorV1::Integrity);
        }
        if operation.operation_kind != token.operation_kind as u64 {
            return Err(FsCasErrorV1::WrongOperationKind);
        }
        Ok(operation)
    }

    fn validate_token_v1(&self, token: FsStorageOperationTokenV1) -> Result<(), FsCasErrorV1> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| FsCasErrorV1::SynchronizationPoisoned)?;
        self.operation_mut_v1(&mut state, token).map(|_| ())
    }

    /// Refine the conservative storage envelope owned by one already-live
    /// root operation. The identity of the lease does not change: owner,
    /// generation, operation kind, slot, and nonce remain exactly the same.
    ///
    /// Refinement is permitted only before the operation creates preparation
    /// state or installs an immutable name, and every component is monotonic.
    /// The shared-root capacity decision and envelope replacement occur under
    /// the same lock, so a refusal leaves the prior envelope unchanged.
    fn widen_for_operation_v1(
        &self,
        token: FsStorageOperationTokenV1,
        envelope: FsStorageEnvelopeV1,
    ) -> Result<(), FsCasErrorV1> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| FsCasErrorV1::SynchronizationPoisoned)?;
        let operation = *self.operation_mut_v1(&mut state, token)?;
        let current = operation.envelope.ok_or(FsCasErrorV1::Integrity)?;
        if operation.preparation_current != RootStorageUsageV1::default()
            || operation.preparation_high_water != RootStorageUsageV1::default()
            || operation.immutable_pending != RootStorageUsageV1::default()
            || envelope.preparation_bytes < current.preparation_bytes
            || envelope.immutable_bytes < current.immutable_bytes
            || envelope.preparation_inodes < current.preparation_inodes
            || envelope.immutable_inodes < current.immutable_inodes
        {
            return Err(FsCasErrorV1::Integrity);
        }

        let active_without_current_bytes = state
            .active_reserved
            .bytes
            .checked_sub(current.requested_bytes())
            .ok_or(FsCasErrorV1::Integrity)?;
        let active_without_current_inodes = state
            .active_reserved
            .inodes
            .checked_sub(current.requested_inodes())
            .ok_or(FsCasErrorV1::Integrity)?;
        let next_active = RootStorageUsageV1 {
            bytes: active_without_current_bytes
                .checked_add(envelope.requested_bytes())
                .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?,
            inodes: active_without_current_inodes
                .checked_add(envelope.requested_inodes())
                .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?,
        };
        let next_bytes = state
            .immutable
            .bytes
            .checked_add(state.preparation.bytes)
            .and_then(|bytes| bytes.checked_add(self.fixed_reservation.bytes))
            .and_then(|bytes| bytes.checked_add(next_active.bytes))
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        if next_bytes > self.byte_capacity {
            return Err(FsCasErrorV1::ResourceExhausted(
                FsCasResourceV1::StorageBytes,
            ));
        }
        let next_inodes = state
            .immutable
            .inodes
            .checked_add(state.preparation.inodes)
            .and_then(|inodes| inodes.checked_add(self.fixed_reservation.inodes))
            .and_then(|inodes| inodes.checked_add(next_active.inodes))
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        if next_inodes > self.inode_capacity {
            return Err(FsCasErrorV1::ResourceExhausted(
                FsCasResourceV1::StorageInodes,
            ));
        }

        state.active_reserved = next_active;
        state.reserved_high_water.bytes = state.reserved_high_water.bytes.max(next_active.bytes);
        state.reserved_high_water.inodes = state.reserved_high_water.inodes.max(next_active.inodes);
        state.operations[usize::from(token.slot)].envelope = Some(envelope);
        Ok(())
    }

    fn record_preparation_create_v1(
        &self,
        token: FsStorageOperationTokenV1,
    ) -> Result<(), FsCasErrorV1> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| FsCasErrorV1::SynchronizationPoisoned)?;
        let operation = self.operation_mut_v1(&mut state, token)?;
        let envelope = operation.envelope.ok_or(FsCasErrorV1::Integrity)?;
        let next_inodes = operation
            .preparation_current
            .inodes
            .checked_add(1)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        if next_inodes > envelope.preparation_inodes {
            return Err(FsCasErrorV1::Integrity);
        }
        operation.preparation_current.inodes = next_inodes;
        operation.preparation_high_water.inodes = operation
            .preparation_high_water
            .inodes
            .max(operation.preparation_current.inodes);
        Ok(())
    }

    fn record_preparation_length_v1(
        &self,
        token: FsStorageOperationTokenV1,
        old_len: u64,
        new_len: u64,
    ) -> Result<(), FsCasErrorV1> {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poison) => poison.into_inner(),
        };
        let operation = self.operation_mut_v1(&mut state, token)?;
        let envelope = operation.envelope.ok_or(FsCasErrorV1::Integrity)?;
        let next_bytes = operation
            .preparation_current
            .bytes
            .checked_sub(old_len)
            .and_then(|bytes| bytes.checked_add(new_len))
            .ok_or(FsCasErrorV1::Integrity)?;
        if next_bytes > envelope.preparation_bytes {
            return Err(FsCasErrorV1::Integrity);
        }
        operation.preparation_current.bytes = next_bytes;
        operation.preparation_high_water.bytes = operation
            .preparation_high_water
            .bytes
            .max(operation.preparation_current.bytes);
        // A poisoned mutex remains poisoned until terminal `finish`, which
        // will return SynchronizationPoisoned and forbid a handoff. Recovering
        // this exact length transition lets explicit cleanup proceed instead
        // of leaving Drop to reconcile a spool after terminal accounting.
        Ok(())
    }

    fn record_preparation_remove_v1(
        &self,
        token: FsStorageOperationTokenV1,
        len: u64,
    ) -> Result<(), FsCasErrorV1> {
        #[cfg(test)]
        if self
            .fail_next_preparation_remove
            .swap(false, Ordering::AcqRel)
        {
            return Err(FsCasErrorV1::Integrity);
        }
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poison) => poison.into_inner(),
        };
        let operation = self.operation_mut_v1(&mut state, token)?;
        let next_bytes = operation
            .preparation_current
            .bytes
            .checked_sub(len)
            .ok_or(FsCasErrorV1::Integrity)?;
        let next_inodes = operation
            .preparation_current
            .inodes
            .checked_sub(1)
            .ok_or(FsCasErrorV1::Integrity)?;
        operation.preparation_current = RootStorageUsageV1 {
            bytes: next_bytes,
            inodes: next_inodes,
        };
        // Cleanup must continue after a poison discovered elsewhere. The
        // mutex remains poisoned, so terminal `finish` still returns the
        // precise synchronization failure and forbids a handoff; returning
        // success here only allows the owned spool's exact length/unlink
        // reconciliation to reach completion before that terminal record.
        Ok(())
    }

    fn record_immutable_install_v1(
        &self,
        token: FsStorageOperationTokenV1,
        bytes: u64,
        inodes: u64,
    ) -> Result<(), FsCasErrorV1> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| FsCasErrorV1::SynchronizationPoisoned)?;
        let operation = self.operation_mut_v1(&mut state, token)?;
        let envelope = operation.envelope.ok_or(FsCasErrorV1::Integrity)?;
        let next_bytes = operation
            .immutable_pending
            .bytes
            .checked_add(bytes)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        let next_inodes = operation
            .immutable_pending
            .inodes
            .checked_add(inodes)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        if next_bytes > envelope.immutable_bytes || next_inodes > envelope.immutable_inodes {
            return Err(FsCasErrorV1::Integrity);
        }
        operation.immutable_pending.bytes = next_bytes;
        operation.immutable_pending.inodes = next_inodes;
        Ok(())
    }

    fn record_immutable_remove_v1(
        &self,
        token: FsStorageOperationTokenV1,
        bytes: u64,
        inodes: u64,
    ) -> Result<(), FsCasErrorV1> {
        #[cfg(test)]
        if self
            .poison_next_immutable_remove
            .swap(false, Ordering::AcqRel)
        {
            let poison = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _guard = self.state.lock().unwrap();
                panic!("inject immutable-remove accounting poison");
            }));
            assert!(poison.is_err());
        }
        let (mut state, poisoned) = match self.state.lock() {
            Ok(state) => (state, false),
            Err(poison) => (poison.into_inner(), true),
        };
        let operation = self.operation_mut_v1(&mut state, token)?;
        let next_bytes = operation
            .immutable_pending
            .bytes
            .checked_sub(bytes)
            .ok_or(FsCasErrorV1::Integrity)?;
        let next_inodes = operation
            .immutable_pending
            .inodes
            .checked_sub(inodes)
            .ok_or(FsCasErrorV1::Integrity)?;
        operation.immutable_pending = RootStorageUsageV1 {
            bytes: next_bytes,
            inodes: next_inodes,
        };
        if poisoned {
            // The carrier/marker unlink already completed before this method
            // is called. Recover only enough of the poisoned operation cell
            // to mirror that irreversible filesystem fact exactly, then
            // return the synchronization failure so the caller invalidates
            // the root and cannot authorize a handoff from poisoned state.
            Err(FsCasErrorV1::SynchronizationPoisoned)
        } else {
            Ok(())
        }
    }

    fn release_without_observation(&self, slot: usize, nonce: u64) -> Result<(), FsCasErrorV1> {
        let (mut state, poisoned) = match self.state.lock() {
            Ok(state) => (state, false),
            Err(poison) => (poison.into_inner(), true),
        };
        let operation = state
            .operations
            .get(slot)
            .copied()
            .ok_or(FsCasErrorV1::Integrity)?;
        if !operation.active || operation.nonce != nonce {
            return Err(FsCasErrorV1::Integrity);
        }
        let envelope = operation.envelope.filter(|_| operation.active);
        let requested = envelope.map(|envelope| RootStorageUsageV1 {
            bytes: envelope.requested_bytes(),
            inodes: envelope.requested_inodes(),
        });
        let mut reservation_failure = envelope.is_none().then_some(FsCasErrorV1::Integrity);
        // Reconstruct the remaining reservation from the other authoritative
        // operation cells instead of subtracting from the aggregate that made
        // the ordinary terminal fail. A generation-matched lease can then be
        // retired exactly even when that aggregate is below or otherwise
        // inconsistent with its envelope.
        let mut next_active_reserved = RootStorageUsageV1::default();
        let mut active_reserved_rebuilt = true;
        for (other_slot, other) in state.operations.iter().enumerate() {
            if other_slot == slot || !other.active {
                continue;
            }
            let Some(other_envelope) = other.envelope else {
                reservation_failure.get_or_insert(FsCasErrorV1::Integrity);
                active_reserved_rebuilt = false;
                continue;
            };
            if !active_reserved_rebuilt {
                continue;
            }
            let Some(bytes) = next_active_reserved
                .bytes
                .checked_add(other_envelope.requested_bytes())
            else {
                reservation_failure.get_or_insert(FsCasErrorV1::Core(CoreError::IntegerOverflow));
                active_reserved_rebuilt = false;
                continue;
            };
            let Some(inodes) = next_active_reserved
                .inodes
                .checked_add(other_envelope.requested_inodes())
            else {
                reservation_failure.get_or_insert(FsCasErrorV1::Core(CoreError::IntegerOverflow));
                active_reserved_rebuilt = false;
                continue;
            };
            next_active_reserved = RootStorageUsageV1 { bytes, inodes };
        }
        let expected_active_reserved = active_reserved_rebuilt
            .then_some(next_active_reserved)
            .zip(requested)
            .and_then(|(remaining, current)| {
                Some(RootStorageUsageV1 {
                    bytes: remaining.bytes.checked_add(current.bytes)?,
                    inodes: remaining.inodes.checked_add(current.inodes)?,
                })
            });
        if active_reserved_rebuilt && requested.is_some() && expected_active_reserved.is_none() {
            reservation_failure.get_or_insert(FsCasErrorV1::Core(CoreError::IntegerOverflow));
        }
        let inconsistent = expected_active_reserved != Some(state.active_reserved)
            || operation.preparation_current != RootStorageUsageV1::default()
            || operation.immutable_pending != RootStorageUsageV1::default();
        // An unwind has no ordinary terminal record, but it must not erase
        // the direct operation-local lifecycle events. Fold every installed
        // immutable name and every still-live preparation name into the
        // shared root's retained domains when each aggregate is representable.
        // When it is not, preserve the exact operation-relative snapshot in a
        // fixed quarantine cell before retiring the active authority. A
        // checked-arithmetic failure must never keep the storage slot live or
        // invent a wrapped/saturated root total.
        let next_immutable = match (
            state
                .immutable
                .bytes
                .checked_add(operation.immutable_pending.bytes),
            state
                .immutable
                .inodes
                .checked_add(operation.immutable_pending.inodes),
        ) {
            (Some(bytes), Some(inodes)) => Some(RootStorageUsageV1 { bytes, inodes }),
            _ => None,
        };
        let next_preparation = match (
            state
                .preparation
                .bytes
                .checked_add(operation.preparation_current.bytes),
            state
                .preparation
                .inodes
                .checked_add(operation.preparation_current.inodes),
        ) {
            (Some(bytes), Some(inodes)) => Some(RootStorageUsageV1 { bytes, inodes }),
            _ => None,
        };
        let terminal_usage = active_reserved_rebuilt
            .then_some(next_active_reserved)
            .zip(next_immutable)
            .zip(next_preparation)
            .and_then(|((remaining, immutable), preparation)| {
                let bytes = immutable
                    .bytes
                    .checked_add(preparation.bytes)?
                    .checked_add(self.fixed_reservation.bytes)?
                    .checked_add(remaining.bytes)?;
                let inodes = immutable
                    .inodes
                    .checked_add(preparation.inodes)?
                    .checked_add(self.fixed_reservation.inodes)?
                    .checked_add(remaining.inodes)?;
                Some(RootStorageUsageV1 { bytes, inodes })
            });
        let arithmetic_unclassified = reservation_failure.is_some()
            || next_immutable.is_none()
            || next_preparation.is_none()
            || terminal_usage.is_none();
        let over_capacity = terminal_usage.is_some_and(|usage| {
            usage.bytes > self.byte_capacity || usage.inodes > self.inode_capacity
        });
        if active_reserved_rebuilt {
            state.active_reserved = next_active_reserved;
        }
        if let Some(immutable) = next_immutable {
            state.immutable = immutable;
        }
        if let Some(preparation) = next_preparation {
            state.preparation = preparation;
        }
        if arithmetic_unclassified {
            debug_assert!(!state.unclassified_terminals[slot].occupied);
            state.unclassified_terminals[slot] = RootStorageUnclassifiedTerminalV1 {
                occupied: true,
                nonce,
                reservation: requested.unwrap_or_default(),
                immutable: operation.immutable_pending,
                preparation: operation.preparation_current,
                reservation_known: requested.is_some(),
                active_reserved_rebuilt,
                immutable_aggregated: next_immutable.is_some(),
                preparation_aggregated: next_preparation.is_some(),
            };
        }
        state.operations[slot] = RootStorageOperationStateV1::default();
        if poisoned {
            // Recover the generation-matched operation cell exactly even
            // when the authoritative mutex is poisoned. The caller must
            // still invalidate the root, but no ordinary terminal path may
            // leave this slot live for a result-discarding Drop retry.
            Err(FsCasErrorV1::SynchronizationPoisoned)
        } else if let Some(error) = reservation_failure {
            Err(error)
        } else if arithmetic_unclassified {
            Err(FsCasErrorV1::Core(CoreError::IntegerOverflow))
        } else if inconsistent || over_capacity {
            Err(FsCasErrorV1::Integrity)
        } else {
            Ok(())
        }
    }

    fn finish(
        &self,
        slot: usize,
        nonce: u64,
        commit_requested: bool,
        counters: &mut OperationCountersV1,
    ) -> Result<RootStorageFinishV1, FsCasErrorV1> {
        let (mut state, poisoned) = match self.state.lock() {
            Ok(state) => (state, false),
            Err(poison) => (poison.into_inner(), true),
        };
        // A poisoned synchronization boundary can never authorize a usable
        // handoff. Recover the operation-owned cell only to terminalize its
        // exact reservation and retain all visible custody before returning
        // the typed fail-closed error.
        let commit = commit_requested && !poisoned;
        let operation = state
            .operations
            .get(slot)
            .copied()
            .filter(|operation| operation.active && operation.nonce == nonce)
            .ok_or(FsCasErrorV1::Integrity)?;
        let envelope = operation.envelope.ok_or(FsCasErrorV1::Integrity)?;
        if commit && operation.preparation_current != RootStorageUsageV1::default() {
            // A usable handoff cannot outlive private operation state.  Treat
            // any attempt to commit with a preparation name/length still
            // charged as lifecycle corruption rather than mislabelling it as
            // a successful terminal observation.
            return Err(FsCasErrorV1::Integrity);
        }
        let committed = if commit {
            operation.immutable_pending
        } else {
            RootStorageUsageV1::default()
        };
        let immutable_retained = if commit {
            RootStorageUsageV1::default()
        } else {
            operation.immutable_pending
        };
        let retained = RootStorageUsageV1 {
            bytes: operation
                .preparation_current
                .bytes
                .checked_add(immutable_retained.bytes)
                .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?,
            inodes: operation
                .preparation_current
                .inodes
                .checked_add(immutable_retained.inodes)
                .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?,
        };
        let accounted_bytes = committed
            .bytes
            .checked_add(retained.bytes)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        let accounted_inodes = committed
            .inodes
            .checked_add(retained.inodes)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        if accounted_bytes > envelope.requested_bytes()
            || accounted_inodes > envelope.requested_inodes()
        {
            return Err(FsCasErrorV1::Integrity);
        }
        let released_bytes = envelope.requested_bytes() - accounted_bytes;
        let released_inodes = envelope.requested_inodes() - accounted_inodes;
        let next_active_reserved = RootStorageUsageV1 {
            bytes: state
                .active_reserved
                .bytes
                .checked_sub(envelope.requested_bytes())
                .ok_or(FsCasErrorV1::Integrity)?,
            inodes: state
                .active_reserved
                .inodes
                .checked_sub(envelope.requested_inodes())
                .ok_or(FsCasErrorV1::Integrity)?,
        };
        let next_immutable = RootStorageUsageV1 {
            bytes: state
                .immutable
                .bytes
                .checked_add(operation.immutable_pending.bytes)
                .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?,
            inodes: state
                .immutable
                .inodes
                .checked_add(operation.immutable_pending.inodes)
                .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?,
        };
        let next_preparation = RootStorageUsageV1 {
            bytes: state
                .preparation
                .bytes
                .checked_add(operation.preparation_current.bytes)
                .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?,
            inodes: state
                .preparation
                .inodes
                .checked_add(operation.preparation_current.inodes)
                .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?,
        };
        let terminal_bytes = next_immutable
            .bytes
            .checked_add(next_preparation.bytes)
            .and_then(|bytes| bytes.checked_add(self.fixed_reservation.bytes))
            .and_then(|bytes| bytes.checked_add(next_active_reserved.bytes))
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        let terminal_inodes = next_immutable
            .inodes
            .checked_add(next_preparation.inodes)
            .and_then(|inodes| inodes.checked_add(self.fixed_reservation.inodes))
            .and_then(|inodes| inodes.checked_add(next_active_reserved.inodes))
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        if terminal_bytes > self.byte_capacity || terminal_inodes > self.inode_capacity {
            return Err(FsCasErrorV1::Integrity);
        }
        let mut terminal_counters = *counters;
        let counter_terminal = (|| {
            if poisoned && commit_requested {
                terminal_counters
                    .record_unreachable_installed_residue(operation.immutable_pending.bytes)
                    .map_err(FsCasErrorV1::Core)?;
            }
            terminal_counters
                .record_storage_admission_v1(
                    envelope.requested_bytes(),
                    envelope.requested_bytes(),
                    released_bytes,
                    committed.bytes,
                    retained.bytes,
                    envelope.requested_inodes(),
                    envelope.requested_inodes(),
                    released_inodes,
                    committed.inodes,
                    retained.inodes,
                    state.reserved_high_water.bytes,
                    state.reserved_high_water.inodes,
                    operation.preparation_high_water.bytes,
                    operation.preparation_high_water.inodes,
                    operation.preparation_current.bytes,
                    operation.preparation_current.inodes,
                    if commit {
                        0
                    } else {
                        operation.preparation_current.bytes
                    },
                    if commit {
                        0
                    } else {
                        operation.preparation_current.inodes
                    },
                    immutable_retained.bytes,
                    immutable_retained.inodes,
                )
                .map_err(FsCasErrorV1::Core)
        })();

        // The exact disposition and root-wide custody transitions above were
        // already checked from the authoritative operation snapshot. Consume
        // the slot even if the caller-owned observation counter cannot record
        // that disposition: counter overflow is a typed evidence failure, not
        // permission to leave storage authority live for Drop to reinterpret.
        state.active_reserved = next_active_reserved;
        state.immutable = next_immutable;
        state.preparation = next_preparation;
        state.operations[slot] = RootStorageOperationStateV1::default();
        if poisoned {
            if counter_terminal.is_ok() {
                *counters = terminal_counters;
            }
            Ok(RootStorageFinishV1::TerminalizedError(
                FsCasErrorV1::SynchronizationPoisoned,
            ))
        } else if let Err(error) = counter_terminal {
            Ok(RootStorageFinishV1::TerminalizedError(error))
        } else {
            *counters = terminal_counters;
            Ok(RootStorageFinishV1::Complete)
        }
    }
}

struct RootStorageLeaseV1<'owner> {
    admission: &'owner RootStorageAdmissionV1,
    slot: usize,
    nonce: u64,
    operation_kind: FsOperationKindV1,
    released: bool,
}

impl RootStorageLeaseV1<'_> {
    fn token_v1(&self) -> Result<FsStorageOperationTokenV1, FsCasErrorV1> {
        Ok(FsStorageOperationTokenV1 {
            owner: self.admission.identity,
            slot: u8::try_from(self.slot).map_err(|_| FsCasErrorV1::Integrity)?,
            nonce: self.nonce,
            operation_kind: self.operation_kind,
        })
    }
}

impl Drop for RootStorageLeaseV1<'_> {
    fn drop(&mut self) {
        if !self.released {
            let _ = self
                .admission
                .release_without_observation(self.slot, self.nonce);
            self.released = true;
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum AdmissionTicketStateV1 {
    Empty = 0,
    Waiting = 1,
    Cancelled = 2,
}

/// The only caller-derived state retained while an operation waits for its
/// root-owned grant. Every cell is part of the preallocated fixed population;
/// no queue admission allocates operation-owned memory.
#[derive(Clone, Copy)]
#[repr(C)]
struct QueueTicketV1 {
    operation_kind: u64,
    cancellation_key: u64,
    reserved: [u8; 240],
}

impl QueueTicketV1 {
    const EMPTY: Self = Self {
        operation_kind: 0,
        cancellation_key: 0,
        reserved: [0; 240],
    };

    const fn new(operation_kind: FsOperationKindV1, cancellation_key: u64) -> Self {
        Self {
            operation_kind: operation_kind as u64,
            cancellation_key,
            reserved: [0; 240],
        }
    }
}

const _: [(); 256] = [(); core::mem::size_of::<QueueTicketV1>()];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u64)]
pub(crate) enum FsOperationKindV1 {
    CompleteC3File = 1,
    CompleteC3Tree = 2,
    RootExtraction = 3,
    ExactFileRangeRead = 4,
    CompleteReplace = 5,
    CompleteUpdate = 6,
    CompleteAdd = 7,
    CompleteRemove = 8,
    CompleteMove = 9,
    CompleteMetadata = 10,
}

struct OperationAdmissionStateV1 {
    next_ticket: u64,
    serving_ticket: u64,
    active: u64,
    queue_tickets: Box<[QueueTicketV1]>,
    tickets: [AdmissionTicketStateV1; MAX_ADMISSION_TICKETS],
}

struct OperationAdmissionQueueV1 {
    capacity: u64,
    state: Mutex<OperationAdmissionStateV1>,
    changed: Condvar,
}

impl OperationAdmissionQueueV1 {
    fn new(capacity: u64) -> Result<Self, FsCasErrorV1> {
        let mut queue_tickets = Vec::new();
        queue_tickets
            .try_reserve_exact(MAX_ADMISSION_TICKETS)
            .map_err(|_| FsCasErrorV1::ResourceExhausted(FsCasResourceV1::Memory))?;
        queue_tickets.resize(MAX_ADMISSION_TICKETS, QueueTicketV1::EMPTY);
        // Seal the phase-one population at exactly 1,024 addressable cells.
        // The ledger charges their deterministic language-level payload only;
        // allocator metadata and physical/RSS behavior remain unavailable.
        let queue_tickets = queue_tickets.into_boxed_slice();
        debug_assert_eq!(queue_tickets.len(), MAX_ADMISSION_TICKETS);
        Ok(Self {
            capacity,
            state: Mutex::new(OperationAdmissionStateV1 {
                next_ticket: 0,
                serving_ticket: 0,
                active: 0,
                queue_tickets,
                tickets: [AdmissionTicketStateV1::Empty; MAX_ADMISSION_TICKETS],
            }),
            changed: Condvar::new(),
        })
    }

    fn issue(
        &self,
        operation_kind: FsOperationKindV1,
        cancellation_key: u64,
        counters: &mut OperationCountersV1,
    ) -> Result<PendingAdmissionTicketV1<'_>, OperationAdmissionIssueFailureV1> {
        let mut state = self.state.lock().map_err(|_| {
            OperationAdmissionIssueFailureV1::new(FsCasErrorV1::SynchronizationPoisoned)
        })?;
        let outstanding = state
            .next_ticket
            .checked_sub(state.serving_ticket)
            .ok_or_else(|| OperationAdmissionIssueFailureV1::new(FsCasErrorV1::Integrity))?;
        if outstanding >= MAX_ADMISSION_TICKETS as u64 {
            // Queue exhaustion is the authoritative first cause. The checked
            // refusal observation happens afterwards and cannot replace it;
            // if that required direct observation cannot be represented, the
            // root-owned production entry path must fail closed.
            return Err(OperationAdmissionIssueFailureV1 {
                first: FsCasErrorV1::ResourceExhausted(FsCasResourceV1::Queue),
                observation_failed: counters.record_root_admission_queue_refusal_v1().is_err(),
            });
        }
        let waiting_depth = outstanding.checked_add(1).ok_or_else(|| {
            OperationAdmissionIssueFailureV1::new(FsCasErrorV1::Core(CoreError::IntegerOverflow))
        })?;
        let next_ticket = state.next_ticket.checked_add(1).ok_or({
            // The empty queue can still exhaust its monotonic sequence after
            // u64::MAX issued tickets. No observation or state transition has
            // happened yet; fail the root closed because this sequence can
            // never mint another non-replayed ticket.
            OperationAdmissionIssueFailureV1 {
                first: FsCasErrorV1::Core(CoreError::IntegerOverflow),
                observation_failed: true,
            }
        })?;
        let ticket = state.next_ticket;
        let slot = usize::try_from(ticket % MAX_ADMISSION_TICKETS as u64)
            .map_err(|_| OperationAdmissionIssueFailureV1::new(FsCasErrorV1::Integrity))?;
        if state.tickets[slot] != AdmissionTicketStateV1::Empty {
            // A non-empty target slot is impossible when the bounded ticket
            // distance above says capacity remains. Validate that invariant
            // before recording an entry or advancing the monotonic sequence,
            // so fail-closed rejection cannot manufacture queue residue.
            return Err(OperationAdmissionIssueFailureV1::new(
                FsCasErrorV1::Integrity,
            ));
        }
        counters
            .record_root_admission_queue_entry_v1(waiting_depth)
            .map_err(|error| OperationAdmissionIssueFailureV1 {
                first: FsCasErrorV1::Core(error),
                // This required direct observation is part of root-owned
                // admission custody.  No ticket has been installed yet, but
                // an unrepresentable entry count makes continued use of the
                // shared root unverifiable and must therefore fail closed.
                observation_failed: true,
            })?;
        state.next_ticket = next_ticket;
        state.queue_tickets[slot] = QueueTicketV1::new(operation_kind, cancellation_key);
        state.tickets[slot] = AdmissionTicketStateV1::Waiting;
        Ok(PendingAdmissionTicketV1 {
            queue: self,
            ticket,
            slot,
            resolved: false,
        })
    }

    fn acquire<'queue, C>(
        &'queue self,
        operation_kind: FsOperationKindV1,
        cancellation_key: u64,
        control: &mut C,
        counters: &mut OperationCountersV1,
    ) -> OperationAdmissionAcquireOutcomeV1<'queue>
    where
        C: FsCasControlV1 + ?Sized,
    {
        let started = Instant::now();
        let primary = self
            .issue(operation_kind, cancellation_key, counters)
            .map_err(OperationAdmissionAcquireFailureV1::from)
            .and_then(|ticket| ticket.wait(control, counters));
        let observation = u64::try_from(started.elapsed().as_nanos())
            .map_err(|_| FsCasErrorV1::Core(CoreError::IntegerOverflow))
            .and_then(|nanoseconds| {
                counters
                    .record_root_admission_wait_v1(nanoseconds)
                    .map_err(FsCasErrorV1::Core)
            });
        match (primary, observation) {
            (Ok(admission), Ok(())) => OperationAdmissionAcquireOutcomeV1::Granted(admission),
            (Ok(admission), Err(first)) => {
                // Admission is already authoritative. Preserve its custody so
                // the outer root owner can explicitly release it and durably
                // invalidate on a release failure; `Drop` is only a backstop.
                OperationAdmissionAcquireOutcomeV1::GrantedWithObservationFailure {
                    admission,
                    first,
                }
            }
            (Err(failure), _) => {
                // Queue/cancellation/deadline/synchronization failure happened
                // before the timing observation. A later checked observation
                // failure cannot replace that chronological typed cause.
                OperationAdmissionAcquireOutcomeV1::Rejected {
                    first: failure.first,
                    fail_closed: failure.fail_closed,
                }
            }
        }
    }

    fn cancel_pending(&self, ticket: u64, slot: usize) -> Result<(), FsCasErrorV1> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| FsCasErrorV1::SynchronizationPoisoned)?;
        if ticket < state.serving_ticket {
            return Ok(());
        }
        if state.tickets[slot] != AdmissionTicketStateV1::Waiting {
            return Err(FsCasErrorV1::Integrity);
        }
        state.tickets[slot] = AdmissionTicketStateV1::Cancelled;
        advance_cancelled_tickets_v1(&mut state)?;
        self.changed.notify_all();
        Ok(())
    }

    fn release(&self) -> Result<(), FsCasErrorV1> {
        let (mut state, poisoned) = match self.state.lock() {
            Ok(state) => (state, false),
            Err(poison) => (poison.into_inner(), true),
        };
        let decremented = state.active.checked_sub(1);
        if let Some(active) = decremented {
            state.active = active;
        }
        // Wake every pending caller even when the recovered state is also
        // inconsistent. The poisoned acquisition is the chronological first
        // authority failure and must not be flattened by a later underflow.
        self.changed.notify_all();
        if poisoned {
            Err(FsCasErrorV1::SynchronizationPoisoned)
        } else if decremented.is_none() {
            Err(FsCasErrorV1::Integrity)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OperationAdmissionIssueFailureV1 {
    first: FsCasErrorV1,
    observation_failed: bool,
}

impl OperationAdmissionIssueFailureV1 {
    const fn new(first: FsCasErrorV1) -> Self {
        Self {
            first,
            observation_failed: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OperationAdmissionAcquireFailureV1 {
    first: FsCasErrorV1,
    fail_closed: bool,
}

impl OperationAdmissionAcquireFailureV1 {
    const fn new(first: FsCasErrorV1) -> Self {
        Self {
            first,
            fail_closed: false,
        }
    }
}

impl From<FsCasErrorV1> for OperationAdmissionAcquireFailureV1 {
    fn from(first: FsCasErrorV1) -> Self {
        Self::new(first)
    }
}

impl From<OperationAdmissionIssueFailureV1> for OperationAdmissionAcquireFailureV1 {
    fn from(failure: OperationAdmissionIssueFailureV1) -> Self {
        Self {
            first: failure.first,
            fail_closed: failure.observation_failed,
        }
    }
}

enum OperationAdmissionAcquireOutcomeV1<'queue> {
    Granted(RootAdmissionLeaseV1<'queue>),
    Rejected {
        first: FsCasErrorV1,
        fail_closed: bool,
    },
    GrantedWithObservationFailure {
        admission: RootAdmissionLeaseV1<'queue>,
        first: FsCasErrorV1,
    },
}

#[cfg(test)]
impl<'queue> OperationAdmissionAcquireOutcomeV1<'queue> {
    fn unwrap(self) -> RootAdmissionLeaseV1<'queue> {
        match self {
            Self::Granted(admission) => admission,
            Self::Rejected { .. } | Self::GrantedWithObservationFailure { .. } => {
                panic!("root admission was not granted cleanly")
            }
        }
    }
}

struct PendingAdmissionTicketV1<'queue> {
    queue: &'queue OperationAdmissionQueueV1,
    ticket: u64,
    slot: usize,
    resolved: bool,
}

/// Test-only custody handle for filling the fixed root-owned admission queue
/// without starting a production operation. The wrapped ticket cannot grant
/// an operation capability and its `Drop` only returns the preallocated queue
/// cell. This exists solely to prove that a production caller rejected at the
/// 1,025th ticket performs no typed request, sink, or storage work.
#[cfg(test)]
pub(crate) struct PendingAdmissionTicketForTestV1<'queue> {
    _ticket: PendingAdmissionTicketV1<'queue>,
}

impl<'queue> PendingAdmissionTicketV1<'queue> {
    fn wait<C>(
        mut self,
        control: &mut C,
        counters: &mut OperationCountersV1,
    ) -> Result<RootAdmissionLeaseV1<'queue>, OperationAdmissionAcquireFailureV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        let mut state = match self.queue.state.lock() {
            Ok(state) => state,
            Err(poison) => {
                // The poison result still owns the authoritative guard. Use
                // it to retire this exact ticket before returning the typed
                // synchronization cause; `Drop` remains only a backstop.
                let mut state = poison.into_inner();
                let failure = self.retire_waiting_with_guard_v1(
                    &mut state,
                    FsCasErrorV1::SynchronizationPoisoned,
                );
                drop(state);
                return Err(failure);
            }
        };

        loop {
            advance_cancelled_tickets_v1(&mut state)
                .map_err(OperationAdmissionAcquireFailureV1::new)?;
            if self.ticket == state.serving_ticket && state.active < self.queue.capacity {
                if state.tickets[self.slot] != AdmissionTicketStateV1::Waiting {
                    return Err(OperationAdmissionAcquireFailureV1::new(
                        FsCasErrorV1::Integrity,
                    ));
                }
                state.queue_tickets[self.slot] = QueueTicketV1::EMPTY;
                state.tickets[self.slot] = AdmissionTicketStateV1::Empty;
                state.serving_ticket = state
                    .serving_ticket
                    .checked_add(1)
                    .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
                let active_slots = state
                    .active
                    .checked_add(1)
                    .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
                counters
                    .record_root_admission_grant_v1(active_slots)
                    .map_err(FsCasErrorV1::Core)?;
                state.active = active_slots;
                advance_cancelled_tickets_v1(&mut state)?;
                self.resolved = true;
                self.queue.changed.notify_all();
                return Ok(RootAdmissionLeaseV1 {
                    queue: self.queue,
                    released: false,
                });
            }

            let stop = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if control.cancellation_requested() {
                    Some(CoreError::Cancelled)
                } else if control.deadline_exceeded() {
                    Some(CoreError::Deadline)
                } else {
                    None
                }
            })) {
                Ok(stop) => stop,
                Err(payload) => {
                    // The callback is inspected while this ticket owns the
                    // queue guard. Resolve the ticket before resuming the
                    // caller's unwind so the guard is dropped normally and
                    // the queue cannot be poisoned or strand a live cell.
                    let retirement = if state.tickets[self.slot] != AdmissionTicketStateV1::Waiting
                    {
                        Err(FsCasErrorV1::Integrity)
                    } else {
                        state.tickets[self.slot] = AdmissionTicketStateV1::Cancelled;
                        #[cfg(test)]
                        let injected = control.inject_pending_unwind_retirement_failure();
                        #[cfg(not(test))]
                        let injected: Option<FsCasErrorV1> = None;
                        match advance_cancelled_tickets_v1(&mut state) {
                            Err(error) => Err(error),
                            Ok(()) => injected.map_or(Ok(()), Err),
                        }
                    };
                    // This owned terminal was attempted even when the queue
                    // state was already inconsistent. Prevent Drop from
                    // retrying and reinterpreting the same ticket after the
                    // outer root owner has durably failed the root closed.
                    self.resolved = true;
                    self.queue.changed.notify_all();
                    drop(state);
                    match retirement {
                        Ok(()) => std::panic::resume_unwind(payload),
                        Err(first) => {
                            drop(payload);
                            return Err(OperationAdmissionAcquireFailureV1 {
                                first,
                                fail_closed: true,
                            });
                        }
                    }
                }
            };
            if let Some(error) = stop {
                let failure =
                    self.retire_waiting_with_guard_v1(&mut state, FsCasErrorV1::Core(error));
                drop(state);
                return Err(failure);
            }

            if let Err(error) = counters.record_root_admission_wait_poll_v1() {
                let failure =
                    self.retire_waiting_with_guard_v1(&mut state, FsCasErrorV1::Core(error));
                drop(state);
                return Err(failure);
            }
            match self
                .queue
                .changed
                .wait_timeout(state, ADMISSION_CONTROL_POLL)
            {
                Ok((observed, _)) => state = observed,
                Err(poison) => {
                    let (mut observed, _) = poison.into_inner();
                    let failure = self.retire_waiting_with_guard_v1(
                        &mut observed,
                        FsCasErrorV1::SynchronizationPoisoned,
                    );
                    drop(observed);
                    return Err(failure);
                }
            }
        }
    }

    fn retire_waiting_with_guard_v1(
        &mut self,
        state: &mut OperationAdmissionStateV1,
        first: FsCasErrorV1,
    ) -> OperationAdmissionAcquireFailureV1 {
        let retired = if state.tickets[self.slot] == AdmissionTicketStateV1::Waiting {
            state.tickets[self.slot] = AdmissionTicketStateV1::Cancelled;
            advance_cancelled_tickets_v1(state)
        } else {
            Err(FsCasErrorV1::Integrity)
        };
        // An explicit attempt now owns this terminal path. Never let `Drop`
        // retry it into a different result; an impossible retirement failure
        // instead makes the root-owned entry fail closed.
        self.resolved = true;
        self.queue.changed.notify_all();
        OperationAdmissionAcquireFailureV1 {
            first,
            fail_closed: retired.is_err(),
        }
    }
}

impl Drop for PendingAdmissionTicketV1<'_> {
    fn drop(&mut self) {
        if !self.resolved && self.queue.cancel_pending(self.ticket, self.slot).is_ok() {
            self.resolved = true;
        }
    }
}

fn advance_cancelled_tickets_v1(state: &mut OperationAdmissionStateV1) -> Result<(), FsCasErrorV1> {
    while state.serving_ticket < state.next_ticket {
        let slot = usize::try_from(state.serving_ticket % MAX_ADMISSION_TICKETS as u64)
            .map_err(|_| FsCasErrorV1::Integrity)?;
        if state.tickets[slot] != AdmissionTicketStateV1::Cancelled {
            break;
        }
        state.queue_tickets[slot] = QueueTicketV1::EMPTY;
        state.tickets[slot] = AdmissionTicketStateV1::Empty;
        state.serving_ticket = state
            .serving_ticket
            .checked_add(1)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
    }
    Ok(())
}

struct RootAdmissionLeaseV1<'queue> {
    queue: &'queue OperationAdmissionQueueV1,
    released: bool,
}

impl RootAdmissionLeaseV1<'_> {
    fn release_v1(&mut self) -> Result<(), FsCasErrorV1> {
        if self.released {
            return Ok(());
        }
        // A failed release makes the queue unusable. Mark this lease terminal
        // so Drop cannot conceal the first failure with a second best-effort
        // attempt; the owning operation capability durably invalidates the
        // root before returning the typed error.
        self.released = true;
        self.queue.release()
    }
}

impl Drop for RootAdmissionLeaseV1<'_> {
    fn drop(&mut self) {
        if !self.released {
            let _ = self.release_v1();
        }
    }
}

impl Drop for FsCasInnerV1 {
    fn drop(&mut self) {
        let invalidated = self.invalidated.load(Ordering::Acquire);
        let file = self.ownership.get_mut().ok().and_then(Option::take);
        drop(file);
        if !invalidated {
            // A normal owner releases exclusivity. Failure is deliberately
            // fail-closed: the remaining active token makes a later opener
            // return Busy instead of guessing that ownership was released.
            let _ = fs::remove_file(self.root.join(ROOT_OWNER_NAME));
        }
    }
}

/// One non-cloneable operation authority minted only by the shared root
/// owner. Lower storage layers may borrow its reservation but cannot mint or
/// replace it. Only this outer capability may monotonically refine its own
/// conservative storage envelope before preparation begins.
#[cfg(feature = "c3-polymorphism")]
pub(crate) struct FsOperationCapabilityV1<'owner> {
    owner: &'owner FsCasV1,
    operation_kind: FsOperationKindV1,
    reservation: OperationReservationV1<'owner>,
    storage: Option<RootStorageLeaseV1<'owner>>,
    admission: RootAdmissionLeaseV1<'owner>,
}

#[cfg(feature = "c3-polymorphism")]
impl FsOperationCapabilityV1<'_> {
    pub(crate) fn owner_ref_v1(&self) -> &FsCasV1 {
        self.owner
    }

    pub(crate) fn invalidate_owner_controlled_v1<C>(
        &self,
        control: &mut C,
    ) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        self.owner.invalidate_root_controlled_v1(control)
    }

    pub(crate) fn invalidate_owner_backstop_v1(&self) -> Result<(), FsCasErrorV1> {
        let mut control = ContinueFsCasControlV1;
        self.owner.invalidate_root_controlled_v1(&mut control)
    }

    pub(crate) fn require_operation_kind_v1(
        &self,
        expected: FsOperationKindV1,
    ) -> Result<(), FsCasErrorV1> {
        if self.operation_kind == expected {
            Ok(())
        } else {
            Err(FsCasErrorV1::WrongOperationKind)
        }
    }

    pub(crate) fn declare_plan_v1(&mut self, plan: OperationMemoryPlanV1) -> CoreResult<()> {
        self.reservation.declare_plan(plan)
    }

    pub(crate) fn memory_high_water_bytes_v1(&self) -> u64 {
        self.owner.inner.operation_ledger.high_water_bytes()
    }

    pub(crate) fn reservation_v1(&self) -> &OperationReservationV1<'_> {
        &self.reservation
    }

    pub(crate) fn declare_storage_envelope_v1(
        &mut self,
        envelope: FsStorageEnvelopeV1,
    ) -> Result<(), FsCasErrorV1> {
        self.owner.ensure_valid()?;
        if let Some(storage) = self.storage.as_ref() {
            self.owner
                .inner
                .storage_admission
                .widen_for_operation_v1(storage.token_v1()?, envelope)?;
        } else {
            self.storage = Some(
                self.owner
                    .inner
                    .storage_admission
                    .reserve_for_operation_v1(envelope, self.operation_kind)?,
            );
        }
        self.owner.ensure_valid()
    }

    pub(crate) fn storage_token_v1(&self) -> Result<FsStorageOperationTokenV1, FsCasErrorV1> {
        self.storage
            .as_ref()
            .ok_or(FsCasErrorV1::Integrity)?
            .token_v1()
    }

    pub(crate) fn finish_storage_admission_v1<C>(
        &mut self,
        commit: bool,
        counters: &mut OperationCountersV1,
        control: &mut C,
    ) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        let Some(mut lease) = self.storage.take() else {
            return Ok(());
        };
        let terminal = lease
            .admission
            .finish(lease.slot, lease.nonce, commit, counters);
        match terminal {
            Ok(RootStorageFinishV1::Complete) => {
                lease.released = true;
                Ok(())
            }
            Ok(RootStorageFinishV1::TerminalizedError(error)) => {
                lease.released = true;
                Err(self
                    .owner
                    .fail_closed_preserving_error_after_unwind_v1(error, control))
            }
            Err(error) => {
                // `finish` could not prove an exact observed terminal. Retain
                // the lease locally, disable its Drop retry, and make one
                // explicit callback-free custody transfer before returning.
                // A later synchronization/integrity observation cannot erase
                // the chronological first cause; persistent invalidation is
                // the only permitted terminal dominance.
                let release = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    lease
                        .admission
                        .release_without_observation(lease.slot, lease.nonce)
                }));
                lease.released = true;
                let error = match release {
                    Ok(Ok(())) => error,
                    // The ordinary `finish` failure is chronological. The
                    // callback-free fallback's reservation/custody failure
                    // is retained in the fixed quarantine state and requires
                    // fail-closed invalidation, but it is not itself a
                    // cleanup/invalidation cause permitted to dominate the
                    // first typed terminal.
                    Ok(Err(_secondary)) => error,
                    Err(payload) => {
                        // The helper has no callbacks and uses checked state
                        // transitions. Classify an unexpected internal unwind
                        // instead of silently discarding it; the controlled
                        // fail-closed barrier below remains the only terminal
                        // cause that may dominate the chronological failure.
                        drop(payload);
                        error
                    }
                };
                Err(self
                    .owner
                    .fail_closed_preserving_error_after_unwind_v1(error, control))
            }
        }
    }

    /// Explicitly terminate the root-owned operation after every storage and
    /// preparation cleanup attempt. Queue release is fallible and therefore
    /// cannot be delegated to Drop on an ordinary return path.
    pub(crate) fn finish_operation_admission_v1<C>(
        &mut self,
        counters: &mut OperationCountersV1,
        control: &mut C,
    ) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        if self.storage.is_some() {
            return Err(FsCasErrorV1::Integrity);
        }
        let memory = self
            .reservation
            .release_v1()
            .map_err(|_| FsCasErrorV1::Integrity);
        let admission = self.admission.release_v1();
        match memory.err().or_else(|| admission.err()) {
            None => Ok(()),
            Some(error) => {
                let observation = counters
                    .record_root_admission_release_failure_v1()
                    .map_err(FsCasErrorV1::Core);
                // Counter observation failure is evidence loss, not a
                // cleanup/invalidation class allowed to replace the first
                // typed authority-release cause. Invalidation is itself
                // unwind-contained so that a callback panic cannot skip the
                // already-known terminal or the other capability half.
                let _ = observation;
                Err(self
                    .owner
                    .fail_closed_preserving_error_after_unwind_v1(error, control))
            }
        }
    }

    /// Terminate the storage ledger and the one outer root admission as one
    /// capability boundary. Storage is terminalized first so its exact
    /// requested/reserved equation is always recorded while the capability is
    /// still live. If the later admission release fails, no handoff may be
    /// returned: reclassify the operation's immutable set from committed to
    /// retained residue before returning the typed fail-closed error.
    pub(crate) fn finish_terminal_v1<C>(
        &mut self,
        commit: bool,
        counters: &mut OperationCountersV1,
        control: &mut C,
    ) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        // Both terminal halves can still encounter an unexpected unwind
        // outside the typed invalidation path. Contain each half while the one
        // outer capability owns the other, and attempt the callback-free
        // invalidation backstop immediately. A successful backstop is not an
        // `InvalidationFailed` error. The original payload is resumed only
        // after both halves have reached a terminal state and only when no
        // typed terminal cause exists.
        let (storage, storage_unwind) =
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.finish_storage_admission_v1(commit, counters, control)
            })) {
                Ok(result) => (result, None),
                Err(payload) => {
                    let backstop = self.invalidate_owner_backstop_v1();
                    (backstop, Some(payload))
                }
            };
        let (admission, admission_unwind) =
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.finish_operation_admission_v1(counters, control)
            })) {
                Ok(result) => (result, None),
                Err(payload) => {
                    let backstop = self.invalidate_owner_backstop_v1();
                    (backstop, Some(payload))
                }
            };

        let reclassification = if commit && storage.is_ok() && admission.is_err() {
            counters
                .reclassify_storage_commit_as_retained_v1()
                .map_err(FsCasErrorV1::Core)
        } else {
            Ok(())
        };
        let terminal = match (storage, admission) {
            (Err(first), Err(later)) if later.has_cleanup_or_invalidation_dominance_v1() => {
                Err(first.dominated_by_v1(later))
            }
            (Err(error), _) => Err(error),
            (Ok(()), Ok(())) => Ok(()),
            (Ok(()), Err(error)) => Err(error),
        };
        let terminal = match (terminal, reclassification) {
            (Err(first), Err(later)) => Err(first.dominated_by_v1(later)),
            (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
            (Ok(()), Ok(())) => Ok(()),
        };
        #[cfg(test)]
        if terminal.is_ok()
            && storage_unwind.is_none()
            && admission_unwind.is_none()
            && control.inject_operation_terminal_unwind_after_release()
        {
            panic!("injected operation-terminal unwind after explicit release");
        }
        if terminal.is_ok() {
            if let Some(payload) = storage_unwind {
                std::panic::resume_unwind(payload);
            }
            if let Some(payload) = admission_unwind {
                std::panic::resume_unwind(payload);
            }
        }
        terminal
    }
}

#[cfg(feature = "c3-polymorphism")]
impl Drop for FsOperationCapabilityV1<'_> {
    fn drop(&mut self) {
        if let Some(mut lease) = self.storage.take() {
            if lease
                .admission
                .release_without_observation(lease.slot, lease.nonce)
                .is_err()
            {
                self.owner.invalidate_root_backstop_v1();
            }
            // `release_without_observation` always clears a
            // generation-matched slot, including the fail-closed residue
            // case. Prevent the lease backstop from attempting a second
            // release after invalidation.
            lease.released = true;
        }
        if self.reservation.release_v1().is_err() {
            self.owner.invalidate_root_backstop_v1();
        }
        if self.admission.release_v1().is_err() {
            self.owner.invalidate_root_backstop_v1();
        }
    }
}

impl FsCasV1 {
    fn record_storage_preparation_create_v1(
        &self,
        token: FsStorageOperationTokenV1,
    ) -> Result<(), FsCasErrorV1> {
        self.ensure_valid()?;
        self.inner
            .storage_admission
            .record_preparation_create_v1(token)
    }

    fn record_storage_preparation_length_v1(
        &self,
        token: FsStorageOperationTokenV1,
        old_len: u64,
        new_len: u64,
    ) -> Result<(), FsCasErrorV1> {
        self.inner
            .storage_admission
            .record_preparation_length_v1(token, old_len, new_len)
    }

    fn record_storage_preparation_remove_v1(
        &self,
        token: FsStorageOperationTokenV1,
        len: u64,
    ) -> Result<(), FsCasErrorV1> {
        self.inner
            .storage_admission
            .record_preparation_remove_v1(token, len)
    }

    fn record_storage_immutable_install_v1(
        &self,
        token: FsStorageOperationTokenV1,
        bytes: u64,
        inodes: u64,
    ) -> Result<(), FsCasErrorV1> {
        self.ensure_valid()?;
        self.inner
            .storage_admission
            .record_immutable_install_v1(token, bytes, inodes)
    }

    fn record_storage_immutable_remove_v1(
        &self,
        token: FsStorageOperationTokenV1,
        bytes: u64,
        inodes: u64,
    ) -> Result<(), FsCasErrorV1> {
        self.inner
            .storage_admission
            .record_immutable_remove_v1(token, bytes, inodes)
    }
}

fn shared_root_owner(
    root: &Path,
    generation: [u8; 32],
    acquired_ownership: Option<File>,
) -> Result<Arc<FsCasInnerV1>, FsCasErrorV1> {
    shared_root_owner_inner_v1(root, generation, acquired_ownership, || Ok(()))
}

fn shared_root_owner_inner_v1<F>(
    root: &Path,
    generation: [u8; 32],
    acquired_ownership: Option<File>,
    after_acquire: F,
) -> Result<Arc<FsCasInnerV1>, FsCasErrorV1>
where
    F: FnOnce() -> Result<(), FsCasErrorV1>,
{
    let registry = OPEN_ROOTS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut roots = registry
        .lock()
        .map_err(|_| FsCasErrorV1::SynchronizationPoisoned)?;
    roots.retain(|_, owner| owner.strong_count() != 0);
    if let Some(owner) = roots.get(root).and_then(Weak::upgrade) {
        if acquired_ownership.is_some() {
            return Err(FsCasErrorV1::Integrity);
        }
        if owner.generation != generation {
            owner.invalidated.store(true, Ordering::Release);
            return Err(FsCasErrorV1::Integrity);
        }
        return Ok(owner);
    }
    let internally_acquired = acquired_ownership.is_none();
    let ownership = match acquired_ownership {
        Some(ownership) => ownership,
        None => acquire_root_ownership(root, generation)?,
    };
    let initialized = (|| {
        after_acquire()?;
        let (immutable_storage, preparation_storage) = observe_root_storage_usage_v1(root)?;
        let operation_admission =
            OperationAdmissionQueueV1::new(admitted_slots_for_budget(MEMORY_PROFILE_72_MIB))?;
        let storage_admission =
            RootStorageAdmissionV1::new(immutable_storage, preparation_storage, generation)?;
        Ok((operation_admission, storage_admission))
    })();
    let (operation_admission, storage_admission) = match initialized {
        Ok(initialized) => initialized,
        Err(error) => {
            drop(ownership);
            if internally_acquired {
                return Err(root_initialization_cleanup_result_v1(
                    error,
                    fs::remove_file(root.join(ROOT_OWNER_NAME)),
                ));
            }
            return Err(error);
        }
    };
    let owner = Arc::new(FsCasInnerV1 {
        root: root.to_path_buf(),
        generation,
        invalidated: AtomicBool::new(false),
        #[cfg(test)]
        invalidation_probe_failure: Mutex::new(None),
        ownership: Mutex::new(Some(ownership)),
        operation_ledger: ResourceLedgerV1::new(MEMORY_PROFILE_72_MIB),
        operation_admission,
        storage_admission,
        visibility: Mutex::new(()),
        publication: Mutex::new(()),
    });
    roots.insert(root.to_path_buf(), Arc::downgrade(&owner));
    Ok(owner)
}

fn observe_directory_storage_usage_v1(
    directory: &Path,
) -> Result<RootStorageUsageV1, FsCasErrorV1> {
    let mut usage = RootStorageUsageV1::default();
    for entry in
        fs::read_dir(directory).map_err(|error| map_required_filesystem_read_error_v1(&error))?
    {
        let entry = entry.map_err(|error| map_required_filesystem_read_error_v1(&error))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| map_required_filesystem_read_error_v1(&error))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(FsCasErrorV1::MalformedOccupant);
        }
        record_root_namespace_entry_usage_v1(&mut usage, metadata.len())?;
    }
    Ok(usage)
}

/// Charge one observed regular-file name in a fixed root-owned namespace.
///
/// `RootStorageUsageV1::inodes` is a frozen compatibility field name for
/// logical namespace entries. It is intentionally not a host allocated-inode
/// observation: the two resource envelopes must remain independently
/// classified even where the host cannot expose physical inode usage.
fn record_root_namespace_entry_usage_v1(
    usage: &mut RootStorageUsageV1,
    logical_bytes: u64,
) -> Result<(), FsCasErrorV1> {
    let inodes = usage
        .inodes
        .checked_add(1)
        .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
    let bytes = usage
        .bytes
        .checked_add(logical_bytes)
        .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;

    // Retain historical byte-first precedence when both independent
    // envelopes are exceeded, while never describing a name-count refusal as
    // logical-byte exhaustion.
    if bytes > ROOT_LOGICAL_STORAGE_BUDGET_V1 {
        return Err(FsCasErrorV1::ResourceExhausted(
            FsCasResourceV1::StorageBytes,
        ));
    }
    if inodes > ROOT_NAMESPACE_ENTRY_BUDGET_V1 {
        return Err(FsCasErrorV1::ResourceExhausted(
            FsCasResourceV1::StorageInodes,
        ));
    }
    *usage = RootStorageUsageV1 { bytes, inodes };
    Ok(())
}

/// Observe only the fixed one-level FsCas namespace. This is not recovery and
/// never adopts preparation state. It supplies exact logical lengths and name
/// counts; allocated blocks and external quota headroom remain unavailable.
fn observe_root_storage_usage_v1(
    root: &Path,
) -> Result<(RootStorageUsageV1, RootStorageUsageV1), FsCasErrorV1> {
    let preparation = observe_directory_storage_usage_v1(&root.join("preparation"))?;
    let mut immutable = RootStorageUsageV1::default();
    for directory in ["carriers", "objects", "catalog", "closures"] {
        let usage = observe_directory_storage_usage_v1(&root.join(directory))?;
        immutable.bytes = immutable
            .bytes
            .checked_add(usage.bytes)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        immutable.inodes = immutable
            .inodes
            .checked_add(usage.inodes)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
    }
    // The generation and owner records plus the five fixed namespace
    // directories consume stable root-owned names. Their file lengths are
    // included because the logical domain is explicitly not a block count.
    for fixed in ["generation", ROOT_OWNER_NAME] {
        let metadata = fs::symlink_metadata(root.join(fixed))
            .map_err(|error| map_required_filesystem_read_error_v1(&error))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(FsCasErrorV1::MalformedOccupant);
        }
        immutable.bytes = immutable
            .bytes
            .checked_add(metadata.len())
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        immutable.inodes = immutable
            .inodes
            .checked_add(1)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
    }
    immutable.inodes = immutable
        .inodes
        .checked_add(5)
        .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
    Ok((immutable, preparation))
}

impl FsCasV1 {
    /// Crate-private logical admission observation for deterministic
    /// qualification. This does not expose the root-owned ledger or permit a
    /// caller to mint an operation reservation.
    #[cfg(feature = "c3-polymorphism")]
    pub(crate) fn operation_admitted_slots_v1(&self) -> u64 {
        self.inner.operation_ledger.admitted_slots()
    }

    #[cfg(test)]
    pub(crate) fn visibility_lock_available_for_test_v1(&self) -> bool {
        self.inner.visibility.try_lock().is_ok()
    }

    #[cfg(test)]
    pub(crate) fn publication_lock_available_for_test_v1(&self) -> bool {
        self.inner.publication.try_lock().is_ok()
    }

    /// Saturate the direct read-call observation on the next authoritative
    /// occupant-pack reader opened by this test thread. The one-shot,
    /// thread-local scope keeps concurrent tests and production state out of
    /// the fault surface.
    #[cfg(test)]
    pub(crate) fn saturate_next_occupant_pack_read_calls_for_test_v1(&self) {
        NEXT_OCCUPANT_PACK_READ_CALLS_FOR_TEST_V1.with(|seed| {
            assert!(seed.replace(Some(u64::MAX)).is_none());
        });
    }

    /// Seed the direct read observation on the next operation spool read by
    /// this test thread. The hook is consumed only after the real file read
    /// succeeds, proving the checked observation pair without altering
    /// production authority or concurrent tests.
    #[cfg(test)]
    pub(crate) fn seed_next_operation_spool_read_observation_for_test_v1(
        &self,
        bytes_read: u64,
        read_calls: u64,
    ) {
        NEXT_OPERATION_SPOOL_READ_OBSERVATION_FOR_TEST_V1.with(|seed| {
            assert!(seed.replace(Some((bytes_read, read_calls))).is_none());
        });
    }

    /// Seed the direct metadata-read observation on the next occupied reader
    /// opened by this test thread. The one-shot seed leaves real locator and
    /// catalog I/O intact while making a late checked commit deterministic.
    #[cfg(test)]
    pub(crate) fn seed_next_occupied_read_observation_for_test_v1(
        &self,
        bytes_read: u64,
        read_calls: u64,
    ) {
        NEXT_OCCUPIED_READ_OBSERVATION_FOR_TEST_V1.with(|seed| {
            assert!(seed.replace(Some((bytes_read, read_calls))).is_none());
        });
    }

    /// Seed the direct read observation immediately before the next occupied
    /// reader performs a real payload read. Resolution and authentication run
    /// normally first, so tests can isolate the payload tuple's checked commit.
    #[cfg(test)]
    pub(crate) fn seed_next_occupied_payload_read_observation_for_test_v1(
        &self,
        bytes_read: u64,
        read_calls: u64,
    ) {
        NEXT_OCCUPIED_PAYLOAD_READ_OBSERVATION_FOR_TEST_V1.with(|seed| {
            assert!(seed.replace(Some((bytes_read, read_calls))).is_none());
        });
    }

    /// Inject one typed failure at the next invalidation-barrier observation.
    /// This is a test-only substitute for nondeterministic permission and I/O
    /// races at the reserved marker pathname; it never affects production
    /// filesystem authority.
    #[cfg(test)]
    pub(crate) fn fail_next_invalidation_probe_for_test_v1(&self, error: FsCasErrorV1) {
        assert!(matches!(
            error,
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied)
                | FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ReadFailure)
        ));
        let mut failure = self
            .inner
            .invalidation_probe_failure
            .lock()
            .expect("test invalidation-probe hook must not be poisoned");
        assert!(failure.replace(error).is_none());
    }

    #[cfg(test)]
    pub(crate) fn hold_visibility_lock_for_test_v1(&self) -> MutexGuard<'_, ()> {
        self.inner
            .visibility
            .lock()
            .expect("test visibility lock must not already be poisoned")
    }

    #[cfg(test)]
    pub(crate) fn poison_operation_admission_for_test_v1(&self) {
        let poison = self.clone();
        let unwind = std::thread::spawn(move || {
            let _guard = poison.inner.operation_admission.state.lock().unwrap();
            panic!("inject operation-admission release poison");
        })
        .join();
        assert!(unwind.is_err());
    }

    #[cfg(test)]
    pub(crate) fn operation_admission_active_for_test_v1(&self) -> u64 {
        match self.inner.operation_admission.state.lock() {
            Ok(state) => state.active,
            Err(poison) => poison.into_inner().active,
        }
    }

    #[cfg(test)]
    pub(crate) fn operation_admission_queue_for_test_v1(&self) -> (u64, u64, u64) {
        let state = match self.inner.operation_admission.state.lock() {
            Ok(state) => state,
            Err(poison) => poison.into_inner(),
        };
        let outstanding = state
            .next_ticket
            .checked_sub(state.serving_ticket)
            .expect("test queue counters must remain ordered");
        let waiting = state
            .tickets
            .iter()
            .filter(|ticket| **ticket == AdmissionTicketStateV1::Waiting)
            .count() as u64;
        let cancelled = state
            .tickets
            .iter()
            .filter(|ticket| **ticket == AdmissionTicketStateV1::Cancelled)
            .count() as u64;
        (outstanding, waiting, cancelled)
    }

    #[cfg(test)]
    pub(crate) fn poison_storage_admission_for_test_v1(&self) {
        let poison = self.clone();
        let unwind = std::thread::spawn(move || {
            let _guard = poison.inner.storage_admission.state.lock().unwrap();
            panic!("inject storage-admission terminal poison");
        })
        .join();
        assert!(unwind.is_err());
    }

    /// Arm a one-shot synchronization poison at the next immutable-charge
    /// rollback. This is narrower than poisoning the whole storage ledger
    /// before publication, because the immutable install must first succeed
    /// for the no-replace losing-incumbent branch to be exercised.
    #[cfg(test)]
    pub(crate) fn poison_next_immutable_remove_for_test_v1(&self) {
        assert!(!self
            .inner
            .storage_admission
            .poison_next_immutable_remove
            .swap(true, Ordering::AcqRel));
    }

    /// Fail exactly one post-unlink preparation-ledger transition before it
    /// mutates the operation cell. The physically absent path then has a
    /// deliberately unreleased logical charge, isolating cleanup-terminal
    /// classification and invalidation-double-fault behavior.
    #[cfg(test)]
    pub(crate) fn fail_next_preparation_remove_for_test_v1(&self) {
        assert!(!self
            .inner
            .storage_admission
            .fail_next_preparation_remove
            .swap(true, Ordering::AcqRel));
    }

    /// Deterministically inject a failed preparation-accounting rollback
    /// after one operation has charged a name but before the filesystem create
    /// reports its directional error. This is test-only state corruption for
    /// the otherwise unreachable bookkeeping-double-fault branch.
    #[cfg(test)]
    pub(crate) fn remove_active_preparation_inode_for_test_v1(&self) {
        let mut state = match self.inner.storage_admission.state.lock() {
            Ok(state) => state,
            Err(poison) => poison.into_inner(),
        };
        let mut found = 0_u64;
        for operation in &mut state.operations {
            if operation.active && operation.preparation_current.inodes == 1 {
                operation.preparation_current.inodes = 0;
                found += 1;
            }
        }
        assert_eq!(found, 1, "expected one charged preparation inode");
    }

    /// Deterministically make one live operation's preparation-byte ledger
    /// disagree with its owned spool. This is test-only corruption for the
    /// explicit cleanup reconciliation failure path; production can reach
    /// the same terminal through a stale/corrupt operation token or checked
    /// accounting failure.
    #[cfg(test)]
    pub(crate) fn clear_active_preparation_bytes_for_test_v1(&self) {
        let mut state = match self.inner.storage_admission.state.lock() {
            Ok(state) => state,
            Err(poison) => poison.into_inner(),
        };
        let mut found = 0_u64;
        for operation in &mut state.operations {
            if operation.active && operation.preparation_current.bytes > 0 {
                operation.preparation_current.bytes = 0;
                found += 1;
            }
        }
        assert_eq!(found, 1, "expected one charged preparation byte domain");
    }

    /// Deterministically make the active preparation-byte total disagree
    /// with a marker's zero-length precharge. The paired control restores the
    /// value before explicit cleanup; this exists only to prove the checked
    /// accounting failure and invalidation-double-fault terminal paths.
    #[cfg(test)]
    pub(crate) fn inject_active_preparation_byte_for_test_v1(&self) {
        let mut state = match self.inner.storage_admission.state.lock() {
            Ok(state) => state,
            Err(poison) => poison.into_inner(),
        };
        let mut found = 0_u64;
        for operation in &mut state.operations {
            if operation.active && operation.preparation_current.bytes == 0 {
                operation.preparation_current.bytes = 1;
                found += 1;
            }
        }
        assert_eq!(found, 1, "expected one zero-byte preparation domain");
    }

    /// Restore a deliberately corrupted zero-byte preparation ledger to the
    /// exact retained private-file length before terminal storage accounting.
    /// This is test-only support for proving that a failed cleanup-time length
    /// reconciliation is classified once and retained exactly rather than
    /// retried by a backstop.
    #[cfg(test)]
    pub(crate) fn restore_active_preparation_bytes_for_test_v1(&self, bytes: u64) {
        let mut state = match self.inner.storage_admission.state.lock() {
            Ok(state) => state,
            Err(poison) => poison.into_inner(),
        };
        let mut found = 0_u64;
        for operation in &mut state.operations {
            if operation.active
                && operation.preparation_current.bytes == 0
                && operation.preparation_current.inodes == 1
            {
                operation.preparation_current.bytes = bytes;
                operation.preparation_high_water.bytes =
                    operation.preparation_high_water.bytes.max(bytes);
                found += 1;
            }
        }
        assert_eq!(found, 1, "expected one zero-byte preparation domain");
    }

    #[cfg(test)]
    pub(crate) fn publish_test_marker_borrowed_v1<C>(
        &self,
        storage_token: FsStorageOperationTokenV1,
        control: &mut C,
    ) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        let preparation = self.inner.root.join("preparation");
        let destination = self.inner.root.join("closures").join("test-marker");
        publish_small_marker_controlled(
            &preparation,
            "test-marker",
            &destination,
            &[0x6d; 8],
            Some(self),
            Some(storage_token),
            None,
            None,
            None,
            control,
        )
        // This test-only helper has no semantic incumbent authenticator.  An
        // incumbent whose temporary marker cleanup failed is therefore not a
        // successful publication: preserve its typed terminal instead of
        // silently dropping it.
        .and_then(MarkerPublicationV1::require_clean)
    }

    /// Build one directly owned private marker whose observed length is one
    /// byte larger than its current root-ledger charge. This test-only setup
    /// isolates cleanup-time length reconciliation without introducing a
    /// second publication error that would become the chronological cause.
    #[cfg(test)]
    pub(crate) fn prepare_test_marker_cleanup_mismatch_v1(
        &self,
        storage_token: FsStorageOperationTokenV1,
    ) -> Result<PathBuf, FsCasErrorV1> {
        self.prepare_test_marker_cleanup_file_v1(storage_token, 9)
    }

    #[cfg(test)]
    pub(crate) fn prepare_test_marker_cleanup_file_v1(
        &self,
        storage_token: FsStorageOperationTokenV1,
        observed_len: u64,
    ) -> Result<PathBuf, FsCasErrorV1> {
        let path = self
            .inner
            .root
            .join("preparation")
            .join("test-marker-cleanup");
        self.record_storage_preparation_create_v1(storage_token)?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .map_err(|error| map_required_filesystem_write_error_v1(&error))?;
        set_private_file_permissions(&path)?;
        self.record_storage_preparation_length_v1(storage_token, 0, 8)?;
        file.write_all(&[0x6d; 8])
            .map_err(|error| map_filesystem_write_error_v1(&error))?;
        file.set_len(observed_len)
            .map_err(|error| map_filesystem_write_error_v1(&error))?;
        Ok(path)
    }

    #[cfg(test)]
    pub(crate) fn cleanup_test_marker_mismatch_borrowed_v1<C>(
        &self,
        storage_token: FsStorageOperationTokenV1,
        control: &mut C,
    ) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        cleanup_unpublished_marker_v1(
            &self
                .inner
                .root
                .join("preparation")
                .join("test-marker-cleanup"),
            Some(self),
            Some(storage_token),
            8,
            FsCasCleanupTargetV1::PreparationSpool,
            control,
        )
    }

    #[cfg(test)]
    pub(crate) fn storage_admission_active_for_test_v1(&self) -> (u64, u64, u64) {
        let state = match self.inner.storage_admission.state.lock() {
            Ok(state) => state,
            Err(poison) => poison.into_inner(),
        };
        let active_operations = state
            .operations
            .iter()
            .filter(|operation| operation.active)
            .count() as u64;
        (
            active_operations,
            state.active_reserved.bytes,
            state.active_reserved.inodes,
        )
    }

    /// Simulate an independent writer winning the carrier no-replace race
    /// after this operation has validated its private pack. The helper is
    /// test-only: it deliberately bypasses this operation's storage authority
    /// so the losing operation can prove that a failed charge rollback stops
    /// before incumbent adoption.
    #[cfg(test)]
    pub(crate) fn install_single_prepared_carrier_for_test_v1(
        &self,
    ) -> Result<PathBuf, FsCasErrorV1> {
        let preparation = self.inner.root.join("preparation");
        let mut candidate = None;
        for entry in fs::read_dir(&preparation)
            .map_err(|error| map_required_filesystem_read_error_v1(&error))?
        {
            let entry = entry.map_err(|error| map_required_filesystem_read_error_v1(&error))?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let mut components = name.split('-');
            let is_private_pack_name = components.next() == Some("pack")
                && components
                    .next()
                    .is_some_and(|process| process.parse::<u32>().is_ok())
                && components.next().is_some_and(|sequence| {
                    sequence.len() == 16 && sequence.bytes().all(|byte| byte.is_ascii_hexdigit())
                })
                && components.next().is_none();
            if !is_private_pack_name {
                continue;
            }
            if candidate.replace(entry.path()).is_some() {
                return Err(FsCasErrorV1::Integrity);
            }
        }
        let candidate = candidate.ok_or(FsCasErrorV1::MissingOccupant)?;
        let mut reader = FilePackReadV1::open(&candidate)?;
        let sealed = read_sealed_shape(&mut reader)?;
        let carrier = self
            .inner
            .root
            .join("carriers")
            .join(hex_id(sealed.id().as_bytes()));
        fs::hard_link(&candidate, &carrier)
            .map_err(|error| map_required_filesystem_write_error_v1(&error))?;
        set_read_only(&carrier)?;
        Ok(carrier)
    }

    fn lock_root_mutex_controlled_v1<'owner, C>(
        &'owner self,
        mutex: &'owner Mutex<()>,
        requested_boundary: FsCasBoundaryV1,
        contended_boundary: FsCasBoundaryV1,
        acquired_boundary: FsCasBoundaryV1,
        terminated_boundary: FsCasBoundaryV1,
        control: &mut C,
    ) -> Result<MutexGuard<'owner, ()>, FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        control.boundary_reached(requested_boundary);
        loop {
            if let Err(error) = self.ensure_valid() {
                return Err(Self::retain_root_lock_wait_terminal_v1(
                    terminated_boundary,
                    error,
                    control,
                ));
            }
            match mutex.try_lock() {
                Ok(guard) => {
                    #[cfg(test)]
                    let validation = control
                        .inject_root_lock_post_acquire_validation_failure()
                        .map_or_else(|| self.ensure_valid(), Err);
                    #[cfg(not(test))]
                    let validation = self.ensure_valid();
                    if let Err(error) = validation {
                        drop(guard);
                        return Err(Self::retain_root_lock_wait_terminal_v1(
                            terminated_boundary,
                            error,
                            control,
                        ));
                    }
                    let acquired = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        control.boundary_reached(acquired_boundary);
                    }));
                    if let Err(payload) = acquired {
                        // Retain ownership of the guard outside the caught
                        // callback. Dropping it normally before resuming the
                        // caller's payload prevents a control unwind from
                        // manufacturing latent synchronization poison.
                        drop(guard);
                        std::panic::resume_unwind(payload);
                    }
                    return Ok(guard);
                }
                Err(TryLockError::WouldBlock) => {
                    control.boundary_reached(contended_boundary);
                    if control.cancellation_requested() {
                        return Err(Self::retain_root_lock_wait_terminal_v1(
                            terminated_boundary,
                            FsCasErrorV1::Core(CoreError::Cancelled),
                            control,
                        ));
                    }
                    if control.deadline_exceeded() {
                        return Err(Self::retain_root_lock_wait_terminal_v1(
                            terminated_boundary,
                            FsCasErrorV1::Core(CoreError::Deadline),
                            control,
                        ));
                    }
                    std::thread::sleep(ADMISSION_CONTROL_POLL);
                }
                Err(TryLockError::Poisoned(poisoned)) => {
                    // Release the recovered guard before invalidation. A
                    // poisoned root coordination primitive is an impossible
                    // shared-owner state, not a retryable filesystem I/O
                    // error. Persist fail-closed invalidation immediately.
                    drop(poisoned.into_inner());
                    let first = FsCasErrorV1::SynchronizationPoisoned;
                    let invalidation = self.invalidate_root_controlled_v1(control);
                    let terminal = match invalidation {
                        Ok(()) => first,
                        Err(dominant) => first.dominated_by_v1(dominant),
                    };
                    return Err(Self::retain_root_lock_wait_terminal_v1(
                        terminated_boundary,
                        terminal,
                        control,
                    ));
                }
            }
        }
    }

    /// A lock-wait terminal is already authoritative before its observation
    /// boundary is emitted. The observation owns no guard or cleanup state,
    /// so a later callback unwind cannot replace cancellation, deadline,
    /// invalidation, synchronization poison, or invalidation dominance.
    fn retain_root_lock_wait_terminal_v1<C>(
        terminated_boundary: FsCasBoundaryV1,
        terminal: FsCasErrorV1,
        control: &mut C,
    ) -> FsCasErrorV1
    where
        C: FsCasControlV1 + ?Sized,
    {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            control.boundary_reached(terminated_boundary);
        }));
        terminal
    }

    fn lock_visibility_controlled_v1<C>(
        &self,
        control: &mut C,
    ) -> Result<MutexGuard<'_, ()>, FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        self.lock_root_mutex_controlled_v1(
            &self.inner.visibility,
            FsCasBoundaryV1::VisibilityLockRequested,
            FsCasBoundaryV1::VisibilityLockContended,
            FsCasBoundaryV1::VisibilityLockAcquired,
            FsCasBoundaryV1::VisibilityLockWaitTerminated,
            control,
        )
    }

    fn lock_visibility_v1(&self) -> Result<MutexGuard<'_, ()>, FsCasErrorV1> {
        let mut control = ContinueFsCasControlV1;
        self.lock_visibility_controlled_v1(&mut control)
    }

    fn lock_publication_controlled_v1<C>(
        &self,
        control: &mut C,
    ) -> Result<MutexGuard<'_, ()>, FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        self.lock_root_mutex_controlled_v1(
            &self.inner.publication,
            FsCasBoundaryV1::PublicationLockRequested,
            FsCasBoundaryV1::PublicationLockContended,
            FsCasBoundaryV1::PublicationLockAcquired,
            FsCasBoundaryV1::PublicationLockWaitTerminated,
            control,
        )
    }

    fn unlock_visibility_controlled_v1<C>(&self, guard: MutexGuard<'_, ()>, control: &mut C)
    where
        C: FsCasControlV1 + ?Sized,
    {
        drop(guard);
        control.boundary_reached(FsCasBoundaryV1::VisibilityLockReleased);
    }

    fn unlock_publication_controlled_v1<C>(&self, guard: MutexGuard<'_, ()>, control: &mut C)
    where
        C: FsCasControlV1 + ?Sized,
    {
        drop(guard);
        control.boundary_reached(FsCasBoundaryV1::PublicationLockReleased);
    }

    #[cfg(all(test, feature = "c3-polymorphism"))]
    pub(crate) fn issue_pending_admission_for_test_v1(
        &self,
        cancellation_key: u64,
    ) -> Result<PendingAdmissionTicketForTestV1<'_>, FsCasErrorV1> {
        self.ensure_valid()?;
        let mut counters = OperationCountersV1::default();
        self.inner
            .operation_admission
            .issue(
                FsOperationKindV1::RootExtraction,
                cancellation_key,
                &mut counters,
            )
            .map(|ticket| PendingAdmissionTicketForTestV1 { _ticket: ticket })
            .map_err(|failure| failure.first)
    }

    /// Acquire the shared root's fixed-profile operation authority before any
    /// typed request, supplier, sink, or preparation object is inspected.
    #[cfg(feature = "c3-polymorphism")]
    pub(crate) fn begin_operation_capability_v1<C>(
        &self,
        operation_kind: FsOperationKindV1,
        cancellation_key: u64,
        counters: &mut OperationCountersV1,
        control: &mut C,
    ) -> Result<FsOperationCapabilityV1<'_>, FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        self.ensure_valid()?;
        let mut admission = match self.inner.operation_admission.acquire(
            operation_kind,
            cancellation_key,
            control,
            counters,
        ) {
            OperationAdmissionAcquireOutcomeV1::Granted(admission) => admission,
            OperationAdmissionAcquireOutcomeV1::GrantedWithObservationFailure {
                mut admission,
                first,
            } => {
                return Err(self.finish_failed_operation_entry_v1(
                    &mut admission,
                    first,
                    counters,
                    control,
                ));
            }
            OperationAdmissionAcquireOutcomeV1::Rejected {
                first: error,
                fail_closed: true,
            }
            | OperationAdmissionAcquireOutcomeV1::Rejected {
                first: error @ (FsCasErrorV1::Integrity | FsCasErrorV1::SynchronizationPoisoned),
                fail_closed: false,
            } => {
                return Err(match self.invalidate_root_controlled_v1(control) {
                    Ok(()) => error,
                    Err(dominant) => error.dominated_by_v1(dominant),
                });
            }
            OperationAdmissionAcquireOutcomeV1::Rejected {
                first,
                fail_closed: false,
            } => return Err(first),
        };
        if let Err(error) = self.ensure_valid() {
            return Err(self.finish_failed_operation_entry_v1(
                &mut admission,
                error,
                counters,
                control,
            ));
        }
        let reservation = match self.inner.operation_ledger.reserve_operation_unplanned() {
            Ok(reservation) => Ok(reservation),
            Err(CoreError::ResourceRefused) => {
                match counters.record_root_admission_memory_refusal_v1() {
                    Ok(()) => Err(FsCasErrorV1::ResourceExhausted(FsCasResourceV1::Memory)),
                    Err(error) => Err(FsCasErrorV1::Core(error)),
                }
            }
            Err(other) => Err(FsCasErrorV1::Core(other)),
        };
        let reservation = match reservation {
            Ok(reservation) => reservation,
            Err(error) => {
                return Err(self.finish_failed_operation_entry_v1(
                    &mut admission,
                    error,
                    counters,
                    control,
                ));
            }
        };
        Ok(FsOperationCapabilityV1 {
            owner: self,
            operation_kind,
            reservation,
            storage: None,
            admission,
        })
    }

    /// Return a root-admission lease after operation entry fails before a
    /// capability exists.  This is an ordinary typed return path: release may
    /// not be delegated to `Drop`, and a poisoned release must synchronously
    /// establish the persistent fail-closed barrier without erasing the
    /// chronological entry failure.
    #[cfg(feature = "c3-polymorphism")]
    fn finish_failed_operation_entry_v1<C>(
        &self,
        admission: &mut RootAdmissionLeaseV1<'_>,
        first: FsCasErrorV1,
        counters: &mut OperationCountersV1,
        control: &mut C,
    ) -> FsCasErrorV1
    where
        C: FsCasControlV1 + ?Sized,
    {
        if admission.release_v1().is_ok() {
            return first;
        }
        // Failure to record this diagnostic cannot replace the authority
        // failure already being returned. The queue release itself is the
        // correctness event and requires fail-closed invalidation.
        let _ = counters.record_root_admission_release_failure_v1();
        self.fail_closed_preserving_error_after_unwind_v1(first, control)
    }
    #[cfg(feature = "c3-polymorphism")]
    fn preparation_path_capacity_bound_v1(&self, prefix: &str) -> CoreResult<u64> {
        // `unique_private_path` appends the fixed preparation component, the
        // caller-owned prefix, a decimal `u32` process id, and a 16-digit
        // sequence. Keep one page of explicit allocator/path slack so this
        // declaration can be made before a private path is allocated. Every
        // created handle is checked against the declaration by the caller.
        u64::try_from(self.inner.root.capacity())
            .map_err(|_| CoreError::IntegerOverflow)?
            .checked_add(u64::try_from(prefix.len()).map_err(|_| CoreError::IntegerOverflow)?)
            .and_then(|bytes| bytes.checked_add(4_096))
            .ok_or(CoreError::IntegerOverflow)
    }

    /// Side-effect-free resident declaration used before the one operation
    /// slot is requested. This allocates or opens no private carrier.
    #[cfg(feature = "c3-polymorphism")]
    pub(crate) fn private_pack_resident_memory_bound_v1(&self) -> CoreResult<u64> {
        u64::try_from(core::mem::size_of::<FsPrivatePackV1>())
            .map_err(|_| CoreError::IntegerOverflow)?
            .checked_add(self.preparation_path_capacity_bound_v1("pack")?)
            .ok_or(CoreError::IntegerOverflow)
    }

    /// Side-effect-free resident declaration used before an operation spool
    /// path is allocated or its `create_new` is attempted.
    #[cfg(feature = "c3-polymorphism")]
    pub(crate) fn operation_spool_resident_memory_bound_v1(&self, prefix: &str) -> CoreResult<u64> {
        u64::try_from(core::mem::size_of::<FsOperationSpoolV1>())
            .map_err(|_| CoreError::IntegerOverflow)?
            .checked_add(self.preparation_path_capacity_bound_v1(prefix)?)
            .ok_or(CoreError::IntegerOverflow)
    }

    /// The occupied reader contains no source- or workspace-sized allocation.
    #[cfg(feature = "c3-polymorphism")]
    pub(crate) fn occupied_resident_memory_bound_v1(&self) -> CoreResult<u64> {
        u64::try_from(core::mem::size_of::<FsCasOccupiedV1>())
            .map_err(|_| CoreError::IntegerOverflow)
    }

    /// Create a new engine-private namespace. The parent must already exist,
    /// be absolute, canonical, and contain no symbolic-link components.
    pub fn create_new(root: &Path) -> Result<Self, FsCasErrorV1> {
        validate_new_root(root)?;
        create_private_directory(root)?;
        let generation = derive_generation(root)?;
        let ownership = match acquire_root_ownership(root, generation) {
            Ok(ownership) => ownership,
            Err(error) => {
                // A racing owner owns its token even though this caller won
                // the directory create; never remove a root another owner has
                // already claimed.
                if matches!(error, FsCasErrorV1::Busy | FsCasErrorV1::Invalidated) {
                    return Err(error);
                }
                return Err(root_initialization_cleanup_result_v1(
                    error,
                    fs::remove_dir_all(root),
                ));
            }
        };
        let setup: Result<(), FsCasErrorV1> = (|| {
            create_private_directory(&root.join("preparation"))?;
            create_private_directory(&root.join("carriers"))?;
            create_private_directory(&root.join("objects"))?;
            create_private_directory(&root.join("catalog"))?;
            create_private_directory(&root.join("closures"))?;
            publish_small_marker(
                &root.join("preparation"),
                "generation",
                &root.join("generation"),
                &encode_generation_marker(generation),
            )?
            .require_clean()?;
            validate_same_filesystem(root, &root.join("preparation"))?;
            validate_same_filesystem(root, &root.join("carriers"))?;
            validate_same_filesystem(root, &root.join("objects"))?;
            validate_same_filesystem(root, &root.join("catalog"))?;
            validate_same_filesystem(root, &root.join("closures"))?;
            Ok(())
        })();
        if let Err(error) = setup {
            drop(ownership);
            return Err(root_initialization_cleanup_result_v1(
                error,
                fs::remove_dir_all(root),
            ));
        }
        let inner = match shared_root_owner(root, generation, Some(ownership)) {
            Ok(inner) => inner,
            Err(error) => {
                return Err(root_initialization_cleanup_result_v1(
                    error,
                    fs::remove_dir_all(root),
                ));
            }
        };
        let cas = Self { inner };
        let fixed_charge = match cas.fixed_handle_ledger_charge_bytes() {
            Ok(charge) => charge,
            Err(error) => {
                drop(cas);
                return Err(root_initialization_cleanup_result_v1(
                    FsCasErrorV1::Core(error),
                    fs::remove_dir_all(root),
                ));
            }
        };
        if fixed_charge > BASE_LEDGER_BYTES {
            drop(cas);
            return Err(root_initialization_cleanup_result_v1(
                FsCasErrorV1::Core(CoreError::ResourceRefused),
                fs::remove_dir_all(root),
            ));
        }
        Ok(cas)
    }

    /// Reopen the committed catalog only. Preparation files are never
    /// scanned, adopted, replayed, or promoted.
    pub fn open_existing(root: &Path) -> Result<Self, FsCasErrorV1> {
        let mut control = ContinueFsCasControlV1;
        Self::open_existing_controlled_inner_v1(root, &mut control)
    }

    fn open_existing_controlled_inner_v1<C>(
        root: &Path,
        control: &mut C,
    ) -> Result<Self, FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::RootValidationRead)?;
        validate_existing_root(root)?;
        if root_invalidation_barrier_present_v1(&root.join(INVALIDATED_ROOT_NAME))? {
            return Err(FsCasErrorV1::Invalidated);
        }
        for child in ["preparation", "carriers", "objects", "catalog", "closures"] {
            validate_required_root_directory(&root.join(child))?;
            validate_same_filesystem(root, &root.join(child))?;
        }
        sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::GenerationMarkerRead)?;
        let generation = read_generation_marker(&root.join("generation"))?;
        let cas = Self {
            inner: shared_root_owner(root, generation, None)?,
        };
        cas.ensure_valid()?;
        if cas.fixed_handle_ledger_charge_bytes()? > BASE_LEDGER_BYTES {
            return Err(FsCasErrorV1::Core(CoreError::ResourceRefused));
        }
        Ok(cas)
    }

    /// Deterministic logical language-owned bytes charged to the frozen 8 MiB
    /// fixed domain: the handle, the complete shared `Arc` allocation
    /// (reference counters, alignment, generation and synchronization state),
    /// the exact 1,024-cell phase-one ticket population, and the root path
    /// allocation capacity. This deliberately makes no RSS/PSS, page-cache, or
    /// allocator-internal metadata claim; those require independent platform
    /// measurement.
    pub fn fixed_handle_ledger_charge_bytes(&self) -> CoreResult<u64> {
        let root_capacity =
            u64::try_from(self.inner.root.capacity()).map_err(|_| CoreError::IntegerOverflow)?;
        let arc_header = Layout::new::<[AtomicUsize; 2]>();
        let (arc_layout, _) = arc_header
            .extend(Layout::new::<FsCasInnerV1>())
            .map_err(|_| CoreError::IntegerOverflow)?;
        let arc_allocation = u64::try_from(arc_layout.pad_to_align().size())
            .map_err(|_| CoreError::IntegerOverflow)?;
        let queue_ticket_bytes = u64::try_from(
            core::mem::size_of::<QueueTicketV1>()
                .checked_mul(MAX_ADMISSION_TICKETS)
                .ok_or(CoreError::IntegerOverflow)?,
        )
        .map_err(|_| CoreError::IntegerOverflow)?;
        u64::try_from(core::mem::size_of::<Self>())
            .map_err(|_| CoreError::IntegerOverflow)?
            .checked_add(arc_allocation)
            .and_then(|bytes| bytes.checked_add(queue_ticket_bytes))
            .and_then(|bytes| bytes.checked_add(root_capacity))
            .ok_or(CoreError::IntegerOverflow)
    }

    #[cfg(test)]
    pub fn begin_private_pack(&self) -> Result<FsPrivatePackV1, FsCasErrorV1> {
        self.begin_private_pack_inner_v1(None)
    }

    #[cfg(feature = "c3-polymorphism")]
    pub(crate) fn begin_private_pack_borrowed_v1(
        &self,
        token: FsStorageOperationTokenV1,
    ) -> Result<FsPrivatePackV1, FsCasErrorV1> {
        self.begin_private_pack_inner_v1(Some(token))
    }

    fn begin_private_pack_inner_v1(
        &self,
        storage_token: Option<FsStorageOperationTokenV1>,
    ) -> Result<FsPrivatePackV1, FsCasErrorV1> {
        if let Some(token) = storage_token {
            self.validate_storage_token_v1(token)?;
        }
        self.ensure_valid()?;
        let _guard = self.lock_visibility_v1()?;
        self.ensure_valid()?;
        validate_required_root_directory(&self.inner.root.join("preparation"))?;
        let path = unique_private_path(&self.inner.root.join("preparation"), "pack")?;
        Ok(FsPrivatePackV1 {
            owner: self.clone(),
            path,
            state: PrivatePackStateV1::Empty,
            first_error: None,
            storage_token,
            accounted_len: 0,
            preparation_accounted: false,
        })
    }

    /// Create an operation-private, file-backed metadata spool.
    ///
    /// The spool is deliberately crate-private: it is an implementation
    /// detail used by bounded storage operations, not a caller-visible CAS
    /// surface. Its name is removed on every drop path and it is never
    /// scanned or recovered by [`Self::open_existing`].
    #[cfg(all(test, feature = "c3-polymorphism"))]
    pub(crate) fn begin_operation_spool_v1<C>(
        &self,
        prefix: &str,
        control: &mut C,
    ) -> Result<FsOperationSpoolV1, FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        self.begin_operation_spool_inner_v1(prefix, None, control)
    }

    #[cfg(feature = "c3-polymorphism")]
    pub(crate) fn begin_operation_spool_borrowed_v1<C>(
        &self,
        prefix: &str,
        token: FsStorageOperationTokenV1,
        control: &mut C,
    ) -> Result<FsOperationSpoolV1, FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        self.begin_operation_spool_inner_v1(prefix, Some(token), control)
    }

    #[cfg(feature = "c3-polymorphism")]
    fn begin_operation_spool_inner_v1<C>(
        &self,
        prefix: &str,
        storage_token: Option<FsStorageOperationTokenV1>,
        control: &mut C,
    ) -> Result<FsOperationSpoolV1, FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        if let Some(token) = storage_token {
            self.validate_storage_token_v1(token)?;
        }
        self.ensure_valid()?;
        let guard = self.lock_visibility_controlled_v1(control)?;
        let mut path = None;
        let mut file = None;
        let mut preparation_accounted = false;
        let construction = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || -> Result<(), FsCasErrorV1> {
                self.ensure_valid()?;
                validate_required_root_directory(&self.inner.root.join("preparation"))?;
                path = Some(unique_private_path(
                    &self.inner.root.join("preparation"),
                    prefix,
                )?);
                if let Some(token) = storage_token {
                    // Charge the private namespace name before invoking the
                    // creating filesystem operation. A poisoned or stale
                    // accounting owner therefore fails without leaving an
                    // untracked preparation inode.
                    if let Err(error) = self.record_storage_preparation_create_v1(token) {
                        return Err(self.fail_closed_preserving_error_controlled_v1(error, control));
                    }
                    preparation_accounted = true;
                }
                let prepared_path = path.as_ref().ok_or(FsCasErrorV1::Integrity)?;
                let opened = sample_filesystem_fault_v1(
                    control,
                    FsCasFilesystemBoundaryV1::PreparationCreate,
                )
                .and_then(|()| {
                    OpenOptions::new()
                        .read(true)
                        .write(true)
                        .create_new(true)
                        .open(prepared_path)
                        .map_err(|error| map_required_filesystem_write_error_v1(&error))
                });
                match opened {
                    Ok(prepared_file) => file = Some(prepared_file),
                    Err(error) => {
                        if let Some(token) = storage_token.filter(|_| preparation_accounted) {
                            if self.record_storage_preparation_remove_v1(token, 0).is_err() {
                                let cleanup = self.cleanup_failure_controlled_v1(
                                    FsCasCleanupTargetV1::PreparationSpool,
                                    control,
                                );
                                return Err(error.dominated_by_v1(cleanup));
                            }
                            preparation_accounted = false;
                        }
                        path = None;
                        return Err(error);
                    }
                }
                sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::PermissionChange)?;
                set_private_file_permissions(prepared_path)
            },
        ));
        // No callback or cleanup may unwind a held root mutex. The partial
        // path/file/accounting state above remains locally owned after this
        // normal guard release and is then either transferred or explicitly
        // cleaned below.
        drop(guard);

        match construction {
            Ok(Ok(())) => {}
            Ok(Err(original)) => {
                drop(file.take());
                if let Some(prepared_path) = path.as_deref() {
                    let cleanup = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        cleanup_unpublished_marker_v1(
                            prepared_path,
                            Some(self),
                            storage_token.filter(|_| preparation_accounted),
                            0,
                            FsCasCleanupTargetV1::PreparationSpool,
                            control,
                        )
                    }));
                    return match cleanup {
                        Ok(Ok(())) => Err(original),
                        Ok(Err(error)) => Err(original.dominated_by_v1(error)),
                        Err(payload) => {
                            // Cleanup owns the partial path and its exact
                            // preparation charge, so its unwind is terminally
                            // classifiable even though no spool value could be
                            // returned. Persist invalidation once, retain the
                            // initiating construction cause, and transport the
                            // original cleanup payload to the preparation
                            // coordinator in one bounded private carrier.
                            let cleanup =
                                FsCasErrorV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool);
                            let invalidation =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    self.cleanup_failure_controlled_v1(
                                        FsCasCleanupTargetV1::PreparationSpool,
                                        control,
                                    )
                                }));
                            let cleanup_terminal = match invalidation {
                                Ok(error) => error,
                                Err(_) => {
                                    // A second injected unwind cannot prevent
                                    // fail-closed persistence or replace the
                                    // first cleanup payload. Complete the same
                                    // transition without another callback.
                                    let mut backstop = ContinueFsCasControlV1;
                                    match self.invalidate_root_controlled_v1(&mut backstop) {
                                        Ok(()) => cleanup,
                                        Err(error) => cleanup.dominated_by_v1(error),
                                    }
                                }
                            };
                            std::panic::resume_unwind(Box::new(
                                FsOperationSpoolConstructionUnwindV1::new_v1(
                                    original.dominated_by_v1(cleanup_terminal),
                                    payload,
                                ),
                            ))
                        }
                    };
                }
                return Err(original);
            }
            Err(payload) => {
                let file_was_created = file.is_some();
                drop(file.take());
                if let Some(prepared_path) = path.as_deref() {
                    if file_was_created {
                        let cleanup =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                cleanup_unpublished_marker_v1(
                                    prepared_path,
                                    Some(self),
                                    storage_token.filter(|_| preparation_accounted),
                                    0,
                                    FsCasCleanupTargetV1::PreparationSpool,
                                    control,
                                )
                            }));
                        match cleanup {
                            Ok(Ok(())) => {}
                            Ok(Err(terminal)) => {
                                // The initiating callback unwind is resumed
                                // only when explicit cleanup completed.  A
                                // typed cleanup/invalidation failure instead
                                // crosses the outer preparation catch in this
                                // bounded carrier so earlier returned spools
                                // are still finished exactly once before the
                                // operation returns the classified terminal.
                                std::panic::resume_unwind(Box::new(
                                    FsOperationSpoolConstructionUnwindV1::new_v1(terminal, payload),
                                ))
                            }
                            Err(cleanup_payload) => {
                                // The partial path remains in exact local
                                // custody. Classify the cleanup unwind and
                                // persist invalidation without allowing a
                                // second callback unwind to skip that state
                                // transition. Retain both bounded payloads
                                // until the preparation coordinator has
                                // finished every earlier spool.
                                let terminal = self.cleanup_failure_after_unwind_v1(
                                    FsCasCleanupTargetV1::PreparationSpool,
                                    control,
                                );
                                std::panic::resume_unwind(Box::new(
                                    FsOperationSpoolConstructionUnwindV1::new_with_secondary_v1(
                                        terminal,
                                        payload,
                                        cleanup_payload,
                                    ),
                                ))
                            }
                        }
                    } else if let Some(token) = storage_token.filter(|_| preparation_accounted) {
                        // A controlled unwind at `PreparationCreate` occurs
                        // before the actual create-new call. No filesystem
                        // target exists, but the pre-charged namespace inode
                        // is still locally owned and must be released before
                        // the original unwind leaves this boundary.
                        if let Err(first) = self.record_storage_preparation_remove_v1(token, 0) {
                            let cleanup = self.cleanup_failure_after_unwind_v1(
                                FsCasCleanupTargetV1::PreparationSpool,
                                control,
                            );
                            std::panic::resume_unwind(Box::new(
                                FsOperationSpoolConstructionUnwindV1::new_v1(
                                    first.dominated_by_v1(cleanup),
                                    payload,
                                ),
                            ))
                        }
                    }
                }
                std::panic::resume_unwind(payload)
            }
        }

        Ok(FsOperationSpoolV1 {
            owner: self.clone(),
            path: path.ok_or(FsCasErrorV1::Integrity)?,
            file,
            len: 0,
            bytes_read: 0,
            read_calls: 0,
            bytes_written: 0,
            cleanup_complete: false,
            cleanup_error: None,
            storage_token,
        })
    }

    /// Validate, install, reopen, and validate a sealed operation pack before
    /// publishing its catalog marker. The hard link is a no-replace,
    /// same-filesystem ownership transfer; the private name is then removed.
    #[cfg(test)]
    pub fn admit_pack<M>(
        &self,
        prepared: &mut FsPrivatePackV1,
        metadata: &mut M,
        ledger: &ResourceLedgerV1,
        counters: &mut OperationCountersV1,
        scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    ) -> Result<FsPackAdmissionV1, FsCasErrorV1>
    where
        M: PackIndexSpoolV1 + ?Sized,
    {
        self.admit_pack_controlled(
            prepared,
            metadata,
            ledger,
            counters,
            scratch,
            &mut ContinueFsCasControlV1,
        )
    }

    /// The controlled form samples cancellation before filesystem visibility
    /// transitions and between bounded incumbent-comparison windows. Every
    /// error path destroys the still-private preparation name before return.
    #[cfg(test)]
    pub fn admit_pack_controlled<M, C>(
        &self,
        prepared: &mut FsPrivatePackV1,
        metadata: &mut M,
        ledger: &ResourceLedgerV1,
        counters: &mut OperationCountersV1,
        scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
        control: &mut C,
    ) -> Result<FsPackAdmissionV1, FsCasErrorV1>
    where
        M: PackIndexSpoolV1 + ?Sized,
        C: FsCasControlV1 + ?Sized,
    {
        self.admit_pack_controlled_inner(
            prepared,
            metadata,
            PackAdmissionAuthorityV1::Independent(ledger),
            counters,
            scratch,
            control,
        )
    }

    #[cfg(feature = "c3-polymorphism")]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn admit_pack_borrowed_controlled_v1<M, C>(
        &self,
        prepared: &mut FsPrivatePackV1,
        metadata: &mut M,
        reservation: &OperationReservationV1<'_>,
        storage_token: FsStorageOperationTokenV1,
        counters: &mut OperationCountersV1,
        scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
        control: &mut C,
    ) -> Result<FsPackAdmissionV1, FsCasErrorV1>
    where
        M: PackIndexSpoolV1 + ?Sized,
        C: FsCasControlV1 + ?Sized,
    {
        self.admit_pack_controlled_inner(
            prepared,
            metadata,
            PackAdmissionAuthorityV1::Borrowed {
                reservation,
                storage_token,
            },
            counters,
            scratch,
            control,
        )
    }

    #[cfg(any(test, feature = "c3-polymorphism"))]
    #[allow(clippy::too_many_arguments)]
    fn admit_pack_controlled_inner<M, C>(
        &self,
        prepared: &mut FsPrivatePackV1,
        metadata: &mut M,
        authority: PackAdmissionAuthorityV1<'_, '_>,
        counters: &mut OperationCountersV1,
        scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
        control: &mut C,
    ) -> Result<FsPackAdmissionV1, FsCasErrorV1>
    where
        M: PackIndexSpoolV1 + ?Sized,
        C: FsCasControlV1 + ?Sized,
    {
        self.ensure_valid()?;
        let storage_token = authority.storage_token_v1();
        let declared_for_unwind = prepared.sealed()?;
        let mut carrier_custody = CarrierPublicationCustodyV1::Absent;
        let mut locator_custody = LocatorPublicationCustodyV1::default();
        let mut catalog_marker_custody = ImmutableMarkerCustodyV1::default();
        // A root-owned immutable charge is acquired before the no-replace
        // carrier link. Keep that prepublication custody outside the unwind
        // boundary so a callback panic between the charge and the link cannot
        // strand logical retained storage without a corresponding name.
        let mut prepublication_carrier_charge_held = false;
        // Keep guard ownership outside the admission-wide unwind boundary. A
        // controlled callback may unwind while publication is serialized, but
        // the guard must then be dropped normally after the payload is caught;
        // otherwise Rust poisons a healthy root mutex even when no immutable
        // publication occurred.
        let mut publication_guard = None;
        let terminal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (|| {
                if !Arc::ptr_eq(&prepared.owner.inner, &self.inner) {
                    return Err(FsCasErrorV1::Integrity);
                }
                counters.observe_layerfs_open_file_handles(1);
                sample_control(control, FsCasBoundaryV1::BeforeCandidateValidation)?;
                let declared = prepared.sealed()?.ok_or(FsCasErrorV1::Integrity)?;
                let validated = match validate_pack_for_operation_v1(
                    prepared,
                    metadata,
                    scratch,
                    declared.record_count(),
                    authority,
                    counters,
                    control,
                ) {
                    Ok(validated) => validated,
                    Err(error) => {
                        return Err(prepared.take_first_error_typed_v1().unwrap_or(error));
                    }
                };
                if declared != validated {
                    return Err(FsCasErrorV1::Integrity);
                }
                sample_control(control, FsCasBoundaryV1::AfterCandidateValidation)?;

                // Candidate validation only reads an operation-private file. Take
                // the shared-root visibility lock after that work, then recheck
                // validity before observing or changing the common namespace.
                publication_guard = Some(self.lock_publication_controlled_v1(control)?);
                self.ensure_valid()?;

                let name = hex_id(validated.id().as_bytes());
                let carrier_path = self.inner.root.join("carriers").join(&name);
                let marker_path = self.inner.root.join("catalog").join(&name);
                let transaction = NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed);
                validate_required_root_directory(&self.inner.root.join("carriers"))?;
                validate_required_root_directory(&self.inner.root.join("objects"))?;
                validate_required_root_directory(&self.inner.root.join("catalog"))?;

                let incumbent_marker =
                    open_regular_file_if_present(&marker_path).map_err(|error| {
                        if error == FsCasErrorV1::Integrity {
                            FsCasErrorV1::MalformedOccupant
                        } else {
                            error
                        }
                    })?;
                let incumbent_carrier =
                    open_regular_file_if_present(&carrier_path).map_err(|error| {
                        if error == FsCasErrorV1::Integrity {
                            FsCasErrorV1::MalformedOccupant
                        } else {
                            error
                        }
                    })?;
                if incumbent_marker.is_some() || incumbent_carrier.is_some() {
                    drop(incumbent_marker);
                    drop(incumbent_carrier);
                    drop(publication_guard.take());
                    return self.admit_against_incumbent(
                        prepared,
                        metadata,
                        authority,
                        counters,
                        scratch,
                        validated,
                        &carrier_path,
                        &marker_path,
                        control,
                    );
                }

                // Every counter delta that can be known from the validated private
                // pack is checked before the first carrier or locator name can
                // become visible. The cleanup-residue bound covers the carrier and
                // every fixed-size locator in case a later rollback itself fails.
                let mut publication_capacity = *counters;
                publication_capacity.record_fscas_catalog_operation()?;
                publication_capacity.record_pack_storage(validated.pack_len(), 0)?;
                publication_capacity.record_unreachable_installed_residue(validated.pack_len())?;
                let retained_carrier_residue_bytes =
                    publication_capacity.unreachable_installed_residue_bytes;
                let locator_residue_bound = u64::from(validated.record_count())
                    .checked_mul(
                        u64::try_from(PERSISTENT_LOCATOR_BYTES_V1)
                            .map_err(|_| FsCasErrorV1::Integrity)?,
                    )
                    .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
                let cleanup_residue_bound = validated
                    .pack_len()
                    .checked_add(locator_residue_bound)
                    .and_then(|bytes| bytes.checked_add(CATALOG_MARKER_BYTES as u64))
                    .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
                let mut cleanup_capacity = *counters;
                cleanup_capacity.record_unreachable_installed_residue(cleanup_residue_bound)?;

                if let Some(token) = storage_token {
                    // Reserve the immutable namespace transition before the
                    // no-replace link.  Therefore a poisoned/stale accounting
                    // owner cannot create a visible but untracked carrier.
                    if let Err(error) =
                        self.record_storage_immutable_install_v1(token, validated.pack_len(), 1)
                    {
                        return Err(self.fail_closed_preserving_error_controlled_v1(error, control));
                    }
                    prepublication_carrier_charge_held = true;
                }
                if let Err(original) =
                    sample_control(control, FsCasBoundaryV1::BeforeCarrierInstall)
                {
                    let terminal = self.release_prepublication_carrier_charge_preserving_error_v1(
                        storage_token,
                        validated.pack_len(),
                        control,
                        original,
                    );
                    prepublication_carrier_charge_held = false;
                    return Err(terminal);
                }
                if let Err(original) =
                    sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::CarrierHardLink)
                {
                    let terminal = self.release_prepublication_carrier_charge_preserving_error_v1(
                        storage_token,
                        validated.pack_len(),
                        control,
                        original,
                    );
                    prepublication_carrier_charge_held = false;
                    return Err(terminal);
                }
                match fs::hard_link(&prepared.path, &carrier_path) {
                    Ok(()) => {
                        prepublication_carrier_charge_held = false;
                        carrier_custody = CarrierPublicationCustodyV1::InstalledUnreported;
                    }
                    Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                        let released = self.release_prepublication_carrier_charge_v1(
                            storage_token,
                            validated.pack_len(),
                            control,
                        );
                        prepublication_carrier_charge_held = false;
                        released?;
                        drop(publication_guard.take());
                        return self.admit_against_incumbent(
                            prepared,
                            metadata,
                            authority,
                            counters,
                            scratch,
                            validated,
                            &carrier_path,
                            &marker_path,
                            control,
                        );
                    }
                    Err(error) if is_unsupported_link_error(&error) => {
                        let terminal = self
                            .release_prepublication_carrier_charge_preserving_error_v1(
                                storage_token,
                                validated.pack_len(),
                                control,
                                FsCasErrorV1::Unsupported,
                            );
                        prepublication_carrier_charge_held = false;
                        return Err(terminal);
                    }
                    Err(error) => {
                        let original = map_required_filesystem_write_error_v1(&error);
                        let terminal = self
                            .release_prepublication_carrier_charge_preserving_error_v1(
                                storage_token,
                                validated.pack_len(),
                                control,
                                original,
                            );
                        prepublication_carrier_charge_held = false;
                        return Err(terminal);
                    }
                }

                // Once the carrier link exists, every validation, permission, and
                // private-alias transition is one owned publication transaction.
                // A callback panic cannot skip both carrier rollback and explicit
                // private-pack cleanup and leave those names to Drop.
                let mut carrier_rollback_attempted = false;
                let carrier_terminal =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        (|| -> Result<(), FsCasErrorV1> {
                            if let Err(error) =
                                sample_control(control, FsCasBoundaryV1::AfterCarrierInstall)
                            {
                                carrier_rollback_attempted = true;
                                return Err(self.rollback_unpublished_carrier_preserving_error_v1(
                                    &carrier_path,
                                    validated,
                                    storage_token,
                                    counters,
                                    &mut carrier_custody,
                                    control,
                                    error,
                                ));
                            }

                            let installed_validation = (|| {
                                let mut installed = FilePackReadV1::open_occupant(&carrier_path)?;
                                counters.observe_layerfs_open_file_handles(2);
                                let observed = match validate_pack_for_operation_v1(
                                    &mut installed,
                                    metadata,
                                    scratch,
                                    validated.record_count(),
                                    authority,
                                    counters,
                                    control,
                                ) {
                                    Ok(observed) => observed,
                                    Err(error) => {
                                        return Err(installed
                                            .take_first_error_typed_v1()
                                            .unwrap_or(error));
                                    }
                                };
                                counters.record_fscas_read(
                                    installed.bytes_read,
                                    installed.read_calls,
                                )?;
                                if observed != validated {
                                    return Err(FsCasErrorV1::Integrity);
                                }
                                Ok(())
                            })();
                            if let Err(error) = installed_validation {
                                carrier_rollback_attempted = true;
                                return Err(self.rollback_unpublished_carrier_preserving_error_v1(
                                    &carrier_path,
                                    validated,
                                    storage_token,
                                    counters,
                                    &mut carrier_custody,
                                    control,
                                    error,
                                ));
                            }
                            if let Err(error) =
                                sample_control(control, FsCasBoundaryV1::AfterCarrierValidation)
                            {
                                carrier_rollback_attempted = true;
                                return Err(self.rollback_unpublished_carrier_preserving_error_v1(
                                    &carrier_path,
                                    validated,
                                    storage_token,
                                    counters,
                                    &mut carrier_custody,
                                    control,
                                    error,
                                ));
                            }

                            let immutable = sample_filesystem_fault_v1(
                                control,
                                FsCasFilesystemBoundaryV1::PermissionChange,
                            )
                            .and_then(|()| set_read_only(&carrier_path));
                            if let Err(error) = immutable {
                                carrier_rollback_attempted = true;
                                return Err(self.rollback_unpublished_carrier_preserving_error_v1(
                                    &carrier_path,
                                    validated,
                                    storage_token,
                                    counters,
                                    &mut carrier_custody,
                                    control,
                                    error,
                                ));
                            }
                            if let Err(error) =
                                sample_control(control, FsCasBoundaryV1::AfterCarrierMadeImmutable)
                            {
                                carrier_rollback_attempted = true;
                                return Err(self.rollback_unpublished_carrier_preserving_error_v1(
                                    &carrier_path,
                                    validated,
                                    storage_token,
                                    counters,
                                    &mut carrier_custody,
                                    control,
                                    error,
                                ));
                            }

                            let alias_cleanup = sample_filesystem_fault_v1(
                                control,
                                FsCasFilesystemBoundaryV1::CarrierAliasUnlink,
                            )
                            .and_then(|_| {
                                fs::remove_file(&prepared.path)
                                    .map_err(|error| map_required_filesystem_write_error_v1(&error))
                            });
                            if let Err(original) = alias_cleanup {
                                carrier_rollback_attempted = true;
                                let original = self
                                    .rollback_unpublished_carrier_preserving_error_v1(
                                        &carrier_path,
                                        validated,
                                        storage_token,
                                        counters,
                                        &mut carrier_custody,
                                        control,
                                        original,
                                    );
                                if original.has_cleanup_or_invalidation_dominance_v1() {
                                    return Err(original);
                                }
                                // The carrier link was visible, but removing its
                                // private preparation alias failed. Do not let an
                                // outer abort retry this cleanup into apparent
                                // success: retain the exact alias and fail closed.
                                let cleanup = self.cleanup_failure_controlled_v1(
                                    FsCasCleanupTargetV1::PrivatePack,
                                    control,
                                );
                                prepared.state = PrivatePackStateV1::CleanupFailed(cleanup);
                                return Err(original.dominated_by_v1(cleanup));
                            }
                            if let Err(first) = prepared.record_preparation_removed_v1() {
                                // The private alias is physically absent, but its
                                // root-owned preparation charge could not be
                                // released.  The installed carrier must remain in
                                // custody: rolling it back under an untrustworthy
                                // storage ledger would compound the accounting
                                // fault.  Commit the preflighted direct-residue
                                // observation without another fallible counter
                                // transition, then make the private-pack cleanup
                                // terminal stable before controlled invalidation.
                                counters.unreachable_installed_residue_bytes =
                                    retained_carrier_residue_bytes;
                                carrier_custody = CarrierPublicationCustodyV1::RetainedAndRecorded;

                                let provisional = first.dominated_by_v1(
                                    FsCasErrorV1::CleanupFailed(FsCasCleanupTargetV1::PrivatePack),
                                );
                                prepared.state = PrivatePackStateV1::CleanupFailed(provisional);
                                let cleanup = self.cleanup_failure_controlled_v1(
                                    FsCasCleanupTargetV1::PrivatePack,
                                    control,
                                );
                                let terminal = first.dominated_by_v1(cleanup);
                                prepared.state = PrivatePackStateV1::CleanupFailed(terminal);
                                return Err(terminal);
                            }
                            prepared.state = PrivatePackStateV1::Transferred;
                            Ok(())
                        })()
                    }));
                match carrier_terminal {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => return Err(error),
                    Err(payload) => {
                        let mut unwind_terminal = None;
                        // If unwind began inside an already-attempted rollback,
                        // never retry that cleanup. Otherwise attempt carrier
                        // rollback exactly once while its storage lease is live.
                        // A cleanup failure is a typed terminal, not permission
                        // to discard its result and resume only the initiating
                        // callback payload.
                        if carrier_custody == CarrierPublicationCustodyV1::InstalledUnreported
                            && !carrier_rollback_attempted
                        {
                            let rollback =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    self.rollback_unpublished_carrier(
                                        &carrier_path,
                                        validated,
                                        storage_token,
                                        counters,
                                        &mut carrier_custody,
                                        control,
                                    )
                                }));
                            match rollback {
                                Ok(Ok(())) => {}
                                Ok(Err(error)) => unwind_terminal = Some(error),
                                Err(_) => {
                                    // The cleanup callback unwound before it
                                    // could return a typed result. Transfer the
                                    // already-visible carrier to exact retained
                                    // custody using the preflighted observation,
                                    // then perform one controlled invalidation.
                                    if carrier_custody
                                        == CarrierPublicationCustodyV1::InstalledUnreported
                                    {
                                        counters.unreachable_installed_residue_bytes =
                                            retained_carrier_residue_bytes;
                                        carrier_custody =
                                            CarrierPublicationCustodyV1::RetainedAndRecorded;
                                    }
                                    let cleanup = self.cleanup_failure_after_unwind_v1(
                                        FsCasCleanupTargetV1::Carrier,
                                        control,
                                    );
                                    unwind_terminal = Some(cleanup);
                                }
                            }
                        }
                        if carrier_rollback_attempted && unwind_terminal.is_none() {
                            // An unwind from an already-entered rollback can
                            // happen after physical removal but before its
                            // controlled cleanup terminal is returned. Preserve
                            // that Carrier boundary even when no visible carrier
                            // remains to transfer into retained custody.
                            unwind_terminal = Some(self.cleanup_failure_after_unwind_v1(
                                FsCasCleanupTargetV1::Carrier,
                                control,
                            ));
                        }
                        if carrier_custody == CarrierPublicationCustodyV1::InstalledUnreported {
                            counters.unreachable_installed_residue_bytes =
                                retained_carrier_residue_bytes;
                            carrier_custody = CarrierPublicationCustodyV1::RetainedAndRecorded;
                            let cleanup = self.cleanup_failure_controlled_v1(
                                FsCasCleanupTargetV1::Carrier,
                                control,
                            );
                            unwind_terminal = Some(cleanup);
                        }

                        // The carrier destination and private alias are distinct
                        // owned cleanup targets. Attempt the latter even if
                        // carrier rollback retained finite residue. Its stable
                        // cleanup terminal survives an unwind and is merged only
                        // after both owned targets have been attempted once.
                        prepared.abort_private();
                        let private_cleanup =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                prepared.cleanup_controlled_v1(control)
                            }));
                        let private_terminal = match private_cleanup {
                            Ok(Ok(())) => None,
                            Ok(Err(error)) => Some(error),
                            Err(_) => prepared.retained_cleanup_terminal_v1().or(Some(
                                FsCasErrorV1::CleanupFailed(FsCasCleanupTargetV1::PrivatePack),
                            )),
                        };
                        if let Some(later) = private_terminal {
                            unwind_terminal = Some(match unwind_terminal {
                                Some(first) if first.has_invalidation_dominance_v1() => first,
                                Some(first) => first.dominated_by_v1(later),
                                None => later,
                            });
                        }
                        if let Some(terminal) = unwind_terminal {
                            return Err(terminal);
                        }
                        std::panic::resume_unwind(payload)
                    }
                }

                if let Err(error) = self.install_object_locators(
                    &carrier_path,
                    validated,
                    metadata,
                    counters,
                    scratch,
                    transaction,
                    storage_token,
                    &mut locator_custody,
                    control,
                ) {
                    if matches!(
                        error.dominant_cause_v1(),
                        FsCasFailureCauseV1::InvalidationFailed
                            | FsCasFailureCauseV1::CleanupFailed(
                                FsCasCleanupTargetV1::PublishedMarkerAlias
                            )
                    ) {
                        // These states can follow a visible locator whose alias
                        // cleanup failed. Never roll the carrier or earlier
                        // locators back below that visibility transition.
                        return Err(self.retain_visible_locator_dependencies_after_terminal_v1(
                            validated,
                            counters,
                            &mut carrier_custody,
                            &mut locator_custody,
                            control,
                            error,
                        ));
                    }
                    return Err(self.rollback_unpublished_admission_preserving_error_v1(
                        &carrier_path,
                        validated,
                        transaction,
                        storage_token,
                        counters,
                        &mut carrier_custody,
                        &mut locator_custody,
                        control,
                        error,
                    ));
                }

                if let Err(error) =
                    sample_control(control, FsCasBoundaryV1::BeforeCatalogPublication)
                {
                    return Err(self.rollback_unpublished_admission_preserving_error_v1(
                        &carrier_path,
                        validated,
                        transaction,
                        storage_token,
                        counters,
                        &mut carrier_custody,
                        &mut locator_custody,
                        control,
                        error,
                    ));
                }

                // All fallible counter arithmetic precedes catalog visibility. The
                // equivalent capacity checks above ran before carrier visibility;
                // rebuilding here retains intervening, directly observed FsCas
                // reads instead of restoring an early snapshot after publication.
                let mut published_counters = *counters;
                published_counters.record_fscas_catalog_operation()?;
                published_counters.record_pack_storage(validated.pack_len(), 0)?;

                let marker = encode_catalog_marker(validated);
                let publication = publish_small_marker_controlled(
                    &self.inner.root.join("preparation"),
                    "catalog",
                    &marker_path,
                    &marker,
                    Some(self),
                    storage_token,
                    Some(FsCasBoundaryV1::AfterCatalogMarkerLink),
                    None,
                    Some(&mut catalog_marker_custody),
                    control,
                );
                match publication {
                    Err(error) => {
                        return Err(self.rollback_unpublished_admission_preserving_error_v1(
                            &carrier_path,
                            validated,
                            transaction,
                            storage_token,
                            counters,
                            &mut carrier_custody,
                            &mut locator_custody,
                            control,
                            error,
                        ));
                    }
                    Ok(MarkerPublicationV1::IncumbentWithPreparationResidue(bytes, cleanup)) => {
                        let authenticated = decode_catalog_marker(bytes)
                            .map_err(|error| match error {
                                FsCasErrorV1::Integrity => FsCasErrorV1::MalformedOccupant,
                                other => other,
                            })
                            .and_then(|incumbent| {
                                classify_catalog_incumbent_v1(incumbent, validated)
                            });
                        let terminal = match authenticated {
                            Ok(()) => cleanup,
                            Err(error) => error.dominated_by_v1(cleanup),
                        };
                        return Err(self.rollback_unpublished_admission_preserving_error_v1(
                            &carrier_path,
                            validated,
                            transaction,
                            storage_token,
                            counters,
                            &mut carrier_custody,
                            &mut locator_custody,
                            control,
                            terminal,
                        ));
                    }
                    Ok(MarkerPublicationV1::VisibleWithPreparationResidue(first_error)) => {
                        // The catalog is authoritative now. Its carrier and
                        // locators must remain intact even though the alias could
                        // not be released.
                        *counters = published_counters;
                        return Err(self.retain_visible_catalog_dependencies_after_terminal_v1(
                            validated,
                            counters,
                            &mut carrier_custody,
                            &mut locator_custody,
                            &mut catalog_marker_custody,
                            control,
                            first_error,
                        ));
                    }
                    Ok(MarkerPublicationV1::VisibleTerminal(error)) => {
                        // Post-link unwind already performed the one explicit
                        // alias-cleanup and invalidation terminalization. The
                        // catalog is nevertheless authoritative, so retain its
                        // complete dependency chain without retrying either.
                        *counters = published_counters;
                        let _ = self.retain_visible_catalog_dependencies_v1(
                            validated,
                            counters,
                            &mut carrier_custody,
                            &mut locator_custody,
                            &mut catalog_marker_custody,
                            control,
                        );
                        return Err(error);
                    }
                    Ok(MarkerPublicationV1::VisibleClean) => {}
                    Ok(MarkerPublicationV1::IncumbentClean(bytes)) => {
                        let authenticated = decode_catalog_marker(bytes)
                            .map_err(|error| match error {
                                FsCasErrorV1::Integrity => FsCasErrorV1::MalformedOccupant,
                                other => other,
                            })
                            .and_then(|incumbent| {
                                classify_catalog_incumbent_v1(incumbent, validated)
                            });
                        if let Err(error) = authenticated {
                            return Err(self.rollback_unpublished_admission_preserving_error_v1(
                                &carrier_path,
                                validated,
                                transaction,
                                storage_token,
                                counters,
                                &mut carrier_custody,
                                &mut locator_custody,
                                control,
                                error,
                            ));
                        }
                    }
                }
                if let Err(error) =
                    sample_control(control, FsCasBoundaryV1::AfterCatalogPublication)
                {
                    *counters = published_counters;
                    return Err(self
                        .retain_visible_catalog_dependencies_after_control_terminal_v1(
                            validated,
                            counters,
                            &mut carrier_custody,
                            &mut locator_custody,
                            &mut catalog_marker_custody,
                            control,
                            error,
                        ));
                }
                *counters = published_counters;
                let installed_residue_bytes = validated
                    .pack_len()
                    .checked_add(locator_custody.take_live_bytes_v1()?)
                    .and_then(|bytes| {
                        bytes.checked_add(catalog_marker_custody.take_live_bytes_v1())
                    })
                    .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
                carrier_custody = CarrierPublicationCustodyV1::ReturnedInstalled;
                Ok(FsPackAdmissionV1 {
                    outcome: FsPackAdmissionOutcomeV1::Installed,
                    sealed: validated,
                    installed_residue_bytes,
                })
            })()
        }));
        drop(publication_guard.take());
        match terminal {
            Ok(Ok(admission)) => Ok(admission),
            Ok(Err(original)) => {
                prepared.abort_private();
                match prepared.cleanup_controlled_v1(control) {
                    Ok(()) => Err(original),
                    Err(cleanup) => Err(original.dominated_by_v1(cleanup)),
                }
            }
            Err(payload) => {
                // Once any carrier is visible, an admission callback unwind
                // cannot roll ownership back by guessing which later locator
                // or catalog transition completed. Attempt every dependency
                // custody observation without early return and retain its
                // first typed accounting failure for the terminal result.
                let visible_immutable = matches!(
                    carrier_custody,
                    CarrierPublicationCustodyV1::InstalledUnreported
                        | CarrierPublicationCustodyV1::RetainedAndRecorded
                );
                let mut unwind_terminal = declared_for_unwind.and_then(|declared| {
                    self.retain_visible_catalog_dependencies_v1(
                        declared,
                        counters,
                        &mut carrier_custody,
                        &mut locator_custody,
                        &mut catalog_marker_custody,
                        control,
                    )
                });

                // No carrier name is visible in this state, but the operation
                // may already own its prepublication immutable charge. Release
                // it explicitly before private-pack cleanup. If the accounting
                // rollback itself fails or unwinds, return a typed Carrier
                // cleanup/invalidation terminal instead of resuming the
                // initiating callback payload with an unbalanced equation.
                if prepublication_carrier_charge_held {
                    let charge_release = declared_for_unwind
                        .map(|declared| {
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                self.release_prepublication_carrier_charge_v1(
                                    storage_token,
                                    declared.pack_len(),
                                    control,
                                )
                            }))
                        })
                        .unwrap_or_else(|| Ok(Err(FsCasErrorV1::Integrity)));
                    let charge_terminal = match charge_release {
                        Ok(Ok(())) => None,
                        Ok(Err(error)) => Some(error),
                        Err(_) => Some(self.cleanup_failure_after_unwind_v1(
                            FsCasCleanupTargetV1::Carrier,
                            control,
                        )),
                    };
                    if let Some(later) = charge_terminal {
                        unwind_terminal = Some(match unwind_terminal {
                            Some(first) if first.has_invalidation_dominance_v1() => first,
                            Some(first) => first.dominated_by_v1(later),
                            None => later,
                        });
                    }
                }

                // The private alias is an independent owned cleanup target.
                // Attempt it exactly once even if immutable custody accounting
                // failed. A cleanup unwind retains its stable typed terminal
                // inside the private-pack owner before propagating its payload.
                prepared.abort_private();
                let cleanup = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    prepared.cleanup_controlled_v1(control)
                }));
                let cleanup_terminal = match cleanup {
                    Ok(Ok(())) => None,
                    Ok(Err(error)) => Some(error),
                    Err(_) => prepared.retained_cleanup_terminal_v1().or_else(|| {
                        Some(self.cleanup_failure_after_unwind_v1(
                            FsCasCleanupTargetV1::PrivatePack,
                            control,
                        ))
                    }),
                };
                if let Some(later) = cleanup_terminal {
                    unwind_terminal = Some(match unwind_terminal {
                        Some(first) if first.has_invalidation_dominance_v1() => first,
                        Some(first) => first.dominated_by_v1(later),
                        None => later,
                    });
                } else if visible_immutable {
                    // Clean private cleanup has not invalidated the root. A
                    // visible immutable escaped without a returned admission,
                    // so persist the fail-closed boundary exactly once. Catch
                    // a callback unwind here and finish persistence with the
                    // non-injecting backstop before choosing the terminal.
                    let invalidation =
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            self.invalidate_root_controlled_v1(control)
                        }));
                    let invalidation_error = match invalidation {
                        Ok(Ok(())) => None,
                        Ok(Err(error)) => Some(error),
                        Err(_) => {
                            let mut backstop = ContinueFsCasControlV1;
                            self.invalidate_root_controlled_v1(&mut backstop).err()
                        }
                    };
                    if let Some(later) = invalidation_error {
                        unwind_terminal = Some(match unwind_terminal {
                            Some(first) => first.dominated_by_v1(later),
                            None => later,
                        });
                    }
                }
                if let Some(terminal) = unwind_terminal {
                    return Err(terminal);
                }
                std::panic::resume_unwind(payload)
            }
        }
    }

    fn ensure_valid(&self) -> Result<(), FsCasErrorV1> {
        if self.inner.invalidated.load(Ordering::Acquire) {
            return Err(FsCasErrorV1::Invalidated);
        }
        #[cfg(test)]
        if let Some(error) = self
            .inner
            .invalidation_probe_failure
            .lock()
            .map_err(|_| FsCasErrorV1::SynchronizationPoisoned)?
            .take()
        {
            return Err(error);
        }
        if root_invalidation_barrier_present_v1(&self.inner.root.join(INVALIDATED_ROOT_NAME))? {
            return Err(FsCasErrorV1::Invalidated);
        }
        Ok(())
    }

    fn validate_storage_token_v1(
        &self,
        token: FsStorageOperationTokenV1,
    ) -> Result<(), FsCasErrorV1> {
        self.ensure_valid()?;
        self.inner.storage_admission.validate_token_v1(token)?;
        self.ensure_valid()
    }

    fn invalidate_root_controlled_v1<C>(&self, control: &mut C) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        self.inner.invalidated.store(true, Ordering::Release);
        // Change and read back the already allocated owner token first. If
        // this transition fails, the still-present active or malformed token
        // remains an allocation-free fail-closed barrier for future processes.
        // The descriptive directory is a secondary independently visible
        // marker, never the sole invalidation mechanism.
        let mut first_failure = None;
        let injected = control.inject_cleanup_failure(FsCasCleanupTargetV1::RootInvalidation);
        let token_persisted = if injected {
            false
        } else {
            let transition = (|| -> Result<bool, FsCasErrorV1> {
                let mut ownership = self
                    .inner
                    .ownership
                    .lock()
                    .map_err(|_| FsCasErrorV1::SynchronizationPoisoned)?;
                let file = ownership.as_mut().ok_or(FsCasErrorV1::Integrity)?;
                file.seek(SeekFrom::Start(8))
                    .map_err(|error| map_filesystem_write_error_v1(&error))?;
                write_all_controlled_v1(
                    file,
                    &[ROOT_OWNER_STATE_INVALIDATED],
                    FsCasFilesystemBoundaryV1::InvalidationWrite,
                    control,
                )?;
                flush_controlled_v1(file, FsCasFilesystemBoundaryV1::InvalidationFlush, control)?;
                file.seek(SeekFrom::Start(0))
                    .map_err(|error| map_filesystem_read_error_v1(&error))?;
                let mut observed = [0_u8; ROOT_OWNER_BYTES];
                file.read_exact(&mut observed)
                    .map_err(|error| map_filesystem_read_error_v1(&error))?;
                Ok(observed
                    == encode_root_owner(self.inner.generation, ROOT_OWNER_STATE_INVALIDATED))
            })();
            match transition {
                Ok(true) => true,
                Ok(false) => {
                    first_failure.get_or_insert(FsCasErrorV1::Integrity);
                    false
                }
                Err(error) => {
                    first_failure.get_or_insert(error);
                    false
                }
            }
        };
        let marker = self.inner.root.join(INVALIDATED_ROOT_NAME);
        let marker_persisted = match fs::symlink_metadata(&marker) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => true,
            Err(error) if error.kind() == ErrorKind::NotFound && !injected => {
                match sample_filesystem_fault_v1(
                    control,
                    FsCasFilesystemBoundaryV1::InvalidationMarkerCreate,
                )
                .and_then(|()| create_private_directory(&marker))
                {
                    Ok(()) => true,
                    Err(error) => {
                        first_failure.get_or_insert(error);
                        false
                    }
                }
            }
            // A synthetic root-invalidation refusal intentionally prevents
            // both persistence attempts. The marker's ordinary absence is
            // not an actual read failure and must not be fabricated as one.
            Err(error) if error.kind() == ErrorKind::NotFound => false,
            Ok(_) => {
                first_failure.get_or_insert(FsCasErrorV1::Integrity);
                false
            }
            Err(error) => {
                first_failure.get_or_insert(map_filesystem_read_error_v1(&error));
                false
            }
        };
        if token_persisted || marker_persisted {
            Ok(())
        } else {
            let invalidation = FsCasErrorV1::InvalidationFailed;
            Err(first_failure.map_or(invalidation, |error| error.dominated_by_v1(invalidation)))
        }
    }

    fn cleanup_failure_controlled_v1<C>(
        &self,
        target: FsCasCleanupTargetV1,
        control: &mut C,
    ) -> FsCasErrorV1
    where
        C: FsCasControlV1 + ?Sized,
    {
        let cleanup = FsCasErrorV1::CleanupFailed(target);
        match self.invalidate_root_controlled_v1(control) {
            Ok(()) => cleanup,
            Err(invalidation) => cleanup.dominated_by_v1(invalidation),
        }
    }

    /// Classify a cleanup callback unwind without allowing a second callback
    /// unwind to skip the persistence transition. The caller retains the
    /// original unwind payload separately and resumes it only if no typed
    /// cleanup terminal exists.
    fn cleanup_failure_after_unwind_v1<C>(
        &self,
        target: FsCasCleanupTargetV1,
        control: &mut C,
    ) -> FsCasErrorV1
    where
        C: FsCasControlV1 + ?Sized,
    {
        let cleanup = FsCasErrorV1::CleanupFailed(target);
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.cleanup_failure_controlled_v1(target, control)
        })) {
            Ok(terminal) => terminal,
            Err(_) => {
                let mut backstop = ContinueFsCasControlV1;
                match self.invalidate_root_controlled_v1(&mut backstop) {
                    Ok(()) => cleanup,
                    Err(error) => cleanup.dominated_by_v1(error),
                }
            }
        }
    }

    /// Finish a required invalidation after another callback has unwound
    /// without allowing a second callback unwind to skip the persistent
    /// transition. Success means either the controlled attempt or the
    /// callback-free backstop durably established the fail-closed barrier.
    fn invalidate_root_after_unwind_v1<C>(&self, control: &mut C) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.invalidate_root_controlled_v1(control)
        })) {
            Ok(terminal) => terminal,
            Err(_) => {
                let mut backstop = ContinueFsCasControlV1;
                self.invalidate_root_controlled_v1(&mut backstop)
            }
        }
    }

    /// Persist fail-closed invalidation on an ordinary return path without
    /// erasing the first typed cause. Only an actual invalidation double fault
    /// may become terminally dominant; this helper is intentionally not used
    /// by `Drop` or another result-less backstop.
    fn fail_closed_preserving_error_controlled_v1<C>(
        &self,
        first: FsCasErrorV1,
        control: &mut C,
    ) -> FsCasErrorV1
    where
        C: FsCasControlV1 + ?Sized,
    {
        match self.invalidate_root_controlled_v1(control) {
            Ok(()) => first,
            Err(invalidation) => first.dominated_by_v1(invalidation),
        }
    }

    /// Preserve a typed terminal that is already known before controlled
    /// invalidation begins. A fault-control unwind is not allowed to erase
    /// that cause or skip the callback-free persistent backstop. Successful
    /// persistence returns the first cause unchanged; only an actual
    /// invalidation double fault may become dominant.
    fn fail_closed_preserving_error_after_unwind_v1<C>(
        &self,
        first: FsCasErrorV1,
        control: &mut C,
    ) -> FsCasErrorV1
    where
        C: FsCasControlV1 + ?Sized,
    {
        match self.invalidate_root_after_unwind_v1(control) {
            Ok(()) => first,
            Err(invalidation) => first.dominated_by_v1(invalidation),
        }
    }

    /// Preserve the terminal that made one object locator non-rollbackable
    /// while taking dependency-safe custody of every visible locator and its
    /// installed carrier. A residue-observation failure cannot authorize
    /// unlinking the carrier, cannot skip the second custody transition, and
    /// cannot replace the earlier cleanup/invalidation cause.
    fn retain_visible_locator_dependencies_after_terminal_v1<C>(
        &self,
        sealed: SealedPackV1,
        counters: &mut OperationCountersV1,
        carrier_custody: &mut CarrierPublicationCustodyV1,
        locator_custody: &mut LocatorPublicationCustodyV1,
        control: &mut C,
        terminal: FsCasErrorV1,
    ) -> FsCasErrorV1
    where
        C: FsCasControlV1 + ?Sized,
    {
        #[cfg(test)]
        let locator_accounting = if control
            .inject_residue_accounting_failure(FsCasResidueAccountingBoundaryV1::ObjectLocator)
        {
            Err(FsCasErrorV1::Core(CoreError::IntegerOverflow))
        } else {
            locator_custody.retain_all_live_v1(counters)
        };
        #[cfg(not(test))]
        let locator_accounting = locator_custody.retain_all_live_v1(counters);

        let mut accounting_failed = locator_accounting.is_err();

        // Either a successfully retained locator or a still-live unclassified
        // locator can name this carrier. Attempt carrier custody even when the
        // locator observation failed; returning early here would strand the
        // dependency outside the operation's exact ownership state.
        if locator_custody.requires_carrier_retention_v1()
            && *carrier_custody == CarrierPublicationCustodyV1::InstalledUnreported
        {
            #[cfg(test)]
            let carrier_accounting = if control
                .inject_residue_accounting_failure(FsCasResidueAccountingBoundaryV1::Carrier)
            {
                Err(CoreError::IntegerOverflow)
            } else {
                counters.record_unreachable_installed_residue(sealed.pack_len())
            };
            #[cfg(not(test))]
            let carrier_accounting =
                counters.record_unreachable_installed_residue(sealed.pack_len());

            match carrier_accounting {
                Ok(()) => {
                    *carrier_custody = CarrierPublicationCustodyV1::RetainedAndRecorded;
                }
                Err(_) => accounting_failed = true,
            }
        }

        if accounting_failed {
            self.fail_closed_preserving_error_controlled_v1(terminal, control)
        } else {
            terminal
        }
    }

    /// A visible catalog marker is authoritative over the locator set and its
    /// carrier. Classify every dependency before returning even when one
    /// direct residue observation fails; a failed observation must not permit
    /// rollback below catalog visibility or prevent later custody attempts.
    /// The alias cleanup terminal is constructed before these transitions and
    /// exactly one controlled invalidation closes the fail-closed boundary.
    #[allow(clippy::too_many_arguments)]
    fn retain_visible_catalog_dependencies_after_terminal_v1<C>(
        &self,
        sealed: SealedPackV1,
        counters: &mut OperationCountersV1,
        carrier_custody: &mut CarrierPublicationCustodyV1,
        locator_custody: &mut LocatorPublicationCustodyV1,
        catalog_marker_custody: &mut ImmutableMarkerCustodyV1,
        control: &mut C,
        first_error: Option<FsCasErrorV1>,
    ) -> FsCasErrorV1
    where
        C: FsCasControlV1 + ?Sized,
    {
        let cleanup = FsCasErrorV1::CleanupFailed(FsCasCleanupTargetV1::PublishedMarkerAlias);
        let terminal = first_error.map_or(cleanup, |first| first.dominated_by_v1(cleanup));

        let _ = self.retain_visible_catalog_dependencies_v1(
            sealed,
            counters,
            carrier_custody,
            locator_custody,
            catalog_marker_custody,
            control,
        );

        // Visible alias residue always invalidates the root. Counter failures
        // remain secondary diagnostics and cannot replace the chronological
        // filesystem/cleanup terminal; only failed persistent invalidation may
        // become dominant.
        self.fail_closed_preserving_error_controlled_v1(terminal, control)
    }

    /// Retain every dependency below a visible catalog without returning
    /// early on direct-observation failure. The optional error is the first
    /// failed direct custody observation; later dependencies are still
    /// attempted so one counter fault cannot strand their ownership state.
    fn retain_visible_catalog_dependencies_v1<C>(
        &self,
        sealed: SealedPackV1,
        counters: &mut OperationCountersV1,
        carrier_custody: &mut CarrierPublicationCustodyV1,
        locator_custody: &mut LocatorPublicationCustodyV1,
        catalog_marker_custody: &mut ImmutableMarkerCustodyV1,
        control: &mut C,
    ) -> Option<FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        #[cfg(not(test))]
        let _ = &mut *control;

        #[cfg(test)]
        let catalog_accounting = if control
            .inject_residue_accounting_failure(FsCasResidueAccountingBoundaryV1::CatalogMarker)
        {
            Err(FsCasErrorV1::Core(CoreError::IntegerOverflow))
        } else {
            catalog_marker_custody.retain_live_v1(counters)
        };
        #[cfg(not(test))]
        let catalog_accounting = catalog_marker_custody.retain_live_v1(counters);
        let mut first_error = catalog_accounting.err();

        #[cfg(test)]
        let locator_accounting = if control
            .inject_residue_accounting_failure(FsCasResidueAccountingBoundaryV1::ObjectLocator)
        {
            Err(FsCasErrorV1::Core(CoreError::IntegerOverflow))
        } else {
            locator_custody.retain_all_live_v1(counters)
        };
        #[cfg(not(test))]
        let locator_accounting = locator_custody.retain_all_live_v1(counters);
        if let Err(error) = locator_accounting {
            first_error.get_or_insert(error);
        }

        // The visible catalog itself binds this carrier, so carrier custody is
        // required even when catalog or locator attribution remains live but
        // unclassified after a counter failure.
        if *carrier_custody == CarrierPublicationCustodyV1::InstalledUnreported {
            #[cfg(test)]
            let carrier_accounting = if control
                .inject_residue_accounting_failure(FsCasResidueAccountingBoundaryV1::Carrier)
            {
                Err(CoreError::IntegerOverflow)
            } else {
                counters.record_unreachable_installed_residue(sealed.pack_len())
            };
            #[cfg(not(test))]
            let carrier_accounting =
                counters.record_unreachable_installed_residue(sealed.pack_len());

            match carrier_accounting {
                Ok(()) => {
                    *carrier_custody = CarrierPublicationCustodyV1::RetainedAndRecorded;
                }
                Err(error) => {
                    first_error.get_or_insert(FsCasErrorV1::Core(error));
                }
            }
        }

        first_error
    }

    /// Preserve a cancellation/deadline observed after clean catalog
    /// visibility. A fully attributed terminal needs no invalidation; any
    /// dependency-attribution failure is fail-closed exactly once and cannot
    /// replace the initiating control cause.
    #[allow(clippy::too_many_arguments)]
    fn retain_visible_catalog_dependencies_after_control_terminal_v1<C>(
        &self,
        sealed: SealedPackV1,
        counters: &mut OperationCountersV1,
        carrier_custody: &mut CarrierPublicationCustodyV1,
        locator_custody: &mut LocatorPublicationCustodyV1,
        catalog_marker_custody: &mut ImmutableMarkerCustodyV1,
        control: &mut C,
        terminal: FsCasErrorV1,
    ) -> FsCasErrorV1
    where
        C: FsCasControlV1 + ?Sized,
    {
        if self
            .retain_visible_catalog_dependencies_v1(
                sealed,
                counters,
                carrier_custody,
                locator_custody,
                catalog_marker_custody,
                control,
            )
            .is_some()
        {
            self.fail_closed_preserving_error_controlled_v1(terminal, control)
        } else {
            terminal
        }
    }

    fn invalidate_root_backstop_v1(&self) {
        let mut control = ContinueFsCasControlV1;
        let _ = self.invalidate_root_controlled_v1(&mut control);
    }

    fn rollback_unpublished_carrier<C>(
        &self,
        path: &Path,
        sealed: SealedPackV1,
        storage_token: Option<FsStorageOperationTokenV1>,
        counters: &mut OperationCountersV1,
        custody: &mut CarrierPublicationCustodyV1,
        control: &mut C,
    ) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        let injected = control.inject_cleanup_failure(FsCasCleanupTargetV1::Carrier);
        let removal = if injected {
            Err(None)
        } else {
            match sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::CarrierUnlink) {
                Err(error) => Err(Some(error)),
                Ok(()) => match fs::remove_file(path) {
                    Ok(()) => Ok(()),
                    Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
                    Err(error) => Err(Some(map_required_filesystem_write_error_v1(&error))),
                },
            }
        };
        if let Err(first) = removal {
            if *custody == CarrierPublicationCustodyV1::InstalledUnreported {
                counters.record_unreachable_installed_residue(sealed.pack_len())?;
                *custody = CarrierPublicationCustodyV1::RetainedAndRecorded;
            }
            let cleanup =
                self.cleanup_failure_controlled_v1(FsCasCleanupTargetV1::Carrier, control);
            return Err(first.map_or(cleanup, |error| error.dominated_by_v1(cleanup)));
        }
        *custody = CarrierPublicationCustodyV1::Absent;
        if let Some(token) = storage_token {
            if let Err(accounting) =
                self.record_storage_immutable_remove_v1(token, sealed.pack_len(), 1)
            {
                // Physical cleanup succeeded, but its root-owned accounting
                // transition observed an untrustworthy synchronization state.
                // Classify this as a controlled cleanup terminal instead of
                // discarding an invalidation backstop and allowing the
                // initiating cancellation/deadline to masquerade as a clean
                // rollback. The accounting cell has already been reconciled
                // to the completed unlink, so terminal residue remains exact.
                let cleanup =
                    self.cleanup_failure_controlled_v1(FsCasCleanupTargetV1::Carrier, control);
                return Err(accounting.dominated_by_v1(cleanup));
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn rollback_unpublished_carrier_preserving_error_v1<C>(
        &self,
        path: &Path,
        sealed: SealedPackV1,
        storage_token: Option<FsStorageOperationTokenV1>,
        counters: &mut OperationCountersV1,
        custody: &mut CarrierPublicationCustodyV1,
        control: &mut C,
        original: FsCasErrorV1,
    ) -> FsCasErrorV1
    where
        C: FsCasControlV1 + ?Sized,
    {
        match self.rollback_unpublished_carrier(
            path,
            sealed,
            storage_token,
            counters,
            custody,
            control,
        ) {
            Ok(()) => original,
            Err(cleanup) if cleanup.has_cleanup_or_invalidation_dominance_v1() => {
                original.dominated_by_v1(cleanup)
            }
            // A non-cleanup rollback observation cannot erase the initiating
            // cancellation, deadline, validation, or directional filesystem
            // failure. Cleanup and invalidation are the only terminally
            // dominant classes in this bounded two-cause model.
            Err(_) => original,
        }
    }

    fn release_prepublication_carrier_charge_v1<C>(
        &self,
        storage_token: Option<FsStorageOperationTokenV1>,
        pack_len: u64,
        control: &mut C,
    ) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        let Some(token) = storage_token else {
            return Ok(());
        };
        match self.record_storage_immutable_remove_v1(token, pack_len, 1) {
            Ok(()) => Ok(()),
            Err(accounting) => {
                // No carrier name became visible, but failure to unwind the
                // root-owned pending charge makes the operation equation
                // untrustworthy. Promote it to an explicit Carrier cleanup
                // terminal and preserve a persistent-invalidation double
                // fault instead of discarding a best-effort backstop result.
                let cleanup =
                    self.cleanup_failure_controlled_v1(FsCasCleanupTargetV1::Carrier, control);
                Err(accounting.dominated_by_v1(cleanup))
            }
        }
    }

    fn release_prepublication_carrier_charge_preserving_error_v1<C>(
        &self,
        storage_token: Option<FsStorageOperationTokenV1>,
        pack_len: u64,
        control: &mut C,
        original: FsCasErrorV1,
    ) -> FsCasErrorV1
    where
        C: FsCasControlV1 + ?Sized,
    {
        match self.release_prepublication_carrier_charge_v1(storage_token, pack_len, control) {
            Ok(()) => original,
            Err(cleanup) => original.dominated_by_v1(cleanup),
        }
    }

    fn release_prepublication_marker_charge_preserving_error_v1<C>(
        &self,
        storage_token: FsStorageOperationTokenV1,
        marker_len: u64,
        control: &mut C,
        original: FsCasErrorV1,
    ) -> FsCasErrorV1
    where
        C: FsCasControlV1 + ?Sized,
    {
        match self.record_storage_immutable_remove_v1(storage_token, marker_len, 1) {
            Ok(()) => original,
            Err(accounting) => {
                // The destination was never visible, so this is not cleanup
                // of a published marker alias. The transaction still owns
                // its private marker and its pending root-ledger charge. A
                // failed charge rollback therefore becomes an explicit
                // prepublication cleanup terminal, while retaining the
                // directional hard-link error as the chronological cause.
                let cleanup = self
                    .cleanup_failure_controlled_v1(FsCasCleanupTargetV1::PreparationSpool, control);
                original.dominated_by_v1(accounting.dominated_by_v1(cleanup))
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn rollback_unpublished_admission<C>(
        &self,
        carrier: &Path,
        sealed: SealedPackV1,
        transaction: u64,
        storage_token: Option<FsStorageOperationTokenV1>,
        counters: &mut OperationCountersV1,
        carrier_custody: &mut CarrierPublicationCustodyV1,
        locator_custody: &mut LocatorPublicationCustodyV1,
        control: &mut C,
    ) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        let objects = self.inner.root.join("objects");
        let mut cleanup_failed = false;
        let mut requires_invalidation = false;
        let mut first_error = None;
        let mut cleanup_terminal = None;
        let mut first_cleanup_unwind = None;

        // The mutable pack-index spool is not cleanup authority: its sort,
        // rewind, and entry reads are independently fallible. Enumerate the
        // already-validated immutable carrier instead. Exact visibility count
        // remains in `locator_custody`, so a carrier-read or locator-auth error
        // can retain the unclassified suffix without guessing at namespace
        // state.
        match FilePackReadV1::open_occupant(carrier) {
            Ok(mut installed) => {
                for ordinal in 0..sealed.record_count() {
                    let entry = match read_validated_pack_index_entry_v1(
                        &mut installed,
                        sealed,
                        ordinal,
                        counters,
                    ) {
                        Ok(entry) => entry,
                        Err(error) => {
                            first_error.get_or_insert_with(|| {
                                installed
                                    .take_first_error_typed_v1()
                                    .unwrap_or(FsCasErrorV1::Core(error))
                            });
                            cleanup_failed = true;
                            break;
                        }
                    };
                    let path = objects.join(hex_typed_id(entry.id()));
                    match read_object_locator_if_present(&path, entry.id()) {
                        Ok(None) => continue,
                        Ok(Some(locator)) if locator.transaction() == transaction => {
                            let removal =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    let injected = control.inject_cleanup_failure(
                                        FsCasCleanupTargetV1::ObjectLocator,
                                    );
                                    if injected {
                                        Err(None)
                                    } else {
                                        match sample_filesystem_fault_v1(
                                            control,
                                            FsCasFilesystemBoundaryV1::LocatorUnlink,
                                        ) {
                                            Err(error) => Err(Some(error)),
                                            Ok(()) => match fs::remove_file(&path) {
                                                Ok(()) => Ok(()),
                                                Err(error)
                                                    if error.kind() == ErrorKind::NotFound =>
                                                {
                                                    Ok(())
                                                }
                                                Err(error) => Err(Some(
                                                    map_required_filesystem_write_error_v1(&error),
                                                )),
                                            },
                                        }
                                    }
                                }));
                            match removal {
                                Ok(Ok(())) => {
                                    if let Err(error) = locator_custody.mark_removed_v1() {
                                        first_error.get_or_insert(error);
                                        cleanup_failed = true;
                                    }
                                    if let Some(token) = storage_token {
                                        if let Err(error) = self.record_storage_immutable_remove_v1(
                                            token,
                                            PERSISTENT_LOCATOR_BYTES_U64_V1,
                                            1,
                                        ) {
                                            first_error.get_or_insert(error);
                                            cleanup_failed = true;
                                        }
                                    }
                                }
                                Ok(Err(first)) => {
                                    if let Some(error) = first {
                                        first_error.get_or_insert(error);
                                    }
                                    if let Err(error) = locator_custody.retain_one_v1(counters) {
                                        first_error.get_or_insert(error);
                                    }
                                    cleanup_failed = true;
                                }
                                Err(payload) => {
                                    #[cfg(test)]
                                    let retention = if control.inject_residue_accounting_failure(
                                        FsCasResidueAccountingBoundaryV1::ObjectLocator,
                                    ) {
                                        Err(FsCasErrorV1::Core(CoreError::IntegerOverflow))
                                    } else {
                                        locator_custody.retain_one_v1(counters)
                                    };
                                    #[cfg(not(test))]
                                    let retention = locator_custody.retain_one_v1(counters);
                                    if let Err(error) = retention {
                                        first_error.get_or_insert(error);
                                    }
                                    cleanup_failed = true;
                                    if first_cleanup_unwind.is_none() {
                                        first_cleanup_unwind =
                                            Some((FsCasCleanupTargetV1::ObjectLocator, payload));
                                    }
                                }
                            }
                        }
                        Ok(Some(_)) => {}
                        Err(error) => {
                            first_error.get_or_insert(error);
                            requires_invalidation = true;
                            // A malformed foreign incumbent is the operation's
                            // typed failure, not itself a failed cleanup.  If
                            // this transaction has an unauthenticated visible
                            // locator, `live_unclassified` below retains and
                            // charges it (and its carrier dependency) exactly.
                            // With no live transaction locator, the carrier is
                            // still safely removable and the original malformed
                            // occupant must retain error precedence.
                        }
                    }
                }
                if let Err(error) =
                    counters.record_fscas_read(installed.bytes_read, installed.read_calls)
                {
                    first_error.get_or_insert(FsCasErrorV1::Core(error));
                    cleanup_failed = true;
                }
            }
            Err(error) => {
                first_error.get_or_insert(error);
                cleanup_failed = true;
            }
        }

        if locator_custody.live_unclassified != 0 {
            if let Err(error) = locator_custody.retain_all_live_v1(counters) {
                first_error.get_or_insert(error);
            }
            cleanup_failed = true;
        }

        if locator_custody.requires_carrier_retention_v1() {
            // A retained, unauthenticated, or not-yet-attributed locator may
            // bind this carrier. A residue-counter failure cannot authorize
            // unlinking the carrier underneath the still-visible name.
            // Keep the dependency intact rather than unlinking it underneath
            // a visible name.
            if *carrier_custody == CarrierPublicationCustodyV1::InstalledUnreported {
                match counters.record_unreachable_installed_residue(sealed.pack_len()) {
                    Ok(()) => {
                        *carrier_custody = CarrierPublicationCustodyV1::RetainedAndRecorded;
                    }
                    Err(error) => {
                        first_error.get_or_insert(FsCasErrorV1::Core(error));
                    }
                }
            }
            cleanup_failed = true;
        } else {
            let carrier_terminal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.rollback_unpublished_carrier(
                    carrier,
                    sealed,
                    storage_token,
                    counters,
                    carrier_custody,
                    control,
                )
            }));
            match carrier_terminal {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    if error.has_cleanup_or_invalidation_dominance_v1() {
                        cleanup_terminal.get_or_insert(error);
                    } else {
                        first_error.get_or_insert(error);
                    }
                    cleanup_failed = true;
                }
                Err(payload) => {
                    if *carrier_custody == CarrierPublicationCustodyV1::InstalledUnreported {
                        #[cfg(test)]
                        let retention = if control.inject_residue_accounting_failure(
                            FsCasResidueAccountingBoundaryV1::Carrier,
                        ) {
                            Err(CoreError::IntegerOverflow)
                        } else {
                            counters.record_unreachable_installed_residue(sealed.pack_len())
                        };
                        #[cfg(not(test))]
                        let retention =
                            counters.record_unreachable_installed_residue(sealed.pack_len());
                        match retention {
                            Ok(()) => {
                                *carrier_custody = CarrierPublicationCustodyV1::RetainedAndRecorded;
                            }
                            Err(error) => {
                                first_error.get_or_insert(FsCasErrorV1::Core(error));
                            }
                        }
                    }
                    cleanup_failed = true;
                    if first_cleanup_unwind.is_none() {
                        first_cleanup_unwind = Some((FsCasCleanupTargetV1::Carrier, payload));
                    }
                }
            }
        }
        if let Some((target, payload)) = first_cleanup_unwind {
            // A cleanup callback unwind is an owned terminal failure, not a
            // caller callback that may be resumed after LayerFS returns. Keep
            // the payload alive until every remaining locator/carrier cleanup
            // target has been attempted once, then consume it and perform one
            // controlled fail-closed invalidation for the exact target.
            drop(payload);
            let unwind_cleanup = self.cleanup_failure_after_unwind_v1(target, control);
            cleanup_terminal = Some(match cleanup_terminal {
                Some(_) if unwind_cleanup.has_invalidation_dominance_v1() => unwind_cleanup,
                Some(later) => unwind_cleanup.dominated_by_v1(later),
                None => unwind_cleanup,
            });
        }
        if cleanup_failed {
            let cleanup = cleanup_terminal.unwrap_or_else(|| {
                self.cleanup_failure_controlled_v1(FsCasCleanupTargetV1::ObjectLocator, control)
            });
            return Err(first_error.map_or(cleanup, |error| error.dominated_by_v1(cleanup)));
        }
        if requires_invalidation {
            let error = first_error.unwrap_or(FsCasErrorV1::Integrity);
            return Err(match self.invalidate_root_controlled_v1(control) {
                Ok(()) => error,
                Err(invalidation) => error.dominated_by_v1(invalidation),
            });
        }
        first_error.map_or(Ok(()), Err)
    }

    #[allow(clippy::too_many_arguments)]
    fn rollback_unpublished_admission_preserving_error_v1<C>(
        &self,
        carrier: &Path,
        sealed: SealedPackV1,
        transaction: u64,
        storage_token: Option<FsStorageOperationTokenV1>,
        counters: &mut OperationCountersV1,
        carrier_custody: &mut CarrierPublicationCustodyV1,
        locator_custody: &mut LocatorPublicationCustodyV1,
        control: &mut C,
        original: FsCasErrorV1,
    ) -> FsCasErrorV1
    where
        C: FsCasControlV1 + ?Sized,
    {
        match self.rollback_unpublished_admission(
            carrier,
            sealed,
            transaction,
            storage_token,
            counters,
            carrier_custody,
            locator_custody,
            control,
        ) {
            Ok(()) => original,
            Err(cleanup @ FsCasErrorV1::CleanupFailed(_))
            | Err(cleanup @ FsCasErrorV1::InvalidationFailed)
            | Err(cleanup @ FsCasErrorV1::TerminalFailure { .. }) => {
                original.dominated_by_v1(cleanup)
            }
            // A cleanup/lifecycle failure is the first retained error when it
            // already occurred before rollback. Other rollback observations
            // (for example the malformed racing occupant that caused this
            // operation to fail) cannot erase that provenance.
            Err(_) => original,
        }
    }

    /// Open an occupied-object reader without exposing the filesystem reader
    /// implementation as a public storage SDK type.
    #[cfg(test)]
    pub fn occupied(&self) -> Result<impl OccupiedImmutableReadPortV1 + use<>, FsCasErrorV1> {
        self.occupied_private_v1()
    }

    #[cfg(test)]
    pub(crate) fn occupied_private_v1(&self) -> Result<FsCasOccupiedV1, FsCasErrorV1> {
        let mut control = ContinueFsCasControlV1;
        self.occupied_private_controlled_v1(&mut control)
    }

    #[cfg(test)]
    pub(crate) fn occupied_private_controlled_v1<C>(
        &self,
        control: &mut C,
    ) -> Result<FsCasOccupiedV1, FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        self.occupied_private_controlled_inner_v1(None, control)
    }

    #[cfg(feature = "c3-polymorphism")]
    pub(crate) fn occupied_private_borrowed_v1(
        &self,
        token: FsStorageOperationTokenV1,
    ) -> Result<FsCasOccupiedV1, FsCasErrorV1> {
        let mut control = ContinueFsCasControlV1;
        self.occupied_private_controlled_borrowed_v1(token, &mut control)
    }

    #[cfg(feature = "c3-polymorphism")]
    pub(crate) fn occupied_private_controlled_borrowed_v1<C>(
        &self,
        token: FsStorageOperationTokenV1,
        control: &mut C,
    ) -> Result<FsCasOccupiedV1, FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        self.occupied_private_controlled_inner_v1(Some(token), control)
    }

    fn occupied_private_controlled_inner_v1<C>(
        &self,
        storage_token: Option<FsStorageOperationTokenV1>,
        control: &mut C,
    ) -> Result<FsCasOccupiedV1, FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        if let Some(token) = storage_token {
            self.validate_storage_token_v1(token)?;
        }
        self.ensure_valid()?;
        let guard = self.lock_visibility_controlled_v1(control)?;
        let validity = self.ensure_valid();
        self.unlock_visibility_controlled_v1(guard, control);
        validity?;
        #[cfg(test)]
        let (bytes_read, read_calls) = NEXT_OCCUPIED_READ_OBSERVATION_FOR_TEST_V1
            .with(|seed| seed.take())
            .unwrap_or((0, 0));
        #[cfg(not(test))]
        let (bytes_read, read_calls) = (0, 0);
        #[cfg(test)]
        let payload_read_observation_for_test =
            NEXT_OCCUPIED_PAYLOAD_READ_OBSERVATION_FOR_TEST_V1.with(|seed| seed.take());
        Ok(FsCasOccupiedV1 {
            cas: self.clone(),
            current: None,
            previous: None,
            bytes_read,
            read_calls,
            first_error: None,
            validation_scratch: [0_u8; COMPARISON_WINDOW_BYTES],
            #[cfg(test)]
            unlocked_payload_read_hook: None,
            #[cfg(test)]
            payload_read_observation_for_test,
        })
    }

    /// Authenticate an already-visible complete-closure fence before a
    /// private read begins. The marker is bound to this FsCas generation and
    /// exact typed version-record identifier; a marker copied from another
    /// namespace or retained across invalidation is rejected.
    #[cfg(test)]
    pub(crate) fn validate_closure_for_read_v1(
        &self,
        version_record: PhysicalVersionRecordIdV1,
    ) -> Result<FsCasAcceptedClosureReadV1, FsCasErrorV1> {
        let mut control = ContinueFsCasControlV1;
        self.validate_closure_for_read_controlled_v1(version_record, &mut control)
    }

    #[cfg(test)]
    pub(crate) fn validate_closure_for_read_controlled_v1<C>(
        &self,
        version_record: PhysicalVersionRecordIdV1,
        control: &mut C,
    ) -> Result<FsCasAcceptedClosureReadV1, FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        self.validate_closure_for_read_controlled_inner_v1(None, version_record, control)
    }

    #[cfg(feature = "c3-polymorphism")]
    pub(crate) fn validate_closure_for_read_controlled_borrowed_v1<C>(
        &self,
        storage_token: FsStorageOperationTokenV1,
        version_record: PhysicalVersionRecordIdV1,
        control: &mut C,
    ) -> Result<FsCasAcceptedClosureReadV1, FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        self.validate_closure_for_read_controlled_inner_v1(
            Some(storage_token),
            version_record,
            control,
        )
    }

    fn validate_closure_for_read_controlled_inner_v1<C>(
        &self,
        storage_token: Option<FsStorageOperationTokenV1>,
        version_record: PhysicalVersionRecordIdV1,
        control: &mut C,
    ) -> Result<FsCasAcceptedClosureReadV1, FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        if let Some(token) = storage_token {
            self.validate_storage_token_v1(token)?;
        }
        self.ensure_valid()?;
        // This boundary is intentionally outside the visibility guard: a
        // controlled fault is a semantic read-path observation and must not
        // manufacture synchronization poison while no filesystem read has
        // begun.
        sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::ClosureMarkerRead)?;
        let guard = self.lock_visibility_controlled_v1(control)?;
        let result = (|| {
            self.ensure_valid()?;
            let typed = TypedPhysicalObjectIdV1::VersionRecord(version_record);
            let path = self.inner.root.join("closures").join(hex_typed_id(typed));
            let bytes =
                read_exact_regular_file::<CLOSURE_MARKER_BYTES>(&path).map_err(|error| {
                    if error == FsCasErrorV1::Integrity {
                        FsCasErrorV1::MalformedOccupant
                    } else {
                        error
                    }
                })?;
            decode_closure_marker_v1(bytes, version_record, self.inner.generation)
        })();
        self.unlock_visibility_controlled_v1(guard, control);
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn install_object_locators<M, C>(
        &self,
        candidate_path: &Path,
        sealed: SealedPackV1,
        metadata: &mut M,
        counters: &mut OperationCountersV1,
        scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
        transaction: u64,
        storage_token: Option<FsStorageOperationTokenV1>,
        locator_custody: &mut LocatorPublicationCustodyV1,
        control: &mut C,
    ) -> Result<(), FsCasErrorV1>
    where
        M: PackIndexSpoolV1 + ?Sized,
        C: FsCasControlV1 + ?Sized,
    {
        let objects = self.inner.root.join("objects");
        validate_required_root_directory(&objects)?;
        {
            let mut work_control = FsCasWorkControlBorrowV1(control);
            if let Err(error) = metadata.sort_by_key_controlled(&mut work_control, counters) {
                return Err(restore_pack_spool_error_v1(
                    metadata,
                    map_pack_spool_error_v1(error),
                ));
            }
        }
        if let Err(error) = metadata.rewind() {
            return Err(restore_pack_spool_error_v1(
                metadata,
                map_pack_spool_error_v1(error),
            ));
        }
        let mut candidate = FilePackReadV1::open(candidate_path)?;

        // Validate every incumbent before creating any locator for this pack.
        loop {
            let entry = match metadata.next() {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(error) => {
                    return Err(restore_pack_spool_error_v1(
                        metadata,
                        map_pack_spool_error_v1(error),
                    ));
                }
            };
            let path = objects.join(hex_typed_id(entry.id()));
            sample_control(control, FsCasBoundaryV1::BeforeObjectLocatorRead)?;
            sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::ObjectLocatorRead)?;
            let locator = read_object_locator_if_present(&path, entry.id())?;
            sample_control(control, FsCasBoundaryV1::AfterObjectLocatorRead)?;
            if let Some(locator) = locator {
                self.validate_and_compare_object_locator(
                    &mut candidate,
                    entry,
                    locator,
                    counters,
                    scratch,
                    control,
                )?;
            }
        }

        if let Err(error) = metadata.rewind() {
            return Err(restore_pack_spool_error_v1(
                metadata,
                map_pack_spool_error_v1(error),
            ));
        }
        loop {
            let entry = match metadata.next() {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(error) => {
                    return Err(restore_pack_spool_error_v1(
                        metadata,
                        map_pack_spool_error_v1(error),
                    ));
                }
            };
            let path = objects.join(hex_typed_id(entry.id()));
            sample_control(control, FsCasBoundaryV1::BeforeObjectLocatorPublication)?;
            let marker = encode_persistent_locator_v1(PersistentObjectLocatorV1::new(
                sealed,
                entry,
                transaction,
            ));
            let publication = publish_small_marker_controlled(
                &self.inner.root.join("preparation"),
                "object",
                &path,
                &marker,
                Some(self),
                storage_token,
                Some(FsCasBoundaryV1::AfterObjectLocatorMarkerLink),
                Some(&mut *locator_custody),
                None,
                control,
            )?;
            match publication {
                MarkerPublicationV1::VisibleClean => {
                    counters.record_locator_install()?;
                }
                MarkerPublicationV1::VisibleWithPreparationResidue(first_error) => {
                    // The locator now names this carrier. Invalidate the root
                    // and retain both objects instead of invoking unpublished
                    // rollback beneath a visible locator.
                    let cleanup = self.cleanup_failure_controlled_v1(
                        FsCasCleanupTargetV1::PublishedMarkerAlias,
                        control,
                    );
                    return Err(first_error.map_or(cleanup, |first| first.dominated_by_v1(cleanup)));
                }
                MarkerPublicationV1::VisibleTerminal(error) => {
                    // Locator custody was recorded at the successful link.
                    // The enclosing admission transaction retains the locator
                    // and carrier after this exact terminal is returned.
                    return Err(error);
                }
                MarkerPublicationV1::IncumbentWithPreparationResidue(bytes, cleanup) => {
                    let locator = decode_persistent_locator_v1(bytes, entry.id()).map_err(
                        |error| match error {
                            PersistentLocatorCodecErrorV1::Malformed => {
                                FsCasErrorV1::MalformedOccupant
                            }
                            PersistentLocatorCodecErrorV1::BindingMismatch => {
                                FsCasErrorV1::Integrity
                            }
                        },
                    );
                    let locator = match locator {
                        Ok(locator) => locator,
                        Err(error) => return Err(error.dominated_by_v1(cleanup)),
                    };
                    if let Err(error) = self.validate_and_compare_object_locator(
                        &mut candidate,
                        entry,
                        locator,
                        counters,
                        scratch,
                        control,
                    ) {
                        return Err(error.dominated_by_v1(cleanup));
                    }
                    return Err(cleanup);
                }
                MarkerPublicationV1::IncumbentClean(bytes) => {
                    let locator = decode_persistent_locator_v1(bytes, entry.id()).map_err(
                        |error| match error {
                            PersistentLocatorCodecErrorV1::Malformed => {
                                FsCasErrorV1::MalformedOccupant
                            }
                            PersistentLocatorCodecErrorV1::BindingMismatch => {
                                FsCasErrorV1::Integrity
                            }
                        },
                    )?;
                    self.validate_and_compare_object_locator(
                        &mut candidate,
                        entry,
                        locator,
                        counters,
                        scratch,
                        control,
                    )?;
                }
            }
            sample_control(control, FsCasBoundaryV1::AfterObjectLocatorPublication)?;
        }
        Ok(())
    }

    fn validate_and_compare_object_locator<C>(
        &self,
        candidate: &mut FilePackReadV1,
        candidate_entry: PackIndexEntryV1,
        locator: PersistentObjectLocatorV1,
        counters: &mut OperationCountersV1,
        scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
        control: &mut C,
    ) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        let pack_name = hex_id(locator.sealed().id().as_bytes());
        sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::CatalogMarkerRead)?;
        let catalog = read_catalog_marker(&self.inner.root.join("catalog").join(&pack_name))?;
        if catalog != locator.sealed() {
            // Both records decoded completely. Their authenticated carrier
            // bindings disagree; this is an integrity failure, not malformed
            // bytes in either occupant.
            return Err(FsCasErrorV1::Integrity);
        }
        sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::CarrierMetadataRead)?;
        let mut incumbent =
            FilePackReadV1::open_occupant(&self.inner.root.join("carriers").join(&pack_name))?;
        counters.observe_layerfs_open_file_handles(2);
        sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::CarrierIndexRead)?;
        let indexed = match locate_validated_pack_index_entry_v1(
            &mut incumbent,
            locator.sealed(),
            candidate_entry.id(),
            counters,
        ) {
            Ok(indexed) => indexed,
            Err(error) => {
                return Err(restore_pack_occupant_failure_v1(&mut incumbent, error));
            }
        }
        .ok_or(FsCasErrorV1::MissingOccupant)?;
        if indexed != locator.entry() {
            return Err(FsCasErrorV1::Integrity);
        }
        sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::CarrierObjectRead)?;
        let location = match validate_validated_pack_object_v1(
            &mut incumbent,
            locator.entry(),
            scratch,
            counters,
        ) {
            Ok(location) => location,
            Err(error) => {
                return Err(restore_pack_occupant_failure_v1(&mut incumbent, error));
            }
        };
        counters.record_fscas_read(incumbent.bytes_read, incumbent.read_calls)?;
        sample_control(control, FsCasBoundaryV1::AfterObjectIncumbentValidation)?;
        compare_complete_object_bytes(
            candidate,
            PackObjectLocationV1 {
                object_offset: candidate_entry
                    .absolute_offset()
                    .checked_add(4)
                    .ok_or(FsCasErrorV1::Integrity)?,
                object_len: u64::from(candidate_entry.object_len()),
            },
            &mut incumbent,
            location,
            scratch,
            counters,
            control,
        )?;
        counters.record_locator_equal_incumbent_reuse()?;
        Ok(())
    }

    fn validate_existing_object_locators<M, C>(
        &self,
        sealed: SealedPackV1,
        metadata: &mut M,
        counters: &mut OperationCountersV1,
        control: &mut C,
    ) -> Result<(), FsCasErrorV1>
    where
        M: PackIndexSpoolV1 + ?Sized,
        C: FsCasControlV1 + ?Sized,
    {
        let objects = self.inner.root.join("objects");
        validate_required_root_directory(&objects)?;
        {
            let mut work_control = FsCasWorkControlBorrowV1(control);
            if let Err(error) = metadata.sort_by_key_controlled(&mut work_control, counters) {
                return Err(restore_pack_spool_error_v1(
                    metadata,
                    map_pack_spool_error_v1(error),
                ));
            }
        }
        if let Err(error) = metadata.rewind() {
            return Err(restore_pack_spool_error_v1(
                metadata,
                map_pack_spool_error_v1(error),
            ));
        }
        loop {
            let entry = match metadata.next() {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(error) => {
                    return Err(restore_pack_spool_error_v1(
                        metadata,
                        map_pack_spool_error_v1(error),
                    ));
                }
            };
            let path = objects.join(hex_typed_id(entry.id()));
            sample_control(control, FsCasBoundaryV1::BeforeObjectLocatorRead)?;
            sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::ObjectLocatorRead)?;
            let locator = read_object_locator_if_present(&path, entry.id())?
                .ok_or(FsCasErrorV1::MissingOccupant)?;
            sample_control(control, FsCasBoundaryV1::AfterObjectLocatorRead)?;
            if !locator.matches_binding(sealed, entry) {
                return Err(FsCasErrorV1::Integrity);
            }
            // The complete carrier and candidate bytes were validated and
            // compared immediately above. Matching the locator to that exact
            // seal and canonical entry therefore validates the incumbent
            // object without reopening or comparing the same bytes twice.
            sample_control(control, FsCasBoundaryV1::AfterObjectIncumbentValidation)?;
            counters.record_locator_equal_incumbent_reuse()?;
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn begin_closure_operation(&self) -> Result<FsClosureOperationV1, FsCasErrorV1> {
        self.begin_closure_operation_inner_v1(None)
    }

    #[cfg(feature = "c3-polymorphism")]
    pub(crate) fn begin_closure_operation_borrowed_v1(
        &self,
        storage_token: FsStorageOperationTokenV1,
    ) -> Result<FsClosureOperationV1, FsCasErrorV1> {
        self.begin_closure_operation_inner_v1(Some(storage_token))
    }

    fn begin_closure_operation_inner_v1(
        &self,
        storage_token: Option<FsStorageOperationTokenV1>,
    ) -> Result<FsClosureOperationV1, FsCasErrorV1> {
        if let Some(token) = storage_token {
            self.validate_storage_token_v1(token)?;
        }
        self.ensure_valid()?;
        let _guard = self.lock_visibility_v1()?;
        self.ensure_valid()?;
        let nonce = NEXT_CLOSURE_OPERATION
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .map_err(|_| FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        Ok(FsClosureOperationV1 {
            owner: self.clone(),
            generation: self.inner.generation,
            nonce,
            storage_token,
            marker_custody: ImmutableMarkerCustodyV1::default(),
            admission_started: false,
            admitted: false,
            consumed: false,
        })
    }

    pub(crate) fn retain_closure_marker_residue_v1(
        operation: &mut FsClosureOperationV1,
        counters: &mut OperationCountersV1,
    ) -> Result<bool, FsCasErrorV1> {
        operation.marker_custody.retain_live_v1(counters)
    }

    pub(crate) fn take_closure_marker_residue_bytes_v1(
        operation: &mut FsClosureOperationV1,
    ) -> u64 {
        operation.marker_custody.take_live_bytes_v1()
    }

    pub(crate) fn invalidate_closure_operation_controlled_v1<C>(
        operation: &FsClosureOperationV1,
        control: &mut C,
    ) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        match operation.owner.ensure_valid() {
            Err(FsCasErrorV1::Invalidated) => Ok(()),
            Ok(()) => operation.owner.invalidate_root_controlled_v1(control),
            Err(first) => match operation.owner.invalidate_root_controlled_v1(control) {
                Ok(()) => Err(first),
                Err(invalidation) => Err(first.dominated_by_v1(invalidation)),
            },
        }
    }

    pub(crate) fn invalidate_closure_operation_backstop_v1(
        operation: &FsClosureOperationV1,
    ) -> Result<(), FsCasErrorV1> {
        let mut control = ContinueFsCasControlV1;
        operation.owner.invalidate_root_controlled_v1(&mut control)
    }

    /// Run the complete closure validator and mint an opaque capability only
    /// after its FsCas-backed fence becomes visible. The supplied operation is
    /// one-shot even when validation fails, preventing a hidden retry path.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub fn admit_complete_closure<C>(
        &self,
        operation: &mut FsClosureOperationV1,
        closure: &mut C,
        expected_version_record: TypedPhysicalObjectIdV1,
        ledger: &ResourceLedgerV1,
        counters: &mut OperationCountersV1,
        buffers: AdmissionBuffersV1<'_>,
    ) -> CoreResult<(AdmittedClosureV1, CompleteValidatedClosureV1)>
    where
        C: CompleteImmutableClosureReadPortV1 + ?Sized,
    {
        self.ensure_valid().map_err(|_| CoreError::SinkRefused)?;
        if !Arc::ptr_eq(&operation.owner.inner, &self.inner)
            || operation.owner.ensure_valid().is_err()
            || operation.generation != self.inner.generation
            || operation.storage_token.is_some()
            || operation.admission_started
        {
            return Err(CoreError::SinkRefused);
        }
        operation.admission_started = true;
        let mut residue_capacity = *counters;
        residue_capacity
            .record_unreachable_installed_residue(CLOSURE_MARKER_BYTES as u64)
            .map_err(|_| CoreError::IntegerOverflow)?;
        let mut occupied = self.occupied().map_err(|_| CoreError::SinkRefused)?;
        let mut control = ContinueFsCasControlV1;
        let mut fence = FsClosureFenceV1::new(
            self.clone(),
            operation.nonce,
            None,
            &mut operation.marker_custody,
            &mut control,
            false,
        );
        let admitted = admit_complete_immutable_v1(
            closure,
            expected_version_record,
            &mut occupied,
            &mut fence,
            ledger,
            counters,
            buffers,
        )?;
        let capability = fence.complete.take().ok_or(CoreError::SinkRefused)?;
        operation.admitted = true;
        Ok((admitted, capability))
    }

    #[cfg(feature = "c3-polymorphism")]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn admit_complete_closure_borrowed_v1<C, K>(
        &self,
        operation: &mut FsClosureOperationV1,
        closure: &mut C,
        storage_token: FsStorageOperationTokenV1,
        expected_version_record: TypedPhysicalObjectIdV1,
        reservation: &OperationReservationV1<'_>,
        counters: &mut OperationCountersV1,
        buffers: AdmissionBuffersV1<'_>,
        algorithm: crate::cdc::C3CdcAlgorithmV1,
        control: &mut K,
    ) -> Result<(AdmittedClosureV1, CompleteValidatedClosureV1), FsClosureAdmissionErrorV1>
    where
        C: CompleteImmutableClosureReadPortV1 + ?Sized,
        K: FsCasControlV1 + ?Sized,
    {
        self.ensure_valid()
            .map_err(FsClosureAdmissionErrorV1::FsCas)?;
        self.validate_storage_token_v1(storage_token)
            .map_err(FsClosureAdmissionErrorV1::FsCas)?;
        if !Arc::ptr_eq(&operation.owner.inner, &self.inner)
            || operation.generation != self.inner.generation
            || operation.storage_token != Some(storage_token)
            || operation.admission_started
        {
            return Err(FsClosureAdmissionErrorV1::FsCas(FsCasErrorV1::Integrity));
        }
        // Pointer and generation mismatches are authority misuse. Once the
        // operation is proven to belong to this owner, a concurrent root
        // invalidation remains its own typed terminal cause rather than being
        // flattened into an authority-integrity failure.
        operation
            .owner
            .ensure_valid()
            .map_err(FsClosureAdmissionErrorV1::FsCas)?;
        operation.admission_started = true;
        let mut residue_capacity = *counters;
        residue_capacity
            .record_unreachable_installed_residue(CLOSURE_MARKER_BYTES as u64)
            .map_err(FsClosureAdmissionErrorV1::Core)?;
        let mut occupied = self
            .occupied_private_borrowed_v1(storage_token)
            .map_err(FsClosureAdmissionErrorV1::FsCas)?;
        let mut fence = FsClosureFenceV1::new(
            self.clone(),
            operation.nonce,
            Some(storage_token),
            &mut operation.marker_custody,
            control,
            true,
        );
        let admitted = match crate::cas::admission::admit_complete_immutable_borrowed_v1(
            closure,
            expected_version_record,
            &mut occupied,
            &mut fence,
            reservation,
            counters,
            buffers,
            algorithm,
        ) {
            Ok(admitted) => admitted,
            Err(error) => {
                if let Some(error) = fence
                    .first_error
                    .or_else(|| occupied.first_error_typed_v1())
                {
                    return Err(FsClosureAdmissionErrorV1::FsCas(error));
                }
                return Err(FsClosureAdmissionErrorV1::Core(error));
            }
        };
        let capability = fence
            .complete
            .take()
            .ok_or(FsClosureAdmissionErrorV1::FsCas(FsCasErrorV1::Integrity))?;
        operation.admitted = true;
        Ok((admitted, capability))
    }

    /// Consume a validated capability at the synchronous closure handoff.
    /// This is not an authority/publication decision and returns no Workspace
    /// Version; it only checks storage generation, operation, transcript, and
    /// one-shot use against the already-visible local closure fence.
    pub fn consume_validated_closure_for_handoff(
        &self,
        operation: &mut FsClosureOperationV1,
        capability: &mut CompleteValidatedClosureV1,
    ) -> Result<(), FsCasErrorV1> {
        if let Some(token) = operation.storage_token {
            self.validate_storage_token_v1(token)?;
        }
        self.ensure_valid()?;
        let _guard = self.lock_visibility_v1()?;
        self.ensure_valid()?;
        if operation.generation != self.inner.generation
            || !Arc::ptr_eq(&operation.owner.inner, &self.inner)
            || !Arc::ptr_eq(&capability.owner.inner, &self.inner)
            || capability.generation != self.inner.generation
            || operation.nonce != capability.operation_nonce
            || !operation.admitted
            || operation.consumed
            || capability.consumed
        {
            return Err(FsCasErrorV1::Integrity);
        }
        // The pointer checks above establish that these are aliases of this
        // owner. Preserve a same-owner invalidation that races the earlier
        // validation as `Invalidated`; only mismatched authority is
        // `Integrity`.
        operation.owner.ensure_valid()?;
        capability.owner.ensure_valid()?;
        let expected = encode_closure_marker(
            capability.version_record,
            capability.object_count,
            capability.generation,
            capability.transcript,
        );
        let path = self
            .inner
            .root
            .join("closures")
            .join(hex_typed_id(capability.version_record));
        let incumbent =
            read_exact_regular_file::<CLOSURE_MARKER_BYTES>(&path).map_err(|error| {
                if error == FsCasErrorV1::Integrity {
                    FsCasErrorV1::MalformedOccupant
                } else {
                    error
                }
            })?;
        decode_closure_marker_v1(
            incumbent,
            match capability.version_record {
                TypedPhysicalObjectIdV1::VersionRecord(version_record) => version_record,
                _ => return Err(FsCasErrorV1::Integrity),
            },
            capability.generation,
        )?;
        if incumbent != expected {
            return Err(FsCasErrorV1::Integrity);
        }
        operation.consumed = true;
        capability.consumed = true;
        Ok(())
    }

    #[cfg(any(test, feature = "c3-polymorphism"))]
    #[allow(clippy::too_many_arguments)]
    fn admit_against_incumbent<M, C>(
        &self,
        prepared: &mut FsPrivatePackV1,
        metadata: &mut M,
        authority: PackAdmissionAuthorityV1<'_, '_>,
        counters: &mut OperationCountersV1,
        scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
        candidate: SealedPackV1,
        carrier_path: &Path,
        marker_path: &Path,
        control: &mut C,
    ) -> Result<FsPackAdmissionV1, FsCasErrorV1>
    where
        M: PackIndexSpoolV1 + ?Sized,
        C: FsCasControlV1 + ?Sized,
    {
        sample_control(control, FsCasBoundaryV1::BeforeIncumbentMarkerRead)?;
        // This is the authoritative catalog read for the no-replace
        // incumbent path. It is a semantic storage-path event, rather than
        // a native syscall counter, so qualification can prove directional
        // read failures without changing publication or retry behavior.
        sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::CatalogMarkerRead)?;
        let marker = read_catalog_marker(marker_path)?;
        sample_control(control, FsCasBoundaryV1::AfterIncumbentMarkerRead)?;
        classify_catalog_incumbent_v1(marker, candidate)?;
        sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::CarrierMetadataRead)?;
        let mut incumbent = FilePackReadV1::open_occupant(carrier_path)?;
        counters.observe_layerfs_open_file_handles(2);
        let validated = match validate_pack_for_operation_v1(
            &mut incumbent,
            metadata,
            scratch,
            marker.record_count(),
            authority,
            counters,
            control,
        ) {
            Ok(validated) => validated,
            Err(error) => {
                if let Some(storage) = incumbent.take_first_error_typed_v1() {
                    return Err(storage);
                }
                return Err(match error {
                    FsCasErrorV1::Core(CoreError::PackInvalid) => FsCasErrorV1::MalformedOccupant,
                    other => other,
                });
            }
        };
        counters.record_fscas_read(incumbent.bytes_read, incumbent.read_calls)?;
        if validated != marker {
            return Err(FsCasErrorV1::Integrity);
        }
        sample_control(control, FsCasBoundaryV1::AfterIncumbentValidation)?;
        compare_complete_pack_bytes(
            prepared,
            &mut incumbent,
            candidate.pack_len(),
            scratch,
            counters,
            control,
        )?;
        self.validate_existing_object_locators(candidate, metadata, counters, control)?;
        prepared.abort_private();
        prepared.cleanup_controlled_v1(control)?;

        // Incumbent pack/object validation and complete byte comparison run
        // without monopolizing the root visibility mutex. Re-enter the fence
        // before returning reuse and authenticate the immutable catalog
        // snapshot again so invalidation or namespace drift fails closed.
        let _guard = self.lock_visibility_controlled_v1(control)?;
        self.ensure_valid()?;
        sample_filesystem_fault_v1(
            control,
            FsCasFilesystemBoundaryV1::CatalogMarkerRevalidationRead,
        )?;
        classify_catalog_incumbent_v1(read_catalog_marker(marker_path)?, candidate)?;
        counters.record_fscas_catalog_operation()?;
        Ok(FsPackAdmissionV1 {
            outcome: FsPackAdmissionOutcomeV1::ExistingComplete,
            sealed: candidate,
            installed_residue_bytes: 0,
        })
    }
}

/// Validation and storage-accounting authority for one pack admission. The
/// production form can only borrow the reservation and storage token minted
/// by the root operation. Independent ledger admission exists solely for the
/// inherited unit-test compatibility wall and is absent from production.
#[cfg(any(test, feature = "c3-polymorphism"))]
#[derive(Clone, Copy)]
enum PackAdmissionAuthorityV1<'operation, 'ledger> {
    #[cfg(test)]
    Independent(&'operation ResourceLedgerV1),
    Borrowed {
        reservation: &'operation OperationReservationV1<'ledger>,
        storage_token: FsStorageOperationTokenV1,
    },
}

#[cfg(any(test, feature = "c3-polymorphism"))]
impl PackAdmissionAuthorityV1<'_, '_> {
    fn storage_token_v1(self) -> Option<FsStorageOperationTokenV1> {
        match self {
            #[cfg(test)]
            Self::Independent(_) => None,
            Self::Borrowed { storage_token, .. } => Some(storage_token),
        }
    }
}

#[cfg(any(test, feature = "c3-polymorphism"))]
#[allow(clippy::too_many_arguments)]
fn validate_pack_for_operation_v1<P, M, C>(
    pack: &mut P,
    metadata: &mut M,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    maximum_entries: u32,
    authority: PackAdmissionAuthorityV1<'_, '_>,
    counters: &mut OperationCountersV1,
    control: &mut C,
) -> Result<SealedPackV1, FsCasErrorV1>
where
    P: PackReadPortV1 + ?Sized,
    M: PackIndexSpoolV1 + ?Sized,
    C: FsCasControlV1 + ?Sized,
{
    let result = match authority {
        #[cfg(test)]
        PackAdmissionAuthorityV1::Independent(ledger) => {
            validate_pack_v1(pack, metadata, scratch, maximum_entries, ledger, counters)
        }
        PackAdmissionAuthorityV1::Borrowed { reservation, .. } => {
            let mut work_control = FsCasWorkControlBorrowV1(control);
            crate::pack::validate_pack_borrowed_v1(
                pack,
                metadata,
                scratch,
                maximum_entries,
                reservation,
                counters,
                &mut work_control,
            )
        }
    };
    result.map_err(|error| restore_pack_spool_error_v1(metadata, FsCasErrorV1::Core(error)))
}

struct FsCasWorkControlBorrowV1<'control, C: ?Sized>(&'control mut C);

impl<C> crate::limits::OperationWorkControlV1 for FsCasWorkControlBorrowV1<'_, C>
where
    C: FsCasControlV1 + ?Sized,
{
    fn cancellation_requested_v1(&mut self) -> bool {
        self.0.cancellation_requested()
    }

    fn deadline_exceeded_v1(&mut self) -> bool {
        self.0.deadline_exceeded()
    }
}

struct NeverStopWorkControlV1;

impl crate::limits::OperationWorkControlV1 for NeverStopWorkControlV1 {
    fn cancellation_requested_v1(&mut self) -> bool {
        false
    }

    fn deadline_exceeded_v1(&mut self) -> bool {
        false
    }
}

const fn map_pack_spool_error_v1(error: PackPortErrorV1) -> FsCasErrorV1 {
    match error {
        PackPortErrorV1::Failure => FsCasErrorV1::Integrity,
        PackPortErrorV1::Cancelled => FsCasErrorV1::Core(CoreError::Cancelled),
        PackPortErrorV1::Deadline => FsCasErrorV1::Core(CoreError::Deadline),
        PackPortErrorV1::WorkExhausted => FsCasErrorV1::Core(CoreError::ResourceRefused),
    }
}

fn restore_pack_spool_error_v1<M>(metadata: &mut M, fallback: FsCasErrorV1) -> FsCasErrorV1
where
    M: PackIndexSpoolV1 + ?Sized,
{
    #[cfg(any(test, feature = "c3-polymorphism"))]
    {
        metadata.take_storage_error_typed_v1().unwrap_or(fallback)
    }
    #[cfg(not(any(test, feature = "c3-polymorphism")))]
    {
        let _ = metadata;
        fallback
    }
}

pub struct FsPrivatePackV1 {
    owner: FsCasV1,
    path: PathBuf,
    state: PrivatePackStateV1,
    // `PackPortErrorV1` deliberately carries no storage provenance. Preserve
    // the first concrete filesystem/CAS failure here so the operation adapter
    // can promote it after the callback boundary instead of reporting a
    // generic source/sink refusal.
    first_error: Option<FsCasErrorV1>,
    storage_token: Option<FsStorageOperationTokenV1>,
    accounted_len: u64,
    preparation_accounted: bool,
}

/// Bounded ownership transfer used only when construction of an operation
/// spool has already produced a typed failure and explicit cleanup then
/// unwinds before the spool can be returned to the preparation coordinator.
///
/// Rust unwind payloads cannot otherwise carry the first typed storage cause.
/// Keeping both values in one private carrier lets the coordinator finish all
/// earlier preparation targets, terminalize the operation, and return the
/// classified error without retrying the partially constructed spool.
#[cfg(feature = "c3-polymorphism")]
pub(crate) struct FsOperationSpoolConstructionUnwindV1 {
    terminal: FsCasErrorV1,
    primary_payload: Box<dyn core::any::Any + Send>,
    secondary_payload: Option<Box<dyn core::any::Any + Send>>,
}

#[cfg(feature = "c3-polymorphism")]
impl FsOperationSpoolConstructionUnwindV1 {
    const fn new_v1(
        terminal: FsCasErrorV1,
        primary_payload: Box<dyn core::any::Any + Send>,
    ) -> Self {
        Self {
            terminal,
            primary_payload,
            secondary_payload: None,
        }
    }

    fn new_with_secondary_v1(
        terminal: FsCasErrorV1,
        primary_payload: Box<dyn core::any::Any + Send>,
        secondary_payload: Box<dyn core::any::Any + Send>,
    ) -> Self {
        Self {
            terminal,
            primary_payload,
            secondary_payload: Some(secondary_payload),
        }
    }

    pub(crate) fn into_parts_v1(
        self,
    ) -> (
        FsCasErrorV1,
        Box<dyn core::any::Any + Send>,
        Option<Box<dyn core::any::Any + Send>>,
    ) {
        (self.terminal, self.primary_payload, self.secondary_payload)
    }
}

/// One bounded operation's file-backed metadata. The file has no recovery or
/// publication semantics and its private name is unconditionally removed.
#[cfg(feature = "c3-polymorphism")]
pub(crate) struct FsOperationSpoolV1 {
    owner: FsCasV1,
    path: PathBuf,
    file: Option<File>,
    len: u64,
    bytes_read: u64,
    read_calls: u64,
    bytes_written: u64,
    cleanup_complete: bool,
    cleanup_error: Option<FsCasErrorV1>,
    storage_token: Option<FsStorageOperationTokenV1>,
}

#[cfg(test)]
std::thread_local! {
    static NEXT_OPERATION_SPOOL_READ_OBSERVATION_FOR_TEST_V1:
        std::cell::Cell<Option<(u64, u64)>> = const { std::cell::Cell::new(None) };
}

#[cfg(feature = "c3-polymorphism")]
impl FsOperationSpoolV1 {
    pub(crate) fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        let path_capacity =
            u64::try_from(self.path.capacity()).map_err(|_| CoreError::IntegerOverflow)?;
        u64::try_from(core::mem::size_of::<Self>())
            .map_err(|_| CoreError::IntegerOverflow)?
            .checked_add(path_capacity)
            .ok_or(CoreError::IntegerOverflow)
    }

    pub(crate) const fn direct_storage_observation(&self) -> (u64, u64, u64) {
        (self.bytes_read, self.read_calls, self.bytes_written)
    }

    pub(crate) fn set_len(&mut self, len: u64) -> Result<(), FsCasErrorV1> {
        self.set_len_controlled_v1(len, &mut ContinueFsCasControlV1)
    }

    pub(crate) fn set_len_controlled_v1<C>(
        &mut self,
        len: u64,
        control: &mut C,
    ) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        self.owner.ensure_valid()?;
        if len > self.len {
            return Err(FsCasErrorV1::Integrity);
        }
        sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::PreparationResize)?;
        self.file
            .as_mut()
            .ok_or(FsCasErrorV1::Integrity)?
            .set_len(len)
            .map_err(|error| map_filesystem_write_error_v1(&error))?;
        // The physical truncate has completed. Keep the owned length in sync
        // before attempting the independently fallible ledger transition so
        // explicit cleanup observes the real file state after an accounting
        // or invalidation failure.
        let old_len = self.len;
        self.len = len;
        if let Some(token) = self.storage_token {
            if let Err(error) = self
                .owner
                .record_storage_preparation_length_v1(token, old_len, len)
            {
                return Err(self
                    .owner
                    .fail_closed_preserving_error_controlled_v1(error, control));
            }
        }
        Ok(())
    }

    /// Establish a zero-filled logical table before it becomes observable to
    /// the operation. This is file-backed allocation, not userspace staging;
    /// it is only valid on a newly created empty operation spool.
    pub(crate) fn initialize_zeroed_len_v1(&mut self, len: u64) -> Result<(), FsCasErrorV1> {
        self.initialize_zeroed_len_controlled_v1(len, &mut ContinueFsCasControlV1)
    }

    pub(crate) fn initialize_zeroed_len_controlled_v1<C>(
        &mut self,
        len: u64,
        control: &mut C,
    ) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        self.owner.ensure_valid()?;
        if self.len != 0 {
            return Err(FsCasErrorV1::Integrity);
        }
        sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::PreparationResize)?;
        if let Some(token) = self.storage_token {
            if let Err(error) = self
                .owner
                .record_storage_preparation_length_v1(token, self.len, len)
            {
                return Err(self
                    .owner
                    .fail_closed_preserving_error_controlled_v1(error, control));
            }
        }
        let resize = self
            .file
            .as_mut()
            .ok_or(FsCasErrorV1::Integrity)?
            .set_len(len)
            .map_err(|error| map_filesystem_write_error_v1(&error));
        if let Err(error) = resize {
            // The checked ceiling was charged before the resize. A failed
            // resize may still have changed the file, and a second metadata
            // read is independently fallible, so do not invent an observed
            // length or roll the charge back here. Explicit cleanup obtains
            // the actual length fallibly before unlink and reconciles it.
            self.len = len;
            return Err(error);
        }
        self.len = len;
        self.owner.ensure_valid()
    }

    pub(crate) fn write_exact_at(&mut self, offset: u64, bytes: &[u8]) -> Result<(), FsCasErrorV1> {
        self.write_exact_at_controlled_v1(offset, bytes, &mut ContinueFsCasControlV1)
    }

    pub(crate) fn write_exact_at_controlled_v1<C>(
        &mut self,
        offset: u64,
        bytes: &[u8],
        control: &mut C,
    ) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        self.owner.ensure_valid()?;
        let amount = u64::try_from(bytes.len()).map_err(|_| FsCasErrorV1::Integrity)?;
        let end = offset.checked_add(amount).ok_or(FsCasErrorV1::Integrity)?;
        if offset > self.len {
            return Err(FsCasErrorV1::Integrity);
        }
        let next_len = self.len.max(end);
        if let Some(token) = self.storage_token {
            if let Err(error) = self
                .owner
                .record_storage_preparation_length_v1(token, self.len, next_len)
            {
                return Err(self
                    .owner
                    .fail_closed_preserving_error_controlled_v1(error, control));
            }
        }
        let file = self.file.as_mut().ok_or(FsCasErrorV1::Integrity)?;
        let write = file
            .seek(SeekFrom::Start(offset))
            .map_err(|error| map_filesystem_write_error_v1(&error))
            .and_then(|_| {
                write_all_controlled_v1(
                    file,
                    bytes,
                    FsCasFilesystemBoundaryV1::PreparationWrite,
                    control,
                )
            });
        if let Err(error) = write {
            // Keep the conservative pre-accounted ceiling. Partial writes can
            // extend the file, and failure of a follow-up metadata query must
            // not be encoded as the previous logical length. Cleanup performs
            // the exact fallible observation and reconciliation.
            self.len = next_len;
            return Err(error);
        }
        self.len = next_len;
        #[cfg(test)]
        if control.inject_operation_spool_write_observation_overflow() {
            self.bytes_written = u64::MAX;
        }
        let bytes_written = self
            .bytes_written
            .checked_add(amount)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        self.bytes_written = bytes_written;
        self.owner.ensure_valid()
    }

    #[cfg(test)]
    pub(crate) const fn logical_len_for_test_v1(&self) -> u64 {
        self.len
    }

    pub(crate) fn read_exact_at(
        &mut self,
        offset: u64,
        destination: &mut [u8],
    ) -> Result<(), FsCasErrorV1> {
        self.owner.ensure_valid()?;
        checked_file_read(
            self.file.as_mut().ok_or(FsCasErrorV1::Integrity)?,
            self.len,
            offset,
            destination,
        )?;
        #[cfg(test)]
        NEXT_OPERATION_SPOOL_READ_OBSERVATION_FOR_TEST_V1.with(|seed| {
            if let Some((bytes_read, read_calls)) = seed.take() {
                self.bytes_read = bytes_read;
                self.read_calls = read_calls;
            }
        });
        let amount = u64::try_from(destination.len())
            .map_err(|_| FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        let bytes_read = self
            .bytes_read
            .checked_add(amount)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        let read_calls = self
            .read_calls
            .checked_add(1)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        self.bytes_read = bytes_read;
        self.read_calls = read_calls;
        self.owner.ensure_valid()
    }

    /// Explicitly release an operation spool while the borrowed operation
    /// capability is still alive. A failed removal invalidates the shared
    /// owner/root, leaves the exact immutable preparation residue observable,
    /// and is returned as a typed storage failure. `Drop` is only the unwind
    /// backstop and cannot report errors.
    pub(crate) fn cleanup_controlled_v1<C>(&mut self, control: &mut C) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        let terminal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.cleanup_controlled_inner_v1(control)
        }));
        match terminal {
            Ok(result) => result,
            Err(payload) => {
                // A fault-control callback may unwind at any cleanup boundary.
                // Retain an exact fail-closed terminal state before the panic
                // leaves this owned spool so the preparation coordinator can
                // continue attempting every later target without relying on
                // this spool's Drop backstop.
                let cleanup = FsCasErrorV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool);
                self.cleanup_error = Some(cleanup);
                let invalidation = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    self.owner.cleanup_failure_controlled_v1(
                        FsCasCleanupTargetV1::PreparationSpool,
                        control,
                    )
                }));
                let error = match invalidation {
                    Ok(error) => error,
                    Err(_) => {
                        // Keep the first cleanup unwind authoritative.  A
                        // second callback unwind cannot skip persistence or
                        // erase an invalidation double fault, so complete the
                        // persistence transition without another injected
                        // callback before resuming the original payload.
                        let mut backstop = ContinueFsCasControlV1;
                        match self.owner.invalidate_root_controlled_v1(&mut backstop) {
                            Ok(()) => cleanup,
                            Err(error) => cleanup.dominated_by_v1(error),
                        }
                    }
                };
                self.cleanup_error = Some(error);
                std::panic::resume_unwind(payload)
            }
        }
    }

    /// Return the stable terminal retained when explicit spool cleanup
    /// unwound.  Preparation cleanup uses this only after catching that
    /// unwind; it performs no cleanup retry and exposes no storage handle.
    pub(crate) const fn retained_cleanup_terminal_v1(&self) -> Option<FsCasErrorV1> {
        self.cleanup_error
    }

    fn cleanup_controlled_inner_v1<C>(&mut self, control: &mut C) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        if self.cleanup_complete {
            return Ok(());
        }
        if let Some(error) = self.cleanup_error {
            return Err(error);
        }
        drop(self.file.take());
        let observed_len = match fs::symlink_metadata(&self.path) {
            Ok(metadata) if metadata.is_file() => metadata.len(),
            Ok(_) => {
                return Err(self.retain_cleanup_failure_v1(FsCasErrorV1::Integrity, control));
            }
            Err(error) => {
                let first = map_required_filesystem_read_error_v1(&error);
                return Err(self.retain_cleanup_failure_v1(first, control));
            }
        };
        if let Some(token) = self.storage_token {
            if let Err(first) =
                self.owner
                    .record_storage_preparation_length_v1(token, self.len, observed_len)
            {
                return Err(self.retain_cleanup_failure_v1(first, control));
            }
            self.len = observed_len;
        }
        let injected = control.inject_cleanup_failure(FsCasCleanupTargetV1::PreparationSpool);
        if injected {
            let error = self
                .owner
                .cleanup_failure_controlled_v1(FsCasCleanupTargetV1::PreparationSpool, control);
            self.cleanup_error = Some(error);
            return Err(error);
        }
        if let Err(error) = fs::remove_file(&self.path) {
            let first = map_required_filesystem_write_error_v1(&error);
            return Err(self.retain_cleanup_failure_v1(first, control));
        }
        if let Some(token) = self.storage_token {
            if let Err(first) = self
                .owner
                .record_storage_preparation_remove_v1(token, self.len)
            {
                // The path is already absent. Persist the exact failed
                // terminal now and forbid Drop or a later explicit call from
                // reinterpreting the same physical unlink as success.
                return Err(self.retain_cleanup_failure_v1(first, control));
            }
        }
        self.cleanup_complete = true;
        Ok(())
    }

    /// Make a cleanup terminal stable before returning it. The initiating
    /// cause may be filesystem metadata provenance, structural integrity, or
    /// storage accounting; only cleanup/invalidation is allowed to dominate
    /// it, and a later explicit call or `Drop` must not reinterpret it.
    fn retain_cleanup_failure_v1<C>(&mut self, first: FsCasErrorV1, control: &mut C) -> FsCasErrorV1
    where
        C: FsCasControlV1 + ?Sized,
    {
        let cleanup = self
            .owner
            .cleanup_failure_controlled_v1(FsCasCleanupTargetV1::PreparationSpool, control);
        let terminal = first.dominated_by_v1(cleanup);
        self.cleanup_error = Some(terminal);
        terminal
    }
}

#[cfg(feature = "c3-polymorphism")]
impl Drop for FsOperationSpoolV1 {
    fn drop(&mut self) {
        if self.cleanup_complete || self.cleanup_error.is_some() {
            return;
        }
        drop(self.file.take());
        // Drop cannot return a typed metadata failure, but it must not turn
        // that failure into a fabricated numeric observation.  Only an
        // observed regular file authorizes reconciliation and later release
        // of the logical preparation charge.  Every other outcome invalidates
        // the root and leaves the charge retained for terminal accounting.
        let accounting_authoritative = match fs::symlink_metadata(&self.path) {
            Ok(metadata) if metadata.is_file() => {
                let observed_len = metadata.len();
                if let Some(token) = self.storage_token {
                    if self
                        .owner
                        .record_storage_preparation_length_v1(token, self.len, observed_len)
                        .is_err()
                    {
                        self.owner.invalidate_root_backstop_v1();
                        false
                    } else {
                        self.len = observed_len;
                        true
                    }
                } else {
                    self.len = observed_len;
                    true
                }
            }
            Ok(_) | Err(_) => {
                self.owner.invalidate_root_backstop_v1();
                false
            }
        };
        match fs::remove_file(&self.path) {
            Ok(()) if accounting_authoritative => {
                if let Some(token) = self.storage_token {
                    if self
                        .owner
                        .record_storage_preparation_remove_v1(token, self.len)
                        .is_err()
                    {
                        self.owner.invalidate_root_backstop_v1();
                    }
                }
            }
            Ok(()) => {}
            Err(_) => self.owner.invalidate_root_backstop_v1(),
        }
    }
}

enum PrivatePackStateV1 {
    Empty,
    Writing {
        file: File,
        expected: u64,
        written: u64,
    },
    Sealed {
        file: File,
        sealed: SealedPackV1,
    },
    Transferred,
    CleanupPending,
    CleanupComplete,
    CleanupFailed(FsCasErrorV1),
}

impl FsPrivatePackV1 {
    fn reconcile_preparation_length_v1(&mut self, observed_len: u64) -> Result<(), FsCasErrorV1> {
        if let Some(token) = self.storage_token.filter(|_| self.preparation_accounted) {
            self.owner.record_storage_preparation_length_v1(
                token,
                self.accounted_len,
                observed_len,
            )?;
        }
        self.accounted_len = observed_len;
        Ok(())
    }

    fn record_preparation_removed_v1(&mut self) -> Result<(), FsCasErrorV1> {
        if let Some(token) = self.storage_token.filter(|_| self.preparation_accounted) {
            self.owner
                .record_storage_preparation_remove_v1(token, self.accounted_len)?;
        }
        self.preparation_accounted = false;
        self.accounted_len = 0;
        Ok(())
    }

    fn retain_error_v1(&mut self, error: FsCasErrorV1) -> PackPortErrorV1 {
        self.first_error.get_or_insert(error);
        PackPortErrorV1::Failure
    }

    fn ensure_owner_valid_v1(&mut self) -> Result<(), PackPortErrorV1> {
        match self.owner.ensure_valid() {
            Ok(()) => Ok(()),
            Err(error) => Err(self.retain_error_v1(error)),
        }
    }

    /// Recover the first concrete storage failure after a deliberately lossy
    /// pack-port callback returns. This is crate-private operation plumbing,
    /// not a caller-visible pack API.
    pub(crate) fn take_first_error_typed_v1(&mut self) -> Option<FsCasErrorV1> {
        self.first_error.take()
    }

    fn sealed(&self) -> Result<Option<SealedPackV1>, FsCasErrorV1> {
        self.owner.ensure_valid()?;
        Ok(match self.state {
            PrivatePackStateV1::Sealed { sealed, .. } => Some(sealed),
            _ => None,
        })
    }

    fn prepare_cleanup_v1(&mut self) {
        if matches!(
            self.state,
            PrivatePackStateV1::Transferred
                | PrivatePackStateV1::CleanupComplete
                | PrivatePackStateV1::CleanupFailed(_)
        ) {
            return;
        }
        let old = core::mem::replace(&mut self.state, PrivatePackStateV1::CleanupPending);
        drop(old);
    }

    /// Explicitly release a private carrier while the borrowed operation
    /// capability is alive. The infallible pack-port abort only closes the
    /// file and marks it pending; this method performs the observable removal
    /// and promotes any failure to a typed invalidation. `Drop` is solely an
    /// unwind backstop.
    pub(crate) fn cleanup_controlled_v1<C>(&mut self, control: &mut C) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        let terminal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.cleanup_controlled_inner_v1(control)
        }));
        match terminal {
            Ok(result) => result,
            Err(payload) => {
                // Retain the exact typed terminal before the original cleanup
                // panic crosses this ownership boundary.  Invalidation is an
                // ordinary, observable part of explicit cleanup here: a
                // persistence double fault must dominate CleanupFailed rather
                // than disappearing through the result-less Drop backstop.
                let cleanup = FsCasErrorV1::CleanupFailed(FsCasCleanupTargetV1::PrivatePack);
                self.state = PrivatePackStateV1::CleanupFailed(cleanup);
                let invalidation = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    self.owner
                        .cleanup_failure_controlled_v1(FsCasCleanupTargetV1::PrivatePack, control)
                }));
                let error = match invalidation {
                    Ok(error) => error,
                    Err(_) => {
                        // Preserve the first cleanup unwind.  The secondary
                        // invalidation callback also unwound, so retry only the
                        // persistence transition with the non-injecting
                        // backstop and classify a failure of that attempt.
                        let mut backstop = ContinueFsCasControlV1;
                        match self.owner.invalidate_root_controlled_v1(&mut backstop) {
                            Ok(()) => cleanup,
                            Err(error) => cleanup.dominated_by_v1(error),
                        }
                    }
                };
                self.state = PrivatePackStateV1::CleanupFailed(error);
                std::panic::resume_unwind(payload)
            }
        }
    }

    /// Return the stable terminal retained when explicit private-pack cleanup
    /// unwound before its caller could receive a `Result`.  Construction
    /// boundaries use this after catching the original payload; it neither
    /// retries cleanup nor exposes the private carrier outside the crate.
    pub(crate) const fn retained_cleanup_terminal_v1(&self) -> Option<FsCasErrorV1> {
        match self.state {
            PrivatePackStateV1::CleanupFailed(error) => Some(error),
            _ => None,
        }
    }

    fn cleanup_controlled_inner_v1<C>(&mut self, control: &mut C) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        if matches!(
            self.state,
            PrivatePackStateV1::Transferred | PrivatePackStateV1::CleanupComplete
        ) {
            return Ok(());
        }
        if let PrivatePackStateV1::CleanupFailed(error) = self.state {
            return Err(error);
        }
        self.prepare_cleanup_v1();
        let observed_len = match fs::symlink_metadata(&self.path) {
            Ok(metadata) if metadata.is_file() => metadata.len(),
            Ok(_) => {
                return Err(self.retain_cleanup_failure_v1(FsCasErrorV1::Integrity, control));
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                if self.preparation_accounted {
                    return Err(
                        self.retain_cleanup_failure_v1(FsCasErrorV1::MissingOccupant, control)
                    );
                }
                self.state = PrivatePackStateV1::CleanupComplete;
                return Ok(());
            }
            Err(error) => {
                let first = map_required_filesystem_read_error_v1(&error);
                return Err(self.retain_cleanup_failure_v1(first, control));
            }
        };
        if let Err(first) = self.reconcile_preparation_length_v1(observed_len) {
            return Err(self.retain_cleanup_failure_v1(first, control));
        }
        let injected = control.inject_cleanup_failure(FsCasCleanupTargetV1::PrivatePack);
        if injected {
            let error = self
                .owner
                .cleanup_failure_controlled_v1(FsCasCleanupTargetV1::PrivatePack, control);
            self.state = PrivatePackStateV1::CleanupFailed(error);
            return Err(error);
        }
        if let Err(error) = fs::remove_file(&self.path) {
            let first = map_required_filesystem_write_error_v1(&error);
            return Err(self.retain_cleanup_failure_v1(first, control));
        }
        if let Err(first) = self.record_preparation_removed_v1() {
            // The private path is already absent. Retain this exact terminal
            // so neither a later explicit call nor Drop can reinterpret the
            // successful unlink as a successful accounting transition.
            return Err(self.retain_cleanup_failure_v1(first, control));
        }
        self.state = PrivatePackStateV1::CleanupComplete;
        Ok(())
    }

    /// Stabilize a private-pack cleanup failure while retaining its initiating
    /// structural, filesystem, or accounting cause. Cleanup/invalidation may
    /// dominate, but a later explicit call or `Drop` must return to this same
    /// terminal state rather than reinterpreting the owned residue.
    fn retain_cleanup_failure_v1<C>(&mut self, first: FsCasErrorV1, control: &mut C) -> FsCasErrorV1
    where
        C: FsCasControlV1 + ?Sized,
    {
        let cleanup = self
            .owner
            .cleanup_failure_controlled_v1(FsCasCleanupTargetV1::PrivatePack, control);
        let terminal = first.dominated_by_v1(cleanup);
        self.state = PrivatePackStateV1::CleanupFailed(terminal);
        terminal
    }

    #[cfg(feature = "c3-polymorphism")]
    pub(crate) fn begin_direct_v1(&mut self) -> Result<(), PackPortErrorV1> {
        self.begin_direct_controlled_v1(MAX_PACK_BYTES, &mut ContinueFsCasControlV1)
    }

    #[cfg(feature = "c3-polymorphism")]
    pub(crate) fn begin_direct_controlled_v1<C>(
        &mut self,
        exact_len: u64,
        control: &mut C,
    ) -> Result<(), PackPortErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        self.begin_private_controlled_v1(exact_len, control)?;
        self.append_controlled_v1(&[0_u8; crate::pack::PACK_HEADER_BYTES as usize], control)
    }

    #[cfg(feature = "c3-polymorphism")]
    pub(crate) fn patch_direct_v1(
        &mut self,
        offset: u64,
        bytes: &[u8],
    ) -> Result<(), PackPortErrorV1> {
        self.patch_direct_controlled_v1(offset, bytes, &mut ContinueFsCasControlV1)
    }

    #[cfg(feature = "c3-polymorphism")]
    pub(crate) fn patch_direct_controlled_v1<C>(
        &mut self,
        offset: u64,
        bytes: &[u8],
        control: &mut C,
    ) -> Result<(), PackPortErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        self.ensure_owner_valid_v1()?;
        let PrivatePackStateV1::Writing { file, written, .. } = &mut self.state else {
            return Err(PackPortErrorV1::Failure);
        };
        let end = offset
            .checked_add(u64::try_from(bytes.len()).map_err(|_| PackPortErrorV1::Failure)?)
            .ok_or(PackPortErrorV1::Failure)?;
        if end > *written {
            return Err(PackPortErrorV1::Failure);
        }
        if let Err(error) = file
            .seek(SeekFrom::Start(offset))
            .map_err(|error| map_filesystem_write_error_v1(&error))
            .and_then(|_| {
                write_all_controlled_v1(
                    file,
                    bytes,
                    FsCasFilesystemBoundaryV1::PrivatePackWrite,
                    control,
                )
            })
        {
            return Err(self.retain_error_v1(error));
        }
        self.ensure_owner_valid_v1()
    }

    #[cfg(feature = "c3-polymorphism")]
    pub(crate) fn truncate_direct_v1(&mut self, len: u64) -> Result<(), PackPortErrorV1> {
        self.truncate_direct_controlled_v1(len, &mut ContinueFsCasControlV1)
    }

    #[cfg(feature = "c3-polymorphism")]
    pub(crate) fn truncate_direct_controlled_v1<C>(
        &mut self,
        len: u64,
        control: &mut C,
    ) -> Result<(), PackPortErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        self.ensure_owner_valid_v1()?;
        let PrivatePackStateV1::Writing { file, written, .. } = &mut self.state else {
            return Err(PackPortErrorV1::Failure);
        };
        if len > *written || len < crate::pack::PACK_HEADER_BYTES {
            return Err(PackPortErrorV1::Failure);
        }
        if let Err(error) =
            sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::PrivatePackResize)
                .and_then(|()| {
                    file.set_len(len)
                        .map_err(|error| map_filesystem_write_error_v1(&error))
                })
        {
            return Err(self.retain_error_v1(error));
        }
        *written = len;
        if let Err(error) = self.reconcile_preparation_length_v1(len) {
            let terminal = self
                .owner
                .fail_closed_preserving_error_controlled_v1(error, control);
            return Err(self.retain_error_v1(terminal));
        }
        self.ensure_owner_valid_v1()
    }

    #[cfg(feature = "c3-polymorphism")]
    pub(crate) fn seal_direct_v1(&mut self, id: PackIdV1) -> Result<(), PackPortErrorV1> {
        self.seal_direct_controlled_v1(id, &mut ContinueFsCasControlV1)
    }

    #[cfg(feature = "c3-polymorphism")]
    pub(crate) fn seal_direct_controlled_v1<C>(
        &mut self,
        id: PackIdV1,
        control: &mut C,
    ) -> Result<(), PackPortErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        let PrivatePackStateV1::Writing {
            expected, written, ..
        } = &mut self.state
        else {
            return Err(PackPortErrorV1::Failure);
        };
        *expected = *written;
        self.seal_private_controlled_v1(id, control)
    }

    fn begin_private_controlled_v1<C>(
        &mut self,
        exact_len: u64,
        control: &mut C,
    ) -> Result<(), PackPortErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        self.ensure_owner_valid_v1()?;
        if exact_len > MAX_PACK_BYTES || !matches!(self.state, PrivatePackStateV1::Empty) {
            return Err(PackPortErrorV1::Failure);
        }
        if let Some(token) = self.storage_token {
            if let Err(error) = self.owner.record_storage_preparation_create_v1(token) {
                let terminal = self
                    .owner
                    .fail_closed_preserving_error_controlled_v1(error, control);
                return Err(self.retain_error_v1(terminal));
            }
            self.preparation_accounted = true;
            self.accounted_len = 0;
        }
        // Charge the operation-owned preparation name before invoking either
        // the fault boundary or the real create-new operation. A failed
        // create can then release exactly that charge, while a failed charge
        // rollback is an explicit PrivatePack cleanup failure rather than a
        // reason to erase the directional create error.
        let opened =
            sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::PrivatePackCreate)
                .and_then(|()| {
                    OpenOptions::new()
                        .read(true)
                        .write(true)
                        .create_new(true)
                        .open(&self.path)
                        .map_err(|error| map_required_filesystem_write_error_v1(&error))
                });
        let file = match opened {
            Ok(file) => file,
            Err(error) => {
                if self.preparation_accounted && self.record_preparation_removed_v1().is_err() {
                    let cleanup = self
                        .owner
                        .cleanup_failure_controlled_v1(FsCasCleanupTargetV1::PrivatePack, control);
                    self.state = PrivatePackStateV1::CleanupFailed(cleanup);
                    return Err(self.retain_error_v1(error.dominated_by_v1(cleanup)));
                }
                return Err(self.retain_error_v1(error));
            }
        };
        self.state = PrivatePackStateV1::Writing {
            file,
            expected: exact_len,
            written: 0,
        };
        let permissions =
            sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::PermissionChange)
                .and_then(|()| set_private_file_permissions(&self.path));
        if let Err(error) = permissions {
            self.prepare_cleanup_v1();
            return Err(self.retain_error_v1(error));
        }
        if let Err(error) = self.owner.ensure_valid() {
            self.prepare_cleanup_v1();
            return Err(self.retain_error_v1(error));
        }
        Ok(())
    }

    pub(crate) fn append_controlled_v1<C>(
        &mut self,
        bytes: &[u8],
        control: &mut C,
    ) -> Result<(), PackPortErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        self.ensure_owner_valid_v1()?;
        let (expected, written) = match &self.state {
            PrivatePackStateV1::Writing {
                expected, written, ..
            } => (*expected, *written),
            _ => return Err(PackPortErrorV1::Failure),
        };
        let next = written
            .checked_add(u64::try_from(bytes.len()).map_err(|_| PackPortErrorV1::Failure)?)
            .ok_or(PackPortErrorV1::Failure)?;
        if next > expected {
            return Err(PackPortErrorV1::Failure);
        }
        if let Err(error) = self.reconcile_preparation_length_v1(next) {
            let terminal = self
                .owner
                .fail_closed_preserving_error_controlled_v1(error, control);
            return Err(self.retain_error_v1(terminal));
        }
        let PrivatePackStateV1::Writing {
            file,
            written: state_written,
            ..
        } = &mut self.state
        else {
            let terminal = self
                .owner
                .fail_closed_preserving_error_controlled_v1(FsCasErrorV1::Integrity, control);
            return Err(self.retain_error_v1(terminal));
        };
        let write = file
            .seek(SeekFrom::Start(written))
            .map_err(|error| map_filesystem_write_error_v1(&error))
            .and_then(|_| {
                write_all_controlled_v1(
                    file,
                    bytes,
                    FsCasFilesystemBoundaryV1::PrivatePackWrite,
                    control,
                )
            });
        if let Err(error) = write {
            // `reconcile_preparation_length_v1(next)` already charged the
            // checked maximum this write could make live. Preserve that safe
            // ceiling; explicit cleanup later observes the actual file length
            // and reconciles it before unlink. Never substitute `written` for
            // an unavailable metadata observation after a partial write.
            return Err(self.retain_error_v1(error));
        }
        *state_written = next;
        self.ensure_owner_valid_v1()
    }

    #[cfg(test)]
    pub(crate) const fn direct_lengths_for_test_v1(&self) -> (Option<u64>, u64) {
        let written = match &self.state {
            PrivatePackStateV1::Writing { written, .. } => Some(*written),
            _ => None,
        };
        (written, self.accounted_len)
    }

    fn seal_private_controlled_v1<C>(
        &mut self,
        id: PackIdV1,
        control: &mut C,
    ) -> Result<(), PackPortErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        self.ensure_owner_valid_v1()?;
        let old = std::mem::replace(&mut self.state, PrivatePackStateV1::CleanupPending);
        let PrivatePackStateV1::Writing {
            mut file,
            expected,
            written,
        } = old
        else {
            return Err(PackPortErrorV1::Failure);
        };
        if expected != written {
            self.state = PrivatePackStateV1::Writing {
                file,
                expected,
                written,
            };
            return Err(PackPortErrorV1::Failure);
        }
        if let Err(error) = flush_controlled_v1(
            &mut file,
            FsCasFilesystemBoundaryV1::PrivatePackFlush,
            control,
        ) {
            drop(file);
            return Err(self.retain_error_v1(error));
        }
        drop(file);
        let file = match open_regular_file(&self.path) {
            Ok(file) => file,
            Err(error) => return Err(self.retain_error_v1(error)),
        };
        let cloned = match file.try_clone() {
            Ok(cloned) => cloned,
            Err(error) => {
                drop(file);
                let error = map_filesystem_read_error_v1(&error);
                return Err(self.retain_error_v1(error));
            }
        };
        let mut reader = match FilePackReadV1::from_file(cloned) {
            Ok(reader) => reader,
            Err(error) => {
                drop(file);
                return Err(self.retain_error_v1(error));
            }
        };
        let sealed = match read_sealed_shape(&mut reader) {
            Ok(sealed) => sealed,
            Err(error) => {
                drop(file);
                return Err(self.retain_error_v1(error));
            }
        };
        if sealed.id() != id || sealed.pack_len() != expected {
            drop(file);
            return Err(self.retain_error_v1(FsCasErrorV1::Integrity));
        }
        if let Err(error) = self.owner.ensure_valid() {
            drop(file);
            return Err(self.retain_error_v1(error));
        }
        self.state = PrivatePackStateV1::Sealed { file, sealed };
        Ok(())
    }
}

impl PackReadPortV1 for FsPrivatePackV1 {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        if self.owner.inner.invalidated.load(Ordering::Acquire) {
            return Err(CoreError::ResourceRefused);
        }
        let path_capacity =
            u64::try_from(self.path.capacity()).map_err(|_| CoreError::IntegerOverflow)?;
        u64::try_from(core::mem::size_of::<Self>())
            .map_err(|_| CoreError::IntegerOverflow)?
            .checked_add(path_capacity)
            .ok_or(CoreError::IntegerOverflow)
    }

    fn len(&mut self) -> Result<u64, PackPortErrorV1> {
        self.ensure_owner_valid_v1()?;
        match &self.state {
            PrivatePackStateV1::Empty => Ok(0),
            PrivatePackStateV1::Writing { written, .. } => Ok(*written),
            PrivatePackStateV1::Sealed { sealed, .. } => Ok(sealed.pack_len()),
            PrivatePackStateV1::Transferred
            | PrivatePackStateV1::CleanupPending
            | PrivatePackStateV1::CleanupComplete
            | PrivatePackStateV1::CleanupFailed(_) => Err(PackPortErrorV1::Failure),
        }
    }

    fn read_exact_at(
        &mut self,
        offset: u64,
        destination: &mut [u8],
    ) -> Result<(), PackPortErrorV1> {
        self.ensure_owner_valid_v1()?;
        let (file, len) = match &mut self.state {
            PrivatePackStateV1::Writing { file, written, .. } => {
                if let Err(error) = file.flush() {
                    let error = map_filesystem_write_error_v1(&error);
                    return Err(self.retain_error_v1(error));
                }
                (file, *written)
            }
            PrivatePackStateV1::Sealed { file, sealed } => (file, sealed.pack_len()),
            _ => return Err(PackPortErrorV1::Failure),
        };
        if let Err(error) = checked_file_read(file, len, offset, destination) {
            return Err(self.retain_error_v1(error));
        }
        self.ensure_owner_valid_v1()
    }
}

impl PrivatePackPortV1 for FsPrivatePackV1 {
    fn begin_private(&mut self, exact_len: u64) -> Result<(), PackPortErrorV1> {
        self.begin_private_controlled_v1(exact_len, &mut ContinueFsCasControlV1)
    }

    fn append(&mut self, bytes: &[u8]) -> Result<(), PackPortErrorV1> {
        self.append_controlled_v1(bytes, &mut ContinueFsCasControlV1)
    }

    fn seal_private(&mut self, id: PackIdV1) -> Result<(), PackPortErrorV1> {
        self.seal_private_controlled_v1(id, &mut ContinueFsCasControlV1)
    }

    fn abort_private(&mut self) {
        self.prepare_cleanup_v1();
    }
}

impl Drop for FsPrivatePackV1 {
    fn drop(&mut self) {
        if matches!(
            self.state,
            PrivatePackStateV1::Transferred
                | PrivatePackStateV1::CleanupComplete
                | PrivatePackStateV1::CleanupFailed(_)
        ) {
            return;
        }
        self.prepare_cleanup_v1();
        // As with operation-spool Drop, a missing/wrong-type/unreadable path
        // is not a zero-cost observation and cannot be replaced with the
        // ledger-owned length.  Retain the logical charge and fail closed;
        // only a real regular-file observation may authorize reconciliation.
        let accounting_authoritative = match fs::symlink_metadata(&self.path) {
            Ok(metadata) if metadata.is_file() => {
                if self
                    .reconcile_preparation_length_v1(metadata.len())
                    .is_err()
                {
                    self.owner.invalidate_root_backstop_v1();
                    false
                } else {
                    true
                }
            }
            Err(error) if error.kind() == ErrorKind::NotFound && !self.preparation_accounted => {
                return;
            }
            Ok(_) | Err(_) => {
                self.owner.invalidate_root_backstop_v1();
                false
            }
        };
        match fs::remove_file(&self.path) {
            Ok(()) if accounting_authoritative => {
                if self.record_preparation_removed_v1().is_err() {
                    self.owner.invalidate_root_backstop_v1();
                }
            }
            Ok(()) => {}
            Err(_) => self.owner.invalidate_root_backstop_v1(),
        }
    }
}

#[cfg(test)]
std::thread_local! {
    static NEXT_OCCUPANT_PACK_READ_CALLS_FOR_TEST_V1: std::cell::Cell<Option<u64>> =
        const { std::cell::Cell::new(None) };
}

struct FilePackReadV1 {
    file: File,
    len: u64,
    bytes_read: u64,
    read_calls: u64,
    first_error: Option<FsCasErrorV1>,
}

impl FilePackReadV1 {
    fn open(path: &Path) -> Result<Self, FsCasErrorV1> {
        let file = open_regular_file(path)?;
        Self::from_file(file)
    }

    fn open_occupant(path: &Path) -> Result<Self, FsCasErrorV1> {
        let file = open_regular_file_if_present(path)
            .map_err(|error| match error {
                FsCasErrorV1::Integrity => FsCasErrorV1::MalformedOccupant,
                other => other,
            })?
            .ok_or(FsCasErrorV1::MissingOccupant)?;
        let reader = Self::from_file(file).map_err(|error| match error {
            FsCasErrorV1::Integrity => FsCasErrorV1::MalformedOccupant,
            other => other,
        })?;
        #[cfg(test)]
        let mut reader = reader;
        #[cfg(test)]
        NEXT_OCCUPANT_PACK_READ_CALLS_FOR_TEST_V1.with(|seed| {
            if let Some(read_calls) = seed.take() {
                reader.read_calls = read_calls;
            }
        });
        Ok(reader)
    }

    fn from_file(file: File) -> Result<Self, FsCasErrorV1> {
        let metadata = file
            .metadata()
            .map_err(|error| map_filesystem_read_error_v1(&error))?;
        if !metadata.file_type().is_file() {
            return Err(FsCasErrorV1::MalformedOccupant);
        }
        Ok(Self {
            file,
            len: metadata.len(),
            bytes_read: 0,
            read_calls: 0,
            first_error: None,
        })
    }

    fn take_first_error_typed_v1(&mut self) -> Option<FsCasErrorV1> {
        self.first_error.take()
    }

    fn restore_failure_v1(&mut self, fallback: FsCasErrorV1) -> FsCasErrorV1 {
        self.take_first_error_typed_v1().unwrap_or(fallback)
    }
}

fn restore_pack_occupant_failure_v1(pack: &mut FilePackReadV1, error: CoreError) -> FsCasErrorV1 {
    pack.restore_failure_v1(match error {
        CoreError::PackInvalid => FsCasErrorV1::MalformedOccupant,
        other => FsCasErrorV1::Core(other),
    })
}

impl PackReadPortV1 for FilePackReadV1 {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        u64::try_from(core::mem::size_of::<Self>()).map_err(|_| CoreError::IntegerOverflow)
    }

    fn len(&mut self) -> Result<u64, PackPortErrorV1> {
        Ok(self.len)
    }

    fn read_exact_at(
        &mut self,
        offset: u64,
        destination: &mut [u8],
    ) -> Result<(), PackPortErrorV1> {
        if let Err(error) = checked_file_read(&mut self.file, self.len, offset, destination) {
            self.first_error.get_or_insert(error);
            return Err(PackPortErrorV1::Failure);
        }
        let Some(bytes_read) = self.bytes_read.checked_add(destination.len() as u64) else {
            self.first_error
                .get_or_insert(FsCasErrorV1::Core(CoreError::IntegerOverflow));
            return Err(PackPortErrorV1::Failure);
        };
        let Some(read_calls) = self.read_calls.checked_add(1) else {
            self.first_error
                .get_or_insert(FsCasErrorV1::Core(CoreError::IntegerOverflow));
            return Err(PackPortErrorV1::Failure);
        };
        self.bytes_read = bytes_read;
        self.read_calls = read_calls;
        Ok(())
    }
}

struct ResolvedObjectV1 {
    id: TypedPhysicalObjectIdV1,
    file: File,
    pack_len: u64,
    location: PackObjectLocationV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FsCasAcceptedClosureReadV1 {
    version_record: PhysicalVersionRecordIdV1,
    object_count: u64,
    transcript: [u8; 32],
}

impl FsCasAcceptedClosureReadV1 {
    pub(crate) const fn version_record(self) -> PhysicalVersionRecordIdV1 {
        self.version_record
    }

    pub(crate) const fn object_count(self) -> u64 {
        self.object_count
    }

    pub(crate) const fn transcript(self) -> [u8; 32] {
        self.transcript
    }
}

pub(crate) struct FsCasOccupiedV1 {
    cas: FsCasV1,
    current: Option<ResolvedObjectV1>,
    previous: Option<ResolvedObjectV1>,
    bytes_read: u64,
    read_calls: u64,
    first_error: Option<FsCasErrorV1>,
    validation_scratch: [u8; COMPARISON_WINDOW_BYTES],
    #[cfg(test)]
    unlocked_payload_read_hook: Option<Arc<dyn Fn() + Send + Sync>>,
    #[cfg(test)]
    payload_read_observation_for_test: Option<(u64, u64)>,
}

#[cfg(test)]
std::thread_local! {
    static NEXT_OCCUPIED_READ_OBSERVATION_FOR_TEST_V1:
        std::cell::Cell<Option<(u64, u64)>> = const { std::cell::Cell::new(None) };
    static NEXT_OCCUPIED_PAYLOAD_READ_OBSERVATION_FOR_TEST_V1:
        std::cell::Cell<Option<(u64, u64)>> = const { std::cell::Cell::new(None) };
}

impl FsCasOccupiedV1 {
    pub(crate) fn direct_storage_read_observation_typed_v1(
        &self,
    ) -> Result<(u64, u64), FsCasErrorV1> {
        self.cas.ensure_valid()?;
        Ok((self.bytes_read, self.read_calls))
    }

    /// The first concrete FsCas failure hidden by a generic immutable-port
    /// callback. Object decoders use the backend-neutral port, so the FsCas
    /// owner retains this side channel to restore the exact private failure at
    /// the orchestration boundary.
    pub(crate) const fn first_error_typed_v1(&self) -> Option<FsCasErrorV1> {
        self.first_error
    }

    pub(crate) fn retain_first_error_typed_v1(&mut self, error: FsCasErrorV1) {
        self.first_error.get_or_insert(error);
    }

    #[cfg(test)]
    pub(crate) fn resolved_object_cached_for_test_v1(&self, id: TypedPhysicalObjectIdV1) -> bool {
        self.current
            .as_ref()
            .is_some_and(|current| current.id == id)
            || self
                .previous
                .as_ref()
                .is_some_and(|previous| previous.id == id)
    }

    pub(crate) fn occupied_len_typed_v1(
        &mut self,
        id: TypedPhysicalObjectIdV1,
    ) -> Result<Option<u64>, FsCasErrorV1> {
        let mut control = ContinueFsCasControlV1;
        self.occupied_len_typed_controlled_v1(id, &mut control)
    }

    pub(crate) fn occupied_len_typed_controlled_v1<C>(
        &mut self,
        id: TypedPhysicalObjectIdV1,
        control: &mut C,
    ) -> Result<Option<u64>, FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        self.cas.ensure_valid()?;
        let (locator, mut pack) = {
            // Snapshot the immutable locator/catalog/carrier relationship
            // while publication is excluded. Complete pack-index and object
            // validation deliberately runs after this guard is released.
            let guard = self.cas.lock_visibility_controlled_v1(control)?;
            let snapshot = (|| {
                self.cas.ensure_valid()?;
                if self
                    .current
                    .as_ref()
                    .is_some_and(|current| current.id == id)
                {
                    return Ok(Err(self
                        .current
                        .as_ref()
                        .map(|current| current.location.object_len)));
                }
                if self
                    .previous
                    .as_ref()
                    .is_some_and(|previous| previous.id == id)
                {
                    core::mem::swap(&mut self.current, &mut self.previous);
                    return Ok(Err(self
                        .current
                        .as_ref()
                        .map(|current| current.location.object_len)));
                }
                let objects = self.cas.inner.root.join("objects");
                validate_required_root_directory(&objects)?;
                let path = objects.join(hex_typed_id(id));
                sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::ObjectLocatorRead)?;
                let Some(locator) = read_object_locator_if_present(&path, id)? else {
                    self.current = None;
                    self.previous = None;
                    return Ok(Err(None));
                };
                let pack_name = hex_id(locator.sealed().id().as_bytes());
                sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::CatalogMarkerRead)?;
                let catalog =
                    read_catalog_marker(&self.cas.inner.root.join("catalog").join(&pack_name))?;
                if catalog != locator.sealed() {
                    return Err(FsCasErrorV1::Integrity);
                }
                let carrier = self.cas.inner.root.join("carriers").join(&pack_name);
                sample_filesystem_fault_v1(
                    control,
                    FsCasFilesystemBoundaryV1::CarrierMetadataRead,
                )?;
                let pack = FilePackReadV1::open_occupant(&carrier)?;
                Ok(Ok((locator, pack)))
            })();
            self.cas.unlock_visibility_controlled_v1(guard, control);
            match snapshot? {
                Ok(snapshot) => snapshot,
                Err(cached) => return Ok(cached),
            }
        };
        // The locator and catalog reads above are one completed metadata
        // snapshot. Commit their direct observation as one transaction so a
        // late checked failure cannot expose only half of the real work.
        let locator_bytes = u64::try_from(PERSISTENT_LOCATOR_BYTES_V1)
            .map_err(|_| FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        let catalog_bytes = u64::try_from(CATALOG_MARKER_BYTES)
            .map_err(|_| FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        let metadata_bytes = locator_bytes
            .checked_add(catalog_bytes)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        let bytes_read = self
            .bytes_read
            .checked_add(metadata_bytes)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        let read_calls = self
            .read_calls
            .checked_add(2)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        self.bytes_read = bytes_read;
        self.read_calls = read_calls;
        let mut local_counters = OperationCountersV1::default();
        sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::CarrierIndexRead)?;
        let indexed = match locate_validated_pack_index_entry_v1(
            &mut pack,
            locator.sealed(),
            id,
            &mut local_counters,
        ) {
            Ok(indexed) => indexed,
            Err(error) => return Err(restore_pack_occupant_failure_v1(&mut pack, error)),
        }
        .ok_or(FsCasErrorV1::MissingOccupant)?;
        if indexed != locator.entry() {
            return Err(FsCasErrorV1::Integrity);
        }
        sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::CarrierObjectRead)?;
        let location = match validate_validated_pack_object_v1(
            &mut pack,
            indexed,
            &mut self.validation_scratch,
            &mut local_counters,
        ) {
            Ok(location) => location,
            Err(error) => return Err(restore_pack_occupant_failure_v1(&mut pack, error)),
        };
        let bytes_read = self
            .bytes_read
            .checked_add(pack.bytes_read)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        let read_calls = self
            .read_calls
            .checked_add(pack.read_calls)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        self.bytes_read = bytes_read;
        self.read_calls = read_calls;

        // The root may have been invalidated while validation was in flight.
        // Re-enter the visibility fence before making the resolved carrier
        // usable by this reader.
        let guard = self.cas.lock_visibility_controlled_v1(control)?;
        let commit = self.cas.ensure_valid();
        if commit.is_ok() {
            self.previous = self.current.take();
            self.current = Some(ResolvedObjectV1 {
                id,
                file: pack.file,
                pack_len: pack.len,
                location,
            });
        }
        self.cas.unlock_visibility_controlled_v1(guard, control);
        commit?;
        Ok(Some(location.object_len))
    }

    pub(crate) fn read_occupied_exact_at_typed_v1(
        &mut self,
        id: TypedPhysicalObjectIdV1,
        offset: u64,
        destination: &mut [u8],
    ) -> Result<(), FsCasErrorV1> {
        let mut control = ContinueFsCasControlV1;
        self.read_occupied_exact_at_typed_controlled_v1(id, offset, destination, &mut control)
    }

    pub(crate) fn read_occupied_exact_at_typed_controlled_v1<C>(
        &mut self,
        id: TypedPhysicalObjectIdV1,
        offset: u64,
        destination: &mut [u8],
        control: &mut C,
    ) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        self.cas.ensure_valid()?;
        let amount = u64::try_from(destination.len())
            .map_err(|_| FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        {
            // Resolve and bounds-check under the visibility fence, but never
            // hold the root mutex over the payload pread itself. The file is
            // private to this occupied-reader instance.
            let guard = self.cas.lock_visibility_controlled_v1(control)?;
            let bounds = (|| {
                self.cas.ensure_valid()?;
                if self.current.as_ref().is_none_or(|current| current.id != id)
                    && self
                        .previous
                        .as_ref()
                        .is_some_and(|previous| previous.id == id)
                {
                    core::mem::swap(&mut self.current, &mut self.previous);
                }
                let resolved = self.current.as_ref().ok_or(FsCasErrorV1::Integrity)?;
                if resolved.id != id {
                    return Err(FsCasErrorV1::Integrity);
                }
                let end = offset
                    .checked_add(amount)
                    .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
                if end > resolved.location.object_len {
                    return Err(FsCasErrorV1::Integrity);
                }
                Ok(())
            })();
            self.cas.unlock_visibility_controlled_v1(guard, control);
            bounds?;
        }
        let resolved = self.current.as_mut().ok_or(FsCasErrorV1::Integrity)?;
        let absolute = resolved
            .location
            .object_offset
            .checked_add(offset)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        #[cfg(test)]
        if let Some((bytes_read, read_calls)) = self.payload_read_observation_for_test.take() {
            self.bytes_read = bytes_read;
            self.read_calls = read_calls;
        }
        #[cfg(test)]
        if let Some(hook) = &self.unlocked_payload_read_hook {
            hook();
        }
        // The authenticated object bounds and carrier identity have already
        // been established above. This semantic boundary attributes a real
        // payload read without pretending to expose native syscall counts.
        sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::CarrierPayloadRead)?;
        checked_file_read(&mut resolved.file, resolved.pack_len, absolute, destination).map_err(
            |error| {
                if error == FsCasErrorV1::Integrity {
                    // Bounds were authenticated above. An integrity result at
                    // this point means the resolved incumbent carrier changed
                    // shape (or its authenticated locator no longer fits it),
                    // which is a malformed occupant. Actual metadata/read I/O
                    // and post-metadata EOF retain their directional variants.
                    FsCasErrorV1::MalformedOccupant
                } else {
                    error
                }
            },
        )?;
        let bytes_read = self
            .bytes_read
            .checked_add(amount)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        let read_calls = self
            .read_calls
            .checked_add(1)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        self.bytes_read = bytes_read;
        self.read_calls = read_calls;

        // Do not return bytes from a root invalidated during the unlocked
        // read. The destination may contain data, but the typed operation
        // fails closed and cannot commit it to a successful handoff.
        let guard = self.cas.lock_visibility_controlled_v1(control)?;
        let validity = self.cas.ensure_valid();
        self.cas.unlock_visibility_controlled_v1(guard, control);
        validity
    }
}

impl OccupiedImmutableReadPortV1 for FsCasOccupiedV1 {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        u64::try_from(core::mem::size_of::<Self>()).map_err(|_| CoreError::IntegerOverflow)
    }

    fn direct_storage_read_observation(&mut self) -> Result<(u64, u64), ImmutablePortErrorV1> {
        match self.direct_storage_read_observation_typed_v1() {
            Ok(observation) => Ok(observation),
            Err(error) => {
                self.first_error.get_or_insert(error);
                Err(ImmutablePortErrorV1::Failure)
            }
        }
    }

    fn occupied_len(
        &mut self,
        id: TypedPhysicalObjectIdV1,
    ) -> Result<Option<u64>, ImmutablePortErrorV1> {
        match self.occupied_len_typed_v1(id) {
            Ok(value) => Ok(value),
            Err(error) => {
                self.first_error.get_or_insert(error);
                Err(ImmutablePortErrorV1::Failure)
            }
        }
    }

    fn read_occupied_exact_at(
        &mut self,
        id: TypedPhysicalObjectIdV1,
        offset: u64,
        destination: &mut [u8],
    ) -> Result<(), ImmutablePortErrorV1> {
        match self.read_occupied_exact_at_typed_v1(id, offset, destination) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.first_error.get_or_insert(error);
                Err(ImmutablePortErrorV1::Failure)
            }
        }
    }
}

struct ClosureTranscriptV1(blake3::Hasher);

impl ClosureTranscriptV1 {
    fn new(object_count: u64) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"LAYERFS-CLOSURE-TRANSCRIPT-V1\0");
        hasher.update(&object_count.to_be_bytes());
        Self(hasher)
    }

    fn push(&mut self, validated: &ValidatedOccupiedObjectV1) {
        self.0.update(&validated.ordinal().to_be_bytes());
        self.0.update(&[typed_kind_byte(validated.id())]);
        self.0.update(validated.id().as_bytes());
        self.0.update(&validated.canonical_len().to_be_bytes());
    }

    fn finish(self) -> [u8; 32] {
        *self.0.finalize().as_bytes()
    }
}

struct FsClosureFenceV1<'custody, 'control, C>
where
    C: FsCasControlV1 + ?Sized,
{
    cas: FsCasV1,
    marker_custody: &'custody mut ImmutableMarkerCustodyV1,
    control: &'control mut C,
    inject_marker_alias_cleanup: bool,
    operation_nonce: u64,
    storage_token: Option<FsStorageOperationTokenV1>,
    expected_count: Option<u64>,
    observed_count: u64,
    previous_id: Option<TypedPhysicalObjectIdV1>,
    observed_version: Option<TypedPhysicalObjectIdV1>,
    transcript: Option<ClosureTranscriptV1>,
    complete: Option<CompleteValidatedClosureV1>,
    first_error: Option<FsCasErrorV1>,
}

impl<'custody, 'control, C> FsClosureFenceV1<'custody, 'control, C>
where
    C: FsCasControlV1 + ?Sized,
{
    fn new(
        cas: FsCasV1,
        operation_nonce: u64,
        storage_token: Option<FsStorageOperationTokenV1>,
        marker_custody: &'custody mut ImmutableMarkerCustodyV1,
        control: &'control mut C,
        inject_marker_alias_cleanup: bool,
    ) -> Self {
        Self {
            cas,
            marker_custody,
            control,
            inject_marker_alias_cleanup,
            operation_nonce,
            storage_token,
            expected_count: None,
            observed_count: 0,
            previous_id: None,
            observed_version: None,
            transcript: None,
            complete: None,
            first_error: None,
        }
    }
}

impl<C> PreparedImmutableClosurePortV1 for FsClosureFenceV1<'_, '_, C>
where
    C: FsCasControlV1 + ?Sized,
{
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        u64::try_from(core::mem::size_of::<Self>()).map_err(|_| CoreError::IntegerOverflow)
    }

    fn begin_private_closure(&mut self, object_count: u64) -> Result<(), ImmutablePortErrorV1> {
        if self.expected_count.replace(object_count).is_some() || self.complete.is_some() {
            return Err(ImmutablePortErrorV1::Failure);
        }
        self.observed_count = 0;
        self.previous_id = None;
        self.observed_version = None;
        self.transcript = Some(ClosureTranscriptV1::new(object_count));
        Ok(())
    }

    fn begin_private_object(
        &mut self,
        _id: TypedPhysicalObjectIdV1,
        _exact_len: u64,
    ) -> Result<(), ImmutablePortErrorV1> {
        // The direct operation pack must already contain every new object.
        Err(ImmutablePortErrorV1::Failure)
    }

    fn write_private_object(
        &mut self,
        _canonical_fragment: &[u8],
    ) -> Result<(), ImmutablePortErrorV1> {
        Err(ImmutablePortErrorV1::Failure)
    }

    fn finish_private_object(
        &mut self,
        _id: TypedPhysicalObjectIdV1,
    ) -> Result<(), ImmutablePortErrorV1> {
        Err(ImmutablePortErrorV1::Failure)
    }

    fn note_reused_object(
        &mut self,
        validated: ValidatedOccupiedObjectV1,
    ) -> Result<(), ImmutablePortErrorV1> {
        if self.expected_count.is_none()
            || validated.ordinal() != self.observed_count
            || self.observed_count >= self.expected_count.unwrap_or(0)
            || self.previous_id.is_some_and(|previous| {
                typed_kind_byte(previous)
                    .cmp(&typed_kind_byte(validated.id()))
                    .then_with(|| previous.as_bytes().cmp(validated.id().as_bytes()))
                    != core::cmp::Ordering::Less
            })
        {
            return Err(ImmutablePortErrorV1::Failure);
        }
        if matches!(validated.id(), TypedPhysicalObjectIdV1::VersionRecord(_))
            && self.observed_version.replace(validated.id()).is_some()
        {
            return Err(ImmutablePortErrorV1::Failure);
        }
        self.transcript
            .as_mut()
            .ok_or(ImmutablePortErrorV1::Failure)?
            .push(&validated);
        self.previous_id = Some(validated.id());
        self.observed_count = self
            .observed_count
            .checked_add(1)
            .ok_or(ImmutablePortErrorV1::Failure)?;
        Ok(())
    }

    fn make_closure_visible(
        &mut self,
        version_record: TypedPhysicalObjectIdV1,
    ) -> Result<(), ImmutablePortErrorV1> {
        self.cas.ensure_valid().map_err(|error| {
            self.first_error.get_or_insert(error);
            ImmutablePortErrorV1::Failure
        })?;
        if !matches!(version_record, TypedPhysicalObjectIdV1::VersionRecord(_))
            || self.expected_count != Some(self.observed_count)
            || self.observed_version != Some(version_record)
        {
            return Err(ImmutablePortErrorV1::Failure);
        }
        let transcript = self
            .transcript
            .take()
            .ok_or(ImmutablePortErrorV1::Failure)?
            .finish();
        // Keep this guard outside the complete closure-publication unwind
        // boundary. Every remaining control hook executes while publication is
        // serialized, so any of them may unwind only after the guard has been
        // caught and dropped normally.
        let mut publication_guard = Some(
            self.cas
                .lock_publication_controlled_v1(self.control)
                .map_err(|error| {
                    self.first_error.get_or_insert(error);
                    ImmutablePortErrorV1::Failure
                })?,
        );
        let terminal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (|| -> Result<(), ImmutablePortErrorV1> {
                self.cas.ensure_valid().map_err(|error| {
                    self.first_error.get_or_insert(error);
                    ImmutablePortErrorV1::Failure
                })?;
                let marker = encode_closure_marker(
                    version_record,
                    self.observed_count,
                    self.cas.inner.generation,
                    transcript,
                );
                let destination = self
                    .cas
                    .inner
                    .root
                    .join("closures")
                    .join(hex_typed_id(version_record));
                let incumbent = {
                    let _visibility_guard = self
                        .cas
                        .lock_visibility_controlled_v1(self.control)
                        .map_err(|error| {
                            self.first_error.get_or_insert(error);
                            ImmutablePortErrorV1::Failure
                        })?;
                    self.cas.ensure_valid().map_err(|error| {
                        self.first_error.get_or_insert(error);
                        ImmutablePortErrorV1::Failure
                    })?;
                    read_exact_regular_file_if_present::<CLOSURE_MARKER_BYTES>(&destination)
                        .map_err(|error| {
                            self.first_error
                                .get_or_insert(if error == FsCasErrorV1::Integrity {
                                    FsCasErrorV1::MalformedOccupant
                                } else {
                                    error
                                });
                            ImmutablePortErrorV1::Failure
                        })?
                };
                if let Some(incumbent) = incumbent {
                    let TypedPhysicalObjectIdV1::VersionRecord(version_record_id) = version_record
                    else {
                        self.first_error.get_or_insert(FsCasErrorV1::Integrity);
                        return Err(ImmutablePortErrorV1::Failure);
                    };
                    decode_closure_marker_v1(
                        incumbent,
                        version_record_id,
                        self.cas.inner.generation,
                    )
                    .map_err(|error| {
                        self.first_error.get_or_insert(error);
                        ImmutablePortErrorV1::Failure
                    })?;
                    if incumbent != marker {
                        self.first_error
                            .get_or_insert(FsCasErrorV1::UnequalOccupant);
                        return Err(ImmutablePortErrorV1::Failure);
                    }
                } else {
                    sample_control(
                        self.control,
                        FsCasBoundaryV1::BeforeClosureMarkerPublication,
                    )
                    .map_err(|error| {
                        self.first_error.get_or_insert(error);
                        ImmutablePortErrorV1::Failure
                    })?;
                    // This marker is only the local complete-closure fence. Publishing
                    // it performs no authority dispatch and creates no private Version.
                    let publication = publish_small_marker_controlled(
                        &self.cas.inner.root.join("preparation"),
                        "closure",
                        &destination,
                        &marker,
                        Some(&self.cas),
                        self.storage_token,
                        self.inject_marker_alias_cleanup
                            .then_some(FsCasBoundaryV1::AfterClosureMarkerLink),
                        None,
                        Some(&mut *self.marker_custody),
                        self.control,
                    );
                    match publication {
                        Err(error) => {
                            self.first_error.get_or_insert(error);
                            return Err(ImmutablePortErrorV1::Failure);
                        }
                        Ok(MarkerPublicationV1::VisibleWithPreparationResidue(first_error)) => {
                            // The complete-closure marker is already visible. Keep
                            // the admitted carrier/catalog/locator closure intact,
                            // invalidate the root, and refuse to mint a usable
                            // handoff capability.
                            let error = self.cas.cleanup_failure_controlled_v1(
                                FsCasCleanupTargetV1::PublishedMarkerAlias,
                                self.control,
                            );
                            let error =
                                first_error.map_or(error, |first| first.dominated_by_v1(error));
                            self.first_error.get_or_insert(error);
                            return Err(ImmutablePortErrorV1::Failure);
                        }
                        Ok(MarkerPublicationV1::VisibleTerminal(error)) => {
                            // Marker custody is already visible. Preserve the
                            // post-link cleanup/invalidation terminal for the
                            // closure lifecycle to retain and terminalize.
                            self.first_error.get_or_insert(error);
                            return Err(ImmutablePortErrorV1::Failure);
                        }
                        Ok(MarkerPublicationV1::VisibleClean) => {}
                        Ok(MarkerPublicationV1::IncumbentWithPreparationResidue(
                            bytes,
                            cleanup,
                        )) => {
                            let TypedPhysicalObjectIdV1::VersionRecord(version_record_id) =
                                version_record
                            else {
                                self.first_error.get_or_insert(
                                    FsCasErrorV1::Integrity.dominated_by_v1(cleanup),
                                );
                                return Err(ImmutablePortErrorV1::Failure);
                            };
                            let authenticated = decode_closure_marker_v1(
                                bytes,
                                version_record_id,
                                self.cas.inner.generation,
                            )
                            .map(|_| ())
                            .and_then(|()| {
                                if bytes == marker {
                                    Ok(())
                                } else {
                                    Err(FsCasErrorV1::UnequalOccupant)
                                }
                            });
                            let terminal = match authenticated {
                                Ok(()) => cleanup,
                                Err(error) => error.dominated_by_v1(cleanup),
                            };
                            self.first_error.get_or_insert(terminal);
                            return Err(ImmutablePortErrorV1::Failure);
                        }
                        Ok(MarkerPublicationV1::IncumbentClean(bytes)) => {
                            let TypedPhysicalObjectIdV1::VersionRecord(version_record_id) =
                                version_record
                            else {
                                self.first_error.get_or_insert(FsCasErrorV1::Integrity);
                                return Err(ImmutablePortErrorV1::Failure);
                            };
                            decode_closure_marker_v1(
                                bytes,
                                version_record_id,
                                self.cas.inner.generation,
                            )
                            .map_err(|error| {
                                self.first_error.get_or_insert(error);
                                ImmutablePortErrorV1::Failure
                            })?;
                            if bytes != marker {
                                self.first_error
                                    .get_or_insert(FsCasErrorV1::UnequalOccupant);
                                return Err(ImmutablePortErrorV1::Failure);
                            }
                        }
                    }
                }
                self.complete = Some(CompleteValidatedClosureV1 {
                    owner: self.cas.clone(),
                    generation: self.cas.inner.generation,
                    operation_nonce: self.operation_nonce,
                    version_record,
                    object_count: self.observed_count,
                    transcript,
                    consumed: false,
                });
                Ok(())
            })()
        }));
        drop(publication_guard.take());
        match terminal {
            Ok(result) => result,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }

    fn abort_private_closure(&mut self) {
        self.expected_count = None;
        self.observed_count = 0;
        self.previous_id = None;
        self.observed_version = None;
        self.transcript = None;
        self.complete = None;
    }
}

fn compare_complete_pack_bytes<C>(
    candidate: &mut FsPrivatePackV1,
    incumbent: &mut FilePackReadV1,
    len: u64,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    counters: &mut OperationCountersV1,
    control: &mut C,
) -> Result<(), FsCasErrorV1>
where
    C: FsCasControlV1 + ?Sized,
{
    let half = scratch.len() / 2;
    let (left, right) = scratch.split_at_mut(half);
    let mut offset = 0_u64;
    while offset < len {
        sample_control(control, FsCasBoundaryV1::BeforeIncumbentComparisonWindow)?;
        sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::IncumbentComparisonRead)?;
        let take = usize::try_from((len - offset).min(half as u64))
            .map_err(|_| FsCasErrorV1::Integrity)?;
        if candidate.read_exact_at(offset, &mut left[..take]).is_err() {
            return Err(candidate
                .take_first_error_typed_v1()
                .unwrap_or(FsCasErrorV1::Integrity));
        }
        if incumbent.read_exact_at(offset, &mut right[..take]).is_err() {
            return Err(incumbent.restore_failure_v1(FsCasErrorV1::MalformedOccupant));
        }
        if left[..take] != right[..take] {
            return Err(FsCasErrorV1::UnequalOccupant);
        }
        offset = offset
            .checked_add(u64::try_from(take).map_err(|_| FsCasErrorV1::Integrity)?)
            .ok_or(FsCasErrorV1::Integrity)?;
        let amount = u64::try_from(take).map_err(|_| FsCasErrorV1::Integrity)?;
        counters.record_fscas_read(amount, 1)?;
        counters.record_incumbent_comparison(amount, 1)?;
        sample_control(control, FsCasBoundaryV1::AfterIncumbentComparisonWindow)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn compare_complete_object_bytes<C>(
    candidate: &mut FilePackReadV1,
    candidate_location: PackObjectLocationV1,
    incumbent: &mut FilePackReadV1,
    incumbent_location: PackObjectLocationV1,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    counters: &mut OperationCountersV1,
    control: &mut C,
) -> Result<(), FsCasErrorV1>
where
    C: FsCasControlV1 + ?Sized,
{
    if candidate_location.object_len != incumbent_location.object_len {
        return Err(FsCasErrorV1::UnequalOccupant);
    }
    let half = scratch.len() / 2;
    let (left, right) = scratch.split_at_mut(half);
    let mut offset = 0_u64;
    while offset < candidate_location.object_len {
        sample_control(control, FsCasBoundaryV1::BeforeObjectComparisonWindow)?;
        sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::IncumbentComparisonRead)?;
        let take = usize::try_from((candidate_location.object_len - offset).min(half as u64))
            .map_err(|_| FsCasErrorV1::Integrity)?;
        let candidate_offset = candidate_location
            .object_offset
            .checked_add(offset)
            .ok_or(FsCasErrorV1::Integrity)?;
        let incumbent_offset = incumbent_location
            .object_offset
            .checked_add(offset)
            .ok_or(FsCasErrorV1::Integrity)?;
        if candidate
            .read_exact_at(candidate_offset, &mut left[..take])
            .is_err()
        {
            return Err(candidate.restore_failure_v1(FsCasErrorV1::Integrity));
        }
        if incumbent
            .read_exact_at(incumbent_offset, &mut right[..take])
            .is_err()
        {
            return Err(incumbent.restore_failure_v1(FsCasErrorV1::MalformedOccupant));
        }
        if left[..take] != right[..take] {
            return Err(FsCasErrorV1::UnequalOccupant);
        }
        let amount = u64::try_from(take).map_err(|_| FsCasErrorV1::Integrity)?;
        offset = offset.checked_add(amount).ok_or(FsCasErrorV1::Integrity)?;
        counters.record_fscas_read(amount.checked_mul(2).ok_or(FsCasErrorV1::Integrity)?, 2)?;
        counters.record_incumbent_comparison(amount, 1)?;
        sample_control(control, FsCasBoundaryV1::AfterObjectComparisonWindow)?;
    }
    Ok(())
}

fn sample_control<C>(control: &mut C, boundary: FsCasBoundaryV1) -> Result<(), FsCasErrorV1>
where
    C: FsCasControlV1 + ?Sized,
{
    control.boundary_reached(boundary);
    if control.cancellation_requested() {
        Err(FsCasErrorV1::Core(CoreError::Cancelled))
    } else if control.deadline_exceeded() {
        Err(FsCasErrorV1::Core(CoreError::Deadline))
    } else {
        Ok(())
    }
}

fn sample_filesystem_fault_v1<C>(
    control: &mut C,
    boundary: FsCasFilesystemBoundaryV1,
) -> Result<(), FsCasErrorV1>
where
    C: FsCasControlV1 + ?Sized,
{
    control
        .inject_filesystem_failure(boundary)
        .map_or(Ok(()), Err)
}

/// Preserve the direction and progress semantics of a failed immutable or
/// preparation read. Structural/malformed classification is deliberately not
/// performed here: it is valid only after the read itself succeeds.
fn map_filesystem_read_error_v1(error: &std::io::Error) -> FsCasErrorV1 {
    #[cfg(unix)]
    match error.raw_os_error() {
        Some(libc::EACCES) | Some(libc::EPERM) => {
            return FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied);
        }
        _ => {}
    }
    match error.kind() {
        ErrorKind::PermissionDenied => {
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied)
        }
        ErrorKind::UnexpectedEof => FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ShortRead),
        _ => FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ReadFailure),
    }
}

/// Map failure while opening or authenticating a namespace component that is
/// required to exist.  Optional vacancy probes handle `NotFound` at their
/// call site; a missing root or fixed FsCas directory is instead a precise
/// missing-occupant failure, never a generic read failure.
fn map_required_filesystem_read_error_v1(error: &std::io::Error) -> FsCasErrorV1 {
    if error.kind() == ErrorKind::NotFound {
        FsCasErrorV1::MissingOccupant
    } else {
        map_filesystem_read_error_v1(error)
    }
}

/// Preserve the direction and progress semantics of a failed write. Capacity
/// variants are emitted only when the platform reports the corresponding
/// concrete condition; unknown failures remain typed `WriteFailure`.
fn map_filesystem_write_error_v1(error: &std::io::Error) -> FsCasErrorV1 {
    #[cfg(unix)]
    match error.raw_os_error() {
        Some(libc::ENOSPC) => {
            return FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::NoSpace);
        }
        Some(libc::EDQUOT) => {
            return FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::Quota);
        }
        Some(libc::EACCES) | Some(libc::EPERM) => {
            return FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied);
        }
        _ => {}
    }
    match error.kind() {
        ErrorKind::PermissionDenied => {
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied)
        }
        ErrorKind::WriteZero => FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ShortWrite),
        _ => FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure),
    }
}

/// Map a write-side namespace transition whose source and parent names were
/// already authenticated as owned/required. An actual `NotFound` at this
/// point is loss of required custody, not an undifferentiated write failure.
fn map_required_filesystem_write_error_v1(error: &std::io::Error) -> FsCasErrorV1 {
    if error.kind() == ErrorKind::NotFound {
        FsCasErrorV1::MissingOccupant
    } else {
        map_filesystem_write_error_v1(error)
    }
}

fn write_all_controlled_v1<C>(
    file: &mut File,
    bytes: &[u8],
    boundary: FsCasFilesystemBoundaryV1,
    control: &mut C,
) -> Result<(), FsCasErrorV1>
where
    C: FsCasControlV1 + ?Sized,
{
    if let Some(error) = control.inject_filesystem_failure(boundary) {
        if error == FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ShortWrite)
            && !bytes.is_empty()
        {
            let prefix = bytes.len().div_ceil(2);
            file.write_all(&bytes[..prefix])
                .map_err(|actual| map_filesystem_write_error_v1(&actual))?;
        }
        return Err(error);
    }
    file.write_all(bytes)
        .map_err(|error| map_filesystem_write_error_v1(&error))
}

fn flush_controlled_v1<C>(
    file: &mut File,
    boundary: FsCasFilesystemBoundaryV1,
    control: &mut C,
) -> Result<(), FsCasErrorV1>
where
    C: FsCasControlV1 + ?Sized,
{
    sample_filesystem_fault_v1(control, boundary)?;
    file.flush()
        .map_err(|error| map_filesystem_write_error_v1(&error))
}

fn read_sealed_shape(pack: &mut FilePackReadV1) -> Result<SealedPackV1, FsCasErrorV1> {
    let len = pack.len;
    if len < 144 {
        return Err(FsCasErrorV1::Integrity);
    }
    let mut header = [0_u8; 64];
    checked_file_read(&mut pack.file, len, 0, &mut header)?;
    let record_count = u32::from_be_bytes(
        header[48..52]
            .try_into()
            .map_err(|_| FsCasErrorV1::Integrity)?,
    );
    let index_offset = u64::from_be_bytes(
        header[56..64]
            .try_into()
            .map_err(|_| FsCasErrorV1::Integrity)?,
    );
    let mut digest = [0_u8; 32];
    checked_file_read(&mut pack.file, len, len - 32, &mut digest)?;
    Ok(SealedPackV1::from_validated_parts(
        PackIdV1::from_digest(digest),
        len,
        record_count,
        index_offset,
    ))
}

fn read_catalog_marker(path: &Path) -> Result<SealedPackV1, FsCasErrorV1> {
    let bytes = read_exact_regular_file_if_present::<CATALOG_MARKER_BYTES>(path)
        .map_err(|error| match error {
            FsCasErrorV1::Integrity => FsCasErrorV1::MalformedOccupant,
            other => other,
        })?
        .ok_or(FsCasErrorV1::MissingOccupant)?;
    decode_catalog_marker(bytes).map_err(|error| match error {
        FsCasErrorV1::Integrity => FsCasErrorV1::MalformedOccupant,
        other => other,
    })
}

fn classify_catalog_incumbent_v1(
    incumbent: SealedPackV1,
    expected: SealedPackV1,
) -> Result<(), FsCasErrorV1> {
    if incumbent.id() != expected.id() {
        return Err(FsCasErrorV1::Integrity);
    }
    (incumbent == expected)
        .then_some(())
        .ok_or(FsCasErrorV1::UnequalOccupant)
}

fn read_object_locator(
    path: &Path,
    expected: TypedPhysicalObjectIdV1,
) -> Result<PersistentObjectLocatorV1, FsCasErrorV1> {
    read_object_locator_if_present(path, expected)?.ok_or(FsCasErrorV1::MissingOccupant)
}

fn read_object_locator_if_present(
    path: &Path,
    expected: TypedPhysicalObjectIdV1,
) -> Result<Option<PersistentObjectLocatorV1>, FsCasErrorV1> {
    let Some(bytes) = read_exact_regular_file_if_present::<PERSISTENT_LOCATOR_BYTES_V1>(path)
        .map_err(|error| match error {
            FsCasErrorV1::Integrity => FsCasErrorV1::MalformedOccupant,
            other => other,
        })?
    else {
        return Ok(None);
    };
    decode_persistent_locator_v1(bytes, expected)
        .map(Some)
        .map_err(|error| match error {
            PersistentLocatorCodecErrorV1::Malformed => FsCasErrorV1::MalformedOccupant,
            PersistentLocatorCodecErrorV1::BindingMismatch => FsCasErrorV1::Integrity,
        })
}

fn encode_root_owner(generation: [u8; 32], state: u8) -> [u8; ROOT_OWNER_BYTES] {
    let mut bytes = [0_u8; ROOT_OWNER_BYTES];
    bytes[..8].copy_from_slice(ROOT_OWNER_MAGIC);
    bytes[8] = state;
    bytes[12..16].copy_from_slice(&std::process::id().to_be_bytes());
    bytes[16..].copy_from_slice(&generation);
    bytes
}

fn decode_existing_root_owner(
    path: &Path,
    generation: [u8; 32],
) -> Result<FsCasErrorV1, FsCasErrorV1> {
    let bytes = read_exact_regular_file::<ROOT_OWNER_BYTES>(path).map_err(|error| {
        if error == FsCasErrorV1::Integrity {
            FsCasErrorV1::MalformedOccupant
        } else {
            error
        }
    })?;
    if bytes[..8] != *ROOT_OWNER_MAGIC || bytes[9..12] != [0_u8; 3] {
        return Err(FsCasErrorV1::MalformedOccupant);
    }
    if bytes[16..] != generation {
        return Err(FsCasErrorV1::Integrity);
    }
    match bytes[8] {
        ROOT_OWNER_STATE_ACTIVE => Ok(FsCasErrorV1::Busy),
        ROOT_OWNER_STATE_INVALIDATED => Ok(FsCasErrorV1::Invalidated),
        _ => Err(FsCasErrorV1::MalformedOccupant),
    }
}

fn acquire_root_ownership(root: &Path, generation: [u8; 32]) -> Result<File, FsCasErrorV1> {
    let path = root.join(ROOT_OWNER_NAME);
    let mut file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            return Err(decode_existing_root_owner(&path, generation)?);
        }
        Err(error) => return Err(map_required_filesystem_write_error_v1(&error)),
    };
    let initialize = (|| {
        set_private_file_permissions(&path)?;
        let bytes = encode_root_owner(generation, ROOT_OWNER_STATE_ACTIVE);
        file.write_all(&bytes)
            .map_err(|error| map_filesystem_write_error_v1(&error))?;
        file.flush()
            .map_err(|error| map_filesystem_write_error_v1(&error))?;
        file.seek(SeekFrom::Start(0))
            .map_err(|error| map_filesystem_read_error_v1(&error))?;
        let mut observed = [0_u8; ROOT_OWNER_BYTES];
        file.read_exact(&mut observed)
            .map_err(|error| map_filesystem_read_error_v1(&error))?;
        if observed != bytes {
            return Err(FsCasErrorV1::Integrity);
        }
        Ok(())
    })();
    if let Err(error) = initialize {
        drop(file);
        // If cleanup fails, the malformed/partial token itself remains a
        // permanent fail-closed barrier for later openers, and that cleanup
        // failure is retained alongside the initialization cause.
        return Err(root_initialization_cleanup_result_v1(
            error,
            fs::remove_file(&path),
        ));
    }
    Ok(file)
}

fn root_initialization_cleanup_result_v1(
    original: FsCasErrorV1,
    cleanup: std::io::Result<()>,
) -> FsCasErrorV1 {
    match cleanup {
        Ok(()) => original,
        Err(error) if error.kind() == ErrorKind::NotFound => original,
        Err(_) => original.dominated_by_v1(FsCasErrorV1::CleanupFailed(
            FsCasCleanupTargetV1::RootInitialization,
        )),
    }
}

fn derive_generation(root: &Path) -> Result<[u8; 32], FsCasErrorV1> {
    // A host clock before the Unix epoch is not filesystem I/O and must not
    // be flattened into a generic I/O failure.  Both sides of the epoch are
    // valid generation entropy; the direction byte keeps their encodings
    // disjoint while the process id and monotonic process-local sequence keep
    // separate root generations distinct within this owner process.
    let (epoch_direction, elapsed_nanos) = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(elapsed) => (0_u8, elapsed.as_nanos()),
        Err(error) => (1_u8, error.duration().as_nanos()),
    };
    let sequence = NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed);
    let root_bytes = root.as_os_str().as_encoded_bytes();
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"LAYERFS-FSCAS-GENERATION-V1\0");
    hasher.update(
        &u64::try_from(root_bytes.len())
            .map_err(|_| FsCasErrorV1::Integrity)?
            .to_be_bytes(),
    );
    hasher.update(root_bytes);
    hasher.update(&std::process::id().to_be_bytes());
    hasher.update(&[epoch_direction]);
    hasher.update(&elapsed_nanos.to_be_bytes());
    hasher.update(&sequence.to_be_bytes());
    Ok(*hasher.finalize().as_bytes())
}

fn encode_generation_marker(generation: [u8; 32]) -> [u8; GENERATION_MARKER_BYTES] {
    let mut bytes = [0_u8; GENERATION_MARKER_BYTES];
    bytes[..8].copy_from_slice(GENERATION_MAGIC);
    bytes[8..].copy_from_slice(&generation);
    bytes
}

fn read_generation_marker(path: &Path) -> Result<[u8; 32], FsCasErrorV1> {
    let bytes = read_exact_regular_file::<GENERATION_MARKER_BYTES>(path).map_err(|error| {
        if error == FsCasErrorV1::Integrity {
            FsCasErrorV1::MalformedOccupant
        } else {
            error
        }
    })?;
    if bytes[..8] != *GENERATION_MAGIC {
        return Err(FsCasErrorV1::MalformedOccupant);
    }
    <[u8; 32]>::try_from(&bytes[8..]).map_err(|_| FsCasErrorV1::MalformedOccupant)
}

fn encode_closure_marker(
    version: TypedPhysicalObjectIdV1,
    object_count: u64,
    generation: [u8; 32],
    transcript: [u8; 32],
) -> [u8; CLOSURE_MARKER_BYTES] {
    let mut bytes = [0_u8; CLOSURE_MARKER_BYTES];
    bytes[..8].copy_from_slice(CLOSURE_MAGIC);
    bytes[8] = typed_kind_byte(version);
    bytes[16..48].copy_from_slice(version.as_bytes());
    bytes[48..56].copy_from_slice(&object_count.to_be_bytes());
    bytes[56..88].copy_from_slice(&generation);
    bytes[88..120].copy_from_slice(&transcript);
    bytes
}

fn decode_closure_marker_v1(
    bytes: [u8; CLOSURE_MARKER_BYTES],
    expected_version: PhysicalVersionRecordIdV1,
    expected_generation: [u8; 32],
) -> Result<FsCasAcceptedClosureReadV1, FsCasErrorV1> {
    let typed = TypedPhysicalObjectIdV1::VersionRecord(expected_version);
    if &bytes[..8] != CLOSURE_MAGIC
        || bytes[8] != typed_kind_byte(typed)
        || bytes[9..16].iter().any(|byte| *byte != 0)
    {
        return Err(FsCasErrorV1::MalformedOccupant);
    }
    // Structurally valid bytes bound to another requested identity or owner
    // generation are authentication failures, not malformed I/O results.
    if bytes[16..48] != expected_version.as_bytes()[..] || bytes[56..88] != expected_generation[..]
    {
        return Err(FsCasErrorV1::Integrity);
    }
    let object_count = u64::from_be_bytes(
        bytes[48..56]
            .try_into()
            .map_err(|_| FsCasErrorV1::MalformedOccupant)?,
    );
    if object_count == 0 {
        return Err(FsCasErrorV1::MalformedOccupant);
    }
    let transcript = bytes[88..120]
        .try_into()
        .map_err(|_| FsCasErrorV1::MalformedOccupant)?;
    Ok(FsCasAcceptedClosureReadV1 {
        version_record: expected_version,
        object_count,
        transcript,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MarkerPublicationV1<const N: usize> {
    VisibleClean,
    /// The destination is authoritative, but its private preparation alias or
    /// logical preparation charge could not be released. `Some` retains the
    /// chronological filesystem/accounting cause; `None` denotes a synthetic
    /// cleanup refusal with no underlying filesystem error. The semantic owner
    /// must retain visible dependency custody before adding cleanup/invalidation
    /// dominance and returning the terminal error.
    VisibleWithPreparationResidue(Option<FsCasErrorV1>),
    /// The destination is authoritative and the post-link unwind path already
    /// classified explicit alias cleanup and durable invalidation. Semantic
    /// callers must retain every visible dependency, but must not retry either
    /// cleanup or invalidation or replace this chronological terminal.
    VisibleTerminal(FsCasErrorV1),
    /// The atomic no-replace link reported `AlreadyExists`. These are the
    /// exact incumbent bytes read while the root visibility lock was still
    /// held; the semantic owner must authenticate them before continuing.
    IncumbentClean([u8; N]),
    /// The no-replace transition found an incumbent, but cleanup of this
    /// operation's private marker failed before the semantic owner could
    /// authenticate those bytes.  The semantic owner must classify the
    /// incumbent first, then pair that primary error with this cleanup terminal
    /// (or return it unchanged when the incumbent is equal).
    IncumbentWithPreparationResidue([u8; N], FsCasErrorV1),
}

impl<const N: usize> MarkerPublicationV1<N> {
    fn require_clean(self) -> Result<(), FsCasErrorV1> {
        match self {
            Self::VisibleClean => Ok(()),
            Self::VisibleTerminal(error) => Err(error),
            Self::VisibleWithPreparationResidue(Some(error)) => Err(error),
            Self::VisibleWithPreparationResidue(None) => Err(FsCasErrorV1::CleanupFailed(
                FsCasCleanupTargetV1::PublishedMarkerAlias,
            )),
            Self::IncumbentClean(_) => Err(FsCasErrorV1::Integrity),
            Self::IncumbentWithPreparationResidue(_, error) => Err(error),
        }
    }
}

fn publish_small_marker<const N: usize>(
    preparation: &Path,
    prefix: &str,
    destination: &Path,
    bytes: &[u8; N],
) -> Result<MarkerPublicationV1<N>, FsCasErrorV1> {
    let mut control = ContinueFsCasControlV1;
    publish_small_marker_controlled(
        preparation,
        prefix,
        destination,
        bytes,
        None,
        None,
        None,
        None,
        None,
        &mut control,
    )
}

#[allow(clippy::too_many_arguments)]
fn publish_small_marker_controlled<const N: usize, C>(
    preparation: &Path,
    prefix: &str,
    destination: &Path,
    bytes: &[u8; N],
    visibility_owner: Option<&FsCasV1>,
    storage_token: Option<FsStorageOperationTokenV1>,
    linked_boundary: Option<FsCasBoundaryV1>,
    mut locator_custody: Option<&mut LocatorPublicationCustodyV1>,
    mut marker_custody: Option<&mut ImmutableMarkerCustodyV1>,
    control: &mut C,
) -> Result<MarkerPublicationV1<N>, FsCasErrorV1>
where
    C: FsCasControlV1 + ?Sized,
{
    validate_required_root_directory(preparation)?;
    let temporary = unique_private_path(preparation, prefix)?;
    if let (Some(owner), Some(token)) = (visibility_owner, storage_token) {
        if let Err(error) = owner.record_storage_preparation_create_v1(token) {
            return Err(owner.fail_closed_preserving_error_controlled_v1(error, control));
        }
    }
    let file = sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::MarkerCreate)
        .and_then(|()| {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|error| map_required_filesystem_write_error_v1(&error))
        });
    let mut accounted_len = 0_u64;
    let mut file = match file {
        Ok(file) => file,
        Err(error) => {
            if let (Some(owner), Some(token)) = (visibility_owner, storage_token) {
                if owner
                    .record_storage_preparation_remove_v1(token, 0)
                    .is_err()
                {
                    let cleanup = owner.cleanup_failure_controlled_v1(
                        FsCasCleanupTargetV1::PreparationSpool,
                        control,
                    );
                    return Err(error.dominated_by_v1(cleanup));
                }
            }
            return Err(error);
        }
    };
    // The raw temporary path has no fallible RAII owner. Keep its complete
    // not-yet-visible lifetime inside one unwind boundary so no control hook
    // can jump past the one explicit cleanup attempt. `None` means this call
    // installed the destination; `Some` is a terminal incumbent result whose
    // private temporary still needs cleanup below.
    let mut destination_linked = false;
    // As with the enclosing carrier publication lock, keep guard ownership
    // outside the unwind boundary. A controlled pre-link callback must not
    // poison a healthy visibility mutex merely because it unwinds while the
    // authoritative transition is serialized.
    let mut visibility_guard = None;
    let pre_link = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> Result<Option<MarkerPublicationV1<N>>, FsCasErrorV1> {
            let prepare = (|| {
                sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::PermissionChange)?;
                set_private_file_permissions(&temporary)?;
                if let (Some(owner), Some(token)) = (visibility_owner, storage_token) {
                    let next_len = u64::try_from(N).map_err(|_| FsCasErrorV1::Integrity)?;
                    if let Err(error) =
                        owner.record_storage_preparation_length_v1(token, accounted_len, next_len)
                    {
                        return Err(
                            owner.fail_closed_preserving_error_controlled_v1(error, control)
                        );
                    }
                    accounted_len = next_len;
                }
                write_all_controlled_v1(
                    &mut file,
                    bytes,
                    FsCasFilesystemBoundaryV1::MarkerWrite,
                    control,
                )?;
                flush_controlled_v1(&mut file, FsCasFilesystemBoundaryV1::MarkerFlush, control)?;
                drop(file);
                let observed = read_exact_regular_file::<N>(&temporary)?;
                if observed != *bytes {
                    return Err(FsCasErrorV1::Integrity);
                }
                sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::PermissionChange)?;
                set_read_only(&temporary)
            })();
            prepare?;

            visibility_guard = visibility_owner
                .map(|owner| owner.lock_visibility_controlled_v1(control))
                .transpose()?;
            if let Some(owner) = visibility_owner {
                owner.ensure_valid()?;
            }
            sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::MarkerHardLink)?;
            let immutable_len = match (visibility_owner, storage_token) {
                (Some(owner), Some(token)) => {
                    let len = u64::try_from(N).map_err(|_| FsCasErrorV1::Integrity)?;
                    if let Err(error) = owner.record_storage_immutable_install_v1(token, len, 1) {
                        return Err(
                            owner.fail_closed_preserving_error_controlled_v1(error, control)
                        );
                    }
                    Some((owner, token, len))
                }
                _ => None,
            };
            match fs::hard_link(&temporary, destination) {
                Ok(()) => {
                    destination_linked = true;
                    if let Some(custody) = locator_custody.as_mut() {
                        custody.mark_visible_v1();
                    }
                    if let Some(custody) = marker_custody.as_mut() {
                        custody.mark_visible_v1(N as u64);
                    }
                    drop(visibility_guard.take());
                    Ok(None)
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    if let Some((owner, token, len)) = immutable_len {
                        if let Err(error) = owner.record_storage_immutable_remove_v1(token, len, 1)
                        {
                            return Err(
                                owner.fail_closed_preserving_error_controlled_v1(error, control)
                            );
                        }
                    }
                    let incumbent = match read_exact_regular_file_if_present::<N>(destination) {
                        Ok(Some(bytes)) => Ok(bytes),
                        Ok(None) => Err(FsCasErrorV1::MissingOccupant),
                        Err(FsCasErrorV1::Integrity) => Err(FsCasErrorV1::MalformedOccupant),
                        Err(other) => Err(other),
                    };
                    drop(visibility_guard.take());
                    incumbent.map(|bytes| Some(MarkerPublicationV1::IncumbentClean(bytes)))
                }
                Err(error) => {
                    let original = if is_unsupported_link_error(&error) {
                        FsCasErrorV1::Unsupported
                    } else {
                        map_required_filesystem_write_error_v1(&error)
                    };
                    if let Some((owner, token, len)) = immutable_len {
                        let terminal = owner
                            .release_prepublication_marker_charge_preserving_error_v1(
                                token, len, control, original,
                            );
                        drop(visibility_guard.take());
                        return Err(terminal);
                    }
                    drop(visibility_guard.take());
                    Err(original)
                }
            }
        },
    ));
    drop(visibility_guard.take());
    match pre_link {
        Ok(Ok(None)) => {}
        Ok(terminal) => {
            let cleanup = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                cleanup_unpublished_marker_v1(
                    &temporary,
                    visibility_owner,
                    storage_token,
                    accounted_len,
                    FsCasCleanupTargetV1::PreparationSpool,
                    control,
                )
            }));
            return match cleanup {
                Ok(Ok(())) => match terminal {
                    Ok(Some(publication)) => Ok(publication),
                    Ok(None) => unreachable!("fresh marker publication already returned"),
                    Err(error) => Err(error),
                },
                Ok(Err(cleanup)) => match terminal {
                    Err(original) => Err(original.dominated_by_v1(cleanup)),
                    Ok(Some(MarkerPublicationV1::IncumbentClean(bytes))) => Ok(
                        MarkerPublicationV1::IncumbentWithPreparationResidue(bytes, cleanup),
                    ),
                    Ok(Some(_)) => unreachable!("only an incumbent may need pre-link cleanup"),
                    Ok(None) => unreachable!("fresh marker publication already returned"),
                },
                Err(_) => {
                    let cleanup = visibility_owner.map_or(
                        FsCasErrorV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool),
                        |owner| {
                            owner.cleanup_failure_after_unwind_v1(
                                FsCasCleanupTargetV1::PreparationSpool,
                                control,
                            )
                        },
                    );
                    match terminal {
                        Err(original) => Err(original.dominated_by_v1(cleanup)),
                        Ok(Some(MarkerPublicationV1::IncumbentClean(bytes))) => Ok(
                            MarkerPublicationV1::IncumbentWithPreparationResidue(bytes, cleanup),
                        ),
                        Ok(Some(_)) => {
                            unreachable!("only an incumbent may need pre-link cleanup")
                        }
                        Ok(None) => unreachable!("fresh marker publication already returned"),
                    }
                }
            };
        }
        Err(payload) => {
            if !destination_linked {
                let cleanup = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    cleanup_unpublished_marker_v1(
                        &temporary,
                        visibility_owner,
                        storage_token,
                        accounted_len,
                        FsCasCleanupTargetV1::PreparationSpool,
                        control,
                    )
                }));
                match cleanup {
                    Ok(Ok(())) => std::panic::resume_unwind(payload),
                    Ok(Err(terminal)) => return Err(terminal),
                    Err(_) => {
                        return Err(visibility_owner.map_or(
                            FsCasErrorV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool),
                            |owner| {
                                owner.cleanup_failure_after_unwind_v1(
                                    FsCasCleanupTargetV1::PreparationSpool,
                                    control,
                                )
                            },
                        ));
                    }
                }
            }
            if destination_linked {
                if let Some(owner) = visibility_owner {
                    owner.invalidate_root_backstop_v1();
                }
            }
            std::panic::resume_unwind(payload)
        }
    }

    // From this point onward `destination` is the visibility authority. A
    // failure to remove its private hard-link alias is cleanup residue, not a
    // failed publication, and callers must retain every dependency beneath
    // the visible marker. Keep the post-link callback and alias cleanup in
    // one unwind-aware ownership transition: an unwind before cleanup gets
    // exactly one explicit cleanup attempt, while an unwind from the cleanup
    // callback itself is retained and is never retried into apparent success.
    let mut alias_cleanup_attempted = false;
    let post_link = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if let Some(boundary) = linked_boundary {
            control.boundary_reached(boundary);
        }
        alias_cleanup_attempted = true;
        let injected = linked_boundary.is_some()
            && control.inject_cleanup_failure(FsCasCleanupTargetV1::PublishedMarkerAlias);
        let unlink_error = if injected {
            None
        } else {
            sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::MarkerAliasUnlink)
                .and_then(|()| {
                    fs::remove_file(&temporary)
                        .map_err(|error| map_required_filesystem_write_error_v1(&error))
                })
                .err()
        };
        if injected || unlink_error.is_some() {
            Ok(MarkerPublicationV1::VisibleWithPreparationResidue(
                unlink_error,
            ))
        } else {
            if let (Some(owner), Some(token)) = (visibility_owner, storage_token) {
                if let Err(error) = owner.record_storage_preparation_remove_v1(token, accounted_len)
                {
                    return Ok(MarkerPublicationV1::VisibleWithPreparationResidue(Some(
                        error,
                    )));
                }
            }
            Ok(MarkerPublicationV1::VisibleClean)
        }
    }));
    match post_link {
        Ok(publication) => publication,
        Err(payload) => {
            let terminal = if !alias_cleanup_attempted {
                // The linked-boundary observer unwound before alias cleanup
                // began. Attempt that owned cleanup once while the storage
                // capability is still live. A second unwind or typed cleanup
                // failure leaves the exact alias charged as residue.
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    cleanup_unpublished_marker_v1(
                        &temporary,
                        visibility_owner,
                        storage_token,
                        accounted_len,
                        FsCasCleanupTargetV1::PublishedMarkerAlias,
                        control,
                    )
                })) {
                    Ok(Ok(())) => visibility_owner
                        .map(|owner| owner.invalidate_root_after_unwind_v1(control))
                        .unwrap_or(Ok(())),
                    Ok(Err(terminal)) => Err(terminal),
                    Err(_) => Err(visibility_owner.map_or(
                        FsCasErrorV1::CleanupFailed(FsCasCleanupTargetV1::PublishedMarkerAlias),
                        |owner| {
                            owner.cleanup_failure_after_unwind_v1(
                                FsCasCleanupTargetV1::PublishedMarkerAlias,
                                control,
                            )
                        },
                    )),
                }
            } else {
                // The unwind originated inside the alias-cleanup transition.
                // Its target is already known and retrying the unlink could
                // reinterpret retained residue as success.
                Err(visibility_owner.map_or(
                    FsCasErrorV1::CleanupFailed(FsCasCleanupTargetV1::PublishedMarkerAlias),
                    |owner| {
                        owner.cleanup_failure_after_unwind_v1(
                            FsCasCleanupTargetV1::PublishedMarkerAlias,
                            control,
                        )
                    },
                ))
            };

            match terminal {
                // Publication became visible but its semantic owner did not
                // receive the result. The initiating payload may leave this
                // transaction only after both alias cleanup and fail-closed
                // invalidation are synchronously verified.
                Ok(()) => std::panic::resume_unwind(payload),
                Err(error) => Ok(MarkerPublicationV1::VisibleTerminal(error)),
            }
        }
    }
}

fn cleanup_unpublished_marker_v1<C>(
    temporary: &Path,
    owner: Option<&FsCasV1>,
    storage_token: Option<FsStorageOperationTokenV1>,
    accounted_len: u64,
    target: FsCasCleanupTargetV1,
    control: &mut C,
) -> Result<(), FsCasErrorV1>
where
    C: FsCasControlV1 + ?Sized,
{
    let mut observed_len = accounted_len;
    if let (Some(owner), Some(token)) = (owner, storage_token) {
        match fs::symlink_metadata(temporary) {
            Ok(metadata) if metadata.file_type().is_file() => {
                observed_len = metadata.len();
                if observed_len != accounted_len {
                    if let Err(accounting) = owner.record_storage_preparation_length_v1(
                        token,
                        accounted_len,
                        observed_len,
                    ) {
                        // The private marker remains owned and charged, but
                        // its exact physical length could not be reconciled
                        // with the root ledger. Do not unlink it and later
                        // reinterpret the failed reconciliation as success.
                        // Retain the typed accounting cause and make this a
                        // stable explicit cleanup terminal; a failed durable
                        // invalidation is the only stronger terminal cause.
                        let cleanup = owner.cleanup_failure_controlled_v1(target, control);
                        return Err(accounting.dominated_by_v1(cleanup));
                    }
                }
            }
            Ok(_) => {
                let first = FsCasErrorV1::Integrity;
                let cleanup = owner.cleanup_failure_controlled_v1(target, control);
                return Err(first.dominated_by_v1(cleanup));
            }
            Err(error) => {
                let first = map_required_filesystem_read_error_v1(&error);
                let cleanup = owner.cleanup_failure_controlled_v1(target, control);
                return Err(first.dominated_by_v1(cleanup));
            }
        }
    }
    let injected = control.inject_cleanup_failure(target);
    let mut unlink_error = None;
    let removed = if injected {
        false
    } else {
        match fs::remove_file(temporary) {
            Ok(()) => true,
            Err(error) if error.kind() == ErrorKind::NotFound => true,
            Err(error) => {
                unlink_error = Some(map_required_filesystem_write_error_v1(&error));
                false
            }
        }
    };
    if removed {
        if let (Some(owner), Some(token)) = (owner, storage_token) {
            if let Err(first) = owner.record_storage_preparation_remove_v1(token, observed_len) {
                let cleanup = owner.cleanup_failure_controlled_v1(target, control);
                return Err(first.dominated_by_v1(cleanup));
            }
        }
        Ok(())
    } else if let Some(owner) = owner {
        let cleanup = owner.cleanup_failure_controlled_v1(target, control);
        Err(unlink_error.map_or(cleanup, |first| first.dominated_by_v1(cleanup)))
    } else {
        let cleanup = FsCasErrorV1::CleanupFailed(target);
        Err(unlink_error.map_or(cleanup, |first| first.dominated_by_v1(cleanup)))
    }
}

fn read_exact_regular_file<const N: usize>(path: &Path) -> Result<[u8; N], FsCasErrorV1> {
    let mut file = open_regular_file(path)?;
    read_exact_regular_file_from_open_v1(&mut file)
}

fn read_exact_regular_file_if_present<const N: usize>(
    path: &Path,
) -> Result<Option<[u8; N]>, FsCasErrorV1> {
    let Some(mut file) = open_regular_file_if_present(path)? else {
        return Ok(None);
    };
    read_exact_regular_file_from_open_v1(&mut file).map(Some)
}

fn read_exact_regular_file_from_open_v1<const N: usize>(
    file: &mut File,
) -> Result<[u8; N], FsCasErrorV1> {
    let metadata = file
        .metadata()
        .map_err(|error| map_filesystem_read_error_v1(&error))?;
    if metadata.len() != N as u64 {
        return Err(FsCasErrorV1::Integrity);
    }
    read_exact_regular_file_after_metadata_v1(file)
}

/// Complete the actual payload read after a successful authenticated metadata
/// observation. Keeping this boundary explicit makes a post-metadata
/// truncation an honest `ShortRead`, rather than reclassifying the failed I/O
/// as a malformed byte sequence that was never fully read.
fn read_exact_regular_file_after_metadata_v1<const N: usize>(
    file: &mut File,
) -> Result<[u8; N], FsCasErrorV1> {
    let mut bytes = [0_u8; N];
    file.read_exact(&mut bytes)
        .map_err(|error| map_filesystem_read_error_v1(&error))?;
    let mut trailing = [0_u8; 1];
    if file
        .read(&mut trailing)
        .map_err(|error| map_filesystem_read_error_v1(&error))?
        != 0
    {
        return Err(FsCasErrorV1::Integrity);
    }
    Ok(bytes)
}

fn checked_file_read(
    file: &mut File,
    len: u64,
    offset: u64,
    destination: &mut [u8],
) -> Result<(), FsCasErrorV1> {
    let end = offset
        .checked_add(u64::try_from(destination.len()).map_err(|_| FsCasErrorV1::Integrity)?)
        .ok_or(FsCasErrorV1::Integrity)?;
    if end > len
        || file
            .metadata()
            .map_err(|error| map_filesystem_read_error_v1(&error))?
            .len()
            != len
    {
        return Err(FsCasErrorV1::Integrity);
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| map_filesystem_read_error_v1(&error))?;
    file.read_exact(destination)
        .map_err(|error| map_filesystem_read_error_v1(&error))
}

fn open_regular_file(path: &Path) -> Result<File, FsCasErrorV1> {
    open_regular_file_if_present(path)?.ok_or(FsCasErrorV1::MissingOccupant)
}

/// Open and authenticate one regular-file name, classifying only an actual
/// `NotFound` from the open itself as vacancy. Permission, metadata, and other
/// I/O failures stay errors; they are never converted to a missing occupant.
fn open_regular_file_if_present(path: &Path) -> Result<Option<File>, FsCasErrorV1> {
    open_regular_file_if_present_impl_v1(path, || {})
}

#[cfg(test)]
fn open_regular_file_if_present_with_post_open_hook_v1<F>(
    path: &Path,
    post_open: F,
) -> Result<Option<File>, FsCasErrorV1>
where
    F: FnOnce(),
{
    open_regular_file_if_present_impl_v1(path, post_open)
}

fn open_regular_file_if_present_impl_v1<F>(
    path: &Path,
    post_open: F,
) -> Result<Option<File>, FsCasErrorV1>
where
    F: FnOnce(),
{
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            // A dangling symlink also makes `File::open` report `NotFound`,
            // but the namespace name is occupied and must not be treated as
            // a vacancy. Authenticate absence with the fallible name lookup;
            // only its own explicit `NotFound` is a missing occupant.
            return match fs::symlink_metadata(path) {
                Err(lookup) if lookup.kind() == ErrorKind::NotFound => Ok(None),
                Ok(_) => Err(FsCasErrorV1::Integrity),
                Err(error) => Err(map_filesystem_read_error_v1(&error)),
            };
        }
        Err(error) => return Err(map_filesystem_read_error_v1(&error)),
    };
    post_open();
    let before = fs::symlink_metadata(path)
        .map_err(|error| map_required_filesystem_read_error_v1(&error))?;
    if !before.file_type().is_file() || before.file_type().is_symlink() {
        return Err(FsCasErrorV1::Integrity);
    }
    let after = file
        .metadata()
        .map_err(|error| map_filesystem_read_error_v1(&error))?;
    if !same_file_identity(&before, &after) {
        return Err(FsCasErrorV1::Integrity);
    }
    Ok(Some(file))
}

#[cfg(unix)]
fn same_file_identity(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(not(unix))]
fn same_file_identity(_left: &fs::Metadata, _right: &fs::Metadata) -> bool {
    false
}

#[cfg(test)]
mod resource_concurrency_tests {
    use super::*;

    fn usage(bytes: u64, inodes: u64) -> RootStorageUsageV1 {
        RootStorageUsageV1 { bytes, inodes }
    }

    fn assert_open_existing_error_v1(
        result: Result<FsCasV1, FsCasErrorV1>,
        expected: FsCasErrorV1,
    ) {
        match result {
            Err(actual) => assert_eq!(actual, expected),
            Ok(cas) => {
                drop(cas);
                panic!("open_existing unexpectedly succeeded");
            }
        }
    }

    /// A single semantic reopen-read fault.  These boundaries deliberately
    /// model LayerFS authority reads, not native syscall counts.
    struct OpenExistingReadFaultControlV1 {
        boundary: FsCasFilesystemBoundaryV1,
        error: FsCasErrorV1,
        injected: bool,
    }

    impl FsCasControlV1 for OpenExistingReadFaultControlV1 {
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

    #[test]
    fn root_namespace_usage_splits_logical_bytes_from_named_entry_exhaustion() {
        // Exact budget boundaries remain admissible.
        let mut exact = usage(
            ROOT_LOGICAL_STORAGE_BUDGET_V1 - 1,
            ROOT_NAMESPACE_ENTRY_BUDGET_V1 - 1,
        );
        record_root_namespace_entry_usage_v1(&mut exact, 1).unwrap();
        assert_eq!(
            exact,
            usage(
                ROOT_LOGICAL_STORAGE_BUDGET_V1,
                ROOT_NAMESPACE_ENTRY_BUDGET_V1
            )
        );

        // Logical bytes retain the historical precedence if both independent
        // envelopes are exceeded; a name-only refusal has its own resource
        // type despite the frozen `StorageInodes` compatibility spelling.
        let mut bytes_only = usage(ROOT_LOGICAL_STORAGE_BUDGET_V1, 0);
        assert_eq!(
            record_root_namespace_entry_usage_v1(&mut bytes_only, 1),
            Err(FsCasErrorV1::ResourceExhausted(
                FsCasResourceV1::StorageBytes
            ))
        );
        assert_eq!(bytes_only, usage(ROOT_LOGICAL_STORAGE_BUDGET_V1, 0));

        let mut entries_only = usage(0, ROOT_NAMESPACE_ENTRY_BUDGET_V1);
        assert_eq!(
            record_root_namespace_entry_usage_v1(&mut entries_only, 0),
            Err(FsCasErrorV1::ResourceExhausted(
                FsCasResourceV1::StorageInodes
            ))
        );
        assert_eq!(entries_only, usage(0, ROOT_NAMESPACE_ENTRY_BUDGET_V1));

        let mut both = usage(
            ROOT_LOGICAL_STORAGE_BUDGET_V1,
            ROOT_NAMESPACE_ENTRY_BUDGET_V1,
        );
        assert_eq!(
            record_root_namespace_entry_usage_v1(&mut both, 1),
            Err(FsCasErrorV1::ResourceExhausted(
                FsCasResourceV1::StorageBytes
            ))
        );
        assert_eq!(
            both,
            usage(
                ROOT_LOGICAL_STORAGE_BUDGET_V1,
                ROOT_NAMESPACE_ENTRY_BUDGET_V1
            )
        );

        for (mut overflow, logical_bytes) in [(usage(0, u64::MAX), 0), (usage(u64::MAX, 0), 1)] {
            let before = overflow;
            assert_eq!(
                record_root_namespace_entry_usage_v1(&mut overflow, logical_bytes),
                Err(FsCasErrorV1::Core(CoreError::IntegerOverflow))
            );
            assert_eq!(overflow, before);
        }
    }

    #[test]
    fn open_existing_root_and_generation_read_failures_are_exact_and_leave_no_owner() {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let faults = [
            FsCasErrorV1::MissingOccupant,
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ReadFailure),
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ShortRead),
        ];

        for boundary in [
            FsCasFilesystemBoundaryV1::RootValidationRead,
            FsCasFilesystemBoundaryV1::GenerationMarkerRead,
        ] {
            for error in faults {
                let root = parent.join(format!(
                    "layerfs-open-existing-read-fault-{boundary:?}-{error:?}-{}-{}",
                    std::process::id(),
                    NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
                ));
                let cas = FsCasV1::create_new(&root).unwrap();
                drop(cas);

                let mut control = OpenExistingReadFaultControlV1 {
                    boundary,
                    error,
                    injected: false,
                };
                assert_open_existing_error_v1(
                    FsCasV1::open_existing_controlled_inner_v1(&root, &mut control),
                    error,
                );
                assert!(control.injected);

                // Both read boundaries precede owner acquisition.  An exact
                // typed failure must not strand a Busy owner record.
                let reopened = FsCasV1::open_existing(&root).unwrap();
                drop(reopened);
                fs::remove_dir_all(root).unwrap();
            }
        }

        let missing_root = parent.join(format!(
            "layerfs-open-existing-missing-root-{}-{}",
            std::process::id(),
            NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
        ));
        assert_open_existing_error_v1(
            FsCasV1::open_existing(&missing_root),
            FsCasErrorV1::MissingOccupant,
        );

        let missing_generation = parent.join(format!(
            "layerfs-open-existing-missing-generation-{}-{}",
            std::process::id(),
            NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
        ));
        let cas = FsCasV1::create_new(&missing_generation).unwrap();
        drop(cas);
        fs::remove_file(missing_generation.join("generation")).unwrap();
        assert_open_existing_error_v1(
            FsCasV1::open_existing(&missing_generation),
            FsCasErrorV1::MissingOccupant,
        );
        fs::remove_dir_all(missing_generation).unwrap();
    }

    #[test]
    fn locator_custody_keeps_carrier_dependency_after_residue_counter_failure() {
        let mut custody = LocatorPublicationCustodyV1::default();
        custody.mark_visible_v1();
        let mut counters = OperationCountersV1 {
            unreachable_installed_residue_bytes: u64::MAX,
            ..OperationCountersV1::default()
        };

        assert_eq!(
            custody.retain_one_v1(&mut counters),
            Err(FsCasErrorV1::Core(CoreError::IntegerOverflow))
        );
        assert_eq!(custody.live_unclassified, 1);
        assert_eq!(custody.retained_and_recorded, 0);
        assert!(custody.requires_carrier_retention_v1());
        assert_eq!(counters.unreachable_installed_residue_bytes, u64::MAX);

        assert_eq!(
            custody.retain_all_live_v1(&mut counters),
            Err(FsCasErrorV1::Core(CoreError::IntegerOverflow))
        );
        assert_eq!(custody.live_unclassified, 1);
        assert_eq!(custody.retained_and_recorded, 0);
        assert!(custody.requires_carrier_retention_v1());
        assert_eq!(counters.unreachable_installed_residue_bytes, u64::MAX);
    }

    #[test]
    fn locator_custody_rejects_removal_without_visible_ownership() {
        let mut custody = LocatorPublicationCustodyV1::default();
        assert_eq!(custody.mark_removed_v1(), Err(FsCasErrorV1::Integrity));
        assert_eq!(custody.live_unclassified, 0);
        assert_eq!(custody.retained_and_recorded, 0);
        assert!(!custody.requires_carrier_retention_v1());
    }

    #[cfg(unix)]
    #[derive(Clone, Copy)]
    enum CarrierRollbackFaultModeV1 {
        SampledUnsupported,
        SampledWriteFailure,
        PermissionDenied,
        WriteFailure,
        InjectedCleanup,
    }

    #[cfg(unix)]
    struct CarrierRollbackFaultControlV1 {
        carriers: PathBuf,
        held_carriers: PathBuf,
        mode: CarrierRollbackFaultModeV1,
        fail_invalidation: bool,
        restored: bool,
    }

    #[cfg(unix)]
    impl CarrierRollbackFaultControlV1 {
        fn restore_filesystem_authority_v1(&mut self) {
            if self.restored {
                return;
            }
            match self.mode {
                CarrierRollbackFaultModeV1::PermissionDenied => {
                    use std::os::unix::fs::PermissionsExt;

                    fs::set_permissions(&self.carriers, fs::Permissions::from_mode(0o700)).unwrap();
                }
                CarrierRollbackFaultModeV1::WriteFailure => {
                    fs::remove_file(&self.carriers).unwrap();
                    fs::rename(&self.held_carriers, &self.carriers).unwrap();
                }
                CarrierRollbackFaultModeV1::SampledUnsupported
                | CarrierRollbackFaultModeV1::SampledWriteFailure
                | CarrierRollbackFaultModeV1::InjectedCleanup => {}
            }
            self.restored = true;
        }
    }

    #[cfg(unix)]
    impl FsCasControlV1 for CarrierRollbackFaultControlV1 {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::Carrier
                && matches!(self.mode, CarrierRollbackFaultModeV1::InjectedCleanup)
            {
                return true;
            }
            if target == FsCasCleanupTargetV1::RootInvalidation {
                self.restore_filesystem_authority_v1();
                return self.fail_invalidation;
            }
            false
        }

        fn inject_filesystem_failure(
            &mut self,
            boundary: FsCasFilesystemBoundaryV1,
        ) -> Option<FsCasErrorV1> {
            if boundary != FsCasFilesystemBoundaryV1::CarrierUnlink {
                return None;
            }
            match self.mode {
                CarrierRollbackFaultModeV1::SampledUnsupported => Some(FsCasErrorV1::Unsupported),
                CarrierRollbackFaultModeV1::SampledWriteFailure => Some(FsCasErrorV1::Filesystem(
                    FsCasFilesystemFailureV1::WriteFailure,
                )),
                CarrierRollbackFaultModeV1::PermissionDenied
                | CarrierRollbackFaultModeV1::WriteFailure
                | CarrierRollbackFaultModeV1::InjectedCleanup => None,
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn carrier_rollback_preserves_directional_unlink_cause_and_exact_custody() {
        use std::os::unix::fs::PermissionsExt;

        const CARRIER_BYTES: u64 = 113;
        for (fault_name, mode, first) in [
            (
                "sampled-unsupported",
                CarrierRollbackFaultModeV1::SampledUnsupported,
                Some(FsCasFailureCauseV1::Unsupported),
            ),
            (
                "sampled-write-failure",
                CarrierRollbackFaultModeV1::SampledWriteFailure,
                Some(FsCasFailureCauseV1::Filesystem(
                    FsCasFilesystemFailureV1::WriteFailure,
                )),
            ),
            (
                "permission",
                CarrierRollbackFaultModeV1::PermissionDenied,
                Some(FsCasFailureCauseV1::Filesystem(
                    FsCasFilesystemFailureV1::PermissionDenied,
                )),
            ),
            (
                "write-failure",
                CarrierRollbackFaultModeV1::WriteFailure,
                Some(FsCasFailureCauseV1::Filesystem(
                    FsCasFilesystemFailureV1::WriteFailure,
                )),
            ),
            (
                "injected-cleanup",
                CarrierRollbackFaultModeV1::InjectedCleanup,
                None,
            ),
        ] {
            for fail_invalidation in [false, true] {
                let parent = std::env::temp_dir().canonicalize().unwrap();
                let root = parent.join(format!(
                    "layerfs-carrier-rollback-{fault_name}-{}-{}",
                    std::process::id(),
                    NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
                ));
                let cas = FsCasV1::create_new(&root).unwrap();
                let carriers = root.join("carriers");
                let carrier = carriers.join("direct-rollback-carrier");
                fs::write(&carrier, vec![0x5a; CARRIER_BYTES as usize]).unwrap();
                let held_carriers = root.join("carriers-held-for-rollback");
                match mode {
                    CarrierRollbackFaultModeV1::PermissionDenied => {
                        fs::set_permissions(&carriers, fs::Permissions::from_mode(0o500)).unwrap();
                    }
                    CarrierRollbackFaultModeV1::WriteFailure => {
                        fs::rename(&carriers, &held_carriers).unwrap();
                        fs::write(&carriers, b"not-a-directory").unwrap();
                    }
                    CarrierRollbackFaultModeV1::SampledUnsupported
                    | CarrierRollbackFaultModeV1::SampledWriteFailure
                    | CarrierRollbackFaultModeV1::InjectedCleanup => {}
                }
                let mut control = CarrierRollbackFaultControlV1 {
                    carriers: carriers.clone(),
                    held_carriers,
                    mode,
                    fail_invalidation,
                    restored: false,
                };
                let sealed = SealedPackV1::from_validated_parts(
                    PackIdV1::from_digest([0x6b; 32]),
                    CARRIER_BYTES,
                    1,
                    64,
                );
                let mut counters = OperationCountersV1::default();
                let mut custody = CarrierPublicationCustodyV1::InstalledUnreported;
                let cleanup = FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::Carrier);
                let expected = if fail_invalidation {
                    FsCasErrorV1::TerminalFailure {
                        first: first.unwrap_or(cleanup),
                        dominant: FsCasFailureCauseV1::InvalidationFailed,
                    }
                } else if let Some(first) = first {
                    FsCasErrorV1::TerminalFailure {
                        first,
                        dominant: cleanup,
                    }
                } else {
                    FsCasErrorV1::CleanupFailed(FsCasCleanupTargetV1::Carrier)
                };

                assert_eq!(
                    cas.rollback_unpublished_carrier(
                        &carrier,
                        sealed,
                        None,
                        &mut counters,
                        &mut custody,
                        &mut control,
                    ),
                    Err(expected),
                    "{fault_name}, invalidation double fault={fail_invalidation}"
                );
                assert!(control.restored);
                assert_eq!(custody, CarrierPublicationCustodyV1::RetainedAndRecorded);
                assert_eq!(counters.unreachable_installed_residue_bytes, CARRIER_BYTES);
                assert_eq!(fs::metadata(&carrier).unwrap().len(), CARRIER_BYTES);
                assert!(matches!(cas.ensure_valid(), Err(FsCasErrorV1::Invalidated)));

                drop(cas);
                fs::remove_dir_all(root).unwrap();
            }
        }
    }

    #[test]
    fn root_storage_admission_refuses_bytes_and_inodes_without_changing_reservations() {
        // One namespace entry is permanently reserved for the root's
        // allocation-independent invalidation barrier.  It is root-owned
        // logical headroom, not a host free-inode observation and not an
        // operation residue.  A root whose visible state already consumes
        // the remaining configured names must therefore fail at open.
        assert!(matches!(
            RootStorageAdmissionV1::new_with_capacities(usage(1, 1), usage(1, 1), 100, 2),
            Err(FsCasErrorV1::ResourceExhausted(
                FsCasResourceV1::StorageInodes
            ))
        ));

        let byte_admission =
            RootStorageAdmissionV1::new_with_capacities(usage(10, 1), usage(2, 1), 100, 10)
                .unwrap();
        let byte_lease = byte_admission
            .reserve(FsStorageEnvelopeV1::new(40, 20, 1, 1).unwrap())
            .unwrap();
        assert!(matches!(
            byte_admission.reserve(FsStorageEnvelopeV1::new(20, 10, 1, 1).unwrap()),
            Err(FsCasErrorV1::ResourceExhausted(
                FsCasResourceV1::StorageBytes
            ))
        ));
        {
            let state = byte_admission.state.lock().unwrap();
            assert_eq!(state.active_reserved, usage(60, 2));
            assert_eq!(state.reserved_high_water, usage(60, 2));
        }
        drop(byte_lease);
        assert_eq!(
            byte_admission.state.lock().unwrap().active_reserved,
            RootStorageUsageV1::default()
        );

        let inode_admission =
            RootStorageAdmissionV1::new_with_capacities(usage(1, 2), usage(1, 1), 100, 6).unwrap();
        let inode_lease = inode_admission
            .reserve(FsStorageEnvelopeV1::new(1, 1, 1, 1).unwrap())
            .unwrap();
        assert!(matches!(
            inode_admission.reserve(FsStorageEnvelopeV1::new(1, 1, 1, 1).unwrap()),
            Err(FsCasErrorV1::ResourceExhausted(
                FsCasResourceV1::StorageInodes
            ))
        ));
        assert_eq!(
            inode_admission.state.lock().unwrap().active_reserved,
            usage(2, 2)
        );
        drop(inode_lease);
        assert_eq!(
            inode_admission.state.lock().unwrap().active_reserved,
            RootStorageUsageV1::default()
        );
    }

    #[test]
    fn root_storage_envelope_widening_is_atomic_monotonic_and_terminally_exact() {
        let admission =
            RootStorageAdmissionV1::new_with_capacities(usage(10, 2), usage(2, 1), 100, 10)
                .unwrap();
        let mut lease = admission
            .reserve_for_operation_v1(
                FsStorageEnvelopeV1::new(0, 0, 0, 0).unwrap(),
                FsOperationKindV1::CompleteReplace,
            )
            .unwrap();
        let token = lease.token_v1().unwrap();
        let final_envelope = FsStorageEnvelopeV1::new(20, 10, 1, 1).unwrap();

        admission
            .widen_for_operation_v1(token, final_envelope)
            .unwrap();
        // Re-declaring the same conservative envelope is idempotent and does
        // not mint a second reservation or change the token identity.
        admission
            .widen_for_operation_v1(token, final_envelope)
            .unwrap();
        {
            let state = admission.state.lock().unwrap();
            assert_eq!(state.active_reserved, usage(30, 2));
            assert_eq!(state.reserved_high_water, usage(30, 2));
            assert_eq!(
                state.operations[usize::from(token.slot)].envelope,
                Some(final_envelope)
            );
        }

        // A component shrink fails closed, even if another component grows.
        assert_eq!(
            admission
                .widen_for_operation_v1(token, FsStorageEnvelopeV1::new(19, 20, 1, 1).unwrap()),
            Err(FsCasErrorV1::Integrity)
        );
        // Capacity refusal is atomic: the final accepted envelope and shared
        // current reservation remain unchanged.
        assert_eq!(
            admission
                .widen_for_operation_v1(token, FsStorageEnvelopeV1::new(60, 40, 1, 1).unwrap()),
            Err(FsCasErrorV1::ResourceExhausted(
                FsCasResourceV1::StorageBytes
            ))
        );
        {
            let state = admission.state.lock().unwrap();
            assert_eq!(state.active_reserved, usage(30, 2));
            assert_eq!(
                state.operations[usize::from(token.slot)].envelope,
                Some(final_envelope)
            );
        }

        let mut counters = OperationCountersV1::default();
        lease
            .admission
            .finish(lease.slot, lease.nonce, false, &mut counters)
            .unwrap();
        lease.released = true;
        assert_eq!(counters.storage_bytes_requested, 30);
        assert_eq!(counters.storage_bytes_reserved, 30);
        assert_eq!(counters.storage_bytes_released, 30);
        assert_eq!(counters.storage_bytes_committed, 0);
        assert_eq!(counters.storage_bytes_retained, 0);
        assert_eq!(counters.storage_inodes_requested, 2);
        assert_eq!(counters.storage_inodes_reserved, 2);
        assert_eq!(counters.storage_inodes_released, 2);
        assert_eq!(counters.storage_inodes_committed, 0);
        assert_eq!(counters.storage_inodes_retained, 0);
        assert_eq!(
            admission.state.lock().unwrap().active_reserved,
            RootStorageUsageV1::default()
        );
    }

    #[test]
    fn root_storage_terminal_counters_reconcile_and_report_shared_high_water() {
        let admission =
            RootStorageAdmissionV1::new_with_capacities(usage(10, 2), usage(2, 1), 200, 20)
                .unwrap();
        let mut first = admission
            .reserve(FsStorageEnvelopeV1::new(20, 20, 1, 1).unwrap())
            .unwrap();
        let mut second = admission
            .reserve(FsStorageEnvelopeV1::new(30, 10, 1, 1).unwrap())
            .unwrap();
        let first_token = first.token_v1().unwrap();
        let second_token = second.token_v1().unwrap();
        admission.record_preparation_create_v1(first_token).unwrap();
        admission
            .record_preparation_length_v1(first_token, 0, 6)
            .unwrap();
        admission
            .record_immutable_install_v1(first_token, 9, 1)
            .unwrap();
        admission
            .record_preparation_create_v1(second_token)
            .unwrap();
        admission
            .record_preparation_length_v1(second_token, 0, 7)
            .unwrap();
        admission
            .record_immutable_install_v1(second_token, 8, 1)
            .unwrap();
        admission
            .record_preparation_remove_v1(first_token, 6)
            .unwrap();
        let mut counters = OperationCountersV1::default();
        first
            .admission
            .finish(first.slot, first.nonce, true, &mut counters)
            .unwrap();
        first.released = true;
        assert_eq!(counters.storage_bytes_requested, 40);
        assert_eq!(counters.storage_bytes_reserved, 40);
        assert_eq!(counters.storage_bytes_released, 31);
        assert_eq!(counters.storage_bytes_committed, 9);
        assert_eq!(counters.storage_bytes_retained, 0);
        assert_eq!(counters.storage_inodes_requested, 2);
        assert_eq!(counters.storage_inodes_released, 1);
        assert_eq!(counters.storage_inodes_committed, 1);
        assert_eq!(
            counters.root_storage_active_reserved_bytes_lifetime_high_water,
            80
        );
        assert_eq!(
            counters.root_storage_active_reserved_inodes_lifetime_high_water,
            4
        );
        assert_eq!(counters.storage_preparation_bytes_high_water, 6);
        assert_eq!(counters.storage_preparation_inodes_high_water, 1);
        assert_eq!(counters.storage_preparation_bytes_current_after_cleanup, 0);
        assert_eq!(counters.storage_preparation_inodes_current_after_cleanup, 0);
        {
            let state = admission.state.lock().unwrap();
            assert_eq!(state.immutable, usage(19, 3));
            assert_eq!(state.preparation, usage(2, 1));
            assert_eq!(
                state.operations[second.slot].preparation_current,
                usage(7, 1)
            );
            assert_eq!(state.operations[second.slot].immutable_pending, usage(8, 1));
        }

        let mut second_counters = OperationCountersV1::default();
        second
            .admission
            .finish(second.slot, second.nonce, false, &mut second_counters)
            .unwrap();
        second.released = true;
        assert_eq!(second_counters.storage_bytes_requested, 40);
        assert_eq!(second_counters.storage_bytes_released, 25);
        assert_eq!(second_counters.storage_bytes_committed, 0);
        assert_eq!(second_counters.storage_bytes_retained, 15);
        assert_eq!(second_counters.storage_inodes_released, 0);
        assert_eq!(second_counters.storage_inodes_retained, 2);
        assert_eq!(second_counters.storage_preparation_bytes_high_water, 7);
        assert_eq!(second_counters.storage_preparation_inodes_high_water, 1);
        assert_eq!(
            second_counters.storage_preparation_bytes_current_after_cleanup,
            7
        );
        assert_eq!(
            second_counters.storage_preparation_inodes_current_after_cleanup,
            1
        );
        assert_eq!(second_counters.immutable_residue_bytes, 8);
        assert_eq!(second_counters.immutable_residue_inodes, 1);
        {
            let state = admission.state.lock().unwrap();
            assert_eq!(state.immutable, usage(27, 4));
            assert_eq!(state.preparation, usage(9, 2));
        }
        drop(first);
        drop(second);
        assert_eq!(
            admission.state.lock().unwrap().active_reserved,
            RootStorageUsageV1::default()
        );
    }

    #[test]
    fn storage_unwind_backstop_retains_exact_pending_state_before_slot_release() {
        let admission =
            RootStorageAdmissionV1::new_with_capacities(usage(10, 2), usage(2, 1), 200, 20)
                .unwrap();
        let lease = admission
            .reserve(FsStorageEnvelopeV1::new(20, 20, 1, 1).unwrap())
            .unwrap();
        let token = lease.token_v1().unwrap();
        admission.record_preparation_create_v1(token).unwrap();
        admission.record_preparation_length_v1(token, 0, 6).unwrap();
        admission.record_immutable_install_v1(token, 9, 1).unwrap();

        // Models capability Drop during unwind. No returned operation record
        // exists, so the shared owner must retain the exact operation-local
        // state before releasing its reservation cell.
        drop(lease);
        let state = admission.state.lock().unwrap();
        assert_eq!(state.active_reserved, RootStorageUsageV1::default());
        assert_eq!(state.immutable, usage(19, 3));
        assert_eq!(state.preparation, usage(8, 2));
        assert!(state.operations.iter().all(|operation| !operation.active));
    }

    #[cfg(feature = "c3-polymorphism")]
    #[test]
    fn storage_tokens_bind_root_generation_owner_instance_and_operation_lifetime() {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let sequence = NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed);
        let root_a = parent.join(format!(
            "layerfs-storage-authority-a-{}-{sequence}",
            std::process::id()
        ));
        let root_b = parent.join(format!(
            "layerfs-storage-authority-b-{}-{sequence}",
            std::process::id()
        ));
        let cas_a = FsCasV1::create_new(&root_a).unwrap();
        let cas_b = FsCasV1::create_new(&root_b).unwrap();
        let envelope = FsStorageEnvelopeV1::new(8, 8, 1, 1).unwrap();
        let mut control = ContinueFsCasControlV1;
        let mut counters_a = OperationCountersV1::default();
        let mut counters_b = OperationCountersV1::default();
        let mut capability_a = cas_a
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x155a,
                &mut counters_a,
                &mut control,
            )
            .unwrap();
        let mut capability_b = cas_b
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x155b,
                &mut counters_b,
                &mut control,
            )
            .unwrap();
        capability_a.declare_storage_envelope_v1(envelope).unwrap();
        capability_b.declare_storage_envelope_v1(envelope).unwrap();
        let token_a = capability_a.storage_token_v1().unwrap();
        let token_b = capability_b.storage_token_v1().unwrap();

        // Kind remains independently bound after the waiting ticket has been
        // consumed. Preserve the live owner/generation/slot/nonce identity
        // and alter only the kind: the borrowed storage boundary must reject
        // it without touching the operation's accounting cell.
        let wrong_kind = FsStorageOperationTokenV1 {
            operation_kind: FsOperationKindV1::CompleteC3Tree,
            ..token_a
        };
        assert_eq!(
            cas_a.record_storage_preparation_create_v1(wrong_kind),
            Err(FsCasErrorV1::WrongOperationKind)
        );
        assert_eq!(
            cas_a
                .inner
                .storage_admission
                .state
                .lock()
                .unwrap()
                .operations[usize::from(token_a.slot)]
            .preparation_current,
            RootStorageUsageV1::default()
        );
        assert_eq!(
            capability_a.require_operation_kind_v1(FsOperationKindV1::CompleteC3Tree),
            Err(FsCasErrorV1::WrongOperationKind)
        );
        capability_a
            .require_operation_kind_v1(FsOperationKindV1::CompleteC3File)
            .unwrap();

        // Independent roots deliberately begin with the same slot/nonce
        // sequence.  The owner binding, rather than accidental sequence
        // divergence, must reject the cross-root authority.
        assert_eq!(token_a.slot, token_b.slot);
        assert_eq!(token_a.nonce, token_b.nonce);
        assert_ne!(token_a.owner, token_b.owner);
        assert_eq!(
            cas_b.record_storage_preparation_create_v1(token_a),
            Err(FsCasErrorV1::CrossOwner)
        );
        assert_eq!(
            cas_b
                .inner
                .storage_admission
                .state
                .lock()
                .unwrap()
                .operations[usize::from(token_b.slot)]
            .preparation_current,
            RootStorageUsageV1::default()
        );

        // The persistent generation participates in the binding even if a
        // hostile in-crate test preserves the process-local owner instance.
        let mut wrong_owner = token_a.owner;
        wrong_owner.generation[0] ^= 0xff;
        let wrong_generation = FsStorageOperationTokenV1 {
            owner: wrong_owner,
            ..token_a
        };
        assert_eq!(
            cas_a.record_storage_preparation_create_v1(wrong_generation),
            Err(FsCasErrorV1::CrossOwner)
        );

        cas_a.record_storage_preparation_create_v1(token_a).unwrap();
        cas_a
            .record_storage_preparation_remove_v1(token_a, 0)
            .unwrap();
        capability_a
            .finish_storage_admission_v1(false, &mut counters_a, &mut control)
            .unwrap();
        capability_a
            .finish_operation_admission_v1(&mut counters_a, &mut control)
            .unwrap();

        // A replay within the still-live shared owner fails on the operation
        // nonce and cannot mutate a later operation's accounting cell.
        let mut replay_counters = OperationCountersV1::default();
        let mut replay_capability = cas_a
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3Tree,
                0x155c,
                &mut replay_counters,
                &mut control,
            )
            .unwrap();
        replay_capability
            .declare_storage_envelope_v1(envelope)
            .unwrap();
        let replay_token = replay_capability.storage_token_v1().unwrap();
        assert_ne!(token_a.nonce, replay_token.nonce);
        assert_eq!(
            cas_a.record_storage_preparation_create_v1(token_a),
            Err(FsCasErrorV1::Integrity)
        );
        assert_eq!(
            cas_a
                .inner
                .storage_admission
                .state
                .lock()
                .unwrap()
                .operations[usize::from(replay_token.slot)]
            .preparation_current,
            RootStorageUsageV1::default()
        );
        replay_capability
            .finish_storage_admission_v1(false, &mut replay_counters, &mut control)
            .unwrap();
        replay_capability
            .finish_operation_admission_v1(&mut replay_counters, &mut control)
            .unwrap();

        capability_b
            .finish_storage_admission_v1(false, &mut counters_b, &mut control)
            .unwrap();
        capability_b
            .finish_operation_admission_v1(&mut counters_b, &mut control)
            .unwrap();
        drop(replay_capability);
        drop(capability_a);
        drop(capability_b);
        drop(cas_a);

        // Close-all/reopen retains the persistent generation but creates a
        // new shared-owner instance.  Its first token again has slot 0/nonce
        // 1, proving that exact sequence reuse still cannot revive token A.
        let reopened = FsCasV1::open_existing(&root_a).unwrap();
        let mut reopened_counters = OperationCountersV1::default();
        let mut reopened_capability = reopened
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x155d,
                &mut reopened_counters,
                &mut control,
            )
            .unwrap();
        reopened_capability
            .declare_storage_envelope_v1(envelope)
            .unwrap();
        let reopened_token = reopened_capability.storage_token_v1().unwrap();
        assert_eq!(token_a.slot, reopened_token.slot);
        assert_eq!(token_a.nonce, reopened_token.nonce);
        assert_eq!(token_a.owner.generation, reopened_token.owner.generation);
        assert_ne!(token_a.owner.instance, reopened_token.owner.instance);
        assert_eq!(
            reopened.record_storage_preparation_create_v1(token_a),
            Err(FsCasErrorV1::CrossOwner)
        );
        reopened_capability
            .finish_storage_admission_v1(false, &mut reopened_counters, &mut control)
            .unwrap();
        reopened_capability
            .finish_operation_admission_v1(&mut reopened_counters, &mut control)
            .unwrap();
        drop(reopened_capability);
        drop(reopened);
        drop(cas_b);
        fs::remove_dir_all(root_a).unwrap();
        fs::remove_dir_all(root_b).unwrap();
    }

    #[test]
    fn phase_one_queue_has_exactly_1024_non_minting_ticket_cells() {
        let queue = OperationAdmissionQueueV1::new(16).expect("fixed queue allocation");
        let mut pending = Vec::with_capacity(MAX_ADMISSION_TICKETS);
        let mut counters = OperationCountersV1::default();
        for cancellation_key in 0..MAX_ADMISSION_TICKETS as u64 {
            pending.push(
                queue
                    .issue(
                        FsOperationKindV1::CompleteC3File,
                        cancellation_key,
                        &mut counters,
                    )
                    .expect("one preallocated ticket per admitted waiter"),
            );
        }
        assert!(matches!(
            queue.issue(FsOperationKindV1::CompleteC3Tree, u64::MAX, &mut counters,),
            Err(OperationAdmissionIssueFailureV1 {
                first: FsCasErrorV1::ResourceExhausted(FsCasResourceV1::Queue),
                observation_failed: false,
            })
        ));

        {
            let state = queue.state.lock().expect("queue state");
            assert_eq!(state.queue_tickets.len(), MAX_ADMISSION_TICKETS);
            assert_eq!(
                state.queue_tickets.len() * core::mem::size_of::<QueueTicketV1>(),
                MAX_ADMISSION_TICKETS * 256
            );
            assert_eq!(state.next_ticket - state.serving_ticket, 1_024);
            assert_eq!(state.active, 0);
            assert!(state
                .tickets
                .iter()
                .all(|ticket| *ticket == AdmissionTicketStateV1::Waiting));
        }

        drop(pending);
        let state = queue.state.lock().expect("released queue state");
        assert_eq!(state.next_ticket, state.serving_ticket);
        assert_eq!(state.active, 0);
        assert!(state
            .tickets
            .iter()
            .all(|ticket| *ticket == AdmissionTicketStateV1::Empty));
        assert!(state
            .queue_tickets
            .iter()
            .all(|ticket| ticket.operation_kind == 0 && ticket.cancellation_key == 0));
    }
}

/// The invalidation name is a reserved fail-closed barrier. Any occupant or
/// any error other than a definite `NotFound` prevents use of the root. An
/// access/I/O failure is still returned with its exact read-side provenance;
/// it is not evidence that the invalidation marker itself exists.
fn root_invalidation_barrier_present_v1(path: &Path) -> Result<bool, FsCasErrorV1> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Ok(_) => Ok(true),
        Err(error) => Err(map_filesystem_read_error_v1(&error)),
    }
}

fn validate_new_root(root: &Path) -> Result<(), FsCasErrorV1> {
    if !root.is_absolute() || root.file_name().is_none() {
        return Err(FsCasErrorV1::Unsupported);
    }
    match fs::symlink_metadata(root) {
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Ok(_) => return Err(FsCasErrorV1::Unsupported),
        Err(error) => return Err(map_filesystem_read_error_v1(&error)),
    }
    let parent = root.parent().ok_or(FsCasErrorV1::Unsupported)?;
    validate_private_directory(parent)?;
    let canonical =
        fs::canonicalize(parent).map_err(|error| map_required_filesystem_read_error_v1(&error))?;
    if canonical != parent {
        return Err(FsCasErrorV1::Unsupported);
    }
    Ok(())
}

fn validate_existing_root(root: &Path) -> Result<(), FsCasErrorV1> {
    if !root.is_absolute() {
        return Err(FsCasErrorV1::Unsupported);
    }
    validate_private_directory(root)?;
    if fs::canonicalize(root).map_err(|error| map_required_filesystem_read_error_v1(&error))?
        != root
    {
        return Err(FsCasErrorV1::Unsupported);
    }
    Ok(())
}

fn validate_private_directory(path: &Path) -> Result<(), FsCasErrorV1> {
    validate_directory_shape_v1(path, FsCasErrorV1::Unsupported)
}

fn validate_required_root_directory(path: &Path) -> Result<(), FsCasErrorV1> {
    validate_directory_shape_v1(path, FsCasErrorV1::MalformedOccupant)
}

fn validate_directory_shape_v1(
    path: &Path,
    observed_shape_error: FsCasErrorV1,
) -> Result<(), FsCasErrorV1> {
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return Err(FsCasErrorV1::Unsupported);
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| map_required_filesystem_read_error_v1(&error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(observed_shape_error);
    }
    Ok(())
}

fn create_private_directory(path: &Path) -> Result<(), FsCasErrorV1> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700);
        builder
            .create(path)
            .map_err(|error| map_required_filesystem_write_error_v1(&error))?;
    }
    #[cfg(not(unix))]
    {
        fs::create_dir(path).map_err(|error| map_required_filesystem_write_error_v1(&error))?;
    }
    validate_private_directory(path)
}

fn set_private_file_permissions(path: &Path) -> Result<(), FsCasErrorV1> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| map_required_filesystem_write_error_v1(&error))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

fn set_read_only(path: &Path) -> Result<(), FsCasErrorV1> {
    let mut permissions = fs::metadata(path)
        .map_err(|error| map_required_filesystem_read_error_v1(&error))?
        .permissions();
    permissions.set_readonly(true);
    fs::set_permissions(path, permissions)
        .map_err(|error| map_required_filesystem_write_error_v1(&error))
}

fn unique_private_path(directory: &Path, prefix: &str) -> Result<PathBuf, FsCasErrorV1> {
    for _ in 0..128 {
        let sequence = NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed);
        let name = format!("{prefix}-{}-{sequence:016x}", std::process::id());
        let path = directory.join(name);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(path),
            Ok(_) => {}
            Err(error) => return Err(map_filesystem_read_error_v1(&error)),
        }
    }
    Err(FsCasErrorV1::Collision)
}

fn hex_id(bytes: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn hex_typed_id(id: TypedPhysicalObjectIdV1) -> String {
    let prefix = match typed_kind_byte(id) {
        1 => "01-",
        2 => "02-",
        3 => "03-",
        4 => "04-",
        5 => "05-",
        _ => unreachable!(),
    };
    format!("{prefix}{}", hex_id(id.as_bytes()))
}

const fn typed_kind_byte(id: TypedPhysicalObjectIdV1) -> u8 {
    match id {
        TypedPhysicalObjectIdV1::VersionRecord(_) => 1,
        TypedPhysicalObjectIdV1::Tree(_) => 2,
        TypedPhysicalObjectIdV1::File(_) => 3,
        TypedPhysicalObjectIdV1::Symlink(_) => 4,
        TypedPhysicalObjectIdV1::Chunk(_) => 5,
    }
}

fn is_unsupported_link_error(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::Unsupported | ErrorKind::CrossesDevices
    )
}

#[cfg(unix)]
fn validate_same_filesystem(left: &Path, right: &Path) -> Result<(), FsCasErrorV1> {
    use std::os::unix::fs::MetadataExt;
    let left = fs::metadata(left).map_err(|error| map_required_filesystem_read_error_v1(&error))?;
    let right =
        fs::metadata(right).map_err(|error| map_required_filesystem_read_error_v1(&error))?;
    if left.dev() == right.dev() {
        Ok(())
    } else {
        Err(FsCasErrorV1::Unsupported)
    }
}

#[cfg(not(unix))]
fn validate_same_filesystem(_left: &Path, _right: &Path) -> Result<(), FsCasErrorV1> {
    Err(FsCasErrorV1::Unsupported)
}

#[cfg(test)]
mod admission_queue_tests {
    use super::*;

    #[test]
    fn filesystem_io_provenance_distinguishes_direction_progress_and_absence() {
        assert_eq!(
            map_filesystem_read_error_v1(&std::io::Error::from(ErrorKind::UnexpectedEof)),
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ShortRead)
        );
        assert_eq!(
            map_filesystem_read_error_v1(&std::io::Error::from(ErrorKind::BrokenPipe)),
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ReadFailure)
        );
        assert_eq!(
            map_filesystem_write_error_v1(&std::io::Error::from(ErrorKind::WriteZero)),
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ShortWrite)
        );
        assert_eq!(
            map_filesystem_write_error_v1(&std::io::Error::from(ErrorKind::BrokenPipe)),
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure)
        );
        assert_eq!(
            map_required_filesystem_write_error_v1(&std::io::Error::from(ErrorKind::NotFound)),
            FsCasErrorV1::MissingOccupant
        );
        assert_eq!(
            map_required_filesystem_read_error_v1(&std::io::Error::from(ErrorKind::NotFound)),
            FsCasErrorV1::MissingOccupant
        );
        assert_eq!(
            map_filesystem_read_error_v1(&std::io::Error::from(ErrorKind::PermissionDenied)),
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied)
        );

        let parent = std::env::temp_dir().canonicalize().unwrap();
        let root = parent.join(format!(
            "layerfs-io-provenance-{}-{}",
            std::process::id(),
            NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let path = root.join("payload");
        fs::write(&path, b"12345678").unwrap();

        assert_eq!(
            root_invalidation_barrier_present_v1(&root.join("absent-marker")),
            Ok(false)
        );
        assert_eq!(root_invalidation_barrier_present_v1(&path), Ok(true));
        assert_eq!(
            root_invalidation_barrier_present_v1(&path.join("not-a-child")),
            Err(FsCasErrorV1::Filesystem(
                FsCasFilesystemFailureV1::ReadFailure
            ))
        );

        let mut write_only = OpenOptions::new().write(true).open(&path).unwrap();
        assert_eq!(
            read_exact_regular_file_from_open_v1::<8>(&mut write_only),
            Err(FsCasErrorV1::Filesystem(
                FsCasFilesystemFailureV1::ReadFailure
            ))
        );

        let mut read_only = File::open(&path).unwrap();
        let actual_write = read_only.write_all(b"x").unwrap_err();
        assert_eq!(
            map_filesystem_write_error_v1(&actual_write),
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure)
        );

        let mut truncated = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        assert_eq!(truncated.metadata().unwrap().len(), 8);
        truncated.set_len(3).unwrap();
        truncated.seek(SeekFrom::Start(0)).unwrap();
        assert_eq!(
            read_exact_regular_file_after_metadata_v1::<8>(&mut truncated),
            Err(FsCasErrorV1::Filesystem(
                FsCasFilesystemFailureV1::ShortRead
            ))
        );
        assert!(matches!(
            open_regular_file(&root.join("absent")),
            Err(FsCasErrorV1::MissingOccupant)
        ));
        assert_eq!(
            validate_private_directory(&root.join("absent-directory")),
            Err(FsCasErrorV1::MissingOccupant)
        );
        assert!(matches!(
            FsCasV1::open_existing(&root.join("absent-root")),
            Err(FsCasErrorV1::MissingOccupant)
        ));

        fs::remove_dir_all(root).unwrap();
    }

    /// These raw errno rows anchor the semantic mapping used by the compact
    /// filesystem-boundary matrix.  Logical counters never manufacture any
    /// of these host failure categories.
    #[cfg(unix)]
    #[test]
    fn raw_unix_filesystem_errno_mapping_is_directional_and_exact() {
        use std::io::Error;

        for errno in [libc::EACCES, libc::EPERM] {
            let error = Error::from_raw_os_error(errno);
            assert_eq!(
                map_filesystem_read_error_v1(&error),
                FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied)
            );
            assert_eq!(
                map_filesystem_write_error_v1(&error),
                FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied)
            );
        }

        assert_eq!(
            map_filesystem_write_error_v1(&Error::from_raw_os_error(libc::ENOSPC)),
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::NoSpace)
        );
        assert_eq!(
            map_filesystem_write_error_v1(&Error::from_raw_os_error(libc::EDQUOT)),
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::Quota)
        );

        // A raw errno outside the specifically classified set remains
        // directionally generic; it is not inferred to mean capacity loss.
        let unknown = Error::from_raw_os_error(i32::MAX);
        assert_eq!(
            map_filesystem_read_error_v1(&unknown),
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ReadFailure)
        );
        assert_eq!(
            map_filesystem_write_error_v1(&unknown),
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure)
        );
        assert_eq!(
            map_filesystem_read_error_v1(&Error::from(ErrorKind::UnexpectedEof)),
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ShortRead)
        );
        assert_eq!(
            map_filesystem_write_error_v1(&Error::from(ErrorKind::WriteZero)),
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ShortWrite)
        );
    }

    #[test]
    fn hard_link_failure_classifier_preserves_no_replace_meaning() {
        let unsupported = std::io::Error::from(ErrorKind::Unsupported);
        let crosses_devices = std::io::Error::from(ErrorKind::CrossesDevices);
        let already_exists = std::io::Error::from(ErrorKind::AlreadyExists);
        let generic = std::io::Error::from(ErrorKind::BrokenPipe);

        assert!(is_unsupported_link_error(&unsupported));
        assert!(is_unsupported_link_error(&crosses_devices));
        // `AlreadyExists` is deliberately *not* folded into Unsupported: the
        // no-replace caller authenticates the incumbent under its visibility
        // lock.  All other errors remain directional write failures.
        assert!(!is_unsupported_link_error(&already_exists));
        assert!(!is_unsupported_link_error(&generic));
        assert_eq!(
            map_required_filesystem_write_error_v1(&already_exists),
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure)
        );
        assert_eq!(
            map_required_filesystem_write_error_v1(&generic),
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure)
        );
    }

    #[test]
    fn fixed_root_directories_distinguish_malformed_shape_from_missing_capability() {
        #[derive(Clone, Copy)]
        enum Shape {
            File,
            Symlink,
            Missing,
        }

        let parent = std::env::temp_dir().canonicalize().unwrap();
        for child in ["preparation", "carriers", "objects", "catalog", "closures"] {
            for shape in [Shape::File, Shape::Symlink, Shape::Missing] {
                let shape_name = match shape {
                    Shape::File => "file",
                    Shape::Symlink => "symlink",
                    Shape::Missing => "missing",
                };
                let root = parent.join(format!(
                    "layerfs-root-component-shape-{child}-{shape_name}-{}-{}",
                    std::process::id(),
                    NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
                ));
                let cas = FsCasV1::create_new(&root).unwrap();
                drop(cas);

                let component = root.join(child);
                fs::remove_dir(&component).unwrap();
                match shape {
                    Shape::File => fs::write(&component, b"not-a-directory").unwrap(),
                    Shape::Symlink => {
                        #[cfg(unix)]
                        std::os::unix::fs::symlink(".", &component).unwrap();
                        #[cfg(windows)]
                        std::os::windows::fs::symlink_dir(".", &component).unwrap();
                    }
                    Shape::Missing => {}
                }

                let expected = match shape {
                    Shape::File | Shape::Symlink => FsCasErrorV1::MalformedOccupant,
                    Shape::Missing => FsCasErrorV1::MissingOccupant,
                };
                assert!(matches!(FsCasV1::open_existing(&root), Err(error) if error == expected));
                fs::remove_dir_all(root).unwrap();
            }
        }

        let root = parent.join(format!(
            "layerfs-live-root-component-shape-{}-{}",
            std::process::id(),
            NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
        ));
        let cas = FsCasV1::create_new(&root).unwrap();
        let preparation = root.join("preparation");
        fs::remove_dir(&preparation).unwrap();
        fs::write(&preparation, b"not-a-directory").unwrap();
        assert!(matches!(
            cas.begin_private_pack(),
            Err(FsCasErrorV1::MalformedOccupant)
        ));
        assert_eq!(cas.operation_admitted_slots_v1(), 0);
        assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
        drop(cas);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn root_initialization_marker_retains_alias_cleanup_cause() {
        let permission = FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied);
        assert_eq!(
            MarkerPublicationV1::<8>::VisibleWithPreparationResidue(Some(permission))
                .require_clean(),
            Err(permission)
        );
        assert_eq!(
            MarkerPublicationV1::<8>::VisibleWithPreparationResidue(None).require_clean(),
            Err(FsCasErrorV1::CleanupFailed(
                FsCasCleanupTargetV1::PublishedMarkerAlias
            ))
        );
        assert_eq!(
            MarkerPublicationV1::<8>::IncumbentClean([0_u8; 8]).require_clean(),
            Err(FsCasErrorV1::Integrity)
        );

        let cleanup_failure = root_initialization_cleanup_result_v1(
            permission,
            Err(std::io::Error::from(ErrorKind::PermissionDenied)),
        );
        assert_eq!(
            cleanup_failure.failure_causes_v1(),
            (
                FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
                FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::RootInitialization),
            )
        );
    }

    #[test]
    fn root_storage_scan_classifies_observed_non_files_as_malformed_occupants() {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let root = parent.join(format!(
            "layerfs-root-storage-shape-{}-{}",
            std::process::id(),
            NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
        ));
        let cas = FsCasV1::create_new(&root).unwrap();
        let generation = cas.inner.generation;
        drop(cas);

        let preparation_directory = root.join("preparation").join("unexpected-directory");
        fs::create_dir(&preparation_directory).unwrap();
        assert_eq!(
            observe_root_storage_usage_v1(&root),
            Err(FsCasErrorV1::MalformedOccupant)
        );
        fs::remove_dir(&preparation_directory).unwrap();

        let immutable_directory = root.join("objects").join("unexpected-directory");
        fs::create_dir(&immutable_directory).unwrap();
        assert_eq!(
            observe_root_storage_usage_v1(&root),
            Err(FsCasErrorV1::MalformedOccupant)
        );
        fs::remove_dir(&immutable_directory).unwrap();

        let generation_path = root.join("generation");
        fs::remove_file(&generation_path).unwrap();
        fs::create_dir(&generation_path).unwrap();
        assert_eq!(
            observe_root_storage_usage_v1(&root),
            Err(FsCasErrorV1::MalformedOccupant)
        );
        fs::remove_dir(&generation_path).unwrap();
        fs::write(&generation_path, encode_generation_marker(generation)).unwrap();
        set_read_only(&generation_path).unwrap();

        let owner_path = root.join(ROOT_OWNER_NAME);
        fs::create_dir(&owner_path).unwrap();
        assert_eq!(
            observe_root_storage_usage_v1(&root),
            Err(FsCasErrorV1::MalformedOccupant)
        );
        fs::remove_dir(&owner_path).unwrap();

        let reopened = FsCasV1::open_existing(&root).unwrap();
        drop(reopened);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn open_existing_removes_internally_acquired_owner_after_storage_scan_failure() {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let root = parent.join(format!(
            "layerfs-open-scan-owner-cleanup-{}-{}",
            std::process::id(),
            NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
        ));
        let cas = FsCasV1::create_new(&root).unwrap();
        drop(cas);

        let malformed = root.join("objects").join("unexpected-directory");
        fs::create_dir(&malformed).unwrap();
        assert_eq!(
            FsCasV1::open_existing(&root).err(),
            Some(FsCasErrorV1::MalformedOccupant)
        );
        assert_eq!(
            fs::symlink_metadata(root.join(ROOT_OWNER_NAME))
                .unwrap_err()
                .kind(),
            ErrorKind::NotFound
        );

        fs::remove_dir(&malformed).unwrap();
        let reopened = FsCasV1::open_existing(&root).unwrap();
        drop(reopened);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn post_acquire_owner_cleanup_failure_retains_first_and_fail_closed_token() {
        use std::os::unix::fs::PermissionsExt;

        let parent = std::env::temp_dir().canonicalize().unwrap();
        let root = parent.join(format!(
            "layerfs-open-owner-cleanup-failure-{}-{}",
            std::process::id(),
            NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
        ));
        let cas = FsCasV1::create_new(&root).unwrap();
        let generation = cas.inner.generation;
        drop(cas);

        let original_permissions = fs::symlink_metadata(&root).unwrap().permissions();
        let result = shared_root_owner_inner_v1(&root, generation, None, || {
            fs::set_permissions(&root, fs::Permissions::from_mode(0o500)).unwrap();
            Err(FsCasErrorV1::Filesystem(
                FsCasFilesystemFailureV1::PermissionDenied,
            ))
        });
        fs::set_permissions(&root, original_permissions).unwrap();
        let error = match result {
            Ok(_) => panic!("post-acquire construction failure unexpectedly succeeded"),
            Err(error) => error,
        };
        assert_eq!(
            error.failure_causes_v1(),
            (
                FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied,),
                FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::RootInitialization,),
            )
        );
        assert_eq!(
            decode_existing_root_owner(&root.join(ROOT_OWNER_NAME), generation),
            Ok(FsCasErrorV1::Busy)
        );
        assert_eq!(
            FsCasV1::open_existing(&root).err(),
            Some(FsCasErrorV1::Busy)
        );

        fs::remove_file(root.join(ROOT_OWNER_NAME)).unwrap();
        let reopened = FsCasV1::open_existing(&root).unwrap();
        drop(reopened);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn root_custody_records_distinguish_malformed_bytes_from_authentication_mismatch() {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let root = parent.join(format!(
            "layerfs-root-custody-{}-{}",
            std::process::id(),
            NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
        ));
        let cas = FsCasV1::create_new(&root).unwrap();
        let generation = cas.inner.generation;
        drop(cas);

        let generation_path = root.join("generation");
        set_private_file_permissions(&generation_path).unwrap();
        fs::write(&generation_path, [0_u8; GENERATION_MARKER_BYTES - 1]).unwrap();
        assert!(matches!(
            FsCasV1::open_existing(&root),
            Err(FsCasErrorV1::MalformedOccupant)
        ));
        let mut malformed_generation = encode_generation_marker(generation);
        malformed_generation[0] ^= 0xff;
        fs::write(&generation_path, malformed_generation).unwrap();
        assert!(matches!(
            FsCasV1::open_existing(&root),
            Err(FsCasErrorV1::MalformedOccupant)
        ));
        fs::write(&generation_path, encode_generation_marker(generation)).unwrap();
        set_read_only(&generation_path).unwrap();

        let owner_path = root.join(ROOT_OWNER_NAME);
        fs::write(&owner_path, [0_u8; ROOT_OWNER_BYTES - 1]).unwrap();
        assert!(matches!(
            FsCasV1::open_existing(&root),
            Err(FsCasErrorV1::MalformedOccupant)
        ));
        let mut malformed_owner = encode_root_owner(generation, ROOT_OWNER_STATE_ACTIVE);
        malformed_owner[9] = 1;
        fs::write(&owner_path, malformed_owner).unwrap();
        assert!(matches!(
            FsCasV1::open_existing(&root),
            Err(FsCasErrorV1::MalformedOccupant)
        ));
        let mut unknown_state = encode_root_owner(generation, ROOT_OWNER_STATE_ACTIVE);
        unknown_state[8] = u8::MAX;
        fs::write(&owner_path, unknown_state).unwrap();
        assert!(matches!(
            FsCasV1::open_existing(&root),
            Err(FsCasErrorV1::MalformedOccupant)
        ));
        let mut other_generation = generation;
        other_generation[0] ^= 0xff;
        fs::write(
            &owner_path,
            encode_root_owner(other_generation, ROOT_OWNER_STATE_ACTIVE),
        )
        .unwrap();
        assert!(matches!(
            FsCasV1::open_existing(&root),
            Err(FsCasErrorV1::Integrity)
        ));

        fs::remove_file(owner_path).unwrap();
        let reopened = FsCasV1::open_existing(&root).unwrap();
        drop(reopened);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn closure_marker_decode_distinguishes_structure_binding_and_unequal_content() {
        let version = PhysicalVersionRecordIdV1::from_digest([0x31; 32]);
        let generation = [0x42; 32];
        let transcript = [0x53; 32];
        let encoded = encode_closure_marker(
            TypedPhysicalObjectIdV1::VersionRecord(version),
            7,
            generation,
            transcript,
        );
        assert_eq!(
            decode_closure_marker_v1(encoded, version, generation),
            Ok(FsCasAcceptedClosureReadV1 {
                version_record: version,
                object_count: 7,
                transcript,
            })
        );

        for offset in [0_usize, 8, 9, 48] {
            let mut malformed = encoded;
            if offset == 48 {
                malformed[48..56].fill(0);
            } else {
                malformed[offset] ^= 0xff;
            }
            assert_eq!(
                decode_closure_marker_v1(malformed, version, generation),
                Err(FsCasErrorV1::MalformedOccupant),
                "structural byte {offset}"
            );
        }

        let mut wrong_version_bytes = *version.as_bytes();
        wrong_version_bytes[0] ^= 0xff;
        let wrong_version = PhysicalVersionRecordIdV1::from_digest(wrong_version_bytes);
        assert_eq!(
            decode_closure_marker_v1(encoded, wrong_version, generation),
            Err(FsCasErrorV1::Integrity)
        );
        let mut wrong_generation = generation;
        wrong_generation[0] ^= 0xff;
        assert_eq!(
            decode_closure_marker_v1(encoded, version, wrong_generation),
            Err(FsCasErrorV1::Integrity)
        );

        let unequal = encode_closure_marker(
            TypedPhysicalObjectIdV1::VersionRecord(version),
            8,
            generation,
            transcript,
        );
        assert!(decode_closure_marker_v1(unequal, version, generation).is_ok());
        assert_ne!(unequal, encoded);
    }

    struct InvalidateAtClosureBoundaryV1 {
        cas: FsCasV1,
        target: Option<FsCasBoundaryV1>,
        triggered: bool,
    }

    impl FsCasControlV1 for InvalidateAtClosureBoundaryV1 {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if self.target == Some(boundary) && !self.triggered {
                self.triggered = true;
                self.cas.invalidate_root_backstop_v1();
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    #[test]
    fn closure_fence_retains_typed_invalidation_at_each_publication_revalidation() {
        for target in [
            None,
            Some(FsCasBoundaryV1::PublicationLockAcquired),
            Some(FsCasBoundaryV1::VisibilityLockAcquired),
        ] {
            let parent = std::env::temp_dir().canonicalize().unwrap();
            let root = parent.join(format!(
                "layerfs-closure-invalidation-{}-{}",
                std::process::id(),
                NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
            ));
            let cas = FsCasV1::create_new(&root).unwrap();
            let mut control = InvalidateAtClosureBoundaryV1 {
                cas: cas.clone(),
                target,
                triggered: false,
            };
            if target.is_none() {
                control.cas.invalidate_root_backstop_v1();
                control.triggered = true;
            }

            let mut marker_custody = ImmutableMarkerCustodyV1::default();
            let version = PhysicalVersionRecordIdV1::from_digest([0x64; 32]);
            let typed = TypedPhysicalObjectIdV1::VersionRecord(version);
            let (result, first_error) = {
                let mut fence = FsClosureFenceV1::new(
                    cas.clone(),
                    1,
                    None,
                    &mut marker_custody,
                    &mut control,
                    false,
                );
                fence.expected_count = Some(1);
                fence.observed_count = 1;
                fence.observed_version = Some(typed);
                fence.transcript = Some(ClosureTranscriptV1::new(1));
                let result = fence.make_closure_visible(typed);
                (result, fence.first_error)
            };

            assert_eq!(result, Err(ImmutablePortErrorV1::Failure), "{target:?}");
            assert_eq!(first_error, Some(FsCasErrorV1::Invalidated), "{target:?}");
            assert!(control.triggered, "{target:?}");
            assert_eq!(marker_custody.live_unclassified_bytes, 0, "{target:?}");
            assert_eq!(marker_custody.retained_and_recorded_bytes, 0, "{target:?}");
            drop(control);
            drop(cas);
            fs::remove_dir_all(root).unwrap();
        }
    }

    struct ClosureProbeTerminalControlV1 {
        refuse_persistence: bool,
        invalidation_attempts: u64,
    }

    impl FsCasControlV1 for ClosureProbeTerminalControlV1 {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::RootInvalidation {
                self.invalidation_attempts += 1;
                self.refuse_persistence
            } else {
                false
            }
        }
    }

    #[cfg(feature = "c3-polymorphism")]
    #[test]
    fn closure_terminal_preserves_invalidation_probe_failure_and_double_fault() {
        for (case, probe_error, first_cause) in [
            (
                "permission",
                FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
                FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
            ),
            (
                "read",
                FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ReadFailure),
                FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::ReadFailure),
            ),
        ] {
            for refuse_persistence in [false, true] {
                let parent = std::env::temp_dir().canonicalize().unwrap();
                let root = parent.join(format!(
                    "layerfs-closure-probe-{case}-{refuse_persistence}-{}-{}",
                    std::process::id(),
                    NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
                ));
                let marker_path = root.join("closures").join("probe-marker");
                let cas = FsCasV1::create_new(&root).unwrap();
                let stale = cas.clone();
                let mut control = ClosureProbeTerminalControlV1 {
                    refuse_persistence,
                    invalidation_attempts: 0,
                };
                let mut counters = OperationCountersV1::default();
                let mut capability = cas
                    .begin_operation_capability_v1(
                        FsOperationKindV1::CompleteC3File,
                        0x155,
                        &mut counters,
                        &mut control,
                    )
                    .unwrap();
                capability
                    .declare_storage_envelope_v1(
                        FsStorageEnvelopeV1::new(0, CLOSURE_MARKER_BYTES as u64, 0, 1).unwrap(),
                    )
                    .unwrap();
                let storage_token = capability.storage_token_v1().unwrap();

                // Model the exact already-visible closure marker at the
                // terminalization boundary: physical name, root storage
                // charge, and operation-local marker custody all transition
                // before the fallible invalidation-barrier observation.
                fs::write(&marker_path, [0x6d; CLOSURE_MARKER_BYTES]).unwrap();
                set_read_only(&marker_path).unwrap();
                cas.record_storage_immutable_install_v1(
                    storage_token,
                    CLOSURE_MARKER_BYTES as u64,
                    1,
                )
                .unwrap();
                let mut closure_operation = cas
                    .begin_closure_operation_borrowed_v1(storage_token)
                    .unwrap();
                closure_operation
                    .marker_custody
                    .mark_visible_v1(CLOSURE_MARKER_BYTES as u64);
                cas.fail_next_invalidation_probe_for_test_v1(probe_error);

                let error = crate::cas::operation_admission::terminalize_failed_closure_marker_v1(
                    &mut closure_operation,
                    &mut counters,
                    &mut control,
                )
                .unwrap_err();
                let expected_dominant = if refuse_persistence {
                    FsCasFailureCauseV1::InvalidationFailed
                } else {
                    first_cause
                };
                assert_eq!(
                    error.failure_causes_v1(),
                    (first_cause, expected_dominant),
                    "{case}/{refuse_persistence}"
                );
                assert_eq!(
                    control.invalidation_attempts, 1,
                    "{case}/{refuse_persistence}"
                );
                assert_eq!(
                    closure_operation.marker_custody.live_unclassified_bytes, 0,
                    "{case}/{refuse_persistence}"
                );
                assert_eq!(
                    closure_operation.marker_custody.retained_and_recorded_bytes,
                    CLOSURE_MARKER_BYTES as u64,
                    "{case}/{refuse_persistence}"
                );
                assert_eq!(
                    counters.unreachable_installed_residue_bytes, CLOSURE_MARKER_BYTES as u64,
                    "{case}/{refuse_persistence}"
                );
                assert_eq!(
                    fs::symlink_metadata(&marker_path).unwrap().len(),
                    CLOSURE_MARKER_BYTES as u64,
                    "{case}/{refuse_persistence}"
                );

                capability
                    .finish_terminal_v1(false, &mut counters, &mut control)
                    .unwrap();
                assert_eq!(
                    counters.storage_bytes_requested, CLOSURE_MARKER_BYTES as u64,
                    "{case}/{refuse_persistence}"
                );
                assert_eq!(
                    counters.storage_bytes_reserved, counters.storage_bytes_requested,
                    "{case}/{refuse_persistence}"
                );
                assert_eq!(
                    counters.storage_bytes_reserved,
                    counters.storage_bytes_released
                        + counters.storage_bytes_committed
                        + counters.storage_bytes_retained,
                    "{case}/{refuse_persistence}"
                );
                assert_eq!(counters.storage_bytes_released, 0);
                assert_eq!(counters.storage_bytes_committed, 0);
                assert_eq!(counters.storage_bytes_retained, CLOSURE_MARKER_BYTES as u64);
                assert_eq!(counters.storage_inodes_requested, 1);
                assert_eq!(
                    counters.storage_inodes_reserved,
                    counters.storage_inodes_requested
                );
                assert_eq!(
                    counters.storage_inodes_reserved,
                    counters.storage_inodes_released
                        + counters.storage_inodes_committed
                        + counters.storage_inodes_retained
                );
                assert_eq!(counters.storage_inodes_released, 0);
                assert_eq!(counters.storage_inodes_committed, 0);
                assert_eq!(counters.storage_inodes_retained, 1);
                assert_eq!(
                    counters.immutable_residue_bytes,
                    CLOSURE_MARKER_BYTES as u64
                );
                assert_eq!(counters.immutable_residue_inodes, 1);
                assert!(counters.has_zero_forbidden_work());
                assert_eq!(cas.operation_admitted_slots_v1(), 0);
                assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
                assert_eq!(fs::read_dir(root.join("preparation")).unwrap().count(), 0);
                assert_eq!(cas.ensure_valid(), Err(FsCasErrorV1::Invalidated));
                assert_eq!(stale.ensure_valid(), Err(FsCasErrorV1::Invalidated));
                assert!(matches!(
                    FsCasV1::open_existing(&root),
                    Err(FsCasErrorV1::Busy | FsCasErrorV1::Invalidated)
                ));

                drop(closure_operation);
                drop(capability);
                drop(stale);
                drop(cas);
                assert!(matches!(
                    FsCasV1::open_existing(&root),
                    Err(FsCasErrorV1::Busy | FsCasErrorV1::Invalidated)
                ));
                fs::remove_dir_all(root).unwrap();
            }
        }
    }

    struct PanicClosureTerminalInvalidationV1 {
        cas: FsCasV1,
        root: PathBuf,
        refuse_backstop: bool,
        attempts: u64,
    }

    impl FsCasControlV1 for PanicClosureTerminalInvalidationV1 {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::RootInvalidation {
                self.attempts = self
                    .attempts
                    .checked_add(1)
                    .expect("closure terminal invalidation attempt count");
                if self.attempts == 1 {
                    if self.refuse_backstop {
                        // Make both callback-free persistence authorities fail
                        // only after the controlled attempt has begun. The
                        // initial ensure-valid probe must still observe a
                        // healthy root so this test exercises the unwind path.
                        fs::write(self.root.join(INVALIDATED_ROOT_NAME), b"wrong marker shape")
                            .unwrap();
                        let poison = self.cas.clone();
                        assert!(std::thread::spawn(move || {
                            let _guard = poison.inner.ownership.lock().unwrap();
                            panic!("inject closure owner-token mutex poison");
                        })
                        .join()
                        .is_err());
                    }
                    panic!("inject closure terminal invalidation unwind");
                }
            }
            false
        }
    }

    #[cfg(feature = "c3-polymorphism")]
    #[test]
    fn closure_terminal_invalidation_unwind_reports_the_real_backstop_result() {
        for refuse_backstop in [false, true] {
            let parent = std::env::temp_dir().canonicalize().unwrap();
            let root = parent.join(format!(
                "layerfs-closure-terminal-unwind-{refuse_backstop}-{}-{}",
                std::process::id(),
                NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
            ));
            let marker_path = root.join("closures").join("unwind-marker");
            let cas = FsCasV1::create_new(&root).unwrap();
            let stale = cas.clone();
            let mut control = PanicClosureTerminalInvalidationV1 {
                cas: cas.clone(),
                root: root.clone(),
                refuse_backstop,
                attempts: 0,
            };
            let mut counters = OperationCountersV1::default();
            let mut capability = cas
                .begin_operation_capability_v1(
                    FsOperationKindV1::CompleteC3File,
                    0x155,
                    &mut counters,
                    &mut control,
                )
                .unwrap();
            capability
                .declare_storage_envelope_v1(
                    FsStorageEnvelopeV1::new(0, CLOSURE_MARKER_BYTES as u64, 0, 1).unwrap(),
                )
                .unwrap();
            let storage_token = capability.storage_token_v1().unwrap();

            fs::write(&marker_path, [0x75; CLOSURE_MARKER_BYTES]).unwrap();
            set_read_only(&marker_path).unwrap();
            cas.record_storage_immutable_install_v1(storage_token, CLOSURE_MARKER_BYTES as u64, 1)
                .unwrap();
            let mut closure_operation = cas
                .begin_closure_operation_borrowed_v1(storage_token)
                .unwrap();
            closure_operation
                .marker_custody
                .mark_visible_v1(CLOSURE_MARKER_BYTES as u64);

            let terminal = crate::cas::operation_admission::terminalize_failed_closure_marker_v1(
                &mut closure_operation,
                &mut counters,
                &mut control,
            );
            if refuse_backstop {
                assert_eq!(
                    terminal.unwrap_err().failure_causes_v1(),
                    (
                        FsCasFailureCauseV1::SynchronizationPoisoned,
                        FsCasFailureCauseV1::InvalidationFailed,
                    )
                );
            } else {
                assert_eq!(terminal, Ok(()));
            }
            assert_eq!(control.attempts, 1);
            assert_eq!(closure_operation.marker_custody.live_unclassified_bytes, 0);
            assert_eq!(
                closure_operation.marker_custody.retained_and_recorded_bytes,
                CLOSURE_MARKER_BYTES as u64
            );
            assert_eq!(
                counters.unreachable_installed_residue_bytes,
                CLOSURE_MARKER_BYTES as u64
            );

            capability
                .finish_terminal_v1(false, &mut counters, &mut control)
                .unwrap();
            assert_eq!(
                counters.storage_bytes_requested,
                CLOSURE_MARKER_BYTES as u64
            );
            assert_eq!(
                counters.storage_bytes_reserved,
                counters.storage_bytes_requested
            );
            assert_eq!(counters.storage_bytes_released, 0);
            assert_eq!(counters.storage_bytes_committed, 0);
            assert_eq!(counters.storage_bytes_retained, CLOSURE_MARKER_BYTES as u64);
            assert_eq!(
                counters.storage_bytes_reserved,
                counters.storage_bytes_released
                    + counters.storage_bytes_committed
                    + counters.storage_bytes_retained
            );
            assert_eq!(counters.storage_inodes_requested, 1);
            assert_eq!(
                counters.storage_inodes_reserved,
                counters.storage_inodes_requested
            );
            assert_eq!(counters.storage_inodes_released, 0);
            assert_eq!(counters.storage_inodes_committed, 0);
            assert_eq!(counters.storage_inodes_retained, 1);
            assert_eq!(
                counters.storage_inodes_reserved,
                counters.storage_inodes_released
                    + counters.storage_inodes_committed
                    + counters.storage_inodes_retained
            );
            assert_eq!(
                counters.immutable_residue_bytes,
                CLOSURE_MARKER_BYTES as u64
            );
            assert_eq!(counters.immutable_residue_inodes, 1);
            assert!(counters.has_zero_forbidden_work());
            assert_eq!(cas.operation_admitted_slots_v1(), 0);
            assert_eq!(cas.operation_admission_active_for_test_v1(), 0);
            assert_eq!(cas.operation_admission_queue_for_test_v1(), (0, 0, 0));
            assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
            assert_eq!(fs::read_dir(root.join("preparation")).unwrap().count(), 0);
            assert_eq!(cas.ensure_valid(), Err(FsCasErrorV1::Invalidated));
            assert_eq!(stale.ensure_valid(), Err(FsCasErrorV1::Invalidated));
            assert!(matches!(
                FsCasV1::open_existing(&root),
                Err(FsCasErrorV1::Invalidated)
            ));

            drop(closure_operation);
            drop(capability);
            drop(control);
            drop(stale);
            drop(cas);
            assert!(matches!(
                FsCasV1::open_existing(&root),
                Err(FsCasErrorV1::Invalidated)
            ));
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[cfg(feature = "c3-polymorphism")]
    #[test]
    fn closure_fence_unwind_returns_owned_terminal_or_resumes_original_payload() {
        const ORIGINAL_PAYLOAD: &str = "inject closure admission callback unwind";

        for (case, overflow_residue, refuse_persistence) in [
            ("terminal-clean", false, false),
            ("retention-failed", true, false),
            ("invalidation-double-fault", true, true),
        ] {
            let parent = std::env::temp_dir().canonicalize().unwrap();
            let root = parent.join(format!(
                "layerfs-closure-fence-unwind-{case}-{}-{}",
                std::process::id(),
                NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
            ));
            let marker_path = root.join("closures").join("unwind-owned-marker");
            let cas = FsCasV1::create_new(&root).unwrap();
            let stale = cas.clone();
            let mut control = ClosureProbeTerminalControlV1 {
                refuse_persistence,
                invalidation_attempts: 0,
            };
            let mut counters = OperationCountersV1::default();
            if overflow_residue {
                counters.unreachable_installed_residue_bytes = u64::MAX;
            }
            let mut capability = cas
                .begin_operation_capability_v1(
                    FsOperationKindV1::CompleteC3File,
                    0x155,
                    &mut counters,
                    &mut control,
                )
                .unwrap();
            capability
                .declare_storage_envelope_v1(
                    FsStorageEnvelopeV1::new(0, CLOSURE_MARKER_BYTES as u64, 0, 1).unwrap(),
                )
                .unwrap();
            let storage_token = capability.storage_token_v1().unwrap();

            fs::write(&marker_path, [0x77; CLOSURE_MARKER_BYTES]).unwrap();
            set_read_only(&marker_path).unwrap();
            cas.record_storage_immutable_install_v1(storage_token, CLOSURE_MARKER_BYTES as u64, 1)
                .unwrap();
            let mut closure_operation = cas
                .begin_closure_operation_borrowed_v1(storage_token)
                .unwrap();
            closure_operation
                .marker_custody
                .mark_visible_v1(CLOSURE_MARKER_BYTES as u64);

            let unwind_terminal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::cas::operation_admission::terminalize_closure_unwind_v1(
                    &mut closure_operation,
                    &mut counters,
                    &mut control,
                    Box::new(ORIGINAL_PAYLOAD),
                )
            }));
            if overflow_residue {
                let terminal = unwind_terminal.expect("typed terminal must replace the unwind");
                let FsClosureAdmissionErrorV1::FsCas(terminal) = terminal else {
                    panic!("closure unwind returned a non-FsCas terminal: {terminal:?}");
                };
                let expected_dominant = if refuse_persistence {
                    FsCasFailureCauseV1::InvalidationFailed
                } else {
                    FsCasFailureCauseV1::Core(CoreError::IntegerOverflow)
                };
                assert_eq!(
                    terminal.failure_causes_v1(),
                    (
                        FsCasFailureCauseV1::Core(CoreError::IntegerOverflow),
                        expected_dominant,
                    ),
                    "{case}"
                );
                assert_eq!(
                    closure_operation.marker_custody.live_unclassified_bytes,
                    CLOSURE_MARKER_BYTES as u64,
                    "{case}"
                );
                assert_eq!(
                    closure_operation.marker_custody.retained_and_recorded_bytes, 0,
                    "{case}"
                );
                assert_eq!(counters.unreachable_installed_residue_bytes, u64::MAX);
            } else {
                let payload = unwind_terminal.expect_err("clean terminal must resume the unwind");
                assert_eq!(
                    payload.downcast_ref::<&str>().copied(),
                    Some(ORIGINAL_PAYLOAD)
                );
                assert_eq!(closure_operation.marker_custody.live_unclassified_bytes, 0);
                assert_eq!(
                    closure_operation.marker_custody.retained_and_recorded_bytes,
                    CLOSURE_MARKER_BYTES as u64
                );
                assert_eq!(
                    counters.unreachable_installed_residue_bytes,
                    CLOSURE_MARKER_BYTES as u64
                );
            }
            assert_eq!(control.invalidation_attempts, 1, "{case}");
            assert_eq!(
                fs::symlink_metadata(&marker_path).unwrap().len(),
                CLOSURE_MARKER_BYTES as u64,
                "{case}"
            );

            capability
                .finish_terminal_v1(false, &mut counters, &mut control)
                .unwrap();
            assert_eq!(
                counters.storage_bytes_requested, CLOSURE_MARKER_BYTES as u64,
                "{case}"
            );
            assert_eq!(
                counters.storage_bytes_reserved, counters.storage_bytes_requested,
                "{case}"
            );
            assert_eq!(
                counters.storage_bytes_reserved,
                counters.storage_bytes_released
                    + counters.storage_bytes_committed
                    + counters.storage_bytes_retained,
                "{case}"
            );
            assert_eq!(counters.storage_bytes_released, 0, "{case}");
            assert_eq!(counters.storage_bytes_committed, 0, "{case}");
            assert_eq!(
                counters.storage_bytes_retained, CLOSURE_MARKER_BYTES as u64,
                "{case}"
            );
            assert_eq!(counters.storage_inodes_requested, 1, "{case}");
            assert_eq!(
                counters.storage_inodes_reserved, counters.storage_inodes_requested,
                "{case}"
            );
            assert_eq!(
                counters.storage_inodes_reserved,
                counters.storage_inodes_released
                    + counters.storage_inodes_committed
                    + counters.storage_inodes_retained,
                "{case}"
            );
            assert_eq!(counters.storage_inodes_released, 0, "{case}");
            assert_eq!(counters.storage_inodes_committed, 0, "{case}");
            assert_eq!(counters.storage_inodes_retained, 1, "{case}");
            assert_eq!(
                counters.immutable_residue_bytes, CLOSURE_MARKER_BYTES as u64,
                "{case}"
            );
            assert_eq!(counters.immutable_residue_inodes, 1, "{case}");
            assert!(counters.has_zero_forbidden_work(), "{case}");
            assert_eq!(cas.operation_admitted_slots_v1(), 0, "{case}");
            assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{case}");
            assert_eq!(
                cas.operation_admission_queue_for_test_v1(),
                (0, 0, 0),
                "{case}"
            );
            assert_eq!(
                cas.storage_admission_active_for_test_v1(),
                (0, 0, 0),
                "{case}"
            );
            assert_eq!(
                fs::read_dir(root.join("preparation")).unwrap().count(),
                0,
                "{case}"
            );
            assert_eq!(cas.ensure_valid(), Err(FsCasErrorV1::Invalidated));
            assert_eq!(stale.ensure_valid(), Err(FsCasErrorV1::Invalidated));
            assert!(matches!(
                FsCasV1::open_existing(&root),
                Err(FsCasErrorV1::Busy | FsCasErrorV1::Invalidated)
            ));

            drop(closure_operation);
            drop(capability);
            drop(stale);
            drop(cas);
            assert!(matches!(
                FsCasV1::open_existing(&root),
                Err(FsCasErrorV1::Busy | FsCasErrorV1::Invalidated)
            ));
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[cfg(feature = "c3-polymorphism")]
    #[test]
    fn closure_spool_restores_missing_and_length_mismatch_causes() {
        use crate::cas::{ClosureObjectRecordV1, FileClosureObjectSpoolV1, FsCasClosureSpoolV1};

        for (case, cached_len, exercise_read, expected) in [
            ("missing-len", None, false, FsCasErrorV1::MissingOccupant),
            ("missing-read", None, true, FsCasErrorV1::MissingOccupant),
            ("mismatch-len", Some(1_u64), false, FsCasErrorV1::Integrity),
            ("mismatch-read", Some(1_u64), true, FsCasErrorV1::Integrity),
        ] {
            let parent = std::env::temp_dir().canonicalize().unwrap();
            let root = parent.join(format!(
                "layerfs-closure-spool-cause-{case}-{}-{}",
                std::process::id(),
                NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
            ));
            let cas = FsCasV1::create_new(&root).unwrap();
            let mut control = ContinueFsCasControlV1;
            let storage = cas
                .begin_operation_spool_v1("closure-cause", &mut control)
                .unwrap();
            let mut objects = FileClosureObjectSpoolV1::new(storage);
            let id = TypedPhysicalObjectIdV1::Chunk(
                crate::identity::PhysicalChunkIdV1::from_digest([0x65; 32]),
            );
            objects
                .push(ClosureObjectRecordV1::complete(id, 2))
                .unwrap();

            let mut occupied = cas.occupied_private_controlled_v1(&mut control).unwrap();
            if let Some(cached_len) = cached_len {
                let payload_path = root.join("cached-object");
                fs::write(&payload_path, [0x65]).unwrap();
                occupied.current = Some(ResolvedObjectV1 {
                    id,
                    file: File::open(payload_path).unwrap(),
                    pack_len: cached_len,
                    location: PackObjectLocationV1 {
                        object_offset: 0,
                        object_len: cached_len,
                    },
                });
            }

            let mut closure = FsCasClosureSpoolV1::new(&mut objects, occupied);
            let port_error = if exercise_read {
                closure.read_object_exact_at(0, 0, &mut [0_u8]).err()
            } else {
                closure.object_len_at(0).err()
            };
            assert_eq!(port_error, Some(ImmutablePortErrorV1::Failure), "{case}");
            assert_eq!(
                closure.take_first_error_typed_v1(),
                Some(expected),
                "{case}"
            );
            drop(closure);
            objects.cleanup_controlled_v1(&mut control).unwrap();
            drop(cas);
            fs::remove_dir_all(root).unwrap();
        }
    }

    struct InvalidationCauseControlV1 {
        token_boundary: Option<FsCasFilesystemBoundaryV1>,
        token_error: Option<FsCasErrorV1>,
        marker_error: Option<FsCasErrorV1>,
        skip_token: bool,
    }

    impl FsCasControlV1 for InvalidationCauseControlV1 {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            self.skip_token && target == FsCasCleanupTargetV1::RootInvalidation
        }

        fn inject_filesystem_failure(
            &mut self,
            boundary: FsCasFilesystemBoundaryV1,
        ) -> Option<FsCasErrorV1> {
            if self.token_boundary == Some(boundary) {
                self.token_boundary = None;
                return self.token_error.take();
            }
            if boundary == FsCasFilesystemBoundaryV1::InvalidationMarkerCreate {
                return self.marker_error.take();
            }
            None
        }
    }

    #[test]
    fn invalidation_double_fault_retains_the_first_typed_persistence_cause() {
        for (case, boundary, first_error, first_cause) in [
            (
                "write-permission",
                FsCasFilesystemBoundaryV1::InvalidationWrite,
                FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
                FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
            ),
            (
                "flush-write",
                FsCasFilesystemBoundaryV1::InvalidationFlush,
                FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure),
                FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure),
            ),
        ] {
            let parent = std::env::temp_dir().canonicalize().unwrap();
            let root = parent.join(format!(
                "layerfs-invalidation-cause-{case}-{}-{}",
                std::process::id(),
                NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
            ));
            let cas = FsCasV1::create_new(&root).unwrap();
            let mut control = InvalidationCauseControlV1 {
                token_boundary: Some(boundary),
                token_error: Some(first_error),
                marker_error: Some(FsCasErrorV1::Filesystem(
                    FsCasFilesystemFailureV1::InodeExhaustion,
                )),
                skip_token: false,
            };
            let error = cas.invalidate_root_controlled_v1(&mut control).unwrap_err();
            assert_eq!(
                error.failure_causes_v1(),
                (first_cause, FsCasFailureCauseV1::InvalidationFailed),
                "{case}"
            );
            assert_eq!(cas.ensure_valid(), Err(FsCasErrorV1::Invalidated), "{case}");
            drop(cas);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn invalidation_double_fault_retains_poison_and_wrong_marker_shape() {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        for (case, poison_owner, expected) in [
            (
                "owner-poison",
                true,
                FsCasFailureCauseV1::SynchronizationPoisoned,
            ),
            ("marker-shape", false, FsCasFailureCauseV1::Integrity),
        ] {
            let root = parent.join(format!(
                "layerfs-invalidation-structural-{case}-{}-{}",
                std::process::id(),
                NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
            ));
            let cas = FsCasV1::create_new(&root).unwrap();
            if poison_owner {
                let poison = cas.clone();
                assert!(std::thread::spawn(move || {
                    let _guard = poison.inner.ownership.lock().unwrap();
                    panic!("inject owner-token mutex poison");
                })
                .join()
                .is_err());
            } else {
                fs::write(root.join(INVALIDATED_ROOT_NAME), b"not a directory").unwrap();
            }
            let mut control = InvalidationCauseControlV1 {
                token_boundary: None,
                token_error: None,
                marker_error: poison_owner.then_some(FsCasErrorV1::Filesystem(
                    FsCasFilesystemFailureV1::InodeExhaustion,
                )),
                skip_token: !poison_owner,
            };
            let error = cas.invalidate_root_controlled_v1(&mut control).unwrap_err();
            assert_eq!(
                error.failure_causes_v1(),
                (expected, FsCasFailureCauseV1::InvalidationFailed),
                "{case}"
            );
            assert_eq!(cas.ensure_valid(), Err(FsCasErrorV1::Invalidated), "{case}");
            drop(cas);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[derive(Default)]
    struct PanicFirstTerminalInvalidationV1 {
        attempts: u64,
        panicked: bool,
    }

    impl FsCasControlV1 for PanicFirstTerminalInvalidationV1 {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target == FsCasCleanupTargetV1::RootInvalidation {
                self.attempts = self
                    .attempts
                    .checked_add(1)
                    .expect("terminal invalidation attempt count");
                if !self.panicked {
                    self.panicked = true;
                    panic!("inject first terminal invalidation unwind");
                }
            }
            false
        }
    }

    #[cfg(feature = "c3-polymorphism")]
    #[test]
    fn terminal_half_invalidation_unwind_preserves_first_cause_and_real_backstop_result() {
        for (case, poison_storage, poison_admission) in [
            ("storage", true, false),
            ("admission", false, true),
            ("both", true, true),
        ] {
            for refuse_backstop in [false, true] {
                let parent = std::env::temp_dir().canonicalize().unwrap();
                let root = parent.join(format!(
                    "layerfs-terminal-half-{case}-{refuse_backstop}-{}-{}",
                    std::process::id(),
                    NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
                ));
                let cas = FsCasV1::create_new(&root).unwrap();
                let stale = cas.clone();
                let mut control = PanicFirstTerminalInvalidationV1::default();
                let mut counters = OperationCountersV1::default();
                let mut capability = cas
                    .begin_operation_capability_v1(
                        FsOperationKindV1::CompleteC3File,
                        0x155,
                        &mut counters,
                        &mut control,
                    )
                    .unwrap();
                capability
                    .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(0, 64, 0, 1).unwrap())
                    .unwrap();
                let storage_token = capability.storage_token_v1().unwrap();
                cas.record_storage_immutable_install_v1(storage_token, 64, 1)
                    .unwrap();

                if poison_storage {
                    cas.poison_storage_admission_for_test_v1();
                }
                if poison_admission {
                    cas.poison_operation_admission_for_test_v1();
                }
                if refuse_backstop {
                    // The callback-free invalidation attempt must encounter
                    // two independent failed persistence authorities: a
                    // poisoned owner-token lock and a wrong-shape marker.
                    // The in-memory bit still fails this process closed, but
                    // the returned terminal must report the real double fault.
                    fs::write(root.join(INVALIDATED_ROOT_NAME), b"wrong marker shape").unwrap();
                    let poison = cas.clone();
                    assert!(std::thread::spawn(move || {
                        let _guard = poison.inner.ownership.lock().unwrap();
                        panic!("inject owner-token mutex poison");
                    })
                    .join()
                    .is_err());
                }

                let error = capability
                    .finish_terminal_v1(true, &mut counters, &mut control)
                    .unwrap_err();
                assert_eq!(
                    error.failure_causes_v1(),
                    (
                        FsCasFailureCauseV1::SynchronizationPoisoned,
                        if refuse_backstop {
                            FsCasFailureCauseV1::InvalidationFailed
                        } else {
                            FsCasFailureCauseV1::SynchronizationPoisoned
                        }
                    ),
                    "{case}/{refuse_backstop}"
                );
                assert!(control.panicked, "{case}/{refuse_backstop}");
                assert!(control.attempts >= 1, "{case}/{refuse_backstop}");

                assert_eq!(
                    counters.storage_bytes_requested, 64,
                    "{case}/{refuse_backstop}"
                );
                assert_eq!(
                    counters.storage_bytes_reserved, counters.storage_bytes_requested,
                    "{case}/{refuse_backstop}"
                );
                assert_eq!(
                    counters.storage_bytes_released, 0,
                    "{case}/{refuse_backstop}"
                );
                assert_eq!(
                    counters.storage_bytes_committed, 0,
                    "{case}/{refuse_backstop}"
                );
                assert_eq!(
                    counters.storage_bytes_retained, 64,
                    "{case}/{refuse_backstop}"
                );
                assert_eq!(
                    counters.storage_bytes_reserved,
                    counters.storage_bytes_released
                        + counters.storage_bytes_committed
                        + counters.storage_bytes_retained,
                    "{case}/{refuse_backstop}"
                );
                assert_eq!(
                    counters.storage_inodes_requested, 1,
                    "{case}/{refuse_backstop}"
                );
                assert_eq!(
                    counters.storage_inodes_reserved, counters.storage_inodes_requested,
                    "{case}/{refuse_backstop}"
                );
                assert_eq!(
                    counters.storage_inodes_released, 0,
                    "{case}/{refuse_backstop}"
                );
                assert_eq!(
                    counters.storage_inodes_committed, 0,
                    "{case}/{refuse_backstop}"
                );
                assert_eq!(
                    counters.storage_inodes_retained, 1,
                    "{case}/{refuse_backstop}"
                );
                assert_eq!(
                    counters.storage_inodes_reserved,
                    counters.storage_inodes_released
                        + counters.storage_inodes_committed
                        + counters.storage_inodes_retained,
                    "{case}/{refuse_backstop}"
                );
                assert_eq!(
                    counters.immutable_residue_bytes, 64,
                    "{case}/{refuse_backstop}"
                );
                assert_eq!(
                    counters.immutable_residue_inodes, 1,
                    "{case}/{refuse_backstop}"
                );
                assert_eq!(
                    counters.unreachable_installed_residue_bytes, 64,
                    "{case}/{refuse_backstop}"
                );
                assert!(
                    counters.has_zero_forbidden_work(),
                    "{case}/{refuse_backstop}"
                );
                assert_eq!(
                    cas.operation_admitted_slots_v1(),
                    0,
                    "{case}/{refuse_backstop}"
                );
                assert_eq!(
                    cas.operation_admission_active_for_test_v1(),
                    0,
                    "{case}/{refuse_backstop}"
                );
                assert_eq!(
                    cas.operation_admission_queue_for_test_v1(),
                    (0, 0, 0),
                    "{case}/{refuse_backstop}"
                );
                assert_eq!(
                    cas.storage_admission_active_for_test_v1(),
                    (0, 0, 0),
                    "{case}/{refuse_backstop}"
                );
                assert_eq!(
                    fs::read_dir(root.join("preparation")).unwrap().count(),
                    0,
                    "{case}/{refuse_backstop}"
                );
                assert_eq!(cas.ensure_valid(), Err(FsCasErrorV1::Invalidated));
                assert_eq!(stale.ensure_valid(), Err(FsCasErrorV1::Invalidated));
                assert!(matches!(
                    FsCasV1::open_existing(&root),
                    Err(FsCasErrorV1::Invalidated)
                ));

                drop(capability);
                drop(stale);
                drop(cas);
                assert!(matches!(
                    FsCasV1::open_existing(&root),
                    Err(FsCasErrorV1::Invalidated)
                ));
                fs::remove_dir_all(root).unwrap();
            }
        }
    }

    #[cfg(feature = "c3-polymorphism")]
    #[test]
    fn storage_terminal_counter_failure_consumes_owned_lease_before_fail_closed_return() {
        for poison_storage in [false, true] {
            for fail_invalidation in [false, true] {
                let case = format!("{poison_storage}/{fail_invalidation}");
                let parent = std::env::temp_dir().canonicalize().unwrap();
                let root = parent.join(format!(
                    "layerfs-storage-terminal-counter-failure-{poison_storage}-{fail_invalidation}-{}-{}",
                    std::process::id(),
                    NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
                ));
                let cas = FsCasV1::create_new(&root).unwrap();
                let stale = cas.clone();
                let mut control = AdmissionInvalidationAttemptControlV1 {
                    fail_invalidation,
                    invalidation_attempts: 0,
                };
                let mut counters = OperationCountersV1::default();
                let mut capability = cas
                    .begin_operation_capability_v1(
                        FsOperationKindV1::CompleteC3File,
                        0x15_505,
                        &mut counters,
                        &mut control,
                    )
                    .unwrap();
                capability
                    .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(0, 64, 0, 1).unwrap())
                    .unwrap();

                // The caller-owned terminal observation cannot represent one
                // more requested byte count. Root storage must nevertheless
                // consume its authoritative operation cell exactly once; it
                // must not leave that cell for a result-discarding Drop retry.
                counters.storage_bytes_requested = u64::MAX;
                if poison_storage {
                    cas.poison_storage_admission_for_test_v1();
                }

                let error = capability
                    .finish_terminal_v1(false, &mut counters, &mut control)
                    .unwrap_err();
                let first = if poison_storage {
                    FsCasFailureCauseV1::SynchronizationPoisoned
                } else {
                    FsCasFailureCauseV1::Core(CoreError::IntegerOverflow)
                };
                assert_eq!(
                    error.failure_causes_v1(),
                    (
                        first,
                        if fail_invalidation {
                            FsCasFailureCauseV1::InvalidationFailed
                        } else {
                            first
                        },
                    ),
                    "{case}"
                );
                assert_eq!(control.invalidation_attempts, 1, "{case}");

                // The overflowing caller observation is left unchanged. No
                // partial or fabricated equation is presented as evidence,
                // while every authoritative root-owned slot is terminal.
                assert_eq!(counters.storage_bytes_requested, u64::MAX, "{case}");
                assert_eq!(counters.storage_bytes_reserved, 0, "{case}");
                assert_eq!(counters.storage_bytes_released, 0, "{case}");
                assert_eq!(counters.storage_bytes_committed, 0, "{case}");
                assert_eq!(counters.storage_bytes_retained, 0, "{case}");
                assert_eq!(counters.storage_inodes_requested, 0, "{case}");
                assert_eq!(counters.storage_inodes_reserved, 0, "{case}");
                assert_eq!(counters.storage_inodes_released, 0, "{case}");
                assert_eq!(counters.storage_inodes_committed, 0, "{case}");
                assert_eq!(counters.storage_inodes_retained, 0, "{case}");
                assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
                assert_eq!(cas.operation_admitted_slots_v1(), 0, "{case}");
                assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{case}");
                assert_eq!(cas.operation_admission_queue_for_test_v1(), (0, 0, 0));
                assert_eq!(fs::read_dir(root.join("preparation")).unwrap().count(), 0);
                assert!(counters.has_zero_forbidden_work(), "{case}");
                assert_eq!(cas.ensure_valid(), Err(FsCasErrorV1::Invalidated));
                assert_eq!(stale.ensure_valid(), Err(FsCasErrorV1::Invalidated));
                assert!(matches!(
                    FsCasV1::open_existing(&root),
                    Err(FsCasErrorV1::Busy | FsCasErrorV1::Invalidated)
                ));

                // Dropping the already terminal capability cannot reinterpret
                // the failed observation or reintroduce root-owned state.
                drop(capability);
                assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
                assert_eq!(cas.operation_admitted_slots_v1(), 0, "{case}");
                assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{case}");
                assert_eq!(cas.operation_admission_queue_for_test_v1(), (0, 0, 0));

                drop(stale);
                drop(cas);
                fs::remove_dir_all(root).unwrap();
            }
        }
    }

    #[cfg(feature = "c3-polymorphism")]
    #[test]
    fn storage_terminal_reservation_underflow_reconciles_and_consumes_owned_lease() {
        for fail_invalidation in [false, true] {
            let case = format!("reservation-underflow/{fail_invalidation}");
            let parent = std::env::temp_dir().canonicalize().unwrap();
            let root = parent.join(format!(
                "layerfs-storage-terminal-reservation-underflow-{fail_invalidation}-{}-{}",
                std::process::id(),
                NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
            ));
            let cas = FsCasV1::create_new(&root).unwrap();
            let stale = cas.clone();
            let mut control = AdmissionInvalidationAttemptControlV1 {
                fail_invalidation,
                invalidation_attempts: 0,
            };
            let mut counters = OperationCountersV1::default();
            let mut capability = cas
                .begin_operation_capability_v1(
                    FsOperationKindV1::CompleteC3File,
                    0x15_506,
                    &mut counters,
                    &mut control,
                )
                .unwrap();
            capability
                .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(0, 64, 0, 1).unwrap())
                .unwrap();

            // Corrupt only the aggregate below the still-authoritative live
            // operation envelope. The ordinary terminal must fail, while the
            // owned fallback reconstructs the exact reservation from the
            // remaining operation cells and consumes this lease once.
            {
                let mut state = cas.inner.storage_admission.state.lock().unwrap();
                assert_eq!(
                    state.active_reserved,
                    RootStorageUsageV1 {
                        bytes: 64,
                        inodes: 1,
                    },
                    "{case}"
                );
                assert_eq!(
                    state
                        .operations
                        .iter()
                        .filter(|operation| operation.active)
                        .count(),
                    1,
                    "{case}"
                );
                state.active_reserved = RootStorageUsageV1::default();
            }

            let error = capability
                .finish_terminal_v1(false, &mut counters, &mut control)
                .unwrap_err();
            assert_eq!(
                error.failure_causes_v1(),
                (
                    FsCasFailureCauseV1::Integrity,
                    if fail_invalidation {
                        FsCasFailureCauseV1::InvalidationFailed
                    } else {
                        FsCasFailureCauseV1::Integrity
                    },
                ),
                "{case}"
            );
            assert_eq!(control.invalidation_attempts, 1, "{case}");

            // No terminal equation is fabricated from the corrupt aggregate,
            // but the root-owned storage cell, queue authority, and memory
            // slot are all explicitly terminal before this Result returns.
            assert_eq!(counters.storage_bytes_requested, 0, "{case}");
            assert_eq!(counters.storage_bytes_reserved, 0, "{case}");
            assert_eq!(counters.storage_bytes_released, 0, "{case}");
            assert_eq!(counters.storage_bytes_committed, 0, "{case}");
            assert_eq!(counters.storage_bytes_retained, 0, "{case}");
            assert_eq!(counters.storage_inodes_requested, 0, "{case}");
            assert_eq!(counters.storage_inodes_reserved, 0, "{case}");
            assert_eq!(counters.storage_inodes_released, 0, "{case}");
            assert_eq!(counters.storage_inodes_committed, 0, "{case}");
            assert_eq!(counters.storage_inodes_retained, 0, "{case}");
            assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
            assert_eq!(cas.operation_admitted_slots_v1(), 0, "{case}");
            assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{case}");
            assert_eq!(cas.operation_admission_queue_for_test_v1(), (0, 0, 0));
            assert_eq!(fs::read_dir(root.join("preparation")).unwrap().count(), 0);
            assert!(counters.has_zero_forbidden_work(), "{case}");
            assert_eq!(cas.ensure_valid(), Err(FsCasErrorV1::Invalidated));
            assert_eq!(stale.ensure_valid(), Err(FsCasErrorV1::Invalidated));
            assert!(matches!(
                FsCasV1::open_existing(&root),
                Err(FsCasErrorV1::Busy | FsCasErrorV1::Invalidated)
            ));

            // The owned storage lease was consumed despite the failed
            // observation, so capability Drop cannot retry or revive it.
            drop(capability);
            assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
            assert_eq!(cas.operation_admitted_slots_v1(), 0, "{case}");
            assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{case}");
            assert_eq!(cas.operation_admission_queue_for_test_v1(), (0, 0, 0));

            drop(stale);
            drop(cas);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[cfg(feature = "c3-polymorphism")]
    #[test]
    fn storage_terminal_custody_overflow_quarantines_snapshot_and_retires_lease() {
        for custody in ["immutable", "preparation"] {
            for fail_invalidation in [false, true] {
                let case = format!("{custody}/{fail_invalidation}");
                let parent = std::env::temp_dir().canonicalize().unwrap();
                let root = parent.join(format!(
                    "layerfs-storage-terminal-custody-overflow-{custody}-{fail_invalidation}-{}-{}",
                    std::process::id(),
                    NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
                ));
                let cas = FsCasV1::create_new(&root).unwrap();
                let stale = cas.clone();
                let mut control = AdmissionInvalidationAttemptControlV1 {
                    fail_invalidation,
                    invalidation_attempts: 0,
                };
                let mut counters = OperationCountersV1::default();
                let mut capability = cas
                    .begin_operation_capability_v1(
                        FsOperationKindV1::CompleteC3File,
                        0x15_507,
                        &mut counters,
                        &mut control,
                    )
                    .unwrap();
                let envelope = if custody == "immutable" {
                    FsStorageEnvelopeV1::new(0, 64, 0, 1).unwrap()
                } else {
                    FsStorageEnvelopeV1::new(64, 0, 1, 0).unwrap()
                };
                capability.declare_storage_envelope_v1(envelope).unwrap();
                let token = capability.storage_token_v1().unwrap();
                if custody == "immutable" {
                    cas.record_storage_immutable_install_v1(token, 64, 1)
                        .unwrap();
                } else {
                    cas.record_storage_preparation_create_v1(token).unwrap();
                    cas.record_storage_preparation_length_v1(token, 0, 64)
                        .unwrap();
                }

                // Make only one root-wide retained-custody aggregate unable
                // to represent this operation's exact 64-byte contribution.
                // The operation-relative snapshot itself remains bounded and
                // must be quarantined before its active slot is retired.
                let (immutable_before, preparation_before) = {
                    let mut state = cas.inner.storage_admission.state.lock().unwrap();
                    if custody == "immutable" {
                        state.immutable.bytes = u64::MAX - 31;
                    } else {
                        state.preparation.bytes = u64::MAX - 31;
                    }
                    (state.immutable, state.preparation)
                };

                let error = capability
                    .finish_terminal_v1(false, &mut counters, &mut control)
                    .unwrap_err();
                assert_eq!(
                    error.failure_causes_v1(),
                    (
                        FsCasFailureCauseV1::Core(CoreError::IntegerOverflow),
                        if fail_invalidation {
                            FsCasFailureCauseV1::InvalidationFailed
                        } else {
                            FsCasFailureCauseV1::Core(CoreError::IntegerOverflow)
                        },
                    ),
                    "{case}"
                );
                assert_eq!(control.invalidation_attempts, 1, "{case}");

                // The caller-owned equation is intentionally unavailable:
                // no wrapped, saturated, or partial terminal observation is
                // fabricated. The root instead retains the exact bounded
                // operation-relative custody outside all active slots.
                assert_eq!(counters.storage_bytes_requested, 0, "{case}");
                assert_eq!(counters.storage_bytes_reserved, 0, "{case}");
                assert_eq!(counters.storage_bytes_released, 0, "{case}");
                assert_eq!(counters.storage_bytes_committed, 0, "{case}");
                assert_eq!(counters.storage_bytes_retained, 0, "{case}");
                assert_eq!(counters.storage_inodes_requested, 0, "{case}");
                assert_eq!(counters.storage_inodes_reserved, 0, "{case}");
                assert_eq!(counters.storage_inodes_released, 0, "{case}");
                assert_eq!(counters.storage_inodes_committed, 0, "{case}");
                assert_eq!(counters.storage_inodes_retained, 0, "{case}");
                {
                    let state = cas.inner.storage_admission.state.lock().unwrap();
                    assert_eq!(
                        state.active_reserved,
                        RootStorageUsageV1::default(),
                        "{case}"
                    );
                    assert!(
                        state.operations.iter().all(|operation| !operation.active),
                        "{case}"
                    );
                    assert_eq!(state.immutable, immutable_before, "{case}");
                    assert_eq!(state.preparation, preparation_before, "{case}");
                    let terminals = state
                        .unclassified_terminals
                        .iter()
                        .filter(|terminal| terminal.occupied)
                        .collect::<Vec<_>>();
                    assert_eq!(terminals.len(), 1, "{case}");
                    let terminal = terminals[0];
                    assert_eq!(terminal.nonce, token.nonce, "{case}");
                    assert_eq!(
                        terminal.reservation,
                        RootStorageUsageV1 {
                            bytes: 64,
                            inodes: 1,
                        },
                        "{case}"
                    );
                    assert!(terminal.reservation_known, "{case}");
                    assert!(terminal.active_reserved_rebuilt, "{case}");
                    assert_eq!(
                        terminal.immutable,
                        if custody == "immutable" {
                            RootStorageUsageV1 {
                                bytes: 64,
                                inodes: 1,
                            }
                        } else {
                            RootStorageUsageV1::default()
                        },
                        "{case}"
                    );
                    assert_eq!(
                        terminal.preparation,
                        if custody == "preparation" {
                            RootStorageUsageV1 {
                                bytes: 64,
                                inodes: 1,
                            }
                        } else {
                            RootStorageUsageV1::default()
                        },
                        "{case}"
                    );
                    assert_eq!(
                        terminal.immutable_aggregated,
                        custody != "immutable",
                        "{case}"
                    );
                    assert_eq!(
                        terminal.preparation_aggregated,
                        custody != "preparation",
                        "{case}"
                    );
                }
                assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
                assert_eq!(cas.operation_admitted_slots_v1(), 0, "{case}");
                assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{case}");
                assert_eq!(cas.operation_admission_queue_for_test_v1(), (0, 0, 0));
                assert_eq!(fs::read_dir(root.join("preparation")).unwrap().count(), 0);
                assert!(counters.has_zero_forbidden_work(), "{case}");
                assert_eq!(cas.ensure_valid(), Err(FsCasErrorV1::Invalidated));
                assert_eq!(stale.ensure_valid(), Err(FsCasErrorV1::Invalidated));
                assert!(matches!(
                    FsCasV1::open_existing(&root),
                    Err(FsCasErrorV1::Busy | FsCasErrorV1::Invalidated)
                ));

                // Capability Drop observes an already retired storage lease;
                // it cannot retry or reinterpret the quarantined terminal.
                drop(capability);
                assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
                assert_eq!(cas.operation_admitted_slots_v1(), 0, "{case}");
                assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{case}");
                assert_eq!(cas.operation_admission_queue_for_test_v1(), (0, 0, 0));

                drop(stale);
                drop(cas);
                fs::remove_dir_all(root).unwrap();
            }
        }
    }

    #[cfg(feature = "c3-polymorphism")]
    #[test]
    fn storage_terminal_sibling_reservation_failure_retires_only_owned_lease() {
        for fail_invalidation in [false, true] {
            let case = format!("missing-sibling-envelope/{fail_invalidation}");
            let parent = std::env::temp_dir().canonicalize().unwrap();
            let root = parent.join(format!(
                "layerfs-storage-terminal-sibling-reservation-{fail_invalidation}-{}-{}",
                std::process::id(),
                NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
            ));
            let cas = FsCasV1::create_new(&root).unwrap();
            let stale = cas.clone();
            let mut begin_control = ContinueFsCasControlV1;
            let mut first_counters = OperationCountersV1::default();
            let mut sibling_counters = OperationCountersV1::default();
            let mut first = cas
                .begin_operation_capability_v1(
                    FsOperationKindV1::CompleteC3File,
                    0x15_508,
                    &mut first_counters,
                    &mut begin_control,
                )
                .unwrap();
            let mut sibling = cas
                .begin_operation_capability_v1(
                    FsOperationKindV1::CompleteC3Tree,
                    0x15_509,
                    &mut sibling_counters,
                    &mut begin_control,
                )
                .unwrap();
            let envelope = FsStorageEnvelopeV1::new(0, 64, 0, 1).unwrap();
            first.declare_storage_envelope_v1(envelope).unwrap();
            sibling.declare_storage_envelope_v1(envelope).unwrap();
            let first_token = first.storage_token_v1().unwrap();
            let sibling_token = sibling.storage_token_v1().unwrap();

            // Make the first lease's ordinary terminal fail while a distinct
            // live sibling remains authoritative but has lost its envelope.
            // The fallback must not return before retiring the owned first
            // cell, and it must not invent a replacement aggregate for the
            // still-live sibling.
            let preserved_active_reserved = RootStorageUsageV1 {
                bytes: 63,
                inodes: 2,
            };
            {
                let mut state = cas.inner.storage_admission.state.lock().unwrap();
                assert_eq!(
                    state.active_reserved,
                    RootStorageUsageV1 {
                        bytes: 128,
                        inodes: 2,
                    },
                    "{case}"
                );
                assert_eq!(
                    state
                        .operations
                        .iter()
                        .filter(|operation| operation.active)
                        .count(),
                    2,
                    "{case}"
                );
                state.operations[usize::from(sibling_token.slot)].envelope = None;
                state.active_reserved = preserved_active_reserved;
            }

            let mut control = AdmissionInvalidationAttemptControlV1 {
                fail_invalidation,
                invalidation_attempts: 0,
            };
            let error = first
                .finish_terminal_v1(false, &mut first_counters, &mut control)
                .unwrap_err();
            assert_eq!(
                error.failure_causes_v1(),
                (
                    FsCasFailureCauseV1::Integrity,
                    if fail_invalidation {
                        FsCasFailureCauseV1::InvalidationFailed
                    } else {
                        FsCasFailureCauseV1::Integrity
                    },
                ),
                "{case}"
            );
            assert_eq!(control.invalidation_attempts, 1, "{case}");
            assert!(first_counters.has_zero_forbidden_work(), "{case}");
            {
                let state = cas.inner.storage_admission.state.lock().unwrap();
                assert_eq!(state.active_reserved, preserved_active_reserved, "{case}");
                assert!(
                    !state.operations[usize::from(first_token.slot)].active,
                    "{case}"
                );
                let sibling_state = state.operations[usize::from(sibling_token.slot)];
                assert!(sibling_state.active, "{case}");
                assert_eq!(sibling_state.nonce, sibling_token.nonce, "{case}");
                assert_eq!(sibling_state.envelope, None, "{case}");
                let terminals = state
                    .unclassified_terminals
                    .iter()
                    .filter(|terminal| terminal.occupied)
                    .collect::<Vec<_>>();
                assert_eq!(terminals.len(), 1, "{case}");
                let terminal = terminals[0];
                assert_eq!(terminal.nonce, first_token.nonce, "{case}");
                assert_eq!(
                    terminal.reservation,
                    RootStorageUsageV1 {
                        bytes: 64,
                        inodes: 1
                    }
                );
                assert!(terminal.reservation_known, "{case}");
                assert!(!terminal.active_reserved_rebuilt, "{case}");
                assert_eq!(terminal.immutable, RootStorageUsageV1::default(), "{case}");
                assert_eq!(
                    terminal.preparation,
                    RootStorageUsageV1::default(),
                    "{case}"
                );
            }
            assert_eq!(cas.operation_admitted_slots_v1(), 1, "{case}");
            assert_eq!(cas.operation_admission_active_for_test_v1(), 1, "{case}");
            assert_eq!(cas.storage_admission_active_for_test_v1(), (1, 63, 2));

            // Quarantine blocks every new root storage reservation even
            // though the remaining operation still owns its original outer
            // queue/memory authority.
            assert!(matches!(
                cas.inner.storage_admission.reserve(envelope),
                Err(FsCasErrorV1::Integrity)
            ));
            drop(first);
            assert_eq!(cas.operation_admitted_slots_v1(), 1, "{case}");
            assert_eq!(cas.storage_admission_active_for_test_v1(), (1, 63, 2));

            // The malformed sibling is later terminalized by its own lease.
            // Its missing envelope is represented explicitly rather than
            // guessed; the last active slot and reservation aggregate return
            // to baseline without retrying the already retired first lease.
            let mut finish_control = ContinueFsCasControlV1;
            assert_eq!(
                sibling
                    .finish_terminal_v1(false, &mut sibling_counters, &mut finish_control)
                    .unwrap_err()
                    .failure_causes_v1(),
                (
                    FsCasFailureCauseV1::Integrity,
                    FsCasFailureCauseV1::Integrity,
                ),
                "{case}"
            );
            assert!(sibling_counters.has_zero_forbidden_work(), "{case}");
            {
                let state = cas.inner.storage_admission.state.lock().unwrap();
                assert_eq!(
                    state.active_reserved,
                    RootStorageUsageV1::default(),
                    "{case}"
                );
                assert!(
                    state.operations.iter().all(|operation| !operation.active),
                    "{case}"
                );
                let terminals = state
                    .unclassified_terminals
                    .iter()
                    .filter(|terminal| terminal.occupied)
                    .collect::<Vec<_>>();
                assert_eq!(terminals.len(), 2, "{case}");
                let sibling_terminal = terminals
                    .into_iter()
                    .find(|terminal| terminal.nonce == sibling_token.nonce)
                    .unwrap();
                assert_eq!(sibling_terminal.reservation, RootStorageUsageV1::default());
                assert!(!sibling_terminal.reservation_known, "{case}");
                assert!(sibling_terminal.active_reserved_rebuilt, "{case}");
            }
            assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
            assert_eq!(cas.operation_admitted_slots_v1(), 0, "{case}");
            assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{case}");
            assert_eq!(cas.operation_admission_queue_for_test_v1(), (0, 0, 0));
            assert_eq!(cas.ensure_valid(), Err(FsCasErrorV1::Invalidated));
            assert_eq!(stale.ensure_valid(), Err(FsCasErrorV1::Invalidated));
            assert!(matches!(
                FsCasV1::open_existing(&root),
                Err(FsCasErrorV1::Busy | FsCasErrorV1::Invalidated)
            ));

            drop(sibling);
            drop(stale);
            drop(cas);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[cfg(feature = "c3-polymorphism")]
    #[test]
    fn storage_terminal_sibling_reservation_overflow_retires_owned_leases_in_order() {
        for fail_invalidation in [false, true] {
            let case = format!("sibling-reservation-overflow/{fail_invalidation}");
            let parent = std::env::temp_dir().canonicalize().unwrap();
            let root = parent.join(format!(
                "layerfs-storage-terminal-sibling-overflow-{fail_invalidation}-{}-{}",
                std::process::id(),
                NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
            ));
            let cas = FsCasV1::create_new(&root).unwrap();
            let stale = cas.clone();
            let mut begin_control = ContinueFsCasControlV1;
            let mut first_counters = OperationCountersV1::default();
            let mut large_counters = OperationCountersV1::default();
            let mut last_counters = OperationCountersV1::default();
            let mut first = cas
                .begin_operation_capability_v1(
                    FsOperationKindV1::CompleteC3File,
                    0x15_510,
                    &mut first_counters,
                    &mut begin_control,
                )
                .unwrap();
            let mut large = cas
                .begin_operation_capability_v1(
                    FsOperationKindV1::CompleteC3Tree,
                    0x15_511,
                    &mut large_counters,
                    &mut begin_control,
                )
                .unwrap();
            let mut last = cas
                .begin_operation_capability_v1(
                    FsOperationKindV1::RootExtraction,
                    0x15_512,
                    &mut last_counters,
                    &mut begin_control,
                )
                .unwrap();
            let envelope = FsStorageEnvelopeV1::new(0, 64, 0, 1).unwrap();
            first.declare_storage_envelope_v1(envelope).unwrap();
            large.declare_storage_envelope_v1(envelope).unwrap();
            last.declare_storage_envelope_v1(envelope).unwrap();
            let first_token = first.storage_token_v1().unwrap();
            let large_token = large.storage_token_v1().unwrap();
            let last_token = last.storage_token_v1().unwrap();

            // Two remaining live sibling envelopes are individually valid
            // bounded values but not additively representable together. The
            // first fallback must scan the complete fixed state, quarantine
            // its own terminal, and preserve the last aggregate rather than
            // wrapping, saturating, or abandoning its owned cell.
            let huge_envelope = FsStorageEnvelopeV1::new(0, u64::MAX, 0, 0).unwrap();
            let preserved_active_reserved = RootStorageUsageV1 {
                bytes: 63,
                inodes: 3,
            };
            {
                let mut state = cas.inner.storage_admission.state.lock().unwrap();
                state.operations[usize::from(large_token.slot)].envelope = Some(huge_envelope);
                state.active_reserved = preserved_active_reserved;
            }

            let mut control = AdmissionInvalidationAttemptControlV1 {
                fail_invalidation,
                invalidation_attempts: 0,
            };
            let error = first
                .finish_terminal_v1(false, &mut first_counters, &mut control)
                .unwrap_err();
            assert_eq!(
                error.failure_causes_v1(),
                (
                    FsCasFailureCauseV1::Integrity,
                    if fail_invalidation {
                        FsCasFailureCauseV1::InvalidationFailed
                    } else {
                        FsCasFailureCauseV1::Integrity
                    },
                ),
                "{case}"
            );
            assert_eq!(control.invalidation_attempts, 1, "{case}");
            assert!(first_counters.has_zero_forbidden_work(), "{case}");
            {
                let state = cas.inner.storage_admission.state.lock().unwrap();
                assert_eq!(state.active_reserved, preserved_active_reserved, "{case}");
                assert!(
                    !state.operations[usize::from(first_token.slot)].active,
                    "{case}"
                );
                assert!(
                    state.operations[usize::from(large_token.slot)].active,
                    "{case}"
                );
                assert!(
                    state.operations[usize::from(last_token.slot)].active,
                    "{case}"
                );
                let terminal = state.unclassified_terminals[usize::from(first_token.slot)];
                assert!(terminal.occupied, "{case}");
                assert_eq!(terminal.nonce, first_token.nonce, "{case}");
                assert_eq!(
                    terminal.reservation,
                    RootStorageUsageV1 {
                        bytes: 64,
                        inodes: 1
                    }
                );
                assert!(terminal.reservation_known, "{case}");
                assert!(!terminal.active_reserved_rebuilt, "{case}");
            }
            assert!(matches!(
                cas.inner.storage_admission.reserve(envelope),
                Err(FsCasErrorV1::Integrity)
            ));
            drop(first);
            assert_eq!(cas.storage_admission_active_for_test_v1(), (2, 63, 3));
            assert_eq!(cas.operation_admitted_slots_v1(), 2, "{case}");

            // Retiring the huge sibling has one exactly reconstructable
            // remaining reservation. Its own requested contribution is
            // recorded verbatim even though adding it to that sibling would
            // overflow. The aggregate is rebuilt to the last live lease.
            let mut finish_control = ContinueFsCasControlV1;
            let large_error = large
                .finish_terminal_v1(false, &mut large_counters, &mut finish_control)
                .unwrap_err();
            assert_eq!(
                large_error.failure_causes_v1(),
                (
                    FsCasFailureCauseV1::Integrity,
                    FsCasFailureCauseV1::Integrity,
                ),
                "{case}"
            );
            assert!(large_counters.has_zero_forbidden_work(), "{case}");
            {
                let state = cas.inner.storage_admission.state.lock().unwrap();
                assert_eq!(
                    state.active_reserved,
                    RootStorageUsageV1 {
                        bytes: 64,
                        inodes: 1
                    }
                );
                assert!(
                    !state.operations[usize::from(large_token.slot)].active,
                    "{case}"
                );
                assert!(
                    state.operations[usize::from(last_token.slot)].active,
                    "{case}"
                );
                let terminal = state.unclassified_terminals[usize::from(large_token.slot)];
                assert!(terminal.occupied, "{case}");
                assert_eq!(terminal.nonce, large_token.nonce, "{case}");
                assert_eq!(
                    terminal.reservation,
                    RootStorageUsageV1 {
                        bytes: u64::MAX,
                        inodes: 0
                    }
                );
                assert!(terminal.reservation_known, "{case}");
                assert!(terminal.active_reserved_rebuilt, "{case}");
            }
            drop(large);

            // The final well-formed sibling retains its own authority until
            // its ordinary terminal, then closes the root-owned aggregate and
            // every outer admission resource exactly.
            last.finish_terminal_v1(false, &mut last_counters, &mut finish_control)
                .unwrap();
            assert_eq!(last_counters.storage_bytes_requested, 64, "{case}");
            assert_eq!(last_counters.storage_bytes_reserved, 64, "{case}");
            assert_eq!(last_counters.storage_bytes_released, 64, "{case}");
            assert_eq!(last_counters.storage_bytes_committed, 0, "{case}");
            assert_eq!(last_counters.storage_bytes_retained, 0, "{case}");
            assert_eq!(last_counters.storage_inodes_requested, 1, "{case}");
            assert_eq!(last_counters.storage_inodes_reserved, 1, "{case}");
            assert_eq!(last_counters.storage_inodes_released, 1, "{case}");
            assert_eq!(last_counters.storage_inodes_committed, 0, "{case}");
            assert_eq!(last_counters.storage_inodes_retained, 0, "{case}");
            assert!(last_counters.has_zero_forbidden_work(), "{case}");
            assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
            assert_eq!(cas.operation_admitted_slots_v1(), 0, "{case}");
            assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{case}");
            assert_eq!(cas.operation_admission_queue_for_test_v1(), (0, 0, 0));
            assert_eq!(cas.ensure_valid(), Err(FsCasErrorV1::Invalidated));
            assert_eq!(stale.ensure_valid(), Err(FsCasErrorV1::Invalidated));
            assert!(matches!(
                FsCasV1::open_existing(&root),
                Err(FsCasErrorV1::Busy | FsCasErrorV1::Invalidated)
            ));

            drop(last);
            drop(stale);
            drop(cas);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[derive(Default)]
    struct CancelControlV1;

    impl FsCasControlV1 for CancelControlV1 {
        fn cancellation_requested(&mut self) -> bool {
            true
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    #[derive(Default)]
    struct DeadlineControlV1;

    impl FsCasControlV1 for DeadlineControlV1 {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            true
        }
    }

    #[test]
    fn phase_one_ticket_cells_are_exact_and_queue_exhaustion_is_typed() {
        assert_eq!(core::mem::size_of::<QueueTicketV1>(), 256);
        let queue = OperationAdmissionQueueV1::new(0).unwrap();
        let mut pending = Vec::with_capacity(MAX_ADMISSION_TICKETS);
        let mut counters = OperationCountersV1::default();
        for ordinal in 0..MAX_ADMISSION_TICKETS {
            pending.push(
                queue
                    .issue(
                        FsOperationKindV1::CompleteC3File,
                        ordinal as u64,
                        &mut counters,
                    )
                    .unwrap(),
            );
            let state = queue.state.lock().unwrap();
            let slot = ordinal % MAX_ADMISSION_TICKETS;
            assert_eq!(
                state.queue_tickets[slot].operation_kind,
                FsOperationKindV1::CompleteC3File as u64
            );
            assert_eq!(state.queue_tickets[slot].cancellation_key, ordinal as u64);
        }
        assert!(matches!(
            queue.issue(FsOperationKindV1::CompleteC3Tree, u64::MAX, &mut counters,),
            Err(OperationAdmissionIssueFailureV1 {
                first: FsCasErrorV1::ResourceExhausted(FsCasResourceV1::Queue),
                observation_failed: false,
            })
        ));
        drop(pending);
        let state = queue.state.lock().unwrap();
        assert_eq!(state.next_ticket, state.serving_ticket);
        assert_eq!(state.active, 0);
        assert!(state
            .tickets
            .iter()
            .all(|ticket| *ticket == AdmissionTicketStateV1::Empty));
        assert!(state
            .queue_tickets
            .iter()
            .all(|ticket| { ticket.operation_kind == 0 && ticket.cancellation_key == 0 }));
    }

    struct AdmissionInvalidationAttemptControlV1 {
        fail_invalidation: bool,
        invalidation_attempts: u64,
    }

    impl FsCasControlV1 for AdmissionInvalidationAttemptControlV1 {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target != FsCasCleanupTargetV1::RootInvalidation {
                return false;
            }
            self.invalidation_attempts = self
                .invalidation_attempts
                .checked_add(1)
                .expect("root invalidation attempt count");
            self.fail_invalidation
        }

        fn inject_filesystem_failure(
            &mut self,
            boundary: FsCasFilesystemBoundaryV1,
        ) -> Option<FsCasErrorV1> {
            (self.fail_invalidation
                && boundary == FsCasFilesystemBoundaryV1::InvalidationMarkerCreate)
                .then_some(FsCasErrorV1::Filesystem(
                    FsCasFilesystemFailureV1::InodeExhaustion,
                ))
        }
    }

    const QUEUED_CONTROL_UNWIND_PAYLOAD_V1: &str = "queued control unwind payload";

    struct QueuedControlUnwindRetirementControlV1 {
        retirement_failure: Option<FsCasErrorV1>,
        fail_invalidation: bool,
        invalidation_attempts: u64,
    }

    impl FsCasControlV1 for QueuedControlUnwindRetirementControlV1 {
        fn cancellation_requested(&mut self) -> bool {
            std::panic::panic_any(QUEUED_CONTROL_UNWIND_PAYLOAD_V1)
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target != FsCasCleanupTargetV1::RootInvalidation {
                return false;
            }
            self.invalidation_attempts = self
                .invalidation_attempts
                .checked_add(1)
                .expect("queued-unwind invalidation attempt count");
            self.fail_invalidation
        }

        fn inject_filesystem_failure(
            &mut self,
            boundary: FsCasFilesystemBoundaryV1,
        ) -> Option<FsCasErrorV1> {
            (self.fail_invalidation
                && boundary == FsCasFilesystemBoundaryV1::InvalidationMarkerCreate)
                .then_some(FsCasErrorV1::Filesystem(
                    FsCasFilesystemFailureV1::InodeExhaustion,
                ))
        }

        fn inject_pending_unwind_retirement_failure(&mut self) -> Option<FsCasErrorV1> {
            self.retirement_failure.take()
        }
    }

    #[derive(Clone, Copy)]
    enum RootLockWaitTerminalModeV1 {
        Continue,
        Cancelled,
        Deadline,
    }

    struct RootLockWaitTerminalPanicControlV1 {
        terminated_boundary: FsCasBoundaryV1,
        mode: RootLockWaitTerminalModeV1,
        terminal_observations: u64,
        fail_invalidation: bool,
        invalidation_attempts: u64,
        invalidate_after_acquire: Option<FsCasV1>,
    }

    impl FsCasControlV1 for RootLockWaitTerminalPanicControlV1 {
        fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
            if boundary == self.terminated_boundary {
                self.terminal_observations = self
                    .terminal_observations
                    .checked_add(1)
                    .expect("root-lock terminal observation count");
                std::panic::panic_any("root-lock wait terminal observation unwind");
            }
        }

        fn cancellation_requested(&mut self) -> bool {
            matches!(self.mode, RootLockWaitTerminalModeV1::Cancelled)
        }

        fn deadline_exceeded(&mut self) -> bool {
            matches!(self.mode, RootLockWaitTerminalModeV1::Deadline)
        }

        fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
            if target != FsCasCleanupTargetV1::RootInvalidation {
                return false;
            }
            self.invalidation_attempts = self
                .invalidation_attempts
                .checked_add(1)
                .expect("root-lock invalidation attempt count");
            self.fail_invalidation
        }

        fn inject_root_lock_post_acquire_validation_failure(&mut self) -> Option<FsCasErrorV1> {
            let cas = self.invalidate_after_acquire.take()?;
            cas.invalidate_root_backstop_v1();
            Some(FsCasErrorV1::Invalidated)
        }
    }

    #[test]
    fn root_lock_wait_terminal_observation_unwind_preserves_cancel_and_deadline() {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let root = parent.join(format!(
            "layerfs-root-lock-stop-terminal-{}-{}",
            std::process::id(),
            NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
        ));
        let cas = FsCasV1::create_new(&root).unwrap();

        for (case, publication, mode, expected) in [
            (
                "visibility-cancelled",
                false,
                RootLockWaitTerminalModeV1::Cancelled,
                FsCasErrorV1::Core(CoreError::Cancelled),
            ),
            (
                "visibility-deadline",
                false,
                RootLockWaitTerminalModeV1::Deadline,
                FsCasErrorV1::Core(CoreError::Deadline),
            ),
            (
                "publication-cancelled",
                true,
                RootLockWaitTerminalModeV1::Cancelled,
                FsCasErrorV1::Core(CoreError::Cancelled),
            ),
            (
                "publication-deadline",
                true,
                RootLockWaitTerminalModeV1::Deadline,
                FsCasErrorV1::Core(CoreError::Deadline),
            ),
        ] {
            let terminated_boundary = if publication {
                FsCasBoundaryV1::PublicationLockWaitTerminated
            } else {
                FsCasBoundaryV1::VisibilityLockWaitTerminated
            };
            let held = if publication {
                cas.inner.publication.lock().unwrap()
            } else {
                cas.inner.visibility.lock().unwrap()
            };
            let mut control = RootLockWaitTerminalPanicControlV1 {
                terminated_boundary,
                mode,
                terminal_observations: 0,
                fail_invalidation: false,
                invalidation_attempts: 0,
                invalidate_after_acquire: None,
            };
            let mut observed = FsOperationObservedControlV1::new(&mut control);
            let error = if publication {
                cas.lock_publication_controlled_v1(&mut observed)
                    .map(drop)
                    .unwrap_err()
            } else {
                cas.lock_visibility_controlled_v1(&mut observed)
                    .map(drop)
                    .unwrap_err()
            };
            drop(held);
            assert_eq!(error, expected, "{case}");
            let mut counters = OperationCountersV1::default();
            observed.finish_v1(&mut counters).unwrap();
            assert_eq!(control.terminal_observations, 1, "{case}");
            assert_eq!(control.invalidation_attempts, 0, "{case}");
            assert!(counters.has_zero_forbidden_work(), "{case}");
            assert_eq!(cas.ensure_valid(), Ok(()), "{case}");
            let available = if publication {
                cas.inner.publication.try_lock().is_ok()
            } else {
                cas.inner.visibility.try_lock().is_ok()
            };
            assert!(available, "{case}: root lock remained held or poisoned");
        }

        drop(cas);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn root_lock_wait_terminal_observation_unwind_preserves_invalidation() {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        for (case, publication, after_acquire) in [
            ("visibility-before", false, false),
            ("visibility-after", false, true),
            ("publication-before", true, false),
            ("publication-after", true, true),
        ] {
            let root = parent.join(format!(
                "layerfs-root-lock-invalidated-{case}-{}-{}",
                std::process::id(),
                NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
            ));
            let cas = FsCasV1::create_new(&root).unwrap();
            let stale = cas.clone();
            if !after_acquire {
                cas.invalidate_root_backstop_v1();
            }
            let terminated_boundary = if publication {
                FsCasBoundaryV1::PublicationLockWaitTerminated
            } else {
                FsCasBoundaryV1::VisibilityLockWaitTerminated
            };
            let mut control = RootLockWaitTerminalPanicControlV1 {
                terminated_boundary,
                mode: RootLockWaitTerminalModeV1::Continue,
                terminal_observations: 0,
                fail_invalidation: false,
                invalidation_attempts: 0,
                invalidate_after_acquire: after_acquire.then(|| cas.clone()),
            };
            let mut observed = FsOperationObservedControlV1::new(&mut control);
            let error = if publication {
                cas.lock_publication_controlled_v1(&mut observed)
                    .map(drop)
                    .unwrap_err()
            } else {
                cas.lock_visibility_controlled_v1(&mut observed)
                    .map(drop)
                    .unwrap_err()
            };
            assert_eq!(error, FsCasErrorV1::Invalidated, "{case}");
            let mut counters = OperationCountersV1::default();
            observed.finish_v1(&mut counters).unwrap();
            assert_eq!(control.terminal_observations, 1, "{case}");
            assert_eq!(control.invalidation_attempts, 0, "{case}");
            assert!(counters.has_zero_forbidden_work(), "{case}");
            assert_eq!(cas.ensure_valid(), Err(FsCasErrorV1::Invalidated), "{case}");
            assert_eq!(
                stale.ensure_valid(),
                Err(FsCasErrorV1::Invalidated),
                "{case}"
            );
            let available = if publication {
                cas.inner.publication.try_lock().is_ok()
            } else {
                cas.inner.visibility.try_lock().is_ok()
            };
            assert!(available, "{case}: invalidation left root lock unavailable");
            assert!(matches!(
                FsCasV1::open_existing(&root),
                Err(FsCasErrorV1::Invalidated)
            ));
            drop(stale);
            drop(cas);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn root_lock_wait_terminal_observation_unwind_preserves_poison_and_double_fault() {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        for (case, publication) in [("visibility", false), ("publication", true)] {
            for fail_invalidation in [false, true] {
                let root = parent.join(format!(
                    "layerfs-root-lock-poison-terminal-{case}-{fail_invalidation}-{}-{}",
                    std::process::id(),
                    NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
                ));
                let cas = FsCasV1::create_new(&root).unwrap();
                let stale = cas.clone();
                let poison = cas.clone();
                assert!(std::thread::spawn(move || {
                    if publication {
                        let _guard = poison.inner.publication.lock().unwrap();
                        panic!("inject publication-mutex poison before terminal observation");
                    } else {
                        let _guard = poison.inner.visibility.lock().unwrap();
                        panic!("inject visibility-mutex poison before terminal observation");
                    }
                })
                .join()
                .is_err());
                let terminated_boundary = if publication {
                    FsCasBoundaryV1::PublicationLockWaitTerminated
                } else {
                    FsCasBoundaryV1::VisibilityLockWaitTerminated
                };
                let mut control = RootLockWaitTerminalPanicControlV1 {
                    terminated_boundary,
                    mode: RootLockWaitTerminalModeV1::Continue,
                    terminal_observations: 0,
                    fail_invalidation,
                    invalidation_attempts: 0,
                    invalidate_after_acquire: None,
                };
                let mut observed = FsOperationObservedControlV1::new(&mut control);
                let error = if publication {
                    cas.lock_publication_controlled_v1(&mut observed)
                        .map(drop)
                        .unwrap_err()
                } else {
                    cas.lock_visibility_controlled_v1(&mut observed)
                        .map(drop)
                        .unwrap_err()
                };
                assert_eq!(
                    error.failure_causes_v1(),
                    (
                        FsCasFailureCauseV1::SynchronizationPoisoned,
                        if fail_invalidation {
                            FsCasFailureCauseV1::InvalidationFailed
                        } else {
                            FsCasFailureCauseV1::SynchronizationPoisoned
                        },
                    ),
                    "{case}/{fail_invalidation}"
                );
                let mut counters = OperationCountersV1::default();
                observed.finish_v1(&mut counters).unwrap();
                assert_eq!(control.terminal_observations, 1);
                assert_eq!(control.invalidation_attempts, 1);
                assert!(counters.has_zero_forbidden_work());
                assert_eq!(cas.ensure_valid(), Err(FsCasErrorV1::Invalidated));
                assert_eq!(stale.ensure_valid(), Err(FsCasErrorV1::Invalidated));
                let lock = if publication {
                    &cas.inner.publication
                } else {
                    &cas.inner.visibility
                };
                match lock.try_lock() {
                    Ok(guard) => drop(guard),
                    Err(TryLockError::Poisoned(poisoned)) => drop(poisoned.into_inner()),
                    Err(TryLockError::WouldBlock) => panic!("{case}: poisoned lock remained held"),
                }
                assert!(matches!(
                    FsCasV1::open_existing(&root),
                    Err(FsCasErrorV1::Busy | FsCasErrorV1::Invalidated)
                ));
                drop(stale);
                drop(cas);
                fs::remove_dir_all(root).unwrap();
            }
        }
    }

    #[cfg(feature = "c3-polymorphism")]
    #[test]
    fn queue_entry_observation_overflow_fails_closed_before_ticket_issue() {
        for fail_invalidation in [false, true] {
            let parent = std::env::temp_dir().canonicalize().unwrap();
            let root = parent.join(format!(
                "layerfs-queue-entry-observation-overflow-{fail_invalidation}-{}-{}",
                std::process::id(),
                NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
            ));
            let cas = FsCasV1::create_new(&root).unwrap();
            let stale = cas.clone();
            let mut counters = OperationCountersV1 {
                root_admission_queue_entries: u64::MAX,
                ..OperationCountersV1::default()
            };
            let mut control = AdmissionInvalidationAttemptControlV1 {
                fail_invalidation,
                invalidation_attempts: 0,
            };

            let error = match cas.begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x15_505,
                &mut counters,
                &mut control,
            ) {
                Ok(_) => panic!("queue-entry observation overflow minted a capability"),
                Err(error) => error,
            };
            assert_eq!(
                error.failure_causes_v1(),
                (
                    FsCasFailureCauseV1::Core(CoreError::IntegerOverflow),
                    if fail_invalidation {
                        FsCasFailureCauseV1::InvalidationFailed
                    } else {
                        FsCasFailureCauseV1::Core(CoreError::IntegerOverflow)
                    },
                )
            );
            assert_eq!(control.invalidation_attempts, 1);

            // Observation fails before the ticket/state transition. No root
            // authority, request work, storage reservation, or preparation
            // object can exist on return.
            assert_eq!(counters.root_admission_queue_entries, u64::MAX);
            assert_eq!(counters.root_admission_queue_refusals, 0);
            assert_eq!(counters.root_admission_queue_depth_high_water, 0);
            assert_eq!(counters.root_admission_active_slots_high_water, 0);
            assert_eq!(cas.operation_admission_queue_for_test_v1(), (0, 0, 0));
            assert_eq!(cas.operation_admission_active_for_test_v1(), 0);
            assert_eq!(cas.operation_admitted_slots_v1(), 0);
            assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
            assert_eq!(counters.source_read_calls, 0);
            assert_eq!(counters.source_bytes_read, 0);
            assert_eq!(counters.storage_bytes_requested, 0);
            assert_eq!(counters.storage_bytes_reserved, 0);
            assert_eq!(counters.storage_inodes_requested, 0);
            assert_eq!(counters.storage_inodes_reserved, 0);
            assert_eq!(fs::read_dir(root.join("preparation")).unwrap().count(), 0);
            assert!(counters.has_zero_forbidden_work());
            assert_eq!(cas.ensure_valid(), Err(FsCasErrorV1::Invalidated));
            assert_eq!(stale.ensure_valid(), Err(FsCasErrorV1::Invalidated));
            assert!(matches!(
                FsCasV1::open_existing(&root),
                Err(FsCasErrorV1::Invalidated)
            ));

            drop(stale);
            drop(cas);
            if !fail_invalidation {
                assert!(matches!(
                    FsCasV1::open_existing(&root),
                    Err(FsCasErrorV1::Invalidated)
                ));
            }
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[cfg(feature = "c3-polymorphism")]
    #[test]
    fn ticket_sequence_overflow_fails_closed_before_observation_or_state_change() {
        for fail_invalidation in [false, true] {
            let parent = std::env::temp_dir().canonicalize().unwrap();
            let root = parent.join(format!(
                "layerfs-ticket-sequence-overflow-{fail_invalidation}-{}-{}",
                std::process::id(),
                NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
            ));
            let cas = FsCasV1::create_new(&root).unwrap();
            let stale = cas.clone();
            {
                let mut state = cas.inner.operation_admission.state.lock().unwrap();
                state.next_ticket = u64::MAX;
                state.serving_ticket = u64::MAX;
                assert_eq!(state.active, 0);
                assert!(state
                    .tickets
                    .iter()
                    .all(|ticket| *ticket == AdmissionTicketStateV1::Empty));
                assert!(state
                    .queue_tickets
                    .iter()
                    .all(|ticket| ticket.operation_kind == 0 && ticket.cancellation_key == 0));
            }
            let mut counters = OperationCountersV1::default();
            let mut control = AdmissionInvalidationAttemptControlV1 {
                fail_invalidation,
                invalidation_attempts: 0,
            };

            let error = match cas.begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x15_506,
                &mut counters,
                &mut control,
            ) {
                Ok(_) => panic!("exhausted ticket sequence minted a capability"),
                Err(error) => error,
            };
            assert_eq!(
                error.failure_causes_v1(),
                (
                    FsCasFailureCauseV1::Core(CoreError::IntegerOverflow),
                    if fail_invalidation {
                        FsCasFailureCauseV1::InvalidationFailed
                    } else {
                        FsCasFailureCauseV1::Core(CoreError::IntegerOverflow)
                    },
                )
            );
            assert_eq!(control.invalidation_attempts, 1);
            assert_eq!(counters.root_admission_queue_entries, 0);
            assert_eq!(counters.root_admission_queue_refusals, 0);
            assert_eq!(counters.root_admission_queue_depth_high_water, 0);
            assert_eq!(counters.root_admission_active_slots_high_water, 0);
            assert_eq!(cas.operation_admission_queue_for_test_v1(), (0, 0, 0));
            {
                let state = cas.inner.operation_admission.state.lock().unwrap();
                assert_eq!(state.next_ticket, u64::MAX);
                assert_eq!(state.serving_ticket, u64::MAX);
                assert!(state
                    .tickets
                    .iter()
                    .all(|ticket| *ticket == AdmissionTicketStateV1::Empty));
                assert!(state
                    .queue_tickets
                    .iter()
                    .all(|ticket| ticket.operation_kind == 0 && ticket.cancellation_key == 0));
            }
            assert_eq!(cas.operation_admission_active_for_test_v1(), 0);
            assert_eq!(cas.operation_admitted_slots_v1(), 0);
            assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
            assert_eq!(counters.source_read_calls, 0);
            assert_eq!(counters.source_bytes_read, 0);
            assert_eq!(counters.storage_bytes_requested, 0);
            assert_eq!(counters.storage_bytes_reserved, 0);
            assert_eq!(counters.storage_inodes_requested, 0);
            assert_eq!(counters.storage_inodes_reserved, 0);
            assert_eq!(fs::read_dir(root.join("preparation")).unwrap().count(), 0);
            assert!(counters.has_zero_forbidden_work());
            assert_eq!(cas.ensure_valid(), Err(FsCasErrorV1::Invalidated));
            assert_eq!(stale.ensure_valid(), Err(FsCasErrorV1::Invalidated));
            assert!(matches!(
                FsCasV1::open_existing(&root),
                Err(FsCasErrorV1::Invalidated)
            ));

            drop(stale);
            drop(cas);
            if !fail_invalidation {
                assert!(matches!(
                    FsCasV1::open_existing(&root),
                    Err(FsCasErrorV1::Invalidated)
                ));
            }
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[cfg(feature = "c3-polymorphism")]
    #[test]
    fn nonempty_target_slot_fails_closed_without_queue_transition() {
        for fail_invalidation in [false, true] {
            let parent = std::env::temp_dir().canonicalize().unwrap();
            let root = parent.join(format!(
                "layerfs-nonempty-admission-slot-{fail_invalidation}-{}-{}",
                std::process::id(),
                NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
            ));
            let cas = FsCasV1::create_new(&root).unwrap();
            let stale = cas.clone();
            let incumbent = QueueTicketV1::new(FsOperationKindV1::CompleteC3Tree, 0x15_507);
            {
                let mut state = cas.inner.operation_admission.state.lock().unwrap();
                assert_eq!(state.next_ticket, 0);
                assert_eq!(state.serving_ticket, 0);
                assert_eq!(state.active, 0);
                state.queue_tickets[0] = incumbent;
                state.tickets[0] = AdmissionTicketStateV1::Waiting;
            }
            let mut counters = OperationCountersV1::default();
            let mut control = AdmissionInvalidationAttemptControlV1 {
                fail_invalidation,
                invalidation_attempts: 0,
            };

            let error = match cas.begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x15_508,
                &mut counters,
                &mut control,
            ) {
                Ok(_) => panic!("a non-empty target slot minted a capability"),
                Err(error) => error,
            };
            assert_eq!(
                error.failure_causes_v1(),
                (
                    FsCasFailureCauseV1::Integrity,
                    if fail_invalidation {
                        FsCasFailureCauseV1::InvalidationFailed
                    } else {
                        FsCasFailureCauseV1::Integrity
                    },
                )
            );
            assert_eq!(control.invalidation_attempts, 1);

            // Slot validation precedes every direct observation and queue
            // mutation. The impossible incumbent remains byte-for-byte owned
            // by the preexisting corrupted state; this rejected operation
            // adds no ticket, sequence movement, or resource work.
            assert_eq!(counters.root_admission_queue_entries, 0);
            assert_eq!(counters.root_admission_queue_refusals, 0);
            assert_eq!(counters.root_admission_queue_depth_high_water, 0);
            assert_eq!(counters.root_admission_active_slots_high_water, 0);
            {
                let state = cas.inner.operation_admission.state.lock().unwrap();
                assert_eq!(state.next_ticket, 0);
                assert_eq!(state.serving_ticket, 0);
                assert_eq!(state.active, 0);
                assert_eq!(state.tickets[0], AdmissionTicketStateV1::Waiting);
                assert_eq!(
                    state.queue_tickets[0].operation_kind,
                    incumbent.operation_kind
                );
                assert_eq!(
                    state.queue_tickets[0].cancellation_key,
                    incumbent.cancellation_key
                );
                assert!(state.tickets[1..]
                    .iter()
                    .all(|ticket| *ticket == AdmissionTicketStateV1::Empty));
                assert!(state.queue_tickets[1..]
                    .iter()
                    .all(|ticket| { ticket.operation_kind == 0 && ticket.cancellation_key == 0 }));
            }
            assert_eq!(cas.operation_admitted_slots_v1(), 0);
            assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
            assert_eq!(counters.source_read_calls, 0);
            assert_eq!(counters.source_bytes_read, 0);
            assert_eq!(counters.storage_bytes_requested, 0);
            assert_eq!(counters.storage_bytes_reserved, 0);
            assert_eq!(counters.storage_inodes_requested, 0);
            assert_eq!(counters.storage_inodes_reserved, 0);
            assert_eq!(fs::read_dir(root.join("preparation")).unwrap().count(), 0);
            assert!(counters.has_zero_forbidden_work());
            assert_eq!(cas.ensure_valid(), Err(FsCasErrorV1::Invalidated));
            assert_eq!(stale.ensure_valid(), Err(FsCasErrorV1::Invalidated));
            assert!(matches!(
                FsCasV1::open_existing(&root),
                Err(FsCasErrorV1::Invalidated)
            ));

            drop(stale);
            drop(cas);
            if !fail_invalidation {
                assert!(matches!(
                    FsCasV1::open_existing(&root),
                    Err(FsCasErrorV1::Invalidated)
                ));
            }
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[cfg(feature = "c3-polymorphism")]
    #[test]
    fn queue_refusal_observation_overflow_preserves_queue_and_fails_closed() {
        for fail_invalidation in [false, true] {
            let parent = std::env::temp_dir().canonicalize().unwrap();
            let root = parent.join(format!(
                "layerfs-queue-refusal-observation-overflow-{fail_invalidation}-{}-{}",
                std::process::id(),
                NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
            ));
            let cas = FsCasV1::create_new(&root).unwrap();
            let stale = cas.clone();
            let mut pending = Vec::with_capacity(MAX_ADMISSION_TICKETS);
            for cancellation_key in 0..MAX_ADMISSION_TICKETS as u64 {
                pending.push(
                    cas.issue_pending_admission_for_test_v1(cancellation_key)
                        .expect("each preallocated queue cell must remain available"),
                );
            }
            assert_eq!(
                cas.operation_admission_queue_for_test_v1(),
                (
                    MAX_ADMISSION_TICKETS as u64,
                    MAX_ADMISSION_TICKETS as u64,
                    0
                )
            );
            assert_eq!(cas.operation_admission_active_for_test_v1(), 0);

            let mut counters = OperationCountersV1 {
                root_admission_queue_refusals: u64::MAX,
                ..OperationCountersV1::default()
            };
            let mut control = InvalidationCauseControlV1 {
                token_boundary: None,
                token_error: None,
                marker_error: fail_invalidation.then_some(FsCasErrorV1::Filesystem(
                    FsCasFilesystemFailureV1::InodeExhaustion,
                )),
                skip_token: fail_invalidation,
            };
            let error = match cas.begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x15_504,
                &mut counters,
                &mut control,
            ) {
                Ok(_) => panic!("the 1,025th production entry unexpectedly minted a capability"),
                Err(error) => error,
            };
            assert_eq!(
                error.failure_causes_v1(),
                (
                    FsCasFailureCauseV1::ResourceExhausted(FsCasResourceV1::Queue),
                    if fail_invalidation {
                        FsCasFailureCauseV1::InvalidationFailed
                    } else {
                        FsCasFailureCauseV1::ResourceExhausted(FsCasResourceV1::Queue)
                    },
                )
            );

            // The 1,025th production entry did not mint a capability, inspect
            // a request, or disturb any of the 1,024 already-owned queue cells.
            assert_eq!(counters.root_admission_queue_refusals, u64::MAX);
            assert_eq!(counters.root_admission_queue_entries, 0);
            assert_eq!(cas.operation_admission_active_for_test_v1(), 0);
            assert_eq!(
                cas.operation_admission_queue_for_test_v1(),
                (
                    MAX_ADMISSION_TICKETS as u64,
                    MAX_ADMISSION_TICKETS as u64,
                    0
                )
            );
            assert_eq!(cas.operation_admitted_slots_v1(), 0);
            assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
            assert_eq!(counters.storage_bytes_requested, 0);
            assert_eq!(counters.storage_bytes_reserved, 0);
            assert_eq!(counters.storage_inodes_requested, 0);
            assert_eq!(counters.storage_inodes_reserved, 0);
            assert_eq!(fs::read_dir(root.join("preparation")).unwrap().count(), 0);
            assert!(counters.has_zero_forbidden_work());
            assert_eq!(cas.ensure_valid(), Err(FsCasErrorV1::Invalidated));
            assert_eq!(stale.ensure_valid(), Err(FsCasErrorV1::Invalidated));
            assert!(matches!(
                FsCasV1::open_existing(&root),
                Err(FsCasErrorV1::Invalidated)
            ));

            drop(pending);
            assert_eq!(cas.operation_admission_queue_for_test_v1(), (0, 0, 0));
            assert_eq!(cas.operation_admission_active_for_test_v1(), 0);
            drop(stale);
            drop(cas);
            if !fail_invalidation {
                // Successful persistence survives close-all/reopen. In the
                // double-fault row both independent persistent barriers were
                // deliberately refused, which is exactly why the terminal is
                // `InvalidationFailed`; only the live shared owner can then
                // guarantee fail-closed behavior.
                assert!(matches!(
                    FsCasV1::open_existing(&root),
                    Err(FsCasErrorV1::Invalidated)
                ));
            }
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn seventeenth_ticket_cancels_without_multiplying_capacity() {
        let queue = OperationAdmissionQueueV1::new(16).unwrap();
        let mut control = ContinueFsCasControlV1;
        let mut counters = OperationCountersV1::default();
        let mut active = Vec::with_capacity(16);
        for _ in 0..16 {
            active.push(
                queue
                    .acquire(
                        FsOperationKindV1::CompleteC3File,
                        7,
                        &mut control,
                        &mut counters,
                    )
                    .unwrap(),
            );
        }
        let mut cancel = CancelControlV1;
        assert!(matches!(
            queue.acquire(
                FsOperationKindV1::CompleteC3Tree,
                8,
                &mut cancel,
                &mut counters,
            ),
            OperationAdmissionAcquireOutcomeV1::Rejected {
                first: FsCasErrorV1::Core(CoreError::Cancelled),
                fail_closed: false,
            }
        ));
        assert_eq!(queue.state.lock().unwrap().active, 16);
        active.pop();
        let replacement = queue
            .acquire(
                FsOperationKindV1::CompleteC3Tree,
                9,
                &mut control,
                &mut counters,
            )
            .unwrap();
        assert_eq!(queue.state.lock().unwrap().active, 16);
        drop(replacement);
        drop(active);
        assert_eq!(queue.state.lock().unwrap().active, 0);
    }

    #[test]
    fn queued_control_cause_precedes_wait_observation_overflow() {
        for deadline in [false, true] {
            let queue = OperationAdmissionQueueV1::new(0).unwrap();
            let mut counters = OperationCountersV1 {
                root_admission_wait_nanoseconds: u64::MAX,
                ..OperationCountersV1::default()
            };
            let outcome = if deadline {
                queue.acquire(
                    FsOperationKindV1::CompleteC3Tree,
                    0x15_500,
                    &mut DeadlineControlV1,
                    &mut counters,
                )
            } else {
                queue.acquire(
                    FsOperationKindV1::CompleteC3Tree,
                    0x15_501,
                    &mut CancelControlV1,
                    &mut counters,
                )
            };
            assert!(
                matches!(
                    outcome,
                    OperationAdmissionAcquireOutcomeV1::Rejected {
                        first: FsCasErrorV1::Core(CoreError::Deadline),
                        fail_closed: false,
                    } if deadline
                ) || matches!(
                    outcome,
                    OperationAdmissionAcquireOutcomeV1::Rejected {
                        first: FsCasErrorV1::Core(CoreError::Cancelled),
                        fail_closed: false,
                    } if !deadline
                )
            );
            assert_eq!(counters.root_admission_wait_nanoseconds, u64::MAX);
            let state = queue.state.lock().unwrap();
            assert_eq!(state.next_ticket, state.serving_ticket);
            assert_eq!(state.active, 0);
            assert!(state
                .tickets
                .iter()
                .all(|ticket| *ticket == AdmissionTicketStateV1::Empty));
            assert!(state
                .queue_tickets
                .iter()
                .all(|ticket| ticket.operation_kind == 0 && ticket.cancellation_key == 0));
        }
    }

    #[test]
    fn queued_control_unwind_resumes_exact_payload_after_clean_ticket_retirement() {
        let queue = OperationAdmissionQueueV1::new(0).unwrap();
        let mut counters = OperationCountersV1::default();
        let ticket = queue
            .issue(FsOperationKindV1::CompleteC3File, 0x15_502, &mut counters)
            .unwrap();
        let mut control = QueuedControlUnwindRetirementControlV1 {
            retirement_failure: None,
            fail_invalidation: false,
            invalidation_attempts: 0,
        };

        let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            match ticket.wait(&mut control, &mut counters) {
                Ok(_) => panic!("queued callback unwind unexpectedly granted admission"),
                Err(failure) => panic!("clean retirement returned {failure:?}"),
            }
        }))
        .expect_err("clean queued retirement must resume the initiating payload");
        assert_eq!(
            unwind.downcast_ref::<&'static str>(),
            Some(&QUEUED_CONTROL_UNWIND_PAYLOAD_V1)
        );
        assert_eq!(control.invalidation_attempts, 0);
        let state = queue.state.lock().unwrap();
        assert_eq!(state.next_ticket, state.serving_ticket);
        assert_eq!(state.active, 0);
        assert!(state
            .tickets
            .iter()
            .all(|ticket| *ticket == AdmissionTicketStateV1::Empty));
        assert!(state
            .queue_tickets
            .iter()
            .all(|ticket| ticket.operation_kind == 0 && ticket.cancellation_key == 0));
    }

    #[test]
    fn queued_control_unwind_wrong_state_returns_typed_fail_closed_custody() {
        let queue = OperationAdmissionQueueV1::new(0).unwrap();
        let mut counters = OperationCountersV1::default();
        let ticket = queue
            .issue(FsOperationKindV1::CompleteC3File, 0x15_503, &mut counters)
            .unwrap();
        let slot = ticket.slot;
        let incumbent = {
            let mut state = queue.state.lock().unwrap();
            let incumbent = state.queue_tickets[slot];
            state.tickets[slot] = AdmissionTicketStateV1::Empty;
            incumbent
        };
        let mut control = QueuedControlUnwindRetirementControlV1 {
            retirement_failure: None,
            fail_invalidation: false,
            invalidation_attempts: 0,
        };

        let failure = match ticket.wait(&mut control, &mut counters) {
            Ok(_) => panic!("wrong-state unwind retirement unexpectedly granted admission"),
            Err(failure) => failure,
        };
        assert_eq!(failure.first, FsCasErrorV1::Integrity);
        assert!(failure.fail_closed);
        assert_eq!(control.invalidation_attempts, 0);
        let state = queue.state.lock().unwrap();
        assert_eq!(state.next_ticket, 1);
        assert_eq!(state.serving_ticket, 0);
        assert_eq!(state.active, 0);
        assert_eq!(state.tickets[slot], AdmissionTicketStateV1::Empty);
        assert_eq!(
            state.queue_tickets[slot].operation_kind,
            incumbent.operation_kind
        );
        assert_eq!(
            state.queue_tickets[slot].cancellation_key,
            incumbent.cancellation_key
        );
    }

    #[cfg(feature = "c3-polymorphism")]
    #[test]
    fn queued_control_unwind_retirement_failure_is_typed_and_durably_fail_closed() {
        for fail_invalidation in [false, true] {
            let parent = std::env::temp_dir().canonicalize().unwrap();
            let root = parent.join(format!(
                "layerfs-queued-unwind-retirement-{fail_invalidation}-{}-{}",
                std::process::id(),
                NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
            ));
            let cas = FsCasV1::create_new(&root).unwrap();
            let stale = cas.clone();

            // Fill the fixed root admission capacity directly. These leases
            // are independently and explicitly released after the rejected
            // production entry has terminalized its owned pending ticket.
            let capacity = 16_u64;
            let mut active = Vec::with_capacity(capacity as usize);
            let mut setup_counters = OperationCountersV1::default();
            let mut setup_control = ContinueFsCasControlV1;
            for ordinal in 0..capacity {
                active.push(
                    cas.inner
                        .operation_admission
                        .acquire(
                            FsOperationKindV1::CompleteC3File,
                            0x15_600 + ordinal,
                            &mut setup_control,
                            &mut setup_counters,
                        )
                        .unwrap(),
                );
            }
            assert_eq!(cas.operation_admission_active_for_test_v1(), capacity);
            assert_eq!(cas.operation_admission_queue_for_test_v1(), (0, 0, 0));

            let mut counters = OperationCountersV1::default();
            let mut control = QueuedControlUnwindRetirementControlV1 {
                retirement_failure: Some(FsCasErrorV1::Core(CoreError::IntegerOverflow)),
                fail_invalidation,
                invalidation_attempts: 0,
            };
            let error = match cas.begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3Tree,
                0x15_700,
                &mut counters,
                &mut control,
            ) {
                Ok(_) => panic!("queued unwind retirement failure minted a capability"),
                Err(error) => error,
            };
            assert_eq!(
                error.failure_causes_v1(),
                (
                    FsCasFailureCauseV1::Core(CoreError::IntegerOverflow),
                    if fail_invalidation {
                        FsCasFailureCauseV1::InvalidationFailed
                    } else {
                        FsCasFailureCauseV1::Core(CoreError::IntegerOverflow)
                    },
                )
            );
            assert_eq!(control.invalidation_attempts, 1);
            assert_eq!(cas.operation_admission_queue_for_test_v1(), (0, 0, 0));
            assert_eq!(cas.operation_admission_active_for_test_v1(), capacity);
            assert_eq!(cas.operation_admitted_slots_v1(), 0);
            assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
            assert_eq!(counters.root_admission_queue_entries, 1);
            assert_eq!(counters.root_admission_queue_refusals, 0);
            assert_eq!(counters.root_admission_active_slots_high_water, 0);
            assert_eq!(counters.source_read_calls, 0);
            assert_eq!(counters.source_bytes_read, 0);
            assert_eq!(counters.storage_bytes_requested, 0);
            assert_eq!(counters.storage_bytes_reserved, 0);
            assert_eq!(counters.storage_inodes_requested, 0);
            assert_eq!(counters.storage_inodes_reserved, 0);
            assert_eq!(fs::read_dir(root.join("preparation")).unwrap().count(), 0);
            assert!(counters.has_zero_forbidden_work());
            assert!(setup_counters.has_zero_forbidden_work());
            assert_eq!(cas.ensure_valid(), Err(FsCasErrorV1::Invalidated));
            assert_eq!(stale.ensure_valid(), Err(FsCasErrorV1::Invalidated));
            assert!(matches!(
                FsCasV1::open_existing(&root),
                Err(FsCasErrorV1::Busy | FsCasErrorV1::Invalidated)
            ));

            for lease in &mut active {
                lease.release_v1().unwrap();
            }
            drop(active);
            assert_eq!(cas.operation_admission_active_for_test_v1(), 0);
            assert_eq!(cas.operation_admission_queue_for_test_v1(), (0, 0, 0));

            drop(stale);
            drop(cas);
            if !fail_invalidation {
                assert!(matches!(
                    FsCasV1::open_existing(&root),
                    Err(FsCasErrorV1::Invalidated)
                ));
            }
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn pending_wait_initial_mutex_poison_retires_the_owned_ticket() {
        let queue = OperationAdmissionQueueV1::new(0).unwrap();
        let mut counters = OperationCountersV1::default();
        let ticket = queue
            .issue(FsOperationKindV1::CompleteC3File, 0x15_505, &mut counters)
            .unwrap();

        let poison = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = queue.state.lock().unwrap();
            panic!("inject pending-wait initial mutex poison");
        }));
        assert!(poison.is_err());

        let failure = match ticket.wait(&mut ContinueFsCasControlV1, &mut counters) {
            Ok(_) => panic!("poisoned pending wait unexpectedly granted admission"),
            Err(failure) => failure,
        };
        assert_eq!(failure.first, FsCasErrorV1::SynchronizationPoisoned);
        assert!(!failure.fail_closed);
        let state = match queue.state.lock() {
            Ok(_) => panic!("pending-wait mutex unexpectedly recovered from poison"),
            Err(poison) => poison.into_inner(),
        };
        assert_eq!(state.next_ticket, state.serving_ticket);
        assert_eq!(state.active, 0);
        assert!(state
            .tickets
            .iter()
            .all(|ticket| *ticket == AdmissionTicketStateV1::Empty));
        assert!(state
            .queue_tickets
            .iter()
            .all(|ticket| ticket.operation_kind == 0 && ticket.cancellation_key == 0));
    }

    #[test]
    fn pending_wait_poll_overflow_retires_the_owned_ticket() {
        let queue = OperationAdmissionQueueV1::new(0).unwrap();
        let mut counters = OperationCountersV1::default();
        let ticket = queue
            .issue(FsOperationKindV1::CompleteC3File, 0x15_506, &mut counters)
            .unwrap();
        counters.root_admission_wait_polls = u64::MAX;

        let failure = match ticket.wait(&mut ContinueFsCasControlV1, &mut counters) {
            Ok(_) => panic!("overflowed pending wait unexpectedly granted admission"),
            Err(failure) => failure,
        };
        assert_eq!(
            failure.first,
            FsCasErrorV1::Core(CoreError::IntegerOverflow)
        );
        assert!(!failure.fail_closed);
        assert_eq!(counters.root_admission_wait_polls, u64::MAX);
        let state = queue.state.lock().unwrap();
        assert_eq!(state.next_ticket, state.serving_ticket);
        assert_eq!(state.active, 0);
        assert!(state
            .tickets
            .iter()
            .all(|ticket| *ticket == AdmissionTicketStateV1::Empty));
        assert!(state
            .queue_tickets
            .iter()
            .all(|ticket| ticket.operation_kind == 0 && ticket.cancellation_key == 0));
    }

    #[test]
    fn pending_wait_condvar_poison_retires_the_owned_ticket() {
        struct SignalBeforeWaitV1 {
            entered: std::sync::mpsc::Sender<()>,
        }

        impl FsCasControlV1 for SignalBeforeWaitV1 {
            fn cancellation_requested(&mut self) -> bool {
                // This callback runs with the authoritative queue guard held,
                // immediately before the condvar wait releases it.
                self.entered.send(()).unwrap();
                false
            }

            fn deadline_exceeded(&mut self) -> bool {
                false
            }
        }

        let queue = OperationAdmissionQueueV1::new(0).unwrap();
        let mut issue_counters = OperationCountersV1::default();
        let ticket = queue
            .issue(
                FsOperationKindV1::CompleteC3File,
                0x15_507,
                &mut issue_counters,
            )
            .unwrap();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();

        let (failure, wait_counters) = std::thread::scope(|scope| {
            let waiter = scope.spawn(move || {
                let mut control = SignalBeforeWaitV1 {
                    entered: entered_tx,
                };
                let mut counters = issue_counters;
                let failure = match ticket.wait(&mut control, &mut counters) {
                    Ok(_) => panic!("poisoned condvar wait unexpectedly granted admission"),
                    Err(failure) => failure,
                };
                (failure, counters)
            });
            entered_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("pending waiter must reach the condvar boundary");
            let poisoner = scope.spawn(|| {
                let _guard = queue.state.lock().unwrap();
                panic!("inject pending-wait condvar mutex poison");
            });
            assert!(poisoner.join().is_err());
            waiter.join().unwrap()
        });

        assert_eq!(failure.first, FsCasErrorV1::SynchronizationPoisoned);
        assert!(!failure.fail_closed);
        assert!(wait_counters.root_admission_wait_polls >= 1);
        let state = match queue.state.lock() {
            Ok(_) => panic!("condvar mutex unexpectedly recovered from poison"),
            Err(poison) => poison.into_inner(),
        };
        assert_eq!(state.next_ticket, state.serving_ticket);
        assert_eq!(state.active, 0);
        assert!(state
            .tickets
            .iter()
            .all(|ticket| *ticket == AdmissionTicketStateV1::Empty));
        assert!(state
            .queue_tickets
            .iter()
            .all(|ticket| ticket.operation_kind == 0 && ticket.cancellation_key == 0));
    }

    #[cfg(feature = "c3-polymorphism")]
    #[test]
    fn granted_wait_observation_overflow_uses_owned_entry_release_path() {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let root = parent.join(format!(
            "layerfs-wait-observation-overflow-{}-{}",
            std::process::id(),
            NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
        ));
        let cas = FsCasV1::create_new(&root).unwrap();
        let mut counters = OperationCountersV1 {
            root_admission_wait_nanoseconds: u64::MAX,
            ..OperationCountersV1::default()
        };
        let mut control = ContinueFsCasControlV1;

        assert!(matches!(
            cas.begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x15_502,
                &mut counters,
                &mut control,
            ),
            Err(FsCasErrorV1::Core(CoreError::IntegerOverflow))
        ));
        assert_eq!(counters.root_admission_wait_nanoseconds, u64::MAX);
        assert_eq!(counters.root_admission_release_failures, 0);
        assert_eq!(cas.operation_admission_active_for_test_v1(), 0);
        assert_eq!(cas.operation_admission_queue_for_test_v1(), (0, 0, 0));
        assert_eq!(cas.operation_admitted_slots_v1(), 0);
        assert_eq!(counters.storage_bytes_requested, 0);
        assert_eq!(counters.storage_bytes_reserved, 0);
        assert_eq!(counters.storage_inodes_requested, 0);
        assert_eq!(counters.storage_inodes_reserved, 0);
        assert_eq!(fs::read_dir(root.join("preparation")).unwrap().count(), 0);
        assert_eq!(cas.ensure_valid(), Ok(()));

        drop(cas);
        assert!(FsCasV1::open_existing(&root).is_ok());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(feature = "c3-polymorphism")]
    #[test]
    fn granted_wait_observation_overflow_retains_release_double_fault() {
        for fail_invalidation in [false, true] {
            let parent = std::env::temp_dir().canonicalize().unwrap();
            let root = parent.join(format!(
                "layerfs-wait-observation-release-poison-{fail_invalidation}-{}-{}",
                std::process::id(),
                NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
            ));
            let cas = FsCasV1::create_new(&root).unwrap();
            let stale = cas.clone();
            let mut counters = OperationCountersV1 {
                root_admission_wait_nanoseconds: u64::MAX,
                ..OperationCountersV1::default()
            };
            let mut continue_control = ContinueFsCasControlV1;
            let (mut admission, first) = match cas.inner.operation_admission.acquire(
                FsOperationKindV1::CompleteC3File,
                0x15_503,
                &mut continue_control,
                &mut counters,
            ) {
                OperationAdmissionAcquireOutcomeV1::GrantedWithObservationFailure {
                    admission,
                    first,
                } => (admission, first),
                OperationAdmissionAcquireOutcomeV1::Granted(_)
                | OperationAdmissionAcquireOutcomeV1::Rejected { .. } => {
                    panic!("wait observation overflow did not retain its granted lease")
                }
            };
            assert_eq!(first, FsCasErrorV1::Core(CoreError::IntegerOverflow));
            assert_eq!(cas.operation_admission_active_for_test_v1(), 1);
            cas.poison_operation_admission_for_test_v1();

            if fail_invalidation {
                fs::write(root.join(INVALIDATED_ROOT_NAME), b"wrong marker shape").unwrap();
            }
            let mut failing_control = InvalidationCauseControlV1 {
                token_boundary: None,
                token_error: None,
                marker_error: None,
                skip_token: fail_invalidation,
            };
            let error = cas.finish_failed_operation_entry_v1(
                &mut admission,
                first,
                &mut counters,
                &mut failing_control,
            );
            assert_eq!(
                error.failure_causes_v1(),
                (
                    FsCasFailureCauseV1::Core(CoreError::IntegerOverflow),
                    if fail_invalidation {
                        FsCasFailureCauseV1::InvalidationFailed
                    } else {
                        FsCasFailureCauseV1::Core(CoreError::IntegerOverflow)
                    },
                )
            );
            assert_eq!(counters.root_admission_wait_nanoseconds, u64::MAX);
            assert_eq!(counters.root_admission_release_failures, 1);
            assert_eq!(cas.operation_admission_active_for_test_v1(), 0);
            assert_eq!(cas.operation_admission_queue_for_test_v1(), (0, 0, 0));
            assert_eq!(cas.operation_admitted_slots_v1(), 0);
            assert_eq!(counters.storage_bytes_requested, 0);
            assert_eq!(counters.storage_bytes_reserved, 0);
            assert_eq!(counters.storage_inodes_requested, 0);
            assert_eq!(counters.storage_inodes_reserved, 0);
            assert_eq!(fs::read_dir(root.join("preparation")).unwrap().count(), 0);
            assert_eq!(cas.ensure_valid(), Err(FsCasErrorV1::Invalidated));
            assert_eq!(stale.ensure_valid(), Err(FsCasErrorV1::Invalidated));
            assert!(matches!(
                FsCasV1::open_existing(&root),
                Err(FsCasErrorV1::Busy | FsCasErrorV1::Invalidated)
            ));

            drop(admission);
            drop(stale);
            drop(cas);
            assert!(matches!(
                FsCasV1::open_existing(&root),
                Err(FsCasErrorV1::Busy | FsCasErrorV1::Invalidated)
            ));
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[cfg(feature = "c3-polymorphism")]
    #[test]
    fn poisoned_admission_release_is_typed_and_durably_invalidates_reopen() {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let root = parent.join(format!(
            "layerfs-poisoned-admission-{}-{}",
            std::process::id(),
            NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
        ));
        let cas = FsCasV1::create_new(&root).unwrap();
        let stale = cas.clone();
        let mut control = ContinueFsCasControlV1;
        let mut counters = OperationCountersV1::default();
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x155,
                &mut counters,
                &mut control,
            )
            .unwrap();
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(0, 64, 0, 1).unwrap())
            .unwrap();
        let storage_token = capability.storage_token_v1().unwrap();
        cas.record_storage_immutable_install_v1(storage_token, 64, 1)
            .unwrap();
        assert_eq!(cas.operation_admitted_slots_v1(), 1);

        let poison = cas.clone();
        let unwind = std::thread::spawn(move || {
            let _guard = poison.inner.operation_admission.state.lock().unwrap();
            panic!("inject operation-admission release poison");
        })
        .join();
        assert!(unwind.is_err());

        assert_eq!(
            capability.finish_terminal_v1(true, &mut counters, &mut control),
            Err(FsCasErrorV1::SynchronizationPoisoned)
        );
        assert_eq!(counters.root_admission_release_failures, 1);
        assert_eq!(counters.storage_bytes_requested, 64);
        assert_eq!(counters.storage_bytes_reserved, 64);
        assert_eq!(counters.storage_bytes_released, 0);
        assert_eq!(counters.storage_bytes_committed, 0);
        assert_eq!(counters.storage_bytes_retained, 64);
        assert_eq!(counters.storage_inodes_requested, 1);
        assert_eq!(counters.storage_inodes_reserved, 1);
        assert_eq!(counters.storage_inodes_released, 0);
        assert_eq!(counters.storage_inodes_committed, 0);
        assert_eq!(counters.storage_inodes_retained, 1);
        assert_eq!(counters.immutable_residue_bytes, 64);
        assert_eq!(counters.immutable_residue_inodes, 1);
        assert_eq!(counters.unreachable_installed_residue_bytes, 64);
        assert_eq!(cas.operation_admitted_slots_v1(), 0);
        let queue_state = match cas.inner.operation_admission.state.lock() {
            Ok(_) => panic!("operation-admission mutex unexpectedly recovered"),
            Err(poison) => poison.into_inner(),
        };
        assert_eq!(queue_state.active, 0);
        drop(queue_state);
        assert!(cas.inner.invalidated.load(Ordering::Acquire));
        assert!(matches!(
            stale.occupied_private_v1(),
            Err(FsCasErrorV1::Invalidated)
        ));
        assert!(matches!(
            FsCasV1::open_existing(&root),
            Err(FsCasErrorV1::Invalidated)
        ));

        drop(capability);
        drop(stale);
        drop(cas);
        assert!(matches!(
            FsCasV1::open_existing(&root),
            Err(FsCasErrorV1::Invalidated)
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(feature = "c3-polymorphism")]
    #[test]
    fn poisoned_admission_release_preserves_poison_when_active_state_underflows() {
        for fail_invalidation in [false, true] {
            let parent = std::env::temp_dir().canonicalize().unwrap();
            let root = parent.join(format!(
                "layerfs-poisoned-admission-underflow-{fail_invalidation}-{}-{}",
                std::process::id(),
                NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
            ));
            let cas = FsCasV1::create_new(&root).unwrap();
            let stale = cas.clone();
            let mut counters = OperationCountersV1::default();
            let mut continue_control = ContinueFsCasControlV1;
            let mut capability = cas
                .begin_operation_capability_v1(
                    FsOperationKindV1::CompleteC3File,
                    0x15_509,
                    &mut counters,
                    &mut continue_control,
                )
                .unwrap();
            assert_eq!(cas.operation_admission_active_for_test_v1(), 1);
            assert_eq!(cas.operation_admitted_slots_v1(), 1);

            let poison = cas.clone();
            let unwind = std::thread::spawn(move || {
                let mut state = poison.inner.operation_admission.state.lock().unwrap();
                state.active = 0;
                panic!("inject admission poison with recovered active underflow");
            })
            .join();
            assert!(unwind.is_err());

            let mut control = AdmissionInvalidationAttemptControlV1 {
                fail_invalidation,
                invalidation_attempts: 0,
            };
            let error = capability
                .finish_terminal_v1(false, &mut counters, &mut control)
                .expect_err("poisoned admission release must fail closed");
            assert_eq!(
                error.failure_causes_v1(),
                (
                    FsCasFailureCauseV1::SynchronizationPoisoned,
                    if fail_invalidation {
                        FsCasFailureCauseV1::InvalidationFailed
                    } else {
                        FsCasFailureCauseV1::SynchronizationPoisoned
                    },
                )
            );
            assert_eq!(control.invalidation_attempts, 1);
            assert_eq!(counters.root_admission_release_failures, 1);
            assert_eq!(cas.operation_admission_queue_for_test_v1(), (0, 0, 0));
            assert_eq!(cas.operation_admission_active_for_test_v1(), 0);
            assert_eq!(cas.operation_admitted_slots_v1(), 0);
            assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
            assert_eq!(counters.source_read_calls, 0);
            assert_eq!(counters.source_bytes_read, 0);
            assert_eq!(counters.storage_bytes_requested, 0);
            assert_eq!(counters.storage_bytes_reserved, 0);
            assert_eq!(counters.storage_inodes_requested, 0);
            assert_eq!(counters.storage_inodes_reserved, 0);
            assert_eq!(fs::read_dir(root.join("preparation")).unwrap().count(), 0);
            assert!(counters.has_zero_forbidden_work());
            assert_eq!(cas.ensure_valid(), Err(FsCasErrorV1::Invalidated));
            assert_eq!(stale.ensure_valid(), Err(FsCasErrorV1::Invalidated));
            assert!(matches!(
                FsCasV1::open_existing(&root),
                Err(FsCasErrorV1::Invalidated)
            ));

            drop(capability);
            drop(stale);
            drop(cas);
            if !fail_invalidation {
                assert!(matches!(
                    FsCasV1::open_existing(&root),
                    Err(FsCasErrorV1::Invalidated)
                ));
            }
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[cfg(feature = "c3-polymorphism")]
    #[test]
    fn release_observation_overflow_is_typed_without_replacing_admission_failure() {
        for fail_invalidation in [false, true] {
            let parent = std::env::temp_dir().canonicalize().unwrap();
            let root = parent.join(format!(
                "layerfs-release-observation-overflow-{fail_invalidation}-{}-{}",
                std::process::id(),
                NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
            ));
            let cas = FsCasV1::create_new(&root).unwrap();
            let stale = cas.clone();
            let mut continue_control = ContinueFsCasControlV1;
            let mut counters = OperationCountersV1::default();
            let mut capability = cas
                .begin_operation_capability_v1(
                    FsOperationKindV1::CompleteC3File,
                    0x156,
                    &mut counters,
                    &mut continue_control,
                )
                .unwrap();
            capability
                .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(0, 0, 0, 0).unwrap())
                .unwrap();

            let poison = cas.clone();
            assert!(std::thread::spawn(move || {
                let _guard = poison.inner.operation_admission.state.lock().unwrap();
                panic!("inject operation-admission release poison");
            })
            .join()
            .is_err());
            counters.root_admission_release_failures = u64::MAX;
            if fail_invalidation {
                fs::write(root.join(INVALIDATED_ROOT_NAME), b"wrong marker shape").unwrap();
            }
            let mut control = InvalidationCauseControlV1 {
                token_boundary: None,
                token_error: None,
                marker_error: None,
                skip_token: fail_invalidation,
            };

            let error = capability
                .finish_terminal_v1(false, &mut counters, &mut control)
                .unwrap_err();
            assert_eq!(
                error.failure_causes_v1(),
                (
                    FsCasFailureCauseV1::SynchronizationPoisoned,
                    if fail_invalidation {
                        FsCasFailureCauseV1::InvalidationFailed
                    } else {
                        FsCasFailureCauseV1::SynchronizationPoisoned
                    },
                ),
                "invalidation double fault={fail_invalidation}"
            );
            assert_eq!(counters.root_admission_release_failures, u64::MAX);
            assert_eq!(
                counters.root_admission_release_failure_observation_error_v1(),
                Some(CoreError::IntegerOverflow)
            );
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
            assert_eq!(cas.operation_admitted_slots_v1(), 0);
            let queue_state = match cas.inner.operation_admission.state.lock() {
                Ok(_) => panic!("operation-admission mutex unexpectedly recovered"),
                Err(poison) => poison.into_inner(),
            };
            assert_eq!(queue_state.active, 0);
            drop(queue_state);
            assert!(cas.inner.invalidated.load(Ordering::Acquire));
            assert!(matches!(
                stale.occupied_private_v1(),
                Err(FsCasErrorV1::Invalidated)
            ));
            assert!(matches!(
                FsCasV1::open_existing(&root),
                Err(FsCasErrorV1::Busy | FsCasErrorV1::Invalidated)
            ));

            drop(capability);
            drop(stale);
            drop(cas);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[cfg(feature = "c3-polymorphism")]
    #[test]
    fn memory_refusal_observation_overflow_uses_owned_entry_release_path() {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let root = parent.join(format!(
            "layerfs-memory-refusal-observation-overflow-{}-{}",
            std::process::id(),
            NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
        ));
        let cas = FsCasV1::create_new(&root).unwrap();
        let capacity = admitted_slots_for_budget(MEMORY_PROFILE_72_MIB);
        let mut held = Vec::with_capacity(capacity as usize);
        for _ in 0..capacity {
            held.push(
                cas.inner
                    .operation_ledger
                    .reserve_operation_unplanned()
                    .unwrap(),
            );
        }
        assert_eq!(cas.operation_admitted_slots_v1(), capacity);

        let mut counters = OperationCountersV1 {
            root_admission_memory_refusals: u64::MAX,
            ..OperationCountersV1::default()
        };
        let mut control = ContinueFsCasControlV1;
        assert!(matches!(
            cas.begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x157,
                &mut counters,
                &mut control,
            ),
            Err(FsCasErrorV1::Core(CoreError::IntegerOverflow))
        ));

        // The root queue lease is returned by the explicit entry terminal,
        // while the independently held ledger reservations prove that the
        // refusal itself did not manufacture or release another slot.
        assert_eq!(counters.root_admission_memory_refusals, u64::MAX);
        assert_eq!(counters.root_admission_release_failures, 0);
        assert_eq!(cas.operation_admission_active_for_test_v1(), 0);
        assert_eq!(cas.operation_admission_queue_for_test_v1(), (0, 0, 0));
        assert_eq!(cas.operation_admitted_slots_v1(), capacity);
        assert_eq!(counters.storage_bytes_requested, 0);
        assert_eq!(counters.storage_bytes_reserved, 0);
        assert_eq!(counters.storage_inodes_requested, 0);
        assert_eq!(counters.storage_inodes_reserved, 0);
        assert_eq!(fs::read_dir(root.join("preparation")).unwrap().count(), 0);
        assert_eq!(cas.ensure_valid(), Ok(()));

        drop(held);
        assert_eq!(cas.operation_admitted_slots_v1(), 0);
        assert!(FsCasV1::open_existing(&root).is_ok());
        drop(cas);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(feature = "c3-polymorphism")]
    #[test]
    fn failed_entry_release_poison_preserves_first_cause_and_double_fault() {
        for fail_invalidation in [false, true] {
            let parent = std::env::temp_dir().canonicalize().unwrap();
            let root = parent.join(format!(
                "layerfs-failed-entry-release-poison-{fail_invalidation}-{}-{}",
                std::process::id(),
                NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
            ));
            let cas = FsCasV1::create_new(&root).unwrap();
            let stale = cas.clone();
            let mut counters = OperationCountersV1 {
                root_admission_memory_refusals: u64::MAX,
                root_admission_release_failures: u64::MAX,
                ..OperationCountersV1::default()
            };
            let mut continue_control = ContinueFsCasControlV1;
            let mut admission = cas
                .inner
                .operation_admission
                .acquire(
                    FsOperationKindV1::CompleteC3File,
                    0x158,
                    &mut continue_control,
                    &mut counters,
                )
                .unwrap();
            assert_eq!(cas.operation_admission_active_for_test_v1(), 1);
            cas.poison_operation_admission_for_test_v1();

            if fail_invalidation {
                fs::write(root.join(INVALIDATED_ROOT_NAME), b"wrong marker shape").unwrap();
            }
            let mut failing_control = InvalidationCauseControlV1 {
                token_boundary: None,
                token_error: None,
                marker_error: None,
                skip_token: fail_invalidation,
            };
            let error = cas.finish_failed_operation_entry_v1(
                &mut admission,
                FsCasErrorV1::Core(CoreError::IntegerOverflow),
                &mut counters,
                &mut failing_control,
            );
            assert_eq!(
                error.failure_causes_v1(),
                (
                    FsCasFailureCauseV1::Core(CoreError::IntegerOverflow),
                    if fail_invalidation {
                        FsCasFailureCauseV1::InvalidationFailed
                    } else {
                        FsCasFailureCauseV1::Core(CoreError::IntegerOverflow)
                    },
                ),
                "invalidation double fault={fail_invalidation}"
            );
            assert_eq!(counters.root_admission_memory_refusals, u64::MAX);
            assert_eq!(counters.root_admission_release_failures, u64::MAX);
            assert_eq!(
                counters.root_admission_release_failure_observation_error_v1(),
                Some(CoreError::IntegerOverflow)
            );
            assert_eq!(cas.operation_admission_active_for_test_v1(), 0);
            assert_eq!(cas.operation_admission_queue_for_test_v1(), (0, 0, 0));
            assert_eq!(cas.operation_admitted_slots_v1(), 0);
            assert_eq!(counters.storage_bytes_requested, 0);
            assert_eq!(counters.storage_bytes_reserved, 0);
            assert_eq!(counters.storage_inodes_requested, 0);
            assert_eq!(counters.storage_inodes_reserved, 0);
            assert_eq!(fs::read_dir(root.join("preparation")).unwrap().count(), 0);
            assert_eq!(cas.ensure_valid(), Err(FsCasErrorV1::Invalidated));
            assert_eq!(stale.ensure_valid(), Err(FsCasErrorV1::Invalidated));
            assert!(matches!(
                FsCasV1::open_existing(&root),
                Err(FsCasErrorV1::Busy | FsCasErrorV1::Invalidated)
            ));

            drop(admission);
            drop(stale);
            drop(cas);
            assert!(matches!(
                FsCasV1::open_existing(&root),
                Err(FsCasErrorV1::Busy | FsCasErrorV1::Invalidated)
            ));
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn file_pack_read_observation_is_transactional_and_retains_overflow_cause() {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let root = parent.join(format!(
            "layerfs-pack-read-observation-{}-{}",
            std::process::id(),
            NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let path = root.join("pack");
        fs::write(&path, [0x4a, 0x5b, 0x6c, 0x7d]).unwrap();

        let mut reader = FilePackReadV1::open(&path).unwrap();
        reader.bytes_read = 7;
        reader.read_calls = u64::MAX;
        let before = (reader.bytes_read, reader.read_calls);
        let mut destination = [0_u8; 2];

        assert_eq!(
            PackReadPortV1::read_exact_at(&mut reader, 0, &mut destination),
            Err(PackPortErrorV1::Failure)
        );
        assert_eq!(destination, [0x4a, 0x5b]);
        assert_eq!((reader.bytes_read, reader.read_calls), before);
        assert_eq!(
            reader.restore_failure_v1(FsCasErrorV1::MalformedOccupant),
            FsCasErrorV1::Core(CoreError::IntegerOverflow)
        );

        drop(reader);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn complete_object_comparator_distinguishes_unequal_occupant_bytes() {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let root = parent.join(format!(
            "layerfs-object-compare-{}-{}",
            std::process::id(),
            NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let candidate_path = root.join("candidate");
        let incumbent_path = root.join("incumbent");
        let len = COMPARISON_WINDOW_BYTES / 2 + 1;
        let candidate_bytes = vec![0x5a; len];
        let mut incumbent_bytes = candidate_bytes.clone();
        incumbent_bytes[len - 1] ^= 0xff;
        fs::write(&candidate_path, candidate_bytes).unwrap();
        fs::write(&incumbent_path, incumbent_bytes).unwrap();

        let mut candidate = FilePackReadV1::open(&candidate_path).unwrap();
        let mut incumbent = FilePackReadV1::open(&incumbent_path).unwrap();
        let location = PackObjectLocationV1 {
            object_offset: 0,
            object_len: len as u64,
        };
        let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut counters = OperationCountersV1::default();
        let mut control = ContinueFsCasControlV1;
        assert_eq!(
            compare_complete_object_bytes(
                &mut candidate,
                location,
                &mut incumbent,
                location,
                &mut scratch,
                &mut counters,
                &mut control,
            ),
            Err(FsCasErrorV1::UnequalOccupant)
        );
        drop(candidate);
        drop(incumbent);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn occupied_payload_read_releases_visibility_lock_around_file_io() {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let root = parent.join(format!(
            "layerfs-unlocked-payload-read-{}-{}",
            std::process::id(),
            NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
        ));
        let cas = FsCasV1::create_new(&root).unwrap();
        let payload_path = root.join("test-payload");
        let payload = b"payload read outside the root visibility lock";
        fs::write(&payload_path, payload).unwrap();
        let id = TypedPhysicalObjectIdV1::Chunk(crate::identity::PhysicalChunkIdV1::from_digest(
            [0x41; 32],
        ));
        let entered = Arc::new(std::sync::Barrier::new(2));
        let release = Arc::new(std::sync::Barrier::new(2));
        let entered_hook = Arc::clone(&entered);
        let release_hook = Arc::clone(&release);
        let mut occupied = FsCasOccupiedV1 {
            cas: cas.clone(),
            current: Some(ResolvedObjectV1 {
                id,
                file: File::open(&payload_path).unwrap(),
                pack_len: payload.len() as u64,
                location: PackObjectLocationV1 {
                    object_offset: 0,
                    object_len: payload.len() as u64,
                },
            }),
            previous: None,
            bytes_read: 0,
            read_calls: 0,
            first_error: None,
            validation_scratch: [0; COMPARISON_WINDOW_BYTES],
            unlocked_payload_read_hook: Some(Arc::new(move || {
                entered_hook.wait();
                release_hook.wait();
            })),
            payload_read_observation_for_test: None,
        };

        let worker = std::thread::spawn(move || {
            let mut actual = vec![0; payload.len()];
            let result = occupied.read_occupied_exact_at_typed_v1(id, 0, &mut actual);
            (result, actual, occupied.bytes_read, occupied.read_calls)
        });
        entered.wait();
        let visibility_lock_available = cas.inner.visibility.try_lock().is_ok();
        release.wait();
        let (result, actual, bytes_read, read_calls) = worker.join().unwrap();
        assert!(visibility_lock_available);
        assert_eq!(result, Ok(()));
        assert_eq!(actual, payload);
        assert_eq!(bytes_read, payload.len() as u64);
        assert_eq!(read_calls, 1);

        fs::remove_file(payload_path).unwrap();
        drop(cas);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn occupied_payload_read_classifies_pre_read_carrier_shape_change_as_malformed() {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let root = parent.join(format!(
            "layerfs-malformed-payload-read-{}-{}",
            std::process::id(),
            NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
        ));
        let cas = FsCasV1::create_new(&root).unwrap();
        let payload_path = root.join("test-payload");
        let payload = b"authenticated carrier bytes changed before payload delivery";
        fs::write(&payload_path, payload).unwrap();
        let id = TypedPhysicalObjectIdV1::Chunk(crate::identity::PhysicalChunkIdV1::from_digest(
            [0x42; 32],
        ));
        let truncate_path = payload_path.clone();
        let mut occupied = FsCasOccupiedV1 {
            cas: cas.clone(),
            current: Some(ResolvedObjectV1 {
                id,
                file: File::open(&payload_path).unwrap(),
                pack_len: payload.len() as u64,
                location: PackObjectLocationV1 {
                    object_offset: 0,
                    object_len: payload.len() as u64,
                },
            }),
            previous: None,
            bytes_read: 0,
            read_calls: 0,
            first_error: None,
            validation_scratch: [0; COMPARISON_WINDOW_BYTES],
            unlocked_payload_read_hook: Some(Arc::new(move || {
                OpenOptions::new()
                    .write(true)
                    .open(&truncate_path)
                    .unwrap()
                    .set_len((payload.len() - 1) as u64)
                    .unwrap();
            })),
            payload_read_observation_for_test: None,
        };

        let mut actual = vec![0; payload.len()];
        assert_eq!(
            occupied.read_occupied_exact_at_typed_v1(id, 0, &mut actual),
            Err(FsCasErrorV1::MalformedOccupant)
        );
        assert_eq!(occupied.bytes_read, 0);
        assert_eq!(occupied.read_calls, 0);
        assert_eq!(occupied.first_error, None);

        fs::remove_file(payload_path).unwrap();
        drop(cas);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn poisoned_visibility_mutex_durably_invalidates_shared_and_reopened_root() {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let root = parent.join(format!(
            "layerfs-poisoned-visibility-{}-{}",
            std::process::id(),
            NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
        ));
        let cas = FsCasV1::create_new(&root).unwrap();
        let stale = cas.clone();
        let poison = cas.clone();

        let unwind = std::thread::spawn(move || {
            let _guard = poison.inner.visibility.lock().unwrap();
            panic!("inject visibility-mutex poison");
        })
        .join();
        assert!(unwind.is_err());

        assert!(matches!(
            cas.occupied_private_v1(),
            Err(FsCasErrorV1::SynchronizationPoisoned)
        ));
        assert!(cas.inner.invalidated.load(Ordering::Acquire));
        assert!(matches!(
            stale.occupied_private_v1(),
            Err(FsCasErrorV1::Invalidated)
        ));
        assert!(matches!(
            FsCasV1::open_existing(&root),
            Err(FsCasErrorV1::Invalidated)
        ));

        drop(stale);
        drop(cas);
        assert!(matches!(
            FsCasV1::open_existing(&root),
            Err(FsCasErrorV1::Invalidated)
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn poisoned_root_lock_preserves_poison_through_invalidation_double_fault() {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        for (case, publication) in [("visibility", false), ("publication", true)] {
            let root = parent.join(format!(
                "layerfs-poisoned-{case}-double-fault-{}-{}",
                std::process::id(),
                NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
            ));
            let cas = FsCasV1::create_new(&root).unwrap();
            let stale = cas.clone();
            let poison = cas.clone();
            assert!(std::thread::spawn(move || {
                if publication {
                    let _guard = poison.inner.publication.lock().unwrap();
                    panic!("inject publication-mutex poison");
                } else {
                    let _guard = poison.inner.visibility.lock().unwrap();
                    panic!("inject visibility-mutex poison");
                }
            })
            .join()
            .is_err());

            let mut control = InvalidationCauseControlV1 {
                token_boundary: None,
                token_error: None,
                marker_error: None,
                // Refuse both persistent barriers. The root must retain the
                // lock poison as the chronological first cause while the
                // invalidation double fault remains terminally dominant.
                skip_token: true,
            };
            let error = if publication {
                cas.lock_publication_controlled_v1(&mut control)
                    .map(drop)
                    .unwrap_err()
            } else {
                cas.lock_visibility_controlled_v1(&mut control)
                    .map(drop)
                    .unwrap_err()
            };
            assert_eq!(
                error.failure_causes_v1(),
                (
                    FsCasFailureCauseV1::SynchronizationPoisoned,
                    FsCasFailureCauseV1::InvalidationFailed,
                ),
                "{case}"
            );
            assert_eq!(cas.ensure_valid(), Err(FsCasErrorV1::Invalidated), "{case}");
            assert_eq!(
                stale.ensure_valid(),
                Err(FsCasErrorV1::Invalidated),
                "{case}"
            );
            let lock = if publication {
                &cas.inner.publication
            } else {
                &cas.inner.visibility
            };
            match lock.try_lock() {
                Ok(guard) => drop(guard),
                Err(TryLockError::Poisoned(poisoned)) => drop(poisoned.into_inner()),
                Err(TryLockError::WouldBlock) => panic!("{case}: root lock remained held"),
            }
            assert!(matches!(
                FsCasV1::open_existing(&root),
                Err(FsCasErrorV1::Busy | FsCasErrorV1::Invalidated)
            ));

            drop(stale);
            drop(cas);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn atomic_marker_no_replace_returns_authenticated_incumbent_and_cleans_alias() {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let root = parent.join(format!(
            "layerfs-marker-incumbent-{}-{}",
            std::process::id(),
            NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let preparation = root.join("preparation");
        fs::create_dir(&preparation).unwrap();
        let destination = root.join("marker");
        let incumbent = *b"MARKER01";
        fs::write(&destination, incumbent).unwrap();

        assert_eq!(
            publish_small_marker(&preparation, "test", &destination, b"CANDID01"),
            Ok(MarkerPublicationV1::IncumbentClean(incumbent))
        );
        assert_eq!(fs::read(&destination).unwrap(), incumbent);
        assert_eq!(fs::read_dir(&preparation).unwrap().count(), 0);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fallible_regular_file_lookup_never_turns_io_failure_into_missing() {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let root = parent.join(format!(
            "layerfs-fallible-lookup-{}-{}",
            std::process::id(),
            NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let non_directory = root.join("not-a-directory");
        fs::write(&non_directory, b"occupied").unwrap();
        assert!(matches!(
            open_regular_file_if_present(&non_directory.join("child")),
            Err(FsCasErrorV1::Filesystem(
                FsCasFilesystemFailureV1::ReadFailure
            ))
        ));

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            let dangling = root.join("dangling");
            symlink(root.join("absent-target"), &dangling).unwrap();
            assert!(matches!(
                open_regular_file_if_present(&dangling),
                Err(FsCasErrorV1::Integrity)
            ));
        }

        assert!(open_regular_file_if_present(&root.join("actually-absent"))
            .unwrap()
            .is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(all(unix, feature = "c3-polymorphism"))]
    #[test]
    fn unix_post_open_path_replacement_fails_closed_with_exact_terminal_authority() {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let sequence = NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed);
        let root = parent.join(format!(
            "layerfs-post-open-root-{}-{sequence}",
            std::process::id(),
        ));
        let probe = parent.join(format!(
            "layerfs-post-open-probe-{}-{sequence}",
            std::process::id(),
        ));
        let cas = FsCasV1::create_new(&root).unwrap();
        let stale = FsCasV1::open_existing(&root).unwrap();
        fs::create_dir(&probe).unwrap();
        let occupant = probe.join("occupant");
        let replacement = probe.join("replacement");
        let displaced = probe.join("displaced-original");
        fs::write(&occupant, b"opened-original").unwrap();
        fs::write(&replacement, b"substituted-path").unwrap();

        let mut control = ContinueFsCasControlV1;
        let mut counters = OperationCountersV1::default();
        let mut operation = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x0011_5524,
                &mut counters,
                &mut control,
            )
            .unwrap();
        operation
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(0, 0, 0, 0).unwrap())
            .unwrap();

        let opened = open_regular_file_if_present_with_post_open_hook_v1(&occupant, || {
            fs::rename(&occupant, &displaced).unwrap();
            fs::rename(&replacement, &occupant).unwrap();
        });
        assert!(matches!(opened, Err(FsCasErrorV1::Integrity)));
        assert_eq!(fs::read(&occupant).unwrap(), b"substituted-path");
        assert_eq!(fs::read(&displaced).unwrap(), b"opened-original");

        operation
            .finish_terminal_v1(false, &mut counters, &mut control)
            .unwrap();
        drop(operation);
        assert_eq!(cas.operation_admitted_slots_v1(), 0);
        assert_eq!(cas.operation_admission_active_for_test_v1(), 0);
        assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
        assert!(cas.visibility_lock_available_for_test_v1());
        assert!(cas.publication_lock_available_for_test_v1());
        assert_eq!(fs::read_dir(root.join("preparation")).unwrap().count(), 0);
        assert_eq!(counters.root_admission_queue_entries, 1);
        assert_eq!(counters.root_admission_queue_refusals, 0);
        assert_eq!(counters.root_admission_release_failures, 0);
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
        assert_eq!(counters.mutable_preparation_residue_bytes, 0);
        assert_eq!(counters.mutable_preparation_residue_inodes, 0);
        assert_eq!(counters.unreachable_installed_residue_bytes, 0);
        assert!(counters.has_zero_forbidden_work());
        assert!(cas.occupied().is_ok());
        assert!(stale.occupied().is_ok());
        let reopened = FsCasV1::open_existing(&root).unwrap();
        assert!(reopened.occupied().is_ok());

        // This proof is scoped to the current Unix dev/inode provider. It is
        // not evidence for a future non-Unix identity implementation.
        drop(reopened);
        drop(stale);
        drop(cas);
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(probe).unwrap();
    }

    #[cfg(feature = "c3-polymorphism")]
    #[test]
    fn cloned_and_reopened_handles_share_one_sixteen_slot_domain() {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let root = parent.join(format!(
            "layerfs-root-admission-{}-{}",
            std::process::id(),
            NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed)
        ));
        let first = FsCasV1::create_new(&root).unwrap();
        let mut handles = Vec::with_capacity(17);
        for ordinal in 0..17 {
            if ordinal % 2 == 0 {
                handles.push(first.clone());
            } else {
                handles.push(FsCasV1::open_existing(&root).unwrap());
            }
        }
        assert!(handles
            .iter()
            .all(|handle| Arc::ptr_eq(&first.inner, &handle.inner)));

        let mut control = ContinueFsCasControlV1;
        let mut counters = OperationCountersV1::default();
        let mut capabilities = Vec::with_capacity(16);
        for handle in handles.iter().take(16) {
            capabilities.push(
                handle
                    .begin_operation_capability_v1(
                        FsOperationKindV1::CompleteC3File,
                        10,
                        &mut counters,
                        &mut control,
                    )
                    .unwrap(),
            );
        }
        assert_eq!(first.inner.operation_ledger.admitted_slots(), 16);
        assert_eq!(
            first.inner.operation_admission.state.lock().unwrap().active,
            16
        );
        assert_eq!(fs::read_dir(root.join("preparation")).unwrap().count(), 0);

        let mut cancel = CancelControlV1;
        assert!(matches!(
            handles[16].begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3Tree,
                11,
                &mut counters,
                &mut cancel,
            ),
            Err(FsCasErrorV1::Core(CoreError::Cancelled))
        ));
        assert_eq!(first.inner.operation_ledger.admitted_slots(), 16);
        assert_eq!(fs::read_dir(root.join("preparation")).unwrap().count(), 0);

        capabilities.pop();
        let replacement = handles[16]
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3Tree,
                12,
                &mut counters,
                &mut control,
            )
            .unwrap();
        assert_eq!(first.inner.operation_ledger.admitted_slots(), 16);
        drop(replacement);
        drop(capabilities);
        assert_eq!(first.inner.operation_ledger.admitted_slots(), 0);
        drop(handles);
        drop(first);
        fs::remove_dir_all(root).unwrap();
    }
}
