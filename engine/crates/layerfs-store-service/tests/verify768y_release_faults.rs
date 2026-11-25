//! Independent #768 check (not the author's tests): a lost reply or a killed
//! backend on the release write a closing Workspace sends when it finds the
//! Store's idle slot taken (`Producer::hand_to`, the conditional rewind
//! `UPDATE ... WHERE next_ordinal = first + count`), on PostgreSQL.
//!
//! Two Workspaces on one service commit one file each (the second handle's
//! block is reserved after the first's). The first unmount takes the idle
//! slot; the second finds it taken, and the handle whose kept ordinals end
//! later (the second Commit's) is released: in `reverse` order that is the
//! slot's handle, in mount order the closing one. A pass-through tap names
//! the SQL of every server message, so the release transaction (the one
//! whose requests run the publication statement) is found in a clean
//! flight, and every one of its messages is then faulted, `Drop` and
//! `HoldKill`, in both orders.
//!
//! After each faulted unmount, checked:
//! * the Store's counter is either unmoved or rewound by exactly the
//!   released handle's kept ordinals, never below a stored value;
//! * the faulted service never moves the counter again (no second release,
//!   none at its close), and a Commit on it is never answered `Committed`
//!   with a duplicate; its answer is reported;
//! * after the faulted service is gone, every id below the counter that no
//!   row uses stays unused for good: a fresh service's Commits on both
//!   Branches and a new one land, and no id lost at that point is used
//!   (`support/verify768y.rs`); trees read back;
//! * at the end: no backend in a transaction, amcheck, no duplicated
//!   allocation.
//!
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
#[path = "support/verify768_tap.rs"]
#[allow(dead_code)]
mod verify768_tap;
#[path = "support/verify768x.rs"]
#[allow(dead_code)]
mod verify768x;
#[path = "support/verify768y.rs"]
#[allow(dead_code)]
mod verify768y;

use adversary::{commit, verdict, write, Service, TestStore, Verdict};
use fault_proxy::{Fault, FaultProxy, Window};
use std::net::SocketAddr;
use verify768_tap::{annotate, Reply, Tap};
use verify768x::{alloc, Alloc};
use verify768y::Lost;

const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n")];

struct Rig {
    store: TestStore,
    proxy: FaultProxy,
    tap: Tap,
    url: String,
}

fn rig(tag: &str) -> Option<Rig> {
    let store = TestStore::postgres(tag, SEED)?;
    let (url, _) = store.postgres.clone().unwrap();
    let (proxy, _) = FaultProxy::start(&url);
    let upstream: SocketAddr = format!("127.0.0.1:{}", proxy.port).parse().unwrap();
    let tap = Tap::start(upstream);
    let url = fault_proxy::proxied_url(&url, tap.port).0;
    Some(Rig {
        store,
        proxy,
        tap,
        url,
    })
}

struct Run {
    window: Window,
    taps: Vec<Vec<Reply>>,
    unmount: Result<(), String>,
    before: Alloc,
    after_unmount: Alloc,
    same_service: String,
}

fn committed(workspace: &layerfs_store_service::store::BoundWorkspace, what: &str) {
    let answer = verdict(&commit(workspace));
    assert!(matches!(answer, Verdict::Committed(_)), "{what}: {answer:?}");
}

/// One faulted (or clean) run on Branches `{name}-0` and `{name}-1`.
fn run(rig: &Rig, lost: &mut Lost, name: &str, reverse: bool, target: Option<(u64, Fault)>) -> Run {
    let store = &rig.store;
    let label = format!("{name} reverse={reverse} target={target:?}");
    let b0 = store.fork(&format!("{name}-0"));
    let b1 = store.fork(&format!("{name}-1"));
    let selection = store.with_lock_timeout(&store.selection_via(&rig.url), 5_000);
    let service = Service::open(store, selection);
    let w0 = service.bind(b0);
    let w1 = service.bind(b1);
    write(&w0, "/zero", format!("{name} zero\n").as_bytes());
    committed(&w0, &label);
    write(&w1, "/one", format!("{name} one\n").as_bytes());
    committed(&w1, &label);
    let (first, second) = if reverse { (w1, w0) } else { (w0, w1) };
    service.unmount(first);
    let before = alloc(store);
    rig.tap.arm();
    rig.proxy.arm(target);
    let unmount = service.try_unmount(second);
    let window = rig.proxy.disarm();
    let taps = rig.tap.disarm();
    let after_unmount = alloc(store);
    // The released handle kept the last ordinals below the counter: a
    // rewind moves it back to the last stored value, never further.
    assert!(
        after_unmount.next_ordinal == before.next_ordinal
            || after_unmount.next_ordinal == before.last_ordinal + 1,
        "{label}: counter {before:?} -> {after_unmount:?}"
    );
    lost.check(store, &format!("{label}: after the unmount"));
    // The same service: a new Workspace's Commit.
    let b2 = store.fork(&format!("{name}-2"));
    let same_service = match service.try_bind(b2) {
        Err(error) => format!("bind refused: {}", error.chars().take(120).collect::<String>()),
        Ok(w2) => {
            write(&w2, "/two", format!("{name} two\n").as_bytes());
            let answer = verdict(&commit(&w2));
            lost.check(store, &format!("{label}: after the same service's Commit"));
            let _ = service.try_unmount(w2);
            match answer {
                Verdict::Committed(_) => "committed".to_owned(),
                other => format!("{other:?}").chars().take(160).collect(),
            }
        }
    };
    let counter_before_close = alloc(store).next_ordinal;
    let Service { service, owner } = service;
    let closed = service.close();
    let _ = owner.stop();
    let after_close = alloc(store);
    if target.is_some() && !same_service.starts_with("committed") {
        assert_eq!(
            after_close.next_ordinal, counter_before_close,
            "{label}: the faulted service released again at its close ({closed:?})"
        );
    }
    let (lost_ord, _) = lost.closed_point(store, &format!("{label}: faulted service gone"));
    // A fresh service lands Commits on all three Branches; no lost id used.
    let fresh = store.open();
    for (branch, path) in [(b0, "/fresh0"), (b1, "/fresh1"), (store.fork(&format!("{name}-3")), "/fresh3")] {
        let workspace = fresh.bind(branch);
        write(&workspace, path, format!("{name} {path}\n").as_bytes());
        committed(&workspace, &format!("{label}: fresh {path}"));
        fresh.unmount(workspace);
    }
    fresh.service.close().unwrap();
    fresh.owner.stop().unwrap();
    lost.closed_point(store, &format!("{label}: after the fresh Commits"));
    let tree0 = store.tree(b0);
    let tree1 = store.tree(b1);
    for (tree, own) in [(&tree0, "/zero"), (&tree1, "/one")] {
        assert!(
            tree.contains_key(own) && tree.contains_key("/seed"),
            "{label}: {tree:?}"
        );
    }
    assert!(tree0.contains_key("/fresh0") && tree1.contains_key("/fresh1"));
    println!(
        "Y_RELEASE_RUN {{\"label\":\"{label}\",\"fired\":\"{:?}\",\"unmount\":\"{}\",\"rewound\":{},\"same_service\":\"{}\",\"close\":\"{}\",\"lost_ordinals_total\":{lost_ord}}}",
        window.fired.as_ref().map(|f| (f.at, f.kind, f.killed)),
        format!("{unmount:?}").chars().take(100).collect::<String>().replace('"', "'"),
        after_unmount.next_ordinal != before.next_ordinal,
        same_service.replace('"', "'"),
        format!("{closed:?}").chars().take(80).collect::<String>().replace('"', "'"),
    );
    Run {
        window,
        taps,
        unmount,
        before,
        after_unmount,
        same_service,
    }
}

/// The release transaction's window positions in a clean flight: the
/// writer transaction whose requests run the publication statement.
fn release_positions(run: &Run) -> Vec<u64> {
    let annotated = annotate(&run.window, &run.taps);
    let mut dump = String::new();
    let mut found: Vec<u64> = Vec::new();
    let mut current: Vec<(u64, usize)> = Vec::new();
    let mut publishes = false;
    let mut connection = None;
    for (n, (conn, reply)) in annotated.iter().enumerate() {
        dump.push_str(&format!("{n} c{conn} {} {}\n", reply.kind, reply.sql));
        let upper = reply.sql.to_ascii_uppercase();
        if upper.starts_with("BEGIN") && current.iter().all(|(_, c)| c != conn) && connection.is_none() {
            connection = Some(*conn);
            current.clear();
            publishes = false;
        }
        if connection != Some(*conn) {
            continue;
        }
        current.push((n as u64, *conn));
        if reply.sql.contains("inode_value_block") || reply.sql.contains("WITH RECURSIVE src") {
            publishes = true;
        }
        let ends = reply.kind == 'Z'
            && (upper.trim_end().ends_with("COMMIT") || upper.trim_end().ends_with("ROLLBACK"));
        if ends {
            if publishes && found.is_empty() {
                found = current.iter().map(|(n, _)| *n).collect();
            }
            connection = None;
            current.clear();
        }
    }
    println!("RELEASE_FLIGHT\n{dump}");
    assert!(!found.is_empty(), "no release transaction in the clean flight");
    found
}

fn sweep(tag: &str, reverse: bool, fault: Fault) {
    let Some(rig) = rig(tag) else { return };
    let _slot = verify768::sweep_slot();
    let mut lost = Lost::default();
    lost.closed_point(&rig.store, "init");
    let clean = run(&rig, &mut lost, "clean", reverse, None);
    assert!(clean.unmount.is_ok(), "{:?}", clean.unmount);
    assert_eq!(
        clean.after_unmount.next_ordinal,
        clean.before.last_ordinal + 1,
        "the clean release rewinds to the last stored value"
    );
    assert_eq!(clean.same_service, "committed");
    let positions = release_positions(&clean);
    println!("RELEASE_TX {positions:?}");
    let mut classes = std::collections::BTreeMap::<String, u64>::new();
    for n in &positions {
        let outcome = run(&rig, &mut lost, &format!("n{n}"), reverse, Some((*n, fault)));
        assert!(outcome.window.fired.is_some(), "n{n}: the fault fired");
        let class = format!(
            "rewound={} unmount_ok={} same={}",
            outcome.after_unmount.next_ordinal != outcome.before.next_ordinal,
            outcome.unmount.is_ok(),
            outcome.same_service.split(['(', ':']).next().unwrap_or("")
        );
        *classes.entry(class).or_default() += 1;
    }
    println!(
        "RELEASE_SUMMARY {{\"tag\":\"{tag}\",\"positions\":{},\"classes\":\"{classes:?}\"}}",
        positions.len()
    );
    let store = &rig.store;
    assert!(store.no_open_transaction(), "a backend left in a transaction");
    let amcheck = verify768::amcheck(store);
    assert!(amcheck.is_empty(), "amcheck: {amcheck:#?}");
    let duplicates = verify768::duplicate_allocations(store);
    assert!(duplicates.is_empty(), "allocations: {duplicates:#?}");
}

#[test]
fn slot_release_lost_reply_reverse() {
    adversary::tmp();
    sweep("y_rel_drop_rev", true, Fault::Drop);
}

#[test]
fn slot_release_backend_killed_reverse() {
    adversary::tmp();
    sweep("y_rel_kill_rev", true, Fault::HoldKill);
}

#[test]
fn slot_release_lost_reply_in_order() {
    adversary::tmp();
    sweep("y_rel_drop_ord", false, Fault::Drop);
}

#[test]
fn slot_release_backend_killed_in_order() {
    adversary::tmp();
    sweep("y_rel_kill_ord", false, Fault::HoldKill);
}

/// The release write held (proxy `Pause`) after its transaction began and
/// before it takes the lane, while another service on the Store reserves
/// and commits past the released handle's kept ordinals. The conditional
/// rewind must then not apply (the counter no longer ends them): the other
/// service's kept block stays above the counter, and its next Commit and a
/// fresh service's Commit never overlap anything. Both orders.
#[test]
fn slot_release_held_while_another_service_reserves() {
    adversary::tmp();
    for reverse in [true, false] {
        let Some(rig) = rig(&format!("y_rel_pause_{reverse}")) else {
            return;
        };
        let store = &rig.store;
        let mut lost = Lost::default();
        lost.closed_point(store, "init");
        let b0 = store.fork("p0");
        let b1 = store.fork("p1");
        let b2 = store.fork("p2");
        let selection = store.with_lock_timeout(&store.selection_via(&rig.url), 5_000);
        let service = Service::open(store, selection);
        let w0 = service.bind(b0);
        let w1 = service.bind(b1);
        write(&w0, "/zero", b"zero\n");
        committed(&w0, "w0");
        write(&w1, "/one", b"one\n");
        committed(&w1, "w1");
        let (first, second) = if reverse { (w1, w0) } else { (w0, w1) };
        service.unmount(first);
        let before = alloc(store);
        // Message 7 is the reply that ends the release transaction's BEGIN
        // batch (the clean flight of the sweeps above): the lane is not
        // taken yet.
        rig.proxy.arm(Some((7, Fault::Pause)));
        let held = std::thread::scope(|scope| {
            let unmount = scope.spawn(|| service.try_unmount(second));
            assert!(rig.proxy.wait_fired(std::time::Duration::from_secs(10)));
            // Another service reserves and commits while the release waits.
            let other = store.open();
            let w2 = other.bind(b2);
            for k in 0..40 {
                write(&w2, &format!("/other{k}"), format!("other {k}\n").as_bytes());
            }
            committed(&w2, "the other service");
            let mid = alloc(store);
            rig.proxy.release();
            let result = unmount.join().unwrap();
            (result, other, w2, mid)
        });
        let window = rig.proxy.disarm();
        let (result, other, w2, mid) = held;
        let after = alloc(store);
        println!(
            "Y_RELEASE_PAUSE {{\"reverse\":{reverse},\"fired\":{},\"unmount\":\"{}\",\"before\":\"{before:?}\",\"mid\":\"{mid:?}\",\"after\":\"{after:?}\"}}",
            window.fired.is_some(),
            format!("{result:?}").replace('"', "'")
        );
        assert!(result.is_ok(), "reverse={reverse}: {result:?}");
        assert!(
            mid.next_ordinal > before.next_ordinal,
            "the other service reserved past the kept ordinals: {before:?} {mid:?}"
        );
        assert_eq!(
            after.next_ordinal, mid.next_ordinal,
            "reverse={reverse}: the rewind applied after another reservation"
        );
        lost.check(store, "after the held release");
        // The other service's handle keeps its block above; it commits on.
        write(&w2, "/again", b"again\n");
        committed(&w2, "the other service again");
        other.unmount(w2);
        other.service.close().unwrap();
        other.owner.stop().unwrap();
        service.service.close().unwrap();
        service.owner.stop().unwrap();
        lost.closed_point(store, "both closed");
        let fresh = store.open();
        let w = fresh.bind(b0);
        write(&w, "/fresh", b"fresh\n");
        committed(&w, "fresh");
        fresh.unmount(w);
        fresh.service.close().unwrap();
        fresh.owner.stop().unwrap();
        lost.closed_point(store, "fresh closed");
        assert!(store.tree(b2).contains_key("/again"));
        let duplicates = verify768::duplicate_allocations(store);
        assert!(duplicates.is_empty(), "{duplicates:#?}");
    }
}
