//! #768: a fused Workspace Commit refused definitely without a fallback
//! returns its Save's unused reserved ids, on SQLite and on PostgreSQL.
//!
//! The Commit's new Storage handle reserves a block (16,384 ordinals) in a
//! transaction of its own; its Save holds its last publication (values, the
//! release of the ordinal tail) for the fused transaction, which a trigger
//! aborts at the Commit row: a definite refusal (SQLite `RAISE(ABORT)`,
//! PostgreSQL `P0001`, `Integrity`), so the whole transaction is rolled back
//! and nothing of the Save's last publication applied. The Save does not
//! finish, but its unused ids, from the first ordinal it did not assign up
//! to the block's end, go to the handle, and the failed Commit returns the
//! ordinals with the conditional rewind. The ordinals the refused
//! publication assigned stay below the counter and are never reused: after
//! a follow-up Commit and an orderly close, the Store's unused ordinals are
//! exactly those. Before the fix the unfinished Save dropped the whole
//! block: the counter stood 16,384 past the last stored value.
//!
//! On PostgreSQL an abort with an unclassified SQLSTATE (`XX000`) is an
//! unknown outcome: the session is quarantined, the tail is dropped and the
//! block is lost, never reused. Needs `LAYERFS_TEST_POSTGRES_URL` for the
//! PostgreSQL half; NOT_RUN without it.
#[path = "../support/verify/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "../support/verify/core.rs"]
#[allow(dead_code)]
mod verify768;

use adversary::{apply, commit, verdict, Kind, Op, Service, TestStore, Verdict};
use layerfs_persistence::StoreSelection;
use verify768::{Step, Target};

const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n")];
/// The Commit row: written after the publication's rows in the fused
/// transaction.
const COMMIT_ROW: Target = Target {
    table: "\"commit\"",
    event: "INSERT",
    step: Step::Commit,
};

/// `store_policy.next_ordinal` and every stored value block's
/// `(first_ordinal, last_ordinal)`, in order.
fn ordinals(store: &TestStore) -> (i64, Vec<(i64, i64)>) {
    const NEXT: &str = "SELECT next_ordinal FROM store_policy WHERE id = 1";
    const BLOCKS: &str =
        "SELECT first_ordinal, last_ordinal FROM inode_value_block ORDER BY first_ordinal";
    match &store.selection {
        StoreSelection::Sqlite(config) => {
            let sql = rusqlite::Connection::open(&config.path).unwrap();
            let next = sql.query_row(NEXT, [], |row| row.get(0)).unwrap();
            let blocks = sql
                .prepare(BLOCKS)
                .unwrap()
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            (next, blocks)
        }
        _ => {
            let mut admin = store.admin();
            let next = admin.query_one(NEXT, &[]).unwrap().get(0);
            let blocks = admin
                .query(BLOCKS, &[])
                .unwrap()
                .iter()
                .map(|row| (row.get(0), row.get(1)))
                .collect();
            (next, blocks)
        }
    }
}

/// Ordinals below the counter that hold no stored value.
fn unused(store: &TestStore) -> i64 {
    let (next, blocks) = ordinals(store);
    next - 1 - blocks.iter().map(|(a, b)| b - a + 1).sum::<i64>()
}

/// Installs an abort of the fused transaction at its Commit row (`code` is
/// the PostgreSQL SQLSTATE).
fn arm(store: &TestStore, code: &str) {
    match store.kind {
        Kind::Sqlite => sqlite(store)
            .execute_batch(
                "CREATE TRIGGER v768_t BEFORE INSERT ON \"commit\" \
                 BEGIN SELECT RAISE(ABORT, 'v768 injected abort'); END;",
            )
            .unwrap(),
        Kind::Postgres => verify768::arm(store, COMMIT_ROW, code),
    }
}

fn disarm(store: &TestStore) {
    match store.kind {
        Kind::Sqlite => sqlite(store).execute_batch("DROP TRIGGER v768_t").unwrap(),
        Kind::Postgres => verify768::disarm(store, COMMIT_ROW),
    }
}

fn sqlite(store: &TestStore) -> rusqlite::Connection {
    let StoreSelection::Sqlite(config) = &store.selection else {
        unreachable!()
    };
    let connection = rusqlite::Connection::open(&config.path).unwrap();
    connection
        .busy_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    connection
}

/// A new directory of `files` small files: that many new inode values.
fn change(workspace: &layerfs_store_service::store::BoundWorkspace, tag: &str, files: usize) {
    apply(workspace, &Op::Mkdir(format!("/{tag}"))).unwrap();
    for n in 0..files {
        apply(
            workspace,
            &Op::Write(format!("/{tag}/f{n}"), format!("{tag} {n}\n").into_bytes()),
        )
        .unwrap();
    }
}

fn close(service: Service) {
    service.service.close().unwrap();
    service.owner.stop().unwrap();
}

#[test]
fn a_definite_fused_refusal_returns_the_unused_block() {
    adversary::tmp();
    for store in TestStore::both("fused-refusal-ids", SEED) {
        let kind = store.kind;
        let branch = store.fork("work");
        let service = store.open();
        let workspace = service.bind(branch);
        change(&workspace, "many", 30);
        let (before, stored) = ordinals(&store);
        assert_eq!(unused(&store), 0, "{kind:?}: no gap before");
        arm(&store, "P0001");
        let first = commit(&workspace);
        disarm(&store);
        let answer = verdict(&first);
        assert!(
            matches!(&answer, Verdict::Refused(_)),
            "{kind:?}: a definite refusal: {answer:?}"
        );
        let (after, blocks) = ordinals(&store);
        assert_eq!(blocks, stored, "{kind:?}: nothing of the Save stored");
        // The block was reserved from `before` and its unused part returned:
        // the counter stands right after the ordinals the refused
        // publication assigned (before the fix: `before + 16,384`).
        let assigned = after - before;
        assert!(
            (1..=64).contains(&assigned),
            "{kind:?}: counter {before} -> {after}"
        );
        // The follow-up Commit lands with values past the assigned ones.
        let again = verdict(&commit(&workspace));
        assert!(
            matches!(again, Verdict::Committed(_)),
            "{kind:?}: {again:?}"
        );
        let (_, blocks) = ordinals(&store);
        assert!(
            blocks
                .iter()
                .filter(|block| !stored.contains(block))
                .all(|block| block.0 >= after),
            "{kind:?}: the refused ordinals are never reused: {blocks:?}"
        );
        service.unmount(workspace);
        close(service);
        // Only the refused publication's own ordinals stay unused.
        assert_eq!(unused(&store), assigned, "{kind:?}: {:?}", ordinals(&store));
        let (next, blocks) = ordinals(&store);
        assert_eq!(next, blocks.last().unwrap().1 + 1, "{kind:?}");
    }
}

#[test]
fn an_unknown_fused_outcome_loses_the_block_and_never_reuses_it() {
    adversary::tmp();
    let Some(store) = TestStore::postgres("fused-unknown-ids", SEED) else {
        return;
    };
    let branch = store.fork("work");
    let service = store.open();
    let workspace = service.bind(branch);
    change(&workspace, "many", 30);
    let (before, stored) = ordinals(&store);
    arm(&store, "XX000");
    let answer = verdict(&commit(&workspace));
    disarm(&store);
    assert!(matches!(answer, Verdict::Unknown(_)), "{answer:?}");
    let _ = service.try_unmount(workspace);
    service.abandon();
    let (after, blocks) = ordinals(&store);
    assert_eq!(blocks, stored);
    // The whole block stays reserved: nothing was returned.
    assert_eq!(after, before + 16_384);
    let later = store.open();
    let workspace = later.bind(store.fork("later"));
    change(&workspace, "later", 5);
    assert!(matches!(
        verdict(&commit(&workspace)),
        Verdict::Committed(_)
    ));
    later.unmount(workspace);
    close(later);
    let (_, blocks) = ordinals(&store);
    assert!(
        blocks
            .iter()
            .filter(|block| !stored.contains(block))
            .all(|block| block.0 >= after),
        "{blocks:?}"
    );
    assert!(verify768::duplicate_allocations(&store).is_empty());
}
