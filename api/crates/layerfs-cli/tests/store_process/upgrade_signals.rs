//! Ctrl-C and SIGTERM during `layerfs store upgrade` (#804 §6). The command
//! blocks SIGINT and SIGTERM for this call alone and takes them on a thread:
//! the first asks the call to stop (`Failed`, `detail.code: "Cancelled"`
//! before COMMIT), the second exits at once with `Unknown` and the fix
//! `layerfs store upgrade <store>`; once the call has answered, a signal
//! changes nothing. Any other call keeps the default: a signal ends it.
//!
//! The command is held at a known point by a full standard error: its
//! write of the refusal (or of the first progress event) waits until the
//! test reads the pipe, so signals land while it is held there. The test
//! sends them only on evidence that it is held: the home the call opens
//! exists or the process has its signal thread, and it has used no CPU
//! over several samples (bounded by a deadline).
//!
//! The tests marked INTEGRATION need the store upgrade engine and control
//! halves (#804 W1, W2) and the frozen fixture
//! `fixtures/store-formats/sqlite/v2`; on a build without them they say
//! NOT_RUN and pass.
#[path = "../support/mod.rs"]
mod common;
use common::{layerfs, tempdir, text, LAYERFS};
use layerfs_home::json;
use layerfs_protocol::sdk::SdkValue;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// `O_NONBLOCK`, whose value differs between the supported platforms.
#[cfg(target_os = "linux")]
const O_NONBLOCK: i32 = 0o4000;
#[cfg(target_os = "macos")]
const O_NONBLOCK: i32 = 0x4;

/// A run that outlasts this is a hang.
const DEADLINE: Duration = Duration::from_secs(60);

/// A pipe (a fifo) that is full: the child's first write to it waits until
/// the test reads. Returns the child's end and the test's reading end.
fn full_pipe(tag: &str) -> (Stdio, File) {
    let dir = tempdir(&format!("signals-fifo-{tag}"));
    let path = dir.join("pipe");
    assert!(Command::new("mkfifo")
        .arg(&path)
        .status()
        .unwrap()
        .success());
    let reader = OpenOptions::new()
        .read(true)
        .custom_flags(O_NONBLOCK)
        .open(&path)
        .unwrap();
    let mut filler = OpenOptions::new()
        .write(true)
        .custom_flags(O_NONBLOCK)
        .open(&path)
        .unwrap();
    let child = OpenOptions::new().write(true).open(&path).unwrap();
    let block = [b' '; 512];
    loop {
        match filler.write(&block) {
            Ok(_) => continue,
            Err(error) if error.kind() == ErrorKind::WouldBlock => break,
            Err(error) => panic!("filling the pipe: {error}"),
        }
    }
    // a 1-byte write fills what a 512-byte one could not
    while filler.write(b" ").is_ok() {}
    drop(filler);
    std::fs::remove_dir_all(dir).unwrap();
    (Stdio::from(child), reader)
}

/// Reads `reader` to its end (the child's exit closes the last writer),
/// then the child's status.
fn drain(mut reader: File, mut child: Child) -> (ExitStatus, Vec<u8>) {
    let until = Instant::now() + DEADLINE;
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 65536];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => {
                if let Some(status) = child.try_wait().unwrap() {
                    return (status, bytes);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(n) => bytes.extend_from_slice(&buffer[..n]),
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                if let Some(status) = child.try_wait().unwrap() {
                    return (status, bytes);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("reading the pipe: {error}"),
        }
        if Instant::now() > until {
            let _ = child.kill();
            panic!("layerfs did not end within {DEADLINE:?}");
        }
    }
}

fn signal(child: &Child, which: Signal) {
    kill(Pid::from_raw(child.id() as i32), which).unwrap();
}

/// The last JSON line of `bytes` (the filler is spaces).
fn last_json(bytes: &[u8]) -> SdkValue {
    let text = text(bytes);
    let line = text
        .lines()
        .map(str::trim)
        .rfind(|line| line.starts_with('{'))
        .unwrap_or_else(|| panic!("no JSON line in {text:?}"));
    json::parse(line).unwrap()
}

/// The number of threads of process `pid`.
fn threads(pid: u32) -> usize {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_dir(format!("/proc/{pid}/task")).map_or(0, |dir| dir.count())
    }
    #[cfg(target_os = "macos")]
    {
        let out = Command::new("ps")
            .args(["-M", "-p", &pid.to_string()])
            .output()
            .unwrap();
        text(&out.stdout).lines().count().saturating_sub(1)
    }
}

/// The CPU time process `pid` has used so far, as the system writes it.
fn cpu(pid: u32) -> String {
    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
        // utime and stime, after the parenthesized command name
        let rest = stat.rsplit_once(')').map_or("", |(_, rest)| rest);
        rest.split_whitespace()
            .skip(11)
            .take(2)
            .collect::<Vec<_>>()
            .join(" ")
    }
    #[cfg(target_os = "macos")]
    {
        let out = Command::new("ps")
            .args(["-o", "time=", "-p", &pid.to_string()])
            .output()
            .unwrap();
        text(&out.stdout).trim().to_owned()
    }
}

/// Waits until `child` is held: `ready()` holds, it has at least
/// `min_threads` threads, and its CPU time stayed the same over four
/// samples 50 ms apart. Panics past the deadline or when it ended.
fn wait_held(child: &mut Child, min_threads: usize, ready: impl Fn() -> bool) {
    let until = Instant::now() + DEADLINE;
    let mut last = String::new();
    let mut steady = 0;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("layerfs ended before it was held: {status:?}");
        }
        assert!(
            Instant::now() < until,
            "layerfs was not held within {DEADLINE:?} (threads {}, cpu {last})",
            threads(child.id())
        );
        let now = cpu(child.id());
        if ready() && threads(child.id()) >= min_threads && now == last {
            steady += 1;
            if steady == 3 {
                return;
            }
        } else {
            steady = 0;
        }
        last = now;
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Holds `layerfs <words>` at its write of a refusal to a full standard
/// error (a home that does not exist and that the call creates), then
/// sends `signals`; `min_threads` is 2 for a call with the signal thread.
fn held(tag: &str, words: &[&str], min_threads: usize, signals: &[Signal]) -> (Child, File) {
    let home = tempdir(tag).join("missing");
    // the first run of a fresh binary waits on the system's check of it
    let _ = common::run(&mut layerfs(&home, &["--version"]));
    let (stderr, reader) = full_pipe(tag);
    let mut child = layerfs(&home, words)
        .stdout(Stdio::null())
        .stderr(stderr)
        .spawn()
        .unwrap();
    // past dispatch (the call made the home) and blocked in the write
    wait_held(&mut child, min_threads, || home.exists());
    for which in signals {
        signal(&child, *which);
        std::thread::sleep(Duration::from_millis(100));
    }
    (child, reader)
}

#[test]
fn another_call_keeps_the_default_a_signal_ends_it() {
    for (which, tag) in [(Signal::SIGINT, "int"), (Signal::SIGTERM, "term")] {
        let tag = format!("default-{tag}");
        let (child, reader) = held(&tag, &["store", "get", "shop", "--json"], 1, &[which]);
        let (status, _) = drain(reader, child);
        assert_eq!(status.signal(), Some(which as i32), "{status:?}");
    }
}

#[test]
fn store_upgrade_blocks_both_signals_and_after_its_answer_they_change_nothing() {
    // two signals, SIGINT then SIGTERM, after the call answered: the
    // process neither dies nor exits Unknown; it prints the call's own
    // refusal (no home here) and exits 125
    let words = ["store", "upgrade", "shop", "--json"];
    let (mut child, reader) = held("answered", &words, 2, &[Signal::SIGINT, Signal::SIGTERM]);
    std::thread::sleep(Duration::from_millis(200));
    let early = child.try_wait().unwrap();
    let (status, stderr) = drain(reader, child);
    assert!(
        early.is_none(),
        "the signals ended it: {early:?} {}",
        text(&stderr).trim()
    );
    assert_eq!(status.code(), Some(125), "{status:?} {}", text(&stderr));
    let refusal = last_json(&stderr);
    assert_ne!(refusal.get_text("error"), Some("Unknown"), "{refusal:?}");
    assert!(!text(&stderr).contains("second signal"));
}

// ---- INTEGRATION (#804 W1 + W2 and the fixture) ----

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../fixtures/store-formats/sqlite/v2/store.sqlite"
);

/// A home with the frozen schema-2 fixture installed as Store `fx` on
/// `native`, through the `sqlite3` command (the record takes the fixture's
/// binding; each project gets the minimal record a Store call needs).
/// `None` when the fixture or `sqlite3` is missing.
fn fixture_home(tag: &str) -> Option<PathBuf> {
    if !Path::new(FIXTURE).exists() {
        eprintln!("NOT_RUN: {FIXTURE} is missing");
        return None;
    }
    let binding = Command::new("sqlite3")
        .arg(format!("file:{FIXTURE}?immutable=1"))
        .arg("SELECT lower(hex(binding_key)) FROM history_meta")
        .output()
        .ok()
        .filter(|out| out.status.success())?;
    let binding = text(&binding.stdout).trim().to_owned();
    let home = tempdir(tag).join("home");
    for words in [&["home", "init"][..], &["store", "create", "fx"]] {
        let out = common::run(&mut layerfs(&home, words));
        assert!(out.status.success(), "{words:?}: {}", text(&out.stderr));
    }
    let store = home.join("store").join("fx");
    let record = std::fs::read_to_string(store.join("record.json")).unwrap();
    let start = record.find(r#""binding":{"hex":""#).unwrap() + r#""binding":{"hex":""#.len();
    let end = start + record[start..].find('"').unwrap();
    let record = format!("{}{binding}{}", &record[..start], &record[end..]);
    std::fs::write(store.join("record.json"), record).unwrap();
    std::fs::copy(FIXTURE, store.join("store.sqlite")).unwrap();
    for project in ["lib", "web"] {
        let dir = store.join("project").join(project);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("record.json"), "{\"v\":1}\n").unwrap();
    }
    Some(home)
}

/// Whether this build has the upgrade (the skeleton refuses "not built yet").
fn built(home: &Path) -> bool {
    let probe = common::run(&mut layerfs(
        home,
        &["store", "get", "fx", "--with", "format"],
    ));
    let built = text(&probe.stdout).contains("\"format\"");
    if !built {
        eprintln!("NOT_RUN: this build has no store upgrade yet");
    }
    built
}

fn file_bytes(home: &Path) -> Vec<u8> {
    std::fs::read(home.join("store/fx/store.sqlite")).unwrap()
}

/// INTEGRATION. SIGINT (or SIGTERM) once while the call is held at its first
/// progress event (`check`): the cancel flag is set and the call answers
/// what control answers, `Failed` with `detail.code: "Cancelled"`, and the
/// file is unchanged byte for byte.
#[test]
fn one_signal_at_check_cancels_and_changes_nothing() {
    for (which, tag) in [(Signal::SIGINT, "int"), (Signal::SIGTERM, "term")] {
        let Some(home) = fixture_home(&format!("cancel-{tag}")) else {
            return;
        };
        if !built(&home) {
            return;
        }
        let before = file_bytes(&home);
        let (stderr, reader) = full_pipe(&format!("cancel-{tag}"));
        let child = held_at_check(&home, stderr);
        signal(&child, which);
        std::thread::sleep(Duration::from_millis(200));
        let (status, stderr) = drain(reader, child);
        let said = text(&stderr);
        assert_eq!(status.code(), Some(125), "{status:?} {said}");
        assert!(said.contains(r#"{"phase":"check""#), "{said}");
        let refusal = last_json(&stderr);
        assert_eq!(refusal.get_text("error"), Some("Failed"), "{refusal:?}");
        let detail = refusal.get("detail").unwrap();
        assert_eq!(detail.get_text("code"), Some("Cancelled"), "{refusal:?}");
        assert_eq!(
            refusal.get_text("message"),
            Some("cancelled before COMMIT; nothing changed")
        );
        assert!(file_bytes(&home) == before, "the file changed");
        // the Store is still outdated, and the next upgrade does the work
        let out = common::run(&mut layerfs(&home, &["store", "upgrade", "fx", "--json"]));
        assert_eq!(text(&out.stdout).trim(), r#"{"outcome":"upgraded"}"#);
    }
}

/// `layerfs store upgrade fx --progress` with standard error `stderr` (a
/// full pipe), held at its write of the first progress event (`check`),
/// which it makes holding the standard error's lock.
fn held_at_check(home: &Path, stderr: Stdio) -> Child {
    let _ = common::run(&mut layerfs(home, &["--version"]));
    let mut child = layerfs(
        home,
        &["store", "upgrade", "fx", "--json", "--progress", "--detail"],
    )
    .env("LAYERFS_PROGRESS_JSON", "1")
    .stdout(Stdio::null())
    .stderr(stderr)
    .spawn()
    .unwrap();
    wait_held(&mut child, 2, || true);
    child
}

/// A Unix socket the engine's failpoint gate connects to (INTERFACES §1):
/// returns the path and what receives the connection once its readiness
/// line came, each step bounded by the deadline.
fn gate(
    dir: &Path,
) -> (
    PathBuf,
    std::sync::mpsc::Receiver<std::os::unix::net::UnixStream>,
) {
    let path = dir.join("gate.sock");
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let (sent, ready) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let until = Instant::now() + DEADLINE;
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    if Instant::now() > until {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("the gate's accept: {error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream.set_read_timeout(Some(DEADLINE)).unwrap();
        let mut byte = [0u8; 1];
        // one readiness line
        while matches!(stream.read(&mut byte), Ok(1)) && byte[0] != b'\n' {}
        let _ = sent.send(stream);
    });
    (path, ready)
}

/// `layerfs store upgrade fx` stopped at the engine's failpoint `n` (after
/// its statement `n`), blocked there until it is killed.
fn at_failpoint(home: &Path, tag: &str, n: u32) -> (Child, std::os::unix::net::UnixStream) {
    let dir = tempdir(&format!("gate-{tag}"));
    let (path, ready) = gate(&dir);
    let child = Command::new(LAYERFS)
        .args(["store", "upgrade", "fx", "--json"])
        .env("LAYERFS_HOME", home)
        .env("LAYERFS_UPGRADE_CRASH_GATE", &path)
        .env("LAYERFS_UPGRADE_CRASH_FAILPOINT", n.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stream = ready.recv_timeout(DEADLINE).unwrap_or_else(|_| {
        panic!("the failpoint gate {n} was not reached (or sent no readiness line) within {DEADLINE:?}")
    });
    (child, stream)
}

/// INTEGRATION. A second signal exits at once, with `Unknown` and its fix,
/// while the call is held in the engine (the failpoint gate); the first set
/// the flag and changed nothing. The next `store upgrade` adopts the
/// unresolved op and converges.
#[test]
fn a_second_signal_exits_at_once_unknown_and_the_next_upgrade_converges() {
    let Some(home) = fixture_home("second") else {
        return;
    };
    if !built(&home) {
        return;
    }
    let (mut child, _gate) = at_failpoint(&home, "second", 1);
    signal(&child, Signal::SIGINT);
    std::thread::sleep(Duration::from_millis(300));
    assert!(child.try_wait().unwrap().is_none(), "one signal ended it");
    let sent = Instant::now();
    signal(&child, Signal::SIGINT);
    let out = common::finish(child);
    assert!(sent.elapsed() < Duration::from_secs(5), "not at once");
    assert_eq!(out.status.code(), Some(125), "{out:?}");
    let refusal = last_json(&out.stderr);
    assert_eq!(refusal.get_text("error"), Some("Unknown"), "{refusal:?}");
    assert_eq!(refusal.get_text("fix"), Some("layerfs store upgrade fx"));
    let next = common::run(&mut layerfs(&home, &["store", "upgrade", "fx", "--json"]));
    assert_eq!(
        text(&next.stdout).trim(),
        r#"{"outcome":"upgraded"}"#,
        "{}",
        text(&next.stderr)
    );
}

/// INTEGRATION. The second signal exits at once even while the call's
/// thread holds the standard error's lock, blocked writing its first
/// progress event to a full pipe: the refusal is written without that lock
/// (best effort; a full pipe may never take it) and the process exits 125.
/// On a build whose exit path takes the lock, the process never ends.
#[test]
fn a_second_signal_exits_while_the_call_blocks_on_a_full_standard_error() {
    let Some(home) = fixture_home("second-full") else {
        return;
    };
    if !built(&home) {
        return;
    }
    let before = file_bytes(&home);
    let (stderr, reader) = full_pipe("second-full");
    let mut child = held_at_check(&home, stderr);
    // two different signals, so the second is never merged into the first
    signal(&child, Signal::SIGINT);
    signal(&child, Signal::SIGTERM);
    let until = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > until {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the second signal did not end layerfs while its stderr was full");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(status.code(), Some(125), "{status:?}");
    drop(reader);
    // the call stopped before its check ran: nothing changed
    assert!(file_bytes(&home) == before, "the file changed");
}

/// INTEGRATION. A killed command (SIGKILL at the failpoint) leaves the Store
/// wholly old; the next `store upgrade` adopts and converges, and a third
/// answers `current`.
#[test]
fn a_killed_command_is_adopted_by_the_next_upgrade() {
    let Some(home) = fixture_home("killed") else {
        return;
    };
    if !built(&home) {
        return;
    }
    let (mut child, _gate) = at_failpoint(&home, "killed", 1);
    child.kill().unwrap();
    child.wait().unwrap();
    let next = common::run(&mut layerfs(&home, &["store", "upgrade", "fx", "--json"]));
    assert_eq!(
        text(&next.stdout).trim(),
        r#"{"outcome":"upgraded"}"#,
        "{}",
        text(&next.stderr)
    );
    let again = common::run(&mut layerfs(&home, &["store", "upgrade", "fx", "--json"]));
    assert_eq!(text(&again.stdout).trim(), r#"{"outcome":"current"}"#);
}
