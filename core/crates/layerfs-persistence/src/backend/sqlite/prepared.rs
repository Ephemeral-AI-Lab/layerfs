//! A scoped cached statement lease with per-execution work and exact errors.
use super::{connection::SqlWork, rows, statement_work};
use crate::backend::records::{BackendError, Record};
use rusqlite::{CachedStatement, Connection, StatementStatus};
use std::{
    cell::{Cell, RefCell},
    time::Instant,
};

/// Held only inside one short Session transaction, never across caller I/O.
pub(crate) struct Prepared<'a> {
    statement: Option<CachedStatement<'a>>,
    work: &'a RefCell<SqlWork>,
    commit: bool,
    uncertain: Option<&'a Cell<bool>>,
}
impl<'a> Prepared<'a> {
    pub(crate) fn new(
        connection: &'a Connection,
        sql: &str,
        work: &'a RefCell<SqlWork>,
        uncertain: Option<&'a Cell<bool>>,
    ) -> Result<Self, BackendError> {
        let start = Instant::now();
        let commit = sql == "COMMIT";
        let statement = {
            let _phase = statement_work::phase(work, 0, commit);
            connection.prepare_cached(sql).map_err(rows::error)
        };
        Self::elapsed(work, commit, start);
        Ok(Self {
            statement: Some(statement?),
            work,
            commit,
            uncertain,
        })
    }
    fn elapsed(work: &RefCell<SqlWork>, commit: bool, start: Instant) {
        let elapsed = start.elapsed().as_nanos() as u64;
        let mut work = work.borrow_mut();
        work.statement_ns += elapsed;
        if commit {
            work.commit_ns += elapsed;
        }
    }
    pub(crate) fn borrowed(
        &mut self,
        values: &[&dyn rusqlite::ToSql],
        bytes: u64,
    ) -> Result<Vec<Record>, BackendError> {
        self.mapped(values, bytes, 0, |row| {
            rows::record(row, row.as_ref().column_count())
        })
    }
    pub(crate) fn mapped<T>(
        &mut self,
        values: &[&dyn rusqlite::ToSql],
        bytes: u64,
        capacity: usize,
        mut decode: impl FnMut(&rusqlite::Row<'_>) -> Result<T, BackendError>,
    ) -> Result<Vec<T>, BackendError> {
        let start = Instant::now();
        let work = self.work;
        let commit = self.commit;
        let statement = self.statement.as_mut().expect("live statement lease");
        let mut returned = 0_u64;
        let result = (|| {
            let mut cursor = {
                let _phase = statement_work::phase(work, 1, commit);
                statement.query(values).map_err(rows::error)?
            };
            let result = (|| {
                let mut result = Vec::with_capacity(capacity);
                loop {
                    let next = {
                        let _phase = statement_work::phase(work, 2, commit);
                        cursor.next().map_err(rows::error)
                    };
                    let Some(row) = next? else { break };
                    let _phase = statement_work::phase(work, 3, commit);
                    result.push(decode(row)?);
                    returned += 1;
                }
                Ok(result)
            })();
            {
                let _phase = statement_work::phase(work, 4, commit);
                drop(cursor);
            }
            result
        })();
        let counters = {
            let _phase = statement_work::phase(work, 5, commit);
            [
                StatementStatus::VmStep,
                StatementStatus::FullscanStep,
                StatementStatus::Sort,
                StatementStatus::AutoIndex,
                StatementStatus::RePrepare,
            ]
            .map(|kind| {
                let count = statement.get_status(kind).max(0) as u64;
                statement.reset_status(kind);
                count
            })
        };
        {
            let mut work = work.borrow_mut();
            work.statements += 1;
            work.vm_steps += counters[0];
            work.fullscan_steps += counters[1];
            work.sorts += counters[2];
            work.autoindex_rows += counters[3];
            work.reprepares += counters[4];
            work.returned_rows += returned;
            work.bound_bytes += bytes;
        }
        Self::elapsed(work, commit, start);
        if result.as_ref().err() == Some(&BackendError::Unknown) {
            if let Some(uncertain) = self.uncertain {
                uncertain.set(true);
            }
        }
        result
    }
}
impl Drop for Prepared<'_> {
    fn drop(&mut self) {
        let start = Instant::now();
        {
            let _phase = statement_work::phase(self.work, 6, self.commit);
            drop(self.statement.take());
        }
        Self::elapsed(self.work, self.commit, start);
    }
}
