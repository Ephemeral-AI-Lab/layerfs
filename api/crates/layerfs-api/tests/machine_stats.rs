//! `machine.stats` and `sandbox.list`'s `live` option (layerfs-project#164)
//! on the two routes of an inspect request: this process for `native`, and
//! a machine's worker (this build's own server on a scratch root) for
//! every other machine, a Sandbox whose daemon the home reaches directly
//! included. Their answers and refusals, and what #179 B asks of every
//! monitoring read: no lock or lease, nothing written on either side,
//! nothing started, no refusal while an operation holds the machine or a
//! Store, and no call-log line for `machine.stats`.
//!
//! Nothing runs but short `sleep`s: Stores are files this test writes
//! (never opened by a read), and a daemon is this test's own process or a
//! child that ended. Inspect permits are the process's, and the worker is
//! in this process, so every test here takes one lock.
#![cfg(unix)]
use layerfs_api::home::{self, lease, paths, read_record, write_record};
use layerfs_bridge::control::SdkValue;
use layerfs_bridge::process::LocalProcess;
use layerfs_sandbox::record::{DaemonRecord, Local};
use std::collections::BTreeMap;
use std::net::{SocketAddr, TcpListener};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

static PROCESS: Mutex<()> = Mutex::new(());
static HOMES: AtomicUsize = AtomicUsize::new(0);

fn serial() -> MutexGuard<'static, ()> {
    PROCESS.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// A new private folder under the build's own temporary folder.
fn tempdir(tag: &str) -> PathBuf {
    let sequence = HOMES.fetch_add(1, Ordering::Relaxed);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "lfs-mstats-{tag}-{}-{sequence}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn map(fields: &[(&str, SdkValue)]) -> SdkValue {
    SdkValue::Map(
        fields
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect(),
    )
}

fn print(value: &SdkValue) -> String {
    layerfs_api::json::print(value)
}

/// One call on a thread of its own: an answer that does not come within
/// 20 s fails the test (a call that waited for the Store's lock would wait
/// 120 s).
fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, (String, String)> {
    let (home, name, args) = (home.to_owned(), name.to_owned(), map(args));
    let (sender, answer) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let got = layerfs_api::call(Some(&home), &name, args, &mut |_| {});
        let _ = sender.send(got.map_err(|r| (r.code, r.message)));
    });
    answer
        .recv_timeout(Duration::from_secs(20))
        .expect("the call did not answer within 20 s")
}

fn stats(home: &Path, machine: &str) -> Result<SdkValue, (String, String)> {
    call(home, "machine.stats", &[("machine", t(machine))])
}

/// `sandbox.list` with `live`: each item's `(name, state, live, live_reason)`.
fn live(home: &Path) -> Vec<(String, String, SdkValue, Option<String>)> {
    let listed = call(home, "sandbox.list", &[("live", SdkValue::Bool(true))]).unwrap();
    let Some(SdkValue::List(items)) = listed.get("items") else {
        panic!("{listed:?}")
    };
    items
        .iter()
        .map(|item| {
            (
                item.get_text("name").unwrap().to_owned(),
                item.get_text("state").unwrap().to_owned(),
                item.get("live")
                    .cloned()
                    .unwrap_or_else(|| panic!("{item:?}")),
                item.get_text("live_reason").map(str::to_owned),
            )
        })
        .collect()
}

fn seen(
    name: &str,
    live: Option<bool>,
    reason: Option<&str>,
) -> (String, String, SdkValue, Option<String>) {
    (
        name.to_owned(),
        "running".to_owned(),
        live.map_or(SdkValue::Null, SdkValue::Bool),
        reason.map(str::to_owned),
    )
}

/// Machine `machine` on `platform` in the home, with its `worker` record
/// when it runs.
fn machine_record(home: &Path, machine: &str, platform: &str, worker: Option<(SocketAddr, &str)>) {
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
}

/// sqlite Store `shop` on `machine` and its project `api`, as the home
/// records them.
fn store_record(home: &Path, machine: &str) {
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

/// Store `shop`'s file and write-ahead log under `root` (a home or a
/// worker's root), which nobody can open: `(bytes, allocated)` of the two.
fn store_files(root: &Path) -> (i64, i64) {
    let file = paths::store_file(root, "shop");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let wal = file.with_file_name("store.sqlite-wal");
    std::fs::write(&file, vec![1u8; 65_536]).unwrap();
    std::fs::write(&wal, vec![2u8; 12_288]).unwrap();
    let mut allocated = 0;
    for path in [&file, &wal] {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000)).unwrap();
        allocated += std::fs::metadata(path).unwrap().blocks() as i64 * 512;
    }
    (65_536 + 12_288, allocated)
}

/// A process that has ended, as a record of it names it.
fn ended() -> LocalProcess {
    let mut child = std::process::Command::new("/bin/sleep")
        .arg("30")
        .spawn()
        .expect("/bin/sleep starts");
    let seen = LocalProcess::observe(child.id()).unwrap();
    child.kill().unwrap();
    child.wait().unwrap();
    seen
}

fn me() -> LocalProcess {
    LocalProcess::observe(std::process::id()).unwrap()
}

/// Sandbox `name` in `root` (the home, or a worker's root) as the side
/// that runs it records it: its daemon is `process`.
fn local_sandbox(root: &Path, name: &str, machine: &str, process: LocalProcess) {
    let folder = format!("layerfs-0a1b2c3d-{name}");
    let daemon = DaemonRecord::local(Local {
        platform: "macos".to_owned(),
        name: folder.clone(),
        container: "ab".repeat(32),
        runtime: "native",
        uid: home::flock::euid(),
        gid: home::flock::egid(),
        directory: paths::run_root(root).join(folder).display().to_string(),
        process: (
            process.pid,
            process.start,
            format!("{:032x}", process.boot.unwrap()),
        ),
        port: 1,
        image: "local".to_owned(),
        labels: Vec::new(),
    });
    write_record(
        &paths::sandbox_record(root, name),
        &map(&[
            ("name", t(name)),
            ("project", t("shop/api")),
            ("machine", t(machine)),
            ("id", t(name)),
            ("runtime", t("native")),
            ("daemon", daemon.to_record()),
        ]),
    )
    .unwrap();
}

/// Sandbox `name` on machine `machine` as the home records it: reached
/// through that machine's worker, or (`forwarded`) with its daemon's port
/// carried to this host, the home keeping what reaches it.
fn worker_sandbox(home: &Path, name: &str, machine: &str, forwarded: bool) {
    let mut fields = vec![
        ("name", t(name)),
        ("project", t("shop/api")),
        ("machine", t(machine)),
        ("id", t(name)),
    ];
    if forwarded {
        let forward = map(&[("address", t("127.0.0.1:1")), ("machine", t(machine))]);
        let daemon = map(&[("name", t("layerfs-0a1b2c3d-s")), ("forward", forward)]);
        fields.extend([("runtime", t("namespaces")), ("daemon", daemon)]);
    }
    write_record(&paths::sandbox_record(home, name), &map(&fields)).unwrap();
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

/// A home with machine `box` (a docker machine) whose worker serves the
/// root beside it, and Store `shop` on it: `(scratch, home, root)`.
fn boxed(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
    let scratch = tempdir(tag);
    let (home, root) = (scratch.join("home"), scratch.join("data"));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    home::open(&home).unwrap();
    machine_record(&home, "box", "docker", Some((worker(&root), "k")));
    store_record(&home, "box");
    (scratch, home, root)
}

/// Every file under `root` with its inode, modification time and size: each
/// record, key file, lock and lease, and the call log (`trace/`) unless
/// `logged` (a call that writes its line there, as every call but a
/// monitoring one does).
fn files(root: &Path, logged: bool) -> BTreeMap<PathBuf, (u64, i64, i64, u64)> {
    let mut found = BTreeMap::new();
    let mut folders = vec![root.to_owned()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            let meta = std::fs::symlink_metadata(&path).unwrap();
            if logged && path == root.join("trace") {
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

/// One public call on `home` before the calls under test: the first call
/// a process makes on a home writes its built-in machine records (once,
/// #535 R6), and it writes a call-log line.
fn settle(home: &Path) {
    layerfs_api::call(Some(home), "sandbox.list", SdkValue::Null, &mut |_| {}).unwrap();
    assert!(paths::calls_file(home).is_file(), "the call log is on");
}

/// A Store being pushed, as far as another call can see it: its record
/// carries a running `store.push` intent and its lock is held.
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

/// Machine `machine` held by a running `machine.stop`, as an ordinary call
/// on it finds it: `Busy`.
fn held(home: &Path, machine: &str) {
    let op = layerfs_home::op::Op::begin("machine.stop", "stop", SdkValue::Null).unwrap();
    let path = paths::machine_record(home, machine);
    let record = read_record(&path).unwrap().unwrap();
    let record = home::with_field(record, layerfs_home::op::FIELD, op.to_record().clone());
    write_record(&path, &record).unwrap();
    let busy = layerfs_store::place::on_machine(home, machine)
        .err()
        .unwrap();
    assert_eq!(busy.code, "Busy", "{busy:?}");
}

/// `root` holds no lease of `shop`, and this process has not opened it.
fn no_lease(root: &Path) {
    assert!(
        !paths::lease_dir(root, "shop").exists(),
        "a lease folder was made in {}",
        root.display()
    );
    assert!(!lease::held_here(root, "shop"));
    let file = paths::store_file(root, "shop");
    assert!(!layerfs_store::open::close(&file).unwrap(), "opened");
    for sidecar in ["store.sqlite-shm", "store.sqlite-journal"] {
        assert!(!file.with_file_name(sidecar).exists(), "{sidecar}");
    }
}

fn keys(value: &SdkValue) -> Vec<&str> {
    match value {
        SdkValue::Map(fields) => fields.iter().map(|(name, _)| name.as_str()).collect(),
        other => panic!("{other:?}"),
    }
}

const PARTS: [&str; 9] = [
    "machine",
    "platform",
    "observed_at",
    "worker",
    "resources",
    "data_disk",
    "stores",
    "sandboxes",
    "complete",
];
const RESOURCES: [&str; 9] = [
    "cpus",
    "memory_total",
    "memory_used",
    "disk_total",
    "disk_used",
    "disk_free",
    "cpu_busy_usec",
    "cpu_total_usec",
    "scope",
];

/// `resources`' fields on this system: macOS reads no processor counters
/// (layerfs-project#182 L1), so `cpu_*` are null and `reason` says so;
/// Linux reads `/proc/stat`, busy within all of it.
fn resources_read(resources: &SdkValue) {
    let mut want = RESOURCES.to_vec();
    if cfg!(target_os = "linux") {
        let busy = resources.get_int("cpu_busy_usec").unwrap();
        assert!(busy > 0 && busy <= resources.get_int("cpu_total_usec").unwrap());
        assert!(resources.get("reason").is_none(), "{resources:?}");
    } else {
        assert_eq!(resources.get("cpu_busy_usec"), Some(&SdkValue::Null));
        assert_eq!(resources.get("cpu_total_usec"), Some(&SdkValue::Null));
        assert_eq!(resources.get_text("reason"), Some("cpu_unsupported"));
        want.push("reason");
    }
    assert_eq!(keys(resources), want, "{resources:?}");
}

/// `sandboxes`' counts, and each row's name and `live` (layerfs-project#182
/// L2): one row per recorded Sandbox, in the home's order.
fn counts(got: &SdkValue) -> String {
    let sandboxes = got.get("sandboxes").unwrap();
    assert_eq!(
        keys(sandboxes),
        ["recorded", "live", "items"],
        "{sandboxes:?}"
    );
    let SdkValue::List(items) = sandboxes.get("items").unwrap() else {
        panic!("{sandboxes:?}")
    };
    assert_eq!(Some(items.len() as i64), sandboxes.get_int("recorded"));
    let rows: Vec<String> = (items.iter())
        .map(|item| {
            let live = item.get("live").map(print).unwrap_or_default();
            format!("{}={live}", item.get_text("name").unwrap())
        })
        .collect();
    format!(
        "{} of {}: {}",
        print(sandboxes.get("live").unwrap()),
        print(sandboxes.get("recorded").unwrap()),
        rows.join(" ")
    )
}

#[test]
fn the_table_lists_machine_stats_and_the_live_option() {
    let call = layerfs_api::find("machine.stats").expect("machine.stats");
    let args: Vec<_> = call.args.iter().map(|a| (a.name, a.optional)).collect();
    assert_eq!(args, [("machine", false)]);
    assert!(call.options.is_empty());
    let list = layerfs_api::find("sandbox.list").unwrap();
    let options: Vec<_> = list.options.iter().map(|a| a.name).collect();
    assert_eq!(options, ["page_size", "page_pos", "live"]);
    // an option that is no flag is refused before any effect
    let home = tempdir("table");
    let (code, message) = call_err(&home, "sandbox.list", &[("live", t("yes"))]);
    assert_eq!(code, "InvalidInput", "{message}");
    std::fs::remove_dir_all(home).unwrap();
}

fn call_err(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> (String, String) {
    call(home, name, args).unwrap_err()
}

#[test]
fn native_stats_are_read_in_this_process_with_no_lock_and_nothing_written() {
    let _turn = serial();
    let home = tempdir("native");
    home::open(&home).unwrap();
    machine_record(&home, "native", "native", None);
    store_record(&home, "native");
    let (bytes, allocated) = store_files(&home);
    local_sandbox(&home, "up", "native", me());
    local_sandbox(&home, "dead", "native", ended());
    settle(&home);
    held(&home, "native");
    let _push = pushing(&home);
    // every file, the call log included
    let before = files(&home, false);
    let asked = home::now_ms();
    let at = Instant::now();
    let got = stats(&home, "native").unwrap();
    // the Store's lock and the machine were held throughout
    assert!(at.elapsed() < Duration::from_secs(10), "{:?}", at.elapsed());
    assert_eq!(keys(&got), PARTS, "{got:?}");
    assert_eq!(got.get_text("machine"), Some("native"));
    assert_eq!(got.get_text("platform"), Some("native"));
    let observed = got.get_int("observed_at").unwrap();
    assert!(observed >= asked && observed <= home::now_ms());
    // no worker: its calls run in this process
    assert_eq!(got.get("worker"), Some(&SdkValue::Null));
    let resources = got.get("resources").unwrap();
    resources_read(resources);
    assert_eq!(resources.get_text("scope"), Some("host"));
    for field in &RESOURCES[..6] {
        assert!(
            resources.get_int(field).unwrap() > 0,
            "{field}: {resources:?}"
        );
    }
    let disk = got.get("data_disk").unwrap();
    assert_eq!(keys(disk), ["path", "fs", "scope", "volume", "inodes_free"]);
    assert_eq!(disk.get_text("path"), home.to_str());
    assert_eq!(disk.get_text("scope"), Some("host_fs"));
    assert_eq!(disk.get("volume"), Some(&SdkValue::Null));
    assert_eq!(
        print(got.get("stores").unwrap()),
        format!(r#"[{{"name":"shop","bytes":{bytes},"allocated":{allocated}}}]"#)
    );
    // the dead daemon is recorded, not live; each has its row, a host
    // process with no cgroup usage
    assert_eq!(counts(&got), "1 of 2: dead=false up=true");
    let SdkValue::List(items) = got.get("sandboxes").unwrap().get("items").unwrap() else {
        unreachable!()
    };
    for item in items {
        assert_eq!(item.get("usage"), Some(&SdkValue::Null), "{item:?}");
        assert_eq!(item.get_text("usage_reason"), Some("host_process"));
    }
    let up = items[1].get_int("started_at").unwrap();
    assert!(
        up <= home::now_ms() && up > home::now_ms() - 86_400_000,
        "{up}"
    );
    assert_eq!(items[0].get("started_at"), Some(&SdkValue::Null));
    assert_eq!(got.get("complete"), Some(&SdkValue::Bool(true)));
    assert!(got.get("reason").is_none());
    // nothing changed in the home: no record, lock, lease or call-log line
    assert_eq!(files(&home, false), before, "machine.stats changed a file");
    no_lease(&home);

    // `sandbox.list` with `live`: the same check, per Sandbox
    let before = files(&home, true);
    assert_eq!(
        live(&home),
        [
            seen("dead", Some(false), None),
            seen("up", Some(true), None)
        ]
    );
    assert_eq!(files(&home, true), before, "a live list changed a file");
    no_lease(&home);

    // without the option the call is what it was: no `live` anywhere, and
    // `live: false` is the same answer
    let plain = call(&home, "sandbox.list", &[]).unwrap();
    assert_eq!(
        print(&plain),
        concat!(
            r#"{"items":[{"name":"dead","project":"shop/api","machine":"native","id":"dead","runtime":"native","state":"running"},"#,
            r#"{"name":"up","project":"shop/api","machine":"native","id":"up","runtime":"native","state":"running"}],"next_page_pos":null}"#
        )
    );
    let off = call(&home, "sandbox.list", &[("live", SdkValue::Bool(false))]).unwrap();
    assert_eq!(print(&off), print(&plain));

    // refusals: a name no machine has, and one that is no name
    let (code, _) = call_err(&home, "machine.stats", &[("machine", t("nobody"))]);
    let unknown = match cfg!(feature = "machine") {
        true => "NotFound",
        false => "Unavailable",
    };
    assert_eq!(code, unknown);
    let (code, _) = call_err(&home, "machine.stats", &[("machine", t("../x"))]);
    assert_eq!(code, "InvalidInput");
    assert_eq!(files(&home, false), files(&home, false));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_worker_answers_its_machine_s_stats_and_who_it_is_and_writes_nothing() {
    let _turn = serial();
    let started = home::now_ms();
    let (scratch, home, root) = boxed("worker");
    let (bytes, allocated) = store_files(&root);
    worker_sandbox(&home, "s1", "box", false);
    worker_sandbox(&home, "s2", "box", false);
    worker_sandbox(&home, "s3", "box", false);
    local_sandbox(&root, "s1", "native", me());
    local_sandbox(&root, "s2", "native", ended());
    // `s3`: the worker holds no record of it
    settle(&home);
    held(&home, "box");
    let _push = pushing(&home);
    let (before, guest) = (files(&home, false), files(&root, false));

    let got = stats(&home, "box").unwrap();
    assert_eq!(keys(&got), PARTS, "{got:?}");
    assert_eq!(got.get_text("platform"), Some("docker"));
    // the worker, in this test's process: taken once when it began to serve
    let worker = got.get("worker").unwrap();
    assert_eq!(
        keys(worker),
        ["version", "pid", "started_at", "protocol", "bundle"]
    );
    assert_eq!(worker.get_text("version"), Some(env!("CARGO_PKG_VERSION")));
    assert_eq!(worker.get_int("pid"), Some(i64::from(std::process::id())));
    let since = worker.get_int("started_at").unwrap();
    assert!(since >= started && since <= home::now_ms(), "{since}");
    assert_eq!(worker.get_int("protocol"), Some(3));
    assert_eq!(worker.get("bundle"), Some(&SdkValue::Null));
    let again = stats(&home, "box").unwrap();
    assert_eq!(
        again.get("worker").unwrap().get_int("started_at"),
        Some(since)
    );
    // whose numbers they are: the Engine host's, and what holds the data
    let resources = got.get("resources").unwrap();
    resources_read(resources);
    assert_eq!(resources.get_text("scope"), Some("engine_host"));
    assert!(resources.get_int("memory_total").unwrap() > 0);
    let disk = got.get("data_disk").unwrap();
    assert_eq!(disk.get_text("path"), root.to_str());
    let holds = match cfg!(target_os = "linux") {
        true => "host_fs",
        false => "vm_disk",
    };
    assert_eq!(disk.get_text("scope"), Some(holds));
    // the provider's name of the data volume, from the record alone
    match cfg!(feature = "machine") {
        true => assert!(
            disk.get_text("volume")
                .is_some_and(|v| v.ends_with("-data")),
            "{disk:?}"
        ),
        false => assert_eq!(disk.get("volume"), Some(&SdkValue::Null)),
    }
    assert_eq!(
        print(got.get("stores").unwrap()),
        format!(r#"[{{"name":"shop","bytes":{bytes},"allocated":{allocated}}}]"#)
    );
    // the worker reads each row from its own records: `s3` it has none of
    assert_eq!(counts(&got), "1 of 3: s1=true s2=false s3=false");
    assert_eq!(got.get("complete"), Some(&SdkValue::Bool(true)));

    // neither side changed: no record, lock, lease, log or call-log line
    assert_eq!(
        files(&home, false),
        before,
        "machine.stats changed the home"
    );
    assert_eq!(files(&root, false), guest, "machine.stats changed the root");
    no_lease(&home);
    no_lease(&root);
    assert!(!paths::store_record(&root, "shop").exists());

    // per Sandbox, where its process is: live, dead, and one the machine
    // has no record of
    let before = files(&home, true);
    assert_eq!(
        live(&home),
        [
            seen("s1", Some(true), None),
            seen("s2", Some(false), None),
            seen("s3", Some(false), None)
        ]
    );
    assert_eq!(files(&home, true), before);
    assert_eq!(files(&root, false), guest);
    std::fs::remove_dir_all(scratch).unwrap();
}

#[test]
fn a_stopped_machine_is_unavailable_and_its_sandboxes_are_not_known() {
    let _turn = serial();
    // a machine that records no worker
    let home = tempdir("stopped");
    home::open(&home).unwrap();
    machine_record(&home, "box", "docker", None);
    store_record(&home, "box");
    worker_sandbox(&home, "s1", "box", false);
    worker_sandbox(&home, "s2", "box", true);
    settle(&home);
    let before = files(&home, false);
    let (code, message) = stats(&home, "box").unwrap_err();
    assert_eq!(code, "Unavailable", "{message}");
    assert!(message.contains("layerfs machine start box"), "{message}");
    assert_eq!(files(&home, false), before);
    // `state` is the record's; `live` is not known, and says why
    let before = files(&home, true);
    assert_eq!(
        live(&home),
        [
            seen("s1", None, Some("machine_stopped")),
            seen("s2", None, Some("machine_stopped"))
        ]
    );
    assert_eq!(files(&home, true), before);
    std::fs::remove_dir_all(home).unwrap();

    // a machine whose recorded worker is gone: refused at the connection
    let home = tempdir("gone");
    home::open(&home).unwrap();
    let nowhere = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    machine_record(&home, "box", "docker", Some((nowhere, "k")));
    store_record(&home, "box");
    worker_sandbox(&home, "s1", "box", false);
    settle(&home);
    let before = files(&home, false);
    let at = Instant::now();
    let (code, message) = stats(&home, "box").unwrap_err();
    assert_eq!(code, "Unavailable", "{message}");
    assert!(at.elapsed() < Duration::from_secs(3), "{:?}", at.elapsed());
    assert_eq!(files(&home, false), before);
    assert_eq!(live(&home), [seen("s1", None, Some("unavailable"))]);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_forwarded_sandbox_s_process_is_checked_in_its_machine() {
    let _turn = serial();
    let (scratch, home, root) = boxed("forwarded");
    // the home reaches the daemon directly: its record holds the forward
    // and no process; the daemon record with the process is the worker's
    worker_sandbox(&home, "s1", "box", true);
    local_sandbox(&root, "s1", "native", me());
    settle(&home);
    let (before, guest) = (files(&home, true), files(&root, false));
    assert_eq!(live(&home), [seen("s1", Some(true), None)]);
    assert_eq!(counts(&stats(&home, "box").unwrap()), "1 of 1: s1=true");
    assert_eq!(files(&home, true), before);
    assert_eq!(files(&root, false), guest);
    // its daemon ended in the machine
    local_sandbox(&root, "s1", "native", ended());
    assert_eq!(live(&home), [seen("s1", Some(false), None)]);
    assert_eq!(counts(&stats(&home, "box").unwrap()), "0 of 1: s1=false");
    std::fs::remove_dir_all(scratch).unwrap();
}

#[test]
fn a_page_over_several_machines_asks_each_once_and_keeps_its_order() {
    let _turn = serial();
    let (scratch, home, root) = boxed("several");
    machine_record(&home, "native", "native", None);
    machine_record(&home, "off", "docker", None);
    // in the home's order: a, b, c, d, e over three machines
    local_sandbox(&home, "a", "native", me());
    worker_sandbox(&home, "b", "box", false);
    worker_sandbox(&home, "c", "off", false);
    local_sandbox(&home, "d", "native", ended());
    worker_sandbox(&home, "e", "box", false);
    local_sandbox(&root, "b", "native", ended());
    local_sandbox(&root, "e", "native", me());
    settle(&home);
    let at = Instant::now();
    assert_eq!(
        live(&home),
        [
            seen("a", Some(true), None),
            seen("b", Some(false), None),
            seen("c", None, Some("machine_stopped")),
            seen("d", Some(false), None),
            seen("e", Some(true), None)
        ]
    );
    assert!(at.elapsed() < Duration::from_secs(4), "{:?}", at.elapsed());
    // one page of it: only that page's machines are asked
    let page = [
        ("live", SdkValue::Bool(true)),
        ("page_size", SdkValue::Int(2)),
    ];
    let first = call(&home, "sandbox.list", &page).unwrap();
    let Some(SdkValue::List(items)) = first.get("items") else {
        panic!("{first:?}")
    };
    let shown: Vec<_> = items
        .iter()
        .map(|item| (item.get_text("name").unwrap(), item.get("live").cloned()))
        .collect();
    assert_eq!(
        shown,
        [
            ("a", Some(SdkValue::Bool(true))),
            ("b", Some(SdkValue::Bool(false)))
        ]
    );
    assert_eq!(first.get_int("next_page_pos"), Some(2));
    std::fs::remove_dir_all(scratch).unwrap();
}
