use super::table::DiskTable;
use crate::error::{map_sqlite_error, EngineError, EngineResult};
use rusqlite::types::ValueRef;
use rusqlite::{params, OptionalExtension};
use std::time::Instant;

impl DiskTable {
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
}

pub struct DiskNamespace<'a> {
    pub(super) table: &'a DiskTable,
    pub(super) prefix: Vec<u8>,
    pub(super) upper: Vec<u8>,
}

impl DiskNamespace<'_> {
    pub fn clear(&self) -> EngineResult<()> {
        self.table.mark_statement()?;
        let rows = self
            .table
            .connection()
            .execute(
                "DELETE FROM entries WHERE key >= ?1 AND key < ?2",
                params![&self.prefix, &self.upper],
            )
            .map_err(map_sqlite_error)?;
        self.table.mark_rows(rows as u64)?;
        self.table.observe_storage()
    }

    pub fn get(&self, key: &[u8]) -> EngineResult<Option<Vec<u8>>> {
        let started = Instant::now();
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
        self.table.observe_operation_time(started);
        Ok(value)
    }

    pub fn get_ordered_batch(
        &self,
        keys: &[&[u8]],
        mut callback: impl FnMut(usize, Option<&[u8]>) -> EngineResult<()>,
    ) -> EngineResult<()> {
        if keys.len() > 64 {
            return Err(EngineError::InvalidRecord("scratch batch exceeds 64"));
        }
        if keys.is_empty() {
            return Ok(());
        }
        let keys = keys.iter().map(|key| self.key(key)).collect::<Vec<_>>();
        let sql = (0..keys.len())
            .map(|index| {
                format!(
                    "SELECT {index} AS ord, (SELECT value FROM entries WHERE key = ?{}) AS value",
                    index + 1
                )
            })
            .collect::<Vec<_>>()
            .join(" UNION ALL ")
            + " ORDER BY 1";
        self.table.mark_statement()?;
        let mut statement = self
            .table
            .connection()
            .prepare_cached(&sql)
            .map_err(map_sqlite_error)?;
        let mut rows = statement
            .query(rusqlite::params_from_iter(keys.iter().map(Vec::as_slice)))
            .map_err(map_sqlite_error)?;
        let mut ordinal = 0;
        while let Some(row) = rows.next().map_err(map_sqlite_error)? {
            if ordinal >= keys.len() {
                return Err(EngineError::InvalidRecord("scratch batch cardinality"));
            }
            let observed = row.get::<_, i64>(0).map_err(map_sqlite_error)?;
            if observed != ordinal as i64 {
                return Err(EngineError::InvalidRecord("scratch batch order"));
            }
            match row.get_ref(1).map_err(map_sqlite_error)? {
                ValueRef::Null => callback(ordinal, None)?,
                ValueRef::Blob(value) => {
                    self.table.mark_rows(1)?;
                    callback(ordinal, Some(value))?;
                }
                _ => {
                    self.table.mark_rows(1)?;
                    return Err(EngineError::InvalidRecord("scratch value"));
                }
            }
            ordinal += 1;
        }
        if ordinal != keys.len() {
            return Err(EngineError::InvalidRecord("scratch batch cardinality"));
        }
        Ok(())
    }

    pub fn storage_bytes(&self) -> EngineResult<u64> {
        self.table.storage_bytes()
    }

    pub fn put(&self, key: &[u8], value: &[u8]) -> EngineResult<()> {
        let started = Instant::now();
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
        let result = self.table.observe_storage();
        self.table.observe_operation_time(started);
        result
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

    pub(super) fn key(&self, key: &[u8]) -> Vec<u8> {
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

pub(super) fn prefix_upper_bound(prefix: &[u8]) -> EngineResult<Vec<u8>> {
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
