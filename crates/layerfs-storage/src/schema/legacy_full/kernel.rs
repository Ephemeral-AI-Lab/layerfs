pub(crate) const LEGACY_SCHEMA_VERSION: i64 = 1;
pub(crate) const SCHEMA_VERSION: i64 = 2;
pub const TRANSITION_FORMAT_VERSION: i64 = 1;
pub(crate) const FORMAT_MARKER: &str = "layerfs-phase4a-sqlite-blob";

pub(crate) const BASE_SCHEMAS: [(&str, &str); 7] = [
    (
        "layerfs_store_meta",
        "CREATE TABLE IF NOT EXISTS layerfs_store_meta (
            store_id INTEGER PRIMARY KEY CHECK (store_id = 1),
            format_marker TEXT NOT NULL,
            schema_version INTEGER NOT NULL,
            journal_mode TEXT NOT NULL,
            synchronous INTEGER NOT NULL,
            temp_store INTEGER NOT NULL,
            mmap_size INTEGER NOT NULL,
            visible_root BLOB
        )",
    ),
    (
        "layerfs_objects",
        "CREATE TABLE IF NOT EXISTS layerfs_objects (
            rowid INTEGER PRIMARY KEY,
            object_id BLOB NOT NULL UNIQUE,
            kind INTEGER NOT NULL,
            canonical_length INTEGER NOT NULL,
            canonical_bytes BLOB NOT NULL
        )",
    ),
    (
        "layerfs_roots",
        "CREATE TABLE IF NOT EXISTS layerfs_roots (
            root_id BLOB PRIMARY KEY,
            directory_object BLOB NOT NULL,
            parent_root BLOB
        )",
    ),
    (
        "layerfs_deltas",
        "CREATE TABLE IF NOT EXISTS layerfs_deltas (
            delta_id BLOB PRIMARY KEY,
            format_version INTEGER NOT NULL,
            parent_root BLOB,
            child_root BLOB NOT NULL,
            payload BLOB NOT NULL
        )",
    ),
    (
        "layerfs_authority",
        "CREATE TABLE IF NOT EXISTS layerfs_authority (
            authority_id INTEGER PRIMARY KEY CHECK (authority_id = 1),
            store_id BLOB NOT NULL CHECK (length(store_id) = 32),
            next_inode_serial INTEGER NOT NULL,
            trusted_history INTEGER NOT NULL CHECK (trusted_history IN (0, 1))
        )",
    ),
    (
        "layerfs_refs",
        "CREATE TABLE IF NOT EXISTS layerfs_refs (
            name TEXT PRIMARY KEY,
            generation INTEGER NOT NULL,
            root_id BLOB NOT NULL CHECK (length(root_id) = 32)
        )",
    ),
    (
        "layerfs_retained_roots",
        "CREATE TABLE IF NOT EXISTS layerfs_retained_roots (
            root_id BLOB PRIMARY KEY CHECK (length(root_id) = 32)
        )",
    ),
];

pub(crate) const LEGACY_DELTA_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS layerfs_deltas (
    delta_id BLOB PRIMARY KEY,
    parent_root BLOB,
    child_root BLOB NOT NULL,
    payload BLOB NOT NULL
)";

use crate::{map_sqlite_error, EngineError, EngineResult, SchemaState, SqliteProfile};
use rusqlite::{params, Connection};

pub(crate) fn initialize_schema_counted(
    connection: &Connection,
    profile: &SqliteProfile,
    state: SchemaState,
    statements: &mut u64,
) -> EngineResult<()> {
    match state {
        SchemaState::Empty => create_current_schema(connection, profile, statements),
        SchemaState::Legacy => migrate_legacy_schema(connection, statements),
        SchemaState::Current => Ok(()),
    }
}

fn create_current_schema(
    connection: &Connection,
    profile: &SqliteProfile,
    statements: &mut u64,
) -> EngineResult<()> {
    note_statement(statements)?;
    connection
        .execute_batch("BEGIN EXCLUSIVE")
        .map_err(map_sqlite_error)?;
    let result = (|| {
        for (_, sql) in BASE_SCHEMAS {
            note_statement(statements)?;
            connection.execute_batch(sql).map_err(map_sqlite_error)?;
        }
        for &(_, sql) in super::PRODUCT_SCHEMAS.into_iter().flatten() {
            note_statement(statements)?;
            connection.execute_batch(sql).map_err(map_sqlite_error)?;
        }
        note_statement(statements)?;
        connection
            .execute(
                "INSERT INTO layerfs_store_meta
                 (store_id, format_marker, schema_version, journal_mode, synchronous, temp_store, mmap_size)
                 VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    FORMAT_MARKER,
                    SCHEMA_VERSION,
                    &profile.journal_mode,
                    profile.synchronous,
                    profile.temp_store,
                    profile.mmap_size,
                ],
            )
            .map_err(map_sqlite_error)?;
        note_statement(statements)?;
        let store_id = connection
            .query_row("SELECT randomblob(32)", [], |row| row.get::<_, Vec<u8>>(0))
            .map_err(map_sqlite_error)?;
        note_statement(statements)?;
        connection
            .execute(
                "INSERT INTO layerfs_authority
                 (authority_id, store_id, next_inode_serial, trusted_history)
                 VALUES (1, ?1, 0, 0)",
                params![store_id.as_slice()],
            )
            .map_err(map_sqlite_error)?;
        note_statement(statements)?;
        connection.execute_batch("COMMIT").map_err(map_sqlite_error)
    })();
    if result.is_err() {
        let _ = connection.execute_batch("ROLLBACK");
    }
    result
}

fn migrate_legacy_schema(connection: &Connection, statements: &mut u64) -> EngineResult<()> {
    note_statement(statements)?;
    connection
        .execute_batch("BEGIN EXCLUSIVE")
        .map_err(map_sqlite_error)?;
    let result = (|| {
        *statements = statements
            .checked_add(4)
            .ok_or(EngineError::CounterOverflow)?;
        connection
            .execute_batch(
                "ALTER TABLE layerfs_deltas RENAME TO layerfs_deltas_v1;
                 CREATE TABLE layerfs_deltas (
                    delta_id BLOB PRIMARY KEY,
                    format_version INTEGER NOT NULL,
                    parent_root BLOB,
                    child_root BLOB NOT NULL,
                    payload BLOB NOT NULL
                 );
                 INSERT INTO layerfs_deltas
                    (delta_id, format_version, parent_root, child_root, payload)
                    SELECT delta_id, 0, parent_root, child_root, payload
                    FROM layerfs_deltas_v1;
                 DROP TABLE layerfs_deltas_v1;",
            )
            .map_err(map_sqlite_error)?;
        for &(_, sql) in super::PRODUCT_SCHEMAS.into_iter().flatten() {
            note_statement(statements)?;
            connection.execute_batch(sql).map_err(map_sqlite_error)?;
        }
        note_statement(statements)?;
        let store_id = connection
            .query_row(
                "SELECT store_id FROM layerfs_authority WHERE authority_id = 1",
                [],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .map_err(map_sqlite_error)?;
        let store_id: [u8; 32] = store_id
            .try_into()
            .map_err(|_| EngineError::SchemaMismatch)?;
        note_statement(statements)?;
        let mut select = connection
            .prepare("SELECT name, generation, root_id FROM layerfs_refs ORDER BY name")
            .map_err(map_sqlite_error)?;
        let mut rows = select.query([]).map_err(map_sqlite_error)?;
        while let Some(row) = rows.next().map_err(map_sqlite_error)? {
            let name = row.get::<_, String>(0).map_err(map_sqlite_error)?;
            let generation = row.get::<_, i64>(1).map_err(map_sqlite_error)?;
            let root_id = row.get::<_, Vec<u8>>(2).map_err(map_sqlite_error)?;
            if generation < 0 || root_id.len() != 32 {
                return Err(EngineError::SchemaMismatch);
            }
            let stack_id = migrated_id(store_id, b"legacy-layer-stack", &name, &root_id);
            let layer_id = migrated_id(store_id, b"legacy-genesis-layer", &name, &root_id);
            note_statement(statements)?;
            connection
                .execute(
                    "INSERT INTO layerfs_layer_stacks
                     (layer_stack_id, name, generation, head_layer_id)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![stack_id.as_slice(), &name, generation, layer_id.as_slice()],
                )
                .map_err(map_sqlite_error)?;
            note_statement(statements)?;
            connection
                .execute(
                    "INSERT INTO layerfs_layers
                     (layer_id, layer_stack_id, root_id, creation_kind, state, accepted_generation)
                     VALUES (?1, ?2, ?3, 'genesis', 'accepted', ?4)",
                    params![
                        layer_id.as_slice(),
                        stack_id.as_slice(),
                        root_id.as_slice(),
                        generation,
                    ],
                )
                .map_err(map_sqlite_error)?;
        }
        drop(rows);
        drop(select);
        note_statement(statements)?;
        connection
            .execute(
                "UPDATE layerfs_store_meta SET schema_version = ?1 WHERE store_id = 1",
                params![SCHEMA_VERSION],
            )
            .map_err(map_sqlite_error)?;
        note_statement(statements)?;
        connection.execute_batch("COMMIT").map_err(map_sqlite_error)
    })();
    if result.is_err() {
        let _ = connection.execute_batch("ROLLBACK");
    }
    result
}

fn migrated_id(store_id: [u8; 32], domain: &[u8], name: &str, root_id: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs-storage-migration-v2\0");
    hasher.update(domain);
    hasher.update(&store_id);
    hasher.update(&(name.len() as u64).to_be_bytes());
    hasher.update(name.as_bytes());
    hasher.update(root_id);
    *hasher.finalize().as_bytes()
}

pub(crate) fn admitted_store_id_counted(
    connection: &Connection,
    statements: &mut u64,
) -> EngineResult<[u8; 32]> {
    note_statement(statements)?;
    connection
        .query_row(
            "SELECT store_id FROM layerfs_authority WHERE authority_id = 1",
            [],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .map_err(map_sqlite_error)?
        .try_into()
        .map_err(|_| EngineError::InvalidRecord("StoreId"))
}

pub(crate) fn note_statement(statements: &mut u64) -> EngineResult<()> {
    *statements = statements
        .checked_add(1)
        .ok_or(EngineError::CounterOverflow)?;
    Ok(())
}
