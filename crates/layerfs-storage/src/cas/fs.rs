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
    decode_persistent_locator_v1, encode_persistent_locator_v1, PersistentObjectLocatorV1,
    PERSISTENT_LOCATOR_BYTES_V1,
};
use crate::cas::{
    admit_complete_immutable_v1, AdmissionBuffersV1, AdmittedClosureV1,
    CompleteImmutableClosureReadPortV1, ImmutablePortErrorV1, OccupiedImmutableReadPortV1,
    PreparedImmutableClosurePortV1, ValidatedOccupiedObjectV1,
};
use crate::identity::{PackIdV1, PhysicalVersionRecordIdV1, COMPARISON_WINDOW_BYTES};
use crate::limits::{
    admitted_slots_for_budget, OperationCountersV1, OperationMemoryPlanV1, OperationReservationV1,
    ResourceLedgerV1, BASE_LEDGER_BYTES, MEMORY_PROFILE_72_MIB,
};
use crate::object::TypedPhysicalObjectIdV1;
use crate::pack::{
    locate_validated_pack_index_entry_v1, validate_pack_v1, validate_validated_pack_object_v1,
    PackIndexEntryV1, PackIndexSpoolV1, PackObjectLocationV1, PackPortErrorV1, PackReadPortV1,
    PrivatePackPortV1, SealedPackV1, MAX_PACK_BYTES,
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
    /// An operation-local storage authority was presented to a different
    /// shared root owner or to a later owner generation.  This is distinct
    /// from a stale/replayed nonce within the correct owner.
    CrossOwner,
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
    ShortWrite,
    PermissionDenied,
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
    CarrierUnlink,
    LocatorUnlink,
    InvalidationWrite,
    InvalidationFlush,
    InvalidationMarkerCreate,
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

    pub(crate) fn finish_v1(self, counters: &mut OperationCountersV1) -> CoreResult<()> {
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
        if self.outcome == FsPackAdmissionOutcomeV1::Installed {
            counters.record_unreachable_installed_residue(self.sealed.pack_len())?;
        }
        Ok(())
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
    envelope: Option<FsStorageEnvelopeV1>,
    preparation_current: RootStorageUsageV1,
    preparation_high_water: RootStorageUsageV1,
    immutable_pending: RootStorageUsageV1,
}

struct RootStorageAdmissionStateV1 {
    immutable: RootStorageUsageV1,
    preparation: RootStorageUsageV1,
    active_reserved: RootStorageUsageV1,
    reserved_high_water: RootStorageUsageV1,
    next_nonce: u64,
    operations: [RootStorageOperationStateV1; ROOT_STORAGE_OPERATION_SLOTS_V1],
}

struct RootStorageAdmissionV1 {
    identity: FsStorageOwnerIdentityV1,
    byte_capacity: u64,
    inode_capacity: u64,
    fixed_reservation: RootStorageUsageV1,
    state: Mutex<RootStorageAdmissionStateV1>,
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
            }),
        })
    }

    fn reserve(
        &self,
        envelope: FsStorageEnvelopeV1,
    ) -> Result<RootStorageLeaseV1<'_>, FsCasErrorV1> {
        let requested = RootStorageUsageV1 {
            bytes: envelope.requested_bytes(),
            inodes: envelope.requested_inodes(),
        };
        let mut state = self.state.lock().map_err(|_| FsCasErrorV1::Integrity)?;
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
        Ok(operation)
    }

    fn record_preparation_create_v1(
        &self,
        token: FsStorageOperationTokenV1,
    ) -> Result<(), FsCasErrorV1> {
        let mut state = self.state.lock().map_err(|_| FsCasErrorV1::Integrity)?;
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
        let mut state = self.state.lock().map_err(|_| FsCasErrorV1::Integrity)?;
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
        Ok(())
    }

    fn record_preparation_remove_v1(
        &self,
        token: FsStorageOperationTokenV1,
        len: u64,
    ) -> Result<(), FsCasErrorV1> {
        let mut state = self.state.lock().map_err(|_| FsCasErrorV1::Integrity)?;
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
        Ok(())
    }

    fn record_immutable_install_v1(
        &self,
        token: FsStorageOperationTokenV1,
        bytes: u64,
        inodes: u64,
    ) -> Result<(), FsCasErrorV1> {
        let mut state = self.state.lock().map_err(|_| FsCasErrorV1::Integrity)?;
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
        let mut state = self.state.lock().map_err(|_| FsCasErrorV1::Integrity)?;
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
        Ok(())
    }

    fn release_without_observation(&self, slot: usize, nonce: u64) -> Result<(), FsCasErrorV1> {
        let mut state = self.state.lock().map_err(|_| FsCasErrorV1::Integrity)?;
        let operation = state
            .operations
            .get(slot)
            .copied()
            .ok_or(FsCasErrorV1::Integrity)?;
        if !operation.active || operation.nonce != nonce {
            return Err(FsCasErrorV1::Integrity);
        }
        let Some(envelope) = operation.envelope.filter(|_| operation.active) else {
            return Err(FsCasErrorV1::Integrity);
        };
        let inconsistent = operation.preparation_current != RootStorageUsageV1::default()
            || operation.immutable_pending != RootStorageUsageV1::default();
        let next_reserved_bytes = state
            .active_reserved
            .bytes
            .checked_sub(envelope.requested_bytes())
            .ok_or(FsCasErrorV1::Integrity)?;
        let next_reserved_inodes = state
            .active_reserved
            .inodes
            .checked_sub(envelope.requested_inodes())
            .ok_or(FsCasErrorV1::Integrity)?;
        // An unwind has no ordinary terminal record, but it must not erase
        // the direct operation-local lifecycle events.  Fold every installed
        // immutable name and every still-live preparation name into the
        // shared root's retained domains before clearing the operation cell.
        // Immutable state is operation-relative retained residue: another
        // successful operation may later authenticate and reuse it, but that
        // does not make this operation's terminal attribution disappear.
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
            .and_then(|bytes| bytes.checked_add(next_reserved_bytes))
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        let terminal_inodes = next_immutable
            .inodes
            .checked_add(next_preparation.inodes)
            .and_then(|inodes| inodes.checked_add(self.fixed_reservation.inodes))
            .and_then(|inodes| inodes.checked_add(next_reserved_inodes))
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        if terminal_bytes > self.byte_capacity || terminal_inodes > self.inode_capacity {
            return Err(FsCasErrorV1::Integrity);
        }
        state.active_reserved = RootStorageUsageV1 {
            bytes: next_reserved_bytes,
            inodes: next_reserved_inodes,
        };
        state.immutable = next_immutable;
        state.preparation = next_preparation;
        state.operations[slot] = RootStorageOperationStateV1::default();
        if inconsistent {
            Err(FsCasErrorV1::Integrity)
        } else {
            Ok(())
        }
    }

    fn finish(
        &self,
        slot: usize,
        nonce: u64,
        commit: bool,
        counters: &mut OperationCountersV1,
    ) -> Result<(), FsCasErrorV1> {
        let mut state = self.state.lock().map_err(|_| FsCasErrorV1::Integrity)?;
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
            .map_err(FsCasErrorV1::Core)?;
        state.active_reserved = next_active_reserved;
        state.immutable = next_immutable;
        state.preparation = next_preparation;
        state.operations[slot] = RootStorageOperationStateV1::default();
        *counters = terminal_counters;
        Ok(())
    }
}

struct RootStorageLeaseV1<'owner> {
    admission: &'owner RootStorageAdmissionV1,
    slot: usize,
    nonce: u64,
    released: bool,
}

impl RootStorageLeaseV1<'_> {
    fn token_v1(&self) -> Result<FsStorageOperationTokenV1, FsCasErrorV1> {
        Ok(FsStorageOperationTokenV1 {
            owner: self.admission.identity,
            slot: u8::try_from(self.slot).map_err(|_| FsCasErrorV1::Integrity)?,
            nonce: self.nonce,
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
    ) -> Result<PendingAdmissionTicketV1<'_>, FsCasErrorV1> {
        let mut state = self.state.lock().map_err(|_| FsCasErrorV1::Integrity)?;
        let outstanding = state
            .next_ticket
            .checked_sub(state.serving_ticket)
            .ok_or(FsCasErrorV1::Integrity)?;
        if outstanding >= MAX_ADMISSION_TICKETS as u64 {
            counters
                .record_root_admission_queue_refusal_v1()
                .map_err(FsCasErrorV1::Core)?;
            return Err(FsCasErrorV1::ResourceExhausted(FsCasResourceV1::Queue));
        }
        let waiting_depth = outstanding
            .checked_add(1)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        counters
            .record_root_admission_queue_entry_v1(waiting_depth)
            .map_err(FsCasErrorV1::Core)?;
        let ticket = state.next_ticket;
        state.next_ticket = state
            .next_ticket
            .checked_add(1)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        let slot = usize::try_from(ticket % MAX_ADMISSION_TICKETS as u64)
            .map_err(|_| FsCasErrorV1::Integrity)?;
        if state.tickets[slot] != AdmissionTicketStateV1::Empty {
            return Err(FsCasErrorV1::Integrity);
        }
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
    ) -> Result<RootAdmissionLeaseV1<'queue>, FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        let started = Instant::now();
        let result = self
            .issue(operation_kind, cancellation_key, counters)
            .and_then(|ticket| ticket.wait(control, counters));
        let elapsed = u64::try_from(started.elapsed().as_nanos())
            .map_err(|_| FsCasErrorV1::Core(CoreError::IntegerOverflow));
        match elapsed {
            Ok(nanoseconds) => counters
                .record_root_admission_wait_v1(nanoseconds)
                .map_err(FsCasErrorV1::Core)?,
            Err(error) => return Err(error),
        }
        result
    }

    fn cancel_pending(&self, ticket: u64, slot: usize) -> Result<(), FsCasErrorV1> {
        let mut state = self.state.lock().map_err(|_| FsCasErrorV1::Io)?;
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
        let mut state = self.state.lock().map_err(|_| FsCasErrorV1::Integrity)?;
        state.active = state.active.checked_sub(1).ok_or(FsCasErrorV1::Integrity)?;
        self.changed.notify_all();
        Ok(())
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
    ) -> Result<RootAdmissionLeaseV1<'queue>, FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        let mut state = self
            .queue
            .state
            .lock()
            .map_err(|_| FsCasErrorV1::Integrity)?;

        loop {
            advance_cancelled_tickets_v1(&mut state)?;
            if self.ticket == state.serving_ticket && state.active < self.queue.capacity {
                if state.tickets[self.slot] != AdmissionTicketStateV1::Waiting {
                    return Err(FsCasErrorV1::Integrity);
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

            let stop = if control.cancellation_requested() {
                Some(CoreError::Cancelled)
            } else if control.deadline_exceeded() {
                Some(CoreError::Deadline)
            } else {
                None
            };
            if let Some(error) = stop {
                if state.tickets[self.slot] != AdmissionTicketStateV1::Waiting {
                    return Err(FsCasErrorV1::Integrity);
                }
                state.tickets[self.slot] = AdmissionTicketStateV1::Cancelled;
                advance_cancelled_tickets_v1(&mut state)?;
                self.resolved = true;
                self.queue.changed.notify_all();
                return Err(FsCasErrorV1::Core(error));
            }

            counters
                .record_root_admission_wait_poll_v1()
                .map_err(FsCasErrorV1::Core)?;
            let (observed, _) = self
                .queue
                .changed
                .wait_timeout(state, ADMISSION_CONTROL_POLL)
                .map_err(|_| FsCasErrorV1::Integrity)?;
            state = observed;
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
/// owner. Lower storage layers may borrow its reservation but cannot mint,
/// replace, or widen it.
#[cfg(feature = "c3-polymorphism")]
pub(crate) struct FsOperationCapabilityV1<'owner> {
    owner: &'owner FsCasV1,
    reservation: OperationReservationV1<'owner>,
    storage: Option<RootStorageLeaseV1<'owner>>,
    admission: RootAdmissionLeaseV1<'owner>,
}

#[cfg(feature = "c3-polymorphism")]
impl FsOperationCapabilityV1<'_> {
    pub(crate) fn owner_ref_v1(&self) -> &FsCasV1 {
        self.owner
    }

    pub(crate) fn declare_plan_v1(&mut self, plan: OperationMemoryPlanV1) -> CoreResult<()> {
        self.reservation.declare_plan(plan)
    }

    pub(crate) fn ledger_v1(&self) -> &ResourceLedgerV1 {
        &self.owner.inner.operation_ledger
    }

    pub(crate) fn reservation_v1(&self) -> &OperationReservationV1<'_> {
        &self.reservation
    }

    pub(crate) fn declare_storage_envelope_v1(
        &mut self,
        envelope: FsStorageEnvelopeV1,
    ) -> Result<(), FsCasErrorV1> {
        if self.storage.is_some() {
            return Err(FsCasErrorV1::Integrity);
        }
        self.owner.ensure_valid()?;
        self.storage = Some(self.owner.inner.storage_admission.reserve(envelope)?);
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
            Ok(()) => {
                lease.released = true;
                Ok(())
            }
            Err(error) => Err(self
                .owner
                .invalidate_root_controlled_v1(control)
                .err()
                .unwrap_or(error)),
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
                let invalidation = self.owner.invalidate_root_controlled_v1(control);
                Err(invalidation
                    .err()
                    .or_else(|| observation.err())
                    .unwrap_or(error))
            }
        }
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

#[cfg(feature = "c3-polymorphism")]
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
    let registry = OPEN_ROOTS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut roots = registry.lock().map_err(|_| FsCasErrorV1::Io)?;
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
    let ownership = match acquired_ownership {
        Some(ownership) => ownership,
        None => acquire_root_ownership(root, generation)?,
    };
    let (immutable_storage, preparation_storage) = observe_root_storage_usage_v1(root)?;
    let owner = Arc::new(FsCasInnerV1 {
        root: root.to_path_buf(),
        generation,
        invalidated: AtomicBool::new(false),
        ownership: Mutex::new(Some(ownership)),
        operation_ledger: ResourceLedgerV1::new(MEMORY_PROFILE_72_MIB),
        operation_admission: OperationAdmissionQueueV1::new(admitted_slots_for_budget(
            MEMORY_PROFILE_72_MIB,
        ))?,
        storage_admission: RootStorageAdmissionV1::new(
            immutable_storage,
            preparation_storage,
            generation,
        )?,
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
    for entry in fs::read_dir(directory).map_err(|_| FsCasErrorV1::Io)? {
        let entry = entry.map_err(|_| FsCasErrorV1::Io)?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(|_| FsCasErrorV1::Io)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(FsCasErrorV1::Integrity);
        }
        usage.inodes = usage
            .inodes
            .checked_add(1)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        usage.bytes = usage
            .bytes
            .checked_add(metadata.len())
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        if usage.inodes > ROOT_NAMESPACE_ENTRY_BUDGET_V1
            || usage.bytes > ROOT_LOGICAL_STORAGE_BUDGET_V1
        {
            return Err(FsCasErrorV1::ResourceExhausted(
                FsCasResourceV1::StorageBytes,
            ));
        }
    }
    Ok(usage)
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
        let metadata = fs::symlink_metadata(root.join(fixed)).map_err(|_| FsCasErrorV1::Io)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(FsCasErrorV1::Integrity);
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
    pub(crate) fn hold_visibility_lock_for_test_v1(&self) -> MutexGuard<'_, ()> {
        self.inner
            .visibility
            .lock()
            .expect("test visibility lock must not already be poisoned")
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
                control.boundary_reached(terminated_boundary);
                return Err(error);
            }
            match mutex.try_lock() {
                Ok(guard) => {
                    if let Err(error) = self.ensure_valid() {
                        drop(guard);
                        control.boundary_reached(terminated_boundary);
                        return Err(error);
                    }
                    control.boundary_reached(acquired_boundary);
                    return Ok(guard);
                }
                Err(TryLockError::WouldBlock) => {
                    control.boundary_reached(contended_boundary);
                    if control.cancellation_requested() {
                        control.boundary_reached(terminated_boundary);
                        return Err(FsCasErrorV1::Core(CoreError::Cancelled));
                    }
                    if control.deadline_exceeded() {
                        control.boundary_reached(terminated_boundary);
                        return Err(FsCasErrorV1::Core(CoreError::Deadline));
                    }
                    std::thread::sleep(ADMISSION_CONTROL_POLL);
                }
                Err(TryLockError::Poisoned(poisoned)) => {
                    // Release the recovered guard before invalidation. A
                    // poisoned root coordination primitive is an impossible
                    // shared-owner state, not a retryable filesystem I/O
                    // error. Persist fail-closed invalidation immediately.
                    drop(poisoned.into_inner());
                    let invalidation = self.invalidate_root_controlled_v1(control);
                    control.boundary_reached(terminated_boundary);
                    invalidation?;
                    return Err(FsCasErrorV1::Invalidated);
                }
            }
        }
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
            Ok(admission) => admission,
            Err(error @ FsCasErrorV1::Integrity) => {
                return Err(self
                    .invalidate_root_controlled_v1(control)
                    .err()
                    .unwrap_or(error));
            }
            Err(error) => return Err(error),
        };
        if let Err(error) = self.ensure_valid() {
            if admission.release_v1().is_err() {
                return Err(self
                    .invalidate_root_controlled_v1(control)
                    .err()
                    .unwrap_or(FsCasErrorV1::Integrity));
            }
            return Err(error);
        }
        let reservation = match self.inner.operation_ledger.reserve_operation_unplanned() {
            Ok(reservation) => Ok(reservation),
            Err(CoreError::ResourceRefused) => {
                counters
                    .record_root_admission_memory_refusal_v1()
                    .map_err(FsCasErrorV1::Core)?;
                Err(FsCasErrorV1::ResourceExhausted(FsCasResourceV1::Memory))
            }
            Err(other) => Err(FsCasErrorV1::Core(other)),
        };
        let reservation = match reservation {
            Ok(reservation) => reservation,
            Err(error) => {
                let release = admission.release_v1();
                if release.is_err() {
                    return Err(self
                        .invalidate_root_controlled_v1(control)
                        .err()
                        .unwrap_or(FsCasErrorV1::Integrity));
                }
                return Err(error);
            }
        };
        Ok(FsOperationCapabilityV1 {
            owner: self,
            reservation,
            storage: None,
            admission,
        })
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
                let _ = fs::remove_dir_all(root);
                return Err(error);
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
            let _ = fs::remove_dir_all(root);
            return Err(error);
        }
        let cas = Self {
            inner: shared_root_owner(root, generation, Some(ownership))?,
        };
        if cas.fixed_handle_ledger_charge_bytes()? > BASE_LEDGER_BYTES {
            let _ = fs::remove_dir_all(root);
            return Err(FsCasErrorV1::Core(CoreError::ResourceRefused));
        }
        Ok(cas)
    }

    /// Reopen the committed catalog only. Preparation files are never
    /// scanned, adopted, replayed, or promoted.
    pub fn open_existing(root: &Path) -> Result<Self, FsCasErrorV1> {
        validate_existing_root(root)?;
        if root_invalidation_barrier_present_v1(&root.join(INVALIDATED_ROOT_NAME)) {
            return Err(FsCasErrorV1::Invalidated);
        }
        for child in ["preparation", "carriers", "objects", "catalog", "closures"] {
            validate_private_directory(&root.join(child))?;
            validate_same_filesystem(root, &root.join(child))?;
        }
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
        self.ensure_valid()?;
        let _guard = self.lock_visibility_v1()?;
        self.ensure_valid()?;
        validate_private_directory(&self.inner.root.join("preparation"))?;
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
    #[cfg(feature = "c3-polymorphism")]
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
        self.ensure_valid()?;
        let _guard = self.lock_visibility_controlled_v1(control)?;
        self.ensure_valid()?;
        validate_private_directory(&self.inner.root.join("preparation"))?;
        let path = unique_private_path(&self.inner.root.join("preparation"), prefix)?;
        sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::PreparationCreate)?;
        if let Some(token) = storage_token {
            // Charge the private namespace name before invoking the creating
            // filesystem operation.  A poisoned/stale accounting owner must
            // therefore fail without leaving an untracked preparation inode.
            self.record_storage_preparation_create_v1(token)
                .inspect_err(|_| self.invalidate_root_backstop_v1())?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| map_filesystem_io_error_v1(&error));
        let file = match file {
            Ok(file) => file,
            Err(error) => {
                if let Some(token) = storage_token {
                    self.record_storage_preparation_remove_v1(token, 0)
                        .inspect_err(|_| self.invalidate_root_backstop_v1())?;
                }
                return Err(error);
            }
        };
        let permissions =
            sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::PermissionChange)
                .and_then(|()| set_private_file_permissions(&path));
        if let Err(error) = permissions {
            drop(file);
            if fs::remove_file(&path).is_err() {
                return Err(self.cleanup_failure_controlled_v1(
                    FsCasCleanupTargetV1::PreparationSpool,
                    control,
                ));
            }
            if let Some(token) = storage_token {
                self.record_storage_preparation_remove_v1(token, 0)
                    .inspect_err(|_| self.invalidate_root_backstop_v1())?;
            }
            return Err(error);
        }
        Ok(FsOperationSpoolV1 {
            owner: self.clone(),
            path,
            file: Some(file),
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
            prepared, metadata, ledger, None, None, counters, scratch, control,
        )
    }

    #[cfg(feature = "c3-polymorphism")]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn admit_pack_borrowed_controlled_v1<M, C>(
        &self,
        prepared: &mut FsPrivatePackV1,
        metadata: &mut M,
        ledger: &ResourceLedgerV1,
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
            ledger,
            Some(reservation),
            Some(storage_token),
            counters,
            scratch,
            control,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn admit_pack_controlled_inner<M, C>(
        &self,
        prepared: &mut FsPrivatePackV1,
        metadata: &mut M,
        ledger: &ResourceLedgerV1,
        reservation: Option<&OperationReservationV1<'_>>,
        storage_token: Option<FsStorageOperationTokenV1>,
        counters: &mut OperationCountersV1,
        scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
        control: &mut C,
    ) -> Result<FsPackAdmissionV1, FsCasErrorV1>
    where
        M: PackIndexSpoolV1 + ?Sized,
        C: FsCasControlV1 + ?Sized,
    {
        self.ensure_valid()?;
        let result = (|| {
            if !Arc::ptr_eq(&prepared.owner.inner, &self.inner) {
                return Err(FsCasErrorV1::Integrity);
            }
            counters.observe_layerfs_open_file_handles(1);
            sample_control(control, FsCasBoundaryV1::BeforeCandidateValidation)?;
            let declared = prepared.sealed().ok_or(FsCasErrorV1::Integrity)?;
            let validated = validate_pack_for_operation_v1(
                prepared,
                metadata,
                scratch,
                declared.record_count(),
                ledger,
                reservation,
                counters,
                control,
            )?;
            if declared != validated {
                return Err(FsCasErrorV1::Integrity);
            }
            sample_control(control, FsCasBoundaryV1::AfterCandidateValidation)?;

            // Candidate validation only reads an operation-private file. Take
            // the shared-root visibility lock after that work, then recheck
            // validity before observing or changing the common namespace.
            let publication_guard = self.lock_publication_controlled_v1(control)?;
            self.ensure_valid()?;

            let name = hex_id(validated.id().as_bytes());
            let carrier_path = self.inner.root.join("carriers").join(&name);
            let marker_path = self.inner.root.join("catalog").join(&name);
            let transaction = NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed);
            validate_private_directory(&self.inner.root.join("carriers"))?;
            validate_private_directory(&self.inner.root.join("objects"))?;
            validate_private_directory(&self.inner.root.join("catalog"))?;

            let incumbent_marker = open_regular_file_if_present(&marker_path)?;
            let incumbent_carrier = open_regular_file_if_present(&carrier_path)?;
            if incumbent_marker.is_some() || incumbent_carrier.is_some() {
                drop(incumbent_marker);
                drop(incumbent_carrier);
                drop(publication_guard);
                return self.admit_against_incumbent(
                    prepared,
                    metadata,
                    ledger,
                    reservation,
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
            let locator_residue_bound = u64::from(validated.record_count())
                .checked_mul(
                    u64::try_from(PERSISTENT_LOCATOR_BYTES_V1)
                        .map_err(|_| FsCasErrorV1::Integrity)?,
                )
                .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
            let cleanup_residue_bound = validated
                .pack_len()
                .checked_add(locator_residue_bound)
                .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
            let mut cleanup_capacity = *counters;
            cleanup_capacity.record_unreachable_installed_residue(cleanup_residue_bound)?;

            sample_control(control, FsCasBoundaryV1::BeforeCarrierInstall)?;
            sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::CarrierHardLink)?;
            if let Some(token) = storage_token {
                // Reserve the immutable namespace transition before the
                // no-replace link.  Therefore a poisoned/stale accounting
                // owner cannot create a visible but untracked carrier.
                self.record_storage_immutable_install_v1(token, validated.pack_len(), 1)
                    .inspect_err(|_| self.invalidate_root_backstop_v1())?;
            }
            match fs::hard_link(&prepared.path, &carrier_path) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    if let Some(token) = storage_token {
                        self.record_storage_immutable_remove_v1(token, validated.pack_len(), 1)
                            .inspect_err(|_| self.invalidate_root_backstop_v1())?;
                    }
                    drop(publication_guard);
                    return self.admit_against_incumbent(
                        prepared,
                        metadata,
                        ledger,
                        reservation,
                        counters,
                        scratch,
                        validated,
                        &carrier_path,
                        &marker_path,
                        control,
                    );
                }
                Err(error) if is_unsupported_link_error(&error) => {
                    if let Some(token) = storage_token {
                        self.record_storage_immutable_remove_v1(token, validated.pack_len(), 1)
                            .inspect_err(|_| self.invalidate_root_backstop_v1())?;
                    }
                    return Err(FsCasErrorV1::Unsupported);
                }
                Err(error) => {
                    if let Some(token) = storage_token {
                        self.record_storage_immutable_remove_v1(token, validated.pack_len(), 1)
                            .inspect_err(|_| self.invalidate_root_backstop_v1())?;
                    }
                    return Err(map_filesystem_io_error_v1(&error));
                }
            }

            if let Err(error) = sample_control(control, FsCasBoundaryV1::AfterCarrierInstall) {
                self.rollback_unpublished_carrier(
                    &carrier_path,
                    validated,
                    storage_token,
                    counters,
                    control,
                )?;
                return Err(error);
            }

            let installed_validation = (|| {
                let mut installed = FilePackReadV1::open(&carrier_path)?;
                counters.observe_layerfs_open_file_handles(2);
                let observed = validate_pack_for_operation_v1(
                    &mut installed,
                    metadata,
                    scratch,
                    validated.record_count(),
                    ledger,
                    reservation,
                    counters,
                    control,
                )?;
                counters.record_fscas_read(installed.bytes_read, installed.read_calls)?;
                if observed != validated {
                    return Err(FsCasErrorV1::Integrity);
                }
                Ok(())
            })();
            if let Err(error) = installed_validation {
                self.rollback_unpublished_carrier(
                    &carrier_path,
                    validated,
                    storage_token,
                    counters,
                    control,
                )?;
                return Err(error);
            }
            if let Err(error) = sample_control(control, FsCasBoundaryV1::AfterCarrierValidation) {
                self.rollback_unpublished_carrier(
                    &carrier_path,
                    validated,
                    storage_token,
                    counters,
                    control,
                )?;
                return Err(error);
            }

            let immutable =
                sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::PermissionChange)
                    .and_then(|()| set_read_only(&carrier_path));
            if let Err(error) = immutable {
                self.rollback_unpublished_carrier(
                    &carrier_path,
                    validated,
                    storage_token,
                    counters,
                    control,
                )?;
                return Err(error);
            }
            if let Err(error) = sample_control(control, FsCasBoundaryV1::AfterCarrierMadeImmutable)
            {
                self.rollback_unpublished_carrier(
                    &carrier_path,
                    validated,
                    storage_token,
                    counters,
                    control,
                )?;
                return Err(error);
            }

            let alias_cleanup =
                sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::CarrierAliasUnlink)
                    .and_then(|_| {
                        fs::remove_file(&prepared.path)
                            .map_err(|error| map_filesystem_io_error_v1(&error))
                    });
            if alias_cleanup.is_err() {
                self.rollback_unpublished_carrier(
                    &carrier_path,
                    validated,
                    storage_token,
                    counters,
                    control,
                )?;
                // The carrier link was visible, but removing its private
                // preparation alias failed.  This is lifecycle cleanup
                // failure, not an ordinary unpublished-admission error: do
                // not let the outer abort path retry the unlink and silently
                // turn an injected/real failure into success.  Retain the
                // exact private alias as finite preparation residue and make
                // every stale/reopened owner fail closed before returning.
                let error =
                    self.cleanup_failure_controlled_v1(FsCasCleanupTargetV1::PrivatePack, control);
                prepared.state = PrivatePackStateV1::CleanupFailed(error);
                return Err(error);
            }
            prepared
                .record_preparation_removed_v1()
                .inspect_err(|_| self.invalidate_root_backstop_v1())?;
            prepared.state = PrivatePackStateV1::Transferred;

            if let Err(error) = self.install_object_locators(
                &carrier_path,
                validated,
                metadata,
                counters,
                scratch,
                transaction,
                storage_token,
                control,
            ) {
                if matches!(
                    error,
                    FsCasErrorV1::InvalidationFailed
                        | FsCasErrorV1::CleanupFailed(FsCasCleanupTargetV1::PublishedMarkerAlias)
                ) {
                    // These states can follow a visible locator whose alias
                    // cleanup failed. Never roll the carrier or earlier
                    // locators back below that visibility transition.
                    return Err(error);
                }
                return Err(self.rollback_unpublished_admission_preserving_error_v1(
                    &carrier_path,
                    validated,
                    metadata,
                    transaction,
                    storage_token,
                    counters,
                    control,
                    error,
                ));
            }

            if let Err(error) = sample_control(control, FsCasBoundaryV1::BeforeCatalogPublication) {
                self.rollback_unpublished_admission(
                    &carrier_path,
                    validated,
                    metadata,
                    transaction,
                    storage_token,
                    counters,
                    control,
                )?;
                return Err(error);
            }

            // All fallible counter arithmetic precedes catalog visibility. The
            // equivalent capacity checks above ran before carrier visibility;
            // rebuilding here retains intervening, directly observed FsCas
            // reads instead of restoring an early snapshot after publication.
            let mut published_counters = *counters;
            published_counters.record_fscas_catalog_operation()?;
            published_counters.record_pack_storage(validated.pack_len(), 0)?;
            let mut residue_counters = published_counters;
            residue_counters.record_unreachable_installed_residue(validated.pack_len())?;

            let marker = encode_catalog_marker(validated);
            let publication = publish_small_marker_controlled(
                &self.inner.root.join("preparation"),
                "catalog",
                &marker_path,
                &marker,
                Some(self),
                storage_token,
                Some(FsCasBoundaryV1::AfterCatalogMarkerLink),
                control,
            );
            match publication {
                Err(error) => {
                    return Err(self.rollback_unpublished_admission_preserving_error_v1(
                        &carrier_path,
                        validated,
                        metadata,
                        transaction,
                        storage_token,
                        counters,
                        control,
                        error,
                    ));
                }
                Ok(MarkerPublicationV1::VisibleWithPreparationResidue) => {
                    // The catalog is authoritative now. Its carrier and
                    // locators must remain intact even though the alias could
                    // not be released.
                    *counters = residue_counters;
                    return Err(self.cleanup_failure_controlled_v1(
                        FsCasCleanupTargetV1::PublishedMarkerAlias,
                        control,
                    ));
                }
                Ok(MarkerPublicationV1::VisibleClean) => {}
                Ok(MarkerPublicationV1::IncumbentClean(bytes)) => {
                    let authenticated = decode_catalog_marker(bytes)
                        .map_err(|error| match error {
                            FsCasErrorV1::Integrity => FsCasErrorV1::MalformedOccupant,
                            other => other,
                        })
                        .and_then(|incumbent| {
                            (incumbent == validated)
                                .then_some(())
                                .ok_or(FsCasErrorV1::MalformedOccupant)
                        });
                    if let Err(error) = authenticated {
                        return Err(self.rollback_unpublished_admission_preserving_error_v1(
                            &carrier_path,
                            validated,
                            metadata,
                            transaction,
                            storage_token,
                            counters,
                            control,
                            error,
                        ));
                    }
                }
            }
            if let Err(error) = sample_control(control, FsCasBoundaryV1::AfterCatalogPublication) {
                *counters = residue_counters;
                return Err(error);
            }
            *counters = published_counters;
            Ok(FsPackAdmissionV1 {
                outcome: FsPackAdmissionOutcomeV1::Installed,
                sealed: validated,
            })
        })();
        if result.is_err() {
            prepared.abort_private();
            prepared.cleanup_controlled_v1(control)?;
        }
        result
    }

    fn ensure_valid(&self) -> Result<(), FsCasErrorV1> {
        if self.inner.invalidated.load(Ordering::Acquire)
            || root_invalidation_barrier_present_v1(&self.inner.root.join(INVALIDATED_ROOT_NAME))
        {
            Err(FsCasErrorV1::Invalidated)
        } else {
            Ok(())
        }
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
        let injected = control.inject_cleanup_failure(FsCasCleanupTargetV1::RootInvalidation);
        let token_persisted = if injected {
            false
        } else {
            let transition = (|| -> Result<bool, FsCasErrorV1> {
                let mut ownership = self
                    .inner
                    .ownership
                    .lock()
                    .map_err(|_| FsCasErrorV1::Integrity)?;
                let file = ownership.as_mut().ok_or(FsCasErrorV1::Integrity)?;
                file.seek(SeekFrom::Start(8))
                    .map_err(|error| map_filesystem_io_error_v1(&error))?;
                write_all_controlled_v1(
                    file,
                    &[ROOT_OWNER_STATE_INVALIDATED],
                    FsCasFilesystemBoundaryV1::InvalidationWrite,
                    control,
                )?;
                flush_controlled_v1(file, FsCasFilesystemBoundaryV1::InvalidationFlush, control)?;
                file.seek(SeekFrom::Start(0))
                    .map_err(|error| map_filesystem_io_error_v1(&error))?;
                let mut observed = [0_u8; ROOT_OWNER_BYTES];
                file.read_exact(&mut observed)
                    .map_err(|error| map_filesystem_io_error_v1(&error))?;
                Ok(observed
                    == encode_root_owner(self.inner.generation, ROOT_OWNER_STATE_INVALIDATED))
            })();
            transition.unwrap_or(false)
        };
        let marker = self.inner.root.join(INVALIDATED_ROOT_NAME);
        let marker_persisted = match fs::symlink_metadata(&marker) {
            Ok(metadata) => metadata.is_dir() && !metadata.file_type().is_symlink(),
            Err(error) if error.kind() == ErrorKind::NotFound && !injected => {
                sample_filesystem_fault_v1(
                    control,
                    FsCasFilesystemBoundaryV1::InvalidationMarkerCreate,
                )
                .and_then(|()| create_private_directory(&marker))
                .is_ok()
            }
            Err(_) => false,
        };
        if token_persisted || marker_persisted {
            Ok(())
        } else {
            Err(FsCasErrorV1::InvalidationFailed)
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
        self.invalidate_root_controlled_v1(control)
            .err()
            .unwrap_or(FsCasErrorV1::CleanupFailed(target))
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
        control: &mut C,
    ) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        let injected = control.inject_cleanup_failure(FsCasCleanupTargetV1::Carrier);
        let removal = if injected {
            Err(None)
        } else if sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::CarrierUnlink)
            .is_err()
        {
            Err(None)
        } else {
            match fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
                Err(error) => Err(Some(error)),
            }
        };
        if removal.is_err() {
            if counters
                .record_unreachable_installed_residue(sealed.pack_len())
                .is_err()
            {
                return Err(
                    self.cleanup_failure_controlled_v1(FsCasCleanupTargetV1::Carrier, control)
                );
            }
            return Err(self.cleanup_failure_controlled_v1(FsCasCleanupTargetV1::Carrier, control));
        }
        if let Some(token) = storage_token {
            self.record_storage_immutable_remove_v1(token, sealed.pack_len(), 1)
                .inspect_err(|_| self.invalidate_root_backstop_v1())?;
        }
        Ok(())
    }

    fn rollback_unpublished_admission<M, C>(
        &self,
        carrier: &Path,
        sealed: SealedPackV1,
        metadata: &mut M,
        transaction: u64,
        storage_token: Option<FsStorageOperationTokenV1>,
        counters: &mut OperationCountersV1,
        control: &mut C,
    ) -> Result<(), FsCasErrorV1>
    where
        M: PackIndexSpoolV1 + ?Sized,
        C: FsCasControlV1 + ?Sized,
    {
        let objects = self.inner.root.join("objects");
        let locator_residue_bytes = match u64::try_from(PERSISTENT_LOCATOR_BYTES_V1) {
            Ok(bytes) => bytes,
            Err(_) => {
                return Err(self
                    .cleanup_failure_controlled_v1(FsCasCleanupTargetV1::ObjectLocator, control));
            }
        };
        let mut cleanup_work_control = NeverStopWorkControlV1;
        let mut cleanup_failed = metadata
            .sort_by_key_controlled(&mut cleanup_work_control, counters)
            .is_err()
            || metadata.rewind().is_err();
        let mut malformed_occupant = None;
        if !cleanup_failed {
            loop {
                let entry = match metadata.next() {
                    Ok(Some(entry)) => entry,
                    Ok(None) => break,
                    Err(_) => {
                        cleanup_failed = true;
                        break;
                    }
                };
                let path = objects.join(hex_typed_id(entry.id()));
                match read_object_locator_if_present(&path, entry.id()) {
                    Ok(None) => continue,
                    Ok(Some(locator)) if locator.transaction() == transaction => {
                        let injected =
                            control.inject_cleanup_failure(FsCasCleanupTargetV1::ObjectLocator);
                        let removal = if injected {
                            Err(None)
                        } else if sample_filesystem_fault_v1(
                            control,
                            FsCasFilesystemBoundaryV1::LocatorUnlink,
                        )
                        .is_err()
                        {
                            Err(None)
                        } else {
                            match fs::remove_file(&path) {
                                Ok(()) => Ok(()),
                                Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
                                Err(error) => Err(Some(error)),
                            }
                        };
                        if removal.is_err() {
                            if counters
                                .record_unreachable_installed_residue(locator_residue_bytes)
                                .is_err()
                            {
                                return Err(self.cleanup_failure_controlled_v1(
                                    FsCasCleanupTargetV1::ObjectLocator,
                                    control,
                                ));
                            }
                            cleanup_failed = true;
                        } else if let Some(token) = storage_token {
                            if self
                                .record_storage_immutable_remove_v1(token, locator_residue_bytes, 1)
                                .is_err()
                            {
                                self.invalidate_root_backstop_v1();
                                cleanup_failed = true;
                            }
                        }
                    }
                    Ok(Some(_)) => {}
                    Err(error) => {
                        malformed_occupant.get_or_insert(error);
                    }
                }
            }
        }

        self.rollback_unpublished_carrier(carrier, sealed, storage_token, counters, control)?;
        if cleanup_failed {
            Err(self.cleanup_failure_controlled_v1(FsCasCleanupTargetV1::ObjectLocator, control))
        } else if let Some(error) = malformed_occupant {
            self.invalidate_root_controlled_v1(control)?;
            Err(error)
        } else {
            Ok(())
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn rollback_unpublished_admission_preserving_error_v1<M, C>(
        &self,
        carrier: &Path,
        sealed: SealedPackV1,
        metadata: &mut M,
        transaction: u64,
        storage_token: Option<FsStorageOperationTokenV1>,
        counters: &mut OperationCountersV1,
        control: &mut C,
        original: FsCasErrorV1,
    ) -> FsCasErrorV1
    where
        M: PackIndexSpoolV1 + ?Sized,
        C: FsCasControlV1 + ?Sized,
    {
        match self.rollback_unpublished_admission(
            carrier,
            sealed,
            metadata,
            transaction,
            storage_token,
            counters,
            control,
        ) {
            Ok(()) => original,
            Err(cleanup @ FsCasErrorV1::CleanupFailed(_))
            | Err(cleanup @ FsCasErrorV1::InvalidationFailed)
                if !matches!(
                    original,
                    FsCasErrorV1::CleanupFailed(_) | FsCasErrorV1::InvalidationFailed
                ) =>
            {
                cleanup
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
    pub fn occupied(&self) -> Result<impl OccupiedImmutableReadPortV1 + use<>, FsCasErrorV1> {
        self.occupied_private_v1()
    }

    pub(crate) fn occupied_private_v1(&self) -> Result<FsCasOccupiedV1, FsCasErrorV1> {
        let mut control = ContinueFsCasControlV1;
        self.occupied_private_controlled_v1(&mut control)
    }

    pub(crate) fn occupied_private_controlled_v1<C>(
        &self,
        control: &mut C,
    ) -> Result<FsCasOccupiedV1, FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        self.ensure_valid()?;
        let guard = self.lock_visibility_controlled_v1(control)?;
        let validity = self.ensure_valid();
        self.unlock_visibility_controlled_v1(guard, control);
        validity?;
        Ok(FsCasOccupiedV1 {
            cas: self.clone(),
            current: None,
            previous: None,
            bytes_read: 0,
            read_calls: 0,
            first_error: None,
            validation_scratch: [0_u8; COMPARISON_WINDOW_BYTES],
            #[cfg(test)]
            unlocked_payload_read_hook: None,
        })
    }

    /// Authenticate an already-visible complete-closure fence before a
    /// private read begins. The marker is bound to this FsCas generation and
    /// exact typed version-record identifier; a marker copied from another
    /// namespace or retained across invalidation is rejected.
    pub(crate) fn validate_closure_for_read_v1(
        &self,
        version_record: PhysicalVersionRecordIdV1,
    ) -> Result<FsCasAcceptedClosureReadV1, FsCasErrorV1> {
        let mut control = ContinueFsCasControlV1;
        self.validate_closure_for_read_controlled_v1(version_record, &mut control)
    }

    pub(crate) fn validate_closure_for_read_controlled_v1<C>(
        &self,
        version_record: PhysicalVersionRecordIdV1,
        control: &mut C,
    ) -> Result<FsCasAcceptedClosureReadV1, FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        self.ensure_valid()?;
        let guard = self.lock_visibility_controlled_v1(control)?;
        let result = (|| {
            self.ensure_valid()?;
            let typed = TypedPhysicalObjectIdV1::VersionRecord(version_record);
            let path = self.inner.root.join("closures").join(hex_typed_id(typed));
            let bytes = read_exact_regular_file::<CLOSURE_MARKER_BYTES>(&path)?;
            if &bytes[..8] != CLOSURE_MAGIC
                || bytes[8] != typed_kind_byte(typed)
                || bytes[9..16].iter().any(|byte| *byte != 0)
                || bytes[16..48] != version_record.as_bytes()[..]
                || bytes[56..88] != self.inner.generation[..]
            {
                return Err(FsCasErrorV1::Integrity);
            }
            let object_count = u64::from_be_bytes(
                bytes[48..56]
                    .try_into()
                    .map_err(|_| FsCasErrorV1::Integrity)?,
            );
            if object_count == 0 {
                return Err(FsCasErrorV1::Integrity);
            }
            let transcript = bytes[88..120]
                .try_into()
                .map_err(|_| FsCasErrorV1::Integrity)?;
            Ok(FsCasAcceptedClosureReadV1 {
                version_record,
                object_count,
                transcript,
            })
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
        control: &mut C,
    ) -> Result<(), FsCasErrorV1>
    where
        M: PackIndexSpoolV1 + ?Sized,
        C: FsCasControlV1 + ?Sized,
    {
        let objects = self.inner.root.join("objects");
        validate_private_directory(&objects)?;
        {
            let mut work_control = FsCasWorkControlBorrowV1(control);
            metadata
                .sort_by_key_controlled(&mut work_control, counters)
                .map_err(map_pack_spool_error_v1)?;
        }
        metadata.rewind().map_err(|_| FsCasErrorV1::Integrity)?;
        let mut candidate = FilePackReadV1::open(candidate_path)?;

        // Validate every incumbent before creating any locator for this pack.
        while let Some(entry) = metadata.next().map_err(|_| FsCasErrorV1::Integrity)? {
            let path = objects.join(hex_typed_id(entry.id()));
            sample_control(control, FsCasBoundaryV1::BeforeObjectLocatorRead)?;
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

        metadata.rewind().map_err(|_| FsCasErrorV1::Integrity)?;
        while let Some(entry) = metadata.next().map_err(|_| FsCasErrorV1::Integrity)? {
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
                control,
            )?;
            match publication {
                MarkerPublicationV1::VisibleClean => {
                    counters.record_locator_install()?;
                }
                MarkerPublicationV1::VisibleWithPreparationResidue => {
                    // The locator now names this carrier. Invalidate the root
                    // and retain both objects instead of invoking unpublished
                    // rollback beneath a visible locator.
                    return Err(self.cleanup_failure_controlled_v1(
                        FsCasCleanupTargetV1::PublishedMarkerAlias,
                        control,
                    ));
                }
                MarkerPublicationV1::IncumbentClean(bytes) => {
                    let locator = decode_persistent_locator_v1(bytes, entry.id())
                        .map_err(|_| FsCasErrorV1::MalformedOccupant)?;
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
        let catalog = read_catalog_marker(&self.inner.root.join("catalog").join(&pack_name))?;
        if catalog != locator.sealed() {
            return Err(FsCasErrorV1::MalformedOccupant);
        }
        let mut incumbent =
            FilePackReadV1::open_occupant(&self.inner.root.join("carriers").join(&pack_name))?;
        counters.observe_layerfs_open_file_handles(2);
        let indexed = locate_validated_pack_index_entry_v1(
            &mut incumbent,
            locator.sealed(),
            candidate_entry.id(),
            counters,
        )
        .map_err(|_| FsCasErrorV1::MalformedOccupant)?
        .ok_or(FsCasErrorV1::MissingOccupant)?;
        if indexed != locator.entry() {
            return Err(FsCasErrorV1::MalformedOccupant);
        }
        let location =
            validate_validated_pack_object_v1(&mut incumbent, locator.entry(), scratch, counters)
                .map_err(|_| FsCasErrorV1::MalformedOccupant)?;
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
        validate_private_directory(&objects)?;
        {
            let mut work_control = FsCasWorkControlBorrowV1(control);
            metadata
                .sort_by_key_controlled(&mut work_control, counters)
                .map_err(map_pack_spool_error_v1)?;
        }
        metadata.rewind().map_err(|_| FsCasErrorV1::Integrity)?;
        while let Some(entry) = metadata.next().map_err(|_| FsCasErrorV1::Integrity)? {
            let path = objects.join(hex_typed_id(entry.id()));
            sample_control(control, FsCasBoundaryV1::BeforeObjectLocatorRead)?;
            let locator = read_object_locator_if_present(&path, entry.id())?
                .ok_or(FsCasErrorV1::MissingOccupant)?;
            sample_control(control, FsCasBoundaryV1::AfterObjectLocatorRead)?;
            if !locator.matches_binding(sealed, entry) {
                return Err(FsCasErrorV1::MalformedOccupant);
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

    pub fn begin_closure_operation(&self) -> Result<FsClosureOperationV1, FsCasErrorV1> {
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
            admission_started: false,
            admitted: false,
            consumed: false,
        })
    }

    /// Run the complete closure validator and mint an opaque capability only
    /// after its FsCas-backed fence becomes visible. The supplied operation is
    /// one-shot even when validation fails, preventing a hidden retry path.
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
            || operation.admission_started
        {
            return Err(CoreError::SinkRefused);
        }
        operation.admission_started = true;
        let mut occupied = self.occupied().map_err(|_| CoreError::SinkRefused)?;
        let mut control = ContinueFsCasControlV1;
        let mut fence =
            FsClosureFenceV1::new(self.clone(), operation.nonce, None, &mut control, false);
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
        if !Arc::ptr_eq(&operation.owner.inner, &self.inner)
            || operation.owner.ensure_valid().is_err()
            || operation.generation != self.inner.generation
            || operation.admission_started
        {
            return Err(FsClosureAdmissionErrorV1::FsCas(FsCasErrorV1::Integrity));
        }
        operation.admission_started = true;
        let mut occupied = self
            .occupied_private_v1()
            .map_err(FsClosureAdmissionErrorV1::FsCas)?;
        let mut fence = FsClosureFenceV1::new(
            self.clone(),
            operation.nonce,
            Some(storage_token),
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
        self.ensure_valid()?;
        let _guard = self.lock_visibility_v1()?;
        self.ensure_valid()?;
        if operation.generation != self.inner.generation
            || !Arc::ptr_eq(&operation.owner.inner, &self.inner)
            || !Arc::ptr_eq(&capability.owner.inner, &self.inner)
            || operation.owner.ensure_valid().is_err()
            || capability.owner.ensure_valid().is_err()
            || capability.generation != self.inner.generation
            || operation.nonce != capability.operation_nonce
            || !operation.admitted
            || operation.consumed
            || capability.consumed
        {
            return Err(FsCasErrorV1::Integrity);
        }
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
        let incumbent = read_exact_regular_file::<CLOSURE_MARKER_BYTES>(&path)?;
        if incumbent != expected {
            return Err(FsCasErrorV1::Integrity);
        }
        operation.consumed = true;
        capability.consumed = true;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn admit_against_incumbent<M, C>(
        &self,
        prepared: &mut FsPrivatePackV1,
        metadata: &mut M,
        ledger: &ResourceLedgerV1,
        reservation: Option<&OperationReservationV1<'_>>,
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
        let marker = read_catalog_marker(marker_path)?;
        sample_control(control, FsCasBoundaryV1::AfterIncumbentMarkerRead)?;
        if marker != candidate {
            return Err(FsCasErrorV1::MalformedOccupant);
        }
        let mut incumbent = FilePackReadV1::open_occupant(carrier_path)?;
        counters.observe_layerfs_open_file_handles(2);
        let validated = validate_pack_for_operation_v1(
            &mut incumbent,
            metadata,
            scratch,
            marker.record_count(),
            ledger,
            reservation,
            counters,
            control,
        )
        .map_err(|_| FsCasErrorV1::MalformedOccupant)?;
        counters.record_fscas_read(incumbent.bytes_read, incumbent.read_calls)?;
        if validated != marker {
            return Err(FsCasErrorV1::MalformedOccupant);
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
        if read_catalog_marker(marker_path)? != candidate {
            return Err(FsCasErrorV1::MalformedOccupant);
        }
        counters.record_fscas_catalog_operation()?;
        Ok(FsPackAdmissionV1 {
            outcome: FsPackAdmissionOutcomeV1::ExistingComplete,
            sealed: candidate,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_pack_for_operation_v1<P, M, C>(
    pack: &mut P,
    metadata: &mut M,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    maximum_entries: u32,
    ledger: &ResourceLedgerV1,
    reservation: Option<&OperationReservationV1<'_>>,
    counters: &mut OperationCountersV1,
    control: &mut C,
) -> CoreResult<SealedPackV1>
where
    P: PackReadPortV1 + ?Sized,
    M: PackIndexSpoolV1 + ?Sized,
    C: FsCasControlV1 + ?Sized,
{
    #[cfg(feature = "c3-polymorphism")]
    if let Some(reservation) = reservation {
        let mut work_control = FsCasWorkControlBorrowV1(control);
        return crate::pack::validate_pack_borrowed_v1(
            pack,
            metadata,
            scratch,
            maximum_entries,
            reservation,
            counters,
            &mut work_control,
        );
    }
    let _ = reservation;
    validate_pack_v1(pack, metadata, scratch, maximum_entries, ledger, counters)
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
            .map_err(|error| map_filesystem_io_error_v1(&error))?;
        if let Some(token) = self.storage_token {
            self.owner
                .record_storage_preparation_length_v1(token, self.len, len)
                .inspect_err(|_| self.owner.invalidate_root_backstop_v1())?;
        }
        self.len = len;
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
            self.owner
                .record_storage_preparation_length_v1(token, self.len, len)
                .inspect_err(|_| self.owner.invalidate_root_backstop_v1())?;
        }
        let resize = self
            .file
            .as_mut()
            .ok_or(FsCasErrorV1::Integrity)?
            .set_len(len)
            .map_err(|error| map_filesystem_io_error_v1(&error));
        if let Err(error) = resize {
            if let Some(token) = self.storage_token {
                let observed_len = self
                    .file
                    .as_ref()
                    .and_then(|file| file.metadata().ok())
                    .map_or(self.len, |metadata| metadata.len());
                self.owner
                    .record_storage_preparation_length_v1(token, len, observed_len)
                    .inspect_err(|_| self.owner.invalidate_root_backstop_v1())?;
                self.len = observed_len;
            }
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
            self.owner
                .record_storage_preparation_length_v1(token, self.len, next_len)
                .inspect_err(|_| self.owner.invalidate_root_backstop_v1())?;
        }
        let file = self.file.as_mut().ok_or(FsCasErrorV1::Integrity)?;
        let write = file
            .seek(SeekFrom::Start(offset))
            .map_err(|error| map_filesystem_io_error_v1(&error))
            .and_then(|_| {
                write_all_controlled_v1(
                    file,
                    bytes,
                    FsCasFilesystemBoundaryV1::PreparationWrite,
                    control,
                )
            });
        if let Err(error) = write {
            if let Some(token) = self.storage_token {
                let observed_len = file.metadata().map_or(self.len, |metadata| metadata.len());
                self.owner
                    .record_storage_preparation_length_v1(token, next_len, observed_len)
                    .inspect_err(|_| self.owner.invalidate_root_backstop_v1())?;
                self.len = observed_len;
            }
            return Err(error);
        }
        self.len = next_len;
        self.bytes_written = self
            .bytes_written
            .checked_add(amount)
            .ok_or(FsCasErrorV1::Integrity)?;
        self.owner.ensure_valid()
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
        self.bytes_read = self
            .bytes_read
            .checked_add(u64::try_from(destination.len()).map_err(|_| FsCasErrorV1::Integrity)?)
            .ok_or(FsCasErrorV1::Integrity)?;
        self.read_calls = self
            .read_calls
            .checked_add(1)
            .ok_or(FsCasErrorV1::Integrity)?;
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
                let error = self
                    .owner
                    .cleanup_failure_controlled_v1(FsCasCleanupTargetV1::PreparationSpool, control);
                self.cleanup_error = Some(error);
                return Err(error);
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                let error = self
                    .owner
                    .cleanup_failure_controlled_v1(FsCasCleanupTargetV1::PreparationSpool, control);
                self.cleanup_error = Some(error);
                return Err(error);
            }
            Err(_) => {
                let error = self
                    .owner
                    .cleanup_failure_controlled_v1(FsCasCleanupTargetV1::PreparationSpool, control);
                self.cleanup_error = Some(error);
                return Err(error);
            }
        };
        if let Some(token) = self.storage_token {
            self.owner
                .record_storage_preparation_length_v1(token, self.len, observed_len)
                .inspect_err(|_| self.owner.invalidate_root_backstop_v1())?;
            self.len = observed_len;
        }
        let injected = control.inject_cleanup_failure(FsCasCleanupTargetV1::PreparationSpool);
        if injected || fs::remove_file(&self.path).is_err() {
            let error = self
                .owner
                .cleanup_failure_controlled_v1(FsCasCleanupTargetV1::PreparationSpool, control);
            self.cleanup_error = Some(error);
            return Err(error);
        }
        if let Some(token) = self.storage_token {
            self.owner
                .record_storage_preparation_remove_v1(token, self.len)
                .inspect_err(|_| self.owner.invalidate_root_backstop_v1())?;
        }
        self.cleanup_complete = true;
        Ok(())
    }
}

#[cfg(feature = "c3-polymorphism")]
impl Drop for FsOperationSpoolV1 {
    fn drop(&mut self) {
        if self.cleanup_complete || self.cleanup_error.is_some() {
            return;
        }
        drop(self.file.take());
        let observed_len = fs::symlink_metadata(&self.path)
            .ok()
            .filter(|metadata| metadata.is_file())
            .map_or(self.len, |metadata| metadata.len());
        if let Some(token) = self.storage_token {
            if self
                .owner
                .record_storage_preparation_length_v1(token, self.len, observed_len)
                .is_err()
            {
                self.owner.invalidate_root_backstop_v1();
            }
        }
        match fs::remove_file(&self.path) {
            Ok(()) => {
                if let Some(token) = self.storage_token {
                    if self
                        .owner
                        .record_storage_preparation_remove_v1(token, observed_len)
                        .is_err()
                    {
                        self.owner.invalidate_root_backstop_v1();
                    }
                }
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
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
    #[cfg(feature = "c3-polymorphism")]
    pub(crate) fn take_first_error_typed_v1(&mut self) -> Option<FsCasErrorV1> {
        self.first_error.take()
    }

    fn sealed(&self) -> Option<SealedPackV1> {
        self.owner.ensure_valid().ok()?;
        match self.state {
            PrivatePackStateV1::Sealed { sealed, .. } => Some(sealed),
            _ => None,
        }
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
                let error = self
                    .owner
                    .cleanup_failure_controlled_v1(FsCasCleanupTargetV1::PrivatePack, control);
                self.state = PrivatePackStateV1::CleanupFailed(error);
                return Err(error);
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                if self.preparation_accounted {
                    let failure = self
                        .owner
                        .cleanup_failure_controlled_v1(FsCasCleanupTargetV1::PrivatePack, control);
                    self.state = PrivatePackStateV1::CleanupFailed(failure);
                    return Err(failure);
                }
                self.state = PrivatePackStateV1::CleanupComplete;
                return Ok(());
            }
            Err(_) => {
                let error = self
                    .owner
                    .cleanup_failure_controlled_v1(FsCasCleanupTargetV1::PrivatePack, control);
                self.state = PrivatePackStateV1::CleanupFailed(error);
                return Err(error);
            }
        };
        self.reconcile_preparation_length_v1(observed_len)
            .inspect_err(|_| self.owner.invalidate_root_backstop_v1())?;
        let injected = control.inject_cleanup_failure(FsCasCleanupTargetV1::PrivatePack);
        if injected || fs::remove_file(&self.path).is_err() {
            let error = self
                .owner
                .cleanup_failure_controlled_v1(FsCasCleanupTargetV1::PrivatePack, control);
            self.state = PrivatePackStateV1::CleanupFailed(error);
            return Err(error);
        }
        self.record_preparation_removed_v1()
            .inspect_err(|_| self.owner.invalidate_root_backstop_v1())?;
        self.state = PrivatePackStateV1::CleanupComplete;
        Ok(())
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
            .map_err(|error| map_filesystem_io_error_v1(&error))
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
                        .map_err(|error| map_filesystem_io_error_v1(&error))
                })
        {
            return Err(self.retain_error_v1(error));
        }
        *written = len;
        if let Err(error) = self.reconcile_preparation_length_v1(len) {
            self.owner.invalidate_root_backstop_v1();
            return Err(self.retain_error_v1(error));
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
        if let Err(error) =
            sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::PrivatePackCreate)
        {
            return Err(self.retain_error_v1(error));
        }
        if let Some(token) = self.storage_token {
            if let Err(error) = self.owner.record_storage_preparation_create_v1(token) {
                self.owner.invalidate_root_backstop_v1();
                return Err(self.retain_error_v1(error));
            }
            self.preparation_accounted = true;
            self.accounted_len = 0;
        }
        let file = match OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&self.path)
        {
            Ok(file) => file,
            Err(error) => {
                let error = map_filesystem_io_error_v1(&error);
                if self.preparation_accounted {
                    if let Err(accounting_error) = self.record_preparation_removed_v1() {
                        self.owner.invalidate_root_backstop_v1();
                        return Err(self.retain_error_v1(accounting_error));
                    }
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
            self.owner.invalidate_root_backstop_v1();
            return Err(self.retain_error_v1(error));
        }
        let PrivatePackStateV1::Writing {
            file,
            written: state_written,
            ..
        } = &mut self.state
        else {
            self.owner.invalidate_root_backstop_v1();
            return Err(self.retain_error_v1(FsCasErrorV1::Integrity));
        };
        let write = file
            .seek(SeekFrom::Start(written))
            .map_err(|error| map_filesystem_io_error_v1(&error))
            .and_then(|_| {
                write_all_controlled_v1(
                    file,
                    bytes,
                    FsCasFilesystemBoundaryV1::PrivatePackWrite,
                    control,
                )
            });
        if let Err(error) = write {
            let observed_len = file.metadata().map_or(written, |metadata| metadata.len());
            if let Some(token) = self.storage_token.filter(|_| self.preparation_accounted) {
                if self
                    .owner
                    .record_storage_preparation_length_v1(token, next, observed_len)
                    .is_err()
                {
                    self.owner.invalidate_root_backstop_v1();
                } else {
                    self.accounted_len = observed_len;
                }
            }
            return Err(self.retain_error_v1(error));
        }
        *state_written = next;
        self.ensure_owner_valid_v1()
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
                let error = map_filesystem_io_error_v1(&error);
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
                if file.flush().is_err() {
                    return Err(self.retain_error_v1(FsCasErrorV1::Io));
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
        let observed_len = fs::symlink_metadata(&self.path)
            .ok()
            .filter(|metadata| metadata.is_file())
            .map_or(self.accounted_len, |metadata| metadata.len());
        if self.reconcile_preparation_length_v1(observed_len).is_err() {
            self.owner.invalidate_root_backstop_v1();
        }
        match fs::remove_file(&self.path) {
            Ok(()) => {
                if self.record_preparation_removed_v1().is_err() {
                    self.owner.invalidate_root_backstop_v1();
                }
            }
            Err(error) if error.kind() == ErrorKind::NotFound && !self.preparation_accounted => {}
            Err(_) => self.owner.invalidate_root_backstop_v1(),
        }
    }
}

struct FilePackReadV1 {
    file: File,
    len: u64,
    bytes_read: u64,
    read_calls: u64,
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
        Self::from_file(file).map_err(|error| match error {
            FsCasErrorV1::Integrity => FsCasErrorV1::MalformedOccupant,
            other => other,
        })
    }

    fn from_file(file: File) -> Result<Self, FsCasErrorV1> {
        let metadata = file.metadata().map_err(|_| FsCasErrorV1::Io)?;
        if !metadata.file_type().is_file() {
            return Err(FsCasErrorV1::Integrity);
        }
        Ok(Self {
            file,
            len: metadata.len(),
            bytes_read: 0,
            read_calls: 0,
        })
    }
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
        checked_file_read(&mut self.file, self.len, offset, destination)
            .map_err(|_| PackPortErrorV1::Failure)?;
        self.bytes_read = self
            .bytes_read
            .checked_add(destination.len() as u64)
            .ok_or(PackPortErrorV1::Failure)?;
        self.read_calls = self
            .read_calls
            .checked_add(1)
            .ok_or(PackPortErrorV1::Failure)?;
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
                validate_private_directory(&objects)?;
                let path = objects.join(hex_typed_id(id));
                let Some(locator) = read_object_locator_if_present(&path, id)? else {
                    self.current = None;
                    self.previous = None;
                    return Ok(Err(None));
                };
                let pack_name = hex_id(locator.sealed().id().as_bytes());
                let catalog =
                    read_catalog_marker(&self.cas.inner.root.join("catalog").join(&pack_name))?;
                if catalog != locator.sealed() {
                    return Err(FsCasErrorV1::MalformedOccupant);
                }
                let carrier = self.cas.inner.root.join("carriers").join(&pack_name);
                let pack = FilePackReadV1::open_occupant(&carrier)?;
                Ok(Ok((locator, pack)))
            })();
            self.cas.unlock_visibility_controlled_v1(guard, control);
            match snapshot? {
                Ok(snapshot) => snapshot,
                Err(cached) => return Ok(cached),
            }
        };
        self.bytes_read = self
            .bytes_read
            .checked_add(PERSISTENT_LOCATOR_BYTES_V1 as u64)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        self.read_calls = self
            .read_calls
            .checked_add(1)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        self.bytes_read = self
            .bytes_read
            .checked_add(CATALOG_MARKER_BYTES as u64)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        self.read_calls = self
            .read_calls
            .checked_add(1)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        let mut local_counters = OperationCountersV1::default();
        let indexed = locate_validated_pack_index_entry_v1(
            &mut pack,
            locator.sealed(),
            id,
            &mut local_counters,
        )
        .map_err(|_| FsCasErrorV1::MalformedOccupant)?
        .ok_or(FsCasErrorV1::MissingOccupant)?;
        if indexed != locator.entry() {
            return Err(FsCasErrorV1::MalformedOccupant);
        }
        let location = validate_validated_pack_object_v1(
            &mut pack,
            indexed,
            &mut self.validation_scratch,
            &mut local_counters,
        )
        .map_err(|_| FsCasErrorV1::MalformedOccupant)?;
        self.bytes_read = self
            .bytes_read
            .checked_add(pack.bytes_read)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        self.read_calls = self
            .read_calls
            .checked_add(pack.read_calls)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;

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
        if let Some(hook) = &self.unlocked_payload_read_hook {
            hook();
        }
        checked_file_read(&mut resolved.file, resolved.pack_len, absolute, destination)?;
        self.bytes_read = self
            .bytes_read
            .checked_add(amount)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
        self.read_calls = self
            .read_calls
            .checked_add(1)
            .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;

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

    fn direct_storage_read_observation(&self) -> Result<(u64, u64), ImmutablePortErrorV1> {
        self.direct_storage_read_observation_typed_v1()
            .map_err(|_| ImmutablePortErrorV1::Failure)
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

struct FsClosureFenceV1<'control, C>
where
    C: FsCasControlV1 + ?Sized,
{
    cas: FsCasV1,
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

impl<'control, C> FsClosureFenceV1<'control, C>
where
    C: FsCasControlV1 + ?Sized,
{
    fn new(
        cas: FsCasV1,
        operation_nonce: u64,
        storage_token: Option<FsStorageOperationTokenV1>,
        control: &'control mut C,
        inject_marker_alias_cleanup: bool,
    ) -> Self {
        Self {
            cas,
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

impl<C> PreparedImmutableClosurePortV1 for FsClosureFenceV1<'_, C>
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
        self.cas
            .ensure_valid()
            .map_err(|_| ImmutablePortErrorV1::Failure)?;
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
        let _publication_guard = self
            .cas
            .lock_publication_controlled_v1(self.control)
            .map_err(|error| {
                self.first_error.get_or_insert(error);
                ImmutablePortErrorV1::Failure
            })?;
        self.cas
            .ensure_valid()
            .map_err(|_| ImmutablePortErrorV1::Failure)?;
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
            self.cas
                .ensure_valid()
                .map_err(|_| ImmutablePortErrorV1::Failure)?;
            read_exact_regular_file_if_present::<CLOSURE_MARKER_BYTES>(&destination).map_err(
                |error| {
                    self.first_error.get_or_insert(error);
                    ImmutablePortErrorV1::Failure
                },
            )?
        };
        if let Some(incumbent) = incumbent {
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
                self.control,
            );
            match publication {
                Err(error) => {
                    self.first_error.get_or_insert(error);
                    return Err(ImmutablePortErrorV1::Failure);
                }
                Ok(MarkerPublicationV1::VisibleWithPreparationResidue) => {
                    // The complete-closure marker is already visible. Keep
                    // the admitted carrier/catalog/locator closure intact,
                    // invalidate the root, and refuse to mint a usable
                    // handoff capability.
                    let error = self.cas.cleanup_failure_controlled_v1(
                        FsCasCleanupTargetV1::PublishedMarkerAlias,
                        self.control,
                    );
                    self.first_error.get_or_insert(error);
                    return Err(ImmutablePortErrorV1::Failure);
                }
                Ok(MarkerPublicationV1::VisibleClean) => {}
                Ok(MarkerPublicationV1::IncumbentClean(bytes)) => {
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
        let take = usize::try_from((len - offset).min(half as u64))
            .map_err(|_| FsCasErrorV1::Integrity)?;
        candidate
            .read_exact_at(offset, &mut left[..take])
            .map_err(|_| FsCasErrorV1::Io)?;
        incumbent
            .read_exact_at(offset, &mut right[..take])
            .map_err(|_| FsCasErrorV1::MalformedOccupant)?;
        if left[..take] != right[..take] {
            return Err(FsCasErrorV1::UnequalOccupant);
        }
        offset = offset
            .checked_add(u64::try_from(take).map_err(|_| FsCasErrorV1::Integrity)?)
            .ok_or(FsCasErrorV1::Integrity)?;
        counters.record_incumbent_comparison(
            u64::try_from(take).map_err(|_| FsCasErrorV1::Integrity)?,
            1,
        )?;
        counters.record_fscas_read(u64::try_from(take).map_err(|_| FsCasErrorV1::Integrity)?, 1)?;
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
        candidate
            .read_exact_at(candidate_offset, &mut left[..take])
            .map_err(|_| FsCasErrorV1::Integrity)?;
        incumbent
            .read_exact_at(incumbent_offset, &mut right[..take])
            .map_err(|_| FsCasErrorV1::MalformedOccupant)?;
        if left[..take] != right[..take] {
            return Err(FsCasErrorV1::UnequalOccupant);
        }
        let amount = u64::try_from(take).map_err(|_| FsCasErrorV1::Integrity)?;
        offset = offset.checked_add(amount).ok_or(FsCasErrorV1::Integrity)?;
        counters.record_incumbent_comparison(amount, 1)?;
        counters.record_fscas_read(amount.checked_mul(2).ok_or(FsCasErrorV1::Integrity)?, 2)?;
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

fn map_filesystem_io_error_v1(error: &std::io::Error) -> FsCasErrorV1 {
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
        _ => FsCasErrorV1::Io,
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
                .map_err(|actual| map_filesystem_io_error_v1(&actual))?;
        }
        return Err(error);
    }
    file.write_all(bytes)
        .map_err(|error| map_filesystem_io_error_v1(&error))
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
        .map_err(|error| map_filesystem_io_error_v1(&error))
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
        .map_err(|_| FsCasErrorV1::MalformedOccupant)
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
    let bytes = read_exact_regular_file::<ROOT_OWNER_BYTES>(path)?;
    if bytes[..8] != *ROOT_OWNER_MAGIC || bytes[9..12] != [0_u8; 3] || bytes[16..] != generation {
        return Err(FsCasErrorV1::Integrity);
    }
    match bytes[8] {
        ROOT_OWNER_STATE_ACTIVE => Ok(FsCasErrorV1::Busy),
        ROOT_OWNER_STATE_INVALIDATED => Ok(FsCasErrorV1::Invalidated),
        _ => Err(FsCasErrorV1::Integrity),
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
        Err(_) => return Err(FsCasErrorV1::Io),
    };
    let initialize = (|| {
        set_private_file_permissions(&path)?;
        let bytes = encode_root_owner(generation, ROOT_OWNER_STATE_ACTIVE);
        file.write_all(&bytes).map_err(|_| FsCasErrorV1::Io)?;
        file.flush().map_err(|_| FsCasErrorV1::Io)?;
        file.seek(SeekFrom::Start(0))
            .map_err(|_| FsCasErrorV1::Io)?;
        let mut observed = [0_u8; ROOT_OWNER_BYTES];
        file.read_exact(&mut observed)
            .map_err(|_| FsCasErrorV1::Io)?;
        if observed != bytes {
            return Err(FsCasErrorV1::Integrity);
        }
        Ok(())
    })();
    if let Err(error) = initialize {
        drop(file);
        // If cleanup fails, the malformed/partial token itself remains a
        // permanent fail-closed barrier for later openers.
        let _ = fs::remove_file(&path);
        return Err(error);
    }
    Ok(file)
}

fn derive_generation(root: &Path) -> Result<[u8; 32], FsCasErrorV1> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| FsCasErrorV1::Io)?;
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
    hasher.update(&elapsed.as_nanos().to_be_bytes());
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
    let bytes = read_exact_regular_file::<GENERATION_MARKER_BYTES>(path)?;
    if bytes[..8] != *GENERATION_MAGIC {
        return Err(FsCasErrorV1::Integrity);
    }
    <[u8; 32]>::try_from(&bytes[8..]).map_err(|_| FsCasErrorV1::Integrity)
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MarkerPublicationV1<const N: usize> {
    VisibleClean,
    VisibleWithPreparationResidue,
    /// The atomic no-replace link reported `AlreadyExists`. These are the
    /// exact incumbent bytes read while the root visibility lock was still
    /// held; the semantic owner must authenticate them before continuing.
    IncumbentClean([u8; N]),
}

impl<const N: usize> MarkerPublicationV1<N> {
    fn require_clean(self) -> Result<(), FsCasErrorV1> {
        match self {
            Self::VisibleClean => Ok(()),
            Self::VisibleWithPreparationResidue | Self::IncumbentClean(_) => {
                Err(FsCasErrorV1::Integrity)
            }
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
        &mut control,
    )
}

fn publish_small_marker_controlled<const N: usize, C>(
    preparation: &Path,
    prefix: &str,
    destination: &Path,
    bytes: &[u8; N],
    visibility_owner: Option<&FsCasV1>,
    storage_token: Option<FsStorageOperationTokenV1>,
    linked_boundary: Option<FsCasBoundaryV1>,
    control: &mut C,
) -> Result<MarkerPublicationV1<N>, FsCasErrorV1>
where
    C: FsCasControlV1 + ?Sized,
{
    validate_private_directory(preparation)?;
    let temporary = unique_private_path(preparation, prefix)?;
    sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::MarkerCreate)?;
    if let (Some(owner), Some(token)) = (visibility_owner, storage_token) {
        owner
            .record_storage_preparation_create_v1(token)
            .inspect_err(|_| owner.invalidate_root_backstop_v1())?;
    }
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| map_filesystem_io_error_v1(&error));
    let mut accounted_len = 0_u64;
    let mut file = match file {
        Ok(file) => file,
        Err(error) => {
            if let (Some(owner), Some(token)) = (visibility_owner, storage_token) {
                owner
                    .record_storage_preparation_remove_v1(token, 0)
                    .inspect_err(|_| owner.invalidate_root_backstop_v1())?;
            }
            return Err(error);
        }
    };
    let prepare = (|| {
        sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::PermissionChange)?;
        set_private_file_permissions(&temporary)?;
        if let (Some(owner), Some(token)) = (visibility_owner, storage_token) {
            let next_len = u64::try_from(N).map_err(|_| FsCasErrorV1::Integrity)?;
            owner
                .record_storage_preparation_length_v1(token, accounted_len, next_len)
                .inspect_err(|_| owner.invalidate_root_backstop_v1())?;
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
    if let Err(error) = prepare {
        return Err(cleanup_unpublished_marker_after_error_v1(
            &temporary,
            visibility_owner,
            storage_token,
            accounted_len,
            error,
            control,
        ));
    }

    let visibility_guard = match visibility_owner
        .map(|owner| owner.lock_visibility_controlled_v1(control))
        .transpose()
    {
        Ok(guard) => guard,
        Err(error) => {
            return Err(cleanup_unpublished_marker_after_error_v1(
                &temporary,
                visibility_owner,
                storage_token,
                accounted_len,
                error,
                control,
            ));
        }
    };
    if let Some(owner) = visibility_owner {
        if let Err(error) = owner.ensure_valid() {
            drop(visibility_guard);
            return Err(cleanup_unpublished_marker_after_error_v1(
                &temporary,
                visibility_owner,
                storage_token,
                accounted_len,
                error,
                control,
            ));
        }
    }
    if let Err(error) =
        sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::MarkerHardLink)
    {
        drop(visibility_guard);
        return Err(cleanup_unpublished_marker_after_error_v1(
            &temporary,
            visibility_owner,
            storage_token,
            accounted_len,
            error,
            control,
        ));
    }
    let immutable_len = match (visibility_owner, storage_token) {
        (Some(owner), Some(token)) => {
            let len = u64::try_from(N).map_err(|_| FsCasErrorV1::Integrity)?;
            owner
                .record_storage_immutable_install_v1(token, len, 1)
                .inspect_err(|_| owner.invalidate_root_backstop_v1())?;
            Some((owner, token, len))
        }
        _ => None,
    };
    match fs::hard_link(&temporary, destination) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            if let Some((owner, token, len)) = immutable_len {
                owner
                    .record_storage_immutable_remove_v1(token, len, 1)
                    .inspect_err(|_| owner.invalidate_root_backstop_v1())?;
            }
            let incumbent = match read_exact_regular_file_if_present::<N>(destination) {
                Ok(Some(bytes)) => Ok(bytes),
                Ok(None) => Err(FsCasErrorV1::MissingOccupant),
                Err(FsCasErrorV1::Integrity) => Err(FsCasErrorV1::MalformedOccupant),
                Err(other) => Err(other),
            };
            drop(visibility_guard);
            cleanup_unpublished_marker_v1(
                &temporary,
                visibility_owner,
                storage_token,
                accounted_len,
                FsCasCleanupTargetV1::PreparationSpool,
                control,
            )?;
            return incumbent.map(MarkerPublicationV1::IncumbentClean);
        }
        Err(error) => {
            if let Some((owner, token, len)) = immutable_len {
                owner
                    .record_storage_immutable_remove_v1(token, len, 1)
                    .inspect_err(|_| owner.invalidate_root_backstop_v1())?;
            }
            drop(visibility_guard);
            let publication_error = if is_unsupported_link_error(&error) {
                FsCasErrorV1::Unsupported
            } else {
                map_filesystem_io_error_v1(&error)
            };
            return Err(cleanup_unpublished_marker_after_error_v1(
                &temporary,
                visibility_owner,
                storage_token,
                accounted_len,
                publication_error,
                control,
            ));
        }
    }
    drop(visibility_guard);

    // From this point onward `destination` is the visibility authority. A
    // failure to remove its private hard-link alias is cleanup residue, not a
    // failed publication, and callers must retain every dependency beneath
    // the visible marker.
    if let Some(boundary) = linked_boundary {
        control.boundary_reached(boundary);
    }
    let injected = linked_boundary.is_some()
        && control.inject_cleanup_failure(FsCasCleanupTargetV1::PublishedMarkerAlias);
    let unlink_failed = if injected {
        true
    } else {
        sample_filesystem_fault_v1(control, FsCasFilesystemBoundaryV1::MarkerAliasUnlink)
            .and_then(|()| {
                fs::remove_file(&temporary).map_err(|error| map_filesystem_io_error_v1(&error))
            })
            .is_err()
    };
    if injected || unlink_failed {
        Ok(MarkerPublicationV1::VisibleWithPreparationResidue)
    } else {
        if let (Some(owner), Some(token)) = (visibility_owner, storage_token) {
            if owner
                .record_storage_preparation_remove_v1(token, accounted_len)
                .is_err()
            {
                owner.invalidate_root_backstop_v1();
                return Ok(MarkerPublicationV1::VisibleWithPreparationResidue);
            }
        }
        Ok(MarkerPublicationV1::VisibleClean)
    }
}

fn cleanup_unpublished_marker_after_error_v1<C>(
    temporary: &Path,
    owner: Option<&FsCasV1>,
    storage_token: Option<FsStorageOperationTokenV1>,
    accounted_len: u64,
    original: FsCasErrorV1,
    control: &mut C,
) -> FsCasErrorV1
where
    C: FsCasControlV1 + ?Sized,
{
    cleanup_unpublished_marker_v1(
        temporary,
        owner,
        storage_token,
        accounted_len,
        FsCasCleanupTargetV1::PreparationSpool,
        control,
    )
    .err()
    .unwrap_or(original)
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
                if observed_len != accounted_len
                    && owner
                        .record_storage_preparation_length_v1(token, accounted_len, observed_len)
                        .is_err()
                {
                    owner.invalidate_root_backstop_v1();
                    return Err(FsCasErrorV1::Integrity);
                }
            }
            Ok(_) | Err(_) => {
                owner.invalidate_root_backstop_v1();
                return Err(owner.cleanup_failure_controlled_v1(target, control));
            }
        }
    }
    let injected = control.inject_cleanup_failure(target);
    let removed = if injected {
        false
    } else {
        match fs::remove_file(temporary) {
            Ok(()) => true,
            Err(error) if error.kind() == ErrorKind::NotFound => true,
            Err(_) => false,
        }
    };
    if removed {
        if let (Some(owner), Some(token)) = (owner, storage_token) {
            owner
                .record_storage_preparation_remove_v1(token, observed_len)
                .inspect_err(|_| owner.invalidate_root_backstop_v1())?;
        }
        Ok(())
    } else if let Some(owner) = owner {
        Err(owner.cleanup_failure_controlled_v1(target, control))
    } else {
        Err(FsCasErrorV1::Io)
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
    let metadata = file.metadata().map_err(|_| FsCasErrorV1::Io)?;
    if metadata.len() != N as u64 {
        return Err(FsCasErrorV1::Integrity);
    }
    let mut bytes = [0_u8; N];
    file.read_exact(&mut bytes)
        .map_err(|_| FsCasErrorV1::Integrity)?;
    let mut trailing = [0_u8; 1];
    if file.read(&mut trailing).map_err(|_| FsCasErrorV1::Io)? != 0 {
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
    if end > len || file.metadata().map_err(|_| FsCasErrorV1::Io)?.len() != len {
        return Err(FsCasErrorV1::Integrity);
    }
    file.seek(SeekFrom::Start(offset))
        .and_then(|_| file.read_exact(destination))
        .map_err(|_| FsCasErrorV1::Integrity)
}

fn open_regular_file(path: &Path) -> Result<File, FsCasErrorV1> {
    open_regular_file_if_present(path)?.ok_or(FsCasErrorV1::Io)
}

/// Open and authenticate one regular-file name, classifying only an actual
/// `NotFound` from the open itself as vacancy. Permission, metadata, and other
/// I/O failures stay errors; they are never converted to a missing occupant.
fn open_regular_file_if_present(path: &Path) -> Result<Option<File>, FsCasErrorV1> {
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
                Err(_) => Err(FsCasErrorV1::Io),
            };
        }
        Err(_) => return Err(FsCasErrorV1::Io),
    };
    let before = fs::symlink_metadata(path).map_err(|_| FsCasErrorV1::Integrity)?;
    if !before.file_type().is_file() || before.file_type().is_symlink() {
        return Err(FsCasErrorV1::Integrity);
    }
    let after = file.metadata().map_err(|_| FsCasErrorV1::Io)?;
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
            Err(FsCasErrorV1::ResourceExhausted(FsCasResourceV1::Queue))
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
/// any error other than a definite `NotFound` must prevent use of the root.
fn root_invalidation_barrier_present_v1(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == ErrorKind::NotFound => false,
        Ok(_) | Err(_) => true,
    }
}

fn validate_new_root(root: &Path) -> Result<(), FsCasErrorV1> {
    if !root.is_absolute() || root.file_name().is_none() {
        return Err(FsCasErrorV1::Unsupported);
    }
    match fs::symlink_metadata(root) {
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Ok(_) => return Err(FsCasErrorV1::Unsupported),
        Err(_) => return Err(FsCasErrorV1::Io),
    }
    let parent = root.parent().ok_or(FsCasErrorV1::Unsupported)?;
    validate_private_directory(parent)?;
    let canonical = fs::canonicalize(parent).map_err(|_| FsCasErrorV1::Unsupported)?;
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
    if fs::canonicalize(root).map_err(|_| FsCasErrorV1::Unsupported)? != root {
        return Err(FsCasErrorV1::Unsupported);
    }
    Ok(())
}

fn validate_private_directory(path: &Path) -> Result<(), FsCasErrorV1> {
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return Err(FsCasErrorV1::Unsupported);
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| FsCasErrorV1::Io)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(FsCasErrorV1::Unsupported);
    }
    Ok(())
}

fn create_private_directory(path: &Path) -> Result<(), FsCasErrorV1> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700);
        builder.create(path).map_err(|_| FsCasErrorV1::Io)?;
    }
    #[cfg(not(unix))]
    {
        fs::create_dir(path).map_err(|_| FsCasErrorV1::Unsupported)?;
    }
    validate_private_directory(path)
}

fn set_private_file_permissions(path: &Path) -> Result<(), FsCasErrorV1> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|_| FsCasErrorV1::Io)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

fn set_read_only(path: &Path) -> Result<(), FsCasErrorV1> {
    let mut permissions = fs::metadata(path)
        .map_err(|_| FsCasErrorV1::Io)?
        .permissions();
    permissions.set_readonly(true);
    fs::set_permissions(path, permissions).map_err(|_| FsCasErrorV1::Io)
}

fn unique_private_path(directory: &Path, prefix: &str) -> Result<PathBuf, FsCasErrorV1> {
    for _ in 0..128 {
        let sequence = NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed);
        let name = format!("{prefix}-{}-{sequence:016x}", std::process::id());
        let path = directory.join(name);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(path),
            Ok(_) => {}
            Err(_) => return Err(FsCasErrorV1::Io),
        }
    }
    Err(FsCasErrorV1::Io)
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
    let left = fs::metadata(left).map_err(|_| FsCasErrorV1::Io)?;
    let right = fs::metadata(right).map_err(|_| FsCasErrorV1::Io)?;
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
            Err(FsCasErrorV1::ResourceExhausted(FsCasResourceV1::Queue))
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
            Err(FsCasErrorV1::Core(CoreError::Cancelled))
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
        assert_eq!(cas.operation_admitted_slots_v1(), 1);

        let poison = cas.clone();
        let unwind = std::thread::spawn(move || {
            let _guard = poison.inner.operation_admission.state.lock().unwrap();
            panic!("inject operation-admission release poison");
        })
        .join();
        assert!(unwind.is_err());

        assert_eq!(
            capability.finish_operation_admission_v1(&mut counters, &mut control),
            Err(FsCasErrorV1::Integrity)
        );
        assert_eq!(counters.root_admission_release_failures, 1);
        assert_eq!(cas.operation_admitted_slots_v1(), 0);
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
            Err(FsCasErrorV1::Invalidated)
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
            Err(FsCasErrorV1::Io)
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
