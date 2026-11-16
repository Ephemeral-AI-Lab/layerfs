//! #41 T12.3a: the SQLite half of the section 12 acceptance matrix, executed
//! and reported.
//!
//! One runner covers AC-01..AC-60: every row is classified as executed here,
//! covered by a named existing test or fixture (asserted to exist), partial
//! (some SQLite arms evidenced, the rest named), deferred (the row is
//! PostgreSQL-only and belongs to T12.3b) or an explicit SQLite gap. The
//! previously uncovered priority rows with SQLite-only contract arms are
//! executed in this file: AC-33 (checkpoint busy and sidecar-at-install),
//! AC-46 (install fence), AC-51 (the recorded op phase), AC-55 (exact SQLite
//! refusal texts) and AC-59 (seal). AC-57 gets a three-run measurement arm;
//! AC-04 gets a seeded reachability property arm (the full random
//! fork/commit/merge/publish history generator is deferred).
//!
//! T12.3c closes the remaining SQLite gaps this runner can execute: AC-06
//! (a `file_digest` row whose root the rebuild collected), and AC-07/AC-49
//! (a mount and a compaction exclude each other; a Commit racing the rebuild
//! is fenced or lands). The derived-tree rows (AC-11..AC-14, AC-22), the bind
//! failure (AC-34) and the worker lost reply (AC-36) are named Store
//! service/control tests; their rows point at those files.
//!
//! The table is checked, not decorative: a `covered`/`partial` row names a
//! file and test function that must exist, and an `executed` row names an arm
//! this binary really runs. The report is written to
//! `core/docs/issues/project-41/gc-t12.3a-sqlite-matrix-report.md`; run with
//! `UPDATE_REPORT=1` to regenerate it, and `UPDATE_MEASURE=1` to refresh the
//! three-run measurement fixture. A normal run asserts both files are current
//! and shape-valid.
//!
//! This page adds no contract: the frozen spec section 12 and the AC-01..AC-60
//! dispositions tracked on layerfs-project#52 win. Where a row is
//! cross-backend, its PostgreSQL half is deferred by name.
#![cfg(any(target_os = "linux", target_os = "macos"))]
mod support;

use layerfs_content::{encode_whole_file_payload, FinalizedObject, ObjectId, ObjectRole};
use layerfs_history::records::{FileDigest, GcIntent, GcIntentKind, GcScopeKind};
use layerfs_history::{
    BranchId, CommitId, CommitMeta, CommitOutcome, CommitRequest, CommitVia, ForkRequest,
    ForkSource, HistoryCatalog, HistoryCatalogConfig, HistoryError, LayerStackId, LayerStackRecord,
    StackInitialization,
};
use layerfs_persistence::{
    compact_op_status, CompactFailpoint, CompactPath, CompactRequest, PersistenceConfig,
    SqliteHandles, SqlitePersistenceProfile,
};
use layerfs_storage::{port::PackPersistence, Storage, StoragePolicy};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Instant;
use support::Temp;

const BINDING: &[u8] = b"sqlite-compact-matrix-binding";
const REPORT_RELATIVE: &str = "core/docs/issues/project-41/gc-t12.3a-sqlite-matrix-report.md";
const MEASURE_RELATIVE: &str =
    "engine/crates/layerfs-persistence/tests/fixtures/sqlite_compact_measure.json";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn report_path() -> PathBuf {
    repo_root().join(REPORT_RELATIVE)
}

fn measure_path() -> PathBuf {
    repo_root().join(MEASURE_RELATIVE)
}

// ---- the Store helpers (the T12.1/T12.2 fixture pattern) -------------------

fn create_store() -> (Temp, SqliteHandles, PathBuf) {
    let temp = Temp::new("sqlite-compact-matrix");
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

fn whole_file(handles: &SqliteHandles, bytes: &[u8]) -> ObjectId {
    let object = FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(bytes).unwrap(),
    )
    .unwrap();
    save_objects(handles, &[object])
}

fn save_objects(handles: &SqliteHandles, objects: &[FinalizedObject]) -> ObjectId {
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    let mut last = None;
    for object in objects {
        last = Some(object.id());
        save.accept(object.clone()).unwrap();
    }
    save.finish().unwrap();
    last.expect("at least one object")
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
    save_objects(handles, &[directory, inode, object])
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

/// One live root, one Stack and a wholly dead pack: the minimal `rebuild` arm.
fn rebuild_store() -> (Temp, SqliteHandles, PathBuf, ObjectId) {
    let (temp, handles, path) = create_store();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let _dead = whole_file(&handles, b"a pack no retained root reaches");
    (temp, handles, path, live)
}

// ---- arms executed by the runner -------------------------------------------

fn arm_checkpoint_busy() {
    let (_temp, handles, path, live) = rebuild_store();
    // A live reader's snapshot in the WAL makes wal_checkpoint(TRUNCATE)
    // report busy > 0; step 1 refuses Busy before any artifact or op.
    let reader = rusqlite::Connection::open(&path).expect("a second connection");
    let mode: String = reader
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .expect("journal mode");
    assert_eq!(
        mode.to_ascii_lowercase(),
        "wal",
        "the disposable profile is WAL"
    );
    reader.execute_batch("BEGIN").unwrap();
    let rows: i64 = reader
        .query_row("SELECT count(*) FROM pack", [], |row| row.get(0))
        .unwrap();
    assert!(rows >= 1, "the reader holds a nonempty snapshot");

    let error = handles.compact(&request(41)).unwrap_err();
    assert!(matches!(error, HistoryError::Busy), "{error:?}");
    assert!(!compact_path(&path).exists(), "no staged Store on Busy");
    assert!(!visited_path(&path).exists(), "no visited scratch on Busy");
    assert!(!op_path(&path).exists(), "no op record on Busy");
    let wal = with_suffix(&path, "-wal");
    assert!(
        std::fs::metadata(&wal).map(|meta| meta.len()).unwrap_or(0) > 0,
        "the refused seal never unlinks the WAL"
    );

    reader.execute_batch("ROLLBACK").unwrap();
    drop(reader);
    // The Store is left wholly old at G and the next run rebuilds it.
    let reopened = reopen(&path);
    let receipt = reopened.compact(&request(42)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    let final_handles = reopen(&path);
    assert!(located(&final_handles, live), "the live root survives");
    clean(&final_handles);
}

/// The install hook writes a -mj* super-journal beside the sealed Store:
/// step 7's sidecar fence must refuse Conflict and write nothing.
fn place_super_journal(store: &Path) {
    let mut name = store.file_name().expect("a file name").to_os_string();
    name.push("-mj-t123a");
    std::fs::write(store.with_file_name(name), b"t123a super-journal").unwrap();
}

fn arm_sidecar_at_install() {
    let (_temp, handles, path) = create_store();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let _dead = whole_file(&handles, b"a pack the rebuild would collect");
    let before = generation(&handles);
    let hook_request = CompactRequest {
        op_id: [43; 16],
        before_install_hook: Some(place_super_journal),
        ..CompactRequest::default()
    };
    let error = handles.compact(&hook_request).unwrap_err();
    match error {
        HistoryError::Conflict(text) => assert_eq!(text, "Store sidecar at install"),
        other => panic!("expected the sidecar Conflict, got {other:?}"),
    }
    assert!(
        !compact_path(&path).exists(),
        "Conflict leaves no staged Store"
    );
    assert!(
        !visited_path(&path).exists(),
        "Conflict leaves no visited scratch"
    );
    assert!(!op_path(&path).exists(), "Conflict leaves no op record");
    let mut name = path.file_name().unwrap().to_os_string();
    name.push("-mj-t123a");
    let _ = std::fs::remove_file(path.with_file_name(name));

    let reopened = reopen(&path);
    assert_eq!(generation(&reopened), before, "generation is unchanged");
    assert!(located(&reopened, live), "the live root survives");
}

fn arm_status_phases() {
    // One stop point per SQLite phase, including an installed one (install).
    let cases: [(CompactFailpoint, &str, bool); 4] = [
        (CompactFailpoint::BeforeSeal, "mark", false),
        (CompactFailpoint::BeforeCopy, "build", false),
        (CompactFailpoint::DuringVerify, "verify", false),
        (CompactFailpoint::DuringRename, "install", true),
    ];
    let mut seen = BTreeSet::new();
    for (index, (failpoint, phase, installed)) in cases.iter().enumerate() {
        let op = [u8::try_from(50 + index).unwrap(); 16];
        let (_temp, handles, path) = create_store();
        let live = filesystem_root(&handles, 1);
        initialize(&handles, 1, live);
        let _dead = whole_file(&handles, b"a pack the rebuild would collect");
        let before = generation(&handles);
        let error = handles.compact(&request_at(op, *failpoint)).unwrap_err();
        assert!(matches!(error, HistoryError::UnknownOutcome), "{error:?}");
        let status = compact_op_status(&path).expect("the op record remains");
        assert_eq!(status.op_id, op);
        assert_eq!(status.generation, i64::try_from(before).unwrap());
        assert_eq!(status.phase, *phase);
        assert!(status.uncertain, "an unfinished run is uncertain");
        seen.insert(status.phase.clone());

        // A pre-install stop rebuilds at G + 1; an installed one settles.
        let reopened = reopen(&path);
        let receipt = reopened.compact(&request(op[0].wrapping_add(20))).unwrap();
        if *installed {
            assert_eq!(receipt.path, CompactPath::None);
            assert_eq!(receipt.generation_before, before + 1);
            assert_eq!(receipt.generation_after, before + 1);
        } else {
            assert_eq!(receipt.path, CompactPath::Rebuild);
            assert_eq!(receipt.generation_before, before);
            assert_eq!(receipt.generation_after, before + 1);
        }
        assert!(compact_op_status(&path).is_none(), "the op settled");
    }
    assert_eq!(
        seen.iter().map(String::as_str).collect::<Vec<_>>(),
        vec!["build", "install", "mark", "verify"],
        "every SQLite phase was observed"
    );
}

fn arm_error_codes() {
    // A stray staged name no record names is Exists, and is not deleted.
    let (_temp, handles, path) = create_store();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let _dead = whole_file(&handles, b"a pack the rebuild would collect");
    let stray = compact_path(&path);
    std::fs::write(&stray, b"not a Store").unwrap();
    let error = handles.compact(&request(44)).unwrap_err();
    match error {
        HistoryError::Exists(text) => assert_eq!(text, "unrecorded compaction artifact"),
        other => panic!("expected Exists, got {other:?}"),
    }
    assert!(stray.exists(), "a name no record names is not deleted");

    // The bounded-filesystem failpoint is the exact Capacity("disk space").
    let (_temp2, handles2, path2) = create_store();
    let live2 = filesystem_root(&handles2, 1);
    initialize(&handles2, 1, live2);
    let _dead2 = whole_file(&handles2, b"a pack the rebuild would collect");
    let before = generation(&handles2);
    let error = handles2
        .compact(&request_at([45; 16], CompactFailpoint::CapacityAtBuild))
        .unwrap_err();
    match error {
        HistoryError::Capacity(text) => assert_eq!(text, "disk space"),
        other => panic!("expected Capacity, got {other:?}"),
    }
    assert!(!compact_path(&path2).exists());
    assert!(!visited_path(&path2).exists());
    assert!(!op_path(&path2).exists());
    let reopened = reopen(&path2);
    assert_eq!(generation(&reopened), before, "the old Store is unchanged");
    assert!(located(&reopened, live2));
}

fn arm_seal() {
    let (_temp, handles, path, live) = rebuild_store();
    let sealed = handles.seal().expect("the seal");
    let bytes = std::fs::metadata(&path).unwrap().len();
    assert_eq!(sealed.path, path);
    assert_eq!(sealed.bytes, bytes);
    assert!(sealed.bytes > 0);
    assert!(
        sealed.sqlite_version.starts_with("3."),
        "{}",
        sealed.sqlite_version
    );
    for suffix in ["-wal", "-shm", "-journal"] {
        assert!(
            !with_suffix(&path, suffix).exists(),
            "the seal leaves no {suffix} sidecar"
        );
    }
    let reopened = reopen(&path);
    assert!(located(&reopened, live));
    clean(&reopened);
}

/// §12 AC-06: a `file_digest` row named a root the rebuild collects; the row
/// goes with its root and a later Save of the same content records the newer
/// root without a stale row blocking it.
fn arm_digest_row_after_collection() {
    let (_temp, handles, path) = create_store();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let dead_root = filesystem_root(&handles, 9);
    let digest = [0xA6; 32];
    let length = 16 * 1024 * 1024u64;
    handles
        .history
        .record_file_digests(&[FileDigest {
            root: *dead_root.as_bytes(),
            length,
            digest,
        }])
        .unwrap();
    assert_eq!(
        handles.history.file_roots(&[(digest, length)]).unwrap(),
        vec![Some(*dead_root.as_bytes())]
    );

    let receipt = handles.compact(&request(80)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    let reopened = reopen(&path);
    assert!(
        !located(&reopened, dead_root),
        "the named root was collected"
    );
    assert_eq!(
        reopened.history.file_roots(&[(digest, length)]).unwrap(),
        vec![None],
        "the row went with its root"
    );

    // A Save of bytes with that content records the new root; no stale row
    // remains to block it.
    let new_root = filesystem_root(&reopened, 2);
    initialize(&reopened, 2, new_root);
    reopened
        .history
        .record_file_digests(&[FileDigest {
            root: *new_root.as_bytes(),
            length,
            digest,
        }])
        .unwrap();
    assert_eq!(
        reopened.history.file_roots(&[(digest, length)]).unwrap(),
        vec![Some(*new_root.as_bytes())]
    );
}

/// One mount intent naming its Workspace, Sandbox and machine.
fn mount_intent(
    stack: LayerStackId,
    branch: BranchId,
    op: u8,
    workspace: [u8; 32],
    sandbox: &str,
    machine: &str,
) -> GcIntent {
    GcIntent::shared(
        [op; 16],
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.as_slice().to_vec(),
    )
    .in_project(stack.as_slice())
    .named(Some("project-1"), Some("work"))
    .placed(Some(workspace), Some(sandbox), Some(machine))
    .owned(Some("runtime-t123c"), Some("boot-t123c"))
}

fn hex16(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// A Stack, a Branch and one wholly dead pack: the fixture the admission arms
/// need.
fn mounted_store() -> (Temp, SqliteHandles, PathBuf, LayerStackId, BranchId) {
    let (temp, handles, path) = create_store();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let stack = support::stack(1);
    let branch = support::branch_id(1);
    let head = handles
        .history
        .layer_stack(stack)
        .unwrap()
        .unwrap()
        .head_layer;
    handles
        .history
        .fork(&ForkRequest {
            stack,
            branch,
            name: support::name("work"),
            source: ForkSource::Layer(head),
        })
        .unwrap();
    let _dead = whole_file(&handles, b"a pack the rebuild would collect");
    (temp, handles, path, stack, branch)
}

/// §12 AC-07 / AC-49: a mount excludes compaction and a live compaction
/// excludes a mount and a second compaction.
fn arm_mount_and_compaction_exclude_each_other() {
    let (_temp, handles, path, stack, branch) = mounted_store();
    let mount = mount_intent(
        stack,
        branch,
        81,
        [0x81; 32],
        "sandbox-t123c",
        "machine-t123c",
    );
    handles.history.gc_register(&mount).unwrap();

    let error = handles.compact(&request(82)).unwrap_err();
    match error {
        HistoryError::MountsActive(state) => {
            assert_eq!(state.holders.len(), 1, "{state:?}");
            let holder = &state.holders[0];
            assert_eq!(holder.kind, GcIntentKind::Mount);
            assert_eq!(
                holder.workspace.as_deref(),
                Some(hex16(&[0x81; 32]).as_str())
            );
            assert_eq!(holder.sandbox.as_deref(), Some("sandbox-t123c"));
            assert_eq!(holder.machine.as_deref(), Some("machine-t123c"));
            assert_eq!(holder.op, hex16(&[81; 16]));
        }
        other => panic!("expected MountsActive, got {other:?}"),
    }
    assert!(
        !compact_path(&path).exists(),
        "a refused run stages no Store"
    );
    assert!(!visited_path(&path).exists(), "and no visited scratch");
    assert!(!op_path(&path).exists(), "and no op record");
    let reopened = reopen(&path);
    reopened.history.gc_release(&mount.op_id).unwrap();

    // A live compaction intent refuses a mount and a second compaction.
    let catalog = reopened.history.catalog_id();
    let compaction = GcIntent::operation(
        [83; 16],
        GcIntentKind::Compaction,
        GcScopeKind::Store,
        catalog.as_slice().to_vec(),
    )
    .owned(Some("runtime-t123c"), Some("boot-t123c"))
    .with_phase(Some("build"));
    reopened.history.gc_register(&compaction).unwrap();
    let mount_again = mount_intent(stack, branch, 84, [0x84; 32], "s", "m");
    match reopened.history.gc_register(&mount_again).unwrap_err() {
        HistoryError::BusyOp(state) => {
            assert_eq!(state.kind, GcIntentKind::Compaction);
            assert_eq!(state.entity, "store");
            assert_eq!(state.phase.as_deref(), Some("build"));
            assert_eq!(state.op, hex16(&[83; 16]));
            assert!(!state.uncertain);
        }
        other => panic!("expected BusyOp, got {other:?}"),
    }
    let second = reopen(&path).compact(&request(85)).unwrap_err();
    assert!(matches!(second, HistoryError::BusyOp(_)), "{second:?}");
    assert!(
        !op_path(&path).exists(),
        "a refused second run leaves no op"
    );
    reopened.history.gc_release(&compaction.op_id).unwrap();
    drop(reopened);

    // The Store is still wholly old and the next run rebuilds it.
    let receipt = reopen(&path).compact(&request(86)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    let final_handles = reopen(&path);
    let mount = mount_intent(stack, branch, 87, [0x87; 32], "s", "m");
    final_handles.history.gc_register(&mount).unwrap();
    final_handles.history.gc_release(&mount.op_id).unwrap();
    clean(&final_handles);
}

/// §12 AC-49: a mount started while a compaction is live is `Busy` with the
/// compaction's operation; the next run adopts the dead owner's leftover.
fn arm_mount_during_compaction_is_busy() {
    let (_temp, handles, path, stack, branch) = mounted_store();
    let error = handles
        .compact(&request_at([88; 16], CompactFailpoint::BeforeSeal))
        .unwrap_err();
    assert!(matches!(error, HistoryError::UnknownOutcome), "{error:?}");

    // The stopped run left its live admission intent: a mount meets the
    // compaction's operation, not a clean Store.
    let reopened = reopen(&path);
    let mount = mount_intent(stack, branch, 89, [0x89; 32], "s", "m");
    match reopened.history.gc_register(&mount).unwrap_err() {
        HistoryError::BusyOp(state) => {
            assert_eq!(state.kind, GcIntentKind::Compaction);
            assert_eq!(state.op, hex16(&[88; 16]));
            assert_eq!(state.phase.as_deref(), Some("mark"));
        }
        other => panic!("expected BusyOp, got {other:?}"),
    }

    // The next run judges the leftover and rebuilds; the mount then proceeds.
    drop(reopened);
    let receipt = reopen(&path).compact(&request(90)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    let final_handles = reopen(&path);
    final_handles.history.gc_register(&mount).unwrap();
    final_handles.history.gc_release(&mount.op_id).unwrap();
    clean(&final_handles);
}

/// The Commit thread the install hook starts: `(Commit, root)` identities.
type RebuildCommit = Result<(Vec<u8>, [u8; 32]), String>;

static REBUILD_COMMIT: std::sync::Mutex<Option<std::thread::JoinHandle<RebuildCommit>>> =
    std::sync::Mutex::new(None);

/// Commits one fresh filesystem root on `work`, opening the Store afresh in
/// this thread, so the rebuilt Store keeps a real tree.
fn commit_once(path: &Path) -> Result<(Vec<u8>, [u8; 32]), String> {
    let handles = reopen(path);
    let branch = support::branch_id(1);
    let snapshot = handles
        .history
        .branch_snapshot(branch)
        .map_err(|error| format!("{error:?}"))?
        .ok_or("the test Branch is absent")?;
    let candidate = filesystem_root(&handles, 2);
    let outcome = handles.history.commit_branch(&CommitRequest {
        branch,
        expected_head: snapshot.branch.head_commit,
        expected_base: snapshot.branch.base_layer,
        expected_root: snapshot.effective_root,
        candidate_root: candidate,
        profile: snapshot.profile,
        scope: snapshot.scope,
        second_parent: None,
        meta: CommitMeta::new(CommitVia::Files),
        force: false,
    });
    match outcome {
        Ok(CommitOutcome::Committed { record, .. }) => {
            Ok((record.id.as_slice().to_vec(), *candidate.as_bytes()))
        }
        Ok(other) => Err(format!("{other:?}")),
        Err(error) => Err(format!("{error:?}")),
    }
}

/// The §10 install hook: start a Commit while the rebuild holds the lane and
/// before the rename, so its outcome is the fence or a landed Commit.
fn commit_during_rebuild(path: &Path) {
    let path = path.to_path_buf();
    let handle = std::thread::spawn(move || commit_once(&path));
    *REBUILD_COMMIT.lock().unwrap() = Some(handle);
    std::thread::sleep(std::time::Duration::from_millis(150));
}

/// §12 AC-49: a Commit started during the rebuild is never lost silently: the
/// install fence refuses its stale handle and the retry lands on `G + 1`.
fn arm_commit_during_rebuild() {
    let (_temp, handles, path, _stack, branch) = mounted_store();
    let hook_request = CompactRequest {
        op_id: [91; 16],
        before_install_hook: Some(commit_during_rebuild),
        ..CompactRequest::default()
    };
    let receipt = handles.compact(&hook_request).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    let committed = REBUILD_COMMIT
        .lock()
        .unwrap()
        .take()
        .expect("the install hook spawned the Commit")
        .join()
        .unwrap();
    let (id, root) = match committed {
        // The thread may have started after the rename, when its handles are
        // already the rebuilt Store's: the Commit landed on `G + 1`.
        Ok(committed) => committed,
        Err(text) => {
            assert!(
                text.contains("StaleStore"),
                "the racing Commit was refused for another reason: {text}"
            );
            commit_once(&path).expect("the Commit retried on G + 1")
        }
    };
    let reopened = reopen(&path);
    let id = CommitId::from_slice(&id).unwrap();
    let record = reopened
        .history
        .commit(id)
        .unwrap()
        .expect("the Commit survives the rebuild");
    assert_eq!(record.root.as_bytes(), &root);
    assert_eq!(
        reopened
            .history
            .branch(branch)
            .unwrap()
            .unwrap()
            .head_commit,
        Some(id)
    );
    clean(&reopened);
}

fn arm_random_store_shapes() {
    // A seeded property arm over reachability shapes: every live object stays
    // readable and every wholly dead object is collected, for the retained
    // closure to be unchanged.
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    for case in 0..6u8 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let live_count = 12 + usize::try_from(state % 9).unwrap();
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let dead_count = live_count;
        let (_temp, handles, path) = create_store();
        let mut live = Vec::new();
        for index in 0..live_count {
            let seed = case * 32 + u8::try_from(index).unwrap() + 1;
            let root = filesystem_root(&handles, seed);
            let tag = u8::try_from(index + 1).unwrap();
            initialize(&handles, tag, root);
            live.push((root, object_bytes(&handles, root)));
        }
        let mut dead = Vec::new();
        for index in 0..dead_count {
            dead.push(whole_file(
                &handles,
                format!("dead-{case}-{index:03}").as_bytes(),
            ));
        }
        let before = handles.history.mark().unwrap().live_heads_digest();
        let before_generation = generation(&handles);
        let receipt = handles.compact(&request(60 + case)).unwrap();
        assert_eq!(receipt.path, CompactPath::Rebuild, "case {case}");
        assert_eq!(receipt.generation_before, before_generation);
        assert_eq!(receipt.generation_after, before_generation + 1);
        let reopened = reopen(&path);
        for (id, bytes) in &live {
            assert!(located(&reopened, *id), "live {id} at case {case}");
            assert_eq!(&object_bytes(&reopened, *id), bytes, "bytes at case {case}");
        }
        for id in &dead {
            assert!(!located(&reopened, *id), "dead {id} at case {case}");
        }
        assert_eq!(
            reopened.history.mark().unwrap().live_heads_digest(),
            before,
            "the retained closure digest at case {case}"
        );
        clean(&reopened);
    }
}

/// Three runs of the same rebuild: the E3 shape (median and range), on a small
/// fixture. The 50 GB and PostgreSQL speed/space arms stay deferred (section 13).
fn arm_measurement() {
    let mut runs = Vec::new();
    for run in 0..3u8 {
        let (_temp, handles, path) = create_store();
        let mut live = Vec::new();
        for index in 0..20 {
            let seed = run * 32 + u8::try_from(index).unwrap() + 1;
            let root = filesystem_root(&handles, seed);
            initialize(&handles, u8::try_from(index + 1).unwrap(), root);
            live.push(root);
        }
        for index in 0..20 {
            let _ = whole_file(
                &handles,
                format!("measure-dead-{run}-{index:03}").as_bytes(),
            );
        }
        let before = generation(&handles);
        let started = Instant::now();
        let receipt = handles.compact(&request(70 + run)).unwrap();
        let micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        assert_eq!(receipt.path, CompactPath::Rebuild);
        assert_eq!(receipt.generation_after, before + 1);
        assert!(receipt.dead_bytes_bound > 0);
        assert!(receipt.physical_shrink > 0);
        assert!(receipt.peak_temp_bytes > 0);
        let reopened = reopen(&path);
        for id in &live {
            assert!(located(&reopened, *id));
        }
        clean(&reopened);
        runs.push((
            micros,
            receipt.dead_bytes_bound,
            receipt.physical_shrink,
            receipt.peak_temp_bytes,
        ));
    }
    let mut times: Vec<u64> = runs.iter().map(|run| run.0).collect();
    times.sort_unstable();
    let median = times[1];
    assert!((times[0]..=times[2]).contains(&median));
    let (dead, shrink, peak) = (runs[0].1, runs[0].2, runs[0].3);
    for run in &runs[1..] {
        assert_eq!(run.1, dead, "dead bytes are deterministic");
        assert_eq!(run.2, shrink, "physical shrink is deterministic");
        assert_eq!(run.3, peak, "peak temp is deterministic");
    }
    let json = format!(
        "{{\n  \"schema\": \"t12.3a-sqlite-compact-measure/1\",\n  \
         \"runs\": {},\n  \"micros\": [{}, {}, {}],\n  \"medianMicros\": {},\n  \
         \"minMicros\": {},\n  \"maxMicros\": {},\n  \"deadBytesBound\": {},\n  \
         \"physicalShrink\": {},\n  \"peakTempBytes\": {},\n  \
         \"liveRoots\": 20,\n  \"deadPacks\": 20,\n  \
         \"profile\": \"SqlitePersistenceProfile::Disposable (WAL/OFF)\",\n  \
         \"platform\": \"{}-{}\",\n  \"rustc\": \"{}\"\n}}\n",
        runs.len(),
        runs[0].0,
        runs[1].0,
        runs[2].0,
        median,
        times[0],
        times[2],
        dead,
        shrink,
        peak,
        std::env::consts::OS,
        std::env::consts::ARCH,
        rustc_version(),
    );
    let path = measure_path();
    if std::env::var_os("UPDATE_MEASURE").is_some() {
        std::fs::write(&path, json.as_bytes()).unwrap();
    } else {
        let committed = std::fs::read_to_string(&path).unwrap_or_else(|_| {
            panic!(
                "missing {}; run UPDATE_MEASURE=1 cargo test -p layerfs-persistence --test sqlite_compact_matrix",
                path.display()
            )
        });
        // Shape-valid on any machine: three runs, median inside its own range.
        assert!(committed.contains("\"runs\": 3"), "{committed}");
        assert!(committed.contains("\"medianMicros\""), "{committed}");
        assert!(committed.contains("\"minMicros\""), "{committed}");
        assert!(committed.contains("\"maxMicros\""), "{committed}");
    }
}

fn rustc_version() -> String {
    std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

const ARMS: [(&str, fn()); 11] = [
    ("arm_checkpoint_busy", arm_checkpoint_busy),
    ("arm_sidecar_at_install", arm_sidecar_at_install),
    ("arm_status_phases", arm_status_phases),
    ("arm_error_codes", arm_error_codes),
    ("arm_seal", arm_seal),
    (
        "arm_digest_row_after_collection",
        arm_digest_row_after_collection,
    ),
    (
        "arm_mount_and_compaction_exclude_each_other",
        arm_mount_and_compaction_exclude_each_other,
    ),
    (
        "arm_mount_during_compaction_is_busy",
        arm_mount_during_compaction_is_busy,
    ),
    ("arm_commit_during_rebuild", arm_commit_during_rebuild),
    ("arm_random_store_shapes", arm_random_store_shapes),
    ("arm_measurement", arm_measurement),
];

// ---- the acceptance matrix ------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Backend {
    Both,
    Sqlite,
    Pg,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Status {
    /// An arm in this binary runs the SQLite half.
    Executed,
    /// A named existing test or fixture runs it.
    Covered,
    /// Some SQLite arms are named; the rest is named as missing.
    Partial,
    /// PostgreSQL-only (or the PostgreSQL half): T12.3b.
    Deferred,
    /// SQLite-applicable with no named evidence yet.
    Gap,
}

struct Row {
    id: &'static str,
    backend: Backend,
    tier: &'static str,
    status: Status,
    /// `repo/path.rs#test_fn`, `repo/path.json`, or `runner#arm_name`.
    evidence: &'static [&'static str],
    note: &'static str,
}

const MATRIX: [Row; 60] = [
    Row { id: "AC-01", backend: Backend::Both, tier: "E2", status: Status::Covered,
        evidence: &["engine/crates/layerfs-persistence/tests/sqlite_compact_crash.rs#crash_before_seal",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact_crash.rs#crash_during_copy",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact_crash.rs#crash_after_verify",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact_crash.rs#crash_after_install",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact_crash.rs#crash_during_settle"],
        note: "The SQLite section 9 crash matrix through named failpoints; a real kill -9 of a separate daemon process is T12.2's in-process stop points, not run here." },
    Row { id: "AC-02", backend: Backend::Both, tier: "E2", status: Status::Partial,
        evidence: &["engine/crates/layerfs-persistence/tests/sqlite_compact.rs#a_wholly_dead_pack_is_rebuilt_away",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact_fixtures.rs#the_sqlite_compaction_receipts_match_the_golden_transcript"],
        note: "The SQLite arm runs through the suite; the converted 1.0 Store arm and the PostgreSQL backend matrix are deferred to T12.3b." },
    Row { id: "AC-03", backend: Backend::Both, tier: "E2", status: Status::Covered,
        evidence: &["engine/crates/layerfs-persistence/tests/sqlite_compact_crash.rs#crash_before_seal",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact_crash.rs#crash_after_wal_switch",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact_crash.rs#crash_before_install",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact_crash.rs#crash_after_install"],
        note: "SQLite mark/build/verify/install and WAL-switch stop points; lost COMMIT and power-loss arms belong to the PostgreSQL/fsync halves." },
    Row { id: "AC-04", backend: Backend::Both, tier: "E3", status: Status::Partial,
        evidence: &["engine/crates/layerfs-persistence/tests/sqlite_compact_fixtures.rs#the_sqlite_compaction_receipts_match_the_golden_transcript",
                    "runner#arm_random_store_shapes"],
        note: "Deterministic closure fixture plus a seeded six-history reachability property arm; the full random fork/commit/merge/publish/removal generator is not built here." },
    Row { id: "AC-05", backend: Backend::Both, tier: "E2", status: Status::Covered,
        evidence: &["engine/crates/layerfs-persistence/tests/sqlite_compact_fixtures.rs#the_sqlite_compaction_receipts_match_the_golden_transcript",
                    "runner#arm_random_store_shapes"],
        note: "Full doctor (including the diagnostic scan) on the compacted Store, also in the property arm." },
    Row { id: "AC-06", backend: Backend::Both, tier: "E2", status: Status::Executed,
        evidence: &["runner#arm_digest_row_after_collection"],
        note: "A Save's `file_digest` row named a root the rebuild collected; the row goes with its root and recording the same content again succeeds. The PostgreSQL half is T12.3b." },
    Row { id: "AC-07", backend: Backend::Both, tier: "E2", status: Status::Executed,
        evidence: &["runner#arm_mount_and_compaction_exclude_each_other",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact.rs#a_store_replaced_under_an_open_handle_is_stale_at_the_next_transaction"],
        note: "SQLite half executed: bypass gives `StaleStore`, a mount blocks compaction (`MountsActive`, naming the holder) and a live compaction refuses a mount and a second compaction (`BusyOp`). The open one-shot SDK transaction arm is PostgreSQL, T12.3b." },
    Row { id: "AC-08", backend: Backend::Both, tier: "E2", status: Status::Covered,
        evidence: &["engine/crates/layerfs-persistence/tests/admission.rs#a_concurrent_mount_and_branch_remove_have_one_winner"],
        note: "One winner; the loser sees MountsActive, and a removed Branch is Missing." },
    Row { id: "AC-09", backend: Backend::Both, tier: "E2", status: Status::Covered,
        evidence: &["engine/crates/layerfs-persistence/tests/branch_removal.rs#a_removed_branch_is_hidden_and_the_epoch_bumps_once",
                    "engine/crates/layerfs-persistence/tests/branch_removal.rs#a_live_holder_is_reported_for_branch_and_project_names",
                    "api/crates/layerfs-api/tests/gc_calls.rs#branch_remove_refuses_reserved_and_absent_branches"],
        note: "Reserved main, absent Branch and holder details are asserted; project-side name collisions are in AC-30." },
    Row { id: "AC-10", backend: Backend::Both, tier: "E2", status: Status::Covered,
        evidence: &["engine/crates/layerfs-persistence/tests/path_removal.rs#marker_frontier_and_up_to_date_follow_the_store_position"],
        note: "Removed/UpToDate codes and the frontier; 2.0 frees no bytes for the removed path." },
    Row { id: "AC-11", backend: Backend::Both, tier: "E2", status: Status::Partial,
        evidence: &["engine/crates/layerfs-store-service/tests/sdk_path_whiteouts.rs#a_merge_of_two_hidden_sides_keeps_the_path_absent",
                    "engine/crates/layerfs-store-service/tests/sdk_path_whiteouts.rs#a_merge_keeps_a_post_frontier_re_creation",
                    "engine/crates/layerfs-store-service/tests/sdk_path_merge_removal.rs#a_fast_forward_merge_creates_no_tree_and_the_path_stays_hidden",
                    "engine/crates/layerfs-store-service/tests/sdk_path_merge_removal.rs#publishing_an_at_or_below_commit_yields_a_hidden_layer_and_a_layer_headed_mount"],
        note: "Non-FF whiteouts, the fast-forward control, a post-frontier re-creation and publish-at-or-below with a layer-headed mount whiteout execute. A merge that cannot materialize the whiteout has no implementation branch to refuse (`removal::derive_root` always derives), so that clause is deferred by name, not claimed." },
    Row { id: "AC-12", backend: Backend::Both, tier: "E2", status: Status::Covered,
        evidence: &["engine/crates/layerfs-store-service/tests/sdk_path_whiteouts.rs#a_merge_of_two_hidden_sides_keeps_the_path_absent",
                    "engine/crates/layerfs-store-service/tests/sdk_path_whiteouts.rs#a_merge_keeps_a_post_frontier_re_creation",
                    "engine/crates/layerfs-store-service/tests/sdk_path_merge_removal.rs#a_recreated_removed_path_still_conflicts_as_an_ordinary_edit"],
        note: "A removed path in a merge is omitted with no conflict; a post-frontier re-creation is kept, and only an ordinary edit conflict on it is reported." },
    Row { id: "AC-13", backend: Backend::Both, tier: "E2", status: Status::Covered,
        evidence: &["engine/crates/layerfs-store-service/tests/sdk_path_merge_removal.rs#an_unmerged_branch_created_before_the_frontier_hides_but_does_not_inherit"],
        note: "The earlier Commit on an unmerged Branch is hidden by the marker; a Commit after the frontier does not inherit the path." },
    Row { id: "AC-14", backend: Backend::Both, tier: "E2", status: Status::Covered,
        evidence: &["engine/crates/layerfs-store-service/tests/sdk_path_merge_removal.rs#publishing_an_at_or_below_commit_yields_a_hidden_layer_and_a_layer_headed_mount",
                    "engine/crates/layerfs-store-service/tests/sdk_path_whiteouts.rs#a_marker_under_a_live_mount_is_refused_and_materializes_after_it_closes"],
        note: "A Branch whose head is empty and whose base Layer is at or below the frontier whiteouts the path at mount; its next Commit records the deletion." },
    Row { id: "AC-15", backend: Backend::Both, tier: "E2", status: Status::Partial,
        evidence: &["engine/crates/layerfs-persistence/tests/kept_objects.rs#a_retained_tree_and_inode_are_kept_with_their_packs",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact_fixtures.rs#the_sqlite_compaction_receipts_match_the_golden_transcript"],
        note: "Retention of the only-reaching-path object is proven; marker hiding in reads/listings and derived whiteouts are not in one named test." },
    Row { id: "AC-16", backend: Backend::Both, tier: "E2", status: Status::Covered,
        evidence: &["engine/crates/layerfs-persistence/tests/branch_removal.rs#a_live_holder_is_reported_for_branch_and_project_names",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact_fixtures.rs#the_sqlite_compaction_receipts_match_the_golden_transcript"],
        note: "T12.1 pins a retained retired Branch name and a collected unreferenced one; the branch.remove-then-rebuild arm is exercised through those markers." },
    Row { id: "AC-17", backend: Backend::Both, tier: "E2", status: Status::Covered,
        evidence: &["engine/crates/layerfs-persistence/tests/sqlite_compact_fixtures.rs#the_sqlite_compaction_receipts_match_the_golden_transcript",
                    "engine/crates/layerfs-persistence/tests/kept_objects.rs#a_delta_base_is_enqueued_and_reported"],
        note: "Depth-2 chain copied, pack ids preserved." },
    Row { id: "AC-18", backend: Backend::Both, tier: "E2", status: Status::Covered,
        evidence: &["engine/crates/layerfs-persistence/tests/sqlite_compact_fixtures.rs#the_sqlite_compaction_receipts_match_the_golden_transcript"],
        note: "Collected digest row gone, retained one kept." },
    Row { id: "AC-19", backend: Backend::Both, tier: "E2", status: Status::Partial,
        evidence: &["api/crates/layerfs-api/tests/gc_calls.rs#project_remove_file_checks_its_path_grammar_then_reports_markers",
                    "engine/crates/layerfs-persistence/tests/path_removal.rs#marker_frontier_and_up_to_date_follow_the_store_position"],
        note: "Path-grammar refusals and the marker frontier are named; the full form table (symlink/hardlink, >4096 bytes, >256 components) is not one test." },
    Row { id: "AC-20", backend: Backend::Both, tier: "E2", status: Status::Partial,
        evidence: &["engine/crates/layerfs-persistence/tests/path_removal.rs#a_raw_marker_frontier_also_yields_up_to_date",
                    "engine/crates/layerfs-persistence/tests/path_removal.rs#an_unknown_project_is_missing"],
        note: "UpToDate and never-existing paths are named; the re-creation and other-project arms are partial." },
    Row { id: "AC-21", backend: Backend::Both, tier: "E2", status: Status::Partial,
        evidence: &["engine/crates/layerfs-persistence/tests/epoch_fence.rs#an_epoch_change_during_a_commit_is_detected_and_rolled_back",
                    "engine/crates/layerfs-persistence/tests/epoch_fence.rs#a_concurrent_commit_and_removal_have_one_winner"],
        note: "One-shot writes racing removals roll back as Conflict; the merge-racing-remove_file arm is not named." },
    Row { id: "AC-22", backend: Backend::Both, tier: "E3", status: Status::Covered,
        evidence: &["engine/crates/layerfs-store-service/tests/sdk_path_merge_removal.rs#a_merge_and_a_removal_started_together_never_leave_the_path_visible",
                    "engine/crates/layerfs-persistence/tests/epoch_fence.rs#an_epoch_change_during_a_merge_is_detected_and_rolled_back"],
        note: "A deterministic barrier starts a merge and a `remove_file` together for three rounds: the loser sees the marker or gets `Conflict` and retries with the whiteout, and a tree derived afterwards never shows the path. The PostgreSQL half is T12.3b." },
    Row { id: "AC-23", backend: Backend::Both, tier: "E2", status: Status::Partial,
        evidence: &["engine/crates/layerfs-persistence/tests/sqlite_compact_fixtures.rs#the_sqlite_compaction_receipts_match_the_golden_transcript",
                    "engine/crates/layerfs-persistence/tests/project_delete.rs#deleting_a_project_hides_it_and_marks_once"],
        note: "Deleted-project rows and markers are collected by the rebuild; a dedicated path_removed cleanup arm is not named." },
    Row { id: "AC-24", backend: Backend::Both, tier: "E2", status: Status::Covered,
        evidence: &["engine/crates/layerfs-persistence/tests/gc_backfill.rs#commits_layers_and_epochs_are_backfilled_in_deterministic_order",
                    "engine/crates/layerfs-persistence/tests/gc_backfill.rs#existing_serials_are_kept_and_new_ones_start_above_the_maximum"],
        note: "Serials backfilled, next_history_serial never falls." },
    Row { id: "AC-25", backend: Backend::Both, tier: "E2", status: Status::Covered,
        evidence: &["engine/crates/layerfs-persistence/tests/sqlite_compact_fixtures.rs#the_sqlite_compaction_receipts_match_the_golden_transcript"],
        note: "Retained branch_retired/layer_stack_retired markers survive; collected rows take their markers." },
    Row { id: "AC-26", backend: Backend::Both, tier: "E2", status: Status::Covered,
        evidence: &["engine/crates/layerfs-persistence/tests/project_delete.rs#retry_classifies_retired_live_and_absent",
                    "engine/crates/layerfs-persistence/tests/project_delete.rs#deleting_a_project_hides_it_and_marks_once"],
        note: "Hidden, entry present, retry returns the stored deleted_ms." },
    Row { id: "AC-27", backend: Backend::Both, tier: "E2", status: Status::Covered,
        evidence: &["engine/crates/layerfs-persistence/tests/project_delete.rs#paging_skips_a_hidden_project",
                    "engine/crates/layerfs-persistence/tests/project_delete.rs#every_layer_stack_lookup_carries_the_marker_filter"],
        note: "Stale home entries stay out of list/lookup." },
    Row { id: "AC-28", backend: Backend::Pg, tier: "E2", status: Status::Deferred,
        evidence: &[], note: "PostgreSQL Phase C cleanup and its failpoints: T12.3b." },
    Row { id: "AC-29", backend: Backend::Both, tier: "E2", status: Status::Covered,
        evidence: &["engine/crates/layerfs-persistence/tests/admission.rs#a_live_operation_is_busy_and_an_uncertain_owner_is_unresolved",
                    "engine/crates/layerfs-persistence/tests/admission.rs#a_mount_blocks_all_three_removals_until_released"],
        note: "Owner liveness and Unresolved settlement; machine_stopped and the other-home intent are partial." },
    Row { id: "AC-30", backend: Backend::Both, tier: "E2", status: Status::Partial,
        evidence: &["engine/crates/layerfs-persistence/tests/branch_removal.rs#a_live_holder_is_reported_for_branch_and_project_names",
                    "api/crates/layerfs-api/tests/gc_calls.rs#wrong_kinds_and_unknown_options_are_refused_before_any_effect"],
        note: "Holder detail and refusal codes are named; the full exact-message matrix (AC-55) is partial." },
    Row { id: "AC-31", backend: Backend::Both, tier: "E2", status: Status::Covered,
        evidence: &["api/crates/layerfs-api/tests/gc_trace.rs#path_arguments_are_byte_counts_in_the_call_log",
                    "api/crates/layerfs-api/tests/gc_trace.rs#the_trace_key_is_salted_by_the_store_policy_salt"],
        note: "Byte-count path arguments and the keyed trace hash are asserted." },
    Row { id: "AC-32", backend: Backend::Both, tier: "E2", status: Status::Partial,
        evidence: &["engine/crates/layerfs-persistence/tests/admission.rs#a_live_operation_is_busy_and_an_uncertain_owner_is_unresolved"],
        note: "Live/uncertain ownership is named; live-but-unreachable, machine_stopped and another home's intent are partial." },
    Row { id: "AC-33", backend: Backend::Sqlite, tier: "E2", status: Status::Executed,
        evidence: &["runner#arm_checkpoint_busy", "runner#arm_sidecar_at_install"],
        note: "A live reader makes wal_checkpoint(TRUNCATE) Busy with no artifact and no op; a -mj* super-journal at install is Conflict with no writes." },
    Row { id: "AC-34", backend: Backend::Both, tier: "E2", status: Status::Partial,
        evidence: &["engine/crates/layerfs-store-service/tests/sdk_mount_bind_failure.rs#a_bind_failure_cleans_its_intent_and_leaves_no_mounts_active"],
        note: "A committed mount intent whose bind fails at the root check is deleted in the error path and no `MountsActive` remains. The failed-delete sub-arm is not inducible on SQLite: the storage schema guard rejects a trigger on `gc_intent`, and a lock held across the bind refuses the register too; the explicit `Cleanup` report is covered by the seam's code path, not executed here. The PostgreSQL half is T12.3b." },
    Row { id: "AC-35", backend: Backend::Pg, tier: "E2", status: Status::Deferred,
        evidence: &[], note: "PostgreSQL advisory-key refcount: T12.3b." },
    Row { id: "AC-36", backend: Backend::Both, tier: "E2", status: Status::Covered,
        evidence: &["control/crates/layerfs-control/tests/project_delete_worker.rs#a_lost_worker_reply_is_unknown_and_keeps_the_sidecar_for_the_retry",
                    "control/crates/layerfs-store/tests/project_delete.rs#a_retired_retry_reuses_the_intent_deleted_ms_and_finishes_the_entry",
                    "control/crates/layerfs-store/tests/project_delete.rs#an_absent_stack_finishes_a_stale_entry"],
        note: "A lost marker reply is `Unknown` with the sidecar kept; the retry resolves retired, live or absent. The PostgreSQL half is T12.3b." },
    Row { id: "AC-37", backend: Backend::Both, tier: "E2", status: Status::Partial,
        evidence: &["api/crates/layerfs-api/tests/gc_trace.rs#the_redacted_set_covers_every_path_bearing_call",
                    "api/crates/layerfs-api/tests/gc_trace.rs#the_trace_key_is_salted_by_the_store_policy_salt"],
        note: "Redaction and the keyed hash are named; salt backfill on an upgraded Store is partial." },
    Row { id: "AC-38", backend: Backend::Both, tier: "E2", status: Status::Covered,
        evidence: &["engine/crates/layerfs-persistence/tests/sqlite_compact_fixtures.rs#the_sqlite_compaction_receipts_match_the_golden_transcript",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact_crash.rs#crash_during_settle"],
        note: "The SQLite artifact set is removed exactly; the PostgreSQL schema/name arms are T12.3b." },
    Row { id: "AC-39", backend: Backend::Both, tier: "E2", status: Status::Covered,
        evidence: &["engine/crates/layerfs-persistence/tests/gc_backfill.rs#commits_layers_and_epochs_are_backfilled_in_deterministic_order"],
        note: "One project_epoch row per stack, gc_op not copied, next_history_serial never lowered." },
    Row { id: "AC-40", backend: Backend::Both, tier: "E2", status: Status::Covered,
        evidence: &["engine/crates/layerfs-persistence/tests/sqlite_compact_fixtures.rs#the_sqlite_compaction_receipts_match_the_golden_transcript",
                    "engine/crates/layerfs-persistence/tests/kept_objects.rs#the_digest_is_stable_and_moves_with_a_head"],
        note: "SQLite live_heads_digest is identical before/after and moves with a head; PostgreSQL agreement is deferred." },
    Row { id: "AC-41", backend: Backend::Pg, tier: "E2", status: Status::Deferred,
        evidence: &[], note: "PostgreSQL Phase M mark transfer: T12.3b. The §11 `markBytes` transfer is measured in the walking mark and recorded as the `compact.mark_bytes` metric (asserted by `postgresql_compact_fixtures.rs`; see AC-52)." },
    Row { id: "AC-42", backend: Backend::Pg, tier: "E2", status: Status::Deferred,
        evidence: &[], note: "PostgreSQL reserved pack/Phase B: T12.3b." },
    Row { id: "AC-43", backend: Backend::Pg, tier: "E2", status: Status::Deferred,
        evidence: &[], note: "PostgreSQL Phase C cleanup resume: T12.3b." },
    Row { id: "AC-44", backend: Backend::Pg, tier: "E2", status: Status::Deferred,
        evidence: &[], note: "PostgreSQL privileges and lost COMMIT reply: T12.3b." },
    Row { id: "AC-45", backend: Backend::Sqlite, tier: "E1", status: Status::Covered,
        evidence: &["engine/crates/layerfs-persistence/tests/sqlite_compact.rs#group_rows_layout_opens_compacts_and_takes_none",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact.rs#group_rows_indexed_layout_opens_compacts_and_takes_none",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact.rs#acquisition_layout_opens_compacts_and_clears_working_rows",
                    "engine/crates/layerfs-persistence/tests/sqlite_group_rows.rs#explicit_creation_reopens_declared_units_and_keeps_sparse_canonical_order"],
        note: "Monolithic/GroupRows/GroupRowsIndexed and the acquisition variants compact; working rows are cleared." },
    Row { id: "AC-46", backend: Backend::Sqlite, tier: "E2", status: Status::Executed,
        evidence: &["runner#arm_sidecar_at_install",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact.rs#a_store_replaced_between_seal_and_install_is_stale",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact.rs#a_store_replaced_under_an_open_handle_is_stale_at_the_next_transaction"],
        note: "Sealed-state/sidecar change at install gives Conflict with no writes; a replaced Store gives StaleStore. The in-flight-transaction exclusion is a documented lane limit, not probed." },
    Row { id: "AC-47", backend: Backend::Sqlite, tier: "E2", status: Status::Covered,
        evidence: &["engine/crates/layerfs-persistence/tests/sqlite_compact_crash.rs#crash_during_copy",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact_crash.rs#crash_during_settle",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact.rs#a_stray_staged_name_is_refused_and_not_deleted",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact_fixtures.rs#the_sqlite_compaction_receipts_match_the_golden_transcript"],
        note: "Crash leaves the full set, the next compact removes it, none leaves no artifact, a stray name is Exists." },
    Row { id: "AC-48", backend: Backend::Both, tier: "E2", status: Status::Covered,
        evidence: &["engine/crates/layerfs-persistence/tests/sqlite_compact.rs#nine_percent_wholly_dead_bytes_take_the_none_path",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact.rs#ten_percent_wholly_dead_bytes_rebuild_at_the_threshold",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact.rs#thirty_percent_wholly_dead_bytes_rebuild_above_the_threshold",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact.rs#an_empty_store_takes_the_none_path",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact.rs#a_capacity_failure_leaves_the_store_unchanged"],
        note: "9/10/30 percent, empty-store zeroReason, capacity abort; verify-failure and re-refine arms are partial." },
    Row { id: "AC-49", backend: Backend::Both, tier: "E2", status: Status::Executed,
        evidence: &["runner#arm_mount_during_compaction_is_busy",
                    "runner#arm_commit_during_rebuild",
                    "engine/crates/layerfs-persistence/tests/admission.rs#a_concurrent_mount_and_branch_remove_have_one_winner"],
        note: "SQLite half executed: a mount during compact is `BusyOp` with the compaction; compaction returns `MountsActive` when the mount wins; a Commit racing the rebuild gets `StaleStore` or lands, and its retry is never lost. The PG Phase A/Phase C arms are T12.3b." },
    Row { id: "AC-50", backend: Backend::Both, tier: "E2", status: Status::Covered,
        evidence: &["api/crates/layerfs-api/tests/calls_scope.rs#a_scope_refuses_every_call_it_does_not_cover",
                    "api/crates/layerfs-api/tests/gc_calls.rs#the_four_gc_calls_take_ids_only"],
        note: "Folder scope refuses the four calls; the admin profile is the only allowance." },
    Row { id: "AC-51", backend: Backend::Both, tier: "E2", status: Status::Executed,
        evidence: &["runner#arm_status_phases",
                    "control/crates/layerfs-store/tests/compact_report_status.rs#status_projects_the_sidecar_phase_and_state",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact_crash.rs#crash_before_seal"],
        note: "The op sidecar records mark/build/verify/install (runner); T10 projects it through store.status, and the named control test asserts the phase, state and phases list." },
    Row { id: "AC-52", backend: Backend::Both, tier: "E2", status: Status::Covered,
        evidence: &["engine/crates/layerfs-persistence/tests/sqlite_compact_fixtures.rs#the_sqlite_compaction_receipts_match_the_golden_transcript",
                    "engine/crates/layerfs-persistence/tests/fixtures/sqlite_compact_golden.json",
                    "control/crates/layerfs-store/tests/compact_report_status.rs#the_report_wire_is_the_snake_case_contract_shape"],
        note: "Every section 11 field pinned with units and sampling point (golden) and rendered by the T10 wire-shape test; markBytes is a metric, not a result field, and PostgreSQL records it as `compact.mark_bytes`." },
    Row { id: "AC-53", backend: Backend::Both, tier: "E2", status: Status::Partial,
        evidence: &["api/crates/layerfs-api/tests/project_export.rs#export_omits_a_path_hidden_by_remove_file",
                    "api/crates/layerfs-api/tests/gc_calls.rs#project_remove_file_checks_its_path_grammar_then_reports_markers"],
        note: "One hidden-path export case is named; the full read-surface table (read/stat/list/glob/diff/revision/edit/grep/blame/cp/apply) is not executed." },
    Row { id: "AC-54", backend: Backend::Both, tier: "E2", status: Status::Partial,
        evidence: &["control/crates/layerfs-store/tests/restore_install.rs#a_folder_made_unprivate_during_the_download_is_refused_before_the_install",
                    "control/crates/layerfs-store/tests/restored_records.rs#a_restored_project_reads_as_a_sandbox_reads_it"],
        note: "Restore install guards and restored records are named; restore before/after a removal and the stale-entry listing arm are partial. The restore-window arm needs a lock-capable filesystem (ext4/xfs/btrfs), so on this Sandbox it reports Unsupported while the other two cases pass." },
    Row { id: "AC-55", backend: Backend::Both, tier: "E2", status: Status::Executed,
        evidence: &["runner#arm_error_codes", "runner#arm_checkpoint_busy",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact.rs#a_stray_staged_name_is_refused_and_not_deleted"],
        note: "Exact SQLite texts asserted: Busy, Conflict(\"Store sidecar at install\"), Exists(\"unrecorded compaction artifact\"), Capacity(\"disk space\"); the full engine/API message matrix stays partial." },
    Row { id: "AC-56", backend: Backend::Both, tier: "E3", status: Status::Partial,

        evidence: &["engine/crates/layerfs-persistence/tests/sqlite_compact_fixtures.rs#the_sqlite_compaction_receipts_match_the_golden_transcript",
                    "engine/crates/layerfs-persistence/tests/fixtures/sqlite_compact_golden.json"],
        note: "The literal SQLite vector is pinned (absent head 33 zero bytes); the shared PostgreSQL literal and the three-run E3 shape are deferred." },
    Row { id: "AC-57", backend: Backend::Both, tier: "E3", status: Status::Partial,
        evidence: &["runner#arm_measurement",
                    "engine/crates/layerfs-persistence/tests/fixtures/sqlite_compact_measure.json",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact_capacity.rs#capacity_measure",
                    "engine/crates/layerfs-persistence/tests/fixtures/sqlite_compact_capacity_measure.json"],
        note: "The tiny three-run arm is median 32.1 ms. T-G61 measured a real 9.987 GB Store on the whole-Store `VACUUM INTO` build (3 runs per arm): `none` at 8.88% dead median 149.318 s and `rebuild` at 10.00% dead median 246.371 s (mark 149.089 s / build 94.317 s / verify 0.312 s / install 1.916 s), plus a WAL/FULL control at 284.078 s. Re-measured on this branch with the fresh-copy build on the same 9.987 GB fixture: `rebuild` median 118.438 s (mark 74.462-75.704 s / build 18.190-19.209 s / quick-check verify 4.330-4.377 s / install 19.197-23.673 s), peak 9.144 GB; `none` median 88.439 s (82.0-149.9 s; this Sandbox shares the host); profile, host and toolchain are declared in the fixture, and the warm page cache is declared below. The §11 `markBytes` transfer is recorded per PostgreSQL run as `compact.mark_bytes`. T-G66 later measured the 50 GB arm once per path; the 30% arm, each removal type, the plain `VACUUM INTO` comparison and PostgreSQL timings remain deferred/[E]." },
    Row { id: "AC-58", backend: Backend::Sqlite, tier: "E2", status: Status::Covered,
        evidence: &["engine/crates/layerfs-persistence/tests/sqlite_compact_fixtures.rs#the_sqlite_compaction_receipts_match_the_golden_transcript",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact_crash.rs#crash_before_copy",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact_crash.rs#crash_during_copy"],
        note: "none rolls its mark transaction back (no kept_node row, no gc_intent row, no committed WAL frame), deletes the scratch and leaves no .compact; rebuild commits the mark and keeps the visited scratch through the build; step 2 writes only on rebuild." },
    Row { id: "AC-59", backend: Backend::Sqlite, tier: "E2", status: Status::Executed,
        evidence: &["runner#arm_checkpoint_busy", "runner#arm_seal",
                    "engine/crates/layerfs-persistence/tests/sqlite_compact.rs#a_store_replaced_between_seal_and_install_is_stale"],
        note: "busy != 0 refuses Busy and the WAL is never unlinked; a successful seal records (size, mtime, counter) as fenced by the StaleStore arm." },
    Row { id: "AC-60", backend: Backend::Pg, tier: "E2", status: Status::Deferred,
        evidence: &[], note: "PostgreSQL synchronous standby handling: T12.3b." },
];

// ---- report ---------------------------------------------------------------

fn evidence_label(entry: &str) -> String {
    if let Some(arm) = entry.strip_prefix("runner#") {
        return format!("this runner #{arm}");
    }
    let (file, function) = entry.split_once('#').unwrap_or((entry, ""));
    let base = file.rsplit('/').next().unwrap_or(file);
    // T-G61's opt-in capacity suite is labelled with its ticket in the matrix
    // evidence; its measured numbers are in the notes and run-environment page.
    if base == "sqlite_compact_capacity.rs" {
        return format!("T-G61 {base}#{function}");
    }
    if function.is_empty() {
        base.to_owned()
    } else {
        format!("{base}#{function}")
    }
}

fn backend_label(backend: Backend) -> &'static str {
    match backend {
        Backend::Both => "both",
        Backend::Sqlite => "SQLite",
        Backend::Pg => "PG",
    }
}

fn status_label(status: Status) -> &'static str {
    match status {
        Status::Executed => "executed",
        Status::Covered => "covered",
        Status::Partial => "partial",
        Status::Deferred => "deferred (T12.3b)",
        Status::Gap => "gap",
    }
}

/// The crash suite's test count and its real-process SIGKILL arms, read from
/// its source so the report's T-G28/T-G60 note cannot drift when an arm is
/// added: the in-process `crash_at!` arms, the `sigkill_at!` arms and the
/// crash child itself are every test the suite declares on Linux.
fn crash_suite_counts() -> (usize, usize) {
    let path = repo_root().join("engine/crates/layerfs-persistence/tests/sqlite_compact_crash.rs");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing {}; the crash suite the report counts",
            path.display()
        )
    });
    let sigkill_arms = text.matches("sigkill_at!(").count();
    let total = text.matches("crash_at!(").count()
        + sigkill_arms
        + text.matches("fn sigkill_at_failpoint(").count();
    (total, sigkill_arms)
}

fn generate_report() -> String {
    use std::fmt::Write as _;
    let count = |status: Status| MATRIX.iter().filter(|row| row.status == status).count();
    let both = MATRIX
        .iter()
        .filter(|row| row.backend == Backend::Both)
        .count();
    let sqlite = MATRIX
        .iter()
        .filter(|row| row.backend == Backend::Sqlite)
        .count();
    let pg = MATRIX
        .iter()
        .filter(|row| row.backend == Backend::Pg)
        .count();
    let mut out = String::new();
    writeln!(out, "# T12.3a: SQLite acceptance-matrix runner report").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "> Generated by `cargo test -p layerfs-persistence --test sqlite_compact_matrix`."
    )
    .unwrap();
    writeln!(
        out,
        "> The frozen [`LayerFS-GC-spec.md`](LayerFS-GC-spec.md) section 12 and"
    )
    .unwrap();
    writeln!(
        out,
        "> AC-01..AC-60 dispositions (layerfs-project#52) win; this page records"
    )
    .unwrap();
    writeln!(out, "> evidence and adds no contract. Regenerate with").unwrap();
    writeln!(
        out,
        "> `UPDATE_REPORT=1 cargo test -p layerfs-persistence --test sqlite_compact_matrix`."
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(out, "## Scope and counts").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "- Rows: {} (AC-01..AC-60); backend split: both {both}, SQLite-only {sqlite}, PG-only {pg}.",
        MATRIX.len()
    )
    .unwrap();
    writeln!(
        out,
        "- Executed here: {}; covered by a named existing test/fixture: {}; partial: {}; deferred to T12.3b: {}; SQLite gaps: {}.",
        count(Status::Executed),
        count(Status::Covered),
        count(Status::Partial),
        count(Status::Deferred),
        count(Status::Gap)
    )
    .unwrap();
    let (crash_tests, crash_sigkill_arms) = crash_suite_counts();
    writeln!(
        out,
        "- **T-G28/T-G60 follow-up.** T-G28 later added the real-process SIGKILL arms"
    )
    .unwrap();
    writeln!(
        out,
        "  and T-G60 extended them to mark, copy, verify and rename, so"
    )
    .unwrap();
    writeln!(
        out,
        "  `sqlite_compact_crash` now has {crash_tests} crash tests ({crash_sigkill_arms} real-process SIGKILL arms and the"
    )
    .unwrap();
    writeln!(
        out,
        "  child placeholder), not the 13 recorded below; the binary also runs one"
    )
    .unwrap();
    writeln!(
        out,
        "  lifecycle guard (`unfinished_init_or_merge_rows_refuse_compaction`), which"
    )
    .unwrap();
    writeln!(out, "  is not a crash arm. Those arms still kill a").unwrap();
    writeln!(out, "  re-executed test child, so T-G65 added").unwrap();
    writeln!(
        out,
        "  `api/crates/layerfs-cli/tests/sqlite_compact_process_kill.rs`: 8 SIGKILL"
    )
    .unwrap();
    writeln!(
        out,
        "  arms on the built `layerfs store compact` process at mark, build, verify and"
    )
    .unwrap();
    writeln!(out, "  install, so AC-01 is `covered`.").unwrap();
    writeln!(
        out,
        "- **Count reconciliation.** The T12.3c run recorded executed 8 / covered"
    )
    .unwrap();
    writeln!(
        out,
        "  29 / partial 16 / deferred 7, and the table below keeps that data."
    )
    .unwrap();
    writeln!(
        out,
        "  T-G38/T-G54 moved AC-44 to `covered`, so the current #52 dispositions"
    )
    .unwrap();
    writeln!(
        out,
        "  are executed 8 / covered 30 / partial 16 / deferred 6; the 29/7 above"
    )
    .unwrap();
    writeln!(out, "  is the T12.3c snapshot, not a live gap.").unwrap();
    writeln!(
        out,
        "- A `covered`/`partial` row names a file and test function that this runner"
    )
    .unwrap();
    writeln!(
        out,
        "  asserts exists; an `executed` row names an arm this binary really runs."
    )
    .unwrap();
    writeln!(
        out,
        "- A cross-backend row records its SQLite half only; every PostgreSQL half"
    )
    .unwrap();
    writeln!(
        out,
        "  and the PostgreSQL-only rows are deferred to T12.3b by name."
    )
    .unwrap();
    writeln!(
        out,
        "- **T-G61 capacity follow-up.** A separate opt-in test,"
    )
    .unwrap();
    writeln!(
        out,
        "  `sqlite_compact_capacity.rs#capacity_measure` (ignored by default), measured a"
    )
    .unwrap();
    writeln!(
        out,
        "  real **9.987 GB** Store. On the superseded whole-Store `VACUUM INTO` build:"
    )
    .unwrap();
    writeln!(
        out,
        "  `none` at 8.88% dead (three runs, median 149.318 s) and `rebuild` at 10.00%"
    )
    .unwrap();
    writeln!(
        out,
        "  dead (three runs, median 246.371 s; mark 149.089 s, build 94.317 s, verify"
    )
    .unwrap();
    writeln!(
        out,
        "  0.312 s, install 1.916 s; dead 1.011 GB, shrink 1.017 GB; the T-G72 packed"
    )
    .unwrap();
    writeln!(
        out,
        "  peak bound 29.481 GB, the old `new + visited` formula 9.144 GB), plus a"
    )
    .unwrap();
    writeln!(out, "  WAL/FULL control (284.078 s).").unwrap();
    writeln!(
        out,
        "  Re-measured on this branch with the fresh-copy build on the same fixture:"
    )
    .unwrap();
    writeln!(
        out,
        "  `rebuild` median 118.438 s (mark 74.462-75.704 s, build 18.190-19.209 s,"
    )
    .unwrap();
    writeln!(
        out,
        "  quick-check verify 4.330-4.377 s, install 19.197-23.673 s; dead 1.011 GB,"
    )
    .unwrap();
    writeln!(
        out,
        "  shrink 1.016-1.018 GB, peak **9.144 GB** = the fresh Store + visited) and"
    )
    .unwrap();
    writeln!(
        out,
        "  `none` median 88.439 s (82.0-149.9 s; this Sandbox shares the host with other"
    )
    .unwrap();
    writeln!(
        out,
        "  runs). The fresh build phase is about 5x shorter (18-19 s against 92-104 s);"
    )
    .unwrap();
    writeln!(
        out,
        "  the verify adds a ~4 s `quick_check` page scan and the install pays a"
    )
    .unwrap();
    writeln!(
        out,
        "  ~19 s `sync_all` for the freshly written 9.1 GB, both inside the 118 s total."
    )
    .unwrap();
    writeln!(
        out,
        "  The numbers are in `tests/fixtures/sqlite_compact_capacity_measure.json`."
    )
    .unwrap();
    writeln!(
        out,
        "  T-G66 measured the 50 GB arm once per path on the old build (see the runbook"
    )
    .unwrap();
    writeln!(out, "  disk-headroom section); the 30% arm remains [E].").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "## Matrix (SQLite half)").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "| Case | Backend | Matrix tier | Disposition | Evidence | Note |"
    )
    .unwrap();
    writeln!(out, "|---|---|---|---|---|---|").unwrap();
    for row in &MATRIX {
        let evidence = if row.evidence.is_empty() {
            "-".to_owned()
        } else {
            row.evidence
                .iter()
                .map(|entry| evidence_label(entry))
                .collect::<Vec<_>>()
                .join("; ")
        };
        writeln!(
            out,
            "| {} | {} | {} | {} | {} | {} |",
            row.id,
            backend_label(row.backend),
            row.tier,
            status_label(row.status),
            evidence,
            row.note
        )
        .unwrap();
    }
    writeln!(out).unwrap();
    writeln!(out, "## Executed in this runner").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "- AC-33 / AC-59: `arm_checkpoint_busy` - a live reader makes"
    )
    .unwrap();
    writeln!(
        out,
        "  `wal_checkpoint(TRUNCATE)` report busy, the compaction refuses `Busy`,"
    )
    .unwrap();
    writeln!(
        out,
        "  leaves no `.compact`, `.compact-visited` or `.compact-op`, and the WAL is"
    )
    .unwrap();
    writeln!(
        out,
        "  never unlinked; the next run rebuilds at `G -> G+1`."
    )
    .unwrap();
    writeln!(
        out,
        "- AC-33 / AC-46: `arm_sidecar_at_install` - a `-mj*` super-journal placed"
    )
    .unwrap();
    writeln!(
        out,
        "  at install is `Conflict(\"Store sidecar at install\")` with no artifact,"
    )
    .unwrap();
    writeln!(out, "  no op and an unchanged generation.").unwrap();
    writeln!(
        out,
        "- AC-51: `arm_status_phases` - stop points at mark, build, verify and install"
    )
    .unwrap();
    writeln!(
        out,
        "  record the same phase in `<store>.compact-op`; every phase is observed."
    )
    .unwrap();
    writeln!(
        out,
        "- AC-55: `arm_error_codes` - exact `Exists(\"unrecorded compaction artifact\")` and"
    )
    .unwrap();
    writeln!(
        out,
        "  `Capacity(\"disk space\")` texts, with no artifact after the capacity abort."
    )
    .unwrap();
    writeln!(
        out,
        "- AC-59: `arm_seal` - a successful seal leaves no sidecar, reports the main-file"
    )
    .unwrap();
    writeln!(out, "  bytes and a SQLite version, and the file reopens.").unwrap();
    writeln!(
        out,
        "- AC-04 / AC-05: `arm_random_store_shapes` - a seeded property arm over six"
    )
    .unwrap();
    writeln!(
        out,
        "  reachability shapes: every live object stays readable, every wholly dead one"
    )
    .unwrap();
    writeln!(
        out,
        "  is collected, the retained-closure digest is unchanged and the doctor is clean."
    )
    .unwrap();
    writeln!(
        out,
        "- AC-06: `arm_digest_row_after_collection` - a `file_digest` row named a"
    )
    .unwrap();
    writeln!(
        out,
        "  root the rebuild collected, so the row goes; recording the same content"
    )
    .unwrap();
    writeln!(out, "  again succeeds and points at the new root.").unwrap();
    writeln!(
        out,
        "- AC-07 / AC-49: `arm_mount_and_compaction_exclude_each_other` - a live mount"
    )
    .unwrap();
    writeln!(
        out,
        "  makes compaction `MountsActive` naming it, and a live compaction makes a"
    )
    .unwrap();
    writeln!(
        out,
        "  mount and a second compaction `BusyOp`; the refused runs leave no op."
    )
    .unwrap();
    writeln!(
        out,
        "- AC-49: `arm_mount_during_compaction_is_busy` - the stopped run's live"
    )
    .unwrap();
    writeln!(
        out,
        "  compaction intent refuses a mount with `BusyOp`, and the next run adopts"
    )
    .unwrap();
    writeln!(out, "  the leftover and rebuilds.").unwrap();
    writeln!(
        out,
        "- AC-49: `arm_commit_during_rebuild` - a Commit started inside the install"
    )
    .unwrap();
    writeln!(
        out,
        "  fence is `StaleStore` or lands on `G + 1`; a retry is never lost."
    )
    .unwrap();
    writeln!(
        out,
        "- AC-57: `arm_measurement` - three runs of the same rebuild with median and"
    )
    .unwrap();
    writeln!(
        out,
        "  range in `engine/crates/layerfs-persistence/tests/fixtures/"
    )
    .unwrap();
    writeln!(
        out,
        "  sqlite_compact_measure.json` (refresh with `UPDATE_MEASURE=1`). The separate"
    )
    .unwrap();
    writeln!(
        out,
        "  T-G61 `sqlite_compact_capacity.rs#capacity_measure` measures a real 9.987 GB"
    )
    .unwrap();
    writeln!(
        out,
        "  Store (three `none` + three `rebuild` runs; `CAPACITY_DURABLE=1` adds a"
    )
    .unwrap();
    writeln!(out, "  WAL/FULL control) into").unwrap();
    writeln!(
        out,
        "  `sqlite_compact_capacity_measure.json`; it is opt-in because the fixture is"
    )
    .unwrap();
    writeln!(
        out,
        "  ~10 GB and the ignored test is not part of a normal suite."
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(out, "## Deferred to T12.3b (PostgreSQL)").unwrap();
    writeln!(out).unwrap();
    for row in MATRIX.iter().filter(|row| row.status == Status::Deferred) {
        writeln!(out, "- {} - {}", row.id, row.note).unwrap();
    }
    writeln!(
        out,
        "- The PostgreSQL half of every `both` row (digest agreement, artifact"
    )
    .unwrap();
    writeln!(
        out,
        "  names, Phase M/B/C, standby, lost reply, 50 GB arms)."
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(out, "## SQLite gaps (not claimed by this runner)").unwrap();
    writeln!(out).unwrap();
    if MATRIX.iter().all(|row| row.status != Status::Gap) {
        writeln!(
            out,
            "- none; every SQLite gap this runner can execute is executed or covered."
        )
        .unwrap();
    } else {
        for row in MATRIX.iter().filter(|row| row.status == Status::Gap) {
            writeln!(out, "- {} - {}", row.id, row.note).unwrap();
        }
    }
    writeln!(out).unwrap();
    writeln!(out, "## E3 arms").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "- AC-04: the T12.1 closure fixture (deterministic) plus the seeded"
    )
    .unwrap();
    writeln!(
        out,
        "  reachability property arm; the full random history generator is deferred."
    )
    .unwrap();
    writeln!(
        out,
        "- AC-22: a deterministic barrier starts a merge and a `remove_file` together"
    )
    .unwrap();
    writeln!(
        out,
        "  for three rounds; the loser retries and a derived tree never shows the path."
    )
    .unwrap();
    writeln!(
        out,
        "- AC-56: the SQLite literal vector is pinned by the T12.1 golden; the shared"
    )
    .unwrap();
    writeln!(out, "  PostgreSQL literal is deferred.").unwrap();
    writeln!(
        out,
        "- AC-57: three-run measurement arm executed on this Sandbox (small fixture),"
    )
    .unwrap();
    writeln!(
        out,
        "  plus the real 9.987 GB capacity fixture with three runs per arm"
    )
    .unwrap();
    writeln!(
        out,
        "  (`none` 88.439 s median, fresh `rebuild` 118.438 s median; the superseded"
    )
    .unwrap();
    writeln!(
        out,
        "  whole-Store build recorded 149.318 s / 246.371 s median on the same fixture)."
    )
    .unwrap();
    writeln!(
        out,
        "  Its 50 GB projection, the 30% arm, PG `ACCESS EXCLUSIVE` duration and the"
    )
    .unwrap();
    writeln!(out, "  plain `VACUUM INTO` comparison are deferred/[E].").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "## Status and report projection").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "T10 (merged from main after this branch forked) projects `<store>.compact-op`"
    )
    .unwrap();
    writeln!(
        out,
        "through `store.status`: `op {{id, name, phase, started, uncertain}}`, the"
    )
    .unwrap();
    writeln!(
        out,
        "busy/unresolved `state` and the backend `phases`. The named control tests"
    )
    .unwrap();
    writeln!(
        out,
        "in AC-51 and AC-52 assert that projection and the snake_case section 11"
    )
    .unwrap();
    writeln!(
        out,
        "render; the runner still asserts the recorded SQLite phases directly, so"
    )
    .unwrap();
    writeln!(out, "AC-51 does not depend on the projection alone.").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "## Run environment").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "- T-G61 capacity run (separate opt-in test): Docker Sandbox `g61sbx`"
    )
    .unwrap();
    writeln!(
        out,
        "  (Linux x86_64, 88 vCPU, 135 GB RAM, ext4 overlay, 4.0 TB free), rustc"
    )
    .unwrap();
    writeln!(
        out,
        "  1.85.1, `SqlitePersistenceProfile::Disposable` (WAL/OFF) plus one"
    )
    .unwrap();
    writeln!(
        out,
        "  `Durable` control, `CARGO_TARGET_DIR=/var/tmp/target-g61`, fixture under"
    )
    .unwrap();
    writeln!(
        out,
        "  `/var/tmp/g61big`; the test ran 1678 s. Caches: the ~10 GB Store fits in the"
    )
    .unwrap();
    writeln!(
        out,
        "  135 GB page cache and no cache drop ran between the three runs of an arm;"
    )
    .unwrap();
    writeln!(out, "  the mark is CPU-bound at ~62 MB/s, not cache-bound.").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "- Fresh-copy re-measurement (this branch): Docker Sandbox `fxfresh`"
    )
    .unwrap();
    writeln!(
        out,
        "  (Linux x86_64, 88 vCPU, 135 GB RAM, ext4 `/dev/sda2` run directory),"
    )
    .unwrap();
    writeln!(
        out,
        "  rustc 1.85.1, release profile, `SqlitePersistenceProfile::Disposable`"
    )
    .unwrap();
    writeln!(
        out,
        "  (WAL/OFF), `CAPACITY_MEASURE=1 UPDATE_CAPACITY=1`, TMPDIR on the ext4 run"
    )
    .unwrap();
    writeln!(
        out,
        "  directory; the test ran 870 s and reused the same 9.987 GB fixture shape."
    )
    .unwrap();
    writeln!(
        out,
        "  The Sandbox shares its host with other runs: the three `none` marks span"
    )
    .unwrap();
    writeln!(
        out,
        "  82.0-149.9 s while the three `rebuild` marks stay 74.5-75.7 s."
    )
    .unwrap();
    writeln!(
        out,
        "- Docker Sandbox `t123csbx` (Linux x86_64, tmpfs `/tmp`, overlayfs"
    )
    .unwrap();
    writeln!(
        out,
        "  `/var/tmp`), rustc 1.85.1, `SqlitePersistenceProfile::Disposable`"
    )
    .unwrap();
    writeln!(out, "  (WAL/OFF), `CARGO_TARGET_DIR=/tmp/target-t123c`.").unwrap();
    writeln!(
        out,
        "- Verified here: `sqlite_compact_matrix` (1 test); the persistence suites"
    )
    .unwrap();
    writeln!(out, "  `sqlite_compact` (20), `sqlite_compact_crash` (13),").unwrap();
    writeln!(
        out,
        "  `sqlite_compact_fixtures` (1), `admission` (5), `path_removal` (3),"
    )
    .unwrap();
    writeln!(
        out,
        "  `branch_removal` (3), `project_delete` (5), `epoch_fence` (7),"
    )
    .unwrap();
    writeln!(
        out,
        "  `gc_backfill` (4), `kept_objects` (5); the Store-service path suites"
    )
    .unwrap();
    writeln!(
        out,
        "  `sdk_path_merge_removal` (5), `sdk_mount_bind_failure` (1),"
    )
    .unwrap();
    writeln!(
        out,
        "  `sdk_path_whiteouts` (7), `sdk_path_hidden_semantics` (2),"
    )
    .unwrap();
    writeln!(
        out,
        "  `sdk_path_removal` (2); and `layerfs-control --features postgresql"
    )
    .unwrap();
    writeln!(
        out,
        "  project_delete_worker` (5), `layerfs-store --features postgresql"
    )
    .unwrap();
    writeln!(
        out,
        "  project_delete` (15) and `compact_report_status` (3)."
    )
    .unwrap();
    writeln!(out, "  All of those pass on the merged branch.").unwrap();
    writeln!(
        out,
        "- The control `restore_install` arm that opens a Store needs ext4/xfs/btrfs;"
    )
    .unwrap();
    writeln!(
        out,
        "  this Sandbox has no such direct filesystem, so that one case reports"
    )
    .unwrap();
    writeln!(
        out,
        "  `Unsupported` while its other two cases pass. It is an environment limit,"
    )
    .unwrap();
    writeln!(
        out,
        "  not a matrix result; the cited API tests are validated by source name only"
    )
    .unwrap();
    writeln!(out, "  in this run.").unwrap();
    out
}

fn validate_registry(executed: &BTreeSet<String>) {
    assert_eq!(MATRIX.len(), 60, "AC-01..AC-60 are 60 rows");
    for (index, row) in MATRIX.iter().enumerate() {
        let expected = format!("AC-{:02}", index + 1);
        assert_eq!(row.id, expected, "rows are in id order");
        if row.status == Status::Deferred {
            assert!(
                row.backend == Backend::Pg || row.note.contains("PostgreSQL"),
                "{} is deferred but not PostgreSQL-only",
                row.id
            );
            continue;
        }
        if row.status == Status::Gap {
            assert!(
                row.evidence.is_empty(),
                "{} is a gap but names evidence",
                row.id
            );
            continue;
        }
        assert!(!row.evidence.is_empty(), "{} has no evidence", row.id);
        for entry in row.evidence {
            if let Some(arm) = entry.strip_prefix("runner#") {
                assert!(
                    executed.contains(arm),
                    "{} names unexecuted arm {arm}",
                    row.id
                );
                continue;
            }
            let (file, function) = entry.split_once('#').unwrap_or((entry, ""));
            let path = repo_root().join(file);
            assert!(path.exists(), "{}: evidence file missing: {file}", row.id);
            if !function.is_empty() {
                let text = std::fs::read_to_string(&path).unwrap();
                assert!(
                    text.contains(function),
                    "{}: {file} does not name {function}",
                    row.id
                );
            }
        }
        if row.status == Status::Executed {
            assert!(
                row.evidence
                    .iter()
                    .any(|entry| entry.starts_with("runner#")),
                "{} is executed but names no arm",
                row.id
            );
        }
    }
}

#[test]
fn the_sqlite_half_of_the_acceptance_matrix_is_executed_and_reported() {
    let mut executed = BTreeSet::new();
    for (name, arm) in ARMS {
        arm();
        executed.insert(name.to_owned());
    }
    validate_registry(&executed);

    let report = generate_report();
    let path = report_path();
    if std::env::var_os("UPDATE_REPORT").is_some() {
        std::fs::write(&path, report.as_bytes()).unwrap();
    } else {
        let committed = std::fs::read_to_string(&path).unwrap_or_else(|_| {
            panic!(
                "missing {}; run UPDATE_REPORT=1 cargo test -p layerfs-persistence --test sqlite_compact_matrix",
                path.display()
            )
        });
        assert_eq!(
            committed, report,
            "the report is stale; rerun with UPDATE_REPORT=1"
        );
    }

    let count = |status: Status| MATRIX.iter().filter(|row| row.status == status).count();
    println!(
        "T12.3a SQLite matrix: {} rows | executed {} | covered {} | partial {} | deferred {} | gap {}",
        MATRIX.len(),
        count(Status::Executed),
        count(Status::Covered),
        count(Status::Partial),
        count(Status::Deferred),
        count(Status::Gap)
    );
}
