//! `machine.start` and `machine.stop` (#535 P1), with a stand-in `limactl`
//! (its state in a file; `stop` stops it) and a stand-in worker: built-in
//! `native` answers both at once; a running machine's `start` answers at
//! once, starting nothing; a stopped one's runs a start (refused here at
//! the missing Linux bundle, after its `start.lock`); `stop` is `InUse`
//! with `detail.related` while Sandboxes run on the machine or on a
//! Firecracker machine it hosts, and otherwise stops the VM, removes
//! `worker.json` and answers the machine as `machine.get` does. One test,
//! because it sets the process's environment.
#![cfg(feature = "machine")]
use layerfs_api::home::{self, paths, read_record, write_record};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
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
    let args = SdkValue::map([("name", t(machine))]);
    layerfs_api::call(Some(home), name, args, &mut |_| {})
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
            if frame(&mut stream).is_none() || send(&mut stream, br#"{"ready":true}"#).is_none() {
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
    let tools = tempdir("tools");
    let limactl = tools.join("limactl");
    std::fs::write(&limactl, STAND_IN).unwrap();
    std::fs::set_permissions(&limactl, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::env::set_var("PATH", &tools);
    let cache = tempdir("cache");
    std::env::set_var("XDG_CACHE_HOME", &cache);
    std::env::remove_var("LAYERFS_BUNDLE");
    let home = tempdir("home");
    home::open(&home).unwrap();

    // native answers both at once
    for name in ["machine.start", "machine.stop"] {
        let native = call(&home, name, "native").unwrap();
        assert_eq!(native.get_text("name"), Some("native"), "{name}");
        assert_eq!(native.get_text("state"), Some("ready"), "{name}");
    }

    // a running machine's start answers at once and starts nothing
    std::fs::write(tools.join("state"), "Running\n").unwrap();
    let address = worker();
    lima(&home, "dev", &[("state", t("running"))]);
    let recorded = SdkValue::map([("address", t(&address.to_string())), ("secret", t("5e"))]);
    write_record(&paths::worker_record(&home, "dev"), &recorded).unwrap();
    let started = call(&home, "machine.start", "dev").unwrap();
    assert_eq!(started.get_text("name"), Some("dev"));
    assert_eq!(started.get_text("state"), Some("running"), "{started:?}");
    assert!(!paths::machine_dir(&home, "dev").join("start.lock").exists());
    assert!(!log(&tools).contains("start"), "{}", log(&tools));

    // stop is InUse while a Sandbox runs on it or on a microVM it hosts
    sandbox(&home, "dev", "s1");
    write_record(
        &paths::machine_record(&home, "fc"),
        &SdkValue::map([
            ("name", t("fc")),
            ("platform", t("firecracker")),
            ("host", t("dev")),
        ]),
    )
    .unwrap();
    sandbox(&home, "fc", "s2");
    let refusal = call(&home, "machine.stop", "dev").unwrap_err();
    assert_eq!(refusal.code, "InUse", "{}", refusal.message);
    let related = refusal.detail.get("related").cloned().unwrap_or_default();
    let named = |name: &str| SdkValue::map([("kind", t("sandbox")), ("name", t(name))]);
    assert_eq!(related, SdkValue::List(vec![named("s1"), named("s2")]));
    assert!(!log(&tools).contains("stop"), "{}", log(&tools));
    for name in ["s1", "s2"] {
        std::fs::remove_dir_all(paths::sandbox_dir(&home, name)).unwrap();
    }

    // stop stops the VM and forgets its worker; disks and records stay
    let stopped = call(&home, "machine.stop", "dev").unwrap();
    assert_eq!(stopped.get_text("state"), Some("stopped"), "{stopped:?}");
    assert!(log(&tools).lines().any(|l| l.starts_with("stop lfs-")));
    assert!(!paths::worker_record(&home, "dev").exists());
    let record = read_record(&paths::machine_record(&home, "dev"))
        .unwrap()
        .unwrap();
    assert_eq!(record.get_text("state"), Some("stopped"));

    // a stopped machine's start runs (refused here for the missing bundle)
    let refusal = call(&home, "machine.start", "dev").unwrap_err();
    assert_eq!(refusal.code, "Unsupported", "{}", refusal.message);
    assert!(refusal.message.contains("bundle"), "{}", refusal.message);
    assert!(paths::machine_dir(&home, "dev").join("start.lock").exists());

    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(tools);
    let _ = std::fs::remove_dir_all(cache);
}
