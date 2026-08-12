use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::Duration;

use layerfs_storage::cas::{
    FsCasBoundaryV1, FsCasCleanupTargetV1, FsCasControlV1, FsCasErrorV1, FsCasFailureCauseV1,
    FsCasFilesystemBoundaryV1, FsCasFilesystemFailureV1, FsCasResidueAccountingBoundaryV1, FsCasV1,
    FsOperationKindV1, FsPackAdmissionOutcomeV1, FsStorageEnvelopeV1,
    ROOT_LOGICAL_STORAGE_BUDGET_V1, ROOT_NAMESPACE_ENTRY_BUDGET_V1,
};
use layerfs_storage::cdc::CdcAlgorithmV1;
use layerfs_storage::cdc::{CdcControlV1, MAXIMUM_CHUNK_BYTES};
use layerfs_storage::content::{
    request_create_operation_v1, request_tree_operation_v1, run_create_tree_v1, run_create_v1,
    OperationBuffersV1, OperationErrorV1, SourceSupplierV1, TreeFileV1,
};
use layerfs_storage::content::{ContentSourceErrorV1, ContentSourceV1};
use layerfs_storage::cow::{TreePageSummaryV1, MAX_TREE_OBJECT_BYTES, MAX_TREE_PAGE_SUMMARIES};
use layerfs_storage::identity::COMPARISON_WINDOW_BYTES;
use layerfs_storage::limits::{
    ObservationScopeV1, OperationCountersV1, OptionalObservationStatusV1, OptionalU64ObservationV1,
    TerminalOptionalObservationsV1,
};
use layerfs_storage::pack::PACK_HEADER_BYTES;
use layerfs_storage::{CoreError, CoreResult};

static NEXT_ROOT: AtomicU64 = AtomicU64::new(1);
const SUBPROCESS_OPEN_PATH_ENV: &str = "LAYERFS_L155_SUBPROCESS_OPEN_PATH";
const SUBPROCESS_OPEN_EXPECT_ENV: &str = "LAYERFS_L155_SUBPROCESS_OPEN_EXPECT";

fn assert_path_absent(path: &Path) {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Ok(metadata) => {
            panic!("expected {path:?} to be absent, but metadata succeeded: {metadata:?}")
        }
        Err(error) => panic!("expected {path:?} to be absent, but lookup failed: {error}"),
    }
}

fn assert_path_is_directory(path: &Path) {
    let metadata = fs::symlink_metadata(path)
        .unwrap_or_else(|error| panic!("expected {path:?} to be a directory: {error}"));
    assert!(
        metadata.file_type().is_dir(),
        "expected {path:?} to be a directory, but found {metadata:?}"
    );
}

#[test]
fn typed_optional_observation_never_fabricates_an_unavailable_value() {
    let observed = OptionalU64ObservationV1::observed(
        0,
        "direct zero-valued operation observation",
        ObservationScopeV1::Operation,
    );
    assert_eq!(observed.status(), OptionalObservationStatusV1::Observed);
    assert_eq!(observed.value(), Some(0));
    assert_eq!(observed.scope(), ObservationScopeV1::Operation);
    assert_eq!(
        observed.method(),
        "direct zero-valued operation observation"
    );

    for absent in [
        OptionalU64ObservationV1::unavailable(
            "platform observation unavailable",
            ObservationScopeV1::Host,
        ),
        OptionalU64ObservationV1::not_applicable(
            "observation does not apply",
            ObservationScopeV1::Process,
        ),
        OptionalU64ObservationV1::deferred(
            "observation deferred to a later milestone",
            ObservationScopeV1::Root,
        ),
    ] {
        assert_ne!(absent.status(), OptionalObservationStatusV1::Observed);
        assert_eq!(absent.value(), None);
        assert!(!absent.method().is_empty());
    }
}

#[test]
fn terminal_host_observations_are_named_typed_and_never_fabricated() {
    let counters = OperationCountersV1::default();
    let observations: TerminalOptionalObservationsV1 = counters.terminal_optional_observations_v1();

    for observation in observations.all() {
        assert_eq!(
            observation.status(),
            OptionalObservationStatusV1::Unavailable
        );
        assert_eq!(observation.value(), None);
        assert!(!observation.method().is_empty());
    }

    for process in [
        observations.process_cpu_nanoseconds(),
        observations.allocator_live_bytes(),
        observations.allocator_high_water_bytes(),
        observations.rss_bytes(),
        observations.pss_bytes(),
        observations.page_cache_bytes(),
        observations.process_open_descriptors(),
    ] {
        assert_eq!(process.scope(), ObservationScopeV1::Process);
    }
    assert_eq!(
        observations.host_open_descriptors().scope(),
        ObservationScopeV1::Host
    );
    for root in [
        observations.filesystem_allocated_bytes(),
        observations.filesystem_allocated_blocks(),
        observations.filesystem_free_bytes(),
        observations.filesystem_quota_bytes(),
        observations.physical_inodes(),
    ] {
        assert_eq!(root.scope(), ObservationScopeV1::Root);
    }
}

#[test]
fn subprocess_open_existing_probe() {
    let Some(path) = std::env::var_os(SUBPROCESS_OPEN_PATH_ENV) else {
        return;
    };
    let expected = std::env::var(SUBPROCESS_OPEN_EXPECT_ENV).expect("probe expectation");
    let result = FsCasV1::open_existing(Path::new(&path));
    match expected.as_str() {
        "busy" => assert!(matches!(result, Err(FsCasErrorV1::Busy))),
        "invalidated" => assert!(matches!(result, Err(FsCasErrorV1::Invalidated))),
        "ok" => assert!(result.is_ok()),
        other => panic!("unrecognized subprocess probe expectation: {other}"),
    }
}

fn run_subprocess_open_probe(path: &Path, expected: &str) {
    let status = Command::new(std::env::current_exe().expect("current unit-test executable"))
        .args([
            "--exact",
            "c3_operation_tests::subprocess_open_existing_probe",
            "--nocapture",
        ])
        .env(SUBPROCESS_OPEN_PATH_ENV, path)
        .env(SUBPROCESS_OPEN_EXPECT_ENV, expected)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("spawn bounded subprocess ownership probe");
    assert!(
        status.success(),
        "subprocess ownership probe failed: {status}"
    );
}

fn boxed_zeroes<const N: usize>() -> Box<[u8; N]> {
    vec![0_u8; N]
        .into_boxed_slice()
        .try_into()
        .unwrap_or_else(|_| unreachable!("boxed slice was created at the exact array length"))
}

fn boxed_tree_pages() -> Box<[Option<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES]> {
    vec![None; MAX_TREE_PAGE_SUMMARIES]
        .into_boxed_slice()
        .try_into()
        .unwrap_or_else(|_| unreachable!("boxed slice was created at the exact array length"))
}

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

impl TestRoot {
    fn new(label: &str) -> Self {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let parent = fs::canonicalize(std::env::temp_dir()).unwrap();
        Self(parent.join(format!(
            "layerfs-c3-operation-{label}-{}-{sequence:016x}",
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

fn exact_directory_usage(path: &Path) -> (u64, u64) {
    fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let metadata = fs::symlink_metadata(entry.path()).unwrap();
            assert!(metadata.file_type().is_file());
            (metadata.len(), 1_u64)
        })
        .fold((0_u64, 0_u64), |(bytes, inodes), (len, one)| {
            (bytes.checked_add(len).unwrap(), inodes + one)
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
                    bytes.checked_add(next_bytes).unwrap(),
                    inodes.checked_add(next_inodes).unwrap(),
                )
            },
        );
    (preparation, immutable)
}

fn assert_operation_authority_baseline(cas: &FsCasV1, root: &Path) {
    assert_eq!(cas.operation_admitted_slots_v1(), 0);
    assert_eq!(cas.operation_admission_active_for_test_v1(), 0);
    assert_eq!(cas.operation_admission_queue_for_test_v1(), (0, 0, 0));
    assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
    assert_eq!(fs::read_dir(root.join("preparation")).unwrap().count(), 0);
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
            + counters.storage_bytes_retained,
    );
    assert_eq!(
        counters.storage_inodes_reserved,
        counters.storage_inodes_released
            + counters.storage_inodes_committed
            + counters.storage_inodes_retained,
    );
}

#[test]
fn exclusive_root_owner_refuses_subprocess_then_transfers_after_clean_last_drop() {
    let fixture = TestRoot::new("exclusive-owner-transfer");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let reopened_alias = FsCasV1::open_existing(fixture.path()).unwrap();

    run_subprocess_open_probe(fixture.path(), "busy");
    drop(reopened_alias);
    run_subprocess_open_probe(fixture.path(), "busy");
    drop(cas);

    run_subprocess_open_probe(fixture.path(), "ok");
}

struct SliceSource<'a> {
    bytes: &'a [u8],
    offset: usize,
}

struct CounterSource {
    len: u64,
    offset: u64,
}

impl ContentSourceV1 for CounterSource {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
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

struct CounterSupplier {
    len: u64,
}

struct BarrierCounterSupplier {
    len: u64,
    ready: mpsc::SyncSender<()>,
    start: Arc<WatchdogGateV1>,
}

struct InvocationCheckedSupplier<'a> {
    invoked: &'a AtomicBool,
}

struct CallbackCheckedSupplier<'a> {
    bound_invoked: &'a AtomicBool,
    supply_invoked: &'a AtomicBool,
}

struct PanickingSupplier;

struct PanicDuringPreparationFreeStageSupplier<'a> {
    cas_to_poison: Option<FsCasV1>,
    bound_invoked: &'a AtomicBool,
    supply_invoked: &'a AtomicBool,
}

struct FailingPreparationFreeStageSupplier<'a> {
    bound_invoked: &'a AtomicBool,
    supply_invoked: &'a AtomicBool,
}

struct FailingBodySource;

struct FailingBodySupplier;

struct FailingAfterBytesSource {
    remaining: u64,
}

struct FailingAfterBytesSupplier {
    bytes_before_failure: u64,
}

impl SourceSupplierV1 for PanickingSupplier {
    type Source = CounterSource;

    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        Ok(core::mem::size_of::<CounterSource>() as u64)
    }

    fn supply(self) -> CoreResult<Self::Source> {
        panic!("injected supplier unwind")
    }
}

impl SourceSupplierV1 for PanicDuringPreparationFreeStageSupplier<'_> {
    type Source = CounterSource;

    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        self.bound_invoked.store(true, Ordering::Release);
        if let Some(cas) = self.cas_to_poison.as_ref() {
            cas.poison_storage_admission_for_test_v1();
        }
        panic!("injected preparation-free supplier-bound unwind")
    }

    fn supply(self) -> CoreResult<Self::Source> {
        self.supply_invoked.store(true, Ordering::Release);
        Ok(CounterSource { len: 1, offset: 0 })
    }
}

impl SourceSupplierV1 for FailingPreparationFreeStageSupplier<'_> {
    type Source = CounterSource;

    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        self.bound_invoked.store(true, Ordering::Release);
        Err(CoreError::ResourceRefused)
    }

    fn supply(self) -> CoreResult<Self::Source> {
        self.supply_invoked.store(true, Ordering::Release);
        Ok(CounterSource { len: 1, offset: 0 })
    }
}

impl ContentSourceV1 for FailingBodySource {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        Ok(core::mem::size_of::<Self>() as u64)
    }

    fn read(&mut self, _destination: &mut [u8]) -> Result<usize, ContentSourceErrorV1> {
        Err(ContentSourceErrorV1::Failure)
    }
}

impl SourceSupplierV1 for FailingBodySupplier {
    type Source = FailingBodySource;

    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        Ok(core::mem::size_of::<FailingBodySource>() as u64)
    }

    fn supply(self) -> CoreResult<Self::Source> {
        Ok(FailingBodySource)
    }
}

impl ContentSourceV1 for FailingAfterBytesSource {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
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

impl SourceSupplierV1 for FailingAfterBytesSupplier {
    type Source = FailingAfterBytesSource;

    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        Ok(core::mem::size_of::<FailingAfterBytesSource>() as u64)
    }

    fn supply(self) -> CoreResult<Self::Source> {
        Ok(FailingAfterBytesSource {
            remaining: self.bytes_before_failure,
        })
    }
}

impl<'a> SourceSupplierV1 for InvocationCheckedSupplier<'a> {
    type Source = CounterSource;

    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        Ok(core::mem::size_of::<CounterSource>() as u64)
    }

    fn supply(self) -> CoreResult<Self::Source> {
        self.invoked.store(true, Ordering::Release);
        Ok(CounterSource { len: 1, offset: 0 })
    }
}

impl<'a> SourceSupplierV1 for CallbackCheckedSupplier<'a> {
    type Source = CounterSource;

    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        self.bound_invoked.store(true, Ordering::Release);
        Ok(core::mem::size_of::<CounterSource>() as u64)
    }

    fn supply(self) -> CoreResult<Self::Source> {
        self.supply_invoked.store(true, Ordering::Release);
        Ok(CounterSource { len: 1, offset: 0 })
    }
}

impl SourceSupplierV1 for CounterSupplier {
    type Source = CounterSource;

    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        Ok(core::mem::size_of::<CounterSource>() as u64)
    }

    fn supply(self) -> CoreResult<Self::Source> {
        Ok(CounterSource {
            len: self.len,
            offset: 0,
        })
    }
}

impl SourceSupplierV1 for BarrierCounterSupplier {
    type Source = CounterSource;

    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        Ok(core::mem::size_of::<CounterSource>() as u64)
    }

    fn supply(self) -> CoreResult<Self::Source> {
        self.ready
            .send(())
            .expect("counter supplier watchdog receiver remains live");
        self.start.wait();
        Ok(CounterSource {
            len: self.len,
            offset: 0,
        })
    }
}

impl ContentSourceV1 for SliceSource<'_> {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        Ok(core::mem::size_of::<Self>() as u64)
    }

    fn read(&mut self, destination: &mut [u8]) -> Result<usize, ContentSourceErrorV1> {
        let take = destination.len().min(self.bytes.len() - self.offset);
        destination[..take].copy_from_slice(&self.bytes[self.offset..self.offset + take]);
        self.offset += take;
        Ok(take)
    }
}

struct CheckedSupplier<'a> {
    bytes: &'a [u8],
}

impl<'a> SourceSupplierV1 for CheckedSupplier<'a> {
    type Source = SliceSource<'a>;

    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        Ok(core::mem::size_of::<SliceSource<'_>>() as u64)
    }

    fn supply(self) -> CoreResult<Self::Source> {
        Ok(SliceSource {
            bytes: self.bytes,
            offset: 0,
        })
    }
}

/// Test-only rendezvous immediately before source delivery.  Production
/// remains synchronous on each caller thread; the barrier exists solely to
/// make two independently reopened operations genuinely overlap.
struct BarrierCheckedSupplier<'a> {
    bytes: &'a [u8],
    ready: mpsc::SyncSender<()>,
    start: Arc<WatchdogGateV1>,
}

impl<'a> SourceSupplierV1 for BarrierCheckedSupplier<'a> {
    type Source = SliceSource<'a>;

    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        Ok(core::mem::size_of::<SliceSource<'_>>() as u64)
    }

    fn supply(self) -> CoreResult<Self::Source> {
        self.ready
            .send(())
            .expect("barrier supplier watchdog receiver remains live");
        self.start.wait();
        Ok(SliceSource {
            bytes: self.bytes,
            offset: 0,
        })
    }
}

struct BarrierFailingSupplier {
    ready: mpsc::SyncSender<()>,
    start: Arc<WatchdogGateV1>,
}

impl SourceSupplierV1 for BarrierFailingSupplier {
    type Source = SliceSource<'static>;

    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        Ok(core::mem::size_of::<SliceSource<'static>>() as u64)
    }

    fn supply(self) -> CoreResult<Self::Source> {
        self.ready
            .send(())
            .expect("failing supplier watchdog receiver remains live");
        self.start.wait();
        Err(CoreError::CountCap)
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
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
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

    fn cancelled_v1(&self) -> bool {
        self.armed && self.stop == CandidateValidationStopV1::Cancelled
    }

    fn deadline_v1(&self) -> bool {
        self.armed && self.stop == CandidateValidationStopV1::Deadline
    }
}

impl CdcControlV1 for StopBeforeCandidateValidationV1 {
    fn cancellation_requested(&mut self) -> bool {
        self.cancelled_v1()
    }

    fn deadline_exceeded(&mut self) -> bool {
        self.deadline_v1()
    }
}

impl FsCasControlV1 for StopBeforeCandidateValidationV1 {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if boundary == FsCasBoundaryV1::BeforeCandidateValidation {
            self.armed = true;
        }
    }

    fn cancellation_requested(&mut self) -> bool {
        self.cancelled_v1()
    }

    fn deadline_exceeded(&mut self) -> bool {
        self.deadline_v1()
    }
}

#[derive(Default)]
struct GlobalSeenCounterOverflowControl {
    injected: bool,
}

struct PackObjectDispositionOverflowControl {
    target_created: bool,
    injected: bool,
}

#[derive(Default)]
struct OperationSpoolWriteObservationOverflowControl {
    injected: bool,
}

#[derive(Default)]
struct CountedPackReadObservationOverflowControl {
    injected: bool,
}

#[derive(Default)]
struct SameCarrierComparisonObservationOverflowControl {
    injected: bool,
}

#[derive(Default)]
struct PostAdmissionCarrierTallyOverflowControl {
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

struct FailFilesystemBoundaryOnceV1 {
    boundary: FsCasFilesystemBoundaryV1,
    error: FsCasErrorV1,
    fired: bool,
}

struct PoisonStorageBeforePreparationAccountingV1 {
    cas: FsCasV1,
    poisoned: bool,
    fail_invalidation: bool,
}

struct BreakPreparationAccountingAndFailCreateV1 {
    cas: FsCasV1,
    fired: bool,
}

struct BreakPrivatePackAccountingAndFailCreateV1 {
    cas: FsCasV1,
    create_error: FsCasErrorV1,
    fired: bool,
    fail_invalidation: bool,
}

struct BreakMarkerAccountingAndFailCreateV1 {
    cas: FsCasV1,
    create_error: FsCasErrorV1,
    break_accounting: bool,
    fired: bool,
    fail_invalidation: bool,
}

struct BreakMarkerLengthAccountingV1 {
    cas: FsCasV1,
    corrupted: bool,
    restored_for_cleanup: bool,
    payload_or_link_seen: bool,
    fail_invalidation: bool,
}

struct ObserveMarkerImmutablePrechargeV1 {
    marker_write_seen: bool,
    marker_link_boundary_seen: bool,
    fail_invalidation: bool,
}

struct RestoreMarkerCleanupAccountingV1 {
    cas: FsCasV1,
    accounting_restored: bool,
    fail_invalidation: bool,
}

#[cfg(unix)]
#[derive(Clone, Copy)]
enum MarkerCleanupUnlinkFaultModeV1 {
    PermissionDenied,
    NonDirectory,
    Injected,
}

#[cfg(unix)]
struct FailMarkerCleanupUnlinkV1 {
    preparation: PathBuf,
    held_preparation: PathBuf,
    mode: MarkerCleanupUnlinkFaultModeV1,
    armed: bool,
    restored: bool,
    fail_invalidation: bool,
}

struct FailRootInvalidationV1 {
    fail: bool,
}

struct PreparationFreeTerminalControlV1 {
    fail_invalidation: bool,
    invalidation_attempts: u32,
}

struct PanicAfterOperationTerminalReleaseV1 {
    unwind_pending: bool,
    terminal_hook_calls: u32,
}

struct FailBodyCleanupTerminalV1 {
    preparation_cleanup_injected: bool,
    fail_invalidation: bool,
    invalidation_attempts: u32,
}

#[cfg(unix)]
#[derive(Clone, Copy)]
enum PreparationMetadataFaultModeV1 {
    WrongType,
    Missing,
    PermissionDenied,
    ReadFailure,
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
#[derive(Clone, Copy)]
enum PreparationUnlinkFaultModeV1 {
    Missing,
    PermissionDenied,
    WriteFailure,
    Injected,
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

struct PoisonStorageAtCarrierLinkV1 {
    cas: FsCasV1,
    link_error: FsCasErrorV1,
    fired: bool,
    fail_invalidation: bool,
}

struct InstallCarrierAndPoisonStorageBeforeLinkV1 {
    cas: FsCasV1,
    installed: Option<PathBuf>,
    fail_invalidation: bool,
}

struct FailCarrierAliasPreparationAccountingV1 {
    cas: FsCasV1,
    armed: bool,
    fail_invalidation: bool,
    root_invalidation_callbacks: usize,
}

#[derive(Default)]
struct FailPreparationCreateAndCleanupV1 {
    preparation_creates: usize,
    create_failed: bool,
    cleanup_failed: bool,
}

struct FailPreparationPermissionAndPanicCleanupV1 {
    first_error: FsCasErrorV1,
    permission_failed: bool,
    preparation_cleanup_calls: usize,
    cleanup_panicked: bool,
    fail_invalidation: bool,
    root_invalidation_callbacks: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PreparationConstructionUnwindModeV1 {
    CleanupFails,
    CleanupUnwinds,
    PreCreateAccountingReleaseFails,
}

struct PanicPreparationConstructionWithCleanupFailureV1 {
    cas: FsCasV1,
    mode: PreparationConstructionUnwindModeV1,
    construction_panicked: bool,
    preparation_cleanup_calls: usize,
    fail_invalidation: bool,
    root_invalidation_callbacks: usize,
}

struct PanicPreparationInitializationWithCleanupFailureV1 {
    construction_panicked: bool,
    preparation_cleanup_calls: usize,
    fail_invalidation: bool,
    root_invalidation_callbacks: usize,
}

struct PanicPreparationInitializationAndPoisonTerminalV1 {
    cas_to_poison: Option<FsCasV1>,
    construction_panicked: bool,
    preparation_cleanup_calls: usize,
    fail_invalidation: bool,
    root_invalidation_callbacks: usize,
}

struct PanicClosureFenceAndPoisonTerminalV1 {
    cas_to_poison: Option<FsCasV1>,
    closure_panicked: bool,
    preparation_cleanup_calls: usize,
    fail_invalidation: bool,
    root_invalidation_callbacks: usize,
}

struct ObserveClosureMarkerLockScopeV1 {
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

impl CdcControlV1 for ObserveClosureMarkerLockScopeV1 {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for ObserveClosureMarkerLockScopeV1 {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if boundary == FsCasBoundaryV1::BeforeClosureMarkerPublication {
            self.observed = true;
            self.closure_phase = true;
            self.visibility_available = self.cas.visibility_lock_available_for_test_v1();
            self.publication_available = self.cas.publication_lock_available_for_test_v1();
        }
        match boundary {
            FsCasBoundaryV1::VisibilityLockAcquired => {
                self.visibility_acquisitions += 1;
            }
            FsCasBoundaryV1::VisibilityLockReleased => {
                self.visibility_releases += 1;
            }
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

impl CdcControlV1 for PanicClosureFenceAndPoisonTerminalV1 {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for PanicClosureFenceAndPoisonTerminalV1 {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if !self.closure_panicked && boundary == FsCasBoundaryV1::BeforeClosureMarkerPublication {
            self.closure_panicked = true;
            if let Some(cas) = self.cas_to_poison.as_ref() {
                cas.poison_storage_admission_for_test_v1();
            }
            panic!("injected closure-fence unwind before outer terminal")
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
                false
            }
            FsCasCleanupTargetV1::RootInvalidation => {
                self.root_invalidation_callbacks += 1;
                self.fail_invalidation
            }
            _ => false,
        }
    }
}

impl CdcControlV1 for PanicPreparationInitializationAndPoisonTerminalV1 {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for PanicPreparationInitializationAndPoisonTerminalV1 {
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
        if !self.construction_panicked && boundary == FsCasFilesystemBoundaryV1::PreparationResize {
            self.construction_panicked = true;
            if let Some(cas) = self.cas_to_poison.as_ref() {
                cas.poison_storage_admission_for_test_v1();
            }
            panic!("injected preparation initialization unwind before outer terminal")
        }
        None
    }

    fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
        match target {
            FsCasCleanupTargetV1::PreparationSpool => {
                self.preparation_cleanup_calls += 1;
                false
            }
            FsCasCleanupTargetV1::RootInvalidation => {
                self.root_invalidation_callbacks += 1;
                self.fail_invalidation
            }
            _ => false,
        }
    }
}

impl CdcControlV1 for PanicPreparationInitializationWithCleanupFailureV1 {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for PanicPreparationInitializationWithCleanupFailureV1 {
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
        if !self.construction_panicked && boundary == FsCasFilesystemBoundaryV1::PreparationResize {
            self.construction_panicked = true;
            panic!("injected preparation initialization unwind")
        }
        None
    }

    fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
        match target {
            FsCasCleanupTargetV1::PreparationSpool if self.preparation_cleanup_calls == 0 => {
                self.preparation_cleanup_calls += 1;
                true
            }
            FsCasCleanupTargetV1::PreparationSpool => {
                self.preparation_cleanup_calls += 1;
                false
            }
            FsCasCleanupTargetV1::RootInvalidation => {
                self.root_invalidation_callbacks += 1;
                self.fail_invalidation
            }
            _ => false,
        }
    }
}

impl CdcControlV1 for PanicPreparationConstructionWithCleanupFailureV1 {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for PanicPreparationConstructionWithCleanupFailureV1 {
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
        let target = match self.mode {
            PreparationConstructionUnwindModeV1::CleanupFails
            | PreparationConstructionUnwindModeV1::CleanupUnwinds => {
                FsCasFilesystemBoundaryV1::PermissionChange
            }
            PreparationConstructionUnwindModeV1::PreCreateAccountingReleaseFails => {
                FsCasFilesystemBoundaryV1::PreparationCreate
            }
        };
        if !self.construction_panicked && boundary == target {
            self.construction_panicked = true;
            if self.mode == PreparationConstructionUnwindModeV1::PreCreateAccountingReleaseFails {
                self.cas.fail_next_preparation_remove_for_test_v1();
            }
            panic!("injected partial preparation construction unwind")
        }
        None
    }

    fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
        match target {
            FsCasCleanupTargetV1::PreparationSpool
                if self.mode
                    != PreparationConstructionUnwindModeV1::PreCreateAccountingReleaseFails =>
            {
                self.preparation_cleanup_calls += 1;
                match self.mode {
                    PreparationConstructionUnwindModeV1::CleanupFails => true,
                    PreparationConstructionUnwindModeV1::CleanupUnwinds => {
                        panic!("injected partial preparation construction cleanup unwind")
                    }
                    PreparationConstructionUnwindModeV1::PreCreateAccountingReleaseFails => {
                        unreachable!()
                    }
                }
            }
            FsCasCleanupTargetV1::RootInvalidation => {
                self.root_invalidation_callbacks += 1;
                self.fail_invalidation
            }
            _ => false,
        }
    }
}

impl CdcControlV1 for FailPreparationPermissionAndPanicCleanupV1 {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for FailPreparationPermissionAndPanicCleanupV1 {
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
                self.preparation_cleanup_calls += 1;
                if !self.cleanup_panicked {
                    self.cleanup_panicked = true;
                    panic!("injected partial preparation cleanup unwind")
                }
                false
            }
            FsCasCleanupTargetV1::RootInvalidation => {
                self.root_invalidation_callbacks += 1;
                self.fail_invalidation
            }
            _ => false,
        }
    }
}

#[derive(Default)]
struct FailMarkerWriteAndCleanupV1 {
    marker_write_failed: bool,
    cleanup_failed: bool,
}

struct PanicMarkerTerminalCleanupV1 {
    first_error: Option<FsCasErrorV1>,
    cleanup_calls: usize,
    invalidation_calls: usize,
    fail_invalidation: bool,
}

#[derive(Clone, Copy)]
enum MarkerCleanupSecondaryV1 {
    Failure,
    Unwind,
}

struct PanicMarkerPreparationWithCleanupTerminalV1 {
    secondary: MarkerCleanupSecondaryV1,
    preparation_panicked: bool,
    cleanup_calls: usize,
    invalidation_calls: usize,
    fail_invalidation: bool,
}

impl CdcControlV1 for FailMarkerWriteAndCleanupV1 {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for FailMarkerWriteAndCleanupV1 {
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

impl FsCasControlV1 for PanicMarkerTerminalCleanupV1 {
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
        if boundary == FsCasFilesystemBoundaryV1::MarkerWrite {
            self.first_error.take()
        } else {
            None
        }
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

impl FsCasControlV1 for PanicMarkerPreparationWithCleanupTerminalV1 {
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
                match self.secondary {
                    MarkerCleanupSecondaryV1::Failure => true,
                    MarkerCleanupSecondaryV1::Unwind => {
                        panic!("injected pre-link marker cleanup unwind")
                    }
                }
            }
            FsCasCleanupTargetV1::RootInvalidation => {
                self.invalidation_calls += 1;
                self.fail_invalidation
            }
            _ => false,
        }
    }
}

impl CdcControlV1 for FailPreparationCreateAndCleanupV1 {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for FailPreparationCreateAndCleanupV1 {
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

impl CdcControlV1 for FailFilesystemBoundaryOnceV1 {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for FailFilesystemBoundaryOnceV1 {
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

impl CdcControlV1 for PoisonStorageBeforePreparationAccountingV1 {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for PoisonStorageBeforePreparationAccountingV1 {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if !self.poisoned && boundary == FsCasBoundaryV1::VisibilityLockAcquired {
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
        self.fail_invalidation && target == FsCasCleanupTargetV1::RootInvalidation
    }
}

impl CdcControlV1 for BreakPreparationAccountingAndFailCreateV1 {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for BreakPreparationAccountingAndFailCreateV1 {
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
        if !self.fired && boundary == FsCasFilesystemBoundaryV1::PreparationCreate {
            self.fired = true;
            self.cas.remove_active_preparation_inode_for_test_v1();
            Some(FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::NoSpace))
        } else {
            None
        }
    }
}

impl FsCasControlV1 for BreakPrivatePackAccountingAndFailCreateV1 {
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
            // This direct capability-level test owns exactly one charged
            // preparation inode. Remove it before the injected create error
            // so the constructor's attempted charge rollback deterministically
            // exercises the otherwise unreachable accounting double fault.
            self.cas.remove_active_preparation_inode_for_test_v1();
            Some(self.create_error)
        } else {
            None
        }
    }

    fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
        self.fail_invalidation && target == FsCasCleanupTargetV1::RootInvalidation
    }
}

impl FsCasControlV1 for BreakMarkerAccountingAndFailCreateV1 {
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
            Some(self.create_error)
        } else {
            None
        }
    }

    fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
        self.fail_invalidation && target == FsCasCleanupTargetV1::RootInvalidation
    }
}

impl FsCasControlV1 for BreakMarkerLengthAccountingV1 {
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
        if target == FsCasCleanupTargetV1::RootInvalidation && !self.restored_for_cleanup {
            self.restored_for_cleanup = true;
            self.cas.clear_active_preparation_bytes_for_test_v1();
        }
        self.fail_invalidation && target == FsCasCleanupTargetV1::RootInvalidation
    }
}

impl FsCasControlV1 for ObserveMarkerImmutablePrechargeV1 {
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
        if boundary == FsCasFilesystemBoundaryV1::MarkerWrite {
            self.marker_write_seen = true;
        } else if boundary == FsCasFilesystemBoundaryV1::MarkerHardLink {
            // This fault boundary precedes both the checked immutable
            // accounting transition and the actual filesystem hard link.
            self.marker_link_boundary_seen = true;
        }
        None
    }

    fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
        self.fail_invalidation && target == FsCasCleanupTargetV1::RootInvalidation
    }
}

impl FsCasControlV1 for RestoreMarkerCleanupAccountingV1 {
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
impl FailMarkerCleanupUnlinkV1 {
    fn restore_preparation_v1(&mut self) {
        if self.restored {
            return;
        }
        match self.mode {
            MarkerCleanupUnlinkFaultModeV1::PermissionDenied => {
                use std::os::unix::fs::PermissionsExt;

                fs::set_permissions(&self.preparation, fs::Permissions::from_mode(0o700)).unwrap();
            }
            MarkerCleanupUnlinkFaultModeV1::NonDirectory => {
                fs::remove_file(&self.preparation).unwrap();
                fs::rename(&self.held_preparation, &self.preparation).unwrap();
            }
            MarkerCleanupUnlinkFaultModeV1::Injected => {}
        }
        self.restored = true;
    }
}

#[cfg(unix)]
impl FsCasControlV1 for FailMarkerCleanupUnlinkV1 {
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
                MarkerCleanupUnlinkFaultModeV1::PermissionDenied => {
                    use std::os::unix::fs::PermissionsExt;

                    fs::set_permissions(&self.preparation, fs::Permissions::from_mode(0o500))
                        .unwrap();
                    return false;
                }
                MarkerCleanupUnlinkFaultModeV1::NonDirectory => {
                    fs::rename(&self.preparation, &self.held_preparation).unwrap();
                    fs::write(&self.preparation, b"not-a-directory").unwrap();
                    return false;
                }
                MarkerCleanupUnlinkFaultModeV1::Injected => return true,
            }
        }
        if target == FsCasCleanupTargetV1::RootInvalidation {
            self.restore_preparation_v1();
            return self.fail_invalidation;
        }
        false
    }
}

impl FsCasControlV1 for FailRootInvalidationV1 {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }

    fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
        self.fail && target == FsCasCleanupTargetV1::RootInvalidation
    }
}

impl CdcControlV1 for PreparationFreeTerminalControlV1 {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for PreparationFreeTerminalControlV1 {
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

impl CdcControlV1 for PanicAfterOperationTerminalReleaseV1 {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for PanicAfterOperationTerminalReleaseV1 {
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

impl CdcControlV1 for FailBodyCleanupTerminalV1 {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for FailBodyCleanupTerminalV1 {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }

    fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
        if target == FsCasCleanupTargetV1::PreparationSpool && !self.preparation_cleanup_injected {
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

#[cfg(unix)]
impl RestorePreparationMetadataAuthorityV1 {
    fn restore_v1(&mut self) {
        if self.restored {
            return;
        }
        match self.mode {
            PreparationMetadataFaultModeV1::PermissionDenied => {
                use std::os::unix::fs::PermissionsExt;

                fs::set_permissions(&self.preparation, fs::Permissions::from_mode(0o700)).unwrap();
            }
            PreparationMetadataFaultModeV1::ReadFailure => {
                fs::remove_file(&self.preparation).unwrap();
                fs::rename(&self.held_preparation, &self.preparation).unwrap();
            }
            PreparationMetadataFaultModeV1::WrongType | PreparationMetadataFaultModeV1::Missing => {
            }
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
impl FailPreparationUnlinkV1 {
    fn restore_v1(&mut self) {
        if self.restored {
            return;
        }
        match self.mode {
            PreparationUnlinkFaultModeV1::PermissionDenied => {
                use std::os::unix::fs::PermissionsExt;

                fs::set_permissions(&self.preparation, fs::Permissions::from_mode(0o700)).unwrap();
            }
            PreparationUnlinkFaultModeV1::WriteFailure => {
                fs::remove_file(&self.preparation).unwrap();
                fs::rename(&self.held_preparation, &self.preparation).unwrap();
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
                    fs::remove_file(&self.spool_path).unwrap();
                }
                PreparationUnlinkFaultModeV1::PermissionDenied => {
                    use std::os::unix::fs::PermissionsExt;

                    fs::set_permissions(&self.preparation, fs::Permissions::from_mode(0o500))
                        .unwrap();
                }
                PreparationUnlinkFaultModeV1::WriteFailure => {
                    fs::rename(&self.preparation, &self.held_preparation).unwrap();
                    fs::write(&self.preparation, b"not-a-directory").unwrap();
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

impl CdcControlV1 for PoisonStorageAtCarrierLinkV1 {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for PoisonStorageAtCarrierLinkV1 {
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
            // CarrierHardLink is sampled at the actual no-replace operation
            // point, after the immutable namespace charge and before any
            // carrier name can become visible.
            self.fired = true;
            self.cas.poison_storage_admission_for_test_v1();
            Some(self.link_error)
        } else {
            None
        }
    }

    fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
        self.fail_invalidation && target == FsCasCleanupTargetV1::RootInvalidation
    }
}

impl CdcControlV1 for InstallCarrierAndPoisonStorageBeforeLinkV1 {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for InstallCarrierAndPoisonStorageBeforeLinkV1 {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if self.installed.is_none() && boundary == FsCasBoundaryV1::BeforeCarrierInstall {
            self.installed = Some(
                self.cas
                    .install_single_prepared_carrier_for_test_v1()
                    .expect("independent carrier install must win the test race"),
            );
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
        self.fail_invalidation && target == FsCasCleanupTargetV1::RootInvalidation
    }
}

impl CdcControlV1 for FailCarrierAliasPreparationAccountingV1 {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for FailCarrierAliasPreparationAccountingV1 {
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
            // Arm the accounting failure at the last boundary before the real
            // alias unlink. Returning `None` deliberately allows that unlink
            // to complete before the root-owned preparation transition fails.
            self.armed = true;
            self.cas.fail_next_preparation_remove_for_test_v1();
        }
        None
    }

    fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
        if target == FsCasCleanupTargetV1::RootInvalidation {
            self.root_invalidation_callbacks += 1;
            return self.fail_invalidation;
        }
        false
    }
}

fn run_small_create_with_callback_observation<C>(
    cas: &FsCasV1,
    cancellation_key: u64,
    control: &mut C,
    bound_invoked: &AtomicBool,
    supply_invoked: &AtomicBool,
) -> (
    Result<layerfs_storage::lifecycle::OperationHandoffV1, OperationErrorV1>,
    OperationCountersV1,
)
where
    C: CdcControlV1 + FsCasControlV1,
{
    run_small_create_with_supplier(
        cas,
        cancellation_key,
        control,
        CallbackCheckedSupplier {
            bound_invoked,
            supply_invoked,
        },
    )
}

#[test]
fn closure_marker_preparation_holds_neither_root_fence() {
    let fixture = TestRoot::new("closure-marker-lock-scope");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let mut control = ObserveClosureMarkerLockScopeV1 {
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
    let (result, counters) = run_small_create_with_supplier(
        &cas,
        0x0011_5500,
        &mut control,
        CheckedSupplier { bytes: &[0x5a] },
    );
    result.unwrap();
    assert!(control.observed);
    assert!(control.visibility_available);
    assert!(control.publication_available);
    // The marker pair first proves that publication is acquirable without
    // prematurely attributing any of its wait to visibility, releases that
    // preflight acquisition, then acquires the final publication/visibility
    // pair. Both physical acquisitions are direct observations.
    assert_eq!(control.closure_publication_acquisitions, 2);
    assert_eq!(control.closure_publication_releases, 2);
    assert_eq!(
        counters.visibility_lock_acquisitions,
        control.visibility_acquisitions
    );
    assert_eq!(control.visibility_acquisitions, control.visibility_releases);
    assert_eq!(
        counters.publication_lock_acquisitions,
        control.publication_acquisitions
    );
    assert_eq!(
        control.publication_acquisitions,
        control.publication_releases
    );
    assert_storage_equations(&counters);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn writer_transfers_direct_visibility_and_publication_observations() {
    let fixture = TestRoot::new("writer-direct-root-lock-observations");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let (result, counters) = run_small_create_with_supplier(
        &cas,
        0x0011_5501,
        &mut ContinueControl,
        CheckedSupplier { bytes: &[0x5b] },
    );

    result.unwrap();
    assert!(counters.visibility_lock_acquisitions > 0);
    assert!(counters.publication_lock_acquisitions > 0);
    assert_storage_equations(&counters);
    assert!(counters.has_zero_forbidden_work());
}

fn run_small_create_with_supplier<C, S>(
    cas: &FsCasV1,
    cancellation_key: u64,
    control: &mut C,
    supplier: S,
) -> (
    Result<layerfs_storage::lifecycle::OperationHandoffV1, OperationErrorV1>,
    OperationCountersV1,
)
where
    C: CdcControlV1 + FsCasControlV1,
    S: SourceSupplierV1,
{
    let mut counters = OperationCountersV1::default();
    let result = run_small_create_with_supplier_and_counters(
        cas,
        cancellation_key,
        control,
        supplier,
        &mut counters,
    );
    (result, counters)
}

fn run_small_create_with_supplier_and_counters<C, S>(
    cas: &FsCasV1,
    cancellation_key: u64,
    control: &mut C,
    supplier: S,
    counters: &mut OperationCountersV1,
) -> Result<layerfs_storage::lifecycle::OperationHandoffV1, OperationErrorV1>
where
    C: CdcControlV1 + FsCasControlV1,
    S: SourceSupplierV1,
{
    run_create_with_supplier_and_counters(
        cas,
        cancellation_key,
        control,
        CdcAlgorithmV1::FastCdc,
        1,
        supplier,
        counters,
    )
}

fn run_create_with_supplier_and_counters<C, S>(
    cas: &FsCasV1,
    cancellation_key: u64,
    control: &mut C,
    algorithm: CdcAlgorithmV1,
    declared_len: u64,
    supplier: S,
    counters: &mut OperationCountersV1,
) -> Result<layerfs_storage::lifecycle::OperationHandoffV1, OperationErrorV1>
where
    C: CdcControlV1 + FsCasControlV1,
    S: SourceSupplierV1,
{
    let mut source_window = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
    let mut cdc_ring = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
    let mut incoming = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut occupied = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut tree_object = boxed_zeroes::<MAX_TREE_OBJECT_BYTES>();
    let mut tree_pages = boxed_tree_pages();
    let mut traversal = [0_u8; 64];
    let grant = request_create_operation_v1(cas, cancellation_key, counters, control).unwrap();
    run_create_v1(
        grant,
        algorithm,
        b"payload.bin",
        0o644,
        declared_len,
        supplier,
        OperationBuffersV1 {
            source: &mut source_window,
            cdc_ring: &mut cdc_ring,
            incoming_comparison: &mut incoming,
            occupied_comparison: &mut occupied,
            tree_object: &mut tree_object,
            tree_pages: &mut *tree_pages,
            traversal_state: &mut traversal,
        },
        control,
        counters,
    )
}

fn run_large_create_with_supplier_and_counters<C, S>(
    cas: &FsCasV1,
    cancellation_key: u64,
    control: &mut C,
    declared_len: u64,
    supplier: S,
    counters: &mut OperationCountersV1,
) -> Result<layerfs_storage::lifecycle::OperationHandoffV1, OperationErrorV1>
where
    C: CdcControlV1 + FsCasControlV1,
    S: SourceSupplierV1,
{
    let mut source_window = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
    let mut cdc_ring = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
    let mut incoming = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut occupied = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut tree_object = boxed_zeroes::<MAX_TREE_OBJECT_BYTES>();
    let mut tree_pages = boxed_tree_pages();
    let mut traversal = vec![0_u8; 64 * 1024];
    let grant = request_create_operation_v1(cas, cancellation_key, counters, control).unwrap();
    run_create_v1(
        grant,
        CdcAlgorithmV1::FastCdc,
        b"payload.bin",
        0o644,
        declared_len,
        supplier,
        OperationBuffersV1 {
            source: &mut source_window,
            cdc_ring: &mut cdc_ring,
            incoming_comparison: &mut incoming,
            occupied_comparison: &mut occupied,
            tree_object: &mut tree_object,
            tree_pages: &mut *tree_pages,
            traversal_state: &mut traversal,
        },
        control,
        counters,
    )
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
                .expect("publication barrier watchdog receiver remains live");
            self.release.wait();
            panic!("injected pre-catalog publication unwind at {boundary:?}")
        }
    }

    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
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
                reached
                    .send(())
                    .expect("active-publication wait receiver remains live");
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
enum CarrierAlreadyExistsTerminalV1 {
    Success,
    CallbackUnwind,
    CleanupFailure,
}

struct CarrierAlreadyExistsRaceControlV1 {
    restore_requested: Option<mpsc::SyncSender<()>>,
    restore_completed: mpsc::Receiver<Result<(), String>>,
    comparison_entered: Option<mpsc::SyncSender<()>>,
    comparison_release: mpsc::Receiver<()>,
    terminal: CarrierAlreadyExistsTerminalV1,
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
        if boundary != FsCasBoundaryV1::BeforeIncumbentComparisonWindow || self.comparison_gated {
            return;
        }
        self.comparison_gated = true;
        self.comparison_entered
            .take()
            .expect("comparison entry signal is emitted exactly once")
            .send(())
            .expect("comparison entry watchdog remains live");
        self.comparison_release
            .recv_timeout(Duration::from_secs(5))
            .expect("comparison release gate timed out");
        if self.terminal == CarrierAlreadyExistsTerminalV1::CallbackUnwind {
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
        if self.terminal == CarrierAlreadyExistsTerminalV1::CleanupFailure
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
        assert!(!self.no_replace_injected);
        self.restore_requested
            .take()
            .expect("carrier restore request is emitted exactly once")
            .send(())
            .expect("independent winner remains live");
        self.restore_completed
            .recv_timeout(Duration::from_secs(5))
            .expect("independent winner timed out")
            .unwrap_or_else(|error| panic!("independent winner failed: {error}"));
        self.no_replace_injected = true;
    }
}

#[test]
fn carrier_already_exists_owner_blocks_same_pack_until_adoption_terminal() {
    for (label, first_terminal) in [
        (
            "carrier-already-exists-success",
            CarrierAlreadyExistsTerminalV1::Success,
        ),
        (
            "carrier-already-exists-unwind",
            CarrierAlreadyExistsTerminalV1::CallbackUnwind,
        ),
        (
            "carrier-already-exists-cleanup-failure",
            CarrierAlreadyExistsTerminalV1::CleanupFailure,
        ),
    ] {
        let fixture = TestRoot::new(label);
        let held = TestRoot::new(&format!("{label}-held"));
        let seed = FsCasV1::create_new(fixture.path()).unwrap();
        let mut seed_control = ContinueControl;
        let mut seed_counters = OperationCountersV1::default();
        run_small_create_with_supplier_and_counters(
            &seed,
            0x0011_5570,
            &mut seed_control,
            CheckedSupplier { bytes: &[0x5a] },
            &mut seed_counters,
        )
        .unwrap();
        assert_storage_equations(&seed_counters);

        let first_cas = FsCasV1::open_existing(fixture.path()).unwrap();
        let contender_cas = FsCasV1::open_existing(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        fs::create_dir_all(held.path()).unwrap();
        let carrier = fs::read_dir(fixture.path().join("carriers"))
            .unwrap()
            .next()
            .expect("seed carrier exists")
            .unwrap()
            .path();
        let catalog = fs::read_dir(fixture.path().join("catalog"))
            .unwrap()
            .next()
            .expect("seed catalog exists")
            .unwrap()
            .path();
        let held_carrier = held.path().join("carrier");
        let held_catalog = held.path().join("catalog");
        fs::rename(&carrier, &held_carrier).unwrap();
        fs::rename(&catalog, &held_catalog).unwrap();

        let (restore_request_tx, restore_request_rx) = mpsc::sync_channel(0);
        let (restore_complete_tx, restore_complete_rx) = mpsc::sync_channel(0);
        let (comparison_entered_tx, comparison_entered_rx) = mpsc::sync_channel(1);
        let (comparison_release_tx, comparison_release_rx) = mpsc::sync_channel(0);
        let (wait_tx, wait_rx) = mpsc::sync_channel(1);
        let (contender_done_tx, contender_done_rx) = mpsc::sync_channel(1);

        std::thread::scope(|scope| {
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
                    terminal: first_terminal,
                    no_replace_injected: false,
                    comparison_gated: false,
                    cleanup_failed: false,
                };
                let mut counters = OperationCountersV1::default();
                let terminal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_small_create_with_supplier_and_counters(
                        &first_cas,
                        0x0011_5571,
                        &mut control,
                        CheckedSupplier { bytes: &[0x5a] },
                        &mut counters,
                    )
                }));
                (terminal, counters, control)
            });

            comparison_entered_rx
                .recv_timeout(Duration::from_secs(5))
                .unwrap_or_else(|error| {
                    panic!("{label}: incumbent comparison not reached: {error}")
                });

            let contender = scope.spawn(move || {
                let mut control = SignalActivePackPublicationWaitV1 {
                    reached: Some(wait_tx),
                };
                let mut counters = OperationCountersV1::default();
                let terminal = run_small_create_with_supplier_and_counters(
                    &contender_cas,
                    0x0011_5572,
                    &mut control,
                    CheckedSupplier { bytes: &[0x5a] },
                    &mut counters,
                );
                let _ = contender_done_tx.send(());
                (terminal, counters)
            });

            let wait_observed = wait_rx.recv_timeout(Duration::from_secs(5));
            let premature = contender_done_rx.recv_timeout(Duration::from_millis(100));
            let _ = comparison_release_tx.send(());
            wait_observed.unwrap_or_else(|error| {
                panic!("{label}: contender missed the active owner: {error}")
            });
            assert!(
                matches!(premature, Err(mpsc::RecvTimeoutError::Timeout)),
                "{label}: contender completed before adoption terminal"
            );

            installer
                .join()
                .expect("independent winner thread did not panic")
                .unwrap_or_else(|error| panic!("{label}: {error}"));
            let (first_result, first_counters, control) =
                first.join().expect("first writer thread did not panic");
            let (contender_result, contender_counters) =
                contender.join().expect("contender thread did not panic");

            assert!(control.no_replace_injected, "{label}");
            assert!(control.comparison_gated, "{label}");
            assert_storage_equations(&first_counters);
            assert_storage_equations(&contender_counters);
            assert!(first_counters.has_zero_forbidden_work(), "{label}");
            assert!(contender_counters.has_zero_forbidden_work(), "{label}");
            assert!(
                first_counters.publication_lock_wait_nanoseconds > 0,
                "{label}"
            );
            assert!(
                first_counters.publication_lock_hold_nanoseconds > 0,
                "{label}"
            );
            assert!(
                first_counters.visibility_lock_wait_nanoseconds > 0,
                "{label}"
            );
            assert!(
                first_counters.visibility_lock_hold_nanoseconds > 0,
                "{label}"
            );
            assert!(
                contender_counters.active_pack_publication_wait_polls > 0,
                "{label}"
            );
            assert!(
                contender_counters.active_pack_publication_wait_nanoseconds > 0,
                "{label}"
            );
            assert!(
                contender_counters.publication_lock_acquisitions > 0,
                "{label}"
            );
            assert!(
                contender_counters.publication_lock_hold_nanoseconds > 0,
                "{label}"
            );

            match first_terminal {
                CarrierAlreadyExistsTerminalV1::Success => {
                    first_result.unwrap().unwrap();
                    contender_result.unwrap();
                    assert_eq!(first_counters.storage_bytes_committed, 0, "{label}");
                    assert_eq!(first_counters.storage_inodes_committed, 0, "{label}");
                    assert_eq!(first_counters.storage_bytes_retained, 0, "{label}");
                    assert_eq!(first_counters.storage_inodes_retained, 0, "{label}");
                    assert_eq!(contender_counters.storage_bytes_committed, 0, "{label}");
                    assert_eq!(contender_counters.storage_inodes_committed, 0, "{label}");
                    assert_operation_authority_baseline(&seed, fixture.path());
                }
                CarrierAlreadyExistsTerminalV1::CallbackUnwind => {
                    assert!(first_result.is_err(), "{label}");
                    contender_result.unwrap();
                    assert_eq!(first_counters.storage_bytes_retained, 0, "{label}");
                    assert_eq!(first_counters.storage_inodes_retained, 0, "{label}");
                    assert_operation_authority_baseline(&seed, fixture.path());
                }
                CarrierAlreadyExistsTerminalV1::CleanupFailure => {
                    assert!(
                        matches!(
                            first_result,
                            Ok(Err(OperationErrorV1::FsCas(FsCasErrorV1::CleanupFailed(
                                FsCasCleanupTargetV1::PrivatePack
                            ))))
                        ),
                        "{label}: {first_result:?}"
                    );
                    assert!(
                        matches!(
                            contender_result,
                            Err(OperationErrorV1::FsCas(FsCasErrorV1::Invalidated))
                        ),
                        "{label}: {contender_result:?}"
                    );
                    assert!(first_counters.storage_bytes_retained > 0, "{label}");
                    assert!(first_counters.storage_inodes_retained > 0, "{label}");
                    assert!(control.cleanup_failed, "{label}");
                    let ((preparation_bytes, preparation_inodes), _) =
                        exact_operation_namespace_usage(fixture.path());
                    assert_eq!(
                        first_counters.storage_bytes_retained, preparation_bytes,
                        "{label}: the failed private cleanup must retain exactly its remaining bytes"
                    );
                    assert_eq!(
                        first_counters.storage_inodes_retained, preparation_inodes,
                        "{label}: the failed private cleanup must retain exactly its remaining inode"
                    );
                    assert_eq!(
                        first_counters.mutable_preparation_residue_bytes, preparation_bytes,
                        "{label}"
                    );
                    assert_eq!(
                        first_counters.mutable_preparation_residue_inodes, preparation_inodes,
                        "{label}"
                    );
                    assert_eq!(
                        first_counters.unreachable_installed_residue_bytes, 0,
                        "{label}"
                    );
                    assert_eq!(seed.operation_admitted_slots_v1(), 0, "{label}");
                    assert_eq!(seed.operation_admission_active_for_test_v1(), 0, "{label}");
                    assert_eq!(
                        seed.storage_admission_active_for_test_v1(),
                        (0, 0, 0),
                        "{label}"
                    );
                    assert_eq!(
                        seed.operation_admission_queue_for_test_v1(),
                        (0, 0, 0),
                        "{label}"
                    );
                    assert!(matches!(seed.occupied(), Err(FsCasErrorV1::Invalidated)));
                    assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
                    assert!(matches!(
                        FsCasV1::open_existing(fixture.path()),
                        Err(FsCasErrorV1::Busy | FsCasErrorV1::Invalidated)
                    ));
                }
            }
        });
    }
}

#[test]
fn same_pack_contender_waits_for_pre_catalog_unwind_terminal_custody() {
    for (label, target) in [
        (
            "same-pack-carrier-visible-unwind",
            FsCasBoundaryV1::AfterCarrierInstall,
        ),
        (
            "same-pack-locator-visible-unwind",
            FsCasBoundaryV1::AfterObjectLocatorPublication,
        ),
    ] {
        let fixture = TestRoot::new(label);
        let seed = FsCasV1::create_new(fixture.path()).unwrap();
        let first_cas = FsCasV1::open_existing(fixture.path()).unwrap();
        let contender_cas = FsCasV1::open_existing(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let release = Arc::new(WatchdogGateV1::new());
        let mut release_guard = WatchdogGateReleaseV1::new(Arc::clone(&release));
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (wait_tx, wait_rx) = mpsc::sync_channel(1);
        let (contender_done_tx, contender_done_rx) = mpsc::sync_channel(1);

        std::thread::scope(|scope| {
            let first_release = Arc::clone(&release);
            let first = scope.spawn(move || {
                let mut control = BarrierPanicAtPackPublicationV1 {
                    target,
                    entered_signal: entered_tx,
                    release: first_release,
                    injected: false,
                };
                let mut counters = OperationCountersV1::default();
                let input = [0x5b_u8];
                let terminal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_small_create_with_supplier_and_counters(
                        &first_cas,
                        0x0011_5580,
                        &mut control,
                        CheckedSupplier { bytes: &input },
                        &mut counters,
                    )
                }));
                (terminal, counters, control.injected)
            });

            entered_rx
                .recv_timeout(Duration::from_secs(5))
                .unwrap_or_else(|error| panic!("{label}: first caller missed target: {error}"));

            let contender = scope.spawn(move || {
                let mut control = SignalActivePackPublicationWaitV1 {
                    reached: Some(wait_tx),
                };
                let mut counters = OperationCountersV1::default();
                let input = [0x5b_u8];
                let terminal = run_small_create_with_supplier_and_counters(
                    &contender_cas,
                    0x0011_5581,
                    &mut control,
                    CheckedSupplier { bytes: &input },
                    &mut counters,
                );
                contender_done_tx.send(()).unwrap();
                (terminal, counters)
            });

            let wait_observed = wait_rx.recv_timeout(Duration::from_secs(5));
            let premature = contender_done_rx.recv_timeout(Duration::from_millis(100));
            release_guard.release_v1();
            wait_observed.unwrap_or_else(|error| {
                panic!("{label}: contender never observed active publication: {error}")
            });
            assert!(
                matches!(premature, Err(mpsc::RecvTimeoutError::Timeout)),
                "{label}: contender completed before terminal custody stabilized"
            );

            let (first_terminal, first_counters, injected) = first.join().unwrap();
            let (contender_terminal, contender_counters) = contender.join().unwrap();
            assert!(injected, "{label}");
            assert!(first_terminal.is_err(), "{label}");
            assert_storage_equations(&first_counters);
            assert_storage_equations(&contender_counters);
            assert!(first_counters.has_zero_forbidden_work(), "{label}");
            assert!(contender_counters.has_zero_forbidden_work(), "{label}");
            assert!(first_counters.visibility_lock_acquisitions > 0, "{label}");
            assert!(first_counters.publication_lock_acquisitions > 0, "{label}");
            assert!(
                contender_counters.active_pack_publication_wait_polls > 0,
                "{label}"
            );
            assert!(
                contender_counters.active_pack_publication_wait_nanoseconds > 0,
                "{label}"
            );
            assert!(
                contender_counters.publication_lock_acquisitions > 0,
                "{label}"
            );
            assert!(
                contender_counters.publication_lock_wait_nanoseconds > 0,
                "{label}"
            );
            assert!(
                contender_counters.publication_lock_hold_nanoseconds > 0,
                "{label}"
            );
            assert_eq!(
                contender_counters.locator_owner_publication_wait_polls, 0,
                "{label}"
            );

            match target {
                FsCasBoundaryV1::AfterCarrierInstall => {
                    contender_terminal.unwrap();
                    assert_eq!(first_counters.storage_bytes_retained, 0, "{label}");
                    assert_eq!(first_counters.storage_inodes_retained, 0, "{label}");
                    assert_eq!(
                        fs::read_dir(fixture.path().join("carriers"))
                            .unwrap()
                            .count(),
                        1,
                        "{label}"
                    );
                    assert_eq!(
                        fs::read_dir(fixture.path().join("catalog"))
                            .unwrap()
                            .count(),
                        1,
                        "{label}"
                    );
                    assert!(seed.occupied().is_ok(), "{label}");
                    assert!(stale.occupied().is_ok(), "{label}");
                }
                FsCasBoundaryV1::AfterObjectLocatorPublication => {
                    assert!(
                        matches!(
                            contender_terminal,
                            Err(OperationErrorV1::FsCas(FsCasErrorV1::Invalidated))
                        ),
                        "{label}: {contender_terminal:?}"
                    );
                    assert!(first_counters.storage_bytes_retained > 0, "{label}");
                    assert!(first_counters.storage_inodes_retained > 0, "{label}");
                    assert_eq!(
                        fs::read_dir(fixture.path().join("catalog"))
                            .unwrap()
                            .count(),
                        0,
                        "{label}"
                    );
                    assert!(matches!(seed.occupied(), Err(FsCasErrorV1::Invalidated)));
                    assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
                    assert!(matches!(
                        FsCasV1::open_existing(fixture.path()),
                        Err(FsCasErrorV1::Busy | FsCasErrorV1::Invalidated)
                    ));
                }
                _ => unreachable!("test covers only pre-catalog visibility boundaries"),
            }
            assert_operation_authority_baseline(&seed, fixture.path());
        });
    }
}

#[test]
fn simultaneous_reopened_complete_writers_cover_equal_and_disjoint_identity_rows() {
    for (label, left_byte, right_byte, equal) in [
        ("equal-complete-writers", 0x61_u8, 0x61_u8, true),
        ("disjoint-complete-writers", 0x62_u8, 0x63_u8, false),
    ] {
        let fixture = TestRoot::new(label);
        let seed = FsCasV1::create_new(fixture.path()).unwrap();
        let left_cas = FsCasV1::open_existing(fixture.path()).unwrap();
        let right_cas = FsCasV1::open_existing(fixture.path()).unwrap();
        let start = Arc::new(WatchdogGateV1::new());
        let (ready_tx, ready_rx) = mpsc::sync_channel(2);

        let ((left_terminal, left_counters), (right_terminal, right_counters)) =
            std::thread::scope(|scope| {
                let mut start_release = WatchdogGateReleaseV1::new(Arc::clone(&start));
                let left_start = Arc::clone(&start);
                let left_ready = ready_tx.clone();
                let left = scope.spawn(move || {
                    let input = [left_byte];
                    let mut control = ContinueControl;
                    let mut counters = OperationCountersV1::default();
                    let terminal = run_small_create_with_supplier_and_counters(
                        &left_cas,
                        0x0011_5590,
                        &mut control,
                        BarrierCheckedSupplier {
                            bytes: &input,
                            ready: left_ready,
                            start: left_start,
                        },
                        &mut counters,
                    );
                    (terminal, counters)
                });
                let right_start = Arc::clone(&start);
                let right = scope.spawn(move || {
                    let input = [right_byte];
                    let mut control = ContinueControl;
                    let mut counters = OperationCountersV1::default();
                    let terminal = run_small_create_with_supplier_and_counters(
                        &right_cas,
                        0x0011_5591,
                        &mut control,
                        BarrierCheckedSupplier {
                            bytes: &input,
                            ready: ready_tx,
                            start: right_start,
                        },
                        &mut counters,
                    );
                    (terminal, counters)
                });

                ready_rx
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap_or_else(|error| panic!("{label}: left rendezvous failed: {error}"));
                ready_rx
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap_or_else(|error| panic!("{label}: right rendezvous failed: {error}"));
                start_release.release_v1();
                (left.join().unwrap(), right.join().unwrap())
            });

        let left = left_terminal.unwrap_or_else(|error| panic!("{label}: {error:?}"));
        let right = right_terminal.unwrap_or_else(|error| panic!("{label}: {error:?}"));
        for counters in [&left_counters, &right_counters] {
            assert_storage_equations(counters);
            assert!(counters.has_zero_forbidden_work(), "{label}");
            assert!(counters.visibility_lock_acquisitions > 0, "{label}");
            assert!(counters.visibility_lock_wait_nanoseconds > 0, "{label}");
            assert!(counters.visibility_lock_hold_nanoseconds > 0, "{label}");
            assert!(counters.publication_lock_acquisitions > 0, "{label}");
            assert!(counters.publication_lock_wait_nanoseconds > 0, "{label}");
            assert!(counters.publication_lock_hold_nanoseconds > 0, "{label}");
            assert_eq!(
                counters.storage_preparation_bytes_current_after_cleanup, 0,
                "{label}"
            );
            assert_eq!(
                counters.storage_preparation_inodes_current_after_cleanup, 0,
                "{label}"
            );
            assert_eq!(counters.mutable_preparation_residue_bytes, 0, "{label}");
            assert_eq!(counters.mutable_preparation_residue_inodes, 0, "{label}");
            assert_eq!(counters.storage_bytes_retained, 0, "{label}");
            assert_eq!(counters.storage_inodes_retained, 0, "{label}");
        }

        if equal {
            assert_eq!(left.version_record(), right.version_record(), "{label}");
            assert_eq!(left.root_tree(), right.root_tree(), "{label}");
            assert_eq!(left.pack(), right.pack(), "{label}");
            let outcomes = [left.pack_outcome(), right.pack_outcome()];
            assert_eq!(
                outcomes
                    .into_iter()
                    .filter(|outcome| *outcome == FsPackAdmissionOutcomeV1::Installed)
                    .count(),
                1,
                "{label}"
            );
            assert_eq!(
                outcomes
                    .into_iter()
                    .filter(|outcome| *outcome == FsPackAdmissionOutcomeV1::ExistingComplete)
                    .count(),
                1,
                "{label}"
            );
            assert_eq!(left.carriers_installed() + right.carriers_installed(), 1);
            assert_eq!(left.carriers_reused() + right.carriers_reused(), 1);
            assert_eq!(
                fs::read_dir(fixture.path().join("carriers"))
                    .unwrap()
                    .count(),
                1,
                "{label}"
            );
            let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
                exact_operation_namespace_usage(fixture.path());
            assert_eq!((preparation_bytes, preparation_inodes), (0, 0), "{label}");
            assert_eq!(
                left_counters.storage_bytes_committed + right_counters.storage_bytes_committed,
                immutable_bytes,
                "{label}: carrier and closure custody must be exact across both tokens"
            );
            assert_eq!(
                left_counters.storage_inodes_committed + right_counters.storage_inodes_committed,
                immutable_inodes,
                "{label}: no canonical namespace entry may be double-counted"
            );
            let (installer_counters, adopter_counters) =
                if left.pack_outcome() == FsPackAdmissionOutcomeV1::Installed {
                    (&left_counters, &right_counters)
                } else {
                    (&right_counters, &left_counters)
                };
            let (pack_namespace_bytes, pack_namespace_inodes) = ["carriers", "objects", "catalog"]
                .into_iter()
                .map(|name| exact_directory_usage(&fixture.path().join(name)))
                .fold(
                    (0_u64, 0_u64),
                    |(bytes, inodes), (next_bytes, next_inodes)| {
                        (
                            bytes.checked_add(next_bytes).unwrap(),
                            inodes.checked_add(next_inodes).unwrap(),
                        )
                    },
                );
            let (closure_bytes, closure_inodes) =
                exact_directory_usage(&fixture.path().join("closures"));
            assert_eq!(
                installer_counters
                    .storage_bytes_committed
                    .checked_sub(pack_namespace_bytes)
                    .unwrap(),
                closure_bytes
                    .checked_sub(adopter_counters.storage_bytes_committed)
                    .unwrap(),
                "{label}: the pack installer owns the exact carrier/locator/catalog bytes; closure ownership may be split"
            );
            assert_eq!(
                installer_counters
                    .storage_inodes_committed
                    .checked_sub(pack_namespace_inodes)
                    .unwrap(),
                closure_inodes
                    .checked_sub(adopter_counters.storage_inodes_committed)
                    .unwrap(),
                "{label}: the pack installer owns the exact carrier/locator/catalog names; closure ownership may be split"
            );
            assert!(
                adopter_counters.storage_bytes_committed <= closure_bytes,
                "{label}: the pack adopter may commit only the independent canonical closure marker"
            );
            assert!(
                adopter_counters.storage_inodes_committed <= closure_inodes,
                "{label}: the pack adopter may commit only the independent canonical closure name"
            );
            assert_eq!(
                adopter_counters.storage_bytes_reserved,
                adopter_counters
                    .storage_bytes_released
                    .checked_add(adopter_counters.storage_bytes_committed)
                    .unwrap(),
                "{label}: the adopter releases every private byte except an independently won closure marker"
            );
            assert_eq!(
                adopter_counters.storage_inodes_reserved,
                adopter_counters
                    .storage_inodes_released
                    .checked_add(adopter_counters.storage_inodes_committed)
                    .unwrap(),
                "{label}: the adopter releases every private name except an independently won closure marker"
            );
            assert_eq!(
                fs::read_dir(fixture.path().join("catalog"))
                    .unwrap()
                    .count(),
                1,
                "{label}"
            );
            assert_eq!(
                fs::read_dir(fixture.path().join("closures"))
                    .unwrap()
                    .count(),
                1,
                "{label}"
            );
        } else {
            assert_ne!(left.version_record(), right.version_record(), "{label}");
            assert_ne!(left.root_tree(), right.root_tree(), "{label}");
            assert_ne!(left.pack(), right.pack(), "{label}");
            assert_eq!(left.pack_outcome(), FsPackAdmissionOutcomeV1::Installed);
            assert_eq!(right.pack_outcome(), FsPackAdmissionOutcomeV1::Installed);
            assert_eq!(left.carriers_installed(), 1, "{label}");
            assert_eq!(right.carriers_installed(), 1, "{label}");
            assert_eq!(left.carriers_reused(), 0, "{label}");
            assert_eq!(right.carriers_reused(), 0, "{label}");
            assert!(left_counters.storage_bytes_committed > 0, "{label}");
            assert!(right_counters.storage_bytes_committed > 0, "{label}");
            assert_eq!(
                fs::read_dir(fixture.path().join("carriers"))
                    .unwrap()
                    .count(),
                2,
                "{label}"
            );
            assert_eq!(
                fs::read_dir(fixture.path().join("catalog"))
                    .unwrap()
                    .count(),
                2,
                "{label}"
            );
            assert_eq!(
                fs::read_dir(fixture.path().join("closures"))
                    .unwrap()
                    .count(),
                2,
                "{label}"
            );
            let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
                exact_operation_namespace_usage(fixture.path());
            assert_eq!((preparation_bytes, preparation_inodes), (0, 0), "{label}");
            assert_eq!(
                left_counters.storage_bytes_committed + right_counters.storage_bytes_committed,
                immutable_bytes,
                "{label}"
            );
            assert_eq!(
                left_counters.storage_inodes_committed + right_counters.storage_inodes_committed,
                immutable_inodes,
                "{label}"
            );
        }
        assert_operation_authority_baseline(&seed, fixture.path());
        assert_eq!(seed.operation_admission_queue_for_test_v1(), (0, 0, 0));
    }
}

#[derive(Clone, Copy, Debug)]
enum ConcurrentFailureV1 {
    Typed,
    Cancelled,
    Deadline,
}

#[test]
fn simultaneous_reopened_success_crosses_typed_cancelled_and_deadline_terminals() {
    for (label, failure) in [
        ("success-crosses-typed-failure", ConcurrentFailureV1::Typed),
        (
            "success-crosses-cancellation",
            ConcurrentFailureV1::Cancelled,
        ),
        ("success-crosses-deadline", ConcurrentFailureV1::Deadline),
    ] {
        let fixture = TestRoot::new(label);
        let seed = FsCasV1::create_new(fixture.path()).unwrap();
        let success_cas = FsCasV1::open_existing(fixture.path()).unwrap();
        let failure_cas = FsCasV1::open_existing(fixture.path()).unwrap();
        let start = Arc::new(WatchdogGateV1::new());
        let (ready_tx, ready_rx) = mpsc::sync_channel(2);

        let ((success_terminal, success_counters), (failure_terminal, failure_counters)) =
            std::thread::scope(|scope| {
                let mut start_release = WatchdogGateReleaseV1::new(Arc::clone(&start));
                let success_start = Arc::clone(&start);
                let success_ready = ready_tx.clone();
                let success = scope.spawn(move || {
                    let input = [0x71_u8];
                    let mut control = ContinueControl;
                    let mut counters = OperationCountersV1::default();
                    let terminal = run_small_create_with_supplier_and_counters(
                        &success_cas,
                        0x0011_55a0,
                        &mut control,
                        BarrierCheckedSupplier {
                            bytes: &input,
                            ready: success_ready,
                            start: success_start,
                        },
                        &mut counters,
                    );
                    (terminal, counters)
                });

                let failure_start = Arc::clone(&start);
                let failed = scope.spawn(move || {
                    let mut counters = OperationCountersV1::default();
                    let terminal = match failure {
                        ConcurrentFailureV1::Typed => {
                            let mut control = ContinueControl;
                            run_small_create_with_supplier_and_counters(
                                &failure_cas,
                                0x0011_55a1,
                                &mut control,
                                BarrierFailingSupplier {
                                    ready: ready_tx,
                                    start: failure_start,
                                },
                                &mut counters,
                            )
                        }
                        ConcurrentFailureV1::Cancelled | ConcurrentFailureV1::Deadline => {
                            let input = [0x72_u8];
                            let stop = match failure {
                                ConcurrentFailureV1::Cancelled => {
                                    CandidateValidationStopV1::Cancelled
                                }
                                ConcurrentFailureV1::Deadline => {
                                    CandidateValidationStopV1::Deadline
                                }
                                ConcurrentFailureV1::Typed => unreachable!(),
                            };
                            let mut control = StopBeforeCandidateValidationV1::new(stop);
                            run_small_create_with_supplier_and_counters(
                                &failure_cas,
                                0x0011_55a2,
                                &mut control,
                                BarrierCheckedSupplier {
                                    bytes: &input,
                                    ready: ready_tx,
                                    start: failure_start,
                                },
                                &mut counters,
                            )
                        }
                    };
                    (terminal, counters)
                });

                ready_rx
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap_or_else(|error| panic!("{label}: first rendezvous failed: {error}"));
                ready_rx
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap_or_else(|error| panic!("{label}: second rendezvous failed: {error}"));
                start_release.release_v1();
                (success.join().unwrap(), failed.join().unwrap())
            });

        let success = success_terminal.unwrap_or_else(|error| panic!("{label}: {error:?}"));
        let failure_error = failure_terminal.unwrap_err();
        match failure {
            ConcurrentFailureV1::Typed => {
                assert_eq!(failure_error, OperationErrorV1::Core(CoreError::CountCap));
            }
            ConcurrentFailureV1::Cancelled => assert_eq!(
                failure_error,
                OperationErrorV1::FsCas(FsCasErrorV1::Core(CoreError::Cancelled))
            ),
            ConcurrentFailureV1::Deadline => assert_eq!(
                failure_error,
                OperationErrorV1::FsCas(FsCasErrorV1::Core(CoreError::Deadline))
            ),
        }

        for counters in [&success_counters, &failure_counters] {
            assert_storage_equations(counters);
            assert!(counters.has_zero_forbidden_work(), "{label}");
            assert_eq!(
                counters.storage_preparation_bytes_current_after_cleanup, 0,
                "{label}"
            );
            assert_eq!(
                counters.storage_preparation_inodes_current_after_cleanup, 0,
                "{label}"
            );
            assert_eq!(counters.mutable_preparation_residue_bytes, 0, "{label}");
            assert_eq!(counters.mutable_preparation_residue_inodes, 0, "{label}");
            assert_eq!(counters.storage_bytes_retained, 0, "{label}");
            assert_eq!(counters.storage_inodes_retained, 0, "{label}");
        }
        assert!(success_counters.visibility_lock_acquisitions > 0, "{label}");
        assert!(
            success_counters.visibility_lock_wait_nanoseconds > 0,
            "{label}"
        );
        assert!(
            success_counters.visibility_lock_hold_nanoseconds > 0,
            "{label}"
        );
        assert!(
            success_counters.publication_lock_acquisitions > 0,
            "{label}"
        );
        assert!(
            success_counters.publication_lock_wait_nanoseconds > 0,
            "{label}"
        );
        assert!(
            success_counters.publication_lock_hold_nanoseconds > 0,
            "{label}"
        );
        assert!(failure_counters.visibility_lock_acquisitions > 0, "{label}");
        assert!(
            failure_counters.visibility_lock_wait_nanoseconds > 0,
            "{label}"
        );
        assert!(
            failure_counters.visibility_lock_hold_nanoseconds > 0,
            "{label}"
        );
        assert_eq!(failure_counters.storage_bytes_committed, 0, "{label}");
        assert_eq!(failure_counters.storage_inodes_committed, 0, "{label}");
        assert!(
            success_counters.root_admission_active_slots_high_water >= 2
                || failure_counters.root_admission_active_slots_high_water >= 2,
            "{label}: barrier-overlapped operations must be directly reflected in root admission"
        );
        assert_eq!(success.pack_outcome(), FsPackAdmissionOutcomeV1::Installed);
        let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
            exact_operation_namespace_usage(fixture.path());
        assert_eq!((preparation_bytes, preparation_inodes), (0, 0), "{label}");
        assert_eq!(
            success_counters.storage_bytes_committed, immutable_bytes,
            "{label}"
        );
        assert_eq!(
            success_counters.storage_inodes_committed, immutable_inodes,
            "{label}"
        );
        assert_operation_authority_baseline(&seed, fixture.path());
        assert!(seed.occupied().is_ok(), "{label}");
        assert!(FsCasV1::open_existing(fixture.path())
            .unwrap()
            .occupied()
            .is_ok());
    }
}

#[test]
fn reopened_complete_writer_admission_levels_balance_every_overlapped_token() {
    for level in [1_usize, 2, 4, 8, 16] {
        let label = format!("complete-writer-admission-level-{level}");
        let fixture = TestRoot::new(&label);
        let seed = FsCasV1::create_new(fixture.path()).unwrap();
        let callers = (0..level)
            .map(|_| FsCasV1::open_existing(fixture.path()).unwrap())
            .collect::<Vec<_>>();
        let start = Arc::new(WatchdogGateV1::new());
        let (ready_tx, ready_rx) = mpsc::sync_channel(level);

        let results = std::thread::scope(|scope| {
            let mut start_release = WatchdogGateReleaseV1::new(Arc::clone(&start));
            let joins = callers
                .into_iter()
                .enumerate()
                .map(|(index, cas)| {
                    let start = Arc::clone(&start);
                    let ready = ready_tx.clone();
                    scope.spawn(move || {
                        let input = [0x90_u8.checked_add(index as u8).unwrap()];
                        let mut control = ContinueControl;
                        let mut counters = OperationCountersV1::default();
                        let terminal = run_small_create_with_supplier_and_counters(
                            &cas,
                            0x0011_5600 + index as u64,
                            &mut control,
                            BarrierCheckedSupplier {
                                bytes: &input,
                                ready,
                                start,
                            },
                            &mut counters,
                        );
                        (terminal, counters)
                    })
                })
                .collect::<Vec<_>>();
            for index in 0..level {
                ready_rx
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap_or_else(|error| {
                        panic!("{label}: rendezvous {index}/{level} failed: {error}")
                    });
            }
            start_release.release_v1();
            joins
                .into_iter()
                .map(|join| join.join().unwrap())
                .collect::<Vec<_>>()
        });

        let mut total_committed_bytes = 0_u64;
        let mut total_committed_inodes = 0_u64;
        let mut total_reserved_bytes = 0_u64;
        let mut total_reserved_inodes = 0_u64;
        let mut observed_admission_high_water = 0_u64;
        let mut observed_root_bytes_high_water = 0_u64;
        let mut observed_root_inodes_high_water = 0_u64;
        for (terminal, counters) in results {
            let handoff = terminal.unwrap_or_else(|error| panic!("{label}: {error:?}"));
            assert_eq!(
                handoff.pack_outcome(),
                FsPackAdmissionOutcomeV1::Installed,
                "{label}"
            );
            assert_eq!(handoff.carriers_installed(), 1, "{label}");
            assert_eq!(handoff.carriers_reused(), 0, "{label}");
            assert_storage_equations(&counters);
            assert!(counters.has_zero_forbidden_work(), "{label}");
            assert!(counters.visibility_lock_acquisitions > 0, "{label}");
            assert!(counters.visibility_lock_wait_nanoseconds > 0, "{label}");
            assert!(counters.visibility_lock_hold_nanoseconds > 0, "{label}");
            assert!(counters.publication_lock_acquisitions > 0, "{label}");
            assert!(counters.publication_lock_wait_nanoseconds > 0, "{label}");
            assert!(counters.publication_lock_hold_nanoseconds > 0, "{label}");
            assert!(counters.storage_preparation_bytes_high_water > 0, "{label}");
            assert!(
                counters.storage_preparation_inodes_high_water > 0,
                "{label}"
            );
            assert!(counters.layerfs_open_file_handles_high_water > 0, "{label}");
            assert!(counters.memory_high_water > 0, "{label}");
            assert_eq!(
                counters.storage_preparation_bytes_current_after_cleanup, 0,
                "{label}"
            );
            assert_eq!(
                counters.storage_preparation_inodes_current_after_cleanup, 0,
                "{label}"
            );
            assert_eq!(counters.mutable_preparation_residue_bytes, 0, "{label}");
            assert_eq!(counters.mutable_preparation_residue_inodes, 0, "{label}");
            assert_eq!(counters.storage_bytes_retained, 0, "{label}");
            assert_eq!(counters.storage_inodes_retained, 0, "{label}");
            total_committed_bytes = total_committed_bytes
                .checked_add(counters.storage_bytes_committed)
                .unwrap();
            total_committed_inodes = total_committed_inodes
                .checked_add(counters.storage_inodes_committed)
                .unwrap();
            total_reserved_bytes = total_reserved_bytes
                .checked_add(counters.storage_bytes_reserved)
                .unwrap();
            total_reserved_inodes = total_reserved_inodes
                .checked_add(counters.storage_inodes_reserved)
                .unwrap();
            observed_admission_high_water =
                observed_admission_high_water.max(counters.root_admission_active_slots_high_water);
            observed_root_bytes_high_water = observed_root_bytes_high_water
                .max(counters.root_storage_active_reserved_bytes_lifetime_high_water);
            observed_root_inodes_high_water = observed_root_inodes_high_water
                .max(counters.root_storage_active_reserved_inodes_lifetime_high_water);
        }

        assert_eq!(
            observed_admission_high_water, level as u64,
            "{label}: the source barrier is reached only after every root slot is granted"
        );
        assert!(
            observed_root_bytes_high_water >= total_reserved_bytes,
            "{label}: byte lifetime high-water must reflect simultaneous reservations"
        );
        assert!(
            observed_root_inodes_high_water >= total_reserved_inodes,
            "{label}: inode lifetime high-water must reflect simultaneous reservations"
        );
        let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
            exact_operation_namespace_usage(fixture.path());
        assert_eq!((preparation_bytes, preparation_inodes), (0, 0), "{label}");
        assert_eq!(total_committed_bytes, immutable_bytes, "{label}");
        assert_eq!(total_committed_inodes, immutable_inodes, "{label}");
        assert_eq!(
            fs::read_dir(fixture.path().join("carriers"))
                .unwrap()
                .count(),
            level,
            "{label}"
        );
        assert_eq!(
            fs::read_dir(fixture.path().join("catalog"))
                .unwrap()
                .count(),
            level,
            "{label}"
        );
        assert_eq!(
            fs::read_dir(fixture.path().join("closures"))
                .unwrap()
                .count(),
            level,
            "{label}"
        );
        assert_operation_authority_baseline(&seed, fixture.path());
        assert!(seed.occupied().is_ok(), "{label}");
    }
}

#[test]
fn reopened_multi_pack_writer_overlaps_disjoint_complete_writer() {
    const MULTI_PACK_BYTES: u64 = 65 * 1024 * 1024;

    let fixture = TestRoot::new("overlapped-multi-pack-writer");
    let seed = FsCasV1::create_new(fixture.path()).unwrap();
    let multi_cas = FsCasV1::open_existing(fixture.path()).unwrap();
    let disjoint_cas = FsCasV1::open_existing(fixture.path()).unwrap();
    let start = Arc::new(WatchdogGateV1::new());
    let (ready_tx, ready_rx) = mpsc::sync_channel(2);

    let ((multi_terminal, multi_counters), (disjoint_terminal, disjoint_counters)) =
        std::thread::scope(|scope| {
            let mut start_release = WatchdogGateReleaseV1::new(Arc::clone(&start));
            let multi_start = Arc::clone(&start);
            let multi_ready = ready_tx.clone();
            let multi = scope.spawn(move || {
                let mut control = ContinueControl;
                let mut counters = OperationCountersV1::default();
                let terminal = run_large_create_with_supplier_and_counters(
                    &multi_cas,
                    0x0011_5700,
                    &mut control,
                    MULTI_PACK_BYTES,
                    BarrierCounterSupplier {
                        len: MULTI_PACK_BYTES,
                        ready: multi_ready,
                        start: multi_start,
                    },
                    &mut counters,
                );
                (terminal, counters)
            });
            let disjoint_start = Arc::clone(&start);
            let disjoint = scope.spawn(move || {
                let input = [0xd1_u8];
                let mut control = ContinueControl;
                let mut counters = OperationCountersV1::default();
                let terminal = run_small_create_with_supplier_and_counters(
                    &disjoint_cas,
                    0x0011_5701,
                    &mut control,
                    BarrierCheckedSupplier {
                        bytes: &input,
                        ready: ready_tx,
                        start: disjoint_start,
                    },
                    &mut counters,
                );
                (terminal, counters)
            });

            ready_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("multi-pack writer did not reach the post-reservation barrier");
            ready_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("disjoint writer did not reach the post-reservation barrier");
            start_release.release_v1();
            (multi.join().unwrap(), disjoint.join().unwrap())
        });

    let multi = multi_terminal.unwrap_or_else(|error| panic!("{error:?}; {multi_counters:#?}"));
    let disjoint = disjoint_terminal.unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(multi.carrier_count(), 2);
    assert_eq!(multi.carrier_rollovers(), 1);
    assert_eq!(multi.carriers_installed(), 2);
    assert_eq!(multi.carriers_reused(), 0);
    assert_eq!(disjoint.carrier_count(), 1);
    assert_eq!(disjoint.carriers_installed(), 1);
    assert_eq!(disjoint.carriers_reused(), 0);
    for counters in [&multi_counters, &disjoint_counters] {
        assert_storage_equations(counters);
        assert!(counters.has_zero_forbidden_work());
        assert!(counters.visibility_lock_acquisitions > 0);
        assert!(counters.visibility_lock_wait_nanoseconds > 0);
        assert!(counters.visibility_lock_hold_nanoseconds > 0);
        assert!(counters.publication_lock_acquisitions > 0);
        assert!(counters.publication_lock_wait_nanoseconds > 0);
        assert!(counters.publication_lock_hold_nanoseconds > 0);
        assert_eq!(counters.storage_preparation_bytes_current_after_cleanup, 0);
        assert_eq!(counters.storage_preparation_inodes_current_after_cleanup, 0);
        assert_eq!(counters.mutable_preparation_residue_bytes, 0);
        assert_eq!(counters.mutable_preparation_residue_inodes, 0);
        assert_eq!(counters.storage_bytes_retained, 0);
        assert_eq!(counters.storage_inodes_retained, 0);
    }
    assert_eq!(multi_counters.source_bytes_read, MULTI_PACK_BYTES);
    assert!(multi_counters.file_sort_control_polls > 0);
    assert!(
        multi_counters.root_admission_active_slots_high_water >= 2
            || disjoint_counters.root_admission_active_slots_high_water >= 2
    );
    let total_reserved_bytes =
        multi_counters.storage_bytes_reserved + disjoint_counters.storage_bytes_reserved;
    let total_reserved_inodes =
        multi_counters.storage_inodes_reserved + disjoint_counters.storage_inodes_reserved;
    assert!(
        multi_counters
            .root_storage_active_reserved_bytes_lifetime_high_water
            .max(disjoint_counters.root_storage_active_reserved_bytes_lifetime_high_water)
            >= total_reserved_bytes
    );
    assert!(
        multi_counters
            .root_storage_active_reserved_inodes_lifetime_high_water
            .max(disjoint_counters.root_storage_active_reserved_inodes_lifetime_high_water)
            >= total_reserved_inodes
    );
    let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
        exact_operation_namespace_usage(fixture.path());
    assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
    assert_eq!(
        multi_counters.storage_bytes_committed + disjoint_counters.storage_bytes_committed,
        immutable_bytes
    );
    assert_eq!(
        multi_counters.storage_inodes_committed + disjoint_counters.storage_inodes_committed,
        immutable_inodes
    );
    assert_eq!(
        fs::read_dir(fixture.path().join("carriers"))
            .unwrap()
            .count(),
        3
    );
    assert_eq!(
        fs::read_dir(fixture.path().join("catalog"))
            .unwrap()
            .count(),
        3
    );
    assert_eq!(
        fs::read_dir(fixture.path().join("closures"))
            .unwrap()
            .count(),
        2
    );
    assert_operation_authority_baseline(&seed, fixture.path());
}

#[test]
fn lifecycle_storage_counter_merge_overflow_is_transactional_and_terminal() {
    let fixture = TestRoot::new("lifecycle-storage-counter-merge-overflow");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let mut counters = OperationCountersV1 {
        physical_carrier_object_writes: 41,
        pack_entries: 43,
        pack_bytes: 47,
        carrier_bytes_total: u64::MAX,
        ..OperationCountersV1::default()
    };
    // These fields precede `carrier_bytes_total` in the checked merge.  A
    // field-by-field in-place implementation would transfer the storage
    // session's real values before discovering the forced late overflow.
    let mut control = ContinueControl;

    let terminal = run_small_create_with_supplier_and_counters(
        &cas,
        0x0011_5504,
        &mut control,
        CheckedSupplier { bytes: &[0x5a] },
        &mut counters,
    );

    assert_eq!(
        terminal.unwrap_err(),
        OperationErrorV1::Core(CoreError::IntegerOverflow)
    );
    assert_eq!(counters.physical_carrier_object_writes, 41);
    assert_eq!(counters.pack_entries, 43);
    assert_eq!(counters.pack_bytes, 47);
    assert_eq!(counters.carrier_bytes_total, u64::MAX);
    assert_operation_authority_baseline(&cas, fixture.path());
    assert_storage_equations(&counters);
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert!(counters.storage_bytes_retained > 0);
    assert!(counters.storage_inodes_retained > 0);
    let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
        exact_operation_namespace_usage(fixture.path());
    assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
    assert_eq!(
        (
            counters.storage_bytes_retained,
            counters.storage_inodes_retained
        ),
        (immutable_bytes, immutable_inodes)
    );
    assert_eq!(
        counters.unreachable_installed_residue_bytes,
        immutable_bytes
    );
    assert!(counters.has_zero_forbidden_work());
    assert!(cas.occupied().is_ok());
    assert!(stale.occupied().is_ok());
}

#[test]
fn complete_create_cdc_stream_overflow_is_transactional_and_terminal() {
    const LOGICAL_BYTES: u64 = 64 * 1024;

    let fixture = TestRoot::new("complete-create-cdc-counter-overflow");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let mut counters = OperationCountersV1 {
        ring_fills: 41,
        ring_wrap_spans: 43,
        cdc_scan_calls: 47,
        cdc_scan_bytes: 53,
        bytes_boundary_inspected: u64::MAX,
        ..OperationCountersV1::default()
    };
    let mut control = ContinueControl;

    let terminal = run_create_with_supplier_and_counters(
        &cas,
        0x0011_5505,
        &mut control,
        CdcAlgorithmV1::FastCdc,
        LOGICAL_BYTES,
        CounterSupplier { len: LOGICAL_BYTES },
        &mut counters,
    );

    assert_eq!(
        terminal.unwrap_err(),
        OperationErrorV1::Core(CoreError::IntegerOverflow)
    );
    assert_eq!(counters.ring_fills, 41);
    assert_eq!(counters.ring_wrap_spans, 43);
    assert_eq!(counters.cdc_scan_calls, 47);
    assert_eq!(counters.cdc_scan_bytes, 53);
    assert_eq!(counters.bytes_boundary_inspected, u64::MAX);
    assert!(counters.source_read_calls > 0);
    assert!(counters.source_bytes_read > 0);
    assert_operation_authority_baseline(&cas, fixture.path());
    assert_storage_equations(&counters);
    let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
        exact_operation_namespace_usage(fixture.path());
    assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
    assert_eq!(
        (
            counters.storage_bytes_retained,
            counters.storage_inodes_retained
        ),
        (immutable_bytes, immutable_inodes)
    );
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert!(counters.has_zero_forbidden_work());
    assert!(cas.occupied().is_ok());
    assert!(stale.occupied().is_ok());
}

#[test]
fn complete_create_seqcdc_overflow_is_transactional_and_terminal() {
    const LOGICAL_BYTES: usize = 64 * 1024;

    let fixture = TestRoot::new("complete-create-seqcdc-counter-overflow");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let input = (0..LOGICAL_BYTES)
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
    let mut control = ContinueControl;

    let terminal = run_create_with_supplier_and_counters(
        &cas,
        0x0011_5506,
        &mut control,
        CdcAlgorithmV1::SeqCdc,
        LOGICAL_BYTES as u64,
        CheckedSupplier { bytes: &input },
        &mut counters,
    );

    assert_eq!(
        terminal.unwrap_err(),
        OperationErrorV1::Core(CoreError::IntegerOverflow)
    );
    assert_eq!(counters.seqcdc_comparisons, 41);
    assert_eq!(counters.seqcdc_equal_absorptions, 43);
    assert_eq!(counters.seqcdc_opposing_slopes, 47);
    assert_eq!(counters.seqcdc_jumps, 53);
    assert_eq!(counters.seqcdc_jump_bytes, u64::MAX);
    assert!(counters.ring_fills > 0);
    assert!(counters.cdc_scan_calls > 0);
    assert!(counters.cdc_scan_bytes > 0);
    assert!(counters.bytes_boundary_inspected > 0);
    assert!(counters.source_read_calls > 0);
    assert_eq!(counters.source_bytes_read, LOGICAL_BYTES as u64);
    assert_operation_authority_baseline(&cas, fixture.path());
    assert_storage_equations(&counters);
    let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
        exact_operation_namespace_usage(fixture.path());
    assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
    assert_eq!(
        (
            counters.storage_bytes_retained,
            counters.storage_inodes_retained
        ),
        (immutable_bytes, immutable_inodes)
    );
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert!(counters.has_zero_forbidden_work());
    assert!(cas.occupied().is_ok());
    assert!(stale.occupied().is_ok());
}

#[test]
fn complete_create_global_seen_overflow_is_transactional_and_terminal() {
    const LOGICAL_BYTES: u64 = 64 * 1024;

    let fixture = TestRoot::new("complete-create-global-seen-counter-overflow");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let mut counters = OperationCountersV1::default();
    let mut control = GlobalSeenCounterOverflowControl::default();

    let terminal = run_create_with_supplier_and_counters(
        &cas,
        0x0011_5507,
        &mut control,
        CdcAlgorithmV1::FastCdc,
        LOGICAL_BYTES,
        CounterSupplier { len: LOGICAL_BYTES },
        &mut counters,
    );

    assert_eq!(
        terminal.unwrap_err(),
        OperationErrorV1::Core(CoreError::IntegerOverflow)
    );
    assert!(control.injected);
    assert_eq!(counters.global_seen_lookups, 41);
    assert_eq!(counters.global_seen_probes, 43);
    assert_eq!(counters.global_seen_metadata_bytes_read, 47);
    assert_eq!(counters.global_seen_metadata_read_calls, 53);
    assert_eq!(counters.global_seen_metadata_bytes_written, u64::MAX);
    assert_eq!(counters.global_seen_maximum_probe, 59);
    assert_eq!(counters.global_seen_entries, 61);
    assert_eq!(counters.global_seen_table_bytes, 67);
    assert!(counters.source_read_calls > 0);
    assert_eq!(counters.source_bytes_read, LOGICAL_BYTES);
    assert!(counters.cdc_scan_calls > 0);
    assert_operation_authority_baseline(&cas, fixture.path());
    assert_storage_equations(&counters);
    let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
        exact_operation_namespace_usage(fixture.path());
    assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
    assert_eq!(
        (
            counters.storage_bytes_retained,
            counters.storage_inodes_retained
        ),
        (immutable_bytes, immutable_inodes)
    );
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert_eq!(
        counters.unreachable_installed_residue_bytes,
        immutable_bytes
    );
    assert!(counters.has_zero_forbidden_work());
    assert!(cas.occupied().is_ok());
    assert!(stale.occupied().is_ok());
}

#[test]
fn complete_create_operation_spool_write_overflow_retains_typed_cause_and_cleans() {
    const LOGICAL_BYTES: u64 = 64 * 1024;

    let fixture = TestRoot::new("complete-create-operation-spool-write-overflow");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let mut counters = OperationCountersV1::default();
    let mut control = OperationSpoolWriteObservationOverflowControl::default();

    let terminal = run_create_with_supplier_and_counters(
        &cas,
        0x0011_550d,
        &mut control,
        CdcAlgorithmV1::FastCdc,
        LOGICAL_BYTES,
        CounterSupplier { len: LOGICAL_BYTES },
        &mut counters,
    );

    assert_eq!(
        terminal.unwrap_err(),
        OperationErrorV1::FsCas(FsCasErrorV1::Core(CoreError::IntegerOverflow))
    );
    assert!(control.injected);
    assert_operation_authority_baseline(&cas, fixture.path());
    assert_storage_equations(&counters);
    let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
        exact_operation_namespace_usage(fixture.path());
    assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
    assert_eq!(
        (
            counters.storage_bytes_retained,
            counters.storage_inodes_retained
        ),
        (immutable_bytes, immutable_inodes)
    );
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert_eq!(
        counters.unreachable_installed_residue_bytes,
        immutable_bytes
    );
    assert!(counters.has_zero_forbidden_work());
    assert!(cas.occupied().is_ok());
    assert!(stale.occupied().is_ok());
}

#[test]
fn complete_create_operation_spool_read_overflow_is_transactional_and_terminal() {
    const LOGICAL_BYTES: u64 = 64 * 1024;
    const SEEDED_BYTES_READ: u64 = 71;

    let fixture = TestRoot::new("complete-create-operation-spool-read-overflow");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let mut counters = OperationCountersV1::default();
    let mut control = ContinueControl;
    cas.seed_next_operation_spool_read_observation_for_test_v1(SEEDED_BYTES_READ, u64::MAX);

    let terminal = run_create_with_supplier_and_counters(
        &cas,
        0x0011_550e,
        &mut control,
        CdcAlgorithmV1::FastCdc,
        LOGICAL_BYTES,
        CounterSupplier { len: LOGICAL_BYTES },
        &mut counters,
    );

    assert_eq!(
        terminal.unwrap_err(),
        OperationErrorV1::FsCas(FsCasErrorV1::Core(CoreError::IntegerOverflow))
    );
    assert_eq!(
        (
            counters.global_seen_metadata_bytes_read,
            counters.global_seen_metadata_read_calls
        ),
        (SEEDED_BYTES_READ, u64::MAX)
    );
    assert_operation_authority_baseline(&cas, fixture.path());
    assert_storage_equations(&counters);
    let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
        exact_operation_namespace_usage(fixture.path());
    assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
    assert_eq!(
        (
            counters.storage_bytes_retained,
            counters.storage_inodes_retained
        ),
        (immutable_bytes, immutable_inodes)
    );
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert_eq!(
        counters.unreachable_installed_residue_bytes,
        immutable_bytes
    );
    assert!(counters.has_zero_forbidden_work());
    assert!(cas.occupied().is_ok());
    assert!(stale.occupied().is_ok());
}

#[test]
fn complete_create_counted_pack_read_overflow_is_transactional_and_terminal() {
    const LOGICAL_BYTES: u64 = 64 * 1024;

    let fixture = TestRoot::new("complete-create-counted-pack-read-overflow");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let mut counters = OperationCountersV1::default();
    let mut control = CountedPackReadObservationOverflowControl::default();

    let terminal = run_create_with_supplier_and_counters(
        &cas,
        0x0011_550f,
        &mut control,
        CdcAlgorithmV1::FastCdc,
        LOGICAL_BYTES,
        CounterSupplier { len: LOGICAL_BYTES },
        &mut counters,
    );

    assert_eq!(
        terminal.unwrap_err(),
        OperationErrorV1::Core(CoreError::IntegerOverflow)
    );
    assert!(control.injected);
    assert_operation_authority_baseline(&cas, fixture.path());
    assert_storage_equations(&counters);
    let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
        exact_operation_namespace_usage(fixture.path());
    assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
    assert_eq!((immutable_bytes, immutable_inodes), (0, 0));
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert_eq!(counters.storage_bytes_retained, 0);
    assert_eq!(counters.storage_inodes_retained, 0);
    assert_eq!(counters.unreachable_installed_residue_bytes, 0);
    assert!(counters.has_zero_forbidden_work());
    assert!(cas.occupied().is_ok());
    assert!(stale.occupied().is_ok());
}

#[test]
fn complete_create_same_carrier_comparison_overflow_is_transactional_and_terminal() {
    const LOGICAL_BYTES: usize = 64 * 1024;

    let fixture = TestRoot::new("complete-create-same-carrier-comparison-overflow");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let input = vec![0x5a_u8; LOGICAL_BYTES];
    let mut counters = OperationCountersV1::default();
    let mut control = SameCarrierComparisonObservationOverflowControl::default();

    let terminal = run_create_with_supplier_and_counters(
        &cas,
        0x0011_5510,
        &mut control,
        CdcAlgorithmV1::FastCdc,
        LOGICAL_BYTES as u64,
        CheckedSupplier { bytes: &input },
        &mut counters,
    );

    assert_eq!(
        terminal.unwrap_err(),
        OperationErrorV1::Core(CoreError::IntegerOverflow)
    );
    assert!(control.injected);
    assert!(counters.source_read_calls > 0);
    assert_eq!(counters.source_bytes_read, LOGICAL_BYTES as u64);
    assert!(counters.cdc_scan_calls > 0);
    assert_operation_authority_baseline(&cas, fixture.path());
    assert_storage_equations(&counters);
    let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
        exact_operation_namespace_usage(fixture.path());
    assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
    assert_eq!((immutable_bytes, immutable_inodes), (0, 0));
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert_eq!(counters.storage_bytes_retained, 0);
    assert_eq!(counters.storage_inodes_retained, 0);
    assert_eq!(counters.unreachable_installed_residue_bytes, 0);
    assert!(counters.has_zero_forbidden_work());
    assert!(cas.occupied().is_ok());
    assert!(stale.occupied().is_ok());
}

#[test]
fn complete_create_post_admission_tally_overflow_retains_exact_visible_residue() {
    const LOGICAL_BYTES: u64 = 64 * 1024;

    let fixture = TestRoot::new("complete-create-post-admission-tally-overflow");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let mut counters = OperationCountersV1::default();
    let mut control = PostAdmissionCarrierTallyOverflowControl::default();

    let terminal = run_create_with_supplier_and_counters(
        &cas,
        0x0011_5511,
        &mut control,
        CdcAlgorithmV1::FastCdc,
        LOGICAL_BYTES,
        CounterSupplier { len: LOGICAL_BYTES },
        &mut counters,
    );

    assert_eq!(
        terminal.unwrap_err(),
        OperationErrorV1::Core(CoreError::IntegerOverflow)
    );
    assert!(control.injected);
    assert!(counters.source_read_calls > 0);
    assert!(counters.cdc_scan_calls > 0);
    assert_operation_authority_baseline(&cas, fixture.path());
    assert_storage_equations(&counters);
    let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
        exact_operation_namespace_usage(fixture.path());
    assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
    assert!(immutable_bytes > 0);
    assert!(immutable_inodes > 0);
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert_eq!(
        (
            counters.storage_bytes_retained,
            counters.storage_inodes_retained
        ),
        (immutable_bytes, immutable_inodes)
    );
    assert_eq!(
        counters.unreachable_installed_residue_bytes,
        immutable_bytes
    );
    assert!(counters.has_zero_forbidden_work());
    assert!(cas.occupied().is_ok());
    assert!(stale.occupied().is_ok());
}

fn saturated_created_kind_count(counters: &OperationCountersV1) -> usize {
    [
        counters.version_objects_created,
        counters.tree_objects_created,
        counters.file_objects_created,
        counters.symlink_objects_created,
        counters.chunk_objects_created,
    ]
    .into_iter()
    .filter(|value| *value == u64::MAX)
    .count()
}

fn saturated_reused_kind_count(counters: &OperationCountersV1) -> usize {
    [
        counters.version_objects_reused,
        counters.tree_objects_reused,
        counters.file_objects_reused,
        counters.symlink_objects_reused,
        counters.chunk_objects_reused,
    ]
    .into_iter()
    .filter(|value| *value == u64::MAX)
    .count()
}

#[test]
fn complete_create_created_disposition_overflow_is_transactional_and_terminal() {
    let fixture = TestRoot::new("complete-create-created-disposition-overflow");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let mut counters = OperationCountersV1::default();
    let mut control = PackObjectDispositionOverflowControl {
        target_created: true,
        injected: false,
    };

    let terminal = run_small_create_with_supplier_and_counters(
        &cas,
        0x0011_5508,
        &mut control,
        CheckedSupplier { bytes: &[0x5a] },
        &mut counters,
    );

    assert_eq!(
        terminal.unwrap_err(),
        OperationErrorV1::Core(CoreError::IntegerOverflow)
    );
    assert!(control.injected);
    assert_eq!(saturated_created_kind_count(&counters), 1);
    assert_eq!(counters.pack_local_objects_created, 0);
    assert_eq!(counters.physical_carrier_object_writes, 0);
    assert_eq!(counters.pack_local_objects_reused, 0);
    assert_eq!(saturated_reused_kind_count(&counters), 0);
    assert!(counters.source_read_calls > 0);
    assert_eq!(counters.source_bytes_read, 1);
    assert_operation_authority_baseline(&cas, fixture.path());
    assert_storage_equations(&counters);
    let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
        exact_operation_namespace_usage(fixture.path());
    assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
    assert_eq!(
        (
            counters.storage_bytes_retained,
            counters.storage_inodes_retained
        ),
        (immutable_bytes, immutable_inodes)
    );
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert!(counters.has_zero_forbidden_work());
    assert!(cas.occupied().is_ok());
    assert!(stale.occupied().is_ok());
}

#[test]
fn complete_tree_reused_disposition_overflow_is_transactional_and_terminal() {
    let fixture = TestRoot::new("complete-tree-reused-disposition-overflow");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let payload = [0x6b_u8; 1];
    let mut files = [
        TreeFileV1::new(
            b"a.bin",
            0o644,
            payload.len() as u64,
            CheckedSupplier { bytes: &payload },
        ),
        TreeFileV1::new(
            b"b.bin",
            0o644,
            payload.len() as u64,
            CheckedSupplier { bytes: &payload },
        ),
    ];
    let mut source_window = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
    let mut cdc_ring = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
    let mut incoming = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut occupied = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut tree_object = boxed_zeroes::<MAX_TREE_OBJECT_BYTES>();
    let mut tree_pages = boxed_tree_pages();
    let mut traversal = [0_u8; 64];
    let mut counters = OperationCountersV1::default();
    let mut control = PackObjectDispositionOverflowControl {
        target_created: false,
        injected: false,
    };
    let operation =
        request_tree_operation_v1(&cas, 0x0011_5509, &mut counters, &mut control).unwrap();

    let terminal = run_create_tree_v1(
        operation,
        CdcAlgorithmV1::FastCdc,
        &mut files,
        OperationBuffersV1 {
            source: &mut source_window,
            cdc_ring: &mut cdc_ring,
            incoming_comparison: &mut incoming,
            occupied_comparison: &mut occupied,
            tree_object: &mut tree_object,
            tree_pages: &mut *tree_pages,
            traversal_state: &mut traversal,
        },
        &mut control,
        &mut counters,
    );

    assert_eq!(
        terminal.unwrap_err(),
        OperationErrorV1::Core(CoreError::IntegerOverflow)
    );
    assert!(control.injected);
    assert_eq!(saturated_reused_kind_count(&counters), 1);
    assert_eq!(counters.pack_local_objects_reused, 0);
    assert_eq!(
        counters.physical_carrier_object_writes,
        counters.pack_local_objects_created
    );
    assert!(counters.pack_local_objects_created > 0);
    assert!(counters.source_read_calls >= 2);
    assert_eq!(counters.source_bytes_read, 2);
    assert_operation_authority_baseline(&cas, fixture.path());
    assert_storage_equations(&counters);
    let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
        exact_operation_namespace_usage(fixture.path());
    assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
    assert_eq!(
        (
            counters.storage_bytes_retained,
            counters.storage_inodes_retained
        ),
        (immutable_bytes, immutable_inodes)
    );
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert!(counters.has_zero_forbidden_work());
    assert!(cas.occupied().is_ok());
    assert!(stale.occupied().is_ok());
}

#[test]
fn preparation_free_unwind_returns_typed_terminal_only_when_terminalization_fails() {
    let clean_fixture = TestRoot::new("preparation-free-clean-unwind");
    let clean_cas = FsCasV1::create_new(clean_fixture.path()).unwrap();
    let clean_bound_invoked = AtomicBool::new(false);
    let clean_supply_invoked = AtomicBool::new(false);
    let mut clean_control = ContinueControl;
    let clean_unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = run_small_create_with_supplier(
            &clean_cas,
            0x8fd,
            &mut clean_control,
            PanicDuringPreparationFreeStageSupplier {
                cas_to_poison: None,
                bound_invoked: &clean_bound_invoked,
                supply_invoked: &clean_supply_invoked,
            },
        );
    }));
    assert!(clean_unwind.is_err());
    assert!(clean_bound_invoked.load(Ordering::Acquire));
    assert!(!clean_supply_invoked.load(Ordering::Acquire));
    assert_operation_authority_baseline(&clean_cas, clean_fixture.path());
    let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
        exact_operation_namespace_usage(clean_fixture.path());
    assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
    assert_eq!((immutable_bytes, immutable_inodes), (0, 0));
    assert!(clean_cas.occupied().is_ok());

    // A clean terminal must resume the initiating payload without damaging
    // the root. A subsequent complete operation proves that no latent queue,
    // storage-admission, or owner state was left behind.
    let followup_bound = AtomicBool::new(false);
    let followup_supply = AtomicBool::new(false);
    let (followup, followup_counters) = run_small_create_with_callback_observation(
        &clean_cas,
        0x8fe,
        &mut clean_control,
        &followup_bound,
        &followup_supply,
    );
    assert!(followup.is_ok());
    assert!(followup_bound.load(Ordering::Acquire));
    assert!(followup_supply.load(Ordering::Acquire));
    assert_operation_authority_baseline(&clean_cas, clean_fixture.path());
    assert_storage_equations(&followup_counters);
    assert!(followup_counters.has_zero_forbidden_work());

    for (case, fail_invalidation, expected) in [
        (
            "preparation-free-storage-poison",
            false,
            FsCasErrorV1::SynchronizationPoisoned,
        ),
        (
            "preparation-free-invalidation-double-fault",
            true,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::SynchronizationPoisoned,
                dominant: FsCasFailureCauseV1::InvalidationFailed,
            },
        ),
    ] {
        let fixture = TestRoot::new(case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let bound_invoked = AtomicBool::new(false);
        let supply_invoked = AtomicBool::new(false);
        let mut control = PreparationFreeTerminalControlV1 {
            fail_invalidation,
            invalidation_attempts: 0,
        };

        let (result, counters) = run_small_create_with_supplier(
            &cas,
            0x8ff + u64::from(fail_invalidation),
            &mut control,
            PanicDuringPreparationFreeStageSupplier {
                cas_to_poison: Some(cas.clone()),
                bound_invoked: &bound_invoked,
                supply_invoked: &supply_invoked,
            },
        );

        assert_eq!(result, Err(OperationErrorV1::FsCas(expected)), "{case}");
        assert!(bound_invoked.load(Ordering::Acquire), "{case}");
        assert!(!supply_invoked.load(Ordering::Acquire), "{case}");
        assert_eq!(control.invalidation_attempts, 1, "{case}");
        assert_operation_authority_baseline(&cas, fixture.path());
        assert_storage_equations(&counters);
        let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
            exact_operation_namespace_usage(fixture.path());
        assert_eq!((preparation_bytes, preparation_inodes), (0, 0), "{case}");
        assert_eq!((immutable_bytes, immutable_inodes), (0, 0), "{case}");
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert_eq!(counters.storage_bytes_retained, 0, "{case}");
        assert_eq!(counters.storage_inodes_retained, 0, "{case}");
        assert_eq!(counters.unreachable_installed_residue_bytes, 0, "{case}");
        assert_eq!(counters.mutable_preparation_residue_bytes, 0, "{case}");
        assert_eq!(counters.mutable_preparation_residue_inodes, 0, "{case}");
        assert!(counters.has_zero_forbidden_work(), "{case}");
        assert!(
            matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        assert!(
            matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        match FsCasV1::open_existing(fixture.path()) {
            Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
            Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
            Ok(_) => panic!("{case}: fail-closed root reopened as usable"),
        }
    }
}

#[test]
fn typed_preparation_free_error_survives_operation_terminal_unwind() {
    let fixture = TestRoot::new("typed-preparation-free-terminal-unwind");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let bound_invoked = AtomicBool::new(false);
    let supply_invoked = AtomicBool::new(false);
    let mut control = PanicAfterOperationTerminalReleaseV1 {
        unwind_pending: true,
        terminal_hook_calls: 0,
    };

    let (result, counters) = run_small_create_with_supplier(
        &cas,
        0x0011_5520,
        &mut control,
        FailingPreparationFreeStageSupplier {
            bound_invoked: &bound_invoked,
            supply_invoked: &supply_invoked,
        },
    );

    assert_eq!(
        result,
        Err(OperationErrorV1::Core(CoreError::ResourceRefused))
    );
    assert!(bound_invoked.load(Ordering::Acquire));
    assert!(!supply_invoked.load(Ordering::Acquire));
    assert_eq!(control.terminal_hook_calls, 1);
    assert_operation_authority_baseline(&cas, fixture.path());
    assert!(cas.visibility_lock_available_for_test_v1());
    assert!(cas.publication_lock_available_for_test_v1());
    assert_storage_equations(&counters);
    assert_eq!(counters.root_admission_queue_entries, 1);
    assert_eq!(counters.root_admission_queue_refusals, 0);
    assert_eq!(counters.root_admission_release_failures, 0);
    assert!(counters.storage_bytes_requested > 0);
    assert!(counters.storage_inodes_requested > 0);
    assert_eq!(
        counters.storage_bytes_released,
        counters.storage_bytes_requested
    );
    assert_eq!(
        counters.storage_inodes_released,
        counters.storage_inodes_requested
    );
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert_eq!(counters.storage_bytes_retained, 0);
    assert_eq!(counters.storage_inodes_retained, 0);
    assert_eq!(counters.mutable_preparation_residue_bytes, 0);
    assert_eq!(counters.mutable_preparation_residue_inodes, 0);
    assert_eq!(counters.unreachable_installed_residue_bytes, 0);
    assert!(counters.has_zero_forbidden_work());
    assert_eq!(
        exact_operation_namespace_usage(fixture.path()),
        ((0, 0), (0, 0))
    );
    assert!(cas.occupied().is_ok());
    assert!(stale.occupied().is_ok());
    let reopened = FsCasV1::open_existing(fixture.path()).unwrap();
    assert!(reopened.occupied().is_ok());
}

#[test]
fn typed_complete_body_error_survives_operation_terminal_unwind() {
    let fixture = TestRoot::new("typed-complete-body-terminal-unwind");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let mut control = PanicAfterOperationTerminalReleaseV1 {
        unwind_pending: true,
        terminal_hook_calls: 0,
    };

    let (result, counters) =
        run_small_create_with_supplier(&cas, 0x0011_5521, &mut control, FailingBodySupplier);

    assert_eq!(
        result,
        Err(OperationErrorV1::Core(CoreError::SourceFailure))
    );
    assert_eq!(control.terminal_hook_calls, 1);
    assert_operation_authority_baseline(&cas, fixture.path());
    assert!(cas.visibility_lock_available_for_test_v1());
    assert!(cas.publication_lock_available_for_test_v1());
    assert_storage_equations(&counters);
    assert_eq!(counters.root_admission_queue_entries, 1);
    assert_eq!(counters.root_admission_queue_refusals, 0);
    assert_eq!(counters.root_admission_release_failures, 0);
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert_eq!(counters.storage_bytes_retained, 0);
    assert_eq!(counters.storage_inodes_retained, 0);
    assert_eq!(counters.mutable_preparation_residue_bytes, 0);
    assert_eq!(counters.mutable_preparation_residue_inodes, 0);
    assert_eq!(counters.unreachable_installed_residue_bytes, 0);
    assert!(counters.has_zero_forbidden_work());
    assert_eq!(
        exact_operation_namespace_usage(fixture.path()),
        ((0, 0), (0, 0))
    );
    assert!(cas.occupied().is_ok());
    assert!(stale.occupied().is_ok());
    let reopened = FsCasV1::open_existing(fixture.path()).unwrap();
    assert!(reopened.occupied().is_ok());
}

#[test]
fn typed_complete_body_error_survives_later_global_seen_observation_failure() {
    const BODY_BYTES: u64 = 64 * 1024;

    let fixture = TestRoot::new("typed-complete-body-global-seen-observation");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let mut control = GlobalSeenCounterOverflowControl::default();

    let mut counters = OperationCountersV1::default();
    let result = run_create_with_supplier_and_counters(
        &cas,
        0x0011_5524,
        &mut control,
        CdcAlgorithmV1::FastCdc,
        BODY_BYTES + 1,
        FailingAfterBytesSupplier {
            bytes_before_failure: BODY_BYTES,
        },
        &mut counters,
    );

    assert_eq!(
        result,
        Err(OperationErrorV1::Core(CoreError::SourceFailure))
    );
    assert!(control.injected);
    assert_operation_authority_baseline(&cas, fixture.path());
    assert!(cas.visibility_lock_available_for_test_v1());
    assert!(cas.publication_lock_available_for_test_v1());
    assert_storage_equations(&counters);
    assert_eq!(counters.global_seen_lookups, 41);
    assert_eq!(counters.global_seen_probes, 43);
    assert_eq!(counters.global_seen_metadata_bytes_read, 47);
    assert_eq!(counters.global_seen_metadata_read_calls, 53);
    assert_eq!(counters.global_seen_metadata_bytes_written, u64::MAX);
    assert_eq!(counters.global_seen_maximum_probe, 59);
    assert_eq!(counters.global_seen_entries, 61);
    assert_eq!(counters.global_seen_table_bytes, 67);
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert_eq!(counters.storage_bytes_retained, 0);
    assert_eq!(counters.storage_inodes_retained, 0);
    assert_eq!(counters.mutable_preparation_residue_bytes, 0);
    assert_eq!(counters.mutable_preparation_residue_inodes, 0);
    assert_eq!(counters.unreachable_installed_residue_bytes, 0);
    assert!(counters.has_zero_forbidden_work());
    assert_eq!(
        exact_operation_namespace_usage(fixture.path()),
        ((0, 0), (0, 0))
    );
    assert!(cas.occupied().is_ok());
    assert!(stale.occupied().is_ok());
    let reopened = FsCasV1::open_existing(fixture.path()).unwrap();
    assert!(reopened.occupied().is_ok());
}

#[test]
fn typed_complete_body_error_survives_later_storage_counter_merge_failure() {
    const BODY_BYTES: u64 = 64 * 1024;

    let fixture = TestRoot::new("typed-complete-body-storage-counter-merge");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let mut counters = OperationCountersV1 {
        global_seen_metadata_bytes_written: u64::MAX,
        ..OperationCountersV1::default()
    };
    let mut control = ContinueControl;

    let result = run_create_with_supplier_and_counters(
        &cas,
        0x0011_5525,
        &mut control,
        CdcAlgorithmV1::FastCdc,
        BODY_BYTES + 1,
        FailingAfterBytesSupplier {
            bytes_before_failure: BODY_BYTES,
        },
        &mut counters,
    );

    assert_eq!(
        result,
        Err(OperationErrorV1::Core(CoreError::SourceFailure))
    );
    assert_operation_authority_baseline(&cas, fixture.path());
    assert!(cas.visibility_lock_available_for_test_v1());
    assert!(cas.publication_lock_available_for_test_v1());
    assert_storage_equations(&counters);
    assert_eq!(counters.global_seen_metadata_bytes_written, u64::MAX);
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert_eq!(counters.storage_bytes_retained, 0);
    assert_eq!(counters.storage_inodes_retained, 0);
    assert_eq!(counters.mutable_preparation_residue_bytes, 0);
    assert_eq!(counters.mutable_preparation_residue_inodes, 0);
    assert_eq!(counters.unreachable_installed_residue_bytes, 0);
    assert!(counters.has_zero_forbidden_work());
    assert_eq!(
        exact_operation_namespace_usage(fixture.path()),
        ((0, 0), (0, 0))
    );
    assert!(cas.occupied().is_ok());
    assert!(stale.occupied().is_ok());
    let reopened = FsCasV1::open_existing(fixture.path()).unwrap();
    assert!(reopened.occupied().is_ok());
}

#[test]
fn typed_body_error_crosses_cleanup_and_invalidation_dominance_exactly() {
    for fail_invalidation in [false, true] {
        let label = if fail_invalidation {
            "typed-body-cleanup-invalidation-dominance"
        } else {
            "typed-body-cleanup-dominance"
        };
        let fixture = TestRoot::new(label);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let mut control = FailBodyCleanupTerminalV1 {
            preparation_cleanup_injected: false,
            fail_invalidation,
            invalidation_attempts: 0,
        };

        let (result, counters) = run_small_create_with_supplier(
            &cas,
            0x0011_5522 + u64::from(fail_invalidation),
            &mut control,
            FailingBodySupplier,
        );

        assert_eq!(
            result,
            Err(OperationErrorV1::FsCas(FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::Core(CoreError::SourceFailure),
                dominant: if fail_invalidation {
                    FsCasFailureCauseV1::InvalidationFailed
                } else {
                    FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool)
                },
            })),
            "{label}",
        );
        assert!(control.preparation_cleanup_injected, "{label}");
        assert_eq!(control.invalidation_attempts, 1, "{label}");
        assert_eq!(cas.operation_admitted_slots_v1(), 0, "{label}");
        assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{label}");
        assert_eq!(
            cas.storage_admission_active_for_test_v1(),
            (0, 0, 0),
            "{label}",
        );
        assert!(cas.visibility_lock_available_for_test_v1(), "{label}");
        assert!(cas.publication_lock_available_for_test_v1(), "{label}");
        assert_storage_equations(&counters);
        assert_eq!(counters.root_admission_queue_entries, 1, "{label}");
        assert_eq!(counters.root_admission_queue_refusals, 0, "{label}");
        assert_eq!(counters.root_admission_release_failures, 0, "{label}");
        assert_eq!(counters.storage_bytes_committed, 0, "{label}");
        assert_eq!(counters.storage_inodes_committed, 0, "{label}");
        assert_eq!(counters.unreachable_installed_residue_bytes, 0, "{label}");
        let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
            exact_operation_namespace_usage(fixture.path());
        assert!(preparation_bytes > 0, "{label}");
        assert_eq!(preparation_inodes, 1, "{label}");
        assert_eq!((immutable_bytes, immutable_inodes), (0, 0), "{label}");
        assert_eq!(
            counters.storage_bytes_retained, preparation_bytes,
            "{label}",
        );
        assert_eq!(
            counters.storage_inodes_retained, preparation_inodes,
            "{label}",
        );
        assert_eq!(
            counters.mutable_preparation_residue_bytes, preparation_bytes,
            "{label}",
        );
        assert_eq!(
            counters.mutable_preparation_residue_inodes, preparation_inodes,
            "{label}",
        );
        assert_eq!(counters.immutable_residue_bytes, 0, "{label}");
        assert_eq!(counters.immutable_residue_inodes, 0, "{label}");
        assert!(counters.has_zero_forbidden_work(), "{label}");
        assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
        assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
        match FsCasV1::open_existing(fixture.path()) {
            Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
            Err(error) => panic!("{label}: unexpected reopen error {error:?}"),
            Ok(_) => panic!("{label}: fail-closed root reopened as usable"),
        }
    }
}

#[test]
fn filesystem_capacity_and_io_failures_are_typed_and_leave_no_unpublished_state() {
    let cases = [
        (
            FsCasFilesystemBoundaryV1::PreparationCreate,
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::NoSpace),
            true,
            "preparation-create-enospc",
        ),
        (
            FsCasFilesystemBoundaryV1::PreparationResize,
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::Quota),
            true,
            "preparation-resize-edquot",
        ),
        (
            FsCasFilesystemBoundaryV1::PermissionChange,
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
            true,
            "preparation-permission",
        ),
        (
            FsCasFilesystemBoundaryV1::PreparationWrite,
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ShortWrite),
            false,
            "preparation-short-write",
        ),
        (
            FsCasFilesystemBoundaryV1::PrivatePackCreate,
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::InodeExhaustion),
            false,
            "pack-create-inodes",
        ),
        (
            FsCasFilesystemBoundaryV1::PrivatePackWrite,
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::ShortWrite),
            false,
            "pack-short-write",
        ),
        (
            FsCasFilesystemBoundaryV1::PrivatePackFlush,
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure),
            false,
            "pack-flush-eio",
        ),
        (
            FsCasFilesystemBoundaryV1::CarrierHardLink,
            FsCasErrorV1::Unsupported,
            false,
            "carrier-link-unsupported",
        ),
        (
            FsCasFilesystemBoundaryV1::MarkerCreate,
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::InodeExhaustion),
            false,
            "marker-create-inodes",
        ),
        (
            FsCasFilesystemBoundaryV1::MarkerWrite,
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::NoSpace),
            false,
            "marker-write-enospc",
        ),
        (
            FsCasFilesystemBoundaryV1::MarkerFlush,
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure),
            false,
            "marker-flush-eio",
        ),
        (
            FsCasFilesystemBoundaryV1::MarkerHardLink,
            FsCasErrorV1::Unsupported,
            false,
            "marker-link-unsupported",
        ),
    ];

    for (index, (boundary, expected, before_supply, label)) in cases.into_iter().enumerate() {
        let fixture = TestRoot::new(label);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let bound_invoked = AtomicBool::new(false);
        let supply_invoked = AtomicBool::new(false);
        let mut control = FailFilesystemBoundaryOnceV1 {
            boundary,
            error: expected,
            fired: false,
        };
        let (result, counters) = run_small_create_with_callback_observation(
            &cas,
            0x800 + index as u64,
            &mut control,
            &bound_invoked,
            &supply_invoked,
        );

        assert_eq!(
            result,
            Err(OperationErrorV1::FsCas(expected)),
            "typed failure at {boundary:?}",
        );
        assert!(control.fired, "unreached filesystem boundary {boundary:?}");
        if before_supply {
            // The side-effect-free resident bound is required to form the
            // already-granted memory plan. The supplier itself and all source
            // reads remain untouched when opening/preallocating preparation
            // fails. Predictable root storage admission refusal is separately
            // proven above to occur before even this bound callback.
            assert!(bound_invoked.load(Ordering::Acquire), "{boundary:?}");
            assert!(!supply_invoked.load(Ordering::Acquire), "{boundary:?}");
            assert_eq!(counters.source_read_calls, 0, "{boundary:?}");
        }
        assert_operation_authority_baseline(&cas, fixture.path());
        for directory in ["carriers", "objects", "catalog", "closures"] {
            assert_eq!(
                fs::read_dir(fixture.path().join(directory))
                    .unwrap()
                    .count(),
                0,
                "unpublished immutable residue in {directory} at {boundary:?}",
            );
        }
        assert_storage_equations(&counters);
        assert_eq!(counters.storage_bytes_committed, 0, "{boundary:?}");
        assert_eq!(counters.storage_inodes_committed, 0, "{boundary:?}");
        assert_eq!(counters.storage_bytes_retained, 0, "{boundary:?}");
        assert_eq!(counters.storage_inodes_retained, 0, "{boundary:?}");
        assert!(cas.occupied().is_ok(), "root invalidated at {boundary:?}");
        assert!(
            stale.occupied().is_ok(),
            "stale alias invalidated at {boundary:?}"
        );
        assert!(counters.has_zero_forbidden_work(), "{boundary:?}");
    }
}

#[test]
fn carrier_link_failure_preserves_first_cause_when_charge_unwind_fails() {
    for (case, link_error, first, fail_invalidation, dominant) in [
        (
            "carrier-link-unsupported-accounting-poison",
            FsCasErrorV1::Unsupported,
            FsCasFailureCauseV1::Unsupported,
            false,
            FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::Carrier),
        ),
        (
            "carrier-link-unsupported-invalidation-double-fault",
            FsCasErrorV1::Unsupported,
            FsCasFailureCauseV1::Unsupported,
            true,
            FsCasFailureCauseV1::InvalidationFailed,
        ),
        (
            "carrier-link-write-accounting-poison",
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure),
            FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure),
            false,
            FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::Carrier),
        ),
        (
            "carrier-link-write-invalidation-double-fault",
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure),
            FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure),
            true,
            FsCasFailureCauseV1::InvalidationFailed,
        ),
    ] {
        let fixture = TestRoot::new(case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let bound_invoked = AtomicBool::new(false);
        let supply_invoked = AtomicBool::new(false);
        let mut control = PoisonStorageAtCarrierLinkV1 {
            cas: cas.clone(),
            link_error,
            fired: false,
            fail_invalidation,
        };
        let (result, counters) = run_small_create_with_callback_observation(
            &cas,
            0x820,
            &mut control,
            &bound_invoked,
            &supply_invoked,
        );

        assert_eq!(
            result,
            Err(OperationErrorV1::FsCas(FsCasErrorV1::TerminalFailure {
                first,
                dominant,
            })),
            "{case}"
        );
        assert!(control.fired, "{case}");
        assert!(bound_invoked.load(Ordering::Acquire), "{case}");
        assert!(supply_invoked.load(Ordering::Acquire), "{case}");
        assert_operation_authority_baseline(&cas, fixture.path());
        assert_storage_equations(&counters);
        let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
            exact_operation_namespace_usage(fixture.path());
        assert_eq!((preparation_bytes, preparation_inodes), (0, 0), "{case}");
        assert_eq!((immutable_bytes, immutable_inodes), (0, 0), "{case}");
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert_eq!(counters.storage_bytes_retained, 0, "{case}");
        assert_eq!(counters.storage_inodes_retained, 0, "{case}");
        assert_eq!(counters.unreachable_installed_residue_bytes, 0, "{case}");
        assert!(counters.has_zero_forbidden_work(), "{case}");
        assert!(
            matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        assert!(
            matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        match FsCasV1::open_existing(fixture.path()) {
            Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
            Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
            Ok(_) => panic!("{case}: failed root reopened as usable"),
        }
    }
}

#[test]
fn actual_carrier_already_exists_stops_when_charge_unwind_fails() {
    for (case, fail_invalidation, dominant) in [
        (
            "carrier-already-exists-accounting-poison",
            false,
            FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::Carrier),
        ),
        (
            "carrier-already-exists-invalidation-double-fault",
            true,
            FsCasFailureCauseV1::InvalidationFailed,
        ),
    ] {
        let fixture = TestRoot::new(case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let bound_invoked = AtomicBool::new(false);
        let supply_invoked = AtomicBool::new(false);
        let mut control = InstallCarrierAndPoisonStorageBeforeLinkV1 {
            cas: cas.clone(),
            installed: None,
            fail_invalidation,
        };
        let (result, counters) = run_small_create_with_callback_observation(
            &cas,
            0x821,
            &mut control,
            &bound_invoked,
            &supply_invoked,
        );

        assert_eq!(
            result,
            Err(OperationErrorV1::FsCas(FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::SynchronizationPoisoned,
                dominant,
            })),
            "{case}"
        );
        let carrier = control
            .installed
            .as_ref()
            .expect("the independent carrier must have been installed");
        assert!(fs::symlink_metadata(carrier).unwrap().is_file(), "{case}");
        assert!(bound_invoked.load(Ordering::Acquire), "{case}");
        assert!(supply_invoked.load(Ordering::Acquire), "{case}");
        assert_operation_authority_baseline(&cas, fixture.path());
        assert_storage_equations(&counters);
        let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
            exact_operation_namespace_usage(fixture.path());
        let (carrier_bytes, carrier_inodes) =
            exact_directory_usage(&fixture.path().join("carriers"));
        assert_eq!((preparation_bytes, preparation_inodes), (0, 0), "{case}");
        assert_eq!(
            (immutable_bytes, immutable_inodes),
            (carrier_bytes, 1),
            "{case}"
        );
        assert_eq!(carrier_inodes, 1, "{case}");
        for directory in ["objects", "catalog", "closures"] {
            assert_eq!(
                fs::read_dir(fixture.path().join(directory))
                    .unwrap()
                    .count(),
                0,
                "incumbent continuation published {directory} in {case}"
            );
        }
        assert_eq!(counters.fscas_catalog_operations, 0, "{case}");
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert_eq!(counters.storage_bytes_retained, 0, "{case}");
        assert_eq!(counters.storage_inodes_retained, 0, "{case}");
        assert_eq!(counters.unreachable_installed_residue_bytes, 0, "{case}");
        assert!(counters.has_zero_forbidden_work(), "{case}");
        assert!(
            matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        assert!(
            matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        match FsCasV1::open_existing(fixture.path()) {
            Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
            Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
            Ok(_) => panic!("{case}: failed root reopened as usable"),
        }
    }
}

#[test]
fn preparation_construction_preserves_first_failure_when_cleanup_dominates() {
    let fixture = TestRoot::new("preparation-create-cleanup-dual-cause");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let bound_invoked = AtomicBool::new(false);
    let supply_invoked = AtomicBool::new(false);
    let mut control = FailPreparationCreateAndCleanupV1::default();
    let (result, counters) = run_small_create_with_callback_observation(
        &cas,
        0x8ff,
        &mut control,
        &bound_invoked,
        &supply_invoked,
    );

    assert_eq!(
        result,
        Err(OperationErrorV1::FsCas(FsCasErrorV1::TerminalFailure {
            first: FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::NoSpace),
            dominant: FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool,),
        }))
    );
    assert!(control.create_failed);
    assert!(control.cleanup_failed);
    assert!(bound_invoked.load(Ordering::Acquire));
    assert!(!supply_invoked.load(Ordering::Acquire));
    assert_eq!(counters.source_read_calls, 0);
    assert_storage_equations(&counters);
    let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
        exact_operation_namespace_usage(fixture.path());
    // The injected second-spool failure is sampled at the real create point,
    // after its namespace charge has been removed exactly. The later injected
    // cleanup failure therefore applies to the already-created first spool and
    // retains its directly observed empty inode—never a fabricated byte value.
    assert_eq!((preparation_bytes, preparation_inodes), (0, 1));
    assert_eq!((immutable_bytes, immutable_inodes), (0, 0));
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
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
    assert!(counters.has_zero_forbidden_work());
    assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(
        FsCasV1::open_existing(fixture.path()),
        Err(FsCasErrorV1::Invalidated)
    ));
}

#[test]
fn partial_preparation_cleanup_unwind_preserves_directional_first_cause_and_dominance() {
    for (first_error, first_cause, first_label) in [
        (
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
            FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
            "permission",
        ),
        (
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure),
            FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure),
            "write",
        ),
    ] {
        for fail_invalidation in [false, true] {
            let label = format!(
                "partial-preparation-{first_label}-cleanup-unwind-{}",
                if fail_invalidation {
                    "double-fault"
                } else {
                    "cleanup"
                }
            );
            let fixture = TestRoot::new(&label);
            let cas = FsCasV1::create_new(fixture.path()).unwrap();
            let stale = FsCasV1::open_existing(fixture.path()).unwrap();
            let bound_invoked = AtomicBool::new(false);
            let supply_invoked = AtomicBool::new(false);
            let mut control = FailPreparationPermissionAndPanicCleanupV1 {
                first_error,
                permission_failed: false,
                preparation_cleanup_calls: 0,
                cleanup_panicked: false,
                fail_invalidation,
                root_invalidation_callbacks: 0,
            };

            let (result, counters) = run_small_create_with_callback_observation(
                &cas,
                0x8f0,
                &mut control,
                &bound_invoked,
                &supply_invoked,
            );
            let dominant = if fail_invalidation {
                FsCasFailureCauseV1::InvalidationFailed
            } else {
                FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool)
            };
            assert_eq!(
                result,
                Err(OperationErrorV1::FsCas(FsCasErrorV1::TerminalFailure {
                    first: first_cause,
                    dominant,
                })),
                "{label}",
            );
            assert!(control.permission_failed, "{label}");
            assert!(control.cleanup_panicked, "{label}");
            assert_eq!(control.preparation_cleanup_calls, 1, "{label}");
            assert_eq!(control.root_invalidation_callbacks, 1, "{label}");
            assert!(bound_invoked.load(Ordering::Acquire), "{label}");
            assert!(!supply_invoked.load(Ordering::Acquire), "{label}");
            assert_eq!(counters.source_read_calls, 0, "{label}");
            assert_eq!(cas.operation_admitted_slots_v1(), 0, "{label}");
            assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{label}");
            assert_eq!(
                cas.storage_admission_active_for_test_v1(),
                (0, 0, 0),
                "{label}",
            );
            assert_storage_equations(&counters);
            let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
                exact_operation_namespace_usage(fixture.path());
            assert_eq!((preparation_bytes, preparation_inodes), (0, 1), "{label}");
            assert_eq!((immutable_bytes, immutable_inodes), (0, 0), "{label}");
            assert_eq!(counters.storage_bytes_committed, 0, "{label}");
            assert_eq!(counters.storage_inodes_committed, 0, "{label}");
            assert_eq!(
                counters.storage_bytes_retained, preparation_bytes,
                "{label}"
            );
            assert_eq!(
                counters.storage_inodes_retained, preparation_inodes,
                "{label}",
            );
            assert_eq!(
                counters.mutable_preparation_residue_bytes, preparation_bytes,
                "{label}",
            );
            assert_eq!(
                counters.mutable_preparation_residue_inodes, preparation_inodes,
                "{label}",
            );
            assert_eq!(counters.unreachable_installed_residue_bytes, 0, "{label}");
            assert!(counters.has_zero_forbidden_work(), "{label}");
            assert!(cas.visibility_lock_available_for_test_v1(), "{label}");
            assert!(cas.publication_lock_available_for_test_v1(), "{label}");
            assert!(
                matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
                "{label}",
            );
            assert!(
                matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
                "{label}",
            );
            match FsCasV1::open_existing(fixture.path()) {
                Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
                Err(error) => panic!("{label}: unexpected reopen error {error:?}"),
                Ok(_) => panic!("{label}: damaged root reopened as usable"),
            }
        }
    }
}

#[test]
fn preparation_construction_unwind_returns_cleanup_terminal_only_after_owned_cleanup() {
    for mode in [
        PreparationConstructionUnwindModeV1::CleanupFails,
        PreparationConstructionUnwindModeV1::CleanupUnwinds,
        PreparationConstructionUnwindModeV1::PreCreateAccountingReleaseFails,
    ] {
        for fail_invalidation in [false, true] {
            let label = format!(
                "preparation-construction-{mode:?}-{}",
                if fail_invalidation {
                    "double-fault"
                } else {
                    "cleanup"
                }
            );
            let fixture = TestRoot::new(&label);
            let cas = FsCasV1::create_new(fixture.path()).unwrap();
            let stale = FsCasV1::open_existing(fixture.path()).unwrap();
            let bound_invoked = AtomicBool::new(false);
            let supply_invoked = AtomicBool::new(false);
            let mut control = PanicPreparationConstructionWithCleanupFailureV1 {
                cas: cas.clone(),
                mode,
                construction_panicked: false,
                preparation_cleanup_calls: 0,
                fail_invalidation,
                root_invalidation_callbacks: 0,
            };

            let (result, counters) = run_small_create_with_callback_observation(
                &cas,
                0x8ef,
                &mut control,
                &bound_invoked,
                &supply_invoked,
            );
            let cleanup =
                FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool);
            let dominant = if fail_invalidation {
                FsCasFailureCauseV1::InvalidationFailed
            } else {
                cleanup
            };
            let expected = match mode {
                PreparationConstructionUnwindModeV1::CleanupFails
                | PreparationConstructionUnwindModeV1::CleanupUnwinds => {
                    if fail_invalidation {
                        FsCasErrorV1::TerminalFailure {
                            first: cleanup,
                            dominant,
                        }
                    } else {
                        FsCasErrorV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool)
                    }
                }
                PreparationConstructionUnwindModeV1::PreCreateAccountingReleaseFails => {
                    FsCasErrorV1::TerminalFailure {
                        first: FsCasFailureCauseV1::Integrity,
                        dominant,
                    }
                }
            };
            assert_eq!(result, Err(OperationErrorV1::FsCas(expected)), "{label}",);
            assert!(control.construction_panicked, "{label}");
            assert_eq!(
                control.preparation_cleanup_calls,
                usize::from(
                    mode != PreparationConstructionUnwindModeV1::PreCreateAccountingReleaseFails
                ),
                "{label}",
            );
            assert_eq!(control.root_invalidation_callbacks, 1, "{label}");
            assert!(bound_invoked.load(Ordering::Acquire), "{label}");
            assert!(!supply_invoked.load(Ordering::Acquire), "{label}");
            assert_eq!(counters.source_read_calls, 0, "{label}");
            assert_eq!(cas.operation_admitted_slots_v1(), 0, "{label}");
            assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{label}");
            assert_eq!(
                cas.storage_admission_active_for_test_v1(),
                (0, 0, 0),
                "{label}",
            );
            assert_storage_equations(&counters);
            let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
                exact_operation_namespace_usage(fixture.path());
            let expected_physical_inodes = usize::from(
                mode != PreparationConstructionUnwindModeV1::PreCreateAccountingReleaseFails,
            ) as u64;
            assert_eq!(preparation_bytes, 0, "{label}");
            assert_eq!(preparation_inodes, expected_physical_inodes, "{label}");
            assert_eq!((immutable_bytes, immutable_inodes), (0, 0), "{label}");
            assert_eq!(counters.storage_bytes_committed, 0, "{label}");
            assert_eq!(counters.storage_inodes_committed, 0, "{label}");
            assert_eq!(counters.storage_bytes_retained, 0, "{label}");
            assert_eq!(counters.storage_inodes_retained, 1, "{label}");
            assert_eq!(counters.mutable_preparation_residue_bytes, 0, "{label}");
            assert_eq!(counters.mutable_preparation_residue_inodes, 1, "{label}");
            assert_eq!(counters.unreachable_installed_residue_bytes, 0, "{label}");
            assert!(counters.has_zero_forbidden_work(), "{label}");
            assert!(cas.visibility_lock_available_for_test_v1(), "{label}");
            assert!(cas.publication_lock_available_for_test_v1(), "{label}");
            assert!(
                matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
                "{label}",
            );
            assert!(
                matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
                "{label}",
            );
            match FsCasV1::open_existing(fixture.path()) {
                Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
                Err(error) => panic!("{label}: unexpected reopen error {error:?}"),
                Ok(_) => panic!("{label}: damaged root reopened as usable"),
            }
        }
    }
}

#[test]
fn preparation_unwind_returns_typed_outer_terminal_only_when_terminalization_fails() {
    let clean_fixture = TestRoot::new("preparation-unwind-clean-outer-terminal");
    let clean_cas = FsCasV1::create_new(clean_fixture.path()).unwrap();
    let clean_bound_invoked = AtomicBool::new(false);
    let clean_supply_invoked = AtomicBool::new(false);
    let mut clean_counters = OperationCountersV1::default();
    let mut clean_control = PanicPreparationInitializationAndPoisonTerminalV1 {
        cas_to_poison: None,
        construction_panicked: false,
        preparation_cleanup_calls: 0,
        fail_invalidation: false,
        root_invalidation_callbacks: 0,
    };
    let clean_unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = run_small_create_with_supplier_and_counters(
            &clean_cas,
            0x901,
            &mut clean_control,
            CallbackCheckedSupplier {
                bound_invoked: &clean_bound_invoked,
                supply_invoked: &clean_supply_invoked,
            },
            &mut clean_counters,
        );
    }));
    assert!(clean_unwind.is_err());
    assert!(clean_control.construction_panicked);
    assert_eq!(clean_control.preparation_cleanup_calls, 4);
    assert_eq!(clean_control.root_invalidation_callbacks, 0);
    assert!(clean_bound_invoked.load(Ordering::Acquire));
    assert!(!clean_supply_invoked.load(Ordering::Acquire));
    assert_operation_authority_baseline(&clean_cas, clean_fixture.path());
    assert_storage_equations(&clean_counters);
    let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
        exact_operation_namespace_usage(clean_fixture.path());
    assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
    assert_eq!((immutable_bytes, immutable_inodes), (0, 0));
    assert_eq!(clean_counters.storage_bytes_committed, 0);
    assert_eq!(clean_counters.storage_inodes_committed, 0);
    assert_eq!(clean_counters.storage_bytes_retained, 0);
    assert_eq!(clean_counters.storage_inodes_retained, 0);
    assert_eq!(clean_counters.mutable_preparation_residue_bytes, 0);
    assert_eq!(clean_counters.mutable_preparation_residue_inodes, 0);
    assert_eq!(clean_counters.unreachable_installed_residue_bytes, 0);
    assert!(clean_counters.has_zero_forbidden_work());
    assert!(clean_cas.visibility_lock_available_for_test_v1());
    assert!(clean_cas.publication_lock_available_for_test_v1());
    assert!(clean_cas.occupied().is_ok());
    drop(FsCasV1::open_existing(clean_fixture.path()).unwrap());

    let mut followup_control = ContinueControl;
    let (followup, followup_counters) = run_small_create_with_supplier(
        &clean_cas,
        0x902,
        &mut followup_control,
        CheckedSupplier { bytes: &[0x7b] },
    );
    assert!(followup.is_ok());
    assert_storage_equations(&followup_counters);
    assert!(followup_counters.has_zero_forbidden_work());

    for (case, fail_invalidation, expected) in [
        (
            "preparation-unwind-storage-terminal-poison",
            false,
            FsCasErrorV1::SynchronizationPoisoned,
        ),
        (
            "preparation-unwind-storage-terminal-double-fault",
            true,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::SynchronizationPoisoned,
                dominant: FsCasFailureCauseV1::InvalidationFailed,
            },
        ),
    ] {
        let fixture = TestRoot::new(case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let bound_invoked = AtomicBool::new(false);
        let supply_invoked = AtomicBool::new(false);
        let mut counters = OperationCountersV1::default();
        let mut control = PanicPreparationInitializationAndPoisonTerminalV1 {
            cas_to_poison: Some(cas.clone()),
            construction_panicked: false,
            preparation_cleanup_calls: 0,
            fail_invalidation,
            root_invalidation_callbacks: 0,
        };

        let result = run_small_create_with_supplier_and_counters(
            &cas,
            0x903 + u64::from(fail_invalidation),
            &mut control,
            CallbackCheckedSupplier {
                bound_invoked: &bound_invoked,
                supply_invoked: &supply_invoked,
            },
            &mut counters,
        );

        assert_eq!(result, Err(OperationErrorV1::FsCas(expected)), "{case}");
        assert!(control.construction_panicked, "{case}");
        assert_eq!(control.preparation_cleanup_calls, 4, "{case}");
        assert_eq!(control.root_invalidation_callbacks, 1, "{case}");
        assert!(bound_invoked.load(Ordering::Acquire), "{case}");
        assert!(!supply_invoked.load(Ordering::Acquire), "{case}");
        assert_eq!(counters.source_read_calls, 0, "{case}");
        assert_operation_authority_baseline(&cas, fixture.path());
        assert_storage_equations(&counters);
        let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
            exact_operation_namespace_usage(fixture.path());
        assert_eq!((preparation_bytes, preparation_inodes), (0, 0), "{case}");
        assert_eq!((immutable_bytes, immutable_inodes), (0, 0), "{case}");
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert_eq!(counters.storage_bytes_retained, 0, "{case}");
        assert_eq!(counters.storage_inodes_retained, 0, "{case}");
        assert_eq!(counters.mutable_preparation_residue_bytes, 0, "{case}");
        assert_eq!(counters.mutable_preparation_residue_inodes, 0, "{case}");
        assert_eq!(counters.unreachable_installed_residue_bytes, 0, "{case}");
        assert!(counters.has_zero_forbidden_work(), "{case}");
        assert!(cas.visibility_lock_available_for_test_v1(), "{case}");
        assert!(cas.publication_lock_available_for_test_v1(), "{case}");
        assert!(
            matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}",
        );
        assert!(
            matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}",
        );
        match FsCasV1::open_existing(fixture.path()) {
            Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
            Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
            Ok(_) => panic!("{case}: damaged root reopened as usable"),
        }
    }
}

#[test]
fn closure_unwind_returns_typed_outer_terminal_only_when_terminalization_fails() {
    for (case, poison_terminal, fail_invalidation) in [
        ("closure-unwind-clean-outer-terminal", false, false),
        ("closure-unwind-storage-terminal-poison", true, false),
        ("closure-unwind-storage-terminal-double-fault", true, true),
    ] {
        let fixture = TestRoot::new(case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let bound_invoked = AtomicBool::new(false);
        let supply_invoked = AtomicBool::new(false);
        let mut counters = OperationCountersV1::default();
        let mut control = PanicClosureFenceAndPoisonTerminalV1 {
            cas_to_poison: poison_terminal.then(|| cas.clone()),
            closure_panicked: false,
            preparation_cleanup_calls: 0,
            fail_invalidation,
            root_invalidation_callbacks: 0,
        };

        let terminal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_small_create_with_supplier_and_counters(
                &cas,
                0x907 + u64::from(poison_terminal) + u64::from(fail_invalidation),
                &mut control,
                CallbackCheckedSupplier {
                    bound_invoked: &bound_invoked,
                    supply_invoked: &supply_invoked,
                },
                &mut counters,
            )
        }));

        if poison_terminal {
            let expected = if fail_invalidation {
                FsCasErrorV1::TerminalFailure {
                    first: FsCasFailureCauseV1::SynchronizationPoisoned,
                    dominant: FsCasFailureCauseV1::InvalidationFailed,
                }
            } else {
                FsCasErrorV1::SynchronizationPoisoned
            };
            assert_eq!(
                terminal.unwrap(),
                Err(OperationErrorV1::FsCas(expected)),
                "{case}",
            );
        } else {
            let payload = terminal.expect_err("clean closure terminal must resume its payload");
            assert_eq!(
                payload.downcast_ref::<&'static str>().copied(),
                Some("injected closure-fence unwind before outer terminal"),
                "{case}",
            );
        }

        assert!(control.closure_panicked, "{case}");
        // File Create owns five always-present preparation spools, including
        // the file-backed locator-receipt spool. The built-file and
        // built-directory spools are tree-operation-only and must not be
        // fabricated merely to make the unwind count uniform.
        assert_eq!(control.preparation_cleanup_calls, 5, "{case}");
        assert_eq!(
            control.root_invalidation_callbacks,
            usize::from(poison_terminal),
            "{case}",
        );
        assert!(bound_invoked.load(Ordering::Acquire), "{case}");
        assert!(supply_invoked.load(Ordering::Acquire), "{case}");
        assert_operation_authority_baseline(&cas, fixture.path());
        assert_storage_equations(&counters);
        let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
            exact_operation_namespace_usage(fixture.path());
        assert_eq!((preparation_bytes, preparation_inodes), (0, 0), "{case}");
        assert!(immutable_bytes > 0, "{case}");
        assert!(immutable_inodes > 0, "{case}");
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert_eq!(counters.storage_bytes_retained, immutable_bytes, "{case}");
        assert_eq!(counters.storage_inodes_retained, immutable_inodes, "{case}");
        assert_eq!(counters.mutable_preparation_residue_bytes, 0, "{case}");
        assert_eq!(counters.mutable_preparation_residue_inodes, 0, "{case}");
        assert_eq!(counters.immutable_residue_bytes, immutable_bytes, "{case}");
        assert_eq!(
            counters.immutable_residue_inodes, immutable_inodes,
            "{case}"
        );
        assert_eq!(
            counters.unreachable_installed_residue_bytes, immutable_bytes,
            "{case}",
        );
        assert!(counters.has_zero_forbidden_work(), "{case}");
        assert!(cas.visibility_lock_available_for_test_v1(), "{case}");
        assert!(cas.publication_lock_available_for_test_v1(), "{case}");

        if poison_terminal {
            assert!(
                matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
                "{case}",
            );
            assert!(
                matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
                "{case}",
            );
            assert!(matches!(
                FsCasV1::open_existing(fixture.path()),
                Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy)
            ));
        } else {
            assert!(cas.occupied().is_ok(), "{case}");
            assert!(stale.occupied().is_ok(), "{case}");
            assert!(FsCasV1::open_existing(fixture.path()).is_ok(), "{case}");
        }
    }
}

#[test]
fn preparation_initialization_unwind_returns_typed_cleanup_terminal_after_all_owned_cleanup() {
    for fail_invalidation in [false, true] {
        let label = if fail_invalidation {
            "preparation-initialization-unwind-invalidation-double-fault"
        } else {
            "preparation-initialization-unwind-cleanup-failure"
        };
        let fixture = TestRoot::new(label);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let bound_invoked = AtomicBool::new(false);
        let supply_invoked = AtomicBool::new(false);
        let mut control = PanicPreparationInitializationWithCleanupFailureV1 {
            construction_panicked: false,
            preparation_cleanup_calls: 0,
            fail_invalidation,
            root_invalidation_callbacks: 0,
        };

        let (result, counters) = run_small_create_with_callback_observation(
            &cas,
            0x8f1,
            &mut control,
            &bound_invoked,
            &supply_invoked,
        );
        let cleanup = FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool);
        let expected = if fail_invalidation {
            FsCasErrorV1::TerminalFailure {
                first: cleanup,
                dominant: FsCasFailureCauseV1::InvalidationFailed,
            }
        } else {
            FsCasErrorV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool)
        };

        assert_eq!(result, Err(OperationErrorV1::FsCas(expected)), "{label}");
        assert!(control.construction_panicked, "{label}");
        assert_eq!(control.preparation_cleanup_calls, 4, "{label}");
        assert_eq!(control.root_invalidation_callbacks, 1, "{label}");
        assert!(bound_invoked.load(Ordering::Acquire), "{label}");
        assert!(!supply_invoked.load(Ordering::Acquire), "{label}");
        assert_eq!(counters.source_read_calls, 0, "{label}");
        assert_eq!(cas.operation_admitted_slots_v1(), 0, "{label}");
        assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{label}");
        assert_eq!(
            cas.storage_admission_active_for_test_v1(),
            (0, 0, 0),
            "{label}",
        );
        assert_storage_equations(&counters);
        let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
            exact_operation_namespace_usage(fixture.path());
        assert_eq!((preparation_bytes, preparation_inodes), (0, 1), "{label}");
        assert_eq!((immutable_bytes, immutable_inodes), (0, 0), "{label}");
        assert_eq!(counters.storage_bytes_committed, 0, "{label}");
        assert_eq!(counters.storage_inodes_committed, 0, "{label}");
        assert_eq!(counters.storage_bytes_retained, 0, "{label}");
        assert_eq!(counters.storage_inodes_retained, 1, "{label}");
        assert_eq!(counters.mutable_preparation_residue_bytes, 0, "{label}");
        assert_eq!(counters.mutable_preparation_residue_inodes, 1, "{label}");
        assert_eq!(counters.unreachable_installed_residue_bytes, 0, "{label}");
        assert!(counters.has_zero_forbidden_work(), "{label}");
        assert!(cas.visibility_lock_available_for_test_v1(), "{label}");
        assert!(cas.publication_lock_available_for_test_v1(), "{label}");
        assert!(
            matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{label}",
        );
        assert!(
            matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{label}",
        );
        match FsCasV1::open_existing(fixture.path()) {
            Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
            Err(error) => panic!("{label}: unexpected reopen error {error:?}"),
            Ok(_) => panic!("{label}: damaged root reopened as usable"),
        }
    }
}

#[test]
fn preparation_accounting_failure_preserves_poison_and_invalidation_dominance() {
    for (case, fail_invalidation, expected) in [
        (
            "preparation-accounting-poison",
            false,
            FsCasErrorV1::SynchronizationPoisoned,
        ),
        (
            "preparation-accounting-invalidation-double-fault",
            true,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::SynchronizationPoisoned,
                dominant: FsCasFailureCauseV1::InvalidationFailed,
            },
        ),
    ] {
        let fixture = TestRoot::new(case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let bound_invoked = AtomicBool::new(false);
        let supply_invoked = AtomicBool::new(false);
        let mut control = PoisonStorageBeforePreparationAccountingV1 {
            cas: cas.clone(),
            poisoned: false,
            fail_invalidation,
        };
        let (result, counters) = run_small_create_with_callback_observation(
            &cas,
            0x8fd,
            &mut control,
            &bound_invoked,
            &supply_invoked,
        );

        assert_eq!(result, Err(OperationErrorV1::FsCas(expected)), "{case}");
        assert!(control.poisoned, "{case}");
        assert!(bound_invoked.load(Ordering::Acquire), "{case}");
        assert!(!supply_invoked.load(Ordering::Acquire), "{case}");
        assert_operation_authority_baseline(&cas, fixture.path());
        assert_storage_equations(&counters);
        let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
            exact_operation_namespace_usage(fixture.path());
        assert_eq!((preparation_bytes, preparation_inodes), (0, 0), "{case}");
        assert_eq!((immutable_bytes, immutable_inodes), (0, 0), "{case}");
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert_eq!(counters.storage_bytes_retained, 0, "{case}");
        assert_eq!(counters.storage_inodes_retained, 0, "{case}");
        assert!(counters.has_zero_forbidden_work(), "{case}");
        assert!(
            matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        assert!(
            matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        match FsCasV1::open_existing(fixture.path()) {
            Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
            Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
            Ok(_) => panic!("{case}: damaged root reopened as usable"),
        }
    }
}

#[test]
fn preparation_open_failure_preserves_cleanup_accounting_failure() {
    let fixture = TestRoot::new("preparation-open-accounting-cleanup");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let bound_invoked = AtomicBool::new(false);
    let supply_invoked = AtomicBool::new(false);
    let mut control = BreakPreparationAccountingAndFailCreateV1 {
        cas: cas.clone(),
        fired: false,
    };
    let (result, counters) = run_small_create_with_callback_observation(
        &cas,
        0x8fc,
        &mut control,
        &bound_invoked,
        &supply_invoked,
    );

    assert_eq!(
        result,
        Err(OperationErrorV1::FsCas(FsCasErrorV1::TerminalFailure {
            first: FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::NoSpace),
            dominant: FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool,),
        }))
    );
    assert!(control.fired);
    assert!(bound_invoked.load(Ordering::Acquire));
    assert!(!supply_invoked.load(Ordering::Acquire));
    assert_operation_authority_baseline(&cas, fixture.path());
    assert_storage_equations(&counters);
    let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
        exact_operation_namespace_usage(fixture.path());
    assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
    assert_eq!((immutable_bytes, immutable_inodes), (0, 0));
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert_eq!(counters.storage_bytes_retained, 0);
    assert_eq!(counters.storage_inodes_retained, 0);
    assert!(counters.has_zero_forbidden_work());
    assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(
        FsCasV1::open_existing(fixture.path()),
        Err(FsCasErrorV1::Invalidated)
    ));
}

#[test]
fn private_pack_precharge_poison_preserves_invalidation_dominance() {
    for (case, fail_invalidation, expected) in [
        (
            "private-pack-precharge-poison",
            false,
            FsCasErrorV1::SynchronizationPoisoned,
        ),
        (
            "private-pack-precharge-invalidation-double-fault",
            true,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::SynchronizationPoisoned,
                dominant: FsCasFailureCauseV1::InvalidationFailed,
            },
        ),
    ] {
        let fixture = TestRoot::new(case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let mut counters = OperationCountersV1::default();
        let mut continue_control = ContinueControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x8fa,
                &mut counters,
                &mut continue_control,
            )
            .unwrap();
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(0, 0, 1, 0).unwrap())
            .unwrap();
        let token = capability.storage_token_v1().unwrap();
        let mut private_pack = cas.begin_private_pack_borrowed_v1(token).unwrap();

        cas.poison_storage_admission_for_test_v1();
        let mut control = FailRootInvalidationV1 {
            fail: fail_invalidation,
        };
        assert!(private_pack
            .begin_direct_controlled_v1(128, &mut control)
            .is_err());
        assert_eq!(
            private_pack.take_first_error_typed_v1(),
            Some(expected),
            "{case}"
        );
        // The charge itself never completed and the create callback was never
        // reached, so explicit cleanup owns no filesystem or accounting
        // target and succeeds without relying on Drop.
        private_pack.cleanup_controlled_v1(&mut control).unwrap();
        drop(private_pack);

        assert_eq!(
            capability.finish_terminal_v1(false, &mut counters, &mut control),
            Err(expected),
            "{case}"
        );
        assert_operation_authority_baseline(&cas, fixture.path());
        assert_storage_equations(&counters);
        let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
            exact_operation_namespace_usage(fixture.path());
        assert_eq!((preparation_bytes, preparation_inodes), (0, 0), "{case}");
        assert_eq!((immutable_bytes, immutable_inodes), (0, 0), "{case}");
        assert_eq!(counters.storage_bytes_requested, 0, "{case}");
        assert_eq!(counters.storage_inodes_requested, 1, "{case}");
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert_eq!(counters.storage_bytes_retained, 0, "{case}");
        assert_eq!(counters.storage_inodes_retained, 0, "{case}");
        assert!(counters.has_zero_forbidden_work(), "{case}");
        assert!(
            matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        assert!(
            matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        match FsCasV1::open_existing(fixture.path()) {
            Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
            Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
            Ok(_) => panic!("{case}: damaged root reopened as usable"),
        }
    }
}

#[test]
fn private_pack_create_failure_preserves_cleanup_accounting_failure() {
    for (case, create_error, first, fail_invalidation, dominant) in [
        (
            "private-pack-create-write-accounting-failure",
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure),
            FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure),
            false,
            FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PrivatePack),
        ),
        (
            "private-pack-create-write-invalidation-double-fault",
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure),
            FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure),
            true,
            FsCasFailureCauseV1::InvalidationFailed,
        ),
        (
            "private-pack-create-permission-accounting-failure",
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
            FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
            false,
            FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PrivatePack),
        ),
        (
            "private-pack-create-permission-invalidation-double-fault",
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
            FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
            true,
            FsCasFailureCauseV1::InvalidationFailed,
        ),
    ] {
        let fixture = TestRoot::new(case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let mut counters = OperationCountersV1::default();
        let mut continue_control = ContinueControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x8fb,
                &mut counters,
                &mut continue_control,
            )
            .unwrap();
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(0, 0, 1, 0).unwrap())
            .unwrap();
        let token = capability.storage_token_v1().unwrap();
        let mut private_pack = cas.begin_private_pack_borrowed_v1(token).unwrap();
        let mut control = BreakPrivatePackAccountingAndFailCreateV1 {
            cas: cas.clone(),
            create_error,
            fired: false,
            fail_invalidation,
        };

        assert!(private_pack
            .begin_direct_controlled_v1(128, &mut control)
            .is_err());
        let expected = FsCasErrorV1::TerminalFailure { first, dominant };
        assert_eq!(
            private_pack.take_first_error_typed_v1(),
            Some(expected),
            "{case}"
        );
        assert!(control.fired, "{case}");
        let cleanup = private_pack
            .cleanup_controlled_v1(&mut control)
            .expect_err("the classified cleanup failure must not be retried");
        assert_eq!(cleanup.dominant_cause_v1(), dominant, "{case}");
        drop(private_pack);

        capability
            .finish_terminal_v1(false, &mut counters, &mut continue_control)
            .unwrap();
        assert_eq!(cas.operation_admitted_slots_v1(), 0, "{case}");
        assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{case}");
        assert_eq!(
            cas.storage_admission_active_for_test_v1(),
            (0, 0, 0),
            "{case}"
        );
        assert_storage_equations(&counters);
        let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
            exact_operation_namespace_usage(fixture.path());
        assert_eq!((preparation_bytes, preparation_inodes), (0, 0), "{case}");
        assert_eq!((immutable_bytes, immutable_inodes), (0, 0), "{case}");
        assert_eq!(counters.storage_bytes_requested, 0, "{case}");
        assert_eq!(counters.storage_inodes_requested, 1, "{case}");
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert_eq!(counters.storage_bytes_retained, 0, "{case}");
        assert_eq!(counters.storage_inodes_retained, 0, "{case}");
        assert!(counters.has_zero_forbidden_work(), "{case}");
        assert!(
            matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        assert!(
            matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        match FsCasV1::open_existing(fixture.path()) {
            Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
            Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
            Ok(_) => panic!("{case}: damaged root reopened as usable"),
        }
    }
}

#[test]
fn marker_create_preserves_directional_error_and_accounting_cleanup_dominance() {
    for (case, create_error, break_accounting, fail_invalidation, expected) in [
        (
            "marker-create-write-failure",
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure),
            false,
            false,
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure),
        ),
        (
            "marker-create-permission-denied",
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
            false,
            false,
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
        ),
        (
            "marker-create-accounting-cleanup-failure",
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure),
            true,
            false,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure),
                dominant: FsCasFailureCauseV1::CleanupFailed(
                    FsCasCleanupTargetV1::PreparationSpool,
                ),
            },
        ),
        (
            "marker-create-accounting-invalidation-double-fault",
            FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
            true,
            true,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
                dominant: FsCasFailureCauseV1::InvalidationFailed,
            },
        ),
    ] {
        let fixture = TestRoot::new(case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let mut counters = OperationCountersV1::default();
        let mut continue_control = ContinueControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x900,
                &mut counters,
                &mut continue_control,
            )
            .unwrap();
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(0, 0, 1, 0).unwrap())
            .unwrap();
        let token = capability.storage_token_v1().unwrap();
        let mut control = BreakMarkerAccountingAndFailCreateV1 {
            cas: cas.clone(),
            create_error,
            break_accounting,
            fired: false,
            fail_invalidation,
        };

        assert_eq!(
            cas.publish_test_marker_borrowed_v1(token, &mut control),
            Err(expected),
            "{case}"
        );
        assert!(control.fired, "{case}");
        assert_eq!(
            fs::read_dir(fixture.path().join("preparation"))
                .unwrap()
                .count(),
            0,
            "{case}"
        );
        assert_eq!(
            fs::read_dir(fixture.path().join("closures"))
                .unwrap()
                .count(),
            0,
            "{case}"
        );

        capability
            .finish_terminal_v1(false, &mut counters, &mut continue_control)
            .unwrap();
        assert_operation_authority_baseline(&cas, fixture.path());
        assert_storage_equations(&counters);
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert_eq!(counters.storage_bytes_retained, 0, "{case}");
        assert_eq!(counters.storage_inodes_retained, 0, "{case}");
        assert!(counters.has_zero_forbidden_work(), "{case}");

        if break_accounting {
            assert!(
                matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
                "{case}"
            );
            assert!(
                matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
                "{case}"
            );
            match FsCasV1::open_existing(fixture.path()) {
                Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
                Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
                Ok(_) => panic!("{case}: damaged root reopened as usable"),
            }
        } else {
            assert!(cas.occupied().is_ok(), "{case}");
            assert!(stale.occupied().is_ok(), "{case}");
        }
    }
}

#[test]
fn marker_length_precharge_preserves_accounting_and_invalidation_cause() {
    for (case, fail_invalidation, expected) in [
        (
            "marker-length-accounting-failure",
            false,
            FsCasErrorV1::Integrity,
        ),
        (
            "marker-length-accounting-invalidation-double-fault",
            true,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::Integrity,
                dominant: FsCasFailureCauseV1::InvalidationFailed,
            },
        ),
    ] {
        let fixture = TestRoot::new(case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let mut counters = OperationCountersV1::default();
        let mut continue_control = ContinueControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x901,
                &mut counters,
                &mut continue_control,
            )
            .unwrap();
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(8, 0, 1, 0).unwrap())
            .unwrap();
        let token = capability.storage_token_v1().unwrap();
        let mut control = BreakMarkerLengthAccountingV1 {
            cas: cas.clone(),
            corrupted: false,
            restored_for_cleanup: false,
            payload_or_link_seen: false,
            fail_invalidation,
        };

        assert_eq!(
            cas.publish_test_marker_borrowed_v1(token, &mut control),
            Err(expected),
            "{case}"
        );
        assert!(control.corrupted, "{case}");
        assert!(control.restored_for_cleanup, "{case}");
        assert!(!control.payload_or_link_seen, "{case}");
        assert_eq!(
            fs::read_dir(fixture.path().join("preparation"))
                .unwrap()
                .count(),
            0,
            "{case}"
        );
        assert_eq!(
            fs::read_dir(fixture.path().join("closures"))
                .unwrap()
                .count(),
            0,
            "{case}"
        );

        capability
            .finish_terminal_v1(false, &mut counters, &mut continue_control)
            .unwrap();
        assert_operation_authority_baseline(&cas, fixture.path());
        assert_storage_equations(&counters);
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert_eq!(counters.storage_bytes_retained, 0, "{case}");
        assert_eq!(counters.storage_inodes_retained, 0, "{case}");
        assert!(counters.has_zero_forbidden_work(), "{case}");
        assert!(
            matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        assert!(
            matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        match FsCasV1::open_existing(fixture.path()) {
            Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
            Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
            Ok(_) => panic!("{case}: damaged root reopened as usable"),
        }
    }
}

#[test]
fn marker_immutable_precharge_preserves_accounting_and_invalidation_cause() {
    for (case, fail_invalidation, expected) in [
        (
            "marker-immutable-accounting-failure",
            false,
            FsCasErrorV1::Integrity,
        ),
        (
            "marker-immutable-accounting-invalidation-double-fault",
            true,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::Integrity,
                dominant: FsCasFailureCauseV1::InvalidationFailed,
            },
        ),
    ] {
        let fixture = TestRoot::new(case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let mut counters = OperationCountersV1::default();
        let mut continue_control = ContinueControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x902,
                &mut counters,
                &mut continue_control,
            )
            .unwrap();
        // The private marker is admitted and written, while the deliberately
        // zero immutable envelope makes the checked no-replace precharge fail
        // before the hard-link attempt.
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(8, 0, 1, 0).unwrap())
            .unwrap();
        let token = capability.storage_token_v1().unwrap();
        let mut control = ObserveMarkerImmutablePrechargeV1 {
            marker_write_seen: false,
            marker_link_boundary_seen: false,
            fail_invalidation,
        };

        assert_eq!(
            cas.publish_test_marker_borrowed_v1(token, &mut control),
            Err(expected),
            "{case}"
        );
        assert!(control.marker_write_seen, "{case}");
        assert!(control.marker_link_boundary_seen, "{case}");
        assert_eq!(
            fs::read_dir(fixture.path().join("preparation"))
                .unwrap()
                .count(),
            0,
            "{case}"
        );
        assert_eq!(
            fs::read_dir(fixture.path().join("closures"))
                .unwrap()
                .count(),
            0,
            "{case}"
        );

        capability
            .finish_terminal_v1(false, &mut counters, &mut continue_control)
            .unwrap();
        assert_operation_authority_baseline(&cas, fixture.path());
        assert_storage_equations(&counters);
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert_eq!(counters.storage_bytes_retained, 0, "{case}");
        assert_eq!(counters.storage_inodes_retained, 0, "{case}");
        assert!(counters.has_zero_forbidden_work(), "{case}");
        assert!(
            matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        assert!(
            matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        match FsCasV1::open_existing(fixture.path()) {
            Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
            Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
            Ok(_) => panic!("{case}: damaged root reopened as usable"),
        }
    }
}

#[test]
fn equal_marker_incumbent_rollback_preserves_poison_and_invalidation_cause() {
    for (case, fail_invalidation, expected) in [
        (
            "marker-incumbent-rollback-poison",
            false,
            FsCasErrorV1::SynchronizationPoisoned,
        ),
        (
            "marker-incumbent-rollback-invalidation-double-fault",
            true,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::SynchronizationPoisoned,
                dominant: FsCasFailureCauseV1::InvalidationFailed,
            },
        ),
    ] {
        let fixture = TestRoot::new(case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let mut continue_control = ContinueControl;

        // Establish the equal immutable incumbent through the same production
        // transaction so root storage custody includes its exact bytes/name.
        let mut setup_counters = OperationCountersV1::default();
        let mut setup = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x903,
                &mut setup_counters,
                &mut continue_control,
            )
            .unwrap();
        setup
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(8, 8, 1, 1).unwrap())
            .unwrap();
        let setup_token = setup.storage_token_v1().unwrap();
        cas.publish_test_marker_borrowed_v1(setup_token, &mut continue_control)
            .unwrap();
        setup
            .finish_terminal_v1(true, &mut setup_counters, &mut continue_control)
            .unwrap();
        assert_storage_equations(&setup_counters);
        assert_eq!(setup_counters.storage_bytes_committed, 8, "{case}");
        assert_eq!(setup_counters.storage_inodes_committed, 1, "{case}");
        assert_eq!(
            fs::read(fixture.path().join("closures/test-marker")).unwrap(),
            [0x6d; 8],
            "{case}"
        );

        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let mut counters = OperationCountersV1::default();
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x904,
                &mut counters,
                &mut continue_control,
            )
            .unwrap();
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(8, 8, 1, 1).unwrap())
            .unwrap();
        let token = capability.storage_token_v1().unwrap();
        cas.poison_next_immutable_remove_for_test_v1();
        let mut control = FailRootInvalidationV1 {
            fail: fail_invalidation,
        };

        assert_eq!(
            cas.publish_test_marker_borrowed_v1(token, &mut control),
            Err(expected),
            "{case}"
        );
        assert_eq!(
            fs::read(fixture.path().join("closures/test-marker")).unwrap(),
            [0x6d; 8],
            "{case}"
        );
        assert_eq!(
            fs::read_dir(fixture.path().join("preparation"))
                .unwrap()
                .count(),
            0,
            "{case}"
        );

        assert_eq!(
            capability.finish_terminal_v1(false, &mut counters, &mut continue_control),
            Err(FsCasErrorV1::SynchronizationPoisoned),
            "{case}"
        );
        assert_operation_authority_baseline(&cas, fixture.path());
        assert_storage_equations(&counters);
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert_eq!(counters.storage_bytes_retained, 0, "{case}");
        assert_eq!(counters.storage_inodes_retained, 0, "{case}");
        assert!(counters.has_zero_forbidden_work(), "{case}");
        assert!(
            matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        assert!(
            matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        match FsCasV1::open_existing(fixture.path()) {
            Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
            Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
            Ok(_) => panic!("{case}: damaged root reopened as usable"),
        }
    }
}

#[test]
fn marker_hard_link_error_preserves_directional_cause_and_cleanup_dominance() {
    for (case, fail_invalidation, expected) in [
        (
            "marker-hard-link-write-failure-cleanup-dominant",
            false,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure),
                dominant: FsCasFailureCauseV1::CleanupFailed(
                    FsCasCleanupTargetV1::PreparationSpool,
                ),
            },
        ),
        (
            "marker-hard-link-write-failure-invalidation-double-fault",
            true,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure),
                dominant: FsCasFailureCauseV1::InvalidationFailed,
            },
        ),
    ] {
        let fixture = TestRoot::new(case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let mut counters = OperationCountersV1::default();
        let mut continue_control = ContinueControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x905,
                &mut counters,
                &mut continue_control,
            )
            .unwrap();
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(8, 8, 1, 1).unwrap())
            .unwrap();
        let token = capability.storage_token_v1().unwrap();

        // Keep preparation valid so the private marker is created and its
        // immutable destination charge succeeds. Replacing only the closures
        // directory with a regular file then drives a real post-charge
        // `hard_link` failure (ENOTDIR on the owner platform), rather than an
        // earlier injected fault boundary.
        let closures = fixture.path().join("closures");
        fs::remove_dir(&closures).unwrap();
        fs::write(&closures, b"not-a-directory").unwrap();
        cas.poison_next_immutable_remove_for_test_v1();
        let mut control = FailRootInvalidationV1 {
            fail: fail_invalidation,
        };

        let terminal = cas.publish_test_marker_borrowed_v1(token, &mut control);

        // Restore the fixed root shape before terminal/root-custody checks.
        // The destination was never visible and the private marker must have
        // been explicitly removed by the publication transaction.
        fs::remove_file(&closures).unwrap();
        fs::create_dir(&closures).unwrap();
        assert_eq!(terminal, Err(expected), "{case}");
        assert_eq!(fs::read_dir(&closures).unwrap().count(), 0, "{case}");
        assert_eq!(
            fs::read_dir(fixture.path().join("preparation"))
                .unwrap()
                .count(),
            0,
            "{case}"
        );

        assert_eq!(
            capability.finish_terminal_v1(false, &mut counters, &mut continue_control),
            Err(FsCasErrorV1::SynchronizationPoisoned),
            "{case}"
        );
        assert_operation_authority_baseline(&cas, fixture.path());
        assert_storage_equations(&counters);
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert_eq!(counters.storage_bytes_retained, 0, "{case}");
        assert_eq!(counters.storage_inodes_retained, 0, "{case}");
        assert_eq!(
            exact_operation_namespace_usage(fixture.path()),
            ((0, 0), (0, 0)),
            "{case}"
        );
        assert!(counters.has_zero_forbidden_work(), "{case}");
        assert!(
            matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        assert!(
            matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        match FsCasV1::open_existing(fixture.path()) {
            Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
            Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
            Ok(_) => panic!("{case}: damaged root reopened as usable"),
        }
    }
}

#[test]
fn marker_cleanup_length_reconciliation_retains_exact_residue_and_terminal_cause() {
    for (case, fail_invalidation, expected) in [
        (
            "marker-cleanup-length-accounting-failure",
            false,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::Integrity,
                dominant: FsCasFailureCauseV1::CleanupFailed(
                    FsCasCleanupTargetV1::PreparationSpool,
                ),
            },
        ),
        (
            "marker-cleanup-length-invalidation-double-fault",
            true,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::Integrity,
                dominant: FsCasFailureCauseV1::InvalidationFailed,
            },
        ),
    ] {
        let fixture = TestRoot::new(case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let mut counters = OperationCountersV1::default();
        let mut continue_control = ContinueControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x906,
                &mut counters,
                &mut continue_control,
            )
            .unwrap();
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(9, 0, 1, 0).unwrap())
            .unwrap();
        let token = capability.storage_token_v1().unwrap();
        let temporary = cas.prepare_test_marker_cleanup_mismatch_v1(token).unwrap();
        assert_eq!(fs::metadata(&temporary).unwrap().len(), 9, "{case}");

        // The owned file is physically nine bytes while the transaction's
        // last successful charge is eight. Corrupting that exact charge makes
        // cleanup-time 8 -> 9 reconciliation fail; the invalidation hook
        // restores the known physical value solely so terminal equations can
        // classify all nine bytes as operation-relative retained residue.
        cas.clear_active_preparation_bytes_for_test_v1();
        let mut control = RestoreMarkerCleanupAccountingV1 {
            cas: cas.clone(),
            accounting_restored: false,
            fail_invalidation,
        };
        assert_eq!(
            cas.cleanup_test_marker_mismatch_borrowed_v1(token, &mut control),
            Err(expected),
            "{case}"
        );
        assert!(control.accounting_restored, "{case}");
        assert_eq!(fs::metadata(&temporary).unwrap().len(), 9, "{case}");
        assert_eq!(
            fs::read_dir(fixture.path().join("preparation"))
                .unwrap()
                .count(),
            1,
            "{case}"
        );

        capability
            .finish_terminal_v1(false, &mut counters, &mut continue_control)
            .unwrap();
        drop(capability);
        assert_eq!(cas.operation_admitted_slots_v1(), 0, "{case}");
        assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{case}");
        assert_eq!(
            cas.storage_admission_active_for_test_v1(),
            (0, 0, 0),
            "{case}"
        );
        assert_storage_equations(&counters);
        assert_eq!(counters.storage_bytes_requested, 9, "{case}");
        assert_eq!(counters.storage_inodes_requested, 1, "{case}");
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert_eq!(counters.storage_bytes_retained, 9, "{case}");
        assert_eq!(counters.storage_inodes_retained, 1, "{case}");
        assert_eq!(
            exact_operation_namespace_usage(fixture.path()),
            ((9, 1), (0, 0)),
            "{case}"
        );
        assert_eq!(fs::metadata(&temporary).unwrap().len(), 9, "{case}");
        assert!(counters.has_zero_forbidden_work(), "{case}");
        assert!(
            matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        assert!(
            matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        match FsCasV1::open_existing(fixture.path()) {
            Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
            Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
            Ok(_) => panic!("{case}: damaged root reopened as usable"),
        }
    }
}

#[test]
fn marker_cleanup_metadata_failure_preserves_first_cause_and_cleanup_dominance() {
    for (case, wrong_type, fail_invalidation, first, dominant) in [
        (
            "marker-cleanup-wrong-type",
            true,
            false,
            FsCasFailureCauseV1::Integrity,
            FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool),
        ),
        (
            "marker-cleanup-wrong-type-invalidation-double-fault",
            true,
            true,
            FsCasFailureCauseV1::Integrity,
            FsCasFailureCauseV1::InvalidationFailed,
        ),
        (
            "marker-cleanup-required-name-missing",
            false,
            false,
            FsCasFailureCauseV1::MissingOccupant,
            FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool),
        ),
        (
            "marker-cleanup-required-name-missing-invalidation-double-fault",
            false,
            true,
            FsCasFailureCauseV1::MissingOccupant,
            FsCasFailureCauseV1::InvalidationFailed,
        ),
    ] {
        let fixture = TestRoot::new(case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let mut counters = OperationCountersV1::default();
        let mut continue_control = ContinueControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x907,
                &mut counters,
                &mut continue_control,
            )
            .unwrap();
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(8, 0, 1, 0).unwrap())
            .unwrap();
        let token = capability.storage_token_v1().unwrap();
        let temporary = cas.prepare_test_marker_cleanup_file_v1(token, 8).unwrap();
        fs::remove_file(&temporary).unwrap();
        if wrong_type {
            fs::create_dir(&temporary).unwrap();
        }
        let mut control = FailRootInvalidationV1 {
            fail: fail_invalidation,
        };

        assert_eq!(
            cas.cleanup_test_marker_mismatch_borrowed_v1(token, &mut control),
            Err(FsCasErrorV1::TerminalFailure { first, dominant }),
            "{case}"
        );
        if wrong_type {
            assert!(
                fs::symlink_metadata(&temporary)
                    .unwrap()
                    .file_type()
                    .is_dir(),
                "{case}"
            );
            assert_eq!(
                fs::read_dir(fixture.path().join("preparation"))
                    .unwrap()
                    .count(),
                1,
                "{case}"
            );
        } else {
            assert_eq!(
                fs::symlink_metadata(&temporary).unwrap_err().kind(),
                std::io::ErrorKind::NotFound,
                "{case}"
            );
            assert_eq!(
                fs::read_dir(fixture.path().join("preparation"))
                    .unwrap()
                    .count(),
                0,
                "{case}"
            );
        }

        capability
            .finish_terminal_v1(false, &mut counters, &mut continue_control)
            .unwrap();
        drop(capability);
        assert_eq!(cas.operation_admitted_slots_v1(), 0, "{case}");
        assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{case}");
        assert_eq!(
            cas.storage_admission_active_for_test_v1(),
            (0, 0, 0),
            "{case}"
        );
        assert_storage_equations(&counters);
        assert_eq!(counters.storage_bytes_requested, 8, "{case}");
        assert_eq!(counters.storage_inodes_requested, 1, "{case}");
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert_eq!(counters.storage_bytes_retained, 8, "{case}");
        assert_eq!(counters.storage_inodes_retained, 1, "{case}");
        for immutable in ["carriers", "objects", "catalog", "closures"] {
            assert_eq!(
                fs::read_dir(fixture.path().join(immutable))
                    .unwrap()
                    .count(),
                0,
                "{case}: {immutable}"
            );
        }
        assert!(counters.has_zero_forbidden_work(), "{case}");
        assert!(
            matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        assert!(
            matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        match FsCasV1::open_existing(fixture.path()) {
            Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
            Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
            Ok(_) => panic!("{case}: damaged root reopened as usable"),
        }
    }
}

#[cfg(unix)]
#[test]
fn marker_cleanup_unlink_preserves_actual_directional_cause_and_injected_cleanup() {
    for (case, mode, fail_invalidation, expected) in [
        (
            "marker-cleanup-unlink-permission-denied",
            MarkerCleanupUnlinkFaultModeV1::PermissionDenied,
            false,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
                dominant: FsCasFailureCauseV1::CleanupFailed(
                    FsCasCleanupTargetV1::PreparationSpool,
                ),
            },
        ),
        (
            "marker-cleanup-unlink-permission-denied-invalidation-double-fault",
            MarkerCleanupUnlinkFaultModeV1::PermissionDenied,
            true,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
                dominant: FsCasFailureCauseV1::InvalidationFailed,
            },
        ),
        (
            "marker-cleanup-unlink-write-failure",
            MarkerCleanupUnlinkFaultModeV1::NonDirectory,
            false,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure),
                dominant: FsCasFailureCauseV1::CleanupFailed(
                    FsCasCleanupTargetV1::PreparationSpool,
                ),
            },
        ),
        (
            "marker-cleanup-unlink-write-failure-invalidation-double-fault",
            MarkerCleanupUnlinkFaultModeV1::NonDirectory,
            true,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::WriteFailure),
                dominant: FsCasFailureCauseV1::InvalidationFailed,
            },
        ),
        (
            "marker-cleanup-injected-failure",
            MarkerCleanupUnlinkFaultModeV1::Injected,
            false,
            FsCasErrorV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool),
        ),
        (
            "marker-cleanup-injected-invalidation-double-fault",
            MarkerCleanupUnlinkFaultModeV1::Injected,
            true,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool),
                dominant: FsCasFailureCauseV1::InvalidationFailed,
            },
        ),
    ] {
        let fixture = TestRoot::new(case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let mut counters = OperationCountersV1::default();
        let mut continue_control = ContinueControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x908,
                &mut counters,
                &mut continue_control,
            )
            .unwrap();
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(8, 0, 1, 0).unwrap())
            .unwrap();
        let token = capability.storage_token_v1().unwrap();
        let temporary = cas.prepare_test_marker_cleanup_file_v1(token, 8).unwrap();
        let preparation = fixture.path().join("preparation");
        let mut control = FailMarkerCleanupUnlinkV1 {
            held_preparation: fixture.path().join("preparation-unlink-held"),
            preparation,
            mode,
            armed: false,
            restored: false,
            fail_invalidation,
        };

        assert_eq!(
            cas.cleanup_test_marker_mismatch_borrowed_v1(token, &mut control),
            Err(expected),
            "{case}"
        );
        assert!(control.armed, "{case}");
        assert!(control.restored, "{case}");
        assert_eq!(fs::metadata(&temporary).unwrap().len(), 8, "{case}");
        assert_eq!(
            fs::read_dir(fixture.path().join("preparation"))
                .unwrap()
                .count(),
            1,
            "{case}"
        );

        capability
            .finish_terminal_v1(false, &mut counters, &mut continue_control)
            .unwrap();
        drop(capability);
        assert_eq!(cas.operation_admitted_slots_v1(), 0, "{case}");
        assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{case}");
        assert_eq!(
            cas.storage_admission_active_for_test_v1(),
            (0, 0, 0),
            "{case}"
        );
        assert_storage_equations(&counters);
        assert_eq!(counters.storage_bytes_requested, 8, "{case}");
        assert_eq!(counters.storage_inodes_requested, 1, "{case}");
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert_eq!(counters.storage_bytes_retained, 8, "{case}");
        assert_eq!(counters.storage_inodes_retained, 1, "{case}");
        assert_eq!(
            exact_operation_namespace_usage(fixture.path()),
            ((8, 1), (0, 0)),
            "{case}"
        );
        assert!(counters.has_zero_forbidden_work(), "{case}");
        assert!(
            matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        assert!(
            matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        match FsCasV1::open_existing(fixture.path()) {
            Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
            Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
            Ok(_) => panic!("{case}: damaged root reopened as usable"),
        }
    }
}

#[test]
fn marker_cleanup_post_unlink_accounting_failure_is_stable_and_fail_closed() {
    for (case, fail_invalidation, expected) in [
        (
            "marker-cleanup-post-unlink-accounting-failure",
            false,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::Integrity,
                dominant: FsCasFailureCauseV1::CleanupFailed(
                    FsCasCleanupTargetV1::PreparationSpool,
                ),
            },
        ),
        (
            "marker-cleanup-post-unlink-accounting-invalidation-double-fault",
            true,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::Integrity,
                dominant: FsCasFailureCauseV1::InvalidationFailed,
            },
        ),
    ] {
        let fixture = TestRoot::new(case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let mut counters = OperationCountersV1::default();
        let mut continue_control = ContinueControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x909,
                &mut counters,
                &mut continue_control,
            )
            .unwrap();
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(8, 0, 1, 0).unwrap())
            .unwrap();
        let token = capability.storage_token_v1().unwrap();
        let temporary = cas.prepare_test_marker_cleanup_file_v1(token, 8).unwrap();
        cas.fail_next_preparation_remove_for_test_v1();
        let mut control = FailRootInvalidationV1 {
            fail: fail_invalidation,
        };

        assert_eq!(
            cas.cleanup_test_marker_mismatch_borrowed_v1(token, &mut control),
            Err(expected),
            "{case}"
        );
        assert_eq!(
            fs::symlink_metadata(&temporary).unwrap_err().kind(),
            std::io::ErrorKind::NotFound,
            "{case}"
        );
        assert_eq!(
            fs::read_dir(fixture.path().join("preparation"))
                .unwrap()
                .count(),
            0,
            "{case}"
        );

        capability
            .finish_terminal_v1(false, &mut counters, &mut continue_control)
            .unwrap();
        drop(capability);
        assert_operation_authority_baseline(&cas, fixture.path());
        assert_storage_equations(&counters);
        assert_eq!(counters.storage_bytes_requested, 8, "{case}");
        assert_eq!(counters.storage_inodes_requested, 1, "{case}");
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert_eq!(counters.storage_bytes_retained, 8, "{case}");
        assert_eq!(counters.storage_inodes_retained, 1, "{case}");
        assert_eq!(
            exact_operation_namespace_usage(fixture.path()),
            ((0, 0), (0, 0)),
            "{case}"
        );
        assert!(counters.has_zero_forbidden_work(), "{case}");
        assert!(
            matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        assert!(
            matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        match FsCasV1::open_existing(fixture.path()) {
            Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
            Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
            Ok(_) => panic!("{case}: damaged root reopened as usable"),
        }
    }
}

#[test]
fn operation_spool_resize_accounting_failure_preserves_physical_state_and_invalidation_cause() {
    for (case, fail_invalidation, expected) in [
        (
            "spool-resize-accounting-failure",
            false,
            FsCasErrorV1::Integrity,
        ),
        (
            "spool-resize-accounting-invalidation-double-fault",
            true,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::Integrity,
                dominant: FsCasFailureCauseV1::InvalidationFailed,
            },
        ),
    ] {
        const ORIGINAL_BYTES: u64 = 17;
        const TRUNCATED_BYTES: u64 = 9;
        let fixture = TestRoot::new(case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let mut counters = OperationCountersV1::default();
        let mut continue_control = ContinueControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x8fe,
                &mut counters,
                &mut continue_control,
            )
            .unwrap();
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(ORIGINAL_BYTES, 0, 1, 0).unwrap())
            .unwrap();
        let token = capability.storage_token_v1().unwrap();
        let mut spool = cas
            .begin_operation_spool_borrowed_v1("resize-accounting", token, &mut continue_control)
            .unwrap();
        spool
            .initialize_zeroed_len_controlled_v1(ORIGINAL_BYTES, &mut continue_control)
            .unwrap();

        // Corrupt only the test ledger so the real truncate succeeds before
        // its authoritative accounting transition fails. This deterministically
        // proves that the owned state follows the physical file and that a
        // failed persistent invalidation cannot erase the initiating cause.
        cas.clear_active_preparation_bytes_for_test_v1();
        let mut control = FailRootInvalidationV1 {
            fail: fail_invalidation,
        };
        assert_eq!(
            spool.set_len_controlled_v1(TRUNCATED_BYTES, &mut control),
            Err(expected),
            "{case}"
        );
        assert_eq!(spool.logical_len_for_test_v1(), TRUNCATED_BYTES, "{case}");
        let preparation_entry = fs::read_dir(fixture.path().join("preparation"))
            .unwrap()
            .next()
            .expect("one operation spool")
            .unwrap();
        assert_eq!(
            preparation_entry.metadata().unwrap().len(),
            TRUNCATED_BYTES,
            "{case}"
        );

        let cleanup = spool
            .cleanup_controlled_v1(&mut continue_control)
            .expect_err("the corrupted accounting state must fail closed during cleanup");
        assert_eq!(
            cleanup,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::Integrity,
                dominant: FsCasFailureCauseV1::CleanupFailed(
                    FsCasCleanupTargetV1::PreparationSpool,
                ),
            },
            "{case}"
        );
        assert_eq!(
            spool.cleanup_controlled_v1(&mut continue_control),
            Err(cleanup),
            "{case}: cleanup retry changed the terminal"
        );
        drop(spool);

        capability
            .finish_terminal_v1(false, &mut counters, &mut continue_control)
            .unwrap();
        assert_eq!(cas.operation_admitted_slots_v1(), 0, "{case}");
        assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{case}");
        assert_eq!(
            cas.storage_admission_active_for_test_v1(),
            (0, 0, 0),
            "{case}"
        );
        assert_storage_equations(&counters);
        let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
            exact_operation_namespace_usage(fixture.path());
        assert_eq!(
            (preparation_bytes, preparation_inodes),
            (TRUNCATED_BYTES, 1),
            "{case}"
        );
        assert_eq!((immutable_bytes, immutable_inodes), (0, 0), "{case}");
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert!(counters.has_zero_forbidden_work(), "{case}");
        assert!(
            matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        assert!(
            matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        match FsCasV1::open_existing(fixture.path()) {
            Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
            Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
            Ok(_) => panic!("{case}: damaged root reopened as usable"),
        }
    }
}

#[test]
fn operation_spool_write_observation_overflow_is_typed_transactional_and_cleanable() {
    const SPOOL_BYTES: u64 = 1;
    let fixture = TestRoot::new("operation-spool-write-observation-overflow");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let mut counters = OperationCountersV1::default();
    let mut continue_control = ContinueControl;
    let mut capability = cas
        .begin_operation_capability_v1(
            FsOperationKindV1::CompleteC3File,
            0x8ff,
            &mut counters,
            &mut continue_control,
        )
        .unwrap();
    capability
        .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(SPOOL_BYTES, 0, 1, 0).unwrap())
        .unwrap();
    let token = capability.storage_token_v1().unwrap();
    let mut spool = cas
        .begin_operation_spool_borrowed_v1("write-observation", token, &mut continue_control)
        .unwrap();
    spool
        .initialize_zeroed_len_controlled_v1(SPOOL_BYTES, &mut continue_control)
        .unwrap();
    let mut overflow_control = OperationSpoolWriteObservationOverflowControl::default();

    assert_eq!(
        spool.write_exact_at_controlled_v1(0, &[0x5a], &mut overflow_control),
        Err(FsCasErrorV1::Core(CoreError::IntegerOverflow))
    );
    assert!(overflow_control.injected);
    assert_eq!(spool.direct_storage_observation(), (0, 0, u64::MAX));
    let preparation_entry = fs::read_dir(fixture.path().join("preparation"))
        .unwrap()
        .next()
        .expect("one operation spool")
        .unwrap();
    assert_eq!(fs::read(preparation_entry.path()).unwrap(), [0x5a]);

    spool.cleanup_controlled_v1(&mut continue_control).unwrap();
    drop(spool);
    capability
        .finish_terminal_v1(false, &mut counters, &mut continue_control)
        .unwrap();
    drop(capability);

    assert_operation_authority_baseline(&cas, fixture.path());
    assert_storage_equations(&counters);
    assert_eq!(counters.storage_bytes_requested, SPOOL_BYTES);
    assert_eq!(counters.storage_bytes_released, SPOOL_BYTES);
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_bytes_retained, 0);
    assert_eq!(counters.storage_inodes_requested, 1);
    assert_eq!(counters.storage_inodes_released, 1);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert_eq!(counters.storage_inodes_retained, 0);
    assert_eq!(
        exact_operation_namespace_usage(fixture.path()),
        ((0, 0), (0, 0))
    );
    assert!(counters.has_zero_forbidden_work());
    assert!(cas.occupied().is_ok());
    assert!(stale.occupied().is_ok());
}

#[test]
fn operation_spool_read_observation_overflow_is_typed_transactional_and_cleanable() {
    const SPOOL_BYTES: u64 = 1;
    const SEEDED_BYTES_READ: u64 = 73;
    let fixture = TestRoot::new("operation-spool-read-observation-overflow");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let mut counters = OperationCountersV1::default();
    let mut continue_control = ContinueControl;
    let mut capability = cas
        .begin_operation_capability_v1(
            FsOperationKindV1::CompleteC3File,
            0x900,
            &mut counters,
            &mut continue_control,
        )
        .unwrap();
    capability
        .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(SPOOL_BYTES, 0, 1, 0).unwrap())
        .unwrap();
    let token = capability.storage_token_v1().unwrap();
    let mut spool = cas
        .begin_operation_spool_borrowed_v1("read-observation", token, &mut continue_control)
        .unwrap();
    spool
        .initialize_zeroed_len_controlled_v1(SPOOL_BYTES, &mut continue_control)
        .unwrap();
    spool
        .write_exact_at_controlled_v1(0, &[0x5a], &mut continue_control)
        .unwrap();
    cas.seed_next_operation_spool_read_observation_for_test_v1(SEEDED_BYTES_READ, u64::MAX);
    let mut destination = [0_u8; 1];

    assert_eq!(
        spool.read_exact_at(0, &mut destination),
        Err(FsCasErrorV1::Core(CoreError::IntegerOverflow))
    );
    assert_eq!(destination, [0x5a]);
    assert_eq!(
        spool.direct_storage_observation(),
        (SEEDED_BYTES_READ, u64::MAX, SPOOL_BYTES)
    );

    spool.cleanup_controlled_v1(&mut continue_control).unwrap();
    drop(spool);
    capability
        .finish_terminal_v1(false, &mut counters, &mut continue_control)
        .unwrap();
    drop(capability);

    assert_operation_authority_baseline(&cas, fixture.path());
    assert_storage_equations(&counters);
    assert_eq!(counters.storage_bytes_requested, SPOOL_BYTES);
    assert_eq!(counters.storage_bytes_released, SPOOL_BYTES);
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_bytes_retained, 0);
    assert_eq!(counters.storage_inodes_requested, 1);
    assert_eq!(counters.storage_inodes_released, 1);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert_eq!(counters.storage_inodes_retained, 0);
    assert_eq!(
        exact_operation_namespace_usage(fixture.path()),
        ((0, 0), (0, 0))
    );
    assert!(counters.has_zero_forbidden_work());
    assert!(cas.occupied().is_ok());
    assert!(stale.occupied().is_ok());
}

#[test]
fn operation_spool_cleanup_accounting_failure_is_stable_before_and_after_unlink() {
    for (case, before_unlink, fail_invalidation, dominant) in [
        (
            "spool-cleanup-reconcile-failure",
            true,
            false,
            FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool),
        ),
        (
            "spool-cleanup-reconcile-invalidation-double-fault",
            true,
            true,
            FsCasFailureCauseV1::InvalidationFailed,
        ),
        (
            "spool-cleanup-remove-accounting-failure",
            false,
            false,
            FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool),
        ),
        (
            "spool-cleanup-remove-invalidation-double-fault",
            false,
            true,
            FsCasFailureCauseV1::InvalidationFailed,
        ),
    ] {
        const SPOOL_BYTES: u64 = 17;
        let fixture = TestRoot::new(case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let mut counters = OperationCountersV1::default();
        let mut continue_control = ContinueControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x8fc,
                &mut counters,
                &mut continue_control,
            )
            .unwrap();
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(SPOOL_BYTES, 0, 1, 0).unwrap())
            .unwrap();
        let token = capability.storage_token_v1().unwrap();
        let mut spool = cas
            .begin_operation_spool_borrowed_v1("cleanup-accounting", token, &mut continue_control)
            .unwrap();
        spool
            .initialize_zeroed_len_controlled_v1(SPOOL_BYTES, &mut continue_control)
            .unwrap();

        if before_unlink {
            cas.clear_active_preparation_bytes_for_test_v1();
        } else {
            cas.remove_active_preparation_inode_for_test_v1();
        }
        let mut control = FailRootInvalidationV1 {
            fail: fail_invalidation,
        };
        let expected = FsCasErrorV1::TerminalFailure {
            first: FsCasFailureCauseV1::Integrity,
            dominant,
        };
        assert_eq!(
            spool.cleanup_controlled_v1(&mut control),
            Err(expected),
            "{case}"
        );
        assert_eq!(
            spool.cleanup_controlled_v1(&mut control),
            Err(expected),
            "{case}: an explicit retry changed the terminal"
        );
        drop(spool);

        capability
            .finish_terminal_v1(false, &mut counters, &mut continue_control)
            .unwrap();
        assert_eq!(cas.operation_admitted_slots_v1(), 0, "{case}");
        assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{case}");
        assert_eq!(
            cas.storage_admission_active_for_test_v1(),
            (0, 0, 0),
            "{case}"
        );
        assert_storage_equations(&counters);
        let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
            exact_operation_namespace_usage(fixture.path());
        assert_eq!((immutable_bytes, immutable_inodes), (0, 0), "{case}");
        if before_unlink {
            assert_eq!(
                (preparation_bytes, preparation_inodes),
                (SPOOL_BYTES, 1),
                "{case}"
            );
            assert_eq!(counters.storage_bytes_retained, 0, "{case}");
            assert_eq!(counters.storage_inodes_retained, 1, "{case}");
        } else {
            assert_eq!((preparation_bytes, preparation_inodes), (0, 0), "{case}");
            assert_eq!(counters.storage_bytes_retained, SPOOL_BYTES, "{case}");
            assert_eq!(counters.storage_inodes_retained, 0, "{case}");
        }
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert!(counters.has_zero_forbidden_work(), "{case}");
        assert!(
            matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        assert!(
            matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        match FsCasV1::open_existing(fixture.path()) {
            Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
            Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
            Ok(_) => panic!("{case}: damaged root reopened as usable"),
        }
    }
}

#[cfg(unix)]
#[test]
fn operation_spool_cleanup_metadata_failure_preserves_first_cause_and_stable_custody() {
    use std::os::unix::fs::PermissionsExt;

    for (fault_name, mode, first) in [
        (
            "wrong-type",
            PreparationMetadataFaultModeV1::WrongType,
            FsCasFailureCauseV1::Integrity,
        ),
        (
            "required-name-missing",
            PreparationMetadataFaultModeV1::Missing,
            FsCasFailureCauseV1::MissingOccupant,
        ),
        (
            "permission",
            PreparationMetadataFaultModeV1::PermissionDenied,
            FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
        ),
        (
            "read-failure",
            PreparationMetadataFaultModeV1::ReadFailure,
            FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::ReadFailure),
        ),
    ] {
        for fail_invalidation in [false, true] {
            const SPOOL_BYTES: u64 = 19;
            let case = format!(
                "spool-cleanup-metadata-{fault_name}{}",
                if fail_invalidation {
                    "-invalidation-double-fault"
                } else {
                    ""
                }
            );
            let fixture = TestRoot::new(&case);
            let cas = FsCasV1::create_new(fixture.path()).unwrap();
            let stale = FsCasV1::open_existing(fixture.path()).unwrap();
            let mut counters = OperationCountersV1::default();
            let mut continue_control = ContinueControl;
            let mut capability = cas
                .begin_operation_capability_v1(
                    FsOperationKindV1::CompleteC3File,
                    0x8f7,
                    &mut counters,
                    &mut continue_control,
                )
                .unwrap();
            capability
                .declare_storage_envelope_v1(
                    FsStorageEnvelopeV1::new(SPOOL_BYTES, 0, 1, 0).unwrap(),
                )
                .unwrap();
            let token = capability.storage_token_v1().unwrap();
            let mut spool = cas
                .begin_operation_spool_borrowed_v1("cleanup-metadata", token, &mut continue_control)
                .unwrap();
            spool
                .initialize_zeroed_len_controlled_v1(SPOOL_BYTES, &mut continue_control)
                .unwrap();

            let preparation = fixture.path().join("preparation");
            let spool_path = fs::read_dir(&preparation)
                .unwrap()
                .next()
                .expect("one operation spool")
                .unwrap()
                .path();
            let held_preparation = fixture.path().join("preparation-held-for-read-failure");
            match mode {
                PreparationMetadataFaultModeV1::WrongType => {
                    fs::remove_file(&spool_path).unwrap();
                    fs::create_dir(&spool_path).unwrap();
                }
                PreparationMetadataFaultModeV1::Missing => {
                    fs::remove_file(&spool_path).unwrap();
                }
                PreparationMetadataFaultModeV1::PermissionDenied => {
                    fs::set_permissions(&preparation, fs::Permissions::from_mode(0o000)).unwrap();
                }
                PreparationMetadataFaultModeV1::ReadFailure => {
                    fs::rename(&preparation, &held_preparation).unwrap();
                    fs::write(&preparation, b"not-a-directory").unwrap();
                }
            }
            let mut control = RestorePreparationMetadataAuthorityV1 {
                preparation: preparation.clone(),
                held_preparation,
                mode,
                restored: false,
                fail_invalidation,
            };
            let expected = FsCasErrorV1::TerminalFailure {
                first,
                dominant: if fail_invalidation {
                    FsCasFailureCauseV1::InvalidationFailed
                } else {
                    FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool)
                },
            };

            assert_eq!(
                spool.cleanup_controlled_v1(&mut control),
                Err(expected),
                "{case}"
            );
            assert_eq!(
                spool.cleanup_controlled_v1(&mut control),
                Err(expected),
                "{case}: an explicit retry changed the terminal"
            );
            drop(spool);

            capability
                .finish_terminal_v1(false, &mut counters, &mut continue_control)
                .unwrap();
            drop(capability);
            assert_eq!(cas.operation_admitted_slots_v1(), 0, "{case}");
            assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{case}");
            assert_eq!(
                cas.storage_admission_active_for_test_v1(),
                (0, 0, 0),
                "{case}"
            );
            assert_storage_equations(&counters);
            assert_eq!(counters.storage_bytes_requested, SPOOL_BYTES, "{case}");
            assert_eq!(counters.storage_inodes_requested, 1, "{case}");
            assert_eq!(counters.storage_bytes_committed, 0, "{case}");
            assert_eq!(counters.storage_inodes_committed, 0, "{case}");
            assert_eq!(counters.storage_bytes_retained, SPOOL_BYTES, "{case}");
            assert_eq!(counters.storage_inodes_retained, 1, "{case}");
            assert_eq!(
                fs::read_dir(&preparation).unwrap().count(),
                usize::from(!matches!(mode, PreparationMetadataFaultModeV1::Missing)),
                "{case}"
            );
            match mode {
                PreparationMetadataFaultModeV1::WrongType => assert!(
                    fs::symlink_metadata(&spool_path)
                        .unwrap()
                        .file_type()
                        .is_dir(),
                    "{case}"
                ),
                PreparationMetadataFaultModeV1::Missing => assert_eq!(
                    fs::symlink_metadata(&spool_path).unwrap_err().kind(),
                    std::io::ErrorKind::NotFound,
                    "{case}"
                ),
                PreparationMetadataFaultModeV1::PermissionDenied
                | PreparationMetadataFaultModeV1::ReadFailure => assert_eq!(
                    fs::metadata(&spool_path).unwrap().len(),
                    SPOOL_BYTES,
                    "{case}"
                ),
            }
            assert!(counters.has_zero_forbidden_work(), "{case}");
            assert!(
                matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
                "{case}"
            );
            assert!(
                matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
                "{case}"
            );
            match FsCasV1::open_existing(fixture.path()) {
                Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
                Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
                Ok(_) => panic!("{case}: damaged root reopened as usable"),
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn operation_spool_drop_never_substitutes_a_failed_metadata_observation() {
    use std::os::unix::fs::PermissionsExt;

    for (fault_name, mode) in [
        ("clean-observed-file", None),
        (
            "wrong-type",
            Some(PreparationMetadataFaultModeV1::WrongType),
        ),
        (
            "required-name-missing",
            Some(PreparationMetadataFaultModeV1::Missing),
        ),
        (
            "permission",
            Some(PreparationMetadataFaultModeV1::PermissionDenied),
        ),
        (
            "read-failure",
            Some(PreparationMetadataFaultModeV1::ReadFailure),
        ),
    ] {
        const LOGICAL_BYTES: u64 = 23;
        const PHYSICAL_BYTES: u64 = 7;
        let case = format!("spool-drop-metadata-{fault_name}");
        let fixture = TestRoot::new(&case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let mut counters = OperationCountersV1::default();
        let mut control = ContinueControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x907,
                &mut counters,
                &mut control,
            )
            .unwrap();
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(LOGICAL_BYTES, 0, 1, 0).unwrap())
            .unwrap();
        let token = capability.storage_token_v1().unwrap();
        let mut spool = cas
            .begin_operation_spool_borrowed_v1("drop-metadata", token, &mut control)
            .unwrap();
        spool
            .initialize_zeroed_len_controlled_v1(LOGICAL_BYTES, &mut control)
            .unwrap();

        let preparation = fixture.path().join("preparation");
        let spool_path = fs::read_dir(&preparation)
            .unwrap()
            .next()
            .expect("one operation spool")
            .unwrap()
            .path();
        fs::OpenOptions::new()
            .write(true)
            .open(&spool_path)
            .unwrap()
            .set_len(PHYSICAL_BYTES)
            .unwrap();
        let held_preparation = fixture.path().join("preparation-held-for-drop-read");
        match mode {
            None => {}
            Some(PreparationMetadataFaultModeV1::WrongType) => {
                fs::remove_file(&spool_path).unwrap();
                fs::create_dir(&spool_path).unwrap();
            }
            Some(PreparationMetadataFaultModeV1::Missing) => {
                fs::remove_file(&spool_path).unwrap();
            }
            Some(PreparationMetadataFaultModeV1::PermissionDenied) => {
                fs::set_permissions(&preparation, fs::Permissions::from_mode(0o000)).unwrap();
            }
            Some(PreparationMetadataFaultModeV1::ReadFailure) => {
                fs::rename(&preparation, &held_preparation).unwrap();
                fs::write(&preparation, b"not-a-directory").unwrap();
            }
        }

        // Exercise only the result-less backstop.  No explicit cleanup call
        // may classify or repair this terminal for Drop.
        drop(spool);
        match mode {
            Some(PreparationMetadataFaultModeV1::PermissionDenied) => {
                fs::set_permissions(&preparation, fs::Permissions::from_mode(0o700)).unwrap();
            }
            Some(PreparationMetadataFaultModeV1::ReadFailure) => {
                fs::remove_file(&preparation).unwrap();
                fs::rename(&held_preparation, &preparation).unwrap();
            }
            None
            | Some(PreparationMetadataFaultModeV1::WrongType)
            | Some(PreparationMetadataFaultModeV1::Missing) => {}
        }

        capability
            .finish_terminal_v1(false, &mut counters, &mut control)
            .unwrap();
        drop(capability);
        assert_eq!(cas.operation_admitted_slots_v1(), 0, "{case}");
        assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{case}");
        assert_eq!(
            cas.storage_admission_active_for_test_v1(),
            (0, 0, 0),
            "{case}"
        );
        assert_storage_equations(&counters);
        assert_eq!(counters.storage_bytes_requested, LOGICAL_BYTES, "{case}");
        assert_eq!(counters.storage_inodes_requested, 1, "{case}");
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert_eq!(counters.unreachable_installed_residue_bytes, 0, "{case}");
        for immutable in ["carriers", "objects", "catalog", "closures"] {
            assert_eq!(
                fs::read_dir(fixture.path().join(immutable))
                    .unwrap()
                    .count(),
                0,
                "{case}"
            );
        }

        if let Some(mode) = mode {
            // The unknown observation keeps the original logical charge.  It
            // is never reconciled to the externally changed physical length.
            assert_eq!(counters.storage_bytes_retained, LOGICAL_BYTES, "{case}");
            assert_eq!(counters.storage_inodes_retained, 1, "{case}");
            assert_eq!(
                counters.mutable_preparation_residue_bytes, LOGICAL_BYTES,
                "{case}"
            );
            assert_eq!(counters.mutable_preparation_residue_inodes, 1, "{case}");
            match mode {
                PreparationMetadataFaultModeV1::WrongType => {
                    assert_eq!(fs::read_dir(&preparation).unwrap().count(), 1, "{case}");
                    assert!(
                        fs::symlink_metadata(&spool_path)
                            .unwrap()
                            .file_type()
                            .is_dir(),
                        "{case}"
                    );
                }
                PreparationMetadataFaultModeV1::Missing => {
                    assert_eq!(fs::read_dir(&preparation).unwrap().count(), 0, "{case}");
                    assert_eq!(
                        fs::symlink_metadata(&spool_path).unwrap_err().kind(),
                        std::io::ErrorKind::NotFound,
                        "{case}"
                    );
                }
                PreparationMetadataFaultModeV1::PermissionDenied
                | PreparationMetadataFaultModeV1::ReadFailure => {
                    assert_eq!(fs::read_dir(&preparation).unwrap().count(), 1, "{case}");
                    assert_eq!(
                        fs::metadata(&spool_path).unwrap().len(),
                        PHYSICAL_BYTES,
                        "{case}"
                    );
                }
            }
            assert!(
                matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
                "{case}"
            );
            assert!(
                matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
                "{case}"
            );
            assert!(matches!(
                FsCasV1::open_existing(fixture.path()),
                Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy)
            ));
        } else {
            assert_eq!(counters.storage_bytes_released, LOGICAL_BYTES, "{case}");
            assert_eq!(counters.storage_inodes_released, 1, "{case}");
            assert_eq!(counters.storage_bytes_retained, 0, "{case}");
            assert_eq!(counters.storage_inodes_retained, 0, "{case}");
            assert_eq!(counters.mutable_preparation_residue_bytes, 0, "{case}");
            assert_eq!(counters.mutable_preparation_residue_inodes, 0, "{case}");
            assert_operation_authority_baseline(&cas, fixture.path());
            assert!(cas.occupied().is_ok(), "{case}");
            assert!(stale.occupied().is_ok(), "{case}");
        }
        assert!(counters.has_zero_forbidden_work(), "{case}");
    }
}

#[cfg(unix)]
#[test]
fn operation_spool_unlink_failure_preserves_directional_cause_and_stable_custody() {
    for (fault_name, mode, first) in [
        (
            "required-name-missing",
            PreparationUnlinkFaultModeV1::Missing,
            Some(FsCasFailureCauseV1::MissingOccupant),
        ),
        (
            "permission",
            PreparationUnlinkFaultModeV1::PermissionDenied,
            Some(FsCasFailureCauseV1::Filesystem(
                FsCasFilesystemFailureV1::PermissionDenied,
            )),
        ),
        (
            "write-failure",
            PreparationUnlinkFaultModeV1::WriteFailure,
            Some(FsCasFailureCauseV1::Filesystem(
                FsCasFilesystemFailureV1::WriteFailure,
            )),
        ),
        ("injected", PreparationUnlinkFaultModeV1::Injected, None),
    ] {
        for fail_invalidation in [false, true] {
            const SPOOL_BYTES: u64 = 23;
            let case = format!(
                "spool-cleanup-unlink-{fault_name}{}",
                if fail_invalidation {
                    "-invalidation-double-fault"
                } else {
                    ""
                }
            );
            let fixture = TestRoot::new(&case);
            let cas = FsCasV1::create_new(fixture.path()).unwrap();
            let stale = FsCasV1::open_existing(fixture.path()).unwrap();
            let mut counters = OperationCountersV1::default();
            let mut continue_control = ContinueControl;
            let mut capability = cas
                .begin_operation_capability_v1(
                    FsOperationKindV1::CompleteC3File,
                    0x8f8,
                    &mut counters,
                    &mut continue_control,
                )
                .unwrap();
            capability
                .declare_storage_envelope_v1(
                    FsStorageEnvelopeV1::new(SPOOL_BYTES, 0, 1, 0).unwrap(),
                )
                .unwrap();
            let token = capability.storage_token_v1().unwrap();
            let mut spool = cas
                .begin_operation_spool_borrowed_v1("cleanup-unlink", token, &mut continue_control)
                .unwrap();
            spool
                .initialize_zeroed_len_controlled_v1(SPOOL_BYTES, &mut continue_control)
                .unwrap();

            let preparation = fixture.path().join("preparation");
            let spool_path = fs::read_dir(&preparation)
                .unwrap()
                .next()
                .expect("one operation spool")
                .unwrap()
                .path();
            let mut control = FailPreparationUnlinkV1 {
                preparation: preparation.clone(),
                held_preparation: fixture.path().join("preparation-held-for-unlink"),
                spool_path: spool_path.clone(),
                mode,
                target: FsCasCleanupTargetV1::PreparationSpool,
                armed: false,
                restored: false,
                fail_invalidation,
            };
            let cleanup =
                FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool);
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
                FsCasErrorV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool)
            };

            assert_eq!(
                spool.cleanup_controlled_v1(&mut control),
                Err(expected),
                "{case}"
            );
            assert_eq!(
                spool.cleanup_controlled_v1(&mut control),
                Err(expected),
                "{case}: an explicit retry changed the terminal"
            );
            drop(spool);

            capability
                .finish_terminal_v1(false, &mut counters, &mut continue_control)
                .unwrap();
            drop(capability);
            assert_eq!(cas.operation_admitted_slots_v1(), 0, "{case}");
            assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{case}");
            assert_eq!(
                cas.storage_admission_active_for_test_v1(),
                (0, 0, 0),
                "{case}"
            );
            assert_storage_equations(&counters);
            assert_eq!(counters.storage_bytes_requested, SPOOL_BYTES, "{case}");
            assert_eq!(counters.storage_inodes_requested, 1, "{case}");
            assert_eq!(counters.storage_bytes_committed, 0, "{case}");
            assert_eq!(counters.storage_inodes_committed, 0, "{case}");
            assert_eq!(counters.storage_bytes_retained, SPOOL_BYTES, "{case}");
            assert_eq!(counters.storage_inodes_retained, 1, "{case}");
            if matches!(mode, PreparationUnlinkFaultModeV1::Missing) {
                assert_eq!(
                    fs::symlink_metadata(&spool_path).unwrap_err().kind(),
                    std::io::ErrorKind::NotFound,
                    "{case}"
                );
                assert_eq!(fs::read_dir(&preparation).unwrap().count(), 0, "{case}");
            } else {
                assert_eq!(
                    fs::metadata(&spool_path).unwrap().len(),
                    SPOOL_BYTES,
                    "{case}"
                );
                assert_eq!(fs::read_dir(&preparation).unwrap().count(), 1, "{case}");
            }
            assert!(counters.has_zero_forbidden_work(), "{case}");
            assert!(
                matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
                "{case}"
            );
            assert!(
                matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
                "{case}"
            );
            match FsCasV1::open_existing(fixture.path()) {
                Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
                Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
                Ok(_) => panic!("{case}: damaged root reopened as usable"),
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn private_pack_cleanup_metadata_failure_preserves_first_cause_and_stable_custody() {
    use std::os::unix::fs::PermissionsExt;

    for (fault_name, mode, first) in [
        (
            "wrong-type",
            PreparationMetadataFaultModeV1::WrongType,
            FsCasFailureCauseV1::Integrity,
        ),
        (
            "required-name-missing",
            PreparationMetadataFaultModeV1::Missing,
            FsCasFailureCauseV1::MissingOccupant,
        ),
        (
            "permission",
            PreparationMetadataFaultModeV1::PermissionDenied,
            FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied),
        ),
        (
            "read-failure",
            PreparationMetadataFaultModeV1::ReadFailure,
            FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::ReadFailure),
        ),
    ] {
        for fail_invalidation in [false, true] {
            const PACK_CEILING: u64 = 128;
            let case = format!(
                "private-cleanup-metadata-{fault_name}{}",
                if fail_invalidation {
                    "-invalidation-double-fault"
                } else {
                    ""
                }
            );
            let fixture = TestRoot::new(&case);
            let cas = FsCasV1::create_new(fixture.path()).unwrap();
            let stale = FsCasV1::open_existing(fixture.path()).unwrap();
            let mut counters = OperationCountersV1::default();
            let mut continue_control = ContinueControl;
            let mut capability = cas
                .begin_operation_capability_v1(
                    FsOperationKindV1::CompleteC3File,
                    0x8f9,
                    &mut counters,
                    &mut continue_control,
                )
                .unwrap();
            capability
                .declare_storage_envelope_v1(
                    FsStorageEnvelopeV1::new(PACK_CEILING, 0, 1, 0).unwrap(),
                )
                .unwrap();
            let token = capability.storage_token_v1().unwrap();
            let mut private_pack = cas.begin_private_pack_borrowed_v1(token).unwrap();
            private_pack
                .begin_direct_controlled_v1(PACK_CEILING, &mut continue_control)
                .unwrap();

            let preparation = fixture.path().join("preparation");
            let pack_path = fs::read_dir(&preparation)
                .unwrap()
                .next()
                .expect("one private pack")
                .unwrap()
                .path();
            let held_preparation = fixture.path().join("preparation-held-for-private-read");
            match mode {
                PreparationMetadataFaultModeV1::WrongType => {
                    fs::remove_file(&pack_path).unwrap();
                    fs::create_dir(&pack_path).unwrap();
                }
                PreparationMetadataFaultModeV1::Missing => {
                    fs::remove_file(&pack_path).unwrap();
                }
                PreparationMetadataFaultModeV1::PermissionDenied => {
                    fs::set_permissions(&preparation, fs::Permissions::from_mode(0o000)).unwrap();
                }
                PreparationMetadataFaultModeV1::ReadFailure => {
                    fs::rename(&preparation, &held_preparation).unwrap();
                    fs::write(&preparation, b"not-a-directory").unwrap();
                }
            }
            let mut control = RestorePreparationMetadataAuthorityV1 {
                preparation: preparation.clone(),
                held_preparation,
                mode,
                restored: false,
                fail_invalidation,
            };
            let expected = FsCasErrorV1::TerminalFailure {
                first,
                dominant: if fail_invalidation {
                    FsCasFailureCauseV1::InvalidationFailed
                } else {
                    FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PrivatePack)
                },
            };

            assert_eq!(
                private_pack.cleanup_controlled_v1(&mut control),
                Err(expected),
                "{case}"
            );
            assert_eq!(
                private_pack.cleanup_controlled_v1(&mut control),
                Err(expected),
                "{case}: an explicit retry changed the terminal"
            );
            drop(private_pack);

            capability
                .finish_terminal_v1(false, &mut counters, &mut continue_control)
                .unwrap();
            drop(capability);
            assert_eq!(cas.operation_admitted_slots_v1(), 0, "{case}");
            assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{case}");
            assert_eq!(
                cas.storage_admission_active_for_test_v1(),
                (0, 0, 0),
                "{case}"
            );
            assert_storage_equations(&counters);
            assert_eq!(counters.storage_bytes_requested, PACK_CEILING, "{case}");
            assert_eq!(counters.storage_inodes_requested, 1, "{case}");
            assert_eq!(counters.storage_bytes_committed, 0, "{case}");
            assert_eq!(counters.storage_inodes_committed, 0, "{case}");
            assert_eq!(counters.storage_bytes_retained, PACK_HEADER_BYTES, "{case}");
            assert_eq!(counters.storage_inodes_retained, 1, "{case}");
            assert_eq!(
                fs::read_dir(&preparation).unwrap().count(),
                usize::from(!matches!(mode, PreparationMetadataFaultModeV1::Missing)),
                "{case}"
            );
            match mode {
                PreparationMetadataFaultModeV1::WrongType => assert!(
                    fs::symlink_metadata(&pack_path)
                        .unwrap()
                        .file_type()
                        .is_dir(),
                    "{case}"
                ),
                PreparationMetadataFaultModeV1::Missing => assert_eq!(
                    fs::symlink_metadata(&pack_path).unwrap_err().kind(),
                    std::io::ErrorKind::NotFound,
                    "{case}"
                ),
                PreparationMetadataFaultModeV1::PermissionDenied
                | PreparationMetadataFaultModeV1::ReadFailure => assert_eq!(
                    fs::metadata(&pack_path).unwrap().len(),
                    PACK_HEADER_BYTES,
                    "{case}"
                ),
            }
            for immutable in ["carriers", "objects", "catalog", "closures"] {
                assert_eq!(
                    fs::read_dir(fixture.path().join(immutable))
                        .unwrap()
                        .count(),
                    0,
                    "{case}"
                );
            }
            assert!(counters.has_zero_forbidden_work(), "{case}");
            assert!(
                matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
                "{case}"
            );
            assert!(
                matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
                "{case}"
            );
            match FsCasV1::open_existing(fixture.path()) {
                Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
                Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
                Ok(_) => panic!("{case}: damaged root reopened as usable"),
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn private_pack_drop_never_substitutes_a_failed_metadata_observation() {
    use std::os::unix::fs::PermissionsExt;

    for (fault_name, mode) in [
        ("clean-observed-file", None),
        (
            "wrong-type",
            Some(PreparationMetadataFaultModeV1::WrongType),
        ),
        (
            "required-name-missing",
            Some(PreparationMetadataFaultModeV1::Missing),
        ),
        (
            "permission",
            Some(PreparationMetadataFaultModeV1::PermissionDenied),
        ),
        (
            "read-failure",
            Some(PreparationMetadataFaultModeV1::ReadFailure),
        ),
    ] {
        const PACK_CEILING: u64 = 128;
        const PHYSICAL_BYTES: u64 = 7;
        let case = format!("private-drop-metadata-{fault_name}");
        let fixture = TestRoot::new(&case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let mut counters = OperationCountersV1::default();
        let mut control = ContinueControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x909,
                &mut counters,
                &mut control,
            )
            .unwrap();
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(PACK_CEILING, 0, 1, 0).unwrap())
            .unwrap();
        let token = capability.storage_token_v1().unwrap();
        let mut private_pack = cas.begin_private_pack_borrowed_v1(token).unwrap();
        private_pack
            .begin_direct_controlled_v1(PACK_CEILING, &mut control)
            .unwrap();

        let preparation = fixture.path().join("preparation");
        let pack_path = fs::read_dir(&preparation)
            .unwrap()
            .next()
            .expect("one private pack")
            .unwrap()
            .path();
        fs::OpenOptions::new()
            .write(true)
            .open(&pack_path)
            .unwrap()
            .set_len(PHYSICAL_BYTES)
            .unwrap();
        let held_preparation = fixture.path().join("preparation-held-for-private-drop");
        match mode {
            None => {}
            Some(PreparationMetadataFaultModeV1::WrongType) => {
                fs::remove_file(&pack_path).unwrap();
                fs::create_dir(&pack_path).unwrap();
            }
            Some(PreparationMetadataFaultModeV1::Missing) => {
                fs::remove_file(&pack_path).unwrap();
            }
            Some(PreparationMetadataFaultModeV1::PermissionDenied) => {
                fs::set_permissions(&preparation, fs::Permissions::from_mode(0o000)).unwrap();
            }
            Some(PreparationMetadataFaultModeV1::ReadFailure) => {
                fs::rename(&preparation, &held_preparation).unwrap();
                fs::write(&preparation, b"not-a-directory").unwrap();
            }
        }

        drop(private_pack);
        match mode {
            Some(PreparationMetadataFaultModeV1::PermissionDenied) => {
                fs::set_permissions(&preparation, fs::Permissions::from_mode(0o700)).unwrap();
            }
            Some(PreparationMetadataFaultModeV1::ReadFailure) => {
                fs::remove_file(&preparation).unwrap();
                fs::rename(&held_preparation, &preparation).unwrap();
            }
            None
            | Some(PreparationMetadataFaultModeV1::WrongType)
            | Some(PreparationMetadataFaultModeV1::Missing) => {}
        }

        capability
            .finish_terminal_v1(false, &mut counters, &mut control)
            .unwrap();
        drop(capability);
        assert_eq!(cas.operation_admitted_slots_v1(), 0, "{case}");
        assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{case}");
        assert_eq!(
            cas.storage_admission_active_for_test_v1(),
            (0, 0, 0),
            "{case}"
        );
        assert_storage_equations(&counters);
        assert_eq!(counters.storage_bytes_requested, PACK_CEILING, "{case}");
        assert_eq!(counters.storage_inodes_requested, 1, "{case}");
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert_eq!(counters.unreachable_installed_residue_bytes, 0, "{case}");
        for immutable in ["carriers", "objects", "catalog", "closures"] {
            assert_eq!(
                fs::read_dir(fixture.path().join(immutable))
                    .unwrap()
                    .count(),
                0,
                "{case}"
            );
        }

        if let Some(mode) = mode {
            assert_eq!(counters.storage_bytes_retained, PACK_HEADER_BYTES, "{case}");
            assert_eq!(counters.storage_inodes_retained, 1, "{case}");
            assert_eq!(
                counters.mutable_preparation_residue_bytes, PACK_HEADER_BYTES,
                "{case}"
            );
            assert_eq!(counters.mutable_preparation_residue_inodes, 1, "{case}");
            match mode {
                PreparationMetadataFaultModeV1::WrongType => {
                    assert_eq!(fs::read_dir(&preparation).unwrap().count(), 1, "{case}");
                    assert!(
                        fs::symlink_metadata(&pack_path)
                            .unwrap()
                            .file_type()
                            .is_dir(),
                        "{case}"
                    );
                }
                PreparationMetadataFaultModeV1::Missing => {
                    assert_eq!(fs::read_dir(&preparation).unwrap().count(), 0, "{case}");
                    assert_eq!(
                        fs::symlink_metadata(&pack_path).unwrap_err().kind(),
                        std::io::ErrorKind::NotFound,
                        "{case}"
                    );
                }
                PreparationMetadataFaultModeV1::PermissionDenied
                | PreparationMetadataFaultModeV1::ReadFailure => {
                    assert_eq!(fs::read_dir(&preparation).unwrap().count(), 1, "{case}");
                    assert_eq!(
                        fs::metadata(&pack_path).unwrap().len(),
                        PHYSICAL_BYTES,
                        "{case}"
                    );
                }
            }
            assert!(
                matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
                "{case}"
            );
            assert!(
                matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
                "{case}"
            );
            assert!(matches!(
                FsCasV1::open_existing(fixture.path()),
                Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy)
            ));
        } else {
            assert_eq!(counters.storage_bytes_released, PACK_CEILING, "{case}");
            assert_eq!(counters.storage_inodes_released, 1, "{case}");
            assert_eq!(counters.storage_bytes_retained, 0, "{case}");
            assert_eq!(counters.storage_inodes_retained, 0, "{case}");
            assert_eq!(counters.mutable_preparation_residue_bytes, 0, "{case}");
            assert_eq!(counters.mutable_preparation_residue_inodes, 0, "{case}");
            assert_operation_authority_baseline(&cas, fixture.path());
            assert!(cas.occupied().is_ok(), "{case}");
            assert!(stale.occupied().is_ok(), "{case}");
        }
        assert!(counters.has_zero_forbidden_work(), "{case}");
    }
}

#[cfg(unix)]
#[test]
fn private_pack_unlink_failure_preserves_directional_cause_and_stable_custody() {
    for (fault_name, mode, first) in [
        (
            "required-name-missing",
            PreparationUnlinkFaultModeV1::Missing,
            Some(FsCasFailureCauseV1::MissingOccupant),
        ),
        (
            "permission",
            PreparationUnlinkFaultModeV1::PermissionDenied,
            Some(FsCasFailureCauseV1::Filesystem(
                FsCasFilesystemFailureV1::PermissionDenied,
            )),
        ),
        (
            "write-failure",
            PreparationUnlinkFaultModeV1::WriteFailure,
            Some(FsCasFailureCauseV1::Filesystem(
                FsCasFilesystemFailureV1::WriteFailure,
            )),
        ),
        ("injected", PreparationUnlinkFaultModeV1::Injected, None),
    ] {
        for fail_invalidation in [false, true] {
            const PACK_CEILING: u64 = 128;
            let case = format!(
                "private-cleanup-unlink-{fault_name}{}",
                if fail_invalidation {
                    "-invalidation-double-fault"
                } else {
                    ""
                }
            );
            let fixture = TestRoot::new(&case);
            let cas = FsCasV1::create_new(fixture.path()).unwrap();
            let stale = FsCasV1::open_existing(fixture.path()).unwrap();
            let mut counters = OperationCountersV1::default();
            let mut continue_control = ContinueControl;
            let mut capability = cas
                .begin_operation_capability_v1(
                    FsOperationKindV1::CompleteC3File,
                    0x8fa,
                    &mut counters,
                    &mut continue_control,
                )
                .unwrap();
            capability
                .declare_storage_envelope_v1(
                    FsStorageEnvelopeV1::new(PACK_CEILING, 0, 1, 0).unwrap(),
                )
                .unwrap();
            let token = capability.storage_token_v1().unwrap();
            let mut private_pack = cas.begin_private_pack_borrowed_v1(token).unwrap();
            private_pack
                .begin_direct_controlled_v1(PACK_CEILING, &mut continue_control)
                .unwrap();

            let preparation = fixture.path().join("preparation");
            let pack_path = fs::read_dir(&preparation)
                .unwrap()
                .next()
                .expect("one private pack")
                .unwrap()
                .path();
            let mut control = FailPreparationUnlinkV1 {
                preparation: preparation.clone(),
                held_preparation: fixture.path().join("preparation-held-for-private-unlink"),
                spool_path: pack_path.clone(),
                mode,
                target: FsCasCleanupTargetV1::PrivatePack,
                armed: false,
                restored: false,
                fail_invalidation,
            };
            let cleanup = FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PrivatePack);
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
                FsCasErrorV1::CleanupFailed(FsCasCleanupTargetV1::PrivatePack)
            };

            assert_eq!(
                private_pack.cleanup_controlled_v1(&mut control),
                Err(expected),
                "{case}"
            );
            assert_eq!(
                private_pack.cleanup_controlled_v1(&mut control),
                Err(expected),
                "{case}: an explicit retry changed the terminal"
            );
            drop(private_pack);

            capability
                .finish_terminal_v1(false, &mut counters, &mut continue_control)
                .unwrap();
            drop(capability);
            assert_eq!(cas.operation_admitted_slots_v1(), 0, "{case}");
            assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{case}");
            assert_eq!(
                cas.storage_admission_active_for_test_v1(),
                (0, 0, 0),
                "{case}"
            );
            assert_storage_equations(&counters);
            assert_eq!(counters.storage_bytes_requested, PACK_CEILING, "{case}");
            assert_eq!(counters.storage_inodes_requested, 1, "{case}");
            assert_eq!(counters.storage_bytes_committed, 0, "{case}");
            assert_eq!(counters.storage_inodes_committed, 0, "{case}");
            assert_eq!(counters.storage_bytes_retained, PACK_HEADER_BYTES, "{case}");
            assert_eq!(counters.storage_inodes_retained, 1, "{case}");
            if matches!(mode, PreparationUnlinkFaultModeV1::Missing) {
                assert_eq!(
                    fs::symlink_metadata(&pack_path).unwrap_err().kind(),
                    std::io::ErrorKind::NotFound,
                    "{case}"
                );
                assert_eq!(fs::read_dir(&preparation).unwrap().count(), 0, "{case}");
            } else {
                assert_eq!(
                    fs::metadata(&pack_path).unwrap().len(),
                    PACK_HEADER_BYTES,
                    "{case}"
                );
                assert_eq!(fs::read_dir(&preparation).unwrap().count(), 1, "{case}");
            }
            for immutable in ["carriers", "objects", "catalog", "closures"] {
                assert_eq!(
                    fs::read_dir(fixture.path().join(immutable))
                        .unwrap()
                        .count(),
                    0,
                    "{case}"
                );
            }
            assert!(counters.has_zero_forbidden_work(), "{case}");
            assert!(
                matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
                "{case}"
            );
            assert!(
                matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
                "{case}"
            );
            match FsCasV1::open_existing(fixture.path()) {
                Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
                Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
                Ok(_) => panic!("{case}: damaged root reopened as usable"),
            }
        }
    }
}

#[test]
fn private_pack_truncate_and_append_accounting_failures_preserve_invalidation_cause() {
    for (case, truncate, fail_invalidation, expected) in [
        (
            "private-truncate-accounting-failure",
            true,
            false,
            FsCasErrorV1::Integrity,
        ),
        (
            "private-truncate-accounting-invalidation-double-fault",
            true,
            true,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::Integrity,
                dominant: FsCasFailureCauseV1::InvalidationFailed,
            },
        ),
        (
            "private-append-accounting-failure",
            false,
            false,
            FsCasErrorV1::Integrity,
        ),
        (
            "private-append-accounting-invalidation-double-fault",
            false,
            true,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::Integrity,
                dominant: FsCasFailureCauseV1::InvalidationFailed,
            },
        ),
    ] {
        const PACK_CEILING: u64 = 128;
        const APPEND_BYTES: u64 = 16;
        const TRUNCATED_BYTES: u64 = PACK_HEADER_BYTES + 6;
        let fixture = TestRoot::new(case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let mut counters = OperationCountersV1::default();
        let mut continue_control = ContinueControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x8ff,
                &mut counters,
                &mut continue_control,
            )
            .unwrap();
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(PACK_CEILING, 0, 1, 0).unwrap())
            .unwrap();
        let token = capability.storage_token_v1().unwrap();
        let mut private_pack = cas.begin_private_pack_borrowed_v1(token).unwrap();
        private_pack
            .begin_direct_controlled_v1(PACK_CEILING, &mut continue_control)
            .unwrap();
        if truncate {
            private_pack
                .append_controlled_v1(&[0x5a; APPEND_BYTES as usize], &mut continue_control)
                .unwrap();
        }

        cas.clear_active_preparation_bytes_for_test_v1();
        let mut control = FailRootInvalidationV1 {
            fail: fail_invalidation,
        };
        let operation = if truncate {
            private_pack.truncate_direct_controlled_v1(TRUNCATED_BYTES, &mut control)
        } else {
            private_pack.append_controlled_v1(&[0x6b; 8], &mut control)
        };
        assert!(operation.is_err(), "{case}");
        assert_eq!(
            private_pack.take_first_error_typed_v1(),
            Some(expected),
            "{case}"
        );

        let physical_bytes = if truncate {
            TRUNCATED_BYTES
        } else {
            PACK_HEADER_BYTES
        };
        let expected_accounted = if truncate {
            PACK_HEADER_BYTES + APPEND_BYTES
        } else {
            PACK_HEADER_BYTES
        };
        assert_eq!(
            private_pack.direct_lengths_for_test_v1(),
            (Some(physical_bytes), expected_accounted),
            "{case}"
        );
        let preparation_entry = fs::read_dir(fixture.path().join("preparation"))
            .unwrap()
            .next()
            .expect("one private pack")
            .unwrap();
        assert_eq!(
            preparation_entry.metadata().unwrap().len(),
            physical_bytes,
            "{case}"
        );

        let cleanup = private_pack
            .cleanup_controlled_v1(&mut continue_control)
            .expect_err("the corrupted accounting state must fail closed during cleanup");
        assert_eq!(
            cleanup,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::Integrity,
                dominant: FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PrivatePack,),
            },
            "{case}"
        );
        assert_eq!(
            private_pack.cleanup_controlled_v1(&mut continue_control),
            Err(cleanup),
            "{case}: cleanup retry changed the terminal"
        );
        drop(private_pack);

        capability
            .finish_terminal_v1(false, &mut counters, &mut continue_control)
            .unwrap();
        assert_eq!(cas.operation_admitted_slots_v1(), 0, "{case}");
        assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{case}");
        assert_eq!(
            cas.storage_admission_active_for_test_v1(),
            (0, 0, 0),
            "{case}"
        );
        assert_storage_equations(&counters);
        let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
            exact_operation_namespace_usage(fixture.path());
        assert_eq!(
            (preparation_bytes, preparation_inodes),
            (physical_bytes, 1),
            "{case}"
        );
        assert_eq!((immutable_bytes, immutable_inodes), (0, 0), "{case}");
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert!(counters.has_zero_forbidden_work(), "{case}");
        assert!(
            matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        assert!(
            matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        match FsCasV1::open_existing(fixture.path()) {
            Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
            Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
            Ok(_) => panic!("{case}: damaged root reopened as usable"),
        }
    }
}

#[test]
fn private_pack_cleanup_accounting_failure_is_stable_before_and_after_unlink() {
    for (case, before_unlink, fail_invalidation, dominant) in [
        (
            "private-cleanup-reconcile-failure",
            true,
            false,
            FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PrivatePack),
        ),
        (
            "private-cleanup-reconcile-invalidation-double-fault",
            true,
            true,
            FsCasFailureCauseV1::InvalidationFailed,
        ),
        (
            "private-cleanup-remove-accounting-failure",
            false,
            false,
            FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PrivatePack),
        ),
        (
            "private-cleanup-remove-invalidation-double-fault",
            false,
            true,
            FsCasFailureCauseV1::InvalidationFailed,
        ),
    ] {
        const PACK_BYTES: u64 = 128;
        let fixture = TestRoot::new(case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let mut counters = OperationCountersV1::default();
        let mut continue_control = ContinueControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x8fd,
                &mut counters,
                &mut continue_control,
            )
            .unwrap();
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(PACK_BYTES, 0, 1, 0).unwrap())
            .unwrap();
        let token = capability.storage_token_v1().unwrap();
        let mut private_pack = cas.begin_private_pack_borrowed_v1(token).unwrap();
        private_pack
            .begin_direct_controlled_v1(PACK_BYTES, &mut continue_control)
            .unwrap();

        if before_unlink {
            cas.clear_active_preparation_bytes_for_test_v1();
        } else {
            cas.remove_active_preparation_inode_for_test_v1();
        }
        let mut control = FailRootInvalidationV1 {
            fail: fail_invalidation,
        };
        let expected = FsCasErrorV1::TerminalFailure {
            first: FsCasFailureCauseV1::Integrity,
            dominant,
        };
        assert_eq!(
            private_pack.cleanup_controlled_v1(&mut control),
            Err(expected),
            "{case}"
        );
        assert_eq!(
            private_pack.cleanup_controlled_v1(&mut control),
            Err(expected),
            "{case}: an explicit retry changed the terminal"
        );
        drop(private_pack);

        capability
            .finish_terminal_v1(false, &mut counters, &mut continue_control)
            .unwrap();
        assert_eq!(cas.operation_admitted_slots_v1(), 0, "{case}");
        assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{case}");
        assert_eq!(
            cas.storage_admission_active_for_test_v1(),
            (0, 0, 0),
            "{case}"
        );
        assert_storage_equations(&counters);
        let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
            exact_operation_namespace_usage(fixture.path());
        assert_eq!((immutable_bytes, immutable_inodes), (0, 0), "{case}");
        if before_unlink {
            assert_eq!(
                (preparation_bytes, preparation_inodes),
                (PACK_HEADER_BYTES, 1),
                "{case}"
            );
            assert_eq!(counters.storage_bytes_retained, 0, "{case}");
            assert_eq!(counters.storage_inodes_retained, 1, "{case}");
        } else {
            assert_eq!((preparation_bytes, preparation_inodes), (0, 0), "{case}");
            assert_eq!(counters.storage_bytes_retained, PACK_HEADER_BYTES, "{case}");
            assert_eq!(counters.storage_inodes_retained, 0, "{case}");
        }
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert!(counters.has_zero_forbidden_work(), "{case}");
        assert!(
            matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        assert!(
            matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        match FsCasV1::open_existing(fixture.path()) {
            Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
            Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
            Ok(_) => panic!("{case}: damaged root reopened as usable"),
        }
    }
}

#[test]
fn marker_write_failure_survives_pre_link_cleanup_failure() {
    let fixture = TestRoot::new("marker-write-cleanup-dual-cause");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let bound_invoked = AtomicBool::new(false);
    let supply_invoked = AtomicBool::new(false);
    let mut control = FailMarkerWriteAndCleanupV1::default();
    let (result, counters) = run_small_create_with_callback_observation(
        &cas,
        0x8fe,
        &mut control,
        &bound_invoked,
        &supply_invoked,
    );

    assert_eq!(
        result,
        Err(OperationErrorV1::FsCas(FsCasErrorV1::TerminalFailure {
            first: FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::NoSpace),
            dominant: FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool,),
        }))
    );
    assert!(control.marker_write_failed);
    assert!(control.cleanup_failed);
    assert!(bound_invoked.load(Ordering::Acquire));
    assert!(supply_invoked.load(Ordering::Acquire));
    assert_storage_equations(&counters);
    let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
        exact_operation_namespace_usage(fixture.path());
    assert_eq!(preparation_inodes, 1);
    // Fault injection precedes the real write. Explicit cleanup observes the
    // empty file and reconciles the pre-accounted marker length back to zero
    // before the injected unlink failure retains its inode.
    assert_eq!(preparation_bytes, 0);
    assert_eq!((immutable_bytes, immutable_inodes), (0, 0));
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
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
    assert!(counters.has_zero_forbidden_work());
    assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(
        FsCasV1::open_existing(fixture.path()),
        Err(FsCasErrorV1::Invalidated)
    ));
}

#[test]
fn pre_link_marker_terminal_cleanup_unwind_is_typed_and_fail_closed() {
    for (case, equal_incumbent, fail_invalidation, expected) in [
        (
            "marker-write-cleanup-unwind",
            false,
            false,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::NoSpace),
                dominant: FsCasFailureCauseV1::CleanupFailed(
                    FsCasCleanupTargetV1::PreparationSpool,
                ),
            },
        ),
        (
            "marker-write-cleanup-invalidation-double-fault",
            false,
            true,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::NoSpace),
                dominant: FsCasFailureCauseV1::InvalidationFailed,
            },
        ),
        (
            "equal-marker-cleanup-unwind",
            true,
            false,
            FsCasErrorV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool),
        ),
        (
            "equal-marker-cleanup-invalidation-double-fault",
            true,
            true,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool),
                dominant: FsCasFailureCauseV1::InvalidationFailed,
            },
        ),
    ] {
        let fixture = TestRoot::new(case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let mut continue_control = ContinueControl;

        if equal_incumbent {
            let mut setup_counters = OperationCountersV1::default();
            let mut setup = cas
                .begin_operation_capability_v1(
                    FsOperationKindV1::CompleteC3File,
                    0x8ff,
                    &mut setup_counters,
                    &mut continue_control,
                )
                .unwrap();
            setup
                .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(8, 8, 1, 1).unwrap())
                .unwrap();
            let setup_token = setup.storage_token_v1().unwrap();
            cas.publish_test_marker_borrowed_v1(setup_token, &mut continue_control)
                .unwrap();
            setup
                .finish_terminal_v1(true, &mut setup_counters, &mut continue_control)
                .unwrap();
            assert_storage_equations(&setup_counters);
            assert_eq!(setup_counters.storage_bytes_committed, 8, "{case}");
            assert_eq!(setup_counters.storage_inodes_committed, 1, "{case}");
        }

        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let mut counters = OperationCountersV1::default();
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x900,
                &mut counters,
                &mut continue_control,
            )
            .unwrap();
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(8, 8, 1, 1).unwrap())
            .unwrap();
        let token = capability.storage_token_v1().unwrap();
        let mut control = PanicMarkerTerminalCleanupV1 {
            first_error: (!equal_incumbent)
                .then_some(FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::NoSpace)),
            cleanup_calls: 0,
            invalidation_calls: 0,
            fail_invalidation,
        };

        assert_eq!(
            cas.publish_test_marker_borrowed_v1(token, &mut control),
            Err(expected),
            "{case}"
        );
        assert_eq!(control.cleanup_calls, 1, "{case}");
        assert_eq!(control.invalidation_calls, 1, "{case}");

        capability
            .finish_terminal_v1(false, &mut counters, &mut continue_control)
            .unwrap();
        assert_eq!(cas.operation_admitted_slots_v1(), 0, "{case}");
        assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{case}");
        assert_eq!(
            cas.storage_admission_active_for_test_v1(),
            (0, 0, 0),
            "{case}"
        );
        assert_storage_equations(&counters);
        let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
            exact_operation_namespace_usage(fixture.path());
        assert_eq!(preparation_inodes, 1, "{case}");
        assert_eq!(preparation_bytes, u64::from(equal_incumbent) * 8, "{case}");
        assert_eq!(
            (immutable_bytes, immutable_inodes),
            if equal_incumbent { (8, 1) } else { (0, 0) },
            "{case}"
        );
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert_eq!(counters.storage_bytes_retained, preparation_bytes, "{case}");
        assert_eq!(
            counters.storage_inodes_retained, preparation_inodes,
            "{case}"
        );
        assert_eq!(
            counters.mutable_preparation_residue_bytes, preparation_bytes,
            "{case}"
        );
        assert_eq!(
            counters.mutable_preparation_residue_inodes, preparation_inodes,
            "{case}"
        );
        assert_eq!(counters.immutable_residue_bytes, 0, "{case}");
        assert_eq!(counters.immutable_residue_inodes, 0, "{case}");
        assert_eq!(counters.unreachable_installed_residue_bytes, 0, "{case}");
        assert!(counters.has_zero_forbidden_work(), "{case}");
        assert!(
            matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        assert!(
            matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        match FsCasV1::open_existing(fixture.path()) {
            Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
            Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
            Ok(_) => panic!("{case}: damaged root reopened as usable"),
        }
    }
}

#[test]
fn pre_link_marker_callback_unwind_yields_typed_cleanup_terminal() {
    for (case, secondary, fail_invalidation, expected) in [
        (
            "marker-callback-unwind-cleanup-failure",
            MarkerCleanupSecondaryV1::Failure,
            false,
            FsCasErrorV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool),
        ),
        (
            "marker-callback-unwind-cleanup-failure-invalidation-double-fault",
            MarkerCleanupSecondaryV1::Failure,
            true,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool),
                dominant: FsCasFailureCauseV1::InvalidationFailed,
            },
        ),
        (
            "marker-callback-unwind-cleanup-unwind",
            MarkerCleanupSecondaryV1::Unwind,
            false,
            FsCasErrorV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool),
        ),
        (
            "marker-callback-unwind-cleanup-unwind-invalidation-double-fault",
            MarkerCleanupSecondaryV1::Unwind,
            true,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool),
                dominant: FsCasFailureCauseV1::InvalidationFailed,
            },
        ),
    ] {
        let fixture = TestRoot::new(case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let mut counters = OperationCountersV1::default();
        let mut continue_control = ContinueControl;
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x901,
                &mut counters,
                &mut continue_control,
            )
            .unwrap();
        capability
            .declare_storage_envelope_v1(FsStorageEnvelopeV1::new(8, 8, 1, 1).unwrap())
            .unwrap();
        let token = capability.storage_token_v1().unwrap();
        let mut control = PanicMarkerPreparationWithCleanupTerminalV1 {
            secondary,
            preparation_panicked: false,
            cleanup_calls: 0,
            invalidation_calls: 0,
            fail_invalidation,
        };

        let terminal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            cas.publish_test_marker_borrowed_v1(token, &mut control)
        }));
        let terminal = match terminal {
            Ok(terminal) => terminal,
            Err(_) => panic!("{case}: initiating callback escaped after cleanup failed"),
        };
        assert_eq!(terminal, Err(expected), "{case}");
        assert!(control.preparation_panicked, "{case}");
        assert_eq!(control.cleanup_calls, 1, "{case}");
        assert_eq!(control.invalidation_calls, 1, "{case}");

        capability
            .finish_terminal_v1(false, &mut counters, &mut continue_control)
            .unwrap();
        assert_eq!(cas.operation_admitted_slots_v1(), 0, "{case}");
        assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{case}");
        assert_eq!(
            cas.storage_admission_active_for_test_v1(),
            (0, 0, 0),
            "{case}"
        );
        assert_storage_equations(&counters);
        let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
            exact_operation_namespace_usage(fixture.path());
        assert_eq!((preparation_bytes, preparation_inodes), (0, 1), "{case}");
        assert_eq!((immutable_bytes, immutable_inodes), (0, 0), "{case}");
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert_eq!(counters.storage_bytes_retained, 0, "{case}");
        assert_eq!(counters.storage_inodes_retained, 1, "{case}");
        assert_eq!(counters.mutable_preparation_residue_bytes, 0, "{case}");
        assert_eq!(counters.mutable_preparation_residue_inodes, 1, "{case}");
        assert_eq!(counters.immutable_residue_bytes, 0, "{case}");
        assert_eq!(counters.immutable_residue_inodes, 0, "{case}");
        assert_eq!(counters.unreachable_installed_residue_bytes, 0, "{case}");
        assert!(counters.has_zero_forbidden_work(), "{case}");
        assert!(
            matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        assert!(
            matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        match FsCasV1::open_existing(fixture.path()) {
            Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
            Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
            Ok(_) => panic!("{case}: damaged root reopened as usable"),
        }
    }
}

#[test]
fn carrier_alias_unlink_failure_is_typed_cleanup_with_exact_preparation_residue() {
    let fixture = TestRoot::new("carrier-alias-unlink-enospc");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let bound_invoked = AtomicBool::new(false);
    let supply_invoked = AtomicBool::new(false);
    let mut control = FailFilesystemBoundaryOnceV1 {
        boundary: FsCasFilesystemBoundaryV1::CarrierAliasUnlink,
        error: FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::NoSpace),
        fired: false,
    };
    let (result, counters) = run_small_create_with_callback_observation(
        &cas,
        0x900,
        &mut control,
        &bound_invoked,
        &supply_invoked,
    );

    assert_eq!(
        result,
        Err(OperationErrorV1::FsCas(FsCasErrorV1::TerminalFailure {
            first: FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::NoSpace),
            dominant: FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PrivatePack,),
        }))
    );
    assert!(control.fired);
    assert!(bound_invoked.load(Ordering::Acquire));
    assert!(supply_invoked.load(Ordering::Acquire));
    assert_eq!(cas.operation_admitted_slots_v1(), 0);
    let preparation: Vec<_> = fs::read_dir(fixture.path().join("preparation"))
        .unwrap()
        .map(|entry| entry.unwrap())
        .collect();
    assert_eq!(preparation.len(), 1);
    assert!(preparation[0]
        .file_name()
        .to_string_lossy()
        .starts_with("pack-"));
    let residue_bytes = preparation[0].metadata().unwrap().len();
    assert!(residue_bytes > 0);
    for directory in ["carriers", "objects", "catalog", "closures"] {
        assert_eq!(
            fs::read_dir(fixture.path().join(directory))
                .unwrap()
                .count(),
            0,
            "unexpected visible dependency in {directory}",
        );
    }
    assert_eq!(counters.storage_bytes_retained, residue_bytes);
    assert_eq!(counters.storage_inodes_retained, 1);
    assert_eq!(counters.mutable_preparation_residue_bytes, residue_bytes);
    assert_eq!(counters.mutable_preparation_residue_inodes, 1);
    assert_eq!(
        counters.storage_bytes_requested,
        counters.storage_bytes_released
            + counters.storage_bytes_committed
            + counters.storage_bytes_retained
    );
    assert_eq!(
        counters.storage_inodes_requested,
        counters.storage_inodes_released
            + counters.storage_inodes_committed
            + counters.storage_inodes_retained
    );
    assert!(fixture.path().join("invalidated").is_dir());
    assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(
        FsCasV1::open_existing(fixture.path()),
        Err(FsCasErrorV1::Invalidated)
    ));
    run_subprocess_open_probe(fixture.path(), "invalidated");
}

#[test]
fn carrier_alias_post_unlink_accounting_failure_retains_exact_dual_custody() {
    for (case, fail_invalidation, expected) in [
        (
            "carrier-alias-post-unlink-accounting-failure",
            false,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::Integrity,
                dominant: FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PrivatePack),
            },
        ),
        (
            "carrier-alias-post-unlink-accounting-invalidation-double-fault",
            true,
            FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::Integrity,
                dominant: FsCasFailureCauseV1::InvalidationFailed,
            },
        ),
    ] {
        let fixture = TestRoot::new(case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let bound_invoked = AtomicBool::new(false);
        let supply_invoked = AtomicBool::new(false);
        let mut control = FailCarrierAliasPreparationAccountingV1 {
            cas: cas.clone(),
            armed: false,
            fail_invalidation,
            root_invalidation_callbacks: 0,
        };
        let (result, counters) = run_small_create_with_callback_observation(
            &cas,
            0x9001,
            &mut control,
            &bound_invoked,
            &supply_invoked,
        );

        assert_eq!(result, Err(OperationErrorV1::FsCas(expected)), "{case}");
        assert!(control.armed, "{case}");
        assert_eq!(control.root_invalidation_callbacks, 1, "{case}");
        assert!(bound_invoked.load(Ordering::Acquire), "{case}");
        assert!(supply_invoked.load(Ordering::Acquire), "{case}");
        assert_operation_authority_baseline(&cas, fixture.path());
        assert!(cas.visibility_lock_available_for_test_v1(), "{case}");
        assert!(cas.publication_lock_available_for_test_v1(), "{case}");
        assert_storage_equations(&counters);

        let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
            exact_operation_namespace_usage(fixture.path());
        let (carrier_bytes, carrier_inodes) =
            exact_directory_usage(&fixture.path().join("carriers"));
        assert_eq!((preparation_bytes, preparation_inodes), (0, 0), "{case}");
        assert!(carrier_bytes > 0, "{case}");
        assert_eq!(carrier_inodes, 1, "{case}");
        assert_eq!(
            (immutable_bytes, immutable_inodes),
            (carrier_bytes, 1),
            "{case}"
        );
        for directory in ["objects", "catalog", "closures"] {
            assert_eq!(
                fs::read_dir(fixture.path().join(directory))
                    .unwrap()
                    .count(),
                0,
                "{case}: unexpected visible dependency in {directory}",
            );
        }

        // The private alias is gone, but the failed root-owned transition
        // retains its exact logical preparation charge. The physical carrier
        // is separately retained and directly attributed exactly once.
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert_eq!(
            counters.mutable_preparation_residue_bytes, carrier_bytes,
            "{case}"
        );
        assert_eq!(counters.mutable_preparation_residue_inodes, 1, "{case}");
        assert_eq!(counters.immutable_residue_bytes, carrier_bytes, "{case}");
        assert_eq!(counters.immutable_residue_inodes, 1, "{case}");
        assert_eq!(counters.storage_bytes_retained, carrier_bytes * 2, "{case}");
        assert_eq!(counters.storage_inodes_retained, 2, "{case}");
        assert_eq!(
            counters.unreachable_installed_residue_bytes, carrier_bytes,
            "{case}",
        );
        assert!(counters.has_zero_forbidden_work(), "{case}");

        assert!(
            matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        assert!(
            matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        match FsCasV1::open_existing(fixture.path()) {
            Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
            Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
            Ok(_) => panic!("{case}: damaged root reopened as usable"),
        }
    }
}

#[test]
fn published_locator_alias_unlink_failure_retains_dependencies_and_exact_residue() {
    let fixture = TestRoot::new("locator-alias-unlink-edquot");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let bound_invoked = AtomicBool::new(false);
    let supply_invoked = AtomicBool::new(false);
    let mut control = FailFilesystemBoundaryOnceV1 {
        boundary: FsCasFilesystemBoundaryV1::MarkerAliasUnlink,
        error: FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::Quota),
        fired: false,
    };
    let (result, counters) = run_small_create_with_callback_observation(
        &cas,
        0x901,
        &mut control,
        &bound_invoked,
        &supply_invoked,
    );

    assert_eq!(
        result,
        Err(OperationErrorV1::FsCas(FsCasErrorV1::TerminalFailure {
            first: FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::Quota),
            dominant: FsCasFailureCauseV1::CleanupFailed(
                FsCasCleanupTargetV1::PublishedMarkerAlias,
            ),
        }))
    );
    assert!(control.fired);
    assert_eq!(cas.operation_admitted_slots_v1(), 0);
    assert_eq!(
        fs::read_dir(fixture.path().join("carriers"))
            .unwrap()
            .count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path().join("objects"))
            .unwrap()
            .count(),
        1
    );
    assert_eq!(
        fs::read_dir(fixture.path().join("catalog"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path().join("closures"))
            .unwrap()
            .count(),
        0
    );
    let preparation: Vec<_> = fs::read_dir(fixture.path().join("preparation"))
        .unwrap()
        .map(|entry| entry.unwrap())
        .collect();
    assert_eq!(preparation.len(), 1);
    assert!(preparation[0]
        .file_name()
        .to_string_lossy()
        .starts_with("object-"));
    let residue_bytes = preparation[0].metadata().unwrap().len();
    assert!(residue_bytes > 0);
    let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
        exact_operation_namespace_usage(fixture.path());
    assert_eq!(preparation_bytes, residue_bytes);
    assert_eq!(preparation_inodes, 1);
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert_eq!(
        counters.storage_bytes_retained,
        preparation_bytes + immutable_bytes
    );
    assert_eq!(
        counters.storage_inodes_retained,
        preparation_inodes + immutable_inodes
    );
    assert_eq!(counters.mutable_preparation_residue_bytes, residue_bytes);
    assert_eq!(counters.mutable_preparation_residue_inodes, 1);
    assert_eq!(counters.immutable_residue_inodes, immutable_inodes);
    assert_storage_equations(&counters);
    assert_eq!(
        counters.unreachable_installed_residue_bytes,
        immutable_bytes
    );
    assert_eq!(counters.immutable_residue_bytes, immutable_bytes);
    assert!(counters.has_zero_forbidden_work());
    assert!(fixture.path().join("invalidated").is_dir());
    assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(
        FsCasV1::open_existing(fixture.path()),
        Err(FsCasErrorV1::Invalidated)
    ));
    run_subprocess_open_probe(fixture.path(), "invalidated");
}

#[derive(Default)]
struct CarrierAliasInvalidationDoubleFaultV1 {
    alias_failed: bool,
    invalidation_write_failed: bool,
    invalidation_marker_failed: bool,
}

impl CdcControlV1 for CarrierAliasInvalidationDoubleFaultV1 {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for CarrierAliasInvalidationDoubleFaultV1 {
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

#[test]
fn alias_cleanup_and_invalidation_persistence_double_fault_stays_fail_closed() {
    let fixture = TestRoot::new("alias-invalidation-double-fault");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let bound_invoked = AtomicBool::new(false);
    let supply_invoked = AtomicBool::new(false);
    let mut control = CarrierAliasInvalidationDoubleFaultV1::default();
    let (result, counters) = run_small_create_with_callback_observation(
        &cas,
        0x902,
        &mut control,
        &bound_invoked,
        &supply_invoked,
    );

    assert_eq!(
        result,
        Err(OperationErrorV1::FsCas(FsCasErrorV1::TerminalFailure {
            first: FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::NoSpace),
            dominant: FsCasFailureCauseV1::InvalidationFailed,
        }))
    );
    assert!(control.alias_failed);
    assert!(control.invalidation_write_failed);
    assert!(control.invalidation_marker_failed);
    assert_eq!(cas.operation_admitted_slots_v1(), 0);
    let preparation: Vec<_> = fs::read_dir(fixture.path().join("preparation"))
        .unwrap()
        .map(|entry| entry.unwrap())
        .collect();
    assert_eq!(preparation.len(), 1);
    assert!(preparation[0]
        .file_name()
        .to_string_lossy()
        .starts_with("pack-"));
    let residue_bytes = preparation[0].metadata().unwrap().len();
    assert_eq!(counters.storage_bytes_retained, residue_bytes);
    assert_eq!(counters.storage_inodes_retained, 1);
    assert_path_absent(&fixture.path().join("invalidated"));
    // The first persistence attempt loses both the preallocated owner-token
    // write and the allocation-free marker alternative. A later lifecycle
    // backstop makes the preallocated token durably invalid without allocating
    // anything, so close-all/reopen remains fail-closed as Busy rather than
    // reviving or adopting the damaged root.
    assert_eq!(fs::read(fixture.path().join("owner")).unwrap()[8], 1);
    assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
    // The cross-process exclusive owner check fires before token inspection
    // while this process still owns the root.
    run_subprocess_open_probe(fixture.path(), "busy");

    drop(stale);
    drop(cas);
    assert!(matches!(
        FsCasV1::open_existing(fixture.path()),
        Err(FsCasErrorV1::Busy)
    ));
    run_subprocess_open_probe(fixture.path(), "busy");
}

struct InstallMalformedClosureAtPublication {
    destination: PathBuf,
    injected: bool,
}

impl CdcControlV1 for InstallMalformedClosureAtPublication {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for InstallMalformedClosureAtPublication {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if boundary == FsCasBoundaryV1::BeforeClosureMarkerPublication && !self.injected {
            fs::write(&self.destination, [0_u8; 120])
                .expect("install deterministic racing malformed closure occupant");
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

struct InstallMalformedClosureAndFailPreparationCleanupV1 {
    destination: PathBuf,
    malformed_installed: bool,
    preparation_cleanup_calls: u64,
    preparation_cleanup_injected: bool,
    root_invalidation_calls: u64,
    fail_invalidation: bool,
}

impl CdcControlV1 for InstallMalformedClosureAndFailPreparationCleanupV1 {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for InstallMalformedClosureAndFailPreparationCleanupV1 {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if boundary == FsCasBoundaryV1::BeforeClosureMarkerPublication && !self.malformed_installed
        {
            fs::write(&self.destination, [0_u8; 120])
                .expect("install deterministic racing malformed closure occupant");
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

#[derive(Clone, Copy, Eq, PartialEq)]
enum AdmissionStopV1 {
    Cancelled,
    Deadline,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum QueuedTransitionV1 {
    Grant,
    Cancelled,
    Deadline,
}

struct ArmableQueuedControlV1 {
    transition: QueuedTransitionV1,
    armed: Arc<AtomicBool>,
    observed_polls: Arc<AtomicU64>,
}

impl FsCasControlV1 for ArmableQueuedControlV1 {
    fn cancellation_requested(&mut self) -> bool {
        self.observed_polls.fetch_add(1, Ordering::AcqRel);
        self.transition == QueuedTransitionV1::Cancelled && self.armed.load(Ordering::Acquire)
    }

    fn deadline_exceeded(&mut self) -> bool {
        self.transition == QueuedTransitionV1::Deadline && self.armed.load(Ordering::Acquire)
    }
}

struct StopWhileQueuedControlV1(AdmissionStopV1);

impl FsCasControlV1 for StopWhileQueuedControlV1 {
    fn cancellation_requested(&mut self) -> bool {
        self.0 == AdmissionStopV1::Cancelled
    }

    fn deadline_exceeded(&mut self) -> bool {
        self.0 == AdmissionStopV1::Deadline
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

#[test]
fn queued_control_unwind_cancels_its_ticket_without_poisoning_root_admission() {
    let fixture = TestRoot::new("queued-control-unwind");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let mut continue_control = ContinueControl;
    let mut counters = OperationCountersV1::default();
    let mut active = Vec::with_capacity(16);
    for cancellation_key in 0..16 {
        active.push(
            request_create_operation_v1(
                &cas,
                cancellation_key,
                &mut counters,
                &mut continue_control,
            )
            .unwrap(),
        );
    }

    let mut panic_control = PanicWhileQueuedControlV1::default();
    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = request_create_operation_v1(&cas, 16, &mut counters, &mut panic_control);
    }))
    .expect_err("queued cancellation observation must unwind");
    assert_eq!(
        unwind.downcast_ref::<&'static str>().copied(),
        Some("injected queued cancellation observation unwind")
    );
    assert!(panic_control.panicked);
    assert_eq!(cas.operation_admission_active_for_test_v1(), 16);
    assert_eq!(cas.operation_admission_queue_for_test_v1(), (0, 0, 0));

    drop(active);
    assert_operation_authority_baseline(&cas, fixture.path());

    let mut terminal = cas
        .begin_operation_capability_v1(
            FsOperationKindV1::CompleteC3File,
            17,
            &mut counters,
            &mut continue_control,
        )
        .unwrap();
    terminal
        .finish_terminal_v1(false, &mut counters, &mut continue_control)
        .unwrap();
    assert_operation_authority_baseline(&cas, fixture.path());
    assert_storage_equations(&counters);
    assert!(counters.has_zero_forbidden_work());
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
        if boundary != self.target {
            return;
        }
        self.matching_boundaries += 1;
        if !self.panicked && self.matching_boundaries == self.target_occurrence {
            self.panicked = true;
            panic!("injected root-lock boundary unwind");
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
fn acquired_and_released_root_lock_callback_unwind_is_balanced_and_does_not_poison() {
    for (label, target) in [
        (
            "visibility-acquired-unwind",
            FsCasBoundaryV1::VisibilityLockAcquired,
        ),
        (
            "publication-acquired-unwind",
            FsCasBoundaryV1::PublicationLockAcquired,
        ),
        (
            "visibility-released-unwind",
            FsCasBoundaryV1::VisibilityLockReleased,
        ),
        (
            "publication-released-unwind",
            FsCasBoundaryV1::PublicationLockReleased,
        ),
    ] {
        let fixture = TestRoot::new(label);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let input = [0x39_u8; 64 * 1024 + 17];
        let mut counters = OperationCountersV1::default();
        let mut source_window = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
        let mut cdc_ring = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
        let mut incoming = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
        let mut occupied = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
        let mut tree_object = boxed_zeroes::<MAX_TREE_OBJECT_BYTES>();
        let mut tree_pages = boxed_tree_pages();
        let mut traversal = [0_u8; 64];
        let mut control = PanicAtRootLockBoundaryV1 {
            target,
            // The first publication release closes the read-only carrier
            // vacancy snapshot. Target the second release so this row crosses
            // the authoritative carrier no-replace transition and exercises
            // visible-custody invalidation rather than a prepublication exit.
            target_occurrence: if target == FsCasBoundaryV1::PublicationLockReleased {
                2
            } else {
                1
            },
            matching_boundaries: 0,
            panicked: false,
        };
        let grant = request_create_operation_v1(&cas, 0x180, &mut counters, &mut control).unwrap();
        let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = run_create_v1(
                grant,
                CdcAlgorithmV1::FastCdc,
                b"first.bin",
                0o644,
                input.len() as u64,
                CheckedSupplier { bytes: &input },
                OperationBuffersV1 {
                    source: &mut source_window,
                    cdc_ring: &mut cdc_ring,
                    incoming_comparison: &mut incoming,
                    occupied_comparison: &mut occupied,
                    tree_object: &mut tree_object,
                    tree_pages: &mut *tree_pages,
                    traversal_state: &mut traversal,
                },
                &mut control,
                &mut counters,
            );
        }))
        .expect_err("root-lock boundary callback must unwind");
        assert_eq!(
            unwind.downcast_ref::<&'static str>().copied(),
            Some("injected root-lock boundary unwind")
        );
        assert!(control.panicked);
        assert_operation_authority_baseline(&cas, fixture.path());
        assert_storage_equations(&counters);
        assert_eq!(counters.storage_bytes_committed, 0, "{label}");
        assert_eq!(counters.storage_inodes_committed, 0, "{label}");
        assert!(counters.has_zero_forbidden_work());
        assert!(cas.visibility_lock_available_for_test_v1());
        assert!(cas.publication_lock_available_for_test_v1());

        if target == FsCasBoundaryV1::PublicationLockReleased {
            assert!(counters.storage_bytes_retained > 0, "{label}");
            assert!(counters.storage_inodes_retained > 0, "{label}");
            assert_eq!(
                counters.storage_bytes_retained, counters.immutable_residue_bytes,
                "{label}"
            );
            assert_eq!(
                counters.storage_inodes_retained, counters.immutable_residue_inodes,
                "{label}"
            );
            assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
            assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
            assert!(matches!(
                FsCasV1::open_existing(fixture.path()),
                Err(FsCasErrorV1::Invalidated)
            ));
            continue;
        }

        assert_eq!(counters.storage_bytes_retained, 0, "{label}");
        assert_eq!(counters.storage_inodes_retained, 0, "{label}");
        cas.occupied().unwrap();
        stale.occupied().unwrap();

        let mut followup_counters = OperationCountersV1::default();
        let mut followup_control = ContinueControl;
        let followup =
            request_create_operation_v1(&cas, 0x181, &mut followup_counters, &mut followup_control)
                .unwrap();
        run_create_v1(
            followup,
            CdcAlgorithmV1::FastCdc,
            b"second.bin",
            0o644,
            input.len() as u64,
            CheckedSupplier { bytes: &input },
            OperationBuffersV1 {
                source: &mut source_window,
                cdc_ring: &mut cdc_ring,
                incoming_comparison: &mut incoming,
                occupied_comparison: &mut occupied,
                tree_object: &mut tree_object,
                tree_pages: &mut *tree_pages,
                traversal_state: &mut traversal,
            },
            &mut followup_control,
            &mut followup_counters,
        )
        .unwrap();
        assert_operation_authority_baseline(&cas, fixture.path());
        assert_storage_equations(&followup_counters);
        assert_eq!(followup_counters.storage_bytes_retained, 0);
        assert_eq!(followup_counters.storage_inodes_retained, 0);
        assert!(followup_counters.has_zero_forbidden_work());
    }
}

#[test]
fn seventeenth_operation_genuinely_queues_then_grants_cancels_or_exceeds_deadline() {
    for transition in [
        QueuedTransitionV1::Grant,
        QueuedTransitionV1::Cancelled,
        QueuedTransitionV1::Deadline,
    ] {
        let label = format!("true-c-plus-one-{transition:?}");
        let fixture = TestRoot::new(&label);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let waiter_cas = FsCasV1::open_existing(fixture.path()).unwrap();
        let armed = Arc::new(AtomicBool::new(false));
        let observed_polls = Arc::new(AtomicU64::new(0));
        let (terminal_tx, terminal_rx) = mpsc::sync_channel(1);

        let (terminal, counters) = std::thread::scope(|scope| {
            let mut setup_control = ContinueControl;
            let mut setup_counters = OperationCountersV1::default();
            let mut active = Vec::with_capacity(16);
            for cancellation_key in 0..16_u64 {
                active.push(
                    request_create_operation_v1(
                        &cas,
                        0x20_000 + cancellation_key,
                        &mut setup_counters,
                        &mut setup_control,
                    )
                    .unwrap(),
                );
            }
            assert_eq!(cas.operation_admission_active_for_test_v1(), 16);

            let waiter_armed = Arc::clone(&armed);
            let waiter_polls = Arc::clone(&observed_polls);
            let waiter = scope.spawn(move || {
                let mut control = ArmableQueuedControlV1 {
                    transition,
                    armed: waiter_armed,
                    observed_polls: waiter_polls,
                };
                let mut counters = OperationCountersV1::default();
                let terminal =
                    request_create_operation_v1(&waiter_cas, 0x20_100, &mut counters, &mut control)
                        .map(drop);
                terminal_tx
                    .send((terminal, counters))
                    .expect("C+1 terminal receiver remains live");
            });

            let queued_deadline = std::time::Instant::now() + Duration::from_secs(5);
            while cas.operation_admission_queue_for_test_v1() != (1, 1, 0)
                || observed_polls.load(Ordering::Acquire) < 2
            {
                assert!(
                    std::time::Instant::now() < queued_deadline,
                    "{label}: seventeenth request did not remain genuinely queued: active={}, queue={:?}, polls={}",
                    cas.operation_admission_active_for_test_v1(),
                    cas.operation_admission_queue_for_test_v1(),
                    observed_polls.load(Ordering::Acquire),
                );
                std::thread::yield_now();
            }
            assert_eq!(cas.operation_admission_active_for_test_v1(), 16);
            assert!(
                matches!(terminal_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
                "{label}: the seventeenth request terminalized before capacity or a stop was released"
            );

            match transition {
                QueuedTransitionV1::Grant => {
                    drop(active.pop().expect("one saturated capability"));
                }
                QueuedTransitionV1::Cancelled | QueuedTransitionV1::Deadline => {
                    armed.store(true, Ordering::Release);
                }
            }

            let terminal = terminal_rx
                .recv_timeout(Duration::from_secs(5))
                .unwrap_or_else(|error| panic!("{label}: queued terminal timed out: {error}"));
            waiter.join().expect("C+1 waiter remains healthy");
            drop(active);
            assert_storage_equations(&setup_counters);
            assert!(setup_counters.has_zero_forbidden_work());
            terminal
        });

        match transition {
            QueuedTransitionV1::Grant => assert_eq!(terminal, Ok(()), "{label}"),
            QueuedTransitionV1::Cancelled => assert_eq!(
                terminal,
                Err(FsCasErrorV1::Core(CoreError::Cancelled)),
                "{label}"
            ),
            QueuedTransitionV1::Deadline => assert_eq!(
                terminal,
                Err(FsCasErrorV1::Core(CoreError::Deadline)),
                "{label}"
            ),
        }
        assert_eq!(counters.root_admission_queue_entries, 1, "{label}");
        assert_eq!(counters.root_admission_queue_refusals, 0, "{label}");
        assert_eq!(counters.root_admission_queue_depth_high_water, 1, "{label}");
        assert_eq!(
            counters.root_admission_active_slots_high_water,
            if transition == QueuedTransitionV1::Grant {
                16
            } else {
                0
            },
            "{label}"
        );
        assert!(counters.root_admission_wait_polls >= 2, "{label}");
        assert!(counters.root_admission_wait_nanoseconds > 0, "{label}");
        assert_eq!(counters.root_admission_release_failures, 0, "{label}");
        assert_storage_equations(&counters);
        assert!(counters.has_zero_forbidden_work(), "{label}");
        assert_operation_authority_baseline(&cas, fixture.path());
        assert_eq!(cas.operation_admission_queue_for_test_v1(), (0, 0, 0));
    }
}

#[test]
fn queued_cancel_and_deadline_create_no_preparation_and_cannot_invoke_typed_supplier() {
    for (stop, expected, label) in [
        (
            AdmissionStopV1::Cancelled,
            layerfs_storage::CoreError::Cancelled,
            "queued-cancel",
        ),
        (
            AdmissionStopV1::Deadline,
            layerfs_storage::CoreError::Deadline,
            "queued-deadline",
        ),
    ] {
        let fixture = TestRoot::new(label);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let mut continue_control = ContinueControl;
        let mut admission_counters = OperationCountersV1::default();
        let mut active = Vec::with_capacity(16);
        for cancellation_key in 0..16 {
            active.push(
                request_create_operation_v1(
                    &cas,
                    cancellation_key,
                    &mut admission_counters,
                    &mut continue_control,
                )
                .unwrap(),
            );
        }

        let supplier_invoked = AtomicBool::new(false);
        let _typed_supplier = InvocationCheckedSupplier {
            invoked: &supplier_invoked,
        };
        let mut stop_control = StopWhileQueuedControlV1(stop);
        assert!(matches!(
            request_create_operation_v1(
                &cas,
                16,
                &mut admission_counters,
                &mut stop_control,
            ),
            Err(FsCasErrorV1::Core(error)) if error == expected
        ));

        // Phase one cannot receive the typed request, supplier, source/sink,
        // or caller buffers at all. A stopped waiter therefore cannot invoke
        // the supplier or create/open an operation preparation artifact.
        assert!(!supplier_invoked.load(Ordering::Acquire));
        assert_eq!(
            fs::read_dir(fixture.path().join("preparation"))
                .unwrap()
                .count(),
            0
        );
        assert_eq!(admission_counters.root_admission_queue_entries, 17);
        assert_eq!(admission_counters.root_admission_queue_refusals, 0);
        assert_eq!(admission_counters.root_admission_queue_depth_high_water, 1);
        assert_eq!(
            admission_counters.root_admission_active_slots_high_water,
            16
        );
        assert_eq!(admission_counters.root_admission_wait_polls, 0);
        assert_eq!(admission_counters.root_admission_memory_refusals, 0);
        assert_eq!(admission_counters.root_admission_release_failures, 0);
        drop(active);
        assert_operation_authority_baseline(&cas, fixture.path());
        assert_storage_equations(&admission_counters);
        assert!(admission_counters.has_zero_forbidden_work());
    }
}

#[test]
fn one_thousand_twenty_fifth_operation_entry_refuses_before_callbacks_or_storage_work() {
    let fixture = TestRoot::new("queue-c-plus-one");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let pending = (0_u64..1_024)
        .map(|key| cas.issue_pending_admission_for_test_v1(key).unwrap())
        .collect::<Vec<_>>();

    let supplier_invoked = AtomicBool::new(false);
    let _typed_supplier = InvocationCheckedSupplier {
        invoked: &supplier_invoked,
    };
    let mut control = ContinueControl;
    let mut counters = OperationCountersV1::default();
    assert!(matches!(
        request_create_operation_v1(&cas, 1_024, &mut counters, &mut control),
        Err(FsCasErrorV1::ResourceExhausted(
            layerfs_storage::cas::FsCasResourceV1::Queue
        ))
    ));

    // Phase-one queue custody has no typed request or supplier parameter, so
    // an exhausted queue cannot inspect either or create operation-owned
    // preparation/descriptors before returning the typed refusal.
    assert!(!supplier_invoked.load(Ordering::Acquire));
    assert_eq!(counters.root_admission_queue_entries, 0);
    assert_eq!(counters.root_admission_queue_refusals, 1);
    assert_eq!(counters.source_read_calls, 0);
    assert_eq!(counters.source_bytes_read, 0);
    assert_eq!(counters.storage_preparation_bytes_high_water, 0);
    assert_eq!(counters.storage_preparation_inodes_high_water, 0);
    assert_eq!(counters.layerfs_open_file_handles_high_water, 0);
    assert_eq!(
        fs::read_dir(fixture.path().join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(pending.len(), 1_024);
    drop(pending);
}

#[test]
fn root_storage_byte_and_inode_refusal_precede_supplier_and_preparation() {
    for (label, resource, envelope) in [
        (
            "storage-byte-refusal",
            layerfs_storage::cas::FsCasResourceV1::StorageBytes,
            FsStorageEnvelopeV1::new(ROOT_LOGICAL_STORAGE_BUDGET_V1 - 16 * 1_024 * 1_024, 0, 1, 0)
                .unwrap(),
        ),
        (
            "storage-inode-refusal",
            layerfs_storage::cas::FsCasResourceV1::StorageInodes,
            FsStorageEnvelopeV1::new(1, 0, ROOT_NAMESPACE_ENTRY_BUDGET_V1 - 16, 0).unwrap(),
        ),
    ] {
        let fixture = TestRoot::new(label);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let mut control = ContinueControl;
        let mut blocker_counters = OperationCountersV1::default();
        let mut blocker = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3File,
                0x51,
                &mut blocker_counters,
                &mut control,
            )
            .unwrap();
        blocker.declare_storage_envelope_v1(envelope).unwrap();

        let bound_invoked = AtomicBool::new(false);
        let supply_invoked = AtomicBool::new(false);
        let mut counters = OperationCountersV1::default();
        let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
        let mut tree_pages = [None::<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES];
        let mut traversal = [0_u8; 64];
        let grant = request_create_operation_v1(&cas, 0x52, &mut counters, &mut control).unwrap();
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
            OperationBuffersV1 {
                source: &mut source_window,
                cdc_ring: &mut cdc_ring,
                incoming_comparison: &mut incoming,
                occupied_comparison: &mut occupied,
                tree_object: &mut tree_object,
                tree_pages: &mut tree_pages,
                traversal_state: &mut traversal,
            },
            &mut control,
            &mut counters,
        );
        assert!(matches!(
            result,
            Err(OperationErrorV1::FsCas(FsCasErrorV1::ResourceExhausted(
                observed
            ))) if observed == resource
        ));
        assert!(!bound_invoked.load(Ordering::Acquire));
        assert!(!supply_invoked.load(Ordering::Acquire));
        assert_eq!(counters.source_read_calls, 0);
        assert_eq!(counters.source_bytes_read, 0);
        assert_eq!(cas.operation_admitted_slots_v1(), 1);
        assert_eq!(cas.operation_admission_active_for_test_v1(), 1);
        assert_eq!(cas.storage_admission_active_for_test_v1().0, 1);
        assert_eq!(
            fs::read_dir(fixture.path().join("preparation"))
                .unwrap()
                .count(),
            0
        );
        assert_storage_equations(&counters);
        assert!(counters.has_zero_forbidden_work());

        blocker
            .finish_storage_admission_v1(false, &mut blocker_counters, &mut control)
            .unwrap();
        blocker
            .finish_operation_admission_v1(&mut blocker_counters, &mut control)
            .unwrap();
        assert_eq!(
            blocker_counters.storage_bytes_requested,
            blocker_counters
                .storage_bytes_released
                .checked_add(blocker_counters.storage_bytes_committed)
                .unwrap()
                .checked_add(blocker_counters.storage_bytes_retained)
                .unwrap()
        );
        assert_eq!(
            blocker_counters.storage_inodes_requested,
            blocker_counters
                .storage_inodes_released
                .checked_add(blocker_counters.storage_inodes_committed)
                .unwrap()
                .checked_add(blocker_counters.storage_inodes_retained)
                .unwrap()
        );
        assert_operation_authority_baseline(&cas, fixture.path());
        assert_storage_equations(&blocker_counters);
        assert!(blocker_counters.has_zero_forbidden_work());
    }
}

struct ExactOperationBoundaryControl<'a> {
    root: &'a Path,
    starts: u32,
    ends: u32,
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
        match boundary {
            FsCasBoundaryV1::BeforeOperationSlotReservationRequest => {
                assert_eq!(
                    fs::read_dir(self.root.join("preparation")).unwrap().count(),
                    0
                );
                self.starts += 1;
            }
            FsCasBoundaryV1::AfterCompleteValidatedHandoff => {
                assert_eq!(
                    fs::read_dir(self.root.join("preparation")).unwrap().count(),
                    0
                );
                self.ends += 1;
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

#[derive(Default)]
struct PanicAtFinalHandoff {
    injected: bool,
    panic_during_invalidation: bool,
    invalidation_injected: bool,
}

impl CdcControlV1 for PanicAtFinalHandoff {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for PanicAtFinalHandoff {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if !self.injected && boundary == FsCasBoundaryV1::AfterCompleteValidatedHandoff {
            self.injected = true;
            panic!("injected final handoff unwind")
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
            && !self.invalidation_injected
            && target == FsCasCleanupTargetV1::RootInvalidation
        {
            self.invalidation_injected = true;
            panic!("injected final-handoff invalidation unwind")
        }
        false
    }
}

struct PoisonAdmissionAtFinalHandoff {
    cas: FsCasV1,
    injected: bool,
}

impl CdcControlV1 for PoisonAdmissionAtFinalHandoff {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

struct PoisonAdmissionAndPanicInvalidationAtFinalHandoff {
    cas: FsCasV1,
    admission_poisoned: bool,
    invalidation_panicked: bool,
}

impl CdcControlV1 for PoisonAdmissionAndPanicInvalidationAtFinalHandoff {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for PoisonAdmissionAndPanicInvalidationAtFinalHandoff {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if !self.admission_poisoned && boundary == FsCasBoundaryV1::AfterCompleteValidatedHandoff {
            self.admission_poisoned = true;
            self.cas.poison_operation_admission_for_test_v1();
        }
    }

    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }

    fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
        if !self.invalidation_panicked && target == FsCasCleanupTargetV1::RootInvalidation {
            self.invalidation_panicked = true;
            panic!("injected admission-terminal invalidation unwind")
        }
        false
    }
}

impl FsCasControlV1 for PoisonAdmissionAtFinalHandoff {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if !self.injected && boundary == FsCasBoundaryV1::AfterCompleteValidatedHandoff {
            self.injected = true;
            self.cas.poison_operation_admission_for_test_v1();
        }
    }

    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

struct PoisonStorageAtFinalHandoff {
    cas: FsCasV1,
    injected: bool,
}

impl CdcControlV1 for PoisonStorageAtFinalHandoff {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

struct PoisonStorageAndPanicInvalidationAtFinalHandoff {
    cas: FsCasV1,
    storage_poisoned: bool,
    invalidation_panicked: bool,
}

impl CdcControlV1 for PoisonStorageAndPanicInvalidationAtFinalHandoff {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for PoisonStorageAndPanicInvalidationAtFinalHandoff {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if !self.storage_poisoned && boundary == FsCasBoundaryV1::AfterCompleteValidatedHandoff {
            self.storage_poisoned = true;
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
        if !self.invalidation_panicked && target == FsCasCleanupTargetV1::RootInvalidation {
            self.invalidation_panicked = true;
            panic!("injected storage-terminal invalidation unwind")
        }
        false
    }
}

impl FsCasControlV1 for PoisonStorageAtFinalHandoff {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if !self.injected && boundary == FsCasBoundaryV1::AfterCompleteValidatedHandoff {
            self.injected = true;
            self.cas.poison_storage_admission_for_test_v1();
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
fn exact_complete_c3_boundary_spans_slot_request_through_clean_validated_handoff() {
    let fixture = TestRoot::new("exact-operation-boundary");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let input = [0x71_u8; 96 * 1024 + 31];
    let mut counters = OperationCountersV1::default();
    let mut source_window = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
    let mut cdc_ring = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
    let mut incoming = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut occupied = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut tree_object = boxed_zeroes::<MAX_TREE_OBJECT_BYTES>();
    let mut tree_pages = boxed_tree_pages();
    let mut traversal = [0_u8; 64];
    let mut control = ExactOperationBoundaryControl {
        root: fixture.path(),
        starts: 0,
        ends: 0,
    };
    let grant = request_create_operation_v1(&cas, 100, &mut counters, &mut control).unwrap();

    run_create_v1(
        grant,
        CdcAlgorithmV1::FastCdc,
        b"payload.bin",
        0o644,
        input.len() as u64,
        CheckedSupplier { bytes: &input },
        OperationBuffersV1 {
            source: &mut source_window,
            cdc_ring: &mut cdc_ring,
            incoming_comparison: &mut incoming,
            occupied_comparison: &mut occupied,
            tree_object: &mut tree_object,
            tree_pages: &mut *tree_pages,
            traversal_state: &mut traversal,
        },
        &mut control,
        &mut counters,
    )
    .unwrap();

    assert_eq!(control.starts, 1);
    assert_eq!(control.ends, 1);
}

#[test]
fn final_handoff_admission_release_failure_retains_exact_immutable_set() {
    let fixture = TestRoot::new("final-handoff-admission-release");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let input = [0x76_u8; 64 * 1024 + 37];
    let mut counters = OperationCountersV1::default();
    let mut source_window = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
    let mut cdc_ring = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
    let mut incoming = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut occupied = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut tree_object = boxed_zeroes::<MAX_TREE_OBJECT_BYTES>();
    let mut tree_pages = boxed_tree_pages();
    let mut traversal = [0_u8; 64];
    let mut control = PoisonAdmissionAtFinalHandoff {
        cas: cas.clone(),
        injected: false,
    };
    let grant = request_create_operation_v1(&cas, 0x742, &mut counters, &mut control).unwrap();

    let result = run_create_v1(
        grant,
        CdcAlgorithmV1::FastCdc,
        b"payload.bin",
        0o644,
        input.len() as u64,
        CheckedSupplier { bytes: &input },
        OperationBuffersV1 {
            source: &mut source_window,
            cdc_ring: &mut cdc_ring,
            incoming_comparison: &mut incoming,
            occupied_comparison: &mut occupied,
            tree_object: &mut tree_object,
            tree_pages: &mut *tree_pages,
            traversal_state: &mut traversal,
        },
        &mut control,
        &mut counters,
    );

    assert!(matches!(
        result,
        Err(OperationErrorV1::FsCas(
            FsCasErrorV1::SynchronizationPoisoned
        ))
    ));
    assert!(control.injected);
    assert_eq!(cas.operation_admitted_slots_v1(), 0);
    assert_eq!(cas.operation_admission_active_for_test_v1(), 0);
    let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
        exact_operation_namespace_usage(fixture.path());
    let (carrier_bytes, carrier_inodes) = exact_directory_usage(&fixture.path().join("carriers"));
    let (locator_bytes, locator_inodes) = exact_directory_usage(&fixture.path().join("objects"));
    let (catalog_bytes, catalog_inodes) = exact_directory_usage(&fixture.path().join("catalog"));
    let (closure_bytes, closure_inodes) = exact_directory_usage(&fixture.path().join("closures"));
    assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
    assert_eq!(carrier_inodes, 1);
    assert!(locator_inodes > 0);
    assert_eq!(catalog_inodes, 1);
    assert_eq!(closure_inodes, 1);
    assert_eq!(
        immutable_bytes,
        carrier_bytes + locator_bytes + catalog_bytes + closure_bytes
    );
    assert_eq!(
        immutable_inodes,
        carrier_inodes + locator_inodes + catalog_inodes + closure_inodes
    );
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
            + counters.storage_bytes_retained,
    );
    assert_eq!(
        counters.storage_inodes_reserved,
        counters.storage_inodes_released
            + counters.storage_inodes_committed
            + counters.storage_inodes_retained,
    );
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert_eq!(counters.storage_bytes_retained, immutable_bytes);
    assert_eq!(counters.storage_inodes_retained, immutable_inodes);
    assert_eq!(counters.immutable_residue_bytes, immutable_bytes);
    assert_eq!(counters.immutable_residue_inodes, immutable_inodes);
    assert_eq!(
        counters.unreachable_installed_residue_bytes,
        immutable_bytes
    );
    assert!(counters.has_zero_forbidden_work());
    assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(
        FsCasV1::open_existing(fixture.path()),
        Err(FsCasErrorV1::Invalidated)
    ));
    drop(stale);
    drop(cas);
    run_subprocess_open_probe(fixture.path(), "invalidated");
}

#[test]
fn admission_terminal_invalidation_unwind_retains_first_cause_and_reclassifies_commit() {
    let fixture = TestRoot::new("admission-terminal-invalidation-unwind");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let input = [0x79_u8; 64 * 1024 + 47];
    let mut counters = OperationCountersV1::default();
    let mut source_window = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
    let mut cdc_ring = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
    let mut incoming = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut occupied = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut tree_object = boxed_zeroes::<MAX_TREE_OBJECT_BYTES>();
    let mut tree_pages = boxed_tree_pages();
    let mut traversal = [0_u8; 64];
    let mut control = PoisonAdmissionAndPanicInvalidationAtFinalHandoff {
        cas: cas.clone(),
        admission_poisoned: false,
        invalidation_panicked: false,
    };
    let grant = request_create_operation_v1(&cas, 0x745, &mut counters, &mut control).unwrap();

    assert_eq!(
        run_create_v1(
            grant,
            CdcAlgorithmV1::FastCdc,
            b"payload.bin",
            0o644,
            input.len() as u64,
            CheckedSupplier { bytes: &input },
            OperationBuffersV1 {
                source: &mut source_window,
                cdc_ring: &mut cdc_ring,
                incoming_comparison: &mut incoming,
                occupied_comparison: &mut occupied,
                tree_object: &mut tree_object,
                tree_pages: &mut *tree_pages,
                traversal_state: &mut traversal,
            },
            &mut control,
            &mut counters,
        ),
        Err(OperationErrorV1::FsCas(
            FsCasErrorV1::SynchronizationPoisoned,
        ))
    );

    assert!(control.admission_poisoned);
    assert!(control.invalidation_panicked);
    assert_operation_authority_baseline(&cas, fixture.path());
    assert_storage_equations(&counters);
    let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
        exact_operation_namespace_usage(fixture.path());
    assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
    assert!(immutable_bytes > 0);
    assert!(immutable_inodes > 0);
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert_eq!(counters.storage_bytes_retained, immutable_bytes);
    assert_eq!(counters.storage_inodes_retained, immutable_inodes);
    assert_eq!(counters.immutable_residue_bytes, immutable_bytes);
    assert_eq!(counters.immutable_residue_inodes, immutable_inodes);
    assert_eq!(
        counters.unreachable_installed_residue_bytes,
        immutable_bytes
    );
    assert!(counters.has_zero_forbidden_work());
    assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(
        FsCasV1::open_existing(fixture.path()),
        Err(FsCasErrorV1::Invalidated)
    ));
}

#[test]
fn final_handoff_storage_poison_terminalizes_exact_immutable_set() {
    let fixture = TestRoot::new("final-handoff-storage-poison");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let input = [0x77_u8; 64 * 1024 + 41];
    let mut counters = OperationCountersV1::default();
    let mut source_window = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
    let mut cdc_ring = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
    let mut incoming = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut occupied = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut tree_object = boxed_zeroes::<MAX_TREE_OBJECT_BYTES>();
    let mut tree_pages = boxed_tree_pages();
    let mut traversal = [0_u8; 64];
    let mut control = PoisonStorageAtFinalHandoff {
        cas: cas.clone(),
        injected: false,
    };
    let grant = request_create_operation_v1(&cas, 0x743, &mut counters, &mut control).unwrap();

    let result = run_create_v1(
        grant,
        CdcAlgorithmV1::FastCdc,
        b"payload.bin",
        0o644,
        input.len() as u64,
        CheckedSupplier { bytes: &input },
        OperationBuffersV1 {
            source: &mut source_window,
            cdc_ring: &mut cdc_ring,
            incoming_comparison: &mut incoming,
            occupied_comparison: &mut occupied,
            tree_object: &mut tree_object,
            tree_pages: &mut *tree_pages,
            traversal_state: &mut traversal,
        },
        &mut control,
        &mut counters,
    );

    assert!(matches!(
        result,
        Err(OperationErrorV1::FsCas(
            FsCasErrorV1::SynchronizationPoisoned
        ))
    ));
    assert!(control.injected);
    assert_eq!(cas.operation_admitted_slots_v1(), 0);
    assert_eq!(cas.operation_admission_active_for_test_v1(), 0);
    assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
    let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
        exact_operation_namespace_usage(fixture.path());
    let (carrier_bytes, carrier_inodes) = exact_directory_usage(&fixture.path().join("carriers"));
    let (locator_bytes, locator_inodes) = exact_directory_usage(&fixture.path().join("objects"));
    let (catalog_bytes, catalog_inodes) = exact_directory_usage(&fixture.path().join("catalog"));
    let (closure_bytes, closure_inodes) = exact_directory_usage(&fixture.path().join("closures"));
    assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
    assert_eq!(carrier_inodes, 1);
    assert!(locator_inodes > 0);
    assert_eq!(catalog_inodes, 1);
    assert_eq!(closure_inodes, 1);
    assert_eq!(
        immutable_bytes,
        carrier_bytes + locator_bytes + catalog_bytes + closure_bytes
    );
    assert_eq!(
        immutable_inodes,
        carrier_inodes + locator_inodes + catalog_inodes + closure_inodes
    );
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
            + counters.storage_bytes_retained,
    );
    assert_eq!(
        counters.storage_inodes_reserved,
        counters.storage_inodes_released
            + counters.storage_inodes_committed
            + counters.storage_inodes_retained,
    );
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert_eq!(counters.storage_bytes_retained, immutable_bytes);
    assert_eq!(counters.storage_inodes_retained, immutable_inodes);
    assert_eq!(counters.immutable_residue_bytes, immutable_bytes);
    assert_eq!(counters.immutable_residue_inodes, immutable_inodes);
    assert_eq!(
        counters.unreachable_installed_residue_bytes,
        immutable_bytes
    );
    assert!(counters.has_zero_forbidden_work());
    assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(
        FsCasV1::open_existing(fixture.path()),
        Err(FsCasErrorV1::Invalidated)
    ));
}

#[test]
fn storage_terminal_invalidation_unwind_still_releases_authority_and_persists_failure() {
    let fixture = TestRoot::new("storage-terminal-invalidation-unwind");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let input = [0x78_u8; 64 * 1024 + 43];
    let mut counters = OperationCountersV1::default();
    let mut source_window = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
    let mut cdc_ring = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
    let mut incoming = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut occupied = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut tree_object = boxed_zeroes::<MAX_TREE_OBJECT_BYTES>();
    let mut tree_pages = boxed_tree_pages();
    let mut traversal = [0_u8; 64];
    let mut control = PoisonStorageAndPanicInvalidationAtFinalHandoff {
        cas: cas.clone(),
        storage_poisoned: false,
        invalidation_panicked: false,
    };
    let grant = request_create_operation_v1(&cas, 0x744, &mut counters, &mut control).unwrap();

    assert_eq!(
        run_create_v1(
            grant,
            CdcAlgorithmV1::FastCdc,
            b"payload.bin",
            0o644,
            input.len() as u64,
            CheckedSupplier { bytes: &input },
            OperationBuffersV1 {
                source: &mut source_window,
                cdc_ring: &mut cdc_ring,
                incoming_comparison: &mut incoming,
                occupied_comparison: &mut occupied,
                tree_object: &mut tree_object,
                tree_pages: &mut *tree_pages,
                traversal_state: &mut traversal,
            },
            &mut control,
            &mut counters,
        ),
        Err(OperationErrorV1::FsCas(
            FsCasErrorV1::SynchronizationPoisoned,
        ))
    );
    assert!(control.storage_poisoned);
    assert!(control.invalidation_panicked);
    assert_eq!(cas.operation_admitted_slots_v1(), 0);
    assert_eq!(cas.operation_admission_active_for_test_v1(), 0);
    assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
    let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
        exact_operation_namespace_usage(fixture.path());
    assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
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
            + counters.storage_bytes_retained,
    );
    assert_eq!(
        counters.storage_inodes_reserved,
        counters.storage_inodes_released
            + counters.storage_inodes_committed
            + counters.storage_inodes_retained,
    );
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert_eq!(counters.storage_bytes_retained, immutable_bytes);
    assert_eq!(counters.storage_inodes_retained, immutable_inodes);
    assert_eq!(counters.immutable_residue_bytes, immutable_bytes);
    assert_eq!(counters.immutable_residue_inodes, immutable_inodes);
    assert_eq!(
        counters.unreachable_installed_residue_bytes,
        immutable_bytes
    );
    assert!(counters.has_zero_forbidden_work());
    assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(
        FsCasV1::open_existing(fixture.path()),
        Err(FsCasErrorV1::Invalidated)
    ));
    drop(stale);
    drop(cas);
    run_subprocess_open_probe(fixture.path(), "invalidated");
}

#[test]
fn final_handoff_unwind_retains_installed_carriers_and_fails_root_closed() {
    let fixture = TestRoot::new("final-handoff-unwind");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let input = [0x72_u8; 64 * 1024 + 19];
    let mut counters = OperationCountersV1::default();
    let mut source_window = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
    let mut cdc_ring = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
    let mut incoming = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut occupied = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut tree_object = boxed_zeroes::<MAX_TREE_OBJECT_BYTES>();
    let mut tree_pages = boxed_tree_pages();
    let mut traversal = [0_u8; 64];
    let mut control = PanicAtFinalHandoff::default();
    let grant = request_create_operation_v1(&cas, 0x740, &mut counters, &mut control).unwrap();

    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = run_create_v1(
            grant,
            CdcAlgorithmV1::FastCdc,
            b"payload.bin",
            0o644,
            input.len() as u64,
            CheckedSupplier { bytes: &input },
            OperationBuffersV1 {
                source: &mut source_window,
                cdc_ring: &mut cdc_ring,
                incoming_comparison: &mut incoming,
                occupied_comparison: &mut occupied,
                tree_object: &mut tree_object,
                tree_pages: &mut *tree_pages,
                traversal_state: &mut traversal,
            },
            &mut control,
            &mut counters,
        );
    }));

    assert!(unwind.is_err());
    assert!(control.injected);
    assert_eq!(cas.operation_admitted_slots_v1(), 0);
    let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
        exact_operation_namespace_usage(fixture.path());
    let (carrier_bytes, carrier_inodes) = exact_directory_usage(&fixture.path().join("carriers"));
    assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
    assert_eq!(carrier_inodes, 1);
    assert!(immutable_bytes >= carrier_bytes);
    assert!(immutable_inodes >= carrier_inodes);
    assert_eq!(
        counters.unreachable_installed_residue_bytes,
        immutable_bytes
    );
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
            + counters.storage_bytes_retained,
    );
    assert_eq!(
        counters.storage_inodes_reserved,
        counters.storage_inodes_released
            + counters.storage_inodes_committed
            + counters.storage_inodes_retained,
    );
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert_eq!(counters.storage_bytes_retained, immutable_bytes);
    assert_eq!(counters.storage_inodes_retained, immutable_inodes);
    assert_eq!(counters.immutable_residue_bytes, immutable_bytes);
    assert_eq!(counters.immutable_residue_inodes, immutable_inodes);
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
fn final_handoff_and_invalidation_double_unwind_still_terminalizes_operation() {
    let fixture = TestRoot::new("final-handoff-invalidation-double-unwind");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let input = [0x74_u8; 64 * 1024 + 23];
    let mut counters = OperationCountersV1::default();
    let mut source_window = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
    let mut cdc_ring = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
    let mut incoming = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut occupied = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut tree_object = boxed_zeroes::<MAX_TREE_OBJECT_BYTES>();
    let mut tree_pages = boxed_tree_pages();
    let mut traversal = [0_u8; 64];
    let mut control = PanicAtFinalHandoff {
        panic_during_invalidation: true,
        ..PanicAtFinalHandoff::default()
    };
    let grant = request_create_operation_v1(&cas, 0x741, &mut counters, &mut control).unwrap();

    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = run_create_v1(
            grant,
            CdcAlgorithmV1::FastCdc,
            b"payload.bin",
            0o644,
            input.len() as u64,
            CheckedSupplier { bytes: &input },
            OperationBuffersV1 {
                source: &mut source_window,
                cdc_ring: &mut cdc_ring,
                incoming_comparison: &mut incoming,
                occupied_comparison: &mut occupied,
                tree_object: &mut tree_object,
                tree_pages: &mut *tree_pages,
                traversal_state: &mut traversal,
            },
            &mut control,
            &mut counters,
        );
    }));

    assert!(unwind.is_err());
    assert!(control.injected);
    assert!(control.invalidation_injected);
    assert_eq!(cas.operation_admitted_slots_v1(), 0);
    let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
        exact_operation_namespace_usage(fixture.path());
    let (carrier_bytes, carrier_inodes) = exact_directory_usage(&fixture.path().join("carriers"));
    assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
    assert_eq!(carrier_inodes, 1);
    assert!(immutable_bytes >= carrier_bytes);
    assert!(immutable_inodes >= carrier_inodes);
    assert_eq!(
        counters.unreachable_installed_residue_bytes,
        immutable_bytes
    );
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
            + counters.storage_bytes_retained,
    );
    assert_eq!(
        counters.storage_inodes_reserved,
        counters.storage_inodes_released
            + counters.storage_inodes_committed
            + counters.storage_inodes_retained,
    );
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert_eq!(counters.storage_bytes_retained, immutable_bytes);
    assert_eq!(counters.storage_inodes_retained, immutable_inodes);
    assert_eq!(counters.immutable_residue_bytes, immutable_bytes);
    assert_eq!(counters.immutable_residue_inodes, immutable_inodes);
    assert!(counters.has_zero_forbidden_work());
    assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(
        FsCasV1::open_existing(fixture.path()),
        Err(FsCasErrorV1::Invalidated)
    ));
}

#[test]
fn supplier_unwind_finishes_explicit_cleanup_storage_equations_and_slot_release() {
    let fixture = TestRoot::new("supplier-unwind");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let mut counters = OperationCountersV1::default();
    let mut source_window = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
    let mut cdc_ring = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
    let mut incoming = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut occupied = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut tree_object = boxed_zeroes::<MAX_TREE_OBJECT_BYTES>();
    let mut tree_pages = boxed_tree_pages();
    let mut traversal = [0_u8; 64];
    let mut control = ContinueControl;
    let grant = request_create_operation_v1(&cas, 109, &mut counters, &mut control).unwrap();

    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = run_create_v1(
            grant,
            CdcAlgorithmV1::FastCdc,
            b"payload.bin",
            0o644,
            1,
            PanickingSupplier,
            OperationBuffersV1 {
                source: &mut source_window,
                cdc_ring: &mut cdc_ring,
                incoming_comparison: &mut incoming,
                occupied_comparison: &mut occupied,
                tree_object: &mut tree_object,
                tree_pages: &mut *tree_pages,
                traversal_state: &mut traversal,
            },
            &mut control,
            &mut counters,
        );
    }));

    assert!(unwind.is_err());
    assert_eq!(cas.operation_admitted_slots_v1(), 0);
    assert_eq!(
        fs::read_dir(fixture.path().join("preparation"))
            .unwrap()
            .count(),
        0
    );
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
    assert!(counters.storage_preparation_inodes_high_water > 0);
    assert_eq!(counters.storage_preparation_bytes_current_after_cleanup, 0);
    assert_eq!(counters.storage_preparation_inodes_current_after_cleanup, 0);
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    assert_eq!(counters.storage_bytes_retained, 0);
    assert_eq!(counters.storage_inodes_retained, 0);
    assert_eq!(counters.mutable_preparation_residue_bytes, 0);
    assert_eq!(counters.mutable_preparation_residue_inodes, 0);
    assert_eq!(counters.immutable_residue_bytes, 0);
    assert_eq!(counters.immutable_residue_inodes, 0);
}

#[test]
fn complete_preflight_rejects_traversal_and_page_scratch_before_supplier_or_preparation() {
    for missing_traversal in [true, false] {
        let fixture = TestRoot::new(if missing_traversal {
            "preflight-traversal"
        } else {
            "preflight-pages"
        });
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let invoked = AtomicBool::new(false);
        let mut counters = OperationCountersV1::default();
        let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
        let mut full_pages = [None::<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES];
        let mut full_traversal = [0_u8; 64];
        let mut no_pages: [Option<TreePageSummaryV1>; 0] = [];
        let mut no_traversal: [u8; 0] = [];
        let tree_pages: &mut [Option<TreePageSummaryV1>] = if missing_traversal {
            &mut full_pages
        } else {
            &mut no_pages
        };
        let traversal_state: &mut [u8] = if missing_traversal {
            &mut no_traversal
        } else {
            &mut full_traversal
        };
        let mut control = ContinueControl;
        let grant = request_create_operation_v1(&cas, 101, &mut counters, &mut control).unwrap();

        let error = run_create_v1(
            grant,
            CdcAlgorithmV1::FastCdc,
            b"payload.bin",
            0o644,
            1,
            InvocationCheckedSupplier { invoked: &invoked },
            OperationBuffersV1 {
                source: &mut source_window,
                cdc_ring: &mut cdc_ring,
                incoming_comparison: &mut incoming,
                occupied_comparison: &mut occupied,
                tree_object: &mut tree_object,
                tree_pages,
                traversal_state,
            },
            &mut control,
            &mut counters,
        )
        .unwrap_err();

        assert_eq!(
            error,
            OperationErrorV1::Core(layerfs_storage::CoreError::ResourceRefused)
        );
        assert!(!invoked.load(Ordering::Acquire));
        assert_eq!(
            fs::read_dir(fixture.path().join("preparation"))
                .unwrap()
                .count(),
            0
        );
        assert_eq!(counters.root_admission_queue_entries, 1);
        assert_eq!(counters.root_admission_queue_refusals, 0);
        assert_eq!(counters.root_admission_queue_depth_high_water, 1);
        assert_eq!(counters.root_admission_active_slots_high_water, 1);
        assert_eq!(counters.root_admission_wait_polls, 0);
        assert_eq!(counters.root_admission_memory_refusals, 0);
        assert_eq!(counters.root_admission_release_failures, 0);
        let mut non_admission = counters;
        non_admission.root_admission_queue_entries = 0;
        non_admission.root_admission_queue_depth_high_water = 0;
        non_admission.root_admission_active_slots_high_water = 0;
        non_admission.root_admission_wait_nanoseconds = 0;
        assert_eq!(non_admission, OperationCountersV1::default());
    }
}

#[derive(Default)]
struct FailPreparationCleanupOnce {
    injected: bool,
}

impl CdcControlV1 for FailPreparationCleanupOnce {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for FailPreparationCleanupOnce {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }

    fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
        if target == FsCasCleanupTargetV1::PreparationSpool && !self.injected {
            self.injected = true;
            true
        } else {
            false
        }
    }
}

#[test]
fn preparation_cleanup_failure_is_typed_invalidates_shared_owner_and_retains_exact_residue() {
    // The production operation remains caller-thread-only. This test harness
    // uses an explicitly sized stack because its independent fixed proof
    // buffers intentionally exceed Rust's default test-thread stack.
    std::thread::Builder::new()
        .name("preparation-cleanup-failure".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let fixture = TestRoot::new("preparation-cleanup-failure");
            let cas = FsCasV1::create_new(fixture.path()).unwrap();
            let stale = FsCasV1::open_existing(fixture.path()).unwrap();
            let mut counters = OperationCountersV1::default();
            let input = [0x5a_u8; 64 * 1024 + 17];
            let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
            let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
            let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
            let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
            let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
            let mut tree_pages = [None::<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES];
            let mut traversal = [0_u8; 64];
            let mut control = FailPreparationCleanupOnce::default();
            let grant =
                request_create_operation_v1(&cas, 102, &mut counters, &mut control).unwrap();

            let error = run_create_v1(
                grant,
                CdcAlgorithmV1::FastCdc,
                b"payload.bin",
                0o644,
                input.len() as u64,
                CheckedSupplier { bytes: &input },
                OperationBuffersV1 {
                    source: &mut source_window,
                    cdc_ring: &mut cdc_ring,
                    incoming_comparison: &mut incoming,
                    occupied_comparison: &mut occupied,
                    tree_object: &mut tree_object,
                    tree_pages: &mut tree_pages,
                    traversal_state: &mut traversal,
                },
                &mut control,
                &mut counters,
            )
            .unwrap_err();

            assert_eq!(
                error,
                OperationErrorV1::FsCas(FsCasErrorV1::CleanupFailed(
                    FsCasCleanupTargetV1::PreparationSpool,
                ))
            );
            let preparation: Vec<_> = fs::read_dir(fixture.path().join("preparation"))
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect();
            assert_eq!(preparation.len(), 1);
            assert!(preparation[0]
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("global-seen-"));
            assert_eq!(
                fs::metadata(&preparation[0]).unwrap().len(),
                counters.global_seen_table_bytes
            );
            assert!(counters.global_seen_table_bytes > 0);
            assert_eq!(
                counters.storage_bytes_requested,
                counters.storage_bytes_released
                    + counters.storage_bytes_committed
                    + counters.storage_bytes_retained
            );
            assert_eq!(
                counters.storage_inodes_requested,
                counters.storage_inodes_released
                    + counters.storage_inodes_committed
                    + counters.storage_inodes_retained
            );
            let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
                exact_operation_namespace_usage(fixture.path());
            assert_eq!(preparation_bytes, counters.global_seen_table_bytes);
            assert_eq!(preparation_inodes, 1);
            assert_eq!(counters.storage_bytes_committed, 0);
            assert_eq!(counters.storage_inodes_committed, 0);
            assert_eq!(
                counters.storage_bytes_retained,
                preparation_bytes + immutable_bytes
            );
            assert_eq!(
                counters.storage_inodes_retained,
                preparation_inodes + immutable_inodes
            );
            assert_eq!(
                counters.mutable_preparation_residue_bytes,
                counters.global_seen_table_bytes
            );
            assert_eq!(counters.mutable_preparation_residue_inodes, 1);
            assert_eq!(counters.immutable_residue_inodes, immutable_inodes);
            assert!(fixture.path().join("invalidated").is_dir());
            assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
            assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
            assert!(matches!(
                FsCasV1::open_existing(fixture.path()),
                Err(FsCasErrorV1::Invalidated)
            ));
        })
        .unwrap()
        .join()
        .unwrap();
}

struct FailNthPreparationCleanup {
    target_call: usize,
    observed_calls: usize,
    injected: bool,
}

struct PanicNthPreparationBoundary {
    boundary: FsCasFilesystemBoundaryV1,
    target_call: usize,
    observed_calls: usize,
    injected: bool,
}

impl CdcControlV1 for PanicNthPreparationBoundary {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for PanicNthPreparationBoundary {
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
            panic!("injected preparation boundary unwind")
        }
        None
    }
}

struct PanicNthPreparationCleanup {
    target_call: usize,
    observed_calls: usize,
    injected: bool,
    fail_invalidation: bool,
    invalidation_attempted: bool,
}

#[derive(Default)]
struct PanicPrivatePackCleanupAfterWriteFailure {
    write_injected: bool,
    cleanup_panicked: bool,
    fail_invalidation: bool,
    invalidation_attempted: bool,
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
            panic!("injected private-pack cleanup unwind")
        } else if target == FsCasCleanupTargetV1::RootInvalidation && self.fail_invalidation {
            self.invalidation_attempted = true;
            true
        } else {
            false
        }
    }
}

impl CdcControlV1 for PanicNthPreparationCleanup {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for PanicNthPreparationCleanup {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }

    fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
        if target == FsCasCleanupTargetV1::RootInvalidation && self.fail_invalidation {
            self.invalidation_attempted = true;
            return true;
        }
        if target != FsCasCleanupTargetV1::PreparationSpool {
            return false;
        }
        self.observed_calls += 1;
        if !self.injected && self.observed_calls == self.target_call {
            self.injected = true;
            panic!("injected preparation cleanup unwind")
        }
        false
    }
}

#[test]
fn preparation_construction_unwind_explicitly_cleans_every_locally_owned_spool() {
    std::thread::Builder::new()
        .name("preparation-construction-unwind".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            for (boundary, target_call, label) in [
                (FsCasFilesystemBoundaryV1::PreparationCreate, 1, "create-1"),
                (FsCasFilesystemBoundaryV1::PreparationCreate, 2, "create-2"),
                (FsCasFilesystemBoundaryV1::PreparationCreate, 3, "create-3"),
                (FsCasFilesystemBoundaryV1::PreparationCreate, 4, "create-4"),
                (FsCasFilesystemBoundaryV1::PreparationCreate, 5, "create-5"),
                (FsCasFilesystemBoundaryV1::PreparationCreate, 6, "create-6"),
                (
                    FsCasFilesystemBoundaryV1::PreparationResize,
                    1,
                    "initialize",
                ),
                (FsCasFilesystemBoundaryV1::PermissionChange, 1, "permission"),
            ] {
                let fixture = TestRoot::new(label);
                let cas = FsCasV1::create_new(fixture.path()).unwrap();
                let mut counters = OperationCountersV1::default();
                let payload = [0x51_u8; 1];
                let mut files = [TreeFileV1::new(
                    b"a.bin",
                    0o644,
                    payload.len() as u64,
                    CheckedSupplier { bytes: &payload },
                )];
                let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
                let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
                let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
                let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
                let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
                let mut tree_pages = [None::<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES];
                let mut traversal = [0_u8; 64];
                let mut control = PanicNthPreparationBoundary {
                    boundary,
                    target_call,
                    observed_calls: 0,
                    injected: false,
                };
                let operation = request_tree_operation_v1(
                    &cas,
                    0x400 + target_call as u64,
                    &mut counters,
                    &mut control,
                )
                .unwrap();
                let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _ = run_create_tree_v1(
                        operation,
                        CdcAlgorithmV1::FastCdc,
                        &mut files,
                        OperationBuffersV1 {
                            source: &mut source_window,
                            cdc_ring: &mut cdc_ring,
                            incoming_comparison: &mut incoming,
                            occupied_comparison: &mut occupied,
                            tree_object: &mut tree_object,
                            tree_pages: &mut tree_pages,
                            traversal_state: &mut traversal,
                        },
                        &mut control,
                        &mut counters,
                    );
                }));

                assert!(unwind.is_err(), "{label}");
                assert!(control.injected, "{label}");
                assert_operation_authority_baseline(&cas, fixture.path());
                assert_eq!(
                    fs::read_dir(fixture.path().join("preparation"))
                        .unwrap()
                        .count(),
                    0,
                    "{label}",
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
                    "{label}",
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
                    "{label}",
                );
                assert_eq!(counters.storage_bytes_committed, 0, "{label}");
                assert_eq!(counters.storage_bytes_retained, 0, "{label}");
                assert_eq!(counters.storage_inodes_committed, 0, "{label}");
                assert_eq!(counters.storage_inodes_retained, 0, "{label}");
                assert!(counters.has_zero_forbidden_work(), "{label}");
                assert_path_absent(&fixture.path().join("invalidated"));
                assert!(cas.visibility_lock_available_for_test_v1(), "{label}");
                assert!(cas.publication_lock_available_for_test_v1(), "{label}");

                let bound_invoked = AtomicBool::new(false);
                let supply_invoked = AtomicBool::new(false);
                let mut followup_control = ContinueControl;
                let (followup, followup_counters) = run_small_create_with_callback_observation(
                    &cas,
                    0x500 + target_call as u64,
                    &mut followup_control,
                    &bound_invoked,
                    &supply_invoked,
                );
                assert!(followup.is_ok(), "{label}: {followup:?}");
                assert!(bound_invoked.load(Ordering::Acquire), "{label}");
                assert!(supply_invoked.load(Ordering::Acquire), "{label}");
                assert_operation_authority_baseline(&cas, fixture.path());
                assert_storage_equations(&followup_counters);
                assert!(followup_counters.has_zero_forbidden_work(), "{label}");
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn preparation_cleanup_unwind_attempts_all_lifecycle_targets_before_typed_terminal() {
    std::thread::Builder::new()
        .name("preparation-cleanup-unwind".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            for (fail_invalidation, target_call) in [false, true]
                .into_iter()
                .flat_map(|fail| (1..=7).map(move |target| (fail, target)))
            {
                let fixture = TestRoot::new("cleanup-unwind");
                let cas = FsCasV1::create_new(fixture.path()).unwrap();
                let stale = FsCasV1::open_existing(fixture.path()).unwrap();
                let payload = [0x61_u8; 1];
                let mut files = [TreeFileV1::new(
                    b"a.bin",
                    0o644,
                    payload.len() as u64,
                    CheckedSupplier { bytes: &payload },
                )];
                let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
                let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
                let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
                let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
                let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
                let mut tree_pages = [None::<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES];
                let mut traversal = [0_u8; 64];
                let mut counters = OperationCountersV1::default();
                let mut control = PanicNthPreparationCleanup {
                    target_call,
                    observed_calls: 0,
                    injected: false,
                    fail_invalidation,
                    invalidation_attempted: false,
                };
                let operation = request_tree_operation_v1(
                    &cas,
                    0x500 + target_call as u64,
                    &mut counters,
                    &mut control,
                )
                .unwrap();
                let error = run_create_tree_v1(
                    operation,
                    CdcAlgorithmV1::FastCdc,
                    &mut files,
                    OperationBuffersV1 {
                        source: &mut source_window,
                        cdc_ring: &mut cdc_ring,
                        incoming_comparison: &mut incoming,
                        occupied_comparison: &mut occupied,
                        tree_object: &mut tree_object,
                        tree_pages: &mut tree_pages,
                        traversal_state: &mut traversal,
                    },
                    &mut control,
                    &mut counters,
                )
                .unwrap_err();

                let cleanup = FsCasErrorV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool);
                let expected = if fail_invalidation {
                    cleanup.dominated_by_v1(FsCasErrorV1::InvalidationFailed)
                } else {
                    cleanup
                };
                assert_eq!(error, OperationErrorV1::FsCas(expected));
                assert!(control.injected);
                assert_eq!(control.observed_calls, 7);
                assert_eq!(control.invalidation_attempted, fail_invalidation);
                assert_eq!(cas.operation_admitted_slots_v1(), 0);
                assert_eq!(cas.operation_admission_active_for_test_v1(), 0);
                assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
                let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
                    exact_operation_namespace_usage(fixture.path());
                let (_carrier_bytes, carrier_inodes) =
                    exact_directory_usage(&fixture.path().join("carriers"));
                assert_eq!(preparation_inodes, 1);
                assert_eq!(carrier_inodes, 1);
                assert_eq!(
                    counters.unreachable_installed_residue_bytes,
                    immutable_bytes,
                );
                assert_eq!(
                    counters.storage_bytes_requested,
                    counters.storage_bytes_reserved,
                );
                assert_eq!(
                    counters.storage_inodes_requested,
                    counters.storage_inodes_reserved,
                );
                assert_eq!(
                    counters.storage_bytes_reserved,
                    counters.storage_bytes_released
                        + counters.storage_bytes_committed
                        + counters.storage_bytes_retained,
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
                    preparation_bytes + immutable_bytes,
                );
                assert_eq!(
                    counters.storage_inodes_retained,
                    preparation_inodes + immutable_inodes,
                );
                assert!(counters.has_zero_forbidden_work());
                let invalidated = fixture.path().join("invalidated");
                if fail_invalidation {
                    assert_path_absent(&invalidated);
                } else {
                    assert_path_is_directory(&invalidated);
                }
                assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
                assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
                match FsCasV1::open_existing(fixture.path()) {
                    Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
                    Err(error) => panic!("unexpected reopen error {error:?}"),
                    Ok(_) => panic!("damaged root reopened as usable"),
                }
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

fn assert_private_pack_cleanup_unwind_terminal(case: &'static str, fail_invalidation: bool) {
    let fixture = TestRoot::new(case);
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let mut counters = OperationCountersV1::default();
    let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
    let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
    let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
    let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
    let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
    let mut tree_pages = [None::<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES];
    let mut traversal = [0_u8; 64];
    let mut control = PanicPrivatePackCleanupAfterWriteFailure {
        fail_invalidation,
        ..PanicPrivatePackCleanupAfterWriteFailure::default()
    };
    let grant = request_create_operation_v1(&cas, 0x600, &mut counters, &mut control).unwrap();

    let error = run_create_v1(
        grant,
        CdcAlgorithmV1::FastCdc,
        b"payload.bin",
        0o644,
        1,
        CounterSupplier { len: 1 },
        OperationBuffersV1 {
            source: &mut source_window,
            cdc_ring: &mut cdc_ring,
            incoming_comparison: &mut incoming,
            occupied_comparison: &mut occupied,
            tree_object: &mut tree_object,
            tree_pages: &mut tree_pages,
            traversal_state: &mut traversal,
        },
        &mut control,
        &mut counters,
    )
    .expect_err("private-pack cleanup unwind must return a typed terminal");

    let first = FsCasFailureCauseV1::Filesystem(FsCasFilesystemFailureV1::ShortWrite);
    let dominant = if fail_invalidation {
        FsCasFailureCauseV1::InvalidationFailed
    } else {
        FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PrivatePack)
    };
    assert_eq!(
        error,
        OperationErrorV1::FsCas(FsCasErrorV1::TerminalFailure { first, dominant })
    );
    assert!(control.write_injected);
    assert!(control.cleanup_panicked);
    assert_eq!(control.invalidation_attempted, fail_invalidation);
    assert_eq!(cas.operation_admitted_slots_v1(), 0);
    assert_eq!(cas.operation_admission_active_for_test_v1(), 0);
    assert_eq!(cas.storage_admission_active_for_test_v1(), (0, 0, 0));
    let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
        exact_operation_namespace_usage(fixture.path());
    assert_eq!(preparation_inodes, 1);
    let residue: Vec<_> = fs::read_dir(fixture.path().join("preparation"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(residue.len(), 1);
    assert!(residue[0]
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("pack-"));
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
        preparation_bytes + immutable_bytes,
    );
    assert_eq!(
        counters.storage_inodes_retained,
        preparation_inodes + immutable_inodes,
    );
    let invalidated = fixture.path().join("invalidated");
    if fail_invalidation {
        assert_path_absent(&invalidated);
    } else {
        assert_path_is_directory(&invalidated);
    }
    assert!(counters.has_zero_forbidden_work());
    assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
    match FsCasV1::open_existing(fixture.path()) {
        Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
        Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
        Ok(_) => panic!("{case}: damaged root reopened as usable"),
    }
}

#[test]
fn private_pack_cleanup_unwind_terminalizes_storage_and_preparation_before_return() {
    std::thread::Builder::new()
        .name("private-pack-cleanup-unwind".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            assert_private_pack_cleanup_unwind_terminal("private-pack-cleanup-unwind", false);
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn private_pack_cleanup_unwind_retains_invalidation_double_fault() {
    std::thread::Builder::new()
        .name("private-pack-cleanup-invalidation-double-fault".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            assert_private_pack_cleanup_unwind_terminal(
                "private-pack-cleanup-invalidation-double-fault",
                true,
            );
        })
        .unwrap()
        .join()
        .unwrap();
}

impl CdcControlV1 for FailNthPreparationCleanup {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for FailNthPreparationCleanup {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }

    fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
        if target != FsCasCleanupTargetV1::PreparationSpool {
            return false;
        }
        self.observed_calls += 1;
        if !self.injected && self.observed_calls == self.target_call {
            self.injected = true;
            true
        } else {
            false
        }
    }
}

#[test]
fn every_lifecycle_preparation_cleanup_boundary_is_fallible_and_invalidates_exactly() {
    // The seven lifecycle-owned spools are cleaned in this fixed order. Inject
    // each boundary independently to prove that cleanup continues through all
    // remaining artifacts, retains only the failed artifact, and releases the
    // root grant only after durable invalidation.
    std::thread::Builder::new()
        .name("all-preparation-cleanup-boundaries".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            for (target_call, expected_prefix) in [
                (1, "built-directories-"),
                (2, "built-files-"),
                (3, "global-seen-"),
                (4, "closure-objects-"),
                (5, "pack-index-"),
                (6, "chunk-references-"),
                (7, "locator-receipts-"),
            ] {
                let fixture = TestRoot::new(expected_prefix);
                let cas = FsCasV1::create_new(fixture.path()).unwrap();
                let stale = FsCasV1::open_existing(fixture.path()).unwrap();
                let first = [0x31_u8; 64 * 1024 + 17];
                let second = [0xa7_u8; 72 * 1024 + 29];
                let mut files = [
                    TreeFileV1::new(
                        b"a.bin",
                        0o644,
                        first.len() as u64,
                        CheckedSupplier { bytes: &first },
                    ),
                    TreeFileV1::new(
                        b"nested/b.bin",
                        0o600,
                        second.len() as u64,
                        CheckedSupplier { bytes: &second },
                    ),
                ];
                let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
                let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
                let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
                let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
                let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
                let mut tree_pages = [None::<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES];
                let mut traversal = [0_u8; 64];
                let mut counters = OperationCountersV1::default();
                let mut control = FailNthPreparationCleanup {
                    target_call,
                    observed_calls: 0,
                    injected: false,
                };
                let operation = request_tree_operation_v1(
                    &cas,
                    0x300 + target_call as u64,
                    &mut counters,
                    &mut control,
                )
                .unwrap();

                let error = run_create_tree_v1(
                    operation,
                    CdcAlgorithmV1::FastCdc,
                    &mut files,
                    OperationBuffersV1 {
                        source: &mut source_window,
                        cdc_ring: &mut cdc_ring,
                        incoming_comparison: &mut incoming,
                        occupied_comparison: &mut occupied,
                        tree_object: &mut tree_object,
                        tree_pages: &mut tree_pages,
                        traversal_state: &mut traversal,
                    },
                    &mut control,
                    &mut counters,
                )
                .unwrap_err();

                assert_eq!(
                    error,
                    OperationErrorV1::FsCas(FsCasErrorV1::CleanupFailed(
                        FsCasCleanupTargetV1::PreparationSpool,
                    )),
                    "cleanup boundary {target_call} ({expected_prefix})",
                );
                assert!(control.injected);
                assert_eq!(control.observed_calls, 7);
                assert_eq!(cas.operation_admitted_slots_v1(), 0);
                let preparation: Vec<_> = fs::read_dir(fixture.path().join("preparation"))
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .collect();
                assert_eq!(preparation.len(), 1);
                assert!(
                    preparation[0]
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with(expected_prefix),
                    "unexpected residue at cleanup boundary {target_call}: {:?}",
                    preparation[0],
                );
                assert!(preparation[0].metadata().unwrap().len() > 0);
                let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
                    exact_operation_namespace_usage(fixture.path());
                assert_eq!(preparation_inodes, 1, "cleanup boundary {target_call}");
                assert_eq!(
                    counters.storage_bytes_committed, 0,
                    "cleanup boundary {target_call}"
                );
                assert_eq!(
                    counters.storage_inodes_committed, 0,
                    "cleanup boundary {target_call}"
                );
                assert_eq!(
                    counters.storage_bytes_retained,
                    preparation_bytes + immutable_bytes,
                    "cleanup boundary {target_call}"
                );
                assert_eq!(
                    counters.storage_inodes_retained,
                    preparation_inodes + immutable_inodes,
                    "cleanup boundary {target_call}"
                );
                assert_eq!(
                    counters.mutable_preparation_residue_bytes, preparation_bytes,
                    "cleanup boundary {target_call}"
                );
                assert_eq!(
                    counters.mutable_preparation_residue_inodes, preparation_inodes,
                    "cleanup boundary {target_call}"
                );
                assert_eq!(
                    counters.immutable_residue_inodes, immutable_inodes,
                    "cleanup boundary {target_call}"
                );
                assert!(fixture.path().join("invalidated").is_dir());
                assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
                assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
                assert!(matches!(
                    FsCasV1::open_existing(fixture.path()),
                    Err(FsCasErrorV1::Invalidated)
                ));
                run_subprocess_open_probe(fixture.path(), "invalidated");
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[derive(Default)]
struct FailPreparationCleanupAndPersistentInvalidation {
    preparation_injected: bool,
    invalidation_injected: bool,
}

impl CdcControlV1 for FailPreparationCleanupAndPersistentInvalidation {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for FailPreparationCleanupAndPersistentInvalidation {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }

    fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
        match target {
            FsCasCleanupTargetV1::PreparationSpool if !self.preparation_injected => {
                self.preparation_injected = true;
                true
            }
            FsCasCleanupTargetV1::RootInvalidation if !self.invalidation_injected => {
                self.invalidation_injected = true;
                true
            }
            _ => false,
        }
    }
}

#[test]
fn cleanup_and_persistent_invalidation_double_fault_remains_fail_closed_after_drop_and_subprocess()
{
    std::thread::Builder::new()
        .name("cleanup-invalidation-double-fault".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let fixture = TestRoot::new("cleanup-invalidation-double-fault");
            let cas = FsCasV1::create_new(fixture.path()).unwrap();
            let stale = FsCasV1::open_existing(fixture.path()).unwrap();
            let mut counters = OperationCountersV1::default();
            let input = [0x71_u8; 64 * 1024 + 17];
            let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
            let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
            let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
            let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
            let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
            let mut tree_pages = [None::<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES];
            let mut traversal = [0_u8; 64];
            let mut control = FailPreparationCleanupAndPersistentInvalidation::default();
            let grant =
                request_create_operation_v1(&cas, 109, &mut counters, &mut control).unwrap();

            let error = run_create_v1(
                grant,
                CdcAlgorithmV1::FastCdc,
                b"payload.bin",
                0o644,
                input.len() as u64,
                CheckedSupplier { bytes: &input },
                OperationBuffersV1 {
                    source: &mut source_window,
                    cdc_ring: &mut cdc_ring,
                    incoming_comparison: &mut incoming,
                    occupied_comparison: &mut occupied,
                    tree_object: &mut tree_object,
                    tree_pages: &mut tree_pages,
                    traversal_state: &mut traversal,
                },
                &mut control,
                &mut counters,
            )
            .unwrap_err();

            assert_eq!(
                error,
                OperationErrorV1::FsCas(FsCasErrorV1::TerminalFailure {
                    first: FsCasFailureCauseV1::CleanupFailed(
                        FsCasCleanupTargetV1::PreparationSpool,
                    ),
                    dominant: FsCasFailureCauseV1::InvalidationFailed,
                })
            );
            assert!(control.preparation_injected);
            assert!(control.invalidation_injected);
            assert_eq!(cas.operation_admitted_slots_v1(), 0);
            let preparation: Vec<_> = fs::read_dir(fixture.path().join("preparation"))
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect();
            assert_eq!(preparation.len(), 1);
            assert!(preparation[0]
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("global-seen-"));
            assert_eq!(
                fs::metadata(&preparation[0]).unwrap().len(),
                counters.global_seen_table_bytes
            );
            assert_path_absent(&fixture.path().join("invalidated"));
            assert_eq!(fs::read(fixture.path().join("owner")).unwrap()[8], 1);
            assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
            assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
            run_subprocess_open_probe(fixture.path(), "busy");

            drop(stale);
            drop(cas);
            assert!(matches!(
                FsCasV1::open_existing(fixture.path()),
                Err(FsCasErrorV1::Busy)
            ));
            run_subprocess_open_probe(fixture.path(), "busy");
        })
        .unwrap()
        .join()
        .unwrap();
}

struct FailPublishedMarkerAliasAt {
    target: FsCasBoundaryV1,
    current: Option<FsCasBoundaryV1>,
    first_error: Option<FsCasErrorV1>,
    fail_invalidation: bool,
    injected: bool,
}

struct FailVisibleLocatorResidueAccountingV1 {
    current: Option<FsCasBoundaryV1>,
    fail_alias: bool,
    alias_boundary: FsCasBoundaryV1,
    first_error: Option<FsCasErrorV1>,
    accounting_boundary: Option<FsCasResidueAccountingBoundaryV1>,
    post_catalog_control_failure: Option<PostCatalogControlFailureV1>,
    fail_invalidation: bool,
    alias_injected: bool,
    accounting_injected: bool,
    root_invalidation_calls: u64,
}

#[derive(Clone, Copy, Debug)]
enum PostCatalogControlFailureV1 {
    Cancelled,
    Deadline,
}

impl FailVisibleLocatorResidueAccountingV1 {
    fn cancellation_active_v1(&self) -> bool {
        self.current == Some(FsCasBoundaryV1::AfterCatalogPublication)
            && matches!(
                self.post_catalog_control_failure,
                Some(PostCatalogControlFailureV1::Cancelled)
            )
    }

    fn deadline_active_v1(&self) -> bool {
        self.current == Some(FsCasBoundaryV1::AfterCatalogPublication)
            && matches!(
                self.post_catalog_control_failure,
                Some(PostCatalogControlFailureV1::Deadline)
            )
    }
}

struct PanicAtPublishedMarkerLink {
    target: FsCasBoundaryV1,
    injected: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PostLinkAliasCleanupV1 {
    Succeeds,
    Fails,
    Unwinds,
}

struct PostLinkMarkerUnwindWithSecondaryV1 {
    target: FsCasBoundaryV1,
    boundary_unwind: bool,
    alias_cleanup: PostLinkAliasCleanupV1,
    fail_invalidation: bool,
    current: Option<FsCasBoundaryV1>,
    boundary_panicked: bool,
    alias_cleanup_calls: u64,
    invalidation_calls: u64,
}

#[derive(Clone, Copy)]
enum PreLinkMarkerPanicV1 {
    Filesystem(FsCasFilesystemBoundaryV1),
    VisibilityRequest,
}

struct PanicBeforeMarkerLink {
    target: PreLinkMarkerPanicV1,
    marker_started: bool,
    injected: bool,
    retain_marker: bool,
    cleanup_injected: bool,
}

#[derive(Default)]
struct PanicBeforeCarrierInstall {
    injected: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CarrierCleanupAfterUnwindV1 {
    Succeeds,
    Fails,
    Unwinds,
}

struct PanicAfterCarrierInstall {
    carrier_cleanup: CarrierCleanupAfterUnwindV1,
    fail_invalidation: bool,
    overflow_carrier_counter_transfer: bool,
    boundary_panicked: bool,
    carrier_counter_overflow_injected: bool,
    carrier_cleanup_calls: u64,
    private_cleanup_calls: u64,
    invalidation_calls: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AdmissionUnwindPrivateCleanupV1 {
    Clean,
    Fails,
    Unwinds,
}

struct PanicAdmissionWithSecondaryTerminalV1 {
    panic_boundary: FsCasBoundaryV1,
    accounting_boundary: Option<FsCasResidueAccountingBoundaryV1>,
    private_cleanup: AdmissionUnwindPrivateCleanupV1,
    fail_invalidation: bool,
    boundary_panicked: bool,
    accounting_injected: bool,
    private_cleanup_calls: u64,
    root_invalidation_calls: u64,
}

#[derive(Default)]
struct LocatorResidueRetainsCarrier {
    cancel: bool,
    locator_retained: bool,
    carrier_cleanup_attempted: bool,
}

struct PanicDuringRollbackCleanupV1 {
    cleanup_target: FsCasCleanupTargetV1,
    accounting_boundary: Option<FsCasResidueAccountingBoundaryV1>,
    fail_invalidation: bool,
    cancel: bool,
    locator_cleanup_calls: u64,
    carrier_cleanup_calls: u64,
    cleanup_panicked: bool,
    accounting_injected: bool,
    invalidation_calls: u64,
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug)]
enum LocatorRollbackUnlinkFaultModeV1 {
    SampledUnsupported,
    SampledWriteFailure,
    PermissionDenied,
    WriteFailure,
    InjectedCleanup,
}

#[cfg(unix)]
struct FailLocatorRollbackUnlinkV1 {
    mode: LocatorRollbackUnlinkFaultModeV1,
    objects: PathBuf,
    held_objects: PathBuf,
    cancel: bool,
    armed: bool,
    fault_reached: bool,
    restored: bool,
    fail_invalidation: bool,
    carrier_cleanup_attempted: bool,
}

struct PoisonLocatorRollbackAccountingV1 {
    cas: FsCasV1,
    cancel: bool,
    armed: bool,
    fail_invalidation: bool,
    carrier_cleanup_attempted: bool,
}

impl CdcControlV1 for PoisonLocatorRollbackAccountingV1 {
    fn cancellation_requested(&mut self) -> bool {
        self.cancel
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for PoisonLocatorRollbackAccountingV1 {
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
        self.fail_invalidation && target == FsCasCleanupTargetV1::RootInvalidation
    }
}

#[cfg(unix)]
impl FailLocatorRollbackUnlinkV1 {
    fn restore_objects_v1(&mut self) {
        if self.restored {
            return;
        }
        match self.mode {
            LocatorRollbackUnlinkFaultModeV1::PermissionDenied => {
                use std::os::unix::fs::PermissionsExt;

                fs::set_permissions(&self.objects, fs::Permissions::from_mode(0o700)).unwrap();
            }
            LocatorRollbackUnlinkFaultModeV1::WriteFailure => {
                fs::remove_file(&self.objects).unwrap();
                fs::rename(&self.held_objects, &self.objects).unwrap();
            }
            LocatorRollbackUnlinkFaultModeV1::SampledUnsupported
            | LocatorRollbackUnlinkFaultModeV1::SampledWriteFailure
            | LocatorRollbackUnlinkFaultModeV1::InjectedCleanup => {}
        }
        self.restored = true;
    }
}

#[cfg(unix)]
impl CdcControlV1 for FailLocatorRollbackUnlinkV1 {
    fn cancellation_requested(&mut self) -> bool {
        self.cancel
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

#[cfg(unix)]
impl FsCasControlV1 for FailLocatorRollbackUnlinkV1 {
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

                    fs::set_permissions(&self.objects, fs::Permissions::from_mode(0o500)).unwrap();
                }
                LocatorRollbackUnlinkFaultModeV1::WriteFailure => {
                    fs::rename(&self.objects, &self.held_objects).unwrap();
                    fs::write(&self.objects, b"not-a-directory").unwrap();
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
            self.restore_objects_v1();
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

impl CdcControlV1 for PanicBeforeCarrierInstall {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for PanicBeforeCarrierInstall {
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

impl CdcControlV1 for PanicDuringRollbackCleanupV1 {
    fn cancellation_requested(&mut self) -> bool {
        self.cancel
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for PanicDuringRollbackCleanupV1 {
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

impl CdcControlV1 for LocatorResidueRetainsCarrier {
    fn cancellation_requested(&mut self) -> bool {
        self.cancel
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for LocatorResidueRetainsCarrier {
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
            _ => false,
        }
    }
}

impl CdcControlV1 for PanicAfterCarrierInstall {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl CdcControlV1 for PanicAdmissionWithSecondaryTerminalV1 {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for PanicAdmissionWithSecondaryTerminalV1 {
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

impl FsCasControlV1 for PanicAfterCarrierInstall {
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

impl CdcControlV1 for PanicAtPublishedMarkerLink {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for PanicAtPublishedMarkerLink {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if !self.injected && boundary == self.target {
            self.injected = true;
            panic!("injected post-link marker unwind")
        }
    }

    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl CdcControlV1 for PostLinkMarkerUnwindWithSecondaryV1 {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for PostLinkMarkerUnwindWithSecondaryV1 {
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
        if target != FsCasCleanupTargetV1::PublishedMarkerAlias || self.current != Some(self.target)
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

impl CdcControlV1 for PanicBeforeMarkerLink {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for PanicBeforeMarkerLink {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if !self.injected
            && self.marker_started
            && matches!(self.target, PreLinkMarkerPanicV1::VisibilityRequest)
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
            true
        } else {
            false
        }
    }

    fn inject_filesystem_failure(
        &mut self,
        boundary: FsCasFilesystemBoundaryV1,
    ) -> Option<FsCasErrorV1> {
        if boundary == FsCasFilesystemBoundaryV1::MarkerWrite {
            self.marker_started = true;
        }
        if !self.injected
            && matches!(self.target, PreLinkMarkerPanicV1::Filesystem(target) if target == boundary)
        {
            self.injected = true;
            panic!("injected pre-link marker filesystem unwind")
        }
        None
    }
}

impl CdcControlV1 for FailPublishedMarkerAliasAt {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for FailPublishedMarkerAliasAt {
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
            true
        } else if self.first_error.is_none()
            && !self.injected
            && target == FsCasCleanupTargetV1::PublishedMarkerAlias
            && self.current == Some(self.target)
        {
            self.injected = true;
            true
        } else {
            false
        }
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
            self.first_error
        } else {
            None
        }
    }
}

impl CdcControlV1 for FailVisibleLocatorResidueAccountingV1 {
    fn cancellation_requested(&mut self) -> bool {
        self.cancellation_active_v1()
    }

    fn deadline_exceeded(&mut self) -> bool {
        self.deadline_active_v1()
    }
}

impl FsCasControlV1 for FailVisibleLocatorResidueAccountingV1 {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        self.current = Some(boundary);
    }

    fn cancellation_requested(&mut self) -> bool {
        self.cancellation_active_v1()
    }

    fn deadline_exceeded(&mut self) -> bool {
        self.deadline_active_v1()
    }

    fn inject_cleanup_failure(&mut self, target: FsCasCleanupTargetV1) -> bool {
        match target {
            FsCasCleanupTargetV1::PublishedMarkerAlias
                if !self.alias_injected
                    && self.fail_alias
                    && self.first_error.is_none()
                    && self.current == Some(self.alias_boundary) =>
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
            && self.current == Some(self.alias_boundary)
        {
            if let Some(error) = self.first_error {
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

#[test]
fn post_link_marker_unwind_cleans_alias_records_exact_residue_and_invalidates() {
    std::thread::Builder::new()
        .name("post-link-marker-unwind".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            for (target, prefix, catalog_visible, closure_visible) in [
                (
                    FsCasBoundaryV1::AfterObjectLocatorMarkerLink,
                    "object-panic-",
                    false,
                    false,
                ),
                (
                    FsCasBoundaryV1::AfterCatalogMarkerLink,
                    "catalog-panic-",
                    true,
                    false,
                ),
                (
                    FsCasBoundaryV1::AfterClosureMarkerLink,
                    "closure-panic-",
                    true,
                    true,
                ),
            ] {
                let fixture = TestRoot::new(prefix);
                let cas = FsCasV1::create_new(fixture.path()).unwrap();
                let stale = FsCasV1::open_existing(fixture.path()).unwrap();
                let mut counters = OperationCountersV1::default();
                let input = [0x7d_u8; 64 * 1024 + 17];
                let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
                let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
                let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
                let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
                let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
                let mut tree_pages = [None::<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES];
                let mut traversal = [0_u8; 64];
                let mut control = PanicAtPublishedMarkerLink {
                    target,
                    injected: false,
                };
                let grant =
                    request_create_operation_v1(&cas, 0x710, &mut counters, &mut control).unwrap();

                let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _ = run_create_v1(
                        grant,
                        CdcAlgorithmV1::FastCdc,
                        b"payload.bin",
                        0o644,
                        input.len() as u64,
                        CheckedSupplier { bytes: &input },
                        OperationBuffersV1 {
                            source: &mut source_window,
                            cdc_ring: &mut cdc_ring,
                            incoming_comparison: &mut incoming,
                            occupied_comparison: &mut occupied,
                            tree_object: &mut tree_object,
                            tree_pages: &mut tree_pages,
                            traversal_state: &mut traversal,
                        },
                        &mut control,
                        &mut counters,
                    );
                }));

                assert!(unwind.is_err(), "{target:?}");
                assert!(control.injected, "{target:?}");
                assert_eq!(cas.operation_admitted_slots_v1(), 0, "{target:?}");
                assert_eq!(
                    fs::read_dir(fixture.path().join("preparation"))
                        .unwrap()
                        .count(),
                    0,
                    "{target:?}",
                );
                assert_eq!(
                    fs::read_dir(fixture.path().join("carriers"))
                        .unwrap()
                        .count(),
                    1,
                    "{target:?}",
                );
                assert_eq!(
                    fs::read_dir(fixture.path().join("catalog"))
                        .unwrap()
                        .count(),
                    usize::from(catalog_visible),
                    "{target:?}",
                );
                assert!(
                    fs::read_dir(fixture.path().join("objects"))
                        .unwrap()
                        .count()
                        > 0,
                    "{target:?}",
                );
                assert_eq!(
                    fs::read_dir(fixture.path().join("closures"))
                        .unwrap()
                        .count(),
                    usize::from(closure_visible),
                    "{target:?}",
                );

                let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
                    exact_operation_namespace_usage(fixture.path());
                let (_carrier_bytes, carrier_inodes) =
                    exact_directory_usage(&fixture.path().join("carriers"));
                assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
                assert_eq!(carrier_inodes, 1);
                assert_eq!(
                    counters.unreachable_installed_residue_bytes, immutable_bytes,
                    "{target:?}",
                );
                assert_eq!(counters.storage_bytes_committed, 0, "{target:?}");
                assert_eq!(counters.storage_inodes_committed, 0, "{target:?}");
                assert_eq!(
                    counters.storage_bytes_requested, counters.storage_bytes_reserved,
                    "{target:?}",
                );
                assert_eq!(
                    counters.storage_inodes_requested, counters.storage_inodes_reserved,
                    "{target:?}",
                );
                assert_eq!(
                    counters.storage_bytes_reserved,
                    counters.storage_bytes_released
                        + counters.storage_bytes_committed
                        + counters.storage_bytes_retained,
                    "{target:?}",
                );
                assert_eq!(
                    counters.storage_inodes_reserved,
                    counters.storage_inodes_released
                        + counters.storage_inodes_committed
                        + counters.storage_inodes_retained,
                    "{target:?}",
                );
                assert_eq!(
                    counters.storage_bytes_retained, immutable_bytes,
                    "{target:?}"
                );
                assert_eq!(
                    counters.storage_inodes_retained, immutable_inodes,
                    "{target:?}",
                );
                assert_eq!(
                    counters.immutable_residue_bytes, immutable_bytes,
                    "{target:?}"
                );
                assert_eq!(
                    counters.immutable_residue_inodes, immutable_inodes,
                    "{target:?}",
                );
                assert!(counters.has_zero_forbidden_work(), "{target:?}");
                assert!(fixture.path().join("invalidated").is_dir(), "{target:?}");
                assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
                assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
                assert!(matches!(
                    FsCasV1::open_existing(fixture.path()),
                    Err(FsCasErrorV1::Invalidated)
                ));
                assert!(cas.visibility_lock_available_for_test_v1(), "{target:?}");
                assert!(cas.publication_lock_available_for_test_v1(), "{target:?}");
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn post_link_marker_unwind_classifies_cleanup_and_invalidation_secondary_terminals() {
    std::thread::Builder::new()
        .name("post-link-marker-secondary".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let cases = [
                (
                    "boundary-clean-invalidation",
                    true,
                    PostLinkAliasCleanupV1::Succeeds,
                    false,
                    None,
                ),
                (
                    "boundary-clean-double-fault",
                    true,
                    PostLinkAliasCleanupV1::Succeeds,
                    true,
                    Some(FsCasErrorV1::InvalidationFailed),
                ),
                (
                    "boundary-cleanup-failure",
                    true,
                    PostLinkAliasCleanupV1::Fails,
                    false,
                    Some(FsCasErrorV1::CleanupFailed(
                        FsCasCleanupTargetV1::PublishedMarkerAlias,
                    )),
                ),
                (
                    "boundary-cleanup-failure-double-fault",
                    true,
                    PostLinkAliasCleanupV1::Fails,
                    true,
                    Some(FsCasErrorV1::TerminalFailure {
                        first: FsCasFailureCauseV1::CleanupFailed(
                            FsCasCleanupTargetV1::PublishedMarkerAlias,
                        ),
                        dominant: FsCasFailureCauseV1::InvalidationFailed,
                    }),
                ),
                (
                    "boundary-cleanup-unwind",
                    true,
                    PostLinkAliasCleanupV1::Unwinds,
                    false,
                    Some(FsCasErrorV1::CleanupFailed(
                        FsCasCleanupTargetV1::PublishedMarkerAlias,
                    )),
                ),
                (
                    "boundary-cleanup-unwind-double-fault",
                    true,
                    PostLinkAliasCleanupV1::Unwinds,
                    true,
                    Some(FsCasErrorV1::TerminalFailure {
                        first: FsCasFailureCauseV1::CleanupFailed(
                            FsCasCleanupTargetV1::PublishedMarkerAlias,
                        ),
                        dominant: FsCasFailureCauseV1::InvalidationFailed,
                    }),
                ),
                (
                    "alias-cleanup-unwind",
                    false,
                    PostLinkAliasCleanupV1::Unwinds,
                    false,
                    Some(FsCasErrorV1::CleanupFailed(
                        FsCasCleanupTargetV1::PublishedMarkerAlias,
                    )),
                ),
                (
                    "alias-cleanup-unwind-double-fault",
                    false,
                    PostLinkAliasCleanupV1::Unwinds,
                    true,
                    Some(FsCasErrorV1::TerminalFailure {
                        first: FsCasFailureCauseV1::CleanupFailed(
                            FsCasCleanupTargetV1::PublishedMarkerAlias,
                        ),
                        dominant: FsCasFailureCauseV1::InvalidationFailed,
                    }),
                ),
            ];

            for (label, boundary_unwind, alias_cleanup, fail_invalidation, expected) in cases {
                let fixture = TestRoot::new(label);
                let cas = FsCasV1::create_new(fixture.path()).unwrap();
                let stale = FsCasV1::open_existing(fixture.path()).unwrap();
                let mut counters = OperationCountersV1::default();
                let input = [0x8f_u8; 1];
                let mut source_window = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
                let mut cdc_ring = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
                let mut incoming = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
                let mut occupied = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
                let mut tree_object = boxed_zeroes::<MAX_TREE_OBJECT_BYTES>();
                let mut tree_pages = boxed_tree_pages();
                let mut traversal = [0_u8; 64];
                let mut control = PostLinkMarkerUnwindWithSecondaryV1 {
                    target: FsCasBoundaryV1::AfterClosureMarkerLink,
                    boundary_unwind,
                    alias_cleanup,
                    fail_invalidation,
                    current: None,
                    boundary_panicked: false,
                    alias_cleanup_calls: 0,
                    invalidation_calls: 0,
                };
                let grant =
                    request_create_operation_v1(&cas, 0x711, &mut counters, &mut control).unwrap();

                let terminal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_create_v1(
                        grant,
                        CdcAlgorithmV1::FastCdc,
                        b"payload.bin",
                        0o644,
                        input.len() as u64,
                        CheckedSupplier { bytes: &input },
                        OperationBuffersV1 {
                            source: &mut source_window,
                            cdc_ring: &mut cdc_ring,
                            incoming_comparison: &mut incoming,
                            occupied_comparison: &mut occupied,
                            tree_object: &mut tree_object,
                            tree_pages: &mut *tree_pages,
                            traversal_state: &mut traversal,
                        },
                        &mut control,
                        &mut counters,
                    )
                }));

                match expected {
                    None => assert!(terminal.is_err(), "{label}"),
                    Some(error) => assert_eq!(
                        terminal.unwrap(),
                        Err(OperationErrorV1::FsCas(error)),
                        "{label}",
                    ),
                }
                assert_eq!(control.boundary_panicked, boundary_unwind, "{label}");
                assert_eq!(control.alias_cleanup_calls, 1, "{label}");
                assert_eq!(control.invalidation_calls, 1, "{label}");
                assert_eq!(cas.operation_admitted_slots_v1(), 0, "{label}");
                assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{label}");
                assert_eq!(
                    cas.storage_admission_active_for_test_v1(),
                    (0, 0, 0),
                    "{label}"
                );
                assert_eq!(
                    fs::read_dir(fixture.path().join("carriers"))
                        .unwrap()
                        .count(),
                    1,
                    "{label}",
                );
                assert!(
                    fs::read_dir(fixture.path().join("objects"))
                        .unwrap()
                        .count()
                        > 0,
                    "{label}",
                );
                assert_eq!(
                    fs::read_dir(fixture.path().join("catalog"))
                        .unwrap()
                        .count(),
                    1,
                    "{label}",
                );
                assert_eq!(
                    fs::read_dir(fixture.path().join("closures"))
                        .unwrap()
                        .count(),
                    1,
                    "{label}",
                );

                let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
                    exact_operation_namespace_usage(fixture.path());
                let cleanup_succeeded = alias_cleanup == PostLinkAliasCleanupV1::Succeeds;
                assert_eq!(preparation_inodes, u64::from(!cleanup_succeeded), "{label}");
                if cleanup_succeeded {
                    assert_eq!(preparation_bytes, 0, "{label}");
                } else {
                    assert!(preparation_bytes > 0, "{label}");
                }
                assert_storage_equations(&counters);
                assert_eq!(counters.storage_bytes_committed, 0, "{label}");
                assert_eq!(counters.storage_inodes_committed, 0, "{label}");
                assert_eq!(
                    counters.storage_bytes_retained,
                    preparation_bytes + immutable_bytes,
                    "{label}",
                );
                assert_eq!(
                    counters.storage_inodes_retained,
                    preparation_inodes + immutable_inodes,
                    "{label}",
                );
                assert_eq!(
                    counters.mutable_preparation_residue_bytes, preparation_bytes,
                    "{label}",
                );
                assert_eq!(
                    counters.mutable_preparation_residue_inodes, preparation_inodes,
                    "{label}",
                );
                assert_eq!(
                    counters.unreachable_installed_residue_bytes, immutable_bytes,
                    "{label}",
                );
                assert_eq!(counters.immutable_residue_bytes, immutable_bytes, "{label}");
                assert_eq!(
                    counters.immutable_residue_inodes, immutable_inodes,
                    "{label}"
                );
                assert!(counters.has_zero_forbidden_work(), "{label}");
                assert!(
                    matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
                    "{label}"
                );
                assert!(
                    matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
                    "{label}"
                );
                assert!(matches!(
                    FsCasV1::open_existing(fixture.path()),
                    Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy)
                ));
                assert!(cas.visibility_lock_available_for_test_v1(), "{label}");
                assert!(cas.publication_lock_available_for_test_v1(), "{label}");
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn pre_link_marker_unwind_cleans_once_or_retains_exact_fail_closed_residue() {
    std::thread::Builder::new()
        .name("pre-link-marker-unwind".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            for (target, retain_marker, label) in [
                (
                    PreLinkMarkerPanicV1::Filesystem(FsCasFilesystemBoundaryV1::MarkerWrite),
                    false,
                    "marker-write-panic",
                ),
                (
                    PreLinkMarkerPanicV1::Filesystem(FsCasFilesystemBoundaryV1::MarkerFlush),
                    false,
                    "marker-flush-panic",
                ),
                (
                    PreLinkMarkerPanicV1::VisibilityRequest,
                    false,
                    "marker-visibility-panic",
                ),
                (
                    PreLinkMarkerPanicV1::Filesystem(FsCasFilesystemBoundaryV1::MarkerHardLink),
                    false,
                    "marker-link-panic",
                ),
                (
                    PreLinkMarkerPanicV1::Filesystem(FsCasFilesystemBoundaryV1::MarkerFlush),
                    true,
                    "marker-flush-cleanup-failure",
                ),
            ] {
                let fixture = TestRoot::new(label);
                let cas = FsCasV1::create_new(fixture.path()).unwrap();
                let stale = FsCasV1::open_existing(fixture.path()).unwrap();
                let mut counters = OperationCountersV1::default();
                let input = [0x73_u8; 64 * 1024 + 17];
                let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
                let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
                let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
                let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
                let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
                let mut tree_pages = [None::<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES];
                let mut traversal = [0_u8; 64];
                let mut control = PanicBeforeMarkerLink {
                    target,
                    marker_started: false,
                    injected: false,
                    retain_marker,
                    cleanup_injected: false,
                };
                let grant =
                    request_create_operation_v1(&cas, 0x718, &mut counters, &mut control).unwrap();

                let terminal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_create_v1(
                        grant,
                        CdcAlgorithmV1::FastCdc,
                        b"payload.bin",
                        0o644,
                        input.len() as u64,
                        CheckedSupplier { bytes: &input },
                        OperationBuffersV1 {
                            source: &mut source_window,
                            cdc_ring: &mut cdc_ring,
                            incoming_comparison: &mut incoming,
                            occupied_comparison: &mut occupied,
                            tree_object: &mut tree_object,
                            tree_pages: &mut tree_pages,
                            traversal_state: &mut traversal,
                        },
                        &mut control,
                        &mut counters,
                    )
                }));
                if retain_marker {
                    match terminal {
                        Ok(Err(OperationErrorV1::FsCas(FsCasErrorV1::CleanupFailed(
                            FsCasCleanupTargetV1::PreparationSpool,
                        )))) => {}
                        Ok(_) => panic!("{label}: cleanup failure did not remain terminal"),
                        Err(_) => panic!("{label}: initiating callback escaped cleanup failure"),
                    }
                } else {
                    assert!(terminal.is_err(), "{label}");
                }
                assert!(control.injected, "{label}");
                assert_eq!(control.cleanup_injected, retain_marker, "{label}");
                assert_eq!(cas.operation_admitted_slots_v1(), 0, "{label}");
                assert_eq!(
                    fs::read_dir(fixture.path().join("objects"))
                        .unwrap()
                        .count(),
                    0,
                    "{label}",
                );
                assert_eq!(
                    fs::read_dir(fixture.path().join("catalog"))
                        .unwrap()
                        .count(),
                    0,
                    "{label}",
                );
                assert_eq!(
                    fs::read_dir(fixture.path().join("closures"))
                        .unwrap()
                        .count(),
                    0,
                    "{label}",
                );

                let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
                    exact_operation_namespace_usage(fixture.path());
                let (carrier_bytes, carrier_inodes) =
                    exact_directory_usage(&fixture.path().join("carriers"));
                assert_eq!(preparation_inodes, u64::from(retain_marker), "{label}");
                let expected_carrier_inodes = u64::from(!retain_marker);
                assert_eq!(carrier_inodes, expected_carrier_inodes, "{label}");
                assert_eq!(
                    (immutable_bytes, immutable_inodes),
                    (carrier_bytes, expected_carrier_inodes)
                );
                assert_eq!(
                    counters.unreachable_installed_residue_bytes, carrier_bytes,
                    "{label}",
                );
                assert_eq!(
                    counters.storage_bytes_requested, counters.storage_bytes_reserved,
                    "{label}",
                );
                assert_eq!(
                    counters.storage_inodes_requested, counters.storage_inodes_reserved,
                    "{label}",
                );
                assert_eq!(
                    counters.storage_bytes_reserved,
                    counters.storage_bytes_released
                        + counters.storage_bytes_committed
                        + counters.storage_bytes_retained,
                    "{label}",
                );
                assert_eq!(
                    counters.storage_inodes_reserved,
                    counters.storage_inodes_released
                        + counters.storage_inodes_committed
                        + counters.storage_inodes_retained,
                    "{label}",
                );
                assert_eq!(counters.storage_bytes_committed, 0, "{label}");
                assert_eq!(counters.storage_inodes_committed, 0, "{label}");
                assert_eq!(
                    counters.storage_bytes_retained,
                    preparation_bytes + immutable_bytes,
                    "{label}",
                );
                assert_eq!(
                    counters.storage_inodes_retained,
                    preparation_inodes + immutable_inodes,
                    "{label}",
                );
                assert_eq!(
                    counters.mutable_preparation_residue_bytes, preparation_bytes,
                    "{label}",
                );
                assert_eq!(
                    counters.mutable_preparation_residue_inodes, preparation_inodes,
                    "{label}",
                );
                assert_eq!(counters.immutable_residue_bytes, immutable_bytes, "{label}");
                assert_eq!(
                    counters.immutable_residue_inodes, immutable_inodes,
                    "{label}",
                );
                assert!(counters.has_zero_forbidden_work(), "{label}");
                assert!(fixture.path().join("invalidated").is_dir(), "{label}");
                assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
                assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
                assert!(matches!(
                    FsCasV1::open_existing(fixture.path()),
                    Err(FsCasErrorV1::Invalidated)
                ));
                assert!(cas.visibility_lock_available_for_test_v1(), "{label}");
                assert!(cas.publication_lock_available_for_test_v1(), "{label}");
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn carrier_pre_link_unwind_releases_publication_guard_and_preserves_healthy_root() {
    std::thread::Builder::new()
        .name("carrier-pre-link-unwind".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let fixture = TestRoot::new("carrier-pre-link-unwind");
            let cas = FsCasV1::create_new(fixture.path()).unwrap();
            let stale = FsCasV1::open_existing(fixture.path()).unwrap();
            let mut counters = OperationCountersV1::default();
            let input = [0x6e_u8; 64 * 1024 + 17];
            let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
            let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
            let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
            let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
            let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
            let mut tree_pages = [None::<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES];
            let mut traversal = [0_u8; 64];
            let mut control = PanicBeforeCarrierInstall::default();
            let grant =
                request_create_operation_v1(&cas, 0x71f, &mut counters, &mut control).unwrap();

            let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = run_create_v1(
                    grant,
                    CdcAlgorithmV1::FastCdc,
                    b"payload.bin",
                    0o644,
                    input.len() as u64,
                    CheckedSupplier { bytes: &input },
                    OperationBuffersV1 {
                        source: &mut source_window,
                        cdc_ring: &mut cdc_ring,
                        incoming_comparison: &mut incoming,
                        occupied_comparison: &mut occupied,
                        tree_object: &mut tree_object,
                        tree_pages: &mut tree_pages,
                        traversal_state: &mut traversal,
                    },
                    &mut control,
                    &mut counters,
                );
            }));

            let payload = unwind.expect_err("carrier pre-link callback must unwind");
            assert_eq!(
                payload.downcast_ref::<&'static str>().copied(),
                Some("injected carrier pre-link unwind")
            );
            assert!(control.injected);
            assert_operation_authority_baseline(&cas, fixture.path());
            assert_eq!(
                exact_operation_namespace_usage(fixture.path()),
                ((0, 0), (0, 0))
            );
            assert_storage_equations(&counters);
            assert_eq!(counters.storage_bytes_committed, 0);
            assert_eq!(counters.storage_inodes_committed, 0);
            assert_eq!(counters.storage_bytes_retained, 0);
            assert_eq!(counters.storage_inodes_retained, 0);
            assert_eq!(counters.unreachable_installed_residue_bytes, 0);
            assert!(counters.has_zero_forbidden_work());
            assert_path_absent(&fixture.path().join("invalidated"));
            assert!(cas.visibility_lock_available_for_test_v1());
            assert!(cas.publication_lock_available_for_test_v1());
            assert!(cas.occupied().is_ok());
            assert!(stale.occupied().is_ok());

            let bound_invoked = AtomicBool::new(false);
            let supply_invoked = AtomicBool::new(false);
            let mut followup_control = ContinueControl;
            let (followup, followup_counters) = run_small_create_with_callback_observation(
                &cas,
                0x720,
                &mut followup_control,
                &bound_invoked,
                &supply_invoked,
            );
            assert!(followup.is_ok(), "{followup:?}");
            assert!(bound_invoked.load(Ordering::Acquire));
            assert!(supply_invoked.load(Ordering::Acquire));
            assert_operation_authority_baseline(&cas, fixture.path());
            assert_storage_equations(&followup_counters);
            assert!(followup_counters.has_zero_forbidden_work());
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn carrier_post_link_unwind_rolls_back_once_or_retains_exact_fail_closed_residue() {
    std::thread::Builder::new()
        .name("carrier-post-link-unwind".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            for (case, carrier_cleanup, fail_invalidation, overflow_counter_transfer) in [
                (
                    "carrier-panic-clean",
                    CarrierCleanupAfterUnwindV1::Succeeds,
                    false,
                    false,
                ),
                (
                    "carrier-panic-clean-counter-overflow",
                    CarrierCleanupAfterUnwindV1::Succeeds,
                    false,
                    true,
                ),
                (
                    "carrier-panic-cleanup-failure",
                    CarrierCleanupAfterUnwindV1::Fails,
                    false,
                    false,
                ),
                (
                    "carrier-panic-cleanup-invalidation-double-fault",
                    CarrierCleanupAfterUnwindV1::Fails,
                    true,
                    false,
                ),
                (
                    "carrier-panic-cleanup-unwind",
                    CarrierCleanupAfterUnwindV1::Unwinds,
                    false,
                    false,
                ),
                (
                    "carrier-panic-cleanup-unwind-invalidation-double-fault",
                    CarrierCleanupAfterUnwindV1::Unwinds,
                    true,
                    false,
                ),
            ] {
                let retain_carrier = carrier_cleanup != CarrierCleanupAfterUnwindV1::Succeeds;
                let fixture = TestRoot::new(case);
                let cas = FsCasV1::create_new(fixture.path()).unwrap();
                let stale = FsCasV1::open_existing(fixture.path()).unwrap();
                let mut counters = OperationCountersV1::default();
                let input = [0x6f_u8; 64 * 1024 + 17];
                let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
                let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
                let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
                let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
                let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
                let mut tree_pages = [None::<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES];
                let mut traversal = [0_u8; 64];
                let mut control = PanicAfterCarrierInstall {
                    carrier_cleanup,
                    fail_invalidation,
                    overflow_carrier_counter_transfer: overflow_counter_transfer,
                    boundary_panicked: false,
                    carrier_counter_overflow_injected: false,
                    carrier_cleanup_calls: 0,
                    private_cleanup_calls: 0,
                    invalidation_calls: 0,
                };
                let grant =
                    request_create_operation_v1(&cas, 0x720, &mut counters, &mut control).unwrap();

                let terminal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_create_v1(
                        grant,
                        CdcAlgorithmV1::FastCdc,
                        b"payload.bin",
                        0o644,
                        input.len() as u64,
                        CheckedSupplier { bytes: &input },
                        OperationBuffersV1 {
                            source: &mut source_window,
                            cdc_ring: &mut cdc_ring,
                            incoming_comparison: &mut incoming,
                            occupied_comparison: &mut occupied,
                            tree_object: &mut tree_object,
                            tree_pages: &mut tree_pages,
                            traversal_state: &mut traversal,
                        },
                        &mut control,
                        &mut counters,
                    )
                }));

                if overflow_counter_transfer {
                    assert_eq!(
                        terminal.unwrap().unwrap_err(),
                        OperationErrorV1::Core(CoreError::IntegerOverflow),
                        "{case}",
                    );
                } else if retain_carrier {
                    let expected = if fail_invalidation {
                        FsCasErrorV1::TerminalFailure {
                            first: FsCasFailureCauseV1::CleanupFailed(
                                FsCasCleanupTargetV1::Carrier,
                            ),
                            dominant: FsCasFailureCauseV1::InvalidationFailed,
                        }
                    } else {
                        FsCasErrorV1::CleanupFailed(FsCasCleanupTargetV1::Carrier)
                    };
                    assert_eq!(
                        terminal.unwrap().unwrap_err(),
                        OperationErrorV1::FsCas(expected),
                        "{case}",
                    );
                } else {
                    assert!(terminal.is_err(), "{case}");
                }
                assert!(control.boundary_panicked);
                assert_eq!(
                    control.carrier_counter_overflow_injected, overflow_counter_transfer,
                    "{case}",
                );
                assert_eq!(control.carrier_cleanup_calls, 1, "{case}");
                assert_eq!(control.private_cleanup_calls, 1, "{case}");
                assert_eq!(
                    control.invalidation_calls,
                    u64::from(retain_carrier),
                    "{case}"
                );
                assert_eq!(cas.operation_admitted_slots_v1(), 0);
                let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
                    exact_operation_namespace_usage(fixture.path());
                let (carrier_bytes, carrier_inodes) =
                    exact_directory_usage(&fixture.path().join("carriers"));
                assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
                assert_eq!(carrier_inodes, u64::from(retain_carrier));
                assert_eq!(immutable_inodes, u64::from(retain_carrier));
                assert_eq!(immutable_bytes, carrier_bytes);
                assert_eq!(counters.unreachable_installed_residue_bytes, carrier_bytes);
                assert_eq!(counters.storage_bytes_committed, 0);
                assert_eq!(counters.storage_inodes_committed, 0);
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
                        + counters.storage_bytes_retained,
                );
                assert_eq!(
                    counters.storage_inodes_reserved,
                    counters.storage_inodes_released
                        + counters.storage_inodes_committed
                        + counters.storage_inodes_retained,
                );
                assert_eq!(counters.storage_bytes_retained, immutable_bytes);
                assert_eq!(counters.storage_inodes_retained, immutable_inodes);
                assert_eq!(counters.immutable_residue_bytes, immutable_bytes);
                assert_eq!(counters.immutable_residue_inodes, immutable_inodes);
                assert!(counters.has_zero_forbidden_work());

                if retain_carrier {
                    let invalidated = fixture.path().join("invalidated");
                    if fail_invalidation {
                        assert_path_absent(&invalidated);
                    } else {
                        assert_path_is_directory(&invalidated);
                    }
                    assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
                    assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
                    assert!(matches!(
                        FsCasV1::open_existing(fixture.path()),
                        Err(FsCasErrorV1::Busy | FsCasErrorV1::Invalidated)
                    ));
                } else {
                    assert_path_absent(&fixture.path().join("invalidated"));
                    assert!(cas.occupied().is_ok());
                    assert!(stale.occupied().is_ok());
                    drop(stale);
                    drop(cas);
                    let reopened = FsCasV1::open_existing(fixture.path()).unwrap();
                    drop(reopened);
                }
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn locator_cleanup_residue_retains_its_carrier_without_unlink_attempt() {
    std::thread::Builder::new()
        .name("locator-carrier-double-fault".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let fixture = TestRoot::new("locator-carrier-double-fault");
            let cas = FsCasV1::create_new(fixture.path()).unwrap();
            let stale = FsCasV1::open_existing(fixture.path()).unwrap();
            let mut counters = OperationCountersV1::default();
            let input = [0x6a_u8; 64 * 1024 + 17];
            let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
            let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
            let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
            let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
            let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
            let mut tree_pages = [None::<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES];
            let mut traversal = [0_u8; 64];
            let mut control = LocatorResidueRetainsCarrier::default();
            let grant =
                request_create_operation_v1(&cas, 0x728, &mut counters, &mut control).unwrap();

            let result = run_create_v1(
                grant,
                CdcAlgorithmV1::FastCdc,
                b"payload.bin",
                0o644,
                input.len() as u64,
                CheckedSupplier { bytes: &input },
                OperationBuffersV1 {
                    source: &mut source_window,
                    cdc_ring: &mut cdc_ring,
                    incoming_comparison: &mut incoming,
                    occupied_comparison: &mut occupied,
                    tree_object: &mut tree_object,
                    tree_pages: &mut tree_pages,
                    traversal_state: &mut traversal,
                },
                &mut control,
                &mut counters,
            );

            assert!(matches!(
                result,
                Err(OperationErrorV1::FsCas(FsCasErrorV1::TerminalFailure {
                    first: FsCasFailureCauseV1::Core(CoreError::Cancelled),
                    dominant: FsCasFailureCauseV1::CleanupFailed(
                        FsCasCleanupTargetV1::ObjectLocator,
                    ),
                }))
            ));
            assert!(control.locator_retained);
            assert!(!control.carrier_cleanup_attempted);
            assert_eq!(cas.operation_admitted_slots_v1(), 0);
            let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
                exact_operation_namespace_usage(fixture.path());
            let (carrier_bytes, carrier_inodes) =
                exact_directory_usage(&fixture.path().join("carriers"));
            let (locator_bytes, locator_inodes) =
                exact_directory_usage(&fixture.path().join("objects"));
            assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
            assert_eq!(carrier_inodes, 1);
            assert_eq!(locator_inodes, 1);
            assert_eq!(immutable_bytes, carrier_bytes + locator_bytes);
            assert_eq!(immutable_inodes, carrier_inodes + locator_inodes);
            assert_eq!(
                counters.unreachable_installed_residue_bytes,
                immutable_bytes,
            );
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
                    + counters.storage_bytes_retained,
            );
            assert_eq!(
                counters.storage_inodes_reserved,
                counters.storage_inodes_released
                    + counters.storage_inodes_committed
                    + counters.storage_inodes_retained,
            );
            assert_eq!(counters.storage_bytes_retained, immutable_bytes);
            assert_eq!(counters.storage_inodes_retained, immutable_inodes);
            assert_eq!(counters.immutable_residue_bytes, immutable_bytes);
            assert_eq!(counters.immutable_residue_inodes, immutable_inodes);
            assert!(counters.has_zero_forbidden_work());
            assert!(fixture.path().join("invalidated").is_dir());
            assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
            assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
            assert!(matches!(
                FsCasV1::open_existing(fixture.path()),
                Err(FsCasErrorV1::Invalidated)
            ));
        })
        .unwrap()
        .join()
        .unwrap();
}

#[cfg(unix)]
#[test]
fn locator_rollback_preserves_directional_unlink_faults_and_dependency_custody() {
    std::thread::Builder::new()
        .name("locator-rollback-directional-faults".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            for mode in [
                LocatorRollbackUnlinkFaultModeV1::SampledUnsupported,
                LocatorRollbackUnlinkFaultModeV1::SampledWriteFailure,
                LocatorRollbackUnlinkFaultModeV1::PermissionDenied,
                LocatorRollbackUnlinkFaultModeV1::WriteFailure,
                LocatorRollbackUnlinkFaultModeV1::InjectedCleanup,
            ] {
                for fail_invalidation in [false, true] {
                    let label = format!("locator-rollback-{mode:?}-{fail_invalidation}");
                    let fixture = TestRoot::new(&label);
                    let cas = FsCasV1::create_new(fixture.path()).unwrap();
                    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
                    let objects = fixture.path().join("objects");
                    let held_objects = fixture.path().join("objects-held-for-fault");
                    let mut counters = OperationCountersV1::default();
                    let input = [0x4f_u8; 64 * 1024 + 41];
                    let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
                    let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
                    let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
                    let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
                    let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
                    let mut tree_pages = [None::<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES];
                    let mut traversal = [0_u8; 64];
                    let mut control = FailLocatorRollbackUnlinkV1 {
                        mode,
                        objects,
                        held_objects,
                        cancel: false,
                        armed: false,
                        fault_reached: false,
                        restored: false,
                        fail_invalidation,
                        carrier_cleanup_attempted: false,
                    };
                    let grant =
                        request_create_operation_v1(&cas, 0x72a, &mut counters, &mut control)
                            .unwrap();

                    let result = run_create_v1(
                        grant,
                        CdcAlgorithmV1::FastCdc,
                        b"payload.bin",
                        0o644,
                        input.len() as u64,
                        CheckedSupplier { bytes: &input },
                        OperationBuffersV1 {
                            source: &mut source_window,
                            cdc_ring: &mut cdc_ring,
                            incoming_comparison: &mut incoming,
                            occupied_comparison: &mut occupied,
                            tree_object: &mut tree_object,
                            tree_pages: &mut tree_pages,
                            traversal_state: &mut traversal,
                        },
                        &mut control,
                        &mut counters,
                    );

                    let expected_dominant = if fail_invalidation {
                        FsCasFailureCauseV1::InvalidationFailed
                    } else {
                        FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::ObjectLocator)
                    };
                    assert_eq!(
                        result.unwrap_err(),
                        OperationErrorV1::FsCas(FsCasErrorV1::TerminalFailure {
                            first: FsCasFailureCauseV1::Core(CoreError::Cancelled),
                            dominant: expected_dominant,
                        }),
                        "{mode:?}, invalidation double fault={fail_invalidation}"
                    );
                    assert!(control.armed, "{mode:?}");
                    assert!(control.fault_reached, "{mode:?}");
                    assert!(control.restored, "{mode:?}");
                    assert!(
                        !control.carrier_cleanup_attempted,
                        "{mode:?}: retained locator must retain its carrier dependency"
                    );
                    assert_operation_authority_baseline(&cas, fixture.path());
                    assert_storage_equations(&counters);

                    let (
                        (preparation_bytes, preparation_inodes),
                        (immutable_bytes, immutable_inodes),
                    ) = exact_operation_namespace_usage(fixture.path());
                    let (carrier_bytes, carrier_inodes) =
                        exact_directory_usage(&fixture.path().join("carriers"));
                    let (locator_bytes, locator_inodes) =
                        exact_directory_usage(&fixture.path().join("objects"));
                    assert_eq!((preparation_bytes, preparation_inodes), (0, 0), "{mode:?}");
                    assert_eq!(carrier_inodes, 1, "{mode:?}");
                    assert!(locator_inodes >= 1, "{mode:?}");
                    assert_eq!(immutable_bytes, carrier_bytes + locator_bytes, "{mode:?}");
                    assert_eq!(
                        immutable_inodes,
                        carrier_inodes + locator_inodes,
                        "{mode:?}"
                    );
                    assert_eq!(
                        counters.unreachable_installed_residue_bytes, immutable_bytes,
                        "{mode:?}"
                    );
                    assert_eq!(counters.storage_bytes_committed, 0, "{mode:?}");
                    assert_eq!(counters.storage_inodes_committed, 0, "{mode:?}");
                    assert_eq!(counters.storage_bytes_retained, immutable_bytes, "{mode:?}");
                    assert_eq!(
                        counters.storage_inodes_retained, immutable_inodes,
                        "{mode:?}"
                    );
                    assert_eq!(
                        counters.immutable_residue_bytes, immutable_bytes,
                        "{mode:?}"
                    );
                    assert_eq!(
                        counters.immutable_residue_inodes, immutable_inodes,
                        "{mode:?}"
                    );
                    assert!(counters.has_zero_forbidden_work(), "{mode:?}");
                    assert!(
                        matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
                        "{mode:?}"
                    );
                    assert!(
                        matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
                        "{mode:?}"
                    );
                    assert!(matches!(
                        FsCasV1::open_existing(fixture.path()),
                        Err(FsCasErrorV1::Busy | FsCasErrorV1::Invalidated)
                    ));
                }
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn locator_rollback_accounting_poison_defers_invalidation_to_owned_terminal() {
    std::thread::Builder::new()
        .name("locator-rollback-accounting-poison".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            for fail_invalidation in [false, true] {
                let fixture = TestRoot::new(if fail_invalidation {
                    "locator-accounting-invalidation-double-fault"
                } else {
                    "locator-accounting-cleanup-failure"
                });
                let cas = FsCasV1::create_new(fixture.path()).unwrap();
                let stale = FsCasV1::open_existing(fixture.path()).unwrap();
                let mut counters = OperationCountersV1::default();
                let input = [0x59_u8; 64 * 1024 + 53];
                let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
                let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
                let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
                let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
                let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
                let mut tree_pages = [None::<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES];
                let mut traversal = [0_u8; 64];
                let mut control = PoisonLocatorRollbackAccountingV1 {
                    cas: cas.clone(),
                    cancel: false,
                    armed: false,
                    fail_invalidation,
                    carrier_cleanup_attempted: false,
                };
                let grant =
                    request_create_operation_v1(&cas, 0x72b, &mut counters, &mut control).unwrap();

                let result = run_create_v1(
                    grant,
                    CdcAlgorithmV1::FastCdc,
                    b"payload.bin",
                    0o644,
                    input.len() as u64,
                    CheckedSupplier { bytes: &input },
                    OperationBuffersV1 {
                        source: &mut source_window,
                        cdc_ring: &mut cdc_ring,
                        incoming_comparison: &mut incoming,
                        occupied_comparison: &mut occupied,
                        tree_object: &mut tree_object,
                        tree_pages: &mut tree_pages,
                        traversal_state: &mut traversal,
                    },
                    &mut control,
                    &mut counters,
                );

                let expected_dominant = if fail_invalidation {
                    FsCasFailureCauseV1::InvalidationFailed
                } else {
                    FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::Carrier)
                };
                assert_eq!(
                    result.unwrap_err(),
                    OperationErrorV1::FsCas(FsCasErrorV1::TerminalFailure {
                        first: FsCasFailureCauseV1::Core(CoreError::Cancelled),
                        dominant: expected_dominant,
                    })
                );
                assert!(control.armed);
                assert!(control.carrier_cleanup_attempted);
                assert_operation_authority_baseline(&cas, fixture.path());
                assert_storage_equations(&counters);
                assert_eq!(
                    exact_operation_namespace_usage(fixture.path()),
                    ((0, 0), (0, 0))
                );
                assert_eq!(counters.storage_bytes_committed, 0);
                assert_eq!(counters.storage_inodes_committed, 0);
                assert_eq!(counters.storage_bytes_retained, 0);
                assert_eq!(counters.storage_inodes_retained, 0);
                assert_eq!(counters.unreachable_installed_residue_bytes, 0);
                assert!(counters.has_zero_forbidden_work());
                assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
                assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
                assert!(matches!(
                    FsCasV1::open_existing(fixture.path()),
                    Err(FsCasErrorV1::Busy | FsCasErrorV1::Invalidated)
                ));
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn locator_cleanup_unwind_attempts_every_remaining_locator_and_carrier_once() {
    std::thread::Builder::new()
        .name("locator-cleanup-unwind".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            for cleanup_target in [
                FsCasCleanupTargetV1::ObjectLocator,
                FsCasCleanupTargetV1::Carrier,
            ] {
                let accounting_boundary = match cleanup_target {
                    FsCasCleanupTargetV1::ObjectLocator => {
                        FsCasResidueAccountingBoundaryV1::ObjectLocator
                    }
                    FsCasCleanupTargetV1::Carrier => {
                        FsCasResidueAccountingBoundaryV1::Carrier
                    }
                    _ => unreachable!(),
                };
                for inject_accounting in [false, true] {
                    for fail_invalidation in [false, true] {
                        let label = format!(
                            "rollback-unwind-{cleanup_target:?}-{inject_accounting}-{fail_invalidation}"
                        );
                        let fixture = TestRoot::new(&label);
                        let cas = FsCasV1::create_new(fixture.path()).unwrap();
                        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
                        let mut counters = OperationCountersV1::default();
                        let input = [0x75_u8; 64 * 1024 + 29];
                        let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
                        let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
                        let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
                        let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
                        let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
                        let mut tree_pages =
                            [None::<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES];
                        let mut traversal = [0_u8; 64];
                        let mut control = PanicDuringRollbackCleanupV1 {
                            cleanup_target,
                            accounting_boundary: inject_accounting
                                .then_some(accounting_boundary),
                            fail_invalidation,
                            cancel: false,
                            locator_cleanup_calls: 0,
                            carrier_cleanup_calls: 0,
                            cleanup_panicked: false,
                            accounting_injected: false,
                            invalidation_calls: 0,
                        };
                        let grant = request_create_operation_v1(
                            &cas,
                            0x729,
                            &mut counters,
                            &mut control,
                        )
                        .unwrap();

                        let error = run_create_v1(
                            grant,
                            CdcAlgorithmV1::FastCdc,
                            b"payload.bin",
                            0o644,
                            input.len() as u64,
                            CheckedSupplier { bytes: &input },
                            OperationBuffersV1 {
                                source: &mut source_window,
                                cdc_ring: &mut cdc_ring,
                                incoming_comparison: &mut incoming,
                                occupied_comparison: &mut occupied,
                                tree_object: &mut tree_object,
                                tree_pages: &mut tree_pages,
                                traversal_state: &mut traversal,
                            },
                            &mut control,
                            &mut counters,
                        )
                        .unwrap_err();

                        let expected_dominant = if fail_invalidation {
                            FsCasFailureCauseV1::InvalidationFailed
                        } else {
                            FsCasFailureCauseV1::CleanupFailed(cleanup_target)
                        };
                        assert_eq!(
                            error,
                            OperationErrorV1::FsCas(FsCasErrorV1::TerminalFailure {
                                first: FsCasFailureCauseV1::Core(CoreError::Cancelled),
                                dominant: expected_dominant,
                            }),
                            "{label}"
                        );
                        assert!(control.cleanup_panicked, "{label}");
                        assert_eq!(
                            control.accounting_injected, inject_accounting,
                            "{label}"
                        );
                        assert_eq!(control.invalidation_calls, 1, "{label}");
                        assert!(control.locator_cleanup_calls > 1, "{label}");
                        match cleanup_target {
                            FsCasCleanupTargetV1::ObjectLocator => {
                                assert_eq!(control.carrier_cleanup_calls, 0, "{label}");
                            }
                            FsCasCleanupTargetV1::Carrier => {
                                assert_eq!(control.carrier_cleanup_calls, 1, "{label}");
                            }
                            _ => unreachable!(),
                        }

                        assert_operation_authority_baseline(&cas, fixture.path());
                        assert_storage_equations(&counters);
                        let (
                            (preparation_bytes, preparation_inodes),
                            (immutable_bytes, immutable_inodes),
                        ) = exact_operation_namespace_usage(fixture.path());
                        let (carrier_bytes, carrier_inodes) =
                            exact_directory_usage(&fixture.path().join("carriers"));
                        let (locator_bytes, locator_inodes) =
                            exact_directory_usage(&fixture.path().join("objects"));
                        assert_eq!((preparation_bytes, preparation_inodes), (0, 0), "{label}");
                        assert_eq!(carrier_inodes, 1, "{label}");
                        let expected_locator_inodes = u64::from(
                            cleanup_target == FsCasCleanupTargetV1::ObjectLocator,
                        );
                        assert_eq!(locator_inodes, expected_locator_inodes, "{label}");
                        assert_eq!(immutable_bytes, carrier_bytes + locator_bytes, "{label}");
                        assert_eq!(
                            immutable_inodes,
                            carrier_inodes + locator_inodes,
                            "{label}"
                        );
                        let expected_direct_residue =
                            if inject_accounting && cleanup_target == FsCasCleanupTargetV1::Carrier
                            {
                                0
                            } else {
                                immutable_bytes
                            };
                        assert_eq!(
                            counters.unreachable_installed_residue_bytes,
                            expected_direct_residue,
                            "{label}"
                        );
                        assert_eq!(counters.storage_bytes_committed, 0, "{label}");
                        assert_eq!(counters.storage_inodes_committed, 0, "{label}");
                        assert_eq!(
                            counters.storage_bytes_retained, immutable_bytes,
                            "{label}"
                        );
                        assert_eq!(
                            counters.storage_inodes_retained, immutable_inodes,
                            "{label}"
                        );
                        assert_eq!(counters.immutable_residue_bytes, immutable_bytes, "{label}");
                        assert_eq!(
                            counters.immutable_residue_inodes, immutable_inodes,
                            "{label}"
                        );
                        assert!(counters.has_zero_forbidden_work(), "{label}");
                        assert!(
                            matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
                            "{label}"
                        );
                        assert!(
                            matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
                            "{label}"
                        );
                        assert!(matches!(
                            FsCasV1::open_existing(fixture.path()),
                            Err(FsCasErrorV1::Busy | FsCasErrorV1::Invalidated)
                        ));
                    }
                }
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn post_link_alias_cleanup_failure_retains_visible_dependencies_and_invalidates_reopen() {
    std::thread::Builder::new()
        .name("post-link-alias-cleanup".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            for (target, prefix, catalog_visible, closure_visible) in [
                (
                    FsCasBoundaryV1::AfterObjectLocatorMarkerLink,
                    "object-",
                    false,
                    false,
                ),
                (
                    FsCasBoundaryV1::AfterCatalogMarkerLink,
                    "catalog-",
                    true,
                    false,
                ),
                (
                    FsCasBoundaryV1::AfterClosureMarkerLink,
                    "closure-",
                    true,
                    true,
                ),
            ] {
                let fixture = TestRoot::new(prefix);
                let cas = FsCasV1::create_new(fixture.path()).unwrap();
                let stale = FsCasV1::open_existing(fixture.path()).unwrap();
                let mut counters = OperationCountersV1::default();
                let input = [0x6d_u8; 64 * 1024 + 17];
                let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
                let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
                let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
                let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
                let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
                let mut tree_pages = [None::<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES];
                let mut traversal = [0_u8; 64];
                let mut control = FailPublishedMarkerAliasAt {
                    target,
                    current: None,
                    first_error: None,
                    fail_invalidation: false,
                    injected: false,
                };
                let grant =
                    request_create_operation_v1(&cas, 103, &mut counters, &mut control).unwrap();

                let error = run_create_v1(
                    grant,
                    CdcAlgorithmV1::FastCdc,
                    b"payload.bin",
                    0o644,
                    input.len() as u64,
                    CheckedSupplier { bytes: &input },
                    OperationBuffersV1 {
                        source: &mut source_window,
                        cdc_ring: &mut cdc_ring,
                        incoming_comparison: &mut incoming,
                        occupied_comparison: &mut occupied,
                        tree_object: &mut tree_object,
                        tree_pages: &mut tree_pages,
                        traversal_state: &mut traversal,
                    },
                    &mut control,
                    &mut counters,
                )
                .unwrap_err();

                assert_eq!(
                    error,
                    OperationErrorV1::FsCas(FsCasErrorV1::CleanupFailed(
                        FsCasCleanupTargetV1::PublishedMarkerAlias,
                    ))
                );
                assert!(control.injected);
                assert_eq!(
                    fs::read_dir(fixture.path().join("carriers"))
                        .unwrap()
                        .count(),
                    1
                );
                assert_eq!(
                    fs::read_dir(fixture.path().join("catalog"))
                        .unwrap()
                        .count(),
                    usize::from(catalog_visible)
                );
                assert!(
                    fs::read_dir(fixture.path().join("objects"))
                        .unwrap()
                        .count()
                        > 0
                );
                assert_eq!(
                    fs::read_dir(fixture.path().join("closures"))
                        .unwrap()
                        .count(),
                    usize::from(closure_visible)
                );
                let preparation: Vec<_> = fs::read_dir(fixture.path().join("preparation"))
                    .unwrap()
                    .map(|entry| entry.unwrap())
                    .collect();
                assert_eq!(preparation.len(), 1);
                assert!(preparation[0]
                    .file_name()
                    .to_string_lossy()
                    .starts_with(prefix));
                assert!(preparation[0].metadata().unwrap().len() > 0);
                let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
                    exact_operation_namespace_usage(fixture.path());
                assert_eq!(preparation_inodes, 1, "{target:?}");
                assert_eq!(counters.storage_bytes_committed, 0, "{target:?}");
                assert_eq!(counters.storage_inodes_committed, 0, "{target:?}");
                assert_eq!(
                    counters.storage_bytes_retained,
                    preparation_bytes + immutable_bytes,
                    "{target:?}"
                );
                assert_eq!(
                    counters.storage_inodes_retained,
                    preparation_inodes + immutable_inodes,
                    "{target:?}"
                );
                assert_eq!(
                    counters.mutable_preparation_residue_bytes, preparation_bytes,
                    "{target:?}"
                );
                assert_eq!(
                    counters.mutable_preparation_residue_inodes, preparation_inodes,
                    "{target:?}"
                );
                assert_eq!(
                    counters.immutable_residue_inodes, immutable_inodes,
                    "{target:?}"
                );
                assert!(fixture.path().join("invalidated").is_dir());
                assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
                assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
                assert!(matches!(
                    FsCasV1::open_existing(fixture.path()),
                    Err(FsCasErrorV1::Invalidated)
                ));
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn visible_locator_terminal_retains_carrier_when_residue_accounting_fails() {
    std::thread::Builder::new()
        .name("visible-locator-residue-accounting".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            for accounting_boundary in [
                FsCasResidueAccountingBoundaryV1::ObjectLocator,
                FsCasResidueAccountingBoundaryV1::Carrier,
            ] {
                for fail_invalidation in [false, true] {
                    let label = format!(
                        "visible-locator-{accounting_boundary:?}-{}",
                        if fail_invalidation {
                            "invalidation-double-fault"
                        } else {
                            "cleanup"
                        }
                    );
                    let fixture = TestRoot::new(&label);
                    let cas = FsCasV1::create_new(fixture.path()).unwrap();
                    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
                    let bound_invoked = AtomicBool::new(false);
                    let supply_invoked = AtomicBool::new(false);
                    let mut control = FailVisibleLocatorResidueAccountingV1 {
                        current: None,
                        fail_alias: true,
                        alias_boundary: FsCasBoundaryV1::AfterObjectLocatorMarkerLink,
                        first_error: None,
                        accounting_boundary: Some(accounting_boundary),
                        post_catalog_control_failure: None,
                        fail_invalidation,
                        alias_injected: false,
                        accounting_injected: false,
                        root_invalidation_calls: 0,
                    };

                    let (result, counters) = run_small_create_with_callback_observation(
                        &cas,
                        0x92a,
                        &mut control,
                        &bound_invoked,
                        &supply_invoked,
                    );
                    let expected = if fail_invalidation {
                        FsCasErrorV1::TerminalFailure {
                            first: FsCasFailureCauseV1::CleanupFailed(
                                FsCasCleanupTargetV1::PublishedMarkerAlias,
                            ),
                            dominant: FsCasFailureCauseV1::InvalidationFailed,
                        }
                    } else {
                        FsCasErrorV1::CleanupFailed(FsCasCleanupTargetV1::PublishedMarkerAlias)
                    };
                    assert_eq!(result, Err(OperationErrorV1::FsCas(expected)), "{label}");
                    assert!(control.alias_injected, "{label}");
                    assert!(control.accounting_injected, "{label}");
                    assert!(control.root_invalidation_calls >= 1, "{label}");
                    assert!(bound_invoked.load(Ordering::Acquire), "{label}");
                    assert!(supply_invoked.load(Ordering::Acquire), "{label}");
                    assert_eq!(cas.operation_admitted_slots_v1(), 0, "{label}");
                    assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{label}");
                    assert_eq!(
                        cas.storage_admission_active_for_test_v1(),
                        (0, 0, 0),
                        "{label}",
                    );

                    let (carrier_bytes, carrier_inodes) =
                        exact_directory_usage(&fixture.path().join("carriers"));
                    let (locator_bytes, locator_inodes) =
                        exact_directory_usage(&fixture.path().join("objects"));
                    assert_eq!(carrier_inodes, 1, "{label}");
                    assert_eq!(locator_inodes, 1, "{label}");
                    assert_eq!(
                        exact_directory_usage(&fixture.path().join("catalog")),
                        (0, 0),
                        "{label}",
                    );
                    assert_eq!(
                        exact_directory_usage(&fixture.path().join("closures")),
                        (0, 0),
                        "{label}",
                    );
                    let (
                        (preparation_bytes, preparation_inodes),
                        (immutable_bytes, immutable_inodes),
                    ) = exact_operation_namespace_usage(fixture.path());
                    assert!(preparation_bytes > 0, "{label}");
                    assert_eq!(preparation_inodes, 1, "{label}");
                    assert_eq!(immutable_bytes, carrier_bytes + locator_bytes, "{label}");
                    assert_eq!(immutable_inodes, carrier_inodes + locator_inodes, "{label}");

                    assert_storage_equations(&counters);
                    assert_eq!(counters.storage_bytes_committed, 0, "{label}");
                    assert_eq!(counters.storage_inodes_committed, 0, "{label}");
                    assert_eq!(
                        counters.storage_bytes_retained,
                        preparation_bytes + immutable_bytes,
                        "{label}",
                    );
                    assert_eq!(
                        counters.storage_inodes_retained,
                        preparation_inodes + immutable_inodes,
                        "{label}",
                    );
                    assert_eq!(
                        counters.mutable_preparation_residue_bytes, preparation_bytes,
                        "{label}",
                    );
                    assert_eq!(
                        counters.mutable_preparation_residue_inodes, preparation_inodes,
                        "{label}",
                    );
                    assert_eq!(counters.immutable_residue_bytes, immutable_bytes, "{label}");
                    assert_eq!(
                        counters.immutable_residue_inodes, immutable_inodes,
                        "{label}",
                    );
                    let directly_observed_residue = match accounting_boundary {
                        FsCasResidueAccountingBoundaryV1::CatalogMarker => unreachable!(),
                        FsCasResidueAccountingBoundaryV1::ObjectLocator => carrier_bytes,
                        FsCasResidueAccountingBoundaryV1::Carrier => locator_bytes,
                    };
                    assert_eq!(
                        counters.unreachable_installed_residue_bytes, directly_observed_residue,
                        "{label}",
                    );
                    assert!(counters.has_zero_forbidden_work(), "{label}");
                    assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
                    assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
                    assert!(matches!(
                        FsCasV1::open_existing(fixture.path()),
                        Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy)
                    ));
                }
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn visible_catalog_terminal_attempts_every_dependency_custody_once() {
    std::thread::Builder::new()
        .name("visible-catalog-residue-accounting".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let directional = FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied);
            for accounting_boundary in [
                FsCasResidueAccountingBoundaryV1::CatalogMarker,
                FsCasResidueAccountingBoundaryV1::ObjectLocator,
                FsCasResidueAccountingBoundaryV1::Carrier,
            ] {
                for first_error in [None, Some(directional)] {
                    for fail_invalidation in [false, true] {
                        let label = format!(
                            "visible-catalog-{accounting_boundary:?}-{}-{}",
                            if first_error.is_some() {
                                "directional"
                            } else {
                                "synthetic"
                            },
                            if fail_invalidation {
                                "invalidation-double-fault"
                            } else {
                                "cleanup"
                            }
                        );
                        let fixture = TestRoot::new(&label);
                        let cas = FsCasV1::create_new(fixture.path()).unwrap();
                        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
                        let bound_invoked = AtomicBool::new(false);
                        let supply_invoked = AtomicBool::new(false);
                        let mut control = FailVisibleLocatorResidueAccountingV1 {
                            current: None,
                            fail_alias: true,
                            alias_boundary: FsCasBoundaryV1::AfterCatalogMarkerLink,
                            first_error,
                            accounting_boundary: Some(accounting_boundary),
                            post_catalog_control_failure: None,
                            fail_invalidation,
                            alias_injected: false,
                            accounting_injected: false,
                            root_invalidation_calls: 0,
                        };

                        let (result, counters) = run_small_create_with_callback_observation(
                            &cas,
                            0x92b,
                            &mut control,
                            &bound_invoked,
                            &supply_invoked,
                        );
                        let dominant = if fail_invalidation {
                            FsCasFailureCauseV1::InvalidationFailed
                        } else {
                            FsCasFailureCauseV1::CleanupFailed(
                                FsCasCleanupTargetV1::PublishedMarkerAlias,
                            )
                        };
                        let expected = match first_error {
                            Some(_) => FsCasErrorV1::TerminalFailure {
                                first: FsCasFailureCauseV1::Filesystem(
                                    FsCasFilesystemFailureV1::PermissionDenied,
                                ),
                                dominant,
                            },
                            None if fail_invalidation => FsCasErrorV1::TerminalFailure {
                                first: FsCasFailureCauseV1::CleanupFailed(
                                    FsCasCleanupTargetV1::PublishedMarkerAlias,
                                ),
                                dominant,
                            },
                            None => FsCasErrorV1::CleanupFailed(
                                FsCasCleanupTargetV1::PublishedMarkerAlias,
                            ),
                        };
                        assert_eq!(result, Err(OperationErrorV1::FsCas(expected)), "{label}",);
                        assert!(control.alias_injected, "{label}");
                        assert!(control.accounting_injected, "{label}");
                        assert_eq!(control.root_invalidation_calls, 1, "{label}");
                        assert!(bound_invoked.load(Ordering::Acquire), "{label}");
                        assert!(supply_invoked.load(Ordering::Acquire), "{label}");
                        assert_eq!(cas.operation_admitted_slots_v1(), 0, "{label}");
                        assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{label}");
                        assert_eq!(
                            cas.storage_admission_active_for_test_v1(),
                            (0, 0, 0),
                            "{label}",
                        );

                        let (carrier_bytes, carrier_inodes) =
                            exact_directory_usage(&fixture.path().join("carriers"));
                        let (locator_bytes, locator_inodes) =
                            exact_directory_usage(&fixture.path().join("objects"));
                        let (catalog_bytes, catalog_inodes) =
                            exact_directory_usage(&fixture.path().join("catalog"));
                        assert_eq!(carrier_inodes, 1, "{label}");
                        assert!(locator_inodes > 0, "{label}");
                        assert_eq!(catalog_inodes, 1, "{label}");
                        assert_eq!(
                            exact_directory_usage(&fixture.path().join("closures")),
                            (0, 0),
                            "{label}",
                        );
                        let (
                            (preparation_bytes, preparation_inodes),
                            (immutable_bytes, immutable_inodes),
                        ) = exact_operation_namespace_usage(fixture.path());
                        assert!(preparation_bytes > 0, "{label}");
                        assert_eq!(preparation_inodes, 1, "{label}");
                        assert_eq!(
                            immutable_bytes,
                            carrier_bytes + locator_bytes + catalog_bytes,
                            "{label}",
                        );
                        assert_eq!(
                            immutable_inodes,
                            carrier_inodes + locator_inodes + catalog_inodes,
                            "{label}",
                        );

                        assert_storage_equations(&counters);
                        assert_eq!(counters.storage_bytes_committed, 0, "{label}");
                        assert_eq!(counters.storage_inodes_committed, 0, "{label}");
                        assert_eq!(
                            counters.storage_bytes_retained,
                            preparation_bytes + immutable_bytes,
                            "{label}",
                        );
                        assert_eq!(
                            counters.storage_inodes_retained,
                            preparation_inodes + immutable_inodes,
                            "{label}",
                        );
                        assert_eq!(
                            counters.mutable_preparation_residue_bytes, preparation_bytes,
                            "{label}",
                        );
                        assert_eq!(
                            counters.mutable_preparation_residue_inodes, preparation_inodes,
                            "{label}",
                        );
                        assert_eq!(counters.immutable_residue_bytes, immutable_bytes, "{label}");
                        assert_eq!(
                            counters.immutable_residue_inodes, immutable_inodes,
                            "{label}",
                        );
                        let missed_bytes = match accounting_boundary {
                            FsCasResidueAccountingBoundaryV1::CatalogMarker => catalog_bytes,
                            FsCasResidueAccountingBoundaryV1::ObjectLocator => locator_bytes,
                            FsCasResidueAccountingBoundaryV1::Carrier => carrier_bytes,
                        };
                        assert_eq!(
                            counters.unreachable_installed_residue_bytes,
                            immutable_bytes - missed_bytes,
                            "{label}",
                        );
                        assert!(counters.has_zero_forbidden_work(), "{label}");
                        assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
                        assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
                        assert!(matches!(
                            FsCasV1::open_existing(fixture.path()),
                            Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy)
                        ));
                    }
                }
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn post_catalog_control_terminal_preserves_cause_and_all_dependency_custody() {
    std::thread::Builder::new()
        .name("post-catalog-control-custody".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            for control_failure in [
                PostCatalogControlFailureV1::Cancelled,
                PostCatalogControlFailureV1::Deadline,
            ] {
                for accounting_boundary in [
                    None,
                    Some(FsCasResidueAccountingBoundaryV1::CatalogMarker),
                    Some(FsCasResidueAccountingBoundaryV1::ObjectLocator),
                    Some(FsCasResidueAccountingBoundaryV1::Carrier),
                ] {
                    for fail_invalidation in [false, true] {
                        if accounting_boundary.is_none() && fail_invalidation {
                            continue;
                        }
                        let label = format!(
                            "post-catalog-{control_failure:?}-{accounting_boundary:?}-{}",
                            if fail_invalidation {
                                "invalidation-double-fault"
                            } else {
                                "terminal"
                            }
                        );
                        let fixture = TestRoot::new(&label);
                        let cas = FsCasV1::create_new(fixture.path()).unwrap();
                        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
                        let bound_invoked = AtomicBool::new(false);
                        let supply_invoked = AtomicBool::new(false);
                        let mut control = FailVisibleLocatorResidueAccountingV1 {
                            current: None,
                            fail_alias: false,
                            alias_boundary: FsCasBoundaryV1::AfterCatalogMarkerLink,
                            first_error: None,
                            accounting_boundary,
                            post_catalog_control_failure: Some(control_failure),
                            fail_invalidation,
                            alias_injected: false,
                            accounting_injected: false,
                            root_invalidation_calls: 0,
                        };

                        let (result, counters) = run_small_create_with_callback_observation(
                            &cas,
                            0x92c,
                            &mut control,
                            &bound_invoked,
                            &supply_invoked,
                        );
                        let first = match control_failure {
                            PostCatalogControlFailureV1::Cancelled => CoreError::Cancelled,
                            PostCatalogControlFailureV1::Deadline => CoreError::Deadline,
                        };
                        let expected = if fail_invalidation {
                            FsCasErrorV1::TerminalFailure {
                                first: FsCasFailureCauseV1::Core(first),
                                dominant: FsCasFailureCauseV1::InvalidationFailed,
                            }
                        } else {
                            FsCasErrorV1::Core(first)
                        };
                        assert_eq!(result, Err(OperationErrorV1::FsCas(expected)), "{label}");
                        assert!(!control.alias_injected, "{label}");
                        assert_eq!(
                            control.accounting_injected,
                            accounting_boundary.is_some(),
                            "{label}",
                        );
                        assert_eq!(
                            control.root_invalidation_calls,
                            u64::from(accounting_boundary.is_some()),
                            "{label}",
                        );
                        assert!(bound_invoked.load(Ordering::Acquire), "{label}");
                        assert!(supply_invoked.load(Ordering::Acquire), "{label}");
                        assert_eq!(cas.operation_admitted_slots_v1(), 0, "{label}");
                        assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{label}");
                        assert_eq!(
                            cas.storage_admission_active_for_test_v1(),
                            (0, 0, 0),
                            "{label}",
                        );

                        let (carrier_bytes, carrier_inodes) =
                            exact_directory_usage(&fixture.path().join("carriers"));
                        let (locator_bytes, locator_inodes) =
                            exact_directory_usage(&fixture.path().join("objects"));
                        let (catalog_bytes, catalog_inodes) =
                            exact_directory_usage(&fixture.path().join("catalog"));
                        assert_eq!(carrier_inodes, 1, "{label}");
                        assert!(locator_inodes > 0, "{label}");
                        assert_eq!(catalog_inodes, 1, "{label}");
                        assert_eq!(
                            exact_directory_usage(&fixture.path().join("closures")),
                            (0, 0),
                            "{label}",
                        );
                        let (
                            (preparation_bytes, preparation_inodes),
                            (immutable_bytes, immutable_inodes),
                        ) = exact_operation_namespace_usage(fixture.path());
                        assert_eq!((preparation_bytes, preparation_inodes), (0, 0), "{label}");
                        assert_eq!(
                            immutable_bytes,
                            carrier_bytes + locator_bytes + catalog_bytes,
                            "{label}",
                        );
                        assert_eq!(
                            immutable_inodes,
                            carrier_inodes + locator_inodes + catalog_inodes,
                            "{label}",
                        );

                        assert_storage_equations(&counters);
                        assert_eq!(counters.storage_bytes_committed, 0, "{label}");
                        assert_eq!(counters.storage_inodes_committed, 0, "{label}");
                        assert_eq!(counters.storage_bytes_retained, immutable_bytes, "{label}");
                        assert_eq!(
                            counters.storage_inodes_retained, immutable_inodes,
                            "{label}"
                        );
                        assert_eq!(counters.mutable_preparation_residue_bytes, 0, "{label}");
                        assert_eq!(counters.mutable_preparation_residue_inodes, 0, "{label}");
                        assert_eq!(counters.immutable_residue_bytes, immutable_bytes, "{label}");
                        assert_eq!(
                            counters.immutable_residue_inodes, immutable_inodes,
                            "{label}",
                        );
                        let missed_bytes = match accounting_boundary {
                            None => 0,
                            Some(FsCasResidueAccountingBoundaryV1::CatalogMarker) => catalog_bytes,
                            Some(FsCasResidueAccountingBoundaryV1::ObjectLocator) => locator_bytes,
                            Some(FsCasResidueAccountingBoundaryV1::Carrier) => carrier_bytes,
                        };
                        assert_eq!(
                            counters.unreachable_installed_residue_bytes,
                            immutable_bytes - missed_bytes,
                            "{label}",
                        );
                        assert!(counters.has_zero_forbidden_work(), "{label}");

                        if accounting_boundary.is_some() {
                            assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
                            assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
                            assert!(matches!(
                                FsCasV1::open_existing(fixture.path()),
                                Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy)
                            ));
                        } else {
                            assert!(cas.occupied().is_ok(), "{label}");
                            assert!(stale.occupied().is_ok(), "{label}");
                            assert!(FsCasV1::open_existing(fixture.path()).is_ok(), "{label}");
                        }
                    }
                }
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn admission_callback_unwind_classifies_secondary_terminal_and_dependency_custody() {
    std::thread::Builder::new()
        .name("admission-unwind-terminal-custody".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let mut cases = vec![
                (
                    "publication-lock-clean",
                    FsCasBoundaryV1::PublicationLockAcquired,
                    None,
                    AdmissionUnwindPrivateCleanupV1::Clean,
                    false,
                    true,
                ),
                (
                    "post-catalog-clean",
                    FsCasBoundaryV1::AfterCatalogPublication,
                    None,
                    AdmissionUnwindPrivateCleanupV1::Clean,
                    false,
                    true,
                ),
                (
                    "post-catalog-invalidation-double-fault",
                    FsCasBoundaryV1::AfterCatalogPublication,
                    None,
                    AdmissionUnwindPrivateCleanupV1::Clean,
                    true,
                    false,
                ),
            ];
            for accounting_boundary in [
                FsCasResidueAccountingBoundaryV1::CatalogMarker,
                FsCasResidueAccountingBoundaryV1::ObjectLocator,
                FsCasResidueAccountingBoundaryV1::Carrier,
            ] {
                for fail_invalidation in [false, true] {
                    cases.push((
                        if fail_invalidation {
                            "post-catalog-accounting-invalidation-double-fault"
                        } else {
                            "post-catalog-accounting"
                        },
                        FsCasBoundaryV1::AfterCatalogPublication,
                        Some(accounting_boundary),
                        AdmissionUnwindPrivateCleanupV1::Clean,
                        fail_invalidation,
                        false,
                    ));
                }
            }
            for private_cleanup in [
                AdmissionUnwindPrivateCleanupV1::Fails,
                AdmissionUnwindPrivateCleanupV1::Unwinds,
            ] {
                for fail_invalidation in [false, true] {
                    cases.push((
                        if fail_invalidation {
                            "publication-lock-private-cleanup-invalidation-double-fault"
                        } else {
                            "publication-lock-private-cleanup"
                        },
                        FsCasBoundaryV1::PublicationLockAcquired,
                        None,
                        private_cleanup,
                        fail_invalidation,
                        false,
                    ));
                }
            }

            for (
                index,
                (
                    prefix,
                    panic_boundary,
                    accounting_boundary,
                    private_cleanup,
                    fail_invalidation,
                    resumes_original,
                ),
            ) in cases.into_iter().enumerate()
            {
                let label = format!(
                    "{prefix}-{accounting_boundary:?}-{private_cleanup:?}-{}",
                    if fail_invalidation {
                        "double-fault"
                    } else {
                        "terminal"
                    }
                );
                let fixture = TestRoot::new(&label);
                let cas = FsCasV1::create_new(fixture.path()).unwrap();
                let stale = FsCasV1::open_existing(fixture.path()).unwrap();
                let mut counters = OperationCountersV1::default();
                let input = [0x7b_u8; 64 * 1024 + 17];
                let mut source_window = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
                let mut cdc_ring = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
                let mut incoming = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
                let mut occupied = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
                let mut tree_object = boxed_zeroes::<MAX_TREE_OBJECT_BYTES>();
                let mut tree_pages = boxed_tree_pages();
                let mut traversal = [0_u8; 64];
                let mut control = PanicAdmissionWithSecondaryTerminalV1 {
                    panic_boundary,
                    accounting_boundary,
                    private_cleanup,
                    fail_invalidation,
                    boundary_panicked: false,
                    accounting_injected: false,
                    private_cleanup_calls: 0,
                    root_invalidation_calls: 0,
                };
                let grant = request_create_operation_v1(
                    &cas,
                    0x92d + index as u64,
                    &mut counters,
                    &mut control,
                )
                .unwrap();

                let terminal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_create_v1(
                        grant,
                        CdcAlgorithmV1::FastCdc,
                        b"payload.bin",
                        0o644,
                        input.len() as u64,
                        CheckedSupplier { bytes: &input },
                        OperationBuffersV1 {
                            source: &mut source_window,
                            cdc_ring: &mut cdc_ring,
                            incoming_comparison: &mut incoming,
                            occupied_comparison: &mut occupied,
                            tree_object: &mut tree_object,
                            tree_pages: &mut *tree_pages,
                            traversal_state: &mut traversal,
                        },
                        &mut control,
                        &mut counters,
                    )
                }));

                if resumes_original {
                    let payload = terminal.expect_err("clean callback unwind must be resumed");
                    assert_eq!(
                        payload.downcast_ref::<&'static str>().copied(),
                        Some("injected admission callback unwind"),
                        "{label}",
                    );
                } else {
                    let expected = if panic_boundary == FsCasBoundaryV1::AfterCatalogPublication {
                        match accounting_boundary {
                            Some(_) if fail_invalidation => FsCasErrorV1::TerminalFailure {
                                first: FsCasFailureCauseV1::Core(CoreError::IntegerOverflow),
                                dominant: FsCasFailureCauseV1::InvalidationFailed,
                            },
                            Some(_) => FsCasErrorV1::Core(CoreError::IntegerOverflow),
                            None => FsCasErrorV1::InvalidationFailed,
                        }
                    } else if fail_invalidation {
                        FsCasErrorV1::TerminalFailure {
                            first: FsCasFailureCauseV1::CleanupFailed(
                                FsCasCleanupTargetV1::PrivatePack,
                            ),
                            dominant: FsCasFailureCauseV1::InvalidationFailed,
                        }
                    } else {
                        FsCasErrorV1::CleanupFailed(FsCasCleanupTargetV1::PrivatePack)
                    };
                    assert_eq!(
                        terminal.unwrap().unwrap_err(),
                        OperationErrorV1::FsCas(expected),
                        "{label}",
                    );
                }

                assert!(control.boundary_panicked, "{label}");
                assert_eq!(
                    control.accounting_injected,
                    accounting_boundary.is_some(),
                    "{label}",
                );
                assert_eq!(
                    control.private_cleanup_calls,
                    u64::from(panic_boundary == FsCasBoundaryV1::PublicationLockAcquired),
                    "{label}",
                );
                let invalidation_expected = panic_boundary
                    == FsCasBoundaryV1::AfterCatalogPublication
                    || private_cleanup != AdmissionUnwindPrivateCleanupV1::Clean;
                assert_eq!(
                    control.root_invalidation_calls,
                    u64::from(invalidation_expected),
                    "{label}",
                );
                assert_eq!(cas.operation_admitted_slots_v1(), 0, "{label}");
                assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{label}");
                assert_eq!(
                    cas.storage_admission_active_for_test_v1(),
                    (0, 0, 0),
                    "{label}",
                );
                assert!(cas.visibility_lock_available_for_test_v1(), "{label}");
                assert!(cas.publication_lock_available_for_test_v1(), "{label}");

                let (carrier_bytes, carrier_inodes) =
                    exact_directory_usage(&fixture.path().join("carriers"));
                let (locator_bytes, locator_inodes) =
                    exact_directory_usage(&fixture.path().join("objects"));
                let (catalog_bytes, catalog_inodes) =
                    exact_directory_usage(&fixture.path().join("catalog"));
                assert_eq!(
                    exact_directory_usage(&fixture.path().join("closures")),
                    (0, 0),
                    "{label}",
                );
                let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
                    exact_operation_namespace_usage(fixture.path());

                if panic_boundary == FsCasBoundaryV1::AfterCatalogPublication {
                    assert_eq!((preparation_bytes, preparation_inodes), (0, 0), "{label}");
                    assert_eq!(carrier_inodes, 1, "{label}");
                    assert!(locator_inodes > 0, "{label}");
                    assert_eq!(catalog_inodes, 1, "{label}");
                    assert_eq!(
                        immutable_bytes,
                        carrier_bytes + locator_bytes + catalog_bytes,
                        "{label}",
                    );
                    assert_eq!(
                        immutable_inodes,
                        carrier_inodes + locator_inodes + catalog_inodes,
                        "{label}",
                    );
                    assert_eq!(counters.storage_bytes_retained, immutable_bytes, "{label}");
                    assert_eq!(
                        counters.storage_inodes_retained, immutable_inodes,
                        "{label}"
                    );
                    assert_eq!(counters.mutable_preparation_residue_bytes, 0, "{label}");
                    assert_eq!(counters.mutable_preparation_residue_inodes, 0, "{label}");
                    assert_eq!(counters.immutable_residue_bytes, immutable_bytes, "{label}");
                    assert_eq!(
                        counters.immutable_residue_inodes, immutable_inodes,
                        "{label}",
                    );
                    let missed_bytes = match accounting_boundary {
                        None => 0,
                        Some(FsCasResidueAccountingBoundaryV1::CatalogMarker) => catalog_bytes,
                        Some(FsCasResidueAccountingBoundaryV1::ObjectLocator) => locator_bytes,
                        Some(FsCasResidueAccountingBoundaryV1::Carrier) => carrier_bytes,
                    };
                    assert_eq!(
                        counters.unreachable_installed_residue_bytes,
                        immutable_bytes - missed_bytes,
                        "{label}",
                    );
                } else {
                    assert_eq!((carrier_bytes, carrier_inodes), (0, 0), "{label}");
                    assert_eq!((locator_bytes, locator_inodes), (0, 0), "{label}");
                    assert_eq!((catalog_bytes, catalog_inodes), (0, 0), "{label}");
                    assert_eq!((immutable_bytes, immutable_inodes), (0, 0), "{label}");
                    let cleanup_failed = private_cleanup != AdmissionUnwindPrivateCleanupV1::Clean;
                    if cleanup_failed {
                        assert!(preparation_bytes > 0, "{label}");
                        assert_eq!(preparation_inodes, 1, "{label}");
                    } else {
                        assert_eq!((preparation_bytes, preparation_inodes), (0, 0), "{label}");
                    }
                    assert_eq!(
                        counters.storage_bytes_retained, preparation_bytes,
                        "{label}",
                    );
                    assert_eq!(
                        counters.storage_inodes_retained, preparation_inodes,
                        "{label}",
                    );
                    assert_eq!(
                        counters.mutable_preparation_residue_bytes, preparation_bytes,
                        "{label}",
                    );
                    assert_eq!(
                        counters.mutable_preparation_residue_inodes, preparation_inodes,
                        "{label}",
                    );
                    assert_eq!(counters.immutable_residue_bytes, 0, "{label}");
                    assert_eq!(counters.immutable_residue_inodes, 0, "{label}");
                    assert_eq!(counters.unreachable_installed_residue_bytes, 0, "{label}");
                }

                assert_storage_equations(&counters);
                assert_eq!(counters.storage_bytes_committed, 0, "{label}");
                assert_eq!(counters.storage_inodes_committed, 0, "{label}");
                assert!(counters.has_zero_forbidden_work(), "{label}");

                if invalidation_expected {
                    assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
                    assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
                    assert!(matches!(
                        FsCasV1::open_existing(fixture.path()),
                        Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy)
                    ));
                } else {
                    assert!(cas.occupied().is_ok(), "{label}");
                    assert!(stale.occupied().is_ok(), "{label}");
                    assert!(FsCasV1::open_existing(fixture.path()).is_ok(), "{label}");
                }
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn post_link_alias_directional_failure_retains_first_cause_across_visible_domains() {
    std::thread::Builder::new()
        .name("post-link-alias-directional".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let first_error = FsCasErrorV1::Filesystem(FsCasFilesystemFailureV1::PermissionDenied);
            for (target, prefix, catalog_visible, closure_visible) in [
                (
                    FsCasBoundaryV1::AfterObjectLocatorMarkerLink,
                    "object",
                    false,
                    false,
                ),
                (
                    FsCasBoundaryV1::AfterCatalogMarkerLink,
                    "catalog",
                    true,
                    false,
                ),
                (
                    FsCasBoundaryV1::AfterClosureMarkerLink,
                    "closure",
                    true,
                    true,
                ),
            ] {
                for fail_invalidation in [false, true] {
                    let label = format!(
                        "{prefix}-alias-directional-{}",
                        if fail_invalidation {
                            "double-fault"
                        } else {
                            "cleanup"
                        }
                    );
                    let fixture = TestRoot::new(&label);
                    let cas = FsCasV1::create_new(fixture.path()).unwrap();
                    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
                    let bound_invoked = AtomicBool::new(false);
                    let supply_invoked = AtomicBool::new(false);
                    let mut control = FailPublishedMarkerAliasAt {
                        target,
                        current: None,
                        first_error: Some(first_error),
                        fail_invalidation,
                        injected: false,
                    };

                    let (result, counters) = run_small_create_with_callback_observation(
                        &cas,
                        0x910,
                        &mut control,
                        &bound_invoked,
                        &supply_invoked,
                    );
                    let expected_dominant = if fail_invalidation {
                        FsCasFailureCauseV1::InvalidationFailed
                    } else {
                        FsCasFailureCauseV1::CleanupFailed(
                            FsCasCleanupTargetV1::PublishedMarkerAlias,
                        )
                    };
                    assert_eq!(
                        result,
                        Err(OperationErrorV1::FsCas(FsCasErrorV1::TerminalFailure {
                            first: FsCasFailureCauseV1::Filesystem(
                                FsCasFilesystemFailureV1::PermissionDenied,
                            ),
                            dominant: expected_dominant,
                        })),
                        "{label}",
                    );
                    assert!(control.injected, "{label}");
                    assert!(bound_invoked.load(Ordering::Acquire), "{label}");
                    assert!(supply_invoked.load(Ordering::Acquire), "{label}");
                    assert_eq!(cas.operation_admitted_slots_v1(), 0, "{label}");
                    assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{label}");
                    assert_eq!(
                        cas.storage_admission_active_for_test_v1(),
                        (0, 0, 0),
                        "{label}",
                    );
                    assert_eq!(
                        fs::read_dir(fixture.path().join("carriers"))
                            .unwrap()
                            .count(),
                        1,
                        "{label}",
                    );
                    assert_eq!(
                        fs::read_dir(fixture.path().join("catalog"))
                            .unwrap()
                            .count(),
                        usize::from(catalog_visible),
                        "{label}",
                    );
                    assert!(
                        fs::read_dir(fixture.path().join("objects"))
                            .unwrap()
                            .count()
                            > 0,
                        "{label}",
                    );
                    assert_eq!(
                        fs::read_dir(fixture.path().join("closures"))
                            .unwrap()
                            .count(),
                        usize::from(closure_visible),
                        "{label}",
                    );
                    let preparation: Vec<_> = fs::read_dir(fixture.path().join("preparation"))
                        .unwrap()
                        .map(|entry| entry.unwrap())
                        .collect();
                    assert_eq!(preparation.len(), 1, "{label}");
                    assert!(
                        preparation[0]
                            .file_name()
                            .to_string_lossy()
                            .starts_with(prefix),
                        "{label}",
                    );
                    let (
                        (preparation_bytes, preparation_inodes),
                        (immutable_bytes, immutable_inodes),
                    ) = exact_operation_namespace_usage(fixture.path());
                    assert!(preparation_bytes > 0, "{label}");
                    assert_eq!(preparation_inodes, 1, "{label}");
                    assert_storage_equations(&counters);
                    assert_eq!(counters.storage_bytes_committed, 0, "{label}");
                    assert_eq!(counters.storage_inodes_committed, 0, "{label}");
                    assert_eq!(
                        counters.storage_bytes_retained,
                        preparation_bytes + immutable_bytes,
                        "{label}",
                    );
                    assert_eq!(
                        counters.storage_inodes_retained,
                        preparation_inodes + immutable_inodes,
                        "{label}",
                    );
                    assert_eq!(
                        counters.mutable_preparation_residue_bytes, preparation_bytes,
                        "{label}",
                    );
                    assert_eq!(
                        counters.mutable_preparation_residue_inodes, preparation_inodes,
                        "{label}",
                    );
                    assert_eq!(
                        counters.unreachable_installed_residue_bytes, immutable_bytes,
                        "{label}",
                    );
                    assert_eq!(counters.immutable_residue_bytes, immutable_bytes, "{label}",);
                    assert_eq!(
                        counters.immutable_residue_inodes, immutable_inodes,
                        "{label}",
                    );
                    assert!(counters.has_zero_forbidden_work(), "{label}");
                    assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
                    assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
                    match FsCasV1::open_existing(fixture.path()) {
                        Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
                        Err(error) => panic!("{label}: unexpected reopen error {error:?}"),
                        Ok(_) => panic!("{label}: fail-closed root reopened as usable"),
                    }
                }
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn atomic_closure_no_replace_authenticates_a_racing_malformed_occupant() {
    let fixture = TestRoot::new("atomic-closure-race");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let input = [0x43_u8; 64 * 1024 + 29];
    let mut source_window = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
    let mut cdc_ring = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
    let mut incoming = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut occupied = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut tree_object = boxed_zeroes::<MAX_TREE_OBJECT_BYTES>();
    let mut tree_pages = boxed_tree_pages();
    let mut traversal = [0_u8; 64];

    let mut first_control = ContinueControl;
    let mut first_counters = OperationCountersV1::default();
    let first_grant =
        request_create_operation_v1(&cas, 0x410, &mut first_counters, &mut first_control).unwrap();
    run_create_v1(
        first_grant,
        CdcAlgorithmV1::FastCdc,
        b"payload.bin",
        0o644,
        input.len() as u64,
        CheckedSupplier { bytes: &input },
        OperationBuffersV1 {
            source: &mut source_window,
            cdc_ring: &mut cdc_ring,
            incoming_comparison: &mut incoming,
            occupied_comparison: &mut occupied,
            tree_object: &mut tree_object,
            tree_pages: &mut *tree_pages,
            traversal_state: &mut traversal,
        },
        &mut first_control,
        &mut first_counters,
    )
    .unwrap();

    let mut closure_entries = fs::read_dir(fixture.path().join("closures")).unwrap();
    let closure = closure_entries.next().unwrap().unwrap().path();
    assert!(closure_entries.next().is_none());
    fs::remove_file(&closure).unwrap();
    let carrier_count = fs::read_dir(fixture.path().join("carriers"))
        .unwrap()
        .count();
    let catalog_count = fs::read_dir(fixture.path().join("catalog"))
        .unwrap()
        .count();
    let object_count = fs::read_dir(fixture.path().join("objects"))
        .unwrap()
        .count();

    let mut control = InstallMalformedClosureAtPublication {
        destination: closure.clone(),
        injected: false,
    };
    let mut counters = OperationCountersV1::default();
    let grant = request_create_operation_v1(&cas, 0x411, &mut counters, &mut control).unwrap();
    let error = run_create_v1(
        grant,
        CdcAlgorithmV1::FastCdc,
        b"payload.bin",
        0o644,
        input.len() as u64,
        CheckedSupplier { bytes: &input },
        OperationBuffersV1 {
            source: &mut source_window,
            cdc_ring: &mut cdc_ring,
            incoming_comparison: &mut incoming,
            occupied_comparison: &mut occupied,
            tree_object: &mut tree_object,
            tree_pages: &mut *tree_pages,
            traversal_state: &mut traversal,
        },
        &mut control,
        &mut counters,
    )
    .unwrap_err();

    assert_eq!(
        error,
        OperationErrorV1::FsCas(FsCasErrorV1::MalformedOccupant)
    );
    assert!(control.injected);
    assert_eq!(fs::read(&closure).unwrap(), [0_u8; 120]);
    assert_eq!(
        fs::read_dir(fixture.path().join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        fs::read_dir(fixture.path().join("carriers"))
            .unwrap()
            .count(),
        carrier_count
    );
    assert_eq!(
        fs::read_dir(fixture.path().join("catalog"))
            .unwrap()
            .count(),
        catalog_count
    );
    assert_eq!(
        fs::read_dir(fixture.path().join("objects"))
            .unwrap()
            .count(),
        object_count
    );
    assert_eq!(counters.closure_fences, 0);
    assert_eq!(counters.unreachable_installed_residue_bytes, 0);
    assert_eq!(cas.operation_admitted_slots_v1(), 0);
    assert!(counters.has_zero_forbidden_work());
}

#[test]
fn malformed_closure_admission_preserves_primary_error_through_marker_cleanup_terminal() {
    // This proves the complete operation path, rather than only the closure
    // fence adapter: malformed closure admission is the first semantic error,
    // while an explicit preparation cleanup or its invalidation double fault
    // is the only later terminal allowed to dominate it.
    std::thread::Builder::new()
        .name("malformed-closure-cleanup-provenance".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            for fail_invalidation in [false, true] {
                let label = if fail_invalidation {
                    "malformed-closure-cleanup-invalidation-double-fault"
                } else {
                    "malformed-closure-cleanup-failure"
                };
                let fixture = TestRoot::new(label);
                let cas = FsCasV1::create_new(fixture.path()).unwrap();
                let stale = FsCasV1::open_existing(fixture.path()).unwrap();
                let input = [0x4c_u8; 64 * 1024 + 29];
                let mut source_window = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
                let mut cdc_ring = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
                let mut incoming = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
                let mut occupied = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
                let mut tree_object = boxed_zeroes::<MAX_TREE_OBJECT_BYTES>();
                let mut tree_pages = boxed_tree_pages();
                let mut traversal = [0_u8; 64];

                let mut first_control = ContinueControl;
                let mut first_counters = OperationCountersV1::default();
                let first_grant = request_create_operation_v1(
                    &cas,
                    0x412,
                    &mut first_counters,
                    &mut first_control,
                )
                .unwrap();
                run_create_v1(
                    first_grant,
                    CdcAlgorithmV1::FastCdc,
                    b"payload.bin",
                    0o644,
                    input.len() as u64,
                    CheckedSupplier { bytes: &input },
                    OperationBuffersV1 {
                        source: &mut source_window,
                        cdc_ring: &mut cdc_ring,
                        incoming_comparison: &mut incoming,
                        occupied_comparison: &mut occupied,
                        tree_object: &mut tree_object,
                        tree_pages: &mut *tree_pages,
                        traversal_state: &mut traversal,
                    },
                    &mut first_control,
                    &mut first_counters,
                )
                .unwrap();

                let mut closure_entries = fs::read_dir(fixture.path().join("closures")).unwrap();
                let closure = closure_entries.next().unwrap().unwrap().path();
                assert!(closure_entries.next().is_none(), "{label}");
                fs::remove_file(&closure).unwrap();
                let (
                    (before_preparation_bytes, before_preparation_inodes),
                    (before_immutable_bytes, before_immutable_inodes),
                ) = exact_operation_namespace_usage(fixture.path());
                assert_eq!(
                    (before_preparation_bytes, before_preparation_inodes),
                    (0, 0),
                    "{label}"
                );

                let mut control = InstallMalformedClosureAndFailPreparationCleanupV1 {
                    destination: closure.clone(),
                    malformed_installed: false,
                    preparation_cleanup_calls: 0,
                    preparation_cleanup_injected: false,
                    root_invalidation_calls: 0,
                    fail_invalidation,
                };
                let mut counters = OperationCountersV1::default();
                let grant =
                    request_create_operation_v1(&cas, 0x413, &mut counters, &mut control).unwrap();
                let result = run_create_v1(
                    grant,
                    CdcAlgorithmV1::FastCdc,
                    b"payload.bin",
                    0o644,
                    input.len() as u64,
                    CheckedSupplier { bytes: &input },
                    OperationBuffersV1 {
                        source: &mut source_window,
                        cdc_ring: &mut cdc_ring,
                        incoming_comparison: &mut incoming,
                        occupied_comparison: &mut occupied,
                        tree_object: &mut tree_object,
                        tree_pages: &mut *tree_pages,
                        traversal_state: &mut traversal,
                    },
                    &mut control,
                    &mut counters,
                );

                let expected = FsCasErrorV1::TerminalFailure {
                    first: FsCasFailureCauseV1::MalformedOccupant,
                    dominant: if fail_invalidation {
                        FsCasFailureCauseV1::InvalidationFailed
                    } else {
                        FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PreparationSpool)
                    },
                };
                assert_eq!(result, Err(OperationErrorV1::FsCas(expected)), "{label}");
                assert!(control.malformed_installed, "{label}");
                assert!(control.preparation_cleanup_injected, "{label}");
                // One explicit cleanup belongs to the temporary closure marker
                // itself; the five preparation spools that existed before the
                // closure fence then each receive their own terminal attempt.
                assert_eq!(control.preparation_cleanup_calls, 6, "{label}");
                assert_eq!(control.root_invalidation_calls, 1, "{label}");
                assert_eq!(fs::read(&closure).unwrap(), [0_u8; 120], "{label}");

                let (
                    (after_preparation_bytes, after_preparation_inodes),
                    (after_immutable_bytes, after_immutable_inodes),
                ) = exact_operation_namespace_usage(fixture.path());
                let preparation_bytes = after_preparation_bytes - before_preparation_bytes;
                let preparation_inodes = after_preparation_inodes - before_preparation_inodes;
                assert!(preparation_bytes > 0, "{label}");
                assert_eq!(preparation_inodes, 1, "{label}");
                // The malformed closure is a racing external occupant, not an
                // operation-owned immutable residue. The only immutable delta
                // is its known 120-byte namespace entry.
                assert_eq!(
                    after_immutable_bytes,
                    before_immutable_bytes + 120,
                    "{label}"
                );
                assert_eq!(
                    after_immutable_inodes,
                    before_immutable_inodes + 1,
                    "{label}"
                );
                assert_eq!(counters.storage_bytes_committed, 0, "{label}");
                assert_eq!(counters.storage_inodes_committed, 0, "{label}");
                assert_eq!(
                    counters.storage_bytes_retained, preparation_bytes,
                    "{label}"
                );
                assert_eq!(
                    counters.storage_inodes_retained, preparation_inodes,
                    "{label}"
                );
                assert_eq!(
                    counters.mutable_preparation_residue_bytes, preparation_bytes,
                    "{label}"
                );
                assert_eq!(
                    counters.mutable_preparation_residue_inodes, preparation_inodes,
                    "{label}"
                );
                assert_eq!(counters.immutable_residue_bytes, 0, "{label}");
                assert_eq!(counters.immutable_residue_inodes, 0, "{label}");
                assert_eq!(counters.unreachable_installed_residue_bytes, 0, "{label}");
                assert_storage_equations(&counters);
                assert!(counters.has_zero_forbidden_work(), "{label}");

                assert_eq!(cas.operation_admitted_slots_v1(), 0, "{label}");
                assert_eq!(cas.operation_admission_active_for_test_v1(), 0, "{label}");
                assert_eq!(
                    cas.storage_admission_active_for_test_v1(),
                    (0, 0, 0),
                    "{label}"
                );
                assert!(cas.visibility_lock_available_for_test_v1(), "{label}");
                assert!(cas.publication_lock_available_for_test_v1(), "{label}");
                assert!(
                    matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
                    "{label}"
                );
                assert!(
                    matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
                    "{label}"
                );
                match FsCasV1::open_existing(fixture.path()) {
                    Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
                    Err(error) => panic!("{label}: unexpected reopen error {error:?}"),
                    Ok(_) => panic!("{label}: fail-closed root reopened as usable"),
                }
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[derive(Default)]
struct CancelBeforeCandidateValidationAndFailPrivatePackCleanupOnce {
    cancel: bool,
    cleanup_injected: bool,
}

impl CdcControlV1 for CancelBeforeCandidateValidationAndFailPrivatePackCleanupOnce {
    fn cancellation_requested(&mut self) -> bool {
        self.cancel
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for CancelBeforeCandidateValidationAndFailPrivatePackCleanupOnce {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if boundary == FsCasBoundaryV1::BeforeCandidateValidation {
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
        if target == FsCasCleanupTargetV1::PrivatePack && !self.cleanup_injected {
            self.cleanup_injected = true;
            true
        } else {
            false
        }
    }
}

struct FailInvalidationProbeBeforeCandidateValidation {
    cas: FsCasV1,
    failure: Option<FsCasErrorV1>,
    injected: bool,
}

impl CdcControlV1 for FailInvalidationProbeBeforeCandidateValidation {
    fn cancellation_requested(&mut self) -> bool {
        false
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for FailInvalidationProbeBeforeCandidateValidation {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if boundary == FsCasBoundaryV1::BeforeCandidateValidation && !self.injected {
            self.injected = true;
            self.cas
                .fail_next_invalidation_probe_for_test_v1(self.failure.take().unwrap());
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
fn invalidation_probe_failure_before_candidate_validation_preserves_typed_cause() {
    for (case, operation, failure) in [
        (
            "candidate-validation-permission",
            0x7c0,
            FsCasFilesystemFailureV1::PermissionDenied,
        ),
        (
            "candidate-validation-read",
            0x7c1,
            FsCasFilesystemFailureV1::ReadFailure,
        ),
    ] {
        let fixture = TestRoot::new(case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let mut counters = OperationCountersV1::default();
        let input = [0x4d_u8; 64 * 1024 + 17];
        let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
        let mut tree_pages = [None::<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES];
        let mut traversal = [0_u8; 64];
        let expected = FsCasErrorV1::Filesystem(failure);
        let mut control = FailInvalidationProbeBeforeCandidateValidation {
            cas: cas.clone(),
            failure: Some(expected),
            injected: false,
        };
        let grant =
            request_create_operation_v1(&cas, operation, &mut counters, &mut control).unwrap();

        let error = run_create_v1(
            grant,
            CdcAlgorithmV1::FastCdc,
            b"payload.bin",
            0o644,
            input.len() as u64,
            CheckedSupplier { bytes: &input },
            OperationBuffersV1 {
                source: &mut source_window,
                cdc_ring: &mut cdc_ring,
                incoming_comparison: &mut incoming,
                occupied_comparison: &mut occupied,
                tree_object: &mut tree_object,
                tree_pages: &mut tree_pages,
                traversal_state: &mut traversal,
            },
            &mut control,
            &mut counters,
        )
        .unwrap_err();

        assert_eq!(error, OperationErrorV1::FsCas(expected), "{case}");
        assert!(control.injected, "{case}");
        assert_operation_authority_baseline(&cas, fixture.path());
        assert_storage_equations(&counters);
        let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
            exact_operation_namespace_usage(fixture.path());
        assert_eq!((preparation_bytes, preparation_inodes), (0, 0), "{case}");
        assert_eq!((immutable_bytes, immutable_inodes), (0, 0), "{case}");
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert_eq!(counters.storage_bytes_retained, 0, "{case}");
        assert_eq!(counters.storage_inodes_retained, 0, "{case}");
        assert_eq!(counters.installed_carrier_logical_bytes, 0, "{case}");
        assert_eq!(counters.locator_installs, 0, "{case}");
        assert_eq!(counters.fscas_catalog_operations, 0, "{case}");
        assert_eq!(counters.closure_fences, 0, "{case}");
        assert_eq!(counters.unreachable_installed_residue_bytes, 0, "{case}");
        assert_eq!(counters.immutable_residue_bytes, 0, "{case}");
        assert_eq!(counters.immutable_residue_inodes, 0, "{case}");
        assert!(counters.has_zero_forbidden_work(), "{case}");
        assert!(cas.occupied().is_ok(), "{case}");
        drop(FsCasV1::open_existing(fixture.path()).unwrap());
    }
}

#[test]
fn private_pack_cleanup_failure_is_typed_invalidates_stale_handles_and_retains_exact_residue() {
    let fixture = TestRoot::new("private-pack-cleanup-failure");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let mut counters = OperationCountersV1::default();
    let input = [0x39_u8; 64 * 1024 + 17];
    let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
    let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
    let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
    let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
    let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
    let mut tree_pages = [None::<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES];
    let mut traversal = [0_u8; 64];
    let mut control = CancelBeforeCandidateValidationAndFailPrivatePackCleanupOnce::default();
    let grant = request_create_operation_v1(&cas, 104, &mut counters, &mut control).unwrap();

    let error = run_create_v1(
        grant,
        CdcAlgorithmV1::FastCdc,
        b"payload.bin",
        0o644,
        input.len() as u64,
        CheckedSupplier { bytes: &input },
        OperationBuffersV1 {
            source: &mut source_window,
            cdc_ring: &mut cdc_ring,
            incoming_comparison: &mut incoming,
            occupied_comparison: &mut occupied,
            tree_object: &mut tree_object,
            tree_pages: &mut tree_pages,
            traversal_state: &mut traversal,
        },
        &mut control,
        &mut counters,
    )
    .unwrap_err();

    assert_eq!(
        error,
        OperationErrorV1::FsCas(FsCasErrorV1::TerminalFailure {
            first: FsCasFailureCauseV1::Core(CoreError::Cancelled),
            dominant: FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::PrivatePack,),
        })
    );
    let preparation: Vec<_> = fs::read_dir(fixture.path().join("preparation"))
        .unwrap()
        .map(|entry| entry.unwrap())
        .collect();
    assert_eq!(preparation.len(), 1);
    assert!(preparation[0]
        .file_name()
        .to_string_lossy()
        .starts_with("pack-"));
    assert!(preparation[0].metadata().unwrap().len() > 0);
    assert_eq!(
        fs::read_dir(fixture.path().join("carriers"))
            .unwrap()
            .count(),
        0
    );
    assert!(fixture.path().join("invalidated").is_dir());
    assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(
        FsCasV1::open_existing(fixture.path()),
        Err(FsCasErrorV1::Invalidated)
    ));
}

#[derive(Default)]
struct CancelAfterCarrierInstallAndFailCleanupOnce {
    cancel: bool,
    cleanup_injected: bool,
}

struct PoisonStorageAndCancelAfterCarrierInstall {
    cas: FsCasV1,
    cancel: bool,
    storage_poisoned: bool,
    fail_invalidation: bool,
}

impl CdcControlV1 for PoisonStorageAndCancelAfterCarrierInstall {
    fn cancellation_requested(&mut self) -> bool {
        self.cancel
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for PoisonStorageAndCancelAfterCarrierInstall {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if !self.storage_poisoned && boundary == FsCasBoundaryV1::AfterCarrierInstall {
            self.storage_poisoned = true;
            self.cas.poison_storage_admission_for_test_v1();
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
        self.fail_invalidation && target == FsCasCleanupTargetV1::RootInvalidation
    }
}

impl CdcControlV1 for CancelAfterCarrierInstallAndFailCleanupOnce {
    fn cancellation_requested(&mut self) -> bool {
        self.cancel
    }

    fn deadline_exceeded(&mut self) -> bool {
        false
    }
}

impl FsCasControlV1 for CancelAfterCarrierInstallAndFailCleanupOnce {
    fn boundary_reached(&mut self, boundary: FsCasBoundaryV1) {
        if boundary == FsCasBoundaryV1::AfterCarrierInstall {
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
        if target == FsCasCleanupTargetV1::Carrier && !self.cleanup_injected {
            self.cleanup_injected = true;
            true
        } else {
            false
        }
    }
}

#[test]
fn carrier_cleanup_failure_is_typed_through_sink_and_retains_exact_residue() {
    let fixture = TestRoot::new("carrier-cleanup-failure");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let stale = FsCasV1::open_existing(fixture.path()).unwrap();
    let mut counters = OperationCountersV1::default();
    let input = [0xa5_u8; 64 * 1024 + 17];
    let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
    let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
    let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
    let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
    let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
    let mut tree_pages = [None::<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES];
    let mut traversal = [0_u8; 64];
    let mut control = CancelAfterCarrierInstallAndFailCleanupOnce::default();
    let grant = request_create_operation_v1(&cas, 105, &mut counters, &mut control).unwrap();

    let error = run_create_v1(
        grant,
        CdcAlgorithmV1::FastCdc,
        b"payload.bin",
        0o644,
        input.len() as u64,
        CheckedSupplier { bytes: &input },
        OperationBuffersV1 {
            source: &mut source_window,
            cdc_ring: &mut cdc_ring,
            incoming_comparison: &mut incoming,
            occupied_comparison: &mut occupied,
            tree_object: &mut tree_object,
            tree_pages: &mut tree_pages,
            traversal_state: &mut traversal,
        },
        &mut control,
        &mut counters,
    )
    .unwrap_err();

    assert_eq!(
        error,
        OperationErrorV1::FsCas(FsCasErrorV1::TerminalFailure {
            first: FsCasFailureCauseV1::Core(CoreError::Cancelled),
            dominant: FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::Carrier,),
        })
    );
    assert_eq!(
        fs::read_dir(fixture.path().join("preparation"))
            .unwrap()
            .count(),
        0
    );
    let carriers: Vec<_> = fs::read_dir(fixture.path().join("carriers"))
        .unwrap()
        .map(|entry| entry.unwrap())
        .collect();
    assert_eq!(carriers.len(), 1);
    let exact_residue_bytes = carriers[0].metadata().unwrap().len();
    assert!(exact_residue_bytes > 0);
    assert_eq!(
        counters.unreachable_installed_residue_bytes,
        exact_residue_bytes
    );
    assert_eq!(
        counters.storage_bytes_requested,
        counters.storage_bytes_released
            + counters.storage_bytes_committed
            + counters.storage_bytes_retained
    );
    assert_eq!(
        counters.storage_inodes_requested,
        counters.storage_inodes_released
            + counters.storage_inodes_committed
            + counters.storage_inodes_retained
    );
    assert_eq!(counters.storage_bytes_committed, 0);
    assert_eq!(counters.storage_inodes_committed, 0);
    let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
        exact_operation_namespace_usage(fixture.path());
    assert_eq!(preparation_bytes, 0);
    assert_eq!(preparation_inodes, 0);
    assert_eq!(immutable_bytes, exact_residue_bytes);
    assert_eq!(immutable_inodes, 1);
    assert_eq!(counters.storage_bytes_retained, immutable_bytes);
    assert_eq!(counters.storage_inodes_retained, immutable_inodes);
    assert_eq!(counters.mutable_preparation_residue_bytes, 0);
    assert_eq!(counters.mutable_preparation_residue_inodes, 0);
    assert_eq!(counters.immutable_residue_inodes, immutable_inodes);
    assert!(fixture.path().join("invalidated").is_dir());
    assert!(matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)));
    assert!(matches!(
        FsCasV1::open_existing(fixture.path()),
        Err(FsCasErrorV1::Invalidated)
    ));
}

#[test]
fn carrier_accounting_poison_preserves_cancellation_and_cleanup_dominance() {
    for (case, fail_invalidation, expected_dominant) in [
        (
            "carrier-accounting-poison",
            false,
            FsCasFailureCauseV1::CleanupFailed(FsCasCleanupTargetV1::Carrier),
        ),
        (
            "carrier-accounting-invalidation-double-fault",
            true,
            FsCasFailureCauseV1::InvalidationFailed,
        ),
    ] {
        let fixture = TestRoot::new(case);
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let stale = FsCasV1::open_existing(fixture.path()).unwrap();
        let mut counters = OperationCountersV1::default();
        let input = [0xa6_u8; 64 * 1024 + 19];
        let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
        let mut tree_pages = [None::<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES];
        let mut traversal = [0_u8; 64];
        let mut control = PoisonStorageAndCancelAfterCarrierInstall {
            cas: cas.clone(),
            cancel: false,
            storage_poisoned: false,
            fail_invalidation,
        };
        let grant = request_create_operation_v1(&cas, 0x75a, &mut counters, &mut control).unwrap();

        let error = run_create_v1(
            grant,
            CdcAlgorithmV1::FastCdc,
            b"payload.bin",
            0o644,
            input.len() as u64,
            CheckedSupplier { bytes: &input },
            OperationBuffersV1 {
                source: &mut source_window,
                cdc_ring: &mut cdc_ring,
                incoming_comparison: &mut incoming,
                occupied_comparison: &mut occupied,
                tree_object: &mut tree_object,
                tree_pages: &mut tree_pages,
                traversal_state: &mut traversal,
            },
            &mut control,
            &mut counters,
        )
        .unwrap_err();

        assert_eq!(
            error,
            OperationErrorV1::FsCas(FsCasErrorV1::TerminalFailure {
                first: FsCasFailureCauseV1::Core(CoreError::Cancelled),
                dominant: expected_dominant,
            }),
            "{case}"
        );
        assert!(control.storage_poisoned, "{case}");
        assert_operation_authority_baseline(&cas, fixture.path());
        assert_storage_equations(&counters);
        let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
            exact_operation_namespace_usage(fixture.path());
        assert_eq!((preparation_bytes, preparation_inodes), (0, 0), "{case}");
        assert_eq!((immutable_bytes, immutable_inodes), (0, 0), "{case}");
        assert_eq!(counters.storage_bytes_committed, 0, "{case}");
        assert_eq!(counters.storage_inodes_committed, 0, "{case}");
        assert_eq!(counters.storage_bytes_retained, 0, "{case}");
        assert_eq!(counters.storage_inodes_retained, 0, "{case}");
        assert_eq!(counters.unreachable_installed_residue_bytes, 0, "{case}");
        assert_eq!(counters.immutable_residue_bytes, 0, "{case}");
        assert_eq!(counters.immutable_residue_inodes, 0, "{case}");
        assert!(counters.has_zero_forbidden_work(), "{case}");
        assert!(
            matches!(cas.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        assert!(
            matches!(stale.occupied(), Err(FsCasErrorV1::Invalidated)),
            "{case}"
        );
        match FsCasV1::open_existing(fixture.path()) {
            Err(FsCasErrorV1::Invalidated | FsCasErrorV1::Busy) => {}
            Err(error) => panic!("{case}: unexpected reopen error {error:?}"),
            Ok(_) => panic!("{case}: damaged root reopened as usable"),
        }
    }
}

#[test]
fn complete_algorithms_use_one_pre_supplier_slot_and_return_all_preparation_resources() {
    let mut input = vec![0_u8; 384 * 1024 + 73];
    let mut state = 0x9e37_79b9_u32;
    for (index, byte) in input.iter_mut().enumerate() {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        *byte = (state as u8) ^ (index as u8).wrapping_mul(17);
    }

    for algorithm in [CdcAlgorithmV1::FastCdc, CdcAlgorithmV1::SeqCdc] {
        let fixture = TestRoot::new("success");
        let cas = FsCasV1::create_new(fixture.path()).unwrap();
        let mut counters = OperationCountersV1::default();
        let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
        let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
        let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
        let mut tree_pages = [None::<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES];
        let mut traversal = [0_u8; 64];
        let mut control = ContinueControl;
        let grant = request_create_operation_v1(&cas, 106, &mut counters, &mut control).unwrap();

        let result = run_create_v1(
            grant,
            algorithm,
            b"payload.bin",
            0o644,
            input.len() as u64,
            CheckedSupplier { bytes: &input },
            OperationBuffersV1 {
                source: &mut source_window,
                cdc_ring: &mut cdc_ring,
                incoming_comparison: &mut incoming,
                occupied_comparison: &mut occupied,
                tree_object: &mut tree_object,
                tree_pages: &mut tree_pages,
                traversal_state: &mut traversal,
            },
            &mut control,
            &mut counters,
        )
        .unwrap_or_else(|error| panic!("{algorithm:?}: {error:?}; {counters:#?}"));

        assert_eq!(result.algorithm(), algorithm);
        assert_eq!(result.pack_outcome(), FsPackAdmissionOutcomeV1::Installed);
        assert!(result.object_count() >= 4);
        assert_eq!(
            result.reference_spool_bytes().status(),
            OptionalObservationStatusV1::Observed
        );
        assert!(result.reference_spool_bytes().value().unwrap() > 0);
        assert_eq!(
            result.reference_spool_bytes().scope(),
            ObservationScopeV1::Operation
        );
        assert_eq!(
            result.reference_spool_bytes().method(),
            "direct chunk-reference spool logical length"
        );
        assert_eq!(
            result.index_spool_bytes().status(),
            OptionalObservationStatusV1::Observed
        );
        assert!(result.index_spool_bytes().value().unwrap() > 0);
        assert_eq!(
            result.index_spool_bytes().scope(),
            ObservationScopeV1::Operation
        );
        assert_eq!(
            result.index_spool_bytes().method(),
            "direct cumulative pack-index spool logical length"
        );
        assert_eq!(
            result.terminal_optional_observations(),
            counters.terminal_optional_observations_v1()
        );
        assert!(result
            .terminal_optional_observations()
            .all()
            .into_iter()
            .all(|observation| observation.value().is_none()));
        assert_eq!(
            fs::read_dir(fixture.path().join("preparation"))
                .unwrap()
                .count(),
            0
        );
        assert!(counters.source_read_calls > 0);
        assert_eq!(counters.source_bytes_read, input.len() as u64);
        assert!(counters.fscas_read_calls > 0);
        assert!(counters.fscas_bytes_read > 0);
        assert!(counters.fscas_bytes_written > 0);
        assert_eq!(counters.closure_fences, 1);
        assert_eq!(counters.unreachable_installed_residue_bytes, 0);
        assert_storage_equations(&counters);
        assert!(counters.storage_bytes_committed > 0);
        assert!(counters.storage_inodes_committed > 0);
        assert_eq!(counters.storage_bytes_retained, 0);
        assert_eq!(counters.storage_inodes_retained, 0);
        assert_eq!(counters.mutable_preparation_residue_bytes, 0);
        assert_eq!(counters.mutable_preparation_residue_inodes, 0);
        let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
            exact_operation_namespace_usage(fixture.path());
        assert_eq!(preparation_bytes, 0);
        assert_eq!(preparation_inodes, 0);
        assert_eq!(counters.storage_bytes_committed, immutable_bytes);
        assert_eq!(counters.storage_inodes_committed, immutable_inodes);
        assert_eq!(counters.immutable_residue_inodes, 0);
        assert!(counters.has_zero_forbidden_work());
    }
}

#[test]
fn exact_100_mib_complete_c3_rolls_over_real_fscas_carriers() {
    const LOGICAL_BYTES: u64 = 100 * 1024 * 1024;

    let fixture = TestRoot::new("multi-pack-100m");
    let cas = FsCasV1::create_new(fixture.path()).unwrap();
    let mut counters = OperationCountersV1::default();
    let mut source_window = [0_u8; MAXIMUM_CHUNK_BYTES];
    let mut cdc_ring = [0_u8; MAXIMUM_CHUNK_BYTES];
    let mut incoming = [0_u8; COMPARISON_WINDOW_BYTES];
    let mut occupied = [0_u8; COMPARISON_WINDOW_BYTES];
    let mut tree_object = [0_u8; MAX_TREE_OBJECT_BYTES];
    let mut tree_pages = [None::<TreePageSummaryV1>; MAX_TREE_PAGE_SUMMARIES];
    let mut traversal = vec![0_u8; 64 * 1024];
    let mut control = ContinueControl;
    let grant = request_create_operation_v1(&cas, 107, &mut counters, &mut control).unwrap();

    let result = run_create_v1(
        grant,
        CdcAlgorithmV1::FastCdc,
        b"payload.bin",
        0o644,
        LOGICAL_BYTES,
        CounterSupplier { len: LOGICAL_BYTES },
        OperationBuffersV1 {
            source: &mut source_window,
            cdc_ring: &mut cdc_ring,
            incoming_comparison: &mut incoming,
            occupied_comparison: &mut occupied,
            tree_object: &mut tree_object,
            tree_pages: &mut tree_pages,
            traversal_state: &mut traversal,
        },
        &mut control,
        &mut counters,
    )
    .unwrap_or_else(|error| panic!("{error:?}; {counters:#?}"));

    assert_eq!(result.carrier_count(), 2);
    assert_eq!(result.carrier_rollovers(), 1);
    assert_eq!(result.carriers_installed(), 2);
    assert_eq!(result.carriers_reused(), 0);
    assert_eq!(counters.source_bytes_read, LOGICAL_BYTES);
    assert_eq!(counters.closure_fences, 1);
    assert_eq!(counters.unreachable_installed_residue_bytes, 0);
    assert!(counters.file_sort_comparisons > 0);
    assert!(counters.file_sort_record_reads > 0);
    assert!(counters.file_sort_record_writes > 0);
    assert!(counters.file_sort_passes > 0);
    assert!(counters.file_sort_control_polls > 0);
    assert_eq!(
        counters.file_sort_work_units,
        counters.file_sort_comparisons
            + counters.file_sort_record_reads
            + counters.file_sort_record_writes
    );
    assert!(counters.file_sort_maximum_work_budget > 0);
    assert_eq!(counters.file_sort_temporary_bytes_high_water, 0);
    assert_operation_authority_baseline(&cas, fixture.path());
    assert_storage_equations(&counters);
    let ((preparation_bytes, preparation_inodes), (immutable_bytes, immutable_inodes)) =
        exact_operation_namespace_usage(fixture.path());
    assert_eq!((preparation_bytes, preparation_inodes), (0, 0));
    assert_eq!(counters.storage_bytes_committed, immutable_bytes);
    assert_eq!(counters.storage_inodes_committed, immutable_inodes);
    assert_eq!(counters.storage_bytes_retained, 0);
    assert_eq!(counters.storage_inodes_retained, 0);
    assert!(counters.has_zero_forbidden_work());
}
