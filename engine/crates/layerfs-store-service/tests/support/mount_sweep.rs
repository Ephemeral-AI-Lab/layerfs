//! The fault sweep the `mount_fault_*` binaries run (#768 step B and C):
//! one Workspace mount, or one unmount, on PostgreSQL through
//! [`FaultProxy`], first clean (to count the `K` server messages of its
//! flight), then once per message `N` in `0..K` with one fault injected at
//! `N`. Each run has its own Branch and its own Store service dialing
//! through the proxy; before the armed call the service has prefilled its
//! idle reader and mounted and unmounted once, so the armed mount is a
//! repeated mount (the writer's snapshot read and its admission message)
//! and the armed unmount is the owner's release.
//!
//! After each run, while the faulted service is still open, from a direct
//! admin connection, and then again after the same service mounted and
//! unmounted once more (which reopens a quarantined writer,
//! `session/reopen.rs`):
//!
//! * a mount answered `Ok` left its live intent row and its three session
//!   keys on the writer's backend (unless the backend was terminated after
//!   every reply arrived: the keys end with it, the row stays, and the
//!   reopen takes the keys again on the new backend); an unmount answered
//!   `Ok` left neither;
//! * a call that failed on a writer that needed no reopen (a definite
//!   outcome) leaked nothing: a failed mount left no row and no key, a
//!   failed unmount kept its row and its keys (marked uncertain or live,
//!   for its owner to settle);
//! * a call whose writer was reopened (an unknown outcome: quarantined)
//!   closed the old connection, so its backend ends and holds no key; a
//!   mount's row may stay until the reopen, which deletes it (the mount
//!   never reached its caller); an unmount's row may stay, uncertain
//!   custody, never deleted;
//! * the same service always mounts again, after at most one reopen, and a
//!   fresh service, not through the proxy, mounts the Branch.
//!
//! At the end no backend of the Store is left idle in a transaction.
use super::adversary::{self, wait_until, Service, TestStore};
use super::fault_proxy::{Fault, FaultProxy, Window};
use std::time::Duration;

/// The lane wait of the faulted sessions.
const LOCK_TIMEOUT_MS: u32 = 5_000;

/// The call a sweep faults.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Flight {
    /// A repeated mount: the snapshot, then the admission message.
    Mount,
    /// The owner's release of a mounted Workspace.
    Unmount,
}

/// One run's outcome.
struct Run {
    window: Window,
    /// The armed call's answer.
    ok: bool,
    /// The armed call's error, when it failed.
    error: Option<String>,
    /// Live intent rows of the Branch after the armed call.
    rows: i64,
    /// Session keys the writer's backend held after the armed call.
    keys: i64,
    /// Whether the old writer's backend still ran after the next mount
    /// (waited for, bounded, when the writer was reopened).
    writer_alive: bool,
    /// Whether the same service could still mount the Branch afterwards.
    rebound: bool,
    /// The writer's reopens by then.
    reopens: u64,
    /// Live intent rows of the Branch, and session keys on every proxied
    /// backend, after that mount and its unmount.
    rows_after: i64,
    keys_after: i64,
}

fn proxied(store: &TestStore, proxy: &FaultProxy) -> String {
    let (url, _) = store.postgres.as_ref().unwrap();
    super::fault_proxy::proxied_url(url, proxy.port).0
}

/// Live intent rows of Branch `name`.
fn rows(store: &TestStore, name: &str) -> i64 {
    store
        .admin()
        .query_one(
            "SELECT count(*) FROM gc_intent WHERE scope_kind = 'branch' AND scope_id = $1 \
             AND state <> 'done'",
            &[&store.branch(name).as_slice()],
        )
        .unwrap()
        .get(0)
}

/// One-key advisory locks backend `pid` holds.
fn keys(store: &TestStore, pid: i32) -> i64 {
    store
        .admin()
        .query_one(
            "SELECT count(*) FROM pg_locks WHERE locktype = 'advisory' AND objsubid = 1 \
             AND granted AND pid = $1",
            &[&pid],
        )
        .unwrap()
        .get(0)
}

/// One-key advisory locks every proxied backend holds.
fn proxied_keys(store: &TestStore, proxy: &FaultProxy) -> i64 {
    let pids: Vec<i32> = proxy.pids().into_iter().flatten().collect();
    store
        .admin()
        .query_one(
            "SELECT count(*) FROM pg_locks WHERE locktype = 'advisory' AND objsubid = 1 \
             AND granted AND pid = ANY($1)",
            &[&pids],
        )
        .unwrap()
        .get(0)
}

/// The writer's reopens so far.
fn reopens(service: &Service) -> u64 {
    (service.service.opened.session_diagnostics(None))
        .unwrap()
        .unwrap()
        .reopens
}

fn alive(store: &TestStore, pid: i32) -> bool {
    store
        .admin()
        .query_one(
            "SELECT count(*) FROM pg_stat_activity WHERE pid = $1",
            &[&pid],
        )
        .unwrap()
        .get::<_, i64>(0)
        > 0
}

/// One run on a new Branch `name`.
fn run(
    store: &TestStore,
    proxy: &FaultProxy,
    name: &str,
    flight: Flight,
    target: Option<(u64, Fault)>,
) -> Run {
    let branch = store.fork(name);
    let selection = store.with_lock_timeout(
        &store.selection_via(&proxied(store, proxy)),
        LOCK_TIMEOUT_MS,
    );
    let writer_index = proxy.pids().len();
    let service = Service::open(store, selection);
    let writer = proxy.pids()[writer_index].expect("the writer's backend");
    if let Some(prefilled) = service.service.store.prefill_reader() {
        prefilled.unwrap();
    }
    let warm = service.bind(branch);
    service.unmount(warm);
    let (window, ok, error) = match flight {
        Flight::Mount => {
            proxy.arm(target);
            let bound = service.try_bind(branch);
            let window = proxy.disarm();
            match bound {
                Ok(workspace) => {
                    // Measured below with the Workspace still mounted.
                    let run = (window, true, None);
                    let checked = check_now(store, proxy, name, writer, &service, run);
                    let _ = service.try_unmount(workspace);
                    service.abandon();
                    return checked;
                }
                Err(error) => (window, false, Some(error)),
            }
        }
        Flight::Unmount => {
            let workspace = service.bind(branch);
            proxy.arm(target);
            let released = service.try_unmount(workspace);
            let window = proxy.disarm();
            (window, released.is_ok(), released.err())
        }
    };
    let checked = check_now(store, proxy, name, writer, &service, (window, ok, error));
    service.abandon();
    checked
}

/// The state right after the armed call, then whether the same service
/// still mounts (an extra mount, unmounted again at once), and the state
/// after that.
fn check_now(
    store: &TestStore,
    proxy: &FaultProxy,
    name: &str,
    writer: i32,
    service: &Service,
    (window, ok, error): (Window, bool, Option<String>),
) -> Run {
    let before = rows(store, name);
    let held = keys(store, writer);
    let rebound = match service.try_bind(store.branch(name)) {
        Ok(other) => {
            let _ = service.try_unmount(other);
            true
        }
        Err(_) => false,
    };
    let reopens = reopens(service);
    // A reopened writer closed its old connection: its backend ends.
    let writer_alive =
        reopens == 0 || !wait_until(Duration::from_secs(10), || !alive(store, writer));
    Run {
        window,
        ok,
        error,
        rows: before,
        keys: held,
        writer_alive,
        rebound,
        reopens,
        rows_after: rows(store, name),
        keys_after: proxied_keys(store, proxy),
    }
}

/// Checks one run's invariants; returns its state label.
fn check(store: &TestStore, name: &str, flight: Flight, run: &Run) -> &'static str {
    let label = format!(
        "{name} {flight:?}: fired {:?}, ok {}, error {:?}, rows {}, keys {}, writer alive {}, \
         rebound {}, reopens {}, after: rows {} keys {}",
        run.window.fired,
        run.ok,
        run.error,
        run.rows,
        run.keys,
        run.writer_alive,
        run.rebound,
        run.reopens,
        run.rows_after,
        run.keys_after
    );
    // A fresh service, not through the proxy, mounts the Branch.
    let fresh = store.open();
    let workspace = fresh
        .try_bind(store.branch(name))
        .unwrap_or_else(|error| panic!("{label}: a fresh service mounts: {error}"));
    fresh.unmount(workspace);
    fresh.stop();
    assert!(run.rebound, "{label}: the same service mounts again");
    assert!(run.reopens <= 1, "{label}: at most one reopen");
    let reopened = run.reopens == 1;
    if reopened {
        assert!(!run.writer_alive, "{label}: the old writer's backend ends");
    }
    match (flight, run.ok, reopened) {
        (Flight::Mount, true, false) => {
            assert_eq!(
                (run.rows, run.keys),
                (1, 3),
                "{label}: the row and the keys"
            );
            assert_eq!(
                (run.rows_after, run.keys_after),
                (1, 3),
                "{label}: still mounted"
            );
            "mounted"
        }
        (Flight::Mount, true, true) => {
            // Every reply arrived, then the backend was terminated: the
            // connection ended after the answer, with its keys; the next
            // call reopened and took them again.
            let killed = run.window.fired.as_ref().and_then(|fired| fired.killed);
            assert_eq!(
                killed,
                Some(true),
                "{label}: only a later kill loses the keys"
            );
            assert_eq!((run.rows, run.keys), (1, 0), "{label}: the row stays");
            assert_eq!(
                (run.rows_after, run.keys_after),
                (1, 3),
                "{label}: the keys are taken again"
            );
            "mounted, then killed, keys taken again"
        }
        (Flight::Unmount, true, reopened) => {
            assert_eq!((run.rows, run.keys), (0, 0), "{label}: neither row nor key");
            assert_eq!(
                (run.rows_after, run.keys_after),
                (0, 0),
                "{label}: nothing after"
            );
            if reopened {
                "released, writer reopened"
            } else {
                "released"
            }
        }
        (Flight::Mount, false, false) => {
            assert_eq!(
                (run.rows, run.keys),
                (0, 0),
                "{label}: a definite refusal leaks nothing"
            );
            assert_eq!((run.rows_after, run.keys_after), (0, 0), "{label}: after");
            "refused"
        }
        (Flight::Unmount, false, false) => {
            assert_eq!(
                (run.rows, run.keys),
                (1, 3),
                "{label}: a definite refusal keeps custody"
            );
            "kept"
        }
        (Flight::Mount, false, true) => {
            assert_eq!(run.keys, 0, "{label}: a closed connection holds no key");
            assert!(run.rows <= 1, "{label}: at most the one row");
            assert_eq!(
                (run.rows_after, run.keys_after),
                (0, 0),
                "{label}: the reopen released the mount that never reached its caller"
            );
            if run.rows == 1 {
                "unknown, row released at reopen"
            } else {
                "unknown, no row"
            }
        }
        (Flight::Unmount, false, true) => {
            assert_eq!(run.keys, 0, "{label}: a closed connection holds no key");
            assert!(run.rows <= 1, "{label}: at most the one row");
            assert_eq!(run.keys_after, 0, "{label}: no key taken again");
            assert_eq!(
                run.rows_after, run.rows,
                "{label}: the row is never deleted"
            );
            if run.rows == 1 {
                "unknown, row kept"
            } else {
                "unknown, no row"
            }
        }
    }
}

/// The sweep: `fault` at every server message of the `flight` call.
pub fn sweep(tag: &str, fault: Fault, flight: Flight) {
    let Some(store) = TestStore::postgres(tag, &[("/seed", b"seed\n")]) else {
        return;
    };
    let (url, _) = store.postgres.clone().unwrap();
    let (proxy, _) = FaultProxy::start(&url);
    let clean = run(&store, &proxy, "clean", flight, None);
    assert!(clean.ok, "the clean call: {:?}", clean.error);
    check(&store, "clean", flight, &clean);
    let k = clean.window.seen;
    let kinds: String = clean.window.log.iter().map(|(_, kind)| *kind).collect();
    let connections: std::collections::BTreeSet<usize> =
        clean.window.log.iter().map(|(c, _)| *c).collect();
    println!(
        "MOUNT_FAULT_FLIGHT {{\"tag\":\"{tag}\",\"fault\":\"{fault:?}\",\"flight\":\"{flight:?}\",\"K\":{k},\"connections\":{},\"kinds\":\"{kinds}\"}}",
        connections.len()
    );
    assert!(k > 0, "the call sends something");
    assert_eq!(connections.len(), 1, "only the writer answers: {kinds}");
    let mut states = std::collections::BTreeMap::<&str, u64>::new();
    for n in 0..k {
        let name = format!("n{n}");
        let faulted = run(&store, &proxy, &name, flight, Some((n, fault)));
        assert!(faulted.window.fired.is_some(), "{name}: the fault fired");
        *states
            .entry(check(&store, &name, flight, &faulted))
            .or_default() += 1;
    }
    println!("MOUNT_FAULT_STATES {{\"tag\":\"{tag}\",\"K\":{k},\"states\":{states:?}}}");
    assert!(
        store.no_open_transaction(),
        "no backend left in a transaction"
    );
    let _ = adversary::tmp;
}
