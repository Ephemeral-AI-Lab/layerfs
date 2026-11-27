//! #768: PostgreSQL backends killed while a Workspace Commit on the
//! separate path waits for, or works inside, the publication lane: the path
//! with the held-lane connection (finding #10). Since #768 stage 2 a Commit
//! over its bound head is fused and takes the lane in its own transaction
//! (`commit_fused_lane.rs` covers that); these Commits are forced, which
//! never fuses, so they keep taking the lane on the lane connection.
//!
//! * The lane row is held by a separate transaction (`store_lane FOR
//!   UPDATE`), so the Commit's writer waits for it in the Save's own
//!   publication transaction (pack bodies, then the lane row); then either
//!   that waiting writer backend or the Commit's idle lane connection is
//!   terminated (`pg_terminate_backend`), and the blocker is released.
//! * Inside the lane: at every writer message between the lane's grant and
//!   its release, the writer's replies are held and the lane backend is
//!   terminated (`support/fault_sweep.rs`, `Fault::HoldKillOther`): finding
//!   #10's nested write must then not publish without the lane.
//!
//! * Paused inside the lane: at each round-trip boundary of the writer
//!   between the lane's grant and its release, the writer's replies are held
//!   while another service removes another Branch of the project
//!   (`branch.remove` bumps the project epoch the Commit checks), then
//!   released. The removal either lands or is refused `Busy` within its lane
//!   wait; the Commit publishes or is refused, never both and never across a
//!   bump it did not see.
//!
//! Each case checks the Branch from fresh sessions (`check_attempt`: the old
//! head, or exactly one Commit past it with the intended tree; an unknown
//! outcome refuses the next Commit on that Workspace while it is retained;
//! a writer a lost reply quarantined is reopened by that next Commit, which
//! may then land the change once), that no backend is
//! left idle in a transaction, and that a new Commit still publishes within
//! a bound (nothing leaked the lane). Needs `LAYERFS_TEST_POSTGRES_URL`;
//! NOT_RUN without it.
#[path = "../support/verify/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "../support/fault/proxy.rs"]
#[allow(dead_code)]
mod fault_proxy;
#[path = "../support/fault/sweep.rs"]
#[allow(dead_code)]
mod fault_sweep;

use adversary::{
    check_attempt, commit, commit_forced, verdict, wait_until, write, Node, Service, TestStore,
    Tree, Verdict,
};
use fault_proxy::{Fault, FaultProxy};
use fault_sweep::{Flight, Positions};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n")];
/// The Commit's lane wait: longer than the test needs, so the kill decides.
const LOCK_TIMEOUT_MS: u32 = 20_000;
const BOUND: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Victim {
    /// The writer, waiting for the lane row.
    Writer,
    /// The idle lane connection, while the writer waits.
    IdleLane,
}

fn pid_alive(client: &mut postgres::Client, pid: i32) -> bool {
    client
        .query_one(
            "SELECT count(*) FROM pg_stat_activity WHERE pid = $1",
            &[&pid],
        )
        .unwrap()
        .get::<_, i64>(0)
        == 1
}

fn waiting_on_lock(client: &mut postgres::Client, pid: i32) -> bool {
    client
        .query_one(
            "SELECT count(*) FROM pg_stat_activity WHERE pid = $1 AND wait_event_type = 'Lock'",
            &[&pid],
        )
        .unwrap()
        .get::<_, i64>(0)
        == 1
}

fn lane_wait_kill(tag: &str, victim: Victim) {
    let Some(store) = TestStore::postgres(tag, SEED) else {
        return;
    };
    let (url, _) = store.postgres.clone().unwrap();
    let (proxy, proxied) = FaultProxy::start(&url);
    let branch = store.fork("work");
    let service = Service::open(
        &store,
        store.with_lock_timeout(&store.selection_via(&proxied), LOCK_TIMEOUT_MS),
    );
    let workspace = service.bind(branch);
    write(&workspace, "/warm", b"warm\n");
    proxy.arm(None);
    let old = match verdict(&commit_forced(&workspace)) {
        Verdict::Committed(id) => id,
        other => panic!("warm-up: {other:?}"),
    };
    let window = proxy.disarm();
    let pids = proxy.pids();
    let writer = window.log[0].0;
    let lane = window
        .log
        .iter()
        .map(|(c, _)| *c)
        .find(|c| *c != writer)
        .expect("the warm-up opened the lane connection");
    let (writer_pid, lane_pid) = (pids[writer].unwrap(), pids[lane].unwrap());
    write(&workspace, "/x", b"x\n");

    let mut blocker = store.admin();
    blocker
        .batch_execute("BEGIN; SELECT * FROM store_lane FOR UPDATE")
        .unwrap();
    let mut observer = store.admin();
    let (sent, answer) = mpsc::channel();
    let started = Instant::now();
    let first = std::thread::scope(|scope| {
        let workspace = &workspace;
        scope.spawn(move || {
            let _ = sent.send(verdict(&commit_forced(workspace)));
        });
        let waits = wait_until(Duration::from_secs(10), || {
            waiting_on_lock(&mut observer, writer_pid)
        });
        if !waits {
            let rows = observer
                .query(
                    "SELECT pid, state, coalesce(wait_event_type,''), left(query, 80) \
                     FROM pg_stat_activity WHERE datname = current_database()",
                    &[],
                )
                .unwrap();
            for row in rows {
                eprintln!(
                    "ACTIVITY {} {} {} {}",
                    row.get::<_, i32>(0),
                    row.get::<_, String>(1),
                    row.get::<_, String>(2),
                    row.get::<_, String>(3)
                );
            }
            eprintln!("PIDS writer {writer_pid} lane {lane_pid} all {pids:?}");
        }
        assert!(waits, "the Commit's writer waits for the held lane row");
        let killed = match victim {
            Victim::Writer => writer_pid,
            Victim::IdleLane => lane_pid,
        };
        observer
            .execute("SELECT pg_terminate_backend($1)", &[&killed])
            .unwrap();
        assert!(
            wait_until(Duration::from_secs(10), || !pid_alive(
                &mut observer,
                killed
            )),
            "the killed backend is gone"
        );
        // The lane row is free again: a Commit whose writer died must not
        // publish; one whose lane connection died takes the lane anew or
        // refuses.
        blocker.batch_execute("ROLLBACK").unwrap();
        answer.recv_timeout(BOUND).expect("the Commit answers")
    });
    let elapsed = started.elapsed();
    let follow = verdict(&commit_forced(&workspace));
    println!(
        "LANE_KILL {{\"victim\":\"{}\",\"first\":\"{first:?}\",\"follow\":\"{follow:?}\",\"ms\":{}}}",
        if victim == Victim::IdleLane {
            "idle_lane"
        } else {
            "writer"
        },
        elapsed.as_millis()
    );
    let _ = service.try_unmount(workspace);
    service.abandon();
    let mut expected: Tree = Tree::new();
    expected.insert("/seed".into(), Node::File(b"seed\n".to_vec()));
    expected.insert("/warm".into(), Node::File(b"warm\n".to_vec()));
    expected.insert("/x".into(), Node::File(b"x\n".to_vec()));
    let state = check_attempt(&store, branch, Some(old), &expected, &first, &follow, tag);
    if victim == Victim::Writer {
        // The killed writer's own call publishes nothing; the follow-up
        // reopens the writer (`session/reopen.rs`) and may land the change
        // itself, exactly once (`check_attempt`).
        assert!(
            !matches!(first, Verdict::Committed(_)),
            "a dead writer publishes nothing: {first:?}"
        );
        if state == "new" {
            assert!(
                matches!(follow, Verdict::Committed(_)),
                "only the follow-up published: {follow:?}"
            );
        }
    }
    assert!(
        store.no_open_transaction(),
        "no backend left in a transaction"
    );
    // Nothing kept the lane: a new Commit publishes well inside the wait.
    let started = Instant::now();
    let after = store.fork("after");
    let service = store.open();
    let workspace = service.bind(after);
    write(&workspace, "/after", b"after\n");
    assert!(matches!(
        verdict(&commit(&workspace)),
        Verdict::Committed(_)
    ));
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the lane is free"
    );
    service.unmount(workspace);
    service.stop();
    let failures = store.integrity_failures();
    assert!(failures.is_empty(), "audit: {failures:#?}");
}

#[test]
fn the_writer_killed_while_it_waits_for_the_lane() {
    lane_wait_kill("lane_writer", Victim::Writer);
}

#[test]
fn the_lane_connection_killed_while_the_writer_waits_for_the_lane() {
    lane_wait_kill("lane_idle", Victim::IdleLane);
}

#[test]
fn the_lane_backend_killed_while_the_commit_works_inside_the_lane() {
    fault_sweep::sweep(
        "lane_inside",
        Fault::HoldKillOther,
        Flight::Forced,
        Positions::InLaneWriter,
    );
}

/// One warm-up Commit and one armed Commit of `/x` on a new Branch `name`
/// through `proxy`; with `during`, run on this thread once the armed fault
/// fired (the Commit runs on another), before the fault is released.
fn paused(
    store: &TestStore,
    proxy: &FaultProxy,
    name: &str,
    target: Option<(u64, Fault)>,
    during: impl FnOnce() -> String,
) -> (
    Option<layerfs_history::CommitId>,
    Verdict,
    Verdict,
    fault_proxy::Window,
    String,
) {
    let branch = store.fork(name);
    let service = Service::open(
        store,
        store.with_lock_timeout(
            &store.selection_via(
                &fault_proxy::proxied_url(&store.postgres.as_ref().unwrap().0, proxy.port).0,
            ),
            5_000,
        ),
    );
    let workspace = service.bind(branch);
    write(&workspace, "/warm", b"warm\n");
    let old = match verdict(&commit_forced(&workspace)) {
        Verdict::Committed(id) => Some(id),
        other => panic!("{name}: warm-up {other:?}"),
    };
    write(&workspace, "/x", b"x\n");
    proxy.arm(target);
    let (first, during) = std::thread::scope(|scope| {
        let workspace = &workspace;
        let committing = scope.spawn(move || verdict(&commit_forced(workspace)));
        let during = if target.is_some() {
            let fired = proxy.wait_fired(Duration::from_secs(10));
            let answer = if fired { during() } else { "not fired".into() };
            proxy.release();
            answer
        } else {
            String::new()
        };
        (committing.join().unwrap(), during)
    });
    let window = proxy.disarm();
    let follow = verdict(&commit_forced(&workspace));
    let _ = service.try_unmount(workspace);
    service.abandon();
    (old, first, follow, window, during)
}

#[test]
fn a_branch_removal_while_the_commit_is_paused_inside_the_lane() {
    let Some(store) = TestStore::postgres("lane_pause", SEED) else {
        return;
    };
    let (url, _) = store.postgres.clone().unwrap();
    let (proxy, _) = FaultProxy::start(&url);
    let nothing = || String::new();
    paused(&store, &proxy, "prime", None, nothing);
    let (_, clean, _, window, _) = paused(&store, &proxy, "clean", None, nothing);
    assert!(matches!(clean, Verdict::Committed(_)));
    let log = &window.log;
    let writer = log[0].0;
    let first_lane = log.iter().position(|(c, _)| *c != writer).unwrap();
    let held = first_lane
        + log[first_lane..]
            .iter()
            .position(|(c, _)| *c == writer)
            .unwrap();
    let released = log.iter().rposition(|(c, _)| *c == writer).unwrap();
    let positions: Vec<u64> = (held..=released)
        .filter(|n| log[*n] == (writer, 'Z'))
        .map(|n| n as u64)
        .collect();
    assert!(!positions.is_empty());
    let remover = Service::open(&store, store.with_lock_timeout(&store.selection, 300));
    let mut expected: Tree = Tree::new();
    expected.insert("/seed".into(), Node::File(b"seed\n".to_vec()));
    expected.insert("/warm".into(), Node::File(b"warm\n".to_vec()));
    expected.insert("/x".into(), Node::File(b"x\n".to_vec()));
    for n in positions {
        let name = format!("p{n}");
        let gone = format!("gone{n}");
        store.fork(&gone);
        let (old, first, follow, window, removal) = paused(
            &store,
            &proxy,
            &name,
            Some((n, Fault::Pause)),
            || match remover.call(
                "branch.remove",
                vec![("branch", layerfs_protocol::sdk::SdkValue::text(&gone))],
            ) {
                Ok(_) => "Ok".into(),
                Err(refusal) => refusal.code,
            },
        );
        assert!(window.fired.is_some(), "{name}: the pause was reached");
        println!(
            "LANE_PAUSE {{\"n\":{n},\"removal\":\"{removal}\",\"first\":\"{first:?}\",\"follow\":\"{follow:?}\"}}"
        );
        assert!(
            removal == "Ok" || removal == "Busy",
            "{name}: branch.remove answered {removal}"
        );
        if let Verdict::Refused(text) = &first {
            assert!(
                text.contains("project epoch")
                    || text.contains("Conflict")
                    || text.contains("Busy"),
                "{name}: {text}"
            );
        }
        check_attempt(
            &store,
            store.branch(&name),
            old,
            &expected,
            &first,
            &follow,
            &name,
        );
        let handles = store.handles();
        let present = adversary::history(&handles)
            .branch(store.branch(&gone))
            .unwrap()
            .is_some();
        handles.close();
        assert_eq!(present, removal != "Ok", "{name}: {gone} after {removal}");
    }
    remover.stop();
    assert!(
        store.no_open_transaction(),
        "no backend left in a transaction"
    );
    let failures = store.integrity_failures();
    assert!(failures.is_empty(), "audit: {failures:#?}");
}
