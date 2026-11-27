//! Measured SQLite execution of the closed statement set.
use super::connection::SqlWork;
use super::rows;
use crate::backend::error::BackendError;
use crate::backend::seam::{Param, Record};
use rusqlite::{Connection, StatementStatus};
use std::{cell::RefCell, time::Instant};
pub(crate) fn run(
    connection: &Connection,
    sql: &str,
    params: Vec<Param>,
    work: &RefCell<SqlWork>,
) -> Result<Vec<Record>, BackendError> {
    let values: Vec<_> = params.into_iter().map(rows::bind).collect();
    let bytes = values
        .iter()
        .map(|v| match v {
            rusqlite::types::Value::Blob(v) => v.len(),
            rusqlite::types::Value::Text(v) => v.len(),
            _ => 8,
        })
        .sum::<usize>() as u64;
    let bindings: Vec<&dyn rusqlite::ToSql> =
        values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
    borrowed(connection, sql, &bindings, bytes, work)
}
pub(crate) fn borrowed(
    connection: &Connection,
    sql: &str,
    values: &[&dyn rusqlite::ToSql],
    bytes: u64,
    work: &RefCell<SqlWork>,
) -> Result<Vec<Record>, BackendError> {
    mapped(connection, sql, values, bytes, 0, work, |row| {
        rows::record(row, row.as_ref().column_count())
    })
}
pub(crate) fn mapped<T>(
    connection: &Connection,
    sql: &str,
    values: &[&dyn rusqlite::ToSql],
    bytes: u64,
    capacity: usize,
    work: &RefCell<SqlWork>,
    decode: impl FnMut(&rusqlite::Row<'_>) -> Result<T, BackendError>,
) -> Result<Vec<T>, BackendError> {
    super::prepared::Prepared::new(connection, sql, work, None)?
        .mapped(values, bytes, capacity, decode)
}

pub(crate) fn batch(
    connection: &Connection,
    sql: &str,
    work: &RefCell<SqlWork>,
) -> Result<(), BackendError> {
    use rusqlite::fallible_iterator::FallibleIterator;
    let start = Instant::now();
    let result = (|| {
        let mut batch = rusqlite::Batch::new(connection, sql);
        while let Some(mut statement) = batch.next().map_err(rows::error)? {
            let result = statement.execute([]).map_err(rows::error);
            let mut w = work.borrow_mut();
            w.statements += 1;
            w.vm_steps += statement.get_status(StatementStatus::VmStep).max(0) as u64;
            w.fullscan_steps += statement.get_status(StatementStatus::FullscanStep).max(0) as u64;
            w.sorts += statement.get_status(StatementStatus::Sort).max(0) as u64;
            w.autoindex_rows += statement.get_status(StatementStatus::AutoIndex).max(0) as u64;
            w.reprepares += statement.get_status(StatementStatus::RePrepare).max(0) as u64;
            result?;
        }
        Ok(())
    })();
    work.borrow_mut().statement_ns += start.elapsed().as_nanos() as u64;
    result
}
