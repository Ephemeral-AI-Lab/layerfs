//! #818 probe: time, client round trips and statements of every `fs.*`
//! Store call and of Namespace Init, through the SDK's own call route
//! (`open_store_observed` with the SDK's reader capacity and cache, then
//! `methods::call`), in three arms chosen by `FS818_ARM`:
//!
//! - `sqlite`: a sealed SQLite Store (Normal profile) in a temporary dir;
//! - `pg`: PostgreSQL at `LAYERFS_TEST_POSTGRES_URL`;
//! - `pg10`: the same server with 10 ms of `netem` delay added on the
//!   container named by `FS818_NETEM_CONTAINER`, after setup and before
//!   the measured calls (Init: before Init), removed afterwards.
//!
//! `FS818_GROUP` selects `reads`, `history` or `writes`; each group runs in
//! its own process from the same setup (Init of the tree, a `work` Branch,
//! 100 `fs.write` Commits), so each command stays under two minutes.
//!
//! A measurement tool, not a gate. Each measured case prints one line
//! `FS_RT <json>`. One sample per case. Run with `--test-threads=1 --nocapture`.
use layerfs_bridge::control::{SdkCall, SdkValue};
use layerfs_history::{
    BranchId, ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig, HistoryName,
    LayerStackId,
};
use layerfs_persistence::{
    PersistenceConfig, PostgresHandles, PostgresSelection, SqlWork, SqliteAcquisitionSchema,
    SqliteHandles, SqlitePersistenceProfile, StoreSelection,
};
use layerfs_project::{init, InitRequest};
use layerfs_storage::{ReservationBlocks, Storage, StoragePolicy};
use layerfs_store_service::bootstrap::open_store_observed;
use layerfs_store_service::{Owner, OwnerConfig, StoreService};
use layerfs_telemetry::timer::Timing;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const BINDING: &[u8] = b"fs-call-round-trips";
const CURSOR: [u8; 32] = [81; 32];
const PROJECT: &str = "rt";
/// The SDK Store service's reader capacity and object cache
/// (`store_service.rs`: `CALLS`, `CACHE_BYTES`).
const CALLS: usize = 64;
const CACHE_BYTES: usize = 64 * 1024;

fn catalog() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: BINDING.to_vec(),
        cursor_key: CURSOR,
        incarnation: 1,
    }
}

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

/// Server statement calls by query id: (calls, top-level calls, text).
/// `pg_stat_statements.track=all` also counts nested statements such as
/// foreign-key trigger checks; top-level calls are the ones a client sent.
type Statements = HashMap<i64, (i64, i64, String)>;

struct Server {
    client: postgres::Client,
}
impl Server {
    fn connect(url: &str) -> Option<Self> {
        let mut client = postgres::Client::connect(url, postgres::NoTls).ok()?;
        client
            .batch_execute("CREATE EXTENSION IF NOT EXISTS pg_stat_statements")
            .ok()?;
        client
            .query("SELECT 1 FROM pg_stat_statements LIMIT 1", &[])
            .ok()?;
        Some(Self { client })
    }
    /// `SELECT 1` simple-query latency from this process, 200 samples.
    fn calibrate(&mut self) {
        let mut us: Vec<u128> = (0..200)
            .map(|_| {
                let started = Instant::now();
                self.client.simple_query("SELECT 1").unwrap();
                started.elapsed().as_micros()
            })
            .collect();
        us.sort_unstable();
        println!(
            "FS_RT {{\"case\":\"calibration.select1\",\"min_us\":{},\"median_us\":{},\"p90_us\":{}}}",
            us[0], us[100], us[180]
        );
    }
    fn snapshot(&mut self) -> Statements {
        let rows = self
            .client
            .query(
                "SELECT coalesce(queryid, 0), calls, CASE WHEN toplevel THEN calls ELSE 0 END, query FROM pg_stat_statements \
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

/// Total server calls between two snapshots and the six most-called templates.
fn server_json(before: &Statements, after: &Statements) -> String {
    let mut changed: Vec<(i64, &str, i64)> = after
        .iter()
        .filter_map(|(id, (calls, top, text))| {
            let (was, was_top) = before.get(id).map_or((0, 0), |b| (b.0, b.1));
            (*calls > was).then(|| (calls - was, text.as_str(), top - was_top))
        })
        .collect();
    changed.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
    let calls: i64 = changed.iter().map(|c| c.0).sum();
    let toplevel: i64 = changed.iter().map(|c| c.2).sum();
    let top: Vec<String> = changed
        .iter()
        .take(6)
        .map(|(calls, text, _)| {
            let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
            let text: String = text.chars().take(90).collect();
            format!(
                "[{calls},\"{}\"]",
                text.replace('\\', "\\\\").replace('"', "\\\"")
            )
        })
        .collect();
    format!(
        "{{\"calls\":{calls},\"toplevel_calls\":{toplevel},\"distinct\":{},\"top\":[{}]}}",
        changed.len(),
        top.join(",")
    )
}

fn work_json(w: &SqlWork) -> String {
    format!(
        "{{\"connects\":{},\"round_trips\":{},\"prepares\":{},\"statements\":{},\"transactions\":{},\"returned_rows\":{}}}",
        w.connects, w.round_trips, w.prepares, w.statements, w.transactions, w.returned_rows
    )
}


fn scratch(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs818-{label}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Sqlite,
    Pg,
    Pg10,
}

/// One arm: its backend, and for PostgreSQL the URL, schema, observer and
/// delay container.
struct Arm {
    kind: Kind,
    name: &'static str,
    url: String,
    schema: String,
    server: Option<Server>,
    netem: Option<String>,
    dir: PathBuf,
    /// The sealed SQLite Store, once Init has run.
    sqlite: Option<PathBuf>,
}
impl Arm {
    /// The arm `FS818_ARM` names, or `None` with a NOT_RUN note.
    fn from_env(label: &str) -> Option<Self> {
        let (kind, name) = match std::env::var("FS818_ARM").as_deref() {
            Ok("sqlite") => (Kind::Sqlite, "sqlite"),
            Ok("pg") => (Kind::Pg, "pg"),
            Ok("pg10") => (Kind::Pg10, "pg10"),
            _ => {
                eprintln!("NOT_RUN: FS818_ARM must be sqlite, pg or pg10");
                return None;
            }
        };
        let dir = scratch(label);
        let mut arm = Self {
            kind,
            name,
            url: String::new(),
            schema: format!("lfs818_{label}_{}", std::process::id()),
            server: None,
            netem: None,
            dir,
            sqlite: None,
        };
        if kind != Kind::Sqlite {
            let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
                eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
                return None;
            };
            let Some(server) = Server::connect(&url) else {
                eprintln!("NOT_RUN: pg_stat_statements unavailable");
                return None;
            };
            arm.url = url;
            arm.server = Some(server);
        }
        if kind == Kind::Pg10 {
            let Ok(container) = std::env::var("FS818_NETEM_CONTAINER") else {
                eprintln!("NOT_RUN: FS818_NETEM_CONTAINER is unset");
                return None;
            };
            arm.netem = Some(container);
        }
        Some(arm)
    }
    fn snapshot(&mut self) -> Option<Statements> {
        self.server.as_mut().map(Server::snapshot)
    }
    fn server_delta(&mut self, before: Option<Statements>) -> String {
        match (before, self.server.as_mut()) {
            (Some(before), Some(server)) => server_json(&before, &server.snapshot()),
            _ => "null".to_owned(),
        }
    }
    /// Adds (`on`) or removes the 10 ms delay, then prints the observer's
    /// `SELECT 1` latency.
    fn delay(&mut self, on: bool) {
        let Some(container) = &self.netem else { return };
        let tc = if on {
            "tc qdisc add dev eth0 root netem delay 10ms"
        } else {
            "tc qdisc del dev eth0 root"
        };
        let status = std::process::Command::new("docker")
            .args([
                "run",
                "--rm",
                "--net",
                &format!("container:{container}"),
                "--cap-add",
                "NET_ADMIN",
                "alpine:3",
                "sh",
                "-c",
                &format!("apk add --no-cache -q iproute2-tc >/dev/null 2>&1 && {tc}"),
            ])
            .status()
            .expect("docker");
        assert!(status.success() || !on, "netem {tc}");
        if on {
            if let Some(server) = self.server.as_mut() {
                server.calibrate();
            }
        }
    }
    /// Inits `source`, prints the Init's work (`setup` names it a setup step
    /// rather than a measured case), and forks `main`.
    fn initialize(&mut self, source: &Path, case: &str) {
        let request = |acquisition| InitRequest {
            source,
            acquisition,
            stack: LayerStackId::from_authority([81; 16]),
            name: HistoryName::new(PROJECT).unwrap(),
            scope_seed: [81; 32],
            deadline: Instant::now() + Duration::from_secs(115),
        };
        let fork = |history: &dyn HistoryCatalog, stack, head| {
            history
                .fork(&ForkRequest {
                    stack,
                    branch: BranchId::from_authority([82; 16]),
                    name: HistoryName::new("main").unwrap(),
                    source: ForkSource::Layer(head),
                })
                .unwrap();
        };
        let measured = !case.starts_with("setup.");
        if self.kind == Kind::Sqlite {
            let config = PersistenceConfig::sqlite(self.dir.join("store.sqlite"))
                .with_sqlite_profile(SqlitePersistenceProfile::Normal)
                .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables);
            let handles =
                SqliteHandles::create(config, StoragePolicy::frozen_default(), &catalog()).unwrap();
            let storage = Storage::new(handles.storage.clone()).unwrap();
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
            let client = handles.diagnostics().unwrap().since(&before);
            self.print_init(case, done.entries, ms, &client, "null".into());
            fork(&handles.history, done.stack.id, done.stack.head_layer);
            drop(storage);
            self.sqlite = Some(handles.seal().unwrap().path);
            return;
        }
        let selection = PostgresSelection::new(&self.url, &self.schema);
        let handles =
            PostgresHandles::create(&selection, StoragePolicy::frozen_default(), &catalog())
                .unwrap();
        let storage = Storage::new(handles.storage.clone()).unwrap();
        if measured {
            self.delay(true);
        }
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
        let client = handles.diagnostics().unwrap().since(&before);
        let server = self.server_delta(server_before);
        if measured {
            self.delay(false);
        }
        self.print_init(case, done.entries, ms, &client, server);
        fork(&handles.history, done.stack.id, done.stack.head_layer);
    }
    fn print_init(&self, case: &str, entries: u64, ms: f64, client: &SqlWork, server: String) {
        println!(
            "FS_RT {{\"arm\":\"{}\",\"case\":\"{case}\",\"method\":\"project.init\",\"entries\":{entries},\"ms\":{ms:.1},\"client_storage\":{},\"server\":{server},\"load1\":\"{}\"}}",
            self.name,
            work_json(client),
            load1()
        );
    }
    /// The Store as the SDK opens it, with an overlay owner.
    fn open(&self) -> (StoreService, Owner) {
        let selection: StoreSelection = match &self.sqlite {
            Some(path) => StoreSelection::Sqlite(
                PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Normal),
            ),
            None => StoreSelection::Postgres(PostgresSelection::new(&self.url, &self.schema)),
        };
        let opened = open_store_observed(
            selection,
            BINDING,
            CURSOR,
            CALLS,
            CACHE_BYTES,
            ReservationBlocks::default(),
        )
        .unwrap();
        let config = OwnerConfig::default();
        let owner = Owner::start(
            &self.dir.join("overlay.sqlite"),
            layerfs_overlay::ProfileConfig::default(),
            config,
        )
        .unwrap();
        (StoreService::new(opened, &owner), owner)
    }
    fn finish(mut self) {
        self.delay(false);
        if self.kind != Kind::Sqlite {
            if let Ok(mut client) = postgres::Client::connect(&self.url, postgres::NoTls) {
                let _ = client.batch_execute(&format!("DROP SCHEMA IF EXISTS {} CASCADE", self.schema));
            }
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn load1() -> String {
    std::process::Command::new("sysctl")
        .args(["-n", "vm.loadavg"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.split_whitespace().nth(1).map(str::to_owned))
        .unwrap_or_default()
}

fn work(service: &StoreService) -> SqlWork {
    let all = service.opened.diagnostics().unwrap();
    let mut total = all.writer;
    for reader in &all.readers {
        total.accumulate(reader);
    }
    total
}

fn call(
    service: &StoreService,
    method: &str,
    fields: Vec<(&str, SdkValue)>,
) -> Result<SdkValue, String> {
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
    layerfs_store_service::methods::call(service, &call)
        .map_err(|r| format!("{}: {}", r.code, r.message))
}

/// One measured call; a refusal is printed, not fatal.
fn measure(
    arm: &mut Arm,
    service: &StoreService,
    case: &str,
    method: &str,
    fields: Vec<(&str, SdkValue)>,
) -> Option<SdkValue> {
    let client_before = work(service);
    let before = arm.snapshot();
    let started = Instant::now();
    let reply = call(service, method, fields);
    let ms = started.elapsed().as_secs_f64() * 1e3;
    let client = work(service).since(&client_before);
    let server = arm.server_delta(before);
    let outcome = match &reply {
        Ok(value) => value.get_text("kind").unwrap_or("ok").to_owned(),
        Err(refusal) => format!("refused {refusal}").replace('"', "'"),
    };
    println!(
        "FS_RT {{\"arm\":\"{}\",\"case\":\"{case}\",\"method\":\"{method}\",\"outcome\":\"{outcome}\",\"ms\":{ms:.2},\"client\":{},\"server\":{server},\"load1\":\"{}\"}}",
        arm.name,
        work_json(&client),
        load1()
    );
    reply.ok()
}

fn commit_id(reply: &SdkValue) -> String {
    reply
        .get("commit")
        .and_then(|c| c.get_text("id"))
        .unwrap_or_else(|| panic!("no commit id: {reply:?}"))
        .to_owned()
}

/// 10 directories of 100 small files under `/src`, a depth-7 file, a
/// 2,000-line text file and the files the history rewrites.
fn build_tree(source: &Path) {
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

#[test]
fn fs_calls() {
    let Some(mut arm) = Arm::from_env("fs") else {
        return;
    };
    let group = std::env::var("FS818_GROUP").unwrap_or_default();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let source = arm.dir.join("source");
        build_tree(&source);
        arm.initialize(&source, "setup.init_nested_1k");
        let (service, owner) = arm.open();
        let s = &service;
        let m = "work";
        call(s, "branch.fork", vec![("name", t(m))]).unwrap();

        // Setup: 100 Commits on `work` alternating a.txt (one appended line
        // each) and other.txt, through fs.write.
        let (before, at) = (work(s), Instant::now());
        let (mut ids, mut text) = (Vec::new(), String::from("v0\n"));
        for n in 1..=100 {
            let (path, bytes) = if n % 2 == 1 {
                text.push_str(&format!("v{n}\n"));
                ("/hist/a.txt", text.clone())
            } else {
                ("/hist/other.txt", format!("o{n}\n"))
            };
            let reply = call(
                s,
                "fs.write",
                vec![
                    ("branch", t(m)),
                    ("path", t(path)),
                    ("bytes", SdkValue::Bytes(bytes.into_bytes())),
                ],
            )
            .unwrap();
            ids.push(commit_id(&reply));
        }
        println!(
            "FS_RT {{\"arm\":\"{}\",\"case\":\"setup.history_100_writes_total\",\"ms\":{:.1},\"client\":{}}}",
            arm.name,
            at.elapsed().as_secs_f64() * 1e3,
            work_json(&work(s).since(&before))
        );

        arm.delay(true);
        let path = |p: &str| ("path", t(p));
        let branch = || ("branch", t(m));
        let mut go = |case: &str, method: &str, fields: Vec<(&str, SdkValue)>| {
            measure(&mut arm, s, case, method, fields)
        };
        match group.as_str() {
            "reads" => {
                go("stat.depth3", "fs.stat", vec![branch(), path("/src/d05/f050.txt")]);
                go("stat.depth7", "fs.stat", vec![branch(), path("/deep/a/b/c/d/e/leaf.txt")]);
                go("stat.root", "fs.stat", vec![branch(), path("/")]);
                go("read.small_depth3", "fs.read", vec![branch(), path("/src/d05/f050.txt")]);
                go("read.2000_lines", "fs.read", vec![branch(), path("/text/big.txt")]);
                go(
                    "read.lines_100_200",
                    "fs.read",
                    vec![
                        branch(),
                        path("/text/big.txt"),
                        ("startLine", SdkValue::Int(100)),
                        ("endLine", SdkValue::Int(200)),
                    ],
                );
                go(
                    "list.dir_100",
                    "fs.list",
                    vec![branch(), path("/src/d05"), ("pageSize", SdkValue::Int(100))],
                );
                go("list.root", "fs.list", vec![branch(), path("/")]);
                go(
                    "glob.dir_100",
                    "fs.glob",
                    vec![branch(), path("/src/d05"), ("pattern", t("f0*"))],
                );
                go("glob.tree", "fs.glob", vec![branch(), path("/"), ("pattern", t("*"))]);
                go(
                    "grep.one_file_dir",
                    "fs.grep",
                    vec![branch(), path("/text"), ("pattern", t("needle"))],
                );
                go(
                    "grep.tree_1k",
                    "fs.grep",
                    vec![branch(), path("/"), ("pattern", t("needle"))],
                );
                let selector =
                    |id: &str| SdkValue::map([("commit", t(id)), ("path", t("/hist/a.txt"))]);
                go(
                    "diff_files.h10_vs_h99",
                    "fs.diff_files",
                    vec![("left", selector(&ids[9])), ("right", selector(&ids[98]))],
                );
            }
            "history" => {
                go(
                    "blame.h10",
                    "fs.blame",
                    vec![branch(), path("/hist/a.txt"), ("commit", t(&ids[9]))],
                );
                go("blame.h100", "fs.blame", vec![branch(), path("/hist/a.txt")]);
                go("revision.h100", "fs.revision", vec![branch(), path("/hist/a.txt")]);
            }
            "writes" => {
                go(
                    "write.new_file",
                    "fs.write",
                    vec![
                        branch(),
                        path("/src/d05/new.txt"),
                        ("bytes", SdkValue::Bytes(b"new\n".to_vec())),
                    ],
                );
                go(
                    "write.overwrite",
                    "fs.write",
                    vec![
                        branch(),
                        path("/src/d05/f050.txt"),
                        ("bytes", SdkValue::Bytes(b"changed\n".to_vec())),
                    ],
                );
                go(
                    "edit.2000_lines",
                    "fs.edit",
                    vec![
                        branch(),
                        path("/text/big.txt"),
                        ("oldText", t("line 1000\n")),
                        ("newText", t("LINE 1000\n")),
                    ],
                );
                go("mkdir", "fs.mkdir", vec![branch(), path("/newdir")]);
                go(
                    "cp.file",
                    "fs.cp",
                    vec![branch(), ("from", t("/src/d05/f051.txt")), ("to", t("/newdir/copy.txt"))],
                );
                go(
                    "mv.file",
                    "fs.mv",
                    vec![branch(), ("from", t("/newdir/copy.txt")), ("to", t("/newdir/moved.txt"))],
                );
                go("rm.file", "fs.rm", vec![branch(), path("/newdir/moved.txt")]);
                let changes: Vec<SdkValue> = (0..10)
                    .map(|n| {
                        SdkValue::map([
                            ("op", t("write")),
                            ("path", t(&format!("/src/d06/a{n}.txt"))),
                            ("bytes", SdkValue::Bytes(format!("a{n}\n").into_bytes())),
                        ])
                    })
                    .collect();
                go(
                    "apply.10_writes",
                    "fs.apply",
                    vec![branch(), ("changes", SdkValue::List(changes))],
                );
                go(
                    "rm.dir_100_recursive",
                    "fs.rm",
                    vec![branch(), path("/src/d07"), ("recursive", SdkValue::Bool(true))],
                );
            }
            other => panic!("FS818_GROUP must be reads, history or writes, not {other:?}"),
        }
        drop(service);
        owner.stop().unwrap();
    }));
    arm.finish();
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

/// Init of `dirs` directories of `per` small files each.
fn init_case(case: &str, dirs: usize, per: usize) {
    let Some(mut arm) = Arm::from_env("init") else {
        return;
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let source = arm.dir.join("source");
        for d in 0..dirs {
            let sub = source.join(format!("d{d:03}"));
            std::fs::create_dir_all(&sub).unwrap();
            for f in 0..per {
                std::fs::write(sub.join(format!("f{f:05}.txt")), format!("file {d} {f}\n"))
                    .unwrap();
            }
        }
        arm.initialize(&source, case);
    }));
    arm.finish();
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

#[test]
fn init_nested_1k() {
    init_case("init.nested_1k", 10, 100);
}

#[test]
fn init_flat_1k() {
    init_case("init.flat_1k", 1, 1000);
}

#[test]
fn init_nested_10k() {
    init_case("init.nested_10k", 100, 100);
}

#[test]
fn init_flat_10k() {
    init_case("init.flat_10k", 1, 10_000);
}
