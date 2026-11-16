//! #41 T9: the §8 PostgreSQL rebuild. A compaction takes the Store-scoped
//! exclusive intent, records the mark tuple, builds the retained payload in a
//! scratch schema, swaps `pack`/`object_location` at `G -> G + 1`, deletes only
//! the persisted `gc_dead` set, settles the totals once, and leaves no
//! scratch schema or op row. A leftover at `G` is removed; one at `G + 1`
//! finishes Phase C (lost-reply resume). A conflicting lock is `Busy` (55P03).
//!
//! Needs a writable PostgreSQL server: set `LAYERFS_TEST_POSTGRES_URL` (for
//! example `host=127.0.0.1 port=55432 user=postgres dbname=postgres
//! sslmode=disable`). Without it each test reports NOT_RUN.
#![cfg(feature = "postgresql")]
mod support;

use layerfs_content::{encode_whole_file_payload, FinalizedObject, ObjectId, ObjectRole};
use layerfs_history::{
    AddLayerOutcome, AddLayerRequest, BranchId, CommitMeta, CommitOutcome, CommitRequest,
    CommitVia, ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig, LayerStackId,
    LayerStackRecord, StackInitialization,
};
use layerfs_persistence::{
    CompactFailpoint, CompactPath, CompactRequest, PostgresHandles, PostgresSelection,
};
use layerfs_storage::{port::PackPersistence, Storage, StoragePolicy};
use support::{branch_id, name, stack, BINDING};

/// Drops the test's schema when the test ends, pass or fail.
struct Schema {
    url: String,
    selection: PostgresSelection,
}
impl Schema {
    fn new(url: &str, tag: &str) -> Self {
        let name = format!("lfs_t9_{tag}_{}", std::process::id());
        let selection = PostgresSelection::new(url, name);
        let schema = Self {
            url: url.to_owned(),
            selection,
        };
        schema.drop_schema();
        schema
    }
    fn open(&self) -> PostgresHandles {
        PostgresHandles::open_writable(&self.selection, BINDING, [71; 32]).expect("Store open")
    }
    fn create(&self) -> PostgresHandles {
        PostgresHandles::create(&self.selection, StoragePolicy::frozen_default(), &catalog())
            .expect("Store creation")
    }
    fn client(&self) -> postgres::Client {
        let mut client = postgres::Client::connect(&self.url, postgres::NoTls).expect("client");
        client
            .batch_execute(&format!("SET search_path TO {}", self.selection.schema))
            .expect("search path");
        client
    }
    fn count(&self, table: &str) -> i64 {
        let sql = format!("SELECT count(*) FROM {}.{}", self.selection.schema, table);
        self.client().query_one(&sql, &[]).expect("count").get(0)
    }
    fn count_where(&self, sql: &str) -> i64 {
        let sql = sql.replace("{s}", &self.selection.schema);
        self.client().query_one(&sql, &[]).expect("count").get(0)
    }
    fn scalar(&self, sql: &str) -> i64 {
        let sql = sql.replace("{s}", &self.selection.schema);
        self.client().query_one(&sql, &[]).expect("scalar").get(0)
    }
    fn exists(&self, sql: &str) -> bool {
        self.scalar(sql) != 0
    }
    fn scratch_exists(&self, op: u8) -> bool {
        let scratch = format!("lfs_gc_{}", format!("{op:02x}").repeat(16));
        self.exists(&format!(
            "SELECT (CASE WHEN EXISTS (SELECT 1 FROM pg_namespace WHERE nspname='{scratch}') THEN 1 ELSE 0 END)::bigint"
        ))
    }
    fn drop_scratch(&self, op: u8) {
        let scratch = format!("lfs_gc_{}", format!("{op:02x}").repeat(16));
        if let Ok(mut client) = postgres::Client::connect(&self.url, postgres::NoTls) {
            let _ = client.batch_execute(&format!("DROP SCHEMA IF EXISTS {scratch} CASCADE"));
        }
    }
    fn drop_schema(&self) {
        if let Ok(mut client) = postgres::Client::connect(&self.url, postgres::NoTls) {
            let _ = client.batch_execute(&format!(
                "DROP SCHEMA IF EXISTS {} CASCADE",
                self.selection.schema
            ));
        }
    }
}
impl Drop for Schema {
    fn drop(&mut self) {
        self.drop_schema();
    }
}

fn catalog() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: BINDING.to_vec(),
        cursor_key: [71; 32],
        incarnation: 7,
    }
}

fn url() -> Option<String> {
    let url = std::env::var("LAYERFS_TEST_POSTGRES_URL").ok();
    if url.is_none() {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
    }
    url
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

fn whole_file(handles: &PostgresHandles, bytes: &[u8]) -> ObjectId {
    let object = FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(bytes).unwrap(),
    )
    .unwrap();
    save_objects(handles, &[object])
}

fn save_objects(handles: &PostgresHandles, objects: &[FinalizedObject]) -> ObjectId {
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

fn filesystem_root(handles: &PostgresHandles, seed: u8) -> ObjectId {
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

fn initialize(handles: &PostgresHandles, tag: u8, genesis: ObjectId) -> LayerStackRecord {
    handles
        .history
        .initialize_layerstack(&StackInitialization {
            logical: 10,
            stack: stack(tag),
            name: name(&format!("project-{tag}")),
            scope: layerfs_content::filesystem::scope_for_seed([1; 32]).object(),
            profile: layerfs_content::filesystem::root::profile_id(),
            genesis_root: genesis,
        })
        .unwrap()
}

fn fork(handles: &PostgresHandles, stack: LayerStackId, branch: BranchId, tag: u8) {
    let record = handles.history.layer_stack(stack).unwrap().unwrap();
    handles
        .history
        .fork(&ForkRequest {
            stack,
            branch,
            name: name(&format!("branch-{tag}")),
            source: ForkSource::Layer(record.head_layer),
        })
        .unwrap();
}

fn commit(
    handles: &PostgresHandles,
    branch: BranchId,
    candidate_root: ObjectId,
) -> layerfs_history::CommitId {
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

fn publish(handles: &PostgresHandles, stack: LayerStackId, branch: BranchId) {
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

fn located(handles: &PostgresHandles, id: ObjectId) -> bool {
    let mut found = Vec::new();
    handles.storage.locate(&[id], &mut found).unwrap();
    !found.is_empty()
}

fn generation(handles: &PostgresHandles) -> u64 {
    handles.history.incarnation()
}

/// The live generation from the Store row (the open session caches the value
/// it opened at, as it does on SQLite).
fn db_generation(schema: &Schema) -> u64 {
    u64::try_from(schema.scalar("SELECT catalog_incarnation FROM {s}.history_meta WHERE id=1"))
        .unwrap()
}

fn clean(handles: &PostgresHandles) {
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

fn assert_settled(schema: &Schema, op: u8) {
    assert_eq!(schema.count("gc_op"), 0, "gc_op is cleared on settle");
    assert_eq!(schema.count("gc_dead"), 0, "gc_dead is cleared on settle");
    assert_eq!(
        schema.count_where("SELECT count(*) FROM {s}.gc_intent WHERE kind='compaction'"),
        0,
        "the compaction intent is released"
    );
    assert!(
        !schema.scratch_exists(op),
        "the recorded scratch schema is dropped"
    );
    let logical = schema.scalar("SELECT logical FROM {s}.store_totals WHERE id=1");
    let sum = schema.scalar(
        "SELECT (SELECT COALESCE(SUM(logical)::bigint,0) FROM {s}.\"commit\")+(SELECT COALESCE(SUM(logical)::bigint,0) FROM {s}.layer)",
    );
    assert_eq!(logical, sum, "store_totals.logical tracks the history rows");
    let canonical = schema.scalar("SELECT canonical FROM {s}.store_totals WHERE id=1");
    let canonical_sum =
        schema.scalar("SELECT COALESCE(SUM(canonical_length)::bigint,0) FROM {s}.object_location");
    assert_eq!(canonical, canonical_sum, "store_totals.canonical");
    let stored = schema.scalar("SELECT stored FROM {s}.store_totals WHERE id=1");
    let stored_sum = schema.scalar("SELECT COALESCE(SUM(length)::bigint,0) FROM {s}.pack");
    assert_eq!(stored, stored_sum, "store_totals.stored");
    let metadata = schema.scalar("SELECT metadata FROM {s}.store_totals WHERE id=1");
    let metadata_sum =
        schema.scalar("SELECT COALESCE(SUM(length)::bigint,0) FROM {s}.pack WHERE domain=0");
    assert_eq!(metadata, metadata_sum, "store_totals.metadata");
}

#[test]
fn an_empty_store_takes_the_none_path() {
    let Some(url) = url() else { return };
    let schema = Schema::new(&url, "empty");
    let handles = schema.create();
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
    assert!(receipt
        .zero_reasons
        .iter()
        .any(|zero| zero.field == "backendReusable" && zero.reason == "postgresql-in-place"));
    assert_settled(&schema, 1);
    assert_eq!(generation(&handles), before);
    handles.history.mark().unwrap();
}

#[test]
fn a_live_store_takes_the_none_path() {
    let Some(url) = url() else { return };
    let schema = Schema::new(&url, "live");
    let handles = schema.create();
    let genesis = filesystem_root(&handles, 1);
    initialize(&handles, 1, genesis);
    let before = generation(&handles);
    let receipt = handles.compact(&request(2)).unwrap();
    assert_eq!(receipt.path, CompactPath::None);
    assert_eq!(receipt.dead_bytes_bound, 0);
    assert_eq!(receipt.dead_bytes_refined, receipt.dead_bytes_bound);
    assert_eq!(receipt.generation_after, before);
    assert_eq!(receipt.physical_shrink, 0);
    assert_settled(&schema, 2);
    assert!(located(&handles, genesis));
}

#[test]
fn a_wholly_dead_pack_is_rebuilt_away_and_totals_settle() {
    let Some(url) = url() else { return };
    let schema = Schema::new(&url, "rebuild");
    let handles = schema.create();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let dead = whole_file(&handles, b"nobody reaches this object");
    assert!(located(&handles, dead));
    let before = generation(&handles);
    let receipt = handles.compact(&request(3)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(receipt.generation_before, before);
    assert_eq!(receipt.generation_after, before + 1);
    assert_eq!(receipt.dead_bytes_refined, receipt.dead_bytes_bound);
    assert!(receipt.dead_bytes_bound > 0);
    assert_eq!(receipt.backend_reusable, 0);
    assert_settled(&schema, 3);
    assert_eq!(db_generation(&schema), before + 1);
    assert!(!located(&handles, dead), "the dead object is collected");
    assert!(located(&handles, live), "the retained object survives");
    clean(&schema.open());
}

#[test]
fn retained_roots_survive_a_rebuild_that_collects_a_deleted_project() {
    let Some(url) = url() else { return };
    let schema = Schema::new(&url, "retained");
    let handles = schema.create();
    let live_genesis = filesystem_root(&handles, 1);
    let live_commit = filesystem_root(&handles, 2);
    let live = initialize(&handles, 1, live_genesis);
    let work = branch_id(1);
    fork(&handles, live.id, work, 1);
    let retained_commit = commit(&handles, work, live_commit);
    publish(&handles, live.id, work);

    let dead_genesis = filesystem_root(&handles, 3);
    let dead_commit = filesystem_root(&handles, 4);
    let gone = initialize(&handles, 2, dead_genesis);
    let dead_branch = branch_id(2);
    fork(&handles, gone.id, dead_branch, 2);
    let collected_commit = commit(&handles, dead_branch, dead_commit);
    publish(&handles, gone.id, dead_branch);
    handles.history.mark_stack_deleted(gone.id, 5_000).unwrap();

    let before = generation(&handles);
    let receipt = handles.compact(&request(4)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert!(
        receipt.logical_discarded > 0,
        "the deleted project's rows are collected"
    );
    assert_eq!(receipt.generation_after, before + 1);
    assert_settled(&schema, 4);
    // The retained project and its history survive.
    assert!(handles.history.layer_stack(live.id).unwrap().is_some());
    assert!(handles.history.branch(work).unwrap().is_some());
    assert!(handles.history.commit(retained_commit).unwrap().is_some());
    assert!(located(&handles, live_genesis));
    assert!(located(&handles, live_commit));
    // The deleted project's history and objects are gone.
    assert!(handles.history.layer_stack(gone.id).unwrap().is_none());
    assert!(handles.history.commit(collected_commit).unwrap().is_none());
    assert!(!located(&handles, dead_genesis));
    assert!(!located(&handles, dead_commit));
    clean(&schema.open());
}

#[test]
fn a_lost_swap_reply_is_resumed_by_the_next_compact() {
    let Some(url) = url() else { return };
    let schema = Schema::new(&url, "lost");
    let handles = schema.create();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let _dead = whole_file(&handles, b"a pack collected by the first round");
    let before = generation(&handles);
    let error = handles
        .compact(&request_at(5, CompactFailpoint::PostgresAfterSwap))
        .unwrap_err();
    assert!(matches!(
        error,
        layerfs_history::HistoryError::UnknownOutcome
    ));
    // The swap committed: the Store is at G + 1 and the op still names Phase C.
    assert_eq!(db_generation(&schema), before + 1);
    assert_eq!(schema.count("gc_op"), 1);
    assert!(schema.scalar("SELECT count(*) FROM {s}.gc_dead") > 0);
    let receipt = handles.compact(&request(6)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(receipt.generation_before, before);
    assert_eq!(receipt.generation_after, before + 1);
    assert_settled(&schema, 6);
    assert_eq!(db_generation(&schema), before + 1);
    clean(&schema.open());
}

#[test]
fn the_same_session_resumes_an_uncertain_round() {
    let Some(url) = url() else { return };
    let schema = Schema::new(&url, "same");
    schema.drop_scratch(11);
    let handles = schema.create();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let _dead = whole_file(&handles, b"a pack the resumed round collects");
    let before = generation(&handles);
    let error = handles
        .compact(&request_at(11, CompactFailpoint::PostgresAfterMark))
        .unwrap_err();
    assert!(matches!(
        error,
        layerfs_history::HistoryError::UnknownOutcome
    ));
    // The same session is the recorded owner and is idle; it adopts its own
    // uncertain round instead of terminating itself.
    let receipt = handles.compact(&request(12)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(receipt.generation_before, before);
    assert_eq!(receipt.generation_after, before + 1);
    assert_settled(&schema, 12);
    clean(&schema.open());
}

#[test]
fn a_stale_owner_backend_is_terminated_before_the_next_round() {
    let Some(url) = url() else { return };
    let schema = Schema::new(&url, "terminate");
    schema.drop_scratch(9);
    let first = schema.create();
    let live = filesystem_root(&first, 1);
    initialize(&first, 1, live);
    let _dead = whole_file(&first, b"a pack the adopted round collects");
    let before = generation(&first);
    let error = first
        .compact(&request_at(9, CompactFailpoint::PostgresAfterMark))
        .unwrap_err();
    assert!(matches!(
        error,
        layerfs_history::HistoryError::UnknownOutcome
    ));
    let pid = schema.scalar("SELECT pid FROM {s}.gc_op WHERE id=1");
    assert!(pid > 0, "the op records its owner backend");
    // The recorded owner is alive but idle: the next round terminates that
    // (pid, backend_start) and adopts the op as a dead owner (§9, §12).
    let second = schema.open();
    let receipt = second.compact(&request(10)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(receipt.generation_before, before);
    assert_eq!(receipt.generation_after, before + 1);
    assert_eq!(db_generation(&schema), before + 1);
    assert_settled(&schema, 10);
    let mut gone = false;
    for _ in 0..50 {
        if schema.scalar(&format!(
            "SELECT count(*) FROM pg_stat_activity WHERE pid={pid}"
        )) == 0
        {
            gone = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(gone, "the stale owner backend is terminated");
    clean(&schema.open());
}

#[test]
fn an_exclusive_table_lock_makes_the_swap_busy() {
    let Some(url) = url() else { return };
    let schema = Schema::new(&url, "busy");
    schema.drop_scratch(7);
    let handles = schema.create();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let _dead = whole_file(&handles, b"a pack the rebuild would collect");
    // Phase A never touches store_totals; the swap's ACCESS EXCLUSIVE NOWAIT
    // does, so this lock is met exactly at the swap (SQLSTATE 55P03).
    let mut blocker = schema.client();
    blocker
        .batch_execute(&format!(
            "BEGIN; LOCK TABLE {}.binding_hint_block IN ACCESS EXCLUSIVE MODE;",
            schema.selection.schema
        ))
        .unwrap();
    let error = handles.compact(&request(7)).unwrap_err();
    blocker.batch_execute("ROLLBACK").unwrap();
    assert!(
        matches!(error, layerfs_history::HistoryError::Busy),
        "a 55P03 lock at the swap is Busy, got {error:?}"
    );
}
