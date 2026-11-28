//! AC-01 (§12 crash injection, §9): `kill -9` the **real process** that
//! carries `store.compact`, not a re-execution of this test binary.
//!
//! The in-process matrix (`engine/crates/layerfs-persistence/tests/
//! sqlite_compact_crash.rs`) stops at a failpoint inside the test process, and
//! its `real_process` arm re-executes the *test binary*, so the killed process
//! is still a test child. This test starts the built `layerfs` command with
//! `store compact`, waits through the SQLite engine's Unix-socket gate until
//! that process stands inside the run, and `SIGKILL`s it there. Per #352
//! F-020 no daemon serves Store calls: the caller's process performs the
//! compaction, so this is the process the §9 crash describes. (It is still not
//! a long-lived `layerfs worker` serving a machine's Store.)
//!
//! Every arm reopens the Store through the CLI and asserts the §9 crash state:
//! wholly old at `G` or wholly new at `G + 1`, the exact artifact set, the
//! sidecars a killed session leaves, `store get --with live` (`broken`
//! `unresolved`, its fix the next compaction), the next `store.compact` settling the leftover
//! (an installed `G + 1` answers the recorded `Rebuild` report, F41-1), the
//! retained layer still readable, and a clean `store doctor`. The `G` baseline
//! is the §3 `none` run before the dead pack exists, so the crash arms start
//! from a Store with exactly one wholly dead pack.
//!
//! Linux only: the arm asserts `SIGKILL` delivery through
//! `ExitStatusExt::signal`. A macOS build has no gate in the engine, so the
//! hook is inert there by construction.
#![cfg(target_os = "linux")]
use crate::common;

use common::{layerfs, run, run_with_stdin, tempdir, text, LAYERFS};
use layerfs_home::json;
use layerfs_protocol::sdk::SdkValue;
use std::io::Read;
use std::os::unix::net::UnixListener;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// The engine's gate: a run that reaches the named failpoint connects to this
/// socket and blocks there. Both variables are read by the production
/// `store.compact` path only when the gate is present.
const GATE_ENV: &str = "LAYERFS_COMPACT_CRASH_GATE";
const FAILPOINT_ENV: &str = "LAYERFS_COMPACT_CRASH_FAILPOINT";

/// A short socket path: the sandbox's own root is longer than `SUN_LEN`.
fn gate_socket() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    PathBuf::from(format!(
        "/tmp/layerfs-g65-gate-{}-{}.sock",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

/// One crash point: the failpoint, the phase the run stood in,
/// whether the rebuild is already installed at `G + 1`, and which staged
/// artifacts the kill leaves beside the Store.
#[derive(Clone, Copy, Debug)]
struct Arm {
    failpoint: &'static str,
    phase: &'static str,
    installed: bool,
    target: bool,
    visited: bool,
    /// A kill before the seal ends the process with the writer session's WAL
    /// on disk (`-wal`, `-shm`); every later point has closed it.
    live_sidecars: bool,
}

/// `layerfs <words>` on `home`, parsing the one-line JSON result.
fn call(home: &Path, words: &[&str]) -> SdkValue {
    let out = run(&mut layerfs(home, words));
    assert_eq!(
        out.status.code(),
        Some(0),
        "{words:?}: {}",
        text(&out.stderr)
    );
    json::parse(text(&out.stdout).trim()).unwrap_or_else(|error| {
        panic!("{words:?}: {error:?}: {}", text(&out.stdout));
    })
}

/// `store.compact s`, the call whose process the crash arms kill, long form.
fn compact(home: &Path) -> SdkValue {
    call(home, &["store", "compact", "s", "--detail"])
}

fn generations(report: &SdkValue) -> (u64, u64) {
    let before = report
        .get_int("generation_before")
        .expect("generation_before");
    let after = report
        .get_int("generation_after")
        .expect("generation_after");
    (before as u64, after as u64)
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

/// Kills `store.compact` for real: waits until the process is stopped inside
/// the run at `failpoint`, then `SIGKILL`s it. The gate keeps the wait from
/// being a sleep, and a process that ends early (a refusal) fails the arm with
/// its own stderr.
fn crash_and_kill(home: &Path, failpoint: &'static str) {
    let gate = gate_socket();
    let _ = std::fs::remove_file(&gate);
    let listener = UnixListener::bind(&gate).expect("the crash gate socket");
    listener
        .set_nonblocking(true)
        .expect("a nonblocking crash gate");
    let mut child = Command::new(LAYERFS)
        .args(["store", "compact", "s"])
        .env("LAYERFS_ROOT", crate::root::root_of(home))
        .env(GATE_ENV, &gate)
        .env(FAILPOINT_ENV, failpoint)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the real store.compact process");

    let deadline = Instant::now() + Duration::from_secs(60);
    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if let Some(status) = child.try_wait().expect("the child's status") {
                    let mut stderr = String::new();
                    if let Some(mut pipe) = child.stderr.take() {
                        let _ = pipe.read_to_string(&mut stderr);
                    }
                    panic!("store.compact ended before the gate ({failpoint}): {status}: {stderr}");
                }
                assert!(
                    Instant::now() < deadline,
                    "store.compact never reached the gate ({failpoint})"
                );
                std::thread::yield_now();
            }
            Err(error) => panic!("crash gate accept: {error}"),
        }
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(60)))
        .expect("a bounded gate read");
    let mut ready = [0u8; 1];
    let mut reader = &stream;
    reader
        .read_exact(&mut ready)
        .expect("the child reached the crash gate");
    assert_eq!(ready[0], b'R', "the child stopped at the crash gate");

    // The real §9 crash: SIGKILL the process that performs the compaction.
    child.kill().expect("SIGKILL the store.compact process");
    let status = child.wait().expect("the killed process exits");
    assert_eq!(
        status.signal(),
        Some(9),
        "the store.compact process was SIGKILLed: {status:?}"
    );
    drop(stream);
    let _ = std::fs::remove_file(&gate);
}

/// The fixture: a Store, one project, a Branch whose commit writes a large
/// blob, and then a `§3 none` baseline that names `G` without moving it.
fn fixture(tag: &str) -> (PathBuf, PathBuf, u64) {
    let home = tempdir(&format!("g65-home-{tag}"));
    let source = tempdir(&format!("g65-src-{tag}"));
    std::fs::write(source.join("a.txt"), b"the retained bytes\n").unwrap();
    call(&home, &["store", "create", "s"]);
    call(&home, &["project", "init", "s/p", source.to_str().unwrap()]);
    call(&home, &["branch", "fork", "s/p", "dev", "main"]);
    // A large blob so its pack is wholly dead once `dev` is soft-removed, and
    // large enough that the rebuild threshold (`dead >= 10% of total`) holds.
    let big: Vec<u8> = (0..256 * 1024).map(|at| (at % 251) as u8).collect();
    let written = run_with_stdin(
        &mut layerfs(&home, &["file", "write", "s/p", "dev", "/big"]),
        &big,
    );
    assert!(written.status.success(), "{}", text(&written.stderr));

    // Nothing is dead yet: §3 `none`, so the generation does not move. This
    // names `G`, the generation every crash arm starts from.
    let baseline = compact(&home);
    assert_eq!(
        baseline.get_text("outcome"),
        Some("unchanged"),
        "{baseline:?}"
    );
    let (before, after) = generations(&baseline);
    assert_eq!(before, after, "a `none` run moves no generation");

    // One wholly dead pack: the Branch that wrote `/big` is soft-removed and
    // its objects become unreachable; the next run must reclaim them.
    call(&home, &["branch", "remove", "s/p", "dev"]);
    (home, source, before)
}

fn exercise(arm: Arm) {
    let (home, source, generation) = fixture(&arm.failpoint.to_lowercase());
    let store = home.join("store/s/store.sqlite");
    assert!(store.is_file(), "{}", store.display());

    crash_and_kill(&home, arm.failpoint);

    // §9 crash state, read before any handle opens the Store: the exact
    // artifact set beside the live file, and the sidecars the kill leaves.
    assert_eq!(
        with_suffix(&store, ".compact").exists(),
        arm.target,
        "staged Store at {}",
        arm.failpoint
    );
    assert_eq!(
        with_suffix(&store, ".compact-visited").exists(),
        arm.visited,
        "visited scratch at {}",
        arm.failpoint
    );
    assert!(
        with_suffix(&store, ".compact-op").exists(),
        "op record at {}",
        arm.failpoint
    );
    if arm.live_sidecars {
        assert!(
            with_suffix(&store, "-wal").exists(),
            "the killed session's WAL at {}",
            arm.failpoint
        );
        assert!(
            with_suffix(&store, "-shm").exists(),
            "the killed session's shared memory at {}",
            arm.failpoint
        );
    }
    for suffix in ["-wal", "-shm", "-journal"] {
        if arm.live_sidecars && suffix != "-journal" {
            continue;
        }
        assert!(
            !with_suffix(&store, suffix).exists(),
            "sidecar {suffix} beside the Store at {}",
            arm.failpoint
        );
        assert!(
            !with_suffix(&with_suffix(&store, ".compact"), suffix).exists(),
            "sidecar {suffix} beside the staged Store at {}",
            arm.failpoint
        );
    }

    // `store get --with live` (§11) reads the recorded run: its owner died
    // with the outcome unknown, so the Store is `broken` `unresolved` and
    // its fix is the next compaction. (The phase it stood in is no longer
    // part of the answer: `op` shows only while its owner lives.)
    let _ = arm.phase;
    let observed = call(&home, &["store", "get", "s", "--with", "live", "--detail"]);
    assert_eq!(observed.get_text("state"), Some("broken"), "{observed:?}");
    assert_eq!(
        observed.get_text("reason"),
        Some("unresolved"),
        "{observed:?}"
    );
    assert_eq!(
        observed.get_text("fix"),
        Some("layerfs store compact s"),
        "{observed:?}"
    );

    // The next run judges the leftover: a rebuild from `G` before install, and
    // the recorded `Rebuild` G -> G + 1 report after it (F41-1: an installed
    // `G + 1` answers that report instead of re-deciding to `none` on the
    // already rebuilt Store), and no residue either way.
    let recovered = compact(&home);
    assert_eq!(
        recovered.get_text("outcome"),
        Some("compacted"),
        "{recovered:?}"
    );
    assert_eq!(
        generations(&recovered),
        (generation, generation + 1),
        "{recovered:?}"
    );
    if arm.installed {
        assert!(
            recovered.get_int("dead_bytes").unwrap_or(0) > 0,
            "the adopted report keeps the collected bytes at {}: {recovered:?}",
            arm.failpoint
        );
    }
    assert_eq!(recovered.get_int("residue_bytes"), Some(0), "{recovered:?}");
    assert!(
        matches!(recovered.get("residue_artifacts"), Some(SdkValue::List(items)) if items.is_empty()),
        "{recovered:?}"
    );

    // Nothing is left beside the Store, and the op record is settled.
    for path in [
        with_suffix(&store, ".compact"),
        with_suffix(&store, ".compact-visited"),
        with_suffix(&store, ".compact-op"),
    ] {
        assert!(
            !path.exists(),
            "{} left at {}",
            path.display(),
            arm.failpoint
        );
    }
    for suffix in ["-wal", "-shm", "-journal"] {
        assert!(
            !with_suffix(&store, suffix).exists(),
            "sidecar {suffix} left at {}",
            arm.failpoint
        );
    }
    let settled = call(&home, &["store", "get", "s", "--with", "live", "--detail"]);
    assert_eq!(settled.get("op"), Some(&SdkValue::Null), "{settled:?}");
    assert_eq!(settled.get_text("state"), Some("ready"), "{settled:?}");

    // The retained layer still reads, and the home passes the doctor: the
    // dead pack is collected while the live one is intact.
    let stats = call(&home, &["project", "stats", "s/p"]);
    assert!(stats.get_int("file_count").unwrap_or(0) >= 1, "{stats:?}");
    call(&home, &["file", "stat", "s/p", "main", "/a.txt"]);
    let doctor = call(&home, &["store", "doctor", "s", "--detail"]);
    let Some(SdkValue::List(findings)) = doctor.get("findings") else {
        panic!("{doctor:?}");
    };
    // The two calls above ran in the host worker (#819), which keeps its
    // warm Store lease; the doctor names that lease as a notice
    // (`store.operation`). That notice alone is allowed: one holder, the
    // host worker, and no other process.
    for finding in findings {
        assert_eq!(
            finding.get_text("check"),
            Some("store.operation"),
            "{doctor:?}"
        );
        assert_eq!(finding.get_text("severity"), Some("notice"), "{doctor:?}");
        let message = finding.get_text("message").unwrap_or_default();
        assert!(message.contains("including host worker"), "{doctor:?}");
        let evidence = finding.get("evidence").expect("lease evidence");
        let Some(SdkValue::List(holders)) = evidence.get("holders") else {
            panic!("{doctor:?}");
        };
        assert_eq!(holders.len(), 1, "{doctor:?}");
        assert_eq!(
            holders[0].get_text("kind"),
            Some("host_worker"),
            "{doctor:?}"
        );
        assert_eq!(evidence.get_int("holder_count"), Some(1), "{doctor:?}");
    }

    std::fs::remove_dir_all(&home).unwrap();
    std::fs::remove_dir_all(&source).unwrap();
}

macro_rules! sigkill_at {
    ($name:ident, $failpoint:literal, $phase:literal, $installed:literal, $target:literal, $visited:literal, $live_sidecars:literal) => {
        #[test]
        fn $name() {
            exercise(Arm {
                failpoint: $failpoint,
                phase: $phase,
                installed: $installed,
                target: $target,
                visited: $visited,
                live_sidecars: $live_sidecars,
            });
        }
    };
}

// The mark phase: the open session and admission intent at `BeforeSeal` (the
// kill leaves the writer's WAL sidecars), the sealed Store at `AfterSeal`.
sigkill_at!(
    sigkill_before_seal,
    "BeforeSeal",
    "mark",
    false,
    false,
    true,
    true
);
sigkill_at!(
    sigkill_after_seal,
    "AfterSeal",
    "mark",
    false,
    false,
    true,
    false
);
// The build: immediately before the fresh file's first write, and after the
// staged Store is built.
sigkill_at!(
    sigkill_before_load,
    "BeforeLoad",
    "build",
    false,
    false,
    true,
    false
);
sigkill_at!(
    sigkill_after_build,
    "AfterBuild",
    "build",
    false,
    true,
    true,
    false
);
// The verification and the install boundary.
sigkill_at!(
    sigkill_during_verify,
    "DuringVerify",
    "verify",
    false,
    true,
    true,
    false
);
sigkill_at!(
    sigkill_before_install,
    "BeforeInstall",
    "verify",
    false,
    true,
    true,
    false
);
// Installed at `G + 1`: after the rename, and partway through the cleanup
// settle (the phase §11 records for it).
sigkill_at!(
    sigkill_after_install,
    "AfterInstall",
    "install",
    true,
    false,
    true,
    false
);
sigkill_at!(
    sigkill_during_settle,
    "DuringSettle",
    "cleanup",
    true,
    false,
    false,
    false
);
