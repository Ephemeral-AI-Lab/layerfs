//! Exact reuse, repeated identities and collision rejection.

mod support;

use layerfs_content::ObjectRole;
use layerfs_storage::{StorageError, Store};
use support::{
    construct_file, create_store, disabled, noise, open_store, patterned, read_objects, repeat,
    save_all, TempDir,
};

fn tamper_pack(path: &std::path::Path) {
    let connection = rusqlite::Connection::open(path).expect("external connection");
    let (pack_id, mut data): (i64, Vec<u8>) = connection
        .query_row(
            "SELECT pack_id, data FROM object_packs LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("a stored pack");
    let last = data.len() - 1;
    data[last] ^= 0x01;
    let affected = connection
        .execute(
            "UPDATE object_packs SET data = ?2 WHERE pack_id = ?1",
            rusqlite::params![pack_id, data],
        )
        .expect("external tamper");
    assert_eq!(affected, 1);
}

#[test]
fn resaving_the_same_object_reuses_its_row_without_a_new_pack() {
    let dir = TempDir::new("reuse");
    let path = dir.store_path("reuse");
    let bytes = patterned(5_000);
    let (collected, root, _) = construct_file(&bytes);
    let store = create_store(&path);

    let first = save_all(&store, &collected).expect("first save");
    assert_eq!(first.inserted, 1);
    assert_eq!(first.packs_created, 1);

    let second = save_all(&store, &collected).expect("second save");
    assert_eq!(second.inserted, 0, "nothing new was written");
    assert_eq!(
        second.reused, 1,
        "the occurrence was served by the existing row"
    );
    assert_eq!(second.packs_created, 0);
    assert_eq!(second.commits, 0, "an all-reuse finish issues no COMMIT");

    let (values, counters) = read_objects(&store, &[root]).unwrap();
    assert_eq!(counters.packs_read, 1);
    assert_eq!(values[0], collected.objects()[0].2);
}

#[test]
fn repeated_identities_inside_one_batch_insert_once_and_reuse_the_rest() {
    let dir = TempDir::new("repeat");
    let path = dir.store_path("repeat");
    // Identical chunks make one payload identity appear many times in one batch.
    let mut bytes = Vec::new();
    for _ in 0..40 {
        bytes.extend_from_slice(&repeat(8_192, 0x77));
    }
    let (collected, root, _) = construct_file(&bytes);
    let distinct: std::collections::BTreeSet<_> = collected
        .objects()
        .iter()
        .map(|(id, _, _, _)| *id)
        .collect();
    assert!(
        distinct.len() < collected.objects().len(),
        "the tree references the same chunk more than once"
    );

    let store = create_store(&path);
    let outcome = save_all(&store, &collected).expect("save succeeds");
    assert_eq!(outcome.inserted, distinct.len() as u64);
    assert_eq!(
        outcome.reused,
        (collected.objects().len() - distinct.len()) as u64
    );

    let (values, _) = read_objects(&store, &[root]).unwrap();
    assert_eq!(values.len(), 1);
}

#[test]
fn repeated_identity_across_batches_reuses_without_rewriting() {
    let dir = TempDir::new("across");
    let path = dir.store_path("across");
    let bytes = noise(131_072);
    let (collected, root, _) = construct_file(&bytes);
    let store = create_store(&path);
    let first = save_all(&store, &collected).unwrap();
    assert!(first.inserted >= 2);

    let chunk = collected
        .objects()
        .iter()
        .find(|(_, role, _, _)| *role == ObjectRole::Chunk)
        .map(|(id, _, _, _)| *id)
        .expect("a chunk");
    let outcome = disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        operation.accept(
            collected
                .finalized()
                .into_iter()
                .find(|object| object.id() == chunk)
                .unwrap(),
            scope.child("storage.accept"),
        )?;
        operation.finish(scope.child("storage.finish"))
    })
    .expect("second save");
    assert_eq!(outcome.inserted, 0);
    assert_eq!(outcome.reused, 1);
    assert_eq!(outcome.commits, 0);
    let (values, _) = read_objects(&store, &[root, chunk]).unwrap();
    assert_eq!(values.len(), 2);
}

#[test]
fn a_corrupted_stored_record_is_rejected_as_a_collision() {
    let dir = TempDir::new("collision");
    let path = dir.store_path("collision");
    let bytes = patterned(6_000);
    let (collected, _, _) = construct_file(&bytes);
    let store = create_store(&path);
    save_all(&store, &collected).expect("first save");
    drop(store);
    tamper_pack(&path);

    let reopened = open_store(&path);
    let error = save_all(&reopened, &collected).unwrap_err();
    assert!(
        matches!(
            error,
            StorageError::Collision(_) | StorageError::Integrity(_)
        ),
        "unexpected error: {error}"
    );
}

#[test]
fn a_stored_row_with_a_short_length_is_rejected_without_reading_it() {
    let dir = TempDir::new("short");
    let path = dir.store_path("short");
    let bytes = patterned(6_000);
    let (collected, root, _) = construct_file(&bytes);
    let store = create_store(&path);
    save_all(&store, &collected).expect("first save");
    drop(store);

    let connection = rusqlite::Connection::open(&path).unwrap();
    let affected = connection
        .execute(
            "UPDATE objects SET canonical_length = canonical_length - 1 WHERE object_id = ?1",
            rusqlite::params![root.to_bytes().to_vec()],
        )
        .unwrap();
    assert_eq!(affected, 1);
    drop(connection);

    let reopened = open_store(&path);
    let error = save_all(&reopened, &collected).unwrap_err();
    assert!(matches!(error, StorageError::Collision(_)), "got {error}");
}

#[test]
fn one_operation_can_save_two_files_into_shared_packs() {
    let dir = TempDir::new("shared");
    let path = dir.store_path("shared");
    let first = repeat(60_000, 0x11);
    let second = repeat(60_000, 0x22);
    let (first_objects, first_root, _) = construct_file(&first);
    let (second_objects, second_root, _) = construct_file(&second);
    assert_eq!(first_objects.objects().len(), 1);
    assert_eq!(second_objects.objects().len(), 1);

    let store = create_store(&path);
    let outcome = disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        for object in first_objects
            .finalized()
            .into_iter()
            .chain(second_objects.finalized())
        {
            operation.accept(object, scope.child("storage.accept"))?;
        }
        operation.finish(scope.child("storage.finish"))
    })
    .expect("save succeeds");
    assert_eq!(outcome.inserted, 2);
    assert_eq!(outcome.packs_created, 1, "both objects share one pack");

    let (values, counters) = read_objects(&store, &[first_root, second_root]).unwrap();
    assert_eq!(counters.packs_read, 1);
    assert_eq!(values[0], first_objects.objects()[0].2);
    assert_eq!(values[1], second_objects.objects()[0].2);
}

#[test]
fn a_directory_is_never_treated_as_a_store() {
    let dir = TempDir::new("directory");
    let error = disabled(|scope| Store::open(dir.path(), scope.child("store"))).unwrap_err();
    assert!(matches!(error, StorageError::Engine(_)), "got {error}");
}
