//! #811 V2: the processes of the multi-host chaos run. One test binary,
//! several roles, each a test that does nothing (NOT_RUN) unless
//! `CHAOS811_ROLE` names it. `tools/chaos811.py` copies the built binary
//! into Linux "host" containers on a private Docker network beside one
//! `postgres:17` server and runs the roles there:
//!
//! * `setup`: creates the PostgreSQL Store (project `adv`) and its Branches
//!   (`shared`, `own<N>` per node, `scratch0..5`) and writes `setup.env`.
//! * `node`: one daemon's in-process Store service (as the daemon opens it:
//!   `open_store_observed`, `StoreService`, an overlay `Owner`) running a
//!   seeded schedule of mounts and unmounts (Branch mount admission, owner
//!   release), Workspace Commits (fused; plain, `--expected`, forced and
//!   forced over an expected head), `branch.fork`, `branch.remove` and
//!   Store compaction. Every operation is logged before and after it runs
//!   (`ops.jsonl`), with its harness token (the Commit message), Branch,
//!   bound and expected head, outcome class and times. One attempt each:
//!   an unknown outcome is logged and never resent; a broken or stale
//!   service is closed and a new one opened (a daemon restart), which is a
//!   new attempt of nothing. With `CHAOS811_PROXY=1` the node reaches the
//!   server through `support/fault_proxy.rs` and arms seeded reply drops.
//! * `monitor`: samples the id allocators from an admin connection.
//! * `probe`: dumps `gc_intent`, advisory `pg_locks` and backends.
//! * `settle`: one compaction admission from this container, which adopts
//!   the intents of owners this PID namespace proves gone.
//! * `dump`: the Store audit (`diagnose` full, amcheck, allocation checks),
//!   every Branch head read and mounted through a fresh service, and the
//!   Commit and Branch rows for the driver's checks.
//!
//! Nothing here is product code.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "support/fault_proxy.rs"]
#[allow(dead_code)]
mod fault_proxy;
#[path = "support/verify768.rs"]
#[allow(dead_code)]
mod verify768;

use adversary::{Kind, Op, Service, TestStore, CURSOR, PROJECT};
use layerfs_bridge::control::SdkValue;
use layerfs_history::{
    BranchId, CommitOutcome, HistoryError, HistoryName, LayerId, LayerStackId, NodeId, WorkspaceId,
};
use layerfs_persistence::{CompactRequest, Handles, PostgresSelection, StoreSelection};
use layerfs_store_service::store::{BindRequest, BoundWorkspace, CommitOptions};
use std::io::Write;
use std::mem::ManuallyDrop;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SCRATCH: usize = 6;

fn role(name: &str) -> bool {
    let on = std::env::var("CHAOS811_ROLE").ok().as_deref() == Some(name);
    if !on {
        eprintln!("NOT_RUN: CHAOS811_ROLE is not {name}");
    }
    on
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is unset"))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn hex(bytes: &[u8]) -> String {
    layerfs_bridge::sdk_values::hex(bytes)
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len() / 2)
        .map(|n| u8::from_str_radix(&text[2 * n..2 * n + 2], 16).unwrap())
        .collect()
}

/// A JSON string literal.
fn js(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn jopt(value: &Option<String>) -> String {
    value.as_deref().map_or_else(|| "null".to_owned(), js)
}

/// A flat JSON object of already-encoded values.
fn obj(fields: &[(&str, String)]) -> String {
    let body: Vec<String> = fields
        .iter()
        .map(|(k, v)| format!("{}:{v}", js(k)))
        .collect();
    format!("{{{}}}", body.join(","))
}

/// Appends lines to a file, each written whole and flushed.
struct Log(Mutex<std::fs::File>);

impl Log {
    fn open(path: &Path) -> Self {
        Self(Mutex::new(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .unwrap(),
        ))
    }
    fn line(&self, text: &str) {
        let mut file = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let _ = file.write_all(format!("{text}\n").as_bytes());
        let _ = file.flush();
    }
}

/// xorshift64*: the seeded schedule.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn stack() -> LayerStackId {
    // The stack `adversary::initialize` creates.
    LayerStackId::from_authority([97; 16])
}

fn branch_id(name: &str) -> BranchId {
    BranchId::named(stack(), &HistoryName::new(name).unwrap())
}

/// The Store as the support helpers know it, never dropped (its `Drop`
/// would remove the schema).
fn store_view(url: &str, schema: &str, binding: &[u8], dir: PathBuf) -> ManuallyDrop<TestStore> {
    ManuallyDrop::new(TestStore {
        kind: Kind::Postgres,
        binding: binding.to_vec(),
        dir,
        selection: StoreSelection::Postgres(PostgresSelection::new(url, schema)),
        postgres: Some((url.to_owned(), schema.to_owned())),
        stack: stack(),
        layer: LayerId::from_slice(&[50; 33]).unwrap_or_else(|_| panic!("layer id")),
    })
}

fn read_setup(dir: &Path) -> (String, Vec<u8>) {
    let text = std::fs::read_to_string(dir.join("setup.env")).unwrap();
    let mut schema = String::new();
    let mut binding = Vec::new();
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("schema=") {
            schema = v.to_owned();
        }
        if let Some(v) = line.strip_prefix("binding=") {
            binding = unhex(v);
        }
    }
    (schema, binding)
}

#[test]
fn chaos811_setup() {
    if !role("setup") {
        return;
    }
    let url = env("CHAOS811_URL");
    let out = PathBuf::from(env("CHAOS811_OUT"));
    let nodes: usize = env("CHAOS811_NODES").parse().unwrap();
    std::env::set_var("LAYERFS_TEST_POSTGRES_URL", &url);
    std::env::set_var("TMPDIR", out.join("tmp"));
    std::fs::create_dir_all(out.join("tmp")).unwrap();
    let store = TestStore::postgres("c811", &[("/seed", b"seed\n")]).expect("a server");
    let mut names = vec!["shared".to_owned()];
    names.extend((0..nodes).map(|n| format!("own{n}")));
    names.extend((0..SCRATCH).map(|n| format!("scratch{n}")));
    for name in &names {
        store.fork(name);
    }
    let (_, schema) = store.postgres.clone().unwrap();
    std::fs::write(
        out.join("setup.env"),
        format!(
            "schema={schema}\nbinding={}\nstack={}\nlayer={}\nbranches={}\n",
            hex(&store.binding),
            hex(store.stack.as_slice()),
            hex(store.layer.as_slice()),
            names.join(",")
        ),
    )
    .unwrap();
    std::mem::forget(store);
}

/// One mounted Workspace of a node.
struct Mount {
    workspace: BoundWorkspace,
    branch: String,
    id: String,
}

/// What an answer says about the service that gave it.
fn breaks_service(class: &str) -> bool {
    matches!(
        class,
        "Unknown" | "Closed" | "StaleStore" | "Quarantined" | "PublishedThenFailed" | "Failed"
    )
}

/// The outcome class of a refusal's text.
fn class_of(text: &str) -> &'static str {
    const ORDER: &[(&str, &str)] = &[
        ("UnknownOutcome", "Unknown"),
        ("StaleStore", "StaleStore"),
        ("MountsActive", "MountsActive"),
        ("HeadMoved", "HeadMoved"),
        ("Unresolved", "Unresolved"),
        ("BusyOp", "Busy"),
        ("Busy", "Busy"),
        ("LaneMerge", "LaneMerge"),
        ("LiveConflict", "LiveConflict"),
        ("NoSuchBranch", "NoSuchBranch"),
        ("MissingBranch", "NoSuchBranch"),
        ("Removed", "Removed"),
        ("NameTaken", "NameTaken"),
        ("Quarantined", "Quarantined"),
        ("Closed", "Closed"),
        ("Conflict", "Conflict"),
    ];
    ORDER
        .iter()
        .find(|(needle, _)| text.contains(needle))
        .map_or("Failed", |(_, class)| class)
}

fn history_class(error: &HistoryError) -> &'static str {
    if error.unknown() {
        return "Unknown";
    }
    match error {
        HistoryError::MountsActive(_) => "MountsActive",
        HistoryError::Busy | HistoryError::BusyOp(_) => "Busy",
        HistoryError::StaleStore => "StaleStore",
        HistoryError::Unresolved(_) => "Unresolved",
        other => class_of(&format!("{other:?}")),
    }
}

struct Node {
    index: usize,
    incarnation: u64,
    run: String,
    rng: Rng,
    log: Arc<Log>,
    store: ManuallyDrop<TestStore>,
    selection: StoreSelection,
    service: Option<Service>,
    mounts: Vec<Mount>,
    branches: Vec<String>,
    seq: u64,
    proxy: Option<fault_proxy::FaultProxy>,
    /// The running operation's start (ms) and name, for the watchdog.
    running: Arc<(AtomicU64, Mutex<String>)>,
    dir: PathBuf,
}

impl Node {
    fn token(&mut self, kind: &str) -> String {
        self.seq += 1;
        format!(
            "c811:{}:{kind}:n{}:i{}:s{}",
            self.run, self.index, self.incarnation, self.seq
        )
    }

    fn begin(&self, token: &str, op: &str, branch: &str) -> u64 {
        let t0 = now_ms();
        self.running.0.store(t0, Ordering::SeqCst);
        *self.running.1.lock().unwrap() = token.to_owned();
        self.log.line(&obj(&[
            ("ev", js("begin")),
            ("t0", t0.to_string()),
            ("node", self.index.to_string()),
            ("inc", self.incarnation.to_string()),
            ("token", js(token)),
            ("op", js(op)),
            ("branch", js(branch)),
        ]));
        t0
    }

    #[allow(clippy::too_many_arguments)]
    fn end(
        &self,
        token: &str,
        op: &str,
        branch: &str,
        t0: u64,
        outcome: &str,
        extra: &[(&str, String)],
    ) {
        self.running.0.store(0, Ordering::SeqCst);
        let mut fields = vec![
            ("ev", js("end")),
            ("t0", t0.to_string()),
            ("t1", now_ms().to_string()),
            ("node", self.index.to_string()),
            ("inc", self.incarnation.to_string()),
            ("token", js(token)),
            ("op", js(op)),
            ("branch", js(branch)),
            ("outcome", js(outcome)),
        ];
        fields.extend(extra.iter().cloned());
        self.log.line(&obj(&fields));
    }

    /// Opens a service: a readiness wait (bounded by the deadline), each
    /// open attempt logged; a failed open changed nothing.
    fn open_service(&mut self, deadline: u64) -> bool {
        while now_ms() < deadline {
            let token = self.token("open");
            let t0 = self.begin(&token, "service.open", "");
            match Service::try_open(&self.store, self.selection.clone()) {
                Ok(service) => {
                    self.end(&token, "service.open", "", t0, "Ok", &[]);
                    self.service = Some(service);
                    return true;
                }
                Err(error) => {
                    let class = class_of(&error);
                    self.end(
                        &token,
                        "service.open",
                        "",
                        t0,
                        class,
                        &[("detail", js(&error))],
                    );
                    std::thread::sleep(Duration::from_millis(300));
                }
            }
        }
        false
    }

    /// Closes a broken or stale service (a daemon restart): each mount's
    /// unmount is attempted once, then the service is abandoned.
    fn restart_service(&mut self, why: &str, deadline: u64) {
        let mounts = std::mem::take(&mut self.mounts);
        for mount in mounts {
            self.unmount_one(mount);
        }
        if let Some(service) = self.service.take() {
            let token = self.token("close");
            let t0 = self.begin(&token, "service.close", "");
            service.abandon();
            self.end(&token, "service.close", "", t0, "Ok", &[("why", js(why))]);
        }
        self.open_service(deadline);
    }

    fn unmount_one(&mut self, mount: Mount) -> &'static str {
        let token = self.token("unmount");
        if self.service.is_none() {
            return "NoService";
        }
        let t0 = self.begin(&token, "unmount", &mount.branch);
        let service = self.service.as_ref().unwrap();
        let (class, detail) = match service.try_unmount(mount.workspace) {
            Ok(()) => ("Ok", String::new()),
            Err(error) => (class_of(&error), error),
        };
        self.end(
            &token,
            "unmount",
            &mount.branch,
            t0,
            class,
            &[("ws", js(&mount.id)), ("detail", js(&detail))],
        );
        class
    }

    fn mount(&mut self, branch: &str) -> &'static str {
        let token = self.token("mount");
        let mut id = [0x5a; 32];
        id[..8].copy_from_slice(&(self.index as u64).to_le_bytes());
        id[8..16].copy_from_slice(&self.incarnation.to_le_bytes());
        id[16..24].copy_from_slice(&self.seq.to_le_bytes());
        let workspace = WorkspaceId::from_authority(id).unwrap();
        let ws = hex(&id);
        let t0 = self.begin(&token, "mount", branch);
        let service = self.service.as_ref().unwrap();
        let bound = service.service.store.bind(
            service.owner.client(),
            BindRequest {
                branch: branch_id(branch),
                workspace,
            },
        );
        match bound {
            Ok(bound) => {
                let head = bound
                    .workspace
                    .snapshot()
                    .ok()
                    .and_then(|s| s.branch.head_commit)
                    .map(|c| hex(c.as_slice()));
                self.end(
                    &token,
                    "mount",
                    branch,
                    t0,
                    "Ok",
                    &[("ws", js(&ws)), ("bound", jopt(&head))],
                );
                self.mounts.push(Mount {
                    workspace: bound.workspace,
                    branch: branch.to_owned(),
                    id: ws,
                });
                "Ok"
            }
            Err(refusal) => {
                let text = format!("{refusal:?}");
                let class = class_of(&text);
                self.end(
                    &token,
                    "mount",
                    branch,
                    t0,
                    class,
                    &[("ws", js(&ws)), ("detail", js(&text))],
                );
                class
            }
        }
    }

    fn commit(&mut self, at: usize) -> &'static str {
        let token = self.token("commit");
        let branch = self.mounts[at].branch.clone();
        let mode = match self.rng.below(10) {
            0..=5 => "plain",
            6 | 7 => "expected",
            8 => "force",
            _ => "force_expected",
        };
        let path = format!("/{}", token.replace(':', "_"));
        let applied = adversary::apply(
            &self.mounts[at].workspace,
            &Op::Write(path, format!("{token}\n").into_bytes()),
        );
        if let Err(error) = applied {
            let t0 = self.begin(&token, "write", &branch);
            let class = class_of(&error);
            self.end(
                &token,
                "write",
                &branch,
                t0,
                class,
                &[("detail", js(&error))],
            );
            return class;
        }
        let bound = self.mounts[at]
            .workspace
            .snapshot()
            .ok()
            .and_then(|s| s.branch.head_commit);
        let expected = if mode.ends_with("expected") {
            // The head this node reads now: another node may move it before
            // the Commit's own checks (HeadMoved).
            let service = self.service.as_ref().unwrap();
            match service
                .service
                .store
                .history()
                .branch_snapshot(branch_id(&branch))
            {
                Ok(Some(snapshot)) => Some(match snapshot.branch.head_commit {
                    Some(head) => NodeId::Commit(head),
                    None => NodeId::Layer(snapshot.branch.base_layer),
                }),
                Ok(None) => None,
                Err(error) => {
                    let t0 = self.begin(&token, "head.read", &branch);
                    let class = history_class(&error);
                    self.end(
                        &token,
                        "head.read",
                        &branch,
                        t0,
                        class,
                        &[("detail", js(&format!("{error:?}")))],
                    );
                    return class;
                }
            }
        } else {
            None
        };
        let expected_hex = expected.map(|node| hex(node.as_slice()));
        let options = CommitOptions {
            force: mode.starts_with("force"),
            message: Some(token.clone()),
            expected,
        };
        let armed = match &self.proxy {
            Some(proxy) if self.rng.below(4) == 0 => {
                let at = 1 + self.rng.below(40);
                proxy.arm(Some((at, fault_proxy::Fault::Drop)));
                Some(at)
            }
            _ => None,
        };
        let t0 = self.begin(&token, "commit", &branch);
        let result = self.mounts[at].workspace.commit_captured(&options);
        let fired = match (&self.proxy, armed) {
            (Some(proxy), Some(at)) => {
                let window = proxy.disarm();
                Some(match window.fired {
                    Some(f) => format!("drop@{at}:fired:conn{}:{}", f.connection, f.kind),
                    None => format!("drop@{at}:not-fired:seen{}", window.seen),
                })
            }
            _ => None,
        };
        let (class, commit, displaced, detail) = match &result {
            Ok(success) => match &success.history {
                Some(CommitOutcome::Committed { record, displaced }) => (
                    "Committed",
                    Some(hex(record.id.as_slice())),
                    displaced.map(|d| hex(d.as_slice())),
                    String::new(),
                ),
                Some(CommitOutcome::UpToDate { .. }) => ("UpToDate", None, None, String::new()),
                None => ("Conflicted", None, None, String::new()),
            },
            Err(failure) => {
                let text = failure.to_string();
                match &failure.published {
                    Some(CommitOutcome::Committed { record, displaced }) => (
                        "PublishedThenFailed",
                        Some(hex(record.id.as_slice())),
                        displaced.map(|d| hex(d.as_slice())),
                        text,
                    ),
                    Some(_) => ("PublishedThenFailed", None, None, text),
                    None if failure.uncertain() => ("Unknown", None, None, text),
                    None => (class_of(&text), None, None, text),
                }
            }
        };
        self.end(
            &token,
            "commit",
            &branch,
            t0,
            class,
            &[
                ("mode", js(mode)),
                ("ws", js(&self.mounts[at].id)),
                ("bound", jopt(&bound.map(|c| hex(c.as_slice())))),
                ("expected", jopt(&expected_hex)),
                ("commit", jopt(&commit)),
                ("displaced", jopt(&displaced)),
                ("proxy", jopt(&fired)),
                ("detail", js(&detail)),
            ],
        );
        class
    }

    fn sdk(&mut self, op: &str, branch: &str) -> &'static str {
        let token = self.token(op);
        let t0 = self.begin(&token, op, branch);
        let field = if op == "branch.fork" {
            "name"
        } else {
            "branch"
        };
        let answer = self
            .service
            .as_ref()
            .unwrap()
            .call(op, vec![(field, SdkValue::text(branch))]);
        let (class, detail) = match answer {
            Ok(_) => ("Ok", String::new()),
            Err(refusal) => {
                let class = match refusal.code.as_str() {
                    "Unknown" | "UnknownOutcome" => "Unknown",
                    "MountsActive" => "MountsActive",
                    "Busy" => "Busy",
                    "NoSuchBranch" => "NoSuchBranch",
                    "NameTaken" => "NameTaken",
                    "Unresolved" => "Unresolved",
                    _ => class_of(&format!("{} {}", refusal.code, refusal.message)),
                };
                (class, format!("{}: {}", refusal.code, refusal.message))
            }
        };
        self.end(&token, op, branch, t0, class, &[("detail", js(&detail))]);
        class
    }

    fn compact(&mut self) -> &'static str {
        let token = self.token("compact");
        let t0 = self.begin(&token, "compact", "");
        let (class, detail) = compact_once(&self.selection, &self.store.binding);
        self.end(&token, "compact", "", t0, class, &[("detail", js(&detail))]);
        class
    }

    fn step(&mut self, deadline: u64, lull: bool) {
        if self.service.is_none() && !self.open_service(deadline) {
            return;
        }
        let pick = self.rng.below(100);
        let class: &'static str = if lull {
            // The lull: this node gives back its mounts and tries the
            // Store- and Branch-scoped calls the mounts held off.
            if let Some(mount) = self.mounts.pop() {
                self.unmount_one(mount)
            } else if pick < 50 {
                self.compact()
            } else {
                let name = self.scratch_name();
                self.sdk("branch.remove", &name)
            }
        } else if self.mounts.is_empty() || (pick < 14 && self.mounts.len() < 3) {
            let branch = self.mount_target();
            self.mount(&branch)
        } else if pick < 64 {
            let at = self.rng.below(self.mounts.len() as u64) as usize;
            self.commit(at)
        } else if pick < 76 {
            let at = self.rng.below(self.mounts.len() as u64) as usize;
            let mount = self.mounts.remove(at);
            self.unmount_one(mount)
        } else if pick < 82 {
            let name = self.scratch_name();
            self.sdk("branch.remove", &name)
        } else if pick < 84 {
            let name = format!("f{}x{}x{}", self.index, self.incarnation, self.seq + 1);
            let class = self.sdk("branch.fork", &name);
            if class == "Ok" {
                self.branches.push(name);
            }
            class
        } else if pick < 91 {
            self.compact()
        } else {
            std::thread::sleep(Duration::from_millis(self.rng.below(60)));
            "Ok"
        };
        if breaks_service(class) {
            self.restart_service(class, deadline);
        }
        // Pacing: a bounded Store, so the full audit fits the run.
        std::thread::sleep(Duration::from_millis(20 + self.rng.below(60)));
    }

    fn scratch_name(&mut self) -> String {
        let n = self.rng.below(self.branches.len() as u64) as usize;
        self.branches[n].clone()
    }

    fn mount_target(&mut self) -> String {
        match self.rng.below(10) {
            0..=4 => "shared".to_owned(),
            5..=7 => format!("own{}", self.index),
            _ => self.scratch_name(),
        }
    }

    fn quiesce(&mut self) {
        let healthy = self.service.is_some();
        let mounts: Vec<String> = self
            .mounts
            .iter()
            .map(|m| obj(&[("ws", js(&m.id)), ("branch", js(&m.branch))]))
            .collect();
        let state = obj(&[
            ("node", self.index.to_string()),
            ("inc", self.incarnation.to_string()),
            ("pid", std::process::id().to_string()),
            ("healthy", healthy.to_string()),
            ("t", now_ms().to_string()),
            ("mounts", format!("[{}]", mounts.join(","))),
        ]);
        let path = self.dir.join(format!("state.{}.json", self.incarnation));
        std::fs::write(path.with_extension("tmp"), &state).unwrap();
        std::fs::rename(path.with_extension("tmp"), &path).unwrap();
        let release = self.dir.join("release");
        let until = Instant::now() + Duration::from_secs(40);
        while !release.exists() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(50));
        }
        let mounts = std::mem::take(&mut self.mounts);
        for mount in mounts {
            self.unmount_one(mount);
        }
        if let Some(service) = self.service.take() {
            let token = self.token("stop");
            let t0 = self.begin(&token, "service.stop", "");
            service.abandon();
            self.end(&token, "service.stop", "", t0, "Ok", &[]);
        }
    }
}

/// One compaction on a session of its own, owned as `store compact` owns
/// it: "Ok"/the refusal class, and the detail.
fn compact_once(selection: &StoreSelection, binding: &[u8]) -> (&'static str, String) {
    let handles = match Handles::open_writable(selection, binding, CURSOR) {
        Ok(handles) => handles,
        Err(error) => {
            let text = format!("open: {error:?}");
            return (class_of(&text), text);
        }
    };
    let Handles::Postgres(postgres) = &handles else {
        unreachable!()
    };
    let op_id = layerfs_store_service::history::admission::op_id();
    let (owner_runtime, owner_boot) = layerfs_store_service::history::admission::owner_tokens(
        &format!("compact:{}", hex(&op_id)),
    );
    let result = postgres.compact(&CompactRequest {
        op_id,
        owner_pid: Some(std::process::id()),
        owner_runtime: Some(owner_runtime),
        owner_boot: Some(owner_boot),
        ..CompactRequest::default()
    });
    handles.close();
    match result {
        Ok(receipt) => ("Ok", format!("{}", receipt.path.as_str())),
        Err(error) => (history_class(&error), format!("{error:?}")),
    }
}

#[test]
fn chaos811_node() {
    if !role("node") {
        return;
    }
    let url = env("CHAOS811_URL");
    let out = PathBuf::from(env("CHAOS811_OUT"));
    let index: usize = env("CHAOS811_NODE").parse().unwrap();
    let incarnation: u64 = env("CHAOS811_INC").parse().unwrap();
    let seed: u64 = env("CHAOS811_SEED").parse().unwrap();
    let deadline: u64 = env("CHAOS811_DEADLINE_MS").parse().unwrap();
    let lull: (u64, u64) = {
        let text = env("CHAOS811_LULL_MS");
        let (a, b) = text.split_once('-').unwrap();
        (a.parse().unwrap(), b.parse().unwrap())
    };
    let dir = out.join(format!("n{index}"));
    std::fs::create_dir_all(&dir).unwrap();
    let state = dir.join(format!("state-dir.{incarnation}"));
    std::fs::create_dir_all(state.join("tmp")).unwrap();
    std::env::set_var("TMPDIR", state.join("tmp"));
    let (schema, binding) = read_setup(&out);
    let log = Arc::new(Log::open(&dir.join("ops.jsonl")));
    let (proxy, dial) = if std::env::var("CHAOS811_PROXY").ok().as_deref() == Some("1") {
        let (proxy, proxied) = fault_proxy::FaultProxy::start(&url);
        (Some(proxy), proxied)
    } else {
        (None, url.clone())
    };
    let selection = StoreSelection::Postgres(PostgresSelection::new(&dial, &schema));
    let store = store_view(&dial, &schema, &binding, state.clone());
    let running: Arc<(AtomicU64, Mutex<String>)> = Arc::default();
    {
        // Watchdog: an operation running past 20 s is logged once.
        let (running, log) = (running.clone(), log.clone());
        std::thread::spawn(move || {
            let mut reported = 0;
            loop {
                std::thread::sleep(Duration::from_millis(500));
                let started = running.0.load(Ordering::SeqCst);
                if started != 0 && started != reported && now_ms() - started > 20_000 {
                    reported = started;
                    let token = running.1.lock().unwrap().clone();
                    log.line(&obj(&[
                        ("ev", js("slow")),
                        ("t", now_ms().to_string()),
                        ("node", index.to_string()),
                        ("token", js(&token)),
                    ]));
                }
            }
        });
    }
    let mut branches: Vec<String> = (0..SCRATCH).map(|n| format!("scratch{n}")).collect();
    branches.push(format!("own{index}"));
    let mut node = Node {
        index,
        incarnation,
        run: env("CHAOS811_RUN"),
        rng: Rng::new(seed ^ ((index as u64 + 1) << 40) ^ (incarnation << 20)),
        log: log.clone(),
        store,
        selection,
        service: None,
        mounts: Vec::new(),
        branches,
        seq: 0,
        proxy,
        running,
        dir: dir.clone(),
    };
    log.line(&obj(&[
        ("ev", js("start")),
        ("t", now_ms().to_string()),
        ("node", index.to_string()),
        ("inc", incarnation.to_string()),
        ("pid", std::process::id().to_string()),
        ("proxy", (node.proxy.is_some()).to_string()),
    ]));
    while now_ms() < deadline {
        let t = now_ms();
        node.step(deadline, t >= lull.0 && t < lull.1);
    }
    node.quiesce();
    log.line(&obj(&[
        ("ev", js("exit")),
        ("t", now_ms().to_string()),
        ("node", index.to_string()),
        ("inc", incarnation.to_string()),
    ]));
}

fn admin(url: &str, schema: &str) -> Result<postgres::Client, String> {
    let mut client = postgres::Client::connect(url, postgres::NoTls).map_err(|e| e.to_string())?;
    client
        .batch_execute(&format!("SET search_path TO {schema}"))
        .map_err(|e| e.to_string())?;
    Ok(client)
}

#[test]
fn chaos811_monitor() {
    if !role("monitor") {
        return;
    }
    let url = env("CHAOS811_URL");
    let out = PathBuf::from(env("CHAOS811_OUT"));
    let (schema, _) = read_setup(&out);
    let log = Log::open(&out.join("monitor.jsonl"));
    let stop = out.join("stop-monitor");
    let until = Instant::now() + Duration::from_secs(110);
    let mut client: Option<postgres::Client> = None;
    const SQL: &str = "SELECT p.next_pack_id, p.next_ordinal, h.next_history_serial, \
        h.catalog_incarnation, \
        (SELECT coalesce(max(pack_id), 0) FROM pack), \
        (SELECT coalesce(max(last_ordinal), 0) FROM inode_value_block), \
        (SELECT coalesce(max(history_serial), 0) FROM \"commit\"), \
        (SELECT coalesce(string_agg(encode(scope_id, 'hex') || ':' || highwater, ',' \
           ORDER BY scope_id), '') FROM scope_allocator), \
        (SELECT count(*) FROM \"commit\") \
        FROM store_policy p, history_meta h WHERE p.id = 1 AND h.id = 1";
    while !stop.exists() && Instant::now() < until {
        if client.is_none() {
            client = admin(&url, &schema).ok();
        }
        if let Some(c) = client.as_mut() {
            match c.query_one(SQL, &[]) {
                Ok(row) => log.line(&obj(&[
                    ("t", now_ms().to_string()),
                    ("next_pack_id", row.get::<_, i64>(0).to_string()),
                    ("next_ordinal", row.get::<_, i64>(1).to_string()),
                    ("next_history_serial", row.get::<_, i64>(2).to_string()),
                    ("incarnation", row.get::<_, i64>(3).to_string()),
                    ("max_pack_id", row.get::<_, i64>(4).to_string()),
                    ("max_ordinal", row.get::<_, i64>(5).to_string()),
                    ("max_history_serial", row.get::<_, i64>(6).to_string()),
                    ("scopes", js(&row.get::<_, String>(7))),
                    ("commits", row.get::<_, i64>(8).to_string()),
                ])),
                Err(error) => {
                    log.line(&obj(&[
                        ("t", now_ms().to_string()),
                        ("error", js(&error.to_string())),
                    ]));
                    client = None;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn rows_json(client: &mut postgres::Client, sql: &str) -> Result<String, String> {
    // Every column is read as text by the statement itself.
    let rows = client.query(sql, &[]).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let fields: Vec<(String, String)> = row
            .columns()
            .iter()
            .enumerate()
            .map(|(n, column)| {
                let value: Option<String> = row.get(n);
                (column.name().to_owned(), jopt(&value))
            })
            .collect();
        let body: Vec<String> = fields
            .iter()
            .map(|(k, v)| format!("{}:{v}", js(k)))
            .collect();
        out.push(format!("{{{}}}", body.join(",")));
    }
    Ok(format!("[{}]", out.join(",")))
}

const INTENTS: &str = "SELECT encode(op_id, 'hex') AS op, kind, scope_kind, \
    encode(scope_id, 'hex') AS scope, workspace, branch, owner_runtime, owner_boot, state, \
    uncertain::text, updated_ms::text FROM gc_intent ORDER BY op_id";
const LOCKS: &str = "SELECT l.pid::text, host(a.client_addr) AS addr, l.classid::text, \
    l.objid::text, l.objsubid::text, l.mode, l.granted::text \
    FROM pg_locks l LEFT JOIN pg_stat_activity a ON a.pid = l.pid \
    WHERE l.locktype = 'advisory' AND l.pid <> pg_backend_pid() ORDER BY 1, 3, 4";
const BACKENDS: &str = "SELECT pid::text, host(client_addr) AS addr, state, \
    backend_start::text, xact_start::text, left(query, 120) AS query \
    FROM pg_stat_activity WHERE datname = current_database() AND pid <> pg_backend_pid() \
    AND backend_type = 'client backend' ORDER BY pid";

#[test]
fn chaos811_probe() {
    if !role("probe") {
        return;
    }
    let url = env("CHAOS811_URL");
    let out = PathBuf::from(env("CHAOS811_OUT"));
    let label = env("CHAOS811_LABEL");
    let (schema, _) = read_setup(&out);
    let mut client = admin(&url, &schema).unwrap();
    let json = obj(&[
        ("t", now_ms().to_string()),
        ("intents", rows_json(&mut client, INTENTS).unwrap()),
        ("locks", rows_json(&mut client, LOCKS).unwrap()),
        ("backends", rows_json(&mut client, BACKENDS).unwrap()),
    ]);
    std::fs::write(out.join(format!("probe-{label}.json")), json).unwrap();
}

#[test]
fn chaos811_settle() {
    if !role("settle") {
        return;
    }
    let url = env("CHAOS811_URL");
    let out = PathBuf::from(env("CHAOS811_OUT"));
    let label = env("CHAOS811_LABEL");
    let (schema, binding) = read_setup(&out);
    let selection = StoreSelection::Postgres(PostgresSelection::new(&url, &schema));
    let t0 = now_ms();
    let (class, detail) = compact_once(&selection, &binding);
    Log::open(&out.join("settle.jsonl")).line(&obj(&[
        ("label", js(&label)),
        ("t0", t0.to_string()),
        ("t1", now_ms().to_string()),
        ("outcome", js(class)),
        ("detail", js(&detail)),
    ]));
}

/// The tree of `branch` read through a Workspace `service` mounts: the
/// head's Commit and root exist and read.
fn walk(service: &Service, branch: &str) -> Result<usize, String> {
    let workspace = service.try_bind(branch_id(branch))?;
    let tree = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        adversary::tree_of(&workspace)
    }))
    .map_err(|_| format!("{branch}: the tree walk failed"))?;
    service.try_unmount(workspace)?;
    Ok(tree.len())
}

/// The Store's full audit (`diagnose`, as `doctor --full` runs it) on a
/// fresh session, bounded by `seconds`: every `Fail`, as text.
fn audit(store: &TestStore, seconds: u64) -> Vec<String> {
    use layerfs_persistence::diagnostic::{Options, Outcome};
    let handles = Handles::open_read_only(&store.selection, &store.binding, CURSOR).unwrap();
    let mut failures = Vec::new();
    let options = Options {
        quick: false,
        deadline: Some(Instant::now() + Duration::from_secs(seconds)),
        ..Options::default()
    };
    let mut observe = |o: layerfs_persistence::diagnostic::Observation| {
        if o.outcome == Outcome::Fail {
            failures.push(format!("{}: {}", o.id, o.message));
        }
    };
    let done = match &handles {
        Handles::Postgres(h) => h.diagnose(options, &mut observe),
        Handles::Sqlite(h) => h.diagnose(options, &mut observe),
    };
    if let Err(error) = done {
        failures.push(format!("diagnose: {error:?}"));
    }
    handles.close();
    failures
}

/// The operator's settlement of the intents left once every owner has
/// exited (`gc_mark_uncertain`, then `gc_release`): the documented path for
/// an owner no admission can prove gone.
#[test]
fn chaos811_operator() {
    if !role("operator") {
        return;
    }
    let url = env("CHAOS811_URL");
    let out = PathBuf::from(env("CHAOS811_OUT"));
    let (schema, binding) = read_setup(&out);
    let selection = StoreSelection::Postgres(PostgresSelection::new(&url, &schema));
    let mut client = admin(&url, &schema).unwrap();
    let ops: Vec<Vec<u8>> = client
        .query(
            "SELECT op_id FROM gc_intent WHERE state <> 'done' ORDER BY op_id",
            &[],
        )
        .unwrap()
        .iter()
        .map(|row| row.get(0))
        .collect();
    let log = Log::open(&out.join("operator.jsonl"));
    let handles = Handles::open_writable(&selection, &binding, CURSOR).unwrap();
    for op in ops {
        let op: [u8; 16] = op.as_slice().try_into().unwrap();
        let marked = adversary::history(&handles).gc_mark_uncertain(&op);
        let released = adversary::history(&handles).gc_release(&op);
        log.line(&obj(&[
            ("t", now_ms().to_string()),
            ("op", js(&hex(&op))),
            ("marked", js(&format!("{marked:?}"))),
            ("released", js(&format!("{released:?}"))),
        ]));
    }
    handles.close();
}

/// The Commit and Branch rows, as JSON arrays.
fn history_rows(client: &mut postgres::Client) -> (String, String) {
    let commits = rows_json(
        client,
        "SELECT encode(commit_id, 'hex') AS id, encode(parent_commit_id, 'hex') AS parent, \
         encode(displaced_id, 'hex') AS displaced, encode(branch_id, 'hex') AS branch, \
         message, encode(attempt, 'hex') AS attempt, encode(root_id, 'hex') AS root, \
         via::text, history_serial::text FROM \"commit\" ORDER BY history_serial",
    )
    .unwrap();
    let branches = rows_json(
        client,
        "SELECT encode(b.branch_id, 'hex') AS id, b.name, encode(b.head_commit_id, 'hex') AS head, \
         (r.branch_id IS NOT NULL)::text AS retired, r.removed_ms::text \
         FROM branch b LEFT JOIN branch_retired r ON r.branch_id = b.branch_id ORDER BY b.name",
    )
    .unwrap();
    (commits, branches)
}

/// The Commit and Branch rows alone, before the last compaction collects
/// the removed Branches' history.
#[test]
fn chaos811_rows() {
    if !role("rows") {
        return;
    }
    let url = env("CHAOS811_URL");
    let out = PathBuf::from(env("CHAOS811_OUT"));
    let (schema, _) = read_setup(&out);
    let mut client = admin(&url, &schema).unwrap();
    let (commits, branches) = history_rows(&mut client);
    let json = obj(&[
        ("t", now_ms().to_string()),
        ("commits", commits),
        ("branches", branches),
    ]);
    std::fs::write(out.join("rows.json"), json).unwrap();
}

#[test]
fn chaos811_dump() {
    if !role("dump") {
        return;
    }
    let url = env("CHAOS811_URL");
    let out = PathBuf::from(env("CHAOS811_OUT"));
    let (schema, binding) = read_setup(&out);
    let dir = out.join("dump-state");
    std::fs::create_dir_all(dir.join("tmp")).unwrap();
    std::env::set_var("TMPDIR", dir.join("tmp"));
    let store = store_view(&url, &schema, &binding, dir);
    let mut client = admin(&url, &schema).unwrap();
    let (commits, branches) = history_rows(&mut client);
    let gc_op = rows_json(
        &mut client,
        "SELECT generation::text, phase, encode(op_id, 'hex') AS op, started_ms::text FROM gc_op",
    )
    .unwrap_or_else(|e| js(&e));
    let meta = rows_json(
        &mut client,
        "SELECT catalog_incarnation::text, next_history_serial::text FROM history_meta",
    )
    .unwrap();
    // Check 3: every live Branch's head and root, read and mounted.
    let handles = Handles::open_read_only(&store.selection, &store.binding, CURSOR).unwrap();
    let names: Vec<(String, bool)> = client
        .query(
            "SELECT b.name, r.branch_id IS NOT NULL FROM branch b \
             LEFT JOIN branch_retired r ON r.branch_id = b.branch_id ORDER BY b.name",
            &[],
        )
        .unwrap()
        .iter()
        .map(|row| (row.get(0), row.get(1)))
        .collect();
    // The full audit runs beside the head walks, on its own session.
    let auditing = {
        let store = store_view(&url, &schema, &binding, store.dir.clone());
        std::thread::spawn(move || {
            let started = Instant::now();
            let failures = audit(&store, 45);
            (failures, started.elapsed().as_millis() as u64)
        })
    };
    let mut heads = Vec::new();
    let walker = Service::try_open(&store, store.selection.clone());
    for (name, retired) in &names {
        if *retired {
            continue;
        }
        let snapshot = adversary::history(&handles).branch_snapshot(branch_id(name));
        let (state, head) = match snapshot {
            Ok(Some(s)) => {
                let head = s.branch.head_commit;
                let commit_ok = match head {
                    Some(id) => matches!(adversary::history(&handles).commit(id), Ok(Some(_))),
                    None => true,
                };
                (
                    if commit_ok {
                        "ok"
                    } else {
                        "head commit missing"
                    }
                    .to_owned(),
                    head.map(|c| hex(c.as_slice())),
                )
            }
            Ok(None) => ("no snapshot".to_owned(), None),
            Err(error) => (format!("{error:?}"), None),
        };
        let walked = match &walker {
            Ok(service) => match walk(service, name) {
                Ok(n) => format!("ok:{n}"),
                Err(error) => error,
            },
            Err(error) => format!("open: {error}"),
        };
        heads.push(obj(&[
            ("branch", js(name)),
            ("snapshot", js(&state)),
            ("head", jopt(&head)),
            ("walk", js(&walked)),
        ]));
    }
    handles.close();
    if let Ok(service) = walker {
        service.stop();
    }
    let (audit, audit_ms) = auditing
        .join()
        .unwrap_or_else(|_| (vec!["audit: panicked".to_owned()], 0));
    let amcheck = verify768::amcheck(&store);
    let allocation = verify768::duplicate_allocations(&store);
    let list = |items: &[String]| {
        format!(
            "[{}]",
            items.iter().map(|i| js(i)).collect::<Vec<_>>().join(",")
        )
    };
    let json = obj(&[
        ("t", now_ms().to_string()),
        ("project", js(PROJECT)),
        ("commits", commits),
        ("branches", branches),
        ("gc_op", gc_op),
        ("meta", meta),
        ("heads", format!("[{}]", heads.join(","))),
        ("audit", list(&audit)),
        ("audit_ms", audit_ms.to_string()),
        ("amcheck", list(&amcheck)),
        ("allocation", list(&allocation)),
    ]);
    std::fs::write(out.join("dump.json"), json).unwrap();
}
