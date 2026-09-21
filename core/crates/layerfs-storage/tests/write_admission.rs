//! The configured per-Store writer budget (#216): admission, retention, reuse.
//!
//! Every assertion here is about persisted state and returned values. Nothing
//! measures time, and no case primes a page cache: each test creates its own
//! Store, acquires and releases real save ownership, and reads the rows back.
mod support;

use layerfs_storage::{StorageError, Store};
use support::{
    construct_file, create_store, disabled, noise, open_store, read_logical, save_all, TempDir,
};

/// Sets the persisted budget on one handle.
fn configure(store: &Store, writes: u8) -> u8 {
    disabled(|scope| store.set_max_concurrent_writes(writes, scope.child("configure")))
        .expect("writer budget is set")
}

/// One direct connection, so a case can read or seed persisted rows itself.
fn rows(path: &std::path::Path) -> rusqlite::Connection {
    rusqlite::Connection::open(path).unwrap()
}

/// Live private save rows.
fn live_owners(path: &std::path::Path) -> i64 {
    rows(path)
        .query_row(
            "SELECT COUNT(*) FROM saves WHERE active_slot IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap()
}

/// Slots of the live private save rows, ascending.
fn live_slots(path: &std::path::Path) -> Vec<i64> {
    rows(path)
        .prepare("SELECT active_slot FROM saves WHERE active_slot IS NOT NULL ORDER BY active_slot")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn a_fresh_store_admits_its_default_budget_and_refuses_the_next_writer() {
    let temp = TempDir::new("admission-default");
    let path = temp.store_path("shared");
    let store = create_store(&path);
    assert_eq!(store.max_concurrent_writes().unwrap(), 2);
    let first = disabled(|s| store.begin_save(s.child("first"))).unwrap();
    let second = disabled(|s| store.begin_save(s.child("second"))).unwrap();
    assert!(matches!(
        disabled(|s| store.begin_save(s.child("third"))),
        Err(StorageError::OwnershipUnavailable)
    ));
    assert_eq!(live_slots(&path), vec![1, 2]);
    drop((first, second));
    assert_eq!(live_owners(&path), 0, "a dropped save releases its slot");
}

#[test]
fn a_raised_budget_admits_that_many_writers_and_refuses_the_next() {
    let temp = TempDir::new("admission-raised");
    let path = temp.store_path("shared");
    let store = create_store(&path);
    assert_eq!(configure(&store, 8), 8);
    // A second handle to the same file sees the same persisted budget.
    let opened = open_store(&path);
    assert_eq!(opened.max_concurrent_writes().unwrap(), 8);
    let held: Vec<_> = (0..8)
        .map(|n| {
            disabled(|s| opened.begin_save(s.child("writer")))
                .unwrap_or_else(|error| panic!("writer {n} is admitted: {error}"))
        })
        .collect();
    assert_eq!(live_slots(&path), (1..=8).collect::<Vec<i64>>());
    assert!(matches!(
        disabled(|s| store.begin_save(s.child("ninth"))),
        Err(StorageError::OwnershipUnavailable)
    ));
    drop(held);
    // Released owners return their slots to the same budget.
    let again: Vec<_> = (0..8)
        .map(|_| disabled(|s| store.begin_save(s.child("again"))).unwrap())
        .collect();
    assert_eq!(live_slots(&path), (1..=8).collect::<Vec<i64>>());
    drop(again);
    assert_eq!(live_owners(&path), 0);
}

#[test]
fn eight_concurrent_writers_publish_duplicate_ownership_and_read_back() {
    let temp = TempDir::new("admission-eight");
    let path = temp.store_path("shared");
    let store = create_store(&path);
    configure(&store, 8);
    let bytes = noise(700_000);
    let (objects, root, _) = construct_file(&bytes);
    // Every writer holds its own private copy of the same identities at once;
    // none of them can see another's row until it publishes.
    let mut saves: Vec<_> = (0..8)
        .map(|_| disabled(|s| store.begin_save(s.child("writer"))).unwrap())
        .collect();
    for save in &mut saves {
        for object in objects.finalized() {
            save.accept(object).unwrap();
        }
    }
    for (index, save) in saves.into_iter().enumerate() {
        let outcome = disabled(|s| save.finish(s.child("finish"))).unwrap();
        assert!(outcome.inserted > 0, "writer {index} inserted its own rows");
    }
    let duplicated: i64 = rows(&path)
        .query_row(
            "SELECT COUNT(*) FROM objects WHERE object_id=?1",
            [root.as_bytes()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(duplicated, 8, "one locator row per simultaneous owner");
    assert_eq!(live_owners(&path), 0, "publication released every slot");
    assert_eq!(read_logical(&store, root), bytes);
    // A later save reuses the published identity instead of adding a ninth copy.
    let outcome = save_all(&store, &objects).unwrap();
    assert_eq!(outcome.inserted, 0);
    assert!(outcome.reused > 0);
    assert_eq!(read_logical(&open_store(&path), root), bytes);
}

#[test]
fn a_lowered_budget_keeps_retained_ownership_and_counts_it() {
    let temp = TempDir::new("admission-lowered");
    let path = temp.store_path("shared");
    let store = create_store(&path);
    configure(&store, 8);
    drop(store);
    // Two owners retained in the wider space, exactly as an unresolved outcome
    // or a failed cleanup leaves them: the product never guesses them away.
    rows(&path)
        .execute("INSERT INTO saves(active_slot) VALUES(7),(8)", [])
        .unwrap();
    let reopened = open_store(&path);
    assert_eq!(configure(&reopened, 2), 2);
    assert_eq!(live_slots(&path), vec![7, 8], "lowering releases nothing");
    assert!(matches!(
        disabled(|s| reopened.begin_save(s.child("refused"))),
        Err(StorageError::OwnershipUnavailable)
    ));
    // An explicit external decision about one named, empty save removes it; the
    // product never does this on reopen or by slot number.
    assert_eq!(
        rows(&path)
            .execute("DELETE FROM saves WHERE active_slot=7", [])
            .unwrap(),
        1
    );
    let admitted = disabled(|s| reopened.begin_save(s.child("admitted"))).unwrap();
    assert_eq!(
        live_slots(&path),
        vec![1, 8],
        "admission uses the budget space"
    );
    drop(admitted);
    assert_eq!(live_owners(&path), 1);
}

#[test]
fn a_retained_owner_above_the_budget_is_never_bypassed() {
    let temp = TempDir::new("admission-above");
    let path = temp.store_path("shared");
    let store = create_store(&path);
    configure(&store, 8);
    drop(store);
    rows(&path)
        .execute("INSERT INTO saves(active_slot) VALUES(5)", [])
        .unwrap();
    let reopened = open_store(&path);
    configure(&reopened, 2);
    assert_eq!(live_slots(&path), vec![5], "the wider slot survives");
    // One live owner is below the budget, so one writer is admitted - inside the
    // budget's own space, leaving the retained row exactly as it was.
    let admitted = disabled(|s| reopened.begin_save(s.child("admitted"))).unwrap();
    assert_eq!(live_slots(&path), vec![1, 5]);
    assert!(matches!(
        disabled(|s| reopened.begin_save(s.child("refused"))),
        Err(StorageError::OwnershipUnavailable)
    ));
    let retained: i64 = rows(&path)
        .query_row(
            "SELECT COUNT(*) FROM saves WHERE active_slot = 5",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(retained, 1, "product code never discarded it");
    drop(admitted);
}

#[test]
fn a_definite_failure_at_a_higher_budget_releases_only_its_own_slot() {
    let temp = TempDir::new("admission-failure");
    let path = temp.store_path("shared");
    let store = create_store(&path);
    configure(&store, 4);
    let mut saves: Vec<_> = (0..4)
        .map(|_| disabled(|s| store.begin_save(s.child("writer"))).unwrap())
        .collect();
    assert_eq!(live_slots(&path), vec![1, 2, 3, 4]);
    // One writer ends definitely; only its own slot and rows are released.
    let failed = saves.remove(2);
    disabled(|s| failed.abort(s.child("abort"))).unwrap();
    assert_eq!(live_slots(&path), vec![1, 2, 4]);
    let replacement = disabled(|s| store.begin_save(s.child("replacement"))).unwrap();
    assert_eq!(live_slots(&path), vec![1, 2, 3, 4]);
    assert!(matches!(
        disabled(|s| store.begin_save(s.child("over"))),
        Err(StorageError::OwnershipUnavailable)
    ));
    drop(saves);
    drop(replacement);
    assert_eq!(live_owners(&path), 0);
}

#[test]
fn content_written_under_a_higher_budget_is_readable_after_it_is_lowered() {
    let temp = TempDir::new("admission-compat");
    let path = temp.store_path("shared");
    let store = create_store(&path);
    configure(&store, 8);
    let bytes = noise(600_000);
    let (objects, root, _) = construct_file(&bytes);
    let outcome = save_all(&store, &objects).unwrap();
    assert!(outcome.inserted > 0);
    drop(store);
    let reopened = open_store(&path);
    configure(&reopened, 1);
    assert_eq!(reopened.max_concurrent_writes().unwrap(), 1);
    assert_eq!(read_logical(&reopened, root), bytes);
    let only = disabled(|s| reopened.begin_save(s.child("only"))).unwrap();
    assert!(matches!(
        disabled(|s| reopened.begin_save(s.child("second"))),
        Err(StorageError::OwnershipUnavailable)
    ));
    drop(only);
    // The budget is an admission setting, not a format property: the immutable
    // construction policy of the same Store is untouched by the change.
    assert_eq!(reopened.policy(), Store::default_policy());
}

#[test]
fn an_unsupported_budget_is_refused_without_changing_the_store() {
    let temp = TempDir::new("admission-unsupported");
    let path = temp.store_path("shared");
    let store = create_store(&path);
    for value in [0u8, 65, u8::MAX] {
        assert!(matches!(
            disabled(|s| store.set_max_concurrent_writes(value, s.child("configure"))),
            Err(StorageError::UnsupportedPolicy {
                field: "max_concurrent_writes"
            })
        ));
        assert_eq!(store.max_concurrent_writes().unwrap(), 2);
        assert_eq!(live_owners(&path), 0);
    }
    assert_eq!(configure(&store, 64), 64);
    let held: Vec<_> = (0..64)
        .map(|_| disabled(|s| store.begin_save(s.child("writer"))).unwrap())
        .collect();
    assert_eq!(
        live_owners(&path),
        64,
        "the whole supported space is usable"
    );
    assert!(matches!(
        disabled(|s| store.begin_save(s.child("over"))),
        Err(StorageError::OwnershipUnavailable)
    ));
    drop(held);
}

/// Every supported setting, one fresh Store each: the whole ladder in one case.
///
/// The matrix is deliberately end-to-end per setting - admission, slot layout,
/// duplicate ownership, publication, read-back, reuse and a later lowering - so a
/// setting that only works in isolation cannot pass by skipping a step.
#[test]
fn every_supported_setting_admits_its_budget_and_publishes_correctly() {
    let bytes = noise(8_192);
    let (objects, root, _) = construct_file(&bytes);
    for writers in [1usize, 2, 3, 5, 8, 16, 64] {
        let temp = TempDir::new("admission-matrix");
        let path = temp.store_path("shared");
        let store = create_store(&path);
        assert_eq!(configure(&store, writers as u8), writers as u8);
        let mut saves: Vec<_> = (0..writers)
            .map(|index| {
                disabled(|s| store.begin_save(s.child("writer")))
                    .unwrap_or_else(|error| panic!("budget {writers}: writer {index}: {error}"))
            })
            .collect();
        assert_eq!(
            live_slots(&path),
            (1..=writers as i64).collect::<Vec<i64>>(),
            "budget {writers} uses exactly its own slot space"
        );
        assert!(
            matches!(
                disabled(|s| store.begin_save(s.child("over"))),
                Err(StorageError::OwnershipUnavailable)
            ),
            "budget {writers} refuses the next writer"
        );
        // Every writer holds its own private copy of the same identity, then
        // publishes; publication releases every slot and leaves the copies.
        for save in &mut saves {
            for object in objects.finalized() {
                save.accept(object).unwrap();
            }
        }
        for save in saves {
            disabled(|s| save.finish(s.child("finish"))).unwrap();
        }
        assert_eq!(
            live_owners(&path),
            0,
            "budget {writers} released every slot"
        );
        let duplicated: i64 = rows(&path)
            .query_row(
                "SELECT COUNT(*) FROM objects WHERE object_id=?1",
                [root.as_bytes()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            duplicated, writers as i64,
            "budget {writers} kept one locator per simultaneous owner"
        );
        assert_eq!(
            read_logical(&store, root),
            bytes,
            "budget {writers} reads back"
        );
        let outcome = save_all(&store, &objects).unwrap();
        assert_eq!(
            outcome.inserted, 0,
            "budget {writers} reuses after publication"
        );
        assert!(outcome.reused > 0);
        // Lowering to one is an admission change: retained rows stay readable.
        assert_eq!(configure(&store, 1), 1);
        let only = disabled(|s| store.begin_save(s.child("only"))).unwrap();
        assert!(matches!(
            disabled(|s| store.begin_save(s.child("second"))),
            Err(StorageError::OwnershipUnavailable)
        ));
        drop(only);
        assert_eq!(read_logical(&store, root), bytes);
    }
}

#[test]
fn a_schema_seven_store_is_refused_without_promotion() {
    let temp = TempDir::new("admission-schema7");
    let path = temp.store_path("seven");
    drop(create_store(&path));
    let connection = rows(&path);
    connection.execute_batch("PRAGMA user_version = 7").unwrap();
    assert!(matches!(
        disabled(|s| Store::open(&path, s.child("open"))),
        Err(StorageError::UnsupportedPolicy {
            field: "schema identity"
        })
    ));
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 7, "the refused Store is left exactly as it was");
}

#[test]
fn the_shipped_schema_constrains_the_persisted_budget() {
    let temp = TempDir::new("admission-range");
    let path = temp.store_path("shared");
    drop(create_store(&path));
    let connection = rows(&path);
    // An out-of-range row cannot exist in a Store this build created, so a
    // reader never has to clamp one into a budget: the constraint refuses it.
    assert!(connection
        .execute(
            "UPDATE store_policy SET max_concurrent_writes = 65 WHERE id = 1",
            []
        )
        .is_err());
    assert!(connection
        .execute(
            "UPDATE store_policy SET max_concurrent_writes = 0 WHERE id = 1",
            []
        )
        .is_err());
    assert_eq!(open_store(&path).max_concurrent_writes().unwrap(), 2);
}
