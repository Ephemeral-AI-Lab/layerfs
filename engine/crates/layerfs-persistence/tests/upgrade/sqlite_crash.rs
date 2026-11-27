//! G and J for SQLite `store upgrade` (#804 §11): the upgrade runs in a
//! separate process that stops at the test failpoint after statement `n`
//! (every statement from `BEGIN` to `COMMIT`), reports on the crash gate and
//! is SIGKILLed there: no Drop, no ROLLBACK and no close runs. The Store is
//! then wholly old (before COMMIT) or wholly new (after it), its format
//! read says which, and a re-run converges. At the stop after COMMIT and
//! before the checkpoint the WAL holds exactly one commit frame: the
//! upgrade's one write transaction (J).
#![cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "../support/mod.rs"]
mod support;

use layerfs_persistence::read_heads;
use layerfs_persistence::upgrade::{sqlite_format, upgrade_sqlite, UpgradeOutcome, UpgradeRequest};
use std::io::Read;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};
use support::upgrade::{sqlite_fixture_copy, SQLITE_V2};
use support::Temp;

const STORE_ENV: &str = "LAYERFS_UPGRADE_CRASH_STORE";
const FAILPOINT_ENV: &str = "LAYERFS_UPGRADE_CRASH_FAILPOINT";
const GATE_ENV: &str = "LAYERFS_UPGRADE_CRASH_GATE";
/// `BEGIN IMMEDIATE`, step 2 -> 3's eleven statements, `COMMIT`.
const STATEMENTS: u32 = 13;

/// The child body, run only by the parent's re-exec: on its own it is
/// NOT_RUN.
#[test]
fn crash_child() {
    let Some(store) = std::env::var_os(STORE_ENV) else {
        eprintln!("NOT_RUN: crash_child runs only inside the crash test's child process");
        return;
    };
    let failpoint: u32 = std::env::var(FAILPOINT_ENV).unwrap().parse().unwrap();
    let request = UpgradeRequest {
        cancel: &AtomicBool::new(false),
        failpoint: Some(failpoint),
    };
    let outcome = upgrade_sqlite(Path::new(&store), &request, &mut |_| {});
    panic!("the parent did not SIGKILL the run: {outcome:?}");
}

struct KillOnDrop(Option<Child>);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

/// `(frames, commit frames)` of the WAL beside `store` whose salts match
/// its header (the frames SQLite's recovery would consider).
fn wal_frames(store: &Path) -> (usize, usize) {
    let Ok(bytes) = std::fs::read(with_suffix(store, "-wal")) else {
        return (0, 0);
    };
    if bytes.len() < 32 {
        return (0, 0);
    }
    let page = u32::from_be_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let salts = &bytes[16..24];
    let (mut frames, mut commits, mut at) = (0, 0, 32);
    while at + 24 + page <= bytes.len() && &bytes[at + 8..at + 16] == salts {
        frames += 1;
        if bytes[at + 4..at + 8] != [0; 4] {
            commits += 1;
        }
        at += 24 + page;
    }
    (frames, commits)
}

fn user_version(path: &Path) -> i64 {
    let c = rusqlite::Connection::open(path).unwrap();
    let v = c
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    let check: String = c
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(check, "ok");
    c.close().unwrap();
    v
}

/// Runs the child to failpoint `n`, kills it there and returns what the
/// WAL held at the kill.
fn kill_at(store: &Path, n: u32) -> (usize, usize) {
    let gate = PathBuf::from(format!(
        "/tmp/lfs-upgrade-gate-{}-{n}.sock",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&gate);
    let listener = std::os::unix::net::UnixListener::bind(&gate).unwrap();
    listener.set_nonblocking(true).unwrap();
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "crash_child", "--nocapture"])
        .env(STORE_ENV, store)
        .env(FAILPOINT_ENV, n.to_string())
        .env(GATE_ENV, &gate)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut child = KillOnDrop(Some(child));
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if let Some(status) = child.0.as_mut().unwrap().try_wait().unwrap() {
                    panic!("failpoint {n}: the child ended before the gate: {status}");
                }
                assert!(Instant::now() < deadline, "failpoint {n}: never reached");
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(e) => panic!("gate: {e}"),
        }
    };
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    let mut ready = [0u8; 2];
    stream.read_exact(&mut ready).unwrap();
    assert_eq!(&ready, b"R\n", "failpoint {n}: one readiness line");
    let wal = wal_frames(store);
    let mut running = child.0.take().unwrap();
    running.kill().unwrap();
    let status = running.wait().unwrap();
    assert_eq!(status.signal(), Some(9), "failpoint {n}: SIGKILLed");
    drop(stream);
    let _ = std::fs::remove_file(&gate);
    wal
}

#[test]
fn killed_after_every_statement_the_store_is_wholly_old_or_new_and_a_rerun_converges() {
    for (variant, folder) in SQLITE_V2 {
        crash_sweep(variant, folder);
    }
}

fn crash_sweep(variant: &str, folder: &str) {
    let temp = Temp::new("upgrade-crash-heads");
    let heads = read_heads(&sqlite_fixture_copy(temp.path(), folder)).unwrap();
    for n in 1..=STATEMENTS {
        let temp = Temp::new("upgrade-crash");
        let store = sqlite_fixture_copy(temp.path(), folder);
        let (frames, commits) = kill_at(&store, n);
        let committed = n == STATEMENTS;
        // J: the upgrade's one write transaction is one WAL commit frame.
        assert_eq!(
            commits,
            usize::from(committed),
            "{variant} failpoint {n}: {frames} frames"
        );
        let format = sqlite_format(&store).unwrap();
        assert_eq!(
            format.version,
            Some(if committed { 3 } else { 2 }),
            "failpoint {n}"
        );
        assert_eq!(
            user_version(&store),
            if committed { 62 } else { 42 },
            "failpoint {n}"
        );
        let request = UpgradeRequest {
            cancel: &AtomicBool::new(false),
            failpoint: None,
        };
        let report = upgrade_sqlite(&store, &request, &mut |_| {}).unwrap();
        let expected = if committed {
            UpgradeOutcome::Current
        } else {
            UpgradeOutcome::Upgraded
        };
        assert_eq!(report.outcome, expected, "failpoint {n}");
        assert!(
            report.warnings.is_empty(),
            "failpoint {n}: {:?}",
            report.warnings
        );
        assert_eq!(user_version(&store), 62);
        assert_eq!(read_heads(&store).unwrap(), heads, "failpoint {n}");
    }
}
