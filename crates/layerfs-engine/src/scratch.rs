use super::{map_sqlite_error, EngineError, EngineResult};
use rusqlite::types::ValueRef;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use std::cell::Cell;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static SCRATCH_SERIAL: AtomicU64 = AtomicU64::new(0);
const DISK_TABLE_CACHE_KIB: u32 = 256;
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
    statements: Cell<u64>,
    rows: Cell<u64>,
    high_water_bytes: Cell<u64>,
}

pub struct DiskNamespace<'a> {
    table: &'a DiskTable,
    prefix: Vec<u8>,
    upper: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScratchObservation {
    pub tables: u64,
    pub statements: u64,
    pub rows: u64,
    pub high_water_bytes: u64,
}

impl DiskTable {
    pub fn create_near(store: &Path, label: &str) -> EngineResult<Self> {
        let store_id = crate::inspect_store_id_readonly(store)?;
        Self::create_near_with_store_id(store, label, store_id)
    }

    pub(crate) fn create_near_with_store_id(
        store: &Path,
        label: &str,
        store_id: [u8; 32],
    ) -> EngineResult<Self> {
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
        let table = Self {
            path,
            connection: Some(connection),
            statements: Cell::new(10),
            rows: Cell::new(1),
            high_water_bytes: Cell::new(0),
        };
        table.set_cache_size_kib(DISK_TABLE_CACHE_KIB)?;
        table.observe_storage()?;
        Ok(table)
    }

    pub fn get(&self, key: &[u8]) -> EngineResult<Option<Vec<u8>>> {
        self.mark_statement()?;
        let value = self
            .connection()
            .query_row(
                "SELECT value FROM entries WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(map_sqlite_error)?;
        self.mark_rows(u64::from(value.is_some()))?;
        Ok(value)
    }

    pub fn storage_bytes(&self) -> EngineResult<u64> {
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

    pub fn set_cache_size_kib(&self, kib: u32) -> EngineResult<()> {
        if kib == 0 {
            return Err(EngineError::InvalidRecord("scratch cache size"));
        }
        self.mark_statement()?;
        let page_size = self
            .connection()
            .query_row("PRAGMA page_size", [], |row| row.get::<_, i64>(0))
            .map_err(map_sqlite_error)?;
        let page_size = u64::try_from(page_size)
            .ok()
            .filter(|size| *size != 0)
            .ok_or(EngineError::InvalidRecord("scratch page size"))?;
        let pages = u64::from(kib)
            .checked_mul(1024)
            .ok_or(EngineError::CounterOverflow)?
            .div_ceil(page_size);
        let pages = i64::try_from(pages).map_err(|_| EngineError::CounterOverflow)?;
        self.mark_statement()?;
        let cache_size = self
            .connection()
            .query_row("PRAGMA cache_size", [], |row| row.get::<_, i64>(0))
            .map_err(map_sqlite_error)?;
        self.mark_statement()?;
        let cache_spill = self
            .connection()
            .query_row("PRAGMA cache_spill", [], |row| row.get::<_, i64>(0))
            .map_err(map_sqlite_error)?;
        if cache_size == pages && cache_spill == pages {
            return Ok(());
        }
        if cache_size != pages {
            self.mark_statement()?;
            self.connection()
                .pragma_update(None, "cache_size", pages)
                .map_err(map_sqlite_error)?;
        }
        if cache_spill != pages {
            self.mark_statement()?;
            self.connection()
                .pragma_update(None, "cache_spill", pages)
                .map_err(map_sqlite_error)?;
        }
        self.mark_statement()?;
        let cache_size = self
            .connection()
            .query_row("PRAGMA cache_size", [], |row| row.get::<_, i64>(0))
            .map_err(map_sqlite_error)?;
        self.mark_statement()?;
        let cache_spill = self
            .connection()
            .query_row("PRAGMA cache_spill", [], |row| row.get::<_, i64>(0))
            .map_err(map_sqlite_error)?;
        if cache_size != pages || cache_spill != pages {
            return Err(EngineError::InvalidRecord("scratch cache profile"));
        }
        Ok(())
    }

    pub fn namespace<'a>(&'a self, name: &[u8]) -> EngineResult<DiskNamespace<'a>> {
        let name_len = u16::try_from(name.len())
            .map_err(|_| EngineError::InvalidRecord("scratch namespace"))?;
        let mut prefix = Vec::with_capacity(8 + name.len());
        prefix.extend_from_slice(b"LFSNS\0");
        prefix.extend_from_slice(&name_len.to_be_bytes());
        prefix.extend_from_slice(name);
        let upper = prefix_upper_bound(&prefix)?;
        Ok(DiskNamespace {
            table: self,
            prefix,
            upper,
        })
    }

    pub fn put(&self, key: &[u8], value: &[u8]) -> EngineResult<()> {
        self.mark_statement()?;
        let rows = self
            .connection()
            .execute(
                "INSERT INTO entries (key, value, pending) VALUES (?1, ?2, 0)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, pending = 0",
                params![key, value],
            )
            .map_err(map_sqlite_error)?;
        self.mark_rows(rows as u64)?;
        self.observe_storage()?;
        Ok(())
    }

    pub fn remove(&self, key: &[u8]) -> EngineResult<()> {
        self.mark_statement()?;
        let rows = self
            .connection()
            .execute("DELETE FROM entries WHERE key = ?1", params![key])
            .map_err(map_sqlite_error)?;
        self.mark_rows(rows as u64)?;
        self.observe_storage()?;
        Ok(())
    }

    pub fn enqueue_once(&self, key: &[u8], payload: &[u8]) -> EngineResult<()> {
        let existing = self.get(key)?;
        match existing {
            Some(existing) if existing == payload => Ok(()),
            Some(_) => Err(EngineError::InvalidRecord("scratch role conflict")),
            None => {
                self.mark_statement()?;
                let rows = self
                    .connection()
                    .execute(
                        "INSERT INTO entries (key, value, pending) VALUES (?1, ?2, 1)",
                        params![key, payload],
                    )
                    .map_err(map_sqlite_error)?;
                self.mark_rows(rows as u64)?;
                self.observe_storage()?;
                Ok(())
            }
        }
    }

    pub fn pop_pending(&self) -> EngineResult<Option<(Vec<u8>, Vec<u8>)>> {
        self.mark_statement()?;
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
        self.mark_rows(1)?;
        self.mark_statement()?;
        let rows = self
            .connection()
            .execute(
                "UPDATE entries SET pending = 0 WHERE key = ?1",
                params![&key],
            )
            .map_err(map_sqlite_error)?;
        self.mark_rows(rows as u64)?;
        self.observe_storage()?;
        Ok(Some((key, value)))
    }

    pub fn pop_pending_prefix(&self, prefix: &[u8; 8]) -> EngineResult<Option<(Vec<u8>, Vec<u8>)>> {
        self.mark_statement()?;
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
        self.mark_rows(1)?;
        self.mark_statement()?;
        let rows = self
            .connection()
            .execute(
                "UPDATE entries SET pending = 0 WHERE key = ?1",
                params![&key],
            )
            .map_err(map_sqlite_error)?;
        self.mark_rows(rows as u64)?;
        self.observe_storage()?;
        Ok(Some((key, value)))
    }

    pub fn for_each(
        &self,
        mut callback: impl FnMut(&[u8]) -> EngineResult<()>,
    ) -> EngineResult<()> {
        self.mark_statement()?;
        let mut statement = self
            .connection()
            .prepare("SELECT value FROM entries ORDER BY key")
            .map_err(map_sqlite_error)?;
        let mut rows = statement.query([]).map_err(map_sqlite_error)?;
        while let Some(row) = rows.next().map_err(map_sqlite_error)? {
            self.mark_rows(1)?;
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
        self.mark_statement()?;
        let mut statement = self
            .connection()
            .prepare("SELECT key FROM entries ORDER BY key")
            .map_err(map_sqlite_error)?;
        let mut rows = statement.query([]).map_err(map_sqlite_error)?;
        while let Some(row) = rows.next().map_err(map_sqlite_error)? {
            self.mark_rows(1)?;
            let key = match row.get_ref(0).map_err(map_sqlite_error)? {
                ValueRef::Blob(key) => key,
                _ => return Err(EngineError::InvalidRecord("scratch key")),
            };
            callback(key)?;
        }
        Ok(())
    }

    pub fn for_each_entry(
        &self,
        mut callback: impl FnMut(&[u8], &[u8]) -> EngineResult<()>,
    ) -> EngineResult<()> {
        self.mark_statement()?;
        let mut statement = self
            .connection()
            .prepare("SELECT key, value FROM entries ORDER BY key")
            .map_err(map_sqlite_error)?;
        let mut rows = statement.query([]).map_err(map_sqlite_error)?;
        while let Some(row) = rows.next().map_err(map_sqlite_error)? {
            self.mark_rows(1)?;
            let key = match row.get_ref(0).map_err(map_sqlite_error)? {
                ValueRef::Blob(value) => value,
                _ => return Err(EngineError::InvalidRecord("scratch key")),
            };
            let value = match row.get_ref(1).map_err(map_sqlite_error)? {
                ValueRef::Blob(value) => value,
                _ => return Err(EngineError::InvalidRecord("scratch value")),
            };
            callback(key, value)?;
        }
        Ok(())
    }

    pub fn for_each_entry_prefix(
        &self,
        prefix: &[u8],
        mut callback: impl FnMut(&[u8], &[u8]) -> EngineResult<()>,
    ) -> EngineResult<()> {
        self.mark_statement()?;
        let mut statement = self
            .connection()
            .prepare(
                "SELECT key, value FROM entries
                 WHERE substr(key, 1, length(?1)) = ?1 ORDER BY key",
            )
            .map_err(map_sqlite_error)?;
        let mut rows = statement.query(params![prefix]).map_err(map_sqlite_error)?;
        while let Some(row) = rows.next().map_err(map_sqlite_error)? {
            self.mark_rows(1)?;
            let key = match row.get_ref(0).map_err(map_sqlite_error)? {
                ValueRef::Blob(value) => value,
                _ => return Err(EngineError::InvalidRecord("scratch key")),
            };
            let value = match row.get_ref(1).map_err(map_sqlite_error)? {
                ValueRef::Blob(value) => value,
                _ => return Err(EngineError::InvalidRecord("scratch value")),
            };
            callback(key, value)?;
        }
        Ok(())
    }

    pub fn observation(&self) -> EngineResult<ScratchObservation> {
        self.observe_storage()?;
        Ok(ScratchObservation {
            tables: 1,
            statements: self.statements.get(),
            rows: self.rows.get(),
            high_water_bytes: self.high_water_bytes.get(),
        })
    }

    fn mark_statement(&self) -> EngineResult<()> {
        self.statements.set(
            self.statements
                .get()
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)?,
        );
        Ok(())
    }

    fn mark_rows(&self, rows: u64) -> EngineResult<()> {
        self.rows.set(
            self.rows
                .get()
                .checked_add(rows)
                .ok_or(EngineError::CounterOverflow)?,
        );
        Ok(())
    }

    fn observe_storage(&self) -> EngineResult<()> {
        self.high_water_bytes
            .set(self.high_water_bytes.get().max(self.storage_bytes()?));
        Ok(())
    }

    fn connection(&self) -> &Connection {
        self.connection.as_ref().expect("scratch connection closed")
    }
}

impl DiskNamespace<'_> {
    pub fn get(&self, key: &[u8]) -> EngineResult<Option<Vec<u8>>> {
        self.table.mark_statement()?;
        let key = self.key(key);
        let value = self
            .table
            .connection()
            .query_row(
                "SELECT value FROM entries WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(map_sqlite_error)?;
        self.table.mark_rows(u64::from(value.is_some()))?;
        Ok(value)
    }

    pub fn storage_bytes(&self) -> EngineResult<u64> {
        self.table.storage_bytes()
    }

    pub fn put(&self, key: &[u8], value: &[u8]) -> EngineResult<()> {
        self.table.mark_statement()?;
        let key = self.key(key);
        let rows = self
            .table
            .connection()
            .execute(
                "INSERT INTO entries (key, value, pending) VALUES (?1, ?2, 0)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, pending = 0",
                params![key, value],
            )
            .map_err(map_sqlite_error)?;
        self.table.mark_rows(rows as u64)?;
        self.table.observe_storage()
    }

    pub fn remove(&self, key: &[u8]) -> EngineResult<()> {
        self.table.mark_statement()?;
        let key = self.key(key);
        let rows = self
            .table
            .connection()
            .execute("DELETE FROM entries WHERE key = ?1", params![key])
            .map_err(map_sqlite_error)?;
        self.table.mark_rows(rows as u64)?;
        self.table.observe_storage()
    }

    pub fn enqueue_once(&self, key: &[u8], payload: &[u8]) -> EngineResult<()> {
        match self.get(key)? {
            Some(existing) if existing == payload => Ok(()),
            Some(_) => Err(EngineError::InvalidRecord("scratch role conflict")),
            None => {
                self.table.mark_statement()?;
                let key = self.key(key);
                let rows = self
                    .table
                    .connection()
                    .execute(
                        "INSERT INTO entries (key, value, pending) VALUES (?1, ?2, 1)",
                        params![key, payload],
                    )
                    .map_err(map_sqlite_error)?;
                self.table.mark_rows(rows as u64)?;
                self.table.observe_storage()
            }
        }
    }

    pub fn pop_pending(&self) -> EngineResult<Option<(Vec<u8>, Vec<u8>)>> {
        self.table.mark_statement()?;
        let row = self
            .table
            .connection()
            .query_row(
                "SELECT key, value FROM entries
                 WHERE pending = 1 AND key >= ?1 AND key < ?2
                 ORDER BY key LIMIT 1",
                params![&self.prefix, &self.upper],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()
            .map_err(map_sqlite_error)?;
        let Some((key, value)) = row else {
            return Ok(None);
        };
        self.table.mark_rows(1)?;
        self.table.mark_statement()?;
        let rows = self
            .table
            .connection()
            .execute(
                "UPDATE entries SET pending = 0 WHERE key = ?1",
                params![&key],
            )
            .map_err(map_sqlite_error)?;
        self.table.mark_rows(rows as u64)?;
        self.table.observe_storage()?;
        Ok(Some((self.strip(key)?, value)))
    }

    pub fn for_each(
        &self,
        mut callback: impl FnMut(&[u8]) -> EngineResult<()>,
    ) -> EngineResult<()> {
        self.table.mark_statement()?;
        let mut statement = self
            .table
            .connection()
            .prepare("SELECT value FROM entries WHERE key >= ?1 AND key < ?2 ORDER BY key")
            .map_err(map_sqlite_error)?;
        let mut rows = statement
            .query(params![&self.prefix, &self.upper])
            .map_err(map_sqlite_error)?;
        while let Some(row) = rows.next().map_err(map_sqlite_error)? {
            self.table.mark_rows(1)?;
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
        self.for_each_entry(|key, _| callback(key))
    }

    pub fn for_each_entry(
        &self,
        mut callback: impl FnMut(&[u8], &[u8]) -> EngineResult<()>,
    ) -> EngineResult<()> {
        self.for_each_entry_range(&self.prefix, &self.upper, &mut callback)
    }

    pub fn for_each_entry_prefix(
        &self,
        prefix: &[u8],
        mut callback: impl FnMut(&[u8], &[u8]) -> EngineResult<()>,
    ) -> EngineResult<()> {
        let lower = self.key(prefix);
        let upper = prefix_upper_bound(&lower)?;
        self.for_each_entry_range(&lower, &upper, &mut callback)
    }

    fn for_each_entry_range(
        &self,
        lower: &[u8],
        upper: &[u8],
        callback: &mut impl FnMut(&[u8], &[u8]) -> EngineResult<()>,
    ) -> EngineResult<()> {
        self.table.mark_statement()?;
        let mut statement = self
            .table
            .connection()
            .prepare("SELECT key, value FROM entries WHERE key >= ?1 AND key < ?2 ORDER BY key")
            .map_err(map_sqlite_error)?;
        let mut rows = statement
            .query(params![lower, upper])
            .map_err(map_sqlite_error)?;
        while let Some(row) = rows.next().map_err(map_sqlite_error)? {
            self.table.mark_rows(1)?;
            let key = match row.get_ref(0).map_err(map_sqlite_error)? {
                ValueRef::Blob(value) => value,
                _ => return Err(EngineError::InvalidRecord("scratch key")),
            };
            let value = match row.get_ref(1).map_err(map_sqlite_error)? {
                ValueRef::Blob(value) => value,
                _ => return Err(EngineError::InvalidRecord("scratch value")),
            };
            callback(
                key.get(self.prefix.len()..)
                    .ok_or(EngineError::InvalidRecord("scratch namespace key"))?,
                value,
            )?;
        }
        Ok(())
    }

    fn key(&self, key: &[u8]) -> Vec<u8> {
        let mut physical = Vec::with_capacity(self.prefix.len() + key.len());
        physical.extend_from_slice(&self.prefix);
        physical.extend_from_slice(key);
        physical
    }

    fn strip(&self, key: Vec<u8>) -> EngineResult<Vec<u8>> {
        key.get(self.prefix.len()..)
            .map(<[u8]>::to_vec)
            .ok_or(EngineError::InvalidRecord("scratch namespace key"))
    }
}

fn prefix_upper_bound(prefix: &[u8]) -> EngineResult<Vec<u8>> {
    let mut upper = prefix.to_vec();
    for index in (0..upper.len()).rev() {
        if upper[index] != u8::MAX {
            upper[index] += 1;
            upper.truncate(index + 1);
            return Ok(upper);
        }
    }
    Err(EngineError::InvalidRecord("scratch namespace"))
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

    fn assert_default_cache_budget(table: &DiskTable) -> i64 {
        let page_size = table
            .connection()
            .query_row("PRAGMA page_size", [], |row| row.get::<_, i64>(0))
            .unwrap();
        let expected_pages = (i64::from(DISK_TABLE_CACHE_KIB) * 1024 + page_size - 1) / page_size;
        assert_eq!(
            table
                .connection()
                .query_row("PRAGMA cache_size", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            expected_pages
        );
        assert_eq!(
            table
                .connection()
                .query_row("PRAGMA cache_spill", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            expected_pages
        );
        expected_pages
    }

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
            assert_default_cache_budget(&table);
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
            let mut prefixed = Vec::new();
            table
                .for_each_entry_prefix(b"prefix01", |key, value| {
                    prefixed.push((key.to_vec(), value.to_vec()));
                    Ok(())
                })
                .unwrap();
            assert_eq!(prefixed.len(), 2);
            let observation = table.observation().unwrap();
            assert_eq!(observation.tables, 1);
            assert!(observation.statements > 23);
            assert_eq!(observation.rows, 12);
            assert!(observation.high_water_bytes > 0);
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
    fn namespaces_share_one_connection_and_isolate_keys_and_queues() {
        let anchor = std::env::temp_dir().join(format!(
            "layerfs-scratch-namespaces-{}-{}",
            std::process::id(),
            SCRATCH_SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let engine = crate::Engine::open(&anchor).unwrap();
        let table = DiskTable::create_near(&anchor, "namespaces").unwrap();
        let expected_pages = assert_default_cache_budget(&table);
        let statements = table.observation().unwrap().statements;
        table.set_cache_size_kib(DISK_TABLE_CACHE_KIB).unwrap();
        assert_eq!(table.observation().unwrap().statements, statements + 3);
        assert_eq!(assert_default_cache_budget(&table), expected_pages);
        let first = table.namespace(b"first").unwrap();
        let second = table.namespace(b"second").unwrap();

        first.put(b"same", b"one").unwrap();
        second.put(b"same", b"two").unwrap();
        first.enqueue_once(b"queue", b"first").unwrap();
        second.enqueue_once(b"queue", b"second").unwrap();
        assert_eq!(first.get(b"same").unwrap(), Some(b"one".to_vec()));
        assert_eq!(second.get(b"same").unwrap(), Some(b"two".to_vec()));
        assert_eq!(
            first.pop_pending().unwrap(),
            Some((b"queue".to_vec(), b"first".to_vec()))
        );
        assert_eq!(
            second.pop_pending().unwrap(),
            Some((b"queue".to_vec(), b"second".to_vec()))
        );
        first
            .for_each_entry(|key, value| {
                assert!(matches!(key, b"queue" | b"same"));
                assert!(matches!(value, b"first" | b"one"));
                assert_eq!(second.get(b"same")?, Some(b"two".to_vec()));
                second.put(b"nested", value)?;
                Ok(())
            })
            .unwrap();
        assert!(second.get(b"nested").unwrap().is_some());

        let observation = table.observation().unwrap();
        assert_eq!(observation.tables, 1);
        assert!(observation.rows > 0);
        assert!(observation.high_water_bytes > 0);
        drop(second);
        drop(first);
        drop(table);
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
    fn store_bound_creation_does_not_reopen_exclusively_locked_authority() {
        let store = std::env::temp_dir().join(format!(
            "layerfs-scratch-exclusive-{}-{}.sqlite",
            std::process::id(),
            SCRATCH_SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let engine = crate::Engine::open(&store).unwrap();
        let store_id = engine.store_id().unwrap();
        drop(engine);
        let connection = Connection::open(&store).unwrap();
        connection.execute_batch("BEGIN EXCLUSIVE").unwrap();
        let table = DiskTable::create_near_with_store_id(&store, "exclusive", store_id).unwrap();
        assert_default_cache_budget(&table);
        table.put(b"key", b"value").unwrap();
        drop(table);
        connection.execute_batch("ROLLBACK").unwrap();
        drop(connection);
        std::fs::remove_file(store).unwrap();
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
