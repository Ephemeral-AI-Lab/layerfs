use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use layerfs_storage::cas::{
    FsCasBoundaryV1, FsCasCleanupTargetV1, FsCasControlV1, FsCasErrorV1, FsCasFilesystemBoundaryV1,
    FsCasFilesystemFailureV1, FsCasV1, FsOperationKindV1, FsPackAdmissionOutcomeV1,
    FsStorageEnvelopeV1, ROOT_LOGICAL_STORAGE_BUDGET_V1, ROOT_NAMESPACE_ENTRY_BUDGET_V1,
};
use layerfs_storage::cdc::C3CdcAlgorithmV1;
use layerfs_storage::cdc::{CdcControlV1, MAXIMUM_CHUNK_BYTES};
use layerfs_storage::content::{
    request_c3_create_qualification_v1, request_c3_tree_operation_v1, run_c3_create_tree_v1,
    run_c3_create_v1, C3OperationBuffersV1, C3OperationErrorV1, C3SourceSupplierV1, C3TreeFileV1,
};
use layerfs_storage::content::{ContentSourceErrorV1, ContentSourceV1};
use layerfs_storage::cow::{TreePageSummaryV1, MAX_TREE_OBJECT_BYTES, MAX_TREE_PAGE_SUMMARIES};
use layerfs_storage::identity::COMPARISON_WINDOW_BYTES;
use layerfs_storage::limits::OperationCountersV1;
use layerfs_storage::CoreResult;

static NEXT_ROOT: AtomicU64 = AtomicU64::new(1);
const SUBPROCESS_OPEN_PATH_ENV: &str = "LAYERFS_L155_SUBPROCESS_OPEN_PATH";
const SUBPROCESS_OPEN_EXPECT_ENV: &str = "LAYERFS_L155_SUBPROCESS_OPEN_EXPECT";

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

struct InvocationCheckedSupplier<'a> {
    invoked: &'a AtomicBool,
}

struct CallbackCheckedSupplier<'a> {
    bound_invoked: &'a AtomicBool,
    supply_invoked: &'a AtomicBool,
}

struct PanickingSupplier;

impl C3SourceSupplierV1 for PanickingSupplier {
    type Source = CounterSource;

    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        Ok(core::mem::size_of::<CounterSource>() as u64)
    }

    fn supply(self) -> CoreResult<Self::Source> {
        panic!("injected supplier unwind")
    }
}

impl<'a> C3SourceSupplierV1 for InvocationCheckedSupplier<'a> {
    type Source = CounterSource;

    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        Ok(core::mem::size_of::<CounterSource>() as u64)
    }

    fn supply(self) -> CoreResult<Self::Source> {
        self.invoked.store(true, Ordering::Release);
        Ok(CounterSource { len: 1, offset: 0 })
    }
}

impl<'a> C3SourceSupplierV1 for CallbackCheckedSupplier<'a> {
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

impl C3SourceSupplierV1 for CounterSupplier {
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

impl<'a> C3SourceSupplierV1 for CheckedSupplier<'a> {
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

struct FailFilesystemBoundaryOnceV1 {
    boundary: FsCasFilesystemBoundaryV1,
    error: FsCasErrorV1,
    fired: bool,
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

fn run_small_create_with_callback_observation<C>(
    cas: &FsCasV1,
    cancellation_key: u64,
    control: &mut C,
    bound_invoked: &AtomicBool,
    supply_invoked: &AtomicBool,
) -> (
    Result<layerfs_storage::lifecycle::C3HandoffV1, C3OperationErrorV1>,
    OperationCountersV1,
)
where
    C: CdcControlV1 + FsCasControlV1,
{
    let mut counters = OperationCountersV1::default();
    let mut source_window = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
    let mut cdc_ring = boxed_zeroes::<MAXIMUM_CHUNK_BYTES>();
    let mut incoming = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut occupied = boxed_zeroes::<COMPARISON_WINDOW_BYTES>();
    let mut tree_object = boxed_zeroes::<MAX_TREE_OBJECT_BYTES>();
    let mut tree_pages = boxed_tree_pages();
    let mut traversal = [0_u8; 64];
    let grant =
        request_c3_create_qualification_v1(cas, cancellation_key, &mut counters, control).unwrap();
    let result = run_c3_create_v1(
        grant,
        C3CdcAlgorithmV1::FastCdc,
        b"payload.bin",
        0o644,
        1,
        CallbackCheckedSupplier {
            bound_invoked,
            supply_invoked,
        },
        C3OperationBuffersV1 {
            source: &mut source_window,
            cdc_ring: &mut cdc_ring,
            incoming_comparison: &mut incoming,
            occupied_comparison: &mut occupied,
            tree_object: &mut tree_object,
            tree_pages: &mut *tree_pages,
            traversal_state: &mut traversal,
        },
        control,
        &mut counters,
    );
    (result, counters)
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
            FsCasErrorV1::Io,
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
            FsCasErrorV1::Io,
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
            Err(C3OperationErrorV1::FsCas(expected)),
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
        assert_eq!(cas.operation_admitted_slots_v1(), 0, "{boundary:?}");
        assert_eq!(
            fs::read_dir(fixture.path().join("preparation"))
                .unwrap()
                .count(),
            0,
            "unpublished preparation residue at {boundary:?}",
        );
        for directory in ["carriers", "objects", "catalog", "closures"] {
            assert_eq!(
                fs::read_dir(fixture.path().join(directory))
                    .unwrap()
                    .count(),
                0,
                "unpublished immutable residue in {directory} at {boundary:?}",
            );
        }
        assert_eq!(
            counters.storage_bytes_requested,
            counters.storage_bytes_released
                + counters.storage_bytes_committed
                + counters.storage_bytes_retained,
            "storage-byte reconciliation at {boundary:?}",
        );
        assert_eq!(
            counters.storage_inodes_requested,
            counters.storage_inodes_released
                + counters.storage_inodes_committed
                + counters.storage_inodes_retained,
            "storage-inode reconciliation at {boundary:?}",
        );
        assert_eq!(counters.storage_bytes_committed, 0, "{boundary:?}");
        assert_eq!(counters.storage_inodes_committed, 0, "{boundary:?}");
        assert_eq!(counters.storage_bytes_retained, 0, "{boundary:?}");
        assert_eq!(counters.storage_inodes_retained, 0, "{boundary:?}");
        assert!(cas.occupied().is_ok(), "root invalidated at {boundary:?}");
        assert!(
            stale.occupied().is_ok(),
            "stale alias invalidated at {boundary:?}"
        );
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
        Err(C3OperationErrorV1::FsCas(FsCasErrorV1::CleanupFailed(
            FsCasCleanupTargetV1::PrivatePack,
        )))
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
        Err(C3OperationErrorV1::FsCas(FsCasErrorV1::CleanupFailed(
            FsCasCleanupTargetV1::PublishedMarkerAlias,
        )))
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
                Some(FsCasErrorV1::Io)
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
        Err(C3OperationErrorV1::FsCas(FsCasErrorV1::InvalidationFailed,))
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
    assert!(!fixture.path().join("invalidated").exists());
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

#[derive(Clone, Copy, Eq, PartialEq)]
enum AdmissionStopV1 {
    Cancelled,
    Deadline,
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
                request_c3_create_qualification_v1(
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
            request_c3_create_qualification_v1(
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
    }
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
        let grant =
            request_c3_create_qualification_v1(&cas, 0x52, &mut counters, &mut control).unwrap();
        let result = run_c3_create_v1(
            grant,
            C3CdcAlgorithmV1::FastCdc,
            b"payload.bin",
            0o644,
            1,
            CallbackCheckedSupplier {
                bound_invoked: &bound_invoked,
                supply_invoked: &supply_invoked,
            },
            C3OperationBuffersV1 {
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
            Err(C3OperationErrorV1::FsCas(FsCasErrorV1::ResourceExhausted(
                observed
            ))) if observed == resource
        ));
        assert!(!bound_invoked.load(Ordering::Acquire));
        assert!(!supply_invoked.load(Ordering::Acquire));
        assert_eq!(counters.source_read_calls, 0);
        assert_eq!(counters.source_bytes_read, 0);
        assert_eq!(
            fs::read_dir(fixture.path().join("preparation"))
                .unwrap()
                .count(),
            0
        );

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
        assert_eq!(cas.operation_admitted_slots_v1(), 0);
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
    let grant = request_c3_create_qualification_v1(&cas, 100, &mut counters, &mut control).unwrap();

    run_c3_create_v1(
        grant,
        C3CdcAlgorithmV1::FastCdc,
        b"payload.bin",
        0o644,
        input.len() as u64,
        CheckedSupplier { bytes: &input },
        C3OperationBuffersV1 {
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
fn supplier_unwind_uses_drop_backstop_and_releases_preparation_and_slot() {
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
    let grant = request_c3_create_qualification_v1(&cas, 109, &mut counters, &mut control).unwrap();

    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = run_c3_create_v1(
            grant,
            C3CdcAlgorithmV1::FastCdc,
            b"payload.bin",
            0o644,
            1,
            PanickingSupplier,
            C3OperationBuffersV1 {
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
        let grant =
            request_c3_create_qualification_v1(&cas, 101, &mut counters, &mut control).unwrap();

        let error = run_c3_create_v1(
            grant,
            C3CdcAlgorithmV1::FastCdc,
            b"payload.bin",
            0o644,
            1,
            InvocationCheckedSupplier { invoked: &invoked },
            C3OperationBuffersV1 {
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
            C3OperationErrorV1::Core(layerfs_storage::CoreError::ResourceRefused)
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
                request_c3_create_qualification_v1(&cas, 102, &mut counters, &mut control).unwrap();

            let error = run_c3_create_v1(
                grant,
                C3CdcAlgorithmV1::FastCdc,
                b"payload.bin",
                0o644,
                input.len() as u64,
                CheckedSupplier { bytes: &input },
                C3OperationBuffersV1 {
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
                C3OperationErrorV1::FsCas(FsCasErrorV1::CleanupFailed(
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
    // The six lifecycle-owned spools are cleaned in this fixed order. Inject
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
            ] {
                let fixture = TestRoot::new(expected_prefix);
                let cas = FsCasV1::create_new(fixture.path()).unwrap();
                let stale = FsCasV1::open_existing(fixture.path()).unwrap();
                let first = [0x31_u8; 64 * 1024 + 17];
                let second = [0xa7_u8; 72 * 1024 + 29];
                let mut files = [
                    C3TreeFileV1::new(
                        b"a.bin",
                        0o644,
                        first.len() as u64,
                        CheckedSupplier { bytes: &first },
                    ),
                    C3TreeFileV1::new(
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
                let operation = request_c3_tree_operation_v1(
                    &cas,
                    0x300 + target_call as u64,
                    &mut counters,
                    &mut control,
                )
                .unwrap();

                let error = run_c3_create_tree_v1(
                    operation,
                    C3CdcAlgorithmV1::FastCdc,
                    &mut files,
                    C3OperationBuffersV1 {
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
                    C3OperationErrorV1::FsCas(FsCasErrorV1::CleanupFailed(
                        FsCasCleanupTargetV1::PreparationSpool,
                    )),
                    "cleanup boundary {target_call} ({expected_prefix})",
                );
                assert!(control.injected);
                assert_eq!(control.observed_calls, 6);
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
                request_c3_create_qualification_v1(&cas, 109, &mut counters, &mut control).unwrap();

            let error = run_c3_create_v1(
                grant,
                C3CdcAlgorithmV1::FastCdc,
                b"payload.bin",
                0o644,
                input.len() as u64,
                CheckedSupplier { bytes: &input },
                C3OperationBuffersV1 {
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
                C3OperationErrorV1::FsCas(FsCasErrorV1::InvalidationFailed)
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
            assert!(!fixture.path().join("invalidated").exists());
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
    injected: bool,
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
        if !self.injected
            && target == FsCasCleanupTargetV1::PublishedMarkerAlias
            && self.current == Some(self.target)
        {
            self.injected = true;
            true
        } else {
            false
        }
    }
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
                    injected: false,
                };
                let grant =
                    request_c3_create_qualification_v1(&cas, 103, &mut counters, &mut control)
                        .unwrap();

                let error = run_c3_create_v1(
                    grant,
                    C3CdcAlgorithmV1::FastCdc,
                    b"payload.bin",
                    0o644,
                    input.len() as u64,
                    CheckedSupplier { bytes: &input },
                    C3OperationBuffersV1 {
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
                    C3OperationErrorV1::FsCas(FsCasErrorV1::CleanupFailed(
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
        request_c3_create_qualification_v1(&cas, 0x410, &mut first_counters, &mut first_control)
            .unwrap();
    run_c3_create_v1(
        first_grant,
        C3CdcAlgorithmV1::FastCdc,
        b"payload.bin",
        0o644,
        input.len() as u64,
        CheckedSupplier { bytes: &input },
        C3OperationBuffersV1 {
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
    let grant =
        request_c3_create_qualification_v1(&cas, 0x411, &mut counters, &mut control).unwrap();
    let error = run_c3_create_v1(
        grant,
        C3CdcAlgorithmV1::FastCdc,
        b"payload.bin",
        0o644,
        input.len() as u64,
        CheckedSupplier { bytes: &input },
        C3OperationBuffersV1 {
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
        C3OperationErrorV1::FsCas(FsCasErrorV1::UnequalOccupant)
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
    let grant = request_c3_create_qualification_v1(&cas, 104, &mut counters, &mut control).unwrap();

    let error = run_c3_create_v1(
        grant,
        C3CdcAlgorithmV1::FastCdc,
        b"payload.bin",
        0o644,
        input.len() as u64,
        CheckedSupplier { bytes: &input },
        C3OperationBuffersV1 {
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
        C3OperationErrorV1::FsCas(FsCasErrorV1::CleanupFailed(
            FsCasCleanupTargetV1::PrivatePack,
        ))
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
    let grant = request_c3_create_qualification_v1(&cas, 105, &mut counters, &mut control).unwrap();

    let error = run_c3_create_v1(
        grant,
        C3CdcAlgorithmV1::FastCdc,
        b"payload.bin",
        0o644,
        input.len() as u64,
        CheckedSupplier { bytes: &input },
        C3OperationBuffersV1 {
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
        C3OperationErrorV1::FsCas(FsCasErrorV1::CleanupFailed(FsCasCleanupTargetV1::Carrier,))
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
fn complete_algorithms_use_one_pre_supplier_slot_and_return_all_preparation_resources() {
    let mut input = vec![0_u8; 384 * 1024 + 73];
    let mut state = 0x9e37_79b9_u32;
    for (index, byte) in input.iter_mut().enumerate() {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        *byte = (state as u8) ^ (index as u8).wrapping_mul(17);
    }

    for algorithm in [C3CdcAlgorithmV1::FastCdc, C3CdcAlgorithmV1::SeqCdc] {
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
        let grant =
            request_c3_create_qualification_v1(&cas, 106, &mut counters, &mut control).unwrap();

        let result = run_c3_create_v1(
            grant,
            algorithm,
            b"payload.bin",
            0o644,
            input.len() as u64,
            CheckedSupplier { bytes: &input },
            C3OperationBuffersV1 {
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
        assert!(result.reference_spool_bytes().unwrap() > 0);
        assert!(result.index_spool_bytes().unwrap() > 0);
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
    let grant = request_c3_create_qualification_v1(&cas, 107, &mut counters, &mut control).unwrap();

    let result = run_c3_create_v1(
        grant,
        C3CdcAlgorithmV1::FastCdc,
        b"payload.bin",
        0o644,
        LOGICAL_BYTES,
        CounterSupplier { len: LOGICAL_BYTES },
        C3OperationBuffersV1 {
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
    assert_eq!(
        fs::read_dir(fixture.path().join("preparation"))
            .unwrap()
            .count(),
        0
    );
    assert!(counters.has_zero_forbidden_work());
}
