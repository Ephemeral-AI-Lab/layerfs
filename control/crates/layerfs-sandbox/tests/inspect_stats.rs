//! The reads of layerfs-project#164 on the Local route (a `native`
//! machine, in this process) and by themselves: a Sandbox's daemon process
//! (`inspect::process`: live or gone by id, start time and boot; when it
//! started; its cgroup's usage from a cgroup v2 folder given as an
//! argument), the inspect kinds `sandboxes_live` and `machine_stats`
//! (processors, memory, the data disk, Store sizes by `stat`, live
//! Sandboxes), what a request past its soft budget answers, and many
//! Sandboxes and Stores inside it.
//!
//! Nothing runs but two short `sleep`s: Stores are files this test writes
//! (never opened by the read), daemons are this test's own process or a
//! child that ended. The inspect permits are the process's, so every test
//! that asks takes one lock.
#![cfg(any(target_os = "linux", target_os = "macos"))]
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::process::LocalProcess;
use layerfs_home::home::{self, paths, write_record};
use layerfs_home::names::MachineName;
use layerfs_sandbox::inspect::{self, process, Side, HARD, SOFT};
use layerfs_sandbox::record::{DaemonRecord, Local, SandboxRecord};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

static PROCESS: Mutex<()> = Mutex::new(());
static NEXT: AtomicU32 = AtomicU32::new(0);

/// The test's turn, once no earlier test's helper is still running.
fn serial() -> MutexGuard<'static, ()> {
    let guard = PROCESS.lock().unwrap_or_else(|poison| poison.into_inner());
    let started = Instant::now();
    while inspect::running() != 0 {
        assert!(started.elapsed() < Duration::from_secs(15), "helpers");
        std::thread::sleep(Duration::from_millis(10));
    }
    guard
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

/// A new folder under the build's own temporary folder.
fn scratch(tag: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "lfs-stats-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    home::private_dir(&dir).unwrap();
    dir
}

/// A new home.
fn new_home(tag: &str) -> PathBuf {
    let dir = scratch(tag);
    home::ensure(&dir).unwrap();
    dir
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

/// A daemon record of `runtime` that names `process`.
fn daemon(process: LocalProcess, runtime: &'static str, directory: &Path) -> DaemonRecord {
    DaemonRecord::local(Local {
        platform: "linux".to_owned(),
        name: "layerfs-0a1b2c3d-s".to_owned(),
        container: "ab".repeat(32),
        runtime,
        uid: home::flock::euid(),
        gid: home::flock::egid(),
        directory: directory.display().to_string(),
        process: (
            process.pid,
            process.start,
            format!("{:032x}", process.boot.unwrap()),
        ),
        port: 1,
        image: "local".to_owned(),
        labels: Vec::new(),
    })
}

/// Sandbox `name` on `machine` in `root`, with daemon record `daemon` when
/// it started.
fn sandbox(root: &Path, name: &str, machine: &str, daemon: Option<&DaemonRecord>) {
    let mut fields = vec![
        ("name", t(name)),
        ("project", t("shop/api")),
        ("machine", t(machine)),
        ("id", t(name)),
        ("runtime", t("native")),
    ];
    fields.extend(daemon.map(|daemon| ("daemon", daemon.to_record())));
    let record = SdkValue::Map(
        (fields.into_iter())
            .map(|(field, value)| (field.to_owned(), value))
            .collect(),
    );
    write_record(&paths::sandbox_record(root, name), &record).unwrap();
}

/// sqlite Store `name` on `machine` as the home records it.
fn store_record(home: &Path, name: &str, machine: &str) {
    write_record(
        &paths::store_record(home, name),
        &SdkValue::map([
            ("name", t(name)),
            ("kind", t("sqlite")),
            ("binding", SdkValue::Bytes(vec![7; 16])),
            ("cursor", SdkValue::Bytes(vec![8; 32])),
            ("profile", t("disposable")),
            ("machine", t(machine)),
        ]),
    )
    .unwrap();
}

fn native() -> MachineName {
    MachineName::parse("native").unwrap()
}

fn now() -> i64 {
    home::now_ms()
}

fn listed<'a>(answer: &'a SdkValue, field: &str) -> &'a [SdkValue] {
    match answer.get(field) {
        Some(SdkValue::List(items)) => items,
        other => panic!("{field}: {other:?} in {answer:?}"),
    }
}

fn names(names: &[&str]) -> SdkValue {
    SdkValue::List(names.iter().map(|name| t(name)).collect())
}

fn refused(outcome: Result<SdkValue, SdkRefusal>) -> String {
    match outcome {
        Ok(value) => format!("ok {value:?}"),
        Err(refusal) => refusal.code,
    }
}

#[test]
fn a_daemon_process_is_live_by_id_start_and_boot_and_says_when_it_started() {
    let run = Path::new("/nowhere");
    let live = daemon(me(), "native", run);
    assert_eq!(process::live(&live), Ok(true));
    let shown = process::shown(&live);
    assert_eq!(shown.get_int("pid"), Some(i64::from(std::process::id())));
    // this test started within the last day, and not in the future
    let started = shown.get_int("started_at").unwrap();
    assert!(
        started <= now() + 2_000 && started > now() - 86_400_000,
        "{started}"
    );
    assert_eq!(process::started_at(me().start), Some(started));

    // a process that ended, and its id under another start time
    let gone = ended();
    let record = daemon(gone, "native", run);
    assert_eq!(process::live(&record), Ok(false));
    let shown = process::shown(&record);
    assert_eq!(shown.get_int("pid"), Some(i64::from(gone.pid)));
    assert_eq!(shown.get("started_at"), Some(&SdkValue::Null), "{shown:?}");
    let reused = LocalProcess {
        start: me().start + 1,
        ..me()
    };
    assert_eq!(process::live(&daemon(reused, "native", run)), Ok(false));
    let rebooted = LocalProcess {
        boot: Some(me().boot.unwrap() ^ 1),
        ..me()
    };
    assert_eq!(process::live(&daemon(rebooted, "native", run)), Ok(false));

    // a record that names no process
    let none = DaemonRecord::from_record(&SdkValue::map([("name", t("lfs-x-s1"))]));
    assert_eq!(process::live(&none), Err("no_pid"));
    assert_eq!(
        layerfs_home::json::print(&process::shown(&none)),
        r#"{"pid":null,"started_at":null}"#
    );
}

#[test]
fn usage_is_a_namespaces_sandbox_s_cgroup_v2_files() {
    let root = scratch("cgroup");
    let id = "ab".repeat(32);
    let host = daemon(me(), "native", &root);
    let spaced = daemon(me(), "namespaces", &root);
    let print = |(usage, reason): (SdkValue, Option<&'static str>)| {
        (layerfs_home::json::print(&usage), reason)
    };
    // a host-process Sandbox has no cgroup, whatever the kernel has
    assert_eq!(
        print(process::usage(&host, &root)),
        ("null".to_owned(), Some("host_process"))
    );
    // a hierarchy that is not cgroup v2 (no `cgroup.controllers`)
    assert_eq!(
        print(process::usage(&spaced, &root)),
        ("null".to_owned(), Some("cgroup_v1"))
    );
    std::fs::write(root.join("cgroup.controllers"), "cpu memory pids\n").unwrap();
    // v2, and the Sandbox's group is not there: it ended
    assert_eq!(
        print(process::usage(&spaced, &root)),
        ("null".to_owned(), Some("no_cgroup"))
    );
    // a group with none of the files: every value null, and no reason
    let group = root.join("layerfs").join(&id);
    std::fs::create_dir_all(&group).unwrap();
    assert_eq!(
        print(process::usage(&spaced, &root)),
        (
            r#"{"cpu_usec":null,"memory_current":null,"memory_peak":null,"pids":null}"#.to_owned(),
            None
        )
    );
    // the files as the kernel writes them; `memory.peak` absent (before 5.19)
    std::fs::write(
        group.join("cpu.stat"),
        "usage_usec 1234567\nuser_usec 1000000\nsystem_usec 234567\n",
    )
    .unwrap();
    std::fs::write(group.join("memory.current"), "52428800\n").unwrap();
    std::fs::write(group.join("pids.current"), "7\n").unwrap();
    assert_eq!(
        print(process::usage(&spaced, &root)),
        (
            r#"{"cpu_usec":1234567,"memory_current":52428800,"memory_peak":null,"pids":7}"#
                .to_owned(),
            None
        )
    );
    std::fs::write(group.join("memory.peak"), "104857600\n").unwrap();
    assert_eq!(
        print(process::usage_in(&root, &id)),
        (
            r#"{"cpu_usec":1234567,"memory_current":52428800,"memory_peak":104857600,"pids":7}"#
                .to_owned(),
            None
        )
    );
    // an id that is no group's name reads nothing outside the hierarchy
    for id in ["", "..", "a/b", "../layerfs"] {
        assert_eq!(process::usage_in(&root, id).1, Some("no_cgroup"), "{id}");
    }
    // what a get shows of this kernel's own hierarchy: a host-process
    // Sandbox has no usage; a namespaces one with no group has none either,
    // and says which of the two reasons applies here
    let fields = |daemon: &DaemonRecord| -> Vec<(String, String)> {
        (process::observed(daemon).into_iter())
            .map(|(field, value)| (field, layerfs_home::json::print(&value)))
            .collect()
    };
    let seen = fields(&host);
    assert_eq!(seen[1], ("usage".to_owned(), "null".to_owned()));
    assert_eq!(
        seen[2],
        ("usage_reason".to_owned(), "\"host_process\"".to_owned())
    );
    let v2 = Path::new(process::CGROUP_ROOT)
        .join("cgroup.controllers")
        .is_file();
    let why = if v2 { "no_cgroup" } else { "cgroup_v1" };
    assert_eq!(
        fields(&spaced)[2],
        ("usage_reason".to_owned(), format!("\"{why}\""))
    );
    assert_eq!(
        layerfs_home::json::print(&SdkValue::Map(process::not_observed())),
        r#"{"daemon":null,"usage":null,"usage_reason":"not_observed"}"#
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn sandboxes_live_checks_each_daemon_record_on_the_side_that_runs_it() {
    let _turn = serial();
    let home = new_home("live");
    let run = paths::run_root(&home);
    sandbox(&home, "up", "native", Some(&daemon(me(), "native", &run)));
    sandbox(
        &home,
        "dead",
        "native",
        Some(&daemon(ended(), "native", &run)),
    );
    sandbox(&home, "new", "native", None);
    let no_pid = DaemonRecord::from_record(&SdkValue::map([("name", t("lfs-x-old"))]));
    sandbox(&home, "old", "native", Some(&no_pid));
    std::fs::create_dir_all(paths::sandbox_dir(&home, "bad")).unwrap();
    std::fs::write(paths::sandbox_record(&home, "bad"), "{ not json").unwrap();

    let asked = ["up", "dead", "new", "nobody", "old", "bad"];
    let request = SdkValue::map([("sandboxes", names(&asked))]);
    let answer = inspect::ask(&home, "native", "sandboxes_live", request).unwrap();
    assert_eq!(
        layerfs_home::json::print(&answer),
        concat!(
            r#"{"sandboxes":[{"name":"up","live":true},{"name":"dead","live":false},"#,
            r#"{"name":"new","live":false},{"name":"nobody","live":false},"#,
            r#"{"name":"old","live":null,"reason":"no_pid"},"#,
            r#"{"name":"bad","live":null,"reason":"record_unreadable"}],"complete":true}"#
        )
    );

    // the home side: one request per machine among the records, in order
    let records: Vec<SandboxRecord> = ["dead", "up", "old"]
        .iter()
        .map(|name| SandboxRecord::read(&home, name).unwrap().unwrap())
        .collect();
    let live = inspect::sandboxes_live(&home, &records);
    let shown: Vec<_> = (live.iter())
        .map(|live| (live.live, live.reason.as_deref()))
        .collect();
    assert_eq!(
        shown,
        [
            (Some(false), None),
            (Some(true), None),
            (None, Some("no_pid"))
        ]
    );
    assert!(inspect::sandboxes_live(&home, &[]).is_empty());

    // what is no list of Sandbox names is refused before anything is read
    for sandboxes in [
        SdkValue::Int(1),
        SdkValue::List(vec![SdkValue::Int(1)]),
        names(&["../up"]),
        SdkValue::List(vec![t("up"); 4097]),
    ] {
        let request = SdkValue::map([("sandboxes", sandboxes)]);
        let asked = inspect::ask(&home, "native", "sandboxes_live", request);
        assert_eq!(refused(asked), "InvalidInput");
    }
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn machine_stats_of_native_is_read_in_this_process_with_no_worker() {
    let _turn = serial();
    let home = new_home("native");
    let run = paths::run_root(&home);
    // Stores: one on this machine with a write-ahead log, one sparse and
    // large, one recorded with no file, one on another machine
    for (store, machine) in [
        ("shop", "native"),
        ("big", "native"),
        ("empty", "native"),
        ("far", "box"),
    ] {
        store_record(&home, store, machine);
    }
    let shop = paths::store_file(&home, "shop");
    std::fs::write(&shop, vec![1u8; 40_960]).unwrap();
    let wal = shop.with_file_name("store.sqlite-wal");
    std::fs::write(&wal, vec![2u8; 8_192]).unwrap();
    // files nobody can open: a size read that opened one would fail
    for file in [&shop, &wal] {
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o000)).unwrap();
    }
    let big = paths::store_file(&home, "big");
    let sparse = std::fs::File::create(&big).unwrap();
    sparse.set_len(3 << 30).unwrap();
    drop(sparse);
    std::fs::write(paths::store_file(&home, "far"), vec![3u8; 4_096]).unwrap();
    // Sandboxes: one live and one dead here, one on another machine
    sandbox(&home, "up", "native", Some(&daemon(me(), "native", &run)));
    sandbox(
        &home,
        "dead",
        "native",
        Some(&daemon(ended(), "native", &run)),
    );
    sandbox(&home, "far", "box", Some(&daemon(me(), "native", &run)));

    let before = now();
    let stats = inspect::machine_stats(&home, &native()).unwrap();
    let SdkValue::Map(fields) = &stats else {
        panic!("{stats:?}")
    };
    let keys: Vec<&str> = fields.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        keys,
        [
            "machine",
            "platform",
            "observed_at",
            "worker",
            "resources",
            "data_disk",
            "stores",
            "sandboxes",
            "complete"
        ]
    );
    assert_eq!(stats.get_text("machine"), Some("native"));
    assert_eq!(stats.get_text("platform"), Some("native"));
    let at = stats.get_int("observed_at").unwrap();
    assert!(at >= before && at <= now(), "{at}");
    assert_eq!(stats.get("worker"), Some(&SdkValue::Null));
    assert_eq!(stats.get("complete"), Some(&SdkValue::Bool(true)));

    // resources: observed here, and the home's filesystem
    let resources = stats.get("resources").unwrap();
    let int = |field: &str| {
        resources
            .get_int(field)
            .unwrap_or_else(|| panic!("{field} in {resources:?}"))
    };
    assert!(int("cpus") >= 1);
    assert!(int("memory_total") > 1 << 20);
    assert!(int("memory_used") >= 0 && int("memory_used") <= int("memory_total"));
    assert!(int("disk_total") > 0 && int("disk_free") > 0);
    assert!(int("disk_used") <= int("disk_total"));
    assert!(int("disk_free") <= int("disk_total"));
    assert_eq!(resources.get_text("scope"), Some("host"));
    assert!(resources.get("reason").is_none(), "{resources:?}");
    let disk = home::filesystem(&home).unwrap();
    assert_eq!(int("disk_total"), disk.total as i64);
    let data_disk = stats.get("data_disk").unwrap();
    let SdkValue::Map(fields) = data_disk else {
        panic!("{data_disk:?}")
    };
    let keys: Vec<&str> = fields.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(keys, ["path", "fs", "scope", "volume", "inodes_free"]);
    assert_eq!(data_disk.get_text("path"), home.to_str());
    assert_eq!(data_disk.get_text("fs"), Some(disk.kind.as_str()));
    assert!(!disk.kind.is_empty());
    assert_eq!(data_disk.get_text("scope"), Some("host_fs"));
    assert_eq!(data_disk.get("volume"), Some(&SdkValue::Null));
    match (disk.inodes_free, data_disk.get_int("inodes_free")) {
        // files come and go between the two reads
        (Some(_), Some(inodes)) => assert!(inodes > 0),
        (None, None) => {}
        other => panic!("{other:?}"),
    }

    // the Stores on this machine, each by `stat` alone
    let allocated = |file: &Path| std::fs::metadata(file).unwrap().blocks() as i64 * 512;
    let stores = listed(&stats, "stores");
    let of = |name: &str| {
        (stores.iter())
            .find(|store| store.get_text("name") == Some(name))
            .unwrap_or_else(|| panic!("{name} in {stores:?}"))
    };
    assert_eq!(stores.len(), 3, "{stores:?}");
    assert_eq!(of("shop").get_int("bytes"), Some(40_960 + 8_192));
    assert_eq!(
        of("shop").get_int("allocated"),
        Some(allocated(&shop) + allocated(&wal))
    );
    assert_eq!(of("big").get_int("bytes"), Some(3 << 30));
    assert_eq!(of("big").get_int("allocated"), Some(allocated(&big)));
    assert!(of("big").get_int("allocated").unwrap() < 1 << 30, "sparse");
    assert_eq!(
        layerfs_home::json::print(of("empty")),
        r#"{"name":"empty","bytes":null,"allocated":null}"#
    );
    // the Sandboxes its records place here: the dead one is recorded, not live
    assert_eq!(
        layerfs_home::json::print(stats.get("sandboxes").unwrap()),
        r#"{"recorded":2,"live":1}"#
    );

    // the request by its kind, as a worker serves it: no scopes, no names
    // the home did not send, and a worker's own identity only on its side
    let request = SdkValue::map([
        ("stores", names(&["shop"])),
        ("sandboxes", names(&["up"])),
        ("worker", SdkValue::map([("pid", SdkValue::Int(1))])),
    ]);
    let raw = inspect::ask(&home, "native", "machine_stats", request.clone()).unwrap();
    assert_eq!(raw.get("worker"), Some(&SdkValue::Null), "{raw:?}");
    assert!(raw.get("resources").unwrap().get("scope").is_none());
    assert_eq!(listed(&raw, "stores").len(), 1);
    let request = home::with_field(request, "kind", t("machine_stats"));
    let served = inspect::admit(move |budget| {
        let root = scratch("served");
        inspect::serve(&Side::Worker(&root), &request, budget)
    })
    .unwrap();
    assert_eq!(
        served.get("worker").unwrap().get_int("pid"),
        Some(1),
        "{served:?}"
    );
    // names that are none are refused before anything is read
    for (field, bad) in [("stores", "../shop"), ("sandboxes", "a/b")] {
        let request = SdkValue::map([(field, names(&[bad]))]);
        let asked = inspect::ask(&home, "native", "machine_stats", request);
        assert_eq!(refused(asked), "InvalidInput", "{field}");
    }
    for file in [&shop, &wal] {
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_request_past_its_soft_budget_answers_what_it_has_and_says_so() {
    let _turn = serial();
    let home = new_home("deadline");
    let run = paths::run_root(&home);
    store_record(&home, "shop", "native");
    std::fs::write(paths::store_file(&home, "shop"), vec![1u8; 4_096]).unwrap();
    sandbox(&home, "up", "native", Some(&daemon(me(), "native", &run)));
    let late = |kind: &'static str| {
        let (root, started) = (home.clone(), Instant::now());
        let answer = inspect::admit(move |budget| {
            // the budget runs out before the parts made of many units
            std::thread::sleep(SOFT + Duration::from_millis(100));
            let request = SdkValue::map([
                ("kind", t(kind)),
                ("stores", names(&["shop"])),
                ("sandboxes", names(&["up"])),
            ]);
            inspect::serve(&Side::Home(&root), &request, budget)
        })
        .unwrap();
        assert!(started.elapsed() < HARD, "{:?}", started.elapsed());
        answer
    };
    let stats = late("machine_stats");
    assert_eq!(stats.get("complete"), Some(&SdkValue::Bool(false)));
    assert_eq!(stats.get_text("reason"), Some("deadline"));
    // the rest is answered
    assert!(
        stats
            .get("resources")
            .unwrap()
            .get_int("disk_total")
            .unwrap()
            > 0
    );
    assert_eq!(
        layerfs_home::json::print(stats.get("stores").unwrap()),
        r#"[{"name":"shop","bytes":null,"allocated":null}]"#
    );
    assert_eq!(
        layerfs_home::json::print(stats.get("sandboxes").unwrap()),
        r#"{"live":null}"#
    );
    assert_eq!(
        layerfs_home::json::print(&late("sandboxes_live")),
        r#"{"sandboxes":[{"name":"up","live":null,"reason":"deadline"}],"complete":false,"reason":"deadline"}"#
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn fifty_sandboxes_and_forty_stores_answer_inside_the_soft_budget() {
    let _turn = serial();
    let home = new_home("many");
    let run = paths::run_root(&home);
    let (up, dead) = (
        daemon(me(), "native", &run),
        daemon(ended(), "native", &run),
    );
    for n in 0..50 {
        let daemon = if n % 5 == 0 { &dead } else { &up };
        sandbox(&home, &format!("s{n}"), "native", Some(daemon));
    }
    for n in 0..40 {
        let store = format!("store{n}");
        store_record(&home, &store, "native");
        let file = std::fs::File::create(paths::store_file(&home, &store)).unwrap();
        file.set_len(1 << 30).unwrap();
    }
    let started = Instant::now();
    let stats = inspect::machine_stats(&home, &native()).unwrap();
    let took = started.elapsed();
    assert!(took < SOFT, "{took:?}");
    assert_eq!(stats.get("complete"), Some(&SdkValue::Bool(true)));
    assert_eq!(listed(&stats, "stores").len(), 40);
    assert!(listed(&stats, "stores")
        .iter()
        .all(|store| store.get_int("bytes") == Some(1 << 30)));
    assert_eq!(
        layerfs_home::json::print(stats.get("sandboxes").unwrap()),
        r#"{"recorded":50,"live":40}"#
    );
    let records = layerfs_sandbox::record::sandboxes(&home).unwrap();
    let started = Instant::now();
    let live = inspect::sandboxes_live(&home, &records);
    let took = started.elapsed();
    assert!(took < SOFT, "{took:?}");
    assert_eq!(live.len(), 50);
    assert_eq!(live.iter().filter(|l| l.live == Some(true)).count(), 40);
    assert_eq!(live.iter().filter(|l| l.live == Some(false)).count(), 10);
    std::fs::remove_dir_all(home).unwrap();
}
