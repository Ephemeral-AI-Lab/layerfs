//! #768 independent verification, finding 5: a Workspace Commit whose new
//! tree adds more name bindings than one hint write transaction carries
//! (4,096 rows) must never write a hint row before the objects of the tree
//! it describes are stored. The Commit over an unmoved head would be fused
//! (its Save's last publication held for the Commit's own transaction); an
//! injected abort refuses the Commit row definitely. Afterwards, if any
//! hint block was written, the attempt's candidate root is stored; and the
//! same Workspace's next Commit lands with the intended tree.
//!
//! On SQLite a trigger aborts the `commit` insert of any transaction; on
//! PostgreSQL a one-shot trigger (`support/verify768.rs`) aborts it inside
//! a fused transaction only (`40001`, a definite `Busy`). The PostgreSQL
//! case needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
use crate::adversary;
#[path = "../support/verify/core.rs"]
#[allow(dead_code)]
mod verify768;

use adversary::{apply, commit, tree_of, verdict, Op, TestStore, Verdict};
use layerfs_persistence::StoreSelection;
use layerfs_store_service::store::{CommitFailure, CommitSuccess};

/// New files in one new directory: each is a new binding, past one hint
/// write transaction's 4,096 rows.
const FILES: usize = 4_200;

const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n")];

fn spill(k: usize) -> Vec<Op> {
    let mut ops = vec![Op::Mkdir(format!("/many-{k}"))];
    for n in 0..FILES {
        ops.push(Op::Write(
            format!("/many-{k}/f{n}"),
            format!("spill {k} file {n}\n").into_bytes(),
        ));
    }
    ops
}

fn sqlite_connection(store: &TestStore) -> rusqlite::Connection {
    let StoreSelection::Sqlite(config) = &store.selection else {
        unreachable!()
    };
    let connection = rusqlite::Connection::open(&config.path).unwrap();
    connection
        .busy_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    connection
}

/// Hint blocks stored, and whether `root` is stored.
type Probe<'a> = dyn Fn(Option<&[u8]>) -> (i64, bool) + 'a;

/// The attempt's candidate root, as its request named it.
fn candidate(result: &Result<CommitSuccess, Box<CommitFailure>>) -> Option<Vec<u8>> {
    match result {
        Ok(success) => match &success.history {
            Some(layerfs_history::CommitOutcome::Committed { record, .. }) => {
                Some(record.root.as_bytes().to_vec())
            }
            _ => None,
        },
        Err(failure) => failure
            .intent
            .as_ref()
            .map(|intent| intent.candidate_root.as_bytes().to_vec()),
    }
}

/// One spilled Commit under an injected abort (`arm` .. `disarm`), then
/// the checks of this file's documentation.
fn spilled_commit(
    store: &TestStore,
    probe: &Probe<'_>,
    arm: &dyn Fn(),
    disarm: &dyn Fn(),
) -> Vec<String> {
    let service = store.open();
    let branch = store.fork("spill");
    let workspace = service.bind(branch);
    apply(&workspace, &Op::Write("/warm".into(), b"warm\n".to_vec())).unwrap();
    assert!(matches!(
        verdict(&commit(&workspace)),
        Verdict::Committed(_)
    ));
    for op in spill(1) {
        apply(&workspace, &op).unwrap();
    }
    let intended = tree_of(&workspace);
    let (hints_before, _) = probe(None);
    let counts = service.service.store.fused_counts();
    arm();
    let first = commit(&workspace);
    disarm();
    let counts = {
        let after = service.service.store.fused_counts();
        (after.0 - counts.0, after.1 - counts.1)
    };
    let root = candidate(&first).expect("the attempt named its candidate root");
    let (hints_after, root_stored) = probe(Some(&root));
    let first = verdict(&first);
    let label = format!(
        "first {first:?}, fused counts {counts:?}, hint blocks {hints_before} -> {hints_after}, \
         candidate root stored {root_stored}"
    );
    assert!(
        hints_after == hints_before || root_stored,
        "{label}: hint rows were written for a tree whose objects were not stored"
    );
    assert!(
        hints_after > hints_before || !root_stored,
        "{label}: the stored tree's bindings were recorded"
    );
    match &first {
        Verdict::Committed(_) => {}
        Verdict::Refused(_) => match verdict(&commit(&workspace)) {
            Verdict::Committed(_) => {}
            other => panic!("{label}: follow-up {other:?}"),
        },
        other => panic!("{label}: {other:?}"),
    }
    assert_eq!(store.tree(branch), intended, "{label}: landed tree");
    service.unmount(workspace);
    service.stop();
    let failures = store.integrity_failures();
    assert!(failures.is_empty(), "{label}: audit: {failures:#?}");
    vec![label]
}

#[test]
fn a_spilled_sqlite_commit_refused_writes_no_hints_before_its_objects() {
    let store = TestStore::sqlite("hint-spill-sqlite", SEED);
    let probe = |root: Option<&[u8]>| {
        let connection = sqlite_connection(&store);
        let hints: i64 = connection
            .query_row("SELECT count(*) FROM binding_hint_block", [], |row| {
                row.get(0)
            })
            .unwrap();
        let stored = root.is_some_and(|root| {
            connection
                .query_row(
                    "SELECT count(*) FROM object_location WHERE object_id = ?1",
                    [root],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap()
                == 1
        });
        (hints, stored)
    };
    let arm = || {
        sqlite_connection(&store)
            .execute_batch(
                "CREATE TRIGGER v768_spill BEFORE INSERT ON \"commit\" \
                 BEGIN SELECT RAISE(ABORT, 'v768 injected abort'); END;",
            )
            .unwrap()
    };
    let disarm = || {
        sqlite_connection(&store)
            .execute_batch("DROP TRIGGER v768_spill")
            .unwrap()
    };
    for line in spilled_commit(&store, &probe, &arm, &disarm) {
        println!("HINT_SPILL sqlite {line}");
    }
}

#[test]
fn a_spilled_postgresql_commit_refused_writes_no_hints_before_its_objects() {
    let Some(store) = TestStore::postgres("hint-spill-pg", SEED) else {
        println!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is not set");
        return;
    };
    let target = verify768::TARGETS
        .iter()
        .copied()
        .find(|target| target.table == "\"commit\"")
        .expect("the commit table target");
    let probe = |root: Option<&[u8]>| {
        let mut admin = store.admin();
        let hints: i64 = admin
            .query_one("SELECT count(*) FROM binding_hint_block", &[])
            .unwrap()
            .get(0);
        let stored = root.is_some_and(|root| {
            admin
                .query_one(
                    "SELECT count(*) FROM object_location WHERE object_id = $1",
                    &[&root],
                )
                .unwrap()
                .get::<_, i64>(0)
                == 1
        });
        (hints, stored)
    };
    let arm = || verify768::arm(&store, target, "40001");
    let disarm = || verify768::disarm(&store, target);
    for line in spilled_commit(&store, &probe, &arm, &disarm) {
        println!("HINT_SPILL postgresql {line}");
    }
}
