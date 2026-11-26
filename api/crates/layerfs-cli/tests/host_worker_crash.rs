#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]

#[path = "common/host_worker.rs"]
mod fixture;
#[path = "common/host_worker_proof.rs"]
mod proof;

use std::collections::BTreeSet;
use std::time::Duration;

#[test]
fn a_killed_cli_caller_does_not_strand_the_other_nine() {
    let home = proof::create();
    let before = proof::commits(&home);
    let mut worker = home.start();
    home.ping();
    proof::assert_reads(proof::spawn_reads(&home, 1, false));
    #[cfg(target_os = "linux")]
    proof::wait_for("the idle worker sockets", || {
        (proof::sockets(worker.pid()) == 4).then_some(())
    });
    let gate = proof::lock_store(&home);

    let mut victim = proof::spawn_reads(&home, 1, false);
    proof::pool(&home, 1, 1, 0);
    let others = proof::spawn_reads(&home, 9, false);
    proof::pool(&home, 10, 8, 2);
    victim[0].kill();
    drop(gate);

    proof::assert_reads(others);
    let settled = proof::pool(&home, 0, 0, 0);
    assert_eq!(settled.get_int("pid"), Some(worker.pid()));
    assert_eq!(settled.get_int("stores"), Some(1));
    #[cfg(target_os = "linux")]
    proof::wait_for("ordinary descriptors released", || {
        (proof::sockets(worker.pid()) == 4).then_some(())
    });
    assert_eq!(proof::commits(&home), before);
    home.drain();
    assert!(worker.wait(Duration::from_secs(5)).success());
    assert!(!proof::lease_file(&home, worker.pid()).exists());
}

#[test]
fn worker_death_distinguishes_sent_work_from_the_unsent_queue() {
    let home = proof::create();
    let before = proof::commits(&home);
    let mut worker = home.start();
    home.ping();
    proof::assert_reads(proof::spawn_reads(&home, 1, false));
    let old_lease = proof::lease_file(&home, worker.pid());
    assert!(old_lease.is_file());
    let gate = proof::lock_store(&home);

    let mut sent = proof::spawn_reads(&home, 8, true);
    let old = proof::pool(&home, 8, 8, 0);
    let mut queued = proof::spawn_reads(&home, 2, true);
    proof::pool(&home, 10, 8, 2);
    worker.kill();
    drop(gate);

    let mut ids = BTreeSet::new();
    for caller in &mut sent {
        let refusal = caller.refused("Unknown");
        let detail = refusal.get("detail").unwrap_or(&refusal);
        let id = detail
            .get_text("request_id")
            .expect("missing correlation ID");
        assert!(!id.is_empty() && ids.insert(id.to_owned()), "{refusal:?}");
    }
    for caller in &mut queued {
        let refusal = caller.refused("Unavailable");
        let detail = refusal.get("detail").unwrap_or(&refusal);
        assert!(detail.get("request_id").is_none(), "{refusal:?}");
    }
    assert_eq!(ids.len(), 8);
    assert!(
        old_lease.is_file(),
        "only an explicit later open may settle this lease"
    );

    // A new CLI request starts the replacement, and the normal Store lease
    // path removes the original worker's proved-stale lease.
    proof::assert_reads(proof::spawn_reads(&home, 1, false));
    let restarted = home.ping();
    let new_pid = restarted.get_int("pid").unwrap();
    assert_ne!(new_pid, worker.pid());
    assert_ne!(restarted.get_text("generation"), old.get_text("generation"));
    assert!(!old_lease.exists());
    let new_lease = proof::lease_file(&home, new_pid);
    assert!(new_lease.is_file());
    assert_eq!(proof::commits(&home), before);
    home.drain();
    proof::wait_for("replacement socket and lease release", || {
        (!home.paths.socket.exists() && !new_lease.exists()).then_some(())
    });
}
