//! Publication visibility: what an ordinary reader may see of an unfinished save.
//!
//! The contract under test is that early bounded commits of a save do not make its
//! output available to an unrelated reader, that same-save reads stay unrestricted,
//! and that the boundary is the *publication watermark* rather than the highest
//! pack that happens to be committed.

mod support;

use layerfs_content::ObjectId;
use layerfs_storage::{SaveHandoff, StorageError, StoragePolicy, Store};
use support::{construct_file, create_store, disabled, noise, open_store, save_all, TempDir};

/// Highest pack id present, and the published watermark, read out of band.
fn pack_state(path: &std::path::Path) -> (i64, i64) {
    let connection = rusqlite::Connection::open(path).unwrap();
    let highest: i64 = connection
        .query_row(
            "SELECT COALESCE(MAX(pack_id), 0) FROM object_packs",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let ceiling: i64 = connection
        .query_row(
            "SELECT retained_pack_ceiling FROM store_policy WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    (ceiling, highest)
}

/// One object id whose row lives in a pack above `ceiling`.
fn object_above(path: &std::path::Path, ceiling: i64) -> Option<ObjectId> {
    let connection = rusqlite::Connection::open(path).unwrap();
    let bytes: Option<Vec<u8>> = connection
        .query_row(
            "SELECT object_id FROM objects WHERE pack_id > ?1 LIMIT 1",
            [ceiling],
            |row| row.get(0),
        )
        .ok();
    bytes.map(|bytes| ObjectId::from_bytes(&bytes).unwrap())
}

#[test]
fn an_unrelated_reader_cannot_see_an_open_save_but_sees_it_after_acknowledgement() {
    let dir = TempDir::new("visibility_open");
    let path = dir.store_path("visibility_open");
    let store = create_store(&path);

    // Large enough to force early bounded commits during acceptance.
    let body = noise(5 * 1024 * 1024);
    let (collected, root, _) = construct_file(&body);
    let ids: Vec<ObjectId> = collected
        .objects()
        .iter()
        .map(|(id, _, _, _)| *id)
        .collect();

    let mut operation = disabled(|scope| store.begin_save(scope.child("storage.begin"))).unwrap();
    for (_, role, bytes, references) in collected.objects() {
        let object = layerfs_content::FinalizedObject::new(*role, bytes.clone())
            .unwrap()
            .with_references(references.clone());
        disabled(|scope| operation.accept(object, scope.child("storage.accept"))).unwrap();
    }

    let (ceiling, highest) = pack_state(&path);
    assert!(
        highest > ceiling,
        "the save must have committed packs early: ceiling={ceiling} highest={highest}"
    );

    // An object that is committed but not yet published must be refused, and
    // refused for the right reason: the pack is beyond the watermark.
    let unpublished = object_above(&path, ceiling).expect("a committed but unpublished object");
    let error = disabled(|scope| store.read_batch(&[unpublished], scope.child("storage.read")))
        .unwrap_err();
    assert!(
        matches!(error, StorageError::VisibilityCeiling { .. }),
        "expected a visibility refusal, got {error}"
    );

    // The whole object set is likewise unavailable.
    let error = disabled(|scope| store.read_batch(&ids, scope.child("storage.read"))).unwrap_err();
    assert!(
        matches!(
            error,
            StorageError::VisibilityCeiling { .. } | StorageError::ObjectMissing(_)
        ),
        "got {error}"
    );

    let outcome = disabled(|scope| operation.finish(scope.child("storage.finish"))).unwrap();
    assert!(outcome.inserted > 0);

    let (ceiling_after, highest_after) = pack_state(&path);
    assert_eq!(
        ceiling_after, highest_after,
        "acknowledgement publishes every pack the save created"
    );
    let (values, _) =
        disabled(|scope| store.read_batch(&[root, unpublished], scope.child("read"))).unwrap();
    assert_eq!(values.len(), 2);
}

#[test]
fn a_definite_failure_never_publishes_and_never_moves_the_watermark() {
    let dir = TempDir::new("visibility_failure");
    let path = dir.store_path("visibility_failure");
    let store = create_store(&path);

    let retained = noise(600_000);
    let (retained_objects, retained_root, _) = construct_file(&retained);
    save_all(&store, &retained_objects).unwrap();
    let ceiling_before = pack_state(&path).0;
    assert!(ceiling_before > 0);

    // A streamed file large enough to commit bounded transactions early, whose
    // source then fails. Construction feeds the save directly, so the failure
    // arrives after this save has already published packs.
    let body = noise(4 * 1024 * 1024);
    let failing = support::FailingAfter::new(body, 4 * 1024 * 1024 - 4_096);
    let policy = layerfs_content::ConstructionPolicy::frozen_default();
    let error = disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        let mut handoff = SaveHandoff::new(&mut operation);
        let constructed = layerfs_content::construct_stream(
            policy,
            &policy.capacities(),
            failing,
            &mut handoff,
            scope.child("content"),
        );
        if let Some(failure) = handoff.take_failure() {
            return Err(failure);
        }
        constructed.map_err(StorageError::Content)?;
        operation.finish(scope.child("storage.finish"))?;
        Ok(())
    })
    .unwrap_err();
    assert!(
        matches!(error, StorageError::Content(_)),
        "the input failure is reported once: {error}"
    );

    let (ceiling_after, highest_after) = pack_state(&path);
    assert_eq!(
        ceiling_after, ceiling_before,
        "a failure must not advance the watermark, even though it committed packs early"
    );
    assert_eq!(
        highest_after, ceiling_before,
        "cleanup removed the failed attempt's packs"
    );

    let (values, _) =
        disabled(|scope| store.read_batch(&[retained_root], scope.child("read"))).unwrap();
    let expected = retained_objects
        .objects()
        .iter()
        .find(|entry| entry.0 == retained_root)
        .expect("the retained root is one of the accepted objects")
        .2
        .clone();
    assert_eq!(values[0], expected);
}

#[test]
fn an_uninspected_store_refuses_to_start_a_new_save() {
    let dir = TempDir::new("visibility_uninspected");
    let path = dir.store_path("visibility_uninspected");
    {
        let store = create_store(&path);
        let (objects, _, _) = construct_file(&noise(600_000));
        save_all(&store, &objects).unwrap();
    }
    let (ceiling, highest) = pack_state(&path);
    assert_eq!(ceiling, highest);

    // Simulate a save whose cleanup did not complete: a pack and an object above
    // the watermark remain, exactly as an interrupted cleanup would leave them.
    {
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute(
                "INSERT INTO object_packs (pack_id, data) VALUES (?1, ?2)",
                rusqlite::params![highest + 1, vec![0u8; 32]],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO objects \
                 (object_id, object_role, canonical_length, base_object_id, pack_id, group_number, record_number) \
                 VALUES (?1, 1, 100, NULL, ?2, 0, 0)",
                rusqlite::params![vec![0x7eu8; 32], highest + 1],
            )
            .unwrap();
    }

    let store = open_store(&path);
    let attempt = disabled(|scope| store.begin_save(scope.child("storage.begin")));
    match attempt {
        Err(StorageError::UninspectedState {
            ceiling: reported,
            highest_pack_id,
        }) => {
            assert_eq!(reported, ceiling);
            assert_eq!(highest_pack_id, highest + 1);
        }
        Err(other) => panic!("expected an uninspected-state refusal, got {other}"),
        Ok(_) => panic!("an uninspected Store must not accept a new save"),
    }
}

#[test]
fn a_watermark_ahead_of_storage_is_rejected_at_open() {
    let dir = TempDir::new("visibility_ahead");
    let path = dir.store_path("visibility_ahead");
    {
        let store = create_store(&path);
        let (objects, _, _) = construct_file(&noise(600_000));
        save_all(&store, &objects).unwrap();
    }
    {
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute(
                "UPDATE store_policy SET retained_pack_ceiling = retained_pack_ceiling + 5 WHERE id = 1",
                [],
            )
            .unwrap();
    }
    match disabled(|scope| Store::open(&path, scope.child("store"))) {
        Err(StorageError::Integrity(_)) => {}
        Err(other) => panic!("expected an integrity refusal, got {other}"),
        Ok(_) => panic!("a watermark ahead of storage must be rejected at open"),
    }
}

#[test]
fn the_watermark_survives_reopen_and_still_hides_a_later_open_save() {
    let dir = TempDir::new("visibility_reopen");
    let path = dir.store_path("visibility_reopen");
    let first = noise(600_000);
    let (first_objects, first_root, _) = construct_file(&first);
    {
        let store = create_store(&path);
        save_all(&store, &first_objects).unwrap();
    }
    let (ceiling, highest) = pack_state(&path);
    assert_eq!(ceiling, highest);

    let reopened = open_store(&path);
    let (values, counters) =
        disabled(|scope| reopened.read_batch(&[first_root], scope.child("read"))).unwrap();
    assert_eq!(values.len(), 1);
    assert_eq!(
        counters.ceiling, ceiling,
        "a reopened Store reads under its persisted watermark"
    );

    // A later save still hides its own output until it is acknowledged.
    let (second_objects, second_root, _) = construct_file(&noise(5 * 1024 * 1024));
    let mut operation =
        disabled(|scope| reopened.begin_save(scope.child("storage.begin"))).unwrap();
    for (_, role, bytes, references) in second_objects.objects() {
        let object = layerfs_content::FinalizedObject::new(*role, bytes.clone())
            .unwrap()
            .with_references(references.clone());
        disabled(|scope| operation.accept(object, scope.child("storage.accept"))).unwrap();
    }
    let (mid_ceiling, mid_highest) = pack_state(&path);
    assert!(mid_highest > mid_ceiling);
    let unpublished = object_above(&path, mid_ceiling).expect("an unpublished object");
    let error =
        disabled(|scope| reopened.read_batch(&[unpublished], scope.child("read"))).unwrap_err();
    assert!(
        matches!(error, StorageError::VisibilityCeiling { .. }),
        "got {error}"
    );

    disabled(|scope| operation.finish(scope.child("storage.finish"))).unwrap();
    let values = disabled(|scope| reopened.read_batch(&[second_root], scope.child("read")))
        .unwrap()
        .0;
    assert_eq!(values.len(), 1);
    assert_eq!(StoragePolicy::frozen_default().format_profile(), 1);

    let _ = SaveHandoff::new;
}
