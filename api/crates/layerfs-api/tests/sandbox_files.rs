//! `sandbox.files`, Sandbox view (layerfs-project#178 step 3), on the routes
//! of an inspect request: this process for a `native` Sandbox, a machine's
//! worker (this build's own server on a scratch root) for every other, and
//! that worker with the home's Workspace records for a Sandbox whose daemon
//! the home reaches directly. Its answers, and what #179 B asks of every
//! monitoring read: no lock or lease, nothing written on either side, no
//! call-log line (the call is `quiet`), nothing started, and no refusal
//! while a machine operation holds the machine. And the Root view (step
//! 5): a namespaces Sandbox's roots offer `/`, read on its machine.
//!
//! Nothing runs: a Sandbox is its record and its run folder. Inspect
//! permits are the process's, and the worker is in this process, so every
//! test here takes one lock. The walker's own tests are layerfs-sandbox
//! `inspect_files`.
#![cfg(unix)]
use layerfs_api::home::{self, lease, paths, read_record, write_record};
use layerfs_bridge::control::SdkValue;
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
    let dir = std::env::temp_dir().join(format!(
        "lfs-sfiles-{tag}-{}-{sequence}",
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

/// One call, in its long form, on a thread of its own: an answer that does
/// not come within 20 s fails the test (a call that waited for the Store's
/// lock would wait 120 s).
fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, (String, String)> {
    let mut args = args.to_vec();
    args.push(("detail", SdkValue::Bool(true)));
    let (home, name, args) = (home.to_owned(), name.to_owned(), map(&args));
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
    if machine != "native" {
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
    sandbox_as(root, "native")
}

/// [`local_sandbox`] under backend `runtime`: `namespaces` is a Sandbox
/// with a root of its own, whose init this process stands in for.
fn sandbox_as(root: &Path, runtime: &'static str) -> PathBuf {
    let name = "layerfs-0a1b2c3d-s1";
    let folder = paths::run_root(root).join(name);
    for part in ["overlay", "config", "workspaces", "tmp/project"] {
        std::fs::create_dir_all(folder.join(part)).unwrap();
    }
    std::fs::write(folder.join("daemon.log"), "listening\n").unwrap();
    std::fs::write(folder.join("config/daemon.setup"), "private key").unwrap();
    std::fs::write(folder.join("tmp/notes.txt"), "left by a command\n").unwrap();
    std::fs::write(folder.join("tmp/project/main.rs"), "fn main() {}\n").unwrap();
    let me = layerfs_bridge::process::LocalProcess::observe(std::process::id()).unwrap();
    let daemon = DaemonRecord::local(Local {
        platform: "macos".to_owned(),
        name: name.to_owned(),
        container: "ab".repeat(32),
        runtime,
        uid: home::flock::euid(),
        gid: home::flock::egid(),
        directory: folder.display().to_string(),
        process: (me.pid, me.start, format!("{:032x}", me.boot.unwrap())),
        port: 1,
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
            ("runtime", t(runtime)),
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
/// mounted at `path` with no name: its handle, `s1:<path>`.
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

/// Each entry as `(path below base, kind, role, workspace)`, its kind why
/// it was not read, else its `type`.
fn shown(answer: &SdkValue, base: &Path) -> Vec<(String, String, String, String)> {
    let text = |entry: &SdkValue, name: &str| entry.get_text(name).unwrap_or("-").to_owned();
    entries(answer)
        .iter()
        .map(|entry| {
            let path = Path::new(entry.get_text("path").unwrap());
            let below = path
                .strip_prefix(base)
                .unwrap_or(path)
                .display()
                .to_string();
            (
                below,
                entry
                    .get_text("not_read")
                    .or(entry.get_text("type"))
                    .unwrap_or("-")
                    .to_owned(),
                text(entry, "role"),
                text(entry, "workspace"),
            )
        })
        .collect()
}

fn row(path: &str, kind: &str, role: &str, workspace: &str) -> (String, String, String, String) {
    (
        path.to_owned(),
        kind.to_owned(),
        role.to_owned(),
        workspace.to_owned(),
    )
}

/// The reads of Sandbox `s1` whose run folder is `folder`, common to every
/// route: its home's listing, a file window, and its refusals.
fn reads_its_home(home: &Path, folder: &Path) {
    reads_its_home_only(home, folder);
    // a host-process Sandbox has no other view: what is not its home
    for path in [
        folder.join("config/daemon.setup"),
        folder.join("daemon.log"),
        PathBuf::from("/etc/hosts"),
    ] {
        let asked = [("sandbox", t("s1")), ("path", p(&path))];
        let (refused, message) = call(home, "sandbox.files", &asked).unwrap_err();
        assert_eq!(refused, "InvalidInput", "{}: {message}", path.display());
    }
}

/// The reads of a Sandbox's home that every Sandbox answers alike.
fn reads_its_home_only(home: &Path, folder: &Path) {
    let tmp = folder.join("tmp");
    let listed = call(
        home,
        "sandbox.files",
        &[
            ("sandbox", t("s1")),
            ("path", p(&tmp)),
            ("depth", SdkValue::Int(2)),
        ],
    )
    .unwrap();
    assert_eq!(
        shown(&listed, &tmp),
        [
            row("notes.txt", "file", "-", "-"),
            row("project", "dir", "-", "-"),
            row("project/main.rs", "file", "-", "-"),
        ]
    );
    assert_eq!(listed.get("next_cursor"), Some(&SdkValue::Null));
    let paged = [
        ("sandbox", t("s1")),
        ("path", p(&tmp)),
        ("limit", SdkValue::Int(1)),
    ];
    let first = call(home, "sandbox.files", &paged).unwrap();
    let cursor = first.get("next_cursor").unwrap().clone();
    assert!(matches!(cursor, SdkValue::Text(_)), "{first:?}");
    let mut next = paged.to_vec();
    next.push(("cursor", cursor));
    let second = call(home, "sandbox.files", &next).unwrap();
    assert_eq!(shown(&second, &tmp), [row("project", "dir", "-", "-")]);
    assert_eq!(second.get("next_cursor"), Some(&SdkValue::Null));
    let file = [
        ("sandbox", t("s1")),
        ("path", p(&tmp.join("notes.txt"))),
        ("byte_offset", SdkValue::Int(5)),
        ("byte_length", SdkValue::Int(4)),
    ];
    assert_eq!(
        layerfs_api::json::print(&call(home, "sandbox.files", &file).unwrap()),
        r#"{"items":null,"next_cursor":null,"text":"by a","binary":false,"size":null,"next_byte_offset":9,"type":"file"}"#
    );
    let asked = [("sandbox", t("s1")), ("path", p(&tmp.join("nothing")))];
    let (refused, message) = call(home, "sandbox.files", &asked).unwrap_err();
    assert_eq!(refused, "NotFound", "{message}");
    let wide = [("sandbox", t("s1")), ("limit", SdkValue::Int(1001))];
    let (code, message) = call(home, "sandbox.files", &wide).unwrap_err();
    assert_eq!(
        (code.as_str(), message.as_str()),
        ("InvalidInput", "sandbox.files: `limit` is 1 to 1000")
    );
}

#[test]
fn the_table_lists_the_call_quiet_with_a_path_and_its_options() {
    let call = layerfs_api::find("sandbox.files").unwrap();
    let args: Vec<_> = call.args.iter().map(|a| (a.name, a.optional)).collect();
    assert_eq!(args, [("sandbox", false), ("path", true)]);
    let options: Vec<_> = call.options.iter().map(|a| a.name).collect();
    assert_eq!(
        options,
        ["depth", "limit", "cursor", "byte_offset", "byte_length"]
    );
    // it, `home.get` (a setup poll), the home's call log calls,
    // `sandbox.log`, `machine.stats` (layerfs-project#164, with `--with
    // space`), `machine.files` and `machine.log` carry the flag: no family is
    // skipped by its name
    let quiet: Vec<_> = layerfs_api::calls()
        .into_iter()
        .filter(|call| call.quiet)
        .map(|call| call.name())
        .collect();
    assert_eq!(
        quiet,
        [
            "home.get",
            "home.log",
            "home.clear_log",
            "sandbox.files",
            "sandbox.log",
            "machine.stats",
            "machine.files",
            "machine.log"
        ]
    );
    let table = layerfs_api::json::parse(&layerfs_api::table_json()).unwrap();
    let SdkValue::List(calls) = table else {
        panic!()
    };
    let sandbox: Vec<_> = calls
        .iter()
        .filter(|c| c.get_text("family") == Some("sandbox"))
        .map(|c| c.get_text("call").unwrap())
        .collect();
    assert_eq!(
        sandbox,
        ["create", "list", "get", "remove", "files", "log", "doctor"]
    );
}

#[test]
fn a_native_sandbox_s_files_are_read_here_with_no_lock_line_or_write() {
    let _turn = serial();
    let home = tempdir("native");
    home::open(&home).unwrap();
    home_records(&home, "native", "native", None);
    let folder = local_sandbox(&home);
    let first = workspace(&home, 1, &folder.join("workspaces/1"));
    let second = workspace(&home, 2, &folder.join("workspaces/2"));
    settle(&home);
    held(&home, "native");
    let push = pushing(&home);
    let log = std::fs::read(paths::calls_file(&home)).unwrap();
    let before = files(&home);
    let at = Instant::now();

    let roots = call(&home, "sandbox.files", &[("sandbox", t("s1"))]).unwrap();
    // the Store's lock was held throughout
    assert!(at.elapsed() < Duration::from_secs(10), "{:?}", at.elapsed());
    assert_eq!(
        shown(&roots, &folder),
        [
            row("workspaces/1", "mount", "workspace", &first),
            row("workspaces/2", "mount", "workspace", &second),
            row("tmp", "dir", "home", "-"),
            row("daemon.log", "file", "log", "-"),
        ]
    );
    reads_its_home(&home, &folder);
    let (code, _) = call(&home, "sandbox.files", &[("sandbox", t("nobody"))]).unwrap_err();
    assert_eq!(code, "NotFound");

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
fn a_worker_reads_its_sandbox_s_files_and_neither_side_changes() {
    let _turn = serial();
    let (scratch, home, root) = boxed("worker");
    worker_sandbox(&home);
    let folder = local_sandbox(&root);
    // the worker's root keeps the Workspace records of its Sandboxes
    let id = workspace(&root, 3, &folder.join("workspaces/3"));
    settle(&home);
    held(&home, "box");
    let _push = pushing(&home);
    let (before, guest) = (files(&home), files(&root));

    let roots = call(&home, "sandbox.files", &[("sandbox", t("s1"))]).unwrap();
    assert_eq!(
        shown(&roots, &folder),
        [
            row("workspaces/3", "mount", "workspace", &id),
            row("tmp", "dir", "home", "-"),
            row("daemon.log", "file", "log", "-"),
        ]
    );
    reads_its_home(&home, &folder);
    assert_eq!(
        files(&home),
        before,
        "a files read changed a file in the home"
    );
    assert_eq!(
        files(&root),
        guest,
        "a files read changed a file under the root"
    );
    no_lease(&home);
    no_lease(&root);

    // a Sandbox the worker holds no run folder for
    std::fs::remove_dir_all(paths::sandbox_dir(&root, "s1")).unwrap();
    let (code, message) = call(&home, "sandbox.files", &[("sandbox", t("s1"))]).unwrap_err();
    assert_eq!(code, "NotFound", "{message}");
    std::fs::remove_dir_all(scratch).unwrap();
}

/// Sandbox `s1` on machine `box` as the home records one whose daemon it
/// reaches directly: the home keeps its Workspace records, and the run
/// folder is in the machine, which keeps none.
fn forwarded_sandbox(home: &Path) {
    let forward = map(&[("address", t("127.0.0.1:1")), ("machine", t("box"))]);
    write_record(
        &paths::sandbox_record(home, "s1"),
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
}

/// A listing in a forwarded Sandbox's root names its Workspace mounts
/// from the home's records too: they go with a request that has a path,
/// not only with the roots read.
#[test]
fn a_forwarded_sandbox_s_root_names_the_workspaces_the_home_records() {
    let _turn = serial();
    let (scratch, home, root) = boxed("forwarded-root");
    forwarded_sandbox(&home);
    // this process stands in for the Sandbox's init: its root is `/`
    let folder = sandbox_as(&root, "namespaces");
    let mounts = folder.join("workspaces");
    for n in ["2", "10"] {
        std::fs::create_dir(mounts.join(n)).unwrap();
    }
    let ten = workspace(&home, 10, &mounts.join("10"));
    let two = workspace(&home, 2, &mounts.join("2"));
    assert!(!paths::workspace_dir(&root, "s1").exists());
    settle(&home);
    let asked = [("sandbox", t("s1")), ("path", t(mounts.to_str().unwrap()))];
    let listed = call(&home, "sandbox.files", &asked);
    if cfg!(target_os = "linux") && home::flock::euid() == 0 {
        assert_eq!(
            shown(&listed.unwrap(), &folder),
            [
                row("workspaces/10", "mount", "-", &ten),
                row("workspaces/2", "mount", "-", &two),
            ]
        );
    } else {
        let (code, message) = listed.unwrap_err();
        assert_eq!(code, "Unsupported", "{message}");
        println!("NOT_RUN a forwarded Sandbox's root listing: {message}");
    }
    std::fs::remove_dir_all(scratch).unwrap();
}

#[test]
fn a_forwarded_sandbox_s_roots_list_the_workspaces_the_home_records() {
    let _turn = serial();
    let (scratch, home, root) = boxed("forwarded");
    forwarded_sandbox(&home);
    let folder = local_sandbox(&root);
    let ten = workspace(&home, 10, &folder.join("workspaces/10"));
    let two = workspace(&home, 2, &folder.join("workspaces/2"));
    assert!(!paths::workspace_dir(&root, "s1").exists());
    settle(&home);
    let (before, guest) = (files(&home), files(&root));

    let roots = call(&home, "sandbox.files", &[("sandbox", t("s1"))]).unwrap();
    assert_eq!(
        shown(&roots, &folder),
        [
            row("workspaces/2", "mount", "workspace", &two),
            row("workspaces/10", "mount", "workspace", &ten),
            row("tmp", "dir", "home", "-"),
            row("daemon.log", "file", "log", "-"),
        ]
    );
    reads_its_home(&home, &folder);
    assert_eq!(files(&home), before);
    assert_eq!(files(&root), guest);
    std::fs::remove_dir_all(scratch).unwrap();
}

#[test]
fn a_namespaces_sandbox_s_roots_offer_its_root_which_its_machine_reads() {
    let _turn = serial();
    let (scratch, home, root) = boxed("rooted");
    worker_sandbox(&home);
    // this process stands in for the Sandbox's init: its root is `/`
    let folder = sandbox_as(&root, "namespaces");
    settle(&home);
    held(&home, "box");
    let (before, guest) = (files(&home), files(&root));

    let roots = call(&home, "sandbox.files", &[("sandbox", t("s1"))]).unwrap();
    assert_eq!(
        shown(&roots, &folder),
        [
            row("tmp", "dir", "home", "-"),
            row("daemon.log", "file", "log", "-"),
            row("/", "dir", "root", "-"),
        ]
    );
    let top = call(
        &home,
        "sandbox.files",
        &[
            ("sandbox", t("s1")),
            ("path", t("/")),
            ("limit", SdkValue::Int(1000)),
        ],
    );
    if cfg!(target_os = "linux") && home::flock::euid() == 0 {
        let top = top.unwrap();
        let kind = |path: &str| {
            let found = (entries(&top).iter()).find(|entry| entry.get_text("path") == Some(path));
            found.and_then(|entry| entry.get_text("not_read").or(entry.get_text("type")))
        };
        assert_eq!(kind("/proc"), Some("system"));
        assert_eq!(kind("/etc"), Some("dir"));
        let asked = [("sandbox", t("s1")), ("path", t("/proc/self/environ"))];
        let (code, message) = call(&home, "sandbox.files", &asked).unwrap_err();
        assert_eq!(code, "InvalidInput", "{message}");
    } else {
        // macOS has no such root, and a reader that is not root may not
        // read one: the view says so, with its reason
        let (code, message) = top.unwrap_err();
        assert_eq!(code, "Unsupported", "{message}");
        println!("NOT_RUN the root's listing: {message}");
    }
    // its home is read as any Sandbox's
    reads_its_home_only(&home, &folder);
    assert_eq!(
        files(&home),
        before,
        "a files read changed a file in the home"
    );
    assert_eq!(
        files(&root),
        guest,
        "a files read changed a file under the root"
    );
    no_lease(&home);
    no_lease(&root);
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
    for path in [None, Some("/layerfs/data/machine/native/run/x/tmp")] {
        let mut asked = vec![("sandbox", t("s1"))];
        asked.extend(path.map(|path| ("path", t(path))));
        let (code, message) = call(&home, "sandbox.files", &asked).unwrap_err();
        assert_eq!(code, "Unavailable", "{message}");
        assert!(message.contains("layerfs machine start box"), "{message}");
    }
    assert_eq!(files(&home), before);
    std::fs::remove_dir_all(home).unwrap();
}
