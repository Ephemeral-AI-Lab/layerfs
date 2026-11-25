//! #768 verification finding 6: a Commit producer's kept ordinals go back to
//! the Store on every orderly drop of its Storage handle, on SQLite and on
//! PostgreSQL, and only an unknown outcome loses them.
//!
//! A Workspace's Storage handle keeps up to `KEPT_ORDINALS` unused ordinals
//! after each finished Save, so its next small Commit reserves nothing. The
//! handle is dropped in order when its Workspace closes and the Store's idle
//! slot is taken, when the Store closes (`StoreService::close`), and when a
//! Commit fails definitely; each returns the kept ordinals first with the
//! finish's conditional rewind (`next_ordinal` moves back only while it
//! still ends them). Before the fix every such drop lost them: the Store's
//! counter stood `KEPT_ORDINALS` past the last stored value. One-shot
//! handles (Project Init here) keep none and release every unused ordinal at
//! finish, as before #768.
//!
//! Each check reads `store_policy.next_ordinal` and the stored value blocks
//! straight from the private test database. The PostgreSQL half needs
//! `LAYERFS_TEST_POSTGRES_URL` and reports NOT_RUN without it.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "support/fault_proxy.rs"]
#[allow(dead_code)]
mod fault_proxy;
#[path = "support/verify768.rs"]
#[allow(dead_code)]
mod verify768;

use adversary::{commit, verdict, write, Kind, Service, TestStore, Verdict};
use fault_proxy::{Fault, FaultProxy};
use layerfs_persistence::StoreSelection;
use layerfs_storage::policy::KEPT_ORDINALS;
use layerfs_store_service::store::CommitOptions;

const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n")];

/// `store_policy.next_ordinal` and every stored value block's
/// `(first_ordinal, last_ordinal)`, in order.
fn ordinals(store: &TestStore) -> (i64, Vec<(i64, i64)>) {
    const NEXT: &str = "SELECT next_ordinal FROM store_policy WHERE id = 1";
    const BLOCKS: &str =
        "SELECT first_ordinal, last_ordinal FROM inode_value_block ORDER BY first_ordinal";
    match (&store.kind, &store.selection) {
        (Kind::Sqlite, StoreSelection::Sqlite(config)) => {
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

/// The last stored ordinal.
fn last(blocks: &[(i64, i64)]) -> i64 {
    blocks.iter().map(|block| block.1).max().unwrap_or(0)
}

/// Asserts that the Store's counter stands right after its last stored
/// value: no kept ordinal was left behind it.
fn no_gap_at_the_end(store: &TestStore, what: &str) {
    let (next, blocks) = ordinals(store);
    assert_eq!(
        next,
        last(&blocks) + 1,
        "{:?} {what}: counter {next}, values {blocks:?} (KEPT_ORDINALS {KEPT_ORDINALS})",
        store.kind
    );
}

/// The orderly close of a Store service: its idle producer returns its
/// kept ordinals, then the overlay owner stops.
fn close(service: Service) {
    service.service.close().unwrap();
    service.owner.stop().unwrap();
}

/// Mount, two Commits (each with a new file, so new inode values), unmount
/// (the handle goes to the Store's idle slot), orderly close: the counter
/// ends right after the last stored value, and no ordinal anywhere was left
/// unused (Init's one-shot handle released its tail too).
#[test]
fn an_orderly_close_returns_the_kept_ordinals() {
    adversary::tmp();
    for store in TestStore::both("ordinals-close", SEED) {
        let branch = store.fork("work");
        let service = store.open();
        let workspace = service.bind(branch);
        for path in ["/one", "/two"] {
            write(&workspace, path, b"changed\n");
            assert!(matches!(
                verdict(&commit(&workspace)),
                Verdict::Committed(_)
            ));
        }
        service.unmount(workspace);
        close(service);
        no_gap_at_the_end(&store, "after the close");
        let (next, blocks) = ordinals(&store);
        let mut expected = 1;
        for (first, last) in &blocks {
            assert_eq!(*first, expected, "{:?}: gap in {blocks:?}", store.kind);
            expected = last + 1;
        }
        assert_eq!(expected, next);
        // A Store opened again reserves from there: its values follow on.
        let service = store.open();
        let workspace = service.bind(branch);
        write(&workspace, "/three", b"changed\n");
        assert!(matches!(
            verdict(&commit(&workspace)),
            Verdict::Committed(_)
        ));
        service.unmount(workspace);
        close(service);
        let (next_again, blocks) = ordinals(&store);
        assert!(blocks.iter().any(|block| block.0 == next), "{blocks:?}");
        assert_eq!(next_again, last(&blocks) + 1);
        assert!(verify768_duplicates(&store).is_empty());
    }
}

/// Two Workspaces on two Branches, each with its own new handle (the
/// second reserves after the first's kept block). The first unmount takes
/// the Store's idle slot; the second finds it taken: its handle returns its
/// kept ordinals at that unmount, before any Store close.
#[test]
fn a_handle_that_finds_the_idle_slot_taken_returns_its_kept_ordinals() {
    adversary::tmp();
    for store in TestStore::both("ordinals-handoff", SEED) {
        let service = store.open();
        let first = service.bind(store.fork("first"));
        let second = service.bind(store.fork("second"));
        write(&first, "/a", b"a\n");
        assert!(matches!(verdict(&commit(&first)), Verdict::Committed(_)));
        write(&second, "/b", b"b\n");
        assert!(matches!(verdict(&commit(&second)), Verdict::Committed(_)));
        service.unmount(first);
        service.unmount(second);
        no_gap_at_the_end(&store, "after the second unmount");
        close(service);
        let (next, blocks) = ordinals(&store);
        assert!(last(&blocks) < next);
        assert!(verify768_duplicates(&store).is_empty());
    }
}

/// A Commit that fails definitely after its Save finished: the second
/// Workspace's Commit, made only over its bound head (`expected`), finds the
/// head moved by the first. Its fused attempt is a race; the separate path
/// publishes the held tail (the Save completes, its handle keeps ordinals)
/// and then refuses `HeadMoved`. The failed Commit's handle returns its kept
/// ordinals before it is dropped.
#[test]
fn a_definite_commit_failure_returns_the_kept_ordinals() {
    adversary::tmp();
    for store in TestStore::both("ordinals-failed", SEED) {
        let branch = store.fork("work");
        let service = store.open();
        let first = service.bind(branch);
        let second = service.bind(branch);
        let bound = layerfs_store_service::removal::node_of(&second.snapshot().unwrap());
        write(&first, "/a", b"a\n");
        assert!(matches!(verdict(&commit(&first)), Verdict::Committed(_)));
        write(&second, "/b", b"b\n");
        let failed = second.commit_captured(&CommitOptions {
            force: false,
            message: Some("only over the bound head".to_owned()),
            expected: Some(bound),
        });
        let answer = verdict(&failed);
        assert!(
            matches!(&answer, Verdict::Refused(why) if why.contains("HeadMoved")),
            "{:?}: {answer:?}",
            store.kind
        );
        no_gap_at_the_end(&store, "after the failed Commit");
        let _ = service.try_unmount(second);
        service.unmount(first);
        close(service);
        assert!(verify768_duplicates(&store).is_empty());
    }
}

/// An unknown outcome on PostgreSQL: the first Workspace's handle keeps its
/// ordinals; another Commit's reply is lost at its first server message (its
/// new handle's reservation), so the writer session is quarantined. The orderly close still tries the
/// release once; it is refused, the kept ordinals are lost, and a later
/// service never reuses them: its values land past the old counter.
#[test]
fn an_unknown_outcome_loses_the_kept_ordinals_and_never_reuses_them() {
    adversary::tmp();
    let Some(store) = TestStore::postgres("ordinals-unknown", SEED) else {
        return;
    };
    let (url, _) = store.postgres.clone().unwrap();
    let (proxy, proxied) = FaultProxy::start(&url);
    let service = Service::open(&store, store.selection_via(&proxied));
    let first = service.bind(store.fork("first"));
    write(&first, "/a", b"a\n");
    assert!(matches!(verdict(&commit(&first)), Verdict::Committed(_)));
    let (counter, blocks) = ordinals(&store);
    // The handle keeps [counter - KEPT_ORDINALS, counter): the finish
    // rewound the counter to the end of the kept prefix.
    let kept = (counter - KEPT_ORDINALS as i64, counter);
    assert!(last(&blocks) < kept.0, "{blocks:?} {kept:?}");
    let second = service.bind(store.fork("second"));
    write(&second, "/b", b"b\n");
    proxy.arm(Some((0, Fault::Drop)));
    let answer = verdict(&commit(&second));
    proxy.disarm();
    // The lost reply is the new handle's reservation: history is untouched
    // (the Commit refuses at Begin), but the write's outcome is unknown and
    // the writer session is quarantined.
    assert!(
        matches!(&answer, Verdict::Unknown(_))
            || matches!(&answer, Verdict::Refused(why) if why.contains("UnknownOutcome")),
        "{answer:?}"
    );
    let _ = service.try_unmount(second);
    let _ = service.try_unmount(first);
    let _ = service.service.close();
    let _ = service.owner.stop();
    let (after, blocks) = ordinals(&store);
    assert!(
        after >= kept.1,
        "never rewound into the lost block: {after}"
    );
    assert!(
        blocks
            .iter()
            .all(|block| block.1 < kept.0 || block.0 >= kept.1),
        "{blocks:?}"
    );
    // A later service's values never use the lost kept ordinals.
    let direct = store.open();
    let workspace = direct.bind(store.fork("later"));
    write(&workspace, "/c", b"c\n");
    assert!(matches!(
        verdict(&commit(&workspace)),
        Verdict::Committed(_)
    ));
    direct.unmount(workspace);
    close(direct);
    let (_, blocks) = ordinals(&store);
    assert!(
        blocks
            .iter()
            .all(|block| block.1 < kept.0 || block.0 >= after),
        "{blocks:?} kept {kept:?} counter {after}"
    );
    assert!(verify768_duplicates(&store).is_empty());
}

/// Duplicate allocations (PostgreSQL: `verify768`; SQLite: the same value
/// block checks).
fn verify768_duplicates(store: &TestStore) -> Vec<String> {
    match store.kind {
        Kind::Postgres => verify768::duplicate_allocations(store),
        Kind::Sqlite => {
            let (next, blocks) = ordinals(store);
            let mut out = Vec::new();
            for pair in blocks.windows(2) {
                if pair[1].0 <= pair[0].1 {
                    out.push(format!("overlapping blocks {pair:?}"));
                }
            }
            if last(&blocks) >= next {
                out.push(format!("values past the allocator {next}"));
            }
            out
        }
    }
}
