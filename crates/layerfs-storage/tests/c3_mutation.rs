use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use layerfs_storage::cas::{
    FsCasBoundaryV1, FsCasCleanupTargetV1, FsCasControlV1, FsCasErrorV1, FsCasFilesystemBoundaryV1,
    FsCasFilesystemFailureV1, FsCasV1,
};
use layerfs_storage::cdc::{CdcAlgorithmV1, CdcControlV1, FastCdcV1, MAXIMUM_CHUNK_BYTES};
use layerfs_storage::content::update::{
    AuthenticatedBaseByteReaderV1, BaseChunkEvidenceSourceV1, BaseChunkEvidenceV1, BaseReadErrorV1,
};
use layerfs_storage::content::{
    request_tree_operation_v1, run_create_tree_v1, ContentSourceErrorV1, ContentSourceV1,
    OperationBuffersV1, PreparedSinkErrorV1, SourceSupplierV1, TreeFileV1,
};
use layerfs_storage::cow::file::{AuthenticatedBaseFileV1, UpdateRangeV1};
use layerfs_storage::cow::{
    CanonicalTreeChildV1, CanonicalTreeEntryV1, DirectoryBuildModeV1, DirectoryLogicalIdentityV1,
    TreePageSummaryV1, MAX_TREE_OBJECT_BYTES, MAX_TREE_PAGE_SUMMARIES,
};
use layerfs_storage::format::{ValidatedComponent, MAX_PATH_BYTES};
use layerfs_storage::identity::{
    derive_file_node_v1, derive_logical_chunk_v1, derive_logical_file_v1,
    derive_physical_chunk_id_v1, derive_physical_file_id_v1, LogicalChunkRefV1,
    LogicalFileIdentityV1, PhysicalFileIdV1, PhysicalTreeIdV1, PhysicalVersionRecordIdV1,
    COMPARISON_WINDOW_BYTES,
};
use layerfs_storage::lifecycle::{
    complete_cross_directory_move_operation_v1, run_complete_add_v1, run_complete_metadata_v1,
    run_complete_move_v1, run_complete_remove_v1, run_complete_replace_v1, run_complete_update_v1,
};
use layerfs_storage::limits::OperationCountersV1;
use layerfs_storage::profile::ProfileSpecV1;
use layerfs_storage::read::extraction::{
    extract_root_v1, read_file_range_impl_v1, ReadBuffersV1, ReadKindV1, ReadSinkErrorV1,
    ReadSinkV1,
};
use layerfs_storage::{CoreError, CoreResult};

use crate::l1_tree_tests::{build, mutation_fixture, replacement_fixture, MutationSource};

static NEXT_ROOT: AtomicU64 = AtomicU64::new(1);

struct TestRoot(PathBuf);

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
            .wait_timeout_while(released, Duration::from_secs(15), |released| !*released)
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

struct LoadAbortOnDropV1 {
    abort: Arc<AtomicBool>,
    armed: bool,
}

impl LoadAbortOnDropV1 {
    fn new(abort: Arc<AtomicBool>) -> Self {
        Self { abort, armed: true }
    }

    fn disarm_v1(&mut self) {
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

impl TestRoot {
    fn new(label: &str) -> Self {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let parent = fs::canonicalize(std::env::temp_dir()).expect("canonical temporary root");
        Self(parent.join(format!(
            "layerfs-c3-mutation-{label}-{}-{sequence:016x}",
            std::process::id()
        )))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Default)]
struct ContinueControl {
    boundaries: Vec<FsCasBoundaryV1>,
}

impl CdcControlV1 for ContinueControl {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for ContinueControl {
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

#[derive(Default)]
struct PanicPrivatePackCleanupAfterInstalledCarrier {
    after_catalog_publication: bool,
    publication_poll_passed: bool,
    cleanup_panicked: bool,
}

impl PanicPrivatePackCleanupAfterInstalledCarrier {
    fn cancellation_requested_v1(&mut self) -> bool {
        if !self.after_catalog_publication {
            return false;
        }
        if !self.publication_poll_passed {
            // The AfterCatalogPublication sample belongs to the completed
            // carrier admission. Let that exact poll return so the writer
            // owns the installed-carrier observation, then cancel the next
            // bounded mutation step while its successor private pack lives.
            self.publication_poll_passed = true;
            return false;
        }
        true
    }
}

impl CdcControlV1 for PanicPrivatePackCleanupAfterInstalledCarrier {
    fn cancellation_requested(&mut self) -> bool {
        self.cancellation_requested_v1()
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for PanicPrivatePackCleanupAfterInstalledCarrier {
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
            panic!("injected post-install private-pack cleanup unwind")
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
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
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

    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        Ok(core::mem::size_of::<SliceSource<'_>>() as u64)
    }

    fn supply(self) -> CoreResult<Self::Source> {
        Ok(SliceSource::new(self.bytes))
    }
}

struct BarrierReadSink {
    ready: Option<mpsc::SyncSender<()>>,
    release: Arc<WatchdogGateV1>,
    bytes: Vec<u8>,
    selected_offset: u64,
    selected_len: u64,
    finished: bool,
    aborted: bool,
}

struct GatedReadSink {
    entered: Option<mpsc::SyncSender<()>>,
    gate: Arc<WatchdogGateV1>,
    bytes: Vec<u8>,
    finished: bool,
    aborted: bool,
}

impl GatedReadSink {
    fn new(capacity: usize, entered: mpsc::SyncSender<()>, gate: Arc<WatchdogGateV1>) -> Self {
        Self {
            entered: Some(entered),
            gate,
            bytes: Vec::with_capacity(capacity),
            finished: false,
            aborted: false,
        }
    }
}

impl ReadSinkV1 for GatedReadSink {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
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
        _selected_offset: u64,
        _selected_len: u64,
    ) -> Result<(), ReadSinkErrorV1> {
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

struct LoadRowControl {
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

#[derive(Debug)]
struct LoadContentionReportV1 {
    reader_successes: usize,
    reader_cancelled: usize,
    reader_deadlines: usize,
    writer_successes: usize,
    writer_faults: usize,
    total_terminals: usize,
    cancellation_terminal_latency: Duration,
    deadline_terminal_latency: Duration,
    elapsed: Duration,
    throughput_numerator: usize,
    terminals_per_second: f64,
    admission_wait_tokens: usize,
    admission_wait_nanoseconds: u64,
    active_publication_wait_tokens: usize,
    active_publication_wait_nanoseconds: u64,
    visibility_wait_nanoseconds: u64,
    visibility_hold_nanoseconds: u64,
    publication_wait_nanoseconds: u64,
    publication_hold_nanoseconds: u64,
    final_preparation_bytes: u64,
    final_preparation_inodes: u64,
}

impl LoadReaderControlV1 {
    fn cancellation_requested_v1(&self) -> bool {
        self.observed_polls.fetch_add(1, Ordering::AcqRel);
        self.abort.load(Ordering::Acquire)
            || (self.stop == LoadReaderStopV1::Cancelled && self.armed.load(Ordering::Acquire) != 0)
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
                entered
                    .send(())
                    .expect("occupied-read gate receiver remains live");
                self.occupied_read_gate
                    .as_ref()
                    .expect("selected occupied reader owns its gate")
                    .wait();
            }
        }
        None
    }
}

impl CdcControlV1 for LoadRowControl {
    fn cancellation_requested(&mut self) -> bool {
        self.abort.load(Ordering::Acquire)
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for LoadRowControl {
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
                        .expect("32-reader/8-writer carrier gate receiver remains live");
                    self.carrier_winner_gate.wait();
                }
            }
            FsCasBoundaryV1::ActivePackPublicationWait if !self.active_wait_reported => {
                self.active_wait_reported = true;
                self.active_wait_entered
                    .send(())
                    .expect("active-publication wait receiver remains live");
            }
            FsCasBoundaryV1::BeforeIncumbentComparisonWindow
            | FsCasBoundaryV1::BeforeObjectComparisonWindow => {
                self.delayed_comparison_windows = self
                    .delayed_comparison_windows
                    .checked_add(1)
                    .expect("bounded incumbent comparison windows");
                if self
                    .comparison_delay_claim
                    .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    self.comparison_entered
                        .send(())
                        .expect("comparison gate receiver remains live");
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

impl BarrierReadSink {
    fn new(capacity: usize, ready: mpsc::SyncSender<()>, release: Arc<WatchdogGateV1>) -> Self {
        Self {
            ready: Some(ready),
            release,
            bytes: Vec::with_capacity(capacity),
            selected_offset: 0,
            selected_len: 0,
            finished: false,
            aborted: false,
        }
    }
}

impl ReadSinkV1 for BarrierReadSink {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
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
        if let Some(ready) = self.ready.take() {
            ready
                .send(())
                .expect("read barrier watchdog receiver remains live");
            self.release.wait();
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

#[derive(Clone)]
struct ExpectedFile {
    logical: LogicalFileIdentityV1,
    physical: PhysicalFileIdV1,
    chunks: Vec<BaseChunkEvidenceV1>,
}

impl ExpectedFile {
    fn child(&self, mode: u16) -> CanonicalTreeChildV1 {
        CanonicalTreeChildV1::File {
            logical: derive_file_node_v1(mode, self.logical).expect("canonical file node"),
            physical: self.physical,
        }
    }

    fn authenticated(&self, mode: u16) -> AuthenticatedBaseFileV1 {
        AuthenticatedBaseFileV1::new(self.logical, self.physical, mode, self.chunks.len() as u32)
    }

    fn evidence(&self) -> Evidence {
        Evidence {
            chunks: self.chunks.clone(),
            cursor: 0,
        }
    }
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

fn expected_file(data: &[u8], mode: u16) -> ExpectedFile {
    let mut chunks = Vec::new();
    let mut logical_refs = Vec::new();
    let mut physical_refs = Vec::new();
    let mut offset = 0_usize;
    while offset < data.len() {
        let cut = FastCdcV1::new()
            .cut(&data[offset..])
            .expect("bounded FastCDC cut");
        let payload = &data[offset..offset + cut];
        let logical = derive_logical_chunk_v1(payload).expect("logical chunk");
        let physical =
            derive_physical_chunk_id_v1(&canonical_object(0x05, payload)).expect("physical chunk");
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
    let logical =
        derive_logical_file_v1(data.len() as u64, &logical_refs).expect("logical file identity");
    let mut payload = Vec::new();
    payload.extend_from_slice(&mode.to_be_bytes());
    payload.extend_from_slice(&(data.len() as u64).to_be_bytes());
    payload.extend_from_slice(&u32::from(!data.is_empty()).to_be_bytes());
    if !data.is_empty() {
        payload.push(0x02);
        payload.extend_from_slice(&(data.len() as u64).to_be_bytes());
        payload.extend_from_slice(&(physical_refs.len() as u32).to_be_bytes());
        for (len, id) in physical_refs {
            payload.extend_from_slice(&len.to_be_bytes());
            payload.extend_from_slice(id.as_bytes());
        }
    }
    let physical = derive_physical_file_id_v1(&canonical_object(0x03, &payload))
        .expect("physical file identity");
    ExpectedFile {
        logical,
        physical,
        chunks,
    }
}

#[derive(Clone)]
struct Evidence {
    chunks: Vec<BaseChunkEvidenceV1>,
    cursor: usize,
}

impl Evidence {
    fn total_len(&self) -> u64 {
        self.chunks
            .last()
            .map(|chunk| chunk.end().expect("bounded chunk end"))
            .unwrap_or(0)
    }
}

impl BaseChunkEvidenceSourceV1 for Evidence {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
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
        Ok(self.chunks.iter().copied().find(|chunk| {
            let end = chunk.end().expect("bounded chunk end");
            (chunk.start() <= offset && offset < end)
                || (include_end && offset == end && end == self.total_len())
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

struct BaseBytes<'a> {
    bytes: &'a [u8],
}

impl AuthenticatedBaseByteReaderV1 for BaseBytes<'_> {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
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
        destination.copy_from_slice(self.bytes.get(start..end).ok_or(BaseReadErrorV1::Missing)?);
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

fn entry<'a>(name: &'a [u8], file: &ExpectedFile, mode: u16) -> CanonicalTreeEntryV1<'a> {
    CanonicalTreeEntryV1::new(
        ValidatedComponent::new(name).expect("canonical component"),
        file.child(mode),
    )
}

fn directory_entry<'a>(
    name: &'a [u8],
    directory: layerfs_storage::cow::CanonicalDirectoryTreeV1,
) -> CanonicalTreeEntryV1<'a> {
    let DirectoryLogicalIdentityV1::Explicit(logical) = directory.logical() else {
        panic!("nested directory must have explicit logical identity");
    };
    CanonicalTreeEntryV1::new(
        ValidatedComponent::new(name).expect("canonical directory component"),
        CanonicalTreeChildV1::Directory {
            logical,
            physical: directory.physical(),
        },
    )
}

fn accept_files(
    cas: &FsCasV1,
    key: u64,
    files: &[(&[u8], u16, &[u8])],
) -> (PhysicalVersionRecordIdV1, PhysicalTreeIdV1) {
    let mut manifest: Vec<_> = files
        .iter()
        .map(|(path, mode, bytes)| {
            TreeFileV1::new(path, *mode, bytes.len() as u64, SliceSupplier { bytes })
        })
        .collect();
    let mut scratch = OperationScratch::new();
    let mut control = ContinueControl::default();
    let mut counters = OperationCountersV1::default();
    let operation =
        request_tree_operation_v1(cas, key, &mut counters, &mut control).expect("root grant");
    let handoff = run_create_tree_v1(
        operation,
        CdcAlgorithmV1::FastCdc,
        &mut manifest,
        scratch.borrow(),
        &mut control,
        &mut counters,
    )
    .expect("accepted base root");
    (handoff.version_record(), handoff.root_tree())
}

fn assert_clean_terminal(cas: &FsCasV1, root: &Path) {
    assert_eq!(cas.operation_admitted_slots_v1(), 0);
    assert_eq!(cas.operation_admission_active_for_test_v1(), 0);
    assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
    assert_eq!(
        fs::read_dir(root.join("preparation"))
            .expect("preparation directory")
            .count(),
        0
    );
}

fn assert_storage_terminal(counters: &OperationCountersV1) {
    assert!(counters.storage_bytes_requested > 0);
    assert_eq!(
        counters.storage_bytes_requested,
        counters.storage_bytes_reserved
    );
    assert_eq!(
        counters.storage_bytes_reserved,
        counters.storage_bytes_released
            + counters.storage_bytes_committed
            + counters.storage_bytes_retained
    );
    assert_eq!(
        counters.storage_inodes_requested,
        counters.storage_inodes_reserved
    );
    assert_eq!(
        counters.storage_inodes_reserved,
        counters.storage_inodes_released
            + counters.storage_inodes_committed
            + counters.storage_inodes_retained
    );
    assert!(
        counters.root_storage_active_reserved_bytes_lifetime_high_water
            >= counters.storage_bytes_reserved
    );
    assert!(
        counters.root_storage_active_reserved_inodes_lifetime_high_water
            >= counters.storage_inodes_reserved
    );
    assert!(counters.storage_bytes_committed > 0);
    assert!(counters.storage_inodes_committed > 0);
    assert_eq!(counters.storage_bytes_retained, 0);
    assert_eq!(counters.storage_inodes_retained, 0);
    assert_eq!(counters.mutable_preparation_residue_bytes, 0);
    assert_eq!(counters.mutable_preparation_residue_inodes, 0);
    assert_eq!(counters.unreachable_installed_residue_bytes, 0);
    assert!(counters.has_zero_forbidden_work());
}

fn assert_balanced_storage_terminal(counters: &OperationCountersV1) {
    assert!(counters.storage_bytes_requested > 0);
    assert_eq!(
        counters.storage_bytes_requested,
        counters.storage_bytes_reserved
    );
    assert_eq!(
        counters.storage_bytes_reserved,
        counters.storage_bytes_released
            + counters.storage_bytes_committed
            + counters.storage_bytes_retained
    );
    assert_eq!(
        counters.storage_inodes_requested,
        counters.storage_inodes_reserved
    );
    assert_eq!(
        counters.storage_inodes_reserved,
        counters.storage_inodes_released
            + counters.storage_inodes_committed
            + counters.storage_inodes_retained
    );
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

#[test]
fn mutation_crosses_reopened_full_and_exact_range_reads_without_serializing_payload_delivery() {
    for (label, range) in [("mutation-full-read", false), ("mutation-range-read", true)] {
        let fixture = TestRoot::new(label);
        let seed = FsCasV1::create_new(fixture.path()).expect("create FsCas");
        let reader_cas = FsCasV1::open_existing(fixture.path()).expect("reopen reader");
        let mutation_cas = FsCasV1::open_existing(fixture.path()).expect("reopen mutator");
        let base_data: Vec<u8> = (0..48_123).map(|index| (index * 37) as u8).collect();
        let replacement_data: Vec<u8> = (0..57_321).map(|index| (index * 19 + 7) as u8).collect();
        let name = b"b.bin";
        let base_file = expected_file(&base_data, 0o644);
        let base_entries = [entry(name, &base_file, 0o644)];
        let base_tree =
            build(DirectoryBuildModeV1::ImplicitRoot, &base_entries).expect("base tree");
        let (base_version, accepted_root) =
            accept_files(&seed, 0x520, &[(name, 0o644, &base_data)]);
        assert_eq!(accepted_root, base_tree.directory.physical());
        let replacement_proof = replacement_fixture(
            DirectoryBuildModeV1::ImplicitRoot,
            &base_entries,
            &base_tree,
            0,
        );
        let selected_offset = if range { 817_u64 } else { 0 };
        let selected_len = if range {
            17_777_u64
        } else {
            base_data.len() as u64
        };
        let expected =
            base_data[selected_offset as usize..(selected_offset + selected_len) as usize].to_vec();
        let release = Arc::new(WatchdogGateV1::new());
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (mutation_done_tx, mutation_done_rx) = mpsc::sync_channel(1);

        let (read_terminal, mutation_terminal) = std::thread::scope(|scope| {
            let mut release_guard = WatchdogGateReleaseV1::new(Arc::clone(&release));
            let read_release = Arc::clone(&release);
            let reader = scope.spawn(move || {
                let mut comparison = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
                let mut path = boxed_zeroes::<MAX_PATH_BYTES>();
                let mut counters = OperationCountersV1::default();
                let mut control = ContinueControl::default();
                let mut sink = BarrierReadSink::new(
                    usize::try_from(selected_len).expect("bounded selected length"),
                    ready_tx,
                    read_release,
                );
                let result = if range {
                    read_file_range_impl_v1(
                        &reader_cas,
                        0x521,
                        base_version,
                        base_tree.directory.physical(),
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
                        base_tree.directory.physical(),
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
                .unwrap_or_else(|error| {
                    panic!("{label}: read never reached payload sink: {error}")
                });

            let mutator = scope.spawn(move || {
                let mut source = SliceSource::new(&replacement_data);
                let mut scratch = OperationScratch::new();
                let mut cow_logical = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
                let mut control = ContinueControl::default();
                let mut counters = OperationCountersV1::default();
                let result = run_complete_replace_v1(
                    &mutation_cas,
                    0x522,
                    CdcAlgorithmV1::FastCdc,
                    base_version,
                    base_tree.directory,
                    replacement_proof.evidence(base_tree.directory),
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
                mutation_done_tx.send(()).unwrap();
                (result, counters)
            });

            let completed_while_reader_was_blocked =
                mutation_done_rx.recv_timeout(Duration::from_secs(5));
            release_guard.release_v1();
            let read_terminal = reader.join().unwrap();
            let mutation_terminal = mutator.join().unwrap();
            completed_while_reader_was_blocked.unwrap_or_else(|error| {
                panic!("{label}: mutation serialized behind external payload delivery: {error}")
            });
            (read_terminal, mutation_terminal)
        });

        let (read_result, sink, read_counters) = read_terminal;
        let read_result = read_result.unwrap_or_else(|error| panic!("{label}: {error:?}"));
        let (mutation_result, mutation_counters) = mutation_terminal;
        let mutation_result = mutation_result.unwrap_or_else(|error| panic!("{label}: {error:?}"));
        assert_eq!(
            read_result.kind(),
            if range {
                ReadKindV1::ExactRange
            } else {
                ReadKindV1::FullExtraction
            }
        );
        assert_eq!(read_result.payload_bytes(), selected_len);
        assert_eq!(sink.bytes, expected);
        assert_eq!(sink.selected_offset, selected_offset);
        assert_eq!(sink.selected_len, selected_len);
        assert!(sink.finished);
        assert!(!sink.aborted);
        assert_ne!(mutation_result.root_tree(), base_tree.directory.physical());
        assert_read_storage_terminal(&read_counters);
        assert_storage_terminal(&mutation_counters);
        assert!(
            read_counters.root_admission_active_slots_high_water >= 2
                || mutation_counters.root_admission_active_slots_high_water >= 2,
            "{label}: root lifetime high-water must record actual caller overlap"
        );
        assert_clean_terminal(&seed, fixture.path());
    }
}

#[test]
fn thirty_two_reopened_readers_and_eight_equal_writers_balance_under_slow_io() {
    const READERS: usize = 32;
    const WRITERS: usize = 8;
    const ROOT_CAPACITY: u64 = 16;

    let fixture = TestRoot::new("32-readers-8-writers");
    let seed = FsCasV1::create_new(fixture.path()).expect("create FsCas");
    let reader_handles = (0..READERS)
        .map(|_| FsCasV1::open_existing(fixture.path()).expect("reopen reader"))
        .collect::<Vec<_>>();
    let writer_handles = (0..WRITERS)
        .map(|_| FsCasV1::open_existing(fixture.path()).expect("reopen writer"))
        .collect::<Vec<_>>();
    let base_data: Vec<u8> = (0..64_321).map(|index| (index * 37 + 11) as u8).collect();
    let replacement_data: Vec<u8> = (0..72_119).map(|index| (index * 19 + 7) as u8).collect();
    let name = b"load.bin";
    let base_file = expected_file(&base_data, 0o644);
    let base_entries = [entry(name, &base_file, 0o644)];
    let base_tree = build(DirectoryBuildModeV1::ImplicitRoot, &base_entries).expect("base tree");
    let (base_version, accepted_root) = accept_files(&seed, 0x580, &[(name, 0o644, &base_data)]);
    assert_eq!(accepted_root, base_tree.directory.physical());
    let replacement_proof = replacement_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &base_entries,
        &base_tree,
        0,
    );
    let before = exact_operation_namespace_usage(fixture.path());
    let before_carriers = fs::read_dir(fixture.path().join("carriers"))
        .expect("seed carrier namespace")
        .count();

    let active_read_start = Arc::new(WatchdogGateV1::new());
    let waiting_read_start = Arc::new(WatchdogGateV1::new());
    // The delivery gate holds fifteen admitted readers without pretending
    // that sink delivery is occupied-carrier I/O. Reader zero is held at the
    // actual CarrierPayloadRead boundary below, so all sixteen admission slots
    // are occupied before the remaining callers are released.
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
        contention_elapsed,
        cancellation_terminal_latency,
        deadline_terminal_latency,
    ) = std::thread::scope(|scope| {
        let mut abort_on_drop = LoadAbortOnDropV1::new(Arc::clone(&abort));
        let mut active_read_start_release =
            WatchdogGateReleaseV1::new(Arc::clone(&active_read_start));
        let mut waiting_read_start_release =
            WatchdogGateReleaseV1::new(Arc::clone(&waiting_read_start));
        let mut reader_delivery_release =
            WatchdogGateReleaseV1::new(Arc::clone(&reader_delivery_gate));
        let mut occupied_read_release = WatchdogGateReleaseV1::new(Arc::clone(&occupied_read_gate));
        let mut winner_writer_start_release =
            WatchdogGateReleaseV1::new(Arc::clone(&winner_writer_start));
        let mut adopting_writer_start_release =
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
                let delivery_gate = Arc::clone(&reader_delivery_gate);
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
                scope.spawn(move || {
                    ready
                        .send(())
                        .expect("reader readiness receiver remains live");
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
                        GatedReadSink::new(base_data.len(), delivery_entered, delivery_gate);
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
                        .expect("reader completion receiver remains live");
                    (terminal, sink, counters)
                })
            })
            .collect::<Vec<_>>();

        for index in 0..READERS {
            read_ready_rx
                .recv_timeout(Duration::from_secs(5))
                .unwrap_or_else(|error| {
                    panic!("reader readiness {index}/{READERS} failed: {error}")
                });
        }
        let contention_started_at = Instant::now();
        active_read_start_release.release_v1();
        occupied_read_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("selected reader missed the occupied-carrier payload boundary");
        for index in 0..ROOT_CAPACITY - 1 {
            reader_delivery_rx
                .recv_timeout(Duration::from_secs(10))
                .unwrap_or_else(|error| {
                    panic!(
                        "admitted reader {index}/{} missed the delivery hold: {error}",
                        ROOT_CAPACITY - 1
                    )
                });
        }

        assert_eq!(seed.operation_admission_queue_for_test_v1(), (0, 0, 0));
        waiting_read_start_release.release_v1();
        let reader_queue_deadline = Instant::now() + Duration::from_secs(5);
        while seed.operation_admission_active_for_test_v1() != ROOT_CAPACITY
            || seed.operation_admission_queue_for_test_v1() != (16, 16, 0)
            || cancelled_reader_polls.load(Ordering::Acquire) < 2
            || deadline_reader_polls.load(Ordering::Acquire) < 2
        {
            if Instant::now() >= reader_queue_deadline {
                panic!(
                    "32 readers did not reach the exact 16-active/16-waiting state: active={}, queue={:?}",
                    seed.operation_admission_active_for_test_v1(),
                    seed.operation_admission_queue_for_test_v1()
                );
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
        for ordinal in 0..2 {
            let (index, terminal_at) = read_done_rx
                .recv_timeout(Duration::from_secs(5))
                .unwrap_or_else(|error| {
                    panic!("stopped reader {ordinal}/2 did not terminalize: {error}")
                });
            match index {
                30 => cancellation_terminal_at = Some(terminal_at),
                31 => deadline_terminal_at = Some(terminal_at),
                other => panic!("reader {other} terminalized before a stopped waiter"),
            }
            stopped_readers.push(index);
        }
        stopped_readers.sort_unstable();
        assert_eq!(stopped_readers, [30, 31]);
        let cancellation_terminal_latency = cancellation_terminal_at
            .expect("cancelled reader terminal timestamp")
            .duration_since(cancellation_armed_at);
        let deadline_terminal_latency = deadline_terminal_at
            .expect("deadline reader terminal timestamp")
            .duration_since(deadline_armed_at);
        let stopped_queue_deadline = Instant::now() + Duration::from_secs(5);
        while seed.operation_admission_queue_for_test_v1() != (16, 14, 2) {
            assert!(
                Instant::now() < stopped_queue_deadline,
                "cancelled/deadline reader tickets did not retire: {:?}",
                seed.operation_admission_queue_for_test_v1()
            );
            std::thread::yield_now();
        }

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
                let replacement_proof = &replacement_proof;
                scope.spawn(move || {
                    ready
                        .send(())
                        .expect("writer readiness receiver remains live");
                    start.wait();
                    let mut source = SliceSource::new(replacement_data);
                    let mut scratch = OperationScratch::new();
                    let mut cow_logical = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
                    let mut counters = OperationCountersV1::default();
                    let mut control = LoadRowControl {
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
                    let terminal = run_complete_replace_v1(
                        &cas,
                        0x5a1 + index as u64,
                        CdcAlgorithmV1::FastCdc,
                        base_version,
                        base_tree.directory,
                        replacement_proof.evidence(base_tree.directory),
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
                    done.send((index, Instant::now()))
                        .expect("writer completion receiver remains live");
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

        for index in 0..WRITERS {
            writer_ready_rx
                .recv_timeout(Duration::from_secs(5))
                .unwrap_or_else(|error| {
                    panic!("writer readiness {index}/{WRITERS} failed: {error}")
                });
        }
        winner_writer_start_release.release_v1();
        let winner_queue_deadline = Instant::now() + Duration::from_secs(5);
        while seed.operation_admission_queue_for_test_v1() != (17, 15, 2) {
            assert!(
                Instant::now() < winner_queue_deadline,
                "faulted writer did not queue behind readers: {:?}",
                seed.operation_admission_queue_for_test_v1()
            );
            std::thread::yield_now();
        }
        adopting_writer_start_release.release_v1();
        let writer_queue_deadline = Instant::now() + Duration::from_secs(5);
        while seed.operation_admission_active_for_test_v1() != ROOT_CAPACITY
            || seed.operation_admission_queue_for_test_v1() != (24, 22, 2)
        {
            if Instant::now() >= writer_queue_deadline {
                panic!(
                    "32 readers and 8 writers did not overlap as 16 active/22 waiting after two stopped readers: active={}, queue={:?}",
                    seed.operation_admission_active_for_test_v1(),
                    seed.operation_admission_queue_for_test_v1()
                );
            }
            std::thread::yield_now();
        }

        reader_delivery_release.release_v1();
        carrier_winner_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("equal-writer winner did not reach post-carrier gate");
        let mut active_waits = 0;
        while active_waits < WRITERS - 1 {
            match active_wait_rx.recv_timeout(Duration::from_secs(10)) {
                Ok(()) => active_waits += 1,
                Err(error) => {
                    panic!(
                        "only {active_waits}/{} equal-writer adopters reached active-owner wait: {error}",
                        WRITERS - 1
                    );
                }
            }
        }
        carrier_winner_release.release_v1();
        comparison_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("equal-writer adopter did not reach the slow comparison boundary");
        comparison_release.release_v1();
        occupied_read_release.release_v1();

        let completion_deadline = Instant::now() + Duration::from_secs(15);
        for ordinal in 0..READERS - 2 {
            read_done_rx
                .recv_timeout(completion_deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|error| {
                    panic!(
                        "reader completion {ordinal}/{} failed: {error}",
                        READERS - 2
                    )
                });
        }
        for ordinal in 0..WRITERS {
            writer_done_rx
                .recv_timeout(completion_deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|error| {
                    panic!("writer completion {ordinal}/{WRITERS} failed: {error}")
                });
        }

        let readers = reader_joins
            .into_iter()
            .map(|join| join.join().expect("reader thread remains healthy"))
            .collect::<Vec<_>>();
        let writers = writer_joins
            .into_iter()
            .map(|join| join.join().expect("writer thread remains healthy"))
            .collect::<Vec<_>>();
        let contention_elapsed = contention_started_at.elapsed();
        abort_on_drop.disarm_v1();
        (
            readers,
            writers,
            contention_elapsed,
            cancellation_terminal_latency,
            deadline_terminal_latency,
        )
    });

    let mut observed_admission_high_water = 0_u64;
    let mut queued_reader_tokens = 0_usize;
    let mut reader_successes = 0_usize;
    let mut reader_cancelled = 0_usize;
    let mut reader_deadlines = 0_usize;
    let mut visibility_wait_nanoseconds = 0_u64;
    let mut visibility_hold_nanoseconds = 0_u64;
    let mut publication_wait_nanoseconds = 0_u64;
    let mut publication_hold_nanoseconds = 0_u64;
    let mut admission_wait_nanoseconds = 0_u64;
    let mut active_publication_wait_nanoseconds = 0_u64;
    for (index, (terminal, sink, counters)) in reader_results.into_iter().enumerate() {
        match index {
            30 | 31 => {
                let expected = if index == 30 {
                    CoreError::Cancelled
                } else {
                    CoreError::Deadline
                };
                assert_eq!(
                    terminal,
                    Err(
                        layerfs_storage::read::extraction::ReadOperationErrorV1::FsCas(
                            FsCasErrorV1::Core(expected),
                        ),
                    ),
                    "queued reader {index} must retain its typed terminal"
                );
                assert!(sink.bytes.is_empty());
                assert!(!sink.finished);
                assert!(!sink.aborted);
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
                assert_eq!(counters.unreachable_installed_residue_bytes, 0);
                assert_eq!(counters.visibility_lock_acquisitions, 0);
                assert_eq!(counters.publication_lock_acquisitions, 0);
                assert!(counters.root_admission_wait_polls > 0);
                assert!(counters.root_admission_wait_nanoseconds > 0);
                assert_eq!(counters.root_admission_active_slots_high_water, 0);
                assert_eq!(counters.root_admission_queue_entries, 1);
                assert_eq!(counters.root_admission_queue_refusals, 0);
                assert!(counters.has_zero_forbidden_work());
                if index == 30 {
                    reader_cancelled += 1;
                } else {
                    reader_deadlines += 1;
                }
            }
            _ => {
                let result =
                    terminal.unwrap_or_else(|error| panic!("reader {index} terminal: {error:?}"));
                assert_eq!(result.kind(), ReadKindV1::FullExtraction);
                assert_eq!(result.payload_bytes(), base_data.len() as u64);
                assert_eq!(sink.bytes, base_data);
                assert!(sink.finished);
                assert!(!sink.aborted);
                assert_read_storage_terminal(&counters);
                assert_eq!(counters.root_admission_queue_entries, 1);
                assert_eq!(counters.root_admission_queue_refusals, 0);
                reader_successes += 1;
            }
        }
        visibility_wait_nanoseconds = visibility_wait_nanoseconds
            .checked_add(counters.visibility_lock_wait_nanoseconds)
            .expect("bounded aggregate visibility wait");
        visibility_hold_nanoseconds = visibility_hold_nanoseconds
            .checked_add(counters.visibility_lock_hold_nanoseconds)
            .expect("bounded aggregate visibility hold");
        publication_wait_nanoseconds = publication_wait_nanoseconds
            .checked_add(counters.publication_lock_wait_nanoseconds)
            .expect("bounded aggregate publication wait");
        publication_hold_nanoseconds = publication_hold_nanoseconds
            .checked_add(counters.publication_lock_hold_nanoseconds)
            .expect("bounded aggregate publication hold");
        admission_wait_nanoseconds = admission_wait_nanoseconds
            .checked_add(counters.root_admission_wait_nanoseconds)
            .expect("bounded aggregate admission wait");
        active_publication_wait_nanoseconds = active_publication_wait_nanoseconds
            .checked_add(counters.active_pack_publication_wait_nanoseconds)
            .expect("bounded aggregate active-publication wait");
        observed_admission_high_water =
            observed_admission_high_water.max(counters.root_admission_active_slots_high_water);
        queued_reader_tokens += usize::from(counters.root_admission_wait_polls > 0);
        if counters.root_admission_wait_polls > 0 {
            assert!(counters.root_admission_wait_nanoseconds > 0);
        }
    }
    assert!(queued_reader_tokens >= READERS - ROOT_CAPACITY as usize);

    let mut canonical_version = None;
    let mut canonical_root = None;
    let mut canonical_carrier_count = None;
    let mut installed_carriers = 0_u32;
    let mut reused_carriers = 0_u32;
    let mut total_committed_bytes = 0_u64;
    let mut total_committed_inodes = 0_u64;
    let mut total_reserved_bytes = 0_u64;
    let mut total_reserved_inodes = 0_u64;
    let mut active_wait_tokens = 0_usize;
    let mut delayed_comparison_windows = 0_u64;
    let mut recorded_comparison_windows = 0_u64;
    let mut observed_root_bytes_high_water = 0_u64;
    let mut observed_root_inodes_high_water = 0_u64;
    let mut faulted_writers = 0_usize;
    let mut successful_writers = 0_usize;
    for (
        index,
        (
            terminal,
            counters,
            delayed_windows,
            carrier_winner_reported,
            catalog_phase,
            catalog_commit_failed,
        ),
    ) in writer_results.into_iter().enumerate()
    {
        if catalog_commit_failed {
            assert!(carrier_winner_reported);
            assert!(catalog_phase);
            assert_eq!(
                terminal,
                Err(layerfs_storage::lifecycle::OperationErrorV1::FsCas(
                    FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::NoSpace),
                ),),
                "the actual carrier winner must retain the injected catalog-write failure"
            );
            faulted_writers += 1;
            assert_eq!(delayed_windows, 0);
            assert_eq!(counters.incumbent_comparison_windows, 0);
            assert_eq!(counters.storage_bytes_committed, 0);
            assert_eq!(counters.storage_inodes_committed, 0);
        } else {
            let handoff =
                terminal.unwrap_or_else(|error| panic!("writer {index} terminal: {error:?}"));
            if let Some(version) = canonical_version {
                assert_eq!(handoff.version_record(), version);
                assert_eq!(handoff.root_tree(), canonical_root.unwrap());
                assert_eq!(handoff.carrier_count(), canonical_carrier_count.unwrap());
            } else {
                canonical_version = Some(handoff.version_record());
                canonical_root = Some(handoff.root_tree());
                canonical_carrier_count = Some(handoff.carrier_count());
            }
            installed_carriers += handoff.carriers_installed();
            reused_carriers += handoff.carriers_reused();
            successful_writers += 1;
        }
        assert_balanced_storage_terminal(&counters);
        assert!(counters.visibility_lock_acquisitions > 0);
        assert!(counters.visibility_lock_wait_nanoseconds > 0);
        assert!(counters.visibility_lock_hold_nanoseconds > 0);
        assert!(counters.visibility_lock_hold_nanoseconds_high_water > 0);
        assert!(
            counters.visibility_lock_hold_nanoseconds
                >= counters.visibility_lock_hold_nanoseconds_high_water
        );
        assert!(counters.publication_lock_acquisitions > 0);
        assert!(counters.publication_lock_wait_nanoseconds > 0);
        assert!(counters.publication_lock_hold_nanoseconds > 0);
        assert!(counters.publication_lock_hold_nanoseconds_high_water > 0);
        assert!(
            counters.publication_lock_hold_nanoseconds
                >= counters.publication_lock_hold_nanoseconds_high_water
        );
        assert_eq!(counters.root_admission_queue_entries, 1);
        assert_eq!(counters.root_admission_queue_refusals, 0);
        assert!(counters.root_admission_wait_polls > 0);
        assert!(counters.root_admission_wait_nanoseconds > 0);
        assert_eq!(counters.locator_owner_publication_wait_polls, 0);
        assert_eq!(counters.locator_owner_publication_wait_nanoseconds, 0);
        assert!(counters.storage_preparation_bytes_high_water > 0);
        assert!(counters.storage_preparation_inodes_high_water > 0);
        assert!(counters.maximum_active_carrier_bytes > 0);
        assert!(counters.layerfs_open_file_handles_high_water > 0);
        active_wait_tokens += usize::from(counters.active_pack_publication_wait_polls > 0);
        if counters.active_pack_publication_wait_polls > 0 {
            assert!(counters.active_pack_publication_wait_nanoseconds > 0);
        }
        delayed_comparison_windows += delayed_windows;
        recorded_comparison_windows += counters.incumbent_comparison_windows;
        total_committed_bytes += counters.storage_bytes_committed;
        total_committed_inodes += counters.storage_inodes_committed;
        total_reserved_bytes += counters.storage_bytes_reserved;
        total_reserved_inodes += counters.storage_inodes_reserved;
        observed_admission_high_water =
            observed_admission_high_water.max(counters.root_admission_active_slots_high_water);
        observed_root_bytes_high_water = observed_root_bytes_high_water
            .max(counters.root_storage_active_reserved_bytes_lifetime_high_water);
        observed_root_inodes_high_water = observed_root_inodes_high_water
            .max(counters.root_storage_active_reserved_inodes_lifetime_high_water);
        visibility_wait_nanoseconds = visibility_wait_nanoseconds
            .checked_add(counters.visibility_lock_wait_nanoseconds)
            .expect("bounded aggregate visibility wait");
        visibility_hold_nanoseconds = visibility_hold_nanoseconds
            .checked_add(counters.visibility_lock_hold_nanoseconds)
            .expect("bounded aggregate visibility hold");
        publication_wait_nanoseconds = publication_wait_nanoseconds
            .checked_add(counters.publication_lock_wait_nanoseconds)
            .expect("bounded aggregate publication wait");
        publication_hold_nanoseconds = publication_hold_nanoseconds
            .checked_add(counters.publication_lock_hold_nanoseconds)
            .expect("bounded aggregate publication hold");
        admission_wait_nanoseconds = admission_wait_nanoseconds
            .checked_add(counters.root_admission_wait_nanoseconds)
            .expect("bounded aggregate admission wait");
        active_publication_wait_nanoseconds = active_publication_wait_nanoseconds
            .checked_add(counters.active_pack_publication_wait_nanoseconds)
            .expect("bounded aggregate active-publication wait");
    }

    assert_eq!(observed_admission_high_water, ROOT_CAPACITY);
    assert_eq!(active_wait_tokens, WRITERS - 1);
    assert_eq!(faulted_writers, 1);
    assert_eq!(
        installed_carriers + reused_carriers,
        canonical_carrier_count.unwrap() * (WRITERS - faulted_writers) as u32
    );
    assert!(delayed_comparison_windows > 0);
    assert_eq!(delayed_comparison_windows, recorded_comparison_windows);
    assert!(observed_root_bytes_high_water >= total_reserved_bytes);
    assert!(observed_root_inodes_high_water >= total_reserved_inodes);

    let after = exact_operation_namespace_usage(fixture.path());
    assert_eq!(after.0, (0, 0));
    assert_eq!(before.0, (0, 0));
    assert_eq!(total_committed_bytes, after.1 .0 - before.1 .0);
    assert_eq!(total_committed_inodes, after.1 .1 - before.1 .1);
    let after_carriers = fs::read_dir(fixture.path().join("carriers"))
        .expect("carrier namespace")
        .count();
    assert_eq!(
        installed_carriers as usize,
        after_carriers - before_carriers
    );
    assert_eq!(
        reused_carriers,
        canonical_carrier_count.unwrap() * (WRITERS - faulted_writers - 1) as u32,
        "every non-winning equal carrier must be attributed as canonical reuse"
    );
    assert_eq!(
        fs::read_dir(fixture.path().join("closures"))
            .expect("closure namespace")
            .count(),
        2,
        "the seed and all eight equal writers own exactly two canonical closures"
    );

    let total_terminals = reader_successes
        + reader_cancelled
        + reader_deadlines
        + successful_writers
        + faulted_writers;
    let throughput_numerator = READERS + WRITERS;
    assert!(
        !contention_elapsed.is_zero(),
        "contention interval must be directly observable"
    );
    let report = LoadContentionReportV1 {
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
        final_preparation_bytes: after.0 .0,
        final_preparation_inodes: after.0 .1,
    };
    assert_eq!(report.reader_successes, 30, "{report:?}");
    assert_eq!(report.reader_cancelled, 1, "{report:?}");
    assert_eq!(report.reader_deadlines, 1, "{report:?}");
    assert_eq!(report.writer_successes, 7, "{report:?}");
    assert_eq!(report.writer_faults, 1, "{report:?}");
    assert_eq!(report.total_terminals, 40, "{report:?}");
    assert_eq!(report.throughput_numerator, 40, "{report:?}");
    assert_eq!(
        report.terminals_per_second,
        report.throughput_numerator as f64 / report.elapsed.as_secs_f64(),
        "{report:?}"
    );
    assert!(
        report.terminals_per_second.is_finite() && report.terminals_per_second > 0.0,
        "{report:?}"
    );
    assert!(
        !report.cancellation_terminal_latency.is_zero()
            && report.cancellation_terminal_latency <= Duration::from_secs(15),
        "{report:?}"
    );
    assert!(
        !report.deadline_terminal_latency.is_zero()
            && report.deadline_terminal_latency <= Duration::from_secs(15),
        "{report:?}"
    );
    assert!(report.admission_wait_tokens >= 24, "{report:?}");
    assert!(report.admission_wait_nanoseconds > 0, "{report:?}");
    assert_eq!(
        report.active_publication_wait_tokens,
        WRITERS - 1,
        "{report:?}"
    );
    assert!(report.active_publication_wait_nanoseconds > 0, "{report:?}");
    assert!(report.visibility_wait_nanoseconds > 0, "{report:?}");
    assert!(report.visibility_hold_nanoseconds > 0, "{report:?}");
    assert!(report.publication_wait_nanoseconds > 0, "{report:?}");
    assert!(report.publication_hold_nanoseconds > 0, "{report:?}");
    assert_eq!(report.final_preparation_bytes, 0, "{report:?}");
    assert_eq!(report.final_preparation_inodes, 0, "{report:?}");
    assert_clean_terminal(&seed, fixture.path());
    assert_eq!(seed.operation_admission_queue_for_test_v1(), (0, 0, 0));
    assert!(seed.occupied().is_ok());
    assert!(FsCasV1::open_existing(fixture.path())
        .expect("post-load reopen")
        .occupied()
        .is_ok());
}

fn exact_directory_usage(path: &Path) -> (u64, u64) {
    fs::read_dir(path)
        .expect("operation namespace directory")
        .map(|entry| {
            let entry = entry.expect("operation namespace entry");
            let metadata =
                fs::symlink_metadata(entry.path()).expect("operation namespace metadata");
            assert!(metadata.file_type().is_file());
            (metadata.len(), 1_u64)
        })
        .fold((0_u64, 0_u64), |(bytes, inodes), (len, one)| {
            (
                bytes.checked_add(len).expect("bounded namespace bytes"),
                inodes + one,
            )
        })
}

fn exact_operation_namespace_usage(root: &Path) -> ((u64, u64), (u64, u64)) {
    let preparation = exact_directory_usage(&root.join("preparation"));
    let immutable = ["carriers", "objects", "catalog", "closures"]
        .into_iter()
        .map(|name| exact_directory_usage(&root.join(name)))
        .fold(
            (0_u64, 0_u64),
            |(bytes, inodes), (next_bytes, next_inodes)| {
                (
                    bytes
                        .checked_add(next_bytes)
                        .expect("bounded immutable bytes"),
                    inodes
                        .checked_add(next_inodes)
                        .expect("bounded immutable inodes"),
                )
            },
        );
    (preparation, immutable)
}

#[test]
fn post_install_cleanup_unwind_records_immutable_residue_exactly_once() {
    let fixture = TestRoot::new("post-install-cleanup-unwind");
    let cas = FsCasV1::create_new(fixture.path()).expect("create FsCas");
    let stale = FsCasV1::open_existing(fixture.path()).expect("reopen shared owner");
    let base_data: Vec<u8> = (0..8_123).map(|index| (index * 37) as u8).collect();
    let replacement_data: Vec<u8> = (0..9_321).map(|index| (index * 19 + 7) as u8).collect();
    let name = b"b.bin";
    let base_file = expected_file(&base_data, 0o644);
    let base_entries = [entry(name, &base_file, 0o644)];
    let base_tree = build(DirectoryBuildModeV1::ImplicitRoot, &base_entries).expect("base tree");
    let (base_version, accepted_root) = accept_files(&cas, 0x515, &[(name, 0o644, &base_data)]);
    assert_eq!(accepted_root, base_tree.directory.physical());
    let before = exact_operation_namespace_usage(fixture.path());
    let before_objects = exact_directory_usage(&fixture.path().join("objects"));
    let before_catalog = exact_directory_usage(&fixture.path().join("catalog"));
    let before_closures = exact_directory_usage(&fixture.path().join("closures"));
    let before_carriers: Vec<_> = fs::read_dir(fixture.path().join("carriers"))
        .expect("base carriers")
        .map(|entry| entry.expect("base carrier").file_name())
        .collect();

    let replacement_proof = replacement_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &base_entries,
        &base_tree,
        0,
    );
    let mut source = SliceSource::new(&replacement_data);
    let mut scratch = OperationScratch::new();
    let mut cow_logical = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut control = PanicPrivatePackCleanupAfterInstalledCarrier::default();
    let mut counters = OperationCountersV1::default();
    let terminal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_complete_replace_v1(
            &cas,
            0x516,
            CdcAlgorithmV1::FastCdc,
            base_version,
            base_tree.directory,
            replacement_proof.evidence(base_tree.directory),
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
    }));

    let error = match terminal {
        Ok(Err(error)) => error,
        Ok(Ok(_)) => panic!("cleanup-unwind terminal must not complete"),
        Err(payload) => std::panic::resume_unwind(payload),
    };
    assert_eq!(
        error,
        layerfs_storage::lifecycle::OperationErrorV1::FsCas(FsCasErrorV1::TerminalFailure {
            first: layerfs_storage::cas::FsCasFailureCauseV1::Core(CoreError::Cancelled),
            dominant: layerfs_storage::cas::FsCasFailureCauseV1::CleanupFailed(
                FsCasCleanupTargetV1::PrivatePack
            ),
        })
    );
    assert!(control.after_catalog_publication);
    assert!(control.publication_poll_passed);
    assert!(control.cleanup_panicked);
    assert_eq!(cas.operation_admitted_slots_v1(), 0);
    assert_eq!(cas.operation_admission_active_for_test_v1(), 0);
    assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));

    let new_carriers: Vec<_> = fs::read_dir(fixture.path().join("carriers"))
        .expect("terminal carriers")
        .map(|entry| entry.expect("terminal carrier"))
        .filter(|entry| !before_carriers.contains(&entry.file_name()))
        .collect();
    assert_eq!(new_carriers.len(), 1);
    let exact_unreachable_carrier_bytes = new_carriers[0]
        .metadata()
        .expect("unreachable carrier metadata")
        .len();
    assert!(exact_unreachable_carrier_bytes > 0);

    let after = exact_operation_namespace_usage(fixture.path());
    let after_objects = exact_directory_usage(&fixture.path().join("objects"));
    let after_catalog = exact_directory_usage(&fixture.path().join("catalog"));
    let after_closures = exact_directory_usage(&fixture.path().join("closures"));
    let locator_delta_bytes = after_objects
        .0
        .checked_sub(before_objects.0)
        .expect("locator byte delta");
    let locator_delta_inodes = after_objects
        .1
        .checked_sub(before_objects.1)
        .expect("locator inode delta");
    let catalog_delta_bytes = after_catalog
        .0
        .checked_sub(before_catalog.0)
        .expect("catalog byte delta");
    let catalog_delta_inodes = after_catalog
        .1
        .checked_sub(before_catalog.1)
        .expect("catalog inode delta");
    let closure_delta_bytes = after_closures
        .0
        .checked_sub(before_closures.0)
        .expect("closure byte delta");
    let closure_delta_inodes = after_closures
        .1
        .checked_sub(before_closures.1)
        .expect("closure inode delta");
    let immutable_delta_bytes = after
        .1
         .0
        .checked_sub(before.1 .0)
        .expect("immutable byte delta");
    let immutable_delta_inodes = after
        .1
         .1
        .checked_sub(before.1 .1)
        .expect("immutable inode delta");
    assert_eq!(after.0 .1, 1);
    assert!(locator_delta_inodes > 0);
    assert_eq!(catalog_delta_inodes, 1);
    assert_eq!((closure_delta_bytes, closure_delta_inodes), (0, 0));
    assert_eq!(
        immutable_delta_bytes,
        exact_unreachable_carrier_bytes + locator_delta_bytes + catalog_delta_bytes,
    );
    assert_eq!(
        immutable_delta_inodes,
        1 + locator_delta_inodes + catalog_delta_inodes,
    );
    assert_eq!(
        counters.unreachable_installed_residue_bytes,
        immutable_delta_bytes,
    );
    assert_eq!(
        counters.storage_bytes_requested,
        counters.storage_bytes_reserved
    );
    assert_eq!(
        counters.storage_bytes_reserved,
        counters.storage_bytes_released
            + counters.storage_bytes_committed
            + counters.storage_bytes_retained,
    );
    assert_eq!(
        counters.storage_inodes_requested,
        counters.storage_inodes_reserved
    );
    assert_eq!(
        counters.storage_inodes_reserved,
        counters.storage_inodes_released
            + counters.storage_inodes_committed
            + counters.storage_inodes_retained,
    );
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert_eq!(
        counters.storage_bytes_retained,
        after.0 .0 + immutable_delta_bytes,
    );
    assert_eq!(
        counters.storage_inodes_retained,
        after.0 .1 + immutable_delta_inodes,
    );
    assert!(counters.has_zero_forbidden_work());
    assert!(fixture.path().join("invalidated").is_dir());
    assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(
        FsCasV1::open_existing(fixture.path()),
        Err(FsCasErrorV1::Invalidated)
    ));
}

#[test]
fn complete_replace_and_metadata_reach_independently_derived_handoffs() {
    let fixture = TestRoot::new("replace-metadata");
    let cas = FsCasV1::create_new(fixture.path()).expect("create FsCas");
    let base_data: Vec<u8> = (0..48_123).map(|index| (index * 37) as u8).collect();
    let replacement_data: Vec<u8> = (0..57_321).map(|index| (index * 19 + 7) as u8).collect();
    let name = b"b.bin";
    let base_file = expected_file(&base_data, 0o644);
    let base_entries = [entry(name, &base_file, 0o644)];
    let base_tree = build(DirectoryBuildModeV1::ImplicitRoot, &base_entries).expect("base tree");
    let (base_version, accepted_root) = accept_files(&cas, 0x510, &[(name, 0o644, &base_data)]);
    assert_eq!(accepted_root, base_tree.directory.physical());

    let replacement_file = expected_file(&replacement_data, 0o600);
    let replacement_entries = [entry(name, &replacement_file, 0o600)];
    let replacement_tree =
        build(DirectoryBuildModeV1::ImplicitRoot, &replacement_entries).expect("replacement tree");
    let replacement_proof = replacement_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &base_entries,
        &base_tree,
        0,
    );
    let mut source = SliceSource::new(&replacement_data);
    let mut scratch = OperationScratch::new();
    let mut cow_logical = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut control = ContinueControl::default();
    let mut counters = OperationCountersV1::default();
    let replaced = run_complete_replace_v1(
        &cas,
        0x511,
        CdcAlgorithmV1::FastCdc,
        base_version,
        base_tree.directory,
        replacement_proof.evidence(base_tree.directory),
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
    .unwrap_or_else(|error| {
        panic!(
            "complete Replace handoff: {error:?}; boundaries={:?}",
            control.boundaries
        )
    });
    assert_eq!(replaced.root_tree(), replacement_tree.directory.physical());
    assert_storage_terminal(&counters);
    assert_clean_terminal(&cas, fixture.path());

    let metadata_file = expected_file(&replacement_data, 0o640);
    assert_ne!(metadata_file.physical, replacement_file.physical);
    let metadata_entries = [entry(name, &metadata_file, 0o640)];
    let metadata_tree =
        build(DirectoryBuildModeV1::ImplicitRoot, &metadata_entries).expect("metadata tree");
    let metadata_proof = replacement_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &replacement_entries,
        &replacement_tree,
        0,
    );
    let mut evidence = replacement_file.evidence();
    let mut scratch = OperationScratch::new();
    let mut counters = OperationCountersV1::default();
    let metadata = run_complete_metadata_v1(
        &cas,
        0x512,
        replaced.version_record(),
        replacement_tree.directory,
        metadata_proof.evidence(replacement_tree.directory),
        0,
        name,
        0o640,
        replacement_file.authenticated(0o600),
        &mut evidence,
        scratch.borrow(),
        &mut cow_logical,
        &mut control,
        &mut counters,
    )
    .expect("complete metadata handoff");
    assert_eq!(metadata.root_tree(), metadata_tree.directory.physical());
    assert_storage_terminal(&counters);
    assert_clean_terminal(&cas, fixture.path());
}

#[test]
fn complete_add_move_and_remove_use_one_candidate_graph_each() {
    let fixture = TestRoot::new("add-move-remove");
    let cas = FsCasV1::create_new(fixture.path()).expect("create FsCas");
    let b_data: Vec<u8> = (0..24_321).map(|index| (index * 11) as u8).collect();
    let d_data: Vec<u8> = (0..31_777).map(|index| (index * 29 + 3) as u8).collect();
    let b_file = expected_file(&b_data, 0o644);
    let d_file = expected_file(&d_data, 0o600);
    let b_name = b"b.bin";
    let d_name = b"d.bin";
    let a_name = b"a.bin";

    let base_entries = [entry(b_name, &b_file, 0o644)];
    let base_tree = build(DirectoryBuildModeV1::ImplicitRoot, &base_entries).expect("base tree");
    let (base_version, accepted_root) = accept_files(&cas, 0x520, &[(b_name, 0o644, &b_data)]);
    assert_eq!(accepted_root, base_tree.directory.physical());

    let added_entries = [entry(b_name, &b_file, 0o644), entry(d_name, &d_file, 0o600)];
    let added_tree = build(DirectoryBuildModeV1::ImplicitRoot, &added_entries).expect("added tree");
    let add_fixture = mutation_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &base_entries,
        &base_tree,
        1,
    );
    let mut tree_source = MutationSource::new(&base_entries, &added_entries);
    let mut source = SliceSource::new(&d_data);
    let mut scratch = OperationScratch::new();
    let mut cow_logical = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut control = ContinueControl::default();
    let mut counters = OperationCountersV1::default();
    let added = run_complete_add_v1(
        &cas,
        0x521,
        base_version,
        base_tree.directory,
        add_fixture.evidence(base_tree.directory),
        1,
        d_name,
        0o600,
        d_data.len() as u64,
        &mut source,
        &mut tree_source,
        scratch.borrow(),
        &mut cow_logical,
        &mut control,
        &mut counters,
    )
    .unwrap_or_else(|error| {
        panic!(
            "complete Add handoff: {error:?}; boundaries={:?}",
            control.boundaries
        )
    });
    assert_eq!(added.root_tree(), added_tree.directory.physical());
    assert_storage_terminal(&counters);

    let moved_entries = [entry(a_name, &d_file, 0o600), entry(b_name, &b_file, 0o644)];
    let moved_tree = build(DirectoryBuildModeV1::ImplicitRoot, &moved_entries).expect("moved tree");
    let move_fixture = mutation_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &added_entries,
        &added_tree,
        0,
    );
    let mut tree_source = MutationSource::new(&added_entries, &moved_entries);
    let mut scratch = OperationScratch::new();
    let mut counters = OperationCountersV1::default();
    let moved = run_complete_move_v1(
        &cas,
        0x522,
        added.version_record(),
        added_tree.directory,
        move_fixture.evidence(added_tree.directory),
        1,
        0,
        added_entries[1],
        a_name,
        &mut tree_source,
        scratch.borrow(),
        &mut cow_logical,
        &mut control,
        &mut counters,
    )
    .unwrap_or_else(|error| {
        panic!(
            "complete Move handoff: {error:?}; boundaries={:?}",
            control.boundaries
        )
    });
    assert_eq!(moved.root_tree(), moved_tree.directory.physical());
    assert_storage_terminal(&counters);

    let removed_entries = [entry(b_name, &b_file, 0o644)];
    let removed_tree =
        build(DirectoryBuildModeV1::ImplicitRoot, &removed_entries).expect("removed tree");
    let remove_fixture = mutation_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &moved_entries,
        &moved_tree,
        0,
    );
    let mut tree_source = MutationSource::new(&moved_entries, &removed_entries);
    let mut scratch = OperationScratch::new();
    let mut counters = OperationCountersV1::default();
    let removed = run_complete_remove_v1(
        &cas,
        0x523,
        moved.version_record(),
        moved_tree.directory,
        remove_fixture.evidence(moved_tree.directory),
        0,
        moved_entries[0],
        &mut tree_source,
        scratch.borrow(),
        &mut cow_logical,
        &mut control,
        &mut counters,
    )
    .expect("complete Remove handoff");
    assert_eq!(removed.root_tree(), removed_tree.directory.physical());
    assert_eq!(removed.root_tree(), base_tree.directory.physical());
    assert_storage_terminal(&counters);
    assert_clean_terminal(&cas, fixture.path());
}

#[test]
fn complete_cross_directory_move_detaches_and_attaches_in_one_handoff() {
    let fixture = TestRoot::new("cross-directory-move");
    let cas = FsCasV1::create_new(fixture.path()).expect("create FsCas");
    let moved_data: Vec<u8> = (0..28_417).map(|index| (index * 17 + 5) as u8).collect();
    let resident_data: Vec<u8> = (0..19_007).map(|index| (index * 31 + 9) as u8).collect();
    let moved_file = expected_file(&moved_data, 0o644);
    let resident_file = expected_file(&resident_data, 0o600);

    let source_base_entries = [entry(b"old.bin", &moved_file, 0o644)];
    let source_result_entries: [CanonicalTreeEntryV1<'_>; 0] = [];
    let destination_base_entries = [entry(b"z.bin", &resident_file, 0o600)];
    let destination_result_entries = [
        entry(b"moved.bin", &moved_file, 0o644),
        entry(b"z.bin", &resident_file, 0o600),
    ];
    let source_base = build(DirectoryBuildModeV1::Explicit(0o755), &source_base_entries)
        .expect("source base directory");
    let source_result = build(
        DirectoryBuildModeV1::Explicit(0o755),
        &source_result_entries,
    )
    .expect("source result directory");
    let destination_base = build(
        DirectoryBuildModeV1::Explicit(0o755),
        &destination_base_entries,
    )
    .expect("destination base directory");
    let destination_result = build(
        DirectoryBuildModeV1::Explicit(0o755),
        &destination_result_entries,
    )
    .expect("destination result directory");

    let root_base_entries = [
        directory_entry(b"left", source_base.directory),
        directory_entry(b"right", destination_base.directory),
    ];
    let root_result_entries = [
        directory_entry(b"left", source_result.directory),
        directory_entry(b"right", destination_result.directory),
    ];
    let root_base =
        build(DirectoryBuildModeV1::ImplicitRoot, &root_base_entries).expect("base root directory");
    let root_result = build(DirectoryBuildModeV1::ImplicitRoot, &root_result_entries)
        .expect("result root directory");
    let (base_version, accepted_root) = accept_files(
        &cas,
        0x524,
        &[
            (b"left/old.bin", 0o644, moved_data.as_slice()),
            (b"right/z.bin", 0o600, resident_data.as_slice()),
        ],
    );
    assert_eq!(accepted_root, root_base.directory.physical());

    let source_fixture = mutation_fixture(
        DirectoryBuildModeV1::Explicit(0o755),
        &source_base_entries,
        &source_base,
        0,
    );
    let destination_fixture = mutation_fixture(
        DirectoryBuildModeV1::Explicit(0o755),
        &destination_base_entries,
        &destination_base,
        0,
    );
    let root_fixture = mutation_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &root_base_entries,
        &root_base,
        0,
    );
    let mut source_view = MutationSource::new(&source_base_entries, &source_result_entries);
    let mut destination_view =
        MutationSource::new(&destination_base_entries, &destination_result_entries);
    let mut root_view = MutationSource::new(&root_base_entries, &root_result_entries);
    let mut scratch = OperationScratch::new();
    let mut cow_logical = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut control = ContinueControl::default();
    let mut counters = OperationCountersV1::default();

    let moved = complete_cross_directory_move_operation_v1(
        &cas,
        0x525,
        base_version,
        root_base.directory,
        root_fixture.evidence(root_base.directory),
        0,
        root_base_entries[0],
        source_base.directory,
        source_fixture.evidence(source_base.directory),
        0,
        source_base_entries[0],
        &mut source_view,
        1,
        root_base_entries[1],
        destination_base.directory,
        destination_fixture.evidence(destination_base.directory),
        0,
        b"moved.bin",
        &mut destination_view,
        &mut root_view,
        scratch.borrow(),
        &mut cow_logical,
        &mut control,
        &mut counters,
    )
    .unwrap_or_else(|error| {
        panic!(
            "complete cross-directory Move handoff: {error:?}; boundaries={:?}; counters={counters:?}",
            control.boundaries,
        )
    });

    assert_eq!(moved.root_tree(), root_result.directory.physical());
    assert_eq!(
        control
            .boundaries
            .iter()
            .filter(|boundary| **boundary == FsCasBoundaryV1::AfterCompleteValidatedHandoff)
            .count(),
        1
    );
    assert_storage_terminal(&counters);
    assert_clean_terminal(&cas, fixture.path());
}

#[test]
fn complete_update_authenticates_and_rejoins_without_replace_fallback() {
    let fixture = TestRoot::new("update");
    let cas = FsCasV1::create_new(fixture.path()).expect("create FsCas");
    // This deterministic counter-PRNG fixture and range are independently
    // proven by the bounded Update suite to establish an exact FastCDC
    // suffix rejoin. A non-rejoining edit must fail closed rather than being
    // silently widened into Replace.
    let base_data = crate::test_support::fastcdc_golden_input(300_000);
    let inserted = b"changed";
    let range = UpdateRangeV1::new(120_000, 120_010, base_data.len() as u64).expect("range");
    let mut result_data = Vec::new();
    result_data.extend_from_slice(&base_data[..range.start() as usize]);
    result_data.extend_from_slice(inserted);
    result_data.extend_from_slice(&base_data[range.end() as usize..]);

    let name = b"b.bin";
    let base_file = expected_file(&base_data, 0o644);
    let result_file = expected_file(&result_data, 0o644);
    let base_entries = [entry(name, &base_file, 0o644)];
    let result_entries = [entry(name, &result_file, 0o644)];
    let base_tree = build(DirectoryBuildModeV1::ImplicitRoot, &base_entries).expect("base tree");
    let result_tree =
        build(DirectoryBuildModeV1::ImplicitRoot, &result_entries).expect("result tree");
    let (base_version, accepted_root) = accept_files(&cas, 0x530, &[(name, 0o644, &base_data)]);
    assert_eq!(accepted_root, base_tree.directory.physical());
    let replacement_proof = replacement_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &base_entries,
        &base_tree,
        0,
    );

    let mut inserted_source = SliceSource::new(inserted);
    let mut base_reader = BaseBytes { bytes: &base_data };
    let mut evidence = base_file.evidence();
    let mut scratch = OperationScratch::new();
    let mut cow_logical = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut control = ContinueControl::default();
    let mut counters = OperationCountersV1::default();
    let updated = run_complete_update_v1(
        &cas,
        0x531,
        base_version,
        base_tree.directory,
        replacement_proof.evidence(base_tree.directory),
        0,
        name,
        0o644,
        base_file.authenticated(0o644),
        range,
        inserted.len() as u64,
        &mut inserted_source,
        &mut base_reader,
        &mut evidence,
        scratch.borrow(),
        &mut cow_logical,
        &mut control,
        &mut counters,
    )
    .unwrap_or_else(|error| {
        panic!(
            "complete Update handoff: {error:?}; boundaries={:?}",
            control.boundaries
        )
    });
    assert_eq!(updated.algorithm(), CdcAlgorithmV1::FastCdc);
    assert_eq!(updated.root_tree(), result_tree.directory.physical());
    assert_storage_terminal(&counters);
    assert_clean_terminal(&cas, fixture.path());
}

#[test]
fn complete_update_reference_metadata_overflow_is_transactional_and_terminal() {
    let fixture = TestRoot::new("update-reference-metadata-overflow");
    let cas = FsCasV1::create_new(fixture.path()).expect("create FsCas");
    let base_data = crate::test_support::fastcdc_golden_input(300_000);
    let inserted = b"changed";
    let range = UpdateRangeV1::new(120_000, 120_010, base_data.len() as u64).expect("range");
    let name = b"b.bin";
    let base_file = expected_file(&base_data, 0o644);
    let base_entries = [entry(name, &base_file, 0o644)];
    let base_tree = build(DirectoryBuildModeV1::ImplicitRoot, &base_entries).expect("base tree");
    let (base_version, accepted_root) = accept_files(&cas, 0x532, &[(name, 0o644, &base_data)]);
    assert_eq!(accepted_root, base_tree.directory.physical());
    let replacement_proof = replacement_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &base_entries,
        &base_tree,
        0,
    );
    let stale = FsCasV1::open_existing(fixture.path()).expect("reopened base root");
    let namespace_before = exact_operation_namespace_usage(fixture.path());

    let mut inserted_source = SliceSource::new(inserted);
    let mut base_reader = BaseBytes { bytes: &base_data };
    let mut evidence = base_file.evidence();
    let mut scratch = OperationScratch::new();
    let mut cow_logical = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut control = ContinueControl::default();
    let mut counters = OperationCountersV1 {
        update_reference_metadata_records: 7,
        update_reference_metadata_bytes: u64::MAX,
        ..OperationCountersV1::default()
    };

    let terminal = run_complete_update_v1(
        &cas,
        0x533,
        base_version,
        base_tree.directory,
        replacement_proof.evidence(base_tree.directory),
        0,
        name,
        0o644,
        base_file.authenticated(0o644),
        range,
        inserted.len() as u64,
        &mut inserted_source,
        &mut base_reader,
        &mut evidence,
        scratch.borrow(),
        &mut cow_logical,
        &mut control,
        &mut counters,
    );

    assert_eq!(
        terminal.unwrap_err(),
        layerfs_storage::lifecycle::OperationErrorV1::Core(CoreError::IntegerOverflow)
    );
    assert_eq!(counters.update_reference_metadata_records, 7);
    assert_eq!(counters.update_reference_metadata_bytes, u64::MAX);
    assert_eq!(counters.update_base_payload_bytes, 0);
    assert_eq!(counters.update_inserted_bytes, 0);
    assert_eq!(counters.update_resynchronization_bytes, 0);
    assert_eq!(counters.exact_rejoin_bytes, 0);
    assert_eq!(counters.anchor_attempts, 0);
    assert_eq!(counters.source_read_calls, 0);
    assert_eq!(counters.source_bytes_read, 0);
    assert_eq!(inserted_source.offset, 0);
    assert_eq!(counters.fscas_bytes_read, 356);
    assert_eq!(counters.fscas_read_calls, 17);
    assert_eq!(
        counters.storage_bytes_requested,
        counters.storage_bytes_reserved
    );
    assert_eq!(
        counters.storage_bytes_reserved,
        counters.storage_bytes_released
            + counters.storage_bytes_committed
            + counters.storage_bytes_retained
    );
    assert_eq!(
        counters.storage_inodes_requested,
        counters.storage_inodes_reserved
    );
    assert_eq!(
        counters.storage_inodes_reserved,
        counters.storage_inodes_released
            + counters.storage_inodes_committed
            + counters.storage_inodes_retained
    );
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert_eq!(counters.storage_bytes_retained, 0);
    assert_eq!(counters.storage_inodes_retained, 0);
    assert_eq!(counters.unreachable_installed_residue_bytes, 0);
    assert_eq!(counters.mutable_preparation_residue_bytes, 0);
    assert_eq!(counters.mutable_preparation_residue_inodes, 0);
    assert_eq!(
        exact_operation_namespace_usage(fixture.path()),
        namespace_before
    );
    assert!(counters.has_zero_forbidden_work());
    assert_clean_terminal(&cas, fixture.path());
    assert!(cas.occupied().is_ok());
    assert!(stale.occupied().is_ok());
}

#[test]
fn complete_update_exact_rejoin_overflow_is_transactional_and_terminal() {
    let fixture = TestRoot::new("update-exact-rejoin-overflow");
    let cas = FsCasV1::create_new(fixture.path()).expect("create FsCas");
    let base_data = crate::test_support::fastcdc_golden_input(300_000);
    let inserted = b"changed";
    let range = UpdateRangeV1::new(120_000, 120_010, base_data.len() as u64).expect("range");
    let name = b"b.bin";
    let base_file = expected_file(&base_data, 0o644);
    let base_entries = [entry(name, &base_file, 0o644)];
    let base_tree = build(DirectoryBuildModeV1::ImplicitRoot, &base_entries).expect("base tree");
    let (base_version, accepted_root) = accept_files(&cas, 0x534, &[(name, 0o644, &base_data)]);
    assert_eq!(accepted_root, base_tree.directory.physical());
    let replacement_proof = replacement_fixture(
        DirectoryBuildModeV1::ImplicitRoot,
        &base_entries,
        &base_tree,
        0,
    );
    let stale = FsCasV1::open_existing(fixture.path()).expect("reopened base root");
    let namespace_before = exact_operation_namespace_usage(fixture.path());

    let mut inserted_source = SliceSource::new(inserted);
    let mut base_reader = BaseBytes { bytes: &base_data };
    let mut evidence = base_file.evidence();
    let mut scratch = OperationScratch::new();
    let mut cow_logical = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut control = ContinueControl::default();
    let mut counters = OperationCountersV1 {
        exact_rejoin_bytes: 7,
        rejoin_successes: u64::MAX,
        rejoin_failures: 11,
        ..OperationCountersV1::default()
    };

    let terminal = run_complete_update_v1(
        &cas,
        0x535,
        base_version,
        base_tree.directory,
        replacement_proof.evidence(base_tree.directory),
        0,
        name,
        0o644,
        base_file.authenticated(0o644),
        range,
        inserted.len() as u64,
        &mut inserted_source,
        &mut base_reader,
        &mut evidence,
        scratch.borrow(),
        &mut cow_logical,
        &mut control,
        &mut counters,
    );

    assert_eq!(
        terminal.unwrap_err(),
        layerfs_storage::lifecycle::OperationErrorV1::Core(CoreError::IntegerOverflow)
    );
    assert_eq!(counters.exact_rejoin_bytes, 7);
    assert_eq!(counters.rejoin_successes, u64::MAX);
    assert_eq!(counters.rejoin_failures, 11);
    assert_eq!(counters.bytes_read, 74_342);
    assert_eq!(counters.source_read_calls, 2);
    assert_eq!(counters.source_bytes_read, inserted.len() as u64);
    assert_eq!(inserted_source.offset, inserted.len());
    assert_eq!(counters.update_base_payload_bytes, 73_979);
    assert_eq!(counters.update_inserted_bytes, inserted.len() as u64);
    assert_eq!(counters.update_reference_metadata_records, 28);
    assert_eq!(counters.update_reference_metadata_bytes, 1_008);
    assert_eq!(counters.update_resynchronization_bytes, 67_808);
    assert_eq!(counters.anchor_attempts, 1);
    assert_eq!(counters.fscas_bytes_read, 356);
    assert_eq!(counters.fscas_read_calls, 17);
    assert_eq!(counters.update_failures, 1);
    assert_eq!(
        counters.storage_bytes_requested,
        counters.storage_bytes_reserved
    );
    assert_eq!(
        counters.storage_bytes_reserved,
        counters.storage_bytes_released
            + counters.storage_bytes_committed
            + counters.storage_bytes_retained
    );
    assert_eq!(
        counters.storage_inodes_requested,
        counters.storage_inodes_reserved
    );
    assert_eq!(
        counters.storage_inodes_reserved,
        counters.storage_inodes_released
            + counters.storage_inodes_committed
            + counters.storage_inodes_retained
    );
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert_eq!(counters.storage_bytes_retained, 0);
    assert_eq!(counters.storage_inodes_retained, 0);
    assert_eq!(counters.unreachable_installed_residue_bytes, 0);
    assert_eq!(counters.mutable_preparation_residue_bytes, 0);
    assert_eq!(counters.mutable_preparation_residue_inodes, 0);
    assert_eq!(counters.storage_preparation_bytes_current_after_cleanup, 0);
    assert_eq!(counters.storage_preparation_inodes_current_after_cleanup, 0);
    assert_eq!(
        exact_operation_namespace_usage(fixture.path()),
        namespace_before
    );
    assert!(counters.has_zero_forbidden_work());
    assert_clean_terminal(&cas, fixture.path());
    assert!(cas.occupied().is_ok());
    assert!(stale.occupied().is_ok());
}

#[test]
fn complete_mutation_rejects_an_unauthenticated_base_without_preparation() {
    let fixture = TestRoot::new("bad-base");
    let cas = FsCasV1::create_new(fixture.path()).expect("create FsCas");
    let data = b"authenticated base";
    let file = expected_file(data, 0o644);
    let entries = [entry(b"b.bin", &file, 0o644)];
    let tree = build(DirectoryBuildModeV1::ImplicitRoot, &entries).expect("base tree");
    let (version, accepted_root) = accept_files(&cas, 0x540, &[(b"b.bin", 0o644, data)]);
    assert_eq!(accepted_root, tree.directory.physical());
    let proof = replacement_fixture(DirectoryBuildModeV1::ImplicitRoot, &entries, &tree, 0);
    let mut source = SliceSource::new(b"replacement");
    let mut scratch = OperationScratch::new();
    let mut cow_logical = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut control = ContinueControl::default();
    let mut counters = OperationCountersV1::default();
    let wrong_version = PhysicalVersionRecordIdV1::from_digest([0x5a; 32]);
    let error = run_complete_replace_v1(
        &cas,
        0x541,
        CdcAlgorithmV1::FastCdc,
        wrong_version,
        tree.directory,
        proof.evidence(tree.directory),
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
    assert!(matches!(
        error,
        layerfs_storage::lifecycle::OperationErrorV1::FsCas(_)
            | layerfs_storage::lifecycle::OperationErrorV1::Core(CoreError::IdMismatch)
    ));
    assert_eq!(source.offset, 0);
    assert_clean_terminal(&cas, fixture.path());
    assert_ne!(version, wrong_version);
}
