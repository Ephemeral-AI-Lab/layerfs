//! Second independent round of writer-reopen tests, R4 (fix round
//! 1d0a121012): orphans are released only once the old backend is proven
//! gone.
//!
//! A mount's one message waits on a table lock an admin connection holds;
//! its backend B1 is stopped (`SIGSTOP` in the server's container) and the
//! lock released, so B1's insert and `COMMIT` are still to run; the
//! mount's client is cut, so the mount is an orphan. The next call reopens:
//! `pg_terminate_backend(B1, bound)` cannot prove B1 gone (it is stopped),
//! so B1 stays a lost backend. The reopen must not delete the orphan's row
//! then (a delete before B1's `COMMIT` lands finds nothing and forgets the
//! orphan, and the row B1 commits later is tracked by nothing). Once B1 is
//! continued it ends (the termination's signal is pending) and the next
//! reopen, which proves it gone, releases the orphan.
//!
//! Observed from outside: the server messages of the first reopen are
//! compared with those of a control reopen with nothing to release (same
//! shape, notices left out), and the admin connection reads `gc_intent`.
//! The row itself cannot be made to land after the termination attempt on
//! this server: the signal that attempt leaves pending aborts B1's
//! transaction once it runs again, before and after the fix alike, so the
//! message comparison is what tells them apart.
//!
//! PostgreSQL only. Needs `LAYERFS_TEST_POSTGRES_URL` and
//! `LAYERFS_TEST_POSTGRES_CONTAINER` (the server's Docker container, for the
//! stop); NOT_RUN without either.
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

use adversary::wait_until;
use std::time::Duration;
use testerwr::{alive, compact, kill, Watchdog};
use testerwr2::*;

/// The proxied connection that answered a window (one reopen and a read).
fn reopened(window: &fault_proxy::Window) -> (usize, String) {
    let connection = window.log.last().expect("the reopen answered").0;
    (connection, kinds(&window.log, connection))
}

#[test]
fn r4_an_orphan_is_not_released_while_its_old_backend_is_not_proven_gone() {
    let Some(container) = container() else {
        return;
    };
    // A 3 s lock timeout: the reopen's termination bound, and the mount's
    // lock wait, which the stop (well under it) interrupts.
    let Some(rig) = rig2_with("wr2_r4", 3_000) else {
        return;
    };
    let _dog = Watchdog::arm("r4", Duration::from_secs(100));
    let a = rig.store.fork("a");
    // Control: a reopen with one lost backend, proven gone, and nothing to
    // release.
    kill(&rig.store, writer(&rig));
    rig.proxy.arm(None);
    read(&rig, a).unwrap();
    let (_, control) = reopened(&rig.proxy.disarm());
    assert_eq!(reopens(&rig), 1);
    // The mount's message waits on the table; B1 is stopped there.
    let intent = mount(&rig.store, a, "a", 0xF1);
    let b1 = writer(&rig);
    let mut holder = rig.store.admin();
    holder
        .batch_execute("BEGIN; LOCK TABLE gc_intent IN SHARE MODE")
        .unwrap();
    let mut observer = rig.store.admin();
    let answer = std::thread::scope(|scope| {
        let first = scope.spawn(|| rig.history().gc_register(&intent));
        let waiting = wait_until(Duration::from_secs(10), || {
            observer
                .query_opt(
                    "SELECT 1 FROM pg_stat_activity WHERE pid = $1 AND wait_event_type = 'Lock'",
                    &[&b1],
                )
                .unwrap()
                .is_some()
        });
        assert!(waiting, "B1's insert waits on the table lock");
        let stopped = Stopped::stop(&container, b1);
        holder.batch_execute("ROLLBACK").unwrap();
        assert!(rig.proxy.cut(b1), "the mount's client is cut");
        let answer = first.join().unwrap();
        (answer, stopped)
    });
    let (answer, stopped) = answer;
    assert!(answer.is_err(), "the mount's reply was lost: {answer:?}");
    // First reopen: B1 cannot be proven gone.
    rig.proxy.arm(None);
    let first = read(&rig, a);
    let (_, during) = reopened(&rig.proxy.disarm());
    let rows_during = op_rows(&rig.store, &intent.op_id);
    stopped.resume();
    let ended = wait_until(Duration::from_secs(10), || !alive(&rig.store, b1));
    let rows_after_b1 = op_rows(&rig.store, &intent.op_id);
    // Next reopen: B1 is gone.
    kill(&rig.store, writer(&rig));
    rig.proxy.arm(None);
    let next = read(&rig, a);
    let (_, later) = reopened(&rig.proxy.disarm());
    let rows_end = op_rows(&rig.store, &intent.op_id);
    println!(
        "TESTERWR2_R4 mount {answer:?}; first reopen {first:?} rows {rows_during:?}; \
         B1 ended {ended}, rows {rows_after_b1:?}; next reopen {next:?} rows {rows_end:?}; \
         reopens {}\n  control {control}\n  during  {during}\n  later   {later}",
        reopens(&rig)
    );
    assert!(first.is_ok() && next.is_ok(), "both reopens served");
    assert!(ended, "B1 ended once continued");
    assert_eq!(
        during, control,
        "the first reopen, with B1 not proven gone, sent more than a reopen with \
         nothing to release: the orphan's delete ran before B1's COMMIT could land"
    );
    // The next reopen proves B1 gone (its end statement has one more row,
    // B1's) and releases the orphan: more than that row.
    assert!(
        later.len() > control.len() + 1,
        "the orphan was not released once B1 was proven gone"
    );
    assert!(rows_end.is_empty(), "the row: {rows_end:?}");
    assert_eq!(writer_keys(&rig).1, Vec::<i64>::new());
    assert_eq!(compact(&rig.store), "Ok");
    rig.handles.close();
}
