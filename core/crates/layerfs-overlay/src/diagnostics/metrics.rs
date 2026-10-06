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
    /// All changed rows, including accounting triggers.
    pub rows_changed: u64,
    /// Direct statement changes; excludes trigger side effects.
    pub direct_rows_changed: u64,
    /// BLOB column bytes returned by SQLite before row decoding. This counts
    /// delivered values, not SQLite internal copies or process residency.
    pub returned_blob_bytes: u64,
    /// Logical SQLite column delivery: BLOB/TEXT lengths, eight bytes per
    /// INTEGER/REAL, zero per NULL. Not VFS I/O or internal-copy attribution.
    pub returned_value_bytes: u64,
    pub vm_steps: u64,
    pub fullscan_steps: u64,
    pub sorts: u64,
    pub autoindex_rows: u64,
    pub reprepares: u64,
    pub bound_bytes: u64,
    /// SQL bytes supplied to statement checkout; repeated cached lookups count.
    pub sql_bytes: u64,
    /// Samples of SQLITE_STMTSTATUS_MEMUSED, not pager/process residency.
    pub statement_memory_samples: u64,
    /// Sum of approximate prepared-statement heap samples. Repeated sampling
    /// counts the same cached statement again; this is not resident allocation.
    pub statement_memory_sample_bytes: u64,
    pub elapsed_ns: u64,
}
/// Connection-scoped counters. Snapshot around one exclusive owner job for scope.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DatabaseWork {
    pub statements: [StatementWork; 14],
}

pub(crate) struct Query<'a> {
    pub sql: &'a str,
    pub params: &'a [&'a dyn ToSql],
    pub bound_bytes: u64,
    pub cached: bool,
}
pub(crate) fn query<T>(
    connection: &Connection,
    work: &RefCell<DatabaseWork>,
    kind: StatementKind,
    input: Query<'_>,
    mut decode: impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
) -> OverlayResult<Vec<T>> {
    let Query {
        sql,
        params,
        bound_bytes,
        cached,
    } = input;
    let start = Instant::now();
    let mut observed = StatementWork {
        attempts: 1,
        bound_bytes,
        sql_bytes: sql.len() as u64,
        ..Default::default()
    };
    let result = (|| {
        let mut plain;
        let mut retained;
        let statement = if cached {
            retained = connection.prepare_cached(sql)?;
            &mut *retained
        } else {
            plain = connection.prepare(sql)?;
            &mut plain
        };
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
                observe_row(&mut observed, row)?;
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
        observed.statement_memory_samples = 1;
        observed.statement_memory_sample_bytes = count(StatementStatus::MemUsed);
        observed.rows_changed = connection.total_changes().saturating_sub(before_changes);
        if observed.rows_changed != 0 {
            observed.direct_rows_changed = connection.changes();
        }
        result.map_err(OverlayError::from)
    })();
    observed.elapsed_ns = start.elapsed().as_nanos().min(u64::MAX as u128) as u64;
    work.borrow_mut().statements[kind as usize].accumulate(observed);
    result
}
impl StatementWork {
    pub fn accumulate(&mut self, other: Self) {
        self.attempts = self.attempts.saturating_add(other.attempts);
        self.executions = self.executions.saturating_add(other.executions);
        self.rows_returned = self.rows_returned.saturating_add(other.rows_returned);
        self.rows_changed = self.rows_changed.saturating_add(other.rows_changed);
        self.direct_rows_changed = self
            .direct_rows_changed
            .saturating_add(other.direct_rows_changed);
        self.returned_blob_bytes = self
            .returned_blob_bytes
            .saturating_add(other.returned_blob_bytes);
        self.returned_value_bytes = self
            .returned_value_bytes
            .saturating_add(other.returned_value_bytes);
        self.vm_steps = self.vm_steps.saturating_add(other.vm_steps);
        self.fullscan_steps = self.fullscan_steps.saturating_add(other.fullscan_steps);
        self.sorts = self.sorts.saturating_add(other.sorts);
        self.autoindex_rows = self.autoindex_rows.saturating_add(other.autoindex_rows);
        self.reprepares = self.reprepares.saturating_add(other.reprepares);
        self.bound_bytes = self.bound_bytes.saturating_add(other.bound_bytes);
        self.sql_bytes = self.sql_bytes.saturating_add(other.sql_bytes);
        self.statement_memory_samples = self
            .statement_memory_samples
            .saturating_add(other.statement_memory_samples);
        self.statement_memory_sample_bytes = self
            .statement_memory_sample_bytes
            .saturating_add(other.statement_memory_sample_bytes);
        self.elapsed_ns = self.elapsed_ns.saturating_add(other.elapsed_ns);
    }
}

impl DatabaseWork {
    /// Aggregate all statement families from the same exclusive scope. Inclusive
    /// elapsed spans are observations, never an exclusive CPU decomposition.
    pub fn total(&self) -> StatementWork {
        let mut total = StatementWork::default();
        for family in self.statements {
            total.accumulate(family);
        }
        total
    }

    /// Work since an earlier connection snapshot. Counters saturate for
    /// observation rather than refusing a product operation.
    pub fn since(&self, before: &Self) -> Self {
        let mut delta = Self::default();
        for ((to, a), b) in delta
            .statements
            .iter_mut()
            .zip(&self.statements)
            .zip(&before.statements)
        {
            *to = StatementWork {
                attempts: a.attempts.saturating_sub(b.attempts),
                executions: a.executions.saturating_sub(b.executions),
                rows_returned: a.rows_returned.saturating_sub(b.rows_returned),
                rows_changed: a.rows_changed.saturating_sub(b.rows_changed),
                direct_rows_changed: a.direct_rows_changed.saturating_sub(b.direct_rows_changed),
                returned_blob_bytes: a.returned_blob_bytes.saturating_sub(b.returned_blob_bytes),
                returned_value_bytes: a
                    .returned_value_bytes
                    .saturating_sub(b.returned_value_bytes),
                vm_steps: a.vm_steps.saturating_sub(b.vm_steps),
                fullscan_steps: a.fullscan_steps.saturating_sub(b.fullscan_steps),
                sorts: a.sorts.saturating_sub(b.sorts),
                autoindex_rows: a.autoindex_rows.saturating_sub(b.autoindex_rows),
                reprepares: a.reprepares.saturating_sub(b.reprepares),
                bound_bytes: a.bound_bytes.saturating_sub(b.bound_bytes),
                sql_bytes: a.sql_bytes.saturating_sub(b.sql_bytes),
                statement_memory_samples: a
                    .statement_memory_samples
                    .saturating_sub(b.statement_memory_samples),
                statement_memory_sample_bytes: a
                    .statement_memory_sample_bytes
                    .saturating_sub(b.statement_memory_sample_bytes),
                elapsed_ns: a.elapsed_ns.saturating_sub(b.elapsed_ns),
            };
        }
        delta
    }
    /// Adds an exclusive owner-job observation to a bounded aggregate.
    pub fn accumulate(&mut self, delta: Self) {
        for (to, from) in self.statements.iter_mut().zip(delta.statements) {
            to.accumulate(from);
        }
    }
}

pub(crate) fn observe_row(
    work: &mut StatementWork,
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<()> {
    use rusqlite::types::ValueRef;
    for column in 0..row.as_ref().column_count() {
        let bytes = match row.get_ref(column)? {
            ValueRef::Blob(bytes) => {
                work.returned_blob_bytes =
                    work.returned_blob_bytes.saturating_add(bytes.len() as u64);
                bytes.len() as u64
            }
            ValueRef::Text(bytes) => bytes.len() as u64,
            ValueRef::Integer(_) | ValueRef::Real(_) => 8,
            ValueRef::Null => 0,
        };
        work.returned_value_bytes = work.returned_value_bytes.saturating_add(bytes);
    }
    Ok(())
}
