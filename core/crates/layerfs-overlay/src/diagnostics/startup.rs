//! Observe each real startup statement, including prepare/step failures.
use crate::{DatabaseWork, OverlayResult, StatementKind, StatementWork};
use rusqlite::{fallible_iterator::FallibleIterator, Connection, StatementStatus};
use std::{cell::RefCell, time::Instant};

/// Executes a finite startup batch once, consuming pragma rows without replay.
pub(crate) fn batch(
    connection: &Connection,
    work: &RefCell<DatabaseWork>,
    sql: &str,
) -> OverlayResult<()> {
    let mut batch = rusqlite::Batch::new(connection, sql);
    let mut input_bytes = sql.len() as u64;
    loop {
        let start = Instant::now();
        let mut observed = StatementWork {
            attempts: 1,
            sql_bytes: input_bytes,
            ..Default::default()
        };
        let mut statement = match batch.next() {
            Ok(Some(statement)) => statement,
            Ok(None) => return Ok(()),
            Err(error) => {
                observed.elapsed_ns = start.elapsed().as_nanos().min(u64::MAX as u128) as u64;
                work.borrow_mut().statements[StatementKind::Startup as usize].accumulate(observed);
                return Err(error.into());
            }
        };
        input_bytes = 0;
        let before = connection.total_changes();
        let result: rusqlite::Result<()> = (|| {
            let mut rows = statement.query([])?;
            while let Some(row) = rows.next()? {
                observed.rows_returned = observed.rows_returned.saturating_add(1);
                crate::metrics::observe_row(&mut observed, row)?;
            }
            Ok(())
        })();
        let count = |counter| statement.get_status(counter).max(0) as u64;
        observed.executions = count(StatementStatus::Run);
        observed.vm_steps = count(StatementStatus::VmStep);
        observed.fullscan_steps = count(StatementStatus::FullscanStep);
        observed.sorts = count(StatementStatus::Sort);
        observed.autoindex_rows = count(StatementStatus::AutoIndex);
        observed.reprepares = count(StatementStatus::RePrepare);
        observed.statement_memory_samples = 1;
        observed.statement_memory_sample_bytes = count(StatementStatus::MemUsed);
        observed.rows_changed = connection.total_changes().saturating_sub(before);
        if observed.rows_changed != 0 {
            observed.direct_rows_changed = connection.changes();
        }
        observed.elapsed_ns = start.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        work.borrow_mut().statements[StatementKind::Startup as usize].accumulate(observed);
        result?;
    }
}
