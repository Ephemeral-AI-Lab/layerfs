//! `sandbox.get` takes no Store lease, opens no Store and writes no record
//! (layerfs-project#181), on each side that runs its two reads: this
//! process for a `native` Sandbox, a machine's worker, and this process
//! again for a Sandbox whose daemon's port is forwarded here. Its answer is
//! the daemon's Workspace list as before, for 0, 1 and 4 Workspaces and
//! from a daemon that predates the Workspaces-only option, and its record
//! alone for a stopped machine.
//!
//! Nothing runs: a stand-in daemon answers the Workspace list and the
//! capacity model, and the worker is this build's own server on a scratch
//! root. Each home has a Store record and no Store file, so a get that
//! opened the Store would be refused. While a get runs, the test holds the
//! Store's lock and the Store's record carries a running `store.push`
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
        let args = map(&[("name", t("s1"))]);
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

/// `name` as every front end shows a result key: snake_case.
fn snake(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// What `sandbox.get` shows for the daemon's `items`: each item as the
/// daemon answered it (its keys in snake_case, as the call surface shows
/// every result), with its `path`, `name` and `ws`.
fn shown(items: &[SdkValue]) -> Vec<BTreeMap<String, SdkValue>> {
    items
        .iter()
        .map(|item| {
            let SdkValue::Map(fields) = item else {
                panic!("{item:?}")
            };
            let mut fields: BTreeMap<String, SdkValue> = fields
                .iter()
                .map(|(name, value)| (snake(name), value.clone()))
                .collect();
            let path = item.get_text("directory").unwrap().to_owned();
            let name = name_of(item.get_int("namespace").unwrap());
            let ws = format!("s1:{}", name.as_deref().unwrap_or(&path));
            fields.insert("path".to_owned(), t(&path));
            fields.insert("name".to_owned(), SdkValue::opt_text(name));
            fields.insert("ws".to_owned(), t(&ws));
            fields
        })
        .collect()
}

/// The `workspaces` of answer `got`, each as a name-keyed map.
fn workspaces(got: &SdkValue) -> Vec<BTreeMap<String, SdkValue>> {
    let Some(SdkValue::List(items)) = got.get("workspaces") else {
        panic!("no workspaces in {got:?}")
    };
    items
        .iter()
        .map(|item| match item {
            SdkValue::Map(fields) => fields.iter().cloned().collect(),
            other => panic!("{other:?}"),
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
/// existing file, so nothing looks for a bundle), its `worker` record when
/// it runs, sqlite Store `shop` on it and project `api`.
fn home_records(home: &Path, machine: &str, platform: &str, worker: Option<(SocketAddr, &str)>) {
    let daemon = std::env::current_exe().unwrap().display().to_string();
    write_record(
        &paths::machine_record(home, machine),
        &map(&[
            ("name", t(machine)),
            ("platform", t(platform)),
            ("daemon", t(&daemon)),
        ]),
    )
    .unwrap();
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
/// `home_id` gave it before layerfs-project#180 (`lfs-<id8>-s1`, which a
/// Sandbox made then keeps), with one Workspace record per listed `items`.
fn local_sandbox(root: &Path, home_id: &str, endpoint: SocketAddr, items: &[SdkValue]) {
    let name = format!("lfs-{}-s1", &home_id[..8]);
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
/// (#659), a get included, and that is not this change's.
fn files(root: &Path) -> BTreeMap<PathBuf, (u64, i64, i64, u64)> {
    let mut found = BTreeMap::new();
    let mut folders = vec![root.to_owned()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            let meta = std::fs::symlink_metadata(&path).unwrap();
            if path == root.join("trace") {
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
/// carries a running `store.push` intent and its lock is held. The lock is
/// released when the answer drops.
fn pushing(home: &Path) -> impl Drop {
    let op = layerfs_home::op::Op::begin("store.push", "check", SdkValue::Null).unwrap();
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
    assert_eq!(got.get_text("name"), Some("s1"), "{got:?}");
    assert_eq!(got.get_text("project"), Some("shop/api"), "{got:?}");
    assert_eq!(got.get_text("state"), Some("running"), "{got:?}");
    assert_eq!(workspaces(got), shown(items), "{got:?}");
    assert_eq!(got.get("next_page_pos"), Some(&SdkValue::Null), "{got:?}");
    assert!(got.get("down_reason").is_none(), "{got:?}");
    assert!(got.get("down").is_none(), "{got:?}");
    let capacity = got
        .get("daemon_capacity")
        .unwrap_or_else(|| panic!("{got:?}"));
    let admission = capacity.get("admission").unwrap();
    assert_eq!(admission.get_int("pool_vacant"), Some(7), "{capacity:?}");
    // what only reopens the Sandbox is never shown
    assert!(got.get("daemon").is_none(), "{got:?}");
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
        assert_eq!(workspaces(&get(&home).unwrap()), shown(&items));
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
    assert_eq!(got.get_text("state"), Some("running"), "{got:?}");
    assert_eq!(got.get("workspaces"), Some(&SdkValue::List(Vec::new())));
    let why = got
        .get_text("down_reason")
        .unwrap_or_else(|| panic!("{got:?}"));
    assert!(why.contains("layerfs machine start box"), "{why}");
    assert!(got.get("daemon_capacity").is_none(), "{got:?}");
    assert_eq!(got.get("next_page_pos"), Some(&SdkValue::Null), "{got:?}");
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
    assert_eq!(got.get_text("state"), Some("gone"), "{got:?}");
    assert_eq!(got.get("workspaces"), Some(&SdkValue::List(Vec::new())));
    assert!(got.get("daemon_capacity").is_none(), "{got:?}");
    assert_eq!(seen.connections.load(Ordering::SeqCst), 0, "{got:?}");
    assert_eq!(files(&home), before, "a get changed a file in the home");
    untouched(&home);
    std::fs::remove_dir_all(home).unwrap();
}
