//! Inspect requests in this process (layerfs-project#178 isolation rules 3
//! and 4, the Local route): at most two at once and the third `Busy`, a
//! stuck one answered `Unavailable` at the hard deadline while its helper
//! keeps its permit, a soft budget its work checks between units, a panic
//! answered `Failed`; and what the Local route reads on `native`: a
//! Sandbox's `daemon.log` and its overlay files' sizes, found from the
//! daemon record in the home, with no worker log.
//!
//! The permits are the process's, so every test here takes one lock.
#![cfg(any(target_os = "linux", target_os = "macos"))]
use layerfs_bridge::control::SdkValue;
use layerfs_bridge::process::LocalProcess;
use layerfs_home::home::{self, paths, write_record};
use layerfs_home::names::MachineName;
use layerfs_sandbox::inspect::{self, Window, HARD, PERMITS, SOFT};
use layerfs_sandbox::record::{DaemonRecord, Local};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

static PROCESS: Mutex<()> = Mutex::new(());
static NEXT: AtomicU32 = AtomicU32::new(0);

/// The test's turn, once no earlier test's helper is still running.
fn serial() -> MutexGuard<'static, ()> {
    let guard = PROCESS.lock().unwrap_or_else(|poison| poison.into_inner());
    until(Duration::from_secs(15), "earlier helpers", || {
        inspect::running() == 0
    });
    guard
}

fn until(within: Duration, what: &str, done: impl Fn() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(started.elapsed() < within, "{what}: not within {within:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

/// A new home.
fn new_home(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "lfs-inspect-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    home::private_dir(&dir).unwrap();
    home::ensure(&dir).unwrap();
    dir
}

/// Sandbox `name` of `root` (a home, or a worker's root) as the side that
/// runs it records it, its run folder `directory` named in its daemon
/// record; the folder itself is the caller's to make.
fn sandbox(root: &Path, name: &str, directory: &Path) {
    let me = LocalProcess::observe(std::process::id()).unwrap();
    let daemon = DaemonRecord::local(Local {
        platform: "macos".to_owned(),
        name: directory
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        container: "ab".repeat(32),
        runtime: "native",
        uid: home::flock::euid(),
        gid: home::flock::egid(),
        directory: directory.display().to_string(),
        process: (me.pid, me.start, format!("{:032x}", me.boot.unwrap())),
        port: 1,
        image: "local".to_owned(),
        labels: Vec::new(),
    });
    write_record(
        &paths::sandbox_record(root, name),
        &SdkValue::map([
            ("name", t(name)),
            ("project", t("shop/api")),
            ("machine", t("native")),
            ("id", t(name)),
            ("runtime", t("native")),
            ("daemon", daemon.to_record()),
        ]),
    )
    .unwrap();
}

/// Sandbox `s1` of `home` with its run folder made, holding `log` as its
/// `daemon.log`.
fn running_sandbox(home: &Path, log: &str) -> PathBuf {
    let folder = paths::run_root(home).join("layerfs-0a1b2c3d-s1");
    std::fs::create_dir_all(folder.join("overlay")).unwrap();
    std::fs::write(folder.join("daemon.log"), log).unwrap();
    sandbox(home, "s1", &folder);
    folder
}

fn texts(answer: &SdkValue) -> Vec<String> {
    let Some(SdkValue::List(lines)) = answer.get("lines") else {
        panic!("{answer:?}")
    };
    lines
        .iter()
        .map(|line| line.get_text("text").unwrap().to_owned())
        .collect()
}

fn code(outcome: Result<SdkValue, layerfs_bridge::control::SdkRefusal>) -> String {
    match outcome {
        Ok(value) => format!("ok {value:?}"),
        Err(refusal) => refusal.code,
    }
}

#[test]
fn the_third_request_is_busy_and_a_stuck_one_is_abandoned_at_the_hard_deadline() {
    let _turn = serial();
    assert_eq!((PERMITS, SOFT.as_secs(), HARD.as_secs()), (2, 4, 6));
    let home = new_home("busy");
    running_sandbox(&home, "started\n");
    let window = Window::default();
    assert_eq!(
        texts(&inspect::sandbox_log(&home, "s1", &window).unwrap()),
        ["started"]
    );

    // two reads that do not come back for 7.5 s
    let stuck = Duration::from_millis(7500);
    let (done, answers) = mpsc::channel();
    let started = Instant::now();
    for _ in 0..2 {
        let done = done.clone();
        std::thread::spawn(move || {
            let outcome = inspect::admit(move |_| {
                std::thread::sleep(stuck);
                Ok(SdkValue::Null)
            });
            let _ = done.send((outcome, started.elapsed()));
        });
    }
    until(Duration::from_secs(2), "both admitted", || {
        inspect::running() == 2
    });

    // the third is refused at once, and its work never runs
    let (ran, did_run) = mpsc::channel();
    let asked = Instant::now();
    let third = inspect::admit(move |_| {
        let _ = ran.send(());
        Ok(SdkValue::Null)
    });
    let refusal = third.unwrap_err();
    assert_eq!(refusal.code, "Busy", "{refusal:?}");
    assert!(asked.elapsed() < Duration::from_millis(500));
    assert!(did_run.try_recv().is_err(), "refused work ran");
    // the Local route of a call takes its permit from the same two
    assert_eq!(code(inspect::sandbox_log(&home, "s1", &window)), "Busy");
    let sizes = inspect::overlay_sizes(&home, "native", "s1", &[1]);
    assert_eq!(sizes.unwrap_err().code, "Busy");

    // each caller is answered at the hard deadline, before its work ends
    for _ in 0..2 {
        let (outcome, after) = answers.recv_timeout(Duration::from_secs(8)).unwrap();
        let refusal = outcome.unwrap_err();
        assert_eq!(refusal.code, "Unavailable", "{refusal:?}");
        assert!(refusal.message.contains("abandoned"), "{refusal:?}");
        assert!(after >= HARD && after < stuck, "{after:?}");
    }
    // the abandoned helpers still hold both permits: still `Busy`
    assert_eq!(inspect::running(), 2);
    assert_eq!(code(inspect::sandbox_log(&home, "s1", &window)), "Busy");
    // once they end, requests are admitted again
    until(Duration::from_secs(4), "the helpers' end", || {
        inspect::running() == 0
    });
    assert!(started.elapsed() >= stuck);
    assert_eq!(
        texts(&inspect::sandbox_log(&home, "s1", &window).unwrap()),
        ["started"]
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn work_past_its_soft_budget_answers_what_it_has_so_far() {
    let _turn = serial();
    let started = Instant::now();
    // a walk of many units, as a folder listing is
    let answer = inspect::admit(|budget| {
        let mut entries = 0;
        while !budget.spent() {
            entries += 1;
            std::thread::sleep(Duration::from_millis(50));
        }
        Ok(SdkValue::map([
            ("entries", SdkValue::Int(entries)),
            ("has_more", SdkValue::Bool(true)),
            ("reason", t("deadline")),
        ]))
    })
    .unwrap();
    let after = started.elapsed();
    assert!(after >= SOFT && after < HARD, "{after:?}");
    assert_eq!(answer.get_text("reason"), Some("deadline"));
    assert!(answer.get_int("entries").unwrap() > 40, "{answer:?}");
    assert_eq!(inspect::running(), 0, "the permit is free with the answer");
    // work that ends in time never sees its budget spent
    let quick = inspect::admit(|budget| Ok(SdkValue::Bool(budget.spent()))).unwrap();
    assert_eq!(quick, SdkValue::Bool(false));
}

#[test]
fn work_that_panics_is_failed_and_frees_its_permit() {
    let _turn = serial();
    for _ in 0..3 {
        let refusal = inspect::admit(|_| panic!("a read\nwent wrong")).unwrap_err();
        assert_eq!(refusal.code, "Failed", "{refusal:?}");
        assert_eq!(
            refusal.message,
            "an inspect request panicked and changed nothing: a read went wrong"
        );
        assert_eq!(inspect::running(), 0);
    }
    assert_eq!(
        inspect::admit(|_| Ok(SdkValue::Int(7))).unwrap(),
        SdkValue::Int(7)
    );
}

#[test]
fn the_local_route_reads_a_native_sandbox_s_log_from_its_recorded_run_folder() {
    let _turn = serial();
    let home = new_home("log");
    let folder = running_sandbox(&home, "one\ntwo\n");
    let first = inspect::sandbox_log(&home, "s1", &Window::default()).unwrap();
    assert_eq!(texts(&first), ["one", "two"]);
    assert_eq!(first.get("has_more"), Some(&SdkValue::Bool(false)));
    // followed by cursor
    std::fs::write(folder.join("daemon.log"), "one\ntwo\nthree\n").unwrap();
    let cursor = first.get_text("cursor").unwrap().to_owned();
    let window = Window {
        cursor: Some(cursor.clone()),
        ..Window::default()
    };
    assert_eq!(
        texts(&inspect::sandbox_log(&home, "s1", &window).unwrap()),
        ["three"]
    );
    // a window is checked before the Sandbox is looked up
    for window in [
        Window {
            limit: Some(0),
            ..Window::default()
        },
        Window {
            cursor: Some(cursor.clone()),
            before: Some(cursor.clone()),
            limit: None,
        },
    ] {
        assert_eq!(
            code(inspect::sandbox_log(&home, "nobody", &window)),
            "InvalidInput"
        );
    }
    // the Sandbox recreated: a new file, and the old cursor is stale
    std::fs::rename(folder.join("daemon.log"), folder.join("kept")).unwrap();
    std::fs::write(folder.join("daemon.log"), "again\n").unwrap();
    let stale = inspect::sandbox_log(&home, "s1", &window).unwrap_err();
    assert_eq!(stale.code, "InvalidInput", "{stale:?}");
    assert!(stale
        .message
        .starts_with("logs.sandbox: the cursor is stale"));

    // a native machine has no worker, so no worker log
    let native = MachineName::parse("native").unwrap();
    let none = inspect::machine_log(&home, &native, &Window::default()).unwrap_err();
    assert_eq!(none.code, "Unsupported", "{none:?}");
    assert_eq!(none.detail.get_text("reason"), Some("no_worker"));
    // and a home is not asked for one, whatever names the kind
    let asked = inspect::ask(&home, "native", "machine_log", SdkValue::map([]));
    assert_eq!(asked.unwrap_err().code, "Unsupported");
    let unknown = inspect::ask(&home, "native", "nothing", SdkValue::map([]));
    assert_eq!(unknown.unwrap_err().code, "InvalidInput");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_sandbox_with_no_run_folder_on_this_side_is_no_such_sandbox() {
    let _turn = serial();
    let home = new_home("lost");
    let window = Window::default();
    // no record
    let none = inspect::sandbox_log(&home, "s1", &window).unwrap_err();
    assert_eq!(none.code, "NoSuchSandbox", "{none:?}");
    // a record with no daemon record: created, never started
    write_record(
        &paths::sandbox_record(&home, "s1"),
        &SdkValue::map([
            ("name", t("s1")),
            ("project", t("shop/api")),
            ("machine", t("native")),
        ]),
    )
    .unwrap();
    assert_eq!(
        code(inspect::sandbox_log(&home, "s1", &window)),
        "NoSuchSandbox"
    );
    // a daemon record whose run folder is gone
    let folder = paths::run_root(&home).join("layerfs-0a1b2c3d-s1");
    sandbox(&home, "s1", &folder);
    let gone = inspect::sandbox_log(&home, "s1", &window).unwrap_err();
    assert_eq!(gone.code, "NoSuchSandbox", "{gone:?}");
    assert_eq!(gone.message, "no Sandbox s1 runs on this machine");
    // a run folder with no log yet: an empty log
    std::fs::create_dir_all(&folder).unwrap();
    let empty = inspect::sandbox_log(&home, "s1", &window).unwrap();
    assert!(texts(&empty).is_empty());
    // the log is read from the folder alone: whether the daemon the record
    // names still runs is never asked, so one that ended leaves it readable
    std::fs::write(folder.join("daemon.log"), "last words\n").unwrap();
    assert_eq!(
        texts(&inspect::sandbox_log(&home, "s1", &window).unwrap()),
        ["last words"]
    );

    // a record that names a folder outside the run root is never read
    let outside = home.join("elsewhere");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("daemon.log"), "not a Sandbox's\n").unwrap();
    for directory in [
        outside.clone(),
        paths::run_root(&home),
        paths::run_root(&home).join("a").join(".."),
        PathBuf::from("/"),
    ] {
        sandbox(&home, "s1", &directory.join("x"));
        // `sandbox` names `<directory>/x`; put the directory itself
        let path = paths::sandbox_record(&home, "s1");
        let record = home::read_record(&path).unwrap().unwrap();
        let daemon = record.get("daemon").unwrap().clone();
        let daemon = home::with_field(daemon, "directory", t(&directory.display().to_string()));
        write_record(&path, &home::with_field(record, "daemon", daemon)).unwrap();
        let refused = inspect::sandbox_log(&home, "s1", &window).unwrap_err();
        assert_eq!(refused.code, "InvalidInput", "{directory:?}: {refused:?}");
        assert!(refused.message.contains("outside"), "{refused:?}");
    }
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn overlay_sizes_are_read_by_stat_without_opening_a_file() {
    let _turn = serial();
    let home = new_home("overlay");
    let folder = running_sandbox(&home, "");
    let overlay = folder.join("overlay");
    // files this process cannot open: a size read that opened one would fail
    for (name, bytes) in [
        ("overlay.sqlite", 4096),
        ("1.sqlite", 8192),
        ("3.sqlite", 0),
    ] {
        let path = overlay.join(name);
        std::fs::write(&path, vec![7u8; bytes]).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
    }
    // a link is not a Workspace's overlay file, whatever it points at
    std::os::unix::fs::symlink(overlay.join("1.sqlite"), overlay.join("4.sqlite")).unwrap();
    let asked = [1, 2, 3, 4];
    // the answer, as a worker sends it and the home side answers it: null
    // for a Workspace with no overlay file (2) and for the link (4)
    let sizes = inspect::overlay_sizes(&home, "native", "s1", &asked).unwrap();
    // with them, what this side sees of the Sandbox's daemon process
    // (layerfs-project#164): the record names this test, a host process,
    // which has no cgroup for its Workspaces either
    let started = sizes.get("daemon").unwrap().get_int("started_at").unwrap();
    assert_eq!(
        layerfs_home::json::print(&sizes),
        format!(
            concat!(
                r#"{{"overlay_sizes":[{{"namespace":1,"bytes":8192}},{{"namespace":2,"bytes":null}},"#,
                r#"{{"namespace":3,"bytes":0}},{{"namespace":4,"bytes":null}}],"#,
                r#""daemon":{{"pid":{},"started_at":{}}},"usage":null,"usage_reason":"host_process","#,
                r#""workspace_usage":null,"workspace_usage_reason":"host_process"}}"#
            ),
            std::process::id(),
            started
        )
    );
    // a Sandbox with no Workspace is asked for its process alone
    let none = inspect::overlay_sizes(&home, "native", "s1", &[]).unwrap();
    assert_eq!(none.get("overlay_sizes"), Some(&SdkValue::List(Vec::new())));
    assert_eq!(none.get("daemon"), sizes.get("daemon"));
    // the request by its kind is the same read
    let request = SdkValue::map([
        ("sandbox", t("s1")),
        (
            "namespaces",
            SdkValue::List(asked.iter().map(|n| SdkValue::Int(*n)).collect()),
        ),
    ]);
    let answer = inspect::ask(&home, "native", "overlay_sizes", request).unwrap();
    assert_eq!(answer, sizes);
    // what is not a list of namespaces is refused, and so is a lost Sandbox
    for namespaces in [
        SdkValue::Null,
        SdkValue::List(vec![t("1")]),
        SdkValue::List(vec![SdkValue::Int(-1)]),
    ] {
        let request = SdkValue::map([("sandbox", t("s1")), ("namespaces", namespaces)]);
        let refused = inspect::ask(&home, "native", "overlay_sizes", request);
        assert_eq!(refused.unwrap_err().code, "InvalidInput");
    }
    let lost = inspect::overlay_sizes(&home, "native", "s2", &asked).unwrap_err();
    assert_eq!(lost.code, "NoSuchSandbox", "{lost:?}");
    for name in ["overlay.sqlite", "1.sqlite", "3.sqlite"] {
        let path = overlay.join(name);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    std::fs::remove_dir_all(home).unwrap();
}
