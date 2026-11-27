//! #768 independent verification: a fused Workspace Commit on PostgreSQL
//! aborted by the server at each table its one message writes.
//!
//! A one-shot trigger (`support/verify768.rs`) raises a chosen SQLSTATE the
//! first time the fused transaction writes a row of one table: the pack
//! bodies, the publication's rows (locators, inode values, signatures,
//! totals, released ordinals), the hint blocks, the history serial, the
//! Commit row, its lineage or the Branch advance. The Commit carries every
//! kind of change at once (a new chunked file, a rewrite of a large seeded
//! file, a new directory of small files, a symlink, a rename, a deletion).
//!
//! * `40001` and `P0001` are definite refusals (`Busy`, `Integrity`):
//!   nothing written (raw counts, totals and every Branch head unchanged),
//!   no fallback, no backend left in a transaction; the same Workspace's
//!   next Commit lands with the same tree.
//! * `22012` is how the message reports a race at the publication rows, the
//!   Commit, the lineage and the advance: the call continues on the
//!   separate path once and lands (counts `(0, 1)`); at the pack bodies and
//!   the hints it is a definite refusal.
//!
//! After either, a second round reuses the aborted attempt's bytes at new
//! paths (so the Save dedups against objects the attempt never published)
//! and Commits again. A fresh service reads every byte back, and the Store
//! passes its own audit, amcheck and the allocation checks. Needs
//! `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
use crate::adversary;
use crate::verify768;

use adversary::{
    apply, assert_linear, commit, random_bytes, text_bytes, tree_of, verdict, Op, TestStore,
    Verdict,
};
use verify768::{Step, Target, TARGETS};

fn seed_doc() -> Vec<u8> {
    text_bytes(7680, 300 * 1024)
}

fn seed() -> Vec<(String, Vec<u8>)> {
    let mut seed = vec![
        ("/seed".to_owned(), b"seed\n".to_vec()),
        ("/doc.txt".to_owned(), seed_doc()),
        ("/gone".to_owned(), b"to be deleted\n".to_vec()),
    ];
    for n in 0..5 {
        seed.push((format!("/dir/f{n}"), format!("dir file {n}\n").into_bytes()));
    }
    seed
}

/// The change set of case `k`: every kind of change, unique bytes per case.
fn changes(k: u64) -> Vec<Op> {
    let mut doc = seed_doc();
    let at = 100_000 + k as usize * 13;
    doc[at..at + 64].copy_from_slice(&text_bytes(k + 1, 64));
    let mut ops = vec![
        Op::Write(
            format!("/big-{k}"),
            random_bytes(k + 100, 200 * 1024 + k as usize),
        ),
        Op::Write("/doc.txt".into(), doc),
        Op::Mkdir(format!("/many-{k}")),
    ];
    for n in 0..40 {
        ops.push(Op::Write(
            format!("/many-{k}/f{n}"),
            format!("case {k} small {n}\n").into_bytes(),
        ));
    }
    ops.push(Op::Symlink(format!("/link-{k}"), b"doc.txt".to_vec()));
    ops.push(Op::Rename("/dir/f0".into(), format!("/dir/renamed-{k}")));
    ops.push(Op::Unlink("/gone".into()));
    ops
}

/// The second round: the aborted attempt's bytes again at new paths, and
/// more changes.
fn second(k: u64) -> Vec<Op> {
    vec![
        Op::Write(
            format!("/copy-{k}"),
            random_bytes(k + 100, 200 * 1024 + k as usize),
        ),
        Op::Write(
            format!("/many-{k}/again"),
            format!("case {k} small 7\n").into_bytes(),
        ),
        Op::Append("/doc.txt".into(), text_bytes(k + 9, 5000)),
        Op::Unlink(format!("/many-{k}/f1")),
    ]
}

fn run(ops: &[Op], workspace: &layerfs_store_service::store::BoundWorkspace) {
    for op in ops {
        apply(workspace, op).unwrap_or_else(|error| panic!("{op:?}: {error}"));
    }
}

/// One case on its own Branch: returns a one-line record.
/// `cold`: the faulted Commit is the first of a newly opened service (its
/// Save reserves a fresh id block and releases the ordinal tail past the
/// kept ones in the same message: `store_policy` is written); otherwise a
/// warm-up Commit comes first and the faulted one uses the kept block.
fn case(store: &TestStore, k: u64, target: Target, code: &str, cold: bool) -> String {
    let name = format!("c{k}");
    let branch = store.fork(&name);
    let service = store.open();
    let workspace = service.bind(branch);
    let warm = if cold {
        None
    } else {
        apply(
            &workspace,
            &Op::Write("/warm".into(), format!("warm {k}\n").into_bytes()),
        )
        .unwrap();
        match verdict(&commit(&workspace)) {
            Verdict::Committed(id) => Some(id),
            other => panic!("{name}: warm-up {other:?}"),
        }
    };
    let commits_before = usize::from(!cold);
    run(&changes(k), &workspace);
    let intended = tree_of(&workspace);
    let before = verify768::written(store);
    let counts = service.service.store.fused_counts();
    verify768::arm(store, target, code);
    let first = verdict(&commit(&workspace));
    let fired = verify768::fired(store);
    verify768::disarm(store, target);
    let after_first = verify768::written(store);
    let fused = service.service.store.fused_counts();
    let counts = (fused.0 - counts.0, fused.1 - counts.1);
    let label = format!(
        "{name} {}/{} {code} cold={cold}: first {first:?} counts {counts:?}",
        target.table, target.event
    );
    assert!(
        store.no_open_transaction(),
        "{label}: a backend left in a transaction"
    );
    if !fired {
        // The Commit never wrote this table in its fused transaction.
        assert!(
            matches!(first, Verdict::Committed(_)),
            "{label}: not fired yet not committed"
        );
        service.unmount(workspace);
        service.stop();
        return format!("{label}: NOT_EXERCISED (table not written by this Commit)");
    }
    let race = code == "22012"
        && matches!(
            target.step,
            Step::Rows | Step::Commit | Step::Lineage | Step::Advance
        );
    let landed = if race {
        // A reported race continues on the separate path once and lands.
        assert_eq!(counts, (0, 1), "{label}: one fallback");
        match first {
            Verdict::Committed(id) => id,
            other => panic!("{label}: a fallback over an unmoved head answered {other:?}"),
        }
    } else {
        match &first {
            Verdict::Refused(_) => {}
            other => panic!("{label}: a definite server refusal answered {other:?}"),
        }
        assert_eq!(counts, (0, 0), "{label}: no fused Commit and no fallback");
        assert_eq!(
            after_first, before,
            "{label}: nothing written by the aborted message"
        );
        let chain = store.chain(branch);
        assert_eq!(chain.first().map(|r| r.id), warm, "{label}: head unmoved");
        // The same Workspace commits the same tree next.
        match verdict(&commit(&workspace)) {
            Verdict::Committed(id) => id,
            other => panic!("{label}: the next Commit after a definite refusal answered {other:?}"),
        }
    };
    let chain = store.chain(branch);
    assert_linear(&chain);
    assert_eq!(
        chain.first().map(|r| r.id),
        Some(landed),
        "{label}: the landed head"
    );
    assert_eq!(
        chain.get(1).map(|r| r.id),
        warm,
        "{label}: one Commit past the warm-up"
    );
    assert_eq!(
        chain.len(),
        commits_before + 1,
        "{label}: exactly one Commit landed"
    );
    assert_eq!(store.tree(branch), intended, "{label}: the landed tree");
    // Round two: dedup against the attempt's objects, then read back.
    run(&second(k), &workspace);
    let intended = tree_of(&workspace);
    let round = verdict(&commit(&workspace));
    let Verdict::Committed(id) = round else {
        panic!("{label}: round two answered {round:?}");
    };
    service.unmount(workspace);
    service.stop();
    let chain = store.chain(branch);
    assert_linear(&chain);
    assert_eq!(
        chain.first().map(|r| r.id),
        Some(id),
        "{label}: round two head"
    );
    assert_eq!(
        chain.len(),
        commits_before + 2,
        "{label}: warm-up, change, round two"
    );
    assert_eq!(store.tree(branch), intended, "{label}: round two tree");
    label
}

fn matrix(tag: &str, code: &str, cold: bool) {
    let seed = seed();
    let seed: Vec<(&str, &[u8])> = seed
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_slice()))
        .collect();
    let Some(store) = TestStore::postgres(tag, &seed) else {
        return;
    };
    let mut lines = Vec::new();
    for (n, target) in TARGETS.iter().enumerate() {
        lines.push(case(&store, n as u64 + 1, *target, code, cold));
    }
    for line in &lines {
        println!("VERIFY_INJECTED {line}");
    }
    let failures = store.integrity_failures();
    assert!(failures.is_empty(), "audit: {failures:#?}");
    let failures = verify768::amcheck(&store);
    assert!(failures.is_empty(), "amcheck: {failures:#?}");
    let failures = verify768::duplicate_allocations(&store);
    assert!(failures.is_empty(), "allocation: {failures:#?}");
}

#[test]
fn a_fused_commit_refused_busy_at_each_table() {
    matrix("v768_busy", "40001", false);
}

#[test]
fn a_fused_commit_refused_by_a_raise_at_each_table() {
    matrix("v768_raise", "P0001", false);
}

#[test]
fn a_fused_commit_reported_racing_at_each_table() {
    matrix("v768_race", "22012", false);
}

#[test]
fn a_first_commit_refused_busy_at_each_table() {
    matrix("v768_cbusy", "40001", true);
}

#[test]
fn a_first_commit_refused_by_a_raise_at_each_table() {
    matrix("v768_craise", "P0001", true);
}

#[test]
fn a_first_commit_reported_racing_at_each_table() {
    matrix("v768_crace", "22012", true);
}
