//! Second independent round of writer-reopen tests: custody across a reopen
//! that another thread's call makes between two writer turns of one GC
//! registration or release (fix round 1d0a121012, R1–R3).
//!
//! Each test holds one call inside a writer turn with the proxy's `Pause`
//! (every reply of that turn held), ends the turn's backend (terminated, or
//! its client cut so the reply is lost), queues a second call behind the
//! held turn, and then lets the held turn finish. The writer's queue is
//! first come, first served, so the queued call takes the next turn: it
//! finds the writer's connection closed and reopens it before the held
//! call's next turn (R1) or before the held call's thread gets past the
//! turn that ended (R2, R3).
//!
//! * R1 (keys before scan), two forms: the statement-by-statement
//!   registration's second turn must not insert its row on the reopened
//!   connection, which holds none of the keys the first turn took; and a
//!   refused one message's unlock must not run on the reopened connection,
//!   where it would drop the level another live mount's keys were taken
//!   again at.
//! * R2: a release whose reply is lost must not leave its keys held on the
//!   new backend with no owner: once everything is released, compaction is
//!   admitted.
//! * R3: a mount whose reply is lost after its `COMMIT` is an orphan the
//!   reopen (or the next one) deletes; a mount admitted just before its
//!   backend ended keeps its keys on the reopened connection.
//!
//! R2 and R3 are the closest tests the proxy can force: in the code before
//! the fix the custody change came right after the turn ended, with no SQL
//! in between, so the queued reopen (a new connection, ms) loses that race
//! to the held call's thread (µs) on almost every run; the proxy cannot
//! hold a thread outside SQL. R1 is forced deterministically.
//!
//! PostgreSQL only. Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "../support/verify/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "../support/fault/proxy.rs"]
#[allow(dead_code)]
mod fault_proxy;
#[path = "../support/verify/reopen_rig.rs"]
#[allow(dead_code)]
mod testerwr;
#[path = "../support/verify/reopen_second_round.rs"]
#[allow(dead_code)]
mod testerwr2;

use adversary::history;
use fault_proxy::Fault;
use layerfs_persistence::internals::{mount_one_message, register_separately};
use std::time::Duration;
use testerwr::{compact, holders_of, kill, remove, Watchdog};
use testerwr2::*;

/// Prepares the registration, one message and release statements on the
/// writer (a mount and release of a Branch of its own), so the held turn is
/// the first server message of an armed window.
fn warm(rig: &Rig2) {
    let branch = rig.store.fork("warm");
    let intent = mount(&rig.store, branch, "warm", 0x01);
    register_separately(&rig.handles.history, &intent).unwrap();
    rig.history().gc_release(&intent.op_id).unwrap();
    let intent = mount(&rig.store, branch, "warm", 0x02);
    assert!(mount_one_message(&rig.handles.history, &intent).unwrap());
    rig.history().gc_release(&intent.op_id).unwrap();
    assert_eq!(writer_keys(rig).1, Vec::<i64>::new(), "warm-up left no key");
}

/// Ends the check: nothing registered, no key held, compaction admitted.
fn settled(rig: &Rig2, what: &str) {
    let rows = testerwr::all_rows(&rig.store);
    let (pid, keys) = writer_keys(rig);
    let compaction = compact(&rig.store);
    println!("TESTERWR2 {what} end: rows={rows} writer={pid} keys={keys:?} compact={compaction}");
    assert_eq!(rows, 0, "{what}: rows left");
    assert_eq!(
        keys,
        Vec::<i64>::new(),
        "{what}: keys held with nothing registered"
    );
    assert_eq!(
        compaction, "Ok",
        "{what}: compaction after everything was released"
    );
}

/// R1, first form. A's statement-by-statement mount registration takes its
/// keys on backend B1 (turn 1, held by the proxy); B1 is terminated; a read
/// queued behind turn 1 finds the connection closed and reopens on B2; then
/// A's turn 2 (the conflict scan and the insert) runs. Before the fix it
/// reopened or ran on B2 and inserted A's row with none of its keys held;
/// after it, turn 2 is pinned to B1's connection and refused.
#[test]
fn r1_a_registration_second_turn_never_runs_on_the_reopened_connection() {
    let Some(rig) = rig2("wr2_r1a") else {
        return;
    };
    let _dog = Watchdog::arm("r1_a", Duration::from_secs(100));
    warm(&rig);
    let a = rig.store.fork("a");
    let intent = mount(&rig.store, a, "a", 0xA1);
    let keys = mount_keys(&rig.store, a);
    let b1 = writer(&rig);
    let reopens_before = reopens(&rig);
    rig.proxy.arm(Some((0, Fault::Pause)));
    let (answer, queued) = std::thread::scope(|scope| {
        let first = scope.spawn(|| register_separately(&rig.handles.history, &intent));
        assert!(
            rig.proxy.wait_fired(Duration::from_secs(10)),
            "A is held in its key turn"
        );
        // The keys were granted on B1 (its reply is held): B1 ends.
        let granted = holders_of(&rig.store, &keys);
        kill(&rig.store, b1);
        let second = scope.spawn(|| read(&rig, a));
        std::thread::sleep(QUEUE);
        rig.proxy.release();
        let answer = first.join().unwrap();
        let queued = second.join().unwrap();
        println!("TESTERWR2_R1A keys granted on B1 {b1} while held: {granted:?}");
        (answer, queued)
    });
    let window = rig.proxy.disarm();
    let fired = window.fired.clone().expect("the pause fired");
    let rows = op_rows(&rig.store, &intent.op_id);
    let holders = holders_of(&rig.store, &keys);
    let (b2, held) = writer_keys(&rig);
    println!(
        "TESTERWR2_R1A fired at {} ({}) pid {:?}; answer={answer:?} queued={queued:?} \
         reopens={} rows={rows:?} holders={holders:?} writer B2 {b2} keys={held:?}",
        fired.at,
        fired.kind,
        fired.pid,
        reopens(&rig) - reopens_before
    );
    assert_eq!(fired.pid, Some(b1), "the held turn was the writer's");
    assert!(queued.is_ok(), "the queued read reopened: {queued:?}");
    assert_eq!(reopens(&rig) - reopens_before, 1, "one reopen");
    // The invariant: a gc_intent row exists only with its keys held.
    assert!(
        rows.is_empty() || holders.iter().any(|(_, held)| *held == 3),
        "A's row {rows:?} exists with none of its keys held (holders {holders:?}, \
         writer B2 {b2} holds {held:?}); A answered {answer:?}"
    );
    if answer.is_ok() {
        // A believes its mount is live: no removal or compaction may pass.
        assert_ne!(compact(&rig.store), "Ok", "compaction while A is mounted");
        assert_ne!(remove(&rig.store, "a"), "Ok", "removal while A is mounted");
        rig.history().gc_release(&intent.op_id).unwrap();
    } else {
        assert!(rows.is_empty(), "a refused registration wrote {rows:?}");
    }
    settled(&rig, "r1_a");
    rig.handles.close();
}

/// R1, second form. Mount M is live on Branch `a` (its keys held on B1). A
/// merge in the project, on a session of its own, makes A's one-message
/// mount of `a` refuse at its insert with its keys held (Holding), so the
/// one message gives them back in a later turn. A's message is held, B1 is
/// terminated, a queued read reopens on B2 and takes M's keys again; then
/// A's unlock runs. Before the fix it ran on B2 and dropped the only level
/// B2 holds, M's; after it, the unlock is pinned to B1's connection, which
/// gave A's keys back when it closed, and sends nothing.
#[test]
fn r1_b_a_refused_admission_never_unlocks_on_the_reopened_connection() {
    let Some(rig) = rig2("wr2_r1b") else {
        return;
    };
    let _dog = Watchdog::arm("r1_b", Duration::from_secs(100));
    warm(&rig);
    let a = rig.store.fork("a");
    let keys = mount_keys(&rig.store, a);
    let held_mount = mount(&rig.store, a, "a", 0xB1);
    assert!(mount_one_message(&rig.handles.history, &held_mount).unwrap());
    let other = rig.store.handles();
    let busy = merge(&rig.store, "z", 0xE1);
    history(&other).gc_register(&busy).unwrap();
    let (b1, before) = writer_keys(&rig);
    assert_eq!(before, keys, "M's keys on B1");
    let intent = mount(&rig.store, a, "a", 0xA2);
    rig.proxy.arm(Some((0, Fault::Pause)));
    let (answer, queued) = std::thread::scope(|scope| {
        let first = scope.spawn(|| mount_one_message(&rig.handles.history, &intent));
        assert!(
            rig.proxy.wait_fired(Duration::from_secs(10)),
            "A is held in its admission"
        );
        kill(&rig.store, b1);
        let second = scope.spawn(|| read(&rig, a));
        std::thread::sleep(QUEUE);
        rig.proxy.release();
        (first.join().unwrap(), second.join().unwrap())
    });
    let window = rig.proxy.disarm();
    let rows = op_rows(&rig.store, &intent.op_id);
    let (b2, after) = writer_keys(&rig);
    let m_rows = op_rows(&rig.store, &held_mount.op_id);
    println!(
        "TESTERWR2_R1B fired {:?}; A answered {answer:?}, queued read {queued:?}; \
         A rows {rows:?}; M rows {m_rows:?}; writer B2 {b2} keys {after:?} (M's {keys:?})",
        window.fired.as_ref().map(|f| (f.at, f.kind, f.pid))
    );
    assert!(queued.is_ok(), "the queued read reopened: {queued:?}");
    assert!(
        matches!(answer, Ok(false) | Err(_)),
        "A was refused: {answer:?}"
    );
    assert!(rows.is_empty(), "A wrote nothing: {rows:?}");
    assert_eq!(m_rows, vec!["live".to_owned()], "M is still mounted");
    assert_eq!(
        after, keys,
        "M is live but its keys are no longer held on the reopened backend {b2}"
    );
    history(&other).gc_release(&busy.op_id).unwrap();
    if let layerfs_persistence::Handles::Postgres(other) = other {
        other.close();
    }
    rig.history().gc_release(&held_mount.op_id).unwrap();
    settled(&rig, "r1_b");
    rig.handles.close();
}

/// R2 (closest test). Mounts X (Branch `a`) and Y (Branch `b`) are live.
/// X's release message is held after the server ran it, a read is queued,
/// and X's client is cut: the release's reply is lost and the writer is
/// quarantined; the queued read reopens. X's keys must not be held on the
/// new backend for nobody: only Y's are, and once Y is released nothing is
/// and compaction is admitted.
#[test]
fn r2_a_lost_release_reply_leaves_no_ownerless_keys_after_a_queued_reopen() {
    let Some(rig) = rig2("wr2_r2") else {
        return;
    };
    let _dog = Watchdog::arm("r2", Duration::from_secs(100));
    warm(&rig);
    let (a, b) = (rig.store.fork("a"), rig.store.fork("b"));
    let x = mount(&rig.store, a, "a", 0xC1);
    let y = mount(&rig.store, b, "b", 0xC2);
    assert!(mount_one_message(&rig.handles.history, &x).unwrap());
    assert!(mount_one_message(&rig.handles.history, &y).unwrap());
    let y_keys = mount_keys(&rig.store, b);
    let b1 = writer(&rig);
    rig.proxy.arm(Some((0, Fault::Pause)));
    let (answer, queued) = std::thread::scope(|scope| {
        let first = scope.spawn(|| rig.history().gc_release(&x.op_id));
        assert!(
            rig.proxy.wait_fired(Duration::from_secs(10)),
            "X's release is held"
        );
        let second = scope.spawn(|| read(&rig, a));
        std::thread::sleep(QUEUE);
        assert!(rig.proxy.cut(b1), "X's client is cut");
        let answer = first.join().unwrap();
        rig.proxy.release();
        (answer, second.join().unwrap())
    });
    rig.proxy.disarm();
    let x_rows = op_rows(&rig.store, &x.op_id);
    let (b2, held) = writer_keys(&rig);
    println!(
        "TESTERWR2_R2 release answered {answer:?}; queued {queued:?}; reopens {}; \
         X rows {x_rows:?}; writer B2 {b2} keys {held:?} (Y's {y_keys:?})",
        reopens(&rig)
    );
    assert!(answer.is_err(), "the release's reply was lost");
    assert!(queued.is_ok(), "the queued read reopened: {queued:?}");
    assert_ne!(b2, b1, "a new backend");
    assert_eq!(held, y_keys, "only Y's keys on the reopened backend");
    rig.history().gc_release(&y.op_id).unwrap();
    // The server ran X's delete before its reply was lost.
    assert!(x_rows.is_empty(), "X's row: {x_rows:?}");
    settled(&rig, "r2");
    rig.handles.close();
}

/// R3, orphan (closest test). A mount's whole message is held after the
/// server committed it; a read is queued and the mount's client is cut, so
/// its reply is lost after `COMMIT`. The queued read reopens: the orphan's
/// row must be deleted by that reopen, or else by the next one.
#[test]
fn r3_a_mount_lost_after_commit_is_an_orphan_the_queued_reopen_deletes() {
    let Some(rig) = rig2("wr2_r3_orphan") else {
        return;
    };
    let _dog = Watchdog::arm("r3_orphan", Duration::from_secs(100));
    warm(&rig);
    let a = rig.store.fork("a");
    let intent = mount(&rig.store, a, "a", 0xD1);
    let b1 = writer(&rig);
    rig.proxy.arm(Some((0, Fault::Pause)));
    let (answer, queued) = std::thread::scope(|scope| {
        let first = scope.spawn(|| rig.history().gc_register(&intent));
        assert!(
            rig.proxy.wait_fired(Duration::from_secs(10)),
            "the mount's message is held"
        );
        let second = scope.spawn(|| read(&rig, a));
        std::thread::sleep(QUEUE);
        assert!(rig.proxy.cut(b1), "the mount's client is cut");
        let answer = first.join().unwrap();
        rig.proxy.release();
        (answer, second.join().unwrap())
    });
    rig.proxy.disarm();
    let after_first = op_rows(&rig.store, &intent.op_id);
    let mut after_next = None;
    if !after_first.is_empty() {
        // Not deleted by the queued reopen: the next reopen must.
        kill(&rig.store, writer(&rig));
        let next = read(&rig, a);
        after_next = Some((next, op_rows(&rig.store, &intent.op_id)));
    }
    println!(
        "TESTERWR2_R3_ORPHAN mount answered {answer:?}; queued {queued:?}; reopens {}; \
         rows after the queued reopen {after_first:?}; after the next {after_next:?}",
        reopens(&rig)
    );
    assert!(answer.is_err(), "the mount's reply was lost");
    assert!(queued.is_ok(), "the queued read reopened: {queued:?}");
    if let Some((next, rows)) = &after_next {
        assert!(next.is_ok(), "the next reopen: {next:?}");
        assert!(
            rows.is_empty(),
            "the orphan's row was never deleted: {rows:?}"
        );
    }
    settled(&rig, "r3_orphan");
    rig.handles.close();
}

/// R3, hold (closest test). A mount's message is held after the server
/// admitted it; its backend is terminated and a read is queued; then the
/// held replies arrive and the mount is admitted. The queued read's reopen
/// must take the new mount's keys again: a live mount never ends up
/// without its keys.
#[test]
fn r3_b_a_mount_admitted_as_its_backend_ends_keeps_its_keys_after_the_reopen() {
    let Some(rig) = rig2("wr2_r3_hold") else {
        return;
    };
    let _dog = Watchdog::arm("r3_hold", Duration::from_secs(100));
    warm(&rig);
    let a = rig.store.fork("a");
    let intent = mount(&rig.store, a, "a", 0xD2);
    let keys = mount_keys(&rig.store, a);
    let b1 = writer(&rig);
    rig.proxy.arm(Some((0, Fault::Pause)));
    let (answer, queued) = std::thread::scope(|scope| {
        let first = scope.spawn(|| mount_one_message(&rig.handles.history, &intent));
        assert!(
            rig.proxy.wait_fired(Duration::from_secs(10)),
            "the mount's message is held"
        );
        kill(&rig.store, b1);
        let second = scope.spawn(|| read(&rig, a));
        std::thread::sleep(QUEUE);
        rig.proxy.release();
        (first.join().unwrap(), second.join().unwrap())
    });
    rig.proxy.disarm();
    let rows = op_rows(&rig.store, &intent.op_id);
    let (b2, held) = writer_keys(&rig);
    println!(
        "TESTERWR2_R3_HOLD mount answered {answer:?}; queued {queued:?}; reopens {}; \
         rows {rows:?}; writer B2 {b2} keys {held:?} (the mount's {keys:?})",
        reopens(&rig)
    );
    assert!(queued.is_ok(), "the queued read reopened: {queued:?}");
    match answer {
        Ok(true) => {
            assert_eq!(rows, vec!["live".to_owned()]);
            assert_eq!(held, keys, "a live mount without its keys on B2 {b2}");
            rig.history().gc_release(&intent.op_id).unwrap();
        }
        other => panic!("the held replies arrived whole: {other:?}"),
    }
    settled(&rig, "r3_hold");
    rig.handles.close();
}
