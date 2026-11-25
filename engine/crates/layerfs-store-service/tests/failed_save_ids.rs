//! #768 review 6a/6b: the ids of a Commit's Storage handle on orderly
//! failure paths, on SQLite and on PostgreSQL.
//!
//! * A Save that fails definitely (here a forced Commit's finish
//!   publication, refused by a trigger: SQLite `RAISE(ABORT)`, PostgreSQL
//!   `P0001`) leaves its unused ids, from the first one it did not assign,
//!   on the handle, and the failed Commit returns the ordinals. Before the
//!   fix `Save::drop` dropped the block: the counter stood 16,384 past the
//!   last stored value. The same rule covers an in-lane step Save and a
//!   fused fallback's merge Save, which share `Save::drop`.
//! * A Workspace close whose overlay close fails returns its idle handle's
//!   kept ordinals before it answers (before: the handle was dropped
//!   unreleased, 128 left behind).
//! * A Commit refused `Busy` (a held SQLite write lock; a PostgreSQL lane
//!   row held past the lock timeout) sends no release on its failure path
//!   and keeps its handle, with its block, for the Workspace's next Commit,
//!   which reads no policy and reserves nothing (before: a new handle).
//!
//! The PostgreSQL half needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "../../layerfs-daemon/tests/support/held_writer.rs"]
mod held_writer;

use adversary::{commit, commit_forced, verdict, write, Kind, Service, TestStore, Verdict};
use layerfs_persistence::StoreSelection;

const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n")];

/// `store_policy.next_ordinal` and every stored value block, in order.
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

fn last(store: &TestStore) -> i64 {
    ordinals(store)
        .1
        .iter()
        .map(|block| block.1)
        .max()
        .unwrap_or(0)
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

/// Refuses every locator insert until [`allow`].
fn refuse_locations(store: &TestStore) {
    match store.kind {
        Kind::Sqlite => sqlite(store)
            .execute_batch(
                "CREATE TRIGGER v768_f BEFORE INSERT ON object_location \
                 BEGIN SELECT RAISE(ABORT, 'v768 refused publication'); END;",
            )
            .unwrap(),
        Kind::Postgres => store
            .admin()
            .batch_execute(
                "CREATE OR REPLACE FUNCTION v768_refuse() RETURNS trigger LANGUAGE plpgsql AS $f$ \
                 BEGIN RAISE EXCEPTION 'v768 refused publication' USING ERRCODE = 'P0001'; END $f$; \
                 CREATE TRIGGER v768_f BEFORE INSERT ON object_location \
                 FOR EACH ROW EXECUTE FUNCTION v768_refuse();",
            )
            .unwrap(),
    }
}

fn allow(store: &TestStore) {
    match store.kind {
        Kind::Sqlite => sqlite(store).execute_batch("DROP TRIGGER v768_f").unwrap(),
        Kind::Postgres => store
            .admin()
            .batch_execute("DROP TRIGGER v768_f ON object_location; DROP FUNCTION v768_refuse();")
            .unwrap(),
    }
}

fn close(service: Service) {
    service.service.close().unwrap();
    service.owner.stop().unwrap();
}

#[test]
fn a_save_refused_definitely_returns_its_unused_block() {
    adversary::tmp();
    for store in TestStore::both("failed-save-ids", SEED) {
        let kind = store.kind;
        let service = store.open();
        let workspace = service.bind(store.fork("work"));
        write(&workspace, "/a", b"a\n");
        let before = ordinals(&store).0;
        refuse_locations(&store);
        let answer = verdict(&commit_forced(&workspace));
        allow(&store);
        assert!(
            matches!(&answer, Verdict::Refused(_)),
            "{kind:?}: a definite refusal: {answer:?}"
        );
        // The new handle reserved from `before`; the refused Save's unused
        // rest went back (before the fix: `before + 16,384`).
        let after = ordinals(&store).0;
        assert!(
            (before + 1..=before + 16).contains(&after),
            "{kind:?}: counter {before} -> {after}"
        );
        assert!(matches!(
            verdict(&commit(&workspace)),
            Verdict::Committed(_)
        ));
        let (_, blocks) = ordinals(&store);
        assert!(
            blocks
                .iter()
                .filter(|b| b.1 >= before)
                .all(|b| b.0 >= after),
            "{kind:?}: the refused ordinals are never reused: {blocks:?}"
        );
        service.unmount(workspace);
        close(service);
        assert_eq!(ordinals(&store).0, last(&store) + 1, "{kind:?}");
    }
}

#[test]
fn a_close_whose_overlay_close_fails_returns_the_kept_ordinals() {
    adversary::tmp();
    for store in TestStore::both("failed-close-ids", SEED) {
        let kind = store.kind;
        let Service { service, owner } = store.open();
        let workspace = service
            .store
            .bind(
                owner.client(),
                layerfs_store_service::store::BindRequest {
                    branch: store.fork("work"),
                    workspace: layerfs_history::WorkspaceId::from_authority([0x6a; 32]).unwrap(),
                },
            )
            .unwrap()
            .workspace;
        write(&workspace, "/a", b"a\n");
        assert!(matches!(
            verdict(&commit(&workspace)),
            Verdict::Committed(_)
        ));
        assert_eq!(
            ordinals(&store).0,
            last(&store) + 1 + layerfs_storage::policy::KEPT_ORDINALS as i64,
            "{kind:?}: the handle keeps its ordinals"
        );
        // The overlay owner is gone: the close fails at its first step.
        owner.stop().unwrap();
        assert!(workspace.close().is_err(), "{kind:?}");
        assert_eq!(ordinals(&store).0, last(&store) + 1, "{kind:?}");
        drop(workspace);
        drop(service);
    }
}

#[test]
fn a_busy_commit_keeps_its_handle_for_the_next_commit() {
    adversary::tmp();
    for store in TestStore::both("busy-handle", SEED) {
        let kind = store.kind;
        let service = Service::open(&store, store.with_lock_timeout(&store.selection, 300));
        let workspace = service.bind(store.fork("work"));
        write(&workspace, "/a", b"a\n");
        assert!(matches!(
            verdict(&commit(&workspace)),
            Verdict::Committed(_)
        ));
        write(&workspace, "/b", b"b\n");
        let counter = ordinals(&store).0;
        let busy = match kind {
            Kind::Sqlite => {
                let StoreSelection::Sqlite(config) = &store.selection else {
                    unreachable!()
                };
                let held = held_writer::HeldWriter::acquire(&config.path);
                let answer = verdict(&commit(&workspace));
                held.release();
                answer
            }
            Kind::Postgres => {
                let mut blocker = store.admin();
                blocker
                    .batch_execute("BEGIN; SELECT 1 FROM store_lane FOR UPDATE")
                    .unwrap();
                let answer = verdict(&commit(&workspace));
                blocker.batch_execute("ROLLBACK").unwrap();
                answer
            }
        };
        assert!(
            matches!(&busy, Verdict::Refused(why) if why.contains("Busy")),
            "{kind:?}: {busy:?}"
        );
        assert_eq!(ordinals(&store).0, counter, "{kind:?}: no release sent");
        let next = commit(&workspace).unwrap_or_else(|f| panic!("{kind:?}: {f}"));
        assert!(next.history.is_some());
        assert_eq!(next.storage.policy, 0, "{kind:?}: the kept handle");
        assert_eq!(
            next.storage.initial_reservations, 0,
            "{kind:?}: its kept block"
        );
        service.unmount(workspace);
        close(service);
        assert_eq!(ordinals(&store).0, last(&store) + 1, "{kind:?}");
    }
}
