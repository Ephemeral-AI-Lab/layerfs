//! T10.5: the PostgreSQL branch of `store.status`, through the control call.
//!
//! `store.status` of a PostgreSQL Store reads the Store-side `gc_op` row with
//! its compaction `gc_intent` in one status query, projects the operation
//! `{id, name, phase, started, uncertain}`, and separates a live owner
//! (`busy`) from a leftover (`unresolved`); `phases` is PostgreSQL's §8 order
//! `mark`, `build`, `swap`, `cleanup`. A Store with no row — never compacted,
//! or a run that settled and removed its row — is idle, with `op: null` and
//! `phases: null`.
//!
//! This is the control-level end-to-end case T10.5 was missing: the
//! persistence suite proves the reader, and this test drives the call the SDK
//! and CLI actually reach (`layerfs_store::sqlite::family::status`, the
//! kind-neutral `store.status` route) against a real PostgreSQL Store. The
//! rows a compaction leaves are written directly, so the test proves the
//! projection, not a failpoint: a live owner is a real non-idle backend
//! recorded in `pg_stat_activity`.
//!
//! Needs a writable PostgreSQL server: set `LAYERFS_TEST_POSTGRES_URL` to a
//! libpq connection string or URL (docs/architecture/engine/store/testing.md).
//! The test creates and drops its own database, because `store.status` reads
//! the one schema a 2.0 PostgreSQL Store uses (`layerfs`); a shared schema
//! would collide with another run. Without the variable it reports NOT_RUN.
use layerfs_bridge::control::SdkValue;
use layerfs_history::HistoryCatalogConfig;
use layerfs_home::home;
use layerfs_home::names::StoreName;
use layerfs_persistence::{PostgresHandles, PostgresSelection};
use layerfs_storage::StoragePolicy;
use layerfs_store::record::StoreRecord;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// The §8 phases `store.status` names for a PostgreSQL compaction, in order.
const PHASES: [&str; 4] = ["mark", "build", "swap", "cleanup"];

const BINDING: &[u8] = b"t105-control-postgresql-status";

static NEXT: AtomicU64 = AtomicU64::new(0);

fn url() -> Option<String> {
    let url = std::env::var("LAYERFS_TEST_POSTGRES_URL").ok();
    if url.is_none() {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
    }
    url
}

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-t105-control-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The URL of `database` on the same server as `source`: a URL's path is
/// replaced; a libpq key/value string gains a final `dbname`, which wins.
fn database_url(source: &str, database: &str) -> String {
    if source.starts_with("postgres://") || source.starts_with("postgresql://") {
        let (base, query) = match source.split_once('?') {
            Some((base, query)) => (base, format!("?{query}")),
            None => (source, String::new()),
        };
        let scheme = base.find("://").expect("a PostgreSQL URL") + 3;
        match base[scheme..].find('/').map(|at| scheme + at) {
            Some(at) => format!("{}/{database}{query}", &base[..at]),
            None => format!("{base}/{database}{query}"),
        }
    } else {
        format!("{source} dbname={database}")
    }
}

/// One disposable database: `store.status` reads the fixed `layerfs` schema,
/// so a test run needs a database nothing else uses.
struct Database {
    source: String,
    name: String,
}

impl Database {
    fn create(source: &str) -> Self {
        let name = format!(
            "lfs_t105_ctl_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let mut admin = postgres::Client::connect(source, postgres::NoTls).unwrap();
        admin
            .batch_execute(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"))
            .unwrap();
        admin
            .batch_execute(&format!("CREATE DATABASE {name}"))
            .unwrap();
        Self {
            source: source.to_owned(),
            name,
        }
    }

    fn url(&self) -> String {
        database_url(&self.source, &self.name)
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        if let Ok(mut admin) = postgres::Client::connect(&self.source, postgres::NoTls) {
            let _ = admin.batch_execute(&format!(
                "DROP DATABASE IF EXISTS {} WITH (FORCE)",
                self.name
            ));
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn op_id(byte: u8) -> String {
    format!("op-{}", hex(&[byte; 16]))
}

fn text(value: &SdkValue) -> &str {
    match value {
        SdkValue::Text(text) => text,
        other => panic!("not text: {other:?}"),
    }
}

fn texts(value: &SdkValue) -> Vec<String> {
    match value {
        SdkValue::List(items) => items
            .iter()
            .map(|item| {
                if let SdkValue::Text(text) = item {
                    text.clone()
                } else {
                    panic!("not text: {item:?}");
                }
            })
            .collect(),
        other => panic!("not a list: {other:?}"),
    }
}

/// What a compaction leaves in `gc_op`/`gc_intent`; `pid` and
/// `backend_start` name the owner the §9 rule tests for liveness.
struct OpRow<'a> {
    op_id: [u8; 16],
    phase: &'a str,
    generation: i64,
    started_ms: i64,
    pid: i64,
    backend_start: &'a str,
}

fn insert(admin: &mut postgres::Client, row: &OpRow<'_>) {
    let artifact = format!("lfs_gc_{}", hex(&row.op_id));
    admin
        .execute(
            "INSERT INTO layerfs.gc_op \
             (id,generation,phase,scratch_schema,mark_serial,mark_pack_id,mark_digest,\
              started_ms,artifact,pid,backend_start) \
             VALUES (1,$1,$2,$3,$4,$5,$6,$7,$3,$8,$9)",
            &[
                &row.generation,
                &row.phase,
                &artifact,
                &11i64,
                &12i64,
                &vec![9u8; 32],
                &row.started_ms,
                &row.pid,
                &row.backend_start,
            ],
        )
        .unwrap();
    admin
        .execute(
            "INSERT INTO layerfs.gc_intent \
             (op_id,kind,scope_kind,owner_runtime,owner_boot,state,uncertain,phase,\
              generation,artifact,updated_ms) \
             VALUES ($1,'compaction','store',$2,$3,'live',FALSE,$4,$5,$6,$7)",
            &[
                &row.op_id.to_vec(),
                &"runtime-t105",
                &"boot-t105",
                &row.phase,
                &row.generation,
                &artifact,
                &1_700_000_000_000i64,
            ],
        )
        .unwrap();
}

fn clear(admin: &mut postgres::Client) {
    admin
        .batch_execute(
            "DELETE FROM layerfs.gc_op; \
             DELETE FROM layerfs.gc_intent WHERE kind='compaction'",
        )
        .unwrap();
}

/// Waits until the recorded owner backend is gone from `pg_stat_activity`,
/// as the persistence fixture does.
fn wait_gone(admin: &mut postgres::Client, pid: i32, start: &str) {
    for _ in 0..400 {
        let live: bool = admin
            .query_one(
                "SELECT EXISTS (SELECT 1 FROM pg_stat_activity \
                  WHERE pid=$1 AND backend_start::text=$2)",
                &[&pid, &start],
            )
            .unwrap()
            .get(0);
        if !live {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("the recorded owner backend never ended");
}

/// The real control entry point: `store.status` for the Store in `home`.
fn status(home: &Path) -> SdkValue {
    let name = StoreName::parse("pgs").unwrap();
    layerfs_store::sqlite::family::status(home, &name).unwrap()
}

#[test]
fn postgresql_store_status_projects_busy_leftover_and_idle() {
    let Some(source) = url() else {
        return;
    };
    let database = Database::create(&source);
    let url = database.url();
    let handles = PostgresHandles::create(
        &PostgresSelection::new(&url, "layerfs"),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: [71; 32],
            incarnation: 7,
        },
    )
    .unwrap();

    let home = tempdir("home");
    home::ensure(&home).unwrap();
    // `store.connect` records exactly this shape; `store.status` reads it.
    StoreRecord::connected("pgs", &url, "layerfs", "off", None)
        .write(&home)
        .unwrap();
    let mut admin = postgres::Client::connect(&url, postgres::NoTls).unwrap();

    // Idle: no compaction row, so no op and no phases.
    let idle = status(&home);
    assert_eq!(idle.get_text("name"), Some("pgs"));
    assert_eq!(idle.get_text("kind"), Some("postgresql"));
    assert_eq!(idle.get_text("state"), Some("idle"));
    assert_eq!(
        idle.get("op"),
        Some(&SdkValue::Null),
        "an idle Store has no op"
    );
    assert_eq!(
        idle.get("phases"),
        Some(&SdkValue::Null),
        "an idle Store has no phases"
    );
    assert!(
        matches!(idle.get("server"), Some(SdkValue::Map(_))),
        "the server status is reported beside the op"
    );

    // A live owner: an open, non-idle backend is what the §9 dead-owner rule
    // treats as still running the op.
    let mut owner = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    owner.batch_execute("BEGIN; SELECT 1").unwrap();
    let (pid, start): (i32, String) = {
        let row = owner
            .query_one(
                "SELECT pg_backend_pid(),backend_start::text FROM pg_stat_activity \
                  WHERE pid=pg_backend_pid()",
                &[],
            )
            .unwrap();
        (row.get(0), row.get(1))
    };
    let started_ms = 1_700_000_000_000i64;
    let live = OpRow {
        op_id: [0xab; 16],
        phase: "build",
        generation: 7,
        started_ms,
        pid: i64::from(pid),
        backend_start: &start,
    };
    insert(&mut admin, &live);

    let capture = layerfs_persistence::compact_metrics::CompactMetricsCapture::start();
    let busy = status(&home);
    assert_eq!(busy.get_text("state"), Some("busy"));
    let op = busy.get("op").unwrap();
    assert_eq!(
        op,
        &SdkValue::map([
            ("id", SdkValue::text(op_id(0xab))),
            ("name", SdkValue::text("store.compact")),
            ("phase", SdkValue::text("build")),
            ("started", SdkValue::Int(started_ms)),
            ("uncertain", SdkValue::Bool(true)),
        ]),
        "the op is exactly the five §11 fields"
    );
    assert_eq!(texts(busy.get("phases").unwrap()), PHASES);
    assert!(
        op.get("state").is_none() && op.get("phases").is_none(),
        "state and phases sit beside the op"
    );

    // A gone owner leaves the same row: uncertain, and now unresolved.
    owner.close().unwrap();
    wait_gone(&mut admin, pid, &start);
    let leftover = status(&home);
    assert_eq!(leftover.get_text("state"), Some("unresolved"));
    let op = leftover.get("op").unwrap();
    assert_eq!(text(op.get("id").unwrap()), op_id(0xab));
    assert_eq!(op.get_text("name"), Some("store.compact"));
    assert_eq!(op.get_text("phase"), Some("build"));
    assert_eq!(op.get_int("started"), Some(started_ms));
    assert_eq!(op.get("uncertain"), Some(&SdkValue::Bool(true)));
    assert_eq!(texts(leftover.get("phases").unwrap()), PHASES);

    // The two observed states emit the bounded `compact.status{state}` sample
    // once each; idle emits none.
    let metrics = capture.finish();
    let sample = |state: &str| {
        metrics
            .samples()
            .iter()
            .find(|sample| sample.name == "compact.status" && sample.label("state") == Some(state))
            .map_or(0, |sample| sample.value)
    };
    assert_eq!(sample("busy"), 1);
    assert_eq!(sample("unresolved"), 1);
    assert_eq!(sample("idle"), 0, "an idle status is not a compaction op");

    // Every recorded phase is projected under its §8 name; a dead owner is
    // an unresolved leftover for each.
    for (index, phase) in PHASES.iter().enumerate() {
        clear(&mut admin);
        let byte = u8::try_from(index).unwrap();
        insert(
            &mut admin,
            &OpRow {
                op_id: [byte; 16],
                phase,
                generation: 7,
                started_ms,
                pid: 99_999_999,
                backend_start: "2000-01-01 00:00:00+00",
            },
        );
        let projected = status(&home);
        assert_eq!(projected.get_text("state"), Some("unresolved"));
        assert_eq!(
            projected.get("op").unwrap().get_text("phase"),
            Some(*phase),
            "{phase}: the row records the phase"
        );
        let expected_id = op_id(byte);
        assert_eq!(
            projected.get("op").unwrap().get_text("id"),
            Some(expected_id.as_str())
        );
        assert_eq!(texts(projected.get("phases").unwrap()), PHASES);
    }

    // A settled run removes the row: idle again, with no op and no phases.
    clear(&mut admin);
    let idle = status(&home);
    assert_eq!(idle.get_text("state"), Some("idle"));
    assert_eq!(idle.get("op"), Some(&SdkValue::Null));
    assert_eq!(idle.get("phases"), Some(&SdkValue::Null));

    drop(admin);
    drop(handles);
    std::fs::remove_dir_all(home).unwrap();
}
