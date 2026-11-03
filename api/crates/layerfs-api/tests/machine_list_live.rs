//! `machine.list` and `machine.get` answer each VM machine's live state, read
//! as `machine.status` reads it (#533 step 19, delegated owner decision
//! 2026-11-02), here with a stand-in `limactl`. A machine recorded `running`
//! whose provider says it is stopped is listed `stopped`; one whose probe
//! does not answer is `unknown` with the reason, within the list's bound.
//! A running VM whose forwarded worker port accepts but does not answer the
//! worker's secret (an `ssh -L` or Lima forward with no worker behind it) is
//! `stopped`; one whose worker answers is `running`, and the probe sends no
//! request. Nothing is started and the records are not rewritten.
//!
//! Final review: a probe past the bound is stopped, its command killed with
//! its process group and reaped (finding 2), and the host's secret check
//! counts a `Denied` reply, or an accept that never answers (its 2 s
//! limit), as no worker (finding 6).
#![cfg(feature = "machine")]
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
    *-slow*) /bin/sleep 40 & echo "$$ $!" > "${0%/*}/slow.pid"; wait ;;
    *-fwd*|*-up*|*-deny*|*-mute*) printf 'Running\t/nonexistent\n' ;;
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
    layerfs_api::call(Some(home), name, args, &mut |_| {})
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

/// How a stand-in worker endpoint meets the host's secret.
#[derive(Clone, Copy, PartialEq)]
enum Meet {
    /// Accepts and closes, as a forward with nothing behind it.
    Close,
    /// Answers `ready`, as the worker does.
    Answer,
    /// Refuses the secret `Denied`, as a worker with another secret does.
    Deny,
    /// Reads the secret and never answers.
    Mute,
}

/// A worker endpoint for machine `name`. The thread reports whether a
/// request followed a `ready`.
fn endpoint(home: &Path, name: &str, meet: Meet) -> std::thread::JoinHandle<Option<bool>> {
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
        if meet == Meet::Close {
            return None;
        }
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
        let mut length = [0u8; 4];
        stream.read_exact(&mut length).ok()?;
        let mut hello = vec![0; u32::from_be_bytes(length) as usize];
        stream.read_exact(&mut hello).ok()?;
        assert_eq!(hello, br#"{"secret":"5e"}"#);
        if meet == Meet::Mute {
            // holds the connection past the host's 2 s limit
            std::thread::sleep(Duration::from_secs(4));
            return None;
        }
        if meet == Meet::Deny {
            let denied = br#"{"refused":{"code":"Denied","message":"the worker's secret does not match","detail":null}}"#;
            stream
                .write_all(&(denied.len() as u32).to_be_bytes())
                .ok()?;
            stream.write_all(denied).ok()?;
            return None;
        }
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
    let fwd = endpoint(&home, "fwd", Meet::Close);
    let up = endpoint(&home, "up", Meet::Answer);

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
    // the built-in docker is probed as `machine.status` probes it (#535
    // D11: its worker starts and stops by hand), never a fixed `ready`
    let docker = listed(&list, "docker");
    match layerfs_api::call(
        Some(&home),
        "machine.status",
        SdkValue::map([("name", SdkValue::text("docker"))]),
        &mut |_| {},
    ) {
        Ok(status) => assert_eq!(
            docker.get_text("state"),
            status.get_text("state"),
            "{docker:?}"
        ),
        Err(_) => assert_eq!(docker.get_text("state"), Some("unknown"), "{docker:?}"),
    }
    assert_eq!(listed(&list, "fwd").get_text("state"), Some("stopped"));
    assert_eq!(listed(&list, "up").get_text("state"), Some("running"));
    assert_eq!(fwd.join().unwrap(), None);
    assert_eq!(up.join().unwrap(), Some(false), "the probe sent a request");

    // the late probe's command and its child were killed and reaped: no
    // process (not even a zombie) is left once the probe's thread has
    // stopped them, just after the list's bound
    let pids = std::fs::read_to_string(tools.join("slow.pid")).unwrap();
    for pid in pids.split_whitespace() {
        let gone = Instant::now() + Duration::from_secs(3);
        while alive(pid) && Instant::now() < gone {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(!alive(pid), "the late probe's process {pid} outlived it");
    }

    let got = call(&home, "machine.get", &[("name", "quiet")]);
    assert_eq!(got.get_text("state"), Some("stopped"));

    // a worker that refuses the secret, and one that never answers it, are
    // no worker; the silent one costs the host its 2 s limit, no more
    for name in ["deny", "mute"] {
        recorded_running(&home, name);
    }
    let deny = endpoint(&home, "deny", Meet::Deny);
    let mute = endpoint(&home, "mute", Meet::Mute);
    let got = call(&home, "machine.get", &[("name", "deny")]);
    assert_eq!(got.get_text("state"), Some("stopped"), "{got:?}");
    let begun = Instant::now();
    let got = call(&home, "machine.get", &[("name", "mute")]);
    let took = begun.elapsed();
    assert_eq!(got.get_text("state"), Some("stopped"), "{got:?}");
    assert!(
        took >= Duration::from_millis(1900) && took < Duration::from_secs(4),
        "the silent worker held the probe {took:?}"
    );
    assert_eq!(deny.join().unwrap(), None);
    assert_eq!(mute.join().unwrap(), None);

    // read-only: the records still say what they said, and nothing started
    for name in ["quiet", "slow", "fwd", "up", "deny", "mute"] {
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

/// Whether process `pid` exists, a zombie included (`kill -0` finds one).
fn alive(pid: &str) -> bool {
    std::process::Command::new("/bin/kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}
