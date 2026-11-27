#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]
//! The first CLI call with no ready host worker (#833): it runs on its
//! direct route, chosen before anything is sent, and starts the worker in
//! the background for the next call, once however many callers race.

#[path = "common/host_worker.rs"]
mod fixture;
#[path = "common/host_worker_proof.rs"]
mod proof;

use fixture::Fixture;
use layerfs_api::home::paths;
use layerfs_api::names::HOST_MACHINE;
use layerfs_bridge::control::SdkValue;
use std::os::unix::fs::DirBuilderExt;
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
        .env("LAYERFS_HOME", home)
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
                .env("LAYERFS_HOME", &home.paths.home)
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

#[test]
fn a_stopped_host_machine_refuses_and_starts_no_worker() {
    // SDK power management discovers its matching installation through PATH.
    let bin = Path::new(CLI).parent().unwrap();
    let previous = std::env::var_os("PATH").unwrap_or_default();
    let search = std::iter::once(bin.to_owned()).chain(std::env::split_paths(&previous));
    std::env::set_var("PATH", std::env::join_paths(search).unwrap());
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
