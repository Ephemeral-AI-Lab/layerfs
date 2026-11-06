//! layerfs-project #61/#62: client-server round trips, prepares, connections,
//! statements and transactions per scenario, through the Store service
//! without FUSE: Store opens, the mount path's Workspace bind, Commits of
//! several shapes on one mount-bound Workspace, and cold committed reads.
//!
//! A measurement tool, not a gate: it asserts only that counted work
//! happened. Each scenario prints one line `ROUND_TRIPS <json>` with stable
//! keys; counts are the evidence, `ms` is a diagnostic. The SQLite test always
//! runs (its wire counters are zero; statements and transactions are real).
//! The PostgreSQL test needs `LAYERFS_TEST_POSTGRES_URL` and reports NOT_RUN
//! without it; when `pg_stat_statements` can be enabled in the test database
//! each scenario also carries the server's per-statement call deltas.
//!
//! Commit scenarios also carry `phase_sql`: the inclusive SQL work of each
//! Store-service Commit phase (`capture`, `construct`, `finish`,
//! `logical.count`, `bindings`, `lane`, `head.lane`, `validate`, `publish`),
//! read from the Store's session totals at each phase's start and end.
//!
//! Run with `--test-threads=1 --nocapture` so the two Stores' timings do not
//! overlap. Cache state: the Commits run on one Store opened once (a warm
//! process, as a daemon serving one Workspace is); the reads reopen the Store
//! and bind a new Workspace, so nothing they read is cached beforehand.
#![cfg(feature = "postgresql")]
use layerfs_content::filesystem::PathName;
use layerfs_history::{
    BranchId, ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig, HistoryName,
    LayerStackId, WorkspaceId,
};
use layerfs_persistence::{
    PersistenceConfig, PostgresHandles, PostgresSelection, SqlWork, SqliteAcquisitionSchema,
    SqliteHandles, SqlitePersistenceProfile, StoreSelection,
};
use layerfs_project::{init, InitRequest};
use layerfs_storage::{ReservationBlocks, Storage, StoragePolicy};
use layerfs_store_service::bootstrap::open_store_observed;
use layerfs_store_service::store::{BindRequest, BoundWorkspace, CommitOptions};
use layerfs_store_service::telemetry::{attribute_sql, phase_sql, Recording};
use layerfs_store_service::write::mutation::Mutation;
use layerfs_store_service::{Owner, OwnerConfig, StoreService};
use layerfs_telemetry::timer::{Timing, TimingNode};
use layerfs_workspace::Operation;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const BINDING: &[u8] = b"postgresql-round-trips";
const CURSOR: [u8; 32] = [62; 32];
/// The daemon's Store object cache.
const CACHE_BYTES: usize = 64 * 1024 * 1024;
const BIG: usize = 64 * 1024 * 1024;
/// One FUSE read request's size (`max_read`).
const READ: u32 = 128 * 1024;

#[test]
fn sqlite_statement_counts() {
    let dir = scratch("sqlite");
    let config = PersistenceConfig::sqlite(dir.join("store.sqlite"))
        .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
        .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables);
    let handles =
        SqliteHandles::create(config.clone(), StoragePolicy::frozen_default(), &catalog()).unwrap();
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let branch = initialize(&dir, &storage, &handles.history, &handles.acquisition);
    drop(storage);
    let sealed = handles.seal().unwrap();
    let config = PersistenceConfig::sqlite(&sealed.path)
        .with_sqlite_profile(SqlitePersistenceProfile::Disposable);
    let session = SqliteHandles::open_read_only(config.clone(), BINDING, CURSOR).unwrap();
    let work = session.storage.diagnostics().unwrap();
    drop(session);
    let run = Run {
        backend: "sqlite",
        server: None,
    };
    run.emit("session_open", &Phase::single(work), None);
    run.scenarios(&dir, || StoreSelection::Sqlite(config.clone()), branch);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn postgresql_round_trip_counts() {
    let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
        return;
    };
    let dir = scratch("postgresql");
    let schema = format!("lfs62_rt_{}", std::process::id());
    let selection = PostgresSelection::new(&url, &schema);
    let result = std::panic::catch_unwind(|| {
        let handles =
            PostgresHandles::create(&selection, StoragePolicy::frozen_default(), &catalog())
                .unwrap();
        let storage = Storage::new(handles.storage.clone()).unwrap();
        let branch = initialize(&dir, &storage, &handles.history, &handles.acquisition);
        drop(storage);
        drop(handles);
        let mut server = Server::connect(&url);
        if let Some(server) = &mut server {
            server.calibrate();
        }
        let run = Run {
            backend: "postgresql",
            server: server.map(std::cell::RefCell::new),
        };
        let before = run.server_snapshot();
        let session = PostgresHandles::open_read_only(&selection, BINDING, CURSOR).unwrap();
        let work = session.diagnostics().unwrap();
        drop(session);
        run.emit(
            "session_open",
            &Phase::single(work),
            run.server_delta(before),
        );
        run.scenarios(&dir, || StoreSelection::Postgres(selection.clone()), branch);
    });
    if let Ok(mut client) = postgres::Client::connect(&url, postgres::NoTls) {
        let _ = client.batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"));
    }
    let _ = std::fs::remove_dir_all(&dir);
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

fn scratch(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs62-rt-{label}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn catalog() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: BINDING.to_vec(),
        cursor_key: CURSOR,
        incarnation: 1,
    }
}

/// Inits a one-file project and forks its `main` Branch.
fn initialize(
    dir: &Path,
    storage: &Storage,
    history: &dyn HistoryCatalog,
    acquisition: &dyn layerfs_storage::port::acquisition::Acquisition,
) -> BranchId {
    let source = dir.join("source");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("seed"), b"seed\n").unwrap();
    let initialized = Timing::disabled("init", |timing| {
        init(
            storage,
            history,
            InitRequest {
                source: &source,
                acquisition,
                stack: LayerStackId::from_authority([62; 16]),
                name: HistoryName::new("rt").unwrap(),
                scope_seed: [62; 32],
                deadline: Instant::now() + Duration::from_secs(60),
            },
            timing,
        )
    })
    .0
    .unwrap();
    let branch = BranchId::from_authority([63; 16]);
    history
        .fork(&ForkRequest {
            stack: initialized.stack.id,
            branch,
            name: HistoryName::new("main").unwrap(),
            source: ForkSource::Layer(initialized.stack.head_layer),
        })
        .unwrap();
    std::fs::remove_dir_all(source).unwrap();
    branch
}

/// One Store open as the daemon opens it, with its overlay owner.
struct Opened {
    service: StoreService,
    owner: Owner,
}
impl Opened {
    fn open(dir: &Path, selection: StoreSelection, tag: &str) -> Self {
        let opened = open_store_observed(
            selection,
            BINDING,
            CURSOR,
            OwnerConfig::default().namespaces,
            CACHE_BYTES,
            ReservationBlocks::default(),
        )
        .unwrap();
        // Each owner's Workspace overlay files live beside its template.
        let overlay = dir.join(tag);
        std::fs::create_dir_all(&overlay).unwrap();
        let owner = Owner::start(
            &overlay.join("overlay.sqlite"),
            layerfs_overlay::ProfileConfig::default(),
            OwnerConfig::default(),
        )
        .unwrap();
        let service = StoreService::new(opened, &owner);
        Self { service, owner }
    }
    /// Writer and summed reader counters, live and retired.
    fn work(&self) -> (SqlWork, SqlWork) {
        let all = self.service.opened.diagnostics().unwrap();
        let mut readers = SqlWork::default();
        for reader in &all.readers {
            readers.accumulate(reader);
        }
        (all.writer, readers)
    }
    /// The mount path's bind: a new Store reader for this Workspace.
    fn mount(&self, n: u8, branch: BranchId) -> BoundWorkspace {
        self.service
            .store
            .bind(
                self.owner.client(),
                BindRequest {
                    branch,
                    workspace: WorkspaceId::from_authority([n; 32]).unwrap(),
                },
            )
            .unwrap()
            .workspace
    }
    fn close(self, workspace: Option<BoundWorkspace>) {
        if let Some(workspace) = workspace {
            workspace.close().unwrap();
            self.owner
                .client()
                .retire_namespace(workspace.route().namespace())
                .unwrap();
        }
        drop(self.service);
        self.owner.stop().unwrap();
    }
}

/// Counter deltas of one measured step.
struct Phase {
    writer: SqlWork,
    readers: SqlWork,
    ms: f64,
    phases_us: BTreeMap<String, u128>,
    /// Inclusive SQL work per Store-service phase, by name.
    phase_sql: BTreeMap<&'static str, SqlWork>,
}
impl Phase {
    fn single(work: SqlWork) -> Self {
        Self {
            writer: work,
            readers: SqlWork::default(),
            ms: 0.0,
            phases_us: BTreeMap::new(),
            phase_sql: BTreeMap::new(),
        }
    }
    fn measure<T>(opened: &Opened, body: impl FnOnce() -> T) -> (T, Self) {
        let (writer, readers) = opened.work();
        let started = Instant::now();
        let value = body();
        let ms = started.elapsed().as_secs_f64() * 1e3;
        let (writer_after, readers_after) = opened.work();
        (
            value,
            Self {
                writer: writer_after.since(&writer),
                readers: readers_after.since(&readers),
                ms,
                phases_us: BTreeMap::new(),
                phase_sql: BTreeMap::new(),
            },
        )
    }
}

struct Run {
    backend: &'static str,
    server: Option<std::cell::RefCell<Server>>,
}
impl Run {
    fn server_snapshot(&self) -> Option<Statements> {
        self.server.as_ref().map(|s| s.borrow_mut().snapshot())
    }
    fn server_delta(&self, before: Option<Statements>) -> Option<String> {
        let server = self.server.as_ref()?;
        let after = server.borrow_mut().snapshot();
        Some(statements_json(&before?, &after))
    }

    /// Runs `body` as one measured scenario and prints it.
    fn scenario<T>(&self, opened: &Opened, name: &str, body: impl FnOnce() -> T) -> T {
        let before = self.server_snapshot();
        let (value, phase) = Phase::measure(opened, body);
        self.emit(name, &phase, self.server_delta(before));
        value
    }

    fn emit(&self, name: &str, phase: &Phase, server: Option<String>) {
        let mut total = phase.writer;
        total.accumulate(&phase.readers);
        let phases: Vec<String> = phase
            .phases_us
            .iter()
            .map(|(name, us)| format!("\"{name}\":{us}"))
            .collect();
        let phase_sql: Vec<String> = phase
            .phase_sql
            .iter()
            .map(|(name, work)| format!("\"{name}\":{}", work_json(work)))
            .collect();
        println!(
            "ROUND_TRIPS {{\"backend\":\"{}\",\"scenario\":\"{name}\",\"total\":{},\"writer\":{},\"readers\":{},\"ms\":{:.1},\"load1\":{},\"phases_us\":{{{}}},\"phase_sql\":{{{}}},\"server\":{}}}",
            self.backend,
            work_json(&total),
            work_json(&phase.writer),
            work_json(&phase.readers),
            phase.ms,
            load1(),
            phases.join(","),
            phase_sql.join(","),
            server.unwrap_or_else(|| "null".to_owned()),
        );
        if name != "commit.up_to_date" {
            assert!(total.statements > 0, "{name}: no counted statements");
        }
        if self.backend == "postgresql" && total.statements > 0 {
            assert!(total.round_trips >= total.statements, "{name}");
        }
    }

    fn scenarios(&self, dir: &Path, selection: impl Fn() -> StoreSelection, branch: BranchId) {
        let before = self.server_snapshot();
        let (opened, mut phase) = {
            let started = Instant::now();
            let opened = Opened::open(dir, selection(), "a");
            let ms = started.elapsed().as_secs_f64() * 1e3;
            let (writer, readers) = opened.work();
            (
                opened,
                Phase {
                    writer,
                    readers,
                    ms,
                    phases_us: BTreeMap::new(),
                    phase_sql: BTreeMap::new(),
                },
            )
        };
        phase.ms = (phase.ms * 10.0).round() / 10.0;
        self.emit("store_open", &phase, self.server_delta(before));
        let workspace = self.scenario(&opened, "workspace_reader_open", || opened.mount(1, branch));

        self.commit(&opened, &workspace, "commit.up_to_date", |_| {});
        self.commit(&opened, &workspace, "commit.net_empty_1000", |m| {
            m.mkdir("/net", None).unwrap();
            for n in 0..1000 {
                m.write(&format!("/net/f{n:04}"), &small(n), None).unwrap();
            }
            let parent = m.resolve("/net").unwrap().serial;
            for n in 0..1000 {
                let name = PathName::new(&format!("f{n:04}")).unwrap();
                m.apply(Operation::Unlink { parent, name }).unwrap();
            }
            let root = m.resolve("/").unwrap().serial;
            let name = PathName::new("net").unwrap();
            m.apply(Operation::Rmdir { parent: root, name }).unwrap();
        });
        self.commit(&opened, &workspace, "commit.files_1000", |m| {
            m.mkdir("/f1000", None).unwrap();
            for n in 1..=1000 {
                m.write(&format!("/f1000/f{n}"), &tiny(n), None).unwrap();
            }
        });
        // Similar 23-byte bodies: each is a delta candidate of the last.
        self.commit(&opened, &workspace, "commit.similar_files_1000", |m| {
            m.mkdir("/s1000", None).unwrap();
            for n in 0..1000 {
                m.write(&format!("/s1000/f{n:04}"), &small(n), None)
                    .unwrap();
            }
        });
        self.commit(&opened, &workspace, "commit.c04_dirs_110_files_1000", |m| {
            for top in 0..10 {
                for sub in 0..10 {
                    let dir = format!("/c04/d{top}/s{sub}");
                    m.mkdir(&dir, None).unwrap();
                    for n in 0..10 {
                        m.write(&format!("{dir}/e{n}"), b"", None).unwrap();
                    }
                }
            }
        });
        self.commit(&opened, &workspace, "commit.files_100", |m| {
            m.mkdir("/f100", None).unwrap();
            for n in 1..=100 {
                m.write(&format!("/f100/f{n}"), &tiny(n), None).unwrap();
            }
        });
        let big = random(BIG);
        self.commit(&opened, &workspace, "commit.random_64mib", |m| {
            m.mkdir("/big", None).unwrap();
            m.write("/big/random.bin", &big, None).unwrap();
        });
        self.commit(&opened, &workspace, "commit.second_one_file", |m| {
            m.write("/f100/f7", b"changed once\n", None).unwrap();
        });
        opened.close(Some(workspace));

        // Cold reads: a new Store open and a new mount-bound Workspace.
        let opened = Opened::open(dir, selection(), "b");
        let workspace = self.scenario(&opened, "workspace_reader_open.cold", || {
            opened.mount(2, branch)
        });
        let m = Mutation::new(&workspace).unwrap();
        self.scenario(&opened, "read.stat_files_1000", || {
            for n in 1..=1000 {
                let stat = m.resolve(&format!("/f1000/f{n}")).unwrap();
                assert_eq!(stat.logical_len, tiny(n).len() as u64);
            }
        });
        self.scenario(&opened, "read.sequential_random_64mib", || {
            let serial = m.resolve("/big/random.bin").unwrap().serial;
            let mut offset = 0usize;
            while offset < BIG {
                let piece = m.read(serial, offset as u64, READ).unwrap();
                assert_eq!(piece, big[offset..offset + piece.len()], "at {offset}");
                assert!(!piece.is_empty());
                offset += piece.len();
            }
        });
        m.finish().unwrap();
        opened.close(Some(workspace));
    }

    /// Writes through `change` (counted as `setup`), then one measured Commit.
    fn commit(
        &self,
        opened: &Opened,
        workspace: &BoundWorkspace,
        name: &str,
        change: impl FnOnce(&mut Mutation<'_>),
    ) {
        let (_, setup) = Phase::measure(opened, || {
            let mut m = Mutation::new(workspace).unwrap();
            change(&mut m);
            m.finish().unwrap();
        });
        let mut total = setup.writer;
        total.accumulate(&setup.readers);
        println!(
            "ROUND_TRIPS_SETUP {{\"backend\":\"{}\",\"scenario\":\"{name}\",\"total\":{},\"ms\":{:.1}}}",
            self.backend,
            work_json(&total),
            setup.ms
        );
        let before = self.server_snapshot();
        let recording = Recording::start();
        let sessions = opened.service.opened.sql_sessions();
        attribute_sql(Some(Box::new(move || {
            let (writer, readers, _) = sessions.totals();
            let mut total = writer.unwrap_or_default();
            total.accumulate(&readers.unwrap_or_default());
            total
        })));
        let (outcome, mut phase) = Phase::measure(opened, || {
            workspace.commit_captured(&CommitOptions {
                force: false,
                message: Some(name.to_owned()),
            })
        });
        let reports = recording.finish();
        for (name, work) in phase_sql() {
            phase.phase_sql.entry(name).or_default().accumulate(&work);
        }
        attribute_sql(None);
        if let Err(failure) = &outcome {
            panic!("{name}: {failure}");
        }
        for report in &reports {
            if let Some(root) = report.root() {
                flatten(root, String::new(), 0, &mut phase.phases_us);
            }
        }
        self.emit(name, &phase, self.server_delta(before));
    }
}

/// Phase wall by path, three levels below each root, summed per path.
fn flatten(node: &TimingNode, prefix: String, depth: usize, out: &mut BTreeMap<String, u128>) {
    let path = if prefix.is_empty() {
        node.name().to_owned()
    } else {
        format!("{prefix}/{}", node.name())
    };
    *out.entry(path.clone()).or_default() += node.elapsed().as_micros();
    if depth < 3 {
        for child in node.children() {
            flatten(child, path.clone(), depth + 1, out);
        }
    }
}

fn work_json(w: &SqlWork) -> String {
    format!(
        "{{\"connects\":{},\"round_trips\":{},\"prepares\":{},\"statements\":{},\"transactions\":{},\"write_transactions\":{},\"commits\":{},\"rollbacks\":{},\"read_snapshots\":{},\"returned_rows\":{},\"bound_bytes\":{}}}",
        w.connects,
        w.round_trips,
        w.prepares,
        w.statements,
        w.transactions,
        w.write_transactions,
        w.commits,
        w.rollbacks,
        w.read_snapshots,
        w.returned_rows,
        w.bound_bytes
    )
}

fn load1() -> String {
    std::fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|text| text.split_whitespace().next().map(str::to_owned))
        .unwrap_or_else(|| "null".to_owned())
}

/// fs-bench-quick C01's body: `echo $n > f$n`.
fn tiny(n: usize) -> Vec<u8> {
    format!("{n}\n").into_bytes()
}

/// A 23-byte file body, distinct per `n` and similar to its neighbours.
fn small(n: usize) -> Vec<u8> {
    format!("round trip file {n:06}\n").into_bytes()
}

/// `length` non-repeating bytes from a fixed xorshift64* stream.
fn random(length: usize) -> Vec<u8> {
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut out = Vec::with_capacity(length + 8);
    while out.len() < length {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        out.extend_from_slice(&state.wrapping_mul(0x2545_F491_4F6C_DD1D).to_le_bytes());
    }
    out.truncate(length);
    out
}

/// Server-side `pg_stat_statements` rows of the test database, by query id:
/// calls, rows and the normalized text.
type Statements = HashMap<i64, (i64, i64, String)>;

/// A separate observer connection; its own snapshot queries are excluded.
struct Server {
    client: postgres::Client,
}
impl Server {
    fn connect(url: &str) -> Option<Self> {
        let mut client = postgres::Client::connect(url, postgres::NoTls).ok()?;
        match client.batch_execute("CREATE EXTENSION IF NOT EXISTS pg_stat_statements") {
            Ok(()) => Some(Self { client }),
            Err(error) => {
                println!("ROUND_TRIPS_NOTE pg_stat_statements unavailable: {error}");
                None
            }
        }
    }
    /// One `SELECT 1` simple-query round trip, 200 times: the route's
    /// latency from this process, printed as min/median/p90 microseconds.
    fn calibrate(&mut self) {
        let mut us: Vec<u128> = (0..200)
            .map(|_| {
                let started = Instant::now();
                self.client.simple_query("SELECT 1").unwrap();
                started.elapsed().as_micros()
            })
            .collect();
        us.sort_unstable();
        let version: String = self
            .client
            .query_one("SHOW server_version", &[])
            .unwrap()
            .get(0);
        println!(
            "ROUND_TRIPS {{\"backend\":\"postgresql\",\"scenario\":\"calibration.select1\",\"samples\":200,\"min_us\":{},\"median_us\":{},\"p90_us\":{},\"server_version\":\"{version}\",\"load1\":{}}}",
            us[0],
            us[100],
            us[180],
            load1()
        );
    }
    fn snapshot(&mut self) -> Statements {
        let rows = self
            .client
            .query(
                "SELECT coalesce(queryid, 0), calls, rows, query FROM pg_stat_statements \
                 WHERE dbid = (SELECT oid FROM pg_database WHERE datname = current_database()) \
                 AND query NOT LIKE '%pg_stat_statements%'",
                &[],
            )
            .unwrap();
        let mut out = Statements::new();
        for row in rows {
            let entry = out
                .entry(row.get(0))
                .or_insert_with(|| (0, 0, row.get::<_, String>(3)));
            entry.0 += row.get::<_, i64>(1);
            entry.1 += row.get::<_, i64>(2);
        }
        out
    }
}

/// Total server calls between two snapshots and the twelve most-called
/// statements as `[calls, rows, text]`.
fn statements_json(before: &Statements, after: &Statements) -> String {
    let mut changed: Vec<(i64, i64, &str)> = after
        .iter()
        .filter_map(|(id, (calls, rows, text))| {
            let (calls_before, rows_before) = before.get(id).map_or((0, 0), |b| (b.0, b.1));
            (calls > &calls_before)
                .then(|| (calls - calls_before, rows - rows_before, text.as_str()))
        })
        .collect();
    changed.sort_by(|a, b| b.0.cmp(&a.0).then(a.2.cmp(b.2)));
    let calls: i64 = changed.iter().map(|c| c.0).sum();
    let top: Vec<String> = changed
        .iter()
        .take(12)
        .map(|(calls, rows, text)| {
            let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
            let text: String = text.chars().take(120).collect();
            let text = text.replace('\\', "\\\\").replace('"', "\\\"");
            format!("[{calls},{rows},\"{text}\"]")
        })
        .collect();
    format!(
        "{{\"calls\":{calls},\"distinct\":{},\"top\":[{}]}}",
        changed.len(),
        top.join(",")
    )
}
