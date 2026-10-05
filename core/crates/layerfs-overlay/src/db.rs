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
};

/// Daemon-local engine. The daemon schedules short jobs on its owning thread.
pub struct Overlay {
    pub(crate) connection: Connection,
    pub(crate) work: RefCell<DatabaseWork>,
    pub(crate) quarantined: Cell<bool>,
    profile: DatabaseProfile,
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
        drop(file);
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        let mut profile = crate::profile::initialize(&connection, config)?;
        let work = RefCell::new(DatabaseWork::default());
        // Startup DDL is finite schema work, independently recorded from jobs.
        use rusqlite::fallible_iterator::FallibleIterator;
        let mut batch = rusqlite::Batch::new(&connection, include_str!("../sql/schema.sql"));
        while let Some(mut statement) = batch.next()? {
            statement.execute([])?;
        }
        profile.schema_version =
            connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        let application: i64 =
            connection.query_row("PRAGMA application_id", [], |row| row.get(0))?;
        if profile.schema_version != 3 || application != 1279676210 {
            return Err(OverlayError::Invalid("overlay schema readback"));
        }
        connection.set_prepared_statement_cache_capacity(48);
        Ok(Self {
            connection,
            work,
            quarantined: Cell::new(false),
            profile,
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
        let pages = self.query(StatementKind::Startup, "PRAGMA page_count", &[], 0, |r| {
            unsigned(r, 0)
        })?[0];
        let free = self.query(
            StatementKind::Startup,
            "PRAGMA freelist_count",
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
    pub(crate) fn query<T>(
        &self,
        kind: StatementKind,
        sql: &str,
        params: &[&dyn ToSql],
        bytes: u64,
        decode: impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    ) -> OverlayResult<Vec<T>> {
        self.available()?;
        let result = metrics::query(
            &self.connection,
            &self.work,
            kind,
            sql,
            params,
            bytes,
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
        self.available()?;
        self.execute(StatementKind::Begin, "BEGIN IMMEDIATE", &[], 0)?;
        let result = job();
        match result {
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
