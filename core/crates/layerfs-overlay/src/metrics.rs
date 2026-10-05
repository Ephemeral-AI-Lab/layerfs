//! Bounded aggregate observations from actual SQLite executions.
use crate::{OverlayError, OverlayResult};
use rusqlite::{Connection, StatementStatus, ToSql};
use std::{cell::RefCell, time::Instant};

/// Fixed statement families; aggregation never stores an input-sized trace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum StatementKind {
    Startup,
    Begin,
    Commit,
    Rollback,
    Workspace,
    Inode,
    Dentry,
    Payload,
    Frontier,
    Capture,
    Scratch,
    Lease,
    Explain,
    Reclaim,
}
/// Inclusive statement/check-out/binding/step/mapping observations.
/// Elapsed nanoseconds are diagnostic wall, not exclusive CPU or a latency gate.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StatementWork {
    pub attempts: u64,
    pub executions: u64,
    pub rows_returned: u64,
    pub rows_changed: u64,
    pub vm_steps: u64,
    pub fullscan_steps: u64,
    pub sorts: u64,
    pub autoindex_rows: u64,
    pub reprepares: u64,
    pub bound_bytes: u64,
    pub elapsed_ns: u64,
}
/// Connection-scoped counters. Snapshot around one exclusive owner job for scope.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DatabaseWork {
    pub statements: [StatementWork; 14],
}

pub(crate) fn query<T>(
    connection: &Connection,
    work: &RefCell<DatabaseWork>,
    kind: StatementKind,
    sql: &str,
    params: &[&dyn ToSql],
    bound_bytes: u64,
    mut decode: impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
) -> OverlayResult<Vec<T>> {
    let start = Instant::now();
    let mut observed = StatementWork {
        attempts: 1,
        bound_bytes,
        ..Default::default()
    };
    let result = (|| {
        let mut statement = connection.prepare_cached(sql)?;
        for counter in [
            StatementStatus::Run,
            StatementStatus::VmStep,
            StatementStatus::FullscanStep,
            StatementStatus::Sort,
            StatementStatus::AutoIndex,
            StatementStatus::RePrepare,
        ] {
            statement.reset_status(counter);
        }
        let before_changes = connection.total_changes();
        let result: rusqlite::Result<Vec<T>> = (|| {
            let mut rows = statement.query(params)?;
            let mut result = Vec::new();
            while let Some(row) = rows.next()? {
                observed.rows_returned = observed.rows_returned.saturating_add(1);
                result.push(decode(row)?);
            }
            Ok(result)
        })();
        let count = |counter| statement.get_status(counter).max(0) as u64;
        observed.executions = count(StatementStatus::Run);
        observed.vm_steps = count(StatementStatus::VmStep);
        observed.fullscan_steps = count(StatementStatus::FullscanStep);
        observed.sorts = count(StatementStatus::Sort);
        observed.autoindex_rows = count(StatementStatus::AutoIndex);
        observed.reprepares = count(StatementStatus::RePrepare);
        observed.rows_changed = connection.total_changes().saturating_sub(before_changes);
        result.map_err(OverlayError::from)
    })();
    observed.elapsed_ns = start.elapsed().as_nanos().min(u64::MAX as u128) as u64;
    work.borrow_mut().statements[kind as usize].add(observed);
    result
}
impl StatementWork {
    fn add(&mut self, other: Self) {
        self.attempts = self.attempts.saturating_add(other.attempts);
        self.executions = self.executions.saturating_add(other.executions);
        self.rows_returned = self.rows_returned.saturating_add(other.rows_returned);
        self.rows_changed = self.rows_changed.saturating_add(other.rows_changed);
        self.vm_steps = self.vm_steps.saturating_add(other.vm_steps);
        self.fullscan_steps = self.fullscan_steps.saturating_add(other.fullscan_steps);
        self.sorts = self.sorts.saturating_add(other.sorts);
        self.autoindex_rows = self.autoindex_rows.saturating_add(other.autoindex_rows);
        self.reprepares = self.reprepares.saturating_add(other.reprepares);
        self.bound_bytes = self.bound_bytes.saturating_add(other.bound_bytes);
        self.elapsed_ns = self.elapsed_ns.saturating_add(other.elapsed_ns);
    }
}
