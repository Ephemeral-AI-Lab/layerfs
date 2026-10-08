//! One original route-free Resources job, explicitly charged to observation.
use super::Records;
use crate::{Command, Completion, OwnerClient, OwnerError, Response};
use std::{
    fmt,
    io::{self, Write},
    sync::Mutex,
};

/// One original global-resource observation with fixed copied counters and exact failure custody.
pub struct ResourceObservation {
    resources: Option<layerfs_overlay::Resources>,
    admitted: bool,
    work: Option<Work>,
    failure: Option<ResourceFailure>,
}
/// Exact original resource-observer failure, separate from the control result.
pub enum ResourceFailure {
    /// Admission refused the original command before any observer SQL attempt.
    Admission {
        /// Original admission refusal, without conversion to a guessed category.
        cause: OwnerError,
        /// Exact original unattempted global Resources command.
        command: Command,
    },
    /// Original admitted completion could not be obtained; no result is guessed.
    Await(OwnerError),
    /// Original failed or unexpected completion, including its exact work and credit.
    Completion(Completion),
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
            Self::Admission { .. } => "resource admission failure",
            Self::Await(_) => "resource completion wait failure",
            Self::Completion(_) => "original resource completion retained",
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
    sql: layerfs_overlay::StatementWork,
    payload: layerfs_overlay::PayloadWork,
    allocation: layerfs_overlay::AllocationWork,
    parked_turns: u64,
    queue_wait_ns: u64,
    service_ns: u64,
}
impl ResourceObservation {
    /// Attempts one existing route-free global Resources job through an operator
    /// owner. It waits only for that original admitted completion, never retries,
    /// and retains exact admission, completion or execution failure.
    pub fn acquire(owner: &OwnerClient) -> Self {
        let pending = match owner.try_submit(None, Command::Resources { global: true }) {
            Ok(pending) => pending,
            Err((cause, command)) => {
                return Self {
                    resources: None,
                    admitted: false,
                    work: None,
                    failure: Some(ResourceFailure::Admission { cause, command }),
                }
            }
        };
        let completion = match pending.wait() {
            Ok(completion) => completion,
            Err(cause) => {
                return Self {
                    resources: None,
                    admitted: true,
                    work: None,
                    failure: Some(ResourceFailure::Await(cause)),
                }
            }
        };
        let resources = if let Ok(Response::Resources(resources)) = completion.result() {
            Some(**resources)
        } else {
            // The exact result remains inside Completion below, not an erased error.
            None
        };
        let work = completion.work();
        let work = Work {
            sql: work.sql.total(),
            payload: work.payload,
            allocation: work.allocation,
            parked_turns: work.parked_turns,
            queue_wait_ns: work.queue_wait_ns,
            service_ns: work.service_ns,
        };
        // Copies are fixed schemas in the existing admitted observation window.
        // Successful caller credit is dropped; failure retains the exact original
        // completion and its charge for existing diagnostic-failure custody. The
        // publisher can also hold its original Arc transiently after the wake.
        let failure = if resources.is_some() {
            drop(completion);
            None
        } else {
            Some(ResourceFailure::Completion(completion))
        };
        Self {
            resources,
            admitted: true,
            work: Some(work),
            failure,
        }
    }
}
pub(super) fn emit<W: Write>(
    rows: &mut Records<'_, W>,
    observed: &ResourceObservation,
) -> io::Result<()> {
    match &observed.resources {
        Some(r) => rows.row(
            28,
            0,
            Some(&[
                r.counts.wait_refs,
                r.counts.namespaces,
                r.counts.inode_rows,
                r.counts.directory_entry_rows,
                r.counts.payload_cells,
                r.counts.payload_bytes,
                r.counts.shrink_rows,
                r.counts.operation_record_rows,
                r.counts.operation_record_bytes,
                r.counts.orphan_rows,
                r.counts.owner_rows,
                r.counts.source_rows,
                r.counts.owner_details,
                r.counts.reply_tickets,
                r.counts.retire_targets,
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
        )?,
        None => rows.row(28, 0, None)?,
    }
    let Some(w) = &observed.work else {
        // These two metadata bits explicitly state that the job's counter
        // schema is unavailable. No absent work is encoded as successful zero.
        return rows.row(30, 0, Some(&[observed.admitted as u64, 0]));
    };
    rows.row(
        30,
        0,
        Some(&[
            observed.admitted as u64,
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
            w.parked_turns,
            w.queue_wait_ns,
            w.service_ns,
        ]),
    )
}
