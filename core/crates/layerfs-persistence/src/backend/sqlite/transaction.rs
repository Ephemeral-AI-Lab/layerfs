//! One-attempt transaction ownership and quarantine on uncertain outcomes.
use super::{
    connection::{Session, SqlWork},
    query,
};
use crate::backend::records::{BackendError, OutcomeError, Param, Record};
use rusqlite::Connection;
use std::{
    cell::{Cell, RefCell},
    sync::TryLockError,
    time::Instant,
};
pub(crate) struct Transaction<'a> {
    pub(crate) connection: &'a Connection,
    pub(crate) work: &'a RefCell<SqlWork>,
    uncertain: Cell<bool>,
    owner: &'a Session,
}
impl Transaction<'_> {
    pub(crate) fn layout(&self) -> crate::SqlitePackLayout {
        self.owner.layout
    }
    pub(crate) fn schema_version(&self) -> i64 {
        self.owner.schema_version()
    }
    #[cfg(target_os = "macos")]
    pub(crate) fn segment_owner(
        &self,
    ) -> Result<&super::segment_owner::SegmentOwner, BackendError> {
        self.owner.segments.as_ref().ok_or(BackendError::Integrity)
    }
    pub(crate) fn before_pack(&self, capacity: usize) -> Result<(), BackendError> {
        #[cfg(target_os = "macos")]
        {
            let start = Instant::now();
            let result = self
                .owner
                .allocation
                .as_ref()
                .ok_or(BackendError::Integrity)?
                .before_pack(capacity);
            let wall = start.elapsed().as_nanos() as u64;
            self.work.borrow_mut().preallocation_ns += wall;
            if let Ok((bytes, close)) = result {
                let mut work = self.work.borrow_mut();
                work.preallocation_calls += u64::from(bytes != 0);
                work.preallocation_bytes += bytes;
                work.preallocation_close_ns += close;
            }
            if result.as_ref().err() == Some(&BackendError::Unknown) {
                self.uncertain.set(true);
            }
            result.map(|_| ())
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = capacity;
            Err(BackendError::Integrity)
        }
    }

    pub(crate) fn input_limit(
        &self,
        bindings: usize,
        prefix: usize,
        per_row: usize,
    ) -> Result<usize, BackendError> {
        let count = (self.variable_limit()? / bindings)
            .min(self.sql_length_limit()?.saturating_sub(prefix) / per_row);
        if count == 0 {
            return Err(BackendError::Capacity);
        }
        Ok(count)
    }

    pub(crate) fn bootstrap(&self, script: &str) -> Result<(), BackendError> {
        let r = query::batch(self.connection, script, self.work);
        if r.as_ref().err() == Some(&BackendError::Unknown) {
            self.uncertain.set(true);
        }
        r
    }

    pub(crate) fn history(
        &self,
        key: &str,
        params: Vec<Param>,
    ) -> Result<Vec<Record>, BackendError> {
        self.query(query::history(key)?, params)
    }
    pub(crate) fn variable_limit(&self) -> Result<usize, BackendError> {
        self.connection
            .limit(rusqlite::limits::Limit::SQLITE_LIMIT_VARIABLE_NUMBER)
            .map(|v| v as usize)
            .map_err(super::rows::error)
    }
    pub(crate) fn sql_length_limit(&self) -> Result<usize, BackendError> {
        self.connection
            .limit(rusqlite::limits::Limit::SQLITE_LIMIT_SQL_LENGTH)
            .map(|v| v as usize)
            .map_err(super::rows::error)
    }
    pub(crate) fn borrowed(
        &self,
        sql: &str,
        values: &[&dyn rusqlite::ToSql],
        bytes: u64,
    ) -> Result<Vec<Record>, BackendError> {
        let r = query::borrowed(self.connection, sql, values, bytes, self.work);
        if r.as_ref().err() == Some(&BackendError::Unknown) {
            self.uncertain.set(true);
        }
        r
    }
    /// Leases one statement for repeated executions inside this transaction.
    pub(crate) fn prepare(&self, sql: &str) -> Result<super::prepared::Prepared<'_>, BackendError> {
        let result =
            super::prepared::Prepared::new(self.connection, sql, self.work, Some(&self.uncertain));
        if result.as_ref().err() == Some(&BackendError::Unknown) {
            self.uncertain.set(true);
        }
        result
    }
    pub(crate) fn mapped<T>(
        &self,
        sql: &str,
        values: &[&dyn rusqlite::ToSql],
        bytes: u64,
        capacity: usize,
        decode: impl FnMut(&rusqlite::Row<'_>) -> Result<T, BackendError>,
    ) -> Result<Vec<T>, BackendError> {
        let result = query::mapped(
            self.connection,
            sql,
            values,
            bytes,
            capacity,
            self.work,
            decode,
        );
        if result.as_ref().err() == Some(&BackendError::Unknown) {
            self.uncertain.set(true);
        }
        result
    }
    pub(crate) fn query(&self, sql: &str, params: Vec<Param>) -> Result<Vec<Record>, BackendError> {
        {
            let r = query::run(self.connection, sql, params, self.work);
            if r.as_ref().err() == Some(&BackendError::Unknown) {
                self.uncertain.set(true);
            }
            r
        }
    }
}
impl Session {
    pub(crate) fn run<T, E: From<BackendError> + OutcomeError>(
        &self,
        writable: bool,
        body: impl FnOnce(&Transaction<'_>) -> Result<T, E>,
    ) -> Result<T, E> {
        if writable && !self.writable {
            return Err(BackendError::ReadOnly.into());
        }
        let start = Instant::now();
        let mut s = match self.state.try_lock() {
            Ok(s) => s,
            Err(TryLockError::WouldBlock) => return Err(BackendError::Busy.into()),
            Err(TryLockError::Poisoned(_)) => return Err(BackendError::Unknown.into()),
        };
        if s.quarantined {
            return Err(BackendError::Unknown.into());
        }
        let tx = Transaction {
            connection: &s.connection,
            work: &s.work,
            uncertain: Cell::new(false),
            owner: self,
        };
        if let Err(e) = tx.query(if writable { "BEGIN IMMEDIATE" } else { "BEGIN" }, vec![]) {
            if e == BackendError::Unknown {
                s.quarantined = true;
            }
            s.work.borrow_mut().transaction_ns += start.elapsed().as_nanos() as u64;
            return Err(e.into());
        }
        tx.work.borrow_mut().transactions += 1;
        if writable {
            tx.work.borrow_mut().write_transactions += 1;
        }
        let result = body(&tx);
        let uncertain =
            tx.uncertain.get() || result.as_ref().err().is_some_and(OutcomeError::uncertain);
        if uncertain {
            s.quarantined = true;
            s.work.borrow_mut().transaction_ns += start.elapsed().as_nanos() as u64;
            return Err(BackendError::Unknown.into());
        }
        let result = match result {
            Ok(value) => match query::run(&s.connection, "COMMIT", vec![], &s.work) {
                Ok(_) => {
                    s.work.borrow_mut().commits += 1;
                    if writable {
                        s.work.borrow_mut().write_commits += 1;
                    }
                    Ok(value)
                }
                Err(e) => {
                    if e == BackendError::Unknown {
                        s.quarantined = true;
                        Err(e.into())
                    } else if !s.connection.is_autocommit()
                        && query::run(&s.connection, "ROLLBACK", vec![], &s.work).is_err()
                    {
                        s.quarantined = true;
                        Err(BackendError::Unknown.into())
                    } else {
                        s.work.borrow_mut().rollbacks += 1;
                        Err(e.into())
                    }
                }
            },
            Err(e) => {
                // A SQL I/O error is marked in the transaction facade before returning.
                if s.quarantined {
                    Err(BackendError::Unknown.into())
                } else if !s.connection.is_autocommit() {
                    match query::run(&s.connection, "ROLLBACK", vec![], &s.work) {
                        Ok(_) => {
                            s.work.borrow_mut().rollbacks += 1;
                            Err(e)
                        }
                        Err(_) => {
                            s.quarantined = true;
                            Err(BackendError::Unknown.into())
                        }
                    }
                } else {
                    Err(e)
                }
            }
        };
        s.work.borrow_mut().transaction_ns += start.elapsed().as_nanos() as u64;
        result
    }
}
