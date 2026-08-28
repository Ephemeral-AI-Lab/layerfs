use crate::error::{map_sqlite_error, EngineError, EngineResult, SqliteErrorKind};
use rusqlite::types::ValueRef;
use rusqlite::{params, Connection, OptionalExtension};
use std::cell::Cell;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::time::Instant;

pub(super) static SCRATCH_SERIAL: AtomicU64 = AtomicU64::new(0);

pub struct DiskTable {
    pub(super) path: PathBuf,
    pub(super) connection: Option<Connection>,
    pub(super) statements: Cell<u64>,
    pub(super) owner_setup_statements: Cell<u64>,
    pub(super) derived_setup_statements: Cell<u64>,
    pub(super) operation_statements: Cell<u64>,
    pub(super) store_reopens: u64,
    pub(super) store_inspection_statements: u64,
    pub(super) store_inspection_wall_ns: u64,
    pub(super) setup_wall_ns: u64,
    pub(super) operation_wall_ns: Cell<u64>,
    pub(super) rows: Cell<u64>,
    pub(super) high_water_bytes: Cell<u64>,
}

impl DiskTable {
    pub fn get(&self, key: &[u8]) -> EngineResult<Option<Vec<u8>>> {
        let started = Instant::now();
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
        self.observe_operation_time(started);
        Ok(value)
    }

    pub fn storage_bytes(&self) -> EngineResult<u64> {
        let database = std::fs::metadata(&self.path)
            .map_err(|error| EngineError::Sqlite {
                kind: SqliteErrorKind::Io,
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
        self.mark_owner_setup_statement()?;
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
        self.mark_owner_setup_statement()?;
        let cache_size = self
            .connection()
            .query_row("PRAGMA cache_size", [], |row| row.get::<_, i64>(0))
            .map_err(map_sqlite_error)?;
        self.mark_owner_setup_statement()?;
        let cache_spill = self
            .connection()
            .query_row("PRAGMA cache_spill", [], |row| row.get::<_, i64>(0))
            .map_err(map_sqlite_error)?;
        if cache_size == pages && cache_spill == pages {
            return Ok(());
        }
        if cache_size != pages {
            self.mark_owner_setup_statement()?;
            self.connection()
                .pragma_update(None, "cache_size", pages)
                .map_err(map_sqlite_error)?;
        }
        if cache_spill != pages {
            self.mark_owner_setup_statement()?;
            self.connection()
                .pragma_update(None, "cache_spill", true)
                .map_err(map_sqlite_error)?;
        }
        self.mark_owner_setup_statement()?;
        let cache_size = self
            .connection()
            .query_row("PRAGMA cache_size", [], |row| row.get::<_, i64>(0))
            .map_err(map_sqlite_error)?;
        self.mark_owner_setup_statement()?;
        let cache_spill = self
            .connection()
            .query_row("PRAGMA cache_spill", [], |row| row.get::<_, i64>(0))
            .map_err(map_sqlite_error)?;
        if cache_size != pages || cache_spill != pages {
            return Err(EngineError::InvalidRecord("scratch cache profile"));
        }
        Ok(())
    }

    pub fn put(&self, key: &[u8], value: &[u8]) -> EngineResult<()> {
        let started = Instant::now();
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
        self.observe_operation_time(started);
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

    pub(super) fn connection(&self) -> &Connection {
        self.connection.as_ref().expect("scratch connection closed")
    }
}
