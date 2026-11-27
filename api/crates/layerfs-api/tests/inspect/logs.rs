//! `machine.log` (`logs.machine` before the call surface 2.0) and
//! `sandbox.log` (`logs.sandbox` before it; layerfs-project#178 step 1) on the
//! two routes of an inspect request: this process for a `native` Sandbox,
//! and a machine's worker (this build's own server on a scratch root) for
//! every other, a Sandbox whose daemon the home reaches directly included.
//! Their answers and refusals, and what #179 B asks of every monitoring
//! read: no lock or lease, nothing written on either side, no call-log
//! line, nothing started, and no refusal while a machine operation holds
//! the machine.
//!
//! Nothing runs: the logs are files this test writes where a worker and a
//! daemon write theirs. Inspect permits are the process's, and the worker
//! is in this process, so every test here takes one lock.
#![cfg(unix)]
use layerfs_api::home::{self, lease, paths, read_record, write_record};
use layerfs_protocol::sdk::SdkValue;
use layerfs_sandbox::record::{DaemonRecord, Local, WorkspaceRecord};
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

fn tempdir(tag: &str) -> PathBuf {
    let sequence = HOMES.fetch_add(1, Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("lfs-logsi-{tag}-{}-{sequence}", std::process::id()));
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

/// The lines of a log read: its `items`.
fn texts(answer: &SdkValue) -> Vec<String> {
    let Some(SdkValue::List(lines)) = answer.get("items") else {
        panic!("{answer:?}")
    };
    lines
        .iter()
        .map(|line| line.get_text("text").unwrap().to_owned())
        .collect()
}

fn numbered(from: usize, to: usize) -> String {
    let mut text = String::new();
    for n in from..to {
        text.push_str("line ");
        text.push_str(&n.to_string());
        text.push('\n');
    }
    text
}

fn names(from: usize, to: usize) -> Vec<String> {
    (from..to).map(|n| format!("line {n}")).collect()
}

/// The home's records: machine `machine` on `platform`, its `worker`
/// record when it runs, sqlite Store `shop` on it and project `api`.
fn home_records(home: &Path, machine: &str, platform: &str, worker: Option<(SocketAddr, &str)>) {
    let daemon = std::env::current_exe().unwrap().display().to_string();
    // every 2.0 machine but `native` records its provider name (`vm_name`)
    let vm_name = format!("layerfs-00000000-{machine}");
    let mut fields = vec![
        ("name", t(machine)),
        ("platform", t(platform)),
        ("daemon", t(&daemon)),
    ];
    if machine != layerfs_home::names::HOST_MACHINE {
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

/// Sandbox `s1` in `root` (the home, or a worker's root) as the side that
/// runs it records it, with its run folder made and `log` as its
/// `daemon.log`; answers the folder.
fn local_sandbox(root: &Path, log: &str) -> PathBuf {
    let name = "layerfs-0a1b2c3d-s1";
    let folder = paths::run_root(root).join(name);
    std::fs::create_dir_all(folder.join("overlay")).unwrap();
    std::fs::write(folder.join("daemon.log"), log).unwrap();
    let me = layerfs_os::process::LocalProcess::observe(std::process::id()).unwrap();
    let daemon = DaemonRecord::local(Local {
        platform: "macos".to_owned(),
        name: name.to_owned(),
        container: "ab".repeat(32),
        runtime: "process",
        uid: home::flock::euid(),
        gid: home::flock::egid(),
        directory: folder.display().to_string(),
        process: (me.pid, me.start, format!("{:032x}", me.boot.unwrap())),
        port: 1,
        image: "local".to_owned(),
        labels: Vec::new(),
        network: None,
    });
    write_record(
        &paths::sandbox_record(root, "s1"),
        &map(&[
            ("name", t("s1")),
            ("project", t("shop/api")),
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
            ("id", t("s1")),
            ("runtime", t("process")),
            ("daemon", daemon.to_record()),
        ]),
    )
    .unwrap();
    folder
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
/// root beside it: `(scratch, home, root)`.
fn boxed(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
    let scratch = tempdir(tag);
    let (home, root) = (scratch.join("home"), scratch.join("data"));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    home::open(&home).unwrap();
    home_records(&home, "box", "docker", Some((worker(&root), "k")));
    (scratch, home, root)
}

/// Every file under `root` with its inode, modification time and size: each
/// record, log, key file, lock and lease, the call log (`trace/`) included.
fn files(root: &Path) -> BTreeMap<PathBuf, (u64, i64, i64, u64)> {
    let mut found = BTreeMap::new();
    let mut folders = vec![root.to_owned()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            let meta = std::fs::symlink_metadata(&path).unwrap();
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
/// #535 R6), and it writes a call-log line, which a `logs` call does not.
fn settle(home: &Path) {
    layerfs_api::call(Some(home), "sandbox.list", SdkValue::Null, &mut |_| {}).unwrap();
    assert!(paths::calls_file(home).is_file(), "the call log is on");
}

/// A Store being pushed, as far as another call can see it: its record
/// carries a running `store.move` intent and its lock is held. The lock is
/// released when the answer drops.
fn pushing(home: &Path) -> impl Drop {
    let op =
        layerfs_home::record::intent::Op::begin("store.move", "check", SdkValue::Null).unwrap();
    let record = layerfs_store::admin::record::store::StoreRecord::read(home, "shop")
        .unwrap()
        .unwrap();
    record
        .with(layerfs_home::record::intent::FIELD, op.to_record().clone())
        .write(home)
        .unwrap();
    layerfs_store::admin::record::store::lock(home, "shop").unwrap()
}

/// Machine `machine` held by a running `machine.stop`, as an ordinary call
/// on it finds it: `Busy`.
fn held(home: &Path, machine: &str) {
    let op =
        layerfs_home::record::intent::Op::begin("machine.stop", "stop", SdkValue::Null).unwrap();
    let path = paths::machine_record(home, machine);
    let record = read_record(&path).unwrap().unwrap();
    let record = home::with_field(
        record,
        layerfs_home::record::intent::FIELD,
        op.to_record().clone(),
    );
    write_record(&path, &record).unwrap();
    let busy = layerfs_store::admin::route::executor::on_machine(home, machine)
        .err()
        .unwrap();
    assert_eq!(busy.code, "Busy", "{busy:?}");
}

/// `root` holds no trace of a lease or an open Store of `shop`.
fn no_lease(root: &Path) {
    assert!(
        !paths::lease_dir(root, "shop").exists(),
        "a lease folder was made in {}",
        root.display()
    );
    assert!(!lease::held_here(root, "shop"));
    let file = paths::store_file(root, "shop");
    assert!(!file.exists(), "a Store file was made");
    assert!(!layerfs_store::admin::open::close::close(&file).unwrap());
}

#[test]
fn the_table_lists_both_calls_with_a_cursor_window() {
    for (name, positional, want) in [
        ("machine.log", "machine", ["limit", "cursor"].as_slice()),
        ("sandbox.log", "sandbox", ["limit", "cursor"].as_slice()),
    ] {
        let call = layerfs_api::find(name).unwrap_or_else(|| panic!("{name}"));
        let args: Vec<_> = call.args.iter().map(|a| (a.name, a.optional)).collect();
        assert_eq!(args, [(positional, false)], "{name}");
        let options: Vec<_> = call.options.iter().map(|a| a.name).collect();
        assert_eq!(options, want, "{name}");
        assert!(call.quiet, "{name}");
    }
    for gone in ["logs.machine", "logs.sandbox"] {
        assert!(layerfs_api::find(gone).is_none(), "{gone}: no alias (R1)");
    }
    // discovery: the table front ends build their functions from lists them
    let table = layerfs_home::json::parse(&layerfs_api::table_json()).unwrap();
    let SdkValue::List(calls) = table else {
        panic!()
    };
    let logs: Vec<_> = calls
        .iter()
        .filter(|c| c.get_text("family") == Some("logs"))
        .map(|c| c.get_text("call").unwrap())
        .collect();
    // the `logs` family is gone: the call log's own two calls are
    // `home.log` and `home.clear_log`, the logs `machine.log` and `sandbox.log`
    assert!(logs.is_empty(), "{logs:?}");
}

#[test]
fn a_native_sandbox_s_log_is_read_here_with_no_lock_and_nothing_written() {
    let _turn = serial();
    let home = tempdir(layerfs_home::names::HOST_MACHINE);
    home::open(&home).unwrap();
    home_records(&home, layerfs_home::names::HOST_MACHINE, "host", None);
    let folder = local_sandbox(&home, &numbered(0, 300));
    settle(&home);
    held(&home, layerfs_home::names::HOST_MACHINE);
    let _push = pushing(&home);
    let before = files(&home);
    let at = Instant::now();
    let tail = call(&home, "sandbox.log", &[("sandbox", t("s1"))]).unwrap();
    // the Store's lock was held throughout
    assert!(at.elapsed() < Duration::from_secs(10), "{:?}", at.elapsed());
    assert_eq!(texts(&tail), names(100, 300));
    // older lines, by next_cursor
    let earlier = call(
        &home,
        "sandbox.log",
        &[
            ("sandbox", t("s1")),
            ("cursor", tail.get("next_cursor").unwrap().clone()),
            ("limit", SdkValue::Int(1000)),
        ],
    )
    .unwrap();
    assert_eq!(texts(&earlier), names(0, 100));
    // the start of the log: the short form leaves next_cursor out
    assert!(earlier.get("next_cursor").is_none(), "{earlier:?}");
    // nothing changed in the home: no record, lock, lease or call-log line
    assert_eq!(
        files(&home),
        before,
        "a log read changed a file in the home"
    );
    no_lease(&home);

    // the daemon writes on: the newest page ends with it
    let mut log = std::fs::OpenOptions::new()
        .append(true)
        .open(folder.join("daemon.log"))
        .unwrap();
    std::io::Write::write_all(&mut log, numbered(300, 303).as_bytes()).unwrap();
    let next = call(
        &home,
        "sandbox.log",
        &[("sandbox", t("s1")), ("limit", SdkValue::Int(3))],
    )
    .unwrap();
    assert_eq!(texts(&next), names(300, 303));

    // refusals: the host machine has no machine worker log; unknown names
    let (code, message) = call(
        &home,
        "machine.log",
        &[("machine", t(layerfs_home::names::HOST_MACHINE))],
    )
    .unwrap_err();
    assert_eq!(code, "Unsupported", "{message}");
    // a name no machine has: `NotFound` in a host build; a build without
    // the machine adapters knows only that no worker is recorded for it
    let (code, _) = call(&home, "machine.log", &[("machine", t("nobody"))]).unwrap_err();
    let unknown = match cfg!(feature = "machine") {
        true => "NotFound",
        false => "Unavailable",
    };
    assert_eq!(code, unknown);
    let (code, _) = call(&home, "sandbox.log", &[("sandbox", t("nobody"))]).unwrap_err();
    assert_eq!(code, "NotFound");
    let stale = [("sandbox", t("s1")), ("cursor", t("c1.0.1.2.3"))];
    let (code, message) = call(&home, "sandbox.log", &stale).unwrap_err();
    assert_eq!(code, "InvalidInput", "{message}");
    let wide = [("sandbox", t("s1")), ("limit", SdkValue::Int(1001))];
    let (code, message) = call(&home, "sandbox.log", &wide).unwrap_err();
    assert_eq!(
        (code.as_str(), message.as_str()),
        ("InvalidInput", "sandbox.log: `limit` is 1 to 1000")
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_worker_reads_its_log_and_its_sandboxes_logs_and_writes_nothing() {
    let _turn = serial();
    let (scratch, home, root) = boxed("worker");
    worker_sandbox(&home);
    local_sandbox(&root, "daemon up\nlistening\n");
    std::fs::create_dir_all(paths::log_dir(&root)).unwrap();
    let log = paths::worker_log(&root);
    assert_eq!(log, root.join("log/worker.log"));
    std::fs::write(root.join("log/worker.1.log"), numbered(0, 250)).unwrap();
    std::fs::write(&log, numbered(250, 300)).unwrap();
    // the one log no inspect call reads (#179 D8)
    std::fs::write(root.join("log/uncertain.jsonl"), "{\"secret\":1}\n").unwrap();
    settle(&home);
    held(&home, "box");
    let _push = pushing(&home);
    let (before, guest) = (files(&home), files(&root));

    let tail = call(&home, "machine.log", &[("machine", t("box"))]).unwrap();
    assert_eq!(texts(&tail), names(250, 300));
    // back into the rotated file, by next_cursor
    let back = ("cursor", tail.get("next_cursor").unwrap().clone());
    let earlier = call(&home, "machine.log", &[("machine", t("box")), back]).unwrap();
    assert_eq!(texts(&earlier), names(50, 250));
    let limited = [
        ("machine", t("box")),
        ("cursor", earlier.get("next_cursor").unwrap().clone()),
        ("limit", SdkValue::Int(1000)),
    ];
    let first = call(&home, "machine.log", &limited).unwrap();
    assert_eq!(texts(&first), names(0, 50));
    // the start of the log: the short form leaves next_cursor out
    assert_eq!(first.get("next_cursor"), None, "{first:?}");
    // the Sandbox's daemon log, from the run folder the worker's record names
    let daemon = call(&home, "sandbox.log", &[("sandbox", t("s1"))]).unwrap();
    assert_eq!(texts(&daemon), ["daemon up", "listening"]);

    // neither side changed: no record, lock, lease, log or call-log line
    assert_eq!(
        files(&home),
        before,
        "a log read changed a file in the home"
    );
    assert_eq!(
        files(&root),
        guest,
        "a log read changed a file under the root"
    );
    no_lease(&home);
    no_lease(&root);
    assert!(!paths::store_record(&root, "shop").exists());

    // the worker writes on: the newest page ends with it
    let cursor = ("cursor", tail.get("next_cursor").unwrap().clone());
    let mut file = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
    std::io::Write::write_all(&mut file, b"\x1b[1mbold\x1b[0m\n").unwrap();
    let newest = call(&home, "machine.log", &[("machine", t("box"))]).unwrap();
    let newest = texts(&newest);
    assert_eq!(newest.last().unwrap(), "\\u{1b}[1mbold\\u{1b}[0m");

    // a restart rotates the log: the old cursor is refused, a new read follows
    std::fs::rename(&log, root.join("log/worker.1.log")).unwrap();
    std::fs::write(&log, "started again\n").unwrap();
    let (code, message) = call(&home, "machine.log", &[("machine", t("box")), cursor]).unwrap_err();
    assert_eq!(code, "InvalidInput", "{message}");
    // the worker's refusal names the call itself, as the host's check does
    assert!(
        message.starts_with("machine.log: the cursor is stale"),
        "{message}"
    );
    let wide = [("machine", t("box")), ("limit", SdkValue::Int(1001))];
    let (code, message) = call(&home, "machine.log", &wide).unwrap_err();
    assert_eq!(
        (code.as_str(), message.as_str()),
        ("InvalidInput", "machine.log: `limit` is 1 to 1000")
    );
    let fresh = call(&home, "machine.log", &[("machine", t("box"))]).unwrap();
    assert_eq!(texts(&fresh), ["started again"]);

    // a Sandbox the worker holds no run folder for
    std::fs::remove_dir_all(paths::sandbox_dir(&root, "s1")).unwrap();
    let (code, message) = call(&home, "sandbox.log", &[("sandbox", t("s1"))]).unwrap_err();
    assert_eq!(code, "NotFound", "{message}");
    std::fs::remove_dir_all(scratch).unwrap();
}

#[test]
fn a_machine_started_by_an_earlier_build_answers_an_empty_log() {
    let _turn = serial();
    let (scratch, home, root) = boxed("old");
    // its worker answers, and wrote no `log/worker.log`
    let guest = files(&root);
    let empty = call(&home, "machine.log", &[("machine", t("box"))]).unwrap();
    assert!(texts(&empty).is_empty());
    assert_eq!(empty.get("next_cursor"), None, "{empty:?}");
    assert_eq!(files(&root), guest, "nothing was made under the root");
    assert!(!paths::log_dir(&root).exists());
    // once a later start writes one, a read answers it
    std::fs::create_dir_all(paths::log_dir(&root)).unwrap();
    std::fs::write(paths::worker_log(&root), "first\n").unwrap();
    let next = call(&home, "machine.log", &[("machine", t("box"))]).unwrap();
    assert_eq!(texts(&next), ["first"]);
    std::fs::remove_dir_all(scratch).unwrap();
}

#[test]
fn a_stopped_machine_is_unavailable_and_nothing_starts_it() {
    let _turn = serial();
    // a machine that records no worker
    let home = tempdir("stopped");
    home::open(&home).unwrap();
    home_records(&home, "box", "docker", None);
    worker_sandbox(&home);
    settle(&home);
    let before = files(&home);
    for (name, args) in [
        ("machine.log", [("machine", t("box"))]),
        ("sandbox.log", [("sandbox", t("s1"))]),
    ] {
        let (code, message) = call(&home, name, &args).unwrap_err();
        assert_eq!(code, "Unavailable", "{name}: {message}");
        assert!(message.contains("layerfs machine start box"), "{message}");
    }
    assert_eq!(files(&home), before);
    std::fs::remove_dir_all(home).unwrap();

    // a machine whose recorded worker is gone: refused at the connection
    let home = tempdir("gone");
    home::open(&home).unwrap();
    let nowhere = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    home_records(&home, "box", "docker", Some((nowhere, "k")));
    worker_sandbox(&home);
    settle(&home);
    let before = files(&home);
    let at = Instant::now();
    let (code, message) = call(&home, "machine.log", &[("machine", t("box"))]).unwrap_err();
    assert_eq!(code, "Unavailable", "{message}");
    assert!(message.contains("layerfs machine start box"), "{message}");
    assert!(at.elapsed() < Duration::from_secs(3), "{:?}", at.elapsed());
    let (code, _) = call(&home, "sandbox.log", &[("sandbox", t("s1"))]).unwrap_err();
    assert_eq!(code, "Unavailable");
    assert_eq!(files(&home), before);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_forwarded_sandbox_s_run_folder_is_read_in_its_machine() {
    let _turn = serial();
    let (scratch, home, root) = boxed("forwarded");
    // the home reaches the daemon directly and keeps the Workspace records
    let forward = map(&[("address", t("127.0.0.1:1")), ("machine", t("box"))]);
    write_record(
        &paths::sandbox_record(&home, "s1"),
        &map(&[
            ("name", t("s1")),
            ("project", t("shop/api")),
            ("machine", t("box")),
            ("id", t("s1")),
            ("runtime", t("namespaces")),
            (
                "daemon",
                map(&[("name", t("layerfs-0a1b2c3d-s1")), ("forward", forward)]),
            ),
        ]),
    )
    .unwrap();
    let mut namespaces = Vec::new();
    for n in [1, 2, 5] {
        let id = format!("{n:064x}");
        let record = WorkspaceRecord::mounted(id.clone(), None, "main", &format!("/w/{n}"), n);
        let file = paths::workspace_dir(&home, "s1").join(format!("{id}.json"));
        record.write(&file).unwrap();
        namespaces.push(n);
    }
    // the run folder, its log and the overlay files are in the machine
    let folder = local_sandbox(&root, "in the guest\n");
    std::fs::write(folder.join("overlay/overlay.sqlite"), vec![0u8; 4096]).unwrap();
    std::fs::write(folder.join("overlay/1.sqlite"), vec![0u8; 4096]).unwrap();
    std::fs::write(folder.join("overlay/5.sqlite"), vec![0u8; 12288]).unwrap();
    settle(&home);
    let (before, guest) = (files(&home), files(&root));
    let log = call(&home, "sandbox.log", &[("sandbox", t("s1"))]).unwrap();
    assert_eq!(texts(&log), ["in the guest"]);
    // the sizes of the Workspaces the home's records name, null for one
    // with no overlay file in the machine
    let record = layerfs_sandbox::record::sandbox(&home, "s1")
        .unwrap()
        .unwrap();
    let sizes =
        layerfs_sandbox::status::overlay::overlay_sizes(&home, record.machine(), "s1", &namespaces);
    let sizes = sizes.unwrap();
    assert_eq!(
        layerfs_home::json::print(sizes.get("overlay_sizes").unwrap()),
        r#"[{"namespace":1,"bytes":4096},{"namespace":2,"bytes":null},{"namespace":5,"bytes":12288}]"#
    );
    // with them, the daemon process the worker's record names
    // (layerfs-project#164): this test's own, a host process
    let SdkValue::Map(fields) = &sizes else {
        panic!("{sizes:?}")
    };
    let names: Vec<&str> = fields.iter().map(|(name, _)| name.as_str()).collect();
    // and its Workspaces' cgroups, of which a host process has none
    assert_eq!(
        names,
        [
            "overlay_sizes",
            "daemon",
            "usage",
            "usage_reason",
            "workspace_usage",
            "workspace_usage_reason"
        ]
    );
    assert_eq!(
        sizes.get_text("workspace_usage_reason"),
        Some("host_process")
    );
    assert_eq!(
        sizes.get("daemon").unwrap().get_int("pid"),
        Some(i64::from(std::process::id()))
    );
    assert_eq!(sizes.get_text("usage_reason"), Some("host_process"));
    assert_eq!(files(&home), before);
    assert_eq!(files(&root), guest);
    std::fs::remove_dir_all(scratch).unwrap();
}
