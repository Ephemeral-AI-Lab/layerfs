//! Real CAS round trip: fresh schema, supported FULL objects, close and reopen.

mod support;

use layerfs_content::{ObjectId, ObjectRole};
use layerfs_storage::{SchemaIdentity, StorageError, StoragePolicy, Store, SCHEMA_IDENTITY};
use support::{
    assembled_small_object, construct_file, create_store, disabled, noise, open_store, patterned,
    read_objects, repeat, save_all, save_one, TempDir,
};

/// The declared role is the producer's, and a disagreement is refused in the
/// role's own decoder rather than reinterpreted (contract: "Caller-declared
/// object role" in `admission-and-persistence.md`).
#[test]
fn a_disagreeing_role_declaration_is_refused_by_its_own_decoder() {
    use layerfs_content::filesystem::directory::codec::decode_directory_page;
    use layerfs_content::filesystem::inode::codec::{
        decode_inode_page, encode_inode_page, InodePage,
    };
    use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
    use layerfs_content::{FinalizedObject, ObjectId};

    let dir = TempDir::new("declared-role");
    let path = dir.store_path("declared-role");
    let store = create_store(&path);

    // A framed role has to find its own payload, so a declaration that disagrees
    // with the bytes is refused at admission instead of being stored.
    let whole = assembled_small_object(&patterned(2_048));
    let mismatched = FinalizedObject::new(ObjectRole::Chunk, whole).expect("canonical object");
    assert!(matches!(
        save_one(&store, mismatched),
        Err(StorageError::Integrity(
            "chunk role with a non-chunk payload"
        ))
    ));

    // An unframed tree role is stored verbatim, so the disagreement is persisted
    // as declared and refused by the declared role's decoder on the next read.
    let page = encode_inode_page(&InodePage::Leaf {
        entries: vec![(
            7,
            InodeValue {
                kind: InodeKind::Directory,
                namespace_ref_count: 1,
                content_root: ObjectId::for_bytes(b"declared-role/content"),
                metadata_root: ObjectId::for_bytes(b"declared-role/metadata"),
            },
        )],
    })
    .expect("inode leaf page");
    let leaf_id = ObjectId::for_bytes(&page);
    let declared =
        FinalizedObject::new(ObjectRole::DirectoryLeaf, page.clone()).expect("canonical object");
    save_one(&store, declared).expect("an unframed declaration is stored as declared");
    drop(store);

    let reopened = open_store(&path);
    let (values, _) = read_objects(&reopened, &[leaf_id]).expect("read succeeds");
    assert_eq!(values[0], page, "the stored bytes are the declared object");
    assert!(
        decode_inode_page(&values[0]).is_ok(),
        "the object's own grammar reads it"
    );
    assert!(
        decode_directory_page(&values[0]).is_err(),
        "the declared role's decoder refuses it instead of reinterpreting"
    );
}

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

#[test]
fn a_read_wave_is_bounded_by_the_declared_ceiling() {
    // A read wave is one grouped query plus one decode workspace. Its size is a
    // declared capacity, so a caller cannot turn one call into unbounded decode
    // work by handing over a longer slice: the demand is refused before a
    // connection is opened.
    let dir = TempDir::new("read-bound");
    let path = dir.store_path("read-bound");
    let store = create_store(&path);
    let limit = store.capacities().read_objects;
    assert!(limit > 0, "the ceiling is declared: {limit}");
    let within = vec![ObjectId::for_bytes(b"not-stored"); limit];
    // A demand at the ceiling is answered as far as it goes: the objects are not
    // there, and that is a read result, not a capacity refusal.
    assert!(
        !matches!(
            disabled(|scope| store.read_batch(&within, scope.child("storage.read"))),
            Err(StorageError::CapacityExceeded { .. })
        ),
        "a demand at the declared ceiling is not refused as oversized"
    );
    let over = vec![ObjectId::for_bytes(b"not-stored"); limit + 1];
    match disabled(|scope| store.read_batch(&over, scope.child("storage.read"))) {
        Err(StorageError::CapacityExceeded {
            what,
            limit: refused,
            actual,
        }) => {
            assert_eq!(what, "storage.read_objects");
            assert_eq!(refused, limit as u64);
            assert_eq!(actual, (limit + 1) as u64);
        }
        other => panic!("one object over the ceiling must be refused: {other:?}"),
    }
}

#[test]
fn a_pooled_session_opens_one_connection_for_every_wave() {
    // P1-2: the operation's session, not the wave, owns the connection and the
    // decode arena. Three waves through one provider are one open, and the
    // per-wave counter reports the open only on the wave that made it.
    let dir = TempDir::new("pooled-session");
    let path = dir.store_path("pooled-session");
    let store = create_store(&path);
    let (collected, root, _) = construct_file(&patterned(64));
    save_all(&store, &collected).unwrap();

    let provider = layerfs_storage::StoreProvider::new(&store);
    let mut opens = Vec::new();
    for _ in 0..3 {
        let (values, counters) =
            disabled(|scope| provider.read_wave(&[root], scope.child("storage.read")))
                .expect("wave");
        assert_eq!(values.len(), 1);
        opens.push(counters.opens);
    }
    assert_eq!(opens, vec![1, 0, 0], "only the first wave opens");
    assert_eq!(
        provider.connection_opens(),
        1,
        "one session, one connection"
    );

    // The Store's own call is unchanged: a direct `read_batch` still opens its
    // own connection per call, because pooling is the provider's per-operation
    // state and not a property of the shared Store.
    let (_values, counters) = read_objects(&store, &[root]).expect("direct read");
    assert_eq!(counters.opens, 1, "a direct read wave still opens its own");
}

#[test]
fn an_oversized_provider_demand_is_refused_before_the_session_opens() {
    // The wave's declared bound survives the pooling: a demand over
    // `read_objects` is refused, and it is refused **before** the session exists,
    // so a refused call opens no connection. Without this check the pooled path
    // would reach `read_objects` without the Store's own entry-point guard.
    let dir = TempDir::new("provider-demand");
    let path = dir.store_path("provider-demand");
    let store = create_store(&path);
    let (collected, root, _) = construct_file(&patterned(64));
    save_all(&store, &collected).unwrap();
    let limit = store.capacities().read_objects;
    let provider = layerfs_storage::StoreProvider::new(&store);
    let over = vec![ObjectId::for_bytes(b"not-stored"); limit + 1];
    match disabled(|scope| provider.read_wave(&over, scope.child("storage.read"))) {
        Err(StorageError::CapacityExceeded { what, .. }) => {
            assert_eq!(what, "storage.read_objects");
        }
        other => panic!("an oversized provider demand must be refused: {other:?}"),
    }
    assert_eq!(
        provider.connection_opens(),
        0,
        "the refusal happens before any connection is opened"
    );
    // And a served demand still goes through the session.
    let (values, counters) =
        disabled(|scope| provider.read_wave(&[root], scope.child("storage.read")))
            .expect("a demand at the ceiling is served");
    assert_eq!(values.len(), 1);
    assert_eq!(counters.opens, 1);
    assert_eq!(provider.connection_opens(), 1);
}

#[test]
fn a_pooled_session_sees_a_save_that_completed_between_its_waves() {
    // The one thing a session must NOT pool: the visibility ceiling. It is the
    // publication watermark, so a save that completed between two waves is
    // visible to the second one. A session that captured the ceiling once would
    // refuse this read with a visibility error.
    let dir = TempDir::new("session-ceiling");
    let path = dir.store_path("session-ceiling");
    let store = create_store(&path);
    let (first, first_root, _) = construct_file(&patterned(2_048));
    save_all(&store, &first).unwrap();

    let provider = layerfs_storage::StoreProvider::new(&store);
    let (values, counters) =
        disabled(|scope| provider.read_wave(&[first_root], scope.child("storage.read")))
            .expect("first wave");
    assert_eq!(values.len(), 1);
    let first_ceiling = counters.ceiling;

    let (second, second_root, _) = construct_file(&patterned(4_096));
    save_all(&store, &second).unwrap();

    let (values, counters) =
        disabled(|scope| provider.read_wave(&[second_root], scope.child("storage.read")))
            .expect("a save between the waves is visible to the second one");
    assert_eq!(values.len(), 1);
    assert!(
        counters.ceiling > first_ceiling,
        "the ceiling advanced across the save: {first_ceiling} -> {}",
        counters.ceiling
    );
    assert_eq!(
        provider.connection_opens(),
        1,
        "both waves still share the session's connection"
    );
}

#[test]
fn a_read_wave_reports_the_connection_it_opened() {
    // The read wave's own connection counter (#178 V3): one `read_batch` call is
    // one wave and opens exactly one connection, whether it carries one id or a
    // grouped demand. The counter exists so P1-2's pooled session has a before
    // value: today an operation's connections are the sum over its waves.
    let dir = TempDir::new("read-opens");
    let path = dir.store_path("read-opens");
    let store = create_store(&path);
    let (collected, root, _) = construct_file(&patterned(64));
    save_all(&store, &collected).unwrap();

    let (values, counters) = read_objects(&store, &[root]).expect("read");
    assert_eq!(values.len(), 1);
    assert_eq!(counters.opens, 1, "one wave opens one connection");

    // A grouped demand is still one wave: many ids, one call, one open.
    let grouped = vec![root; 8];
    let (values, counters) = read_objects(&store, &grouped).expect("grouped read");
    assert_eq!(values.len(), 8);
    assert_eq!(counters.opens, 1, "a grouped demand is one connection");

    // The product bridge carries the same figure, summed over the waves it
    // issued. The assertion is a BOUND, not an equality, on purpose: today two
    // waves are two connections, and P1-2 is authorized to pool them into one, so
    // an equality here would have to be re-pinned by that item. What must hold
    // both before and after pooling is that the counter is not zero (a wave that
    // read something opened a connection) and never exceeds the waves issued.
    let provider = layerfs_storage::StoreProvider::new(&store);
    assert_eq!(provider.connection_opens(), 0, "no wave yet, no connection");
    for _ in 0..2 {
        disabled(|scope| provider.read_wave(&[root], scope.child("storage.read"))).expect("wave");
    }
    let opens = provider.connection_opens();
    assert!(
        (1..=2).contains(&opens),
        "two waves open one or two connections, not {opens}"
    );
}

#[test]
fn an_old_same_version_store_is_refused_at_open() {
    // The role ceiling is the one constraint that changes without changing the
    // column shape. A Store written with a narrower one was accepted at open and
    // failed only at the first tree-role INSERT; the text is now required, so the
    // refusal happens where the caller can see it.
    let dir = TempDir::new("old-role-ceiling");
    let path = dir.store_path("old-role-ceiling");
    let store = create_store(&path);
    drop(store);
    {
        let connection = rusqlite::Connection::open(&path).expect("raw connection");
        connection
            .execute_batch(
                "PRAGMA foreign_keys = OFF;\
                 DROP TABLE objects;\
                 CREATE TABLE objects (\
                     object_id BLOB NOT NULL PRIMARY KEY CHECK (length(object_id) = 32),\
                     object_role INTEGER NOT NULL CHECK (object_role BETWEEN 1 AND 6),\
                     canonical_length INTEGER NOT NULL CHECK (canonical_length > 0 AND canonical_length <= 16777216),\
                     base_object_id BLOB CHECK (base_object_id IS NULL OR (length(base_object_id) = 32 AND base_object_id <> object_id)) REFERENCES objects(object_id) ON DELETE NO ACTION,\
                     pack_id INTEGER NOT NULL REFERENCES object_packs(pack_id),\
                     group_number INTEGER NOT NULL CHECK (group_number >= 0 AND group_number < 256),\
                     record_number INTEGER NOT NULL CHECK (record_number >= 0 AND record_number < 8191)\
                 ) STRICT, WITHOUT ROWID;\
                 CREATE UNIQUE INDEX objects_locations ON objects(pack_id, group_number, record_number);",
            )
            .expect("rewrite with the previous role ceiling");
    }
    match disabled(|scope| Store::open(&path, scope.child("store"))) {
        Err(StorageError::UnsupportedPolicy { field }) => {
            assert_eq!(field, "object role constraint");
        }
        other => panic!("an old role ceiling must be refused at open: {other:?}"),
    }
}
