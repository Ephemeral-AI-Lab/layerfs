//! T10.5: the PostgreSQL branch of a live read (`store get --with live`,
//! call surface 2.0; it was `store.status`), through the control call.
//!
//! A live read of a PostgreSQL Store reads the Store-side `gc_op` row with
//! its compaction `gc_intent` in one status query and separates a live
//! owner (`busy`, with `op {name, phase, started_at}`) from a leftover
//! (`broken`, `unresolved`, fixed by the next `store compact`); the `live`
//! section's `phases` is PostgreSQL's §8 order `mark`, `build`, `swap`,
//! `cleanup`. A Store with no row — never compacted, or a run that settled
//! and removed its row — is ready, with `phases: null`.
//!
//! This is the control-level end-to-end case T10.5 was missing: the
//! persistence suite proves the reader, and this test drives the call the SDK
//! and CLI actually reach (`layerfs_store::get::get`, the kind-neutral
//! `store.get` route) against a real PostgreSQL Store. The
//! rows a compaction leaves are written directly, so the test proves the
//! projection, not a failpoint: a live owner is a real non-idle backend
//! recorded in `pg_stat_activity`.
//!
//! Needs a writable PostgreSQL server: set `LAYERFS_TEST_POSTGRES_URL` to a
//! URL (docs/architecture/engine/store/testing.md); its user becomes the
//! Store's saved login.
//! The test creates and drops its own database, because `store.status` reads
//! the one schema a 2.0 PostgreSQL Store uses (`layerfs`); a shared schema
//! would collide with another run. Without the variable it reports NOT_RUN.
use layerfs_history::HistoryCatalogConfig;
use layerfs_home::events::Events;
use layerfs_home::home;
use layerfs_persistence::{PostgresHandles, PostgresSelection};
use layerfs_protocol::sdk::SdkValue;
use layerfs_storage::StoragePolicy;
use layerfs_store::get::With;
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

/// A machine's records-only state, as the Store reads take it: the machine
/// family's derivation (`layerfs-control` passes the same, with the removed
/// platform check this crate does not see).
fn machines(
    home: &Path,
    name: &str,
) -> Result<layerfs_home::state::State, layerfs_protocol::sdk::SdkRefusal> {
    Ok(
        match layerfs_machine::record::MachineRecord::read(home, name)? {
            Some(record) => record.status(false, 0),
            None => layerfs_machine::state::unrecorded(name),
        },
    )
}

/// The real control entry point: `store get pgs --with live` in `home`.
fn status(home: &Path) -> SdkValue {
    let mut sink = |_| {};
    let with = With {
        live: true,
        stats: false,
        format: false,
    };
    layerfs_store::get::get(home, &mut Events::new(&mut sink), "pgs", with, &machines).unwrap()
}

/// The `live` section's `phases`.
fn phases(answer: &SdkValue) -> &SdkValue {
    answer.get("live").unwrap().get("phases").unwrap()
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
    // `store.connect` records exactly this shape, its user in the record
    // and its password a secret of the home; `store.status` reads it. The login is the one
    // the client reads in the URL (a trust-auth server takes any password).
    let read: postgres::Config = url.parse().expect("LAYERFS_TEST_POSTGRES_URL is a URL");
    let user = read.get_user().unwrap_or("postgres");
    let password = read.get_password().map_or("unused".to_owned(), |password| {
        String::from_utf8(password.to_vec()).expect("a UTF-8 password")
    });
    layerfs_store::postgresql::login::save(&home, "pgs", &password).unwrap();
    // the record keeps the address alone: the URL as given when it holds no
    // login, else (the test server's URL names its user) the URL after the
    // user part the client reads (its first `@`), which must then be an
    // address alone; LayerFS never cuts a login out of a URL
    let held = layerfs_store::postgresql::connect::holds_login(&url);
    let user_is_password = read.get_password() == Some(user.as_bytes());
    assert!(!user_is_password || held, "{url} shown");
    let (scheme, after) =
        (url.split_once("://")).expect("LAYERFS_TEST_POSTGRES_URL is a postgres:// URL");
    let address = match held {
        false => url.clone(),
        true => format!(
            "{scheme}://{}",
            after.split_once('@').map_or(after, |(_, at)| at)
        ),
    };
    layerfs_store::postgresql::connect::refuse_login(&address)
        .expect("LAYERFS_TEST_POSTGRES_URL is an address with its login in its user part");
    StoreRecord::connected("pgs", &address, user, "layerfs", "off")
        .write(&home)
        .unwrap();
    let mut admin = postgres::Client::connect(&url, postgres::NoTls).unwrap();

    // Idle: no compaction row, so no op and no phases.
    let idle = status(&home);
    assert_eq!(idle.get_text("store"), Some("pgs"));
    assert_eq!(idle.get_text("engine"), Some("postgresql"));
    assert_eq!(idle.get_text("state"), Some("ready"));
    assert!(idle.get("op").is_none(), "a ready Store has no op");
    assert_eq!(
        phases(&idle),
        &SdkValue::Null,
        "an idle Store has no phases"
    );
    let live = idle.get("live").unwrap();
    assert!(
        live.get_text("server_version").is_some() && live.get_int("latency_ms").is_some(),
        "the server's answer is reported in the live section"
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
            ("name", SdkValue::text("compact")),
            ("phase", SdkValue::text("build")),
            (
                "started_at",
                SdkValue::text(layerfs_home::time::iso(started_ms))
            ),
        ]),
        "the op is the state's companion"
    );
    assert_eq!(texts(phases(&busy)), PHASES);

    // A gone owner leaves the same row: uncertain, and now unresolved.
    owner.close().unwrap();
    wait_gone(&mut admin, pid, &start);
    let leftover = status(&home);
    assert_eq!(leftover.get_text("state"), Some("broken"));
    assert_eq!(leftover.get_text("reason"), Some("unresolved"));
    assert_eq!(leftover.get_text("fix"), Some("layerfs store compact pgs"));
    assert_eq!(texts(phases(&leftover)), PHASES);

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

    // A dead owner is an unresolved leftover in every recorded phase.
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
        assert_eq!(projected.get_text("state"), Some("broken"), "{phase}");
        assert_eq!(projected.get_text("reason"), Some("unresolved"), "{phase}");
        assert_eq!(texts(phases(&projected)), PHASES);
    }

    // A settled run removes the row: ready again, with no phases.
    clear(&mut admin);
    let idle = status(&home);
    assert_eq!(idle.get_text("state"), Some("ready"));
    assert_eq!(phases(&idle), &SdkValue::Null);

    drop(admin);
    drop(handles);
    std::fs::remove_dir_all(home).unwrap();
}
