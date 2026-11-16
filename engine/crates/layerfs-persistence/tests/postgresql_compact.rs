//! #41 T9: the §8 PostgreSQL rebuild. A compaction takes the Store-scoped
//! exclusive intent, records the mark tuple, builds the retained payload in a
//! scratch schema, swaps `pack`/`object_location` at `G -> G + 1`, deletes only
//! the persisted `gc_dead` set, settles the totals once, and leaves no
//! scratch schema or op row. A leftover at `G` is removed; one at `G + 1`
//! finishes Phase C (lost-reply resume). A conflicting lock is `Busy` (55P03).
//!
//! T10.4 adds the bounded `compact.*` capture tests: a successful rebuild, the
//! `none` path, and the Busy/Unresolved refusals. They assert the same closed
//! labels and values as the SQLite capture test.
//!
//! AC-44 adds the kill arms: a real `pg_terminate_backend` of the recorded
//! `(pid, backend_start)` before the swap COMMIT (wholly at `G`) and during
//! Phase C after the swap COMMIT (wholly at `G + 1`), plus the deterministic
//! `PostgresAfterSwap` lost-reply arm whose owner is then really killed. A
//! named failpoint alone is a simulated lost reply: the process survives and
//! releases its intent. A killed backend leaves the op, the intent and (before
//! COMMIT) the scratch for the next call to resolve.
//!
//! Needs a writable PostgreSQL server: set `LAYERFS_TEST_POSTGRES_URL` (for
//! example `host=127.0.0.1 port=55432 user=postgres dbname=postgres
//! sslmode=disable`). Without it each test reports NOT_RUN.
#![cfg(feature = "postgresql")]
mod support;

use layerfs_content::{encode_whole_file_payload, FinalizedObject, ObjectId, ObjectRole};
use layerfs_history::records::{GcIntent, GcIntentKind, GcScopeKind};
use layerfs_history::{
    AddLayerOutcome, AddLayerRequest, BranchId, CommitMeta, CommitOutcome, CommitRequest,
    CommitVia, ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig, LayerStackId,
    LayerStackRecord, StackInitialization,
};
use layerfs_persistence::compact_metrics::CompactMetricsCapture;
use layerfs_persistence::{
    CompactFailpoint, CompactPath, CompactRequest, PostgresHandles, PostgresSelection,
};
use layerfs_storage::{port::PackPersistence, Storage, StoragePolicy};
use support::metrics::{assert_bounded, metric, value};
use support::{branch_id, name, stack};

/// Drops the test's schema when the test ends, pass or fail.
struct Schema {
    url: String,
    selection: PostgresSelection,
    /// This test's own catalog identity. The Store-scoped admission advisory
    /// key derives from it, and PostgreSQL advisory locks are server-wide
    /// (not schema-scoped), so tests that shared one binding would exclude
    /// each other while running in parallel.
    binding: Vec<u8>,
}
impl Schema {
    fn new(url: &str, tag: &str) -> Self {
        let name = format!("lfs_t9_{tag}_{}", std::process::id());
        let selection = PostgresSelection::new(url, name);
        let binding = binding(tag);
        let schema = Self {
            url: url.to_owned(),
            selection,
            binding,
        };
        schema.drop_schema();
        schema
    }
    fn open(&self) -> PostgresHandles {
        PostgresHandles::open_writable(&self.selection, &self.binding, [71; 32])
            .expect("Store open")
    }
    fn create(&self) -> PostgresHandles {
        PostgresHandles::create(
            &self.selection,
            StoragePolicy::frozen_default(),
            &catalog(&self.binding),
        )
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

/// Every test opens its own catalog identity, as a real Store does: the
/// PostgreSQL advisory keys are server-global, so two tests that shared a
/// binding could see each other's intents while running in parallel.
fn binding(tag: &str) -> Vec<u8> {
    format!("postgresql-compact-{tag}").into_bytes()
}

fn catalog(binding: &[u8]) -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: binding.to_vec(),
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

/// The phase counts one PostgreSQL rebuild times: Phase M/A/B/C, each once.
fn assert_pg_rebuild_phases(metrics: &layerfs_persistence::compact_metrics::CompactMetrics) {
    for phase in ["pg_mark", "pg_build", "pg_swap", "pg_cleanup"] {
        assert_eq!(
            value(
                metrics,
                "compact.phase.duration_ns",
                &[("phase", phase), ("statistic", "count")]
            ),
            1,
            "PostgreSQL {phase} was timed once"
        );
        assert!(
            value(
                metrics,
                "compact.phase.duration_ns",
                &[("phase", phase), ("statistic", "sum")]
            ) > 0,
            "PostgreSQL {phase} took time"
        );
        assert!(
            value(
                metrics,
                "compact.phase.duration_ns",
                &[("phase", phase), ("statistic", "max")]
            ) > 0,
            "PostgreSQL {phase} has a duration"
        );
    }
    for phase in ["mark", "build", "verify", "install"] {
        assert_eq!(
            metric(
                metrics,
                "compact.phase.duration_ns",
                &[("phase", phase), ("statistic", "count")]
            ),
            None,
            "a PostgreSQL run never emits the SQLite phase {phase}"
        );
    }
}

#[test]
fn a_rebuild_records_bounded_metrics_with_pg_phases() {
    let Some(url) = url() else { return };
    let schema = Schema::new(&url, "metrics_rebuild");
    let handles = schema.create();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let _dead = whole_file(&handles, b"a pack the metrics rebuild collects");
    // The same mark state the compaction's Phase M runs at: its body reads are
    // the `compact.mark_bytes` gauge the run records.
    let (mark, _) = handles.history.mark_gc().unwrap();
    let mark_bytes = mark.mark_bytes();
    assert!(mark_bytes > 0, "the mark reads the live pack bodies");

    let capture = CompactMetricsCapture::start();
    let receipt = handles.compact(&request(21)).unwrap();
    let metrics = capture.finish();

    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_bounded(&metrics);
    assert_eq!(value(&metrics, "compact.runs", &[]), 1);
    assert_eq!(
        value(&metrics, "compact.operations", &[("outcome", "rebuild")]),
        1
    );
    assert_pg_rebuild_phases(&metrics);
    assert_eq!(
        value(&metrics, "compact.duration_ns", &[("statistic", "count")]),
        1
    );
    assert!(value(&metrics, "compact.duration_ns", &[("statistic", "sum")]) > 0);
    assert_eq!(
        value(&metrics, "compact.mark_bytes", &[]),
        mark_bytes,
        "AC-41: the run records the Phase M mark's distinct body bytes"
    );
    assert_eq!(
        value(&metrics, "compact.dead_bytes", &[("kind", "bound")]),
        receipt.dead_bytes_bound
    );
    assert_eq!(
        value(&metrics, "compact.dead_bytes", &[("kind", "refined")]),
        receipt.dead_bytes_refined
    );
    let kept = receipt
        .kept_waste
        .dead_in_retained_packs_bytes
        .saturating_add(receipt.kept_waste.delta_bases_bytes)
        .saturating_add(receipt.kept_waste.inode_blocks_bytes);
    assert_eq!(value(&metrics, "compact.kept_waste_bytes", &[]), kept);
    assert_eq!(
        value(&metrics, "compact.residue_bytes", &[]),
        receipt.residue.bytes
    );
    assert_eq!(
        value(&metrics, "compact.generation", &[("which", "before")]),
        receipt.generation_before
    );
    assert_eq!(
        value(&metrics, "compact.generation", &[("which", "after")]),
        receipt.generation_after
    );
    assert_eq!(
        value(
            &metrics,
            "compact.slo",
            &[("objective", "residue_free"), ("result", "pass")]
        ),
        1
    );
    assert_eq!(
        value(
            &metrics,
            "compact.slo",
            &[("objective", "generation"), ("result", "pass")]
        ),
        1
    );
    assert_eq!(value(&metrics, "compact.settles", &[("kind", "run")]), 1);
    assert_eq!(
        metric(&metrics, "compact.settles", &[("kind", "leftover")]),
        None,
        "a fresh rebuild settles no leftover"
    );
    assert_eq!(
        metric(&metrics, "compact.retries", &[("reason", "leftover")]),
        None
    );

    clean(&schema.open());
}

#[test]
fn a_none_path_records_only_the_pg_mark_phase() {
    let Some(url) = url() else { return };
    let schema = Schema::new(&url, "metrics_none");
    let handles = schema.create();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);

    let capture = CompactMetricsCapture::start();
    let receipt = handles.compact(&request(22)).unwrap();
    let metrics = capture.finish();

    assert_eq!(receipt.path, CompactPath::None);
    assert_bounded(&metrics);
    assert_eq!(value(&metrics, "compact.runs", &[]), 1);
    assert_eq!(
        value(&metrics, "compact.operations", &[("outcome", "none")]),
        1
    );
    assert_eq!(
        value(
            &metrics,
            "compact.phase.duration_ns",
            &[("phase", "pg_mark"), ("statistic", "count")]
        ),
        1
    );
    for phase in ["pg_build", "pg_swap", "pg_cleanup"] {
        assert_eq!(
            metric(
                &metrics,
                "compact.phase.duration_ns",
                &[("phase", phase), ("statistic", "count")]
            ),
            None,
            "the PostgreSQL none path never enters {phase}"
        );
    }
    assert_eq!(
        value(&metrics, "compact.dead_bytes", &[("kind", "bound")]),
        receipt.dead_bytes_bound
    );
    assert_eq!(
        value(&metrics, "compact.dead_bytes", &[("kind", "refined")]),
        receipt.dead_bytes_refined
    );
    assert_eq!(
        value(&metrics, "compact.residue_bytes", &[]),
        0,
        "none leaves no artifact"
    );
    assert_eq!(
        value(&metrics, "compact.generation", &[("which", "before")]),
        receipt.generation_before
    );
    assert_eq!(
        value(&metrics, "compact.generation", &[("which", "after")]),
        receipt.generation_after,
        "none keeps the generation"
    );
    assert_eq!(receipt.generation_after, receipt.generation_before);
    assert_eq!(
        value(
            &metrics,
            "compact.slo",
            &[("objective", "generation"), ("result", "pass")]
        ),
        1
    );
    assert_eq!(
        value(
            &metrics,
            "compact.slo",
            &[("objective", "residue_free"), ("result", "pass")]
        ),
        1
    );
    assert_eq!(value(&metrics, "compact.settles", &[("kind", "run")]), 1);

    clean(&schema.open());
}

#[test]
fn a_busy_swap_records_a_failed_outcome_and_a_busy_refusal() {
    let Some(url) = url() else { return };
    let schema = Schema::new(&url, "metrics_busy");
    schema.drop_scratch(23);
    let handles = schema.create();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let _dead = whole_file(&handles, b"a pack the metrics busy run would collect");
    // Phase A never touches store_totals; the swap's ACCESS EXCLUSIVE NOWAIT
    // does, so this lock is met exactly at Phase B (SQLSTATE 55P03).
    let mut blocker = schema.client();
    blocker
        .batch_execute(&format!(
            "BEGIN; LOCK TABLE {}.binding_hint_block IN ACCESS EXCLUSIVE MODE;",
            schema.selection.schema
        ))
        .unwrap();

    let capture = CompactMetricsCapture::start();
    let error = handles.compact(&request(23)).unwrap_err();
    let metrics = capture.finish();
    blocker.batch_execute("ROLLBACK").unwrap();

    let layerfs_history::HistoryError::BusyOp(state) = &error else {
        panic!("a 55P03 lock at the swap is a typed Busy, got {error:?}");
    };
    assert_eq!(state.entity, "store");
    assert_eq!(state.phase.as_deref(), Some("swap"));
    assert!(
        state.pid.is_some(),
        "the blocker backend is named: {state:?}"
    );
    assert_bounded(&metrics);
    assert_eq!(value(&metrics, "compact.runs", &[]), 1);
    assert_eq!(
        value(&metrics, "compact.operations", &[("outcome", "failed")]),
        1
    );
    assert_eq!(value(&metrics, "compact.refusals", &[("code", "busy")]), 1);
    for phase in ["pg_mark", "pg_build", "pg_swap"] {
        assert_eq!(
            value(
                &metrics,
                "compact.phase.duration_ns",
                &[("phase", phase), ("statistic", "count")]
            ),
            1,
            "the refused run reached {phase}"
        );
    }
    assert_eq!(
        metric(
            &metrics,
            "compact.phase.duration_ns",
            &[("phase", "pg_cleanup"), ("statistic", "count")]
        ),
        None,
        "the refused run never reaches Phase C"
    );
    assert_eq!(
        metric(&metrics, "compact.dead_bytes", &[("kind", "bound")]),
        None,
        "a failed run publishes no report gauge"
    );
    assert_eq!(
        metric(&metrics, "compact.generation", &[("which", "after")]),
        None
    );
    assert_eq!(
        metric(&metrics, "compact.settles", &[("kind", "run")]),
        None
    );
    assert_eq!(
        value(&metrics, "compact.duration_ns", &[("statistic", "count")]),
        1
    );
}

#[test]
fn an_uncertain_intent_records_an_unresolved_refusal() {
    let Some(url) = url() else { return };
    let schema = Schema::new(&url, "metrics_unresolved");
    let handles = schema.create();
    // A dead owner's uncertain compaction intent with no `gc_op` row: the
    // next call meets it at admission, before Phase M writes anything.
    let dead = [0x44u8; 16];
    let intent = GcIntent::operation(
        dead,
        GcIntentKind::Compaction,
        GcScopeKind::Store,
        vec![0u8; 32],
    )
    .with_phase(Some("mark"));
    handles.history.gc_register(&intent).unwrap();
    handles.history.gc_mark_uncertain(&dead).unwrap();

    let capture = CompactMetricsCapture::start();
    let error = handles.compact(&request(24)).unwrap_err();
    let metrics = capture.finish();

    assert!(
        matches!(error, layerfs_history::HistoryError::Unresolved(_)),
        "{error:?}"
    );
    assert_bounded(&metrics);
    assert_eq!(value(&metrics, "compact.runs", &[]), 1);
    assert_eq!(
        value(&metrics, "compact.operations", &[("outcome", "failed")]),
        1
    );
    assert_eq!(
        value(&metrics, "compact.refusals", &[("code", "unresolved")]),
        1
    );
    for phase in ["pg_mark", "pg_build", "pg_swap", "pg_cleanup"] {
        assert_eq!(
            metric(
                &metrics,
                "compact.phase.duration_ns",
                &[("phase", phase), ("statistic", "count")]
            ),
            None,
            "the refusal stops before {phase}"
        );
    }
    assert_eq!(
        value(&metrics, "compact.duration_ns", &[("statistic", "count")]),
        1
    );
    assert_eq!(
        metric(&metrics, "compact.generation", &[("which", "after")]),
        None
    );

    handles.history.gc_release(&dead).unwrap();
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

/// Finding 1: a non-owner grantee, a `PUBLIC` grant, a grant option and a
/// column ACL all survive the swap. Replaying them before the drop wrote them
/// on the table that was about to disappear, so the new table came back with
/// an empty ACL (reproduced on PostgreSQL 17).
#[test]
fn a_rebuild_keeps_the_owner_and_every_acl_level() {
    let Some(url) = url() else { return };
    let schema = Schema::new(&url, "acl");
    let handles = schema.create();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let _dead = whole_file(&handles, b"a pack the acl rebuild replaces");

    let suffix = format!("{}_{}", std::process::id(), "acl");
    let grantee = format!("lfs_acl_grantee_{suffix}");
    let public = format!("lfs_acl_public_{suffix}");
    let mut admin = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    for role in [&grantee, &public] {
        admin
            .batch_execute(&format!("DROP ROLE IF EXISTS {role}"))
            .ok();
        admin
            .batch_execute(&format!("CREATE ROLE {role} NOLOGIN"))
            .unwrap();
    }
    let s = schema.selection.schema.clone();
    admin
        .batch_execute(&format!(
            "GRANT SELECT ON TABLE {s}.pack TO {grantee}; \
             GRANT UPDATE ON TABLE {s}.object_location TO {grantee} WITH GRANT OPTION; \
             GRANT SELECT (object_id) ON TABLE {s}.object_location TO {grantee}; \
             GRANT SELECT ON TABLE {s}.pack TO PUBLIC"
        ))
        .unwrap();

    handles.compact(&request(42)).unwrap();

    let table_privilege =
        |admin: &mut postgres::Client, role: &str, table: &str, privilege: &str| -> bool {
            admin
                .query_one(
                    "SELECT has_table_privilege($1::name, $2, $3)",
                    &[&role, &format!("{s}.{table}"), &privilege],
                )
                .unwrap()
                .get(0)
        };
    assert!(
        table_privilege(&mut admin, &grantee, "pack", "SELECT"),
        "the non-owner grantee keeps SELECT on the rebuilt pack table"
    );
    assert!(
        table_privilege(&mut admin, &grantee, "object_location", "UPDATE"),
        "the non-owner grantee keeps UPDATE on the rebuilt object_location table"
    );
    assert!(
        table_privilege(
            &mut admin,
            &grantee,
            "object_location",
            "UPDATE WITH GRANT OPTION"
        ),
        "the grant option survives"
    );
    assert!(
        table_privilege(&mut admin, &public, "pack", "SELECT"),
        "a PUBLIC grant survives"
    );
    let column: bool = admin
        .query_one(
            "SELECT has_column_privilege($1::name, $2, $3, $4)",
            &[
                &grantee,
                &format!("{s}.object_location"),
                &"object_id",
                &"SELECT",
            ],
        )
        .unwrap()
        .get(0);
    assert!(column, "the column ACL survives");
    let owner: String = admin
        .query_one(
            "SELECT pg_get_userbyid(relowner)::text FROM pg_class \
              WHERE oid = to_regclass($1)",
            &[&format!("{s}.pack")],
        )
        .unwrap()
        .get(0);
    let current: String = admin
        .query_one("SELECT current_user::text", &[])
        .unwrap()
        .get(0);
    assert_eq!(owner, current, "the owner is unchanged");

    clean(&schema.open());
    for role in [&grantee, &public] {
        admin.batch_execute(&format!("DROP OWNED BY {role}")).ok();
        admin
            .batch_execute(&format!("DROP ROLE IF EXISTS {role}"))
            .ok();
    }
}

/// The advisory key the Store's exclusive intent uses (spec 5.2).
fn store_advisory_key(binding: &[u8]) -> i64 {
    let catalog = layerfs_history::CatalogId::derive(binding).unwrap();
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs/advisory/v1\0");
    hasher.update(catalog.as_slice());
    hasher.update(&[1u8]);
    hasher.update(catalog.as_slice());
    let digest = hasher.finalize();
    let mut head = [0u8; 8];
    head.copy_from_slice(&digest.as_bytes()[..8]);
    i64::from_be_bytes(head)
}

/// Finding 6: the G+1 resume runs under the exclusive Store key. Holding the
/// key from another session refuses the resume with `Busy`; before the claim
/// was taken first, `settle_leftover` resumed and returned a receipt while
/// the key was held elsewhere.
#[test]
fn a_g_plus_one_resume_takes_the_exclusive_store_key_first() {
    let Some(url) = url() else { return };
    let schema = Schema::new(&url, "resume_key");
    let handles = schema.create();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let _dead = whole_file(&handles, b"a pack the resumed run collects");
    let before = generation(&handles);
    let error = handles
        .compact(&request_at(72, CompactFailpoint::PostgresAfterSwap))
        .unwrap_err();
    assert!(matches!(
        error,
        layerfs_history::HistoryError::UnknownOutcome
    ));
    assert_eq!(schema.count("gc_op"), 1, "the G+1 leftover is recorded");

    let key = store_advisory_key(&schema.binding);
    let mut admin = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    admin
        .execute("SELECT pg_advisory_lock($1)", &[&key])
        .unwrap();
    let refused = handles.compact(&request(73)).unwrap_err();
    let layerfs_history::HistoryError::BusyOp(state) = &refused else {
        panic!("the held key refuses the resume with a typed Busy: {refused:?}");
    };
    assert!(
        state.pid.is_some(),
        "the key holder backend is named: {state:?}"
    );
    admin
        .execute("SELECT pg_advisory_unlock($1)", &[&key])
        .unwrap();
    let receipt = handles.compact(&request(74)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(receipt.generation_before, before);
    assert_eq!(receipt.generation_after, before + 1);
    assert_settled(&schema, 74);
    clean(&schema.open());
}

/// Finding 6: the live phase is written into `gc_op`, not only recorded in
/// the report: a run stopped after the build leaves `build`, and a resumed
/// G+1 run writes `cleanup`.
#[test]
fn a_rebuild_writes_the_build_and_cleanup_phases_to_the_op() {
    let Some(url) = url() else { return };
    let schema = Schema::new(&url, "op_phases");
    let handles = schema.create();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let _dead = whole_file(&handles, b"a pack the phase run collects");
    let error = handles
        .compact(&request_at(75, CompactFailpoint::PostgresAfterBuild))
        .unwrap_err();
    assert!(matches!(
        error,
        layerfs_history::HistoryError::UnknownOutcome
    ));
    let phase: String = schema
        .client()
        .query_one("SELECT phase FROM gc_op WHERE id=1", &[])
        .unwrap()
        .get(0);
    assert_eq!(phase, "build", "the live Phase A phase is recorded");
    handles.compact(&request(76)).unwrap();
    assert_settled(&schema, 76);
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
fn an_idle_owner_backend_is_not_terminated_under_the_exclusive_key() {
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
    // The recorded owner is alive but idle. The next round takes the exclusive
    // Store key first, which proves the owner is no longer inside the op, and
    // adopts the leftover without terminating a healthy connection (§9, §12).
    let second = schema.open();
    let receipt = second.compact(&request(10)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(receipt.generation_before, before);
    assert_eq!(receipt.generation_after, before + 1);
    assert_eq!(db_generation(&schema), before + 1);
    assert_settled(&schema, 10);
    assert_eq!(
        schema.scalar(&format!(
            "SELECT count(*) FROM pg_stat_activity WHERE pid={pid}"
        )),
        1,
        "the live owner backend is not terminated"
    );
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
    let layerfs_history::HistoryError::BusyOp(state) = &error else {
        panic!("a 55P03 lock at the swap is a typed Busy, got {error:?}");
    };
    assert_eq!(state.entity, "store");
    assert_eq!(state.phase.as_deref(), Some("swap"));
    assert!(
        state.pid.is_some(),
        "the blocker backend is named: {state:?}"
    );
}

// --- AC-44: a killed backend and the lost COMMIT reply ---------------------
//
// The failpoints above stop a live session and return `UnknownOutcome`: the
// process survives and releases its own intent, so they are a *simulated* lost
// reply. The tests below kill the PostgreSQL backend that owns the round, at a
// point where the server's own `pg_terminate_backend(pid, timeout)` is scoped to
// the `(pid, backend_start)` recorded in `gc_op` (the §12 proof). The next call
// then meets a real leftover whose recorded owner is gone and must resolve it:
//
// * `a_killed_backend_before_the_swap_commit_leaves_the_store_at_g` parks the
//   backend inside Phase B's own `UPDATE gc_op`, before COMMIT, and terminates
//   it. The Store is wholly at `G`; the next compact proves the owner ended and
//   adopts the `G` leftover.
// * `a_killed_backend_during_phase_c_resumes_at_g_plus_1` parks the backend in
//   Phase C's `store_totals` settle, after COMMIT committed the swap, and
//   terminates it. The Store is wholly at `G + 1`; the next compact reads the
//   incarnation and finishes Phase C.
// * `a_lost_swap_reply_from_an_ended_owner_resumes_at_g_plus_1` uses the named
//   `PostgresAfterSwap` failpoint as the deterministic stand-in for the instant
//   a COMMIT reply is lost (the caller sees `UnknownOutcome` while the swap is
//   committed), then kills that owner backend so the owner is provably ended
//   before a fresh connection resolves the reply.
//
// The only injected condition is a test-only `BEFORE UPDATE` trigger on the
// statement whose commit must be interrupted; it calls `pg_sleep` so a second
// connection can find the backend in `pg_stat_activity` and terminate exactly
// that process. Nothing in the Store is modified by the injection.

/// The four `store_totals` columns, so a killed round can be shown not to have
/// settled them.
fn totals(schema: &Schema) -> (i64, i64, i64, i64) {
    let mut client = schema.client();
    let row = client
        .query_one(
            &format!(
                "SELECT logical,canonical,stored,metadata FROM {}.store_totals WHERE id=1",
                schema.selection.schema
            ),
            &[],
        )
        .expect("store totals");
    (row.get(0), row.get(1), row.get(2), row.get(3))
}

/// One `gc_op` row: `(pid, backend_start, phase)`.
fn op_owner(schema: &Schema) -> Option<(i64, String, String)> {
    let mut client = schema.client();
    let row = client
        .query_opt(
            &format!(
                "SELECT pid, backend_start::text, phase::text FROM {}.gc_op WHERE id=1",
                schema.selection.schema
            ),
            &[],
        )
        .expect("the gc_op row");
    row.map(|row| (row.get(0), row.get(1), row.get(2)))
}

/// Waits for the interrupted round to record its `(pid, backend_start)`.
fn wait_for_op_owner(schema: &Schema) -> (i64, String, String) {
    for _ in 0..2400 {
        if let Some(owner) = op_owner(schema) {
            return owner;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("the interrupted round never wrote its gc_op row");
}

/// Installs a test-only trigger that parks the backend in `pg_sleep` on the
/// next `UPDATE` of `table`, before the update becomes visible or is committed.
fn park_backend(schema: &Schema, table: &str) {
    let name = format!("lfs_test_park_{table}");
    let mut client = schema.client();
    client
        .batch_execute(&format!(
            // `gc_op` gains the live `build` phase before Phase B, so parking
            // every update would stop the run before Phase A; only the swap
            // phase parks there. Other tables park on every update.
            "CREATE OR REPLACE FUNCTION {schema}.{name}() RETURNS trigger LANGUAGE plpgsql AS \
             $fn$ BEGIN \
               IF TG_TABLE_NAME = 'gc_op' THEN \
                 IF NEW.phase = 'swap' THEN PERFORM pg_sleep(120); END IF; \
               ELSE PERFORM pg_sleep(120); END IF; \
               RETURN NEW; \
             END $fn$;\
             CREATE TRIGGER {name} BEFORE UPDATE ON {schema}.{table} \
              FOR EACH ROW EXECUTE FUNCTION {schema}.{name}();",
            schema = schema.selection.schema,
            table = table,
            name = name,
        ))
        .expect("the test parking trigger");
}

/// Removes the test-only parking trigger and its function.
fn unpark_backend(schema: &Schema, table: &str) {
    let name = format!("lfs_test_park_{table}");
    let mut client = schema.client();
    let _ = client.batch_execute(&format!(
        "DROP TRIGGER IF EXISTS {name} ON {schema}.{table};\
         DROP FUNCTION IF EXISTS {schema}.{name}();",
        schema = schema.selection.schema,
        table = table,
        name = name,
    ));
}

/// Waits for the child backend parked in the unique schema-qualified
/// statement the test injected, and returns `(pid, backend_start)`. Matching on
/// the statement avoids reading `gc_op` while Phase B holds every base table
/// `ACCESS EXCLUSIVE`, so there is no race between the parent's poll and the
/// parked swap transaction.
fn wait_for_parked_statement(schema: &Schema, table: &str) -> (i64, String) {
    let pattern = format!("%{}.{table}%", schema.selection.schema);
    let mut client = schema.client();
    for _ in 0..4800 {
        let rows = client
            .query(
                "SELECT pid, backend_start::text FROM pg_stat_activity \
                  WHERE datname=current_database() AND state='active' \
                    AND wait_event='PgSleep' AND query LIKE $1 AND pid<>pg_backend_pid()",
                &[&pattern],
            )
            .expect("pg_stat_activity");
        if let Some(row) = rows.first() {
            let pid: i32 = row.get(0);
            return (i64::from(pid), row.get(1));
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("no backend parked in a {table} statement");
}

/// Kills exactly the recorded owner: the two-argument
/// `pg_terminate_backend(pid, timeout)`, scoped by `(pid, backend_start)`, never
/// the caller and never another database. This is the §12 proof the next call
/// relies on.
fn kill_recorded_owner(schema: &Schema, pid: i64, backend_start: &str) {
    let mut client = schema.client();
    let killed: bool = client
        .query_one(
            "SELECT COALESCE(bool_or(pg_terminate_backend(a.pid, 30000)), false) \
               FROM pg_stat_activity a \
              WHERE a.pid::bigint=$1 AND a.backend_start::text=$2 \
                AND a.datname=current_database() AND a.pid<>pg_backend_pid()",
            &[&pid, &backend_start],
        )
        .expect("pg_terminate_backend")
        .get(0);
    assert!(killed, "the recorded owner backend {pid} was terminated");
}

/// Proves the recorded owner ended: no `pg_stat_activity` row keeps the same
/// `(pid, backend_start)` pair.
fn assert_owner_ended(schema: &Schema, pid: i64, backend_start: &str) {
    let mut client = schema.client();
    for _ in 0..2400 {
        let alive: i64 = client
            .query_one(
                "SELECT count(*) FROM pg_stat_activity \
                  WHERE pid::bigint=$1 AND backend_start::text=$2",
                &[&pid, &backend_start],
            )
            .expect("pg_stat_activity")
            .get(0);
        if alive == 0 {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("the recorded owner backend {pid} did not end");
}

/// The compaction intents a Store's admission table still holds.
fn compaction_intents(handles: &PostgresHandles, schema: &Schema) -> Vec<GcIntent> {
    let catalog = layerfs_history::CatalogId::derive(&schema.binding).expect("catalog identity");
    handles
        .history
        .gc_intents(GcScopeKind::Store, catalog.as_slice())
        .expect("compaction intents")
}

/// Runs one compact on a connection of its own, so the parent can terminate
/// exactly that backend.
fn spawn_round(
    selection: PostgresSelection,
    binding: Vec<u8>,
    op: u8,
    failpoint: Option<CompactFailpoint>,
) -> std::thread::JoinHandle<layerfs_history::HistoryResult<layerfs_persistence::CompactReceipt>> {
    std::thread::spawn(move || {
        let handles =
            PostgresHandles::open_writable(&selection, &binding, [71; 32]).expect("child open");
        handles.compact(&CompactRequest {
            op_id: [op; 16],
            failpoint,
            ..CompactRequest::default()
        })
    })
}

#[test]
fn a_killed_backend_before_the_swap_commit_leaves_the_store_at_g() {
    let Some(url) = url() else { return };
    let schema = Schema::new(&url, "kill_before_commit");
    schema.drop_scratch(41);
    let setup = schema.create();
    let live = filesystem_root(&setup, 1);
    initialize(&setup, 1, live);
    let dead = whole_file(&setup, b"a pack the killed round would collect");
    let before = generation(&setup);
    let totals_before = totals(&schema);

    // Park Phase B's own `UPDATE gc_op` before COMMIT. The kill therefore lands
    // after Phase A built the scratch and before the swap transaction can
    // commit, which is the "killed before COMMIT" arm.
    park_backend(&schema, "gc_op");
    let child = spawn_round(schema.selection.clone(), schema.binding.clone(), 41, None);
    let (pid, start) = wait_for_parked_statement(&schema, "gc_op");
    kill_recorded_owner(&schema, pid, &start);
    let error = child
        .join()
        .expect("the killed round's thread")
        .unwrap_err();
    unpark_backend(&schema, "gc_op");
    assert!(
        matches!(error, layerfs_history::HistoryError::UnknownOutcome),
        "a killed backend loses the reply: {error:?}"
    );

    // Wholly old at `G`: the swap transaction rolled back, so the Store is not
    // mixed and the old totals still hold.
    assert_eq!(db_generation(&schema), before);
    assert_eq!(totals(&schema), totals_before);
    assert!(located(&setup, live), "the retained object survives");
    assert!(located(&setup, dead), "the swap did not commit");
    // The documented leftover of an interrupted round: Phase M's op row keeps
    // the owner, Phase A's scratch schema and the dead set remain.
    let (op_pid, op_start, op_phase) = op_owner(&schema).expect("the op row survives the kill");
    // §11: the live Phase A phase was written before the build, so the last
    // committed phase is `build`; Phase B's `swap` write rolled back.
    assert_eq!((op_pid, op_phase), (pid, "build".to_owned()));
    assert_eq!(op_start, start);
    assert!(
        schema.scratch_exists(41),
        "Phase A's scratch schema remains"
    );
    assert!(schema.scalar("SELECT count(*) FROM {s}.gc_dead") > 0);

    // The owner is provably ended, and the admission intent the killed process
    // never released is still visible: a compaction intent is not an uncertain
    // one, so the resume adopts it instead of reporting `Unresolved`.
    assert_owner_ended(&schema, pid, &start);
    let intents = compaction_intents(&setup, &schema);
    assert_eq!(
        intents.len(),
        1,
        "the killed process never released its intent"
    );
    assert_eq!(intents[0].kind, GcIntentKind::Compaction);
    assert_eq!(intents[0].phase.as_deref(), Some("mark"));
    assert!(
        !intents[0].uncertain,
        "a compaction intent carries no uncertain owner"
    );

    // The next compact proves the owner ended, adopts the `G` leftover and
    // rebuilds once: `G -> G + 1`.
    let resumed = schema.open();
    let receipt = resumed.compact(&request(42)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(receipt.generation_before, before);
    assert_eq!(receipt.generation_after, before + 1);
    assert_settled(&schema, 42);
    assert_eq!(db_generation(&schema), before + 1);
    assert!(
        !located(&resumed, dead),
        "the next round collects the dead pack"
    );
    assert!(
        located(&resumed, live),
        "the retained object survives the resume"
    );
    clean(&schema.open());
}

#[test]
fn a_killed_backend_during_phase_c_resumes_at_g_plus_1() {
    let Some(url) = url() else { return };
    let schema = Schema::new(&url, "kill_phase_c");
    schema.drop_scratch(43);
    let setup = schema.create();
    let live = filesystem_root(&setup, 1);
    initialize(&setup, 1, live);
    let dead = whole_file(&setup, b"a pack the killed cleanup would collect");
    let before = generation(&setup);
    let totals_before = totals(&schema);

    // Park Phase C's `store_totals` settle. The swap transaction already
    // committed `G + 1` before Phase C starts, so the kill lands on the
    // "installed but cleanup unfinished" arm.
    park_backend(&schema, "store_totals");
    let child = spawn_round(schema.selection.clone(), schema.binding.clone(), 43, None);
    let (pid, start) = wait_for_parked_statement(&schema, "store_totals");
    kill_recorded_owner(&schema, pid, &start);
    let error = child
        .join()
        .expect("the killed round's thread")
        .unwrap_err();
    unpark_backend(&schema, "store_totals");
    assert!(
        matches!(error, layerfs_history::HistoryError::UnknownOutcome),
        "a killed backend loses the reply: {error:?}"
    );

    // Wholly new at `G + 1`, but Phase C rolled back: the committed swap has
    // already replaced the payload tables (the dead pack is gone), while the
    // persisted dead history rows, the op row and the old totals remain.
    assert_eq!(db_generation(&schema), before + 1);
    assert_eq!(totals(&schema), totals_before);
    assert!(
        !located(&setup, dead),
        "the committed swap already replaced the payload tables"
    );
    assert!(located(&setup, live));
    let (op_pid, op_start, op_phase) = op_owner(&schema).expect("the op row survives the kill");
    assert_eq!((op_pid, op_phase), (pid, "swap".to_owned()));
    assert_eq!(op_start, start);
    assert!(
        !schema.scratch_exists(43),
        "the swap transaction dropped the scratch before COMMIT"
    );
    assert!(schema.scalar("SELECT count(*) FROM {s}.gc_dead") > 0);
    assert_owner_ended(&schema, pid, &start);
    // A real kill runs no error path: the intent is still registered.
    let intents = compaction_intents(&setup, &schema);
    assert_eq!(
        intents.len(),
        1,
        "the killed process never released its intent"
    );
    assert_eq!(intents[0].kind, GcIntentKind::Compaction);
    assert!(!intents[0].uncertain);

    // The next compact reads the incarnation (`G + 1`) and finishes Phase C,
    // reporting the outcome the lost reply would have reported.
    let resumed = schema.open();
    let receipt = resumed.compact(&request(44)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(receipt.generation_before, before);
    assert_eq!(receipt.generation_after, before + 1);
    assert_settled(&schema, 44);
    assert_eq!(db_generation(&schema), before + 1);
    assert!(
        !located(&resumed, dead),
        "the resumed Phase C collects the dead pack"
    );
    assert!(located(&resumed, live));
    clean(&schema.open());
}

#[test]
fn a_lost_swap_reply_from_an_ended_owner_resumes_at_g_plus_1() {
    let Some(url) = url() else { return };
    let schema = Schema::new(&url, "lost_commit");
    let setup = schema.create();
    let live = filesystem_root(&setup, 1);
    initialize(&setup, 1, live);
    let dead = whole_file(&setup, b"a pack the lost COMMIT would collect");
    let before = generation(&setup);

    // The named failpoint is the deterministic stand-in for the instant the
    // swap's COMMIT reply is lost: the swap committed at `G + 1`, the caller
    // saw `UnknownOutcome`, and its backend is still alive. The child holds its
    // handles open so the parent can end that owner with a real kill.
    let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let selection = schema.selection.clone();
    let binding = schema.binding.clone();
    let child = std::thread::spawn(move || {
        let handles =
            PostgresHandles::open_writable(&selection, &binding, [71; 32]).expect("child open");
        let result = handles.compact(&request_at(45, CompactFailpoint::PostgresAfterSwap));
        started_tx
            .send(())
            .expect("the child reports its lost reply");
        release_rx.recv().expect("the parent releases the child");
        drop(handles);
        result
    });
    started_rx
        .recv_timeout(std::time::Duration::from_secs(60))
        .expect("the failpoint returned");

    // The committed incarnation says the swap happened; the op still names
    // Phase C and the scratch is already gone. The child is still holding its
    // handles open, so this backend is alive and idle: the real kill below
    // proves the owner ended.
    assert_eq!(db_generation(&schema), before + 1);
    let (pid, start, phase) = wait_for_op_owner(&schema);
    assert_eq!(phase, "swap");
    assert!(!schema.scratch_exists(45));
    assert!(schema.scalar("SELECT count(*) FROM {s}.gc_dead") > 0);
    // A simulated lost reply keeps the process alive and releases its intent;
    // the real kill below then proves the owner ended.
    assert!(
        compaction_intents(&setup, &schema).is_empty(),
        "the failpoint path released its own intent"
    );
    kill_recorded_owner(&schema, pid, &start);
    assert_owner_ended(&schema, pid, &start);
    release_tx.send(()).expect("release the lost-reply child");
    let error = child.join().expect("the lost-reply thread").unwrap_err();
    assert!(
        matches!(error, layerfs_history::HistoryError::UnknownOutcome),
        "the failpoint simulates the lost reply: {error:?}"
    );

    // A fresh connection resolves the committed swap: it reads `G + 1` and
    // finishes Phase C without replaying the swap.
    let resumed = schema.open();
    let receipt = resumed.compact(&request(46)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(receipt.generation_before, before);
    assert_eq!(receipt.generation_after, before + 1);
    assert_settled(&schema, 46);
    assert_eq!(db_generation(&schema), before + 1);
    assert!(
        !located(&resumed, dead),
        "the resumed Phase C collects the dead pack"
    );
    assert!(located(&resumed, live));
    clean(&schema.open());
}

// --- AC-44: the privilege half ---------------------------------------------

/// The base URL with `role` as the login user, for either the keyword or the
/// URL form.
fn role_url(url: &str, role: &str) -> String {
    if let Some((scheme, tail)) = url.split_once("://") {
        let authority_end = tail.find('/').unwrap_or(tail.len());
        let authority = &tail[..authority_end];
        let host = authority.rsplit('@').next().unwrap_or(authority);
        format!("{scheme}://{role}@{host}{}", &tail[authority_end..])
    } else {
        let mut parts = Vec::new();
        let mut replaced = false;
        for part in url.split_whitespace() {
            if part.starts_with("user=") {
                parts.push(format!("user={role}"));
                replaced = true;
            } else {
                parts.push(part.to_owned());
            }
        }
        if !replaced {
            parts.push(format!("user={role}"));
        }
        parts.join(" ")
    }
}

/// Serializes the role and database ACL writes of the AC-44 matrix: the
/// tests run in parallel, and two `GRANT`/`REVOKE` statements on the same
/// `pg_database` row can fail with `tuple concurrently updated`. The lock is
/// held only while a fixture writes, never through a compaction.
static ROLE_SETUP: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn role_setup() -> std::sync::MutexGuard<'static, ()> {
    ROLE_SETUP
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A login role of the AC-44 privilege matrix. It owns the Store's tables and
/// sequences, so a swap may drop, move and re-create them, while the schema
/// and database stay the fixture administrator's; each test grants exactly
/// the Store-wide privileges its arm needs. The suite's PostgreSQL server is
/// this test binary's own, so it revokes the PUBLIC database grants once per
/// role: a superuser bypasses them.
struct Role {
    name: String,
    url: String,
    admin_url: String,
    database: String,
}
impl Role {
    fn new(schema: &Schema, tag: &str) -> Self {
        let _setup = role_setup();
        let name = format!("lfs_g54_{tag}_{}", std::process::id());
        let mut admin =
            postgres::Client::connect(&schema.url, postgres::NoTls).expect("role admin");
        let _ = admin.batch_execute(&format!("DROP ROLE IF EXISTS {name}"));
        admin
            .batch_execute(&format!("CREATE ROLE {name} LOGIN"))
            .expect("role creation");
        let database: String = admin
            .query_one("SELECT current_database()", &[])
            .expect("database")
            .get(0);
        admin
            .batch_execute(&format!(
                "REVOKE CREATE, TEMP ON DATABASE {database} FROM PUBLIC"
            ))
            .expect("database public grants");
        admin
            .batch_execute(&format!(
                "GRANT USAGE ON SCHEMA {} TO {name}",
                schema.selection.schema
            ))
            .expect("schema usage");
        let owned: Vec<(String, String)> = admin
            .query(
                "SELECT c.relname::text, c.relkind::text FROM pg_class c \
                 JOIN pg_namespace n ON n.oid = c.relnamespace \
                 WHERE n.nspname = $1 AND (c.relkind IN ('r', 'p') \
                   OR (c.relkind = 'S' AND NOT EXISTS (SELECT 1 FROM pg_depend d \
                        WHERE d.classid = 'pg_class'::regclass AND d.objid = c.oid \
                          AND d.refclassid = 'pg_class'::regclass \
                          AND d.deptype IN ('a', 'i'))))",
                &[&schema.selection.schema],
            )
            .expect("schema relations")
            .iter()
            .map(|row| (row.get(0), row.get(1)))
            .collect();
        for (relation, kind) in owned {
            let relation = relation.replace('"', "\"\"");
            let keyword = if kind == "S" { "SEQUENCE" } else { "TABLE" };
            admin
                .batch_execute(&format!(
                    "ALTER {keyword} \"{}\".\"{relation}\" OWNER TO {name}",
                    schema.selection.schema
                ))
                .expect("relation ownership");
        }
        Self {
            url: role_url(&schema.url, &name),
            name,
            admin_url: schema.url.clone(),
            database,
        }
    }
    fn admin(&self) -> postgres::Client {
        postgres::Client::connect(&self.admin_url, postgres::NoTls).expect("role admin")
    }
    fn grant_schema_create(&self, schema: &Schema) {
        let _setup = role_setup();
        self.admin()
            .batch_execute(&format!(
                "GRANT CREATE ON SCHEMA {} TO {}",
                schema.selection.schema, self.name
            ))
            .expect("schema create");
    }
    fn grant_database(&self, create: bool, temp: bool) {
        let _setup = role_setup();
        let mut sql = String::new();
        if create {
            sql.push_str(&format!(
                "GRANT CREATE ON DATABASE {} TO {};",
                self.database, self.name
            ));
        }
        if temp {
            sql.push_str(&format!(
                "GRANT TEMP ON DATABASE {} TO {};",
                self.database, self.name
            ));
        }
        if !sql.is_empty() {
            self.admin().batch_execute(&sql).expect("database grants");
        }
    }
    fn grant_read_all_stats(&self) {
        let _setup = role_setup();
        self.admin()
            .batch_execute(&format!("GRANT pg_read_all_stats TO {}", self.name))
            .expect("stats grant");
    }
    fn open(&self, schema: &Schema) -> PostgresHandles {
        let selection = PostgresSelection::new(&self.url, schema.selection.schema.clone());
        PostgresHandles::open_writable(&selection, &schema.binding, [71; 32]).expect("role open")
    }
}
impl Drop for Role {
    fn drop(&mut self) {
        let _setup = role_setup();
        if let Ok(mut admin) = postgres::Client::connect(&self.admin_url, postgres::NoTls) {
            let _ = admin.batch_execute(&format!("DROP OWNED BY {}", self.name));
            let _ = admin.batch_execute(&format!("DROP ROLE IF EXISTS {}", self.name));
        }
    }
}

/// A Store with a live root and a wholly dead pack, so an arm that got past
/// the privilege preflight would take the rebuild path and create the scratch
/// schema.
fn seed_rebuild(schema: &Schema) -> (PostgresHandles, ObjectId, ObjectId) {
    let handles = schema.create();
    let live = filesystem_root(&handles, 1);
    initialize(&handles, 1, live);
    let dead = whole_file(&handles, b"a pack a privileged compaction collects");
    (handles, live, dead)
}

#[test]
fn a_role_without_create_on_the_database_is_refused_as_unsupported() {
    let Some(url) = url() else { return };
    let schema = Schema::new(&url, "privilege_database_create");
    let (setup, _live, _dead) = seed_rebuild(&schema);
    let before = generation(&setup);

    let role = Role::new(&schema, "no_database_create");
    role.grant_schema_create(&schema);
    role.grant_database(false, true);
    role.grant_read_all_stats();
    let handles = role.open(&schema);
    let error = handles.compact(&request(61)).unwrap_err();
    assert!(
        matches!(&error, layerfs_history::HistoryError::Unsupported(m) if m.contains("CREATE on the database")),
        "{error:?}"
    );
    assert!(!schema.scratch_exists(61), "no scratch schema was created");
    assert_eq!(schema.count("gc_op"), 0, "no compaction op row");
    assert_eq!(db_generation(&schema), before, "nothing was marked");
}

#[test]
fn a_role_without_create_on_the_store_schema_is_refused_as_unsupported() {
    let Some(url) = url() else { return };
    let schema = Schema::new(&url, "privilege_schema_create");
    let (setup, _live, _dead) = seed_rebuild(&schema);
    let before = generation(&setup);

    let role = Role::new(&schema, "no_schema_create");
    role.grant_database(true, true);
    role.grant_read_all_stats();
    let handles = role.open(&schema);
    let error = handles.compact(&request(62)).unwrap_err();
    assert!(
        matches!(&error, layerfs_history::HistoryError::Unsupported(m) if m.contains("CREATE on the Store schema")),
        "{error:?}"
    );
    assert!(!schema.scratch_exists(62), "no scratch schema was created");
    assert_eq!(schema.count("gc_op"), 0, "no compaction op row");
    assert_eq!(db_generation(&schema), before, "nothing was marked");
}

#[test]
fn a_role_without_temp_is_refused_as_unsupported() {
    let Some(url) = url() else { return };
    let schema = Schema::new(&url, "privilege_temp");
    let (setup, _live, _dead) = seed_rebuild(&schema);
    let before = generation(&setup);

    let role = Role::new(&schema, "no_temp");
    role.grant_schema_create(&schema);
    role.grant_database(true, false);
    role.grant_read_all_stats();
    let handles = role.open(&schema);
    let error = handles.compact(&request(63)).unwrap_err();
    assert!(
        matches!(&error, layerfs_history::HistoryError::Unsupported(m) if m.contains("TEMP on the database")),
        "{error:?}"
    );
    assert!(!schema.scratch_exists(63), "no scratch schema was created");
    assert_eq!(schema.count("gc_op"), 0, "no compaction op row");
    assert_eq!(db_generation(&schema), before, "nothing was marked");
}

#[test]
fn a_role_without_pg_read_all_stats_is_refused_as_unsupported() {
    let Some(url) = url() else { return };
    let schema = Schema::new(&url, "privilege_stats");
    let (setup, _live, _dead) = seed_rebuild(&schema);
    let before = generation(&setup);

    let role = Role::new(&schema, "no_stats");
    role.grant_schema_create(&schema);
    role.grant_database(true, true);
    let handles = role.open(&schema);
    let error = handles.compact(&request(64)).unwrap_err();
    assert!(
        matches!(&error, layerfs_history::HistoryError::Unsupported(m) if m.contains("pg_read_all_stats")),
        "{error:?}"
    );
    assert!(!schema.scratch_exists(64), "no scratch schema was created");
    assert_eq!(schema.count("gc_op"), 0, "no compaction op row");
    assert_eq!(db_generation(&schema), before, "nothing was marked");
}

#[test]
fn a_fully_privileged_role_compacts() {
    let Some(url) = url() else { return };
    let schema = Schema::new(&url, "privilege_full");
    let (setup, live, dead) = seed_rebuild(&schema);
    let before = generation(&setup);

    let role = Role::new(&schema, "full");
    role.grant_schema_create(&schema);
    role.grant_database(true, true);
    role.grant_read_all_stats();
    let handles = role.open(&schema);
    let receipt = handles.compact(&request(65)).unwrap();
    assert_eq!(receipt.path, CompactPath::Rebuild);
    assert_eq!(receipt.generation_before, before);
    assert_eq!(receipt.generation_after, before + 1);
    assert_settled(&schema, 65);
    assert_eq!(db_generation(&schema), before + 1);
    assert!(
        !located(&handles, dead),
        "the rebuild collected the dead pack"
    );
    assert!(
        located(&handles, live),
        "the retained root survives the role's rebuild"
    );
    drop(handles);
    clean(&schema.open());
}
