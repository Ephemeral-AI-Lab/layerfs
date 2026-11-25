//! #768 independent verification on PostgreSQL: the Store's allocators after
//! a Commit whose reply is lost at every server message.
//!
//! Each run Commits a change that takes many inode ordinals (a new
//! directory of 150 small files, 40 on the raced flight) and new pack ids (a 140 KiB file) through a
//! proxy that faults message `N`: the connection dropped (`Drop`) or the
//! backend killed while its replies are held (`HoldKill`). Flights: a warm
//! Commit (the handle's kept id block), the first Commit of a newly opened
//! service (a fresh reservation and the ordinal tail released in the same
//! message), and a raced one (the fallback path). The service is abandoned
//! after the follow-up Commit, so its kept block is leaked, never released.
//!
//! After every run, besides the Branch being old or exactly one Commit on
//! (`adversary::check_attempt`), the allocator never stands behind a stored
//! row: no pack id or inode ordinal at or past `store_policy`'s counters, no
//! two value blocks overlapping, no locator into a missing pack, no object
//! located twice. Every tenth run a direct service Commits another 150-file
//! change, so ids reserved after a rewind are used. At the end the audit
//! and amcheck pass. Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
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
    apply, check_attempt, commit, random_bytes, tree_of, verdict, write, Op, Service, TestStore,
    Verdict,
};
use fault_proxy::{Fault, FaultProxy};

const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n")];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Flight {
    Warm,
    Cold,
    Raced,
}

/// A new directory of `files` small files and one 140 KiB file.
fn change(tag: &str, files: usize) -> Vec<Op> {
    let mut ops = vec![Op::Mkdir(format!("/{tag}"))];
    for n in 0..files {
        ops.push(Op::Write(
            format!("/{tag}/f{n}"),
            format!("{tag} {n}\n").into_bytes(),
        ));
    }
    ops.push(Op::Write(
        format!("/{tag}/big"),
        random_bytes(tag.len() as u64 * 31 + 7, 140 * 1024),
    ));
    ops
}

/// One run on Branch `name`; returns the messages the window saw and a
/// state label.
fn run(
    store: &TestStore,
    proxy: &FaultProxy,
    proxied: &str,
    name: &str,
    flight: Flight,
    fault: Option<(u64, Fault)>,
) -> (u64, String) {
    let branch = store.fork(name);
    let service = Service::open(
        store,
        store.with_lock_timeout(&store.selection_via(proxied), 5_000),
    );
    let workspace = service.bind(branch);
    let mut old = None;
    if flight != Flight::Cold {
        write(&workspace, "/warm", b"warm\n");
        match verdict(&commit(&workspace)) {
            Verdict::Committed(id) => old = Some(id),
            other => panic!("{name}: warm-up {other:?}"),
        }
    }
    if flight == Flight::Raced {
        let mover = store.open();
        let other = mover.bind(branch);
        write(&other, "/mover", b"mover\n");
        match verdict(&commit(&other)) {
            Verdict::Committed(id) => old = Some(id),
            v => panic!("{name}: mover {v:?}"),
        }
        mover.unmount(other);
        mover.stop();
    }
    // The raced flight's merge makes its message long: fewer files keep
    // the sweep and the final audit inside their bounds.
    let files = if flight == Flight::Raced { 40 } else { 150 };
    for op in change(name, files) {
        apply(&workspace, &op).unwrap();
    }
    // The intended tree: the Branch's head merged with this change.
    let mut expected = store.tree(branch);
    for (path, node) in tree_of(&workspace) {
        expected.insert(path, node);
    }
    proxy.arm(fault);
    let first = verdict(&commit(&workspace));
    let window = proxy.disarm();
    let follow = verdict(&commit(&workspace));
    let _ = service.try_unmount(workspace);
    service.abandon();
    let state = check_attempt(store, branch, old, &expected, &first, &follow, name);
    let failures = verify768::duplicate_allocations(store);
    assert!(
        failures.is_empty(),
        "{name}: first {first:?}, follow {follow:?}: allocation {failures:#?}"
    );
    let class = match first {
        Verdict::Committed(_) => "committed",
        Verdict::Refused(_) => "refused",
        Verdict::Unknown(_) => "unknown",
        _ => "other",
    };
    (window.seen, format!("{class}/{state}"))
}

fn sweep(tag: &str, flight: Flight, fault: Fault, part: u64, parts: u64) {
    let Some(store) = TestStore::postgres(tag, SEED) else {
        return;
    };
    let (url, _) = store.postgres.clone().unwrap();
    let (proxy, proxied) = FaultProxy::start(&url);
    run(&store, &proxy, &proxied, "prime", flight, None);
    let (k, _) = run(&store, &proxy, &proxied, "clean", flight, None);
    let mut states = std::collections::BTreeMap::<String, u64>::new();
    let mut direct = 0;
    for n in (0..k).filter(|n| n % parts == part) {
        let (_, state) = run(
            &store,
            &proxy,
            &proxied,
            &format!("n{n}"),
            flight,
            Some((n, fault)),
        );
        *states.entry(state).or_default() += 1;
        if n % 10 == part {
            direct += 1;
            let service = store.open();
            let branch = store.fork(&format!("direct{direct}"));
            let workspace = service.bind(branch);
            for op in change(&format!("direct{direct}"), 150) {
                apply(&workspace, &op).unwrap();
            }
            let answer = verdict(&commit(&workspace));
            assert!(
                matches!(answer, Verdict::Committed(_)),
                "direct after n{n}: {answer:?}"
            );
            service.unmount(workspace);
            service.stop();
            let failures = verify768::duplicate_allocations(&store);
            assert!(failures.is_empty(), "direct after n{n}: {failures:#?}");
        }
    }
    println!(
        "VERIFY_UNKNOWN {{\"tag\":\"{tag}\",\"flight\":\"{flight:?}\",\"fault\":\"{fault:?}\",\"K\":{k},\"states\":{states:?}}}"
    );
    let failures = store.integrity_failures();
    assert!(failures.is_empty(), "audit: {failures:#?}");
    let failures = verify768::amcheck(&store);
    assert!(failures.is_empty(), "amcheck: {failures:#?}");
}

#[test]
fn drop_warm() {
    sweep("v768u_dw", Flight::Warm, Fault::Drop, 0, 1);
}
#[test]
fn drop_cold() {
    sweep("v768u_dc", Flight::Cold, Fault::Drop, 0, 1);
}
#[test]
fn holdkill_warm() {
    sweep("v768u_kw", Flight::Warm, Fault::HoldKill, 0, 1);
}
#[test]
fn holdkill_cold() {
    sweep("v768u_kc", Flight::Cold, Fault::HoldKill, 0, 1);
}
#[test]
fn drop_raced_part_0() {
    sweep("v768u_dr0", Flight::Raced, Fault::Drop, 0, 6);
}
#[test]
fn drop_raced_part_1() {
    sweep("v768u_dr1", Flight::Raced, Fault::Drop, 1, 6);
}
#[test]
fn drop_raced_part_2() {
    sweep("v768u_dr2", Flight::Raced, Fault::Drop, 2, 6);
}
#[test]
fn drop_raced_part_3() {
    sweep("v768u_dr3", Flight::Raced, Fault::Drop, 3, 6);
}
#[test]
fn drop_raced_part_4() {
    sweep("v768u_dr4", Flight::Raced, Fault::Drop, 4, 6);
}
#[test]
fn drop_raced_part_5() {
    sweep("v768u_dr5", Flight::Raced, Fault::Drop, 5, 6);
}
