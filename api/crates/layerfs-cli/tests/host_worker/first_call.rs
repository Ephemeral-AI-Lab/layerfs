#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]
//! The first CLI call with no ready host worker (#833): it runs on its
//! direct route, chosen before anything is sent, and starts the worker in
//! the background for the next call, once however many callers race.

use crate::fixture::Fixture;
use crate::proof;
use layerfs_api::home::paths;
use layerfs_api::names::HOST_MACHINE;
use layerfs_protocol::sdk::SdkValue;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const CLI: &str = env!("CARGO_BIN_EXE_layerfs");
const READ: &[&str] = &["file", "read", "shop/api", "main", "/hello"];
const TEXT: &str = "the host worker proof\n";

/// One CLI call of `words` on `home`, its route lines appended to `trace`.
fn traced(home: &Path, trace: &Path, words: &[&str]) -> Output {
    Command::new(CLI)
        .args(words)
        .env("LAYERFS_ROOT", crate::root::root_of(home))
        .env("LAYERFS_TRACE", trace)
        .env_remove("LAYERFS_TRACE_STEP")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn read_ok(home: &Path, trace: &Path) {
    let out = traced(home, trace, READ);
    assert!(out.status.success(), "{out:?}");
    let value = layerfs_api::json::parse(std::str::from_utf8(&out.stdout).unwrap()).unwrap();
    assert_eq!(value.get_text("text"), Some(TEXT), "{value:?}");
}

/// The trace's lines of `event`.
fn events(trace: &Path, event: &str) -> Vec<SdkValue> {
    std::fs::read_to_string(trace)
        .unwrap_or_default()
        .lines()
        .map(|line| layerfs_api::json::parse(line).unwrap())
        .filter(|line| line.get_text("event") == Some(event))
        .collect()
}

/// The worker's ping, once its management endpoint answers.
fn ready(home: &Fixture) -> SdkValue {
    proof::wait_for("the background-started worker", || {
        std::os::unix::net::UnixStream::connect(&home.paths.control)
            .ok()
            .map(|_| ())
    });
    home.ping()
}

#[test]
fn the_first_call_runs_direct_and_the_next_one_reaches_the_started_worker() {
    let home = proof::create();
    let trace = home.paths.home.join("first.jsonl");
    read_ok(&home.paths.home, &trace);
    let routes = events(&trace, "worker.route");
    assert_eq!(routes.len(), 1, "{routes:?}");
    assert_eq!(routes[0].get_text("route"), Some("direct"));
    assert_eq!(routes[0].get_text("reason"), Some("worker started"));
    assert_eq!(routes[0].get_text("op"), Some("store.call"));
    assert!(events(&trace, "worker.connection").is_empty());

    // the worker it started becomes ready without another call, having
    // served none, and keeps no caller's startup lock
    let first = ready(&home);
    assert_eq!(first.get_int("requests"), Some(0), "{first:?}");
    assert_eq!(first.get_int("stores"), Some(0), "{first:?}");
    let startup = paths::host_worker_startup_lock(&home.paths.home);
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(startup)
        .unwrap();
    proof::wait_for("the startup lock's release", || {
        layerfs_api::home::flock::try_lock(&lock, true)
            .unwrap()
            .then_some(())
    });
    assert!(layerfs_api::worker::start_failure(&home.paths.home).is_none());
    assert!(!paths::host_worker_start_pending(&home.paths.home).exists());

    let next = home.paths.home.join("next.jsonl");
    read_ok(&home.paths.home, &next);
    assert!(events(&next, "worker.route").is_empty());
    let connections = events(&next, "worker.connection");
    assert_eq!(connections.len(), 1, "{connections:?}");
    assert!(connections[0].get_bool("sent"));
    let after = home.ping();
    assert_eq!(after.get_int("pid"), first.get_int("pid"));
    // the routed read opened the Store there
    assert_eq!(after.get_int("stores"), Some(1), "{after:?}");
    home.drain();
}

#[test]
fn concurrent_first_calls_start_one_worker() {
    let home = proof::create();
    let traces: Vec<_> = (0..8)
        .map(|index| home.paths.home.join(format!("race-{index}.jsonl")))
        .collect();
    let calls: Vec<_> = traces
        .iter()
        .map(|trace| {
            Command::new(CLI)
                .args(READ)
                .env("LAYERFS_ROOT", crate::root::root_of(&home.paths.home))
                .env("LAYERFS_TRACE", trace)
                .env_remove("LAYERFS_TRACE_STEP")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    for call in calls {
        let out = call.wait_with_output().unwrap();
        assert!(out.status.success(), "{out:?}");
    }
    let reasons: Vec<String> = traces
        .iter()
        .flat_map(|trace| events(trace, "worker.route"))
        .map(|line| line.get_text("reason").unwrap().to_owned())
        .collect();
    let started = reasons.iter().filter(|r| *r == "worker started").count();
    assert_eq!(started, 1, "{reasons:?}");
    assert!(
        reasons
            .iter()
            .all(|r| matches!(r.as_str(), "worker started" | "worker starting")),
        "{reasons:?}"
    );
    let ping = ready(&home);
    // one worker ran: a second would have logged that another owns the home
    let log = std::fs::read_to_string(paths::worker_log(&home.paths.home)).unwrap_or_default();
    assert!(!log.contains("another host worker owns"), "{log}");
    assert!(ping.get_int("pid").is_some());
    home.drain();
}

#[test]
fn a_failed_background_start_keeps_the_direct_answer_and_is_reported() {
    let home = proof::create();
    // the worker's log cannot be opened, so the start fails at spawn
    let log = paths::worker_log(&home.paths.home);
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&log)
        .unwrap();
    let trace = home.paths.home.join("failed.jsonl");
    read_ok(&home.paths.home, &trace);
    let routes = events(&trace, "worker.route");
    assert_eq!(routes.len(), 1, "{routes:?}");
    let reason = routes[0].get_text("reason").unwrap();
    assert!(reason.starts_with("worker start failed: "), "{reason}");
    let record = layerfs_api::worker::start_failure(&home.paths.home).expect("a record");
    assert_eq!(record.get_text("by"), Some("caller"), "{record:?}");
    assert_eq!(record.get_text("launch"), Some("background"), "{record:?}");
    assert!(!paths::host_worker_start_pending(&home.paths.home).exists());
    assert!(
        record.get_text("message").unwrap().contains("worker log"),
        "{record:?}"
    );
    assert!(!home.paths.socket.exists());

    // doctor names it while no worker listens
    let out = traced(
        &home.paths.home,
        &trace,
        &["machine", "doctor", HOST_MACHINE, "--detail"],
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("the last background start of the host worker failed"),
        "{text}"
    );
    assert!(text.contains("start_failure"), "{text}");

    // once the start can succeed, the next call starts the worker, which
    // removes the record when its endpoints are published
    std::fs::remove_dir(&log).unwrap();
    read_ok(&home.paths.home, &trace);
    ready(&home);
    assert!(layerfs_api::worker::start_failure(&home.paths.home).is_none());
    home.drain();
}

/// SDK power management discovers its matching installation through PATH.
fn cli_on_path() {
    let bin = Path::new(CLI).parent().unwrap();
    let previous = std::env::var_os("PATH").unwrap_or_default();
    if std::env::split_paths(&previous).any(|entry| entry == bin) {
        return;
    }
    let search = std::iter::once(bin.to_owned()).chain(std::env::split_paths(&previous));
    std::env::set_var("PATH", std::env::join_paths(search).unwrap());
}

/// `machine doctor host --detail`'s text.
fn doctor(home: &Path) -> String {
    let trace = home.join("doctor.jsonl");
    let out = traced(
        home,
        &trace,
        &["machine", "doctor", HOST_MACHINE, "--detail"],
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn a_descriptor_the_caller_was_given_does_not_stay_open_in_the_started_worker() {
    let home = proof::create();
    let trace = home.paths.home.join("inherit.jsonl");
    // The CLI gets a pipe's write end as descriptor 3 without close-on-exec,
    // as a caller's IPC channel or jobserver is passed; its stderr is
    // /dev/null, so the pipe ends when every holder of descriptor 3 ends.
    let mut call = Command::new("/bin/sh")
        .args(["-c", r#"exec "$0" "$@" 3>&2 2>/dev/null"#, CLI])
        .args(READ)
        .env("LAYERFS_ROOT", crate::root::root_of(&home.paths.home))
        .env("LAYERFS_TRACE", &trace)
        .env_remove("LAYERFS_TRACE_STEP")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut passed = call.stderr.take().unwrap();
    let mut stdout = call.stdout.take().unwrap();
    let (sent, ended) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut rest = Vec::new();
        let read = std::io::Read::read_to_end(&mut passed, &mut rest).map(|_| rest);
        let _ = sent.send(read);
    });
    let mut answer = String::new();
    std::io::Read::read_to_string(&mut stdout, &mut answer).unwrap();
    assert!(call.wait().unwrap().success(), "{answer}");
    assert!(answer.contains("the host worker proof"), "{answer}");
    let routes = events(&trace, "worker.route");
    assert_eq!(routes.len(), 1, "{routes:?}");
    assert_eq!(routes[0].get_text("reason"), Some("worker started"));
    // the pipe ends with the CLI, while the worker it started runs on
    let rest = ended
        .recv_timeout(Duration::from_secs(10))
        .expect("the passed pipe ends once the CLI exits")
        .unwrap();
    assert!(rest.is_empty(), "{rest:?}");
    let ping = ready(&home);
    assert!(ping.get_int("pid").is_some(), "{ping:?}");
    home.drain();
}

#[test]
fn a_worker_that_ends_before_publishing_is_reported() {
    let home = proof::create();
    // A background start writes its pending marker under the startup lock
    // its worker then holds until it publishes; a worker killed first
    // leaves the marker with that lock free.
    let pending = paths::host_worker_start_pending(&home.paths.home);
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(pending.parent().unwrap())
        .unwrap();
    layerfs_api::home::write_record(
        &pending,
        &SdkValue::map([("at", SdkValue::Int(1)), ("pid", SdkValue::Int(1))]),
    )
    .unwrap();
    let record = layerfs_api::worker::start_failure(&home.paths.home).expect("a report");
    assert_eq!(record.get_text("launch"), Some("background"), "{record:?}");
    let message = record.get_text("message").unwrap();
    assert!(message.contains("ended before publishing"), "{record:?}");
    let text = doctor(&home.paths.home);
    assert!(
        text.contains("the last background start of the host worker failed: the worker ended before publishing"),
        "{text}"
    );

    // while a start holds the startup lock, the marker is that start's
    let startup = paths::host_worker_startup_lock(&home.paths.home);
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(startup)
        .unwrap();
    assert!(layerfs_api::home::flock::try_lock(&lock, true).unwrap());
    assert!(layerfs_api::worker::start_failure(&home.paths.home).is_none());
    layerfs_api::home::flock::unlock(&lock);
    drop(lock);

    // the next call starts a worker, which removes the marker once published
    let trace = home.paths.home.join("next.jsonl");
    read_ok(&home.paths.home, &trace);
    ready(&home);
    assert!(!pending.exists());
    assert!(layerfs_api::worker::start_failure(&home.paths.home).is_none());
    home.drain();
}

#[test]
fn doctor_words_a_failed_start_by_its_cause() {
    cli_on_path();
    let home = proof::create();
    let failed = paths::host_worker_start_failure(&home.paths.home);
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(failed.parent().unwrap())
        .unwrap();
    layerfs_api::home::write_record(
        &failed,
        &SdkValue::map([
            ("at", SdkValue::Int(1)),
            ("by", SdkValue::text("worker")),
            ("launch", SdkValue::text("machine start")),
            ("pid", SdkValue::Int(1)),
            ("message", SdkValue::text("the probe failure")),
        ]),
    )
    .unwrap();
    // `machine start` was answered itself: not a background start
    let text = doctor(&home.paths.home);
    assert!(
        text.contains(&format!(
            "the last `layerfs machine start {HOST_MACHINE}` failed before its worker published its endpoints: the probe failure"
        )),
        "{text}"
    );
    assert!(!text.contains("calls run directly"), "{text}");

    // a stopped host machine refuses calls whatever the record says
    proof::Cli::spawn(&home.paths.home, &["machine", "stop", HOST_MACHINE]).ok();
    let text = doctor(&home.paths.home);
    assert!(
        text.contains(&format!(
            "host machine {HOST_MACHINE} is stopped: calls are refused until `layerfs machine start {HOST_MACHINE}`"
        )),
        "{text}"
    );
    assert!(!text.contains("calls run directly"), "{text}");
}

#[test]
fn a_stopped_host_machine_refuses_and_starts_no_worker() {
    cli_on_path();
    let home = proof::create();
    proof::Cli::spawn(&home.paths.home, &["machine", "stop", HOST_MACHINE]).ok();
    let trace = home.paths.home.join("stopped.jsonl");
    let began = Instant::now();
    let out = traced(&home.paths.home, &trace, READ);
    assert!(!out.status.success(), "{out:?}");
    let bytes = if out.stdout.is_empty() {
        &out.stderr
    } else {
        &out.stdout
    };
    let refusal = layerfs_api::json::parse(std::str::from_utf8(bytes).unwrap()).unwrap();
    assert_eq!(
        refusal.get_text("error"),
        Some("Unavailable"),
        "{refusal:?}"
    );
    assert!(events(&trace, "worker.route").is_empty());
    std::thread::sleep(Duration::from_millis(200).saturating_sub(began.elapsed()));
    assert!(!home.paths.socket.exists());
    assert!(!paths::worker_log(&home.paths.home).exists());
    assert!(home.paths.try_hold().unwrap().is_some());
}
