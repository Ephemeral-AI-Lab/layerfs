use super::*;
use crate::sqlite::connection::read_ref_reconcile_readonly;
use std::path::PathBuf;

#[test]
fn sqlite_error_mapping_preserves_required_classes() {
    for (code, expected) in [
        (rusqlite::ErrorCode::DatabaseBusy, SqliteErrorKind::Busy),
        (rusqlite::ErrorCode::DatabaseLocked, SqliteErrorKind::Locked),
        (
            rusqlite::ErrorCode::PermissionDenied,
            SqliteErrorKind::PermissionDenied,
        ),
        (rusqlite::ErrorCode::DiskFull, SqliteErrorKind::NoSpace),
        (
            rusqlite::ErrorCode::DatabaseCorrupt,
            SqliteErrorKind::Corrupt,
        ),
        (rusqlite::ErrorCode::ReadOnly, SqliteErrorKind::ReadOnly),
        (
            rusqlite::ErrorCode::ConstraintViolation,
            SqliteErrorKind::Constraint,
        ),
        (rusqlite::ErrorCode::SystemIoFailure, SqliteErrorKind::Io),
    ] {
        assert!(matches!(
            map_sqlite_error(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error {
                    code,
                    extended_code: 0,
                },
                None,
            )),
            EngineError::Sqlite { kind, .. } if kind == expected
        ));
    }
}

#[test]
fn admission_never_mutates_foreign_or_incomplete_databases() {
    let foreign = test_path();
    let connection = Connection::open(&foreign).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE foreign_data (value TEXT); INSERT INTO foreign_data VALUES ('keep');",
        )
        .unwrap();
    drop(connection);
    let before = fs::read(&foreign).unwrap();
    assert!(matches!(
        Engine::open(&foreign),
        Err(EngineError::SchemaMismatch)
    ));
    assert_eq!(fs::read(&foreign).unwrap(), before);
    let connection = Connection::open(&foreign).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT value FROM foreign_data", [], |row| row
                .get::<_, String>(0))
            .unwrap(),
        "keep"
    );
    drop(connection);
    fs::remove_file(&foreign).unwrap();

    for table in ["layerfs_store_meta", "layerfs_authority"] {
        let path = test_path();
        drop(Engine::open(&path).unwrap());
        let connection = Connection::open(&path).unwrap();
        connection
            .execute(&format!("DELETE FROM {table}"), [])
            .unwrap();
        drop(connection);
        let before = fs::read(&path).unwrap();
        assert!(matches!(
            Engine::open(&path),
            Err(EngineError::SchemaMismatch)
        ));
        assert_eq!(fs::read(&path).unwrap(), before, "opening replaced {table}");
        let connection = Connection::open(&path).unwrap();
        let count = connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();
        assert_eq!(count, 0);
        drop(connection);
        fs::remove_file(path).unwrap();
    }

    let impostor = test_path();
    drop(Engine::open(&impostor).unwrap());
    let connection = Connection::open(&impostor).unwrap();
    connection
        .execute_batch(
            "ALTER TABLE layerfs_authority RENAME TO saved_authority;
                 CREATE TABLE layerfs_authority (
                    authority_id INTEGER PRIMARY KEY,
                    store_id BLOB NOT NULL,
                    next_inode_serial INTEGER NOT NULL,
                    trusted_history INTEGER NOT NULL
                 );
                 INSERT INTO layerfs_authority SELECT * FROM saved_authority;
                 DROP TABLE saved_authority;",
        )
        .unwrap();
    let authority = connection
            .query_row(
                "SELECT authority_id, store_id, next_inode_serial, trusted_history FROM layerfs_authority",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .unwrap();
    drop(connection);
    let before = fs::read(&impostor).unwrap();
    assert!(matches!(
        Engine::open(&impostor),
        Err(EngineError::SchemaMismatch)
    ));
    assert_eq!(fs::read(&impostor).unwrap(), before);
    let connection = Connection::open(&impostor).unwrap();
    let after = connection
            .query_row(
                "SELECT authority_id, store_id, next_inode_serial, trusted_history FROM layerfs_authority",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .unwrap();
    assert_eq!(after, authority);
    drop(connection);
    fs::remove_file(impostor).unwrap();

    let escaped = test_path();
    drop(Engine::open(&escaped).unwrap());
    let connection = Connection::open(&escaped).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE sqliteX_data (value TEXT);
                 INSERT INTO sqliteX_data VALUES ('keep');
                 CREATE TRIGGER sqliteX_trigger AFTER INSERT ON sqliteX_data
                 BEGIN UPDATE sqliteX_data SET value = value; END;",
        )
        .unwrap();
    drop(connection);
    let before = fs::read(&escaped).unwrap();
    assert!(matches!(
        Engine::open(&escaped),
        Err(EngineError::SchemaMismatch)
    ));
    assert_eq!(fs::read(&escaped).unwrap(), before);
    let connection = Connection::open(&escaped).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT value FROM sqliteX_data", [], |row| {
                row.get::<_, String>(0)
            })
            .unwrap(),
        "keep"
    );
    drop(connection);
    fs::remove_file(escaped).unwrap();
}

#[test]
fn reconciliation_read_never_creates_missing_or_accepts_replaced_store() {
    let path = test_path();
    let original = Engine::open(&path).unwrap();
    let store_id = original.store_id().unwrap();
    let saved = path.with_extension("saved");
    fs::rename(&path, &saved).unwrap();
    assert!(read_ref_reconcile_readonly(&original, "main", store_id).is_err());
    assert!(
        !path.exists(),
        "read-only reconciliation created a database"
    );

    let replacement = Engine::open(&path).unwrap();
    assert_ne!(replacement.store_id().unwrap(), store_id);
    assert_eq!(original.store_id().unwrap(), store_id);
    drop(replacement);
    assert!(matches!(
        read_ref_reconcile_readonly(&original, "main", store_id),
        Err(EngineError::InvalidRecord("reconciliation StoreId"))
    ));
    drop(original);
    fs::remove_file(path).unwrap();
    fs::remove_file(saved).unwrap();
}

#[test]
fn read_only_admission_preserves_foreign_hot_journal_bytes() {
    let path = test_path();
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "PRAGMA journal_mode=DELETE;
                 CREATE TABLE foreign_table (value TEXT NOT NULL);
                 INSERT INTO foreign_table VALUES ('prior');",
        )
        .unwrap();
    drop(connection);
    let database_before = fs::read(&path).unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "tests::profile::foreign_hot_journal_child"])
        .env("LAYERFS_FOREIGN_HOT_JOURNAL", &path)
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(92));
    let journal = PathBuf::from(format!("{}-journal", path.display()));
    let journal_before = fs::read(&journal).unwrap();
    assert!(Engine::open(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), database_before);
    assert_eq!(fs::read(&journal).unwrap(), journal_before);
    fs::remove_file(journal).unwrap();
    fs::remove_file(path).unwrap();
}

#[test]
fn two_verified_snapshot_readers_coexist() {
    let path = test_path();
    let first = Engine::open(&path).unwrap();
    let second = Engine::open(&path).unwrap();
    let guard = first.lock_connection().unwrap();
    guard
        .query_row(
            "SELECT trusted_history FROM layerfs_authority WHERE authority_id = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap();
    assert_eq!(second.read_ref("main").unwrap(), None);
    drop(guard);
    drop(first);
    drop(second);
    fs::remove_file(path).unwrap();
}

#[test]
fn v1_migration_preserves_identity_and_creates_only_genesis_history() {
    let path = test_path();
    let connection = Connection::open(&path).unwrap();
    for (name, sql) in schema::BASE_SCHEMAS {
        connection
            .execute_batch(if name == "layerfs_deltas" {
                schema::LEGACY_DELTA_SCHEMA
            } else {
                sql
            })
            .unwrap();
    }
    let storage_id = [0x71_u8; 32];
    let root_id = ObjectId::for_bytes(b"migrated root");
    let directory_id = ObjectId::for_bytes(b"migrated directory");
    let legacy_delta = DeltaRecord::new(None, root_id, b"legacy bytes".to_vec());
    let delta_id = legacy_delta.id;
    connection
        .execute(
            "INSERT INTO layerfs_store_meta
                 (store_id, format_marker, schema_version, journal_mode,
                  synchronous, temp_store, mmap_size)
                 VALUES (1, ?1, 1, 'delete', 2, 1, 0)",
            params![FORMAT_MARKER],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO layerfs_authority
                 (authority_id, store_id, next_inode_serial, trusted_history)
                 VALUES (1, ?1, 9, 0)",
            params![storage_id.as_slice()],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO layerfs_roots (root_id, directory_object, parent_root)
                 VALUES (?1, ?2, NULL)",
            params![
                root_id.as_bytes().as_slice(),
                directory_id.as_bytes().as_slice()
            ],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO layerfs_deltas (delta_id, parent_root, child_root, payload)
                 VALUES (?1, NULL, ?2, ?3)",
            params![
                delta_id.as_bytes().as_slice(),
                root_id.as_bytes().as_slice(),
                legacy_delta.payload.as_slice()
            ],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO layerfs_refs (name, generation, root_id)
                 VALUES ('legacy-main', 7, ?1)",
            params![root_id.as_bytes().as_slice()],
        )
        .unwrap();
    drop(connection);

    let engine = Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev)
        .expect("migrate v1");
    assert_eq!(engine.store_id().unwrap(), storage_id);
    assert_eq!(
        engine.read_ref("legacy-main").unwrap().unwrap(),
        refs::RefState {
            name: "legacy-main".to_owned(),
            generation: 7,
            root: root_id
        }
    );
    assert_eq!(
        engine.load_delta(delta_id).unwrap().payload,
        b"legacy bytes"
    );
    drop(engine);

    let connection = Connection::open(&path).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT schema_version FROM layerfs_store_meta WHERE store_id = 1",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        SCHEMA_VERSION
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT format_version FROM layerfs_deltas WHERE delta_id = ?1",
                params![delta_id.as_bytes().as_slice()],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM layerfs_layer_stacks", [], |row| row
                .get::<_, i64>(
                0
            ))
            .unwrap(),
        1
    );
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM layerfs_layers", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM layerfs_operations", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    fs::remove_file(path).unwrap();
}
