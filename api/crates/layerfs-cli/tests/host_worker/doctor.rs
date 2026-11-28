#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]

use crate::fixture;

use fixture::Fixture;
use layerfs_api::{
    home::paths,
    json,
    worker::{write_host_frame, MAX_HELLO},
};
use layerfs_protocol::sdk::SdkValue;
use std::fs;
use std::io::{ErrorKind, Read};
use std::os::unix::{fs::PermissionsExt, net::UnixListener};
use std::process::Command;
use std::time::{Duration, Instant};

const CLI: &str = env!("CARGO_BIN_EXE_layerfs");

fn cli(home: &Fixture, words: &[&str]) -> SdkValue {
    let output = Command::new(CLI)
        .args(words)
        .arg("--detail")
        .env("LAYERFS_ROOT", crate::root::root_of(&home.paths.home))
        .output()
        .unwrap();
    assert!(output.status.success(), "{words:?}: {output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    json::parse(text.trim()).unwrap_or_else(|error| panic!("{words:?}: {error:?}: {text}"))
}

fn machine(home: &Fixture) -> SdkValue {
    cli(
        home,
        &[
            "machine",
            "get",
            layerfs_api::names::HOST_MACHINE,
            "--with",
            "live",
        ],
    )
}

fn list<'a>(value: &'a SdkValue, key: &str) -> &'a [SdkValue] {
    match value.get(key) {
        Some(SdkValue::List(rows)) => rows,
        _ => panic!("{key} is absent: {value:?}"),
    }
}

#[test]
fn live_and_doctor_observe_absent_ready_and_stale_without_starting() {
    let home = Fixture::new();
    let before = machine(&home);
    let live = before.get("live").unwrap();
    assert_eq!(live.get_text("machine_state"), Some("enabled"));
    assert_eq!(
        live.get("worker").unwrap().get_text("health"),
        Some("absent")
    );
    cli(
        &home,
        &["machine", "doctor", layerfs_api::names::HOST_MACHINE],
    );
    assert!(!home.paths.control.exists());
    assert!(!paths::host_worker_lock(&home.paths.home).exists());

    let mut worker = home.start();
    let ping = home.ping();
    let after = machine(&home);
    let fact = after.get("live").unwrap().get("worker").unwrap();
    assert_eq!(fact.get_text("health"), Some("ready"));
    for key in ["pid", "pid_start", "pid_boot", "build", "generation"] {
        assert!(ping.get(key).is_some(), "{key}: {ping:?}");
        assert_eq!(fact.get(key), ping.get(key), "{fact:?}");
    }
    assert_eq!(fact.get_int("admitted"), Some(1), "{fact:?}");
    assert_eq!(fact.get_int("queued"), Some(0));
    assert_eq!(fact.get_int("stores"), Some(0));
    assert_eq!(fact.get("last_close"), Some(&SdkValue::Null));
    let report = cli(
        &home,
        &["machine", "doctor", layerfs_api::names::HOST_MACHINE],
    );
    let checks = list(report.get("tree").unwrap(), "checks");
    let checked = checks
        .iter()
        .find(|row| row.get_text("check") == Some("machine.worker.ping"))
        .unwrap();
    assert_eq!(checked.get_text("outcome"), Some("pass"));
    assert_eq!(
        checked.get("evidence").unwrap().get_int("pid"),
        Some(worker.pid())
    );

    worker.kill();
    assert!(
        home.paths.control.exists(),
        "the killed worker left its socket"
    );
    let stale = machine(&home);
    assert_eq!(
        stale
            .get("live")
            .unwrap()
            .get("worker")
            .unwrap()
            .get_text("health"),
        Some("stale"),
    );
    assert!(home.paths.control.exists(), "the probe changed the socket");

    // A worker killed between bind and chmod leaves a socket without its
    // private mode; the next start replaces it, so doctor reports it stale.
    fs::set_permissions(&home.paths.control, fs::Permissions::from_mode(0o755)).unwrap();
    let unready = machine(&home);
    assert_eq!(
        unready
            .get("live")
            .unwrap()
            .get("worker")
            .unwrap()
            .get_text("health"),
        Some("stale"),
    );
    let report = cli(
        &home,
        &["machine", "doctor", layerfs_api::names::HOST_MACHINE],
    );
    let checks = list(report.get("tree").unwrap(), "checks");
    let endpoint = checks
        .iter()
        .find(|row| row.get_text("check") == Some("machine.worker.endpoint"))
        .unwrap();
    assert_eq!(endpoint.get_text("outcome"), Some("notice"), "{endpoint:?}");
    assert!(home.paths.control.exists(), "the probe changed the socket");

    // The same socket while a worker holds the lifetime lock (here this
    // test, as a worker between bind and chmod would): nothing replaces it,
    // so it is `owned`, not `stale`, and doctor names the lock holder.
    let guard = home.paths.try_hold().unwrap().unwrap();
    let owned = machine(&home);
    assert_eq!(
        owned
            .get("live")
            .unwrap()
            .get("worker")
            .unwrap()
            .get_text("health"),
        Some("owned"),
    );
    let report = cli(
        &home,
        &["machine", "doctor", layerfs_api::names::HOST_MACHINE],
    );
    let warning = list(&report, "findings")
        .iter()
        .find(|finding| finding.get_text("check") == Some("machine.worker.endpoint"))
        .unwrap();
    assert_eq!(warning.get_text("severity"), Some("warning"), "{warning:?}");
    let message = warning.get_text("message").unwrap();
    assert!(message.contains("holds its lifetime lock"), "{message}");
    assert!(!message.contains("the next start replaces it"), "{message}");
    drop(guard);
    assert!(home.paths.control.exists(), "the probe changed the socket");
}

#[test]
fn incompatible_hello_is_a_notice_and_receives_no_ping() {
    let home = Fixture::new();
    let listener = UnixListener::bind(&home.paths.control).unwrap();
    fs::set_permissions(&home.paths.control, fs::Permissions::from_mode(0o600)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let home_id = home.paths.home_id.clone();
    let server = std::thread::spawn(move || {
        let end = Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == ErrorKind::WouldBlock && Instant::now() < end => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("the doctor did not connect: {error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        let hello = SdkValue::map([
            ("ready", SdkValue::Bool(true)),
            ("kind", SdkValue::text("host")),
            ("home_id", SdkValue::text(home_id)),
            ("generation", SdkValue::text("00".repeat(16))),
            ("build", SdkValue::text("other build")),
            ("protocol", SdkValue::Int(1)),
        ]);
        write_host_frame(&stream, &hello, MAX_HELLO, end).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        assert_eq!(
            stream.read(&mut [0; 1]).unwrap(),
            0,
            "diagnostic sent a request"
        );
    });
    let report = cli(
        &home,
        &["machine", "doctor", layerfs_api::names::HOST_MACHINE],
    );
    server.join().unwrap();
    let notice = list(&report, "findings")
        .iter()
        .find(|finding| finding.get_text("check") == Some("machine.worker.compatibility"))
        .unwrap();
    assert_eq!(notice.get_text("severity"), Some("notice"));
    let evidence = notice.get("evidence").unwrap();
    assert_eq!(evidence.get_text("build"), Some("other build"));
    assert!(evidence.get_text("expected_build").is_some());
    assert!(!paths::host_worker_lock(&home.paths.home).exists());
}

#[test]
fn store_doctor_names_only_an_exact_live_worker_lease() {
    let home = Fixture::new();
    cli(&home, &["store", "create", "sample"]);
    let mut worker = home.start();
    let ping = home.ping();
    let dir = paths::lease_dir(&home.paths.home, "sample");
    fs::create_dir_all(&dir).unwrap();
    let worker_file = dir.join(worker.pid().to_string());
    let other_pid = std::process::id();
    fs::write(dir.join(other_pid.to_string()), "1\n").unwrap();
    fs::write(&worker_file, "1\n").unwrap();
    let find_holders = || {
        let report = cli(&home, &["store", "doctor", "sample"]);
        let evidence = list(&report, "findings")
            .iter()
            .filter_map(|row| row.get("evidence"))
            .find(|evidence| evidence.get("holders").is_some())
            .unwrap();
        list(evidence, "holders").to_vec()
    };
    let partial = find_holders();
    assert_eq!(partial.len(), 2);
    assert!(partial
        .iter()
        .all(|row| row.get_text("kind") == Some("process")));

    let exact = format!(
        "1 {} {}\n",
        ping.get_text("pid_start").unwrap(),
        ping.get_text("pid_boot").unwrap()
    );
    fs::write(&worker_file, &exact).unwrap();
    let holders = find_holders();
    assert_eq!(holders.len(), 2);
    assert_eq!(holders[0].get_int("pid"), Some(worker.pid()));
    assert_eq!(holders[0].get_text("kind"), Some("host_worker"));
    assert_eq!(holders[1].get_int("pid"), Some(i64::from(other_pid)));
    assert_eq!(holders[1].get_text("kind"), Some("process"));
    assert_eq!(fs::read_to_string(worker_file).unwrap(), exact);
    home.drain();
    assert!(worker.wait(Duration::from_secs(5)).success());
}
