use super::fixture::{path, remove};
use layerfs_storage::{migration::migrate_legacy_durable_file, Engine};
use rusqlite::{params, Connection};

#[test]
fn wrong_expected_store_id_rejects_without_creating_a_candidate() {
    let source = path("wrong-id-source");
    let candidate = path("wrong-id-candidate");
    let engine = Engine::open(&source).unwrap();
    let storage_id = engine.store_id().unwrap();
    drop(engine);

    let mut wrong_id = storage_id;
    wrong_id[0] ^= 0xff;
    assert!(migrate_legacy_durable_file(&source, &candidate, wrong_id).is_err());
    assert!(!candidate.exists());
    let source_reopened = Engine::open(&source).unwrap();
    assert_eq!(source_reopened.store_id().unwrap(), storage_id);
    drop(source_reopened);

    remove(&source);
}

#[test]
fn forbidden_legacy_generic_state_rejects_without_creating_a_candidate() {
    let source = path("generic-source");
    let candidate = path("generic-candidate");
    let engine = Engine::open(&source).unwrap();
    let storage_id = engine.store_id().unwrap();
    drop(engine);
    let connection = Connection::open(&source).unwrap();
    connection
        .execute(
            "INSERT INTO layerfs_refs (name, generation, root_id) VALUES ('legacy', 0, ?1)",
            params![[0x51_u8; 32]],
        )
        .unwrap();
    drop(connection);

    assert!(migrate_legacy_durable_file(&source, &candidate, storage_id).is_err());
    assert!(!candidate.exists());
    assert_eq!(
        Connection::open(&source)
            .unwrap()
            .query_row("SELECT count(*) FROM layerfs_refs", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );

    remove(&source);
}

#[test]
fn source_foreign_key_failure_rejects_without_changing_the_source() {
    let source = path("transform-failure-source");
    let candidate = path("transform-failure-candidate");
    let engine = Engine::open(&source).unwrap();
    let storage_id = engine.store_id().unwrap();
    drop(engine);
    let delta_id = [0x71_u8; 32];
    let missing_result = [0x72_u8; 32];
    let connection = Connection::open(&source).unwrap();
    connection.execute_batch("PRAGMA foreign_keys=OFF").unwrap();
    connection
        .execute(
            "INSERT INTO layerfs_deltas
             (delta_id, format_version, parent_root, child_root, payload)
             VALUES (?1, 1, NULL, ?2, X'73')",
            params![delta_id, missing_result],
        )
        .unwrap();
    drop(connection);
    let source_before = std::fs::read(&source).unwrap();

    assert!(migrate_legacy_durable_file(&source, &candidate, storage_id).is_err());
    assert_eq!(std::fs::read(&source).unwrap(), source_before);
    assert!(!candidate.exists());
    assert_eq!(
        Connection::open(&source)
            .unwrap()
            .query_row(
                "SELECT format_version, parent_root, child_root, payload
                 FROM layerfs_deltas WHERE delta_id = ?1",
                params![delta_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<Vec<u8>>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                    ))
                },
            )
            .unwrap(),
        (1, None, missing_result.to_vec(), vec![0x73])
    );

    remove(&source);
}
