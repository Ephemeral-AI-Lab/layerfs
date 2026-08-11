use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::Duration;

use crate::cas::FsOperationObservedControlV1;
use layerfs_storage::cas::{
    AdmissionBuffersV1, CompleteImmutableClosureReadPortV1, ImmutablePortErrorV1,
    OccupiedImmutableReadPortV1,
};
use layerfs_storage::cas::{
    FsCasBoundaryV1, FsCasCleanupTargetV1, FsCasControlV1, FsCasErrorV1, FsCasFailureCauseV1,
    FsCasFilesystemBoundaryV1, FsCasFilesystemFailureV1, FsCasV1, FsPackAdmissionOutcomeV1,
    FsPrivatePackV1, CATALOG_MARKER_BYTES, PERSISTENT_LOCATOR_BYTES_V1,
};
use layerfs_storage::identity::{
    derive_implicit_root_directory_v1, derive_physical_tree_id_v1,
    derive_physical_version_record_id_v1, derive_version_v1,
};
use layerfs_storage::limits::{OperationCountersV1, ResourceLedgerV1, BASE_LEDGER_BYTES};
use layerfs_storage::object::{
    decode_physical_object_v1, DiscardStrongEdgesV1, TypedPhysicalObjectIdV1,
};
use layerfs_storage::pack::{
    build_dense_pack_v1, PackIndexEntryV1, PackIndexSpoolV1, PackObjectSourceV1, PackPortErrorV1,
    PackReadPortV1, PrivatePackPortV1,
};
use layerfs_storage::profile::{ChunkerSpecV1, DigestSpecV1, ProfileSpecV1};
use layerfs_storage::{CoreError, CoreResult};

static NEXT_ROOT: AtomicU64 = AtomicU64::new(1);

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

    fn release_v1(&mut self) {
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

fn assert_path_absent(path: &Path) {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Ok(metadata) => {
            panic!("expected {path:?} to be absent, but metadata succeeded: {metadata:?}")
        }
        Err(error) => panic!("expected {path:?} to be absent, but lookup failed: {error}"),
    }
}

fn exact_fresh_pack_immutable_bytes(pack_len: u64, record_count: u32) -> u64 {
    pack_len
        + u64::from(record_count) * u64::try_from(PERSISTENT_LOCATOR_BYTES_V1).unwrap()
        + u64::try_from(CATALOG_MARKER_BYTES).unwrap()
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

fn make_owner_writable(path: &Path) -> fs::Permissions {
    let original = fs::metadata(path).unwrap().permissions();
    let mut permissions = original.clone();
    #[cfg(unix)]
    permissions.set_mode(permissions.mode() | 0o200);
    #[cfg(windows)]
    permissions.set_readonly(false);
    fs::set_permissions(path, permissions).unwrap();
    original
}

#[derive(Clone, Copy)]
enum StopKind {
    Cancellation,
    Deadline,
}

struct StopAtBoundary {
    kind: StopKind,
    target: FsCasBoundaryV1,
    current: Option<FsCasBoundaryV1>,
}

impl StopAtBoundary {
    const fn new(kind: StopKind, target: FsCasBoundaryV1) -> Self {
        Self {
            kind,
            target,
            current: None,
        }
    }
}

impl FsCasControlV1 for StopAtBoundary {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        self.current = Some(boundary);
    }

    fn cancellation_requested(&mut self) -> bool {
        matches!(self.kind, StopKind::Cancellation) && self.current == Some(self.target)
    }

    fn deadline_exceeded(&mut self) -> bool {
        matches!(self.kind, StopKind::Deadline) && self.current == Some(self.target)
    }
}

struct StopWithCleanupFailure {
    stop: FsCasBoundaryV1,
    cleanup_target: FsCasCleanupTargetV1,
    current: Option<FsCasBoundaryV1>,
    injected: bool,
}

impl StopWithCleanupFailure {
    const fn new(stop: FsCasBoundaryV1, cleanup_target: FsCasCleanupTargetV1) -> Self {
        Self {
            stop,
            cleanup_target,
            current: None,
            injected: false,
        }
    }
}

impl FsCasControlV1 for StopWithCleanupFailure {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        self.current = Some(boundary);
    }

    fn cancellation_requested(&mut self) -> bool {
        self.current == Some(self.stop)
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }

    fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
        if !self.injected && target == self.cleanup_target {
            self.injected = true;
            true
        } else {
            false
        }
    }
}

/// A single semantic immutable-read fault. This exercises an authority read
/// boundary, not an inferred native syscall count.
struct ReadFaultAtBoundary {
    boundary: FsCasFilesystemBoundaryV1,
    error: FsCasErrorV1,
    injected: bool,
}

impl FsCasControlV1 for ReadFaultAtBoundary {
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

struct BreakCatalogAtPublication {
    catalog: PathBuf,
    injected: bool,
}

struct InstallMalformedLocatorAtPublication {
    locator: PathBuf,
    injected: bool,
}

#[cfg(unix)]
struct ReplaceLocatorAfterCompleteComparison {
    locator: PathBuf,
    displaced: PathBuf,
    injected: bool,
}

struct InstallLocatorAndFailPreparationCleanup {
    locator: PathBuf,
    occupant_injected: bool,
    cleanup_injected: bool,
}

struct InstallMalformedCatalogAtPublication {
    root: PathBuf,
    injected: bool,
}

struct InstallUnequalCatalogAtPublication {
    root: PathBuf,
    bytes: Vec<u8>,
    bind_candidate_id: bool,
    injected: bool,
}

struct CorruptCarrierBeforeRollback {
    root: PathBuf,
    injected: bool,
}

struct ObserveIncumbentComparisonLock {
    cas: FsCasV1,
    observed: bool,
    visibility_available: bool,
    publication_available: bool,
}

struct ObserveFreshCarrierValidationLock {
    cas: FsCasV1,
    observed: bool,
    visibility_available: bool,
    publication_available: bool,
}

struct BlockCatalogMarkerWrite {
    entered_signal: mpsc::SyncSender<()>,
    release: mpsc::Receiver<()>,
    catalog_phase: bool,
    blocked: bool,
}

struct BlockPreparationCreate {
    entered_signal: mpsc::SyncSender<()>,
    release: mpsc::Receiver<()>,
    blocked: bool,
}

struct BlockAfterObjectLocatorPublication {
    release: Arc<WatchdogGateV1>,
    entered_signal: mpsc::SyncSender<()>,
    blocked: bool,
}

struct BlockAtIncumbentAuthorityV1 {
    release: Arc<WatchdogGateV1>,
    entered_signal: Option<mpsc::SyncSender<()>>,
}

struct ContinueControlV1;

struct SignalLocatorOwnerPublicationWait {
    entered_signal: Option<mpsc::SyncSender<()>>,
}

impl FsCasControlV1 for ObserveFreshCarrierValidationLock {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if boundary == FsCasBoundaryV1::AfterCarrierInstall {
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

impl FsCasControlV1 for ObserveIncumbentComparisonLock {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if matches!(
            boundary,
            FsCasBoundaryV1::BeforeIncumbentComparisonWindow
                | FsCasBoundaryV1::BeforeObjectComparisonWindow
        ) {
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

impl FsCasControlV1 for BlockCatalogMarkerWrite {
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
        if self.catalog_phase && !self.blocked && boundary == FsCasFilesystemBoundaryV1::MarkerWrite
        {
            self.blocked = true;
            self.entered_signal
                .send(())
                .expect("catalog preparation watchdog receiver remains live");
            self.release
                .recv_timeout(Duration::from_secs(5))
                .expect("catalog preparation release watchdog expired");
        }
        None
    }
}

impl FsCasControlV1 for BlockPreparationCreate {
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
            self.entered_signal
                .send(())
                .expect("preparation-create watchdog receiver remains live");
            self.release
                .recv_timeout(Duration::from_secs(5))
                .expect("preparation-create release watchdog expired");
        }
        None
    }
}

impl FsCasControlV1 for BlockAfterObjectLocatorPublication {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if !self.blocked && boundary == FsCasBoundaryV1::AfterObjectLocatorPublication {
            self.blocked = true;
            self.entered_signal
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

impl FsCasControlV1 for BlockAtIncumbentAuthorityV1 {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if boundary == FsCasBoundaryV1::BeforeIncumbentMarkerRead {
            if let Some(entered) = self.entered_signal.take() {
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

impl FsCasControlV1 for ContinueControlV1 {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for SignalLocatorOwnerPublicationWait {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if boundary == FsCasBoundaryV1::LocatorOwnerPublicationWait {
            if let Some(signal) = self.entered_signal.take() {
                signal
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

impl FsCasControlV1 for BreakCatalogAtPublication {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if boundary == FsCasBoundaryV1::BeforeCatalogPublication && !self.injected {
            fs::remove_dir(&self.catalog).unwrap();
            fs::write(&self.catalog, b"injected-not-a-directory").unwrap();
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

impl FsCasControlV1 for InstallMalformedLocatorAtPublication {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if boundary == FsCasBoundaryV1::BeforeObjectLocatorPublication && !self.injected {
            fs::write(&self.locator, [0_u8; 160]).unwrap();
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

#[cfg(unix)]
impl FsCasControlV1 for ReplaceLocatorAfterCompleteComparison {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if boundary == FsCasBoundaryV1::AfterObjectComparisonWindow && !self.injected {
            fs::rename(&self.locator, &self.displaced).unwrap();
            fs::write(&self.locator, [0_u8; PERSISTENT_LOCATOR_BYTES_V1]).unwrap();
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

impl FsCasControlV1 for InstallLocatorAndFailPreparationCleanup {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if boundary == FsCasBoundaryV1::BeforeObjectLocatorPublication && !self.occupant_injected {
            fs::write(&self.locator, [0_u8; 160]).unwrap();
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

impl FsCasControlV1 for InstallMalformedCatalogAtPublication {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if boundary == FsCasBoundaryV1::BeforeCatalogPublication && !self.injected {
            let carrier = only_entry(&self.root.join("carriers"));
            let destination = self.root.join("catalog").join(carrier.file_name().unwrap());
            fs::write(destination, [0_u8; 64]).unwrap();
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

impl FsCasControlV1 for InstallUnequalCatalogAtPublication {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if boundary == FsCasBoundaryV1::BeforeCatalogPublication && !self.injected {
            let carrier = only_entry(&self.root.join("carriers"));
            let destination = self.root.join("catalog").join(carrier.file_name().unwrap());
            if self.bind_candidate_id {
                let name = carrier.file_name().unwrap().to_str().unwrap();
                for (index, slot) in self.bytes[8..40].iter_mut().enumerate() {
                    *slot = u8::from_str_radix(&name[index * 2..index * 2 + 2], 16).unwrap();
                }
            }
            fs::write(destination, &self.bytes).unwrap();
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

impl FsCasControlV1 for CorruptCarrierBeforeRollback {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if boundary == FsCasBoundaryV1::BeforeCatalogPublication && !self.injected {
            let carrier = only_entry(&self.root.join("carriers"));
            let original_permissions = make_owner_writable(&carrier);
            let mut bytes = fs::read(&carrier).unwrap();
            let index_offset =
                usize::try_from(u64::from_be_bytes(bytes[56..64].try_into().unwrap())).unwrap();
            // Preserve the carrier's exact length and every payload byte, but
            // make the first index entry structurally invalid. Rollback owns
            // this immutable index as its cleanup-enumeration authority.
            bytes[index_offset + 1] = 1;
            fs::write(&carrier, bytes).unwrap();
            fs::set_permissions(&carrier, original_permissions).unwrap();
            self.injected = true;
        }
    }

    fn cancellation_requested(&mut self) -> bool {
        self.injected
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

struct TestRoot {
    path: PathBuf,
}

impl TestRoot {
    fn new(label: &str) -> Self {
        let parent = fs::canonicalize(std::env::temp_dir()).unwrap();
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            "layerfs-c3-{label}-{}-{sequence:016x}",
            std::process::id()
        ));
        Self { path }
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn object(kind: u8, payload: &[u8]) -> Vec<u8> {
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

fn typed_id(bytes: &[u8]) -> TypedPhysicalObjectIdV1 {
    decode_physical_object_v1(bytes, &mut DiscardStrongEdgesV1)
        .unwrap()
        .physical_id()
        .unwrap()
}

fn empty_closure() -> (
    Vec<u8>,
    Vec<u8>,
    TypedPhysicalObjectIdV1,
    TypedPhysicalObjectIdV1,
) {
    let root = object(2, &[1, 0x10, 0, 0, 0, 0, 0, 0, 0]);
    let root_id = derive_physical_tree_id_v1(&root).unwrap();
    let logical_root = derive_implicit_root_directory_v1(&[]).unwrap();
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
    let version = object(1, &payload);
    let version_id = TypedPhysicalObjectIdV1::VersionRecord(
        derive_physical_version_record_id_v1(&version).unwrap(),
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

struct ObjectSource<'a> {
    bytes: &'a [Vec<u8>],
    ids: Vec<TypedPhysicalObjectIdV1>,
    fail_payload: bool,
    payload_bytes_read: u64,
}

impl<'a> ObjectSource<'a> {
    fn new(bytes: &'a [Vec<u8>]) -> Self {
        Self {
            bytes,
            ids: bytes.iter().map(|bytes| typed_id(bytes)).collect(),
            fail_payload: false,
            payload_bytes_read: 0,
        }
    }
}

impl PackObjectSourceV1 for ObjectSource<'_> {
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
struct Spool {
    entries: Vec<PackIndexEntryV1>,
    cursor: usize,
    maximum: usize,
}

impl PackIndexSpoolV1 for Spool {
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
        if self.entries.len() == self.maximum {
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
        let result = self.entries.get(self.cursor).copied();
        self.cursor = self
            .cursor
            .checked_add(usize::from(result.is_some()))
            .ok_or(PackPortErrorV1::Failure)?;
        Ok(result)
    }

    fn abort(&mut self) {
        self.entries.clear();
        self.cursor = 0;
    }
}

fn build_private_pack(
    cas: &FsCasV1,
    objects: &[Vec<u8>],
    ledger: &ResourceLedgerV1,
    counters: &mut OperationCountersV1,
    scratch: &mut [u8; 65_536],
) -> FsPrivatePackV1 {
    let mut source = ObjectSource::new(objects);
    let mut pack = cas.begin_private_pack().unwrap();
    let mut spool = Spool::default();
    build_dense_pack_v1(
        &mut source,
        &mut pack,
        &mut spool,
        ledger,
        counters,
        scratch,
    )
    .unwrap();
    assert_eq!(
        source.payload_bytes_read,
        objects.iter().map(|bytes| bytes.len() as u64).sum::<u64>()
    );
    pack
}

fn only_entry(directory: &Path) -> PathBuf {
    let mut entries = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 1);
    entries.pop().unwrap()
}

fn locator_path(root: &Path, id: TypedPhysicalObjectIdV1) -> PathBuf {
    let prefix = match id {
        TypedPhysicalObjectIdV1::VersionRecord(_) => "01-",
        TypedPhysicalObjectIdV1::Tree(_) => "02-",
        TypedPhysicalObjectIdV1::File(_) => "03-",
        TypedPhysicalObjectIdV1::Symlink(_) => "04-",
        TypedPhysicalObjectIdV1::Chunk(_) => "05-",
    };
    let mut name = String::from(prefix);
    for byte in id.as_bytes() {
        use std::fmt::Write as _;
        write!(&mut name, "{byte:02x}").unwrap();
    }
    root.join("objects").join(name)
}

#[test]
fn pack_is_transferred_once_then_reopened_through_committed_catalog() {
    let fixture = TestRoot::new("install");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    assert!(cas.fixed_handle_ledger_charge_bytes().unwrap() <= BASE_LEDGER_BYTES);
    let objects = [object(5, b"first"), object(5, b"second")];
    let ids = objects
        .iter()
        .map(|bytes| typed_id(bytes))
        .collect::<Vec<_>>();
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
    let mut spool = Spool::default();
    let admission = cas
        .admit_pack(&mut pack, &mut spool, &ledger, &mut counters, &mut scratch)
        .unwrap();

    assert_eq!(admission.outcome(), FsPackAdmissionOutcomeV1::Installed);
    assert_eq!(admission.sealed().pack_len(), 432);
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        1
    );
    assert_eq!(counters.fscas_bytes_written, 0);
    assert!(counters.fscas_bytes_read > 0);
    assert!(counters.fscas_read_calls > 0);
    assert_eq!(counters.fscas_catalog_operations, 1);
    assert_eq!(
        counters.installed_carrier_logical_bytes,
        admission.sealed().pack_len()
    );
    assert!(counters.has_zero_forbidden_work());
    assert_eq!(ledger.admitted_slots(), 0);

    let reopened = FsCasV1::open_existing(&fixture.path).unwrap();
    let mut occupied = reopened.occupied().unwrap();
    for (id, expected) in ids.into_iter().zip(&objects) {
        assert_eq!(
            occupied.occupied_len(id).unwrap(),
            Some(expected.len() as u64)
        );
        let mut actual = vec![0_u8; expected.len()];
        for (offset, block) in actual.chunks_mut(7).enumerate() {
            occupied
                .read_occupied_exact_at(id, (offset * 7) as u64, block)
                .unwrap();
        }
        assert_eq!(actual, *expected);
    }
    let (bytes_read, read_calls) = occupied.direct_storage_read_observation().unwrap();
    assert!(read_calls > 0);
    assert!(bytes_read >= objects.iter().map(|bytes| bytes.len() as u64).sum());
}

#[test]
fn occupied_locator_catalog_observation_overflow_is_typed_and_transactional() {
    let fixture = TestRoot::new("occupied-metadata-observation-overflow");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let shared = object(5, b"occupied-metadata-observation");
    let shared_id = typed_id(&shared);
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(
        &cas,
        std::slice::from_ref(&shared),
        &ledger,
        &mut counters,
        &mut scratch,
    );
    let mut spool = Spool::default();
    cas.admit_pack(&mut pack, &mut spool, &ledger, &mut counters, &mut scratch)
        .unwrap();
    assert_eq!(ledger.admitted_slots(), 0);
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );

    const SEEDED_BYTES: u64 = 37;
    const SEEDED_CALLS: u64 = u64::MAX - 1;
    cas.seed_next_occupied_read_observation_for_test_v1(SEEDED_BYTES, SEEDED_CALLS);
    let mut occupied = cas.occupied_private_v1().unwrap();
    assert_eq!(
        occupied.occupied_len_typed_v1(shared_id),
        Err(FsCasErrorV1::Core(CoreError::IntegerOverflow))
    );
    assert_eq!(
        occupied.direct_storage_read_observation_typed_v1(),
        Ok((SEEDED_BYTES, SEEDED_CALLS))
    );
    assert!(!occupied.resolved_object_cached_for_test_v1(shared_id));

    // Let the locator+catalog tuple commit, then overflow only when the real
    // validated pack tuple is merged. Its bytes and calls are indivisible.
    const PACK_SEEDED_BYTES: u64 = 53;
    const PACK_SEEDED_CALLS: u64 = u64::MAX - 2;
    cas.seed_next_occupied_read_observation_for_test_v1(PACK_SEEDED_BYTES, PACK_SEEDED_CALLS);
    let mut pack_overflow = cas.occupied_private_v1().unwrap();
    assert_eq!(
        pack_overflow.occupied_len_typed_v1(shared_id),
        Err(FsCasErrorV1::Core(CoreError::IntegerOverflow))
    );
    assert_eq!(
        pack_overflow.direct_storage_read_observation_typed_v1(),
        Ok((
            PACK_SEEDED_BYTES
                + u64::try_from(PERSISTENT_LOCATOR_BYTES_V1).unwrap()
                + u64::try_from(CATALOG_MARKER_BYTES).unwrap(),
            u64::MAX,
        ))
    );
    assert!(!pack_overflow.resolved_object_cached_for_test_v1(shared_id));

    // Resolve the authenticated object normally, then saturate only the
    // observation state immediately before a real payload read. The bytes
    // reach the caller, but the payload bytes+call tuple is rejected whole.
    const PAYLOAD_SEEDED_BYTES: u64 = 71;
    cas.seed_next_occupied_payload_read_observation_for_test_v1(PAYLOAD_SEEDED_BYTES, u64::MAX);
    let mut payload_overflow = cas.occupied_private_v1().unwrap();
    assert_eq!(
        payload_overflow.occupied_len_typed_v1(shared_id),
        Ok(Some(shared.len() as u64))
    );
    assert!(payload_overflow.resolved_object_cached_for_test_v1(shared_id));
    let mut payload_prefix = [0_u8; 11];
    assert_eq!(
        payload_overflow.read_occupied_exact_at_typed_v1(shared_id, 0, &mut payload_prefix,),
        Err(FsCasErrorV1::Core(CoreError::IntegerOverflow))
    );
    assert_eq!(payload_prefix, shared[..payload_prefix.len()]);
    assert_eq!(
        payload_overflow.direct_storage_read_observation_typed_v1(),
        Ok((PAYLOAD_SEEDED_BYTES, u64::MAX))
    );
    assert!(payload_overflow.resolved_object_cached_for_test_v1(shared_id));

    // The failed observation commit is not a storage-integrity failure. The
    // current handle and an independently reopened handle remain usable.
    assert!(cas.occupied_private_v1().is_ok());
    assert!(FsCasV1::open_existing(&fixture.path).is_ok());
    assert_eq!(ledger.admitted_slots(), 0);
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn valid_locator_binding_mismatches_are_integrity_not_malformed_bytes() {
    for catalog_binding in [true, false] {
        let label = if catalog_binding {
            "locator-catalog-binding"
        } else {
            "locator-entry-binding"
        };
        let fixture = TestRoot::new(label);
        let cas = FsCasV1::create_new(&fixture.path).unwrap();
        let shared = object(5, b"authenticated-locator-binding");
        let shared_id = typed_id(&shared);
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut counters = OperationCountersV1::default();
        let mut scratch = [0_u8; 65_536];
        let mut spool = Spool::default();
        let mut winner = build_private_pack(
            &cas,
            std::slice::from_ref(&shared),
            &ledger,
            &mut counters,
            &mut scratch,
        );
        cas.admit_pack(
            &mut winner,
            &mut spool,
            &ledger,
            &mut counters,
            &mut scratch,
        )
        .unwrap();

        // Keep the locator structurally valid while changing one decoded
        // authenticated binding. A catalog-shape mismatch and a canonical
        // pack-entry mismatch are integrity failures, not malformed records.
        let path = locator_path(&fixture.path, shared_id);
        let original_permissions = make_owner_writable(&path);
        let mut locator = fs::read(&path).unwrap();
        if catalog_binding {
            let pack_len = u64::from_be_bytes(locator[80..88].try_into().unwrap());
            locator[80..88].copy_from_slice(&pack_len.checked_add(1).unwrap().to_be_bytes());
        } else {
            let object_len = u32::from_be_bytes(locator[112..116].try_into().unwrap());
            locator[112..116].copy_from_slice(&object_len.checked_add(1).unwrap().to_be_bytes());
        }
        fs::write(&path, locator).unwrap();
        fs::set_permissions(&path, original_permissions).unwrap();

        let mut occupied = cas.occupied_private_v1().unwrap();
        assert_eq!(
            occupied.occupied_len_typed_v1(shared_id),
            Err(FsCasErrorV1::Integrity),
            "read path: {label}"
        );

        // A distinct candidate carrier containing the same object exercises
        // incumbent locator authentication during publication as well.
        let candidate_objects = [shared, object(5, b"new-candidate-object")];
        let mut candidate_counters = OperationCountersV1::default();
        let mut candidate = build_private_pack(
            &cas,
            &candidate_objects,
            &ledger,
            &mut candidate_counters,
            &mut scratch,
        );
        assert_eq!(
            cas.admit_pack(
                &mut candidate,
                &mut spool,
                &ledger,
                &mut candidate_counters,
                &mut scratch,
            ),
            Err(FsCasErrorV1::Integrity),
            "admission path: {label}"
        );
        assert_eq!(ledger.admitted_slots(), 0, "{label}");
        assert!(candidate_counters.has_zero_forbidden_work(), "{label}");
    }

    // Equal-carrier reuse has a separate persistent-locator verification
    // path; prove that it uses the same valid-record integrity taxonomy.
    let fixture = TestRoot::new("locator-equal-carrier-binding");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let shared = object(5, b"equal-carrier-locator-binding");
    let shared_id = typed_id(&shared);
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let mut spool = Spool::default();
    let mut winner = build_private_pack(
        &cas,
        std::slice::from_ref(&shared),
        &ledger,
        &mut counters,
        &mut scratch,
    );
    cas.admit_pack(
        &mut winner,
        &mut spool,
        &ledger,
        &mut counters,
        &mut scratch,
    )
    .unwrap();
    let path = locator_path(&fixture.path, shared_id);
    let original_permissions = make_owner_writable(&path);
    let mut locator = fs::read(&path).unwrap();
    let object_len = u32::from_be_bytes(locator[112..116].try_into().unwrap());
    locator[112..116].copy_from_slice(&object_len.checked_add(1).unwrap().to_be_bytes());
    fs::write(&path, locator).unwrap();
    fs::set_permissions(&path, original_permissions).unwrap();

    let mut reuse_counters = OperationCountersV1::default();
    let mut candidate = build_private_pack(
        &cas,
        std::slice::from_ref(&shared),
        &ledger,
        &mut reuse_counters,
        &mut scratch,
    );
    assert_eq!(
        cas.admit_pack(
            &mut candidate,
            &mut spool,
            &ledger,
            &mut reuse_counters,
            &mut scratch,
        ),
        Err(FsCasErrorV1::Integrity)
    );
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(reuse_counters.has_zero_forbidden_work());
}

#[test]
fn catalog_counter_overflow_precedes_every_visibility_transition() {
    let fixture = TestRoot::new("catalog-counter-overflow");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [object(5, b"counter-overflow")];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
    counters.fscas_catalog_operations = u64::MAX;
    let mut spool = Spool::default();

    assert_eq!(
        cas.admit_pack(&mut pack, &mut spool, &ledger, &mut counters, &mut scratch,),
        Err(FsCasErrorV1::Core(CoreError::IntegerOverflow))
    );
    assert_eq!(counters.fscas_catalog_operations, u64::MAX);
    assert_eq!(counters.unreachable_installed_residue_bytes, 0);
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("objects")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("closures")).unwrap().count(),
        0
    );
    assert_eq!(ledger.admitted_slots(), 0);
    assert_eq!(counters.fscas_bytes_written, 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn carrier_cleanup_failure_invalidates_owner_and_root() {
    let fixture = TestRoot::new("carrier-cleanup-failure");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [object(5, b"carrier-cleanup")];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let mut stale_occupied = cas.occupied().unwrap();
    let mut stale_private = cas.begin_private_pack().unwrap();
    stale_private.begin_private(1).unwrap();
    let mut stale_operation = cas.begin_closure_operation().unwrap();
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
    let pack_len = pack.len().unwrap();
    let mut spool = Spool::default();
    let mut control = StopWithCleanupFailure::new(
        FsCasBoundaryV1::AfterCarrierInstall,
        FsCasCleanupTargetV1::Carrier,
    );

    assert_eq!(
        cas.admit_pack_controlled(
            &mut pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut scratch,
            &mut control,
        ),
        Err(FsCasErrorV1::TerminalFailure {
            first: FsCasFailureCauseV1::Core(CoreError::Cancelled),
            dominant: FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::Carrier,),
        })
    );
    assert!(control.injected);
    assert_eq!(counters.unreachable_installed_residue_bytes, pack_len);
    assert_eq!(stale_private.append(b"x"), Err(PackPortErrorV1::Failure));
    drop(stale_private);
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("objects")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        0
    );
    assert!(fixture.path.join("invalidated").is_dir());
    assert_eq!(
        stale_occupied.occupied_len(typed_id(&objects[0])),
        Err(ImmutablePortErrorV1::Failure)
    );
    let no_objects = [];
    let mut stale_closure = ClosureSource {
        objects: &no_objects,
    };
    let mut incoming_comparison = [0_u8; 65_536];
    let mut occupied_comparison = [0_u8; 65_536];
    let mut source_window = [0_u8; 32_768];
    let mut cdc_ring = [0_u8; 32_768];
    let mut traversal = [0_u8; 1];
    let (_, _, version_id, _) = empty_closure();
    assert_eq!(
        cas.admit_complete_closure(
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
        ),
        Err(CoreError::SinkRefused)
    );
    assert!(matches!(
        cas.begin_private_pack(),
        Err(FsCasErrorV1::Invalidated)
    ));
    assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(
        cas.begin_closure_operation(),
        Err(FsCasErrorV1::Invalidated)
    ));
    assert!(matches!(
        FsCasV1::open_existing(&fixture.path),
        Err(FsCasErrorV1::Invalidated)
    ));
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn rollback_carrier_authentication_failure_preserves_cleanup_dominance() {
    let fixture = TestRoot::new("rollback-carrier-authentication");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [object(5, b"rollback carrier authentication")];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
    let pack_len = pack.len().unwrap();
    let mut spool = Spool::default();
    let mut control = CorruptCarrierBeforeRollback {
        root: fixture.path.clone(),
        injected: false,
    };

    assert_eq!(
        cas.admit_pack_controlled(
            &mut pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut scratch,
            &mut control,
        ),
        Err(FsCasErrorV1::TerminalFailure {
            first: FsCasFailureCauseV1::Core(CoreError::Cancelled),
            dominant: FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::ObjectLocator,),
        })
    );
    assert!(control.injected);
    assert_eq!(
        counters.unreachable_installed_residue_bytes,
        pack_len + u64::try_from(PERSISTENT_LOCATOR_BYTES_V1).unwrap(),
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("objects")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        0
    );
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
    assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(
        FsCasV1::open_existing(&fixture.path),
        Err(FsCasErrorV1::Invalidated)
    ));
}

#[test]
fn locator_cleanup_failure_is_counted_and_cannot_poison_a_later_admission() {
    let fixture = TestRoot::new("locator-cleanup-failure");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [
        object(5, b"locator-cleanup-a"),
        object(5, b"locator-cleanup-b"),
    ];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
    let pack_len = pack.len().unwrap();
    let mut spool = Spool::default();
    let mut control = StopWithCleanupFailure::new(
        FsCasBoundaryV1::AfterObjectLocatorPublication,
        FsCasCleanupTargetV1::ObjectLocator,
    );

    assert_eq!(
        cas.admit_pack_controlled(
            &mut pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut scratch,
            &mut control,
        ),
        Err(FsCasErrorV1::TerminalFailure {
            first: FsCasFailureCauseV1::Core(CoreError::Cancelled),
            dominant: FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::ObjectLocator,),
        })
    );
    assert!(control.injected);
    assert_eq!(
        counters.unreachable_installed_residue_bytes,
        pack_len + u64::try_from(PERSISTENT_LOCATOR_BYTES_V1).unwrap(),
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("objects")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        0
    );
    assert!(fixture.path.join("invalidated").is_dir());
    assert!(matches!(
        cas.begin_private_pack(),
        Err(FsCasErrorV1::Invalidated)
    ));
    assert!(matches!(
        FsCasV1::open_existing(&fixture.path),
        Err(FsCasErrorV1::Invalidated)
    ));
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn overlapping_packs_reuse_one_object_without_poisoning_lookup() {
    let fixture = TestRoot::new("overlapping-packs");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let (version, root, version_id, root_id) = empty_closure();
    let extra = object(5, b"pack-b-only");
    let extra_id = typed_id(&extra);
    let pack_a_objects = [version.clone(), root.clone()];
    let pack_b_objects = [version.clone(), extra.clone()];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let mut spool = Spool::default();

    let mut canonical_shared_locator = None;
    for (index, objects) in [&pack_a_objects[..], &pack_b_objects[..]]
        .into_iter()
        .enumerate()
    {
        let mut pack = build_private_pack(&cas, objects, &ledger, &mut counters, &mut scratch);
        assert_eq!(
            cas.admit_pack(&mut pack, &mut spool, &ledger, &mut counters, &mut scratch,)
                .unwrap()
                .outcome(),
            FsPackAdmissionOutcomeV1::Installed
        );
        let observed = fs::read(locator_path(&fixture.path, version_id)).unwrap();
        if index == 0 {
            canonical_shared_locator = Some(observed);
        } else {
            assert_eq!(Some(observed), canonical_shared_locator);
        }
    }
    assert_eq!(
        fs::read_dir(fixture.path.join("objects")).unwrap().count(),
        3
    );

    let mut occupied = cas.occupied().unwrap();
    for (id, expected) in [(version_id, &version), (root_id, &root), (extra_id, &extra)] {
        assert_eq!(
            occupied.occupied_len(id).unwrap(),
            Some(expected.len() as u64)
        );
        let mut actual = vec![0_u8; expected.len()];
        occupied.read_occupied_exact_at(id, 0, &mut actual).unwrap();
        assert_eq!(actual, *expected);
    }

    let closure_objects = [(version_id, version), (root_id, root)];
    let mut closure = ClosureSource {
        objects: &closure_objects,
    };
    let mut operation = cas.begin_closure_operation().unwrap();
    let mut incoming_comparison = [0_u8; 65_536];
    let mut occupied_comparison = [0_u8; 65_536];
    let mut source_window = [0_u8; 32_768];
    let mut cdc_ring = [0_u8; 32_768];
    let mut traversal = [0_u8; 1];
    assert!(cas
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
        .is_ok());
}

#[test]
fn overlapping_pack_incumbent_comparison_holds_neither_root_fence() {
    let fixture = TestRoot::new("overlapping-pack-lock-scope");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let shared = object(5, &[0x4d; 16_384]);
    let additional = object(5, &[0x9e; 16_384]);
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut scratch = [0_u8; 65_536];
    let mut spool = Spool::default();

    let mut incumbent_counters = OperationCountersV1::default();
    let mut incumbent = build_private_pack(
        &cas,
        std::slice::from_ref(&shared),
        &ledger,
        &mut incumbent_counters,
        &mut scratch,
    );
    cas.admit_pack(
        &mut incumbent,
        &mut spool,
        &ledger,
        &mut incumbent_counters,
        &mut scratch,
    )
    .unwrap();

    let mut candidate_counters = OperationCountersV1::default();
    let mut candidate = build_private_pack(
        &cas,
        &[shared, additional],
        &ledger,
        &mut candidate_counters,
        &mut scratch,
    );
    let mut control = ObserveIncumbentComparisonLock {
        cas: cas.clone(),
        observed: false,
        visibility_available: false,
        publication_available: false,
    };
    assert_eq!(
        cas.admit_pack_controlled(
            &mut candidate,
            &mut spool,
            &ledger,
            &mut candidate_counters,
            &mut scratch,
            &mut control,
        )
        .unwrap()
        .outcome(),
        FsPackAdmissionOutcomeV1::Installed
    );
    assert!(control.observed);
    assert!(control.visibility_available);
    assert!(control.publication_available);
}

#[test]
fn nonexistent_objects_cannot_mint_a_closure_capability() {
    let fixture = TestRoot::new("closure-spoof-regression");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let (version, root, version_id, root_id) = empty_closure();
    let closure_objects = [(version_id, version), (root_id, root)];
    let mut closure = ClosureSource {
        objects: &closure_objects,
    };
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut operation = cas.begin_closure_operation().unwrap();
    let mut incoming_comparison = [0_u8; 65_536];
    let mut occupied_comparison = [0_u8; 65_536];
    let mut source_window = [0_u8; 32_768];
    let mut cdc_ring = [0_u8; 32_768];
    let mut traversal = [0_u8; 1];
    assert_eq!(
        cas.admit_complete_closure(
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
        ),
        Err(CoreError::SinkRefused)
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("closures")).unwrap().count(),
        0
    );
    assert_eq!(counters.closure_fences, 0);
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn spoofed_closure_bytes_cannot_mint_a_capability() {
    let fixture = TestRoot::new("closure-spoofed-bytes");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let (version, root, version_id, root_id) = empty_closure();
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut pack_scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(
        &cas,
        &[version.clone(), root.clone()],
        &ledger,
        &mut counters,
        &mut pack_scratch,
    );
    let mut spool = Spool::default();
    cas.admit_pack(
        &mut pack,
        &mut spool,
        &ledger,
        &mut counters,
        &mut pack_scratch,
    )
    .unwrap();

    let mut spoofed_version = version;
    spoofed_version[52] ^= 1;
    let closure_objects = [(version_id, spoofed_version), (root_id, root)];
    let mut closure = ClosureSource {
        objects: &closure_objects,
    };
    let mut operation = cas.begin_closure_operation().unwrap();
    let mut incoming_comparison = [0_u8; 65_536];
    let mut occupied_comparison = [0_u8; 65_536];
    let mut source_window = [0_u8; 32_768];
    let mut cdc_ring = [0_u8; 32_768];
    let mut traversal = [0_u8; 1];
    assert_eq!(
        cas.admit_complete_closure(
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
        ),
        Err(CoreError::IdMismatch)
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("closures")).unwrap().count(),
        0
    );
    assert_eq!(counters.closure_fences, 0);
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn duplicate_typed_ids_cannot_enter_the_closure_transcript() {
    let fixture = TestRoot::new("closure-duplicate-id");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let (version, root, version_id, root_id) = empty_closure();
    let closure_objects = [
        (version_id, version.clone()),
        (version_id, version),
        (root_id, root),
    ];
    let mut closure = ClosureSource {
        objects: &closure_objects,
    };
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut operation = cas.begin_closure_operation().unwrap();
    let mut incoming_comparison = [0_u8; 65_536];
    let mut occupied_comparison = [0_u8; 65_536];
    let mut source_window = [0_u8; 32_768];
    let mut cdc_ring = [0_u8; 32_768];
    let mut traversal = [0_u8; 1];
    assert_eq!(
        cas.admit_complete_closure(
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
        ),
        Err(CoreError::NonCanonicalOrder)
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("closures")).unwrap().count(),
        0
    );
    assert_eq!(counters.closure_fences, 0);
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn wrong_version_record_cannot_mint_a_closure_capability() {
    let fixture = TestRoot::new("closure-wrong-version");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let (version, root, version_id, root_id) = empty_closure();
    let wrong_version = TypedPhysicalObjectIdV1::VersionRecord(
        derive_physical_version_record_id_v1(&object(1, b"wrong-version")).unwrap(),
    );
    let closure_objects = [(version_id, version.clone()), (root_id, root.clone())];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut pack_scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(
        &cas,
        &[version, root],
        &ledger,
        &mut counters,
        &mut pack_scratch,
    );
    let mut spool = Spool::default();
    cas.admit_pack(
        &mut pack,
        &mut spool,
        &ledger,
        &mut counters,
        &mut pack_scratch,
    )
    .unwrap();

    let mut closure = ClosureSource {
        objects: &closure_objects,
    };
    let mut operation = cas.begin_closure_operation().unwrap();
    let mut incoming_comparison = [0_u8; 65_536];
    let mut occupied_comparison = [0_u8; 65_536];
    let mut source_window = [0_u8; 32_768];
    let mut cdc_ring = [0_u8; 32_768];
    let mut traversal = [0_u8; 1];
    assert_eq!(
        cas.admit_complete_closure(
            &mut operation,
            &mut closure,
            wrong_version,
            &ledger,
            &mut counters,
            AdmissionBuffersV1::new(
                &mut incoming_comparison,
                &mut occupied_comparison,
                &mut source_window,
                &mut cdc_ring,
                &mut traversal,
            ),
        ),
        Err(CoreError::MissingClosureEdge)
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("closures")).unwrap().count(),
        0
    );
    assert_eq!(counters.closure_fences, 0);
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn forced_equal_typed_id_with_unequal_incumbent_bytes_fails_closed() {
    let fixture = TestRoot::new("forced-object-id-collision");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let shared = object(5, &[0x71; 4096]);
    let shared_id = typed_id(&shared);
    let winner_only = object(5, b"winner-only");
    let loser_only = object(5, b"loser-only");
    let loser_only_id = typed_id(&loser_only);
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut scratch = [0_u8; 65_536];
    let mut spool = Spool::default();
    let mut winner_counters = OperationCountersV1::default();
    let mut winner = build_private_pack(
        &cas,
        &[shared.clone(), winner_only],
        &ledger,
        &mut winner_counters,
        &mut scratch,
    );
    cas.admit_pack(
        &mut winner,
        &mut spool,
        &ledger,
        &mut winner_counters,
        &mut scratch,
    )
    .unwrap();

    // Preserve the incumbent locator's claimed typed ID while changing one
    // byte in the canonical object it names. A new valid pack with the same
    // object ID must validate those incumbent bytes completely and fail
    // closed, rather than publishing a second locator.
    let marker = fs::read(locator_path(&fixture.path, shared_id)).unwrap();
    let object_offset = u64::from_be_bytes(marker[104..112].try_into().unwrap())
        .checked_add(4)
        .unwrap();
    let object_len = u32::from_be_bytes(marker[112..116].try_into().unwrap());
    let carrier = only_entry(&fixture.path.join("carriers"));
    let original_permissions = make_owner_writable(&carrier);
    let mut bytes = fs::read(&carrier).unwrap();
    let corrupt_at = usize::try_from(object_offset + u64::from(object_len) - 1).unwrap();
    bytes[corrupt_at] ^= 0xff;
    fs::write(&carrier, bytes).unwrap();
    fs::set_permissions(&carrier, original_permissions).unwrap();

    let mut loser_counters = OperationCountersV1::default();
    let mut loser = build_private_pack(
        &cas,
        &[shared, loser_only],
        &ledger,
        &mut loser_counters,
        &mut scratch,
    );
    assert_eq!(
        cas.admit_pack(
            &mut loser,
            &mut spool,
            &ledger,
            &mut loser_counters,
            &mut scratch,
        ),
        Err(FsCasErrorV1::Core(CoreError::IdMismatch))
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("objects")).unwrap().count(),
        2
    );
    assert_path_absent(&locator_path(&fixture.path, loser_only_id));
    assert_eq!(loser_counters.unreachable_installed_residue_bytes, 0);
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(loser_counters.has_zero_forbidden_work());
}

#[test]
fn cancellation_during_shared_object_validation_removes_only_the_loser() {
    let fixture = TestRoot::new("overlap-cancel");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let (version, root, version_id, _) = empty_closure();
    let extra = object(5, b"loser-only");
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut scratch = [0_u8; 65_536];
    let mut spool = Spool::default();
    let mut winner_counters = OperationCountersV1::default();
    let mut winner = build_private_pack(
        &cas,
        &[version.clone(), root],
        &ledger,
        &mut winner_counters,
        &mut scratch,
    );
    cas.admit_pack(
        &mut winner,
        &mut spool,
        &ledger,
        &mut winner_counters,
        &mut scratch,
    )
    .unwrap();

    let mut loser_counters = OperationCountersV1::default();
    let mut loser = build_private_pack(
        &cas,
        &[version, extra],
        &ledger,
        &mut loser_counters,
        &mut scratch,
    );
    let mut control = StopAtBoundary::new(
        StopKind::Cancellation,
        FsCasBoundaryV1::AfterObjectComparisonWindow,
    );
    assert_eq!(
        cas.admit_pack_controlled(
            &mut loser,
            &mut spool,
            &ledger,
            &mut loser_counters,
            &mut scratch,
            &mut control,
        ),
        Err(FsCasErrorV1::Core(CoreError::Cancelled))
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("objects")).unwrap().count(),
        2
    );
    assert!(locator_path(&fixture.path, version_id).is_file());
    assert_eq!(loser_counters.unreachable_installed_residue_bytes, 0);
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(loser_counters.has_zero_forbidden_work());
}

#[test]
fn malformed_object_locator_fails_closed_without_publishing_the_loser() {
    let fixture = TestRoot::new("malformed-object-locator");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let (version, root, version_id, _) = empty_closure();
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut scratch = [0_u8; 65_536];
    let mut spool = Spool::default();
    let mut counters = OperationCountersV1::default();
    let mut winner = build_private_pack(
        &cas,
        &[version.clone(), root],
        &ledger,
        &mut counters,
        &mut scratch,
    );
    cas.admit_pack(
        &mut winner,
        &mut spool,
        &ledger,
        &mut counters,
        &mut scratch,
    )
    .unwrap();
    let locator = locator_path(&fixture.path, version_id);
    let original_permissions = make_owner_writable(&locator);
    fs::write(&locator, b"truncated").unwrap();
    fs::set_permissions(&locator, original_permissions).unwrap();

    let mut loser_counters = OperationCountersV1::default();
    let mut loser = build_private_pack(
        &cas,
        &[version, object(5, b"loser")],
        &ledger,
        &mut loser_counters,
        &mut scratch,
    );
    assert_eq!(
        cas.admit_pack(
            &mut loser,
            &mut spool,
            &ledger,
            &mut loser_counters,
            &mut scratch,
        ),
        Err(FsCasErrorV1::MalformedOccupant)
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        1
    );
    assert_eq!(loser_counters.unreachable_installed_residue_bytes, 0);
    assert!(fixture.path.join("invalidated").is_dir());
    assert!(matches!(
        cas.begin_private_pack(),
        Err(FsCasErrorV1::Invalidated)
    ));
    assert!(matches!(
        FsCasV1::open_existing(&fixture.path),
        Err(FsCasErrorV1::Invalidated)
    ));
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(loser_counters.has_zero_forbidden_work());
}

#[cfg(unix)]
#[test]
fn post_comparison_locator_path_replacement_fails_before_catalog_publication() {
    let fixture = TestRoot::new("post-comparison-locator-replacement");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let stale = FsCasV1::open_existing(&fixture.path).unwrap();
    let shared = object(5, b"shared-complete-object");
    let winner_only = object(5, b"winner-only-object");
    let candidate_only = object(5, b"candidate-only-object");
    let shared_locator = locator_path(&fixture.path, typed_id(&shared));
    let displaced = fixture.path.join("displaced-shared-locator");
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut scratch = [0_u8; 65_536];
    let mut spool = Spool::default();

    let mut winner_counters = OperationCountersV1::default();
    let mut winner = build_private_pack(
        &cas,
        &[shared.clone(), winner_only],
        &ledger,
        &mut winner_counters,
        &mut scratch,
    );
    cas.admit_pack(
        &mut winner,
        &mut spool,
        &ledger,
        &mut winner_counters,
        &mut scratch,
    )
    .unwrap();

    let mut candidate_counters = OperationCountersV1::default();
    let mut candidate = build_private_pack(
        &cas,
        &[shared, candidate_only],
        &ledger,
        &mut candidate_counters,
        &mut scratch,
    );
    let mut control = ReplaceLocatorAfterCompleteComparison {
        locator: shared_locator.clone(),
        displaced: displaced.clone(),
        injected: false,
    };
    assert_eq!(
        cas.admit_pack_controlled(
            &mut candidate,
            &mut spool,
            &ledger,
            &mut candidate_counters,
            &mut scratch,
            &mut control,
        ),
        Err(FsCasErrorV1::Integrity)
    );
    assert!(control.injected);
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(ledger.admitted_slots(), 0);
    assert_storage_equations(&candidate_counters);
    assert_eq!(candidate_counters.storage_bytes_retained, 0);
    assert_eq!(candidate_counters.storage_inodes_retained, 0);
    assert!(candidate_counters.has_zero_forbidden_work());

    fs::remove_file(&shared_locator).unwrap();
    fs::rename(&displaced, &shared_locator).unwrap();
    assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(
        FsCasV1::open_existing(&fixture.path),
        Err(FsCasErrorV1::Invalidated)
    ));
}

#[test]
fn atomic_locator_no_replace_authenticates_a_racing_malformed_occupant() {
    let fixture = TestRoot::new("atomic-locator-race");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [object(5, b"atomic-no-replace")];
    let id = typed_id(&objects[0]);
    let locator = locator_path(&fixture.path, id);
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
    let mut spool = Spool::default();
    let mut control = InstallMalformedLocatorAtPublication {
        locator: locator.clone(),
        injected: false,
    };

    assert_eq!(
        cas.admit_pack_controlled(
            &mut pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut scratch,
            &mut control,
        ),
        Err(FsCasErrorV1::MalformedOccupant)
    );
    assert!(control.injected);
    assert_eq!(fs::read(&locator).unwrap(), [0_u8; 160]);
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("objects")).unwrap().count(),
        1
    );
    assert!(fixture.path.join("invalidated").is_dir());
    assert!(matches!(
        FsCasV1::open_existing(&fixture.path),
        Err(FsCasErrorV1::Invalidated)
    ));
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn atomic_locator_incumbent_cleanup_failure_preserves_typed_lifecycle_error() {
    let fixture = TestRoot::new("atomic-locator-cleanup-failure");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let stale = FsCasV1::open_existing(&fixture.path).unwrap();
    let objects = [object(5, b"atomic-cleanup")];
    let locator = locator_path(&fixture.path, typed_id(&objects[0]));
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
    let mut spool = Spool::default();
    let mut control = InstallLocatorAndFailPreparationCleanup {
        locator,
        occupant_injected: false,
        cleanup_injected: false,
    };

    assert_eq!(
        cas.admit_pack_controlled(
            &mut pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut scratch,
            &mut control,
        ),
        Err(FsCasErrorV1::TerminalFailure {
            first: FsCasFailureCauseV1::MalformedOccupant,
            dominant: FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool,),
        })
    );
    assert!(control.occupant_injected);
    assert!(control.cleanup_injected);
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("objects")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        0
    );
    assert_eq!(counters.unreachable_installed_residue_bytes, 0);
    assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(
        FsCasV1::open_existing(&fixture.path),
        Err(FsCasErrorV1::Invalidated)
    ));
    assert_eq!(ledger.admitted_slots(), 0);
}

#[test]
fn atomic_catalog_no_replace_authenticates_a_racing_malformed_occupant() {
    let fixture = TestRoot::new("atomic-catalog-race");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [object(5, b"atomic-catalog")];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
    let mut spool = Spool::default();
    let mut control = InstallMalformedCatalogAtPublication {
        root: fixture.path.clone(),
        injected: false,
    };

    assert_eq!(
        cas.admit_pack_controlled(
            &mut pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut scratch,
            &mut control,
        ),
        Err(FsCasErrorV1::MalformedOccupant)
    );
    assert!(control.injected);
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("objects")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read(only_entry(&fixture.path.join("catalog"))).unwrap(),
        [0_u8; 64]
    );
    assert_eq!(counters.unreachable_installed_residue_bytes, 0);
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn atomic_catalog_no_replace_classifies_valid_binding_and_unequal_incumbents() {
    let donor_fixture = TestRoot::new("catalog-unequal-donor");
    let donor = FsCasV1::create_new(&donor_fixture.path).unwrap();
    let donor_objects = [object(5, b"canonical unequal catalog donor")];
    let donor_ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut donor_counters = OperationCountersV1::default();
    let mut donor_scratch = [0_u8; 65_536];
    let mut donor_pack = build_private_pack(
        &donor,
        &donor_objects,
        &donor_ledger,
        &mut donor_counters,
        &mut donor_scratch,
    );
    let mut donor_spool = Spool::default();
    assert_eq!(
        donor
            .admit_pack(
                &mut donor_pack,
                &mut donor_spool,
                &donor_ledger,
                &mut donor_counters,
                &mut donor_scratch,
            )
            .unwrap()
            .outcome(),
        FsPackAdmissionOutcomeV1::Installed
    );
    let unequal_catalog = fs::read(only_entry(&donor_fixture.path.join("catalog"))).unwrap();
    assert_eq!(unequal_catalog.len(), CATALOG_MARKER_BYTES);

    for (label, bind_candidate_id, expected) in [
        ("binding", false, FsCasErrorV1::Integrity),
        ("same-id-unequal", true, FsCasErrorV1::UnequalOccupant),
    ] {
        let fixture = TestRoot::new(&format!("atomic-catalog-{label}"));
        let cas = FsCasV1::create_new(&fixture.path).unwrap();
        let objects = [object(5, b"candidate with a distinct sealed pack")];
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut counters = OperationCountersV1::default();
        let mut scratch = [0_u8; 65_536];
        let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
        assert_ne!(
            u64::from_be_bytes(unequal_catalog[40..48].try_into().unwrap()),
            pack.len().unwrap(),
        );
        let mut spool = Spool::default();
        let mut control = InstallUnequalCatalogAtPublication {
            root: fixture.path.clone(),
            bytes: unequal_catalog.clone(),
            bind_candidate_id,
            injected: false,
        };

        assert_eq!(
            cas.admit_pack_controlled(
                &mut pack,
                &mut spool,
                &ledger,
                &mut counters,
                &mut scratch,
                &mut control,
            ),
            Err(expected),
            "{label}"
        );
        assert!(control.injected, "{label}");
        assert_eq!(
            fs::read_dir(fixture.path.join("preparation"))
                .unwrap()
                .count(),
            0,
            "{label}"
        );
        assert_eq!(
            fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
            0,
            "{label}"
        );
        assert_eq!(
            fs::read_dir(fixture.path.join("objects")).unwrap().count(),
            0,
            "{label}"
        );
        assert_eq!(
            fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
            1,
            "{label}"
        );
        assert_eq!(
            fs::read(only_entry(&fixture.path.join("catalog"))).unwrap(),
            control.bytes,
            "{label}"
        );
        assert_eq!(counters.unreachable_installed_residue_bytes, 0, "{label}");
        assert_eq!(ledger.admitted_slots(), 0, "{label}");
        assert!(counters.has_zero_forbidden_work(), "{label}");
    }
}

#[test]
fn existing_catalog_classifies_valid_binding_and_unequal_incumbents() {
    for (label, mutate_id, expected) in [
        ("binding", true, FsCasErrorV1::Integrity),
        ("same-id-unequal", false, FsCasErrorV1::UnequalOccupant),
    ] {
        let fixture = TestRoot::new(&format!("existing-catalog-{label}"));
        let cas = FsCasV1::create_new(&fixture.path).unwrap();
        let objects = [object(5, b"existing canonical catalog candidate")];
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut scratch = [0_u8; 65_536];
        let mut installed_counters = OperationCountersV1::default();
        let mut installed = build_private_pack(
            &cas,
            &objects,
            &ledger,
            &mut installed_counters,
            &mut scratch,
        );
        let mut spool = Spool::default();
        assert_eq!(
            cas.admit_pack(
                &mut installed,
                &mut spool,
                &ledger,
                &mut installed_counters,
                &mut scratch,
            )
            .unwrap()
            .outcome(),
            FsPackAdmissionOutcomeV1::Installed
        );

        let marker_path = only_entry(&fixture.path.join("catalog"));
        let original_permissions = make_owner_writable(&marker_path);
        let mut marker = fs::read(&marker_path).unwrap();
        if mutate_id {
            marker[8] ^= 1;
        } else {
            let pack_len = u64::from_be_bytes(marker[40..48].try_into().unwrap());
            marker[40..48].copy_from_slice(&pack_len.checked_add(1).unwrap().to_be_bytes());
        }
        fs::write(&marker_path, &marker).unwrap();
        fs::set_permissions(&marker_path, original_permissions).unwrap();

        let mut candidate_counters = OperationCountersV1::default();
        let mut candidate = build_private_pack(
            &cas,
            &objects,
            &ledger,
            &mut candidate_counters,
            &mut scratch,
        );
        assert_eq!(
            cas.admit_pack(
                &mut candidate,
                &mut spool,
                &ledger,
                &mut candidate_counters,
                &mut scratch,
            ),
            Err(expected),
            "{label}"
        );
        assert_eq!(
            fs::read_dir(fixture.path.join("preparation"))
                .unwrap()
                .count(),
            0,
            "{label}"
        );
        assert_eq!(
            fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
            1,
            "{label}"
        );
        assert_eq!(
            fs::read_dir(fixture.path.join("objects")).unwrap().count(),
            1,
            "{label}"
        );
        assert_eq!(
            fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
            1,
            "{label}"
        );
        assert_eq!(fs::read(&marker_path).unwrap(), marker, "{label}");
        assert_eq!(candidate_counters.unreachable_installed_residue_bytes, 0);
        assert_eq!(ledger.admitted_slots(), 0, "{label}");
        assert!(candidate_counters.has_zero_forbidden_work(), "{label}");
    }
}

#[test]
fn simultaneous_reopened_pack_callers_publish_one_canonical_shared_locator() {
    let fixture = TestRoot::new("overlap-race");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let left_cas = FsCasV1::open_existing(&fixture.path).unwrap();
    let right_cas = FsCasV1::open_existing(&fixture.path).unwrap();
    let shared = object(5, &[0x6d; 4096]);
    let left = object(5, b"left-only");
    let right = object(5, b"right-only");
    let shared_id = typed_id(&shared);
    let left_id = typed_id(&left);
    let right_id = typed_id(&right);
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut build_scratch = [0_u8; 65_536];
    let mut left_build = OperationCountersV1::default();
    let mut right_build = OperationCountersV1::default();
    let left_objects = [shared.clone(), left.clone()];
    let right_objects = [shared.clone(), right.clone()];
    let mut left_pack = build_private_pack(
        &left_cas,
        &left_objects,
        &ledger,
        &mut left_build,
        &mut build_scratch,
    );
    let mut right_pack = build_private_pack(
        &right_cas,
        &right_objects,
        &ledger,
        &mut right_build,
        &mut build_scratch,
    );
    let start = Arc::new(WatchdogGateV1::new());
    let (ready_tx, ready_rx) = mpsc::sync_channel(2);

    let (left_result, right_result) = std::thread::scope(|scope| {
        let mut start_release = WatchdogGateReleaseV1::new(Arc::clone(&start));
        let left_start = Arc::clone(&start);
        let left_ready = ready_tx.clone();
        let left_ledger = &ledger;
        let left_join = scope.spawn(move || {
            let mut spool = Spool::default();
            let mut scratch = [0_u8; 65_536];
            left_ready
                .send(())
                .expect("left readiness receiver remains live");
            left_start.wait();
            let admission = left_cas.admit_pack(
                &mut left_pack,
                &mut spool,
                left_ledger,
                &mut left_build,
                &mut scratch,
            );
            (admission, left_build)
        });
        let right_start = Arc::clone(&start);
        let right_ledger = &ledger;
        let right_join = scope.spawn(move || {
            let mut spool = Spool::default();
            let mut scratch = [0_u8; 65_536];
            ready_tx
                .send(())
                .expect("right readiness receiver remains live");
            right_start.wait();
            let admission = right_cas.admit_pack(
                &mut right_pack,
                &mut spool,
                right_ledger,
                &mut right_build,
                &mut scratch,
            );
            (admission, right_build)
        });
        for caller in 0..2 {
            ready_rx
                .recv_timeout(Duration::from_secs(5))
                .unwrap_or_else(|error| panic!("caller {caller} readiness failed: {error}"));
        }
        start_release.release_v1();
        (left_join.join().unwrap(), right_join.join().unwrap())
    });

    for (result, counters) in [left_result, right_result] {
        assert_eq!(
            result.unwrap().outcome(),
            FsPackAdmissionOutcomeV1::Installed
        );
        assert_eq!(counters.fscas_bytes_written, 0);
        assert!(counters.has_zero_forbidden_work());
    }
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        2
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        2
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("objects")).unwrap().count(),
        3
    );
    let mut occupied = cas.occupied().unwrap();
    for (id, expected) in [(shared_id, shared), (left_id, left), (right_id, right)] {
        assert_eq!(
            occupied.occupied_len(id).unwrap(),
            Some(expected.len() as u64)
        );
    }
    assert_eq!(ledger.admitted_slots(), 0);
}

#[test]
fn locator_owner_wait_is_direct_and_distinct_from_publication_mutex_wait() {
    let fixture = TestRoot::new("locator-owner-publication-wait");
    let seed = FsCasV1::create_new(&fixture.path).unwrap();
    let first_cas = FsCasV1::open_existing(&fixture.path).unwrap();
    let second_cas = FsCasV1::open_existing(&fixture.path).unwrap();
    let shared = object(5, &[0x6e; 4_096]);
    let second_only = object(5, b"second-pack-only");
    let shared_id = typed_id(&shared);
    let second_only_id = typed_id(&second_only);
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut build_scratch = [0_u8; 65_536];
    let mut first_counters = OperationCountersV1::default();
    let mut second_counters = OperationCountersV1::default();
    let mut first_pack = build_private_pack(
        &first_cas,
        std::slice::from_ref(&shared),
        &ledger,
        &mut first_counters,
        &mut build_scratch,
    );
    let mut second_pack = build_private_pack(
        &second_cas,
        &[shared.clone(), second_only.clone()],
        &ledger,
        &mut second_counters,
        &mut build_scratch,
    );
    let release = Arc::new(WatchdogGateV1::new());
    let (first_entered_tx, first_entered_rx) = mpsc::sync_channel(1);
    let (locator_wait_tx, locator_wait_rx) = mpsc::sync_channel(1);
    let (second_done_tx, second_done_rx) = mpsc::sync_channel(1);

    let (first_result, second_result) = std::thread::scope(|scope| {
        let mut release_guard = WatchdogGateReleaseV1::new(Arc::clone(&release));
        let first_release = Arc::clone(&release);
        let first = scope.spawn(|| {
            let mut spool = Spool::default();
            let mut scratch = [0_u8; 65_536];
            let mut control = BlockAfterObjectLocatorPublication {
                release: first_release,
                entered_signal: first_entered_tx,
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
            .expect("first pack did not expose its shared locator before catalog visibility");

        let second = scope.spawn(|| {
            let mut spool = Spool::default();
            let mut scratch = [0_u8; 65_536];
            let mut control = SignalLocatorOwnerPublicationWait {
                entered_signal: Some(locator_wait_tx),
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
            second_done_tx
                .send(())
                .expect("locator-owner completion watchdog receiver remains live");
            (admission, observation, second_counters)
        });

        let locator_wait_observed = locator_wait_rx.recv_timeout(Duration::from_secs(5));
        let completed_before_owner = second_done_rx.recv_timeout(Duration::from_millis(100));
        release_guard.release_v1();
        let first_result = first.join().unwrap();
        let second_result = second.join().unwrap();
        assert!(
            locator_wait_observed.is_ok(),
            "second pack did not report direct locator-owner coordination wait"
        );
        assert!(
            completed_before_owner.is_err(),
            "second pack completed before the locator owner made its catalog visible"
        );
        (first_result, second_result)
    });

    let (first_admission, first_observation, first_blocked, first_counters) = first_result;
    assert!(first_blocked);
    assert_eq!(first_observation, Ok(()));
    assert_eq!(
        first_admission.unwrap().outcome(),
        FsPackAdmissionOutcomeV1::Installed
    );
    let (second_admission, second_observation, second_counters) = second_result;
    assert_eq!(second_observation, Ok(()));
    assert_eq!(
        second_admission.unwrap().outcome(),
        FsPackAdmissionOutcomeV1::Installed
    );
    assert!(first_counters.publication_lock_acquisitions > 0);
    assert!(second_counters.publication_lock_acquisitions > 0);
    assert_eq!(second_counters.active_pack_publication_wait_polls, 0);
    assert_eq!(second_counters.active_pack_publication_wait_nanoseconds, 0);
    assert!(second_counters.locator_owner_publication_wait_polls > 0);
    assert!(second_counters.locator_owner_publication_wait_nanoseconds > 0);
    assert!(first_counters.has_zero_forbidden_work());
    assert!(second_counters.has_zero_forbidden_work());
    assert_eq!(ledger.admitted_slots(), 0);
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        2
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        2
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("objects")).unwrap().count(),
        2
    );
    let mut occupied = seed.occupied().unwrap();
    for (id, expected) in [(shared_id, shared), (second_only_id, second_only)] {
        assert_eq!(
            occupied.occupied_len(id).unwrap(),
            Some(expected.len() as u64)
        );
    }
}

#[test]
fn every_fresh_admission_boundary_cleans_or_counts_exact_residue() {
    let boundaries = [
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
    ];
    for boundary in boundaries {
        let fixture = TestRoot::new(&format!("fresh-boundary-{boundary:?}"));
        let cas = FsCasV1::create_new(&fixture.path).unwrap();
        let objects = [object(5, b"boundary-fault")];
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut counters = OperationCountersV1::default();
        let mut scratch = [0_u8; 65_536];
        let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
        let pack_len = pack.len().unwrap();
        assert_eq!(pack_len, 296);
        let mut spool = Spool::default();
        let mut control = StopAtBoundary::new(StopKind::Cancellation, boundary);
        assert_eq!(
            cas.admit_pack_controlled(
                &mut pack,
                &mut spool,
                &ledger,
                &mut counters,
                &mut scratch,
                &mut control,
            ),
            Err(FsCasErrorV1::Core(CoreError::Cancelled)),
            "boundary {boundary:?}"
        );
        assert_eq!(ledger.admitted_slots(), 0, "boundary {boundary:?}");
        assert_eq!(
            fs::read_dir(fixture.path.join("preparation"))
                .unwrap()
                .count(),
            0,
            "boundary {boundary:?}"
        );
        assert_eq!(
            fs::read_dir(fixture.path.join("closures")).unwrap().count(),
            0,
            "boundary {boundary:?}"
        );
        if boundary == FsCasBoundaryV1::AfterCatalogPublication {
            assert_eq!(
                fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
                1
            );
            assert_eq!(
                fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
                1
            );
            assert_eq!(
                fs::read_dir(fixture.path.join("objects")).unwrap().count(),
                1
            );
            assert_eq!(
                counters.unreachable_installed_residue_bytes,
                exact_fresh_pack_immutable_bytes(pack_len, objects.len() as u32),
            );
            assert_eq!(counters.fscas_bytes_written, 0);
        } else {
            assert_eq!(
                fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
                0,
                "boundary {boundary:?}"
            );
            assert_eq!(
                fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
                0,
                "boundary {boundary:?}"
            );
            assert_eq!(
                fs::read_dir(fixture.path.join("objects")).unwrap().count(),
                0,
                "boundary {boundary:?}"
            );
            assert_eq!(counters.unreachable_installed_residue_bytes, 0);
        }
        assert_eq!(counters.closure_fences, 0);
        assert!(counters.layerfs_open_file_handles_high_water <= 2);
        assert!(counters.has_zero_forbidden_work());
    }
}

#[test]
fn partial_multi_object_locator_publication_is_fully_rolled_back() {
    let fixture = TestRoot::new("partial-locator-publication");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [
        object(5, b"locator-a"),
        object(5, b"locator-b"),
        object(5, b"locator-c"),
    ];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
    let mut spool = Spool::default();
    let mut control = StopAtBoundary::new(
        StopKind::Cancellation,
        FsCasBoundaryV1::AfterObjectLocatorPublication,
    );
    assert_eq!(
        cas.admit_pack_controlled(
            &mut pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut scratch,
            &mut control,
        ),
        Err(FsCasErrorV1::Core(CoreError::Cancelled))
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("objects")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        0
    );
    assert_eq!(counters.unreachable_installed_residue_bytes, 0);
    assert_eq!(counters.fscas_bytes_written, 0);
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn every_incumbent_boundary_cleans_loser_without_changing_winner() {
    let fixture = TestRoot::new("incumbent-boundaries");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [object(5, &[0x3c; 32_768])];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut scratch = [0_u8; 65_536];
    let mut spool = Spool::default();
    let mut incumbent_counters = OperationCountersV1::default();
    let mut incumbent = build_private_pack(
        &cas,
        &objects,
        &ledger,
        &mut incumbent_counters,
        &mut scratch,
    );
    cas.admit_pack(
        &mut incumbent,
        &mut spool,
        &ledger,
        &mut incumbent_counters,
        &mut scratch,
    )
    .unwrap();
    let carrier = only_entry(&fixture.path.join("carriers"));
    let original = fs::read(&carrier).unwrap();

    for boundary in [
        FsCasBoundaryV1::BeforeIncumbentMarkerRead,
        FsCasBoundaryV1::AfterIncumbentMarkerRead,
        FsCasBoundaryV1::AfterIncumbentValidation,
        FsCasBoundaryV1::BeforeIncumbentComparisonWindow,
        FsCasBoundaryV1::AfterIncumbentComparisonWindow,
        FsCasBoundaryV1::BeforeObjectLocatorRead,
        FsCasBoundaryV1::AfterObjectLocatorRead,
        FsCasBoundaryV1::AfterObjectIncumbentValidation,
    ] {
        let mut counters = OperationCountersV1::default();
        let mut candidate =
            build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
        let mut control = StopAtBoundary::new(StopKind::Cancellation, boundary);
        assert_eq!(
            cas.admit_pack_controlled(
                &mut candidate,
                &mut spool,
                &ledger,
                &mut counters,
                &mut scratch,
                &mut control,
            ),
            Err(FsCasErrorV1::Core(CoreError::Cancelled)),
            "boundary {boundary:?}"
        );
        assert_eq!(ledger.admitted_slots(), 0);
        assert_eq!(
            fs::read_dir(fixture.path.join("preparation"))
                .unwrap()
                .count(),
            0
        );
        assert_eq!(fs::read(&carrier).unwrap(), original);
        assert_eq!(
            fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
            1
        );
        assert_eq!(
            fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
            1
        );
        assert_eq!(counters.unreachable_installed_residue_bytes, 0);
        assert_eq!(counters.closure_fences, 0);
        assert!(counters.layerfs_open_file_handles_high_water <= 2);
        assert!(counters.has_zero_forbidden_work());
    }
}

#[test]
fn catalog_publication_io_fault_removes_validated_unpublished_carrier() {
    let fixture = TestRoot::new("catalog-fault");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [object(5, b"catalog-fault")];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
    let mut spool = Spool::default();
    let mut control = BreakCatalogAtPublication {
        catalog: fixture.path.join("catalog"),
        injected: false,
    };
    assert_eq!(
        cas.admit_pack_controlled(
            &mut pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut scratch,
            &mut control,
        ),
        Err(FsCasErrorV1::Filesystem(
            FsCasFilesystemFailureV1::WriteFailure,
        ))
    );
    assert!(control.injected);
    assert_eq!(ledger.admitted_slots(), 0);
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        0
    );
    assert!(fixture.path.join("catalog").is_file());
    assert_eq!(counters.unreachable_installed_residue_bytes, 0);
    assert_eq!(counters.closure_fences, 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn malformed_root_owned_carrier_directory_fails_closed_without_fallback() {
    let fixture = TestRoot::new("malformed-carrier-directory");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [object(5, b"unsupported")];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
    fs::remove_dir(fixture.path.join("carriers")).unwrap();
    fs::write(fixture.path.join("carriers"), b"not-a-private-directory").unwrap();
    let mut spool = Spool::default();
    assert_eq!(
        cas.admit_pack(&mut pack, &mut spool, &ledger, &mut counters, &mut scratch),
        Err(FsCasErrorV1::MalformedOccupant)
    );
    assert_eq!(ledger.admitted_slots(), 0);
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        0
    );
    assert_eq!(counters.fscas_bytes_written, 0);
    assert_eq!(counters.unreachable_installed_residue_bytes, 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn closure_catalog_is_visible_only_after_complete_carrier_backed_validation() {
    let fixture = TestRoot::new("closure-fence");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let (version, root, version_id, root_id) = empty_closure();
    let objects = [version.clone(), root.clone()];
    let closure_objects = [(version_id, version), (root_id, root)];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut pack_scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut pack_scratch);
    let mut spool = Spool::default();
    let admission = cas
        .admit_pack(
            &mut pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut pack_scratch,
        )
        .unwrap();
    assert_eq!(admission.outcome(), FsPackAdmissionOutcomeV1::Installed);
    assert_eq!(admission.sealed().pack_len(), 616);
    assert_eq!(
        fs::read_dir(fixture.path.join("closures")).unwrap().count(),
        0
    );

    let mut closure = ClosureSource {
        objects: &closure_objects,
    };
    let preclosure_fscas_bytes_read = counters.fscas_bytes_read;
    let preclosure_fscas_read_calls = counters.fscas_read_calls;
    let mut operation = cas.begin_closure_operation().unwrap();
    let mut incoming_comparison = [0_u8; 65_536];
    let mut occupied_comparison = [0_u8; 65_536];
    let mut source_window = [0_u8; 32_768];
    let mut cdc_ring = [0_u8; 32_768];
    let mut traversal = [0_u8; 1];
    let (admitted, mut capability) = cas
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
        .unwrap();

    assert_eq!(admitted.version_record(), version_id);
    assert_eq!(admitted.object_count(), 2);
    assert_eq!(admitted.created_count(), 0);
    assert_eq!(admitted.reused_count(), 2);
    assert_eq!(capability.version_record().unwrap(), version_id);
    assert_eq!(capability.object_count().unwrap(), 2);
    assert_eq!(
        fs::read_dir(fixture.path.join("closures")).unwrap().count(),
        1
    );
    assert_eq!(counters.closure_fences, 1);
    assert!(counters.bytes_read > 0);
    assert!(counters.fscas_bytes_read > preclosure_fscas_bytes_read);
    assert!(counters.fscas_read_calls > preclosure_fscas_read_calls);
    cas.consume_validated_closure_for_handoff(&mut operation, &mut capability)
        .unwrap();
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn closure_counter_overflow_precedes_the_complete_fence() {
    let fixture = TestRoot::new("closure-counter-overflow");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let (version, root, version_id, root_id) = empty_closure();
    let objects = [version.clone(), root.clone()];
    let closure_objects = [(version_id, version), (root_id, root)];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut pack_scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut pack_scratch);
    let mut spool = Spool::default();
    cas.admit_pack(
        &mut pack,
        &mut spool,
        &ledger,
        &mut counters,
        &mut pack_scratch,
    )
    .unwrap();
    counters.closure_fences = u64::MAX;

    let mut closure = ClosureSource {
        objects: &closure_objects,
    };
    let mut operation = cas.begin_closure_operation().unwrap();
    let mut incoming_comparison = [0_u8; 65_536];
    let mut occupied_comparison = [0_u8; 65_536];
    let mut source_window = [0_u8; 32_768];
    let mut cdc_ring = [0_u8; 32_768];
    let mut traversal = [0_u8; 1];
    assert_eq!(
        cas.admit_complete_closure(
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
        ),
        Err(CoreError::IntegerOverflow)
    );
    assert_eq!(counters.closure_fences, u64::MAX);
    assert_eq!(
        fs::read_dir(fixture.path.join("closures")).unwrap().count(),
        0
    );
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn fscas_read_counter_overflow_precedes_the_complete_fence() {
    let fixture = TestRoot::new("closure-read-counter-overflow");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let (version, root, version_id, root_id) = empty_closure();
    let objects = [version.clone(), root.clone()];
    let closure_objects = [(version_id, version), (root_id, root)];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut install_counters = OperationCountersV1::default();
    let mut pack_scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(
        &cas,
        &objects,
        &ledger,
        &mut install_counters,
        &mut pack_scratch,
    );
    let mut spool = Spool::default();
    cas.admit_pack(
        &mut pack,
        &mut spool,
        &ledger,
        &mut install_counters,
        &mut pack_scratch,
    )
    .unwrap();

    let mut counters = OperationCountersV1 {
        fscas_bytes_read: u64::MAX,
        ..OperationCountersV1::default()
    };
    let mut closure = ClosureSource {
        objects: &closure_objects,
    };
    let mut operation = cas.begin_closure_operation().unwrap();
    let mut incoming_comparison = [0_u8; 65_536];
    let mut occupied_comparison = [0_u8; 65_536];
    let mut source_window = [0_u8; 32_768];
    let mut cdc_ring = [0_u8; 32_768];
    let mut traversal = [0_u8; 1];
    assert_eq!(
        cas.admit_complete_closure(
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
        ),
        Err(CoreError::IntegerOverflow)
    );
    assert_eq!(counters.fscas_bytes_read, u64::MAX);
    assert_eq!(counters.fscas_read_calls, 0);
    assert_eq!(counters.closure_fences, 0);
    assert_eq!(
        fs::read_dir(fixture.path.join("closures")).unwrap().count(),
        0
    );
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn closure_capability_rejects_cross_fscas_cross_operation_and_replay() {
    let fixture = TestRoot::new("closure-capability-binding");
    let other_fixture = TestRoot::new("closure-capability-other-fscas");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let other_cas = FsCasV1::create_new(&other_fixture.path).unwrap();
    let (version, root, version_id, root_id) = empty_closure();
    let closure_objects = [(version_id, version.clone()), (root_id, root.clone())];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut pack_scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(
        &cas,
        &[version, root],
        &ledger,
        &mut counters,
        &mut pack_scratch,
    );
    let mut spool = Spool::default();
    cas.admit_pack(
        &mut pack,
        &mut spool,
        &ledger,
        &mut counters,
        &mut pack_scratch,
    )
    .unwrap();

    let mut operation_a = cas.begin_closure_operation().unwrap();
    let mut closure_a = ClosureSource {
        objects: &closure_objects,
    };
    let mut incoming_a = [0_u8; 65_536];
    let mut occupied_a = [0_u8; 65_536];
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
        .unwrap();

    let mut operation_b = cas.begin_closure_operation().unwrap();
    let mut closure_b = ClosureSource {
        objects: &closure_objects,
    };
    let mut incoming_b = [0_u8; 65_536];
    let mut occupied_b = [0_u8; 65_536];
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
        .unwrap();

    assert_eq!(
        other_cas.consume_validated_closure_for_handoff(&mut operation_a, &mut capability_a,),
        Err(FsCasErrorV1::Integrity)
    );
    assert_eq!(
        cas.consume_validated_closure_for_handoff(&mut operation_b, &mut capability_a),
        Err(FsCasErrorV1::Integrity)
    );
    cas.consume_validated_closure_for_handoff(&mut operation_a, &mut capability_a)
        .unwrap();
    assert_eq!(
        cas.consume_validated_closure_for_handoff(&mut operation_a, &mut capability_a),
        Err(FsCasErrorV1::Integrity)
    );
    cas.consume_validated_closure_for_handoff(&mut operation_b, &mut capability_b)
        .unwrap();
    assert_eq!(
        fs::read_dir(fixture.path.join("closures")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(other_fixture.path.join("closures"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(counters.closure_fences, 2);

    let doomed_objects = [object(5, b"invalidate-issued-closure")];
    let mut doomed_pack = build_private_pack(
        &cas,
        &doomed_objects,
        &ledger,
        &mut counters,
        &mut pack_scratch,
    );
    let mut cleanup_failure = StopWithCleanupFailure::new(
        FsCasBoundaryV1::AfterCarrierInstall,
        FsCasCleanupTargetV1::Carrier,
    );
    assert_eq!(
        cas.admit_pack_controlled(
            &mut doomed_pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut pack_scratch,
            &mut cleanup_failure,
        ),
        Err(FsCasErrorV1::TerminalFailure {
            first: FsCasFailureCauseV1::Core(CoreError::Cancelled),
            dominant: FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::Carrier,),
        })
    );
    assert_eq!(
        capability_b.version_record(),
        Err(FsCasErrorV1::Invalidated)
    );
    assert_eq!(capability_b.object_count(), Err(FsCasErrorV1::Invalidated));
    assert_eq!(
        cas.consume_validated_closure_for_handoff(&mut operation_b, &mut capability_b),
        Err(FsCasErrorV1::Invalidated)
    );
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn closure_validation_failure_returns_no_closure_and_counts_installed_residue() {
    let fixture = TestRoot::new("closure-validation-fault");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let (version, root, version_id, root_id) = empty_closure();
    let objects = [version.clone(), root.clone()];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut pack_scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut pack_scratch);
    let mut spool = Spool::default();
    let admission = cas
        .admit_pack(
            &mut pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut pack_scratch,
        )
        .unwrap();
    assert_eq!(admission.sealed().pack_len(), 616);
    let mut malformed_root = root;
    *malformed_root.last_mut().unwrap() ^= 1;
    let closure_objects = [(version_id, version), (root_id, malformed_root)];
    let mut closure = ClosureSource {
        objects: &closure_objects,
    };
    let mut operation = cas.begin_closure_operation().unwrap();
    let mut incoming_comparison = [0_u8; 65_536];
    let mut occupied_comparison = [0_u8; 65_536];
    let mut source_window = [0_u8; 32_768];
    let mut cdc_ring = [0_u8; 32_768];
    let mut traversal = [0_u8; 1];
    assert_eq!(
        cas.admit_complete_closure(
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
        ),
        Err(CoreError::TypedEdge)
    );
    assert_eq!(counters.closure_fences, 0);
    assert_eq!(
        fs::read_dir(fixture.path.join("closures")).unwrap().count(),
        0
    );
    admission
        .record_later_unreachable_residue(&mut counters)
        .unwrap();
    assert_eq!(
        counters.unreachable_installed_residue_bytes,
        exact_fresh_pack_immutable_bytes(
            admission.sealed().pack_len(),
            admission.sealed().record_count(),
        )
    );
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn closure_fence_io_failure_returns_no_closure_or_publication() {
    let fixture = TestRoot::new("closure-fence-fault");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let (version, root, version_id, root_id) = empty_closure();
    let objects = [version.clone(), root.clone()];
    let closure_objects = [(version_id, version), (root_id, root)];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut pack_scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut pack_scratch);
    let mut spool = Spool::default();
    let admission = cas
        .admit_pack(
            &mut pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut pack_scratch,
        )
        .unwrap();
    assert_eq!(admission.sealed().pack_len(), 616);
    fs::remove_dir(fixture.path.join("closures")).unwrap();
    fs::write(fixture.path.join("closures"), b"injected-not-a-directory").unwrap();

    let mut closure = ClosureSource {
        objects: &closure_objects,
    };
    let mut operation = cas.begin_closure_operation().unwrap();
    let mut incoming_comparison = [0_u8; 65_536];
    let mut occupied_comparison = [0_u8; 65_536];
    let mut source_window = [0_u8; 32_768];
    let mut cdc_ring = [0_u8; 32_768];
    let mut traversal = [0_u8; 1];
    assert_eq!(
        cas.admit_complete_closure(
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
        ),
        Err(CoreError::SinkRefused)
    );
    assert!(counters.bytes_read > 0);
    assert_eq!(counters.closure_fences, 0);
    assert_eq!(counters.publication_authority_dispatches, 0);
    assert!(fixture.path.join("closures").is_file());
    admission
        .record_later_unreachable_residue(&mut counters)
        .unwrap();
    assert_eq!(
        counters.unreachable_installed_residue_bytes,
        exact_fresh_pack_immutable_bytes(
            admission.sealed().pack_len(),
            admission.sealed().record_count(),
        )
    );
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn fresh_carrier_validation_does_not_hold_the_visibility_lock() {
    let fixture = TestRoot::new("fresh-carrier-validation-lock");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [object(5, &[0x4d; 32_768])];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut scratch = [0_u8; 65_536];
    let mut counters = OperationCountersV1::default();
    let mut private = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
    let mut spool = Spool::default();
    let mut control = ObserveFreshCarrierValidationLock {
        cas: cas.clone(),
        observed: false,
        visibility_available: false,
        publication_available: false,
    };

    let admission = cas
        .admit_pack_controlled(
            &mut private,
            &mut spool,
            &ledger,
            &mut counters,
            &mut scratch,
            &mut control,
        )
        .unwrap();

    assert!(control.observed);
    assert!(control.visibility_available);
    assert!(control.publication_available);
    assert_eq!(admission.outcome(), FsPackAdmissionOutcomeV1::Installed);
}

#[test]
fn preparation_spool_creation_does_not_hold_root_visibility_or_publication() {
    let fixture = TestRoot::new("preparation-create-lock-scope");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let worker_cas = cas.clone();
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::channel();

    std::thread::scope(|scope| {
        let worker = scope.spawn(move || {
            let mut control = BlockPreparationCreate {
                entered_signal: entered_tx,
                release: release_rx,
                blocked: false,
            };
            let spool = worker_cas.begin_operation_spool_v1("preparation-lock-scope", &mut control);
            (spool, control)
        });

        entered_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("spool construction did not reach PreparationCreate");
        assert!(cas.visibility_lock_available_for_test_v1());
        assert!(cas.publication_lock_available_for_test_v1());
        release_tx
            .send(())
            .expect("preparation-create worker remains live");

        let (spool, control) = worker.join().unwrap();
        let mut spool = spool.unwrap();
        let mut control = control;
        assert!(control.blocked);
        spool.cleanup_controlled_v1(&mut control).unwrap();
    });

    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert!(cas.visibility_lock_available_for_test_v1());
    assert!(cas.publication_lock_available_for_test_v1());
}

#[test]
fn catalog_marker_preparation_does_not_serialize_disjoint_publication() {
    let fixture = TestRoot::new("catalog-preparation-publication-lock");
    let seed = FsCasV1::create_new(&fixture.path).unwrap();
    let first_cas = FsCasV1::open_existing(&fixture.path).unwrap();
    let second_cas = FsCasV1::open_existing(&fixture.path).unwrap();
    let first_objects = [object(5, &[0x51; 8_192])];
    let second_objects = [object(5, &[0x52; 8_192])];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut build_scratch = [0_u8; 65_536];
    let mut first_counters = OperationCountersV1::default();
    let mut second_counters = OperationCountersV1::default();
    let mut first_pack = build_private_pack(
        &first_cas,
        &first_objects,
        &ledger,
        &mut first_counters,
        &mut build_scratch,
    );
    let mut second_pack = build_private_pack(
        &second_cas,
        &second_objects,
        &ledger,
        &mut second_counters,
        &mut build_scratch,
    );
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::channel();
    let (second_done_tx, second_done_rx) = mpsc::sync_channel(1);

    std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            let mut spool = Spool::default();
            let mut scratch = [0_u8; 65_536];
            let mut control = BlockCatalogMarkerWrite {
                entered_signal: entered_tx,
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
            .expect("first publication did not reach catalog marker preparation");
        let second = scope.spawn(|| {
            let mut spool = Spool::default();
            let mut scratch = [0_u8; 65_536];
            let result = second_cas.admit_pack(
                &mut second_pack,
                &mut spool,
                &ledger,
                &mut second_counters,
                &mut scratch,
            );
            second_done_tx.send(result).unwrap();
        });

        let second_result = second_done_rx.recv_timeout(Duration::from_secs(5));
        release_tx
            .send(())
            .expect("catalog preparation worker remains live");
        let (first_result, first_blocked) = first.join().unwrap();
        second.join().unwrap();

        assert!(first_blocked);
        assert_eq!(
            first_result.unwrap().outcome(),
            FsPackAdmissionOutcomeV1::Installed
        );
        assert_eq!(
            second_result
                .expect("disjoint publication was serialized by catalog preparation")
                .unwrap()
                .outcome(),
            FsPackAdmissionOutcomeV1::Installed
        );
    });

    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        2
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        2
    );
    assert_eq!(ledger.admitted_slots(), 0);
    assert!(first_counters.has_zero_forbidden_work());
    assert!(second_counters.has_zero_forbidden_work());
    drop(seed);
}

#[test]
fn same_pack_race_is_no_replace_and_compares_every_incumbent_byte() {
    let fixture = TestRoot::new("race");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [object(5, &[0x5a; 32_768])];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut scratch = [0_u8; 65_536];
    let mut first_counters = OperationCountersV1::default();
    let mut first = build_private_pack(&cas, &objects, &ledger, &mut first_counters, &mut scratch);
    let mut spool = Spool::default();
    let installed = cas
        .admit_pack(
            &mut first,
            &mut spool,
            &ledger,
            &mut first_counters,
            &mut scratch,
        )
        .unwrap();
    assert_eq!(installed.sealed().pack_len(), 33_048);
    let carrier = only_entry(&fixture.path.join("carriers"));
    #[cfg(unix)]
    let original_inode = {
        use std::os::unix::fs::MetadataExt;
        fs::metadata(&carrier).unwrap().ino()
    };

    let mut second_counters = OperationCountersV1::default();
    let mut second =
        build_private_pack(&cas, &objects, &ledger, &mut second_counters, &mut scratch);
    let mut control = ObserveIncumbentComparisonLock {
        cas: cas.clone(),
        observed: false,
        visibility_available: false,
        publication_available: false,
    };
    let reused = cas
        .admit_pack_controlled(
            &mut second,
            &mut spool,
            &ledger,
            &mut second_counters,
            &mut scratch,
            &mut control,
        )
        .unwrap();
    assert!(control.observed);
    assert!(control.visibility_available);
    assert!(control.publication_available);
    assert_eq!(reused.outcome(), FsPackAdmissionOutcomeV1::ExistingComplete);
    assert_eq!(
        second_counters.incumbent_comparison_bytes,
        installed.sealed().pack_len()
    );
    assert_eq!(second_counters.incumbent_comparison_windows, 2);
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(fs::metadata(&carrier).unwrap().ino(), original_inode);
    }
    assert!(second_counters.has_zero_forbidden_work());
}

#[test]
fn same_carrier_incumbent_read_failures_are_typed_and_cleanup_the_candidate() {
    let fixture = TestRoot::new("same-carrier-incumbent-read-failures");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [object(5, &[0x5b; 32_768])];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut scratch = [0_u8; 65_536];
    let mut spool = Spool::default();
    let mut incumbent_counters = OperationCountersV1::default();
    let mut incumbent = build_private_pack(
        &cas,
        &objects,
        &ledger,
        &mut incumbent_counters,
        &mut scratch,
    );
    cas.admit_pack(
        &mut incumbent,
        &mut spool,
        &ledger,
        &mut incumbent_counters,
        &mut scratch,
    )
    .unwrap();

    for boundary in [
        FsCasFilesystemBoundaryV1::CatalogMarkerRead,
        FsCasFilesystemBoundaryV1::CatalogMarkerRevalidationRead,
        FsCasFilesystemBoundaryV1::CarrierMetadataRead,
        FsCasFilesystemBoundaryV1::IncumbentComparisonRead,
    ] {
        for error in [
            FsCasErrorV1::MissingOccupant,
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ReadFailure),
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ShortRead),
        ] {
            let mut counters = OperationCountersV1::default();
            let mut candidate =
                build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
            let mut control = ReadFaultAtBoundary {
                boundary,
                error,
                injected: false,
            };

            assert_eq!(
                cas.admit_pack_controlled(
                    &mut candidate,
                    &mut spool,
                    &ledger,
                    &mut counters,
                    &mut scratch,
                    &mut control,
                ),
                Err(error)
            );
            assert!(control.injected, "{boundary:?} did not inject");
            assert_eq!(ledger.admitted_slots(), 0);
            assert_eq!(
                fs::read_dir(fixture.path.join("preparation"))
                    .unwrap()
                    .count(),
                0
            );
            assert_eq!(
                fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
                1
            );
            assert_eq!(
                fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
                1
            );
            assert_eq!(counters.unreachable_installed_residue_bytes, 0);
            assert!(counters.has_zero_forbidden_work());
            assert!(cas.occupied().is_ok());
        }
    }
}

#[test]
fn cross_carrier_object_validation_read_failures_are_typed_and_cleanup_the_candidate() {
    let fixture = TestRoot::new("cross-carrier-object-validation-read-failures");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let shared = object(5, &[0x4d; 16_384]);
    let additional = object(5, &[0x9e; 16_384]);
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut scratch = [0_u8; 65_536];
    let mut spool = Spool::default();
    let mut incumbent_counters = OperationCountersV1::default();
    let mut incumbent = build_private_pack(
        &cas,
        std::slice::from_ref(&shared),
        &ledger,
        &mut incumbent_counters,
        &mut scratch,
    );
    cas.admit_pack(
        &mut incumbent,
        &mut spool,
        &ledger,
        &mut incumbent_counters,
        &mut scratch,
    )
    .unwrap();

    for boundary in [
        FsCasFilesystemBoundaryV1::ObjectLocatorRead,
        FsCasFilesystemBoundaryV1::CatalogMarkerRead,
        FsCasFilesystemBoundaryV1::CarrierMetadataRead,
        FsCasFilesystemBoundaryV1::CarrierIndexRead,
        FsCasFilesystemBoundaryV1::CarrierObjectRead,
        FsCasFilesystemBoundaryV1::IncumbentComparisonRead,
    ] {
        for error in [
            FsCasErrorV1::MissingOccupant,
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ReadFailure),
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ShortRead),
        ] {
            let mut counters = OperationCountersV1::default();
            let mut candidate = build_private_pack(
                &cas,
                &[shared.clone(), additional.clone()],
                &ledger,
                &mut counters,
                &mut scratch,
            );
            let mut control = ReadFaultAtBoundary {
                boundary,
                error,
                injected: false,
            };

            assert_eq!(
                cas.admit_pack_controlled(
                    &mut candidate,
                    &mut spool,
                    &ledger,
                    &mut counters,
                    &mut scratch,
                    &mut control,
                ),
                Err(error),
                "{boundary:?} / {error:?}"
            );
            assert!(control.injected, "{boundary:?} did not inject");
            assert_eq!(ledger.admitted_slots(), 0);
            assert_eq!(
                fs::read_dir(fixture.path.join("preparation"))
                    .unwrap()
                    .count(),
                0
            );
            assert_eq!(
                fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
                1
            );
            assert_eq!(
                fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
                1
            );
            assert_eq!(
                fs::read_dir(fixture.path.join("objects")).unwrap().count(),
                1
            );
            assert_eq!(counters.unreachable_installed_residue_bytes, 0);
            assert!(counters.has_zero_forbidden_work());
            assert!(cas.occupied().is_ok());
        }
    }
}

#[test]
fn equal_incumbent_comparison_overflow_is_transactional_and_keeps_read_observation() {
    let fixture = TestRoot::new("equal-incumbent-comparison-overflow");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let stale = FsCasV1::open_existing(&fixture.path).unwrap();
    let objects = [object(5, &[0x6a; 32_768])];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut scratch = [0_u8; 65_536];
    let mut spool = Spool::default();
    let mut incumbent_counters = OperationCountersV1::default();
    let mut incumbent = build_private_pack(
        &cas,
        &objects,
        &ledger,
        &mut incumbent_counters,
        &mut scratch,
    );
    let installed = cas
        .admit_pack(
            &mut incumbent,
            &mut spool,
            &ledger,
            &mut incumbent_counters,
            &mut scratch,
        )
        .unwrap();
    let carrier = only_entry(&fixture.path.join("carriers"));
    #[cfg(unix)]
    let incumbent_inode = {
        use std::os::unix::fs::MetadataExt;
        fs::metadata(&carrier).unwrap().ino()
    };

    let mut candidate_counters = OperationCountersV1::default();
    let mut candidate = build_private_pack(
        &cas,
        &objects,
        &ledger,
        &mut candidate_counters,
        &mut scratch,
    );
    candidate_counters.incumbent_comparison_bytes = 7;
    candidate_counters.incumbent_comparison_windows = u64::MAX;
    let comparison_before = candidate_counters.incumbent_comparison_bytes;
    let read_bytes_before = candidate_counters.fscas_bytes_read;
    let read_calls_before = candidate_counters.fscas_read_calls;

    assert_eq!(
        cas.admit_pack(
            &mut candidate,
            &mut spool,
            &ledger,
            &mut candidate_counters,
            &mut scratch,
        ),
        Err(FsCasErrorV1::Core(CoreError::IntegerOverflow))
    );
    assert_eq!(
        candidate_counters.incumbent_comparison_bytes,
        comparison_before
    );
    assert_eq!(candidate_counters.incumbent_comparison_windows, u64::MAX);
    assert_eq!(
        candidate_counters.fscas_bytes_read - read_bytes_before,
        98_832
    );
    assert_eq!(candidate_counters.fscas_read_calls - read_calls_before, 8);
    assert_eq!(ledger.admitted_slots(), 0);
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        1
    );
    assert_eq!(candidate_counters.storage_bytes_requested, 0);
    assert_eq!(candidate_counters.storage_bytes_reserved, 0);
    assert_eq!(candidate_counters.storage_bytes_released, 0);
    assert_eq!(candidate_counters.storage_bytes_committed, 0);
    assert_eq!(candidate_counters.storage_bytes_retained, 0);
    assert_eq!(candidate_counters.storage_inodes_requested, 0);
    assert_eq!(candidate_counters.storage_inodes_reserved, 0);
    assert_eq!(candidate_counters.storage_inodes_released, 0);
    assert_eq!(candidate_counters.storage_inodes_committed, 0);
    assert_eq!(candidate_counters.storage_inodes_retained, 0);
    assert_eq!(candidate_counters.unreachable_installed_residue_bytes, 0);
    assert_eq!(installed.sealed().pack_len(), 33_048);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(fs::metadata(&carrier).unwrap().ino(), incumbent_inode);
    }
    assert!(candidate_counters.has_zero_forbidden_work());
    assert!(cas.occupied().is_ok());
    assert!(stale.occupied().is_ok());
}

#[test]
fn incumbent_pack_read_observation_overflow_retains_typed_cause() {
    let fixture = TestRoot::new("incumbent-pack-read-observation-overflow");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let stale = FsCasV1::open_existing(&fixture.path).unwrap();
    let objects = [object(5, &[0x7b; 32_768])];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut scratch = [0_u8; 65_536];
    let mut spool = Spool::default();
    let mut incumbent_counters = OperationCountersV1::default();
    let mut incumbent = build_private_pack(
        &cas,
        &objects,
        &ledger,
        &mut incumbent_counters,
        &mut scratch,
    );
    let installed = cas
        .admit_pack(
            &mut incumbent,
            &mut spool,
            &ledger,
            &mut incumbent_counters,
            &mut scratch,
        )
        .unwrap();
    let carrier = only_entry(&fixture.path.join("carriers"));
    #[cfg(unix)]
    let incumbent_inode = {
        use std::os::unix::fs::MetadataExt;
        fs::metadata(&carrier).unwrap().ino()
    };

    let mut candidate_counters = OperationCountersV1::default();
    let mut candidate = build_private_pack(
        &cas,
        &objects,
        &ledger,
        &mut candidate_counters,
        &mut scratch,
    );
    cas.saturate_next_occupant_pack_read_calls_for_test_v1();

    assert_eq!(
        cas.admit_pack(
            &mut candidate,
            &mut spool,
            &ledger,
            &mut candidate_counters,
            &mut scratch,
        ),
        Err(FsCasErrorV1::Core(CoreError::IntegerOverflow))
    );
    assert_eq!(ledger.admitted_slots(), 0);
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        1
    );
    assert_eq!(candidate_counters.storage_bytes_requested, 0);
    assert_eq!(candidate_counters.storage_bytes_reserved, 0);
    assert_eq!(candidate_counters.storage_bytes_released, 0);
    assert_eq!(candidate_counters.storage_bytes_committed, 0);
    assert_eq!(candidate_counters.storage_bytes_retained, 0);
    assert_eq!(candidate_counters.storage_inodes_requested, 0);
    assert_eq!(candidate_counters.storage_inodes_reserved, 0);
    assert_eq!(candidate_counters.storage_inodes_released, 0);
    assert_eq!(candidate_counters.storage_inodes_committed, 0);
    assert_eq!(candidate_counters.storage_inodes_retained, 0);
    assert_eq!(candidate_counters.unreachable_installed_residue_bytes, 0);
    assert_eq!(installed.sealed().pack_len(), 33_048);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(fs::metadata(&carrier).unwrap().ino(), incumbent_inode);
    }
    assert!(candidate_counters.has_zero_forbidden_work());
    assert!(cas.occupied().is_ok());
    assert!(stale.occupied().is_ok());
}

#[test]
fn malformed_incumbent_fails_closed_without_overwrite_or_fallback() {
    let fixture = TestRoot::new("malformed");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [object(5, b"immutable")];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut scratch = [0_u8; 65_536];
    let mut counters = OperationCountersV1::default();
    let mut first = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
    let mut spool = Spool::default();
    cas.admit_pack(&mut first, &mut spool, &ledger, &mut counters, &mut scratch)
        .unwrap();
    let carrier = only_entry(&fixture.path.join("carriers"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&carrier, fs::Permissions::from_mode(0o600)).unwrap();
    }
    #[cfg(not(unix))]
    {
        let mut permissions = fs::metadata(&carrier).unwrap().permissions();
        permissions.set_readonly(false);
        fs::set_permissions(&carrier, permissions).unwrap();
    }
    let mut bytes = fs::read(&carrier).unwrap();
    bytes[0] ^= 0xff;
    fs::write(&carrier, bytes).unwrap();
    #[cfg(unix)]
    let incumbent_inode = {
        use std::os::unix::fs::MetadataExt;
        fs::metadata(&carrier).unwrap().ino()
    };

    let mut candidate_counters = OperationCountersV1::default();
    let mut candidate = build_private_pack(
        &cas,
        &objects,
        &ledger,
        &mut candidate_counters,
        &mut scratch,
    );
    let result = cas.admit_pack(
        &mut candidate,
        &mut spool,
        &ledger,
        &mut candidate_counters,
        &mut scratch,
    );
    assert_eq!(result, Err(FsCasErrorV1::MalformedOccupant));
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(fs::metadata(&carrier).unwrap().ino(), incumbent_inode);
    }
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        1
    );
    assert!(candidate_counters.has_zero_forbidden_work());
    drop(candidate);
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
}

#[derive(Clone, Copy, Debug)]
enum ConcurrentIncumbentFailureV1 {
    UnequalCompleteBytes,
    Malformed,
}

#[test]
fn simultaneous_reopened_disjoint_success_crosses_unequal_and_malformed_incumbents() {
    for (label, failure) in [
        (
            "concurrent-unequal-incumbent",
            ConcurrentIncumbentFailureV1::UnequalCompleteBytes,
        ),
        (
            "concurrent-malformed-incumbent",
            ConcurrentIncumbentFailureV1::Malformed,
        ),
    ] {
        let fixture = TestRoot::new(label);
        let seed = FsCasV1::create_new(&fixture.path).unwrap();
        let failing_cas = FsCasV1::open_existing(&fixture.path).unwrap();
        let success_cas = FsCasV1::open_existing(&fixture.path).unwrap();
        let shared = object(5, &[0x81; 4_096]);
        let shared_id = typed_id(&shared);
        let disjoint = object(5, &[0x82; 4_097]);
        let disjoint_id = typed_id(&disjoint);
        let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
        let mut build_scratch = [0_u8; 65_536];
        let mut seed_counters = OperationCountersV1::default();
        let mut seed_pack = build_private_pack(
            &seed,
            &[shared.clone()],
            &ledger,
            &mut seed_counters,
            &mut build_scratch,
        );
        let mut seed_spool = Spool::default();
        assert_eq!(
            seed.admit_pack(
                &mut seed_pack,
                &mut seed_spool,
                &ledger,
                &mut seed_counters,
                &mut build_scratch,
            )
            .unwrap()
            .outcome(),
            FsPackAdmissionOutcomeV1::Installed
        );

        let incumbent_locator_path = locator_path(&fixture.path, shared_id);
        let incumbent_locator = fs::read(&incumbent_locator_path).unwrap();
        let incumbent_carrier = only_entry(&fixture.path.join("carriers"));
        let original_permissions = make_owner_writable(&incumbent_carrier);
        let mut corrupted_carrier = fs::read(&incumbent_carrier).unwrap();
        match failure {
            ConcurrentIncumbentFailureV1::UnequalCompleteBytes => {
                let object_offset =
                    u64::from_be_bytes(incumbent_locator[104..112].try_into().unwrap()) + 4;
                let object_len =
                    u32::from_be_bytes(incumbent_locator[112..116].try_into().unwrap());
                let corrupt_at =
                    usize::try_from(object_offset + u64::from(object_len) - 1).unwrap();
                corrupted_carrier[corrupt_at] ^= 0xff;
            }
            ConcurrentIncumbentFailureV1::Malformed => corrupted_carrier[0] ^= 0xff,
        }
        fs::write(&incumbent_carrier, &corrupted_carrier).unwrap();
        fs::set_permissions(&incumbent_carrier, original_permissions).unwrap();
        #[cfg(unix)]
        let incumbent_inode = {
            use std::os::unix::fs::MetadataExt;
            fs::metadata(&incumbent_carrier).unwrap().ino()
        };

        let mut failure_counters = OperationCountersV1::default();
        let mut failure_pack = build_private_pack(
            &failing_cas,
            &[shared.clone()],
            &ledger,
            &mut failure_counters,
            &mut build_scratch,
        );
        let mut success_counters = OperationCountersV1::default();
        let mut success_pack = build_private_pack(
            &success_cas,
            &[disjoint.clone()],
            &ledger,
            &mut success_counters,
            &mut build_scratch,
        );
        let incumbent_gate = Arc::new(WatchdogGateV1::new());
        let (incumbent_entered_tx, incumbent_entered_rx) = mpsc::sync_channel(1);
        let (success_done_tx, success_done_rx) = mpsc::sync_channel(1);

        let (
            (failure_terminal, failure_observation, failure_counters),
            (success_terminal, success_observation, success_counters),
        ) = std::thread::scope(|scope| {
            let mut incumbent_release = WatchdogGateReleaseV1::new(Arc::clone(&incumbent_gate));
            let failure_gate = Arc::clone(&incumbent_gate);
            let failure_ledger = &ledger;
            let failure_thread = scope.spawn(move || {
                let mut spool = Spool::default();
                let mut scratch = [0_u8; 65_536];
                let mut control = BlockAtIncumbentAuthorityV1 {
                    release: failure_gate,
                    entered_signal: Some(incumbent_entered_tx),
                };
                let (terminal, observation) = {
                    let mut observed = FsOperationObservedControlV1::new(&mut control);
                    let terminal = failing_cas.admit_pack_controlled(
                        &mut failure_pack,
                        &mut spool,
                        failure_ledger,
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
                .unwrap_or_else(|error| {
                    panic!("{label}: failing caller missed incumbent boundary: {error}")
                });

            let success_ledger = &ledger;
            let success_thread = scope.spawn(move || {
                let mut spool = Spool::default();
                let mut scratch = [0_u8; 65_536];
                let mut control = ContinueControlV1;
                let (terminal, observation) = {
                    let mut observed = FsOperationObservedControlV1::new(&mut control);
                    let terminal = success_cas.admit_pack_controlled(
                        &mut success_pack,
                        &mut spool,
                        success_ledger,
                        &mut success_counters,
                        &mut scratch,
                        &mut observed,
                    );
                    let observation = observed.finish_v1(&mut success_counters);
                    (terminal, observation)
                };
                success_done_tx
                    .send(())
                    .expect("disjoint-success watchdog receiver remains live");
                (terminal, observation, success_counters)
            });

            success_done_rx
                .recv_timeout(Duration::from_secs(5))
                .unwrap_or_else(|error| {
                    panic!(
                        "{label}: disjoint publication did not cross incumbent validation: {error}"
                    )
                });
            incumbent_release.release_v1();
            (
                failure_thread.join().unwrap(),
                success_thread.join().unwrap(),
            )
        });

        let expected = match failure {
            ConcurrentIncumbentFailureV1::UnequalCompleteBytes => {
                FsCasErrorV1::Core(CoreError::IdMismatch)
            }
            ConcurrentIncumbentFailureV1::Malformed => FsCasErrorV1::MalformedOccupant,
        };
        assert_eq!(failure_terminal, Err(expected), "{label}");
        assert_eq!(failure_observation, Ok(()), "{label}");
        assert_eq!(
            success_terminal.unwrap().outcome(),
            FsPackAdmissionOutcomeV1::Installed,
            "{label}"
        );
        assert_eq!(success_observation, Ok(()), "{label}");
        for counters in [&failure_counters, &success_counters] {
            assert_storage_equations(counters);
            assert!(counters.has_zero_forbidden_work(), "{label}");
            assert_eq!(counters.unreachable_installed_residue_bytes, 0, "{label}");
            assert!(counters.publication_lock_acquisitions > 0, "{label}");
            assert!(counters.publication_lock_hold_nanoseconds > 0, "{label}");
        }
        assert!(success_counters.visibility_lock_acquisitions > 0, "{label}");
        assert!(
            success_counters.visibility_lock_hold_nanoseconds > 0,
            "{label}"
        );
        assert_eq!(fs::read(&incumbent_carrier).unwrap(), corrupted_carrier);
        assert_eq!(
            fs::read(&incumbent_locator_path).unwrap(),
            incumbent_locator
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(
                fs::metadata(&incumbent_carrier).unwrap().ino(),
                incumbent_inode
            );
        }
        assert_eq!(
            fs::read_dir(fixture.path.join("preparation"))
                .unwrap()
                .count(),
            0,
            "{label}"
        );
        assert_eq!(
            fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
            2,
            "{label}"
        );
        assert_eq!(
            fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
            2,
            "{label}"
        );
        assert_eq!(
            fs::read_dir(fixture.path.join("objects")).unwrap().count(),
            2,
            "{label}"
        );
        assert_eq!(ledger.admitted_slots(), 0, "{label}");
        let mut occupied = seed.occupied().unwrap();
        assert_eq!(
            occupied.occupied_len(disjoint_id).unwrap(),
            Some(disjoint.len() as u64),
            "{label}"
        );
    }
}

#[test]
fn source_failure_cleans_preinstall_state_and_releases_the_slot() {
    let fixture = TestRoot::new("cleanup");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [object(5, b"never-installed")];
    let mut source = ObjectSource::new(&objects);
    source.fail_payload = true;
    let mut pack = cas.begin_private_pack().unwrap();
    let mut spool = Spool::default();
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    assert_eq!(
        build_dense_pack_v1(
            &mut source,
            &mut pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut scratch,
        ),
        Err(CoreError::SourceFailure)
    );
    drop(pack);
    assert_eq!(ledger.admitted_slots(), 0);
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        0
    );
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn deadline_before_install_removes_private_pack_and_releases_resources() {
    let fixture = TestRoot::new("deadline");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [object(5, b"deadline-before-install")];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
    let mut spool = Spool::default();
    let mut control =
        StopAtBoundary::new(StopKind::Deadline, FsCasBoundaryV1::BeforeCarrierInstall);
    assert_eq!(
        cas.admit_pack_controlled(
            &mut pack,
            &mut spool,
            &ledger,
            &mut counters,
            &mut scratch,
            &mut control,
        ),
        Err(FsCasErrorV1::Core(CoreError::Deadline))
    );
    assert_eq!(ledger.admitted_slots(), 0);
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        0
    );
    assert_eq!(counters.unreachable_installed_residue_bytes, 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn cancellation_during_loser_readback_keeps_incumbent_and_cleans_candidate() {
    let fixture = TestRoot::new("cancel-loser");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [object(5, &[0xa5; 32_768])];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut scratch = [0_u8; 65_536];
    let mut spool = Spool::default();
    let mut incumbent_counters = OperationCountersV1::default();
    let mut incumbent = build_private_pack(
        &cas,
        &objects,
        &ledger,
        &mut incumbent_counters,
        &mut scratch,
    );
    cas.admit_pack(
        &mut incumbent,
        &mut spool,
        &ledger,
        &mut incumbent_counters,
        &mut scratch,
    )
    .unwrap();
    let carrier = only_entry(&fixture.path.join("carriers"));
    #[cfg(unix)]
    let incumbent_inode = {
        use std::os::unix::fs::MetadataExt;
        fs::metadata(&carrier).unwrap().ino()
    };

    let mut candidate_counters = OperationCountersV1::default();
    let mut candidate = build_private_pack(
        &cas,
        &objects,
        &ledger,
        &mut candidate_counters,
        &mut scratch,
    );
    let mut control = StopAtBoundary::new(
        StopKind::Cancellation,
        FsCasBoundaryV1::AfterIncumbentComparisonWindow,
    );
    assert_eq!(
        cas.admit_pack_controlled(
            &mut candidate,
            &mut spool,
            &ledger,
            &mut candidate_counters,
            &mut scratch,
            &mut control,
        ),
        Err(FsCasErrorV1::Core(CoreError::Cancelled))
    );
    assert_eq!(candidate_counters.incumbent_comparison_windows, 1);
    assert_eq!(candidate_counters.incumbent_comparison_bytes, 32_768);
    assert_eq!(ledger.admitted_slots(), 0);
    assert_eq!(
        fs::read_dir(fixture.path.join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("carriers")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("catalog")).unwrap().count(),
        1
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(fs::metadata(&carrier).unwrap().ino(), incumbent_inode);
    }
    assert_eq!(candidate_counters.unreachable_installed_residue_bytes, 0);
    assert!(candidate_counters.has_zero_forbidden_work());
}

#[test]
fn later_closure_failure_is_counted_residue_not_a_private_version() {
    let fixture = TestRoot::new("residue");
    let cas = FsCasV1::create_new(&fixture.path).unwrap();
    let objects = [object(5, b"carrier-only")];
    let ledger = ResourceLedgerV1::new(32 * 1024 * 1024);
    let mut counters = OperationCountersV1::default();
    let mut scratch = [0_u8; 65_536];
    let mut pack = build_private_pack(&cas, &objects, &ledger, &mut counters, &mut scratch);
    let mut spool = Spool::default();
    let admission = cas
        .admit_pack(&mut pack, &mut spool, &ledger, &mut counters, &mut scratch)
        .unwrap();
    assert_eq!(admission.sealed().pack_len(), 296);
    let (version, root, version_id, root_id) = empty_closure();
    let closure_objects = [(version_id, version), (root_id, root)];
    let mut closure = ClosureSource {
        objects: &closure_objects,
    };
    let mut operation = cas.begin_closure_operation().unwrap();
    let mut incoming_comparison = [0_u8; 65_536];
    let mut occupied_comparison = [0_u8; 65_536];
    let mut source_window = [0_u8; 32_768];
    let mut cdc_ring = [0_u8; 32_768];
    let mut traversal = [0_u8; 1];
    assert_eq!(
        cas.admit_complete_closure(
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
        ),
        Err(CoreError::SinkRefused)
    );
    admission
        .record_later_unreachable_residue(&mut counters)
        .unwrap();
    assert_eq!(
        counters.unreachable_installed_residue_bytes,
        exact_fresh_pack_immutable_bytes(
            admission.sealed().pack_len(),
            admission.sealed().record_count(),
        )
    );
    assert_eq!(
        fs::read_dir(fixture.path.join("closures")).unwrap().count(),
        0
    );
    assert_eq!(counters.closure_fences, 0);
    assert!(counters.has_zero_forbidden_work());
}

#[cfg(unix)]
#[test]
fn symlinked_parent_is_typed_unsupported_before_namespace_creation() {
    use std::os::unix::fs::symlink;

    let fixture = TestRoot::new("symlink-parent");
    fs::create_dir(&fixture.path).unwrap();
    let actual = fixture.path.join("actual");
    fs::create_dir(&actual).unwrap();
    let linked = fixture.path.join("linked");
    symlink(&actual, &linked).unwrap();
    assert!(matches!(
        FsCasV1::create_new(&linked.join("cas")),
        Err(FsCasErrorV1::Unsupported)
    ));
    assert_path_absent(&actual.join("cas"));
}
