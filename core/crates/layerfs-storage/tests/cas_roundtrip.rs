//! Real CAS round trip: fresh schema, supported FULL objects, close and reopen.

mod support;

use layerfs_content::ObjectRole;
use layerfs_storage::{SchemaIdentity, StorageError, StoragePolicy, Store, SCHEMA_IDENTITY};
use support::{
    construct_file, create_store, disabled, noise, open_store, patterned, read_objects, repeat,
    save_all, TempDir,
};

#[test]
fn fresh_store_writes_the_declared_schema_identity() {
    let dir = TempDir::new("schema");
    let path = dir.store_path("fresh");
    let store = create_store(&path);
    assert_eq!(store.policy(), StoragePolicy::frozen_default());
    assert_eq!(
        SCHEMA_IDENTITY,
        SchemaIdentity {
            application_id: 1_279_677_261,
            user_version: 4,
        }
    );
    drop(store);
    assert!(path.exists());
    let reopened = open_store(&path);
    assert_eq!(reopened.policy(), StoragePolicy::frozen_default());
}

#[test]
fn whole_file_object_survives_close_and_reopen() {
    let dir = TempDir::new("whole");
    let path = dir.store_path("whole");
    let bytes = patterned(3_000);
    let (collected, root, logical_len) = construct_file(&bytes);
    assert_eq!(logical_len, bytes.len() as u64);
    assert_eq!(collected.objects().len(), 1);

    let store = create_store(&path);
    let outcome = save_all(&store, &collected).expect("save succeeds");
    assert_eq!(outcome.inserted, 1);
    assert_eq!(outcome.reused, 0);
    assert_eq!(outcome.packs_created, 1);
    drop(store);

    let reopened = open_store(&path);
    let (values, counters) = read_objects(&reopened, &[root]).expect("read succeeds");
    assert_eq!(counters.objects, 1);
    assert_eq!(counters.packs_read, 1);
    assert_eq!(values.len(), 1);
    assert_eq!(
        values[0],
        collected.objects()[0].2,
        "canonical bytes are identical after reopen"
    );
}

#[test]
fn every_initial_role_round_trips() {
    let dir = TempDir::new("roles");
    let path = dir.store_path("roles");
    let body = noise(131_072 * 2 + 17);
    let (collected, root, logical_len) = construct_file(&body);
    assert_eq!(logical_len, body.len() as u64);
    let roles: Vec<ObjectRole> = collected
        .objects()
        .iter()
        .map(|(_, role, _, _)| *role)
        .collect();
    assert!(roles.contains(&ObjectRole::FileState));
    assert!(roles.contains(&ObjectRole::Chunk));
    assert!(roles.contains(&ObjectRole::ExtentLeaf));

    let store = create_store(&path);
    let outcome = save_all(&store, &collected).expect("save succeeds");
    assert_eq!(outcome.inserted, collected.objects().len() as u64);
    drop(store);

    let reopened = open_store(&path);
    let ids: Vec<_> = collected
        .objects()
        .iter()
        .map(|(id, _, _, _)| *id)
        .collect();
    let (values, _) = read_objects(&reopened, &ids).expect("read succeeds");
    for (index, (_, _, expected, _)) in collected.objects().iter().enumerate() {
        assert_eq!(&values[index], expected, "object {index}");
    }
    let (state, _) = read_objects(&reopened, &[root]).expect("root read");
    assert_eq!(state.len(), 1);
}

#[test]
fn compressible_and_incompressible_payloads_round_trip() {
    for body in [repeat(40_000, 0x5a), noise(40_000)] {
        let dir = TempDir::new("payload");
        let path = dir.store_path("payload");
        let (collected, root, _) = construct_file(&body);
        let store = create_store(&path);
        save_all(&store, &collected).expect("save succeeds");
        drop(store);
        let reopened = open_store(&path);
        let (values, _) = read_objects(&reopened, &[root]).expect("read succeeds");
        assert_eq!(values[0], collected.objects()[0].2);
    }
}

#[test]
fn malformed_store_identity_is_rejected() {
    let dir = TempDir::new("malformed");
    let path = dir.store_path("malformed");
    {
        let store = create_store(&path);
        assert_eq!(store.policy(), StoragePolicy::frozen_default());
    }
    // Rewrite the schema identity the way another product's Store would look.
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch("PRAGMA application_id = 1279677260; PRAGMA user_version = 10;")
        .unwrap();
    drop(connection);
    let error = disabled(|scope| Store::open(&path, scope.child("store"))).unwrap_err();
    assert!(
        matches!(error, StorageError::UnsupportedPolicy { field } if field == "schema identity"),
        "unexpected error: {error}"
    );
}

#[test]
fn missing_object_is_a_definite_failure() {
    let dir = TempDir::new("missing");
    let path = dir.store_path("missing");
    let store = create_store(&path);
    let (collected, root, _) = construct_file(&patterned(64));
    save_all(&store, &collected).unwrap();
    let absent = layerfs_content::ObjectId::for_bytes(b"never stored");
    let error = disabled(|scope| store.read_batch(&[absent], scope.child("read"))).unwrap_err();
    assert!(matches!(error, StorageError::ObjectMissing(_)));
    assert_ne!(absent, root);
}
