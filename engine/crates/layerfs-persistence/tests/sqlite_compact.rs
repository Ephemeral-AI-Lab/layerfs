//! #41 T8: the §7 SQLite rebuild. A compaction takes the writer lane, runs the
//! mark, chooses `none`/`rebuild` from the wholly dead packs, builds and
//! verifies `<store>.compact`, installs it at `G → G+1`, and leaves no
//! artifact; a recorded leftover is judged by the generation and removed by
//! the next `store.compact`.
#![cfg(any(target_os = "macos", target_os = "linux"))]
mod support;

use layerfs_content::{encode_whole_file_payload, FinalizedObject, ObjectId, ObjectRole};
use layerfs_history::{
    AddLayerOutcome, AddLayerRequest, BranchId, CommitId, CommitMeta, CommitOutcome,
    CommitRequest, CommitVia, ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig,
    LayerStackId, LayerStackRecord, StackInitialization,
};
use layerfs_persistence::{
    CompactFailpoint, CompactPath, CompactRequest, PersistenceConfig, SqliteHandles,
    SqlitePersistenceProfile,
};
use layerfs_storage::{
    port::PackPersistence, Storage, StoragePolicy,
};
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
            root::profile_id, scope_for_seed, FilesystemRoot,
        },
        inode_leaf::{InodeKind, InodeMetadata, InodeValue},
    };
    let directory = FinalizedObject::new(
        ObjectRole::DirectoryLeaf,
        encode_directory_page(&DirectoryPage::Leaf { entries: Vec::new() }).unwrap(),
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
    let root = FilesystemRoot::new(
        profile_id(),
        scope_for_seed([1; 32]),
        1,
        inode_id,
    )
    .unwrap();
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
        None => handles
            .history
            .layer(snapshot.base_layer)
            .unwrap()
            .unwrap()
            .root,
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
