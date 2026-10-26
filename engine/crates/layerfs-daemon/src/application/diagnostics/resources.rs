//! One Workspace resource observation, explicitly charged to its call.
use super::Records;
use crate::{OwnerClient, OwnerError};
use std::{
    fmt,
    io::{self, Write},
    sync::Mutex,
};

/// One original Workspace resource observation with fixed copied counters and exact failure custody.
pub struct ResourceObservation {
    resources: Option<layerfs_overlay::Resources>,
    attempted: bool,
    work: Option<Work>,
    failure: Option<ResourceFailure>,
}
/// Exact original resource-observer failure, separate from the control result.
pub enum ResourceFailure {
    /// Exact failure to select the live Workspace observer.
    Selection(Mutex<crate::control::Failure>),
    /// The original operation was refused before any observer attempt.
    BeforeEffect(OwnerError),
    /// The exact attempted result and its statement/allocation work.
    Result(OwnerError),
}
/// Existing diagnostic I/O custody carries both independent original failures.
/// The mutex owns only this fixed failure carrier; it adds no service state.
pub struct ResourceDiagnosticError {
    /// Exact observer command/cause or original completion. Transfer does not
    /// release or resolve an uncertain result, or authorize another attempt.
    pub original: Mutex<ResourceFailure>,
    /// Separate original numeric-output error, if formatting/transfer also failed.
    pub output: Option<io::Error>,
}
impl fmt::Debug for ResourceFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Selection(_) => "original resource selection failure",
            Self::BeforeEffect(_) => "original resource refusal",
            Self::Result(_) => "original resource result retained",
        })
    }
}
impl fmt::Display for ResourceDiagnosticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("original resource observer failure retained")?;
        if self.output.is_some() {
            f.write_str("; separate numeric-output failure retained")?;
        }
        Ok(())
    }
}
impl fmt::Debug for ResourceDiagnosticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
impl std::error::Error for ResourceDiagnosticError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.output
            .as_ref()
            .map(|error| error as &(dyn std::error::Error + 'static))
    }
}
impl ResourceObservation {
    pub(super) fn unavailable() -> Self {
        Self {
            resources: None,
            attempted: false,
            work: None,
            failure: None,
        }
    }
    pub(super) fn selection_failed(cause: crate::control::Failure) -> Self {
        Self {
            resources: None,
            attempted: false,
            work: None,
            failure: Some(ResourceFailure::Selection(Mutex::new(cause))),
        }
    }
    /// Transfers the original observer failure and independent output failure to
    /// diagnostic I/O custody; it changes no outcome and performs no replay.
    pub fn into_diagnostic_error(self, output: Option<io::Error>) -> Option<io::Error> {
        match self.failure {
            Some(original) => Some(io::Error::other(ResourceDiagnosticError {
                original: Mutex::new(original),
                output,
            })),
            None => output,
        }
    }
}
struct Work {
    jobs: u64,
    sql: layerfs_overlay::StatementWork,
    payload: layerfs_overlay::PayloadWork,
    allocation: layerfs_overlay::AllocationWork,
    lock_wait_ns: u64,
    hold_ns: u64,
}
impl From<&crate::JobWork> for Work {
    fn from(work: &crate::JobWork) -> Self {
        Self {
            jobs: work.jobs,
            sql: work.sql.total(),
            payload: work.payload,
            allocation: work.allocation,
            lock_wait_ns: work.lock_wait_ns,
            hold_ns: work.hold_ns,
        }
    }
}
impl ResourceObservation {
    /// One typed call on a bound Workspace handle; no roster scan or retry.
    pub fn acquire(owner: &OwnerClient) -> Self {
        let (resources, observed, failure) = match owner.resources_observed() {
            Ok((value, work)) => (Some(value), Some(work), None),
            Err(cause) => {
                let work = match &cause {
                    OwnerError::Attempted(failure) => Some(&failure.work),
                    _ => None,
                };
                let work = work.map(Work::from);
                return Self {
                    resources: None,
                    attempted: work.is_some(),
                    work,
                    failure: Some(if matches!(cause, OwnerError::Attempted(_)) {
                        ResourceFailure::Result(cause)
                    } else {
                        ResourceFailure::BeforeEffect(cause)
                    }),
                };
            }
        };
        Self {
            resources,
            attempted: true,
            work: observed.as_ref().map(Work::from),
            failure,
        }
    }
}
pub(super) fn emit<W: Write>(
    rows: &mut Records<'_, W>,
    observed: &ResourceObservation,
) -> io::Result<()> {
    let jobs = observed.work.as_ref().map(|work| [work.jobs]);
    rows.row(34, 0, jobs.as_ref().map(|value| value.as_slice()))?;
    match &observed.resources {
        Some(r) => {
            rows.row(32, 0, Some(&r.open.values()))?;
            rows.row(
                28,
                0,
                Some(&[
                    r.counts.namespaces,
                    r.counts.inode_rows,
                    r.counts.directory_entry_rows,
                    r.counts.payload_cells,
                    r.counts.payload_bytes,
                    r.counts.shrink_rows,
                    r.counts.operation_record_rows,
                    r.counts.operation_record_bytes,
                    r.counts.owner_rows,
                    r.counts.source_rows,
                    r.counts.owner_details,
                    r.counts.reply_tickets,
                    r.counts.maintenance_targets,
                    r.counts.ready_targets,
                    r.allocation.logical_bytes,
                    r.allocation.allocated_bytes,
                    r.allocation.high_water_allocated_bytes,
                    r.allocation.reserved_tail_bytes,
                    r.allocation.cleanup_headroom_bytes,
                    r.allocation.work.attempts,
                    r.allocation.work.requested_bytes,
                    r.allocation.work.admitted_jobs,
                    r.allocation.work.refusals,
                    r.allocation.work.observations,
                    r.allocation.work.freelist_queries,
                    r.database_pages,
                    r.free_pages,
                    r.debt_upper_bytes,
                ]),
            )?
        }
        None => rows.row(28, 0, None)?,
    }
    let Some(w) = &observed.work else {
        // These two metadata bits explicitly state that the job's counter
        // schema is unavailable. No absent work is encoded as successful zero.
        return rows.row(30, 0, Some(&[observed.attempted as u64, 0]));
    };
    rows.row(
        30,
        0,
        Some(&[
            observed.attempted as u64,
            1,
            w.sql.attempts,
            w.sql.executions,
            w.sql.rows_returned,
            w.sql.rows_changed,
            w.sql.direct_rows_changed,
            w.sql.returned_blob_bytes,
            w.sql.returned_value_bytes,
            w.sql.vm_steps,
            w.sql.fullscan_steps,
            w.sql.sorts,
            w.sql.autoindex_rows,
            w.sql.reprepares,
            w.sql.bound_bytes,
            w.sql.sql_bytes,
            w.sql.statement_memory_samples,
            w.sql.statement_memory_sample_bytes,
            w.sql.elapsed_ns,
            w.payload.write_input_bytes,
            w.payload.write_cells,
            w.payload.partial_write_cells,
            w.payload.cell_copy_bytes,
            w.payload.cell_zeroed_bytes,
            w.payload.read_window_zeroed_bytes,
            w.payload.read_local_copy_bytes,
            w.allocation.attempts,
            w.allocation.requested_bytes,
            w.allocation.admitted_jobs,
            w.allocation.refusals,
            w.allocation.observations,
            w.allocation.freelist_queries,
            w.lock_wait_ns,
            w.hold_ns,
        ]),
    )
}
