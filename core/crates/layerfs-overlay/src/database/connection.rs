//! One owner connection; no database, schema or connection per Workspace.
use crate::{
    metrics, DatabaseProfile, DatabaseWork, OverlayError, OverlayResult, ProfileConfig,
    StatementKind,
};
use rusqlite::{Connection, ToSql};
use std::{
    cell::{Cell, RefCell},
    path::Path,
};

/// Daemon-local engine. The daemon runs short jobs on it one exclusive turn
/// at a time, on its owner thread or on the thread that submitted the job.
pub struct Overlay {
    pub(crate) identity: u64,
    pub(crate) connection: Connection,
    pub(crate) work: RefCell<DatabaseWork>,
    pub(crate) payload_work: Cell<crate::PayloadWork>,
    pub(crate) quarantined: Cell<bool>,
    /// Connection-local readiness hints, never namespace/owner data. False is
    /// established only by an exact empty ready query; enqueue/release marks
    /// possible work. A rolled-back enqueue can leave only a false positive.
    pub(crate) maintenance_ready: Cell<bool>,
    pub(crate) closed_ready: Cell<bool>,
    /// What the running atomic job has asked of this connection.
    pub(super) transaction: Cell<Transaction>,
    /// Next owner identity of this engine. The database is created by this
    /// connection and never reopened, so the counter needs no stored row.
    pub(crate) next_owner: Cell<u64>,
    pub(super) profile: DatabaseProfile,
    pub(super) allocation: crate::database::allocation::Allocation,
}
/// An atomic job starts its SQLite transaction at its first writing statement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Transaction {
    None,
    /// The job has executed no writing statement; its reads ran in autocommit.
    Wanted {
        cleanup: bool,
    },
    /// Admission and BEGIN are executing for the job's first writing statement.
    Beginning,
    Begun,
}
impl Overlay {
    /// Creates fresh disposable state once. An existing path is refused.
    /// Failed startup leaves its created artifact in custody; no implicit replay.
    pub fn create(path: &Path, config: ProfileConfig) -> OverlayResult<Self> {
        Self::create_observed(path, config).result
    }

    /// Creates one engine while retaining startup work on success or failure.
    pub fn create_observed(path: &Path, config: ProfileConfig) -> crate::Creation {
        crate::database::startup::create(path, config)
    }
    /// Selected settings actually read back from this initialized connection.
    pub fn profile(&self) -> &DatabaseProfile {
        &self.profile
    }
    /// Bounded cumulative statement-family observations, not phase-local memory.
    pub fn diagnostics(&self) -> DatabaseWork {
        *self.work.borrow()
    }
    /// Whether a maintenance turn may find work: the connection-local hints
    /// that `maintain` and `reclaim_closed` consult. False is exact; true can
    /// be a false positive that the next turn clears.
    pub fn maintenance_pending(&self) -> bool {
        self.maintenance_ready.get() || self.closed_ready.get()
    }
    /// Codec/composed-window copies from the same exclusive owner scope.
    pub fn payload_work(&self) -> crate::PayloadWork {
        self.payload_work.get()
    }
    /// Physical page-count and freelist observations; not exclusive Workspace bytes.
    pub fn pages(&self) -> OverlayResult<(u64, u64)> {
        self.available()?;
        // Pagecount expires its own VM. Prepare it once for this observation
        // rather than caching an expired statement and automatically repreparing.
        let pages = self.query_using(
            StatementKind::Startup,
            "PRAGMA main.page_count",
            &[],
            0,
            false,
            |r| unsigned(r, 0),
        )?[0];
        let free = self.query(
            StatementKind::Startup,
            "PRAGMA main.freelist_count",
            &[],
            0,
            |r| unsigned(r, 0),
        )?[0];
        Ok((pages, free))
    }
    pub(crate) fn available(&self) -> OverlayResult<()> {
        if self.quarantined.get() {
            Err(OverlayError::Quarantined)
        } else {
            Ok(())
        }
    }
    pub(crate) fn check_route(&self, route: crate::Route) -> OverlayResult<()> {
        self.available()?;
        if route.engine != self.identity {
            return Err(OverlayError::Stale);
        }
        Ok(())
    }
    pub(crate) fn query<T>(
        &self,
        kind: StatementKind,
        sql: &str,
        params: &[&dyn ToSql],
        bytes: u64,
        decode: impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    ) -> OverlayResult<Vec<T>> {
        self.query_using(kind, sql, params, bytes, true, decode)
    }
    fn query_using<T>(
        &self,
        kind: StatementKind,
        sql: &str,
        params: &[&dyn ToSql],
        bytes: u64,
        cached: bool,
        decode: impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    ) -> OverlayResult<Vec<T>> {
        self.available()?;
        let begin = || self.begin();
        let wanted = matches!(self.transaction.get(), Transaction::Wanted { .. });
        let result = metrics::query(
            &self.connection,
            &self.work,
            kind,
            metrics::Query {
                sql,
                params,
                bound_bytes: bytes,
                cached,
                before_write: if wanted { Some(&begin) } else { None },
            },
            decode,
        );
        match result {
            Err(cause) if cause.unsafe_database_state() => {
                self.quarantined.set(true);
                Err(OverlayError::Uncertain {
                    cause: Box::new(cause),
                    completion: None,
                })
            }
            result => result,
        }
    }
    pub(crate) fn execute(
        &self,
        kind: StatementKind,
        sql: &str,
        params: &[&dyn ToSql],
        bytes: u64,
    ) -> OverlayResult<u64> {
        self.query(kind, sql, params, bytes, |_| Ok(()))?;
        Ok(self.connection.changes())
    }
    pub(crate) fn atomic<T>(&self, job: impl FnOnce() -> OverlayResult<T>) -> OverlayResult<T> {
        self.transaction(false, job)
    }
    pub(crate) fn atomic_cleanup<T>(
        &self,
        job: impl FnOnce() -> OverlayResult<T>,
    ) -> OverlayResult<T> {
        self.transaction(true, job)
    }
    /// Allocation-call counters without performing filesystem or SQL I/O.
    pub fn allocation_work(&self) -> crate::AllocationWork {
        self.allocation.work()
    }
    pub fn allocation(&self) -> OverlayResult<crate::AllocationState> {
        self.available()?;
        self.allocation.state()
    }
    /// Runs one job atomically. Reads before the job's first writing statement
    /// execute in autocommit: this connection is the only one, holds the
    /// exclusive lock and runs one job at a time, so they see the same state.
    /// A job that writes nothing begins, commits and admits nothing.
    fn transaction<T>(
        &self,
        cleanup: bool,
        job: impl FnOnce() -> OverlayResult<T>,
    ) -> OverlayResult<T> {
        self.available()?;
        if self.transaction.get() != Transaction::None {
            return Err(OverlayError::Invalid("nested transaction"));
        }
        self.transaction.set(Transaction::Wanted { cleanup });
        let result = job();
        if self.transaction.replace(Transaction::None) != Transaction::Begun {
            return result;
        }
        let failure = match result {
            Ok(value) => match self.execute(StatementKind::Commit, "COMMIT", &[], 0) {
                // A commit never shrinks this file. The next admission's own
                // observation sees the committed length and allocation.
                Ok(_) => return Ok(value),
                Err(cause) => {
                    self.quarantined.set(true);
                    OverlayError::Uncertain {
                        cause: Box::new(cause),
                        completion: None,
                    }
                }
            },
            Err(cause) => {
                if self.quarantined.get() {
                    return Err(cause);
                }
                if !self.connection.is_autocommit() {
                    if let Err(error) = self.execute(StatementKind::Rollback, "ROLLBACK", &[], 0) {
                        self.quarantined.set(true);
                        return Err(OverlayError::Uncertain {
                            cause: Box::new(cause),
                            completion: Some(Box::new(error)),
                        });
                    }
                }
                cause
            }
        };
        // Observe rollback truncation before another admission or resource
        // report can credit discarded physical tail. An unknown identity or
        // metadata result quarantines this connection with original custody.
        if let Err(cause) = self.allocation.state() {
            self.quarantined.set(true);
            return Err(OverlayError::Uncertain {
                cause: Box::new(cause),
                completion: Some(Box::new(failure)),
            });
        }
        Err(failure)
    }
    /// Admission and BEGIN for the first writing statement of an atomic job,
    /// before that statement executes. A failure is returned by that statement
    /// and leaves no transaction open.
    fn begin(&self) -> OverlayResult<()> {
        let Transaction::Wanted { cleanup } = self.transaction.get() else {
            return Ok(());
        };
        // BEGIN IMMEDIATE is itself a writing statement.
        self.transaction.set(Transaction::Beginning);
        let begun = self
            .admit(cleanup)
            .and_then(|()| self.execute(StatementKind::Begin, "BEGIN IMMEDIATE", &[], 0));
        self.transaction.set(match begun {
            Ok(_) => Transaction::Begun,
            Err(_) => Transaction::Wanted { cleanup },
        });
        begun.map(|_| ())
    }
    fn admit(&self, cleanup: bool) -> OverlayResult<()> {
        // page_count includes Expire in the supported SQLite VM. Keep it out
        // of the hot admission path; committed dense descriptor length supplies
        // the physical bound, and freelist_count reads one nonexpiring cookie.
        self.allocation.freelist_query();
        let free = self.query(
            StatementKind::Startup,
            "PRAGMA main.freelist_count",
            &[],
            0,
            |r| unsigned(r, 0),
        )?[0];
        let admission = match self.allocation.state() {
            Ok(state)
                if state.logical_bytes != 0
                    && state.logical_bytes % 4096 == 0
                    && free <= state.logical_bytes / 4096 =>
            {
                self.allocation.admit(cleanup, free * 4096, state)
            }
            Ok(_) => Err(OverlayError::Invalid(
                "committed database length/accounting",
            )),
            Err(error) => Err(error),
        };
        match admission {
            Err(cause)
                if !matches!(
                    cause,
                    OverlayError::Reservation { .. } | OverlayError::UnsupportedPlatform
                ) =>
            {
                self.quarantined.set(true);
                Err(OverlayError::Uncertain {
                    cause: Box::new(cause),
                    completion: None,
                })
            }
            admission => admission,
        }
    }
}
pub(crate) fn integer(value: u64) -> OverlayResult<i64> {
    i64::try_from(value).map_err(|_| OverlayError::Invalid("SQLite signed identity/offset"))
}

pub(crate) fn unsigned(row: &rusqlite::Row<'_>, column: usize) -> rusqlite::Result<u64> {
    let value: i64 = row.get(column)?;
    u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(column, value))
}
