//! External #768 check (not the author's sweep): a connection lost, or its
//! backend killed, while a raced fused Commit's fallback publishes the held
//! tail, and separately while it writes the hint rows, on PostgreSQL.
//!
//! The positions are chosen by what the writer is doing, not by message
//! index: a pass-through tap (`support/verify768_tap.rs`) names the SQL of
//! every server message, and the fallback's writer transactions after the
//! fused message are split into the tail's publication (pack, locator,
//! value rows) and the hint transaction (`binding_hint_block`). Every
//! message of each is faulted, `Drop` and `HoldKill`.
//!
//! After each faulted Commit the SAME Workspace commits again with no new
//! change. Checked from fresh sessions:
//!
//! * the follow-up is never refused `InFlight`, and no Commit is retained;
//! * the Branch's chain is exactly the Commits answered `Committed`
//!   (warm-up, the mover's, then the faulted call's and the follow-up's if
//!   answered so), in order; the Store holds no Commit row off a chain;
//! * the change is not lost to the Store: a fresh service commits it;
//! * at the end: the Store audit, amcheck, no duplicated allocation, and no
//!   backend left in a transaction.
//!
//! `the_same_service_reopens_after_a_lost_fallback_reply` pins the
//! behaviour after a lost reply: the faulted service's writer session is
//! quarantined, and the same Workspace's follow-up reopens it once
//! (postgresql.md, "Writer reopen") and lands the change exactly once
//! (never `InFlight`, nothing published twice); nothing is resent.
//!
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "../support/verify/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "../support/fault/proxy.rs"]
#[allow(dead_code)]
mod fault_proxy;
#[path = "../support/verify/core.rs"]
#[allow(dead_code)]
mod verify768;
#[path = "../support/fault/wire_tap.rs"]
#[allow(dead_code)]
mod verify768_tap;

use adversary::{commit, verdict, write, Node, Service, TestStore, Tree, Verdict};
use fault_proxy::{Fault, FaultProxy, Window};
use layerfs_history::CommitId;
use layerfs_store_service::store::CommitError;
use std::net::SocketAddr;
use verify768_tap::{annotate, Reply, Tap};

const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n"), ("/dir/inner", b"inner\n")];

/// One Store reached through tap -> fault proxy -> server.
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
    first: Verdict,
    follow: Verdict,
    follow_in_flight: bool,
    retained_after_follow: bool,
    answered: Vec<CommitId>,
    /// The faulted service writer's reopens by the end of the run.
    reopens: u64,
}

fn content(name: &str) -> Vec<u8> {
    format!("fault {name}\n").into_bytes()
}

fn expected(name: &str) -> Tree {
    let mut tree = Tree::new();
    tree.insert("/seed".into(), Node::File(b"seed\n".to_vec()));
    tree.insert("/dir".into(), Node::Dir);
    tree.insert("/dir/inner".into(), Node::File(b"inner\n".to_vec()));
    tree.insert("/warm".into(), Node::File(b"warm\n".to_vec()));
    tree.insert("/other".into(), Node::File(b"other\n".to_vec()));
    tree.insert("/fault".into(), Node::File(content(name)));
    tree
}

/// A warm-up Commit, a mover's Commit on a direct service, then the armed
/// raced Commit of `/fault`, then one follow-up on the same Workspace.
fn run(rig: &Rig, name: &str, target: Option<(u64, Fault)>) -> Run {
    let store = &rig.store;
    let branch = store.fork(name);
    let selection = store.with_lock_timeout(&store.selection_via(&rig.url), 5_000);
    let service = Service::open(store, selection);
    let workspace = service.bind(branch);
    let mut answered = Vec::new();
    write(&workspace, "/warm", b"warm\n");
    match verdict(&commit(&workspace)) {
        Verdict::Committed(id) => answered.push(id),
        other => panic!("{name}: warm-up {other:?}"),
    }
    let mover = store.open();
    let other = mover.bind(branch);
    write(&other, "/other", b"other\n");
    match verdict(&commit(&other)) {
        Verdict::Committed(id) => answered.push(id),
        other => panic!("{name}: mover {other:?}"),
    }
    mover.unmount(other);
    mover.stop();
    write(&workspace, "/fault", &content(name));
    rig.tap.arm();
    rig.proxy.arm(target);
    let first = verdict(&commit(&workspace));
    let window = rig.proxy.disarm();
    let taps = rig.tap.disarm();
    if let Verdict::Committed(id) = &first {
        answered.push(*id);
    }
    let followed = commit(&workspace);
    let follow_in_flight =
        matches!(&followed, Err(failure) if matches!(failure.error, CommitError::InFlight));
    let follow = verdict(&followed);
    if let Verdict::Committed(id) = &follow {
        answered.push(*id);
    }
    let retained_after_follow = workspace.has_retained();
    let reopens = (service.service.opened.session_diagnostics(None))
        .unwrap()
        .unwrap()
        .reopens;
    let _ = service.try_unmount(workspace);
    service.abandon();
    Run {
        window,
        taps,
        first,
        follow,
        follow_in_flight,
        retained_after_follow,
        answered,
        reopens,
    }
}

/// The checks every run must pass; `name`'s Branch is left holding the
/// change (committed by a fresh service when the run did not land it).
fn check(rig: &Rig, name: &str, run: &Run) {
    let store = &rig.store;
    let label = format!(
        "{name}: fired {:?}, first {:?}, follow {:?}",
        run.window.fired.as_ref().map(|f| (f.at, f.kind, f.killed)),
        run.first,
        run.follow
    );
    assert!(!run.follow_in_flight, "{label}: follow-up refused InFlight");
    assert!(
        !run.retained_after_follow,
        "{label}: a Commit is still retained after the follow-up"
    );
    assert!(
        !matches!(run.first, Verdict::PublishedThenFailed(_)),
        "{label}: nothing is published in the fallback's Finish phase"
    );
    let branch = store.branch(name);
    let chain: Vec<CommitId> = store.chain(branch).iter().map(|r| r.id).rev().collect();
    assert_eq!(
        chain, run.answered,
        "{label}: the chain is exactly the answered Commits"
    );
    // The change is not lost to the Store: it lands from a fresh service.
    let landed = run.answered.len() == 3;
    if !landed {
        let service = store.open();
        let workspace = service.bind(branch);
        write(&workspace, "/fault", &content(name));
        let again = verdict(&commit(&workspace));
        assert!(
            matches!(again, Verdict::Committed(_)),
            "{label}: a fresh service could not land the change: {again:?}"
        );
        service.unmount(workspace);
        service.stop();
    }
    assert_eq!(store.tree(branch), expected(name), "{label}: the tree");
}

/// The writer's transactions after the fused message, each as the window
/// positions of its messages, classified by the tables its requests name.
#[derive(Debug, Default)]
struct Fallback {
    tail: Vec<u64>,
    hints: Vec<u64>,
    dump: String,
}

fn fallback(window: &Window, taps: &[Vec<Reply>]) -> Fallback {
    let annotated = annotate(window, taps);
    let kinds: String = window.log.iter().map(|(_, kind)| *kind).collect();
    let refusal = kinds.find('E').expect("the fused attempt's race refusal");
    let end = refusal + kinds[refusal..].find('C').expect("the fused COMMIT") + 1;
    let writer = window.log[refusal].0;
    // The fused message really is the refused one: its first error answers a
    // request of the fused transaction.
    let mut out = Fallback::default();
    let mut current: Vec<u64> = Vec::new();
    let mut tables = String::new();
    let mut in_tx = false;
    for (n, (connection, reply)) in annotated.iter().enumerate().skip(end + 1) {
        if *connection != writer {
            continue;
        }
        out.dump
            .push_str(&format!("{n} {} {}\n", reply.kind, reply.sql));
        let upper = reply.sql.to_ascii_uppercase();
        if !in_tx && upper.starts_with("BEGIN") {
            in_tx = true;
            current.clear();
            tables.clear();
        }
        if !in_tx {
            continue;
        }
        current.push(n as u64);
        tables.push_str(&reply.sql);
        tables.push(' ');
        // The transaction's last request is its `SELECT 1; COMMIT` (a
        // `synchronous_commit` setting in the BEGIN batch is not an end).
        let ends = reply.kind == 'Z'
            && (upper.trim_end().ends_with("COMMIT") || upper.trim_end().ends_with("ROLLBACK"));
        if ends {
            in_tx = false;
            if tables.contains("binding_hint_block") && out.hints.is_empty() {
                out.hints = std::mem::take(&mut current);
            } else if (tables.contains("inode_value_block") || tables.contains("pack"))
                && out.tail.is_empty()
            {
                out.tail = std::mem::take(&mut current);
            }
        }
        if !out.tail.is_empty() && !out.hints.is_empty() {
            break;
        }
    }
    out
}

/// The clean flight (after a prime run): its fallback transactions.
fn clean(rig: &Rig) -> Fallback {
    let prime = run(rig, "prime", None);
    check(rig, "prime", &prime);
    let clean = run(rig, "clean", None);
    assert!(
        matches!(clean.first, Verdict::Committed(_)),
        "{:?}",
        clean.first
    );
    check(rig, "clean", &clean);
    let found = fallback(&clean.window, &clean.taps);
    println!("FALLBACK_WRITER\n{}", found.dump);
    assert!(
        !found.tail.is_empty() && !found.hints.is_empty(),
        "tail and hint transactions not found: {found:?}"
    );
    println!(
        "FALLBACK_TX {{\"tail\":{:?},\"hints\":{:?}}}",
        found.tail, found.hints
    );
    found
}

/// Every message of the selected transaction, faulted once each.
fn sweep(tag: &str, fault: Fault, hints: bool) {
    let Some(rig) = rig(tag) else { return };
    let _slot = verify768::sweep_slot();
    let found = clean(&rig);
    let positions = if hints { &found.hints } else { &found.tail };
    let mut classes = std::collections::BTreeMap::<String, u64>::new();
    for n in positions {
        let name = format!("n{n}");
        let outcome = run(&rig, &name, Some((*n, fault)));
        assert!(outcome.window.fired.is_some(), "{name}: the fault fired");
        check(&rig, &name, &outcome);
        let class = format!("{}/{}", short(&outcome.first), short(&outcome.follow));
        println!(
            "FALLBACK_RUN {{\"tag\":\"{tag}\",\"n\":{n},\"class\":\"{class}\",\"first\":\"{:?}\",\"follow\":\"{:?}\"}}",
            outcome.first, outcome.follow
        );
        *classes.entry(class).or_default() += 1;
    }
    println!(
        "FALLBACK_SUMMARY {{\"tag\":\"{tag}\",\"positions\":{},\"classes\":{classes:?}}}",
        positions.len()
    );
    final_checks(&rig);
}

fn short(verdict: &Verdict) -> &'static str {
    match verdict {
        Verdict::Committed(_) => "committed",
        Verdict::UpToDate => "up_to_date",
        Verdict::Conflicted => "conflicted",
        Verdict::Refused(_) => "refused",
        Verdict::PublishedThenFailed(_) => "published_then_failed",
        Verdict::Unknown(_) => "unknown",
    }
}

fn final_checks(rig: &Rig) {
    let store = &rig.store;
    assert!(
        store.no_open_transaction(),
        "a backend left in a transaction"
    );
    let failures = store.integrity_failures();
    assert!(failures.is_empty(), "audit: {failures:#?}");
    let amcheck = verify768::amcheck(store);
    assert!(amcheck.is_empty(), "amcheck: {amcheck:#?}");
    let duplicates = verify768::duplicate_allocations(store);
    assert!(duplicates.is_empty(), "allocations: {duplicates:#?}");
    // Every Commit row is on some Branch's chain.
    let rows: i64 = store
        .admin()
        .query_one(
            "SELECT count(*) FROM \"commit\" c WHERE NOT EXISTS (\
             WITH RECURSIVE line(id) AS (\
               SELECT head_commit_id FROM branch WHERE head_commit_id IS NOT NULL \
               UNION SELECT p.parent_commit_id FROM \"commit\" p JOIN line ON p.commit_id = line.id \
               WHERE p.parent_commit_id IS NOT NULL) \
             SELECT 1 FROM line WHERE line.id = c.commit_id)",
            &[],
        )
        .unwrap()
        .get(0);
    assert_eq!(rows, 0, "Commit rows on no Branch's first-parent chain");
}

#[test]
fn tail_publication_lost_reply_keeps_history_exact() {
    adversary::tmp();
    sweep("v768x_tail_drop", Fault::Drop, false);
}

#[test]
fn tail_publication_backend_killed_keeps_history_exact() {
    adversary::tmp();
    sweep("v768x_tail_kill", Fault::HoldKill, false);
}

#[test]
fn hint_rows_lost_reply_keeps_history_exact() {
    adversary::tmp();
    sweep("v768x_hints_drop", Fault::Drop, true);
}

#[test]
fn hint_rows_backend_killed_keeps_history_exact() {
    adversary::tmp();
    sweep("v768x_hints_kill", Fault::HoldKill, true);
}

/// After a lost reply in the fallback's tail publication (its first message)
/// and in its hint write (its first message): the same Workspace's next
/// Commit reopens the quarantined writer once and lands the change, exactly
/// once (`check`: the chain is exactly the answered Commits, no Commit
/// retained, never `InFlight`). Before the reopen existed it was refused
/// for the service's life.
#[test]
fn the_same_service_reopens_after_a_lost_fallback_reply() {
    adversary::tmp();
    let Some(rig) = rig("v768x_same_ws") else {
        return;
    };
    let found = clean(&rig);
    let mut outcomes = Vec::new();
    for (what, n) in [("tail", found.tail[0]), ("hints", found.hints[0])] {
        let name = format!("same_{what}");
        let outcome = run(&rig, &name, Some((n, Fault::Drop)));
        check(&rig, &name, &outcome);
        outcomes.push((
            what,
            n,
            outcome.first.clone(),
            outcome.follow.clone(),
            outcome.reopens,
        ));
    }
    for (what, n, first, follow, reopens) in &outcomes {
        assert!(
            matches!(first, Verdict::Unknown(_) | Verdict::Refused(_)),
            "{what} (message {n}): a lost fallback reply does not answer Committed: {first:?}"
        );
        assert!(
            matches!(follow, Verdict::Committed(_)),
            "{what} (message {n}): first {first:?}, the same service's next Commit lands the change: {follow:?}"
        );
        assert_eq!(*reopens, 1, "{what} (message {n}): one reopen");
    }
}
