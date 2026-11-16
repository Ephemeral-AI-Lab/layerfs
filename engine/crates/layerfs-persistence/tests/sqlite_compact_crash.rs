//! #41 T12.2: the §9 crash matrix for SQLite `store.compact`.
//!
//! Each case aborts at one named failpoint, reopens the Store and asserts the
//! documented crash state: wholly old at `G` or wholly new at `G + 1`, the
//! exact artifact set, the live objects still readable, the admission record
//! and the op/phase/uncertain `store.status` projection. It then runs the next
//! `store.compact` and asserts it settles (installed) or rebuilds (pre-install)
//! and leaves a clean Store.
//!
//! The expectations below are backend-neutral data; a PostgreSQL harness can
//! reuse the same table with its own failpoints and its own artifact set.
#![cfg(any(target_os = "macos", target_os = "linux"))]
mod support;

use layerfs_content::{encode_whole_file_payload, FinalizedObject, ObjectId, ObjectRole};
use layerfs_history::{
    AddLayerOutcome, AddLayerRequest, BranchId, CatalogId, CommitId, CommitMeta, CommitOutcome,
    CommitRequest, CommitVia, ForkRequest, ForkSource, GcIntentKind, GcScopeKind, HistoryCatalog,
    HistoryCatalogConfig, LayerStackId, LayerStackRecord, StackInitialization,
};
use layerfs_persistence::{
    compact_op_status, CompactFailpoint, CompactPath, CompactRequest, PersistenceConfig,
    SqliteHandles, SqlitePersistenceProfile,
};
use layerfs_storage::{port::PackPersistence, Storage, StoragePolicy};
use std::path::{Path, PathBuf};
use support::Temp;

const BINDING: &[u8] = b"sqlite-compact-crash-binding";

/// One crash point: the failpoint, the phase `store.status` must show, whether
/// the rebuild is already installed at `G + 1`, and which staged artifacts the
/// crash leaves beside the Store.
#[derive(Clone, Copy, Debug)]
struct Case {
    failpoint: CompactFailpoint,
    phase: &'static str,
    installed: bool,
    target: bool,
    visited: bool,
    /// A real `SIGKILL` before the seal ends the process with the writer
    /// session's WAL on disk (`-wal`, `-shm`); the in-process matrix stops and
    /// drops cleanly, so only the real-process `BeforeSeal` arm sets this.
    live_sidecars: bool,
}

fn create_store() -> (Temp, SqliteHandles, PathBuf) {
    let temp = Temp::new("sqlite-compact-crash");
    let path = temp.join("store.sqlite");
    let handles = SqliteHandles::create(
        PersistenceConfig::sqlite(path.clone())
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: [71; 32],
            incarnation: 7,
        },
    )
    .unwrap();
    (temp, handles, path)
}

fn reopen(path: &Path) -> SqliteHandles {
    SqliteHandles::open_writable(
        PersistenceConfig::sqlite(path.to_path_buf())
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        BINDING,
        [71; 32],
    )
    .unwrap()
}

fn request(op: u8) -> CompactRequest {
    CompactRequest {
        op_id: [op; 16],
        ..CompactRequest::default()
    }
}

fn request_at(op: [u8; 16], failpoint: CompactFailpoint) -> CompactRequest {
    CompactRequest {
        op_id: op,
        failpoint: Some(failpoint),
        ..CompactRequest::default()
    }
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn compact_path(store: &Path) -> PathBuf {
    with_suffix(store, ".compact")
}

fn op_path(store: &Path) -> PathBuf {
    with_suffix(store, ".compact-op")
}

fn visited_path(store: &Path) -> PathBuf {
    with_suffix(store, ".compact-visited")
}

fn catalog_id() -> CatalogId {
    CatalogId::derive(BINDING).expect("catalog identity")
}

fn intents(handles: &SqliteHandles) -> Vec<layerfs_history::GcIntent> {
    handles
        .history
        .gc_intents(GcScopeKind::Store, catalog_id().as_slice())
        .unwrap()
}

fn whole_file(handles: &SqliteHandles, bytes: &[u8]) -> ObjectId {
    let object = FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(bytes).unwrap(),
    )
    .unwrap();
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    save.accept(object.clone()).unwrap();
    save.finish().unwrap();
    object.id()
}

fn filesystem_root(handles: &SqliteHandles, seed: u8) -> ObjectId {
    use layerfs_content::{
        filesystem::{
            attributes::PortableMetadata,
            directory::{encode_directory_page, DirectoryPage},
            inode::{encode_inode_page, InodePage},
            root::profile_id,
            scope_for_seed, FilesystemRoot,
        },
        inode_leaf::{InodeKind, InodeMetadata, InodeValue},
    };
    let directory = FinalizedObject::new(
        ObjectRole::DirectoryLeaf,
        encode_directory_page(&DirectoryPage::Leaf {
            entries: Vec::new(),
        })
        .unwrap(),
    )
    .unwrap();
    let directory_id = directory.id();
    let inode = FinalizedObject::new(
        ObjectRole::InodeLeaf,
        encode_inode_page(&InodePage::Leaf {
            entries: vec![(
                1,
                InodeValue {
                    kind: InodeKind::Directory,
                    namespace_ref_count: 0,
                    content_root: directory_id,
                    metadata_root: InodeMetadata::Inline(PortableMetadata {
                        mode: 0o755,
                        mtime_seconds: i64::from(seed),
                        mtime_nanoseconds: 0,
                    }),
                },
            )],
        })
        .unwrap(),
    )
    .unwrap()
    .with_references(vec![directory_id]);
    let inode_id = inode.id();
    let root = FilesystemRoot::new(profile_id(), scope_for_seed([1; 32]), 1, inode_id).unwrap();
    let object = FinalizedObject::new(ObjectRole::FilesystemRoot, root.encode().unwrap())
        .unwrap()
        .with_references(vec![inode_id]);
    let root_id = object.id();
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    for object in [directory, inode, object] {
        save.accept(object).unwrap();
    }
    save.finish().unwrap();
    root_id
}

fn initialize(handles: &SqliteHandles, tag: u8, genesis: ObjectId) -> LayerStackRecord {
    handles
        .history
        .initialize_layerstack(&StackInitialization {
            logical: 10,
            stack: support::stack(tag),
            name: support::name(&format!("project-{tag}")),
            scope: layerfs_content::filesystem::scope_for_seed([1; 32]).object(),
            profile: layerfs_content::filesystem::root::profile_id(),
            genesis_root: genesis,
        })
        .unwrap()
}

#[allow(dead_code)]
fn fork(handles: &SqliteHandles, stack: LayerStackId, branch: BranchId, tag: u8) {
    let record = handles.history.layer_stack(stack).unwrap().unwrap();
    handles
        .history
        .fork(&ForkRequest {
            stack,
            branch,
            name: support::name(&format!("branch-{tag}")),
            source: ForkSource::Layer(record.head_layer),
        })
        .unwrap();
}

#[allow(dead_code)]
fn commit(handles: &SqliteHandles, branch: BranchId, candidate_root: ObjectId) -> CommitId {
    let snapshot = handles.history.branch(branch).unwrap().unwrap();
    let expected_root = match snapshot.head_commit {
        Some(head) => handles.history.commit(head).unwrap().unwrap().root,
        None => {
            handles
                .history
                .layer(snapshot.base_layer)
                .unwrap()
                .unwrap()
                .root
        }
    };
    match handles
        .history
        .commit_branch(&CommitRequest {
            branch,
            expected_head: snapshot.head_commit,
            expected_base: snapshot.base_layer,
            expected_root,
            candidate_root,
            profile: layerfs_content::filesystem::root::profile_id(),
            scope: layerfs_content::filesystem::scope_for_seed([1; 32]).object(),
            second_parent: None,
            meta: CommitMeta::new(CommitVia::Files),
            force: false,
        })
        .unwrap()
    {
        CommitOutcome::Committed { record, .. } => record.id,
        other => panic!("commit: {other:?}"),
    }
}

#[allow(dead_code)]
fn publish(handles: &SqliteHandles, stack: LayerStackId, branch: BranchId) {
    let branch_record = handles.history.branch(branch).unwrap().unwrap();
    let stack_record = handles.history.layer_stack(stack).unwrap().unwrap();
    match handles
        .history
        .add_layer(&AddLayerRequest {
            stack,
            branch,
            commit: branch_record.head_commit.expect("a committed Branch"),
            expected_stack_head: stack_record.head_layer,
            expected_branch_base: branch_record.base_layer,
        })
        .unwrap()
    {
        AddLayerOutcome::Added { .. } => {}
        other => panic!("publish: {other:?}"),
    }
}

fn generation(handles: &SqliteHandles) -> u64 {
    handles.history.incarnation()
}

fn located(handles: &SqliteHandles, id: ObjectId) -> bool {
    let mut found = Vec::new();
    handles.storage.locate(&[id], &mut found).unwrap();
    !found.is_empty()
}

fn object_bytes(handles: &SqliteHandles, id: ObjectId) -> Vec<u8> {
    let storage = Storage::new(handles.storage.clone()).unwrap();
    storage
        .reader()
        .unwrap()
        .read_objects(&[id])
        .unwrap()
        .into_iter()
        .next()
        .expect("one object")
}

fn clean(handles: &SqliteHandles) {
    let mut out = Vec::new();
    handles
        .diagnose(
            layerfs_persistence::diagnostic::Options::default(),
            &mut |observation| out.push(observation),
        )
        .unwrap();
    assert!(
        !out.iter().any(|observation| matches!(
            observation.outcome,
            layerfs_persistence::diagnostic::Outcome::Fail
                | layerfs_persistence::diagnostic::Outcome::Unknown
        )),
        "{out:?}"
    );
}

fn assert_no_sidecars(store: &Path, case: Case) {
    for suffix in ["-wal", "-shm", "-journal"] {
        assert!(
            !with_suffix(store, suffix).exists(),
            "sidecar {suffix} beside {} at {case:?}",
            store.display()
        );
    }
}

/// Asserts the operation record and the artifact set the failpoint leaves
/// beside the live Store, before any handle reopens it.
fn assert_crash_state(case: Case, store: &Path, before: u64, op: [u8; 16]) {
    // `store.status` (T10.1) projects the recorded operation, its phase and
    // its uncertain outcome; the engine reads them from the op record.
    let status = compact_op_status(store).expect("the op record remains");
    assert_eq!(status.op_id, op);
    assert_eq!(status.generation, i64::try_from(before).unwrap());
    assert_eq!(status.phase, case.phase, "phase at {case:?}");
    assert!(
        status.uncertain,
        "an unfinished run is uncertain at {case:?}"
    );

    // The exact artifact set the point leaves beside the live Store.
    assert_eq!(
        compact_path(store).exists(),
        case.target,
        "staged Store at {case:?}"
    );
    assert_eq!(
        visited_path(store).exists(),
        case.visited,
        "visited scratch at {case:?}"
    );
    assert!(op_path(store).exists(), "op record at {case:?}");
    if case.live_sidecars {
        // A real kill before the seal ends the process with the writer
        // session's WAL still on disk: the next open replays it, and the crash
        // state itself keeps both sidecars, exactly as a killed daemon does.
        assert!(
            with_suffix(store, "-wal").exists(),
            "the killed session's WAL at {case:?}"
        );
        assert!(
            with_suffix(store, "-shm").exists(),
            "the killed session's shared memory at {case:?}"
        );
        assert!(
            !with_suffix(store, "-journal").exists(),
            "no rollback journal at {case:?}"
        );
    } else {
        assert_no_sidecars(store, case);
    }
    assert_no_sidecars(&compact_path(store), case);
}

/// The fixtures one crash case needs to prove its recovery.
struct Recovery<'a> {
    case: Case,
    store: &'a Path,
    before: u64,
    live: ObjectId,
    live_bytes: &'a [u8],
    dead: ObjectId,
    stack: LayerStackRecord,
    op: [u8; 16],
}

/// Reopens the crashed Store, asserts the §9 state (wholly old or wholly new,
/// live data readable, exact artifacts) and then proves the next
/// `store.compact` settles or rebuilds and leaves a clean Store.
fn assert_recovery(recovery: Recovery<'_>) {
    let Recovery {
        case,
        store,
        before,
        live,
        live_bytes,
        dead,
        stack,
        op,
    } = recovery;
    let reopened = reopen(store);

    // §5.1: the intent stays in the live Store through build and verify and
    // is cleared from the staged copy at settle, so an old Store keeps it and
    // an installed `G + 1` has already dropped it.
    let listed = intents(&reopened);
    if case.installed {
        assert!(listed.is_empty(), "the settled intent at {case:?}");
    } else {
        assert_eq!(listed.len(), 1, "admission intent at {case:?}");
        assert_eq!(listed[0].op_id, op);
        assert_eq!(listed[0].kind, GcIntentKind::Compaction);
        assert_eq!(listed[0].scope_kind, GcScopeKind::Store);
    }

    // Wholly old or wholly new: never a mixed Store.
    let expected_generation = if case.installed { before + 1 } else { before };
    assert_eq!(
        generation(&reopened),
        expected_generation,
        "generation at {case:?}"
    );
    assert!(located(&reopened, live), "live root at {case:?}");
    assert_eq!(
        object_bytes(&reopened, live).as_slice(),
        live_bytes,
        "live bytes readable at {case:?}"
    );
    assert_eq!(
        located(&reopened, dead),
        !case.installed,
        "the dead pack at {case:?}"
    );
    assert!(
        reopened.history.layer_stack(stack.id).unwrap().is_some(),
        "retained history readable at {case:?}"
    );

    // A crash during the copy can tear the staged file. The next run must
    // remove the recorded set instead of trusting it.
    if case.failpoint == CompactFailpoint::DuringCopy {
        let target = compact_path(store);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&target)
            .unwrap()
            .set_len(17)
            .unwrap();
        std::fs::write(with_suffix(&target, "-journal"), b"torn copy journal").unwrap();
    }
    // A crash partway through settle can leave a subset of the recorded set.
    // Put one back to prove the next run removes the whole set.
    if case.failpoint == CompactFailpoint::DuringSettle {
        std::fs::write(
            with_suffix(&compact_path(store), "-journal"),
            b"partial settle",
        )
        .unwrap();
        std::fs::write(visited_path(store), b"partial settle").unwrap();
    }

    // The next run judges the leftover and reaches a clean state: a rebuild
    // from `G` before install, a settle at `G + 1` after it.
    let receipt = reopened.compact(&request(42)).unwrap();
    if case.installed {
        assert_eq!(receipt.path, CompactPath::None, "settle at {case:?}");
        assert_eq!(receipt.generation_before, before + 1);
        assert_eq!(receipt.generation_after, before + 1);
    } else {
        assert_eq!(receipt.path, CompactPath::Rebuild, "rebuild at {case:?}");
        assert_eq!(receipt.generation_before, before);
        assert_eq!(receipt.generation_after, before + 1);
    }
    assert!(
        receipt.residue.artifacts.is_empty(),
        "residue at {case:?}: {:?}",
        receipt.residue
    );

    assert!(compact_op_status(store).is_none(), "op settled at {case:?}");
    assert!(!compact_path(store).exists(), "target removed at {case:?}");
    assert!(!visited_path(store).exists(), "visited removed at {case:?}");
    assert!(!op_path(store).exists(), "op removed at {case:?}");
    assert_no_sidecars(store, case);

    let final_handles = reopen(store);
    assert_eq!(generation(&final_handles), before + 1, "final generation");
    assert!(located(&final_handles, live), "final live root");
    assert_eq!(
        object_bytes(&final_handles, live).as_slice(),
        live_bytes,
        "final bytes"
    );
    assert!(!located(&final_handles, dead), "the dead pack is collected");
    assert!(intents(&final_handles).is_empty(), "the intent is released");
    // Closure and the full doctor both pass on the settled Store.
    final_handles.history.mark().unwrap();
    clean(&final_handles);
}

fn exercise(case: Case) {
    let (_temp, handles, path) = create_store();
    let live = filesystem_root(&handles, 1);
    let stack = initialize(&handles, 1, live);
    let dead = whole_file(&handles, b"a pack no retained root reaches");
    let live_bytes = object_bytes(&handles, live);
    let before = generation(&handles);

    let op = [41u8; 16];
    let error = handles
        .compact(&request_at(op, case.failpoint))
        .unwrap_err();
    assert!(
        matches!(error, layerfs_history::HistoryError::UnknownOutcome),
        "{error:?}"
    );

    assert_crash_state(case, &path, before, op);
    assert_recovery(Recovery {
        case,
        store: &path,
        before,
        live,
        live_bytes: &live_bytes,
        dead,
        stack,
        op,
    });
}

macro_rules! crash_at {
    ($name:ident, $failpoint:expr, $phase:literal, $installed:literal, $target:literal, $visited:literal) => {
        #[test]
        fn $name() {
            exercise(Case {
                failpoint: $failpoint,
                phase: $phase,
                installed: $installed,
                target: $target,
                visited: $visited,
                live_sidecars: false,
            });
        }
    };
}

crash_at!(
    crash_before_seal,
    CompactFailpoint::BeforeSeal,
    "mark",
    false,
    false,
    false
);
crash_at!(
    crash_before_seal_read,
    CompactFailpoint::BeforeSealRead,
    "mark",
    false,
    false,
    false
);
crash_at!(
    crash_after_seal,
    CompactFailpoint::AfterSeal,
    "mark",
    false,
    false,
    false
);
crash_at!(
    crash_before_copy,
    CompactFailpoint::BeforeCopy,
    "build",
    false,
    false,
    false
);
crash_at!(
    crash_during_copy,
    CompactFailpoint::DuringCopy,
    "build",
    false,
    true,
    true
);
crash_at!(
    crash_after_build,
    CompactFailpoint::AfterBuild,
    "build",
    false,
    true,
    true
);
crash_at!(
    crash_after_wal_switch,
    CompactFailpoint::AfterWalSwitch,
    "build",
    false,
    true,
    true
);
crash_at!(
    crash_during_verify,
    CompactFailpoint::DuringVerify,
    "verify",
    false,
    true,
    true
);
crash_at!(
    crash_after_verify,
    CompactFailpoint::AfterVerify,
    "verify",
    false,
    true,
    true
);
crash_at!(
    crash_before_install,
    CompactFailpoint::BeforeInstall,
    "verify",
    false,
    true,
    true
);
crash_at!(
    crash_during_rename,
    CompactFailpoint::DuringRename,
    "install",
    true,
    false,
    true
);
crash_at!(
    crash_after_install,
    CompactFailpoint::AfterInstall,
    "install",
    true,
    false,
    true
);
crash_at!(
    crash_during_settle,
    CompactFailpoint::DuringSettle,
    "install",
    true,
    false,
    false
);

/// The same §9 crash points, but the compaction runs in a **separate process**
/// that the parent `SIGKILL`s while it is stopped inside the failpoint. This
/// is the crash arm the in-process matrix above cannot claim: no Rust `Drop`,
/// no connection close and no guard release runs after the kill.
///
/// The child re-executes this test binary by name and reopens the fixture
/// Store the parent built. The parent learns the child reached the point from
/// a Unix-socket gate (no sleeps), then kills it, then makes the same
/// assertions as `assert_crash_state` / `assert_recovery`.
///
/// Nine points are covered: the mark phase (`BeforeSeal`, `AfterSeal`); the
/// copy (`BeforeVacuum`, immediately before `VACUUM INTO` because SQLite runs
/// that copy as one uninterruptible call, so a kill cannot be injected inside
/// it); verification (`DuringVerify`); the rename boundary (`DuringRename`);
/// and the earlier build, install and settle points.
///
/// Linux only: `SIGKILL` delivery and `ExitStatusExt::signal` are asserted
/// through the Unix process API. On other Unix targets the test below prints
/// a skip line instead of claiming coverage.
#[cfg(target_os = "linux")]
mod real_process {
    use super::*;
    use std::io::Read;
    use std::os::unix::process::ExitStatusExt;
    use std::process::{Child, Command, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    /// The operation identity the child records and the parent asserts.
    const OP: [u8; 16] = [43u8; 16];
    const STORE_ENV: &str = "LAYERFS_COMPACT_CRASH_STORE";
    const FAILPOINT_ENV: &str = "LAYERFS_COMPACT_CRASH_FAILPOINT";
    const GATE_ENV: &str = "LAYERFS_COMPACT_CRASH_GATE";

    /// A short socket path: `Temp` lives under the (ext4) sandbox run
    /// directory, whose absolute path exceeds `SUN_LEN`.
    fn gate_socket() -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        PathBuf::from(format!(
            "/tmp/layerfs-compact-gate-{}-{}.sock",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn parse_failpoint(text: &str) -> CompactFailpoint {
        match text {
            "BeforeSeal" => CompactFailpoint::BeforeSeal,
            "AfterSeal" => CompactFailpoint::AfterSeal,
            "BeforeVacuum" => CompactFailpoint::BeforeVacuum,
            "DuringVerify" => CompactFailpoint::DuringVerify,
            "DuringRename" => CompactFailpoint::DuringRename,
            "AfterBuild" => CompactFailpoint::AfterBuild,
            "BeforeInstall" => CompactFailpoint::BeforeInstall,
            "AfterInstall" => CompactFailpoint::AfterInstall,
            "DuringSettle" => CompactFailpoint::DuringSettle,
            other => panic!("unknown failpoint {other}"),
        }
    }

    /// The child body. A normal test run returns immediately; the parent
    /// re-executes this test by name with the fixture variables set.
    #[test]
    fn sigkill_at_failpoint() {
        let Some(store) = std::env::var_os(STORE_ENV) else {
            return;
        };
        let point = parse_failpoint(
            &std::env::var(FAILPOINT_ENV).expect("a failpoint for the crash child"),
        );
        let handles = reopen(Path::new(&store));
        let outcome = handles.compact(&request_at(OP, point));
        panic!("the parent did not SIGKILL the run: {outcome:?}");
    }

    /// Kills the child on every path out of the test, including a panic.
    struct KillOnDrop(Option<Child>);

    impl KillOnDrop {
        fn new(child: Child) -> Self {
            Self(Some(child))
        }

        fn child_mut(&mut self) -> &mut Child {
            self.0.as_mut().expect("a running child")
        }

        fn kill(&mut self) -> std::process::ExitStatus {
            let mut child = self.0.take().expect("a running child");
            child.kill().expect("SIGKILL the crash child");
            child.wait().expect("the crash child exits")
        }
    }

    impl Drop for KillOnDrop {
        fn drop(&mut self) {
            if let Some(mut child) = self.0.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    /// Waits for the child's gate connection. `yield_now` keeps the bounded
    /// wait from sleeping and still notices a child that ended early.
    fn wait_for_gate(
        listener: &std::os::unix::net::UnixListener,
        child: &mut Child,
    ) -> std::os::unix::net::UnixStream {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            match listener.accept() {
                Ok((stream, _)) => return stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if let Some(status) = child.try_wait().expect("the crash child's status") {
                        panic!("the crash child ended before the gate: {status}");
                    }
                    assert!(
                        Instant::now() < deadline,
                        "the crash child never reached the gate"
                    );
                    std::thread::yield_now();
                }
                Err(error) => panic!("crash gate accept: {error}"),
            }
        }
    }

    fn exercise_sigkill(case: Case) {
        let (_temp, handles, path) = create_store();
        let live = filesystem_root(&handles, 1);
        let stack = initialize(&handles, 1, live);
        let dead = whole_file(&handles, b"a pack no retained root reaches");
        let live_bytes = object_bytes(&handles, live);
        let before = generation(&handles);

        // The child owns the writable Store alone; the parent keeps only the
        // fixture paths and the expected bytes.
        drop(handles);

        let gate = gate_socket();
        let listener =
            std::os::unix::net::UnixListener::bind(&gate).expect("the crash gate socket");
        listener
            .set_nonblocking(true)
            .expect("a nonblocking crash gate");
        let child = Command::new(std::env::current_exe().expect("the test binary"))
            .args([
                "--exact",
                "real_process::sigkill_at_failpoint",
                "--nocapture",
            ])
            .env(STORE_ENV, &path)
            .env(FAILPOINT_ENV, format!("{:?}", case.failpoint))
            .env(GATE_ENV, &gate)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("a separate crash process");
        let mut child = KillOnDrop::new(child);

        let mut gate_stream = wait_for_gate(&listener, child.child_mut());
        gate_stream
            .set_read_timeout(Some(Duration::from_secs(60)))
            .expect("a bounded gate read");
        let mut ready = [0u8; 1];
        gate_stream
            .read_exact(&mut ready)
            .expect("the child reached the crash gate");
        assert_eq!(ready[0], b'R', "the child stopped at the crash gate");

        // The real crash: SIGKILL while the child is still inside the run.
        let status = child.kill();
        assert_eq!(
            status.signal(),
            Some(9),
            "the crash child was SIGKILLed: {status:?}"
        );
        drop(gate_stream);
        let _ = std::fs::remove_file(&gate);

        assert_crash_state(case, &path, before, OP);
        assert_recovery(Recovery {
            case,
            store: &path,
            before,
            live,
            live_bytes: &live_bytes,
            dead,
            stack,
            op: OP,
        });
    }

    macro_rules! sigkill_at {
        ($name:ident, $failpoint:expr, $phase:literal, $installed:literal, $target:literal, $visited:literal, $live_sidecars:literal) => {
            #[test]
            fn $name() {
                exercise_sigkill(Case {
                    failpoint: $failpoint,
                    phase: $phase,
                    installed: $installed,
                    target: $target,
                    visited: $visited,
                    live_sidecars: $live_sidecars,
                });
            }
        };
    }

    // The mark phase: the open session and admission intent at `BeforeSeal`
    // (the real kill leaves the writer's WAL sidecars), and the sealed,
    // intent-released Store at `AfterSeal`.
    sigkill_at!(
        sigkill_before_seal,
        CompactFailpoint::BeforeSeal,
        "mark",
        false,
        false,
        false,
        true
    );
    sigkill_at!(
        sigkill_after_seal,
        CompactFailpoint::AfterSeal,
        "mark",
        false,
        false,
        false,
        false
    );
    // The copy: SQLite runs `VACUUM INTO` as one call that cannot be
    // interrupted deterministically, so this arm stops immediately before it.
    // The in-process `DuringCopy` arm keeps the torn-target state; this real
    // kill proves the crash after the visited scratch exists and before the
    // staged file does.
    sigkill_at!(
        sigkill_during_copy,
        CompactFailpoint::BeforeVacuum,
        "build",
        false,
        false,
        true,
        false
    );
    // The verify pass and the rename boundary, at the same failpoints the
    // in-process matrix uses.
    sigkill_at!(
        sigkill_during_verify,
        CompactFailpoint::DuringVerify,
        "verify",
        false,
        true,
        true,
        false
    );
    sigkill_at!(
        sigkill_during_rename,
        CompactFailpoint::DuringRename,
        "install",
        true,
        false,
        true,
        false
    );

    sigkill_at!(
        sigkill_after_build,
        CompactFailpoint::AfterBuild,
        "build",
        false,
        true,
        true,
        false
    );
    sigkill_at!(
        sigkill_before_install,
        CompactFailpoint::BeforeInstall,
        "verify",
        false,
        true,
        true,
        false
    );
    sigkill_at!(
        sigkill_after_install,
        CompactFailpoint::AfterInstall,
        "install",
        true,
        false,
        true,
        false
    );
    sigkill_at!(
        sigkill_during_settle,
        CompactFailpoint::DuringSettle,
        "install",
        true,
        false,
        false,
        false
    );
}

#[cfg(not(target_os = "linux"))]
#[test]
fn real_process_sigkill_skipped_outside_linux() {
    eprintln!(
        "skipped: the real-process SIGKILL compaction test requires Linux, not {}",
        std::env::consts::OS
    );
}

/// §5: a compaction refuses while an Init/merge working row is present, so a
/// rebuild never wipes an operation it was not admitted to touch.
#[test]
fn unfinished_init_or_merge_rows_refuse_compaction() {
    let (_temp, handles, path) = create_store();
    let connection = rusqlite::Connection::open(&path).unwrap();
    let table = |name: &str| {
        connection
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name=?1",
                [name],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
            > 0
    };
    if !table("init_operation") {
        connection
            .execute_batch(include_str!("../sql/sqlite/acquisition/schema.sql"))
            .unwrap();
    }
    connection
        .execute(
            "INSERT INTO init_operation(owner_epoch,phase,source_device,source_inode,stack_id,scope) \
             VALUES(1,1,zeroblob(8),zeroblob(8),zeroblob(16),zeroblob(32))",
            [],
        )
        .unwrap();
    drop(connection);
    let error = handles.compact(&request(99)).unwrap_err();
    assert!(
        matches!(error, layerfs_history::HistoryError::Busy),
        "an unfinished Init must refuse the compaction: {error:?}"
    );
}
