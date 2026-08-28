use super::metrics::ScratchObservation;
use super::schema::{DISK_TABLE_CACHE_KIB, SCRATCH_MARKER, SCRATCH_SCHEMA};
use super::table::{DiskTable, SCRATCH_SERIAL};
use crate::error::{io_engine_error, map_sqlite_error, EngineError, EngineResult, SqliteErrorKind};
use crate::generation::StoreGenerationDriver;
use crate::sqlite::admission::schema_shape;
use crate::sqlite::connection::inspect_store_id_readonly;
use rusqlite::OpenFlags;
use rusqlite::{params, Connection};
use std::cell::Cell;
use std::fmt::Write as _;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Instant;
use std::time::{SystemTime, UNIX_EPOCH};

impl DiskTable {
    pub fn create_near(store: &Path, label: &str) -> EngineResult<Self> {
        let started = Instant::now();
        let store_id = inspect_store_id_readonly(store)?;
        let wall_ns = u64::try_from(started.elapsed().as_nanos())
            .map_err(|_| EngineError::CounterOverflow)?;
        Self::create_near_observed(store, label, store_id, 1, 11, wall_ns)
    }

    pub(crate) fn create_near_with_store_id(
        store: &Path,
        label: &str,
        store_id: [u8; 32],
    ) -> EngineResult<Self> {
        Self::create_near_observed(store, label, store_id, 0, 0, 0)
    }

    fn create_near_observed(
        store: &Path,
        label: &str,
        store_id: [u8; 32],
        store_reopens: u64,
        store_inspection_statements: u64,
        store_inspection_wall_ns: u64,
    ) -> EngineResult<Self> {
        let parent = store.parent().unwrap_or_else(|| Path::new("."));
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| EngineError::InvalidRecord("scratch clock"))?
            .as_nanos();
        let store_tag = hex_store_id(store_id);
        let path = parent.join(format!(
            ".layerfs-{store_tag}-{label}-{}-{stamp}-{}.sqlite",
            std::process::id(),
            SCRATCH_SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        Self::create_at(
            path,
            SCRATCH_SCHEMA,
            store_id,
            store_reopens,
            store_inspection_statements,
            store_inspection_wall_ns,
        )
    }

    pub(super) fn create_at(
        path: PathBuf,
        schema: &str,
        store_id: [u8; 32],
        store_reopens: u64,
        store_inspection_statements: u64,
        store_inspection_wall_ns: u64,
    ) -> EngineResult<Self> {
        let setup_started = Instant::now();
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| EngineError::Sqlite {
                kind: SqliteErrorKind::Io,
                message: error.to_string(),
            })?;
        let initialized =
            Connection::open(&path)
                .map_err(map_sqlite_error)
                .and_then(|connection| {
                    connection.execute_batch(schema).map_err(map_sqlite_error)?;
                    connection
                        .execute(
                            "INSERT INTO scratch_owner (id, format_marker, store_id)
                             VALUES (1, ?1, ?2)",
                            params![SCRATCH_MARKER, store_id.as_slice()],
                        )
                        .map_err(map_sqlite_error)?;
                    connection
                        .execute_batch("BEGIN IMMEDIATE")
                        .map_err(map_sqlite_error)?;
                    Ok(connection)
                });
        let connection = match initialized {
            Ok(connection) => connection,
            Err(error) => {
                cleanup_files(&path);
                return Err(error);
            }
        };
        let table = Self {
            path,
            connection: Some(connection),
            statements: Cell::new(10),
            owner_setup_statements: Cell::new(8),
            derived_setup_statements: Cell::new(2),
            operation_statements: Cell::new(0),
            store_reopens,
            store_inspection_statements,
            store_inspection_wall_ns,
            setup_wall_ns: 0,
            operation_wall_ns: Cell::new(0),
            rows: Cell::new(1),
            high_water_bytes: Cell::new(0),
        };
        table.set_cache_size_kib(DISK_TABLE_CACHE_KIB)?;
        let mut table = table;
        table.setup_wall_ns = u64::try_from(setup_started.elapsed().as_nanos())
            .map_err(|_| EngineError::CounterOverflow)?;
        table.observe_storage()?;
        Ok(table)
    }

    pub fn finish(mut self) -> EngineResult<ScratchObservation> {
        let rollback = self.finish_transaction();
        let observation = self.observation();
        rollback?;
        observation
    }
    fn finish_transaction(&mut self) -> EngineResult<()> {
        let Some(connection) = self.connection.take() else {
            return Ok(());
        };
        let rollback = connection
            .execute_batch("ROLLBACK")
            .map_err(map_sqlite_error);
        self.mark_derived_setup_statement()?;
        rollback
    }
}

impl Drop for DiskTable {
    fn drop(&mut self) {
        let _ = self.finish_transaction();
        cleanup_files(&self.path);
    }
}

pub(crate) fn recover_owned_near(
    store: &Path,
    store_id: [u8; 32],
    driver: &dyn StoreGenerationDriver,
) -> EngineResult<()> {
    let parent = store.parent().unwrap_or_else(|| Path::new("."));
    let expected_tag = hex_store_id(store_id);
    for entry in std::fs::read_dir(parent).map_err(io_engine_error)? {
        let entry = entry.map_err(io_engine_error)?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with(".layerfs-") || !name.ends_with(".sqlite") {
            continue;
        }
        let suffix = &name.as_bytes()[9..];
        if suffix.get(64) == Some(&b'-')
            && suffix[..64].iter().all(u8::is_ascii_hexdigit)
            && &suffix[..64] != expected_tag.as_bytes()
        {
            continue;
        }
        let path = entry.path();
        let Ok(identity) = driver.file_identity(&path) else {
            continue;
        };
        let journal = PathBuf::from(format!("{}-journal", path.display()));
        let journal_identity = driver.file_identity(&journal).ok();
        let Ok(connection) = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        else {
            continue;
        };
        if scratch_store_id(&connection).ok() != Some(store_id) {
            continue;
        }
        drop(connection);
        let connection = match Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE)
        {
            Ok(connection) => connection,
            Err(error) => {
                let error = map_sqlite_error(error);
                if matches!(
                    error,
                    EngineError::Sqlite {
                        kind: SqliteErrorKind::Busy | SqliteErrorKind::Locked,
                        ..
                    }
                ) {
                    continue;
                }
                return Err(error);
            }
        };
        connection
            .busy_timeout(std::time::Duration::ZERO)
            .map_err(map_sqlite_error)?;
        if scratch_store_id(&connection).ok() != Some(store_id) {
            continue;
        }
        if let Err(error) = connection.execute_batch("BEGIN IMMEDIATE") {
            let error = map_sqlite_error(error);
            if matches!(
                error,
                EngineError::Sqlite {
                    kind: SqliteErrorKind::Busy | SqliteErrorKind::Locked,
                    ..
                }
            ) {
                continue;
            }
            return Err(error);
        }
        connection
            .execute_batch("ROLLBACK")
            .map_err(map_sqlite_error)?;
        drop(connection);
        driver
            .remove_file_if_identity(&path, &identity)
            .map_err(io_engine_error)?;
        if let Some(identity) = journal_identity {
            driver
                .remove_file_if_identity(&journal, &identity)
                .map_err(io_engine_error)?;
        }
    }
    Ok(())
}

fn scratch_store_id(connection: &Connection) -> EngineResult<[u8; 32]> {
    if connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema WHERE name NOT GLOB 'sqlite_*'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(map_sqlite_error)?
        != 3
    {
        return Err(EngineError::SchemaMismatch);
    }
    for (kind, name, expected) in [
        (
            "table",
            "entries",
            "CREATE TABLE entries (
                key BLOB PRIMARY KEY,
                value BLOB NOT NULL,
                pending INTEGER NOT NULL CHECK (pending IN (0, 1))
             )",
        ),
        (
            "index",
            "entries_pending_key",
            "CREATE INDEX entries_pending_key ON entries (pending, key)",
        ),
        (
            "table",
            "scratch_owner",
            "CREATE TABLE scratch_owner (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                format_marker TEXT NOT NULL,
                store_id BLOB NOT NULL CHECK (length(store_id) = 32)
             )",
        ),
    ] {
        let actual = connection
            .query_row(
                "SELECT sql FROM sqlite_schema WHERE type = ?1 AND name = ?2",
                params![kind, name],
                |row| row.get::<_, String>(0),
            )
            .map_err(map_sqlite_error)?;
        if schema_shape(&actual) != schema_shape(expected) {
            return Err(EngineError::SchemaMismatch);
        }
    }
    if connection
        .query_row(
            "SELECT COUNT(*), MIN(id), MAX(id) FROM scratch_owner",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            },
        )
        .map_err(map_sqlite_error)?
        != (1, Some(1), Some(1))
    {
        return Err(EngineError::SchemaMismatch);
    }
    let (marker, store_id) = connection
        .query_row(
            "SELECT format_marker, store_id FROM scratch_owner WHERE id = 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )
        .map_err(map_sqlite_error)?;
    if marker != SCRATCH_MARKER {
        return Err(EngineError::SchemaMismatch);
    }
    store_id.try_into().map_err(|_| EngineError::SchemaMismatch)
}

fn cleanup_files(path: &Path) {
    let _ = std::fs::remove_file(path);
    let mut journal = path.as_os_str().to_os_string();
    journal.push("-journal");
    let _ = std::fs::remove_file(PathBuf::from(journal));
}

fn hex_store_id(store_id: [u8; 32]) -> String {
    let mut output = String::with_capacity(64);
    for byte in store_id {
        let _ = write!(output, "{byte:02x}");
    }
    output
}
