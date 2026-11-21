//! `sandbox.get` takes no Store lease, opens no Store and writes no record
//! of a ready Sandbox (layerfs-project#181), on each side that runs its two
//! reads: this process for a `native` Sandbox, a machine's worker, and this
//! process again for a Sandbox whose daemon's port is forwarded here. Its
//! answer is the daemon's Workspace list, for 0, 1 and 4 Workspaces and
//! from a daemon that predates the Workspaces-only option, and its records
//! alone for a stopped machine. The one file a get that reached the daemon
//! writes is `observed.json` beside the record (call surface 2.0), which
//! these comparisons leave out.
//!
//! The fields of layerfs-project#178 ride the same reads and add none:
//! `workspace_capacity` (16, whatever the Sandbox's state), each Workspace's
//! `running` and `commands` (the commands another client started, from the
//! exec records on the side that keeps them, with their `tag` and
//! `program`; one that ended or whose owner is gone is not listed) and,
//! with `--with stats`, its `overlay_bytes` (the size of its overlay file
//! where that side has the run folder, by path alone; null for a forwarded
//! Sandbox).
//!
//! The fields of layerfs-project#164 ride them too, under `stats`: `daemon`
//! (`{ pid, started_at }` of the process the daemon record names) and
//! `usage` (the Sandbox's cgroup, null with `usage_reason` for a
//! host-process Sandbox), read on the side that runs the Sandbox; a
//! forwarded Sandbox's come from its machine's worker with its overlay
//! sizes, in the one inspect request.
//!
//! Nothing runs: a stand-in daemon answers the Workspace list and the
//! capacity model, and the worker is this build's own server on a scratch
//! root. Each home has a Store record and no Store file, so a get that
//! opened the Store would be refused. While a get runs, the test holds the
//! Store's lock and the Store's record carries a running `store.move`
//! intent, as a push does while it checks that the Store is idle.
#![cfg(unix)]
use layerfs_api::home::{self, lease, paths, read_record, write_record};
use layerfs_bridge::control::{
    AdmissionModel, Answer, Call, CapacityModel, ControlCode, ControlRefusal, DaemonPhase,
    DaemonStatus, Reply, Request, RetainedModel, SdkValue, OPERATION,
};
use layerfs_bridge::duplex;
use layerfs_sandbox::record::{DaemonRecord, Local, WorkspaceRecord};
use std::collections::BTreeMap;
use std::net::{SocketAddr, TcpListener};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const CLIENT_PRIVATE: [u8; 32] = [2; 32];
const DAEMON_PRIVATE: [u8; 32] = [1; 32];
const DAEMON_INSTANCE: [u8; 32] = [3; 32];

/// Names the homes of one test process apart.
static HOMES: AtomicUsize = AtomicUsize::new(0);

/// The turn of a test whose get makes an inspect request (a forwarded
/// Sandbox's): the two inspect permits are the process's, and a request
/// refused `Busy` would leave what it reads not observed.
static INSPECTS: Mutex<()> = Mutex::new(());

fn inspect_turn() -> std::sync::MutexGuard<'static, ()> {
    INSPECTS.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn tempdir(tag: &str) -> PathBuf {
    let sequence = HOMES.fetch_add(1, Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("lfs-sgnl-{tag}-{}-{sequence}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

/// A record from `fields`, in order.
fn map(fields: &[(&str, SdkValue)]) -> SdkValue {
    SdkValue::Map(
        fields
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect(),
    )
}

/// `sandbox.get` of Sandbox `s1`, on a thread of its own: an answer that
/// does not come within 20 s fails the test instead of hanging it (a get
/// that waited for the Store's lock would wait 120 s).
fn get(home: &Path) -> Result<SdkValue, (String, String)> {
    let home = home.to_owned();
    let (sender, answer) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let args = map(&[
            ("sandbox", t("s1")),
            ("detail", SdkValue::Bool(true)),
            ("with", SdkValue::List(vec![t("stats")])),
        ]);
        let got = layerfs_api::call(Some(&home), "sandbox.get", args, &mut |_| {});
        let _ = sender.send(got.map_err(|r| (r.code, r.message)));
    });
    answer
        .recv_timeout(Duration::from_secs(20))
        .expect("sandbox.get did not answer within 20 s")
}

/// Workspace `n` as a daemon lists it (`operator::inspect`'s item).
fn item(n: i64) -> SdkValue {
    map(&[
        ("workspace", t(&format!("{n:064x}"))),
        ("namespace", SdkValue::Int(n)),
        ("branch", t(&format!("b{n}"))),
        ("base", SdkValue::Null),
        ("baseLayer", t(&format!("{:064x}", n + 100))),
        ("root", t(&format!("{:064x}", n + 200))),
        ("activity", t("idle")),
        ("mounted", SdkValue::Bool(true)),
        ("directory", t(&format!("/workspaces/{n}"))),
        ("headMovedSinceMount", SdkValue::Bool(false)),
        (
            "dirty",
            map(&[("inodes", SdkValue::Int(n)), ("entries", SdkValue::Int(0))]),
        ),
        ("pendingMerge", SdkValue::Bool(false)),
        ("revision", SdkValue::Int(7)),
    ])
}

/// The name Workspace `n`'s record gives it: every second one has one.
fn name_of(n: i64) -> Option<String> {
    (n % 2 == 0).then(|| format!("w{n}"))
}

/// The keys of a Workspace that [`shown`] compares; `mounted_at` (the
/// record's own time) is left out.
const COMPARED: [&str; 12] = [
    "workspace",
    "branch",
    "running",
    "state",
    "path",
    "commit",
    "base_layer",
    "branch_moved",
    "pending_merge",
    "commands",
    "id",
    "op",
];

/// What `sandbox.get --detail` shows of the daemon's `items`: each by its
/// handle (`s1:<name>`, or `s1:<path>` when its record gives it no name),
/// its Branch, ready, with no command in it, and what its daemon said of
/// it under the rows' names.
fn shown(items: &[SdkValue]) -> Vec<BTreeMap<String, SdkValue>> {
    items
        .iter()
        .map(|item| {
            let path = item.get_text("directory").unwrap().to_owned();
            let name = name_of(item.get_int("namespace").unwrap());
            let ws = format!("s1:{}", name.as_deref().unwrap_or(&path));
            let field = |name: &str| item.get(name).cloned().unwrap();
            BTreeMap::from([
                ("workspace".to_owned(), t(&ws)),
                ("branch".to_owned(), field("branch")),
                ("running".to_owned(), SdkValue::List(Vec::new())),
                ("state".to_owned(), t("ready")),
                ("path".to_owned(), t(&path)),
                ("commit".to_owned(), field("base")),
                ("base_layer".to_owned(), field("baseLayer")),
                ("branch_moved".to_owned(), field("headMovedSinceMount")),
                ("pending_merge".to_owned(), field("pendingMerge")),
                ("commands".to_owned(), SdkValue::List(Vec::new())),
                ("id".to_owned(), field("workspace")),
                ("op".to_owned(), SdkValue::Null),
            ])
        })
        .collect()
}

/// The `workspaces` of answer `got`, each as a name-keyed map of the keys
/// [`shown`] compares.
fn workspaces(got: &SdkValue) -> Vec<BTreeMap<String, SdkValue>> {
    let Some(SdkValue::List(items)) = got.get("workspaces") else {
        panic!("no workspaces in {got:?}")
    };
    items
        .iter()
        .map(|item| match item {
            SdkValue::Map(fields) => fields
                .iter()
                .filter(|&(name, _)| COMPARED.contains(&name.as_str()))
                .cloned()
                .collect(),
            other => panic!("{other:?}"),
        })
        .collect()
}

/// The `stats` section of answer `got`.
fn stats(got: &SdkValue) -> &SdkValue {
    got.get("stats")
        .unwrap_or_else(|| panic!("no stats in {got:?}"))
}

/// Field `name` of each Workspace in `got`'s `stats`, in order.
fn each_stat(got: &SdkValue, name: &str) -> Vec<SdkValue> {
    let Some(SdkValue::List(items)) = stats(got).get("workspaces") else {
        panic!("no stats.workspaces in {got:?}")
    };
    (items.iter())
        .map(|item| {
            item.get(name)
                .cloned()
                .unwrap_or_else(|| panic!("no {name}"))
        })
        .collect()
}

/// What a stand-in daemon saw.
#[derive(Default)]
struct Seen {
    connections: AtomicUsize,
    /// The arguments of each `sandbox.list_workspaces` it answered.
    lists: Mutex<Vec<SdkValue>>,
}

/// A stand-in daemon at its own loopback port that lists `items`. A
/// daemon from before the Workspaces-only option (`old`) answers the whole
/// read model, history included, whatever it is asked.
fn daemon(items: Vec<SdkValue>, old: bool) -> (SocketAddr, Arc<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = listener.local_addr().unwrap();
    let seen = Arc::new(Seen::default());
    let counted = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { return };
            counted.connections.fetch_add(1, Ordering::SeqCst);
            let (seen, items) = (counted.clone(), items.clone());
            std::thread::spawn(move || serve(stream, &items, old, &seen));
        }
    });
    (endpoint, seen)
}

/// One stand-in connection: its Hello, then each call's answer until the
/// caller closes it. A Workspace status is answered `Missing`, which a
/// reopen reads as a record whose owner removes it next; the list names
/// each Workspace from its record all the same.
fn serve(stream: std::net::TcpStream, items: &[SdkValue], old: bool, seen: &Seen) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
    let Ok(mut channel) = duplex::accept(
        stream,
        &DAEMON_PRIVATE,
        duplex::public_key(&CLIENT_PRIVATE).unwrap(),
    ) else {
        return;
    };
    while let Ok(bytes) = channel.receive.receive() {
        let Ok(call) = Call::decode(bytes) else { break };
        let id = call.id;
        let reply = match call.request {
            Request::Hello(_) => Reply::Hello(DaemonStatus {
                instance: DAEMON_INSTANCE,
                phase: DaemonPhase::ControlReady,
                overlay_sqlite: Some("3.51.0".into()),
                store_sqlite: Some("3.51.0".into()),
            }),
            Request::EndSession => Reply::SessionEnded,
            Request::Status(_) => Reply::Refused(ControlRefusal {
                code: ControlCode::Missing,
                phase: "status".into(),
                moved: None,
                published: None,
                detail: "the stand-in holds no such Workspace".into(),
            }),
            Request::Sdk(sdk) if sdk.method == "sandbox.list_workspaces" => {
                seen.lists.lock().unwrap().push(sdk.args.clone());
                let mut fields = vec![("kind", t("ok")), ("version", t("00000000000000aa"))];
                if old {
                    fields.extend([
                        ("store", map(&[("stats", SdkValue::Null)])),
                        ("project", t("api")),
                        (
                            "branches",
                            SdkValue::List(vec![map(&[("name", t("main"))])]),
                        ),
                        ("moreBranches", SdkValue::Bool(false)),
                        ("commits", SdkValue::List(vec![map(&[("id", t("c1"))])])),
                        ("more", map(&[])),
                        ("layers", SdkValue::List(vec![map(&[("id", t("l1"))])])),
                        ("moreLayers", SdkValue::Bool(false)),
                    ]);
                }
                fields.push(("workspaces", SdkValue::List(items.to_vec())));
                Reply::Sdk(map(&fields))
            }
            Request::Sdk(sdk) if sdk.method == OPERATION => Reply::Sdk(
                CapacityModel {
                    admission: AdmissionModel {
                        pool_vacant: 7,
                        pool_running: 1,
                        reserved_vacant: 1,
                        reserved_running: 0,
                    },
                    retained: RetainedModel::default(),
                }
                .to_sdk(),
            ),
            _ => continue,
        };
        if channel
            .send
            .send(&Answer { id, reply }.encode().unwrap())
            .is_err()
        {
            break;
        }
    }
}

/// The home's records: machine `machine` on `platform` (its daemon an
/// existing file, so nothing looks for a bundle), recorded `running` with
/// its `worker` record when it runs, sqlite Store `shop` on it and project
/// `api`.
fn home_records(home: &Path, machine: &str, platform: &str, worker: Option<(SocketAddr, &str)>) {
    let daemon = std::env::current_exe().unwrap().display().to_string();
    let mut fields = vec![
        ("name", t(machine)),
        ("platform", t(platform)),
        ("daemon", t(&daemon)),
    ];
    // every 2.0 create records its provider name; `native` has none
    let vm_name = format!("layerfs-00000000-{machine}");
    if platform != "native" {
        fields.push(("vm_name", t(&vm_name)));
    }
    if worker.is_some() {
        fields.push(("state", t("running")));
    }
    write_record(&paths::machine_record(home, machine), &map(&fields)).unwrap();
    if let Some((address, secret)) = worker {
        write_record(
            &paths::worker_record(home, machine),
            &map(&[("address", t(&address.to_string())), ("secret", t(secret))]),
        )
        .unwrap();
    }
    write_record(
        &paths::store_record(home, "shop"),
        &map(&[
            ("name", t("shop")),
            ("kind", t("sqlite")),
            ("binding", SdkValue::Bytes(vec![7; 16])),
            ("cursor", SdkValue::Bytes(vec![8; 32])),
            ("profile", t("disposable")),
            ("machine", t(machine)),
        ]),
    )
    .unwrap();
    write_record(
        &paths::project_record(home, "shop", "api"),
        &map(&[("name", t("api")), ("created", SdkValue::Int(5))]),
    )
    .unwrap();
}

/// The daemon keys of the stand-in at `endpoint`.
fn keys(endpoint: SocketAddr) -> SdkValue {
    map(&[
        ("endpoint", t(&endpoint.to_string())),
        ("secret", SdkValue::Bytes(CLIENT_PRIVATE.to_vec())),
        (
            "peer",
            SdkValue::Bytes(duplex::public_key(&DAEMON_PRIVATE).unwrap().to_vec()),
        ),
        ("instance", SdkValue::Bytes(DAEMON_INSTANCE.to_vec())),
    ])
}

/// Sandbox `s1` in `root` (the home, or a worker's root) as the side that
/// runs it records it: a native daemon process (this test's own, so it is
/// found alive) listening at `endpoint`, under the name the home with id
/// `home_id` gives it (`layerfs-<id8>-s1`), with one Workspace record per
/// listed `items`.
fn local_sandbox(root: &Path, home_id: &str, endpoint: SocketAddr, items: &[SdkValue]) {
    let name = format!("layerfs-{}-s1", &home_id[..8]);
    sandbox_named(root, &name, endpoint, items);
}

/// [`local_sandbox`], under the run-folder name `name`.
fn sandbox_named(root: &Path, name: &str, endpoint: SocketAddr, items: &[SdkValue]) {
    let name = name.to_owned();
    let me = layerfs_bridge::process::LocalProcess::observe(std::process::id()).unwrap();
    let daemon = DaemonRecord::local(Local {
        platform: "macos".to_owned(),
        name: name.clone(),
        container: "ab".repeat(32),
        runtime: "native",
        uid: home::flock::euid(),
        gid: home::flock::egid(),
        directory: paths::run_root(root).join(&name).display().to_string(),
        process: (me.pid, me.start, format!("{:032x}", me.boot.unwrap())),
        port: endpoint.port(),
        image: "local".to_owned(),
        labels: Vec::new(),
    });
    write_record(
        &paths::sandbox_record(root, "s1"),
        &map(&[
            ("name", t("s1")),
            ("project", t("shop/api")),
            ("machine", t("native")),
            ("id", t("s1")),
            ("runtime", t("native")),
            ("daemon", daemon.to_record()),
        ]),
    )
    .unwrap();
    write_record(&paths::sandbox_control(root, "s1"), &keys(endpoint)).unwrap();
    for item in items {
        let id = item.get_text("workspace").unwrap();
        let n = item.get_int("namespace").unwrap();
        let record = WorkspaceRecord::mounted(
            id.to_owned(),
            name_of(n).as_deref(),
            item.get_text("branch").unwrap(),
            item.get_text("directory").unwrap(),
            n,
        );
        let file = paths::sandbox_dir(root, "s1")
            .join("workspace")
            .join(format!("{id}.json"));
        record.write(&file).unwrap();
    }
}

/// Every file under `root` with its inode, modification time and size:
/// each record, key file, lock and lease. The call log (`trace/`) is left
/// out: the call surface appends one line to it for every public call
/// (#659), a get included, and that is not this change's. So is each
/// `observed.json`: a get that reached the daemon notes when there.
fn files(root: &Path) -> BTreeMap<PathBuf, (u64, i64, i64, u64)> {
    let mut found = BTreeMap::new();
    let mut folders = vec![root.to_owned()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            let meta = std::fs::symlink_metadata(&path).unwrap();
            if path == root.join("trace") || path.file_name() == Some("observed.json".as_ref()) {
                continue;
            }
            if meta.is_dir() {
                folders.push(path);
            } else {
                found.insert(
                    path,
                    (meta.ino(), meta.mtime(), meta.mtime_nsec(), meta.len()),
                );
            }
        }
    }
    found
}

/// One public call on `home` before the get under test. The first call a
/// process makes on a home writes the home's built-in machine records
/// (once, #535 R6), whichever call it is, so the get is not that call.
fn settle(home: &Path) {
    layerfs_api::call(Some(home), "sandbox.list", SdkValue::Null, &mut |_| {}).unwrap();
}

/// A Store being pushed, as far as another call can see it: its record
/// carries a running `store.move` intent and its lock is held. The lock is
/// released when the answer drops.
fn pushing(home: &Path) -> impl Drop {
    let op = layerfs_home::op::Op::begin("store.move", "check", SdkValue::Null).unwrap();
    let record = layerfs_store::record::StoreRecord::read(home, "shop")
        .unwrap()
        .unwrap();
    record
        .with(layerfs_home::op::FIELD, op.to_record().clone())
        .write(home)
        .unwrap();
    layerfs_store::record::lock(home, "shop").unwrap()
}

/// `root` (a home or a worker's root) holds no trace of a lease or an open
/// Store of `shop`, and no run root was made in it.
fn untouched(root: &Path) {
    assert!(
        !paths::lease_dir(root, "shop").exists(),
        "a lease folder was made in {}",
        root.display()
    );
    assert!(!lease::held_here(root, "shop"));
    let file = paths::store_file(root, "shop");
    assert!(!file.exists(), "a Store file was made");
    assert!(
        !layerfs_store::open::close(&file).unwrap(),
        "the Store was open in this process"
    );
    assert!(
        !paths::run_root(root).exists(),
        "a run root was made in {}",
        root.display()
    );
}

/// The answer's fields a client reads for a running Sandbox that answered.
fn running(got: &SdkValue, items: &[SdkValue]) {
    assert_eq!(got.get_text("sandbox"), Some("s1"), "{got:?}");
    assert_eq!(got.get_text("project"), Some("shop/api"), "{got:?}");
    assert_eq!(got.get_text("state"), Some("ready"), "{got:?}");
    assert_eq!(workspaces(got), shown(items), "{got:?}");
    for gone in ["name", "next_page_pos", "down_reason", "down", "daemon"] {
        assert!(got.get(gone).is_none(), "{gone}: {got:?}");
    }
    let stats = stats(got);
    let capacity = stats
        .get("daemon_capacity")
        .unwrap_or_else(|| panic!("{got:?}"));
    let admission = capacity.get("admission").unwrap();
    assert_eq!(admission.get_int("pool_vacant"), Some(7), "{capacity:?}");
    assert_eq!(got.get_int("workspace_capacity"), Some(16), "{got:?}");
    // each Workspace's stats, by handle, in the list's order: no overlay
    // file (these fixtures have no run folder) and no cgroup of its own,
    // for the Sandbox's own `usage_reason` (`host_process` for these
    // host-process Sandboxes, `not_observed` when the side that runs one
    // did not answer)
    let handles: Vec<SdkValue> = (shown(items).into_iter())
        .map(|workspace| workspace["workspace"].clone())
        .collect();
    assert_eq!(each_stat(got, "workspace"), handles, "{got:?}");
    let reason = stats.get("usage_reason").cloned().unwrap_or_default();
    assert_eq!(each_stat(got, "usage_reason"), vec![reason; items.len()]);
    assert_eq!(each_stat(got, "usage"), vec![SdkValue::Null; items.len()]);
    // what only reopens the Sandbox is never shown: `daemon` is the process
    // a monitoring read saw (layerfs-project#164), `{ pid, started_at }`,
    // or null when the side that runs it did not say; never the record's
    // directory, port, runtime id or forward
    match stats.get("daemon") {
        Some(SdkValue::Map(fields)) => {
            let names: Vec<&str> = fields.iter().map(|(name, _)| name.as_str()).collect();
            assert_eq!(names, ["pid", "started_at"], "{got:?}");
        }
        Some(SdkValue::Null) => {}
        other => panic!("{other:?} in {got:?}"),
    }
}

/// Every list the stand-in answered asked for the Workspaces alone, named
/// the project and carried no PostgreSQL selection.
fn asked_workspaces_only(seen: &Seen) {
    let lists = seen.lists.lock().unwrap();
    assert!(!lists.is_empty(), "the daemon was never asked");
    for args in lists.iter() {
        assert_eq!(
            args.get("history"),
            Some(&SdkValue::Bool(false)),
            "{args:?}"
        );
        assert_eq!(args.get_text("project"), Some("api"), "{args:?}");
        assert!(args.get("postgresql").is_none(), "{args:?}");
    }
}

#[test]
fn a_native_sandbox_s_get_takes_no_lease_and_writes_nothing() {
    for count in [0, 1, 4] {
        let items: Vec<SdkValue> = (1..=count).map(item).collect();
        let (endpoint, seen) = daemon(items.clone(), false);
        let home = tempdir(&format!("native-{count}"));
        let id = home::open(&home).unwrap().id;
        home_records(&home, "native", "native", None);
        local_sandbox(&home, &id, endpoint, &items);
        settle(&home);
        let _push = pushing(&home);
        let before = files(&home);
        let at = Instant::now();
        let got = get(&home).unwrap();
        // the Store's lock was held throughout: a get that took it would
        // still be waiting, and one that read the intent would be `Busy`
        assert!(at.elapsed() < Duration::from_secs(10), "{:?}", at.elapsed());
        running(&got, &items);
        assert_eq!(files(&home), before, "a get changed a file in the home");
        untouched(&home);
        asked_workspaces_only(&seen);
        // a second get is the same answer, from the connection it kept
        let again = get(&home).unwrap();
        assert_eq!(workspaces(&again), shown(&items));
        assert_eq!(seen.connections.load(Ordering::SeqCst), 1);
        assert_eq!(files(&home), before);
        untouched(&home);
        std::fs::remove_dir_all(home).unwrap();
    }
}

#[test]
fn the_holder_of_the_store_finds_no_lease_after_a_get() {
    let items = vec![item(1)];
    let (endpoint, _seen) = daemon(items.clone(), false);
    let home = tempdir("holder");
    let id = home::open(&home).unwrap().id;
    home_records(&home, "native", "native", None);
    local_sandbox(&home, &id, endpoint, &items);
    settle(&home);
    let push = pushing(&home);
    running(&get(&home).unwrap(), &items);
    // what `store push`, `pull`, `remove` and `disconnect` check under the
    // Store's lock: no process holds the Store. The Sandbox's own record is
    // what still holds it, as it did before any get.
    assert_eq!(
        lease::read_holders(&home, "shop").unwrap(),
        (Vec::new(), Vec::new())
    );
    let refusal = layerfs_control::inventory::store_idle(&home, "shop").unwrap_err();
    assert_eq!(refusal.code, "InUse", "{refusal:?}");
    let Some(SdkValue::List(related)) = refusal.detail.get("related") else {
        panic!("{refusal:?}")
    };
    let related: Vec<_> = related
        .iter()
        .map(|entry| (entry.get_text("kind"), entry.get_text("name")))
        .collect();
    assert_eq!(related, [(Some("sandbox"), Some("s1"))], "{refusal:?}");
    drop(push);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_daemon_from_before_the_option_gives_the_same_answer() {
    let items: Vec<SdkValue> = (1..=4).map(item).collect();
    let mut answers = Vec::new();
    for old in [false, true] {
        let (endpoint, seen) = daemon(items.clone(), old);
        let home = tempdir(&format!("old-{old}"));
        let id = home::open(&home).unwrap().id;
        home_records(&home, "native", "native", None);
        local_sandbox(&home, &id, endpoint, &items);
        settle(&home);
        let got = get(&home).unwrap();
        running(&got, &items);
        asked_workspaces_only(&seen);
        // the answer's own field names, the Workspaces' included
        let SdkValue::Map(fields) = &got else {
            panic!("{got:?}")
        };
        let names: Vec<String> = fields.iter().map(|(name, _)| name.clone()).collect();
        answers.push((names, workspaces(&got)));
        untouched(&home);
        std::fs::remove_dir_all(home).unwrap();
    }
    assert_eq!(answers[0], answers[1]);
    // nothing of the read model's history half reaches the answer
    for field in ["branches", "commits", "layers", "store", "version", "kind"] {
        assert!(!answers[1].0.iter().any(|name| name == field), "{field}");
    }
}

/// A get reopens a Sandbox by the run-folder name its record holds, on
/// this process's side and on a worker's: the name a new Sandbox gets
/// (`layerfs-<id8>-s1`) like the earlier one every other test here records
/// (layerfs-project#180). A record under any other name is refused, with
/// nothing it names used.
#[test]
fn a_get_reopens_a_sandbox_under_the_name_its_record_holds() {
    let items = vec![item(1)];
    // native: this process reopens it
    let (endpoint, seen) = daemon(items.clone(), false);
    let home = tempdir("named-native");
    let id = home::open(&home).unwrap().id;
    home_records(&home, "native", "native", None);
    sandbox_named(&home, &format!("layerfs-{}-s1", &id[..8]), endpoint, &items);
    running(&get(&home).unwrap(), &items);
    asked_workspaces_only(&seen);
    std::fs::remove_dir_all(home).unwrap();

    // a machine's worker reopens it under its root, with the caller's id
    let (endpoint, seen) = daemon(items.clone(), false);
    let scratch = tempdir("named-worker");
    let (home, root) = (scratch.join("home"), scratch.join("data"));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let id = home::open(&home).unwrap().id;
    home_records(&home, "box", "docker", Some((worker(&root), "k")));
    worker_sandbox(&home);
    sandbox_named(&root, &format!("layerfs-{}-s1", &id[..8]), endpoint, &items);
    running(&get(&home).unwrap(), &items);
    asked_workspaces_only(&seen);
    std::fs::remove_dir_all(scratch).unwrap();

    // neither name: refused before the daemon is asked
    let (endpoint, seen) = daemon(items.clone(), false);
    let home = tempdir("named-other");
    let id = home::open(&home).unwrap().id;
    home_records(&home, "native", "native", None);
    sandbox_named(&home, &format!("other-{}-s1", &id[..8]), endpoint, &items);
    let (code, message) = get(&home).unwrap_err();
    assert_eq!(code, "InvalidInput", "{message}");
    assert!(message.contains("neither used nor deleted"), "{message}");
    assert_eq!(seen.connections.load(Ordering::SeqCst), 0);
    std::fs::remove_dir_all(home).unwrap();
}

/// A worker serving `root` behind secret `k`, once it answers.
fn worker(root: &Path) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let served = root.to_owned();
    std::thread::spawn(move || layerfs_api::worker::serve(listener, "k".to_owned(), served));
    let endpoint = layerfs_api::worker::Endpoint {
        address,
        secret: "k".to_owned(),
    };
    for _ in 0..200 {
        if layerfs_api::worker::ping(&endpoint).is_ok() {
            return address;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the worker did not answer within 2 s");
}

/// Sandbox `s1` on machine `box` as the home records it: reached through
/// that machine's worker.
fn worker_sandbox(home: &Path) {
    write_record(
        &paths::sandbox_record(home, "s1"),
        &map(&[
            ("name", t("s1")),
            ("project", t("shop/api")),
            ("machine", t("box")),
            ("id", t("s1")),
        ]),
    )
    .unwrap();
}

#[test]
fn a_worker_runs_a_get_s_reads_without_a_record_or_a_store() {
    for count in [0, 1, 4] {
        let items: Vec<SdkValue> = (1..=count).map(item).collect();
        let (endpoint, seen) = daemon(items.clone(), false);
        let scratch = tempdir(&format!("worker-{count}"));
        let (home, root) = (scratch.join("home"), scratch.join("data"));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let id = home::open(&home).unwrap().id;
        home_records(&home, "box", "docker", Some((worker(&root), "k")));
        worker_sandbox(&home);
        local_sandbox(&root, &id, endpoint, &items);
        settle(&home);
        let _push = pushing(&home);
        let (before, guest) = (files(&home), files(&root));
        let got = get(&home).unwrap();
        running(&got, &items);
        asked_workspaces_only(&seen);
        // the caller's side: no lease, no Store, no record changed
        assert_eq!(files(&home), before, "a get changed a file in the home");
        untouched(&home);
        // the worker's side: it kept neither record the caller sent and
        // opened no Store under its root
        assert_eq!(files(&root), guest, "a get changed a file under the root");
        assert!(!paths::store_record(&root, "shop").exists());
        assert!(!paths::project_record(&root, "shop", "api").exists());
        untouched(&root);
        std::fs::remove_dir_all(scratch).unwrap();
    }
}

#[test]
fn a_worker_leaves_the_records_it_holds_as_they_are() {
    let items = vec![item(1), item(2)];
    let (endpoint, _seen) = daemon(items.clone(), false);
    let scratch = tempdir("worker-kept");
    let (home, root) = (scratch.join("home"), scratch.join("data"));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let id = home::open(&home).unwrap().id;
    home_records(&home, "box", "docker", Some((worker(&root), "k")));
    worker_sandbox(&home);
    local_sandbox(&root, &id, endpoint, &items);
    // records another call would rewrite: a Store record that is not the
    // thin one, and a project record of another `created` than the caller's
    let store = map(&[("name", t("shop")), ("kind", t("sqlite")), ("old", t("x"))]);
    write_record(&paths::store_record(&root, "shop"), &store).unwrap();
    let project = map(&[("name", t("api")), ("created", SdkValue::Int(1))]);
    write_record(&paths::project_record(&root, "shop", "api"), &project).unwrap();
    settle(&home);
    let guest = files(&root);
    running(&get(&home).unwrap(), &items);
    assert_eq!(files(&root), guest, "a get changed a file under the root");
    let kept = |path: &Path| read_record(path).unwrap();
    assert_eq!(kept(&paths::store_record(&root, "shop")), Some(store));
    assert_eq!(
        kept(&paths::project_record(&root, "shop", "api")),
        Some(project)
    );
    assert!(!paths::lease_dir(&root, "shop").exists());
    assert!(!paths::run_root(&root).exists());
    std::fs::remove_dir_all(scratch).unwrap();
}

/// Sandbox `s1` on `native` whose daemon this host reaches through a
/// forward to `endpoint` (a port nothing carries), with one Workspace
/// record per listed `items` in the home's folder.
fn forwarded_sandbox(home: &Path, endpoint: SocketAddr, items: &[SdkValue]) {
    let forward = map(&[
        ("address", t(&endpoint.to_string())),
        ("machine", t("native")),
    ]);
    write_record(
        &paths::sandbox_record(home, "s1"),
        &map(&[
            ("name", t("s1")),
            ("project", t("shop/api")),
            ("machine", t("native")),
            ("id", t("s-1")),
            (
                "daemon",
                map(&[("name", t("lfs-x-s1")), ("forward", forward)]),
            ),
        ]),
    )
    .unwrap();
    write_record(&paths::sandbox_control(home, "s1"), &keys(endpoint)).unwrap();
    for item in items {
        let id = item.get_text("workspace").unwrap();
        let n = item.get_int("namespace").unwrap();
        let record = WorkspaceRecord::mounted(
            id.to_owned(),
            name_of(n).as_deref(),
            item.get_text("branch").unwrap(),
            item.get_text("directory").unwrap(),
            n,
        );
        let file = paths::sandbox_dir(home, "s1")
            .join("workspace")
            .join(format!("{id}.json"));
        record.write(&file).unwrap();
    }
}

#[test]
fn a_forwarded_sandbox_s_get_takes_no_lease_and_writes_nothing() {
    let _turn = inspect_turn();
    for count in [0, 1, 4] {
        let items: Vec<SdkValue> = (1..=count).map(item).collect();
        let (endpoint, seen) = daemon(items.clone(), false);
        let home = tempdir(&format!("forwarded-{count}"));
        home::open(&home).unwrap();
        // the forward's machine answers: its worker is recorded
        let nowhere: SocketAddr = "127.0.0.1:1".parse().unwrap();
        home_records(&home, "native", "native", Some((nowhere, "5e")));
        forwarded_sandbox(&home, endpoint, &items);
        settle(&home);
        let _push = pushing(&home);
        let before = files(&home);
        let got = get(&home).unwrap();
        running(&got, &items);
        asked_workspaces_only(&seen);
        assert_eq!(files(&home), before, "a get changed a file in the home");
        untouched(&home);
        std::fs::remove_dir_all(home).unwrap();
    }
}

#[test]
fn a_stopped_machine_s_get_is_its_record_and_touches_nothing() {
    // a machine that records no worker: nothing starts it (#535 D11)
    let home = tempdir("stopped");
    home::open(&home).unwrap();
    home_records(&home, "box", "docker", None);
    worker_sandbox(&home);
    settle(&home);
    let _push = pushing(&home);
    let before = files(&home);
    let got = get(&home).unwrap();
    // blocked by its stopped machine, and not asked: no Workspaces, no stats
    assert_eq!(got.get_text("state"), Some("blocked"), "{got:?}");
    let by = got.get("blocked_by").unwrap_or_else(|| panic!("{got:?}"));
    assert_eq!(
        (by.get_text("machine"), by.get_text("reason")),
        (Some("box"), Some("stopped")),
        "{got:?}"
    );
    let why = got.get_text("message").unwrap_or_else(|| panic!("{got:?}"));
    assert!(why.contains("machine box is stopped"), "{why}");
    unasked(&got);
    assert_eq!(files(&home), before, "a get changed a file in the home");
    untouched(&home);
    std::fs::remove_dir_all(home).unwrap();

    // a Sandbox a confirmed machine stop ended: answered from its record
    let (endpoint, seen) = daemon(vec![item(1)], false);
    let home = tempdir("stop-recorded");
    let id = home::open(&home).unwrap().id;
    home_records(&home, "native", "native", None);
    local_sandbox(&home, &id, endpoint, &[]);
    let path = paths::sandbox_record(&home, "s1");
    let record = read_record(&path).unwrap().unwrap();
    let record = home::with_field(record, "machine_stopped", SdkValue::Bool(true));
    write_record(&path, &record).unwrap();
    settle(&home);
    let _push = pushing(&home);
    let before = files(&home);
    let got = get(&home).unwrap();
    // broken, its runtime missing: the fix is to remove it
    assert_eq!(got.get_text("state"), Some("broken"), "{got:?}");
    assert_eq!(got.get_text("reason"), Some("missing"), "{got:?}");
    assert_eq!(got.get_text("fix"), Some("layerfs sandbox remove s1"));
    unasked(&got);
    assert_eq!(seen.connections.load(Ordering::SeqCst), 0, "{got:?}");
    assert_eq!(files(&home), before, "a get changed a file in the home");
    untouched(&home);
    std::fs::remove_dir_all(home).unwrap();
}

/// A get whose daemon was not asked: no Workspaces and no stats, and
/// control's own constant `workspace_capacity` there all the same.
fn unasked(got: &SdkValue) {
    for field in ["workspaces", "stats"] {
        let value = got.get(field).unwrap_or(&SdkValue::Null);
        assert_eq!(value, &SdkValue::Null, "{field}: {got:?}");
    }
    assert_eq!(got.get_int("workspace_capacity"), Some(16), "{got:?}");
}

/// The three sides a get's Workspace list is built on.
const SIDES: [&str; 3] = ["native", "worker", "forwarded"];

/// Sandbox `s1` with Workspaces `items` on `side`: the scratch folder to
/// remove, the home, and the root that keeps the Sandbox's Workspace and
/// exec records (the home itself, but a worker's root on `worker`).
fn on_side(side: &str, tag: &str, items: &[SdkValue]) -> (PathBuf, PathBuf, PathBuf) {
    let (endpoint, _seen) = daemon(items.to_vec(), false);
    let scratch = tempdir(&format!("{tag}-{side}"));
    let (home, root) = (scratch.join("home"), scratch.join("data"));
    let id = home::open(&home).unwrap().id;
    match side {
        "native" => {
            home_records(&home, "native", "native", None);
            local_sandbox(&home, &id, endpoint, items);
            (scratch, home.clone(), home)
        }
        "worker" => {
            std::fs::create_dir_all(&root).unwrap();
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
            home_records(&home, "box", "docker", Some((worker(&root), "k")));
            worker_sandbox(&home);
            local_sandbox(&root, &id, endpoint, items);
            (scratch, home, root)
        }
        _ => {
            let nowhere: SocketAddr = "127.0.0.1:1".parse().unwrap();
            home_records(&home, "native", "native", Some((nowhere, "5e")));
            forwarded_sandbox(&home, endpoint, items);
            (scratch, home.clone(), home)
        }
    }
}

/// Field `name` of each Workspace `got` lists, in order.
fn each(got: &SdkValue, name: &str) -> Vec<SdkValue> {
    (workspaces(got).iter())
        .map(|workspace| {
            workspace
                .get(name)
                .cloned()
                .unwrap_or_else(|| panic!("no {name}"))
        })
        .collect()
}

/// Another client: a process that is not this one, which owns a command's
/// record as the client that started the command does while it runs.
struct Client(std::process::Child);

impl Client {
    fn start() -> Self {
        let sleep = std::process::Command::new("/bin/sleep").arg("60").spawn();
        Client(sleep.expect("/bin/sleep starts"))
    }

    /// How a record names it its owner: its id, start time and boot.
    fn owner(&self) -> SdkValue {
        let me = layerfs_bridge::process::LocalProcess::observe(self.0.id()).unwrap();
        map(&[
            ("pid", SdkValue::Int(i64::from(me.pid))),
            ("pid_start", t(&me.start.to_string())),
            ("pid_boot", t(&format!("{:032x}", me.boot.unwrap()))),
        ])
    }

    /// Ends it, as a client that crashed: its records stay.
    fn end(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.end();
    }
}

/// One command `sandbox.get` lists under `commands`.
fn command(exec_id: &str, started: i64, tag: Option<&str>, program: Option<&str>) -> SdkValue {
    map(&[
        ("exec", t(exec_id)),
        ("started_at", t(&layerfs_home::time::iso(started))),
        ("program", SdkValue::opt_text(program.map(str::to_owned))),
        ("tag", SdkValue::opt_text(tag.map(str::to_owned))),
    ])
}

/// The exec ids `ids` as a Workspace's `running` lists them.
fn execs(ids: &[&str]) -> SdkValue {
    SdkValue::List(ids.iter().map(|id| t(id)).collect())
}

#[test]
fn a_get_lists_the_commands_another_client_runs_in_each_workspace() {
    let _turn = inspect_turn();
    use layerfs_sandbox::exec::id::ExecId;
    use layerfs_sandbox::exec::record::{file, ExecRecord, Route};
    for side in SIDES {
        let items: Vec<SdkValue> = (1..=4).map(item).collect();
        let (scratch, home, root) = on_side(side, "running", &items);
        let folder = paths::sandbox_dir(&root, "s1");
        let mut other = Client::start();
        // the record `workspace.exec` keeps while its command runs, in the
        // Workspace `n`, owned by `owner`; `old` leaves out the fields a
        // record had none of before layerfs-project#178
        let record = |n: i64, id: &str, started: i64, owner: SdkValue, old: bool| {
            let workspace = format!("{n:064x}");
            let exec = ExecId::parse(id).unwrap();
            let (tag, line) = match id {
                "e-tagged" => (Some("dsh:s9:t1"), "CI=1 npm test -- --token=zq9"),
                _ => (None, "sleep 60"),
            };
            let mut made =
                ExecRecord::new(&exec, &workspace, ("/workspaces", None)).started_by(tag, line);
            made.started = started;
            made.owner = owner;
            if side == "forwarded" {
                made.route = Route::Machine("native".to_owned());
            }
            let SdkValue::Map(fields) = made.encode() else {
                panic!()
            };
            let fields = (fields.into_iter())
                .filter(|(name, _)| !(old && matches!(name.as_str(), "tag" | "program")));
            let path = file(&folder, &workspace, id);
            layerfs_home::home::write_entity(&path, &SdkValue::Map(fields.collect())).unwrap();
            path
        };
        let tagged = record(2, "e-tagged", 2_000, other.owner(), false);
        record(2, "e-plain", 1_000, other.owner(), false);
        record(3, "e-before", 3_000, other.owner(), true);
        // a command whose owner is gone: its record is stale
        let stale = record(1, "e-stale", 500, map(&[("pid", SdkValue::Int(0))]), false);
        settle(&home);
        let _push = pushing(&home);
        let before = (files(&home), files(&root));

        let got = get(&home).unwrap();
        assert_eq!(got.get_text("state"), Some("ready"), "{side}: {got:?}");
        assert_eq!(got.get_int("workspace_capacity"), Some(16), "{side}");
        let none = SdkValue::List(Vec::new());
        assert_eq!(
            each(&got, "commands"),
            [
                none.clone(),
                SdkValue::List(vec![
                    command("e-plain", 1_000, None, Some("sleep")),
                    command("e-tagged", 2_000, Some("dsh:s9:t1"), Some("npm")),
                ]),
                SdkValue::List(vec![command("e-before", 3_000, None, None)]),
                none.clone(),
            ],
            "{side}: {got:?}"
        );
        assert_eq!(
            each(&got, "running"),
            [
                none.clone(),
                execs(&["e-plain", "e-tagged"]),
                execs(&["e-before"]),
                none.clone(),
            ],
            "{side}: {got:?}"
        );
        // nothing of a command but its program is shown
        let text = format!("{got:?}");
        for kept_out in ["zq9", "token", "CI=1"] {
            assert!(!text.contains(kept_out), "{side}: {kept_out}");
        }
        // a read: every record is as it was, the stale one included
        assert_eq!((files(&home), files(&root)), before, "{side}");
        untouched(&home);

        // a command that ended: its owner removed its record
        std::fs::remove_file(&tagged).unwrap();
        let got = get(&home).unwrap();
        assert_eq!(
            each(&got, "commands")[1],
            SdkValue::List(vec![command("e-plain", 1_000, None, Some("sleep"))]),
            "{side}: {got:?}"
        );
        assert_eq!(each(&got, "running")[1], execs(&["e-plain"]), "{side}");

        // the other client is gone: what it left is stale, listed no more
        // and still there for the cancel, unmount or close that ends it
        other.end();
        let got = get(&home).unwrap();
        assert_eq!(each(&got, "running"), vec![none.clone(); 4], "{side}");
        assert_eq!(each(&got, "commands"), vec![none; 4], "{side}: {got:?}");
        assert!(stale.exists() && file(&folder, &format!("{:064x}", 3), "e-before").exists());
        std::fs::remove_dir_all(scratch).unwrap();
    }
}

#[test]
fn a_get_sizes_each_workspace_s_overlay_file_by_path() {
    let _turn = inspect_turn();
    for side in SIDES {
        let items: Vec<SdkValue> = (1..=4).map(item).collect();
        let (scratch, home, root) = on_side(side, "overlay", &items);
        // the run folder the Sandbox's record names on the side that runs
        // it; a forwarded Sandbox's is in its machine, not in this home
        let record = read_record(&paths::sandbox_record(&root, "s1")).unwrap();
        let named = record
            .unwrap()
            .get("daemon")
            .unwrap()
            .get_text("directory")
            .map(PathBuf::from);
        assert_eq!(named.is_none(), side == "forwarded", "{side}");
        let run = named.unwrap_or_else(|| paths::run_root(&root).join("lfs-x-s1"));
        let overlay = run.join("overlay");
        std::fs::create_dir_all(&overlay).unwrap();
        std::fs::write(overlay.join("overlay.sqlite"), vec![0; 4096]).unwrap();
        std::fs::write(overlay.join("1.sqlite"), vec![1; 8192]).unwrap();
        // one nobody can open: its size is read from its folder entry
        std::fs::write(overlay.join("2.sqlite"), vec![2; 20_480]).unwrap();
        let closed = std::fs::Permissions::from_mode(0o000);
        std::fs::set_permissions(overlay.join("2.sqlite"), closed).unwrap();
        // a link is not followed, and Workspace 4 has no file
        std::os::unix::fs::symlink(overlay.join("1.sqlite"), overlay.join("3.sqlite")).unwrap();
        settle(&home);
        let _push = pushing(&home);
        let before = (files(&home), files(&root));

        let got = get(&home).unwrap();
        assert_eq!(got.get_text("state"), Some("ready"), "{side}: {got:?}");
        let sizes = match side {
            "forwarded" => vec![SdkValue::Null; 4],
            _ => vec![
                SdkValue::Int(8192),
                SdkValue::Int(20_480),
                SdkValue::Null,
                SdkValue::Null,
            ],
        };
        assert_eq!(each_stat(&got, "overlay_bytes"), sizes, "{side}: {got:?}");
        assert_eq!((files(&home), files(&root)), before, "{side}");
        assert!(!paths::lease_dir(&home, "shop").exists(), "{side}");
        std::fs::remove_dir_all(scratch).unwrap();
    }
}

/// Sandbox `s1` on machine `box`, its daemon reached from this host
/// through a forward to `endpoint`: the home keeps its record, its keys
/// and one Workspace record per listed `items`, and names no run folder.
fn forwarded_on_box(home: &Path, endpoint: SocketAddr, items: &[SdkValue]) {
    forwarded_sandbox(home, endpoint, items);
    let path = paths::sandbox_record(home, "s1");
    let record = read_record(&path).unwrap().unwrap();
    let forward = map(&[("address", t(&endpoint.to_string())), ("machine", t("box"))]);
    let daemon = map(&[("name", t("lfs-x-s1")), ("forward", forward)]);
    let record = home::with_field(record, "machine", t("box"));
    write_record(&path, &home::with_field(record, "daemon", daemon)).unwrap();
}

#[test]
fn a_forwarded_get_sizes_its_overlays_in_its_machine_or_leaves_them_null() {
    let items: Vec<SdkValue> = (1..=4).map(item).collect();
    let (endpoint, _seen) = daemon(items.clone(), false);
    let scratch = tempdir("forwarded-sizes");
    let (home, root) = (scratch.join("home"), scratch.join("data"));
    home::open(&home).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    // the machine's worker answers: the run folder and the overlay files
    // are under its root, the Workspace records are in the home
    home_records(&home, "box", "docker", Some((worker(&root), "k")));
    forwarded_on_box(&home, endpoint, &items);
    sandbox_named(&root, "lfs-x-s1", endpoint, &[]);
    let overlay = paths::run_root(&root).join("lfs-x-s1/overlay");
    std::fs::create_dir_all(&overlay).unwrap();
    std::fs::write(overlay.join("overlay.sqlite"), vec![0; 4096]).unwrap();
    std::fs::write(overlay.join("1.sqlite"), vec![1; 8192]).unwrap();
    std::fs::write(overlay.join("2.sqlite"), vec![2; 20_480]).unwrap();
    assert!(!paths::run_root(&home).join("lfs-x-s1").exists());
    settle(&home);
    let _push = pushing(&home);
    let before = (files(&home), files(&root));

    let got = get(&home).unwrap();
    assert_eq!(got.get_text("state"), Some("ready"), "{got:?}");
    let ids: Vec<SdkValue> = (items.iter())
        .map(|item| item.get("workspace").unwrap().clone())
        .collect();
    assert_eq!(each(&got, "id"), ids, "{got:?}");
    let sizes = vec![
        SdkValue::Int(8192),
        SdkValue::Int(20_480),
        SdkValue::Null,
        SdkValue::Null,
    ];
    assert_eq!(each_stat(&got, "overlay_bytes"), sizes, "{got:?}");
    assert_eq!((files(&home), files(&root)), before);
    assert!(!paths::lease_dir(&home, "shop").exists());
    assert!(!paths::lease_dir(&root, "shop").exists());

    // the worker stops answering (its port takes a connection and says
    // nothing): the get is still answered, every size null
    let silent = TcpListener::bind("127.0.0.1:0").unwrap();
    write_record(
        &paths::worker_record(&home, "box"),
        &map(&[
            ("address", t(&silent.local_addr().unwrap().to_string())),
            ("secret", t("k")),
        ]),
    )
    .unwrap();
    let before = files(&home);
    let at = std::time::Instant::now();
    let got = get(&home).unwrap();
    assert!(at.elapsed() < Duration::from_secs(15), "{:?}", at.elapsed());
    running(&got, &items);
    assert_eq!(each_stat(&got, "overlay_bytes"), vec![SdkValue::Null; 4]);
    assert_eq!(files(&home), before);
    drop(silent);
    std::fs::remove_dir_all(scratch).unwrap();
}

/// Whether ISO time `at` (`started_at`) is within the last day.
fn recent(at: &str) -> bool {
    let now = home::now_ms();
    let iso = layerfs_home::time::iso;
    at > iso(now - 86_400_000).as_str() && at <= iso(now + 2_000).as_str()
}

/// The record in `root` of Sandbox `s1`, with its `daemon` changed by `change`.
fn redaemon(root: &Path, change: impl FnOnce(SdkValue) -> SdkValue) {
    let path = paths::sandbox_record(root, "s1");
    let record = read_record(&path).unwrap().unwrap();
    let daemon = change(record.get("daemon").unwrap().clone());
    write_record(&path, &home::with_field(record, "daemon", daemon)).unwrap();
}

#[test]
fn a_get_shows_the_daemon_process_as_the_side_that_runs_it_sees_it() {
    let me = layerfs_bridge::process::LocalProcess::observe(std::process::id()).unwrap();
    for side in ["native", "worker"] {
        let items = vec![item(1)];
        let (scratch, home, root) = on_side(side, "process", &items);
        settle(&home);
        let _push = pushing(&home);
        let before = (files(&home), files(&root));
        let got = get(&home).unwrap();
        running(&got, &items);
        // the record names this test's process: a host process, since when
        let daemon = stats(&got).get("daemon").unwrap();
        assert_eq!(daemon.get_int("pid"), Some(i64::from(me.pid)), "{side}");
        let started = daemon.get_text("started_at").unwrap();
        assert!(recent(started), "{started}");
        assert_eq!(
            (
                stats(&got).get("usage"),
                stats(&got).get_text("usage_reason")
            ),
            (Some(&SdkValue::Null), Some("host_process")),
            "{side}: {got:?}"
        );
        assert_eq!((files(&home), files(&root)), before, "{side}");
        untouched(&home);

        // a namespaces Sandbox's usage is its cgroup's: this kernel has no
        // such group (or no cgroup v2 at all), and the get says which
        redaemon(&root, |daemon| {
            home::with_field(daemon, "runtime", t("namespaces"))
        });
        let got = get(&home).unwrap();
        let v2 = Path::new("/sys/fs/cgroup/cgroup.controllers").is_file();
        let why = if v2 { "no_cgroup" } else { "cgroup_v1" };
        assert_eq!(
            (
                stats(&got).get("usage"),
                stats(&got).get_text("usage_reason")
            ),
            (Some(&SdkValue::Null), Some(why)),
            "{side}: {got:?}"
        );
        std::fs::remove_dir_all(scratch).unwrap();
    }
}

/// A Sandbox whose daemon process ended is `broken` `lost` once a get
/// asks for it: its record says so from then on, and no stats are shown,
/// since only a ready Sandbox's are. A stopped machine's is not asked.
#[test]
fn a_get_of_a_sandbox_whose_daemon_ended_finds_it_lost() {
    // a process that ended, as the daemon record names it
    let mut other = Client::start();
    let owner = other.owner();
    other.end();
    let dead = |daemon: SdkValue| {
        let daemon = home::with_field(daemon, "pid", owner.get("pid").unwrap().clone());
        let start = owner.get("pid_start").unwrap().clone();
        let daemon = home::with_field(daemon, "start", start);
        home::with_field(daemon, "boot", owner.get("pid_boot").unwrap().clone())
    };
    for side in ["native", "worker"] {
        let (scratch, home, root) = on_side(side, "ended", &[item(1)]);
        redaemon(&root, dead);
        settle(&home);
        let got = get(&home).unwrap();
        assert_eq!(got.get_text("state"), Some("broken"), "{side}: {got:?}");
        assert_eq!(got.get_text("reason"), Some("lost"), "{side}: {got:?}");
        assert_eq!(got.get_text("fix"), Some("layerfs sandbox remove s1"));
        assert_eq!(got.get("stats"), Some(&SdkValue::Null), "{side}: {got:?}");
        // the reason is the record's now: a list that asks nothing says it
        let record = read_record(&paths::sandbox_record(&home, "s1")).unwrap();
        assert_eq!(record.unwrap().get_text("reason"), Some("lost"), "{side}");
        let listed = layerfs_api::call(Some(&home), "sandbox.list", SdkValue::Null, &mut |_| {});
        let row = match listed.unwrap().get("items") {
            Some(SdkValue::List(rows)) => rows[0].clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(row.get_text("reason"), Some("lost"), "{side}: {row:?}");
        std::fs::remove_dir_all(scratch).unwrap();
    }

    // a stopped machine: not asked, no stats
    let home = tempdir("ended-stopped");
    home::open(&home).unwrap();
    home_records(&home, "box", "docker", None);
    worker_sandbox(&home);
    let got = get(&home).unwrap();
    assert_eq!(got.get_text("state"), Some("blocked"), "{got:?}");
    unasked(&got);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_forwarded_sandbox_s_process_comes_from_its_machine_s_worker() {
    let _turn = inspect_turn();
    for count in [0, 2] {
        let items: Vec<SdkValue> = (1..=count).map(item).collect();
        let (endpoint, _seen) = daemon(items.clone(), false);
        let scratch = tempdir(&format!("forwarded-process-{count}"));
        let (home, root) = (scratch.join("home"), scratch.join("data"));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let id = home::open(&home).unwrap().id;
        home_records(&home, "box", "docker", Some((worker(&root), "k")));
        // the home reaches the daemon directly and keeps its Workspace
        // records; the daemon record with the process and the run folder
        // are the worker's
        forwarded_sandbox(&home, endpoint, &items);
        let path = paths::sandbox_record(&home, "s1");
        let record = read_record(&path).unwrap().unwrap();
        let record = home::with_field(record, "machine", t("box"));
        let forward = map(&[("address", t(&endpoint.to_string())), ("machine", t("box"))]);
        let daemon = map(&[("name", t("lfs-x-s1")), ("forward", forward)]);
        write_record(&path, &home::with_field(record, "daemon", daemon)).unwrap();
        local_sandbox(&root, &id, endpoint, &[]);
        let run = paths::run_root(&root).join(format!("layerfs-{}-s1", &id[..8]));
        std::fs::create_dir_all(run.join("overlay")).unwrap();
        settle(&home);
        let _push = pushing(&home);
        let before = (files(&home), files(&root));
        let got = get(&home).unwrap();
        running(&got, &items);
        let daemon = stats(&got).get("daemon").unwrap();
        assert_eq!(
            daemon.get_int("pid"),
            Some(i64::from(std::process::id())),
            "{count}: {got:?}"
        );
        assert!(daemon.get_text("started_at").is_some_and(recent), "{got:?}");
        assert_eq!(stats(&got).get_text("usage_reason"), Some("host_process"));
        assert_eq!((files(&home), files(&root)), before, "{count}");
        untouched(&home);
        std::fs::remove_dir_all(scratch).unwrap();
    }
}
