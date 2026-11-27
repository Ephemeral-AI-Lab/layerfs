#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]

#[path = "common/host_worker.rs"]
mod fixture;
#[path = "common/host_worker_proof.rs"]
mod proof;

use std::process::Stdio;
use std::time::{Duration, Instant};

fn check_readers(callers: usize, accepted: i64) {
    let home = proof::create();
    let before = proof::commits(&home);
    let mut worker = home.start();
    let initial = home.ping();
    proof::assert_reads(proof::spawn_reads(&home, 3, false));
    let warm = proof::pool(&home, 0, 0, 0);
    assert_eq!(warm.get_int("stores"), Some(1), "{warm:?}");
    assert!(proof::lease_file(&home, worker.pid()).is_file());

    #[cfg(target_os = "linux")]
    let idle_threads = proof::threads(worker.pid());

    let gate = proof::lock_store(&home);
    let reads = proof::spawn_reads(&home, callers, false);
    let full = proof::pool(&home, accepted, 8, accepted - 8);
    assert_eq!(full.get_text("state"), Some("ready"));
    assert_eq!(full.get_int("control"), Some(1));
    assert_eq!(full.get_int("stores"), Some(1));
    assert_eq!(full.get_text("generation"), initial.get_text("generation"));
    #[cfg(target_os = "linux")]
    {
        proof::wait_for("all caller sockets to connect", || {
            reads.iter().all(proof::Cli::connected).then_some(())
        });
        // Three listeners and the wake pair plus accepted ordinary sockets
        // and up to two reserved management connections.
        assert!(proof::sockets(worker.pid()) <= 5 + accepted as usize + 2);
        assert_eq!(proof::threads(worker.pid()), idle_threads);
    }
    drop(gate);

    proof::assert_reads(reads);
    let after = proof::pool(&home, 0, 0, 0);
    assert_eq!(after.get_int("stores"), Some(1));
    assert_eq!(after.get_int("pid"), Some(worker.pid()));
    assert_eq!(proof::commits(&home), before);
    home.drain();
    assert!(worker.wait(Duration::from_secs(5)).success());
    assert!(!proof::lease_file(&home, worker.pid()).exists());
}

#[test]
fn ten_cli_reads_share_the_real_warm_store() {
    check_readers(10, 10);
}

#[test]
fn twenty_cli_reads_clear_the_backlog_and_leave_management_available() {
    check_readers(20, 16);
}

#[test]
fn callers_queued_behind_slow_calls_wait_past_the_hello_deadline() {
    // §16.1 C7: 8 calls hold every ordinary thread for longer than the
    // 10 s hello deadline; 8 more wait accepted and 4 in the listen queue.
    let home = proof::create();
    let before = proof::commits(&home);
    let mut worker = home.start();
    proof::assert_reads(proof::spawn_reads(&home, 1, false));
    let gate = proof::lock_store(&home);
    let reads = proof::spawn_reads(&home, 20, false);
    proof::pool(&home, 16, 8, 8);
    std::thread::sleep(Duration::from_secs(12));
    let held = proof::pool(&home, 16, 8, 8);
    assert_eq!(held.get_text("state"), Some("ready"));
    drop(gate);

    proof::assert_reads(reads);
    assert_eq!(
        proof::pool(&home, 0, 0, 0).get_int("pid"),
        Some(worker.pid())
    );
    assert_eq!(proof::commits(&home), before);
    home.drain();
    assert!(worker.wait(Duration::from_secs(5)).success());
}

#[test]
fn a_caller_behind_a_wedged_worker_is_refused_before_sending() {
    // §11: a caller that gets no hello within its 30 s bound is refused
    // `Unavailable` naming the load, and nothing was sent. Runs about 31 s.
    let home = proof::create();
    let mut worker = home.start();
    proof::assert_reads(proof::spawn_reads(&home, 1, false));
    let signal = |name: &str| {
        let status = std::process::Command::new("kill")
            .args([name, &worker.pid().to_string()])
            .status()
            .unwrap();
        assert!(status.success());
    };
    signal("-STOP");
    let began = Instant::now();
    let mut read = std::process::Command::new(env!("CARGO_BIN_EXE_layerfs"))
        .args(["file", "read", "shop/api", "main", "/hello"])
        .env("LAYERFS_HOME", &home.paths.home)
        .env_remove("LAYERFS_TRACE")
        .env_remove("LAYERFS_TRACE_STEP")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    while read.try_wait().unwrap().is_none() {
        if began.elapsed() > Duration::from_secs(45) {
            let _ = read.kill();
            signal("-CONT");
            panic!("the queued read was not bounded");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let waited = began.elapsed();
    signal("-CONT");
    let output = read.wait_with_output().unwrap();
    assert!(!output.status.success(), "{output:?}");
    let text = String::from_utf8_lossy(if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    })
    .into_owned();
    let refusal = layerfs_api::json::parse(text.trim()).unwrap();
    assert_eq!(refusal.get_text("error"), Some("Unavailable"), "{text}");
    let message = refusal.get_text("message").unwrap_or_default();
    assert!(message.contains("no call thread was free"), "{text}");
    assert!(message.contains("no operation was sent"), "{text}");
    assert!(waited >= Duration::from_secs(29), "{waited:?}");
    // The resumed worker serves the next call.
    proof::assert_reads(proof::spawn_reads(&home, 1, false));
    home.drain();
    assert!(worker.wait(Duration::from_secs(5)).success());
}
