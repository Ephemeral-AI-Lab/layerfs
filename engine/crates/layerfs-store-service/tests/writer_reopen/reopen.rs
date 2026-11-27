//! A quarantined PostgreSQL writer reopens at its next call (#768 follow-up,
//! `session/reopen.rs`): one new connection, the closed one's leftovers
//! settled first, never a resend.
//!
//! * a killed writer backend, and a connection dropped at the proxy: the
//!   next Commit on the same service lands, a mount and an unmount after it
//!   work, one reopen;
//! * a Commit whose reply was lost after its `COMMIT` reached the server is
//!   resolved committed and installed once; one cut while the server still
//!   waited before the lane folds back, and the next Commit carries the
//!   change once (head and tree read from a fresh service);
//! * a Workspace mounted across the reopen keeps refusing `branch.remove`
//!   and compaction, its keys are held again on the new backend, and its
//!   unmount releases its row and keys;
//! * a key the reopen cannot take again refuses the reopen (the writer stays
//!   quarantined) until it is free;
//! * a mount whose reply was lost after its `COMMIT` leaves a row the next
//!   call's reopen deletes, so compaction is admitted again;
//! * a reopen against a server that is down fails and stays quarantined;
//!   the next call after it is back reopens;
//! * two Commits queued when the writer dies both land, after one reopen.
//!
//! PostgreSQL only. Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
use crate::adversary;
use crate::fault_proxy;

use adversary::{
    commit, verdict, wait_until, write, Gate, Node, Service, TestStore, Verdict, CURSOR,
};
use fault_proxy::{Fault, FaultProxy};
use layerfs_history::{BranchId, CommitOutcome, HistoryError};
use layerfs_persistence::{CompactRequest, Handles};
use layerfs_protocol::sdk::SdkValue;
use layerfs_store_service::store::BoundWorkspace;
use std::time::Duration;

const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n")];
/// The lane wait of the proxied services.
const LOCK_TIMEOUT_MS: u32 = 10_000;

/// A Store, a proxy to its server and a service dialing through it, with
/// the backend of the service's writer connection.
struct Rig {
    store: TestStore,
    proxy: FaultProxy,
    service: Service,
    writer: i32,
}

fn rig(label: &str) -> Option<Rig> {
    let store = TestStore::postgres(label, SEED)?;
    let (url, _) = store.postgres.clone().unwrap();
    let (proxy, proxied) = FaultProxy::start(&url);
    let selection = store.with_lock_timeout(&store.selection_via(&proxied), LOCK_TIMEOUT_MS);
    let index = proxy.pids().len();
    let service = Service::open(&store, selection);
    let writer = proxy.pids()[index].expect("the writer's backend");
    Some(Rig {
        store,
        proxy,
        service,
        writer,
    })
}

/// The service writer's reopens so far.
fn reopens(service: &Service) -> u64 {
    service
        .service
        .opened
        .session_diagnostics(None)
        .unwrap()
        .unwrap()
        .reopens
}

fn alive(store: &TestStore, pid: i32) -> bool {
    store
        .admin()
        .query_opt("SELECT 1 FROM pg_stat_activity WHERE pid = $1", &[&pid])
        .unwrap()
        .is_some()
}

fn kill(store: &TestStore, pid: i32) {
    let ended: bool = store
        .admin()
        .query_one("SELECT pg_terminate_backend($1, 10000)", &[&pid])
        .unwrap()
        .get(0);
    assert!(ended, "backend {pid} ended");
}

/// Session advisory keys per backend among the proxied connections.
fn keys_by_backend(rig: &Rig) -> Vec<(i32, i64)> {
    let pids: Vec<i32> = rig.proxy.pids().into_iter().flatten().collect();
    rig.store
        .admin()
        .query(
            "SELECT pid, count(*) FROM pg_locks WHERE locktype = 'advisory' AND objsubid = 1 \
             AND granted AND pid = ANY($1) GROUP BY pid ORDER BY pid",
            &[&pids],
        )
        .unwrap()
        .iter()
        .map(|row| (row.get(0), row.get(1)))
        .collect()
}

/// Live intent rows of Branch `branch`.
fn intent_rows(store: &TestStore, branch: BranchId) -> i64 {
    store
        .admin()
        .query_one(
            "SELECT count(*) FROM gc_intent WHERE scope_kind = 'branch' AND scope_id = $1 \
             AND state <> 'done'",
            &[&branch.as_slice()],
        )
        .unwrap()
        .get(0)
}

/// One compaction on a session of its own: "Ok" or the refusal's class.
fn compact(store: &TestStore, op: u8) -> String {
    let request = CompactRequest {
        op_id: [op; 16],
        ..CompactRequest::default()
    };
    let Handles::Postgres(handles) =
        Handles::open_writable(&store.selection, &store.binding, CURSOR).unwrap()
    else {
        unreachable!()
    };
    let result = handles.compact(&request);
    handles.close();
    match result {
        Ok(_) => "Ok".into(),
        Err(HistoryError::MountsActive(_)) => "MountsActive".into(),
        Err(other) => format!("{other:?}"),
    }
}

/// `branch.remove` of `name` through a fresh direct service: "Ok" or the
/// refusal's code.
fn remove(store: &TestStore, name: &str) -> String {
    let service = store.open();
    let answer = service.call("branch.remove", vec![("branch", SdkValue::text(name))]);
    service.stop();
    match answer {
        Ok(_) => "Ok".into(),
        Err(refusal) => refusal.code,
    }
}

fn committed(workspace: &BoundWorkspace, what: &str) -> layerfs_history::CommitId {
    match verdict(&commit(workspace)) {
        Verdict::Committed(id) => id,
        other => panic!("{what}: {other:?}"),
    }
}

/// The Branch's head and whether its tree holds `path` with `bytes`, read
/// through a fresh direct service.
fn landed(store: &TestStore, branch: BranchId, path: &str, bytes: &[u8]) -> bool {
    store.tree(branch).get(path) == Some(&Node::File(bytes.to_vec()))
}

/// The next Commit, a mount and an unmount after the writer's connection
/// ended without a call in flight.
fn next_calls_work(rig: &Rig, branch: BranchId, workspace: &BoundWorkspace) {
    assert!(
        wait_until(Duration::from_secs(10), || !alive(&rig.store, rig.writer)),
        "the old writer's backend ended"
    );
    write(workspace, "/after", b"after\n");
    let id = committed(workspace, "the Commit after the writer ended");
    assert_eq!(reopens(&rig.service), 1, "one reopen");
    assert_eq!(rig.store.chain(branch).first().map(|c| c.id), Some(id));
    assert!(landed(&rig.store, branch, "/after", b"after\n"));
    let other = rig.service.bind(branch);
    rig.service.unmount(other);
    assert_eq!(reopens(&rig.service), 1, "still one reopen");
}

#[test]
fn a_killed_writer_backend_reopens_at_the_next_commit() {
    let Some(rig) = rig("reopen_kill") else {
        return;
    };
    let branch = rig.store.fork("kill");
    let workspace = rig.service.bind(branch);
    write(&workspace, "/warm", b"warm\n");
    committed(&workspace, "warm-up");
    kill(&rig.store, rig.writer);
    next_calls_work(&rig, branch, &workspace);
    rig.service.unmount(workspace);
    assert_eq!(intent_rows(&rig.store, branch), 0, "no row left");
    rig.service.stop();
}

#[test]
fn a_dropped_writer_connection_reopens_at_the_next_commit() {
    let Some(rig) = rig("reopen_drop") else {
        return;
    };
    let branch = rig.store.fork("drop");
    let workspace = rig.service.bind(branch);
    write(&workspace, "/warm", b"warm\n");
    committed(&workspace, "warm-up");
    assert!(rig.proxy.cut(rig.writer), "the writer's connection is cut");
    next_calls_work(&rig, branch, &workspace);
    rig.service.unmount(workspace);
    rig.service.stop();
}

/// The server messages of one warm Commit on a twin Branch, so the armed
/// run can fault the last of them: the twin's second warm Commit, once the
/// statements of the path are prepared on the writer (the armed run's
/// Commit prepares none).
fn warm_flight(rig: &Rig, name: &str) -> u64 {
    let branch = rig.store.fork(name);
    let workspace = rig.service.bind(branch);
    write(&workspace, "/warm", b"warm\n");
    committed(&workspace, "twin warm-up");
    write(&workspace, "/one", b"one\n");
    committed(&workspace, "twin first warm Commit");
    write(&workspace, "/fault", b"fault\n");
    rig.proxy.arm(None);
    committed(&workspace, "twin Commit");
    let window = rig.proxy.disarm();
    rig.service.unmount(workspace);
    // The flight ends with the fused message's `COMMIT` reply.
    let kinds: String = window.log.iter().map(|(_, kind)| *kind).collect();
    assert!(kinds.ends_with("CZ"), "the flight: {kinds}");
    window.seen
}

#[test]
fn a_commit_lost_after_its_commit_is_installed_once_across_the_reopen() {
    let Some(rig) = rig("reopen_lost_after") else {
        return;
    };
    let k = warm_flight(&rig, "twin");
    let branch = rig.store.fork("after");
    let workspace = rig.service.bind(branch);
    write(&workspace, "/warm", b"warm\n");
    let warm = committed(&workspace, "warm-up");
    write(&workspace, "/fault", b"fault\n");
    // The last server message of the warm Commit is its message's final
    // reply: the server committed before it sent it.
    rig.proxy.arm(Some((k - 1, Fault::Drop)));
    let first = verdict(&commit(&workspace));
    let window = rig.proxy.disarm();
    assert!(window.fired.is_some(), "the fault fired");
    assert!(
        matches!(first, Verdict::Unknown(_)),
        "lost reply: {first:?}"
    );
    assert!(workspace.has_retained(), "the unknown Commit is retained");
    // The next Commit request first resolves it, as the daemon does
    // (`resolve_custody`): committed, found by its attempt, installed.
    let resolution = workspace
        .resolve_retained()
        .unwrap_or_else(|unresolved| panic!("{unresolved:?}"))
        .expect("a retained Commit");
    let Some(CommitOutcome::Committed { record, .. }) = resolution.outcome else {
        panic!("resolved committed: {:?}", resolution.outcome);
    };
    let follow = verdict(&commit(&workspace));
    assert_eq!(follow, Verdict::UpToDate, "published once, not again");
    let chain = rig.store.chain(branch);
    assert_eq!(chain[0].id, record.id, "the lost Commit is the head");
    assert_eq!(
        chain[0].parent,
        Some(warm),
        "exactly one Commit past the warm-up"
    );
    assert!(landed(&rig.store, branch, "/fault", b"fault\n"));
    // The Workspace serves on: a new change lands once.
    write(&workspace, "/next", b"next\n");
    let next = committed(&workspace, "the Commit after");
    assert_eq!(reopens(&rig.service), 1, "one reopen");
    let chain = rig.store.chain(branch);
    assert_eq!((chain[0].id, chain[0].parent), (next, Some(record.id)));
    rig.service.unmount(workspace);
    rig.service.stop();
}

#[test]
fn a_commit_cut_before_the_server_commits_folds_back_across_the_reopen() {
    let Some(rig) = rig("reopen_lost_before") else {
        return;
    };
    let (_, schema) = rig.store.postgres.clone().unwrap();
    let branch = rig.store.fork("before");
    let workspace = rig.service.bind(branch);
    write(&workspace, "/warm", b"warm\n");
    let warm = committed(&workspace, "warm-up");
    let payload = adversary::random_bytes(11, 64 * 1024);
    write(&workspace, "/fault", &payload);
    // The fused message's pack insert waits for this hold, before the lane.
    let mut holder = rig.store.admin();
    holder
        .batch_execute("BEGIN; LOCK TABLE pack IN SHARE MODE")
        .unwrap();
    let mut observer = rig.store.admin();
    let pattern = format!("%INTO {schema}.pack %");
    let first = std::thread::scope(|scope| {
        let (proxy, writer) = (&rig.proxy, rig.writer);
        let cutter = scope.spawn(move || {
            let waiting = wait_until(Duration::from_secs(20), || {
                observer
                    .query_opt(
                        "SELECT 1 FROM pg_stat_activity WHERE pid = $1 \
                          AND wait_event_type = 'Lock' AND query LIKE $2",
                        &[&writer, &pattern],
                    )
                    .unwrap()
                    .is_some()
            });
            assert!(proxy.cut(writer), "cut");
            waiting
        });
        let first = verdict(&commit(&workspace));
        assert!(cutter.join().unwrap(), "the insert waited at the hold");
        first
    });
    assert!(
        matches!(first, Verdict::Unknown(_)),
        "lost reply: {first:?}"
    );
    assert!(alive(&rig.store, rig.writer), "the old backend still waits");
    assert!(workspace.has_retained(), "the unknown Commit is retained");
    // The next Commit ends the old backend before it concludes "not
    // committed", folds the change back and carries it itself; the hold
    // goes once the old backend is gone.
    let follow = std::thread::scope(|scope| {
        let releaser = scope.spawn(|| {
            let gone = wait_until(Duration::from_secs(20), || !alive(&rig.store, rig.writer));
            holder.batch_execute("ROLLBACK").unwrap();
            gone
        });
        let resolution = workspace
            .resolve_retained()
            .unwrap_or_else(|unresolved| panic!("{unresolved:?}"))
            .expect("a retained Commit");
        assert!(resolution.outcome.is_none(), "not committed: folded back");
        let follow = verdict(&commit(&workspace));
        assert!(releaser.join().unwrap(), "the old backend was ended");
        follow
    });
    let Verdict::Committed(id) = follow else {
        panic!("the follow-up carries the change: {follow:?}");
    };
    assert_eq!(reopens(&rig.service), 1, "one reopen");
    let chain = rig.store.chain(branch);
    assert_eq!(chain[0].id, id);
    assert_eq!(chain[0].parent, Some(warm), "the change landed once");
    assert!(landed(&rig.store, branch, "/fault", &payload));
    rig.service.unmount(workspace);
    rig.service.stop();
    assert!(rig.store.no_open_transaction());
}

#[test]
fn a_mount_kept_across_the_reopen_keeps_its_row_and_keys() {
    let Some(rig) = rig("reopen_mounts") else {
        return;
    };
    let (a, b) = (rig.store.fork("a"), rig.store.fork("b"));
    let held = rig.service.bind(a);
    let committer = rig.service.bind(b);
    assert_eq!(
        keys_by_backend(&rig),
        vec![(rig.writer, 4)],
        "Store, project, a, b"
    );
    kill(&rig.store, rig.writer);
    write(&committer, "/b", b"b\n");
    committed(&committer, "b's Commit reopens");
    assert_eq!(reopens(&rig.service), 1, "one reopen");
    let keys = keys_by_backend(&rig);
    assert_eq!(keys.len(), 1, "one backend holds keys: {keys:?}");
    assert_ne!(keys[0].0, rig.writer, "the new backend");
    assert_eq!(keys[0].1, 4, "a's and b's keys are held again");
    assert_eq!(intent_rows(&rig.store, a), 1, "a's row stays");
    assert_eq!(remove(&rig.store, "a"), "MountsActive");
    assert_eq!(compact(&rig.store, 1), "MountsActive");
    rig.service.unmount(held);
    assert_eq!(intent_rows(&rig.store, a), 0, "a's row released");
    assert_eq!(keys_by_backend(&rig)[0].1, 3, "a's Branch key released");
    rig.service.unmount(committer);
    assert_eq!(keys_by_backend(&rig), vec![], "no key left");
    assert_eq!(compact(&rig.store, 2), "Ok");
    rig.service.stop();
}

#[test]
fn a_key_that_cannot_be_taken_again_refuses_the_reopen() {
    let Some(rig) = rig("reopen_key_refused") else {
        return;
    };
    let a = rig.store.fork("a");
    let workspace = rig.service.bind(a);
    let keys = keys_of(&rig, rig.writer);
    assert_eq!(keys.len(), 3, "a's Store, project and Branch keys");
    write(&workspace, "/a", b"a\n");
    kill(&rig.store, rig.writer);
    // An admin session takes one of a's keys exclusively.
    let mut admin = rig.store.admin();
    assert!(
        keys_of(&rig, rig.writer).is_empty(),
        "the old backend holds nothing"
    );
    let exclusive = keys[2];
    admin
        .query_one("SELECT pg_advisory_lock($1)", &[&exclusive])
        .unwrap();
    let refused = verdict(&commit(&workspace));
    assert!(
        matches!(refused, Verdict::Refused(_)),
        "the reopen is refused: {refused:?}"
    );
    assert_eq!(reopens(&rig.service), 0, "no reopen");
    assert_eq!(intent_rows(&rig.store, a), 1, "a's row stays");
    admin
        .query_one("SELECT pg_advisory_unlock($1)", &[&exclusive])
        .unwrap();
    committed(&workspace, "the next Commit reopens");
    assert_eq!(reopens(&rig.service), 1, "one reopen");
    assert!(landed(&rig.store, a, "/a", b"a\n"));
    rig.service.unmount(workspace);
    assert_eq!(intent_rows(&rig.store, a), 0, "a's row released");
    rig.service.stop();
}

/// The session advisory keys backend `pid` holds, as `bigint` keys.
fn keys_of(rig: &Rig, pid: i32) -> Vec<i64> {
    rig.store
        .admin()
        .query(
            "SELECT ((classid::bigint << 32) | objid::bigint) FROM pg_locks \
              WHERE locktype = 'advisory' AND objsubid = 1 AND granted AND pid = $1 \
              ORDER BY 1",
            &[&pid],
        )
        .unwrap()
        .iter()
        .map(|row| row.get(0))
        .collect()
}

#[test]
fn a_mount_lost_after_its_commit_leaves_no_row_after_the_reopen() {
    let Some(rig) = rig("reopen_orphan") else {
        return;
    };
    let (twin, branch, other) = (
        rig.store.fork("twin"),
        rig.store.fork("orphan"),
        rig.store.fork("other"),
    );
    if let Some(prefilled) = rig.service.service.store.prefill_reader() {
        prefilled.unwrap();
    }
    // A repeated mount's flight: the writer's snapshot read and its
    // admission message, whose last reply follows the intent's COMMIT.
    let warm = rig.service.bind(twin);
    rig.service.unmount(warm);
    rig.proxy.arm(None);
    let counted = rig.service.bind(twin);
    let k = rig.proxy.disarm().seen;
    rig.service.unmount(counted);
    rig.proxy.arm(Some((k - 1, Fault::Drop)));
    let lost = rig.service.try_bind(branch);
    assert!(rig.proxy.disarm().fired.is_some(), "the fault fired");
    let error = lost.err().expect("the mount's reply was lost");
    assert!(error.contains("UnknownOutcome"), "unknown: {error}");
    assert_eq!(intent_rows(&rig.store, branch), 1, "the row committed");
    assert_eq!(compact(&rig.store, 3), "MountsActive");
    // The next write call reopens and releases the orphaned row.
    let workspace = rig.service.bind(other);
    assert_eq!(reopens(&rig.service), 1, "one reopen");
    assert_eq!(
        intent_rows(&rig.store, branch),
        0,
        "the orphaned row is gone"
    );
    rig.service.unmount(workspace);
    assert_eq!(compact(&rig.store, 4), "Ok");
    rig.service.stop();
}

#[test]
fn a_reopen_against_a_server_that_is_down_fails_and_the_next_one_works() {
    let Some(rig) = rig("reopen_down") else {
        return;
    };
    let branch = rig.store.fork("down");
    let workspace = rig.service.bind(branch);
    write(&workspace, "/warm", b"warm\n");
    let warm = committed(&workspace, "warm-up");
    rig.proxy.refuse(true);
    kill(&rig.store, rig.writer);
    write(&workspace, "/down", b"down\n");
    let refused = verdict(&commit(&workspace));
    assert!(
        matches!(refused, Verdict::Refused(_)),
        "no connection, nothing sent: {refused:?}"
    );
    assert_eq!(reopens(&rig.service), 0, "no reopen");
    assert_eq!(rig.store.chain(branch)[0].id, warm, "nothing published");
    rig.proxy.refuse(false);
    let id = committed(&workspace, "the Commit after the server is back");
    assert_eq!(reopens(&rig.service), 1, "one reopen");
    assert_eq!(rig.store.chain(branch)[0].parent, Some(warm));
    assert_eq!(rig.store.chain(branch)[0].id, id);
    assert!(landed(&rig.store, branch, "/down", b"down\n"));
    rig.service.unmount(workspace);
    rig.service.stop();
}

#[test]
fn two_commits_queued_when_the_writer_dies_reopen_once() {
    let Some(rig) = rig("reopen_queue") else {
        return;
    };
    let (a, b) = (rig.store.fork("a"), rig.store.fork("b"));
    let (wa, wb) = (rig.service.bind(a), rig.service.bind(b));
    write(&wa, "/a", b"a\n");
    write(&wb, "/b", b"b\n");
    kill(&rig.store, rig.writer);
    let gate = Gate::new(2);
    let (va, vb) = std::thread::scope(|scope| {
        let gate = &gate;
        let ta = scope.spawn(|| {
            assert!(gate.wait());
            verdict(&commit(&wa))
        });
        let tb = scope.spawn(|| {
            assert!(gate.wait());
            verdict(&commit(&wb))
        });
        (ta.join().unwrap(), tb.join().unwrap())
    });
    assert!(matches!(va, Verdict::Committed(_)), "a: {va:?}");
    assert!(matches!(vb, Verdict::Committed(_)), "b: {vb:?}");
    assert_eq!(reopens(&rig.service), 1, "one reopen for both");
    assert!(landed(&rig.store, a, "/a", b"a\n"));
    assert!(landed(&rig.store, b, "/b", b"b\n"));
    rig.service.unmount(wa);
    rig.service.unmount(wb);
    rig.service.stop();
}
