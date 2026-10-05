//! One owner connection; no database, schema or connection per Workspace.
use crate::{
    metrics, DatabaseProfile, DatabaseWork, OverlayError, OverlayResult, ProfileConfig,
    StatementKind,
};
use rusqlite::{Connection, OpenFlags, ToSql};
use std::{
    cell::{Cell, RefCell},
    fs::OpenOptions,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_ENGINE: AtomicU64 = AtomicU64::new(1);

/// Daemon-local engine. The daemon schedules short jobs on its owning thread.
pub struct Overlay {
    pub(crate) identity: u64,
    pub(crate) connection: Connection,
    pub(crate) work: RefCell<DatabaseWork>,
    pub(crate) quarantined: Cell<bool>,
    /// Connection-local readiness hints, never namespace/owner data. False is
    /// established only by an exact empty ready query; enqueue/release marks
    /// possible work. A rolled-back enqueue can leave only a false positive.
    pub(crate) maintenance_ready: Cell<bool>,
    pub(crate) closed_ready: Cell<bool>,
    profile: DatabaseProfile,
    allocation: crate::database::allocation::Allocation,
}
impl Overlay {
    /// Creates fresh disposable state once. An existing path is refused.
    /// Failed startup leaves its created artifact in custody; no implicit replay.
    pub fn create(path: &Path, config: ProfileConfig) -> OverlayResult<Self> {
        if !cfg!(any(target_os = "macos", target_os = "linux")) {
            return Err(OverlayError::UnsupportedPlatform);
        }
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(path)?;
        let allocation = crate::database::allocation::Allocation::new(file, path)?;
        allocation.admit(false, 0)?;
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        let mut profile = crate::profile::initialize(&connection, config)?;
        let work = RefCell::new(DatabaseWork::default());
        // Startup DDL is finite schema work, independently recorded from jobs.
        use rusqlite::fallible_iterator::FallibleIterator;
        let mut batch = rusqlite::Batch::new(&connection, include_str!("../../sql/schema.sql"));
        while let Some(mut statement) = batch.next()? {
            statement.execute([])?;
        }
        let mut accounting =
            rusqlite::Batch::new(&connection, include_str!("../../sql/accounting.sql"));
        while let Some(mut statement) = accounting.next()? {
            statement.execute([])?;
        }
        profile.schema_version =
            connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        let application: i64 =
            connection.query_row("PRAGMA application_id", [], |row| row.get(0))?;
        if profile.schema_version != 14 || application != 1279676210 {
            return Err(OverlayError::Invalid("overlay schema readback"));
        }
        connection.set_prepared_statement_cache_capacity(48);
        // A process-local capability cannot cross daemon owners even when their
        // local namespace/incarnation numbers happen to match. Remote routing
        // still requires the authenticated runtime's daemon incarnation.
        let identity = NEXT_ENGINE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .map_err(|_| OverlayError::Invalid("engine identity exhausted"))?;
        Ok(Self {
            identity,
            connection,
            work,
            quarantined: Cell::new(false),
            maintenance_ready: Cell::new(false),
            closed_ready: Cell::new(false),
            profile,
            allocation,
        })
    }
    /// Selected settings actually read back from this initialized connection.
    pub fn profile(&self) -> &DatabaseProfile {
        &self.profile
    }
    /// Bounded cumulative statement-family observations, not phase-local memory.
    pub fn diagnostics(&self) -> DatabaseWork {
        *self.work.borrow()
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
