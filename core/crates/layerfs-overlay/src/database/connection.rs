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

/// Daemon-local engine. The daemon schedules short jobs on its owning thread.
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
    pub(super) profile: DatabaseProfile,
    pub(super) allocation: crate::database::allocation::Allocation,
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
        let result = metrics::query(
            &self.connection,
            &self.work,
            kind,
            metrics::Query {
                sql,
                params,
                bound_bytes: bytes,
                cached,
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
    fn transaction<T>(
        &self,
        cleanup: bool,
        job: impl FnOnce() -> OverlayResult<T>,
    ) -> OverlayResult<T> {
        self.available()?;
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
        let physical = self.allocation.state();
        let admission = match physical {
            Ok(state)
                if state.logical_bytes != 0
                    && state.logical_bytes % 4096 == 0
                    && free <= state.logical_bytes / 4096 =>
            {
                self.allocation.admit(cleanup, free * 4096)
            }
            Ok(_) => Err(OverlayError::Invalid(
                "committed database length/accounting",
            )),
            Err(error) => Err(error),
        };
        if let Err(cause) = admission {
            if !matches!(
                cause,
                OverlayError::Reservation { .. } | OverlayError::UnsupportedPlatform
            ) {
                self.quarantined.set(true);
                return Err(OverlayError::Uncertain {
                    cause: Box::new(cause),
                    completion: None,
                });
            }
            return Err(cause);
        }
        self.execute(StatementKind::Begin, "BEGIN IMMEDIATE", &[], 0)?;
        let result = job();
        let finished = match result {
            Ok(value) => match self.execute(StatementKind::Commit, "COMMIT", &[], 0) {
                Ok(_) => Ok(value),
                Err(cause) => {
                    self.quarantined.set(true);
                    Err(OverlayError::Uncertain {
                        cause: Box::new(cause),
                        completion: None,
                    })
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
                Err(cause)
            }
        };
        // Observe rollback truncation before another admission or resource
        // report can credit discarded physical tail. An unknown identity or
        // metadata result quarantines this connection with original custody.
        if let Err(cause) = self.allocation.state() {
            self.quarantined.set(true);
            return Err(OverlayError::Uncertain {
                cause: Box::new(cause),
                completion: finished.err().map(Box::new),
            });
        }
        finished
    }
}
pub(crate) fn integer(value: u64) -> OverlayResult<i64> {
    i64::try_from(value).map_err(|_| OverlayError::Invalid("SQLite signed identity/offset"))
}

pub(crate) fn unsigned(row: &rusqlite::Row<'_>, column: usize) -> rusqlite::Result<u64> {
    let value: i64 = row.get(column)?;
    u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(column, value))
}
