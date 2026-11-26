#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]

#[path = "common/host_worker.rs"]
mod fixture;
#[path = "common/host_worker_proof.rs"]
mod proof;

use std::time::Duration;

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
        // Two listeners and the wake pair plus accepted ordinary sockets
        // and up to two reserved management connections.
        assert!(proof::sockets(worker.pid()) <= 4 + accepted as usize + 2);
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
