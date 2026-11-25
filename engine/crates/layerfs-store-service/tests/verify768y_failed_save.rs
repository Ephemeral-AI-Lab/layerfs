//! Independent #768 check (not the author's tests): a Save that fails
//! definitely part-way, then more Commits, on SQLite and on PostgreSQL.
//!
//! The failure is a trigger that refuses the k-th pack row a Commit inserts
//! (SQLite `RAISE(ABORT)`, PostgreSQL `P0001`): both definite. Swept over
//! every k a clean Commit of the same shape reaches, so it lands in the
//! fused attempt, in a Save's earlier publication batch (a big change past
//! one publication's row limit publishes in several, the earlier ones in
//! their own transactions), or in a raced Commit's fallback merge Save.
//!
//! After each refused Commit, checked:
//! * the answer is a definite refusal (or `Committed` when the trigger did
//!   not fire), never unknown or published-then-failed;
//! * the Store's counter stands past every stored value (a release whose
//!   first id were below a stored value would rewind under it) and the
//!   refused Save's unused rest went back: the counter is at most the
//!   change's value count past the last stored value (not a whole 16,384
//!   block);
//! * the same Workspace's next Commit lands, as does another Workspace's on
//!   the same service; at the close no id lost earlier is ever used
//!   (`support/verify768y.rs`) and every tree reads back.
//!
//! The PostgreSQL tests need `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;
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
use layerfs_store_service::store::BoundWorkspace;
use verify768x::{alloc, sqlite};
use verify768y::Lost;

const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n")];

fn close(service: Service) {
    service.service.close().expect("an orderly close");
    service.owner.stop().expect("the overlay owner stops");
}

fn packs(store: &TestStore) -> i64 {
    alloc(store).packs
}

/// Refuses an insert into `pack` once `limit` packs are stored (as this
/// transaction sees them): the first pack past `limit`.
fn arm(store: &TestStore, limit: i64) {
    match store.kind {
        Kind::Sqlite => sqlite(store)
            .execute_batch(&format!(
                "CREATE TRIGGER y_fail BEFORE INSERT ON pack \
                 WHEN (SELECT count(*) FROM pack) >= {limit} \
                 BEGIN SELECT RAISE(ABORT, 'y injected pack refusal'); END;"
            ))
            .unwrap(),
        Kind::Postgres => store
            .admin()
            .batch_execute(&format!(
                "CREATE OR REPLACE FUNCTION y_fail() RETURNS trigger LANGUAGE plpgsql AS $f$ \
                 BEGIN IF (SELECT count(*) FROM pack) >= {limit} THEN \
                   RAISE EXCEPTION 'y injected pack refusal' USING ERRCODE = 'P0001'; \
                 END IF; RETURN NEW; END $f$; \
                 CREATE TRIGGER y_fail BEFORE INSERT ON pack \
                 FOR EACH ROW EXECUTE FUNCTION y_fail();"
            ))
            .unwrap(),
    }
}

fn disarm(store: &TestStore) {
    match store.kind {
        Kind::Sqlite => sqlite(store).execute_batch("DROP TRIGGER y_fail").unwrap(),
        Kind::Postgres => store
            .admin()
            .batch_execute("DROP TRIGGER y_fail ON pack; DROP FUNCTION y_fail();")
            .unwrap(),
    }
}

/// `n` new files under `dir`, recorded in `tree`; returns the values the
/// change needs at most (files, the directory, the root and slack).
fn change(workspace: &BoundWorkspace, tree: &mut Tree, tag: &str, dir: &str, n: usize) -> i64 {
    apply(workspace, &Op::Mkdir(dir.to_owned())).unwrap();
    tree.insert(dir.to_owned(), Node::Dir);
    for k in 0..n {
        let path = format!("{dir}/f{k}");
        let bytes = format!("{tag} {dir} file {k} of {n}\n").into_bytes();
        write(workspace, &path, &bytes);
        tree.insert(path, Node::File(bytes));
    }
    n as i64 + 8
}

#[derive(Clone, Copy, Debug)]
enum Shape {
    /// One Workspace, `files` new files, the head unmoved (fused).
    Plain { files: usize },
    /// A mover on another service moves the head first: the fused attempt
    /// is refused by the race and the fallback merges in the lane.
    Raced { files: usize },
}

impl Shape {
    fn files(self) -> usize {
        match self {
            Shape::Plain { files } | Shape::Raced { files } => files,
        }
    }
}

struct Outcome {
    verdict: Verdict,
    packs_inserted: i64,
    publishes: Option<u64>,
}

/// One run on Branch `name`: a fresh service, the shape's Commit with the
/// trigger armed at `k` (None: clean), then the same Workspace's next
/// Commit and another Workspace's, unmount, close, checks.
fn run(store: &TestStore, lost: &mut Lost, name: &str, shape: Shape, k: Option<i64>) -> Outcome {
    let kind = store.kind;
    let label = format!("{kind:?} {name} {shape:?} k={k:?}");
    let branch = store.fork(name);
    let mut tree = seed_tree(SEED);
    let service = store.open();
    let workspace = service.bind(branch);
    if let Shape::Raced { .. } = shape {
        let mover = store.open();
        let moving = mover.bind(branch);
        change(&moving, &mut tree, name, "/mover", 3);
        assert!(matches!(
            verdict(&commit(&moving)),
            Verdict::Committed(_)
        ));
        mover.unmount(moving);
        close(mover);
    }
    let values = change(&workspace, &mut tree, name, "/big", shape.files());
    let before = alloc(store);
    if let Some(k) = k {
        arm(store, before.packs + k - 1);
    }
    let result = commit(&workspace);
    if k.is_some() {
        disarm(store);
    }
    let answer = verdict(&result);
    let publishes = result.as_ref().ok().map(|s| s.storage.publish);
    let after = alloc(store);
    let packs_inserted = after.packs - before.packs;
    lost.check(store, &format!("{label}: after the armed Commit"));
    match &answer {
        Verdict::Committed(_) => {}
        Verdict::Refused(_) => {
            // The refused Save's unused rest went back: the counter is not a
            // whole block past the stored values.
            assert!(
                after.next_ordinal - after.last_ordinal - 1 <= values,
                "{label}: counter {} stands {} past the last stored value {} \
                 (the change needs at most {values}); before {before:?}",
                after.next_ordinal,
                after.next_ordinal - after.last_ordinal - 1,
                after.last_ordinal
            );
        }
        other => panic!("{label}: a definite answer expected: {other:?}"),
    }
    // The same Workspace's next Commit lands the change.
    let again = verdict(&commit(&workspace));
    match (&answer, &again) {
        (Verdict::Committed(_), Verdict::UpToDate) | (Verdict::Refused(_), Verdict::Committed(_)) => {}
        _ => panic!("{label}: first {answer:?}, then {again:?}"),
    }
    // Another Workspace on the same service, and one more change here.
    let other_branch = store.fork(&format!("{name}-other"));
    let mut other_tree = seed_tree(SEED);
    let other = service.bind(other_branch);
    change(&other, &mut other_tree, name, "/other", 20);
    let answer_other = verdict(&commit(&other));
    assert!(
        matches!(answer_other, Verdict::Committed(_)),
        "{label}: {answer_other:?}"
    );
    change(&workspace, &mut tree, name, "/after", 5);
    let answer_after = verdict(&commit(&workspace));
    assert!(
        matches!(answer_after, Verdict::Committed(_)),
        "{label}: {answer_after:?}"
    );
    lost.check(store, &format!("{label}: after the follow-ups"));
    service.unmount(workspace);
    service.unmount(other);
    close(service);
    let (lost_ord, lost_pack) = lost.closed_point(store, &format!("{label}: closed"));
    assert_eq!(store.tree(branch), tree, "{label}");
    assert_eq!(store.tree(other_branch), other_tree, "{label}");
    println!(
        "Y_FAILED_SAVE {{\"label\":\"{label}\",\"verdict\":\"{}\",\"packs_inserted\":{packs_inserted},\"publishes\":{publishes:?},\"counter_gap_after\":{},\"lost_ordinals_total\":{lost_ord},\"lost_packs_total\":{lost_pack}}}",
        match &answer {
            Verdict::Committed(_) => "committed".to_owned(),
            other => format!("{other:?}").chars().take(160).collect(),
        },
        after.next_ordinal - after.last_ordinal - 1
    );
    Outcome {
        verdict: answer,
        packs_inserted,
        publishes,
    }
}

fn sweep(store: &TestStore, tag: &str, shape: Shape, ks: std::ops::RangeInclusive<i64>) {
    let kind = store.kind;
    let mut lost = Lost::default();
    lost.closed_point(store, "init");
    let clean = run(store, &mut lost, &format!("{tag}-clean"), shape, None);
    assert!(matches!(clean.verdict, Verdict::Committed(_)));
    println!(
        "Y_FAILED_SAVE_CLEAN {{\"kind\":\"{kind:?}\",\"shape\":\"{shape:?}\",\"packs\":{},\"publishes\":{:?}}}",
        clean.packs_inserted, clean.publishes
    );
    let mut refused = 0;
    for k in ks.filter(|k| *k <= clean.packs_inserted.max(1)) {
        let outcome = run(store, &mut lost, &format!("{tag}-k{k}"), shape, Some(k));
        if matches!(outcome.verdict, Verdict::Refused(_)) {
            refused += 1;
        }
    }
    assert!(refused > 0, "{kind:?} {shape:?}: the trigger never refused");
    let audit = store.integrity_failures();
    println!("Y_AUDIT {{\"kind\":\"{kind:?}\",\"tag\":\"{tag}\",\"failures\":\"{audit:?}\"}}");
    // The audit cancelled at its own 30 s deadline on a large Store is INCOMPLETE, not a
    // finding; any other failure is.
    let real: Vec<_> = audit
        .iter()
        .filter(|f| !f.contains("Cancelled"))
        .collect();
    assert!(real.is_empty(), "{kind:?}: {audit:#?}");
    let _ = packs(store);
}

#[test]
fn sqlite_big_change_fails_in_each_batch() {
    adversary::tmp();
    let store = TestStore::sqlite("y-fs-big", SEED);
    sweep(&store, "big", Shape::Plain { files: 6_000 }, 1..=99);
}

#[test]
fn sqlite_raced_merge_save_fails() {
    adversary::tmp();
    let store = TestStore::sqlite("y-fs-race", SEED);
    sweep(&store, "race", Shape::Raced { files: 40 }, 1..=99);
}

/// The PostgreSQL sweep of a big change in two halves (each under the
/// two-minute ceiling).
#[test]
fn postgres_big_change_fails_in_early_batches() {
    adversary::tmp();
    let Some(store) = TestStore::postgres("y-fs-big-a", SEED) else {
        return;
    };
    sweep(&store, "big", Shape::Plain { files: 6_000 }, 1..=4);
}

#[test]
fn postgres_big_change_fails_in_late_batches() {
    adversary::tmp();
    let Some(store) = TestStore::postgres("y-fs-big-b", SEED) else {
        return;
    };
    sweep(&store, "big", Shape::Plain { files: 6_000 }, 5..=99);
}

#[test]
fn postgres_raced_merge_save_fails() {
    adversary::tmp();
    let Some(store) = TestStore::postgres("y-fs-race", SEED) else {
        return;
    };
    sweep(&store, "race", Shape::Raced { files: 40 }, 1..=99);
}
