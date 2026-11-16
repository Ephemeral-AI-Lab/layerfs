//! #41 T8: the §7 SQLite rebuild. A compaction takes the writer lane, runs the
//! mark, chooses `none`/`rebuild` from the wholly dead packs, builds and
//! verifies `<store>.compact`, installs it at `G → G+1`, and leaves no
//! artifact; a recorded leftover is judged by the generation and removed by
//! the next `store.compact`.
#![cfg(any(target_os = "macos", target_os = "linux"))]
mod support;

use layerfs_content::{encode_whole_file_payload, FinalizedObject, ObjectId, ObjectRole};
use layerfs_history::{
    AddLayerOutcome, AddLayerRequest, BranchId, CommitId, CommitMeta, CommitOutcome, CommitRequest,
    CommitVia, ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig, HistoryError,
    LayerStackId, LayerStackRecord, StackInitialization,
};
use layerfs_persistence::{
    compact_op_status, CompactFailpoint, CompactPath, CompactReport, CompactReportSource,
    CompactRequest, PersistenceConfig, SqliteAcquisitionSchema, SqliteHandles, SqlitePackLayout,
    SqlitePersistenceProfile,
};
use layerfs_storage::{port::PackPersistence, Storage, StoragePolicy};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use support::Temp;

const BINDING: &[u8] = b"sqlite-compact-test-binding";

fn create_store() -> (Temp, SqliteHandles, PathBuf) {
    let temp = Temp::new("sqlite-compact");
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

#[cfg(target_os = "linux")]
fn beside(store: &Path, suffix: &str) -> PathBuf {
    let mut name = store.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn request(op: u8) -> CompactRequest {
    CompactRequest {
        op_id: [op; 16],
        ..CompactRequest::default()
    }
}

fn request_at(op: u8, failpoint: CompactFailpoint) -> CompactRequest {
    CompactRequest {
        op_id: [op; 16],
        failpoint: Some(failpoint),
        ..CompactRequest::default()
    }
}

fn compact_path(store: &Path) -> PathBuf {
    let mut name = store.as_os_str().to_os_string();
    name.push(".compact");
    PathBuf::from(name)
}

fn op_path(store: &Path) -> PathBuf {
    let mut name = store.as_os_str().to_os_string();
    name.push(".compact-op");
    PathBuf::from(name)
}

fn visited_path(store: &Path) -> PathBuf {
    let mut name = store.as_os_str().to_os_string();
    name.push(".compact-visited");
    PathBuf::from(name)
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

#[test]
fn an_empty_store_takes_the_none_path() {
    let (_temp, handles, path) = create_store();
    let before = generation(&handles);
    let receipt = handles.compact(&request(1)).unwrap();
    assert_eq!(receipt.path, CompactPath::None);
    assert_eq!(receipt.generation_before, before);
    assert_eq!(receipt.generation_after, before);
    assert_eq!(receipt.dead_bytes_bound, 0);
    assert!(receipt
        .zero_reasons
        .iter()
        .any(|zero| zero.field == "deadBytesBound" && zero.reason == "empty-store"));
    assert!(!compact_path(&path).exists());
    assert!(!visited_path(&path).exists());
    assert!(!op_path(&path).exists());
    let reopened = reopen(&path);
    assert_eq!(generation(&reopened), before);
    reopened.history.mark().unwrap();
}

const FOREIGN_STORE: &str = "LAYERFS_COMPACTION_FOREIGN_STORE";
const FOREIGN_READY: &str = "LAYERFS_COMPACTION_FOREIGN_READY";

/// Child half of `another_client_holding_the_store_is_busy_at_admission`:
/// opens the Store, announces readiness and holds it until killed.
#[test]
fn foreign_client_child() {
    let Some(store) = std::env::var_os(FOREIGN_STORE) else {
        return;
    };
    let ready = std::env::var_os(FOREIGN_READY).expect("the ready path");
    let connection = rusqlite::Connection::open(Path::new(&store)).expect("the child's connection");
    let _: i64 = connection
        .query_row("SELECT count(*) FROM pack", [], |row| row.get(0))
        .expect("a read through the held Store");
    std::fs::write(&ready, b"ready").expect("announce readiness");
    std::thread::sleep(std::time::Duration::from_secs(60));
    drop(connection);
}

/// Kills and reaps the foreign client on every exit, so a failed assertion
/// cannot leave it holding the Store for the rest of the test run.
#[cfg(target_os = "linux")]
struct Foreign(std::process::Child);

#[cfg(target_os = "linux")]
impl Drop for Foreign {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// §5.1/§7 admission: another process holds the Store open, so its `-shm`
/// remains and the seal would refuse only after the whole mark. The run
/// refuses `Busy` at admission instead, leaving no intent, op record or
/// scratch. Once that client is gone (here a killed client, which also leaves
/// the sidecars a crash leaves), the next run rebuilds.
#[cfg(target_os = "linux")]
#[test]
fn another_client_holding_the_store_is_busy_at_admission() {
    let (temp, handles, path) = create_store();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let _dead = whole_file(&handles, b"a pack the rebuild would collect");
    let before = generation(&handles);
    // `store.compact` closes this process's cached handle first; the Store is
    // then cleanly closed and the foreign client is what holds it open.
    handles.seal().expect("close the Store");
    let ready = temp.join("foreign.ready");
    let foreign = std::process::Command::new(std::env::current_exe().expect("test binary"))
        .args(["--exact", "foreign_client_child", "--nocapture"])
        .env(FOREIGN_STORE, &path)
        .env(FOREIGN_READY, &ready)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("the foreign client");
    let foreign = Foreign(foreign);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !ready.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "the foreign client never opened the Store"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        beside(&path, "-shm").exists(),
        "the open client keeps the Store sidecar"
    );

    let refused = reopen(&path);
    let error = refused.compact(&request(46)).unwrap_err();
    assert!(matches!(error, HistoryError::Busy), "{error:?}");
    assert!(
        !compact_path(&path).exists(),
        "no staged Store on admission Busy"
    );
    assert!(
        !visited_path(&path).exists(),
        "no mark scratch on admission Busy"
    );
    assert!(!op_path(&path).exists(), "no op record on admission Busy");

    // The client is killed, leaving the sidecars a crash leaves. The next run
    // is not refused by them: no process holds those descriptions, so the
    // seal checkpoints, closes and removes them.
    drop(foreign);
    let reopened = reopen(&path);
    assert_eq!(generation(&reopened), before, "the Store is unchanged");
    let receipt = reopened.compact(&request(47)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    let final_handles = reopen(&path);
    assert!(located(&final_handles, live), "the live root survives");
    clean(&final_handles);
}

#[test]
fn a_store_without_wholly_dead_packs_takes_the_none_path() {
    let (_temp, handles, path) = create_store();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let before = generation(&handles);
    let receipt = handles.compact(&request(2)).unwrap();
    assert_eq!(receipt.path, CompactPath::None);
    assert_eq!(receipt.dead_bytes_bound, 0);
    assert_eq!(
        receipt.dead_bytes_refined, receipt.dead_bytes_bound,
        "the none path does not refine"
    );
    assert_eq!(receipt.generation_after, before);
    assert_eq!(receipt.physical_shrink, 0);
    assert!(!compact_path(&path).exists());
    assert!(!op_path(&path).exists());
}

#[test]
fn a_wholly_dead_pack_is_rebuilt_away() {
    let (_temp, handles, path) = create_store();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let dead = whole_file(&handles, b"nobody reaches this object");
    let before = generation(&handles);
    let receipt = handles.compact(&request(3)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(receipt.generation_before, before);
    assert_eq!(receipt.generation_after, before + 1);
    assert!(!compact_path(&path).exists());
    assert!(!visited_path(&path).exists());
    assert!(!op_path(&path).exists());
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        assert!(!PathBuf::from(name).exists(), "sidecar {suffix} remains");
    }

    let reopened = reopen(&path);
    assert_eq!(generation(&reopened), before + 1);
    assert!(located(&reopened, live), "the retained object survives");
    assert!(!located(&reopened, dead), "the dead object is collected");
    clean(&reopened);
}

#[test]
fn retained_history_and_its_objects_survive_a_rebuild() {
    let (_temp, handles, path) = create_store();
    let genesis = filesystem_root(&handles, 1);
    let committed = filesystem_root(&handles, 2);
    let stack = initialize(&handles, 1, genesis);
    let work = support::branch_id(1);
    fork(&handles, stack.id, work, 1);
    let commit_id = commit(&handles, work, committed);
    publish(&handles, stack.id, work);
    let dead = whole_file(&handles, b"a pack no retained root reaches");
    let before = generation(&handles);
    let receipt = handles.compact(&request(4)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    let reopened = reopen(&path);
    assert_eq!(generation(&reopened), before + 1);
    assert!(reopened.history.commit(commit_id).unwrap().is_some());
    assert!(reopened.history.layer_stack(stack.id).unwrap().is_some());
    for id in [genesis, committed] {
        assert!(located(&reopened, id), "object {id} survives");
    }
    assert!(!located(&reopened, dead));
    clean(&reopened);
}

#[test]
fn a_stray_staged_name_is_refused_and_not_deleted() {
    let (_temp, handles, path) = create_store();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let _ = whole_file(&handles, b"dead");
    let stray = compact_path(&path);
    std::fs::write(&stray, b"not a Store").unwrap();
    let error = handles.compact(&request(5)).unwrap_err();
    assert!(
        matches!(error, layerfs_history::HistoryError::Exists(_)),
        "{error:?}"
    );
    assert!(stray.exists(), "a name no record names is not deleted");
}

#[test]
fn a_build_failpoint_leaves_a_restartable_artifact() {
    let (_temp, handles, path) = create_store();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let dead = whole_file(&handles, b"dead");
    let before = generation(&handles);
    let error = handles
        .compact(&request_at(6, CompactFailpoint::AfterBuild))
        .unwrap_err();
    assert!(
        matches!(error, layerfs_history::HistoryError::UnknownOutcome),
        "{error:?}"
    );
    assert!(compact_path(&path).exists(), "the staged Store remains");
    assert!(visited_path(&path).exists(), "the visited scratch remains");
    assert!(op_path(&path).exists(), "the op record remains");

    let reopened = reopen(&path);
    assert_eq!(
        generation(&reopened),
        before,
        "the old Store is still the live one"
    );
    let receipt = reopened.compact(&request(7)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(receipt.generation_after, before + 1);
    assert!(!compact_path(&path).exists());
    assert!(!visited_path(&path).exists());
    assert!(!op_path(&path).exists());
    let final_handles = reopen(&path);
    assert!(!located(&final_handles, dead));
    clean(&final_handles);
}

#[test]
fn an_installed_but_unsettled_run_is_judged_at_g_plus_one() {
    let (_temp, handles, path) = create_store();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let _ = whole_file(&handles, b"dead");
    let before = generation(&handles);
    let error = handles
        .compact(&request_at(8, CompactFailpoint::AfterInstall))
        .unwrap_err();
    assert!(
        matches!(error, layerfs_history::HistoryError::UnknownOutcome),
        "{error:?}"
    );
    assert!(op_path(&path).exists());
    let reopened = reopen(&path);
    assert_eq!(
        generation(&reopened),
        before + 1,
        "the new Store is already installed"
    );
    let receipt = reopened.compact(&request(9)).unwrap();
    assert_eq!(receipt.generation_before, before + 1);
    assert!(!op_path(&path).exists());
    assert!(!compact_path(&path).exists());
}

/// The staged artifact name the tests below make non-removable: a directory,
/// which `remove_file` refuses, so `clear_artifacts` reports it as residue.
fn stuck_artifact(store: &Path) -> PathBuf {
    let mut name = store.as_os_str().to_os_string();
    name.push(".compact-visited-journal");
    PathBuf::from(name)
}

/// The test-only `CompactRequest::before_install_hook` runs after the admission
/// leftover check and before the install fence: it stages the stuck artifact so
/// the settle meets a non-removable residue (§9).
fn stage_stuck_artifact(store: &Path) {
    std::fs::create_dir(stuck_artifact(store)).expect("stage the stuck artifact");
}

/// §9/§11: a settle that leaves a residue keeps the op record, so the next run
/// adopts the recorded leftover instead of refusing an unrecorded staged name
/// as `Exists`. While the artifact cannot be removed the recorded leftover is
/// refused and the Store is untouched; once it is gone the next run settles it
/// and leaves no residue.
#[test]
fn a_settle_residue_keeps_the_op_record_recoverable() {
    let (_temp, handles, path) = create_store();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let dead = whole_file(&handles, b"a pack the rebuild would collect");
    let before = generation(&handles);
    let staged = CompactRequest {
        op_id: [24; 16],
        before_install_hook: Some(stage_stuck_artifact),
        ..CompactRequest::default()
    };

    let receipt = handles.compact(&staged).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    let stuck = stuck_artifact(&path);
    assert_eq!(
        receipt.residue.artifacts,
        vec![stuck
            .file_name()
            .expect("artifact name")
            .to_string_lossy()
            .into_owned()],
        "the settle reports the artifact it cannot remove: {:?}",
        receipt.residue
    );
    assert!(stuck.is_dir(), "the stuck artifact remains");
    assert!(
        compact_op_status(&path).is_some(),
        "a settle residue keeps the op record as the next run's breadcrumb"
    );

    // The recorded leftover is refused while the artifact cannot be removed;
    // the record still names it.
    let reopened = reopen(&path);
    let error = reopened.compact(&request(25)).unwrap_err();
    assert!(
        matches!(error, HistoryError::Integrity(_)),
        "the recorded residue is refused: {error:?}"
    );
    assert!(
        compact_op_status(&path).is_some(),
        "the record survives the refusal"
    );

    // The artifact removable, the recorded leftover settles: the next run
    // removes the record and leaves a clean Store.
    std::fs::remove_dir(&stuck).expect("clear the stuck artifact");
    let reopened = reopen(&path);
    let receipt = reopened.compact(&request(26)).unwrap();
    assert_eq!(receipt.path, CompactPath::None);
    assert_eq!(receipt.generation_before, before + 1);
    assert_eq!(receipt.generation_after, before + 1);
    assert!(
        receipt.residue.artifacts.is_empty(),
        "residue after recovery: {:?}",
        receipt.residue
    );
    assert!(compact_op_status(&path).is_none(), "the record settles");
    assert!(!op_path(&path).exists());
    let final_handles = reopen(&path);
    assert_eq!(generation(&final_handles), before + 1);
    assert!(!located(&final_handles, dead), "the dead pack is collected");
    clean(&final_handles);
}

/// §9: with no op record, a non-removable artifact is refused as `Exists` and
/// nothing is deleted or installed, so the no-record branch does not strand the
/// Store: once the artifact is gone the next run recovers from `G`.
#[test]
fn an_unrecorded_non_removable_artifact_does_not_strand_the_store() {
    let (_temp, handles, path) = create_store();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let dead = whole_file(&handles, b"a pack the rebuild would collect");
    let before = generation(&handles);
    let stuck = stuck_artifact(&path);
    std::fs::create_dir(&stuck).expect("stage the unrecorded artifact");

    let error = handles.compact(&request(27)).unwrap_err();
    assert!(
        matches!(error, HistoryError::Exists(_)),
        "the unrecorded artifact is refused: {error:?}"
    );
    assert!(compact_op_status(&path).is_none(), "no record is written");
    assert!(stuck.is_dir(), "the refusal deletes nothing");
    // The live Store is untouched: same generation, same stored objects.
    let reopened = reopen(&path);
    assert_eq!(generation(&reopened), before);
    assert!(located(&reopened, live), "the live object survives");
    assert!(located(&reopened, dead), "the dead pack is still stored at G");

    // The obstacle gone, the next run rebuilds from `G` and leaves a clean
    // Store: the refusal was recoverable, never a stranded state.
    std::fs::remove_dir(&stuck).expect("clear the unrecorded artifact");
    let receipt = reopened.compact(&request(28)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(receipt.generation_before, before);
    assert_eq!(receipt.generation_after, before + 1);
    assert!(
        receipt.residue.artifacts.is_empty(),
        "residue after recovery: {:?}",
        receipt.residue
    );
    let final_handles = reopen(&path);
    assert!(!located(&final_handles, dead), "the dead pack is collected");
    clean(&final_handles);
}

#[test]
fn the_report_projects_every_section_11_field() {
    let (_temp, handles, _path) = create_store();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let _ = whole_file(&handles, b"dead");
    let before = generation(&handles);
    let receipt = handles.compact(&request(30)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    let report = CompactReport::from(&receipt);
    assert_eq!(report.path, CompactPath::Rebuild);
    assert_eq!(report.generation.before, before);
    assert_eq!(report.generation.after, before + 1);
    assert_eq!(report.dead_bytes_bound, receipt.dead_bytes_bound);
    assert_eq!(report.dead_bytes_refined, report.dead_bytes_bound);
    assert_eq!(report.logical_discarded, receipt.logical_discarded);
    assert_eq!(report.backend_reusable, receipt.backend_reusable);
    assert_eq!(report.physical_shrink, receipt.physical_shrink);
    assert_eq!(report.kept_waste, receipt.kept_waste);
    assert_eq!(report.peak_temp_bytes, receipt.peak_temp_bytes);
    assert_eq!(report.residue, receipt.residue);
    assert!(report.estimate);
    assert_eq!(report.zero_reasons, receipt.zero_reasons);
    assert!(
        report
            .zero_reasons
            .iter()
            .all(|zero| zero.wire_field() != "dead_bytes_bound"),
        "a rebuild with dead bytes does not explain a zero"
    );
    // `From` and the shared source trait project the same report.
    assert_eq!(report, receipt.compact_report());
}

#[test]
fn the_empty_store_report_explains_its_zeros() {
    let (_temp, handles, path) = create_store();
    let receipt = handles.compact(&request(31)).unwrap();
    let report = CompactReport::from(&receipt);
    assert_eq!(report.path, CompactPath::None);
    assert_eq!(report.generation.before, report.generation.after);
    assert_eq!(report.dead_bytes_refined, report.dead_bytes_bound);
    assert!(report.estimate);
    assert!(
        report.zero_reasons.iter().any(|zero| {
            zero.field == "deadBytesBound"
                && zero.wire_field() == "dead_bytes_bound"
                && zero.reason == "empty-store"
        }),
        "the empty store names its zero: {:?}",
        report.zero_reasons
    );
    assert!(report.residue.artifacts.is_empty());
    assert!(compact_op_status(&path).is_none(), "none settles its op");
}

#[test]
fn a_leftover_op_reports_its_phase_and_state() {
    let (_temp, handles, path) = create_store();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let _ = whole_file(&handles, b"dead");
    let before = generation(&handles);
    let error = handles
        .compact(&CompactRequest {
            op_id: [21; 16],
            owner_pid: Some(std::process::id()),
            failpoint: Some(CompactFailpoint::AfterBuild),
            ..CompactRequest::default()
        })
        .unwrap_err();
    assert!(
        matches!(error, layerfs_history::HistoryError::UnknownOutcome),
        "{error:?}"
    );
    let op = compact_op_status(&path).expect("the leftover op record is readable");
    assert_eq!(op.op_id, [21; 16]);
    assert_eq!(op.generation, i64::try_from(before).unwrap());
    assert_eq!(op.phase, "build");
    assert_eq!(op.owner_pid, Some(std::process::id()));
    assert!(op.started_ms > 0);

    let busy = op.status(true);
    assert_eq!(busy.id, "op-15151515151515151515151515151515");
    assert_eq!(busy.name, "store.compact");
    assert_eq!(busy.phase, "build");
    assert_eq!(busy.started, op.started_ms);
    assert!(busy.uncertain, "an unsettled record is uncertain");
    assert_eq!(busy.state, "busy");
    assert_eq!(
        busy.phases,
        &["mark", "build", "verify", "install", "cleanup"][..],
        "the SQLite phase order, including the cleanup settle"
    );

    let leftover = op.status(false);
    assert!(leftover.uncertain);
    assert_eq!(leftover.state, "unresolved");
    assert_eq!(leftover.phase, "build");

    // The next run settles the recorded leftover: artifact, intent and op go.
    let reopened = reopen(&path);
    let receipt = reopened.compact(&request(22)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert!(
        compact_op_status(&path).is_none(),
        "the op record is settled"
    );
    assert!(receipt.residue.artifacts.is_empty());
    assert_eq!(receipt.residue.bytes, 0);
    assert!(receipt.residue.reason.is_none());
    assert!(!compact_path(&path).exists());
}

#[test]
fn generation_increases_once_per_installed_rebuild() {
    let (_temp, handles, path) = create_store();
    let first = filesystem_root(&handles, 1);
    initialize(&handles, 1, first);
    let _ = whole_file(&handles, b"first dead root");
    let start = generation(&handles);
    assert_eq!(
        handles.compact(&request(10)).unwrap().generation_after,
        start + 1
    );
    let reopened = reopen(&path);
    let _ = whole_file(&reopened, b"second dead root");
    let receipt = reopened.compact(&request(11)).unwrap();
    assert_eq!(receipt.generation_before, start + 1);
    assert_eq!(receipt.generation_after, start + 2);
    let final_handles = reopen(&path);
    assert_eq!(generation(&final_handles), start + 2);
    assert!(located(&final_handles, first));
    clean(&final_handles);
}

// --- Fences, thresholds, grouped and acquisition layouts (T8.1) ---

thread_local! {
    /// The Store file the install-fence test renames over the sealed path.
    static REPLACEMENT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// The test-only hook CompactRequest::before_install_hook runs: replace the
/// Store path with the prepared file after the seal and before the rename.
fn replace_store(path: &Path) {
    let replacement = REPLACEMENT
        .with(|slot| slot.borrow_mut().take())
        .expect("a prepared replacement");
    std::fs::rename(&replacement, path).expect("replace the sealed Store");
}

/// The stored length of every pack, read after the Store is closed.
fn pack_lengths(path: &Path) -> Vec<u64> {
    let connection =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("read-only Store");
    let mut statement = connection
        .prepare("SELECT length FROM pack ORDER BY pack_id")
        .expect("pack rows");
    let rows = statement
        .query_map([], |row| row.get::<_, i64>(0))
        .expect("pack lengths");
    rows.map(|row| u64::try_from(row.expect("a pack length")).expect("nonnegative"))
        .collect()
}

/// Every acquisition working row the rebuild must clear.
fn acquisition_rows(path: &Path) -> i64 {
    let connection =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("read-only Store");
    ["init_operation", "init_entry", "init_host_file"]
        .iter()
        .map(|table| {
            connection
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("acquisition row count")
        })
        .sum()
}

fn create_with(
    layout: SqlitePackLayout,
    acquisition: SqliteAcquisitionSchema,
) -> (Temp, SqliteHandles, PathBuf) {
    let temp = Temp::new("sqlite-compact-layout");
    let path = temp.join("store.sqlite");
    let handles = SqliteHandles::create(
        PersistenceConfig::sqlite(path.clone())
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
            .with_sqlite_pack_layout(layout)
            .with_sqlite_acquisition(acquisition),
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

/// One Store whose live whole-file roots are retained and whose dead
/// whole-file packs no retained root reaches.
///
/// Every object has the same canonical length, so every pack has the same
/// stored length: the wholly dead byte ratio is dead / (live + dead) exactly.
fn ratio_store(live: usize, dead: usize) -> (Temp, SqliteHandles, PathBuf) {
    let (temp, handles, path) = create_store();
    for index in 0..live {
        let root = whole_file(&handles, format!("live-{index:06}").as_bytes());
        initialize(&handles, u8::try_from(index + 1).unwrap(), root);
    }
    for index in 0..dead {
        let _ = whole_file(&handles, format!("dead-{index:06}").as_bytes());
    }
    (temp, handles, path)
}

#[test]
fn nine_percent_wholly_dead_bytes_take_the_none_path() {
    let (_temp, handles, path) = ratio_store(91, 9);
    let before = generation(&handles);
    let receipt = handles.compact(&request(20)).unwrap();
    assert_eq!(receipt.path, CompactPath::None);
    assert_eq!(receipt.generation_after, before);
    assert!(!compact_path(&path).exists());
    assert!(!op_path(&path).exists());
    let lengths = pack_lengths(&path);
    assert_eq!(lengths.len(), 100, "one pack per object: {lengths:?}");
    let unit = lengths[0];
    assert!(lengths.iter().all(|length| *length == unit));
    assert_eq!(receipt.dead_bytes_bound, 9 * unit);
    assert!(
        9 * unit * 10 < 100 * unit,
        "9% wholly dead bytes are below the threshold"
    );
}

#[test]
fn ten_percent_wholly_dead_bytes_rebuild_at_the_threshold() {
    let (_temp, handles, path) = ratio_store(90, 10);
    let before = generation(&handles);
    let receipt = handles.compact(&request(21)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(receipt.generation_before, before);
    assert_eq!(receipt.generation_after, before + 1);
    let lengths = pack_lengths(&path);
    assert_eq!(
        lengths.len(),
        90,
        "ten dead packs are collected: {lengths:?}"
    );
    let unit = lengths[0];
    assert!(lengths.iter().all(|length| *length == unit));
    assert_eq!(receipt.dead_bytes_bound, 10 * unit);
    assert!(
        10 * unit * 10 >= (90 + 10) * unit,
        "exactly 10% wholly dead bytes take the rebuild path"
    );
}

#[test]
fn thirty_percent_wholly_dead_bytes_rebuild_above_the_threshold() {
    let (_temp, handles, path) = ratio_store(70, 30);
    let before = generation(&handles);
    let receipt = handles.compact(&request(22)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(receipt.generation_after, before + 1);
    let lengths = pack_lengths(&path);
    assert_eq!(lengths.len(), 70, "thirty dead packs are collected");
    let unit = lengths[0];
    assert!(lengths.iter().all(|length| *length == unit));
    assert_eq!(receipt.dead_bytes_bound, 30 * unit);
    assert!(30 * unit * 10 > (70 + 30) * unit);
}

#[test]
fn a_store_replaced_between_seal_and_install_is_stale() {
    let (_temp, handles, path) = create_store();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let _ = whole_file(&handles, b"a pack the rebuild would collect");

    // A second valid Store at the same generation: replacing the path with it
    // changes the (dev, ino) pair and leaves size, mtime and counter unrelated.
    let (_replacement_temp, replacement_handles, replacement_path) = create_store();
    let replacement_generation = generation(&replacement_handles);
    replacement_handles.seal().unwrap();
    REPLACEMENT.with(|slot| *slot.borrow_mut() = Some(replacement_path));

    let request = CompactRequest {
        op_id: [23; 16],
        before_install_hook: Some(replace_store),
        ..CompactRequest::default()
    };
    let error = handles.compact(&request).unwrap_err();
    assert!(matches!(error, HistoryError::StaleStore), "{error:?}");

    // No install: every artifact is gone and the replacement is untouched.
    assert!(!compact_path(&path).exists());
    assert!(!visited_path(&path).exists());
    assert!(!op_path(&path).exists());
    let reopened = reopen(&path);
    assert_eq!(generation(&reopened), replacement_generation);
}

#[test]
fn a_store_replaced_under_an_open_handle_is_stale_at_the_next_transaction() {
    let (_temp, handles, path) = create_store();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);

    let (_replacement_temp, replacement_handles, replacement_path) = create_store();
    replacement_handles.seal().unwrap();
    std::fs::rename(&replacement_path, &path).unwrap();

    // The handle still owns the old inode; the fence refuses before anything runs.
    let error = handles.history.mark().unwrap_err();
    assert!(matches!(error, HistoryError::StaleStore), "{error:?}");
}

#[test]
fn a_capacity_failure_leaves_the_store_unchanged() {
    let (_temp, handles, path) = create_store();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let dead = whole_file(&handles, b"a pack the rebuild would collect");
    let before = generation(&handles);
    let error = handles
        .compact(&request_at(24, CompactFailpoint::CapacityAtBuild))
        .unwrap_err();
    assert!(matches!(error, HistoryError::Capacity(_)), "{error:?}");
    assert!(!compact_path(&path).exists());
    assert!(!visited_path(&path).exists());
    assert!(!op_path(&path).exists());
    let reopened = reopen(&path);
    assert_eq!(generation(&reopened), before, "the old Store is unchanged");
    assert!(located(&reopened, dead), "nothing was collected");
    let receipt = reopened.compact(&request(25)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    let final_handles = reopen(&path);
    assert!(!located(&final_handles, dead));
    clean(&final_handles);
}

/// The Store file's `(size, mtime, database change counter)`: the same tuple the
/// install fence seals, so an equal tuple across a call means no write was
/// committed to the file.
fn store_file_tuple(path: &Path) -> (u64, std::time::SystemTime, u32) {
    use std::io::{Read as _, Seek as _, SeekFrom};
    let metadata = std::fs::metadata(path).unwrap();
    let modified = metadata.modified().unwrap();
    let mut file = std::fs::File::open(path).unwrap();
    let mut header = [0u8; 4];
    file.seek(SeekFrom::Start(24)).unwrap();
    file.read_exact(&mut header).unwrap();
    (metadata.len(), modified, u32::from_be_bytes(header))
}

/// The raw table counts a compaction must leave behind; the Store is closed
/// after `compact` consumes its handles, so a plain read is safe.
fn table_count(path: &Path, table: &str) -> i64 {
    let connection = rusqlite::Connection::open(path).unwrap();
    connection
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}

/// Writes an op record the way `RunOp` does, with `generation` deliberately
/// not the Store's.
fn write_op_record(path: &Path, generation: u64) {
    let zeros = "00".repeat(32);
    let text = format!(
        "version=1\nop_id=00112233445566778899aabbccddeeff\ngeneration={generation}\n\
phase=mark\nstarted_ms=1\nowner_pid=\nowner_pid_start=\nowner_pid_boot=\n\
catalog_id=0102030405060708090a0b0c0d0e0f10\nnext_history_serial=1\nnext_pack_id=1\n\
next_ordinal=1\nmark_serial=0\nmark_pack_id=1\nmark_digest={zeros}\nmark_totals=0,0,0,0\n\
mark_stamp=0\nhistory_rows=0\nlive_heads_digest={zeros}\n"
    );
    std::fs::write(op_path(path), text).unwrap();
}

/// #711 finding 1: a `none` compaction writes nothing into the live Store's
/// persistent state. The §6.1 closure and the §5.1 admission intent are one
/// write transaction, and the `none` decision rolls it back: no `kept_node`
/// row, no `gc_intent` row and no WAL frame is committed, so the Store file's
/// `(size, mtime, change counter)` is unchanged. The §6.3 visited scratch is
/// created, counted in `peakTempBytes` and deleted before the reply.
#[test]
fn a_none_path_leaves_no_mark_state() {
    let (_temp, handles, path) = create_store();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    // Close the seeding session first: its close checkpoints the WAL, so the
    // tuple below is the quiesced file the `none` call must leave untouched.
    drop(handles);
    let handles = reopen(&path);
    let before = store_file_tuple(&path);
    assert_eq!(table_count(&path, "kept_node"), 0);
    let receipt = handles.compact(&request(0x75)).unwrap();
    assert_eq!(receipt.path, CompactPath::None);
    assert!(
        receipt.peak_temp_bytes > 0,
        "the none path's visited scratch counts in peakTempBytes"
    );
    assert!(
        !visited_path(&path).exists(),
        "the none path deletes its visited scratch"
    );
    assert!(!op_path(&path).exists());
    assert_eq!(
        table_count(&path, "kept_node"),
        0,
        "the rolled-back mark leaves no mark state in the live Store"
    );
    assert_eq!(
        table_count(&path, "gc_intent"),
        0,
        "the rolled-back mark leaves no admission intent"
    );
    assert_eq!(
        store_file_tuple(&path),
        before,
        "the none path commits no write: the Store file is unchanged"
    );
    clean(&reopen(&path));
}

/// #711 finding 4: the incomplete op record is an artifact of the recorded
/// set, not an unrecorded staged name: the next run removes it and proceeds.
#[test]
fn an_incomplete_op_record_is_removed_rather_than_refused() {
    let (_temp, handles, path) = create_store();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let mut temporary = op_path(&path).as_os_str().to_os_string();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    std::fs::write(&temporary, b"version=1\n").unwrap();
    let receipt = handles.compact(&request(0x76)).unwrap();
    assert_eq!(receipt.path, CompactPath::None);
    assert!(!temporary.exists(), "the incomplete record is removed");
    clean(&reopen(&path));
}

/// #711 finding 5: §9 judges a leftover by the generation it names. A record at
/// neither `G` nor `G-1` does not describe this Store, so it is refused and
/// nothing it names is deleted.
#[test]
fn a_leftover_record_for_another_generation_is_refused() {
    let (_temp, handles, path) = create_store();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let before = generation(&handles);
    let stray = compact_path(&path);
    std::fs::write(&stray, b"a staged file no generation names").unwrap();
    write_op_record(&path, before + 5);
    let error = handles.compact(&request(0x77)).unwrap_err();
    assert!(
        matches!(error, HistoryError::Integrity(_)),
        "a record for another generation is Integrity: {error:?}"
    );
    assert!(
        stray.exists(),
        "a record at another generation deletes nothing it names"
    );
    assert!(
        op_path(&path).exists(),
        "the record stays for its own Store"
    );
}

fn layout_round_trip(layout: SqlitePackLayout) {
    let (_temp, handles, path) = create_with(layout, SqliteAcquisitionSchema::Absent);
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);

    // The none path: the stored layout opens and a mark-only run leaves it.
    let receipt = handles.compact(&request(26)).unwrap();
    assert_eq!(receipt.path, CompactPath::None);
    let reopened = reopen(&path);
    assert_eq!(reopened.profile().pack_layout, layout);

    // A rebuild collects a wholly dead pack and preserves the layout.
    let dead = whole_file(&reopened, b"a wholly dead pack in a grouped layout");
    let before = generation(&reopened);
    let receipt = reopened.compact(&request(27)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(receipt.generation_after, before + 1);
    let final_handles = reopen(&path);
    assert_eq!(final_handles.profile().pack_layout, layout);
    assert!(located(&final_handles, live));
    assert!(!located(&final_handles, dead));
    clean(&final_handles);
}

#[test]
fn group_rows_layout_opens_compacts_and_takes_none() {
    layout_round_trip(SqlitePackLayout::GroupRows);
}

#[test]
fn group_rows_indexed_layout_opens_compacts_and_takes_none() {
    layout_round_trip(SqlitePackLayout::GroupRowsIndexed);
}

#[test]
fn acquisition_layout_opens_compacts_and_clears_working_rows() {
    let (_temp, handles, path) =
        create_with(SqlitePackLayout::GroupRows, SqliteAcquisitionSchema::Tables);
    assert_eq!(
        handles.profile().acquisition,
        SqliteAcquisitionSchema::Tables
    );
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    // A live acquisition leaves unfinished working rows; §5 refuses the
    // compaction rather than wipe rows it was not admitted to touch.
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute(
            "INSERT INTO init_operation(owner_epoch,phase,source_device,source_inode,stack_id,scope) \
             VALUES(1,1,zeroblob(8),zeroblob(8),zeroblob(16),zeroblob(32))",
            [],
        )
        .unwrap();
    drop(connection);
    let error = handles.compact(&request(28)).unwrap_err();
    assert!(
        matches!(error, layerfs_history::HistoryError::Busy),
        "an unfinished acquisition must refuse the compaction: {error:?}"
    );

    // Drop the stale working row; the none path then leaves the acquisition
    // tables and takes no rebuild.
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute("DELETE FROM init_operation", [])
        .unwrap();
    drop(connection);
    let none_handles = reopen(&path);
    let receipt = none_handles.compact(&request(28)).unwrap();
    assert_eq!(receipt.path, CompactPath::None);
    let reopened = reopen(&path);
    assert_eq!(
        reopened.profile().acquisition,
        SqliteAcquisitionSchema::Tables
    );

    // A rebuild clears the working rows and keeps the acquisition tables.
    let dead = whole_file(&reopened, b"a wholly dead pack with acquisition tables");
    let before = generation(&reopened);
    let receipt = reopened.compact(&request(29)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(receipt.generation_after, before + 1);
    let final_handles = reopen(&path);
    assert_eq!(
        final_handles.profile().acquisition,
        SqliteAcquisitionSchema::Tables
    );
    assert!(located(&final_handles, live));
    assert!(!located(&final_handles, dead));
    assert_eq!(acquisition_rows(&path), 0, "working rows are cleared");
    clean(&final_handles);
}

/// §6.3: the mark itself streams the visited set into `<store>.compact-visited`
/// as it discovers objects, so the build has no second in-memory map to write.
#[test]
fn the_mark_streams_its_visited_set_into_the_scratch_table() {
    let (_temp, handles, path) = create_store();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let dead = whole_file(&handles, b"reached by no retained root");
    let scratch = visited_path(&path);
    let mark = handles.history.mark_with_scratch(&scratch).unwrap();
    assert!(scratch.exists(), "the mark writes the temp-file table");
    let connection =
        rusqlite::Connection::open_with_flags(&scratch, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let rows: i64 = connection
        .query_row("SELECT count(*) FROM visited", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        u64::try_from(rows).unwrap(),
        mark.objects().len() as u64,
        "one row per reached object"
    );
    let packs: i64 = connection
        .query_row("SELECT count(*) FROM live_pack", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        u64::try_from(packs).unwrap(),
        mark.objects().packs().len() as u64,
        "one row per retained pack"
    );
    let present: i64 = connection
        .query_row(
            "SELECT count(*) FROM visited WHERE object_id=?1",
            rusqlite::params![live.as_bytes().as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(present, 1, "the reached root has a row");
    let absent: i64 = connection
        .query_row(
            "SELECT count(*) FROM visited WHERE object_id=?1",
            rusqlite::params![dead.as_bytes().as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(absent, 0, "a dead object is not in the visited set");
    for suffix in ["-journal", "-wal", "-shm"] {
        let mut sidecar = scratch.as_os_str().to_os_string();
        sidecar.push(suffix);
        assert!(
            !PathBuf::from(sidecar).exists(),
            "the table leaves no journal sidecar"
        );
    }
    drop(connection);
    std::fs::remove_file(&scratch).unwrap();
}
