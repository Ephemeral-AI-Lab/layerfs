//! `machine.stats` (layerfs-project#164; `sandbox.list`'s `live` option is
//! gone in the call surface 2.0, where a list answers from records)
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
use {layerfs_api::home::{self, paths, read_record, write_record}, layerfs_store::lease};
use layerfs_protocol::provision::install::SdkValue;
use layerfs_os::process::LocalProcess;
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
    layerfs_home::json::print(value)
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

/// `machine.stats`' long form.
fn stats(home: &Path, machine: &str) -> Result<SdkValue, (String, String)> {
    let detail = ("detail", SdkValue::Bool(true));
    call(home, "machine.stats", &[("machine", t(machine)), detail])
}

/// Machine `machine` on `platform` in the home, with its `worker` record
/// when it runs.
fn machine_record(home: &Path, machine: &str, platform: &str, worker: Option<(SocketAddr, &str)>) {
    let daemon = std::env::current_exe().unwrap().display().to_string();
    let mut fields = vec![
        ("name", t(machine)),
        ("platform", t(platform)),
        ("daemon", t(&daemon)),
    ];
    // every 2.0 record of a machine with a provider names its VM
    let vm_name = format!("layerfs-test-{machine}");
    if platform != "host" {
        fields.push(("vm_name", t(&vm_name)));
    }
    write_record(&paths::machine_record(home, machine), &map(&fields)).unwrap();
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
            ("profile", t("normal")),
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
        runtime: "process",
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
        network: None,
    });
    write_record(
        &paths::sandbox_record(root, name),
        &map(&[
            ("name", t(name)),
            ("project", t("shop/api")),
            ("machine", t(machine)),
            ("id", t(name)),
            ("runtime", t("process")),
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
/// carries a running `store.move` intent and its lock is held.
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

const PARTS: [&str; 18] = [
    "cpus",
    "memory_used_bytes",
    "memory_total_bytes",
    "disk_free_bytes",
    "disk_total_bytes",
    "incomplete",
    "disk_used_bytes",
    "reasons",
    "machine",
    "platform",
    "observed_at",
    "scope",
    "cpu_busy_ms",
    "cpu_total_ms",
    "worker",
    "data_disk",
    "stores",
    "sandboxes",
];
const RESOURCES_SHORT: [&str; 5] = [
    "cpus",
    "memory_used_bytes",
    "memory_total_bytes",
    "disk_free_bytes",
    "disk_total_bytes",
];
const RESOURCES: [&str; 6] = [
    "cpus",
    "memory_total_bytes",
    "memory_used_bytes",
    "disk_total_bytes",
    "disk_used_bytes",
    "disk_free_bytes",
];

/// The processor counters on this system: macOS reads none
/// (layerfs-project#182 L1), so `cpu_*` are null and `reasons` says so;
/// Linux reads `/proc/stat`, busy within all of it, in milliseconds.
fn resources_read(got: &SdkValue) {
    if cfg!(target_os = "linux") {
        let busy = got.get_int("cpu_busy_ms").unwrap();
        assert!(busy > 0 && busy <= got.get_int("cpu_total_ms").unwrap());
        assert_eq!(got.get("reasons"), Some(&SdkValue::List(Vec::new())));
    } else {
        assert_eq!(got.get("cpu_busy_ms"), Some(&SdkValue::Null));
        assert_eq!(got.get("cpu_total_ms"), Some(&SdkValue::Null));
        assert_eq!(print(got.get("reasons").unwrap()), r#"["cpu_unsupported"]"#);
    }
}

/// How many of the Sandbox rows are live, of how many, and each row's
/// name and `live` (layerfs-project#182 L2): one row per recorded Sandbox,
/// in the home's order.
fn counts(got: &SdkValue) -> String {
    let SdkValue::List(items) = got.get("sandboxes").unwrap() else {
        panic!("{got:?}")
    };
    let live = (items.iter())
        .filter(|item| item.get("live") == Some(&SdkValue::Bool(true)))
        .count();
    let rows: Vec<String> = (items.iter())
        .map(|item| {
            let live = item.get("live").map(print).unwrap_or_default();
            format!("{}={live}", item.get_text("sandbox").unwrap())
        })
        .collect();
    format!("{live} of {}: {}", items.len(), rows.join(" "))
}

/// A time as answers spell it: `YYYY-MM-DDTHH:MM:SSZ`.
fn is_time(value: Option<&str>) -> bool {
    value.is_some_and(|at| at.len() == 20 && at.ends_with('Z') && at.as_bytes()[10] == b'T')
}

#[test]
fn the_table_lists_machine_stats_and_no_live_list() {
    let call = layerfs_api::find("machine.stats").expect("machine.stats");
    let args: Vec<_> = call.args.iter().map(|a| (a.name, a.optional)).collect();
    assert_eq!(args, [("machine", false)]);
    assert!(call.options.is_empty());
    // `sandbox.list` answers from records alone (call surface 2.0): its
    // `live` option is gone, and refused before any effect
    let home = tempdir("table");
    let (code, message) = call_err(&home, "sandbox.list", &[("live", SdkValue::Bool(true))]);
    assert_eq!(code, "InvalidInput", "{message}");
    std::fs::remove_dir_all(home).unwrap();
}

fn call_err(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> (String, String) {
    call(home, name, args).unwrap_err()
}

#[test]
fn native_stats_are_read_in_this_process_with_no_lock_and_nothing_written() {
    let _turn = serial();
    let home = tempdir(layerfs_home::names::HOST_MACHINE);
    home::open(&home).unwrap();
    machine_record(&home, layerfs_home::names::HOST_MACHINE, "host", None);
    store_record(&home, layerfs_home::names::HOST_MACHINE);
    let (bytes, allocated) = store_files(&home);
    local_sandbox(&home, "up", layerfs_home::names::HOST_MACHINE, me());
    local_sandbox(&home, "dead", layerfs_home::names::HOST_MACHINE, ended());
    settle(&home);
    held(&home, layerfs_home::names::HOST_MACHINE);
    let _push = pushing(&home);
    // every file, the call log included
    let before = files(&home, false);
    let at = Instant::now();
    let got = stats(&home, layerfs_home::names::HOST_MACHINE).unwrap();
    // the Store's lock and the machine were held throughout
    assert!(at.elapsed() < Duration::from_secs(10), "{:?}", at.elapsed());
    assert_eq!(keys(&got), PARTS, "{got:?}");
    assert_eq!(
        got.get_text("machine"),
        Some(layerfs_home::names::HOST_MACHINE)
    );
    assert_eq!(got.get_text("platform"), Some("host"));
    assert!(is_time(got.get_text("observed_at")), "{got:?}");
    // no worker: its calls run in this process
    assert_eq!(got.get("worker"), Some(&SdkValue::Null));
    resources_read(&got);
    assert_eq!(got.get_text("scope"), Some("host"));
    for field in RESOURCES {
        assert!(got.get_int(field).unwrap() > 0, "{field}: {got:?}");
    }
    let disk = got.get("data_disk").unwrap();
    assert_eq!(keys(disk), ["path", "fs", "scope", "volume", "inodes_free"]);
    assert_eq!(disk.get_text("path"), home.to_str());
    assert_eq!(disk.get_text("scope"), Some("host_fs"));
    assert_eq!(disk.get("volume"), Some(&SdkValue::Null));
    assert_eq!(
        print(got.get("stores").unwrap()),
        format!(r#"[{{"store":"shop","size":{bytes},"allocated_bytes":{allocated}}}]"#)
    );
    // the dead daemon is recorded, not live; each has its row, a host
    // process with no cgroup usage
    assert_eq!(counts(&got), "1 of 2: dead=false up=true");
    let SdkValue::List(items) = got.get("sandboxes").unwrap() else {
        unreachable!()
    };
    for item in items {
        assert_eq!(item.get("usage"), Some(&SdkValue::Null), "{item:?}");
        assert_eq!(item.get_text("usage_reason"), Some("host_process"));
    }
    assert!(is_time(items[1].get_text("started_at")), "{items:?}");
    assert_eq!(items[0].get("started_at"), Some(&SdkValue::Null));
    assert_eq!(got.get("incomplete"), Some(&SdkValue::Bool(false)));
    // the short form: what the machine has and uses, nothing else
    let short = call(
        &home,
        "machine.stats",
        &[("machine", t(layerfs_home::names::HOST_MACHINE))],
    )
    .unwrap();
    assert_eq!(keys(&short), RESOURCES_SHORT, "{short:?}");
    // nothing changed in the home: no record, lock, lease or call-log line
    assert_eq!(files(&home, false), before, "machine.stats changed a file");
    no_lease(&home);

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
    local_sandbox(&root, "s1", layerfs_home::names::HOST_MACHINE, me());
    local_sandbox(&root, "s2", layerfs_home::names::HOST_MACHINE, ended());
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
    assert_eq!(got.get_text("machine"), Some("box"));
    assert_eq!(worker.get_int("protocol"), Some(3));
    assert_eq!(worker.get("bundle"), Some(&SdkValue::Null));
    let again = stats(&home, "box").unwrap();
    assert_eq!(
        again.get("worker").unwrap().get_int("started_at"),
        Some(since)
    );
    // whose numbers they are: the Engine host's, and what holds the data
    resources_read(&got);
    assert_eq!(got.get_text("scope"), Some("engine_host"));
    assert!(got.get_int("memory_total_bytes").unwrap() > 0);
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
        format!(r#"[{{"store":"shop","size":{bytes},"allocated_bytes":{allocated}}}]"#)
    );
    // the worker reads each row from its own records: `s3` it has none of
    assert_eq!(counts(&got), "1 of 3: s1=true s2=false s3=false");
    assert_eq!(got.get("incomplete"), Some(&SdkValue::Bool(false)));

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
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_forwarded_sandbox_s_process_is_checked_in_its_machine() {
    let _turn = serial();
    let (scratch, home, root) = boxed("forwarded");
    // the home reaches the daemon directly: its record holds the forward
    // and no process; the daemon record with the process is the worker's
    worker_sandbox(&home, "s1", "box", true);
    local_sandbox(&root, "s1", layerfs_home::names::HOST_MACHINE, me());
    settle(&home);
    let (before, guest) = (files(&home, true), files(&root, false));
    assert_eq!(counts(&stats(&home, "box").unwrap()), "1 of 1: s1=true");
    assert_eq!(files(&home, true), before);
    assert_eq!(files(&root, false), guest);
    // its daemon ended in the machine
    local_sandbox(&root, "s1", layerfs_home::names::HOST_MACHINE, ended());
    assert_eq!(counts(&stats(&home, "box").unwrap()), "0 of 1: s1=false");
    std::fs::remove_dir_all(scratch).unwrap();
}

/// `machine stats --with space` (`machine.space` before the call surface
/// 2.0) on a worker's machine: one more request to its worker, which walks
/// its own root.
/// Neither side changes, no lease is taken, it is answered while a
/// `machine.stop` holds the machine and a push holds the Store, and it
/// writes no call-log line.
#[test]
fn a_worker_answers_its_machine_s_space_and_writes_nothing() {
    let _turn = serial();
    let (scratch, home, root) = boxed("space");
    store_files(&root);
    worker_sandbox(&home, "s1", "box", false);
    worker_sandbox(&home, "s2", "box", false);
    local_sandbox(&root, "s1", layerfs_home::names::HOST_MACHINE, me());
    let run = paths::run_root(&root).join("layerfs-0a1b2c3d-s1");
    std::fs::create_dir_all(run.join("overlay").join("upper")).unwrap();
    std::fs::write(run.join("overlay").join("1.sqlite"), vec![3u8; 65_536]).unwrap();
    std::fs::write(
        run.join("overlay").join("upper").join("f"),
        vec![4u8; 32_768],
    )
    .unwrap();
    std::fs::create_dir_all(run.join("workspaces").join("1")).unwrap();
    std::fs::write(
        run.join("workspaces").join("1").join("never"),
        vec![5u8; 1 << 20],
    )
    .unwrap();
    settle(&home);
    held(&home, "box");
    let _push = pushing(&home);
    let (before, guest) = (files(&home, false), files(&root, false));

    let asked = [
        ("machine", t("box")),
        ("detail", SdkValue::Bool(true)),
        ("with", SdkValue::List(vec![t("space")])),
    ];
    let both = call(&home, "machine.stats", &asked).unwrap();
    assert_eq!(both.get_text("platform"), Some("docker"));
    let got = both.get("space").unwrap();
    assert_eq!(
        keys(got),
        [
            "observed_at",
            "layerfs_bytes",
            "outside_bytes",
            "parts",
            "stores",
            "sandboxes",
            "unreadable_count",
            "incomplete",
            "reasons"
        ],
        "{got:?}"
    );
    assert!(is_time(got.get_text("observed_at")), "{got:?}");
    let SdkValue::List(parts) = got.get("parts").unwrap() else {
        panic!("{got:?}")
    };
    let named: Vec<_> = parts.iter().map(|p| p.get_text("part").unwrap()).collect();
    assert_eq!(
        named,
        [
            "stores",
            "sandboxes",
            "images",
            "other_runs",
            "logs",
            "other"
        ]
    );
    let SdkValue::List(stores) = got.get("stores").unwrap() else {
        panic!("{got:?}")
    };
    assert_eq!(stores.len(), 1);
    assert_eq!(stores[0].get_text("store"), Some("shop"));
    assert!(stores[0].get_int("allocated_bytes").unwrap() >= 65_536 + 12_288);
    let SdkValue::List(rows) = got.get("sandboxes").unwrap() else {
        panic!("{got:?}")
    };
    // `s1` read from the worker's record, `s2` it has none of
    assert_eq!(rows[0].get_text("sandbox"), Some("s1"));
    assert!(
        rows[0].get_int("overlays_bytes").unwrap() >= 65_536,
        "{rows:?}"
    );
    assert!(
        rows[0].get_int("writes_bytes").unwrap() >= 32_768,
        "{rows:?}"
    );
    assert!(
        rows[0].get_int("allocated_bytes").unwrap() < 1 << 20,
        "{rows:?}"
    );
    assert_eq!(rows[1].get_text("reason"), Some("no_run_folder"));
    let layerfs = got.get_int("layerfs_bytes").unwrap();
    let total: i64 = parts.iter().map(|p| p.get_int("bytes").unwrap()).sum();
    assert_eq!(layerfs, total);
    let used = both.get_int("disk_used_bytes").unwrap();
    let outside = got.get_int("outside_bytes").unwrap();
    // the stats' read and the walk's are two reads of a disk in use
    assert!(outside <= used, "{outside} of {used}");
    assert_eq!(got.get("incomplete"), Some(&SdkValue::Bool(false)));

    assert_eq!(
        files(&home, false),
        before,
        "machine stats --with space changed the home"
    );
    let changed = "machine stats --with space changed the root";
    assert_eq!(files(&root, false), guest, "{changed}");
    no_lease(&home);
    no_lease(&root);

    // refusals: a name no machine has, and one that is no name
    let bad = [
        ("machine", t("../x")),
        ("with", SdkValue::List(vec![t("space")])),
    ];
    let (code, _) = call(&home, "machine.stats", &bad).unwrap_err();
    assert_eq!(code, "InvalidInput");
    std::fs::remove_dir_all(scratch).unwrap();
}
