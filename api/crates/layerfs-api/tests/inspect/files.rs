//! `machine.files` (layerfs-project#178 step 4) on the routes of an inspect
//! request: this process for `native`, a machine's worker (this build's
//! own server on a scratch root) for every other, and that worker with the
//! home's Workspace records for a Sandbox whose daemon the home reaches
//! directly. Its answers, and what #179 B asks of every monitoring read:
//! no lock or lease, nothing written on either side, no call-log line (the
//! call is `quiet`), nothing started, and no refusal while a machine
//! operation holds the machine.
//!
//! Nothing runs: a machine is the folder its worker serves. Inspect
//! permits are the process's, and the worker is in this process, so every
//! test here takes one lock. The walker's and the views' own tests are
//! layerfs-sandbox `inspect_machine_files`.
#![cfg(unix)]
use layerfs_protocol::sdk::SdkValue;
use layerfs_sandbox::record::{DaemonRecord, Local, WorkspaceRecord};
use std::collections::BTreeMap;
use std::net::{SocketAddr, TcpListener};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};
use {
    layerfs_api::home::{self, paths, read_record, write_record},
    layerfs_store::lease,
};

static PROCESS: Mutex<()> = Mutex::new(());
static HOMES: AtomicUsize = AtomicUsize::new(0);

fn serial() -> MutexGuard<'static, ()> {
    PROCESS.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn tempdir(tag: &str) -> PathBuf {
    let sequence = HOMES.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "lfs-mfiles-{tag}-{}-{sequence}",
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
/// runs it records it, with its run folder as a start leaves it and two
/// files a command left in its home; answers the folder.
fn local_sandbox(root: &Path) -> PathBuf {
    let name = "layerfs-0a1b2c3d-s1";
    let folder = paths::run_root(root).join(name);
    for part in ["overlay", "config", "workspaces", "tmp/project"] {
        std::fs::create_dir_all(folder.join(part)).unwrap();
    }
    std::fs::write(folder.join("daemon.log"), "listening\n").unwrap();
    std::fs::write(folder.join("config/daemon.setup"), "private key").unwrap();
    std::fs::write(folder.join("tmp/notes.txt"), "left by a command\n").unwrap();
    std::fs::write(folder.join("tmp/project/main.rs"), "fn main() {}\n").unwrap();
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
    assert!(!layerfs_store::open::close(&file).unwrap());
}

/// Workspace `n` of Sandbox `s1`, recorded in `root`'s Sandbox folder as
/// mounted at `path`, with no name: its handle, `s1:<path>`, as a mount
/// entry names it (call surface 2.0).
fn workspace(root: &Path, n: u8, path: &Path) -> String {
    let id = format!("{n:064x}");
    let record = WorkspaceRecord::mounted(id.clone(), None, "main", path.to_str().unwrap(), 0);
    let file = paths::workspace_dir(root, "s1").join(format!("{id}.json"));
    record.write(&file).unwrap();
    format!("s1:{}", path.display())
}

fn p(path: &Path) -> SdkValue {
    t(path.to_str().unwrap())
}

fn entries(answer: &SdkValue) -> &[SdkValue] {
    assert_eq!(answer.get_text("type"), Some("dir"), "{answer:?}");
    match answer.get("items") {
        Some(SdkValue::List(entries)) => entries,
        _ => panic!("{answer:?}"),
    }
}

/// Each entry as `<path below base> <type>[ <not_read>][ <role>][
/// <workspace>]`.
fn shown(answer: &SdkValue, base: &Path) -> Vec<String> {
    entries(answer)
        .iter()
        .map(|entry| {
            let path = Path::new(entry.get_text("path").unwrap());
            let below = path.strip_prefix(base).unwrap_or(path).display();
            let mut line = format!("{below} {}", entry.get_text("type").unwrap());
            for field in ["not_read", "role", "workspace"] {
                if let Some(value) = entry.get_text(field) {
                    line = format!("{line} {value}");
                }
            }
            line
        })
        .collect()
}

fn files_of(
    home: &Path,
    machine: &str,
    more: &[(&str, SdkValue)],
) -> Result<SdkValue, (String, String)> {
    let mut asked = vec![("machine", t(machine)), ("detail", SdkValue::Bool(true))];
    asked.extend(more.iter().cloned());
    call(home, "machine.files", &asked)
}

#[test]
fn the_table_lists_the_call_quiet_with_a_path_and_its_options() {
    let call = layerfs_api::find("machine.files").unwrap();
    let args: Vec<_> = call.args.iter().map(|a| (a.name, a.optional)).collect();
    assert_eq!(args, [("machine", false), ("path", true)]);
    let options: Vec<_> = call.options.iter().map(|a| a.name).collect();
    assert_eq!(
        options,
        ["depth", "limit", "cursor", "byte_offset", "byte_length"]
    );
    assert!(call.quiet);
    let table = layerfs_home::json::parse(&layerfs_api::table_json()).unwrap();
    let SdkValue::List(calls) = table else {
        panic!()
    };
    let machine: Vec<_> = calls
        .iter()
        .filter(|c| c.get_text("family") == Some("machine"))
        .map(|c| c.get_text("call").unwrap())
        .collect();
    assert_eq!(
        machine,
        [
            "list",
            "get",
            "images",
            "load_image",
            "prune",
            "stats",
            "files",
            "log",
            "start",
            "stop",
            "create",
            "remove",
            "update",
            "doctor"
        ]
    );
}

#[test]
fn a_native_machine_s_layerfs_folders_are_read_here_with_no_lock_line_or_write() {
    let _turn = serial();
    let home = tempdir(layerfs_home::names::HOST_MACHINE);
    home::open(&home).unwrap();
    home_records(&home, layerfs_home::names::HOST_MACHINE, "host", None);
    let folder = local_sandbox(&home);
    // a mounted Workspace's folder is there; its record names it
    std::fs::create_dir(folder.join("workspaces/1")).unwrap();
    let id = workspace(&home, 1, &folder.join("workspaces/1"));
    std::fs::create_dir_all(paths::log_dir(&home)).unwrap();
    settle(&home);
    held(&home, layerfs_home::names::HOST_MACHINE);
    let push = pushing(&home);
    let log = std::fs::read(paths::calls_file(&home)).unwrap();
    let before = files(&home);
    let at = Instant::now();

    let roots = files_of(&home, layerfs_home::names::HOST_MACHINE, &[]).unwrap();
    // the Store's lock was held throughout
    assert!(at.elapsed() < Duration::from_secs(10), "{:?}", at.elapsed());
    // the home's run root is its host machine's (#819 R)
    let run_root = format!(
        "machine/{}/run dir run_folders",
        layerfs_home::names::HOST_MACHINE
    );
    assert_eq!(
        shown(&roots, &home),
        [
            "store dir stores",
            "sandbox dir sandbox_records",
            run_root.as_str(),
            "log dir logs"
        ]
    );
    let run = files_of(
        &home,
        layerfs_home::names::HOST_MACHINE,
        &[("path", p(&folder)), ("depth", SdkValue::Int(2))],
    )
    .unwrap();
    assert_eq!(
        shown(&run, &folder),
        [
            "config dir restricted".to_owned(),
            "daemon.log file log".to_owned(),
            "overlay dir restricted".to_owned(),
            "tmp dir".to_owned(),
            "tmp/notes.txt file".to_owned(),
            "tmp/project dir".to_owned(),
            "workspaces dir".to_owned(),
            format!("workspaces/1 dir mount {id}"),
        ]
    );
    // a page that ends early answers next_cursor, which continues it
    let one = [
        ("path", p(&folder)),
        ("depth", SdkValue::Int(2)),
        ("limit", SdkValue::Int(1)),
    ];
    let first = files_of(&home, layerfs_home::names::HOST_MACHINE, &one).unwrap();
    assert_eq!(shown(&first, &folder), ["config dir restricted"]);
    let cursor = first.get("next_cursor").cloned().unwrap();
    assert!(matches!(cursor, SdkValue::Text(_)), "{first:?}");
    let mut next = one.to_vec();
    next.push(("cursor", cursor));
    let second = files_of(&home, layerfs_home::names::HOST_MACHINE, &next).unwrap();
    assert_eq!(shown(&second, &folder), ["daemon.log file log"]);
    let window = [
        ("path", p(&folder.join("tmp/notes.txt"))),
        ("byte_offset", SdkValue::Int(5)),
        ("byte_length", SdkValue::Int(4)),
    ];
    assert_eq!(
        layerfs_home::json::print(
            &files_of(&home, layerfs_home::names::HOST_MACHINE, &window).unwrap()
        ),
        r#"{"items":null,"next_cursor":null,"text":"by a","binary":false,"size":null,"next_byte_offset":9,"type":"file"}"#
    );
    for (path, code) in [
        (folder.join("config/daemon.setup"), "InvalidInput"),
        (paths::store_file(&home, "shop"), "NotFound"),
        (
            paths::worker_record(&home, layerfs_home::names::HOST_MACHINE),
            "InvalidInput",
        ),
        (PathBuf::from("/etc/hosts"), "InvalidInput"),
        (PathBuf::from("/"), "InvalidInput"),
    ] {
        let (refused, message) = files_of(
            &home,
            layerfs_home::names::HOST_MACHINE,
            &[("path", p(&path))],
        )
        .unwrap_err();
        assert_eq!(refused, code, "{}: {message}", path.display());
    }
    let (code, message) = files_of(
        &home,
        layerfs_home::names::HOST_MACHINE,
        &[("limit", SdkValue::Int(1001))],
    )
    .unwrap_err();
    assert_eq!(
        (code.as_str(), message.as_str()),
        ("InvalidInput", "machine.files: `limit` is 1 to 1000")
    );
    let (code, _) = files_of(&home, "No Such", &[]).unwrap_err();
    assert_eq!(code, "InvalidInput");

    // nothing changed in the home: no record, lock or lease, and not one
    // call-log line, for an answer or for a refusal
    assert_eq!(
        files(&home),
        before,
        "a files read changed a file in the home"
    );
    assert_eq!(std::fs::read(paths::calls_file(&home)).unwrap(), log);
    no_lease(&home);
    // the log is on: the next ordinary call writes its line
    drop(push);
    layerfs_api::call(Some(&home), "sandbox.list", SdkValue::Null, &mut |_| {}).unwrap();
    assert!(std::fs::read(paths::calls_file(&home)).unwrap().len() > log.len());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_worker_reads_its_machine_s_files_and_neither_side_changes() {
    let _turn = serial();
    let (scratch, home, root) = boxed("worker");
    worker_sandbox(&home);
    let folder = local_sandbox(&root);
    // the worker's root keeps the Workspace records of its Sandboxes
    std::fs::create_dir(folder.join("workspaces/3")).unwrap();
    let id = workspace(&root, 3, &folder.join("workspaces/3"));
    std::fs::create_dir_all(root.join("store/shop")).unwrap();
    std::fs::create_dir_all(scratch.join("state")).unwrap();
    std::fs::write(scratch.join("state/secret"), "k").unwrap();
    settle(&home);
    held(&home, "box");
    let _push = pushing(&home);
    let (before, guest) = (files(&home), files(&scratch.join("state")));
    let served = files(&root);

    let roots = files_of(&home, "box", &[]).unwrap();
    assert_eq!(
        shown(&roots, &scratch),
        [
            "data dir data",
            "data/store dir stores",
            "data/sandbox dir sandbox_records",
            "data/machine/linux-guest/run dir run_folders",
            "state dir state",
            "/ dir machine"
        ]
    );
    let run = files_of(&home, "box", &[("path", p(&folder.join("workspaces")))]).unwrap();
    assert_eq!(
        shown(&run, &folder),
        [format!("workspaces/3 dir mount {id}")]
    );
    let state = files_of(&home, "box", &[("path", p(&scratch.join("state")))]).unwrap();
    assert_eq!(shown(&state, &scratch), ["state/secret file restricted"]);
    let (code, message) =
        files_of(&home, "box", &[("path", p(&scratch.join("state/secret")))]).unwrap_err();
    assert_eq!(code, "InvalidInput", "{message}");
    // the machine from `/`: a system folder is listed and not entered
    let top = files_of(
        &home,
        "box",
        &[("path", t("/")), ("limit", SdkValue::Int(1000))],
    )
    .unwrap();
    let dev = (entries(&top).iter()).find(|entry| entry.get_text("path") == Some("/dev"));
    assert_eq!(dev.and_then(|dev| dev.get_text("not_read")), Some("system"));
    let (code, message) = files_of(&home, "box", &[("path", t("/dev/null"))]).unwrap_err();
    assert_eq!(code, "InvalidInput", "{message}");
    assert!(message.contains("is in a system folder"), "{message}");

    assert_eq!(
        files(&home),
        before,
        "a files read changed a file in the home"
    );
    assert_eq!(
        files(&root),
        served,
        "a files read changed a file under the root"
    );
    assert_eq!(files(&scratch.join("state")), guest);
    no_lease(&home);
    no_lease(&root);
    std::fs::remove_dir_all(scratch).unwrap();
}

#[test]
fn a_forwarded_sandbox_s_workspaces_are_named_from_the_home_s_records() {
    let _turn = serial();
    let (scratch, home, root) = boxed("forwarded");
    // the home reaches the daemon directly and keeps the Workspace
    // records; the run folder is in the machine, which keeps none
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
    let folder = local_sandbox(&root);
    std::fs::create_dir(folder.join("workspaces/2")).unwrap();
    std::fs::create_dir(folder.join("workspaces/10")).unwrap();
    let ten = workspace(&home, 10, &folder.join("workspaces/10"));
    let two = workspace(&home, 2, &folder.join("workspaces/2"));
    assert!(!paths::workspace_dir(&root, "s1").exists());
    settle(&home);
    let (before, guest) = (files(&home), files(&root));

    let listed = files_of(&home, "box", &[("path", p(&folder.join("workspaces")))]).unwrap();
    assert_eq!(
        shown(&listed, &folder),
        [
            format!("workspaces/10 dir mount {ten}"),
            format!("workspaces/2 dir mount {two}"),
        ]
    );
    assert_eq!(files(&home), before);
    assert_eq!(files(&root), guest);
    std::fs::remove_dir_all(scratch).unwrap();
}

#[test]
fn a_stopped_machine_is_unavailable_and_nothing_starts_it() {
    let _turn = serial();
    // a machine that records no worker
    let home = tempdir("stopped");
    home::open(&home).unwrap();
    home_records(&home, "box", "docker", None);
    settle(&home);
    let before = files(&home);
    for path in [None, Some("/layerfs/data"), Some("/")] {
        let asked: Vec<_> = path.map(|path| ("path", t(path))).into_iter().collect();
        let (code, message) = files_of(&home, "box", &asked).unwrap_err();
        assert_eq!(code, "Unavailable", "{message}");
        assert!(message.contains("layerfs machine start box"), "{message}");
    }
    assert_eq!(files(&home), before);
    std::fs::remove_dir_all(home).unwrap();
}
