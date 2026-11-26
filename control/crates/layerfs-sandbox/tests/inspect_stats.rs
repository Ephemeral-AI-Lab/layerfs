//! The reads of layerfs-project#164 on the Local route (a `native`
//! machine, in this process) and by themselves: a Sandbox's daemon process
//! (`inspect::process`: live or gone by id, start time and boot; when it
//! started; its cgroup's usage from a cgroup v2 folder given as an
//! argument), the inspect kind `machine_stats`
//! (processors, memory, the data disk, Store sizes by `stat`, live
//! Sandboxes), the processor counters and boot time of `/proc/stat`, each
//! Sandbox's row (layerfs-project#182 L2), what a request past its soft
//! budget answers, and many Sandboxes and Stores inside it.
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
use layerfs_sandbox::inspect::process::KernelStat;
use layerfs_sandbox::inspect::{self, process, Side, HARD, SOFT};
use layerfs_sandbox::record::{DaemonRecord, Local};
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
        network: None,
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
        ("runtime", t("process")),
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
            ("profile", t("normal")),
            ("machine", t(machine)),
        ]),
    )
    .unwrap();
}

fn native() -> MachineName {
    MachineName::parse(layerfs_home::names::HOST_MACHINE).unwrap()
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

/// This kernel's boot time, as one read of `/proc/stat` gives it.
fn boot() -> Option<u64> {
    KernelStat::read().boot
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
    let live = daemon(me(), "process", run);
    assert_eq!(process::live(&live), Ok(true));
    let shown = process::shown(&live, boot());
    assert_eq!(shown.get_int("pid"), Some(i64::from(std::process::id())));
    // this test started within the last day, and not in the future
    let started = shown.get_int("started_at").unwrap();
    assert!(
        started <= now() + 2_000 && started > now() - 86_400_000,
        "{started}"
    );
    assert_eq!(process::started_at(me().start, boot()), Some(started));
    assert_eq!(process::start_of(&live, boot()), Some(started));

    // a process that ended, and its id under another start time
    let gone = ended();
    let record = daemon(gone, layerfs_home::names::HOST_MACHINE, run);
    assert_eq!(process::live(&record), Ok(false));
    let shown = process::shown(&record, boot());
    assert_eq!(shown.get_int("pid"), Some(i64::from(gone.pid)));
    assert_eq!(shown.get("started_at"), Some(&SdkValue::Null), "{shown:?}");
    let reused = LocalProcess {
        start: me().start + 1,
        ..me()
    };
    assert_eq!(process::live(&daemon(reused, "process", run)), Ok(false));
    let rebooted = LocalProcess {
        boot: Some(me().boot.unwrap() ^ 1),
        ..me()
    };
    assert_eq!(process::live(&daemon(rebooted, "process", run)), Ok(false));

    // a record that names no process
    let none = DaemonRecord::from_record(&SdkValue::map([("name", t("lfs-x-s1"))]));
    assert_eq!(process::live(&none), Err("no_pid"));
    assert_eq!(
        layerfs_home::json::print(&process::shown(&none, boot())),
        r#"{"pid":null,"started_at":null}"#
    );
}

#[test]
fn usage_is_a_namespaces_sandbox_s_cgroup_v2_files() {
    let root = scratch("cgroup");
    let id = "ab".repeat(32);
    let host = daemon(me(), "process", &root);
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
    // a group with none of the files: every value null, and the reason
    // names the controllers it lacks
    let group = root.join("layerfs").join(&id);
    std::fs::create_dir_all(&group).unwrap();
    assert_eq!(
        print(process::usage(&spaced, &root)),
        (
            r#"{"cpu_usec":null,"memory_current":null,"memory_peak":null,"memory_file":null,"pids":null,"oom_kills":null}"#.to_owned(),
            Some("no_controller")
        )
    );
    // the files as the kernel writes them; `memory.peak` absent (before 5.19)
    std::fs::write(
        group.join("cpu.stat"),
        "usage_usec 1234567\nuser_usec 1000000\nsystem_usec 234567\n",
    )
    .unwrap();
    // as on a machine whose `layerfs` group enables no controller for its
    // children: the processor time is there, memory and processes are not
    assert_eq!(
        print(process::usage(&spaced, &root)),
        (
            r#"{"cpu_usec":1234567,"memory_current":null,"memory_peak":null,"memory_file":null,"pids":null,"oom_kills":null}"#
                .to_owned(),
            Some("no_controller")
        )
    );
    std::fs::write(group.join("memory.current"), "52428800\n").unwrap();
    std::fs::write(group.join("pids.current"), "7\n").unwrap();
    assert_eq!(
        print(process::usage(&spaced, &root)),
        (
            r#"{"cpu_usec":1234567,"memory_current":52428800,"memory_peak":null,"memory_file":null,"pids":7,"oom_kills":null}"#
                .to_owned(),
            None
        )
    );
    std::fs::write(group.join("memory.peak"), "104857600\n").unwrap();
    // `memory.stat` and `memory.events` as the kernel writes them: the
    // values are their `file` and `oom_kill` lines, not a longer key that
    // starts the same (`file_mapped`, `oom_kill_disable`-like names)
    std::fs::write(
        group.join("memory.stat"),
        "anon 1000\nfile_mapped 99\nfile 8388608\nkernel 12\n",
    )
    .unwrap();
    std::fs::write(
        group.join("memory.events"),
        "low 0\nhigh 0\nmax 3\noom 1\noom_kill 1\noom_group_kill 0\n",
    )
    .unwrap();
    assert_eq!(
        print(process::usage_in(&root, &id)),
        (
            r#"{"cpu_usec":1234567,"memory_current":52428800,"memory_peak":104857600,"memory_file":8388608,"pids":7,"oom_kills":1}"#
                .to_owned(),
            None
        )
    );
    // as `machine.stats` and `sandbox.get --with stats` name it (UI pass 1
    // FAIL 2: `machine.stats` answered the read's own names)
    assert_eq!(
        layerfs_home::json::print(&process::shown_usage(&process::usage_in(&root, &id).0)),
        r#"{"cpu_ms":1234,"memory_bytes":52428800,"memory_peak_bytes":104857600,"memory_file_bytes":8388608,"pid_count":7,"oom_kill_count":1}"#
    );
    assert_eq!(process::shown_usage(&SdkValue::Null), SdkValue::Null);
    // a runtime that is neither `namespaces` nor the host-process one is
    // not taken for a host process
    for runtime in ["firecracker", ""] {
        let other = daemon(me(), runtime, &root);
        assert_eq!(
            print(process::usage(&other, &root)),
            ("null".to_owned(), Some("not_supported")),
            "{runtime}"
        );
    }
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
fn machine_stats_checks_each_daemon_record_on_the_side_that_runs_it() {
    let _turn = serial();
    let home = new_home("live");
    let run = paths::run_root(&home);
    sandbox(
        &home,
        "up",
        layerfs_home::names::HOST_MACHINE,
        Some(&daemon(me(), "process", &run)),
    );
    sandbox(
        &home,
        "dead",
        layerfs_home::names::HOST_MACHINE,
        Some(&daemon(ended(), "process", &run)),
    );
    sandbox(&home, "new", layerfs_home::names::HOST_MACHINE, None);
    let no_pid = DaemonRecord::from_record(&SdkValue::map([("name", t("lfs-x-old"))]));
    sandbox(
        &home,
        "old",
        layerfs_home::names::HOST_MACHINE,
        Some(&no_pid),
    );
    std::fs::create_dir_all(paths::sandbox_dir(&home, "bad")).unwrap();
    std::fs::write(paths::sandbox_record(&home, "bad"), "{ not json").unwrap();

    let asked = ["up", "dead", "new", "nobody", "old", "bad"];
    let request = SdkValue::map([("sandboxes", names(&asked))]);
    let answer = inspect::ask(
        &home,
        layerfs_home::names::HOST_MACHINE,
        "machine_stats",
        request,
    )
    .unwrap();
    let items = listed(answer.get("sandboxes").unwrap(), "items");
    let shown: Vec<_> = (items.iter())
        .map(|item| {
            (
                item.get_text("name").unwrap(),
                item.get("live").cloned().unwrap_or_default(),
                item.get_text("live_reason"),
            )
        })
        .collect();
    assert_eq!(
        shown,
        [
            ("up", SdkValue::Bool(true), None),
            ("dead", SdkValue::Bool(false), None),
            ("new", SdkValue::Bool(false), None),
            ("nobody", SdkValue::Bool(false), None),
            ("old", SdkValue::Null, Some("no_pid")),
            ("bad", SdkValue::Null, Some("record_unreadable")),
        ]
    );

    // what is no list of Sandbox names is refused before anything is read
    for sandboxes in [
        SdkValue::Int(1),
        SdkValue::List(vec![SdkValue::Int(1)]),
        names(&["../up"]),
        SdkValue::List(vec![t("up"); 4097]),
    ] {
        let request = SdkValue::map([("sandboxes", sandboxes)]);
        let asked = inspect::ask(
            &home,
            layerfs_home::names::HOST_MACHINE,
            "machine_stats",
            request,
        );
        assert_eq!(refused(asked), "InvalidInput");
    }
    // the kind `sandbox.list`'s live option asked is gone with it
    let request = SdkValue::map([("sandboxes", names(&["up"]))]);
    let asked = inspect::ask(
        &home,
        layerfs_home::names::HOST_MACHINE,
        "sandboxes_live",
        request,
    );
    assert_eq!(refused(asked), "InvalidInput");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn machine_stats_of_native_is_read_in_this_process_with_no_worker() {
    let _turn = serial();
    let home = new_home(layerfs_home::names::HOST_MACHINE);
    let run = paths::run_root(&home);
    // Stores: one on this machine with a write-ahead log, one sparse and
    // large, one recorded with no file, one on another machine
    for (store, machine) in [
        ("shop", layerfs_home::names::HOST_MACHINE),
        ("big", layerfs_home::names::HOST_MACHINE),
        ("empty", layerfs_home::names::HOST_MACHINE),
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
    sandbox(
        &home,
        "up",
        layerfs_home::names::HOST_MACHINE,
        Some(&daemon(me(), "process", &run)),
    );
    sandbox(
        &home,
        "dead",
        layerfs_home::names::HOST_MACHINE,
        Some(&daemon(ended(), "process", &run)),
    );
    sandbox(&home, "far", "box", Some(&daemon(me(), "process", &run)));

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
    assert_eq!(
        stats.get_text("machine"),
        Some(layerfs_home::names::HOST_MACHINE)
    );
    assert_eq!(stats.get_text("platform"), Some("host"));
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
    let SdkValue::Map(fields) = resources else {
        panic!("{resources:?}")
    };
    let keys: Vec<&str> = fields.iter().map(|(name, _)| name.as_str()).collect();
    let mut want = vec![
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
    if cfg!(target_os = "linux") {
        // the machine's processor time since boot, busy within all of it
        assert!(int("cpu_busy_usec") > 0);
        assert!(int("cpu_busy_usec") <= int("cpu_total_usec"));
        assert!(resources.get("reason").is_none(), "{resources:?}");
    } else {
        // macOS has no `/proc/stat`: null, and the reason word says so
        assert_eq!(resources.get("cpu_busy_usec"), Some(&SdkValue::Null));
        assert_eq!(resources.get("cpu_total_usec"), Some(&SdkValue::Null));
        assert_eq!(resources.get_text("reason"), Some("cpu_unsupported"));
        want.push("reason");
    }
    assert_eq!(keys, want);
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
    // the Sandboxes its records place here: the dead one is recorded, not
    // live; each has its row, in the records' order, and a host-process
    // Sandbox has no cgroup usage
    let sandboxes = stats.get("sandboxes").unwrap();
    assert_eq!(sandboxes.get_int("recorded"), Some(2));
    assert_eq!(sandboxes.get_int("live"), Some(1));
    let started = process::started_at(me().start, boot()).unwrap();
    assert_eq!(
        layerfs_home::json::print(sandboxes.get("items").unwrap()),
        format!(
            concat!(
                r#"[{{"name":"dead","live":false,"started_at":null,"usage":null,"usage_reason":"host_process"}},"#,
                r#"{{"name":"up","live":true,"started_at":{},"usage":null,"usage_reason":"host_process"}}]"#
            ),
            started
        )
    );

    // the request by its kind, as a worker serves it: no scopes, no names
    // the home did not send, and a worker's own identity only on its side
    let request = SdkValue::map([
        ("stores", names(&["shop"])),
        ("sandboxes", names(&["up"])),
        ("worker", SdkValue::map([("pid", SdkValue::Int(1))])),
    ]);
    let raw = inspect::ask(
        &home,
        layerfs_home::names::HOST_MACHINE,
        "machine_stats",
        request.clone(),
    )
    .unwrap();
    assert_eq!(raw.get("worker"), Some(&SdkValue::Null), "{raw:?}");
    assert!(raw.get("resources").unwrap().get("scope").is_none());
    assert_eq!(listed(&raw, "stores").len(), 1);
    let request = home::with_field(request, "kind", t("machine_stats"));
    let served = inspect::admit(move |budget| {
        let root = scratch("served");
        home::guest::serve_root(&root);
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
        let asked = inspect::ask(
            &home,
            layerfs_home::names::HOST_MACHINE,
            "machine_stats",
            request,
        );
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
    store_record(&home, "shop", layerfs_home::names::HOST_MACHINE);
    std::fs::write(paths::store_file(&home, "shop"), vec![1u8; 4_096]).unwrap();
    sandbox(
        &home,
        "up",
        layerfs_home::names::HOST_MACHINE,
        Some(&daemon(me(), "process", &run)),
    );
    let late = |kind: &'static str| {
        let (root, started) = (home.clone(), Instant::now());
        let answer = inspect::admit(move |budget| {
            // the budget runs out before the parts made of many units
            std::thread::sleep(SOFT + Duration::from_millis(100));
            let request = SdkValue::map([
                ("kind", t(kind)),
                ("stores", names(&["shop"])),
                ("sandboxes", names(&["up", "s2", "s3"])),
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
    // the Sandbox the budget did not reach keeps its row, null with why
    assert_eq!(
        layerfs_home::json::print(stats.get("sandboxes").unwrap()),
        format!(
            r#"{{"live":null,"items":[{}]}}"#,
            ["up", "s2", "s3"]
                .map(|name| format!(
                    r#"{{"name":"{name}","live":null,"live_reason":"deadline","started_at":null,"usage":null,"usage_reason":"deadline"}}"#
                ))
                .join(",")
        )
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn fifty_sandboxes_and_forty_stores_answer_inside_the_soft_budget() {
    let _turn = serial();
    let home = new_home("many");
    let run = paths::run_root(&home);
    let (up, dead) = (
        daemon(me(), "process", &run),
        daemon(ended(), "process", &run),
    );
    for n in 0..50 {
        let daemon = if n % 5 == 0 { &dead } else { &up };
        sandbox(
            &home,
            &format!("s{n}"),
            layerfs_home::names::HOST_MACHINE,
            Some(daemon),
        );
    }
    for n in 0..40 {
        let store = format!("store{n}");
        store_record(&home, &store, layerfs_home::names::HOST_MACHINE);
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
    let sandboxes = stats.get("sandboxes").unwrap();
    assert_eq!(sandboxes.get_int("recorded"), Some(50));
    assert_eq!(sandboxes.get_int("live"), Some(40));
    let items = listed(sandboxes, "items");
    assert_eq!(items.len(), 50);
    assert_eq!(
        (items.iter())
            .filter(|item| item.get("live") == Some(&SdkValue::Bool(true)))
            .count(),
        40
    );
    assert!(items.iter().all(|item| item.get("live_reason").is_none()));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn proc_stat_gives_processor_counters_in_microseconds_and_the_boot_time() {
    // as a kernel writes it: user nice system idle iowait irq softirq steal
    // guest guest_nice, in ticks of 1/100 s
    let text = concat!(
        "cpu  100 20 30 1000 50 5 6 7 40 2\n",
        "cpu0 50 10 15 500 25 2 3 3 20 1\n",
        "intr 12345\n",
        "btime 1763251200\n",
        "processes 99\n",
    );
    let stat = KernelStat::parse(text);
    // busy = user + nice + system + irq + softirq + steal (guest is inside
    // user and nice already); total = busy + idle + iowait
    assert_eq!(stat.busy_usec, Some((100 + 20 + 30 + 5 + 6 + 7) * 10_000));
    assert_eq!(stat.total_usec, Some((168 + 1000 + 50) * 10_000));
    assert_eq!(stat.boot, Some(1_763_251_200));
    // a line too short or not numbers: no counters, the boot time still read
    for line in [
        "cpu  1 2 3 4 5 6 7\n",
        "cpu  1 2 x 4 5 6 7 8\n",
        "cpu0 1 2 3 4 5 6 7 8\n",
    ] {
        let stat = KernelStat::parse(&format!("{line}btime 7\n"));
        assert_eq!(
            (stat.busy_usec, stat.total_usec, stat.boot),
            (None, None, Some(7)),
            "{line}"
        );
    }
    assert_eq!(KernelStat::parse(""), KernelStat::default());
    // a start in ticks since boot, placed by the boot time read with it
    #[cfg(target_os = "linux")]
    {
        assert_eq!(process::started_at(250, Some(1_000)), Some(1_002_500));
        assert_eq!(process::started_at(250, None), None);
    }
    // this kernel's own file, read once
    let here = KernelStat::read();
    if cfg!(target_os = "linux") {
        assert!(here.busy_usec.unwrap() <= here.total_usec.unwrap());
        assert!(here.boot.unwrap() > 1_000_000_000);
    } else {
        assert_eq!(here, KernelStat::default());
    }
}

#[test]
fn a_worker_s_rows_read_each_sandbox_s_record_on_its_side() {
    let _turn = serial();
    // a worker's root: a namespaces Sandbox, one whose record names no
    // daemon, one whose record is not readable, one not recorded there
    let root = scratch("rows");
    // a worker's root: its run root is the guest's (#819 R)
    home::guest::serve_root(&root);
    let run = paths::run_root(&root);
    assert_eq!(run, root.join("machine/linux-guest/run"));
    sandbox(&root, "ns", "box", Some(&daemon(me(), "namespaces", &run)));
    sandbox(&root, "fresh", "box", None);
    let bad = paths::sandbox_record(&root, "bad");
    std::fs::create_dir_all(bad.parent().unwrap()).unwrap();
    std::fs::write(&bad, b"not a record").unwrap();
    let request = SdkValue::map([
        ("kind", t("machine_stats")),
        ("sandboxes", names(&["ns", "fresh", "bad", "gone"])),
    ]);
    let served = {
        let root = root.clone();
        inspect::admit(move |budget| inspect::serve(&Side::Worker(&root), &request, budget))
            .unwrap()
    };
    let rows = listed(served.get("sandboxes").unwrap(), "items");
    let row = |at: usize| layerfs_home::json::print(&rows[at]);
    // the namespaces Sandbox's usage is its cgroup on this kernel: none on
    // macOS (no v2 hierarchy), and no group of that id on a Linux runner
    let v2 = Path::new(process::CGROUP_ROOT)
        .join("cgroup.controllers")
        .is_file();
    let why = if v2 { "no_cgroup" } else { "cgroup_v1" };
    let started = process::started_at(me().start, boot()).unwrap();
    assert_eq!(
        row(0),
        format!(
            r#"{{"name":"ns","live":true,"started_at":{started},"usage":null,"usage_reason":"{why}"}}"#
        )
    );
    assert_eq!(
        row(1),
        r#"{"name":"fresh","live":false,"started_at":null,"usage":null,"usage_reason":"not_observed"}"#
    );
    assert_eq!(
        row(2),
        concat!(
            r#"{"name":"bad","live":null,"live_reason":"record_unreadable","#,
            r#""started_at":null,"usage":null,"usage_reason":"record_unreadable"}"#
        )
    );
    assert_eq!(
        row(3),
        r#"{"name":"gone","live":false,"started_at":null,"usage":null,"usage_reason":"not_observed"}"#
    );
    assert_eq!(served.get("sandboxes").unwrap().get_int("live"), Some(1));
    std::fs::remove_dir_all(root).unwrap();
}
