//! `machine.list` and `machine.get` answer from the records alone, probing
//! nothing; with `--with live` (call surface 2.0, review fix 1) each VM
//! machine is probed once, here with a stand-in `limactl`, all at the same
//! time within 10 s. A machine recorded running whose provider says it is
//! stopped is `stopped`; one whose probe does not answer is `unknown` with
//! `observed_at`, and is never recorded so. A running VM whose forwarded
//! worker port accepts but does not answer the worker's secret (an `ssh -L`
//! or Lima forward with no worker behind it), refuses it, or never answers
//! it within 2 s is `broken` `not_answering`; one whose worker is of
//! another build is `broken` `outdated`; one whose worker answers is ready,
//! and the probe sends no request. Nothing is started. The record is
//! written, moving the home's generation, only when the state word or its
//! reason changes; when it was observed is kept beside it, and a
//! records-only read answers both. An uncertain start whose owner is gone
//! is settled by a live read (C13).
//!
//! Final review: a probe past the bound is stopped, its command killed with
//! its process group and reaped (finding 2).
#![cfg(feature = "machine")]
use layerfs_api::home::{paths, read_record, write_record};
use layerfs_protocol::sdk::SdkValue;
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
    *-fwd*|*-up*|*-deny*|*-mute*|*-old*) printf 'Running\t/nonexistent\n' ;;
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

/// The call's long form; with `live`, `--with live`.
fn call(home: &Path, name: &str, args: &[(&str, &str)], live: bool) -> SdkValue {
    let mut args: Vec<(String, SdkValue)> = (args.iter())
        .map(|(n, v)| ((*n).to_owned(), SdkValue::text(*v)))
        .collect();
    args.push(("detail".to_owned(), SdkValue::Bool(true)));
    if live {
        args.push((
            "with".to_owned(),
            SdkValue::List(vec![SdkValue::text("live")]),
        ));
    }
    let args = SdkValue::Map(args);
    layerfs_api::call(Some(home), name, args, &mut |_| {})
        .unwrap_or_else(|r| panic!("{name}: {} {}", r.code, r.message))
}

fn recorded_running(home: &Path, name: &str) {
    write_record(
        &paths::machine_record(home, name),
        &SdkValue::map([
            ("name", SdkValue::text(name)),
            ("platform", SdkValue::text("lima")),
            ("vm_name", SdkValue::text(format!("layerfs-test-{name}"))),
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
    /// Answers `ready` with this build's protocol, as the worker does.
    Answer,
    /// Answers `ready` with no protocol, as a worker of another build.
    Old,
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
        let ready: &[u8] = match meet {
            Meet::Old => br#"{"ready":true}"#,
            _ => br#"{"ready":true,"protocol":3}"#,
        };
        stream.write_all(&(ready.len() as u32).to_be_bytes()).ok()?;
        stream.write_all(ready).ok()?;
        let mut more = [0u8; 1];
        Some(stream.read(&mut more).ok()? > 0)
    })
}

fn listed<'a>(list: &'a SdkValue, name: &str) -> &'a SdkValue {
    let Some(SdkValue::List(machines)) = list.get("items") else {
        panic!("no machines in {list:?}")
    };
    machines
        .iter()
        .find(|m| m.get_text("machine") == Some(name))
        .unwrap_or_else(|| panic!("{name} not listed"))
}

fn record(home: &Path, name: &str) -> SdkValue {
    read_record(&paths::machine_record(home, name))
        .unwrap()
        .unwrap()
}

fn generation(home: &Path) -> String {
    std::fs::read_to_string(home.join("generation")).unwrap_or_default()
}

fn log(tools: &Path) -> String {
    std::fs::read_to_string(tools.join("calls.log")).unwrap_or_default()
}

#[test]
fn list_and_get_answer_the_records_then_the_live_state() {
    let tools = tempdir("tools");
    let limactl = tools.join("limactl");
    std::fs::write(&limactl, STAND_IN).unwrap();
    std::fs::set_permissions(&limactl, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::env::set_var("PATH", &tools);
    // only the stand-ins: no fallback folder after PATH
    std::env::set_var("LAYERFS_TOOL_FALLBACKS", "");
    let home = tempdir("home");
    call(&home, "machine.list", &[], false); // makes the home and its built-ins
    for name in ["quiet", "slow", "fwd", "up"] {
        recorded_running(&home, name);
    }
    let fwd = endpoint(&home, "fwd", Meet::Close);
    let up = endpoint(&home, "up", Meet::Answer);

    // records only: what the records say, no probe and no write
    let moved = generation(&home);
    let list = call(&home, "machine.list", &[], false);
    for name in [
        "quiet",
        "slow",
        "fwd",
        "up",
        layerfs_home::names::HOST_MACHINE,
    ] {
        let machine = listed(&list, name);
        assert_eq!(machine.get_text("state"), Some("ready"), "{machine:?}");
        assert_eq!(machine.get("observed_at"), Some(&SdkValue::Null));
    }
    assert_eq!(listed(&list, "docker").get_text("state"), Some("stopped"));
    assert_eq!(log(&tools), "", "a records-only list probed");
    assert_eq!(generation(&home), moved);

    let begun = Instant::now();
    let list = call(&home, "machine.list", &[], true);
    let took = begun.elapsed();
    let quiet = listed(&list, "quiet");
    assert_eq!(quiet.get_text("state"), Some("stopped"), "{quiet:?}");
    assert_eq!(quiet.get("message"), Some(&SdkValue::Null));
    assert!(quiet.get_text("observed_at").is_some(), "{quiet:?}");
    let slow = listed(&list, "slow");
    assert_eq!(slow.get_text("state"), Some("unknown"), "{slow:?}");
    let why = slow.get_text("message").unwrap_or("");
    assert!(why.contains("did not answer within 10 s"), "{why}");
    // when LayerFS last knew: the record's write, before any live read
    assert!(slow.get_text("observed_at").is_some(), "{slow:?}");
    assert!(
        took >= Duration::from_secs(9) && took < Duration::from_secs(20),
        "the list waited {took:?}"
    );
    assert_eq!(
        listed(&list, layerfs_home::names::HOST_MACHINE).get_text("state"),
        Some("ready")
    );
    let forward = listed(&list, "fwd");
    assert_eq!(forward.get_text("state"), Some("broken"), "{forward:?}");
    assert_eq!(forward.get_text("reason"), Some("not_answering"));
    // a start does not replace a worker that runs and does not answer
    assert_eq!(
        forward.get_text("fix"),
        Some("layerfs machine stop fwd && layerfs machine start fwd")
    );
    assert_eq!(listed(&list, "up").get_text("state"), Some("ready"));
    assert_eq!(fwd.join().unwrap(), None);
    assert_eq!(up.join().unwrap(), Some(false), "the probe sent a request");
    // the two changes were written, and moved the generation; the
    // unchanged and the unknown were not
    assert_ne!(generation(&home), moved);
    assert_eq!(record(&home, "quiet").get_text("state"), Some("stopped"));
    assert_eq!(
        record(&home, "fwd").get_text("reason"),
        Some("not_answering")
    );
    for name in ["slow", "up"] {
        let kept = record(&home, name);
        assert_eq!(kept.get_text("state"), Some("running"), "{name}");
        assert_eq!(kept.get("reason"), None, "{name}");
    }
    let observed = |name: &str| paths::machine_dir(&home, name).join("observed.json");
    assert!(observed("quiet").is_file() && observed("up").is_file());
    assert!(
        !observed("slow").exists(),
        "an unknown probe observed nothing"
    );

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

    // a records-only read answers what the live read wrote, and when
    let got = call(&home, "machine.get", &[("machine", "fwd")], false);
    assert_eq!(got.get_text("reason"), Some("not_answering"), "{got:?}");
    assert_eq!(got.get_text("observed_at"), forward.get_text("observed_at"));
    // a live read that finds what is recorded writes no record
    let moved = generation(&home);
    let got = call(&home, "machine.get", &[("machine", "quiet")], true);
    assert_eq!(got.get_text("state"), Some("stopped"));
    let live = got.get("live").unwrap();
    assert_eq!(live.get("worker"), Some(&SdkValue::Null));
    let version = live.get_text("tool_version");
    assert_eq!(version, Some("limactl version 0.0-stand-in"), "{live:?}");
    assert_eq!(generation(&home), moved);

    // a worker that refuses the secret, and one that never answers it, do
    // not answer; the silent one costs the host its 2 s limit, no more; a
    // worker of another build is outdated
    for name in ["deny", "mute", "old"] {
        recorded_running(&home, name);
    }
    let deny = endpoint(&home, "deny", Meet::Deny);
    let mute = endpoint(&home, "mute", Meet::Mute);
    let old = endpoint(&home, "old", Meet::Old);
    let got = call(&home, "machine.get", &[("machine", "deny")], true);
    assert_eq!(got.get_text("reason"), Some("not_answering"), "{got:?}");
    let begun = Instant::now();
    let got = call(&home, "machine.get", &[("machine", "mute")], true);
    let took = begun.elapsed();
    assert_eq!(got.get_text("reason"), Some("not_answering"), "{got:?}");
    assert!(
        took >= Duration::from_millis(1900) && took < Duration::from_secs(4),
        "the silent worker held the probe {took:?}"
    );
    let got = call(&home, "machine.get", &[("machine", "old")], true);
    assert_eq!(got.get_text("reason"), Some("outdated"), "{got:?}");
    assert_eq!(
        got.get_text("fix"),
        Some("layerfs machine stop old && layerfs machine start old")
    );
    let worker = got.get("live").unwrap().get("worker").unwrap();
    assert_eq!(worker.get("protocol"), Some(&SdkValue::Null), "{worker:?}");
    assert_eq!(deny.join().unwrap(), None);
    assert_eq!(mute.join().unwrap(), None);
    assert_eq!(old.join().unwrap(), Some(false));

    // a start whose owner is gone and whose outcome is not known is
    // unresolved; a live read settles it by what it sees
    let lost = SdkValue::map([
        ("id", SdkValue::text("op-0000000000000001")),
        ("name", SdkValue::text("machine.start")),
        ("phase", SdkValue::text("boot")),
        ("owner", SdkValue::map([("pid", SdkValue::Int(0))])),
        ("uncertain", SdkValue::Bool(true)),
    ]);
    recorded_running(&home, "settle");
    let held = layerfs_api::home::with_field(record(&home, "settle"), "op", lost);
    write_record(&paths::machine_record(&home, "settle"), &held).unwrap();
    let got = call(&home, "machine.get", &[("machine", "settle")], false);
    assert_eq!(got.get_text("reason"), Some("unresolved"), "{got:?}");
    assert_eq!(
        got.get_text("fix"),
        Some("layerfs machine get settle --with live")
    );
    let got = call(&home, "machine.get", &[("machine", "settle")], true);
    assert_eq!(got.get_text("state"), Some("stopped"), "{got:?}");
    assert_eq!(record(&home, "settle").get("op"), None);

    // nothing was started: the stand-in saw only probes
    let log = log(&tools);
    assert!(
        log.lines()
            .all(|l| l == "--version" || l.starts_with("list ") || l.starts_with("shell ")),
        "{log}"
    );
    assert!(!log.contains("start"), "{log}");
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(tools);
}

/// The host machine is this computer: `--with live` reads the free bytes of
/// the disk that holds the home, its data disk, and answers no provider
/// tool. It reports the machine state and the host worker's health
/// separately (#819 D7, spec §12): an enabled machine whose worker has not
/// started is `absent`. The short form leaves out the worker's empty fields.
#[test]
fn native_with_live_answers_the_free_bytes_of_the_homes_disk() {
    let home = tempdir(layerfs_home::names::HOST_MACHINE);
    // a real home is private; the worker probe refuses one that is not
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    let got = call(
        &home,
        "machine.get",
        &[("machine", layerfs_home::names::HOST_MACHINE)],
        true,
    );
    assert_eq!(got.get_text("state"), Some("ready"), "{got:?}");
    let live = got.get("live").unwrap();
    assert!(live.get_int("disk_free_bytes").unwrap() > 0, "{live:?}");
    assert_eq!(live.get("tool_version"), Some(&SdkValue::Null));
    assert_eq!(live.get_text("machine_state"), Some("enabled"), "{live:?}");
    let worker = live.get("worker").unwrap();
    assert_eq!(worker.get_text("health"), Some("absent"), "{worker:?}");
    assert_eq!(worker.get("pid"), Some(&SdkValue::Null), "{worker:?}");
    let args = SdkValue::map([
        ("machine", SdkValue::text(layerfs_home::names::HOST_MACHINE)),
        ("with", SdkValue::List(vec![SdkValue::text("live")])),
    ]);
    let short = layerfs_api::call(Some(&home), "machine.get", args, &mut |_| {}).unwrap();
    let live = short.get("live").unwrap();
    assert!(live.get_int("disk_free_bytes").unwrap() > 0, "{short:?}");
    assert_eq!(live.get_text("machine_state"), Some("enabled"), "{short:?}");
    let worker = live.get("worker").unwrap();
    assert_eq!(worker.get_text("health"), Some("absent"), "{short:?}");
    assert_eq!(worker.get("pid"), None, "{short:?}");
    std::fs::remove_dir_all(home).unwrap();
}

/// Whether process `pid` exists, a zombie included (`kill -0` finds one).
fn alive(pid: &str) -> bool {
    std::process::Command::new("/bin/kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}
