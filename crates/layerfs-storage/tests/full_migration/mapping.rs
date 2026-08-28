use super::fixture::{object_row, path, publish_valid_root, remove, schema_counts, BYTES_KIND};
use layerfs_storage::{
    migration::migrate_legacy_durable_file, DeltaRecord, Engine, FullStorage, StoreRole,
};
use rusqlite::{params, Connection};

#[test]
fn empty_legacy_durable_migrates_side_by_side_without_changing_the_source() {
    let source = path("empty-source");
    let candidate = path("empty-candidate");
    let engine = Engine::open(&source).unwrap();
    let storage_id = engine.store_id().unwrap();
    drop(engine);

    let full = migrate_legacy_durable_file(&source, &candidate, storage_id).unwrap();
    assert_eq!(full.role(), StoreRole::Durable);
    assert_eq!(full.storage_id(), storage_id);
    assert_eq!(full.durable_storage_id(), storage_id);
    assert_eq!(schema_counts(&candidate), (21, 4));
    drop(full);

    let source_reopened = Engine::open(&source).unwrap();
    assert_eq!(source_reopened.store_id().unwrap(), storage_id);
    drop(source_reopened);
    let candidate_reopened = FullStorage::open_durable(&candidate).unwrap();
    assert_eq!(candidate_reopened.storage_id(), storage_id);
    drop(candidate_reopened);

    remove(&source);
    remove(&candidate);
}

#[test]
fn populated_legacy_rows_map_exactly_without_changing_the_source() {
    let source = path("populated-source");
    let candidate = path("populated-candidate");
    let engine = Engine::open(&source).unwrap();
    let storage_id = engine.store_id().unwrap();
    let (object_id, canonical) = publish_valid_root(&engine, "migration-source", [0x51; 32]);
    drop(engine);

    let delta = DeltaRecord::new(None, object_id, b"format-v1 delta".to_vec());
    let connection = Connection::open(&source).unwrap();
    connection.execute_batch("BEGIN IMMEDIATE").unwrap();
    connection
        .execute(
            "INSERT INTO layerfs_deltas
             (delta_id, format_version, parent_root, child_root, payload)
             VALUES (?1, 1, NULL, ?2, ?3)",
            params![
                delta.id.as_bytes().as_slice(),
                object_id.as_bytes().as_slice(),
                delta.payload
            ],
        )
        .unwrap();
    connection.execute("DELETE FROM layerfs_refs", []).unwrap();
    connection.execute_batch("COMMIT").unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT (SELECT count(*) FROM layerfs_refs)
                      + (SELECT count(*) FROM layerfs_roots)",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
    drop(connection);
    let source_before = std::fs::read(&source).unwrap();
    let expected_object = object_row(&source, object_id.as_bytes());

    let full = migrate_legacy_durable_file(&source, &candidate, storage_id).unwrap();
    drop(full);
    assert_eq!(std::fs::read(&source).unwrap(), source_before);

    for database in [&source, &candidate] {
        let connection = Connection::open(database).unwrap();
        assert_eq!(object_row(database, object_id.as_bytes()), expected_object);
        assert_eq!(expected_object.1, object_id.as_bytes());
        assert_eq!(expected_object.2, BYTES_KIND);
        assert_eq!(expected_object.3, i64::try_from(canonical.len()).unwrap());
        assert_eq!(expected_object.4, canonical);
        assert_eq!(
            connection
                .query_row(
                    "SELECT count(*) FROM layerfs_retained_roots WHERE root_id = ?1",
                    params![object_id.as_bytes().as_slice()],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
    }

    let source_connection = Connection::open(&source).unwrap();
    let source_delta = source_connection
        .query_row(
            "SELECT format_version, parent_root, child_root, payload
             FROM layerfs_deltas WHERE delta_id = ?1",
            params![delta.id.as_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                ))
            },
        )
        .unwrap();
    drop(source_connection);
    let candidate_connection = Connection::open(&candidate).unwrap();
    let candidate_delta = candidate_connection
        .query_row(
            "SELECT format_version, parent_root_id, result_root_id, payload
             FROM layerfs_deltas WHERE delta_id = ?1",
            params![delta.id.as_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                ))
            },
        )
        .unwrap();
    drop(candidate_connection);
    assert_eq!(candidate_delta, source_delta);

    let source_reopened = Engine::open(&source).unwrap();
    assert_eq!(source_reopened.store_id().unwrap(), storage_id);
    drop(source_reopened);
    let candidate_reopened = FullStorage::open_durable(&candidate).unwrap();
    assert_eq!(candidate_reopened.storage_id(), storage_id);
    drop(candidate_reopened);

    remove(&source);
    remove(&candidate);
}
