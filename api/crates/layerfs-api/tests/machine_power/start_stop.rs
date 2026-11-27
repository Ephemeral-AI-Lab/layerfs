//! `machine.start` and `machine.stop` (#535 P1), with a stand-in `limactl`
//! (its state in a file; `stop` stops it) and a stand-in worker: the
//! built-in host machine's stop records `stopped` and its start needs the
//! matching `layerfs` CLI on PATH (#819 D7; the start with that CLI is
//! `layerfs-cli`'s `host_machine_stop`); a running machine's `start` answers at
//! once, starting nothing; a stopped one's runs a start (refused here at
//! the missing Linux bundle, after its `start.lock`); `stop` is `InUse`
//! with `detail.related` while Sandboxes run on the machine, and otherwise
//! stops the VM and removes `worker.json`. A call with nothing to do
//! answers `{"unchanged":true}`; one that works reports `{ phase, done,
//! total }` events and leaves no `op` (call surface 2.0). The lima part runs
//! on macOS only (lima needs macOS). One test, because it sets the
//! process's environment.
#![cfg(feature = "machine")]
use layerfs_api::home::{self, paths, read_record, write_record};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

const STAND_IN: &str = r#"#!/bin/sh
d="${0%/*}"
echo "$*" >> "$d/calls.log"
case "$1" in
  --version) echo "limactl version 0.0-stand-in" ;;
  list) read -r state < "$d/state"; printf '%s\t/nonexistent\n' "$state" ;;
  stop) echo Stopped > "$d/state" ;;
  *) echo "the stand-in refuses: $*" >&2; exit 9 ;;
esac
"#;

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-power-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn call(home: &Path, name: &str, machine: &str) -> Result<SdkValue, SdkRefusal> {
    Ok(called(home, name, machine)?.0)
}

/// The call's short answer and the `phase` of each progress event.
fn called(home: &Path, name: &str, machine: &str) -> Result<(SdkValue, Vec<String>), SdkRefusal> {
    let args = SdkValue::map([("machine", t(machine))]);
    let mut phases = Vec::new();
    let mut seen = |event: SdkValue| {
        if let Some(phase) = event.get("progress").and_then(|p| p.get_text("phase")) {
            phases.push(phase.to_owned());
        }
    };
    let answer = layerfs_api::call(Some(home), name, args, &mut seen)?;
    Ok((answer, phases))
}

fn json(value: &SdkValue) -> String {
    layerfs_home::json::print(value)
}

fn frame(stream: &mut TcpStream) -> Option<Vec<u8>> {
    let mut length = [0u8; 4];
    stream.read_exact(&mut length).ok()?;
    let mut body = vec![0; u32::from_be_bytes(length) as usize];
    stream.read_exact(&mut body).ok()?;
    Some(body)
}

fn send(stream: &mut TcpStream, body: &[u8]) -> Option<()> {
    stream.write_all(&(body.len() as u32).to_be_bytes()).ok()?;
    stream.write_all(body).ok()
}

/// A stand-in worker that takes the secret and answers every request.
fn worker() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            if frame(&mut stream).is_none()
                || send(&mut stream, br#"{"ready":true,"protocol":3}"#).is_none()
            {
                continue;
            }
            if frame(&mut stream).is_some() {
                let _ = send(&mut stream, br#"{"value":{"kind":"worker"}}"#);
            }
        }
    });
    address
}

fn lima(home: &Path, name: &str, more: &[(&str, SdkValue)]) {
    let mut fields = vec![
        ("name".to_owned(), t(name)),
        ("platform".to_owned(), t("lima")),
        // every 2.0 create records its VM's name
        ("vm_name".to_owned(), t(&format!("layerfs-test-{name}"))),
    ];
    fields.extend(more.iter().map(|(n, v)| ((*n).to_owned(), v.clone())));
    write_record(&paths::machine_record(home, name), &SdkValue::Map(fields)).unwrap();
}

fn sandbox(home: &Path, machine: &str, name: &str) {
    write_record(
        &paths::sandbox_record(home, name),
        &SdkValue::map([
            ("name", t(name)),
            ("project", t("shop/api")),
            ("machine", t(machine)),
            ("state", t("running")),
        ]),
    )
    .unwrap();
}

fn log(tools: &Path) -> String {
    std::fs::read_to_string(tools.join("calls.log")).unwrap_or_default()
}

#[test]
fn start_and_stop_a_machine() {
    let _env = crate::env_lock();
    let tools = tempdir("tools");
    let limactl = tools.join("limactl");
    std::fs::write(&limactl, STAND_IN).unwrap();
    std::fs::set_permissions(&limactl, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::env::set_var("PATH", &tools);
    // only the stand-ins: no fallback folder after PATH
    std::env::set_var("LAYERFS_TOOL_FALLBACKS", "");
    let cache = tempdir("cache");
    std::env::set_var("XDG_CACHE_HOME", &cache);
    std::env::remove_var("LAYERFS_BUNDLE");
    let home = tempdir("home");
    home::open(&home).unwrap();

    // the host machine (#819 D7): a stop records `stopped`, and a second
    // stop has nothing to do; a start runs the host worker from the matching
    // `layerfs` CLI on PATH, which this PATH (the stand-ins alone) lacks
    let host = layerfs_home::names::HOST_MACHINE;
    let stopped = call(&home, "machine.stop", host).unwrap();
    assert_eq!(json(&stopped), r#"{"ok":true}"#);
    let record = read_record(&paths::machine_record(&home, host))
        .unwrap()
        .unwrap();
    assert_eq!(record.get_text("state"), Some("stopped"));
    let again = call(&home, "machine.stop", host).unwrap();
    assert_eq!(json(&again), r#"{"unchanged":true}"#);
    let refusal = call(&home, "machine.start", host).unwrap_err();
    assert_eq!(refusal.code, "Unavailable", "{}", refusal.message);
    assert_eq!(
        refusal.message,
        "host worker management requires the matching layerfs CLI on PATH"
    );

    // The lima machine `dev` (start, InUse, stop, a second stop, the start
    // of a stopped one) runs on macOS only: lima needs macOS, and on Linux
    // its second stop answers `{"ok":true}`, not `unchanged`. The host
    // machine's assertions above run on both.
    if cfg!(target_os = "macos") {
        lima_power(&home, &tools);
    }

    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(tools);
    let _ = std::fs::remove_dir_all(cache);
}

/// The lima machine `dev`'s start and stop with the stand-in `limactl`
/// in `tools` (macOS only, see the caller).
fn lima_power(home: &Path, tools: &Path) {
    // a running machine's start answers at once and starts nothing
    std::fs::write(tools.join("state"), "Running\n").unwrap();
    let address = worker();
    lima(home, "dev", &[("state", t("running"))]);
    let recorded = SdkValue::map([("address", t(&address.to_string())), ("secret", t("5e"))]);
    write_record(&paths::worker_record(home, "dev"), &recorded).unwrap();
    let (started, phases) = called(home, "machine.start", "dev").unwrap();
    assert_eq!(json(&started), r#"{"unchanged":true}"#);
    assert!(phases.is_empty(), "{phases:?}");
    assert!(!paths::machine_dir(home, "dev").join("start.lock").exists());
    assert!(!log(tools).contains("start"), "{}", log(tools));

    // stop is InUse while a Sandbox runs on it
    sandbox(home, "dev", "s1");
    let refusal = call(home, "machine.stop", "dev").unwrap_err();
    assert_eq!(refusal.code, "InUse", "{}", refusal.message);
    let related = refusal.detail.get("related").cloned().unwrap_or_default();
    let named = |name: &str| SdkValue::map([("kind", t("sandbox")), ("name", t(name))]);
    assert_eq!(related, SdkValue::List(vec![named("s1")]));
    // the next call's fields: the Sandboxes, and a fix that removes them
    // and stops the machine (UI pass 1 FAIL 5)
    let next = refusal.detail.get("next").cloned().unwrap_or_default();
    assert_eq!(next.get("sandboxes"), Some(&SdkValue::List(vec![t("s1")])));
    assert_eq!(
        refusal.detail.get_text("fix"),
        Some("layerfs sandbox remove s1 && layerfs machine stop dev")
    );
    assert!(!refusal.message.contains('['), "{}", refusal.message);
    assert!(!log(tools).contains("stop"), "{}", log(tools));
    std::fs::remove_dir_all(paths::sandbox_dir(home, "s1")).unwrap();

    // stop stops the VM and forgets its worker; disks and records stay
    let (stopped, phases) = called(home, "machine.stop", "dev").unwrap();
    assert_eq!(json(&stopped), r#"{"ok":true}"#);
    assert_eq!(phases, ["check", "stop", "stop"]);
    assert!(log(tools)
        .lines()
        .any(|l| l.starts_with("stop layerfs-test-dev")));
    assert!(!paths::worker_record(home, "dev").exists());
    let record = read_record(&paths::machine_record(home, "dev"))
        .unwrap()
        .unwrap();
    assert_eq!(record.get_text("state"), Some("stopped"));
    assert!(record.get("op").is_none(), "{record:?}");
    // stopping it again has nothing to do
    let again = call(home, "machine.stop", "dev").unwrap();
    assert_eq!(json(&again), r#"{"unchanged":true}"#);

    // a stopped machine's start runs (refused here for the missing bundle)
    let mut phases = Vec::new();
    let mut seen = |event: SdkValue| phases.push(json(&event));
    let args = SdkValue::map([("machine", t("dev"))]);
    let refusal = layerfs_api::call(Some(home), "machine.start", args, &mut seen).unwrap_err();
    assert_eq!(refusal.code, "Unsupported", "{}", refusal.message);
    assert!(refusal.message.contains("bundle"), "{}", refusal.message);
    assert!(paths::machine_dir(home, "dev").join("start.lock").exists());
    assert_eq!(
        phases,
        [r#"{"progress":{"phase":"bundle","done":0,"total":3}}"#]
    );
    // a start refused before its provider was asked leaves no op
    let record = read_record(&paths::machine_record(home, "dev"))
        .unwrap()
        .unwrap();
    assert!(record.get("op").is_none(), "{record:?}");
}
