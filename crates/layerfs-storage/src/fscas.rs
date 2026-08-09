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
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::cas::{
    admit_complete_immutable_v1, AdmissionBuffersV1, AdmittedClosureV1,
    CompleteImmutableClosureReadPortV1, ImmutablePortErrorV1, OccupiedImmutableReadPortV1,
    PreparedImmutableClosurePortV1, ValidatedOccupiedObjectV1,
};
use crate::identity::{ObjectChecksumV1, PackIdV1, COMPARISON_WINDOW_BYTES};
use crate::limits::{
    OperationCountersV1, OperationReservationV1, ResourceLedgerV1, BASE_LEDGER_BYTES,
};
use crate::object::TypedPhysicalObjectIdV1;
use crate::pack::{
    locate_validated_pack_index_entry_v1, validate_pack_v1, validate_validated_pack_object_v1,
    PackIndexEntryV1, PackIndexSpoolV1, PackObjectLocationV1, PackPortErrorV1, PackReadPortV1,
    PrivatePackPortV1, SealedPackV1, MAX_PACK_BYTES,
};
use crate::{CoreError, CoreResult};

const CATALOG_MAGIC: &[u8; 8] = b"LFSCAT01";
const CATALOG_MARKER_BYTES: usize = 64;
const OBJECT_LOCATOR_MAGIC: &[u8; 8] = b"LFSOBJ01";
const OBJECT_LOCATOR_BYTES: usize = 160;
const GENERATION_MAGIC: &[u8; 8] = b"LFSGEN01";
const GENERATION_MARKER_BYTES: usize = 40;
const CLOSURE_MAGIC: &[u8; 8] = b"LFSCLO01";
const CLOSURE_MARKER_BYTES: usize = 120;
const INVALIDATED_ROOT_NAME: &str = "invalidated";

static NEXT_PRIVATE_NAME: AtomicU64 = AtomicU64::new(1);
static NEXT_CLOSURE_OPERATION: AtomicU64 = AtomicU64::new(1);
static OPEN_ROOTS: OnceLock<Mutex<HashMap<PathBuf, Weak<FsCasInnerV1>>>> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FsCasErrorV1 {
    Unsupported,
    Invalidated,
    Io,
    Integrity,
    Collision,
    Core(CoreError),
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FsCasCleanupTargetV1 {
    ObjectLocator,
    Carrier,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FsCasBoundaryV1 {
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
    AfterObjectLocatorPublication,
    BeforeCatalogPublication,
    AfterCatalogPublication,
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
    serial: Mutex<()>,
}

fn shared_root_owner(root: &Path, generation: [u8; 32]) -> Result<Arc<FsCasInnerV1>, FsCasErrorV1> {
    let registry = OPEN_ROOTS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut roots = registry.lock().map_err(|_| FsCasErrorV1::Io)?;
    roots.retain(|_, owner| owner.strong_count() != 0);
    if let Some(owner) = roots.get(root).and_then(Weak::upgrade) {
        if owner.generation != generation {
            owner.invalidated.store(true, Ordering::Release);
            return Err(FsCasErrorV1::Integrity);
        }
        return Ok(owner);
    }
    let owner = Arc::new(FsCasInnerV1 {
        root: root.to_path_buf(),
        generation,
        invalidated: AtomicBool::new(false),
        serial: Mutex::new(()),
    });
    roots.insert(root.to_path_buf(), Arc::downgrade(&owner));
    Ok(owner)
}

impl FsCasV1 {
    /// Create a new engine-private namespace. The parent must already exist,
    /// be absolute, canonical, and contain no symbolic-link components.
    pub fn create_new(root: &Path) -> Result<Self, FsCasErrorV1> {
        validate_new_root(root)?;
        create_private_directory(root)?;
        let generation = derive_generation(root)?;
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
            )?;
            Ok(())
        })();
        if setup.is_err() {
            let _ = fs::remove_dir_all(root);
        }
        setup?;
        validate_same_filesystem(root, &root.join("preparation"))?;
        validate_same_filesystem(root, &root.join("carriers"))?;
        validate_same_filesystem(root, &root.join("objects"))?;
        validate_same_filesystem(root, &root.join("catalog"))?;
        validate_same_filesystem(root, &root.join("closures"))?;
        let cas = Self {
            inner: shared_root_owner(root, generation)?,
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
        if root.join(INVALIDATED_ROOT_NAME).exists() {
            return Err(FsCasErrorV1::Invalidated);
        }
        for child in ["preparation", "carriers", "objects", "catalog", "closures"] {
            validate_private_directory(&root.join(child))?;
            validate_same_filesystem(root, &root.join(child))?;
        }
        let generation = read_generation_marker(&root.join("generation"))?;
        let cas = Self {
            inner: shared_root_owner(root, generation)?,
        };
        if cas.fixed_handle_ledger_charge_bytes()? > BASE_LEDGER_BYTES {
            return Err(FsCasErrorV1::Core(CoreError::ResourceRefused));
        }
        Ok(cas)
    }

    /// Deterministic language-owned bytes charged to the frozen 8 MiB handle
    /// ledger: the handle, the complete shared `Arc` allocation (reference
    /// counters, alignment, generation and synchronization state), and the root
    /// path allocation capacity. This deliberately makes no RSS/PSS, page
    /// cache, or allocator-internal metadata claim; those require independent
    /// platform measurement.
    pub fn fixed_handle_ledger_charge_bytes(&self) -> CoreResult<u64> {
        let root_capacity =
            u64::try_from(self.inner.root.capacity()).map_err(|_| CoreError::IntegerOverflow)?;
        let arc_header = Layout::new::<[AtomicUsize; 2]>();
        let (arc_layout, _) = arc_header
            .extend(Layout::new::<FsCasInnerV1>())
            .map_err(|_| CoreError::IntegerOverflow)?;
        let arc_allocation = u64::try_from(arc_layout.pad_to_align().size())
            .map_err(|_| CoreError::IntegerOverflow)?;
        u64::try_from(core::mem::size_of::<Self>())
            .map_err(|_| CoreError::IntegerOverflow)?
            .checked_add(arc_allocation)
            .and_then(|bytes| bytes.checked_add(root_capacity))
            .ok_or(CoreError::IntegerOverflow)
    }

    pub fn begin_private_pack(&self) -> Result<FsPrivatePackV1, FsCasErrorV1> {
        self.ensure_valid()?;
        let _guard = self.inner.serial.lock().map_err(|_| FsCasErrorV1::Io)?;
        self.ensure_valid()?;
        validate_private_directory(&self.inner.root.join("preparation"))?;
        let path = unique_private_path(&self.inner.root.join("preparation"), "pack")?;
        Ok(FsPrivatePackV1 {
            owner: self.clone(),
            path,
            state: PrivatePackStateV1::Empty,
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
            prepared, metadata, ledger, None, counters, scratch, control,
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
            counters.observe_open_files(1);
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
            )?;
            if declared != validated {
                return Err(FsCasErrorV1::Integrity);
            }
            sample_control(control, FsCasBoundaryV1::AfterCandidateValidation)?;

            // Candidate validation only reads an operation-private file. Take
            // the shared-root visibility lock after that work, then recheck
            // validity before observing or changing the common namespace.
            let _guard = self.inner.serial.lock().map_err(|_| FsCasErrorV1::Io)?;
            self.ensure_valid()?;

            let name = hex_id(validated.id().as_bytes());
            let carrier_path = self.inner.root.join("carriers").join(&name);
            let marker_path = self.inner.root.join("catalog").join(&name);
            let transaction = NEXT_PRIVATE_NAME.fetch_add(1, Ordering::Relaxed);
            validate_private_directory(&self.inner.root.join("carriers"))?;
            validate_private_directory(&self.inner.root.join("objects"))?;
            validate_private_directory(&self.inner.root.join("catalog"))?;

            if marker_path.exists() || carrier_path.exists() {
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
            let allocated = allocated_bytes(&prepared.path)?;
            let mut publication_capacity = *counters;
            publication_capacity.record_fscas_catalog_operation()?;
            publication_capacity.record_pack_storage(allocated, 0)?;
            publication_capacity.record_unreachable_installed_residue(validated.pack_len())?;
            let locator_residue_bound = u64::from(validated.record_count())
                .checked_mul(
                    u64::try_from(OBJECT_LOCATOR_BYTES).map_err(|_| FsCasErrorV1::Integrity)?,
                )
                .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
            let cleanup_residue_bound = validated
                .pack_len()
                .checked_add(locator_residue_bound)
                .ok_or(FsCasErrorV1::Core(CoreError::IntegerOverflow))?;
            let mut cleanup_capacity = *counters;
            cleanup_capacity.record_unreachable_installed_residue(cleanup_residue_bound)?;

            sample_control(control, FsCasBoundaryV1::BeforeCarrierInstall)?;
            match fs::hard_link(&prepared.path, &carrier_path) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
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
                    return Err(FsCasErrorV1::Unsupported);
                }
                Err(_) => return Err(FsCasErrorV1::Io),
            }

            if let Err(error) = sample_control(control, FsCasBoundaryV1::AfterCarrierInstall) {
                self.rollback_unpublished_carrier(&carrier_path, validated, counters, control)?;
                return Err(error);
            }

            let installed_validation = (|| {
                let mut installed = FilePackReadV1::open(&carrier_path)?;
                counters.observe_open_files(2);
                let observed = validate_pack_for_operation_v1(
                    &mut installed,
                    metadata,
                    scratch,
                    validated.record_count(),
                    ledger,
                    reservation,
                    counters,
                )?;
                counters.record_fscas_read(installed.bytes_read, installed.read_calls)?;
                if observed != validated {
                    return Err(FsCasErrorV1::Integrity);
                }
                Ok(())
            })();
            if let Err(error) = installed_validation {
                self.rollback_unpublished_carrier(&carrier_path, validated, counters, control)?;
                return Err(error);
            }
            if let Err(error) = sample_control(control, FsCasBoundaryV1::AfterCarrierValidation) {
                self.rollback_unpublished_carrier(&carrier_path, validated, counters, control)?;
                return Err(error);
            }

            if let Err(error) = set_read_only(&carrier_path) {
                self.rollback_unpublished_carrier(&carrier_path, validated, counters, control)?;
                return Err(error);
            }
            if let Err(error) = sample_control(control, FsCasBoundaryV1::AfterCarrierMadeImmutable)
            {
                self.rollback_unpublished_carrier(&carrier_path, validated, counters, control)?;
                return Err(error);
            }

            if fs::remove_file(&prepared.path).is_err() {
                self.rollback_unpublished_carrier(&carrier_path, validated, counters, control)?;
                return Err(FsCasErrorV1::Io);
            }
            prepared.state = PrivatePackStateV1::Transferred;

            if let Err(error) = self.install_object_locators(
                &carrier_path,
                validated,
                metadata,
                counters,
                scratch,
                transaction,
                control,
            ) {
                self.rollback_unpublished_admission(
                    &carrier_path,
                    validated,
                    metadata,
                    transaction,
                    counters,
                    control,
                )?;
                return Err(error);
            }

            if let Err(error) = sample_control(control, FsCasBoundaryV1::BeforeCatalogPublication) {
                self.rollback_unpublished_admission(
                    &carrier_path,
                    validated,
                    metadata,
                    transaction,
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
            published_counters.record_pack_storage(allocated, 0)?;
            let mut residue_counters = published_counters;
            residue_counters.record_unreachable_installed_residue(validated.pack_len())?;

            let marker = encode_catalog_marker(validated);
            if let Err(error) = publish_small_marker(
                &self.inner.root.join("preparation"),
                "catalog",
                &marker_path,
                &marker,
            ) {
                self.rollback_unpublished_admission(
                    &carrier_path,
                    validated,
                    metadata,
                    transaction,
                    counters,
                    control,
                )?;
                return Err(error);
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
        }
        result
    }

    fn ensure_valid(&self) -> Result<(), FsCasErrorV1> {
        if self.inner.invalidated.load(Ordering::Acquire)
            || self.inner.root.join(INVALIDATED_ROOT_NAME).exists()
        {
            Err(FsCasErrorV1::Invalidated)
        } else {
            Ok(())
        }
    }

    fn invalidate_root(&self) {
        self.inner.invalidated.store(true, Ordering::Release);
        let marker = self.inner.root.join(INVALIDATED_ROOT_NAME);
        if !marker.exists() {
            // The in-memory owner is invalidated before this best-effort
            // persistent marker. A successfully created directory makes every
            // later open fail closed without relying on marker contents.
            let _ = create_private_directory(&marker);
        }
    }

    fn rollback_unpublished_carrier<C>(
        &self,
        path: &Path,
        sealed: SealedPackV1,
        counters: &mut OperationCountersV1,
        control: &mut C,
    ) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        let injected = control.inject_cleanup_failure(FsCasCleanupTargetV1::Carrier);
        if injected || fs::remove_file(path).is_err() {
            if path.exists()
                && counters
                    .record_unreachable_installed_residue(sealed.pack_len())
                    .is_err()
            {
                self.invalidate_root();
                return Err(FsCasErrorV1::Invalidated);
            }
            self.invalidate_root();
            return Err(FsCasErrorV1::Invalidated);
        }
        Ok(())
    }

    fn rollback_unpublished_admission<M, C>(
        &self,
        carrier: &Path,
        sealed: SealedPackV1,
        metadata: &mut M,
        transaction: u64,
        counters: &mut OperationCountersV1,
        control: &mut C,
    ) -> Result<(), FsCasErrorV1>
    where
        M: PackIndexSpoolV1 + ?Sized,
        C: FsCasControlV1 + ?Sized,
    {
        let objects = self.inner.root.join("objects");
        let locator_residue_bytes = match u64::try_from(OBJECT_LOCATOR_BYTES) {
            Ok(bytes) => bytes,
            Err(_) => {
                self.invalidate_root();
                return Err(FsCasErrorV1::Invalidated);
            }
        };
        let mut cleanup_failed = metadata.sort_by_key().is_err() || metadata.rewind().is_err();
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
                if !path.exists() {
                    continue;
                }
                match read_object_locator(&path, entry.id()) {
                    Ok(locator) if locator.transaction == transaction => {
                        let injected =
                            control.inject_cleanup_failure(FsCasCleanupTargetV1::ObjectLocator);
                        if injected || fs::remove_file(&path).is_err() {
                            if path.exists()
                                && counters
                                    .record_unreachable_installed_residue(locator_residue_bytes)
                                    .is_err()
                            {
                                self.invalidate_root();
                                return Err(FsCasErrorV1::Invalidated);
                            }
                            cleanup_failed = true;
                        }
                    }
                    Ok(_) => {}
                    Err(_) => cleanup_failed = true,
                }
            }
        }

        self.rollback_unpublished_carrier(carrier, sealed, counters, control)?;
        if cleanup_failed {
            self.invalidate_root();
            Err(FsCasErrorV1::Invalidated)
        } else {
            Ok(())
        }
    }

    pub fn occupied(&self) -> Result<FsCasOccupiedV1, FsCasErrorV1> {
        self.ensure_valid()?;
        let _guard = self.inner.serial.lock().map_err(|_| FsCasErrorV1::Io)?;
        self.ensure_valid()?;
        Ok(FsCasOccupiedV1 {
            cas: self.clone(),
            current: None,
            bytes_read: 0,
            read_calls: 0,
            validation_scratch: [0_u8; COMPARISON_WINDOW_BYTES],
        })
    }

    #[cfg(feature = "c3-polymorphism")]
    pub(crate) fn open_admitted_pack_closure_v1(
        &self,
        sealed: SealedPackV1,
    ) -> Result<FsAdmittedPackClosureV1, FsCasErrorV1> {
        self.ensure_valid()?;
        let _guard = self.inner.serial.lock().map_err(|_| FsCasErrorV1::Io)?;
        self.ensure_valid()?;
        let name = hex_id(sealed.id().as_bytes());
        let marker = read_catalog_marker(&self.inner.root.join("catalog").join(&name))?;
        if marker != sealed {
            return Err(FsCasErrorV1::Integrity);
        }
        let pack = FilePackReadV1::open(&self.inner.root.join("carriers").join(name))?;
        if pack.len != sealed.pack_len() {
            return Err(FsCasErrorV1::Integrity);
        }
        Ok(FsAdmittedPackClosureV1 {
            owner: self.clone(),
            pack,
            sealed,
        })
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
        control: &mut C,
    ) -> Result<(), FsCasErrorV1>
    where
        M: PackIndexSpoolV1 + ?Sized,
        C: FsCasControlV1 + ?Sized,
    {
        let objects = self.inner.root.join("objects");
        validate_private_directory(&objects)?;
        metadata
            .sort_by_key()
            .map_err(|_| FsCasErrorV1::Integrity)?;
        metadata.rewind().map_err(|_| FsCasErrorV1::Integrity)?;
        let mut candidate = FilePackReadV1::open(candidate_path)?;

        // Validate every incumbent before creating any locator for this pack.
        while let Some(entry) = metadata.next().map_err(|_| FsCasErrorV1::Integrity)? {
            let path = objects.join(hex_typed_id(entry.id()));
            if path.exists() {
                self.validate_and_compare_object_locator(
                    &mut candidate,
                    entry,
                    &path,
                    counters,
                    scratch,
                    control,
                )?;
            }
        }

        metadata.rewind().map_err(|_| FsCasErrorV1::Integrity)?;
        while let Some(entry) = metadata.next().map_err(|_| FsCasErrorV1::Integrity)? {
            let path = objects.join(hex_typed_id(entry.id()));
            if path.exists() {
                continue;
            }
            sample_control(control, FsCasBoundaryV1::BeforeObjectLocatorPublication)?;
            let marker = encode_object_locator(ObjectLocatorV1 {
                sealed,
                entry,
                transaction,
            });
            publish_small_marker(
                &self.inner.root.join("preparation"),
                "object",
                &path,
                &marker,
            )?;
            sample_control(control, FsCasBoundaryV1::AfterObjectLocatorPublication)?;
        }
        Ok(())
    }

    fn validate_and_compare_object_locator<C>(
        &self,
        candidate: &mut FilePackReadV1,
        candidate_entry: PackIndexEntryV1,
        locator_path: &Path,
        counters: &mut OperationCountersV1,
        scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
        control: &mut C,
    ) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        sample_control(control, FsCasBoundaryV1::BeforeObjectLocatorRead)?;
        let locator = read_object_locator(locator_path, candidate_entry.id())?;
        sample_control(control, FsCasBoundaryV1::AfterObjectLocatorRead)?;
        let pack_name = hex_id(locator.sealed.id().as_bytes());
        let catalog = read_catalog_marker(&self.inner.root.join("catalog").join(&pack_name))?;
        if catalog != locator.sealed {
            return Err(FsCasErrorV1::Integrity);
        }
        let mut incumbent =
            FilePackReadV1::open(&self.inner.root.join("carriers").join(&pack_name))?;
        counters.observe_open_files(2);
        let indexed = locate_validated_pack_index_entry_v1(
            &mut incumbent,
            locator.sealed,
            candidate_entry.id(),
            counters,
        )
        .map_err(|_| FsCasErrorV1::Integrity)?
        .ok_or(FsCasErrorV1::Integrity)?;
        if indexed != locator.entry {
            return Err(FsCasErrorV1::Integrity);
        }
        let location =
            validate_validated_pack_object_v1(&mut incumbent, locator.entry, scratch, counters)
                .map_err(|_| FsCasErrorV1::Integrity)?;
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
        )
    }

    fn validate_existing_object_locators<M, C>(
        &self,
        sealed: SealedPackV1,
        metadata: &mut M,
        control: &mut C,
    ) -> Result<(), FsCasErrorV1>
    where
        M: PackIndexSpoolV1 + ?Sized,
        C: FsCasControlV1 + ?Sized,
    {
        let objects = self.inner.root.join("objects");
        validate_private_directory(&objects)?;
        metadata
            .sort_by_key()
            .map_err(|_| FsCasErrorV1::Integrity)?;
        metadata.rewind().map_err(|_| FsCasErrorV1::Integrity)?;
        while let Some(entry) = metadata.next().map_err(|_| FsCasErrorV1::Integrity)? {
            let path = objects.join(hex_typed_id(entry.id()));
            if !path.exists() {
                return Err(FsCasErrorV1::Integrity);
            }
            sample_control(control, FsCasBoundaryV1::BeforeObjectLocatorRead)?;
            let locator = read_object_locator(&path, entry.id())?;
            sample_control(control, FsCasBoundaryV1::AfterObjectLocatorRead)?;
            if locator.sealed != sealed || locator.entry != entry {
                return Err(FsCasErrorV1::Integrity);
            }
            // The complete carrier and candidate bytes were validated and
            // compared immediately above. Matching the locator to that exact
            // seal and canonical entry therefore validates the incumbent
            // object without reopening or comparing the same bytes twice.
            sample_control(control, FsCasBoundaryV1::AfterObjectIncumbentValidation)?;
        }
        Ok(())
    }

    pub fn begin_closure_operation(&self) -> Result<FsClosureOperationV1, FsCasErrorV1> {
        self.ensure_valid()?;
        let _guard = self.inner.serial.lock().map_err(|_| FsCasErrorV1::Io)?;
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
        let mut fence = FsClosureFenceV1::new(self.clone(), operation.nonce);
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
    pub(crate) fn admit_complete_closure_borrowed_v1<C>(
        &self,
        operation: &mut FsClosureOperationV1,
        closure: &mut C,
        expected_version_record: TypedPhysicalObjectIdV1,
        reservation: &OperationReservationV1<'_>,
        counters: &mut OperationCountersV1,
        buffers: AdmissionBuffersV1<'_>,
        algorithm: crate::cdc::algorithms::C3CdcAlgorithmV1,
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
        let mut fence = FsClosureFenceV1::new(self.clone(), operation.nonce);
        let admitted = crate::cas_stream::admit_complete_immutable_borrowed_v1(
            closure,
            expected_version_record,
            &mut occupied,
            &mut fence,
            reservation,
            counters,
            buffers,
            algorithm,
        )?;
        let capability = fence.complete.take().ok_or(CoreError::SinkRefused)?;
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
        let _guard = self.inner.serial.lock().map_err(|_| FsCasErrorV1::Io)?;
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
            return Err(FsCasErrorV1::Collision);
        }
        let mut incumbent = FilePackReadV1::open(carrier_path)?;
        counters.observe_open_files(2);
        let validated = validate_pack_for_operation_v1(
            &mut incumbent,
            metadata,
            scratch,
            marker.record_count(),
            ledger,
            reservation,
            counters,
        )?;
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
        self.validate_existing_object_locators(candidate, metadata, control)?;
        prepared.abort_private();
        counters.record_fscas_catalog_operation()?;
        Ok(FsPackAdmissionV1 {
            outcome: FsPackAdmissionOutcomeV1::ExistingComplete,
            sealed: candidate,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_pack_for_operation_v1<P, M>(
    pack: &mut P,
    metadata: &mut M,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    maximum_entries: u32,
    ledger: &ResourceLedgerV1,
    reservation: Option<&OperationReservationV1<'_>>,
    counters: &mut OperationCountersV1,
) -> CoreResult<SealedPackV1>
where
    P: PackReadPortV1 + ?Sized,
    M: PackIndexSpoolV1 + ?Sized,
{
    #[cfg(feature = "c3-polymorphism")]
    if let Some(reservation) = reservation {
        return crate::pack::validate_pack_borrowed_v1(
            pack,
            metadata,
            scratch,
            maximum_entries,
            reservation,
            counters,
        );
    }
    let _ = reservation;
    validate_pack_v1(pack, metadata, scratch, maximum_entries, ledger, counters)
}

pub struct FsPrivatePackV1 {
    owner: FsCasV1,
    path: PathBuf,
    state: PrivatePackStateV1,
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
    Aborted,
}

impl FsPrivatePackV1 {
    fn sealed(&self) -> Option<SealedPackV1> {
        self.owner.ensure_valid().ok()?;
        match self.state {
            PrivatePackStateV1::Sealed { sealed, .. } => Some(sealed),
            _ => None,
        }
    }

    #[cfg(feature = "c3-polymorphism")]
    pub(crate) fn begin_direct_v1(&mut self) -> Result<(), PackPortErrorV1> {
        self.begin_private(MAX_PACK_BYTES)?;
        self.append(&[0_u8; crate::pack::PACK_HEADER_BYTES as usize])
    }

    #[cfg(feature = "c3-polymorphism")]
    pub(crate) fn patch_direct_v1(
        &mut self,
        offset: u64,
        bytes: &[u8],
    ) -> Result<(), PackPortErrorV1> {
        self.owner
            .ensure_valid()
            .map_err(|_| PackPortErrorV1::Failure)?;
        let PrivatePackStateV1::Writing { file, written, .. } = &mut self.state else {
            return Err(PackPortErrorV1::Failure);
        };
        let end = offset
            .checked_add(u64::try_from(bytes.len()).map_err(|_| PackPortErrorV1::Failure)?)
            .ok_or(PackPortErrorV1::Failure)?;
        if end > *written {
            return Err(PackPortErrorV1::Failure);
        }
        file.seek(SeekFrom::Start(offset))
            .and_then(|_| file.write_all(bytes))
            .map_err(|_| PackPortErrorV1::Failure)?;
        self.owner
            .ensure_valid()
            .map_err(|_| PackPortErrorV1::Failure)
    }

    #[cfg(feature = "c3-polymorphism")]
    pub(crate) fn truncate_direct_v1(&mut self, len: u64) -> Result<(), PackPortErrorV1> {
        self.owner
            .ensure_valid()
            .map_err(|_| PackPortErrorV1::Failure)?;
        let PrivatePackStateV1::Writing { file, written, .. } = &mut self.state else {
            return Err(PackPortErrorV1::Failure);
        };
        if len > *written || len < crate::pack::PACK_HEADER_BYTES {
            return Err(PackPortErrorV1::Failure);
        }
        file.set_len(len).map_err(|_| PackPortErrorV1::Failure)?;
        *written = len;
        self.owner
            .ensure_valid()
            .map_err(|_| PackPortErrorV1::Failure)
    }

    #[cfg(feature = "c3-polymorphism")]
    pub(crate) fn seal_direct_v1(&mut self, id: PackIdV1) -> Result<(), PackPortErrorV1> {
        let PrivatePackStateV1::Writing {
            expected, written, ..
        } = &mut self.state
        else {
            return Err(PackPortErrorV1::Failure);
        };
        *expected = *written;
        self.seal_private(id)
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
        self.owner
            .ensure_valid()
            .map_err(|_| PackPortErrorV1::Failure)?;
        match &self.state {
            PrivatePackStateV1::Empty => Ok(0),
            PrivatePackStateV1::Writing { written, .. } => Ok(*written),
            PrivatePackStateV1::Sealed { sealed, .. } => Ok(sealed.pack_len()),
            PrivatePackStateV1::Transferred | PrivatePackStateV1::Aborted => {
                Err(PackPortErrorV1::Failure)
            }
        }
    }

    fn read_exact_at(
        &mut self,
        offset: u64,
        destination: &mut [u8],
    ) -> Result<(), PackPortErrorV1> {
        self.owner
            .ensure_valid()
            .map_err(|_| PackPortErrorV1::Failure)?;
        let (file, len) = match &mut self.state {
            PrivatePackStateV1::Writing { file, written, .. } => {
                file.flush().map_err(|_| PackPortErrorV1::Failure)?;
                (file, *written)
            }
            PrivatePackStateV1::Sealed { file, sealed } => (file, sealed.pack_len()),
            _ => return Err(PackPortErrorV1::Failure),
        };
        checked_file_read(file, len, offset, destination).map_err(|_| PackPortErrorV1::Failure)?;
        self.owner
            .ensure_valid()
            .map_err(|_| PackPortErrorV1::Failure)
    }
}

impl PrivatePackPortV1 for FsPrivatePackV1 {
    fn begin_private(&mut self, exact_len: u64) -> Result<(), PackPortErrorV1> {
        self.owner
            .ensure_valid()
            .map_err(|_| PackPortErrorV1::Failure)?;
        if exact_len > MAX_PACK_BYTES || !matches!(self.state, PrivatePackStateV1::Empty) {
            return Err(PackPortErrorV1::Failure);
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&self.path)
            .map_err(|_| PackPortErrorV1::Failure)?;
        set_private_file_permissions(&self.path).map_err(|_| PackPortErrorV1::Failure)?;
        if self.owner.ensure_valid().is_err() {
            drop(file);
            let _ = fs::remove_file(&self.path);
            return Err(PackPortErrorV1::Failure);
        }
        self.state = PrivatePackStateV1::Writing {
            file,
            expected: exact_len,
            written: 0,
        };
        Ok(())
    }

    fn append(&mut self, bytes: &[u8]) -> Result<(), PackPortErrorV1> {
        self.owner
            .ensure_valid()
            .map_err(|_| PackPortErrorV1::Failure)?;
        let PrivatePackStateV1::Writing {
            file,
            expected,
            written,
        } = &mut self.state
        else {
            return Err(PackPortErrorV1::Failure);
        };
        let next = written
            .checked_add(u64::try_from(bytes.len()).map_err(|_| PackPortErrorV1::Failure)?)
            .ok_or(PackPortErrorV1::Failure)?;
        if next > *expected {
            return Err(PackPortErrorV1::Failure);
        }
        file.seek(SeekFrom::Start(*written))
            .and_then(|_| file.write_all(bytes))
            .map_err(|_| PackPortErrorV1::Failure)?;
        *written = next;
        self.owner
            .ensure_valid()
            .map_err(|_| PackPortErrorV1::Failure)
    }

    fn seal_private(&mut self, id: PackIdV1) -> Result<(), PackPortErrorV1> {
        self.owner
            .ensure_valid()
            .map_err(|_| PackPortErrorV1::Failure)?;
        let old = std::mem::replace(&mut self.state, PrivatePackStateV1::Aborted);
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
        file.flush().map_err(|_| PackPortErrorV1::Failure)?;
        drop(file);
        let file = open_regular_file(&self.path).map_err(|_| PackPortErrorV1::Failure)?;
        let mut reader =
            FilePackReadV1::from_file(file.try_clone().map_err(|_| PackPortErrorV1::Failure)?)
                .map_err(|_| PackPortErrorV1::Failure)?;
        let sealed = read_sealed_shape(&mut reader).map_err(|_| PackPortErrorV1::Failure)?;
        if sealed.id() != id || sealed.pack_len() != expected {
            return Err(PackPortErrorV1::Failure);
        }
        if self.owner.ensure_valid().is_err() {
            drop(file);
            let _ = fs::remove_file(&self.path);
            return Err(PackPortErrorV1::Failure);
        }
        self.state = PrivatePackStateV1::Sealed { file, sealed };
        Ok(())
    }

    fn abort_private(&mut self) {
        self.state = PrivatePackStateV1::Aborted;
        let _ = fs::remove_file(&self.path);
    }
}

impl Drop for FsPrivatePackV1 {
    fn drop(&mut self) {
        if !matches!(self.state, PrivatePackStateV1::Transferred) {
            self.state = PrivatePackStateV1::Aborted;
            let _ = fs::remove_file(&self.path);
        }
    }
}

struct FilePackReadV1 {
    file: File,
    len: u64,
    bytes_read: u64,
    read_calls: u64,
}

#[cfg(feature = "c3-polymorphism")]
pub(crate) struct FsAdmittedPackClosureV1 {
    owner: FsCasV1,
    pack: FilePackReadV1,
    sealed: SealedPackV1,
}

#[cfg(feature = "c3-polymorphism")]
impl FsAdmittedPackClosureV1 {
    fn entry(&mut self, ordinal: u64) -> Result<PackIndexEntryV1, ImmutablePortErrorV1> {
        self.owner
            .ensure_valid()
            .map_err(|_| ImmutablePortErrorV1::Failure)?;
        if ordinal >= u64::from(self.sealed.record_count()) {
            return Err(ImmutablePortErrorV1::Failure);
        }
        let offset = self
            .sealed
            .index_offset()
            .checked_add(
                ordinal
                    .checked_mul(crate::pack::PACK_INDEX_ENTRY_BYTES)
                    .ok_or(ImmutablePortErrorV1::Failure)?,
            )
            .ok_or(ImmutablePortErrorV1::Failure)?;
        let mut bytes = [0_u8; crate::pack::PACK_INDEX_ENTRY_BYTES as usize];
        self.pack
            .read_exact_at(offset, &mut bytes)
            .map_err(|_| ImmutablePortErrorV1::Failure)?;
        crate::pack::decode_index_entry(&bytes).map_err(|_| ImmutablePortErrorV1::Failure)
    }
}

#[cfg(feature = "c3-polymorphism")]
impl CompleteImmutableClosureReadPortV1 for FsAdmittedPackClosureV1 {
    fn object_count(&mut self) -> Result<u64, ImmutablePortErrorV1> {
        self.owner
            .ensure_valid()
            .map_err(|_| ImmutablePortErrorV1::Failure)?;
        Ok(u64::from(self.sealed.record_count()))
    }

    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        u64::try_from(core::mem::size_of::<Self>()).map_err(|_| CoreError::IntegerOverflow)
    }

    fn direct_storage_read_observation(&self) -> Result<(u64, u64), ImmutablePortErrorV1> {
        self.owner
            .ensure_valid()
            .map_err(|_| ImmutablePortErrorV1::Failure)?;
        Ok((self.pack.bytes_read, self.pack.read_calls))
    }

    fn object_id_at(
        &mut self,
        ordinal: u64,
    ) -> Result<TypedPhysicalObjectIdV1, ImmutablePortErrorV1> {
        self.entry(ordinal).map(PackIndexEntryV1::id)
    }

    fn object_len_at(&mut self, ordinal: u64) -> Result<u64, ImmutablePortErrorV1> {
        self.entry(ordinal)
            .map(|entry| u64::from(entry.object_len()))
    }

    fn read_object_exact_at(
        &mut self,
        ordinal: u64,
        offset: u64,
        destination: &mut [u8],
    ) -> Result<(), ImmutablePortErrorV1> {
        let entry = self.entry(ordinal)?;
        let requested =
            u64::try_from(destination.len()).map_err(|_| ImmutablePortErrorV1::Failure)?;
        let end = offset
            .checked_add(requested)
            .ok_or(ImmutablePortErrorV1::Failure)?;
        if end > u64::from(entry.object_len()) {
            return Err(ImmutablePortErrorV1::Failure);
        }
        let absolute = entry
            .absolute_offset()
            .checked_add(4)
            .and_then(|start| start.checked_add(offset))
            .ok_or(ImmutablePortErrorV1::Failure)?;
        self.pack
            .read_exact_at(absolute, destination)
            .map_err(|_| ImmutablePortErrorV1::Failure)
    }
}

impl FilePackReadV1 {
    fn open(path: &Path) -> Result<Self, FsCasErrorV1> {
        let file = open_regular_file(path)?;
        Self::from_file(file)
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

pub struct FsCasOccupiedV1 {
    cas: FsCasV1,
    current: Option<ResolvedObjectV1>,
    bytes_read: u64,
    read_calls: u64,
    validation_scratch: [u8; COMPARISON_WINDOW_BYTES],
}

impl FsCasOccupiedV1 {
    pub const fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    pub const fn read_calls(&self) -> u64 {
        self.read_calls
    }
}

impl OccupiedImmutableReadPortV1 for FsCasOccupiedV1 {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        u64::try_from(core::mem::size_of::<Self>()).map_err(|_| CoreError::IntegerOverflow)
    }

    fn direct_storage_read_observation(&self) -> Result<(u64, u64), ImmutablePortErrorV1> {
        if self.cas.inner.invalidated.load(Ordering::Acquire) {
            return Err(ImmutablePortErrorV1::Failure);
        }
        Ok((self.bytes_read, self.read_calls))
    }

    fn occupied_len(
        &mut self,
        id: TypedPhysicalObjectIdV1,
    ) -> Result<Option<u64>, ImmutablePortErrorV1> {
        self.cas
            .ensure_valid()
            .map_err(|_| ImmutablePortErrorV1::Failure)?;
        let _guard = self
            .cas
            .inner
            .serial
            .lock()
            .map_err(|_| ImmutablePortErrorV1::Failure)?;
        self.cas
            .ensure_valid()
            .map_err(|_| ImmutablePortErrorV1::Failure)?;
        self.current = None;
        let objects = self.cas.inner.root.join("objects");
        validate_private_directory(&objects).map_err(|_| ImmutablePortErrorV1::Failure)?;
        let path = objects.join(hex_typed_id(id));
        if !path.exists() {
            return Ok(None);
        }
        let locator = read_object_locator(&path, id).map_err(|_| ImmutablePortErrorV1::Failure)?;
        let pack_name = hex_id(locator.sealed.id().as_bytes());
        let catalog = read_catalog_marker(&self.cas.inner.root.join("catalog").join(&pack_name))
            .map_err(|_| ImmutablePortErrorV1::Failure)?;
        if catalog != locator.sealed {
            return Err(ImmutablePortErrorV1::Failure);
        }
        let carrier = self.cas.inner.root.join("carriers").join(&pack_name);
        let mut pack = FilePackReadV1::open(&carrier).map_err(|_| ImmutablePortErrorV1::Failure)?;
        let mut local_counters = OperationCountersV1::default();
        let indexed = locate_validated_pack_index_entry_v1(
            &mut pack,
            locator.sealed,
            id,
            &mut local_counters,
        )
        .map_err(|_| ImmutablePortErrorV1::Failure)?
        .ok_or(ImmutablePortErrorV1::Failure)?;
        if indexed != locator.entry {
            return Err(ImmutablePortErrorV1::Failure);
        }
        let location = validate_validated_pack_object_v1(
            &mut pack,
            indexed,
            &mut self.validation_scratch,
            &mut local_counters,
        )
        .map_err(|_| ImmutablePortErrorV1::Failure)?;
        self.bytes_read = self
            .bytes_read
            .checked_add(pack.bytes_read)
            .ok_or(ImmutablePortErrorV1::Failure)?;
        self.read_calls = self
            .read_calls
            .checked_add(pack.read_calls)
            .ok_or(ImmutablePortErrorV1::Failure)?;
        self.current = Some(ResolvedObjectV1 {
            id,
            file: pack.file,
            pack_len: pack.len,
            location,
        });
        Ok(Some(location.object_len))
    }

    fn read_occupied_exact_at(
        &mut self,
        id: TypedPhysicalObjectIdV1,
        offset: u64,
        destination: &mut [u8],
    ) -> Result<(), ImmutablePortErrorV1> {
        self.cas
            .ensure_valid()
            .map_err(|_| ImmutablePortErrorV1::Failure)?;
        let _guard = self
            .cas
            .inner
            .serial
            .lock()
            .map_err(|_| ImmutablePortErrorV1::Failure)?;
        self.cas
            .ensure_valid()
            .map_err(|_| ImmutablePortErrorV1::Failure)?;
        let resolved = self.current.as_mut().ok_or(ImmutablePortErrorV1::Failure)?;
        if resolved.id != id {
            return Err(ImmutablePortErrorV1::Failure);
        }
        let end = offset
            .checked_add(
                u64::try_from(destination.len()).map_err(|_| ImmutablePortErrorV1::Failure)?,
            )
            .ok_or(ImmutablePortErrorV1::Failure)?;
        if end > resolved.location.object_len {
            return Err(ImmutablePortErrorV1::Failure);
        }
        let absolute = resolved
            .location
            .object_offset
            .checked_add(offset)
            .ok_or(ImmutablePortErrorV1::Failure)?;
        checked_file_read(&mut resolved.file, resolved.pack_len, absolute, destination)
            .map_err(|_| ImmutablePortErrorV1::Failure)?;
        let amount = u64::try_from(destination.len()).map_err(|_| ImmutablePortErrorV1::Failure)?;
        self.bytes_read = self
            .bytes_read
            .checked_add(amount)
            .ok_or(ImmutablePortErrorV1::Failure)?;
        self.read_calls = self
            .read_calls
            .checked_add(1)
            .ok_or(ImmutablePortErrorV1::Failure)?;
        Ok(())
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

struct FsClosureFenceV1 {
    cas: FsCasV1,
    operation_nonce: u64,
    expected_count: Option<u64>,
    observed_count: u64,
    previous_id: Option<TypedPhysicalObjectIdV1>,
    observed_version: Option<TypedPhysicalObjectIdV1>,
    transcript: Option<ClosureTranscriptV1>,
    complete: Option<CompleteValidatedClosureV1>,
}

impl FsClosureFenceV1 {
    fn new(cas: FsCasV1, operation_nonce: u64) -> Self {
        Self {
            cas,
            operation_nonce,
            expected_count: None,
            observed_count: 0,
            previous_id: None,
            observed_version: None,
            transcript: None,
            complete: None,
        }
    }
}

impl PreparedImmutableClosurePortV1 for FsClosureFenceV1 {
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
        let _guard = self
            .cas
            .inner
            .serial
            .lock()
            .map_err(|_| ImmutablePortErrorV1::Failure)?;
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
        if destination.exists() {
            let incumbent = read_exact_regular_file::<CLOSURE_MARKER_BYTES>(&destination)
                .map_err(|_| ImmutablePortErrorV1::Failure)?;
            if incumbent != marker {
                return Err(ImmutablePortErrorV1::Failure);
            }
        } else {
            // This marker is only the local complete-closure fence. Publishing
            // it performs no authority dispatch and creates no private Version.
            publish_small_marker(
                &self.cas.inner.root.join("preparation"),
                "closure",
                &destination,
                &marker,
            )
            .map_err(|_| ImmutablePortErrorV1::Failure)?;
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
            .map_err(|_| FsCasErrorV1::Integrity)?;
        if left[..take] != right[..take] {
            return Err(FsCasErrorV1::Collision);
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
        return Err(FsCasErrorV1::Collision);
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
            .map_err(|_| FsCasErrorV1::Integrity)?;
        if left[..take] != right[..take] {
            return Err(FsCasErrorV1::Collision);
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

fn encode_catalog_marker(sealed: SealedPackV1) -> [u8; CATALOG_MARKER_BYTES] {
    let mut bytes = [0_u8; CATALOG_MARKER_BYTES];
    bytes[..8].copy_from_slice(CATALOG_MAGIC);
    bytes[8..40].copy_from_slice(sealed.id().as_bytes());
    bytes[40..48].copy_from_slice(&sealed.pack_len().to_be_bytes());
    bytes[48..52].copy_from_slice(&sealed.record_count().to_be_bytes());
    bytes[56..64].copy_from_slice(&sealed.index_offset().to_be_bytes());
    bytes
}

fn read_catalog_marker(path: &Path) -> Result<SealedPackV1, FsCasErrorV1> {
    let bytes = read_exact_regular_file::<CATALOG_MARKER_BYTES>(path)?;
    if &bytes[..8] != CATALOG_MAGIC || bytes[52..56] != [0_u8; 4] {
        return Err(FsCasErrorV1::Integrity);
    }
    let id = <[u8; 32]>::try_from(&bytes[8..40]).map_err(|_| FsCasErrorV1::Integrity)?;
    let pack_len = u64::from_be_bytes(
        bytes[40..48]
            .try_into()
            .map_err(|_| FsCasErrorV1::Integrity)?,
    );
    let record_count = u32::from_be_bytes(
        bytes[48..52]
            .try_into()
            .map_err(|_| FsCasErrorV1::Integrity)?,
    );
    let index_offset = u64::from_be_bytes(
        bytes[56..64]
            .try_into()
            .map_err(|_| FsCasErrorV1::Integrity)?,
    );
    Ok(SealedPackV1::from_validated_parts(
        PackIdV1::from_digest(id),
        pack_len,
        record_count,
        index_offset,
    ))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ObjectLocatorV1 {
    sealed: SealedPackV1,
    entry: PackIndexEntryV1,
    transaction: u64,
}

fn encode_object_locator(locator: ObjectLocatorV1) -> [u8; OBJECT_LOCATOR_BYTES] {
    let mut bytes = [0_u8; OBJECT_LOCATOR_BYTES];
    bytes[..8].copy_from_slice(OBJECT_LOCATOR_MAGIC);
    bytes[8] = typed_kind_byte(locator.entry.id());
    bytes[16..48].copy_from_slice(locator.entry.id().as_bytes());
    bytes[48..80].copy_from_slice(locator.sealed.id().as_bytes());
    bytes[80..88].copy_from_slice(&locator.sealed.pack_len().to_be_bytes());
    bytes[88..92].copy_from_slice(&locator.sealed.record_count().to_be_bytes());
    bytes[96..104].copy_from_slice(&locator.sealed.index_offset().to_be_bytes());
    bytes[104..112].copy_from_slice(&locator.entry.absolute_offset().to_be_bytes());
    bytes[112..116].copy_from_slice(&locator.entry.object_len().to_be_bytes());
    bytes[120..152].copy_from_slice(locator.entry.object_checksum().as_bytes());
    bytes[152..160].copy_from_slice(&locator.transaction.to_be_bytes());
    bytes
}

fn read_object_locator(
    path: &Path,
    expected: TypedPhysicalObjectIdV1,
) -> Result<ObjectLocatorV1, FsCasErrorV1> {
    let bytes = read_exact_regular_file::<OBJECT_LOCATOR_BYTES>(path)?;
    if &bytes[..8] != OBJECT_LOCATOR_MAGIC
        || bytes[8] != typed_kind_byte(expected)
        || bytes[9..16] != [0_u8; 7]
        || bytes[16..48] != *expected.as_bytes()
        || bytes[92..96] != [0_u8; 4]
        || bytes[116..120] != [0_u8; 4]
    {
        return Err(FsCasErrorV1::Integrity);
    }
    let pack_id = <[u8; 32]>::try_from(&bytes[48..80]).map_err(|_| FsCasErrorV1::Integrity)?;
    let pack_len = u64::from_be_bytes(
        bytes[80..88]
            .try_into()
            .map_err(|_| FsCasErrorV1::Integrity)?,
    );
    let record_count = u32::from_be_bytes(
        bytes[88..92]
            .try_into()
            .map_err(|_| FsCasErrorV1::Integrity)?,
    );
    let index_offset = u64::from_be_bytes(
        bytes[96..104]
            .try_into()
            .map_err(|_| FsCasErrorV1::Integrity)?,
    );
    let absolute_offset = u64::from_be_bytes(
        bytes[104..112]
            .try_into()
            .map_err(|_| FsCasErrorV1::Integrity)?,
    );
    let object_len = u32::from_be_bytes(
        bytes[112..116]
            .try_into()
            .map_err(|_| FsCasErrorV1::Integrity)?,
    );
    let checksum = <[u8; 32]>::try_from(&bytes[120..152]).map_err(|_| FsCasErrorV1::Integrity)?;
    let transaction = u64::from_be_bytes(
        bytes[152..160]
            .try_into()
            .map_err(|_| FsCasErrorV1::Integrity)?,
    );
    Ok(ObjectLocatorV1 {
        sealed: SealedPackV1::from_validated_parts(
            PackIdV1::from_digest(pack_id),
            pack_len,
            record_count,
            index_offset,
        ),
        entry: PackIndexEntryV1::from_validated_parts(
            expected,
            absolute_offset,
            object_len,
            ObjectChecksumV1::from_digest(checksum),
        ),
        transaction,
    })
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

fn publish_small_marker<const N: usize>(
    preparation: &Path,
    prefix: &str,
    destination: &Path,
    bytes: &[u8; N],
) -> Result<(), FsCasErrorV1> {
    validate_private_directory(preparation)?;
    let temporary = unique_private_path(preparation, prefix)?;
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| FsCasErrorV1::Io)?;
        set_private_file_permissions(&temporary)?;
        file.write_all(bytes).map_err(|_| FsCasErrorV1::Io)?;
        file.flush().map_err(|_| FsCasErrorV1::Io)?;
        drop(file);
        let observed = read_exact_regular_file::<N>(&temporary)?;
        if observed != *bytes {
            return Err(FsCasErrorV1::Integrity);
        }
        set_read_only(&temporary)?;
        fs::hard_link(&temporary, destination).map_err(|error| {
            if is_unsupported_link_error(&error) {
                FsCasErrorV1::Unsupported
            } else {
                FsCasErrorV1::Io
            }
        })?;
        fs::remove_file(&temporary).map_err(|_| FsCasErrorV1::Io)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn read_exact_regular_file<const N: usize>(path: &Path) -> Result<[u8; N], FsCasErrorV1> {
    let mut file = open_regular_file(path)?;
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
    let before = fs::symlink_metadata(path).map_err(|_| FsCasErrorV1::Io)?;
    if !before.file_type().is_file() || before.file_type().is_symlink() {
        return Err(FsCasErrorV1::Integrity);
    }
    let file = File::open(path).map_err(|_| FsCasErrorV1::Io)?;
    let after = file.metadata().map_err(|_| FsCasErrorV1::Io)?;
    if !same_file_identity(&before, &after) {
        return Err(FsCasErrorV1::Integrity);
    }
    Ok(file)
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

fn validate_new_root(root: &Path) -> Result<(), FsCasErrorV1> {
    if !root.is_absolute() || root.file_name().is_none() || root.exists() {
        return Err(FsCasErrorV1::Unsupported);
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
        if !path.exists() {
            return Ok(path);
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

#[cfg(unix)]
fn allocated_bytes(path: &Path) -> Result<u64, FsCasErrorV1> {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(path)
        .map_err(|_| FsCasErrorV1::Io)?
        .blocks()
        .checked_mul(512)
        .ok_or(FsCasErrorV1::Integrity)
}

#[cfg(not(unix))]
fn allocated_bytes(path: &Path) -> Result<u64, FsCasErrorV1> {
    Ok(fs::metadata(path).map_err(|_| FsCasErrorV1::Io)?.len())
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
