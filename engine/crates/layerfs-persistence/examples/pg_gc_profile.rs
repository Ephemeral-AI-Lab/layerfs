//! A PostgreSQL garbage-collection fixture and one counted `store.compact`
//! or mark over it: the PostgreSQL counterpart of `gc_bench` and
//! `gc_profile`, which are SQLite only. A diagnostic, not a measurement
//! harness: it writes no receipt and claims nothing.
//!
//! ```text
//! LAYERFS_TEST_POSTGRES_URL="host=... dbname=postgres ..." \
//! pg_gc_profile build <db>            # creates database <db> with one Store
//! pg_gc_profile run <template> <db>   # clones <template> into <db>, compacts
//! pg_gc_profile mark <template> <db>  # clones, runs the mark only
//! pg_gc_profile seed <db> <rows>      # adds dead file_digest rows
//! pg_gc_profile sizes <db>
//! pg_gc_profile drop <db>
//! ```
//!
//! `build` reads its shape from `PROBE_STACKS` (4), `PROBE_DEAD_STACKS` (0),
//! `PROBE_LAYERS` (16), `PROBE_GROUP` (48 objects per Layer, at most 99),
//! `PROBE_OBJECT_BYTES` (1024), `PROBE_DEAD_OBJECTS` (0), `PROBE_DIGESTS` (0)
//! and `PROBE_SHARED_DEAD_DIGESTS` (0). `run` and `mark` work on a clone, so
//! one built database serves any number of runs; they print wall time, the
//! compaction phases and this session's SQL work (statements, round trips,
//! rows). `PROBE_LOG_STATEMENTS` turns on the server's per-statement log for
//! the clone, `PROBE_KEEP` keeps the clone, and `PROBE_FAILPOINT` stops a run
//! after `mark`, `build`, `swap` or `cleanup`.

use layerfs_content::filesystem::root::profile_id;
use layerfs_content::filesystem::tree::attribute::PortableMetadata;
use layerfs_content::filesystem::tree::directory::{encode_directory_page, DirectoryPage};
use layerfs_content::filesystem::tree::inode::{encode_inode_page, InodePage};
use layerfs_content::filesystem::{scope_for_seed, FilesystemRoot, InodeScope};
use layerfs_content::inode_leaf::{InodeKind, InodeMetadata, InodeValue};
use layerfs_content::{encode_whole_file_payload, FinalizedObject, ObjectId, ObjectRole};
use layerfs_history::{
    AddLayerOutcome, AddLayerRequest, BranchId, CommitMeta, CommitOutcome, CommitRequest,
    CommitStatistics, CommitVia, FileDigest, ForkRequest, ForkSource, HistoryCatalog,
    HistoryCatalogConfig, HistoryName, LayerStackId, StackInitialization,
};
use layerfs_persistence::gc::metrics::CompactMetricsCapture;
use layerfs_persistence::{
    CompactFailpoint, CompactRequest, PostgresHandles, PostgresSelection, SqlWork,
    SynchronousCommit,
};
use layerfs_storage::{Storage, StoragePolicy};
use std::time::Instant;

const BINDING: &[u8] = b"pg-gc-profile-v1";
const CURSOR_KEY: [u8; 32] = [45; 32];
const SCHEMA: &str = "probe_store";
const BATCH: usize = 100;

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn base_url() -> String {
    std::env::var("LAYERFS_TEST_POSTGRES_URL").expect("LAYERFS_TEST_POSTGRES_URL")
}

/// The base URL with its `dbname=` replaced.
fn url_for(db: &str) -> String {
    base_url()
        .split_whitespace()
        .map(|part| {
            if part.starts_with("dbname=") {
                format!("dbname={db}")
            } else {
                part.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn admin() -> postgres::Client {
    postgres::Client::connect(&base_url(), postgres::NoTls).expect("admin connection")
}

fn client(db: &str) -> postgres::Client {
    postgres::Client::connect(&url_for(db), postgres::NoTls).expect("database connection")
}

fn selection(db: &str, sync: SynchronousCommit) -> PostgresSelection {
    PostgresSelection::new(url_for(db), SCHEMA).with_synchronous_commit(sync)
}

fn payload(seed: u64, len: usize) -> Vec<u8> {
    let mut out = vec![0u8; len];
    let head = len.min(8);
    out[..head].copy_from_slice(&seed.to_le_bytes()[..head]);
    let mut state = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    for chunk in out[head..].chunks_exact_mut(8) {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        chunk.copy_from_slice(&state.to_le_bytes());
    }
    out
}

fn whole_file(bytes: &[u8]) -> FinalizedObject {
    FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(bytes).expect("whole-file payload"),
    )
    .expect("finalized object")
}

fn save_batch(handles: &PostgresHandles, objects: Vec<FinalizedObject>) -> Vec<ObjectId> {
    let storage = Storage::new(handles.storage.clone()).expect("storage");
    let save = storage.begin_save().expect("save");
    let mut ids = Vec::with_capacity(objects.len());
    for object in objects {
        ids.push(object.id());
        save.accept(object).expect("accept");
    }
    save.finish().expect("save finish");
    ids
}

fn scope_for_stack(stack: u64) -> InodeScope {
    let mut seed = [0u8; 32];
    seed[0] = 1;
    seed[1..9].copy_from_slice(&stack.to_le_bytes());
    scope_for_seed(seed)
}

/// One Layer's tree and its content objects in one Save.
fn publish_layer(
    handles: &PostgresHandles,
    scope: InodeScope,
    serial: u64,
    files: Vec<FinalizedObject>,
) -> ObjectId {
    let roots: Vec<ObjectId> = files.iter().map(FinalizedObject::id).collect();
    let directory = FinalizedObject::new(
        ObjectRole::DirectoryLeaf,
        encode_directory_page(&DirectoryPage::Leaf {
            entries: Vec::new(),
        })
        .expect("directory"),
    )
    .expect("directory leaf");
    let directory_id = directory.id();
    let mut entries = Vec::with_capacity(roots.len() + 1);
    let metadata = |mode| {
        InodeMetadata::Inline(PortableMetadata {
            mode,
            mtime_seconds: i64::try_from(serial).unwrap_or(0),
            mtime_nanoseconds: 0,
        })
    };
    entries.push((
        1,
        InodeValue {
            kind: InodeKind::Directory,
            namespace_ref_count: 0,
            content_root: directory_id,
            metadata_root: metadata(0o755),
        },
    ));
    for (offset, root) in roots.iter().enumerate() {
        entries.push((
            u64::try_from(offset + 2).expect("inode serial"),
            InodeValue {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 1,
                content_root: *root,
                metadata_root: metadata(0o644),
            },
        ));
    }
    let mut references = Vec::with_capacity(roots.len() + 1);
    references.push(directory_id);
    references.extend(roots.iter().copied());
    let leaf = FinalizedObject::new(
        ObjectRole::InodeLeaf,
        encode_inode_page(&InodePage::Leaf { entries }).expect("inode page"),
    )
    .expect("inode leaf")
    .with_references(references);
    let leaf_id = leaf.id();
    let root = FilesystemRoot::new(profile_id(), scope, 1, leaf_id).expect("filesystem root");
    let root_object = FinalizedObject::new(
        ObjectRole::FilesystemRoot,
        root.encode().expect("root encode"),
    )
    .expect("filesystem root object")
    .with_references(vec![leaf_id]);
    let root_id = root_object.id();
    let mut objects = files;
    objects.extend([directory, leaf, root_object]);
    save_batch(handles, objects);
    root_id
}

fn stack_id(index: u64) -> LayerStackId {
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&index.to_le_bytes());
    bytes[8..].copy_from_slice(b"pgprobe0");
    LayerStackId::from_authority(bytes)
}

fn branch_id(index: u64) -> BranchId {
    let mut bytes = [0u8; 16];
    bytes[0] = 2;
    bytes[1..9].copy_from_slice(&index.to_le_bytes());
    BranchId::from_authority(bytes)
}

fn initialize_project(
    handles: &PostgresHandles,
    index: u64,
    scope: InodeScope,
    genesis_root: ObjectId,
) -> BranchId {
    let stack = stack_id(index);
    handles
        .history
        .initialize_layerstack(&StackInitialization {
            stack,
            name: HistoryName::new(&format!("probe-{index:07}")).expect("name"),
            scope: scope.object(),
            profile: profile_id(),
            genesis_root,
            logical: 0,
        })
        .expect("layerstack initialization");
    let branch = branch_id(index);
    let record = handles
        .history
        .layer_stack(stack)
        .expect("stack read")
        .expect("stack exists");
    handles
        .history
        .fork(&ForkRequest {
            stack,
            branch,
            name: HistoryName::new(&format!("probe-main-{index:07}")).expect("name"),
            source: ForkSource::Layer(record.head_layer),
        })
        .expect("branch fork");
    branch
}

fn commit_and_publish(
    handles: &PostgresHandles,
    stack: LayerStackId,
    branch: BranchId,
    scope: InodeScope,
    candidate_root: ObjectId,
    logical: u64,
) {
    let snapshot = handles
        .history
        .branch(branch)
        .expect("branch read")
        .expect("branch");
    let expected_root = match snapshot.head_commit {
        Some(head) => {
            handles
                .history
                .commit(head)
                .expect("head read")
                .expect("head commit")
                .root
        }
        None => {
            handles
                .history
                .layer(snapshot.base_layer)
                .expect("base layer read")
                .expect("base layer")
                .root
        }
    };
    let meta = CommitMeta {
        statistics: CommitStatistics {
            logical,
            ..CommitStatistics::ZERO
        },
        ..CommitMeta::new(CommitVia::Files)
    };
    match handles
        .history
        .commit_branch(&CommitRequest {
            branch,
            expected_head: snapshot.head_commit,
            expected_base: snapshot.base_layer,
            expected_root,
            candidate_root,
            profile: profile_id(),
            scope: scope.object(),
            second_parent: None,
            meta,
            force: false,
            required_head: None,
        })
        .expect("commit")
    {
        CommitOutcome::Committed { .. } => {}
        other => panic!("commit: {other:?}"),
    }
    // PROBE_CHAIN: the Commits stay on the Branch, unpublished, so the Branch
    // head is the only way to reach them and the chain is as deep as it is long.
    if env_u64("PROBE_CHAIN", 0) != 0 {
        return;
    }
    let branch_record = handles
        .history
        .branch(branch)
        .expect("branch read")
        .expect("branch");
    let stack_record = handles
        .history
        .layer_stack(stack)
        .expect("stack read")
        .expect("stack");
    match handles
        .history
        .add_layer(&AddLayerRequest {
            stack,
            branch,
            commit: branch_record.head_commit.expect("a committed Branch"),
            expected_stack_head: stack_record.head_layer,
            expected_branch_base: branch_record.base_layer,
        })
        .expect("add layer")
    {
        AddLayerOutcome::Added { .. } => {}
        other => panic!("add layer: {other:?}"),
    }
}

fn build(db: &str) {
    let live_stacks = env_u64("PROBE_STACKS", 4);
    let dead_stacks = env_u64("PROBE_DEAD_STACKS", 0);
    let layers = env_u64("PROBE_LAYERS", 16);
    let group = env_u64("PROBE_GROUP", 48).min(99);
    let object_bytes = env_u64("PROBE_OBJECT_BYTES", 1024) as usize;
    let dead_objects = env_u64("PROBE_DEAD_OBJECTS", 0);
    let digests = env_u64("PROBE_DIGESTS", 0) != 0;
    // Dead digest rows that share one dead root (lead 7).
    let shared_dead_digests = env_u64("PROBE_SHARED_DEAD_DIGESTS", 0);

    admin()
        .batch_execute(&format!("CREATE DATABASE {db}"))
        .expect("create database");
    let started = Instant::now();
    let handles = PostgresHandles::create(
        &selection(db, SynchronousCommit::Off),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: CURSOR_KEY,
            incarnation: 44,
        },
    )
    .expect("Store creation");

    let mut seed = 0u64;
    let mut digest_rows = 0u64;
    for stack_index in 0..(live_stacks + dead_stacks) {
        let scope = scope_for_stack(stack_index);
        let stack = stack_id(stack_index);
        let mut branch = BranchId::from_authority([0; 16]);
        for layer in 0..layers {
            let mut files = Vec::with_capacity(group as usize);
            let mut rows = Vec::new();
            for offset in 0..group {
                let bytes = payload(seed + offset, object_bytes);
                let object = whole_file(&bytes);
                if digests {
                    rows.push(FileDigest {
                        root: *object.id().as_bytes(),
                        length: bytes.len() as u64,
                        digest: *blake3::hash(&bytes).as_bytes(),
                    });
                }
                files.push(object);
            }
            seed += group;
            let root = publish_layer(&handles, scope, layer, files);
            if !rows.is_empty() {
                digest_rows += rows.len() as u64;
                handles
                    .history
                    .record_file_digests(&rows)
                    .expect("digest rows");
            }
            if layer == 0 {
                branch = initialize_project(&handles, stack_index, scope, root);
            } else {
                commit_and_publish(
                    &handles,
                    stack,
                    branch,
                    scope,
                    root,
                    group * object_bytes as u64,
                );
            }
        }
        if stack_index >= live_stacks {
            handles
                .history
                .mark_stack_deleted(stack, 2_000)
                .expect("stack deletion");
        }
        eprintln!(
            "probe: stack {}/{} built ({:.1}s)",
            stack_index + 1,
            live_stacks + dead_stacks,
            started.elapsed().as_secs_f64()
        );
    }
    let mut left = dead_objects;
    let mut dead_seed = 1u64 << 40;
    while left > 0 {
        let take = left.min(BATCH as u64);
        let objects = (0..take)
            .map(|offset| whole_file(&payload(dead_seed + offset, object_bytes)))
            .collect();
        save_batch(&handles, objects);
        dead_seed += take;
        left -= take;
    }
    if shared_dead_digests > 0 {
        let dead = whole_file(&payload(1u64 << 41, object_bytes));
        let root = *dead.id().as_bytes();
        save_batch(&handles, vec![dead]);
        let rows: Vec<FileDigest> = (0..shared_dead_digests)
            .map(|index| FileDigest {
                root,
                length: object_bytes as u64 + index,
                digest: *blake3::hash(&index.to_le_bytes()).as_bytes(),
            })
            .collect();
        handles
            .history
            .record_file_digests(&rows)
            .expect("shared dead digest rows");
        digest_rows += rows.len() as u64;
    }
    drop(handles);
    let mut db_client = client(db);
    // VACUUM refuses a transaction block, and a multi-statement simple query
    // is one implicit transaction: each statement goes alone.
    db_client.batch_execute("CHECKPOINT").expect("checkpoint");
    db_client
        .batch_execute("VACUUM ANALYZE")
        .expect("vacuum analyze");
    println!(
        "built db={db} seconds={:.1} live_stacks={live_stacks} dead_stacks={dead_stacks} \
layers={layers} group={group} object_bytes={object_bytes} dead_objects={dead_objects} \
digest_rows={digest_rows}",
        started.elapsed().as_secs_f64()
    );
    sizes(db);
}

fn sizes(db: &str) {
    let mut c = client(db);
    let row = c
        .query_one(
            &format!(
                "SELECT (SELECT count(*) FROM {s}.pack),(SELECT COALESCE(sum(length),0)::bigint FROM {s}.pack),\
                 (SELECT count(*) FROM {s}.object_location),(SELECT count(*) FROM {s}.\"commit\"),\
                 (SELECT count(*) FROM {s}.layer),(SELECT count(*) FROM {s}.file_digest),\
                 (SELECT count(*) FROM {s}.content_signature),pg_database_size(current_database())",
                s = SCHEMA
            ),
            &[],
        )
        .expect("sizes");
    println!(
        "sizes db={db} packs={} pack_bytes={} objects={} commits={} layers={} digests={} \
signatures={} database_bytes={}",
        row.get::<_, i64>(0),
        row.get::<_, i64>(1),
        row.get::<_, i64>(2),
        row.get::<_, i64>(3),
        row.get::<_, i64>(4),
        row.get::<_, i64>(5),
        row.get::<_, i64>(6),
        row.get::<_, i64>(7),
    );
}

fn clone_db(template: &str, db: &str) {
    let mut admin = admin();
    admin
        .batch_execute(&format!("DROP DATABASE IF EXISTS {db}"))
        .expect("drop work database");
    admin
        .batch_execute(&format!("CREATE DATABASE {db} TEMPLATE {template}"))
        .expect("clone database");
    // Per-statement server timing for the clone only (new sessions pick it up).
    if std::env::var_os("PROBE_LOG_STATEMENTS").is_some() {
        admin
            .batch_execute(&format!(
                "ALTER DATABASE {db} SET log_min_duration_statement = 0"
            ))
            .expect("statement logging");
    }
}

fn print_work(label: &str, work: &SqlWork) {
    println!(
        "{label} statements={} round_trips={} prepares={} transactions={} commits={} \
returned_rows={} bound_bytes={} statement_ms={:.1} transaction_ms={:.1}",
        work.statements,
        work.round_trips,
        work.prepares,
        work.transactions,
        work.commits,
        work.returned_rows,
        work.bound_bytes,
        work.statement_ns as f64 / 1e6,
        work.transaction_ns as f64 / 1e6,
    );
}

fn load() -> String {
    std::process::Command::new("uptime")
        .output()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .unwrap_or_default()
}

fn failpoint() -> Option<CompactFailpoint> {
    match std::env::var("PROBE_FAILPOINT").ok().as_deref() {
        Some("mark") => Some(CompactFailpoint::PostgresAfterMark),
        Some("build") => Some(CompactFailpoint::PostgresAfterBuild),
        Some("swap") => Some(CompactFailpoint::PostgresAfterSwap),
        Some("cleanup") => Some(CompactFailpoint::PostgresAfterCleanup),
        _ => None,
    }
}

fn run(template: &str, db: &str) {
    clone_db(template, db);
    sizes(db);
    let handles = PostgresHandles::open_writable(
        &selection(db, SynchronousCommit::Local),
        BINDING,
        CURSOR_KEY,
    )
    .expect("Store open");
    println!("load_before: {}", load());
    let before = handles.diagnostics().expect("diagnostics");
    let capture = CompactMetricsCapture::start();
    let started = Instant::now();
    let result = handles.compact(&CompactRequest {
        op_id: [0x51; 16],
        failpoint: failpoint(),
        ..CompactRequest::default()
    });
    let wall = started.elapsed();
    let metrics = capture.finish();
    let work = handles.diagnostics().expect("diagnostics").since(&before);
    println!("compact_wall_ms={:.1}", wall.as_secs_f64() * 1e3);
    match &result {
        Ok(receipt) => println!(
            "receipt path={:?} generation={}->{} dead_bytes_bound={} physical_shrink={} \
peak_temp_bytes={}",
            receipt.path,
            receipt.generation_before,
            receipt.generation_after,
            receipt.dead_bytes_bound,
            receipt.physical_shrink,
            receipt.peak_temp_bytes
        ),
        Err(error) => println!("compact_error={error:?}"),
    }
    for sample in metrics.samples() {
        if sample.name == "compact.phase.duration_ns" && sample.label("statistic") == Some("sum") {
            println!(
                "phase {} wall_ms={:.1}",
                sample.label("phase").unwrap_or("?"),
                sample.value as f64 / 1e6
            );
        }
    }
    print_work("total", &work);
    println!("load_after: {}", load());
    drop(handles);
    if std::env::var_os("PROBE_KEEP").is_none() {
        sizes(db);
        admin()
            .batch_execute(&format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)"))
            .expect("drop work database");
    }
}

fn mark(template: &str, db: &str) {
    clone_db(template, db);
    let handles = PostgresHandles::open_writable(
        &selection(db, SynchronousCommit::Local),
        BINDING,
        CURSOR_KEY,
    )
    .expect("Store open");
    println!("load_before: {}", load());
    let before = handles.diagnostics().expect("diagnostics");
    let started = Instant::now();
    let mark = handles.history.mark().expect("mark");
    let wall = started.elapsed();
    let work = handles.diagnostics().expect("diagnostics").since(&before);
    println!(
        "mark_wall_ms={:.1} objects={} packs={} history_nodes={} mark_bytes={}",
        wall.as_secs_f64() * 1e3,
        mark.objects().len(),
        mark.objects().packs().len(),
        mark.history().len(),
        mark.mark_bytes()
    );
    print_work("mark", &work);
    drop(handles);
    admin()
        .batch_execute(&format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)"))
        .expect("drop work database");
}

/// Adds `rows` file_digest rows whose roots no object carries: dead digests
/// without the cost of saving that many objects.
fn seed(db: &str, rows: u64) {
    let mut db_client = client(db);
    db_client
        .batch_execute(&format!(
            "INSERT INTO probe_store.file_digest(digest,length,root) \
             SELECT sha256(('d'||i)::bytea),i,sha256(('r'||i)::bytea) FROM generate_series(1,{rows}) i"
        ))
        .expect("seed digests");
    db_client.batch_execute("CHECKPOINT").expect("checkpoint");
    db_client
        .batch_execute("VACUUM ANALYZE")
        .expect("vacuum analyze");
    sizes(db);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["build", db] => build(db),
        ["run", template, db] => run(template, db),
        ["mark", template, db] => mark(template, db),
        ["sizes", db] => sizes(db),
        ["seed", db, rows] => seed(db, rows.parse().expect("rows")),
        ["drop", db] => admin()
            .batch_execute(&format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)"))
            .expect("drop database"),
        _ => panic!("usage: pg_gc_profile build|run|mark|seed|sizes|drop ..."),
    }
}
