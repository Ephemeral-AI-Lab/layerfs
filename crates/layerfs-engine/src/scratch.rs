use super::{map_sqlite_error, EngineError, EngineResult};
use rusqlite::types::ValueRef;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static SCRATCH_SERIAL: AtomicU64 = AtomicU64::new(0);
const SCRATCH_SCHEMA: &str = "PRAGMA journal_mode=DELETE;
                 PRAGMA synchronous=FULL;
                 PRAGMA temp_store=FILE;
                 PRAGMA mmap_size=0;
                 PRAGMA busy_timeout=0;
                 CREATE TABLE entries (
                    key BLOB PRIMARY KEY,
                    value BLOB NOT NULL,
                    pending INTEGER NOT NULL CHECK (pending IN (0, 1))
                 );
                 CREATE INDEX entries_pending_key ON entries (pending, key);
                 CREATE TABLE scratch_owner (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    format_marker TEXT NOT NULL,
                    store_id BLOB NOT NULL CHECK (length(store_id) = 32)
                 );";
const SCRATCH_MARKER: &str = "layerfs-owned-scratch-v1";

pub struct DiskTable {
    path: PathBuf,
    connection: Option<Connection>,
}

impl DiskTable {
    pub fn create_near(store: &Path, label: &str) -> EngineResult<Self> {
        let store_id = crate::inspect_store_id_readonly(store)?;
        let parent = store.parent().unwrap_or_else(|| Path::new("."));
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| EngineError::InvalidRecord("scratch clock"))?
            .as_nanos();
        let path = parent.join(format!(
            ".layerfs-{label}-{}-{stamp}-{}.sqlite",
            std::process::id(),
            SCRATCH_SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        Self::create_at(path, SCRATCH_SCHEMA, store_id)
    }

    fn create_at(path: PathBuf, schema: &str, store_id: [u8; 32]) -> EngineResult<Self> {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| EngineError::Sqlite {
                kind: super::SqliteErrorKind::Io,
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
        Ok(Self {
            path,
            connection: Some(connection),
        })
    }

    pub fn get(&self, key: &[u8]) -> EngineResult<Option<Vec<u8>>> {
        self.connection()
            .query_row(
                "SELECT value FROM entries WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(map_sqlite_error)
    }

    pub(crate) fn storage_bytes(&self) -> EngineResult<u64> {
        let database = std::fs::metadata(&self.path)
            .map_err(|error| EngineError::Sqlite {
                kind: super::SqliteErrorKind::Io,
                message: error.to_string(),
            })?
            .len();
        let mut journal = self.path.as_os_str().to_os_string();
        journal.push("-journal");
        Ok(database.saturating_add(
            std::fs::metadata(PathBuf::from(journal))
                .map(|value| value.len())
                .unwrap_or(0),
        ))
    }

    pub fn put(&self, key: &[u8], value: &[u8]) -> EngineResult<()> {
        self.connection()
            .execute(
                "INSERT INTO entries (key, value, pending) VALUES (?1, ?2, 0)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, pending = 0",
                params![key, value],
            )
            .map_err(map_sqlite_error)?;
        Ok(())
    }

    pub fn remove(&self, key: &[u8]) -> EngineResult<()> {
        self.connection()
            .execute("DELETE FROM entries WHERE key = ?1", params![key])
            .map_err(map_sqlite_error)?;
        Ok(())
    }

    pub fn enqueue_once(&self, key: &[u8], payload: &[u8]) -> EngineResult<()> {
        let existing = self.get(key)?;
        match existing {
            Some(existing) if existing == payload => Ok(()),
            Some(_) => Err(EngineError::InvalidRecord("scratch role conflict")),
            None => {
                self.connection()
                    .execute(
                        "INSERT INTO entries (key, value, pending) VALUES (?1, ?2, 1)",
                        params![key, payload],
                    )
                    .map_err(map_sqlite_error)?;
                Ok(())
            }
        }
    }

    pub fn pop_pending(&self) -> EngineResult<Option<(Vec<u8>, Vec<u8>)>> {
        let row = self
            .connection()
            .query_row(
                "SELECT key, value FROM entries WHERE pending = 1 ORDER BY key LIMIT 1",
                [],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()
            .map_err(map_sqlite_error)?;
        let Some((key, value)) = row else {
            return Ok(None);
        };
        self.connection()
            .execute(
                "UPDATE entries SET pending = 0 WHERE key = ?1",
                params![&key],
            )
            .map_err(map_sqlite_error)?;
        Ok(Some((key, value)))
    }

    pub fn pop_pending_prefix(&self, prefix: &[u8; 8]) -> EngineResult<Option<(Vec<u8>, Vec<u8>)>> {
        let row = self
            .connection()
            .query_row(
                "SELECT key, value FROM entries
                 WHERE pending = 1 AND substr(key, 1, 8) = ?1
                 ORDER BY key LIMIT 1",
                params![prefix.as_slice()],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()
            .map_err(map_sqlite_error)?;
        let Some((key, value)) = row else {
            return Ok(None);
        };
        self.connection()
            .execute(
                "UPDATE entries SET pending = 0 WHERE key = ?1",
                params![&key],
            )
            .map_err(map_sqlite_error)?;
        Ok(Some((key, value)))
    }

    pub fn for_each(
        &self,
        mut callback: impl FnMut(&[u8]) -> EngineResult<()>,
    ) -> EngineResult<()> {
        let mut statement = self
            .connection()
            .prepare("SELECT value FROM entries ORDER BY key")
            .map_err(map_sqlite_error)?;
        let mut rows = statement.query([]).map_err(map_sqlite_error)?;
        while let Some(row) = rows.next().map_err(map_sqlite_error)? {
            let value = match row.get_ref(0).map_err(map_sqlite_error)? {
                ValueRef::Blob(value) => value,
                _ => return Err(EngineError::InvalidRecord("scratch value")),
            };
            callback(value)?;
        }
        Ok(())
    }

    pub fn for_each_key(
        &self,
        mut callback: impl FnMut(&[u8]) -> EngineResult<()>,
    ) -> EngineResult<()> {
        let mut statement = self
            .connection()
            .prepare("SELECT key FROM entries ORDER BY key")
            .map_err(map_sqlite_error)?;
        let mut rows = statement.query([]).map_err(map_sqlite_error)?;
        while let Some(row) = rows.next().map_err(map_sqlite_error)? {
            let key = match row.get_ref(0).map_err(map_sqlite_error)? {
                ValueRef::Blob(key) => key,
                _ => return Err(EngineError::InvalidRecord("scratch key")),
            };
            callback(key)?;
        }
        Ok(())
    }

    fn connection(&self) -> &Connection {
        self.connection.as_ref().expect("scratch connection closed")
    }
}

impl Drop for DiskTable {
    fn drop(&mut self) {
        if let Some(connection) = self.connection.take() {
            let _ = connection.execute_batch("ROLLBACK");
        }
        cleanup_files(&self.path);
    }
}

pub(crate) fn recover_owned_near(
    store: &Path,
    store_id: [u8; 32],
    driver: &dyn crate::generation::StoreGenerationDriver,
) -> EngineResult<()> {
    let parent = store.parent().unwrap_or_else(|| Path::new("."));
    for entry in std::fs::read_dir(parent).map_err(super::io_engine_error)? {
        let entry = entry.map_err(super::io_engine_error)?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with(".layerfs-") || !name.ends_with(".sqlite") {
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
                        kind: crate::SqliteErrorKind::Busy | crate::SqliteErrorKind::Locked,
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
                    kind: crate::SqliteErrorKind::Busy | crate::SqliteErrorKind::Locked,
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
            .map_err(super::io_engine_error)?;
        if let Some(identity) = journal_identity {
            driver
                .remove_file_if_identity(&journal, &identity)
                .map_err(super::io_engine_error)?;
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
        if super::schema_shape(&actual) != super::schema_shape(expected) {
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

#[cfg(test)]
mod tests {
    use super::*;

    struct ScratchDriver;
    impl crate::generation::StoreGenerationDriver for ScratchDriver {
        fn available_bytes(&self, _directory: &Path) -> std::io::Result<u64> {
            Ok(u64::MAX)
        }
        fn install_selector(&self, prepared: &Path, current: &Path) -> std::io::Result<()> {
            std::fs::rename(prepared, current)
        }
        fn sync_directory(&self, directory: &Path) -> std::io::Result<()> {
            std::fs::File::open(directory)?.sync_all()
        }
        fn file_identity(&self, path: &Path) -> std::io::Result<Vec<u8>> {
            Ok(path.as_os_str().to_string_lossy().into_owned().into_bytes())
        }
        fn remove_file_if_identity(&self, path: &Path, _expected: &[u8]) -> std::io::Result<()> {
            match std::fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error),
            }
        }
    }

    #[test]
    fn drop_closes_connection_and_removes_exact_owned_files() {
        let anchor =
            std::env::temp_dir().join(format!("layerfs-scratch-anchor-{}", std::process::id()));
        let engine = crate::Engine::open(&anchor).unwrap();
        let path = {
            let table = DiskTable::create_near(&anchor, "cleanup").unwrap();
            table.put(b"key", b"value").unwrap();
            assert_eq!(table.get(b"key").unwrap(), Some(b"value".to_vec()));
            table.enqueue_once(b"prefix01-b", b"second").unwrap();
            table.enqueue_once(b"prefix02-a", b"other").unwrap();
            table.enqueue_once(b"prefix01-a", b"first").unwrap();
            assert_eq!(
                table.pop_pending_prefix(b"prefix01").unwrap(),
                Some((b"prefix01-a".to_vec(), b"first".to_vec()))
            );
            assert_eq!(
                table.pop_pending_prefix(b"prefix02").unwrap(),
                Some((b"prefix02-a".to_vec(), b"other".to_vec()))
            );
            assert!(table.path.exists());
            table.path.clone()
        };
        assert!(!path.exists());
        let mut journal = path.into_os_string();
        journal.push("-journal");
        assert!(!PathBuf::from(journal).exists());
        drop(engine);
        std::fs::remove_file(anchor).unwrap();
    }

    #[test]
    fn constructor_failure_removes_exact_owned_files() {
        let path = std::env::temp_dir().join(format!(
            ".layerfs-scratch-failed-{}-{}.sqlite",
            std::process::id(),
            SCRATCH_SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        assert!(DiskTable::create_at(path.clone(), "not valid sqlite", [0; 32]).is_err());
        assert!(!path.exists());
        let mut journal = path.into_os_string();
        journal.push("-journal");
        assert!(!PathBuf::from(journal).exists());
    }

    #[test]
    fn exclusive_recovery_removes_only_authenticated_unlocked_crash_scratch() {
        let store = std::env::temp_dir().join(format!(
            "layerfs-scratch-recovery-{}-{}.sqlite",
            std::process::id(),
            SCRATCH_SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let engine = crate::Engine::open(&store).unwrap();
        let mut table = DiskTable::create_near(&store, "crash").unwrap();
        let scratch = table.path.clone();
        let foreign = scratch.with_file_name(format!(
            ".layerfs-foreign-{}-{}.sqlite",
            std::process::id(),
            SCRATCH_SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&foreign, b"foreign").unwrap();
        recover_owned_near(&store, engine.store_id().unwrap(), &ScratchDriver).unwrap();
        assert!(scratch.exists(), "reopen removed live scratch");

        table
            .connection
            .take()
            .unwrap()
            .execute_batch("ROLLBACK")
            .unwrap();
        std::mem::forget(table);
        recover_owned_near(&store, engine.store_id().unwrap(), &ScratchDriver).unwrap();
        assert!(!scratch.exists(), "reopen retained stale owned scratch");
        assert!(
            foreign.exists(),
            "reopen removed foreign scratch-shaped file"
        );

        std::fs::remove_file(foreign).unwrap();
        drop(engine);
        std::fs::remove_file(store).unwrap();
    }

    #[test]
    fn recovery_preserves_same_marker_impostor_schema() {
        let store = std::env::temp_dir().join(format!(
            "layerfs-scratch-impostor-store-{}-{}.sqlite",
            std::process::id(),
            SCRATCH_SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let engine = crate::Engine::open(&store).unwrap();
        let impostor = store.with_file_name(format!(
            ".layerfs-impostor-{}-{}.sqlite",
            std::process::id(),
            SCRATCH_SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let connection = Connection::open(&impostor).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE entries (key BLOB PRIMARY KEY, value BLOB, pending INTEGER);
                 CREATE INDEX entries_pending_key ON entries (pending, key);
                 CREATE TABLE scratch_owner (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    format_marker TEXT NOT NULL,
                    store_id BLOB NOT NULL CHECK (length(store_id) = 32)
                 );",
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO scratch_owner VALUES (1, ?1, ?2)",
                params![SCRATCH_MARKER, engine.store_id().unwrap().as_slice()],
            )
            .unwrap();
        drop(connection);
        let before = std::fs::read(&impostor).unwrap();
        recover_owned_near(&store, engine.store_id().unwrap(), &ScratchDriver).unwrap();
        assert_eq!(std::fs::read(&impostor).unwrap(), before);
        std::fs::remove_file(impostor).unwrap();
        drop(engine);
        std::fs::remove_file(store).unwrap();
    }
}
