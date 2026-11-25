//! #768 independent verification on PostgreSQL: a Workspace Commit whose
//! session is paused at every server message of its flight (the proxy holds
//! the replies from message `N`; the client is stopped there), and
//! disturbed from elsewhere while it waits:
//!
//! * `Swap`: what a compaction swap does, faithfully: one transaction locks
//!   the base tables `ACCESS EXCLUSIVE NOWAIT` and bumps
//!   `history_meta.catalog_incarnation`, returning the history serial it
//!   saw. Either the lock is refused (the paused Commit holds a lock: a real
//!   swap is `Busy` there) and the Commit must land, or the generation moved
//!   and the Commit lands only if its Commit row's history serial is below
//!   the bump's (published before the swap), else it is a definite `Stale`
//!   refusal with the Branch at its old head. A Commit published at the old
//!   generation after the swap fails the test.
//! * `Third`: a third writer (another service, short lane wait) commits its
//!   own file on the same Branch while the Commit is paused. Every answer is
//!   Committed or a definite refusal; the Branch's tree is exactly the seed,
//!   the head mover's file, and the file of each Commit answered Committed:
//!   no lost update and no publication nobody answered for.
//!
//! Each runs on a warm fused Commit and on a raced one (another Workspace
//! moved the head first: the fused attempt's race refusal and the fallback
//! on the separate path). Shares of each flight run as separate tests. At
//! the end every Commit row is on a chain, the audit and amcheck pass.
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "support/fault_proxy.rs"]
#[allow(dead_code)]
mod fault_proxy;
#[path = "support/verify768.rs"]
#[allow(dead_code)]
mod verify768;

use adversary::{
    assert_linear, commit, seed_tree, verdict, write, Node, Service, TestStore, Tree, Verdict,
};
use fault_proxy::{Fault, FaultProxy};
use std::sync::mpsc;
use std::time::Duration;

const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n"), ("/dir/inner", b"inner\n")];
/// Shares of a warm flight (62 messages) and of a raced one (451).
const WARM_PARTS: u64 = 3;
const RACED_PARTS: u64 = 6;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Flight {
    Warm,
    Raced,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Disturb {
    Swap,
    Third,
}

/// The base tables of the Store's schema, as a swap locks them.
fn tables(store: &TestStore) -> String {
    let schema = store.postgres.as_ref().unwrap().1.clone();
    let rows = store
        .admin()
        .query(
            "SELECT c.relname::text FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = $1 AND c.relkind = 'r' ORDER BY c.relname",
            &[&schema],
        )
        .unwrap();
    rows.iter()
        .map(|row| format!("{schema}.\"{}\"", row.get::<_, String>(0)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A swap's lock and generation bump: `Some(next_history_serial)` when it
/// committed, `None` when its NOWAIT lock was refused.
fn swap(store: &TestStore, locked: &str) -> Option<i64> {
    let mut admin = store.admin();
    let mut tx = admin.transaction().unwrap();
    if let Err(error) = tx.batch_execute(&format!(
        "LOCK TABLE {locked} IN ACCESS EXCLUSIVE MODE NOWAIT"
    )) {
        assert_eq!(
            error.code().map(|c| c.code()),
            Some("55P03"),
            "the swap lock: {error}"
        );
        return None;
    }
    let serial: i64 = tx
        .query_one(
            "UPDATE history_meta SET catalog_incarnation = catalog_incarnation + 1 WHERE id = 1 \
             RETURNING next_history_serial",
            &[],
        )
        .unwrap()
        .get(0);
    tx.commit().unwrap();
    Some(serial)
}

fn history_serial(store: &TestStore, id: &layerfs_history::CommitId) -> i64 {
    store
        .admin()
        .query_one(
            "SELECT history_serial FROM \"commit\" WHERE commit_id = $1",
            &[&id.to_bytes().to_vec()],
        )
        .unwrap()
        .get(0)
}

struct Run {
    first: Verdict,
    fired: bool,
    seen: u64,
    disturbed: String,
    third: Option<Verdict>,
}

/// One run on its own Branch `name`: a warm-up, the head moved (raced),
/// `/x` written, the Commit paused at `at` and disturbed.
fn run(
    store: &TestStore,
    proxy: &FaultProxy,
    proxied: &str,
    name: &str,
    flight: Flight,
    disturb: Disturb,
    at: Option<u64>,
) -> (Run, Tree) {
    let branch = store.fork(name);
    let service = Service::open(
        store,
        store.with_lock_timeout(&store.selection_via(proxied), 5_000),
    );
    let workspace = service.bind(branch);
    write(&workspace, "/warm", b"warm\n");
    assert!(
        matches!(verdict(&commit(&workspace)), Verdict::Committed(_)),
        "{name}: warm-up"
    );
    let mut base = seed_tree(SEED);
    base.insert("/warm".into(), Node::File(b"warm\n".to_vec()));
    if flight == Flight::Raced {
        let mover = store.open();
        let other = mover.bind(branch);
        write(&other, "/mover", b"mover\n");
        assert!(
            matches!(verdict(&commit(&other)), Verdict::Committed(_)),
            "{name}: mover"
        );
        mover.unmount(other);
        mover.stop();
        base.insert("/mover".into(), Node::File(b"mover\n".to_vec()));
    }
    write(&workspace, "/x", b"x\n");
    let third_service = (disturb == Disturb::Third)
        .then(|| Service::open(store, store.with_lock_timeout(&store.selection, 300)));
    let third_workspace = third_service.as_ref().map(|s| s.bind(branch));
    if let Some(w) = &third_workspace {
        write(w, "/third", b"third\n");
    }
    let locked = tables(store);
    proxy.arm(at.map(|n| (n, Fault::Pause)));
    let (sent, answer) = mpsc::channel();
    let (first, fired, disturbed, third) = std::thread::scope(|scope| {
        let workspace = &workspace;
        scope.spawn(move || {
            let _ = sent.send(verdict(&commit(workspace)));
        });
        let fired = at.is_some() && proxy.wait_fired(Duration::from_secs(5));
        let mut disturbed = String::from("none");
        let mut third = None;
        if fired {
            match disturb {
                Disturb::Swap => {
                    disturbed = match swap(store, &locked) {
                        Some(serial) => format!("swapped@{serial}"),
                        None => "busy".into(),
                    };
                }
                Disturb::Third => {
                    let w = third_workspace.as_ref().unwrap();
                    let (tx, rx) = mpsc::channel();
                    let handle = scope.spawn(move || {
                        let _ = tx.send(verdict(&commit(w)));
                    });
                    // The third writer may wait for a lock the paused
                    // Commit holds: its short lane wait answers it.
                    let early = rx.recv_timeout(Duration::from_secs(5)).ok();
                    proxy.release();
                    third = Some(early.unwrap_or_else(|| {
                        rx.recv_timeout(Duration::from_secs(30))
                            .expect("the third writer answers")
                    }));
                    let _ = handle.join();
                    disturbed = "third".into();
                }
            }
        }
        proxy.release();
        let first = answer
            .recv_timeout(Duration::from_secs(40))
            .expect("the Commit answers");
        (first, fired, disturbed, third)
    });
    let window = proxy.disarm();
    let _ = service.try_unmount(workspace);
    service.abandon();
    if let (Some(s), Some(w)) = (third_service, third_workspace) {
        let _ = s.try_unmount(w);
        s.abandon();
    }
    (
        Run {
            first,
            fired,
            seen: window.seen,
            disturbed,
            third,
        },
        base,
    )
}

fn check(store: &TestStore, name: &str, run: &Run, base: &Tree) -> String {
    let branch = store.branch(name);
    let chain = store.chain(branch);
    assert_linear(&chain);
    let label = format!(
        "{name}: first {:?}, disturbed {}, third {:?}",
        run.first, run.disturbed, run.third
    );
    let mut expected = base.clone();
    match &run.first {
        Verdict::Committed(id) => {
            assert!(
                chain.iter().any(|r| r.id == *id),
                "{label}: answered Commit off the chain"
            );
            expected.insert("/x".into(), Node::File(b"x\n".to_vec()));
            if let Some(serial) = run.disturbed.strip_prefix("swapped@") {
                let bump: i64 = serial.parse().unwrap();
                let made = history_serial(store, id);
                assert!(
                    made < bump,
                    "{label}: published at serial {made} after the swap saw {bump}: \
                     a publication into the swapped catalog"
                );
            }
        }
        Verdict::Refused(text) => {
            assert!(
                run.disturbed.starts_with("swapped@") || run.disturbed == "third",
                "{label}: refused with nothing to refuse it"
            );
            if run.disturbed.starts_with("swapped@") {
                assert!(
                    text.contains("Stale"),
                    "{label}: a refusal after a swap: {text}"
                );
            }
        }
        other => panic!("{label}: {other:?}"),
    }
    match &run.third {
        Some(Verdict::Committed(id)) => {
            assert!(
                chain.iter().any(|r| r.id == *id),
                "{label}: third off the chain"
            );
            expected.insert("/third".into(), Node::File(b"third\n".to_vec()));
        }
        Some(Verdict::Refused(_)) | None => {}
        Some(other) => panic!("{label}: the third writer answered {other:?}"),
    }
    assert_eq!(
        store.tree(branch),
        expected,
        "{label}: no lost update, nothing unanswered"
    );
    let first = match run.first {
        Verdict::Committed(_) => "committed",
        _ => "refused",
    };
    let third = match &run.third {
        Some(Verdict::Committed(_)) => "/third-committed",
        Some(_) => "/third-refused",
        None => "",
    };
    format!(
        "{first}/{}{third}",
        run.disturbed.split('@').next().unwrap()
    )
}

fn sweep(tag: &str, flight: Flight, disturb: Disturb, part: u64) {
    let Some(store) = TestStore::postgres(tag, SEED) else {
        return;
    };
    let (url, _) = store.postgres.clone().unwrap();
    let (proxy, proxied) = FaultProxy::start(&url);
    let mut names = Vec::new();
    // Prime, then count the flight's messages.
    for name in ["prime", "clean"] {
        let (outcome, base) = run(&store, &proxy, &proxied, name, flight, disturb, None);
        check(&store, name, &outcome, &base);
        names.push(name.to_owned());
        if name == "clean" {
            names.push(format!("K={}", outcome.seen));
        }
    }
    let k: u64 = names
        .pop()
        .unwrap()
        .trim_start_matches("K=")
        .parse()
        .unwrap();
    let mut states = std::collections::BTreeMap::<String, u64>::new();
    let parts = match flight {
        Flight::Warm => WARM_PARTS,
        Flight::Raced => RACED_PARTS,
    };
    for n in (0..k).filter(|n| n % parts == part) {
        let name = format!("n{n}");
        let started = std::time::Instant::now();
        let (outcome, base) = run(&store, &proxy, &proxied, &name, flight, disturb, Some(n));
        assert!(outcome.fired, "{name}: the pause at {n} of {k} fired");
        let state = check(&store, &name, &outcome, &base);
        println!(
            "VERIFY_PAUSE_RUN {{\"tag\":\"{tag}\",\"n\":{n},\"state\":\"{state}\",\"ms\":{}}}",
            started.elapsed().as_millis()
        );
        *states.entry(state).or_default() += 1;
        names.push(name);
    }
    println!(
        "VERIFY_PAUSE {{\"tag\":\"{tag}\",\"flight\":\"{flight:?}\",\"disturb\":\"{disturb:?}\",\"K\":{k},\"states\":{states:?}}}"
    );
    let reached: usize = names
        .iter()
        .map(|name| store.chain(store.branch(name)).len())
        .sum();
    assert_eq!(
        store.commit_rows(),
        reached as i64,
        "Commit rows off every chain"
    );
    let failures = store.integrity_failures();
    assert!(failures.is_empty(), "audit: {failures:#?}");
    let failures = verify768::amcheck(&store);
    assert!(failures.is_empty(), "amcheck: {failures:#?}");
}

#[test]
fn swap_at_every_pause_of_a_warm_commit_part_0() {
    sweep("v768p_sw_w0", Flight::Warm, Disturb::Swap, 0);
}

#[test]
fn swap_at_every_pause_of_a_warm_commit_part_1() {
    sweep("v768p_sw_w1", Flight::Warm, Disturb::Swap, 1);
}

#[test]
fn swap_at_every_pause_of_a_warm_commit_part_2() {
    sweep("v768p_sw_w2", Flight::Warm, Disturb::Swap, 2);
}

#[test]
fn third_writer_at_every_pause_of_a_warm_commit_part_0() {
    sweep("v768p_th_w0", Flight::Warm, Disturb::Third, 0);
}

#[test]
fn third_writer_at_every_pause_of_a_warm_commit_part_1() {
    sweep("v768p_th_w1", Flight::Warm, Disturb::Third, 1);
}

#[test]
fn third_writer_at_every_pause_of_a_warm_commit_part_2() {
    sweep("v768p_th_w2", Flight::Warm, Disturb::Third, 2);
}

#[test]
fn swap_at_every_pause_of_a_raced_commit_part_0() {
    sweep("v768p_sw_r0", Flight::Raced, Disturb::Swap, 0);
}

#[test]
fn swap_at_every_pause_of_a_raced_commit_part_1() {
    sweep("v768p_sw_r1", Flight::Raced, Disturb::Swap, 1);
}

#[test]
fn swap_at_every_pause_of_a_raced_commit_part_2() {
    sweep("v768p_sw_r2", Flight::Raced, Disturb::Swap, 2);
}

#[test]
fn swap_at_every_pause_of_a_raced_commit_part_3() {
    sweep("v768p_sw_r3", Flight::Raced, Disturb::Swap, 3);
}

#[test]
fn swap_at_every_pause_of_a_raced_commit_part_4() {
    sweep("v768p_sw_r4", Flight::Raced, Disturb::Swap, 4);
}

#[test]
fn swap_at_every_pause_of_a_raced_commit_part_5() {
    sweep("v768p_sw_r5", Flight::Raced, Disturb::Swap, 5);
}

#[test]
fn third_writer_at_every_pause_of_a_raced_commit_part_0() {
    sweep("v768p_th_r0", Flight::Raced, Disturb::Third, 0);
}

#[test]
fn third_writer_at_every_pause_of_a_raced_commit_part_1() {
    sweep("v768p_th_r1", Flight::Raced, Disturb::Third, 1);
}

#[test]
fn third_writer_at_every_pause_of_a_raced_commit_part_2() {
    sweep("v768p_th_r2", Flight::Raced, Disturb::Third, 2);
}

#[test]
fn third_writer_at_every_pause_of_a_raced_commit_part_3() {
    sweep("v768p_th_r3", Flight::Raced, Disturb::Third, 3);
}

#[test]
fn third_writer_at_every_pause_of_a_raced_commit_part_4() {
    sweep("v768p_th_r4", Flight::Raced, Disturb::Third, 4);
}

#[test]
fn third_writer_at_every_pause_of_a_raced_commit_part_5() {
    sweep("v768p_th_r5", Flight::Raced, Disturb::Third, 5);
}
