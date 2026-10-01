//! Persisted writer budget and actual scoped Save/index byte admission.
//!
//! Prospective R1 correction: SQL budgets1..64 remain persisted format/admission
//! settings, while the176MiB actual-Store pool supports two72MiB Standard Saves
//! plus each live handle's8MiB index pair. Tests distinguish exact byte refusal
//! before SQL from budget1 SQL-slot refusal, then verify release/readmission,
//! duplicate publication, content/policy compatibility and unchanged real rows.
//! The five old high-concurrency expectations are renamed for this behavior;
//! their historical failures remain unchanged evidence. No byte pool is enlarged.
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

/// Exact pre-effect byte refusal, independent of the still-available SQL budget.
fn byte_refusal(store: &Store, path: &std::path::Path) {
    let before = store.save_working_status().unwrap();
    let slots = live_slots(path);
    let count: i64 = rows(path)
        .query_row("SELECT count(*) FROM saves", [], |row| row.get(0))
        .unwrap();
    assert_eq!(before.limit_bytes, 176 * 1024 * 1024);
    assert_eq!(before.owners.len(), 2);
    assert!(before.owners.iter().all(|owner| owner.save_id.is_some()
        && owner.reserved_bytes == 72 * 1024 * 1024
        && !owner.retained
        && !owner.quarantined));
    assert_eq!(
        before.reserved_bytes,
        before.shared_index_bytes + 2 * 72 * 1024 * 1024
    );
    assert!(matches!(
        disabled(|scope| store.begin_save(scope.child("third-byte-refused"))),
        Err(StorageError::OwnershipUnavailable)
    ));
    let after = store.save_working_status().unwrap();
    assert_eq!(after.byte_refusals, before.byte_refusals + 1);
    assert_eq!(after.last_refused_bytes, Some(72 * 1024 * 1024));
    assert_eq!(
        (
            after.reserved_bytes,
            after.shared_index_bytes,
            after.owners.len()
        ),
        (
            before.reserved_bytes,
            before.shared_index_bytes,
            before.owners.len()
        )
    );
    assert_eq!(
        after
            .owners
            .iter()
            .map(|owner| (owner.slot, owner.save_id, owner.reserved_bytes))
            .collect::<Vec<_>>(),
        before
            .owners
            .iter()
            .map(|owner| (owner.slot, owner.save_id, owner.reserved_bytes))
            .collect::<Vec<_>>()
    );
    assert_eq!(live_slots(path), slots);
    assert_eq!(
        rows(path)
            .query_row("SELECT count(*) FROM saves", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        count,
        "byte refusal created no native Save row"
    );
}
/// SQL budget1 refuses its second owner and returns the tentative byte lease.
fn single_sql_refusal(store: &Store, path: &std::path::Path) {
    let before = store.save_working_status().unwrap();
    let slots = live_slots(path);
    let count: i64 = rows(path)
        .query_row("SELECT count(*) FROM saves", [], |row| row.get(0))
        .unwrap();
    assert_eq!(store.max_concurrent_writes().unwrap(), 1);
    assert_eq!(before.owners.len(), 1);
    assert!(matches!(
        disabled(|scope| store.begin_save(scope.child("second-sql-refused"))),
        Err(StorageError::OwnershipUnavailable)
    ));
    let after = store.save_working_status().unwrap();
    assert_eq!(
        (
            after.byte_refusals,
            after.reserved_bytes,
            after.owners.len()
        ),
        (
            before.byte_refusals,
            before.reserved_bytes,
            before.owners.len()
        )
    );
    assert_eq!(live_slots(path), slots);
    assert_eq!(
        rows(path)
            .query_row("SELECT count(*) FROM saves", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        count
    );
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
fn raised_sql_budget_keeps_alias_visibility_and_two_owner_byte_admission() {
    let temp = TempDir::new("admission-raised");
    let path = temp.store_path("shared");
    let store = create_store(&path);
    assert_eq!(configure(&store, 8), 8);
    let opened = open_store(&path);
    assert_eq!(opened.max_concurrent_writes().unwrap(), 8);
    assert!(store.same_authority(&opened));
    let baseline = store.save_working_status().unwrap();
    assert_eq!(baseline.shared_index_bytes, 2 * 8 * 1024 * 1024);
    let held: Vec<_> = (0..2)
        .map(|_| disabled(|s| opened.begin_save(s.child("writer"))).unwrap())
        .collect();
    assert_eq!(live_slots(&path), vec![1, 2]);
    byte_refusal(&store, &path);
    assert_eq!(
        opened.save_working_status().unwrap().byte_refusals,
        baseline.byte_refusals + 1
    );
    drop(held);
    assert_eq!(
        store.save_working_status().unwrap().reserved_bytes,
        baseline.reserved_bytes
    );
    let again: Vec<_> = (0..2)
        .map(|_| disabled(|s| store.begin_save(s.child("again"))).unwrap())
        .collect();
    assert_eq!(live_slots(&path), vec![1, 2]);
    byte_refusal(&opened, &path);
    drop(again);
    assert_eq!(live_owners(&path), 0);
    assert_eq!(store.max_concurrent_writes().unwrap(), 8);
}

#[test]
fn raised_sql_budget_two_admitted_writers_publish_duplicate_ownership_and_read_back() {
    let temp = TempDir::new("admission-two-at-eight-budget");
    let path = temp.store_path("shared");
    let store = create_store(&path);
    configure(&store, 8);
    let bytes = noise(700_000);
    let (objects, root, _) = construct_file(&bytes);
    // Every writer holds its own private copy of the same identities at once;
    // none of them can see another's row until it publishes.
    let mut saves: Vec<_> = (0..2)
        .map(|_| disabled(|s| store.begin_save(s.child("writer"))).unwrap())
        .collect();
    assert_eq!(live_slots(&path), vec![1, 2]);
    byte_refusal(&store, &path);
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
    assert_eq!(duplicated, 2, "one locator row per simultaneous owner");
    assert_eq!(live_owners(&path), 0, "publication released every slot");
    assert_eq!(read_logical(&store, root), bytes);
    // A later save reuses the published identity instead of adding a third copy.
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
fn raised_budget_definite_failure_returns_only_its_byte_credit_and_sql_slot() {
    let temp = TempDir::new("admission-failure");
    let path = temp.store_path("shared");
    let store = create_store(&path);
    configure(&store, 4);
    let baseline = store.save_working_status().unwrap().reserved_bytes;
    let first = disabled(|s| store.begin_save(s.child("first"))).unwrap();
    let failed = disabled(|s| store.begin_save(s.child("second"))).unwrap();
    assert_eq!(live_slots(&path), vec![1, 2]);
    byte_refusal(&store, &path);
    disabled(|s| failed.abort(s.child("abort"))).unwrap();
    assert_eq!(live_slots(&path), vec![1]);
    assert_eq!(
        store.save_working_status().unwrap().reserved_bytes,
        baseline + 72 * 1024 * 1024
    );
    let replacement = disabled(|s| store.begin_save(s.child("replacement"))).unwrap();
    assert_eq!(live_slots(&path), vec![1, 2]);
    byte_refusal(&store, &path);
    drop(first);
    assert_eq!(live_slots(&path), vec![2]);
    let refill = disabled(|s| store.begin_save(s.child("refill"))).unwrap();
    assert_eq!(live_slots(&path), vec![1, 2]);
    drop((replacement, refill));
    assert_eq!(live_owners(&path), 0);
    assert_eq!(
        store.save_working_status().unwrap().reserved_bytes,
        baseline
    );
    assert_eq!(store.max_concurrent_writes().unwrap(), 4);
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
fn invalid_sql_budgets_preserve_state_and_supported_ceiling_obeys_byte_gate() {
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
    let held: Vec<_> = (0..2)
        .map(|_| disabled(|s| store.begin_save(s.child("writer"))).unwrap())
        .collect();
    assert_eq!(
        live_owners(&path),
        2,
        "the persisted64 budget does not enlarge the actual byte pool"
    );
    assert_eq!(live_slots(&path), vec![1, 2]);
    byte_refusal(&store, &path);
    assert_eq!(store.max_concurrent_writes().unwrap(), 64);
    drop(held);
    assert_eq!(live_owners(&path), 0);
    assert_eq!(store.policy(), Store::default_policy());
}

/// Persisted ladder with the independently fixed two-owner working class.
///
/// The matrix is deliberately end-to-end per setting - admission, slot layout,
/// duplicate ownership, publication, read-back, reuse and a later lowering - so a
/// persisted setting cannot be confused with a larger process-local byte pool.
#[test]
fn persisted_writer_ladder_preserves_content_with_two_owner_byte_gate() {
    let bytes = noise(8_192);
    let (objects, root, _) = construct_file(&bytes);
    for writers in (1usize..=16).chain(std::iter::once(64)) {
        let temp = TempDir::new("admission-matrix");
        let path = temp.store_path("shared");
        let store = create_store(&path);
        assert_eq!(configure(&store, writers as u8), writers as u8);
        assert_eq!(store.max_concurrent_writes().unwrap(), writers as u8);
        let admitted = writers.min(2);
        let mut saves: Vec<_> = (0..admitted)
            .map(|index| {
                disabled(|s| store.begin_save(s.child("writer")))
                    .unwrap_or_else(|error| panic!("budget {writers}: writer {index}: {error}"))
            })
            .collect();
        assert_eq!(
            live_slots(&path),
            (1..=admitted as i64).collect::<Vec<i64>>(),
            "budget {writers} uses exactly its own slot space"
        );
        if writers == 1 {
            single_sql_refusal(&store, &path);
        } else {
            byte_refusal(&store, &path);
        }
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
            duplicated, admitted as i64,
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
        single_sql_refusal(&store, &path);
        drop(only);
        assert_eq!(read_logical(&store, root), bytes);
        assert_eq!(store.policy(), Store::default_policy());
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
