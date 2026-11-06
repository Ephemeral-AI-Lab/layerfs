//! layerfs-project #94: the PostgreSQL statements and connections of one
//! fs-bench-quick case (Sandbox create, Workspace mount, Commit), step by
//! step, as each process of the Docker CLI route makes them. A measurement
//! tool, not a gate: it asserts only that counted work happened.
//!
//! Each case opens a fresh daemon Store, as `sandbox.create` starts a fresh
//! daemon, so every connection, light check and statement prepare a case
//! pays is counted where it happens:
//!
//! - `create.reach`: the caller's held read session (control
//!   `store/reader.rs`): attach with the light check, find the project.
//! - `create.daemon_open`: the daemon's Store (daemon `application/owner.rs`
//!   selection, light check) and, timed apart, its overlay template.
//! - The mount's caller reads no Store (#64 option B: the daemon finds the
//!   Branch by its project's name and its own).
//! - `mount.daemon_bind`, `mount.daemon_serials`: the daemon's named bind
//!   (its Workspace reader, the by-name snapshot, the root) and the initial
//!   serial window its kernel attach prepares (`control/attach.rs`).
//! - `commit.*`: one Commit; each top-level phase carries the product's SQL
//!   counters and the server's `pg_stat_statements` calls by statement.
//!
//! Every step prints `ATTRIBUTION <json>`: product counters (writer and
//! readers, live plus retired), the server's statement calls with their
//! full normalized text, the new backend pids, and wall `ms` (diagnostic:
//! the test profile, a container on the server's network, and observer
//! queries between steps). `CONNECT <json>` lines time one connection's
//! parts. Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it. Run with
//! `--test-threads=1 --nocapture`.
#![cfg(feature = "postgresql")]
use layerfs_history::{
    BranchId, ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig, HistoryName,
    LayerStackId, WorkspaceId,
};
use layerfs_persistence::{PostgresCheck, PostgresHandles, PostgresSelection, SqlWork};
use layerfs_project::{init, InitRequest};
use layerfs_storage::{ReservationBlocks, Storage, StoragePolicy};
use layerfs_store_service::bootstrap::{open_store_observed, OpenedStore};
use layerfs_store_service::store::{BoundWorkspace, CommitOptions, NamedBindRequest};
use layerfs_store_service::telemetry::{attribute_sql, mark_phases, phase_sql, Recording};
use layerfs_store_service::write::mutation::Mutation;
use layerfs_store_service::{Owner, OwnerConfig, StoreService};
use layerfs_telemetry::timer::Timing;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

const BINDING: &[u8] = b"postgresql-phase-attribution";
const CURSOR: [u8; 32] = [94; 32];
const PROJECT: &str = "attr";
const BRANCH: &str = "work";
/// The daemon's Store object cache.
const CACHE_BYTES: usize = 64 * 1024 * 1024;
/// The Commit phases each carry their own server statement counts.
const PHASES: &[&str] = &[
    "uptodate.state",
    "uptodate.head",
    "capture",
    "producer",
    "begin",
    "construct",
    "head",
    "logical.known",
    "finish",
    "logical.count",
    "bindings",
    "validate",
    "lane",
    "head.lane",
    "validate.lane",
    "publish",
    "install",
];

#[test]
fn postgresql_case_attribution() {
    let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
        return;
    };
    let dir = std::env::temp_dir().join(format!("lfs94-attr-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let schema = format!("lfs94_attr_{}", std::process::id());
    let selection = PostgresSelection::new(&url, &schema);
    let result = std::panic::catch_unwind(|| {
        // the Branch the cases mount by name (`BRANCH`)
        initialize(&dir, &selection);
        let server = Rc::new(RefCell::new(Server::connect(&url)));
        server.borrow_mut().calibrate();
        connect_anatomy(&url, &selection);
        let run = Run {
            dir: dir.clone(),
            selection: selection.clone(),
            server,
        };
        run.case("C01", &[files(1000)]);
        run.case("C13", &[one("a", b"1\n"), one("b", b"2\n")]);
        run.case("C14", &[Box::new(|_| {})]);
        run.case("C16", &[files(10_000)]);
    });
    if let Ok(mut client) = postgres::Client::connect(&url, postgres::NoTls) {
        let _ = client.batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"));
    }
    let _ = std::fs::remove_dir_all(&dir);
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

/// The first PostgreSQL session open of a process against later ones, after
/// one plain driver connect: the first open also builds the process's TLS
/// connector (persistence `session/connection.rs` `tls()`), whatever the
/// URL's `sslmode`. Run it
/// alone (`postgresql_first_open --exact`) so nothing opened a session
/// before it in this process. Each open reads only the server version
/// (`PostgresHandles::binding`), so it needs no LayerFS schema.
#[test]
fn postgresql_first_open() {
    let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
        return;
    };
    let schema = format!("lfs94_first_{}", std::process::id());
    let selection = PostgresSelection::new(&url, &schema);
    // The driver's own connect first, without TLS: name resolution and the
    // driver's first-use costs are paid here, not by the first open.
    let started = Instant::now();
    let mut client = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    let driver = started.elapsed().as_secs_f64() * 1e3;
    client.simple_query("SELECT 1").unwrap();
    let mut opens = Vec::new();
    for _ in 0..3 {
        let started = Instant::now();
        // The schema does not exist: the open connects, then its table check
        // finds no Store, which is the expected refusal.
        let refused = PostgresHandles::binding(&selection).is_err();
        assert!(refused);
        opens.push(format!("{:.3}", started.elapsed().as_secs_f64() * 1e3));
    }
    println!(
        "FIRST_OPEN {{\"opens_ms\":[{}],\"driver_connect_ms\":{driver:.3},\"load1\":{}}}",
        opens.join(","),
        load1()
    );
}

type Change = Box<dyn Fn(&mut Mutation<'_>)>;
/// One phase boundary: its name, whether it is the end, the server's
/// statement counts and the writer and reader counters there.
type Mark = (&'static str, bool, Statements, SqlWork, SqlWork);

/// fs-bench-quick C01/C16's body: `echo $n > f$n` for `n` in `1..=count`.
fn files(count: usize) -> Change {
    Box::new(move |m| {
        for n in 1..=count {
            m.write(&format!("/f{n}"), format!("{n}\n").as_bytes(), None)
                .unwrap();
        }
    })
}

fn one(name: &'static str, body: &'static [u8]) -> Change {
    Box::new(move |m| m.write(&format!("/{name}"), body, None).unwrap())
}

fn catalog() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: BINDING.to_vec(),
        cursor_key: CURSOR,
        incarnation: 1,
    }
}

/// Creates the schema, Inits an empty project and forks Branch `work`, as
/// fs-bench-quick's `store connect`, `project init` and `branch fork` do.
fn initialize(dir: &Path, selection: &PostgresSelection) -> BranchId {
    let handles =
        PostgresHandles::create(selection, StoragePolicy::frozen_default(), &catalog()).unwrap();
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let source = dir.join("source");
    std::fs::create_dir_all(&source).unwrap();
    let initialized = Timing::disabled("init", |timing| {
        init(
            &storage,
            &handles.history,
            InitRequest {
                source: &source,
                acquisition: &handles.acquisition,
                stack: LayerStackId::from_authority([94; 16]),
                name: HistoryName::new(PROJECT).unwrap(),
                scope_seed: [94; 32],
                deadline: Instant::now() + Duration::from_secs(60),
            },
            timing,
        )
    })
    .0
    .unwrap();
    let branch = BranchId::from_authority([95; 16]);
    handles
        .history
        .fork(&ForkRequest {
            stack: initialized.stack.id,
            branch,
            name: HistoryName::new(BRANCH).unwrap(),
            source: ForkSource::Layer(initialized.stack.head_layer),
        })
        .unwrap();
    std::fs::remove_dir_all(source).unwrap();
    branch
}

/// One connection's parts, each timed on its own new connection: a TCP
/// connect, a driver connect (TCP, startup and password exchange, no TLS),
/// and product session opens with each check (the first in this process
/// also builds the process's TLS connector).
fn connect_anatomy(url: &str, selection: &PostgresSelection) {
    let config: postgres::Config = url.parse().unwrap();
    let host = match &config.get_hosts()[0] {
        postgres::config::Host::Tcp(host) => host.clone(),
        _ => panic!("a TCP host"),
    };
    let port = config.get_ports().first().copied().unwrap_or(5432);
    let ms = |started: Instant| started.elapsed().as_secs_f64() * 1e3;
    let mut parts = Vec::new();
    let started = Instant::now();
    let addresses: Vec<_> = std::net::ToSocketAddrs::to_socket_addrs(&(host.as_str(), port))
        .unwrap()
        .collect();
    parts.push(("resolve", ms(started)));
    let started = Instant::now();
    drop(std::net::TcpStream::connect(addresses[0]).unwrap());
    parts.push(("tcp", ms(started)));
    let started = Instant::now();
    let mut client = postgres::Client::connect(url, postgres::NoTls).unwrap();
    parts.push(("driver_connect", ms(started)));
    let started = Instant::now();
    client.simple_query("SELECT 1").unwrap();
    parts.push(("select1", ms(started)));
    drop(client);
    let light = selection.clone().with_check(PostgresCheck::Light);
    for (name, selection) in [
        ("open_light_first", &light),
        ("open_light_second", &light),
        ("open_full", selection),
    ] {
        let started = Instant::now();
        let handles = PostgresHandles::open_read_only(selection, BINDING, CURSOR).unwrap();
        parts.push((name, ms(started)));
        drop(handles);
    }
    let fields: Vec<String> = parts
        .iter()
        .map(|(name, ms)| format!("\"{name}\":{ms:.3}"))
        .collect();
    println!(
        "CONNECT {{\"host\":\"{host}\",\"port\":{port},{},\"load1\":{}}}",
        fields.join(","),
        load1()
    );
}

struct Run {
    dir: PathBuf,
    selection: PostgresSelection,
    server: Rc<RefCell<Server>>,
}

/// One process's opened Store with its overlay owner.
struct Process {
    service: StoreService,
    owner: Owner,
    /// Wall of the Store open and of the overlay owner's start, ms.
    open_ms: f64,
    owner_ms: f64,
}
impl Process {
    fn open(dir: &Path, tag: &str, selection: PostgresSelection, capacity: usize) -> Self {
        let started = Instant::now();
        let opened: OpenedStore = open_store_observed(
            selection,
            BINDING,
            CURSOR,
            capacity,
            CACHE_BYTES,
            ReservationBlocks::default(),
        )
        .unwrap();
        let open_ms = started.elapsed().as_secs_f64() * 1e3;
        let overlay = dir.join(tag);
        std::fs::create_dir_all(&overlay).unwrap();
        let started = Instant::now();
        let owner = Owner::start(
            &overlay.join("overlay.sqlite"),
            layerfs_overlay::ProfileConfig::default(),
            OwnerConfig::default(),
        )
        .unwrap();
        let owner_ms = started.elapsed().as_secs_f64() * 1e3;
        let service = StoreService::new(opened, &owner);
        Self {
            service,
            owner,
            open_ms,
            owner_ms,
        }
    }
    fn work(&self) -> (SqlWork, SqlWork) {
        let (writer, readers, _) = self.service.opened.sql_sessions().totals();
        (writer.unwrap_or_default(), readers.unwrap_or_default())
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

impl Run {
    fn emit(&self, case: &str, step: &str, fields: &[(&str, String)]) {
        let body: Vec<String> = fields
            .iter()
            .map(|(name, value)| format!("\"{name}\":{value}"))
            .collect();
        println!(
            "ATTRIBUTION {{\"case\":\"{case}\",\"step\":\"{step}\",{},\"load1\":{}}}",
            body.join(","),
            load1()
        );
    }

    /// Runs `body` as one step: the server's statements and new backends
    /// around it, and `work` (writer, readers) before and after.
    fn step<T>(
        &self,
        case: &str,
        step: &str,
        work: &dyn Fn() -> (SqlWork, SqlWork),
        body: impl FnOnce() -> T,
        extra: Vec<(&str, String)>,
    ) -> T {
        let before = self.server.borrow_mut().snapshot();
        let pids = self.server.borrow_mut().backends();
        let (writer, readers) = work();
        let started = Instant::now();
        let value = body();
        let ms = started.elapsed().as_secs_f64() * 1e3;
        let (writer_after, readers_after) = work();
        let after = self.server.borrow_mut().snapshot();
        let new: Vec<String> = (self.server.borrow_mut().backends())
            .difference(&pids)
            .map(|pid| pid.to_string())
            .collect();
        let mut fields = vec![
            ("ms", format!("{ms:.2}")),
            ("writer", work_json(&writer_after.since(&writer))),
            ("readers", work_json(&readers_after.since(&readers))),
            ("new_backends", format!("[{}]", new.join(","))),
            ("server", statements_json(&before, &after)),
        ];
        fields.extend(extra);
        self.emit(case, step, &fields);
        value
    }

    fn case(&self, case: &str, changes: &[Change]) {
        let none = || (SqlWork::default(), SqlWork::default());
        let light = self.selection.clone().with_check(PostgresCheck::Light);
        // create: the caller's reach check on its held session
        self.step(
            case,
            "create.reach",
            &none,
            || {
                let (binding, handles) = PostgresHandles::attach_read_only(&light, CURSOR).unwrap();
                assert_eq!(binding, BINDING);
                let stack = handles
                    .history
                    .layer_stack_named(&HistoryName::new(PROJECT).unwrap())
                    .unwrap();
                assert!(stack.is_some());
                let work = handles.diagnostics().unwrap();
                println!(
                    "ATTRIBUTION_SESSION {{\"case\":\"{case}\",\"step\":\"create.reach\",\"held\":{}}}",
                    work_json(&work)
                );
            },
            vec![],
        );
        // create: the daemon's Store, as `application/owner.rs` opens it
        let daemon = self.step(
            case,
            "create.daemon_open",
            &none,
            || {
                Process::open(
                    &self.dir,
                    &format!("{case}-daemon"),
                    light.clone(),
                    OwnerConfig::default().namespaces,
                )
            },
            vec![],
        );
        let (writer, readers) = daemon.work();
        self.emit(
            case,
            "create.daemon_open.split",
            &[
                ("store_open_ms", format!("{:.2}", daemon.open_ms)),
                ("overlay_owner_ms", format!("{:.2}", daemon.owner_ms)),
                ("writer", work_json(&writer)),
                ("readers", work_json(&readers)),
            ],
        );
        // mount: the caller reads no Store (#64 option B); the daemon finds
        // the Branch by name on the Workspace's own reader
        let daemon_work = || daemon.work();
        let workspace = self.step(
            case,
            "mount.daemon_bind",
            &daemon_work,
            || {
                daemon
                    .service
                    .store
                    .bind_named(
                        daemon.owner.client(),
                        NamedBindRequest {
                            workspace: WorkspaceId::from_authority([7; 32]).unwrap(),
                            project: PROJECT.to_owned(),
                            branch: BRANCH.to_owned(),
                        },
                    )
                    .unwrap()
                    .workspace
            },
            vec![],
        );
        self.step(
            case,
            "mount.daemon_serials",
            &daemon_work,
            || {
                let operation = workspace.operation().unwrap();
                operation
                    .workspace()
                    .prepare_serials(operation.ports())
                    .unwrap();
            },
            vec![],
        );
        for (n, change) in changes.iter().enumerate() {
            let label = if n == 0 { "exec" } else { "exec_2" };
            self.step(
                case,
                label,
                &daemon_work,
                || {
                    let mut m = Mutation::new(&workspace).unwrap();
                    change(&mut m);
                    m.finish().unwrap();
                },
                vec![],
            );
            let label = if n == 0 { "commit" } else { "commit_2" };
            self.commit(case, label, &daemon, &workspace);
        }
        daemon.close(Some(workspace));
    }

    /// One Commit: the whole step, then each marked phase's product SQL
    /// work and server statements (inclusive; nested phases overlap).
    fn commit(&self, case: &str, label: &str, daemon: &Process, workspace: &BoundWorkspace) {
        let marks: Rc<RefCell<Vec<Mark>>> = Rc::default();
        let sessions = Rc::new(daemon.service.opened.sql_sessions());
        let recording = Recording::start();
        let probe = sessions.clone();
        attribute_sql(Some(Box::new(move || {
            let (writer, readers, _) = probe.totals();
            let mut total = writer.unwrap_or_default();
            total.accumulate(&readers.unwrap_or_default());
            total
        })));
        let (server, kept, totals) = (self.server.clone(), marks.clone(), sessions.clone());
        mark_phases(Some(Box::new(move |name, end| {
            if !PHASES.contains(&name) {
                return;
            }
            let (writer, readers, _) = totals.totals();
            let snapshot = server.borrow_mut().snapshot();
            kept.borrow_mut().push((
                name,
                end,
                snapshot,
                writer.unwrap_or_default(),
                readers.unwrap_or_default(),
            ));
        })));
        let daemon_work = || daemon.work();
        let outcome = self.step(
            case,
            label,
            &daemon_work,
            || {
                workspace.commit_captured(&CommitOptions {
                    force: false,
                    message: Some(label.to_owned()),
                })
            },
            vec![],
        );
        mark_phases(None);
        let sql: BTreeMap<&str, SqlWork> = phase_sql().into_iter().collect();
        attribute_sql(None);
        drop(recording.finish());
        let saved = outcome
            .unwrap_or_else(|f| panic!("{case} {label}: {f}"))
            .saved;
        println!(
            "STORAGE {{\"case\":\"{case}\",\"step\":\"{label}\",\"inserted\":{},\"packs\":{},\"stored\":{},\"metadata\":{}}}",
            saved.inserted, saved.packs, saved.stored_bytes, saved.metadata_bytes
        );
        let marks = marks.borrow();
        let mut open: HashMap<&str, (Statements, SqlWork, SqlWork)> = HashMap::new();
        for (name, end, snapshot, writer, readers) in marks.iter() {
            if !end {
                open.insert(name, (snapshot.clone(), *writer, *readers));
                continue;
            }
            let Some((before, writer_before, readers_before)) = open.remove(name) else {
                continue;
            };
            let product = sql.get(name).copied().unwrap_or_default();
            self.emit(
                case,
                &format!("{label}.{name}"),
                &[
                    ("phase_sql", work_json(&product)),
                    ("writer", work_json(&writer.since(&writer_before))),
                    ("readers", work_json(&readers.since(&readers_before))),
                    ("server", statements_json(&before, snapshot)),
                ],
            );
        }
    }
}

fn work_json(w: &SqlWork) -> String {
    format!(
        "{{\"connects\":{},\"round_trips\":{},\"prepares\":{},\"statements\":{},\"transactions\":{},\"write_transactions\":{},\"commits\":{},\"rollbacks\":{},\"read_snapshots\":{},\"returned_rows\":{}}}",
        w.connects,
        w.round_trips,
        w.prepares,
        w.statements,
        w.transactions,
        w.write_transactions,
        w.commits,
        w.rollbacks,
        w.read_snapshots,
        w.returned_rows
    )
}

fn load1() -> String {
    std::fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|text| text.split_whitespace().next().map(str::to_owned))
        .unwrap_or_else(|| "null".to_owned())
}

/// Server-side `pg_stat_statements` rows of the test database, by query id:
/// calls and the normalized text.
type Statements = HashMap<i64, (i64, String)>;

/// A separate observer connection; its own queries are excluded.
struct Server {
    client: postgres::Client,
}
impl Server {
    fn connect(url: &str) -> Self {
        let mut client = postgres::Client::connect(url, postgres::NoTls).unwrap();
        client
            .batch_execute("CREATE EXTENSION IF NOT EXISTS pg_stat_statements")
            .unwrap();
        Self { client }
    }
    /// One `SELECT 1` simple-query round trip, 100 times, from this process.
    fn calibrate(&mut self) {
        let mut us: Vec<u128> = (0..100)
            .map(|_| {
                let started = Instant::now();
                self.client.simple_query("SELECT 1").unwrap();
                started.elapsed().as_micros()
            })
            .collect();
        us.sort_unstable();
        println!(
            "CALIBRATION {{\"select1_samples\":100,\"min_us\":{},\"median_us\":{},\"p90_us\":{},\"load1\":{}}}",
            us[0],
            us[50],
            us[90],
            load1()
        );
    }
    fn snapshot(&mut self) -> Statements {
        let rows = self
            .client
            .query(
                "SELECT coalesce(queryid, 0), calls, query FROM pg_stat_statements \
                 WHERE dbid = (SELECT oid FROM pg_database WHERE datname = current_database()) \
                 AND query NOT LIKE '%pg_stat_statements%' AND query NOT LIKE '%pg_stat_activity%'",
                &[],
            )
            .unwrap();
        let mut out = Statements::new();
        for row in rows {
            let entry = out
                .entry(row.get(0))
                .or_insert_with(|| (0, row.get::<_, String>(2)));
            entry.0 += row.get::<_, i64>(1);
        }
        out
    }
    /// The client backends of this database other than this observer.
    fn backends(&mut self) -> BTreeSet<i32> {
        self.client
            .query(
                "SELECT pid FROM pg_stat_activity WHERE datname = current_database() \
                 AND pid <> pg_backend_pid() AND backend_type = 'client backend'",
                &[],
            )
            .unwrap()
            .iter()
            .map(|row| row.get(0))
            .collect()
    }
}

/// Total server calls between two snapshots and every changed statement as
/// `[calls, text]`, most called first.
fn statements_json(before: &Statements, after: &Statements) -> String {
    let mut changed: Vec<(i64, &str)> = after
        .iter()
        .filter_map(|(id, (calls, text))| {
            let calls_before = before.get(id).map_or(0, |b| b.0);
            (calls > &calls_before).then(|| (calls - calls_before, text.as_str()))
        })
        .collect();
    changed.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
    let calls: i64 = changed.iter().map(|c| c.0).sum();
    let all: Vec<String> = changed
        .iter()
        .map(|(calls, text)| {
            let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
            let text: String = text.chars().take(160).collect();
            let text = text.replace('\\', "\\\\").replace('"', "\\\"");
            format!("[{calls},\"{text}\"]")
        })
        .collect();
    format!(
        "{{\"calls\":{calls},\"distinct\":{},\"statements\":[{}]}}",
        changed.len(),
        all.join(",")
    )
}
