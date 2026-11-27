//! #818 W2 (lane W): a dropped connection, and a backend killed, at every
//! server message of the SDK file call's private bind by name
//! (`Store::bind_named_private`: the writer's named snapshot statement,
//! then the admission message), on PostgreSQL through the fault proxy.
//!
//! The flight is the repeated mount `mount_fault_*` sweeps (#768 step B)
//! with the named snapshot statement in place of the by-id one, and the
//! same invariants (`support/mount_sweep.rs`), checked after each run while
//! the faulted service is open, from a direct admin connection, and again
//! after the same service bound by name and closed once more:
//!
//! * a bind answered `Ok` left its live intent row and its three session
//!   keys on the writer's backend (or, after a kill that came once every
//!   reply arrived, the row; the reopen takes the keys again);
//! * a refusal on a writer that needed no reopen leaked nothing;
//! * a refusal whose writer was reopened (an unknown outcome) closed the old
//!   connection, which holds no key, and the reopen deleted the row of the
//!   mount that never reached its caller;
//! * the same service binds again after at most one reopen, and a fresh
//!   service, not through the proxy, binds the Branch.
//!
//! Each run has its own Branch and its own service; before the armed bind
//! the service prefilled its idle reader and bound and closed once by name,
//! so the armed bind is a repeated one, as an SDK call's. Needs
//! `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "../support/verify/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "../support/fault/proxy.rs"]
#[allow(dead_code)]
mod fault_proxy;

use adversary::{wait_until, Service, TestStore, PROJECT};
use fault_proxy::{Fault, FaultProxy, Window};
use layerfs_history::WorkspaceId;
use layerfs_store_service::store::{BoundWorkspace, NamedBindRequest};
use std::time::Duration;

/// The lane wait of the faulted sessions.
const LOCK_TIMEOUT_MS: u32 = 5_000;

/// The private bind by name an SDK file call makes.
fn bind_named(service: &Service, branch: &str) -> Result<BoundWorkspace, String> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let mut id = [0x77; 32];
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    id[..8].copy_from_slice(&n.to_le_bytes());
    let request = NamedBindRequest {
        workspace: WorkspaceId::from_authority(id).unwrap(),
        project: PROJECT.to_owned(),
        branch: branch.to_owned(),
    };
    (service.service.store)
        .bind_named_private(
            service.owner.client(),
            request,
            &std::cell::Cell::new(false),
        )
        .map(|bound| bound.workspace)
        .map_err(|error| format!("{error:?}"))
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

/// One-key advisory locks the backends `pids` hold.
fn keys(store: &TestStore, pids: &[i32]) -> i64 {
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

fn alive(store: &TestStore, pid: i32) -> bool {
    let count: i64 = store
        .admin()
        .query_one(
            "SELECT count(*) FROM pg_stat_activity WHERE pid = $1",
            &[&pid],
        )
        .unwrap()
        .get(0);
    count > 0
}

/// One run's observations.
struct Run {
    window: Window,
    error: Option<String>,
    /// Rows and the writer's keys right after the armed bind.
    rows: i64,
    keys: i64,
    rebound: bool,
    reopens: u64,
    writer_alive: bool,
    /// Rows and every proxied backend's keys after the next bind and close.
    rows_after: i64,
    keys_after: i64,
}

/// One run on a new Branch `name`.
fn run(store: &TestStore, proxy: &FaultProxy, name: &str, target: Option<(u64, Fault)>) -> Run {
    store.fork(name);
    let (url, _) = store.postgres.as_ref().unwrap();
    let via = fault_proxy::proxied_url(url, proxy.port).0;
    let selection = store.with_lock_timeout(&store.selection_via(&via), LOCK_TIMEOUT_MS);
    let writer_index = proxy.pids().len();
    let service = Service::open(store, selection);
    let writer = proxy.pids()[writer_index].expect("the writer's backend");
    if let Some(prefilled) = service.service.store.prefill_reader() {
        prefilled.unwrap();
    }
    service.unmount(bind_named(&service, name).expect("the warm bind"));
    proxy.arm(target);
    let bound = bind_named(&service, name);
    let window = proxy.disarm();
    let (rows_now, keys_now) = (rows(store, name), keys(store, &[writer]));
    let (armed, error) = match bound {
        Ok(workspace) => (Some(workspace), None),
        Err(error) => (None, Some(error)),
    };
    // The same service binds again (reopening a quarantined writer once),
    // with the armed Workspace still bound.
    let rebound = match bind_named(&service, name) {
        Ok(other) => service.try_unmount(other).is_ok(),
        Err(_) => false,
    };
    let reopens = (service.service.opened.session_diagnostics(None))
        .unwrap()
        .unwrap()
        .reopens;
    let writer_alive =
        reopens == 0 || !wait_until(Duration::from_secs(10), || !alive(store, writer));
    let pids: Vec<i32> = proxy.pids().into_iter().flatten().collect();
    let observed = Run {
        window,
        error,
        rows: rows_now,
        keys: keys_now,
        rebound,
        reopens,
        writer_alive,
        rows_after: rows(store, name),
        keys_after: keys(store, &pids),
    };
    if let Some(workspace) = armed {
        let _ = service.try_unmount(workspace);
    }
    service.abandon();
    observed
}

/// Checks one run's invariants; returns its state label.
fn check(store: &TestStore, name: &str, run: &Run) -> &'static str {
    let label = format!(
        "{name}: fired {:?}, error {:?}, rows {}, keys {}, rebound {}, reopens {}, \
         writer alive {}, after: rows {} keys {}",
        run.window.fired,
        run.error,
        run.rows,
        run.keys,
        run.rebound,
        run.reopens,
        run.writer_alive,
        run.rows_after,
        run.keys_after
    );
    let fresh = store.open();
    let workspace = bind_named(&fresh, name)
        .unwrap_or_else(|error| panic!("{label}: a fresh service binds: {error}"));
    fresh.unmount(workspace);
    fresh.stop();
    assert!(run.rebound, "{label}: the same service binds again");
    assert!(run.reopens <= 1, "{label}: at most one reopen");
    let reopened = run.reopens == 1;
    if reopened {
        assert!(!run.writer_alive, "{label}: the old writer's backend ends");
    }
    // After the rebind and its close; the armed Workspace is still bound.
    let after = (run.rows_after, run.keys_after);
    match (run.error.is_none(), reopened) {
        (true, false) => {
            assert_eq!((run.rows, run.keys), (1, 3), "{label}: the row and keys");
            assert_eq!(after, (1, 3), "{label}: still bound");
            "bound"
        }
        (true, true) => {
            let killed = run.window.fired.as_ref().and_then(|fired| fired.killed);
            assert_eq!(killed, Some(true), "{label}: only a later kill");
            assert_eq!((run.rows, run.keys), (1, 0), "{label}: the row stays");
            assert_eq!(after, (1, 3), "{label}: the keys are taken again");
            "bound, then killed"
        }
        (false, false) => {
            assert_eq!(
                (run.rows, run.keys),
                (0, 0),
                "{label}: a definite refusal leaks nothing"
            );
            assert_eq!(after, (0, 0), "{label}: after");
            "refused"
        }
        (false, true) => {
            assert_eq!(run.keys, 0, "{label}: a closed connection holds no key");
            assert!(run.rows <= 1, "{label}: at most the one row");
            assert_eq!(
                after,
                (0, 0),
                "{label}: the reopen released the bind that never reached its caller"
            );
            if run.rows == 1 {
                "unknown, row released at reopen"
            } else {
                "unknown, no row"
            }
        }
    }
}

fn sweep(tag: &str, fault: Fault) {
    let Some(store) = TestStore::postgres(tag, &[("/seed", b"seed\n")]) else {
        return;
    };
    let (url, _) = store.postgres.clone().unwrap();
    let (proxy, _) = FaultProxy::start(&url);
    let clean = run(&store, &proxy, "clean", None);
    assert!(clean.error.is_none(), "the clean bind: {:?}", clean.error);
    check(&store, "clean", &clean);
    let k = clean.window.seen;
    let kinds: String = clean.window.log.iter().map(|(_, kind)| *kind).collect();
    let connections: std::collections::BTreeSet<usize> =
        clean.window.log.iter().map(|(c, _)| *c).collect();
    println!(
        "NAMED_BIND_FLIGHT {{\"tag\":\"{tag}\",\"fault\":\"{fault:?}\",\"K\":{k},\"connections\":{},\"kinds\":\"{kinds}\"}}",
        connections.len()
    );
    assert!(k > 0, "the bind sends something");
    assert_eq!(connections.len(), 1, "only the writer answers: {kinds}");
    let mut states = std::collections::BTreeMap::<&str, u64>::new();
    for n in 0..k {
        let name = format!("n{n}");
        let faulted = run(&store, &proxy, &name, Some((n, fault)));
        assert!(faulted.window.fired.is_some(), "{name}: the fault fired");
        *states.entry(check(&store, &name, &faulted)).or_default() += 1;
    }
    println!("NAMED_BIND_STATES {{\"tag\":\"{tag}\",\"K\":{k},\"states\":{states:?}}}");
    assert!(
        store.no_open_transaction(),
        "no backend left in a transaction"
    );
}

#[test]
fn a_connection_dropped_at_any_message_of_a_private_named_bind() {
    sweep("named_drop", Fault::Drop);
}

#[test]
fn a_backend_killed_at_any_message_of_a_private_named_bind() {
    sweep("named_kill", Fault::HoldKill);
}
