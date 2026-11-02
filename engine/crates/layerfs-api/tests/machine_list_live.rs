//! `machine.list` and `machine.get` answer each VM machine's live state, read
//! as `machine.status` reads it (#533 step 19, delegated owner decision
//! 2026-11-02), here with a stand-in `limactl`. A machine recorded `running`
//! whose provider says it is stopped is listed `stopped`; one whose probe
//! does not answer is `unknown` with the reason, within the list's bound.
//! A running VM whose forwarded worker port accepts but does not answer the
//! worker's secret (an `ssh -L` or Lima forward with no worker behind it) is
//! `stopped`; one whose worker answers is `running`, and the probe sends no
//! request. Nothing is started and the records are not rewritten.
#![cfg(feature = "machine")]
use layerfs_api::calls;
use layerfs_api::home::{paths, read_record, write_record};
use layerfs_bridge::control::SdkValue;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const STAND_IN: &str = r#"#!/bin/sh
echo "$*" >> "${0%/*}/calls.log"
case "$1" in
  --version) echo "limactl version 0.0-stand-in" ;;
  list) case "$*" in
    *-slow*) exec /bin/sleep 40 ;;
    *-fwd*|*-up*) printf 'Running\t/nonexistent\n' ;;
    *) printf 'Stopped\t/nonexistent\n' ;;
  esac ;;
  *) echo "the stand-in refuses: $*" >&2; exit 9 ;;
esac
"#;

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-live-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn call(home: &Path, name: &str, args: &[(&str, &str)]) -> SdkValue {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), SdkValue::text(*v)))
            .collect(),
    );
    calls::call(Some(home), name, args, &mut |_| {})
        .unwrap_or_else(|r| panic!("{name}: {} {}", r.code, r.message))
}

fn recorded_running(home: &Path, name: &str) {
    write_record(
        &paths::machine_record(home, name),
        &SdkValue::map([
            ("name", SdkValue::text(name)),
            ("platform", SdkValue::text("lima")),
            ("state", SdkValue::text("running")),
            ("cpus", SdkValue::Int(2)),
            ("memory", SdkValue::Int(4 << 30)),
            ("disk", SdkValue::Int(30 << 30)),
        ]),
    )
    .unwrap();
}

/// A worker endpoint for machine `name`: `answer` makes it reply to the
/// secret as the worker does; otherwise it accepts and closes, as a forward
/// with nothing behind it. The thread reports whether a request followed.
fn endpoint(home: &Path, name: &str, answer: bool) -> std::thread::JoinHandle<Option<bool>> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    write_record(
        &paths::worker_record(home, name),
        &SdkValue::map([
            (
                "address",
                SdkValue::text(listener.local_addr().unwrap().to_string()),
            ),
            ("secret", SdkValue::text("5e")),
        ]),
    )
    .unwrap();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().ok()?;
        if !answer {
            return None;
        }
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
        let mut length = [0u8; 4];
        stream.read_exact(&mut length).ok()?;
        let mut hello = vec![0; u32::from_be_bytes(length) as usize];
        stream.read_exact(&mut hello).ok()?;
        assert_eq!(hello, br#"{"secret":"5e"}"#);
        let ready = br#"{"ready":true}"#;
        stream.write_all(&(ready.len() as u32).to_be_bytes()).ok()?;
        stream.write_all(ready).ok()?;
        let mut more = [0u8; 1];
        Some(stream.read(&mut more).ok()? > 0)
    })
}

fn listed<'a>(list: &'a SdkValue, name: &str) -> &'a SdkValue {
    let Some(SdkValue::List(machines)) = list.get("machines") else {
        panic!("no machines in {list:?}")
    };
    machines
        .iter()
        .find(|m| m.get_text("name") == Some(name))
        .unwrap_or_else(|| panic!("{name} not listed"))
}

#[test]
fn list_and_get_answer_the_live_state() {
    let tools = tempdir("tools");
    let limactl = tools.join("limactl");
    std::fs::write(&limactl, STAND_IN).unwrap();
    std::fs::set_permissions(&limactl, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::env::set_var("PATH", &tools);
    let home = tempdir("home");
    call(&home, "machine.list", &[]); // makes the home and its built-ins
    for name in ["quiet", "slow", "fwd", "up"] {
        recorded_running(&home, name);
    }
    let fwd = endpoint(&home, "fwd", false);
    let up = endpoint(&home, "up", true);

    let begun = Instant::now();
    let list = call(&home, "machine.list", &[]);
    let took = begun.elapsed();
    let quiet = listed(&list, "quiet");
    assert_eq!(quiet.get_text("state"), Some("stopped"), "{quiet:?}");
    assert_eq!(quiet.get("error"), None);
    let slow = listed(&list, "slow");
    assert_eq!(slow.get_text("state"), Some("unknown"), "{slow:?}");
    let why = slow.get_text("error").unwrap_or("");
    assert!(why.contains("did not answer within 10 s"), "{why}");
    assert!(
        took >= Duration::from_secs(9) && took < Duration::from_secs(20),
        "the list waited {took:?}"
    );
    assert_eq!(listed(&list, "native").get_text("state"), Some("ready"));
    assert_eq!(listed(&list, "fwd").get_text("state"), Some("stopped"));
    assert_eq!(listed(&list, "up").get_text("state"), Some("running"));
    assert_eq!(fwd.join().unwrap(), None);
    assert_eq!(up.join().unwrap(), Some(false), "the probe sent a request");

    let got = call(&home, "machine.get", &[("name", "quiet")]);
    assert_eq!(got.get_text("state"), Some("stopped"));

    // read-only: the records still say what they said, and nothing started
    for name in ["quiet", "slow", "fwd", "up"] {
        let record = read_record(&paths::machine_record(&home, name))
            .unwrap()
            .unwrap();
        assert_eq!(record.get_text("state"), Some("running"), "{name}");
    }
    let log = std::fs::read_to_string(tools.join("calls.log")).unwrap();
    assert!(
        log.lines()
            .all(|l| l == "--version" || l.starts_with("list ") || l.starts_with("shell ")),
        "{log}"
    );
    assert!(!log.contains("start"), "{log}");
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(tools);
}
