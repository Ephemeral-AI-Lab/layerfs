//! #818 round-trip harness shared by the `fs.*` and Init pins
//! (`sdk_read_round_trips.rs`, `sdk_history_round_trips.rs`,
//! `sdk_write_round_trips.rs`, `init_round_trips*.rs`) and the measurement
//! probe `fs_call_round_trips.rs`.
//!
//! - [`Server`]: the server-side cross-check, `pg_stat_statements` calls of
//!   the current database only (`dbid` of `current_database()`, since several
//!   databases share one server) and **top-level calls only**: with
//!   `pg_stat_statements.track=all` the server also counts nested statements
//!   such as foreign-key trigger checks (about 30k on Init 10k at baseline),
//!   which are server work, not round trips. An eviction (`dealloc` moved)
//!   inside a window is reported, never absorbed.
//! - [`Store`]: one Store per test, SQLite (sealed, Normal profile) or
//!   PostgreSQL in its own schema; Init of a source tree, then Branch `main`.
//! - [`Opened`]: the Store as the SDK opens it (`open_store_observed` with the
//!   SDK's `CALLS` readers and 64 KiB object cache), with its SQL work split
//!   into the writer (with its lane connection), the control reader (session
//!   0, which serves every SDK call) and the Workspace readers.
//! - [`check`]: prints each case and compares it with its [`Pin`].
//!
//! Counts are `[connects, round_trips, statements, transactions]`:
//! `connects` and `round_trips` are PostgreSQL only (zero on SQLite); on
//! PostgreSQL `statements` are what a body sends, without the `BEGIN` batch,
//! `COMMIT` or `ROLLBACK`, and `round_trips` include those batches and
//! statement prepares; on SQLite every prepared statement run counts,
//! transaction controls included.
#![allow(dead_code)] // each test crate uses a part of it

use layerfs_history::records::StoreStatistics;
use layerfs_history::{
    BranchId, ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig, HistoryName,
    LayerStackId,
};
use layerfs_persistence::{
    PersistenceConfig, PostgresHandles, PostgresSelection, SqlWork, SqliteAcquisitionSchema,
    SqliteHandles, SqlitePersistenceProfile, StoreSelection,
};
use layerfs_project::{init, InitRequest};
use layerfs_protocol::sdk::{SdkCall, SdkValue};
use layerfs_storage::{ReservationBlocks, Storage, StoragePolicy};
use layerfs_store_service::bootstrap::open_store_observed;
use layerfs_store_service::{Owner, OwnerConfig, StoreService};
use layerfs_telemetry::timer::Timing;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

pub const BINDING: &[u8] = b"fs-call-round-trips";
pub const CURSOR: [u8; 32] = [81; 32];
pub const PROJECT: &str = "rt";
pub const BRANCH: &str = "work";
/// The SDK Store service's reader capacity and object cache
/// (`store_service.rs`: `CALLS`, `CACHE_BYTES`).
pub const CALLS: usize = layerfs_store_service::CALLS;
pub const CACHE_BYTES: usize = 64 * 1024;

/// One session group's counted work: connects, round trips, statements and
/// transactions.
pub type Counts = [u64; 4];
/// Writer, control reader, Workspace readers.
pub type Roles = [Counts; 3];

pub fn counts(work: &SqlWork) -> Counts {
    [
        work.connects,
        work.round_trips,
        work.statements,
        work.transactions,
    ]
}

pub fn work_json(w: &SqlWork) -> String {
    format!(
        "{{\"connects\":{},\"round_trips\":{},\"prepares\":{},\"statements\":{},\"transactions\":{},\"returned_rows\":{}}}",
        w.connects, w.round_trips, w.prepares, w.statements, w.transactions, w.returned_rows
    )
}

pub fn roles_json(work: &[SqlWork; 3]) -> String {
    format!(
        "{{\"writer\":{},\"control\":{},\"workspace\":{}}}",
        work_json(&work[0]),
        work_json(&work[1]),
        work_json(&work[2])
    )
}

pub fn load1() -> String {
    std::process::Command::new("sysctl")
        .args(["-n", "vm.loadavg"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.split_whitespace().nth(1).map(str::to_owned))
        .unwrap_or_default()
}

fn catalog() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: BINDING.to_vec(),
        cursor_key: CURSOR,
        incarnation: 1,
    }
}

pub fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

/// PostgreSQL scenarios of one test binary run one at a time, so the
/// server's statement counters see one scenario's work (the rule of one
/// PostgreSQL test binary per database at a time still holds across
/// binaries).
pub fn postgresql_turn() -> MutexGuard<'static, ()> {
    static TURN: Mutex<()> = Mutex::new(());
    TURN.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

// ---------------------------------------------------------------------------
// Server-side cross-check

/// Statement calls of the current database by query id: (all calls,
/// top-level calls, normalized text).
#[derive(Clone, Default)]
pub struct Statements {
    rows: HashMap<i64, (i64, i64, String)>,
    /// `pg_stat_statements_info.dealloc`: entries evicted so far.
    dealloc: i64,
}

/// A separate observer connection; its own queries (which all name
/// `pg_stat_statements`) are excluded.
pub struct Server {
    client: postgres::Client,
}
impl Server {
    /// The observer, or `None` with a NOT_RUN note when the server has no
    /// `pg_stat_statements` (it must be in `shared_preload_libraries`).
    pub fn connect(url: &str) -> Option<Self> {
        let mut client = match postgres::Client::connect(url, postgres::NoTls) {
            Ok(client) => client,
            Err(error) => {
                eprintln!("NOT_RUN: server observer: {error}");
                return None;
            }
        };
        let ready = client
            .batch_execute("CREATE EXTENSION IF NOT EXISTS pg_stat_statements")
            .and_then(|()| client.query("SELECT 1 FROM pg_stat_statements LIMIT 1", &[]));
        if let Err(error) = ready {
            eprintln!("NOT_RUN: pg_stat_statements unavailable: {error}");
            return None;
        }
        Some(Self { client })
    }
    /// `SELECT 1` simple-query latency from this process, 200 samples.
    pub fn calibrate(&mut self) -> (u128, u128, u128) {
        let mut us: Vec<u128> = (0..200)
            .map(|_| {
                let started = Instant::now();
                self.client.simple_query("SELECT 1").unwrap();
                started.elapsed().as_micros()
            })
            .collect();
        us.sort_unstable();
        (us[0], us[100], us[180])
    }
    pub fn snapshot(&mut self) -> Statements {
        let rows = self
            .client
            .query(
                "SELECT coalesce(queryid, 0), calls, CASE WHEN toplevel THEN calls ELSE 0 END, query \
                 FROM pg_stat_statements \
                 WHERE dbid = (SELECT oid FROM pg_database WHERE datname = current_database()) \
                 AND query NOT LIKE '%pg_stat_statements%'",
                &[],
            )
            .unwrap();
        let mut out = Statements::default();
        for row in rows {
            let entry = out
                .rows
                .entry(row.get(0))
                .or_insert_with(|| (0, 0, row.get::<_, String>(3)));
            entry.0 += row.get::<_, i64>(1);
            entry.1 += row.get::<_, i64>(2);
        }
        out.dealloc = self
            .client
            .query_one("SELECT dealloc FROM pg_stat_statements_info", &[])
            .unwrap()
            .get(0);
        out
    }
}

/// The server's statement calls between two snapshots.
pub struct ServerDelta {
    /// Top-level calls: the statements clients sent. This is the count.
    pub toplevel: i64,
    /// Nested calls (trigger checks and the like), server work only.
    pub nested: i64,
    /// `pg_stat_statements` evicted entries inside the window: the
    /// difference may be short and is not to be compared.
    pub evicted: bool,
    /// `(top-level calls, all calls, text)` per changed statement, most
    /// top-level calls first.
    pub templates: Vec<(i64, i64, String)>,
}
impl ServerDelta {
    pub fn between(before: &Statements, after: &Statements) -> Self {
        let mut templates: Vec<(i64, i64, String)> = after
            .rows
            .iter()
            .filter_map(|(id, (calls, top, text))| {
                let (was, was_top) = before.rows.get(id).map_or((0, 0), |b| (b.0, b.1));
                (*calls > was).then(|| (top - was_top, calls - was, text.clone()))
            })
            .collect();
        templates.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
        let toplevel = templates.iter().map(|t| t.0).sum();
        let all: i64 = templates.iter().map(|t| t.1).sum();
        Self {
            toplevel,
            nested: all - toplevel,
            evicted: after.dealloc != before.dealloc,
            templates,
        }
    }
    /// JSON with the first `limit` templates by top-level calls.
    pub fn json(&self, limit: usize) -> String {
        let top: Vec<String> = self
            .templates
            .iter()
            .filter(|t| t.0 > 0)
            .take(limit)
            .map(|(top, _, text)| {
                let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
                let text: String = text.chars().take(110).collect();
                format!(
                    "[{top},\"{}\"]",
                    text.replace('\\', "\\\\").replace('"', "\\\"")
                )
            })
            .collect();
        format!(
            "{{\"toplevel_calls\":{},\"nested_calls\":{},\"evicted\":{},\"distinct\":{},\"top\":[{}]}}",
            self.toplevel,
            self.nested,
            self.evicted,
            self.templates.iter().filter(|t| t.0 > 0).count(),
            top.join(",")
        )
    }
}

// ---------------------------------------------------------------------------
// Stores

/// Which backend a [`Store`] uses.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Backend {
    Sqlite,
    Postgresql,
}
impl Backend {
    pub fn name(self) -> &'static str {
        match self {
            Self::Sqlite => "sqlite",
            Self::Postgresql => "postgresql",
        }
    }
}

/// What one Init did and what it left.
pub struct InitOutcome {
    /// The Init's own SQL work on the Store's one session.
    pub work: SqlWork,
    pub server: Option<ServerDelta>,
    pub ms: f64,
    pub entries: u64,
    /// The genesis Layer's recorded logical bytes.
    pub logical: u64,
    /// `logical_size(reader, root)` walked after Init, outside the count.
    pub walked: u64,
    /// The genesis root's id, hex.
    pub root: String,
    /// The Store's byte totals after Init (the Save outcome of a new Store).
    pub statistics: StoreStatistics,
}

/// One Store for one test or probe run; dropped, it removes its directory
/// and its PostgreSQL schema.
pub struct Store {
    pub backend: Backend,
    pub dir: PathBuf,
    pub url: String,
    pub schema: String,
    /// The sealed SQLite Store, once Init has run.
    sqlite: Option<PathBuf>,
    pub server: Option<Server>,
}
impl Store {
    pub fn sqlite(label: &str) -> Self {
        Self {
            backend: Backend::Sqlite,
            dir: scratch(label),
            url: String::new(),
            schema: String::new(),
            sqlite: None,
            server: None,
        }
    }
    /// A PostgreSQL Store in its own schema, or `None` with a NOT_RUN note
    /// without `LAYERFS_TEST_POSTGRES_URL`. The server observer is attached
    /// when `pg_stat_statements` is available.
    pub fn postgresql(label: &str) -> Option<Self> {
        let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
            eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
            return None;
        };
        let server = Server::connect(&url);
        let schema = format!("lfs818_{label}_{}", std::process::id());
        Some(Self {
            backend: Backend::Postgresql,
            dir: scratch(label),
            url,
            schema,
            sqlite: None,
            server,
        })
    }
    pub fn snapshot(&mut self) -> Option<Statements> {
        self.server.as_mut().map(Server::snapshot)
    }
    pub fn server_delta(&mut self, before: Option<Statements>) -> Option<ServerDelta> {
        match (before, self.server.as_mut()) {
            (Some(before), Some(server)) => Some(ServerDelta::between(&before, &server.snapshot())),
            _ => None,
        }
    }
    /// Inits `source` into a new Store and forks `main` from its Layer.
    /// `around(true, observer)` runs just before the counted Init and
    /// `around(false, observer)` just after (the probe's delay arm).
    pub fn init(
        &mut self,
        source: &Path,
        around: &mut dyn FnMut(bool, Option<&mut Server>),
    ) -> InitOutcome {
        let request = |acquisition| InitRequest {
            source,
            acquisition,
            stack: LayerStackId::from_authority([81; 16]),
            name: HistoryName::new(PROJECT).unwrap(),
            scope_seed: [81; 32],
            deadline: Instant::now() + Duration::from_secs(115),
        };
        if self.backend == Backend::Sqlite {
            let config = PersistenceConfig::sqlite(self.dir.join("store.sqlite"))
                .with_sqlite_profile(SqlitePersistenceProfile::Normal)
                .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables);
            let handles =
                SqliteHandles::create(config, StoragePolicy::frozen_default(), &catalog()).unwrap();
            let storage = Storage::new(handles.storage.clone()).unwrap();
            around(true, None);
            let before = handles.diagnostics().unwrap();
            let started = Instant::now();
            let done = Timing::disabled("init", |timing| {
                init(
                    &storage,
                    &handles.history,
                    request(&handles.acquisition),
                    timing,
                )
            })
            .0
            .unwrap();
            let ms = started.elapsed().as_secs_f64() * 1e3;
            let work = handles.diagnostics().unwrap().since(&before);
            around(false, None);
            let outcome = settle(&storage, &handles.history, &done, work, None, ms);
            drop(storage);
            self.sqlite = Some(handles.seal().unwrap().path);
            return outcome;
        }
        let selection = PostgresSelection::new(&self.url, &self.schema);
        let handles =
            PostgresHandles::create(&selection, StoragePolicy::frozen_default(), &catalog())
                .unwrap();
        let storage = Storage::new(handles.storage.clone()).unwrap();
        around(true, self.server.as_mut());
        let before = handles.diagnostics().unwrap();
        let server_before = self.snapshot();
        let started = Instant::now();
        let done = Timing::disabled("init", |timing| {
            init(
                &storage,
                &handles.history,
                request(&handles.acquisition),
                timing,
            )
        })
        .0
        .unwrap();
        let ms = started.elapsed().as_secs_f64() * 1e3;
        let work = handles.diagnostics().unwrap().since(&before);
        let server = self.server_delta(server_before);
        around(false, self.server.as_mut());
        settle(&storage, &handles.history, &done, work, server, ms)
    }
    pub fn selection(&self) -> StoreSelection {
        match &self.sqlite {
            Some(path) => StoreSelection::Sqlite(
                PersistenceConfig::sqlite(path)
                    .with_sqlite_profile(SqlitePersistenceProfile::Normal),
            ),
            None => StoreSelection::Postgres(PostgresSelection::new(&self.url, &self.schema)),
        }
    }
    /// A new open of the Store, as the SDK opens it, with its own overlay
    /// owner under `tag`.
    pub fn open(&self, tag: &str) -> Opened {
        let opened = open_store_observed(
            self.selection(),
            BINDING,
            CURSOR,
            CALLS,
            CACHE_BYTES,
            ReservationBlocks::default(),
        )
        .unwrap();
        // As `StoreService::open_postgres` (#818 W4); nothing on SQLite.
        opened.prepare_sdk_writes().unwrap();
        let overlay = self.dir.join(format!("overlay-{tag}"));
        let _ = std::fs::remove_dir_all(&overlay);
        std::fs::create_dir_all(&overlay).unwrap();
        let owner = Owner::start(
            &overlay.join("overlay.sqlite"),
            layerfs_overlay::ProfileConfig::default(),
            OwnerConfig::default(),
        )
        .unwrap();
        Opened {
            service: StoreService::new(opened, &owner),
            owner,
        }
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        if self.backend == Backend::Postgresql {
            if let Ok(mut client) = postgres::Client::connect(&self.url, postgres::NoTls) {
                let _ =
                    client.batch_execute(&format!("DROP SCHEMA IF EXISTS {} CASCADE", self.schema));
            }
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// After the counted Init: the logical oracle, the Store totals and
/// Branch `main`, none of it counted.
fn settle(
    storage: &Storage,
    history: &dyn HistoryCatalog,
    done: &layerfs_project::Initialized,
    work: SqlWork,
    server: Option<ServerDelta>,
    ms: f64,
) -> InitOutcome {
    let layer = history.layer(done.stack.head_layer).unwrap().unwrap();
    let reader = storage.reader().unwrap();
    let walked = layerfs_content::filesystem::statistics::logical_size(
        &reader,
        layerfs_content::filesystem::FilesystemRootId(layer.root),
    )
    .unwrap();
    let statistics = history.statistics().unwrap();
    history
        .fork(&ForkRequest {
            stack: done.stack.id,
            branch: BranchId::from_authority([82; 16]),
            name: HistoryName::new("main").unwrap(),
            source: ForkSource::Layer(done.stack.head_layer),
        })
        .unwrap();
    InitOutcome {
        work,
        server,
        ms,
        entries: done.entries,
        logical: layer.logical,
        walked,
        root: layer
            .root
            .as_bytes()
            .iter()
            .fold(String::new(), |mut hex, b| {
                use std::fmt::Write as _;
                let _ = write!(hex, "{b:02x}");
                hex
            }),
        statistics,
    }
}

pub fn scratch(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs818-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// ---------------------------------------------------------------------------
// The SDK's Store open and its calls

/// One Store open as an SDK call opens it, with its overlay owner.
pub struct Opened {
    pub service: StoreService,
    pub owner: Owner,
}
impl Opened {
    /// Writer, control reader and Workspace readers (live and retired).
    pub fn work(&self) -> [SqlWork; 3] {
        let opened = &self.service.opened;
        let writer = opened.session_diagnostics(None).unwrap().unwrap();
        let control = opened
            .session_diagnostics(Some(0))
            .unwrap()
            .unwrap_or_default();
        let mut readers = SqlWork::default();
        for reader in &opened.diagnostics().unwrap().readers {
            readers.accumulate(reader);
        }
        [writer, control, readers.since(&control)]
    }
    /// One SDK method call on project [`PROJECT`].
    pub fn call(&self, method: &str, fields: Vec<(&str, SdkValue)>) -> Result<SdkValue, String> {
        let mut fields = fields;
        fields.push(("project", t(PROJECT)));
        let call = SdkCall {
            method: method.into(),
            args: SdkValue::Map(
                fields
                    .into_iter()
                    .map(|(name, value)| (name.to_owned(), value))
                    .collect(),
            ),
        };
        layerfs_store_service::methods::call(&self.service, &call)
            .map_err(|r| format!("{}: {}", r.code, r.message))
    }
    pub fn stop(self) {
        drop(self.service);
        self.owner.stop().unwrap();
    }
}

pub fn since(after: &[SqlWork; 3], before: &[SqlWork; 3]) -> [SqlWork; 3] {
    [
        after[0].since(&before[0]),
        after[1].since(&before[1]),
        after[2].since(&before[2]),
    ]
}

pub fn total(work: &[SqlWork; 3]) -> SqlWork {
    let mut total = work[0];
    total.accumulate(&work[1]);
    total.accumulate(&work[2]);
    total
}

pub fn commit_id(reply: &SdkValue) -> String {
    reply
        .get("commit")
        .and_then(|c| c.get_text("id"))
        .unwrap_or_else(|| panic!("no commit id: {reply:?}"))
        .to_owned()
}

/// The probe tree: 10 directories of 100 small files under `/src`, a
/// depth-7 file, a 2,000-line text file and the files the history rewrites.
pub fn build_tree(source: &Path) {
    for d in 0..10 {
        let dir = source.join(format!("src/d{d:02}"));
        std::fs::create_dir_all(&dir).unwrap();
        for f in 0..100 {
            std::fs::write(dir.join(format!("f{f:03}.txt")), format!("file {d} {f}\n")).unwrap();
        }
    }
    let deep = source.join("deep/a/b/c/d/e");
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::write(deep.join("leaf.txt"), "leaf\n").unwrap();
    std::fs::create_dir_all(source.join("text")).unwrap();
    let mut big = String::new();
    for n in 1..=2000 {
        if n == 1500 {
            big.push_str("needle here\n");
        } else {
            big.push_str(&format!("line {n}\n"));
        }
    }
    std::fs::write(source.join("text/big.txt"), big).unwrap();
    std::fs::create_dir_all(source.join("hist")).unwrap();
    std::fs::write(source.join("hist/a.txt"), "v0\n").unwrap();
    std::fs::write(source.join("hist/other.txt"), "o0\n").unwrap();
}

/// Forks Branch [`BRANCH`] from `main`.
pub fn fork_work(opened: &Opened) {
    opened
        .call("branch.fork", vec![("name", t(BRANCH))])
        .unwrap();
}

/// The probe's history on Branch [`BRANCH`]: 100 Commits through
/// `fs.write`, alternating `/hist/a.txt` (one appended line each) and
/// `/hist/other.txt`. The Commit ids, oldest first.
pub fn write_history(opened: &Opened) -> Vec<String> {
    let (mut ids, mut text) = (Vec::new(), String::from("v0\n"));
    for n in 1..=100 {
        let (path, bytes) = if n % 2 == 1 {
            text.push_str(&format!("v{n}\n"));
            ("/hist/a.txt", text.clone())
        } else {
            ("/hist/other.txt", format!("o{n}\n"))
        };
        let reply = opened
            .call(
                "fs.write",
                vec![
                    ("branch", t(BRANCH)),
                    ("path", t(path)),
                    ("bytes", SdkValue::Bytes(bytes.into_bytes())),
                ],
            )
            .unwrap();
        ids.push(commit_id(&reply));
    }
    ids
}

/// The probe's setup on a new Store: Init of [`build_tree`], Branch
/// [`BRANCH`] and [`write_history`], on one Store open that is then closed.
pub fn probe_store(store: &mut Store) -> Vec<String> {
    let source = store.dir.join("source");
    build_tree(&source);
    store.init(&source, &mut |_, _| {});
    let opened = store.open("setup");
    fork_work(&opened);
    let ids = write_history(&opened);
    opened.stop();
    ids
}

// ---------------------------------------------------------------------------
// Measured cases and their pins

/// One measured case: its counted work by role and the server's view.
pub struct Measured {
    pub case: String,
    pub roles: Roles,
    pub server: Option<ServerDelta>,
    pub outcome: String,
    pub ms: f64,
}

/// One SDK call on `opened`, counted by role (and on the server when
/// observed). A refusal is part of the outcome, not a failure.
pub fn measure_call(
    store: &mut Store,
    opened: &Opened,
    case: &str,
    method: &str,
    fields: Vec<(&str, SdkValue)>,
) -> (Measured, Result<SdkValue, String>) {
    let before = opened.work();
    let server_before = store.snapshot();
    let started = Instant::now();
    let reply = opened.call(method, fields);
    let ms = started.elapsed().as_secs_f64() * 1e3;
    let work = since(&opened.work(), &before);
    let server = store.server_delta(server_before);
    let outcome = match &reply {
        Ok(value) => value.get_text("kind").unwrap_or("ok").to_owned(),
        Err(refusal) => format!("refused {refusal}").replace('"', "'"),
    };
    let measured = Measured {
        case: case.to_owned(),
        roles: [counts(&work[0]), counts(&work[1]), counts(&work[2])],
        server,
        outcome,
        ms,
    };
    (measured, reply)
}

/// A pinned case: writer, control reader and Workspace readers.
pub struct Pin {
    pub case: &'static str,
    pub writer: Counts,
    pub control: Counts,
    pub workspace: Counts,
}

/// Prints every case (`RT_COUNTS`), then compares each with its pin.
pub fn check(file: &str, backend: Backend, measured: &[Measured], pins: &[Pin]) {
    for m in measured {
        let server = m.server.as_ref().map_or("null".to_owned(), |s| s.json(8));
        println!(
            "RT_COUNTS {{\"pins\":\"{file}\",\"backend\":\"{}\",\"case\":\"{}\",\"outcome\":\"{}\",\"writer\":{:?},\"control\":{:?},\"workspace\":{:?},\"round_trips\":{},\"statements\":{},\"ms\":{:.2},\"server\":{server}}}",
            backend.name(),
            m.case,
            m.outcome,
            m.roles[0],
            m.roles[1],
            m.roles[2],
            m.roles.iter().map(|r| r[1]).sum::<u64>(),
            m.roles.iter().map(|r| r[2]).sum::<u64>(),
            m.ms,
        );
    }
    let cases: Vec<&str> = measured.iter().map(|m| m.case.as_str()).collect();
    let pinned: Vec<&str> = pins.iter().map(|p| p.case).collect();
    assert_eq!(cases, pinned, "{} cases", backend.name());
    let mut wrong = Vec::new();
    for (m, pin) in measured.iter().zip(pins) {
        if m.roles != [pin.writer, pin.control, pin.workspace] {
            wrong.push(format!(
                "{} {}: measured writer {:?} control {:?} workspace {:?}, pinned {:?} {:?} {:?}",
                backend.name(),
                m.case,
                m.roles[0],
                m.roles[1],
                m.roles[2],
                pin.writer,
                pin.control,
                pin.workspace
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "[connects, round_trips, statements, transactions] per role:\n{}",
        wrong.join("\n")
    );
}

/// One SDK call's arguments.
pub type Fields = Vec<(&'static str, SdkValue)>;

/// Measures `case.cold` on a new open of `store`, then, with `warm`
/// arguments, `case.warm` on the same open; each reply's `kind` (or the
/// refusal) must be `expect`.
pub fn cold_warm(
    store: &mut Store,
    out: &mut Vec<Measured>,
    case: &str,
    method: &str,
    expect: &str,
    cold: Fields,
    warm: Option<Fields>,
) {
    let opened = store.open(case);
    let mut one = |label: &str, fields: Fields| {
        let (measured, _) = measure_call(store, &opened, label, method, fields);
        assert!(
            measured.outcome.starts_with(expect),
            "{label}: {} (expected {expect})",
            measured.outcome
        );
        out.push(measured);
    };
    one(&format!("{case}.cold"), cold);
    if let Some(warm) = warm {
        one(&format!("{case}.warm"), warm);
    }
    opened.stop();
}

pub fn branch_arg() -> (&'static str, SdkValue) {
    ("branch", t(BRANCH))
}
pub fn path_arg(path: &str) -> (&'static str, SdkValue) {
    ("path", t(path))
}

// ---------------------------------------------------------------------------
// Init pins

/// One pinned Init on a new Store: the Store session's counts, the allowed
/// spread (see `init_round_trips_10k.rs`), and its byte outcome. Pack bytes
/// (`stored`) are printed, not pinned: at baseline they differ by a few
/// bytes from run to run (packing follows the order the parallel
/// construction workers finish in; e.g. 24,225 / 24,227 / 24,234 for one
/// tree), while the root, the canonical bytes and the logical size repeat.
pub struct InitPin {
    pub case: &'static str,
    /// `[connects, round_trips, statements, transactions]` of the Store's
    /// one session (SQLite: `[0, 0, statements, transactions]`).
    pub session: Counts,
    /// Round trips and statements may differ from the pin by at most this
    /// much (0: exact).
    pub slack: u64,
    /// The genesis Layer's logical bytes, which must also equal
    /// `logical_size(reader, root)`.
    pub logical: u64,
    /// Store canonical bytes after Init (the Save outcome of a new Store).
    pub canonical: u64,
    /// The first 16 hex digits of the genesis root's id.
    pub root: &'static str,
}

/// One measured Init.
pub struct InitMeasured {
    pub case: &'static str,
    pub outcome: InitOutcome,
}

/// Builds the source with `build`, fixes every entry's times (so the
/// genesis root repeats from run to run and across backends), then Inits it
/// into `store` (new).
pub fn init_shape(store: &mut Store, case: &'static str, build: &dyn Fn(&Path)) -> InitMeasured {
    let source = store.dir.join(format!("source-{case}"));
    build(&source);
    freeze_times(&source);
    let outcome = store.init(&source, &mut |_, _| {});
    InitMeasured { case, outcome }
}

/// Sets every entry's access and modification time under `path` (a
/// directory's after its children) to one fixed instant.
pub fn freeze_times(path: &Path) {
    let at = std::time::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let meta = std::fs::symlink_metadata(path).unwrap();
    if meta.is_dir() {
        for entry in std::fs::read_dir(path).unwrap() {
            freeze_times(&entry.unwrap().path());
        }
    }
    let times = std::fs::FileTimes::new().set_accessed(at).set_modified(at);
    std::fs::File::open(path).unwrap().set_times(times).unwrap();
}

/// `dirs` directories of `per` small files each, as the probe's Init cases.
pub fn files(source: &Path, dirs: usize, per: usize) {
    for d in 0..dirs {
        let sub = source.join(format!("d{d:03}"));
        std::fs::create_dir_all(&sub).unwrap();
        for f in 0..per {
            std::fs::write(sub.join(format!("f{f:05}.txt")), format!("file {d} {f}\n")).unwrap();
        }
    }
}

/// Prints every Init (`RT_INIT`), checks the logical oracle, then compares
/// each with its pin.
pub fn check_init(file: &str, backend: Backend, measured: &[InitMeasured], pins: &[InitPin]) {
    for m in measured {
        let o = &m.outcome;
        println!(
            "RT_INIT {{\"pins\":\"{file}\",\"backend\":\"{}\",\"case\":\"{}\",\"entries\":{},\"session\":{:?},\"prepares\":{},\"ms\":{:.1},\"logical\":{},\"walked\":{},\"root\":\"{}\",\"canonical\":{},\"stored\":{},\"metadata\":{},\"server\":{}}}",
            backend.name(),
            m.case,
            o.entries,
            counts(&o.work),
            o.work.prepares,
            o.ms,
            o.logical,
            o.walked,
            o.root,
            o.statistics.canonical,
            o.statistics.stored,
            o.statistics.metadata,
            o.server.as_ref().map_or("null".to_owned(), |s| s.json(6)),
        );
    }
    for m in measured {
        assert_eq!(
            m.outcome.logical,
            m.outcome.walked,
            "{} {}: Init logical equals logical_size(reader, root)",
            backend.name(),
            m.case
        );
    }
    let cases: Vec<&str> = measured.iter().map(|m| m.case).collect();
    let pinned: Vec<&str> = pins.iter().map(|p| p.case).collect();
    assert_eq!(cases, pinned, "{} cases", backend.name());
    let mut wrong = Vec::new();
    for (m, pin) in measured.iter().zip(pins) {
        let got = counts(&m.outcome.work);
        let near = |at: usize| got[at].abs_diff(pin.session[at]) <= pin.slack;
        let counts_ok = got[0] == pin.session[0] && near(1) && near(2) && got[3] == pin.session[3];
        let o = &m.outcome;
        let bytes = [o.logical, o.statistics.canonical];
        if !counts_ok || bytes != [pin.logical, pin.canonical] || !o.root.starts_with(pin.root) {
            wrong.push(format!(
                "{} {}: measured {:?} [logical, canonical] {:?} root {}, pinned {:?} (slack {}) {:?} root {}",
                backend.name(),
                m.case,
                got,
                bytes,
                &o.root[..16],
                pin.session,
                pin.slack,
                [pin.logical, pin.canonical],
                pin.root
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "[connects, round_trips, statements, transactions]:\n{}",
        wrong.join("\n")
    );
}
