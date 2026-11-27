//! Independent #768 check (not the author's tests): a Commit refused `Busy`
//! keeps its handle and block, on SQLite (an external process holds the
//! write lock) and on PostgreSQL (another session holds the lane row past
//! the service's 300 ms lock timeout).
//!
//! * `repeated_busy_then_commit_uses_the_kept_block`: a fresh service's
//!   first Commit (a new handle) is refused Busy three times in a row, the
//!   last with 300 new files. No Busy moves the Store's ordinal counter (no
//!   release, no second reservation). The Commit after them lands, reads no
//!   policy and reserves nothing when a block was kept, and its values lie
//!   below the counter the Busy rounds left (they came from the kept
//!   block). A second Workspace then commits (its own new handle), both
//!   unmount, the service closes: no id is reused, the trees read back.
//! * `busy_handle_handed_on_at_unmount`: a Busy-kept handle that is never
//!   used again goes to the Store's idle slot at unmount; a new Workspace
//!   takes it and commits from its block; the close returns the rest. With
//!   one handle in play nothing is lost: the counter ends right after the
//!   last stored value.
//!
//! The PostgreSQL halves need `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "../../layerfs-daemon/tests/support/held_writer.rs"]
mod held_writer;
#[path = "support/verify768.rs"]
#[allow(dead_code)]
mod verify768;
#[path = "support/verify768x.rs"]
#[allow(dead_code)]
mod verify768x;
#[path = "support/verify768y.rs"]
#[allow(dead_code)]
mod verify768y;

use adversary::{
    apply, commit, seed_tree, verdict, write, Kind, Node, Op, Service, TestStore, Tree, Verdict,
};
use layerfs_persistence::StoreSelection;
use layerfs_store_service::store::{BoundWorkspace, CommitFailure, CommitSuccess};
use verify768x::{alloc, value_blocks};
use verify768y::Lost;

const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n")];

fn close(service: Service) {
    service.service.close().expect("an orderly close");
    service.owner.stop().expect("the overlay owner stops");
}

/// A Commit while the write lock (SQLite) or the lane row (PostgreSQL) is
/// held by another party.
fn held_commit(
    store: &TestStore,
    workspace: &BoundWorkspace,
) -> Result<CommitSuccess, Box<CommitFailure>> {
    match store.kind {
        Kind::Sqlite => {
            let StoreSelection::Sqlite(config) = &store.selection else {
                unreachable!()
            };
            let held = held_writer::HeldWriter::acquire(&config.path);
            let answer = commit(workspace);
            held.release();
            answer
        }
        Kind::Postgres => {
            let mut blocker = store.admin();
            blocker
                .batch_execute("BEGIN; SELECT 1 FROM store_lane FOR UPDATE")
                .unwrap();
            let answer = commit(workspace);
            blocker.batch_execute("ROLLBACK").unwrap();
            answer
        }
    }
}

fn files(workspace: &BoundWorkspace, tree: &mut Tree, dir: &str, n: usize) {
    apply(workspace, &Op::Mkdir(dir.to_owned())).unwrap();
    tree.insert(dir.to_owned(), Node::Dir);
    for k in 0..n {
        let path = format!("{dir}/f{k}");
        let bytes = format!("{dir} {k}\n").into_bytes();
        write(workspace, &path, &bytes);
        tree.insert(path, Node::File(bytes));
    }
}

fn stores(label: &str) -> Vec<TestStore> {
    TestStore::both(label, SEED)
}

fn open(store: &TestStore) -> Service {
    Service::open(store, store.with_lock_timeout(&store.selection, 300))
}

#[test]
fn repeated_busy_then_commit_uses_the_kept_block() {
    busy_rounds("y-busy-rep", &[3, 5, 300]);
}

/// Small refused changes only: the kept block outlasts them, and the
/// Commit that lands uses its rest (no reservation at all), skipping only
/// the ordinals the refused attempts assigned.
#[test]
fn small_busy_rounds_leave_the_kept_rest_used() {
    busy_rounds("y-busy-small", &[3, 5]);
}

fn busy_rounds(label: &str, sizes: &[usize]) {
    adversary::tmp();
    let large = sizes.iter().any(|n| *n > 128);
    for store in stores(label) {
        let kind = store.kind;
        let mut lost = Lost::default();
        lost.closed_point(&store, "init");
        let branch = store.fork("busy");
        let mut tree = seed_tree(SEED);
        let service = open(&store);
        let workspace = service.bind(branch);
        // A warm-up Commit: the handle keeps up to KEPT_ORDINALS ordinals.
        files(&workspace, &mut tree, "/warm", 1);
        assert!(matches!(
            verdict(&commit(&workspace)),
            Verdict::Committed(_)
        ));
        let before = alloc(&store);
        assert!(
            before.next_ordinal > before.last_ordinal + 1,
            "{kind:?}: the warm handle keeps ordinals: {before:?}"
        );
        let mut counters = Vec::new();
        for (round, n) in sizes.iter().copied().enumerate() {
            files(&workspace, &mut tree, &format!("/busy{round}"), n);
            let answer = verdict(&held_commit(&store, &workspace));
            assert!(
                matches!(&answer, Verdict::Refused(why) if why.contains("Busy")),
                "{kind:?} round {round}: {answer:?}"
            );
            counters.push(alloc(&store).next_ordinal);
            lost.check(&store, &format!("{kind:?} busy round {round}"));
        }
        // No release (never a rewind) and at most one new reservation
        // across the three Busy refusals.
        let mut all = vec![before.next_ordinal];
        all.extend(&counters);
        assert!(
            all.windows(2).all(|w| w[0] <= w[1])
                && all.windows(2).filter(|w| w[0] != w[1]).count() <= 1,
            "{kind:?}: Busy refusals moved the counter: {all:?}"
        );
        let after_busy = *counters.last().unwrap();
        let result = commit(&workspace);
        let answer = verdict(&result);
        assert!(
            matches!(answer, Verdict::Committed(_)),
            "{kind:?}: {answer:?}"
        );
        let landed = result.unwrap();
        assert_eq!(landed.storage.policy, 0, "{kind:?}: the kept handle");
        let new: Vec<_> = value_blocks(&store)
            .into_iter()
            .filter(|b| b.1 > before.last_ordinal)
            .collect();
        let first_new = new.iter().map(|b| b.0).min().unwrap();
        assert!(
            first_new > before.last_ordinal,
            "{kind:?}: {new:?} after {before:?}"
        );
        let kept_block = after_busy > before.next_ordinal;
        if kept_block {
            assert_eq!(
                landed.storage.initial_reservations, 0,
                "{kind:?}: the block reserved during the Busy rounds served the Commit"
            );
        }
        let skipped = first_new - before.last_ordinal - 1;
        if !large {
            // Each refused attempt assigned its capture's values (files,
            // their directories, the root): at most 2 * files + 4 each.
            let bound: i64 = sizes
                .iter()
                .scan(0, |sum, n| {
                    *sum += n;
                    Some(*sum as i64 * 2 + 4)
                })
                .sum();
            assert!(
                skipped <= bound,
                "{kind:?}: {skipped} kept ordinals skipped, more than the refused attempts assigned ({bound})"
            );
            assert_eq!(
                (
                    landed.storage.initial_reservations,
                    landed.storage.reservation_refills
                ),
                (0, 0),
                "{kind:?}: the kept rest served the Commit"
            );
            assert_eq!(
                alloc(&store).next_ordinal,
                before.next_ordinal,
                "{kind:?}: no reservation, the counter stays"
            );
        }
        println!(
            "Y_BUSY {{\"kind\":\"{kind:?}\",\"warm\":\"{before:?}\",\"counters\":\"{counters:?}\",\"skipped_before_first_new\":{skipped},\"landed_reservations\":{},\"landed_refills\":{},\"landed_policy\":{},\"after_landed\":\"{:?}\"}}",
            landed.storage.initial_reservations,
            landed.storage.reservation_refills,
            landed.storage.policy,
            alloc(&store)
        );
        // A second Workspace with its own new handle.
        let other_branch = store.fork("other");
        let mut other_tree = seed_tree(SEED);
        let other = service.bind(other_branch);
        files(&other, &mut other_tree, "/other", 20);
        assert!(matches!(verdict(&commit(&other)), Verdict::Committed(_)));
        files(&workspace, &mut tree, "/again", 4);
        assert!(matches!(
            verdict(&commit(&workspace)),
            Verdict::Committed(_)
        ));
        lost.check(&store, &format!("{kind:?} after the follow-ups"));
        service.unmount(workspace);
        service.unmount(other);
        close(service);
        let (lost_ord, _) = lost.closed_point(&store, &format!("{kind:?} closed"));
        assert_eq!(store.tree(branch), tree, "{kind:?}");
        assert_eq!(store.tree(other_branch), other_tree, "{kind:?}");
        lost.closed_point(&store, &format!("{kind:?} after read-back"));
        println!(
            "Y_BUSY_END {{\"kind\":\"{kind:?}\",\"lost_ordinals\":{lost_ord},\"final\":\"{:?}\"}}",
            alloc(&store)
        );
        let audit = store.integrity_failures();
        assert!(audit.is_empty(), "{kind:?}: {audit:#?}");
    }
}

#[test]
fn busy_handle_handed_on_at_unmount() {
    adversary::tmp();
    for store in stores("y-busy-hand") {
        let kind = store.kind;
        let mut lost = Lost::default();
        lost.closed_point(&store, "init");
        let first_branch = store.fork("first");
        let second_branch = store.fork("second");
        let service = open(&store);
        let first = service.bind(first_branch);
        let mut first_tree = seed_tree(SEED);
        files(&first, &mut Tree::new(), "/lost", 6);
        let before = alloc(&store);
        let answer = verdict(&held_commit(&store, &first));
        assert!(
            matches!(&answer, Verdict::Refused(why) if why.contains("Busy")),
            "{kind:?}: {answer:?}"
        );
        let after_busy = alloc(&store);
        // Unmounted without another Commit: the change is discarded with
        // the Workspace, the handle goes to the Store's idle slot.
        service.unmount(first);
        assert_eq!(
            alloc(&store).next_ordinal,
            after_busy.next_ordinal,
            "{kind:?}: the unmount found the slot free: no release"
        );
        let second = service.bind(second_branch);
        let mut second_tree = seed_tree(SEED);
        files(&second, &mut second_tree, "/second", 40);
        let result = commit(&second);
        let answer = verdict(&result);
        assert!(
            matches!(answer, Verdict::Committed(_)),
            "{kind:?}: {answer:?}"
        );
        let landed = result.unwrap();
        if after_busy.next_ordinal > before.next_ordinal {
            assert_eq!(
                landed.storage.initial_reservations, 0,
                "{kind:?}: the slot's Busy-kept block served the Commit"
            );
        }
        service.unmount(second);
        close(service);
        let end = alloc(&store);
        println!(
            "Y_BUSY_HAND {{\"kind\":\"{kind:?}\",\"before\":{},\"after_busy\":{},\"landed_reservations\":{},\"end\":\"{end:?}\"}}",
            before.next_ordinal, after_busy.next_ordinal, landed.storage.initial_reservations
        );
        assert_eq!(
            end.next_ordinal,
            end.last_ordinal + 1,
            "{kind:?}: one handle in play, its rest returned at close"
        );
        lost.closed_point(&store, &format!("{kind:?} closed"));
        // The first Branch never received its change.
        first_tree.remove("/lost");
        assert_eq!(store.tree(first_branch), first_tree, "{kind:?}");
        assert_eq!(store.tree(second_branch), second_tree, "{kind:?}");
        lost.closed_point(&store, &format!("{kind:?} after read-back"));
    }
}
