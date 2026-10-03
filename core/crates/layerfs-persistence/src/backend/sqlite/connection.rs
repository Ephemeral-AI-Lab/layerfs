//! Durable host connection and recorded work; no busy retry or fallback.
use super::{query, rows};
use crate::backend::records::BackendError;
use rusqlite::{limits::Limit, Connection, OpenFlags};
use std::{cell::RefCell, path::Path, sync::Mutex, time::Instant};
/// The selected SQLite settings and actual runtime capabilities.
#[derive(Clone, Debug)]
pub struct ConnectionProfile {
    /// Immutable profile identity.
    pub identity: &'static str,
    /// Linked engine version.
    pub sqlite_version: String,
    /// Platform required by this profile.
    pub platform: &'static str,
    /// VFS selection; no direct VFS-name observation is claimed.
    pub vfs_selection: &'static str,
    /// SQLite compile options.
    pub compile_options: Vec<String>,
    /// Selected journal mode, checked on every relevant connection.
    pub journal_mode: String,
    /// FULL is 2.
    pub synchronous: i64,
    /// Foreign keys must be enabled.
    pub foreign_keys: i64,
    /// macOS full-sync selection.
    pub fullfsync: i64,
    /// macOS checkpoint full-sync selection.
    pub checkpoint_fullfsync: i64,
    /// Database page bytes.
    pub page_size: i64,
    /// Automatic checkpoint page interval.
    pub wal_autocheckpoint: i64,
    /// Retained journal-byte limit, not a WAL high-water guarantee.
    pub journal_size_limit: i64,
    /// Negative KiB page-cache selection.
    pub cache_size: i64,
    /// Busy handler bound, zero.
    pub busy_timeout: i64,
    /// Memory-map window, zero.
    pub mmap_size: i64,
    /// Actual bind-variable limit.
    pub variable_limit: usize,
    /// Actual SQL-byte limit.
    pub sql_length_limit: usize,
    /// Actual row/BLOB byte limit.
    pub length_limit: usize,
    /// Actual column count limit, required by the full history schema.
    pub column_limit: usize,
}
/// Counts and inclusive wall from actual attempted SQLite work.
#[derive(Clone, Copy, Debug, Default)]
pub struct SqlWork {
    /// Prepared statements executed, including transaction controls and failures.
    pub statements: u64,
    /// Actual sqlite3_stmt_status VM steps, not EXPLAIN instruction counts.
    pub vm_steps: u64,
    /// Input binding bytes; integer scalars count eight.
    pub bound_bytes: u64,
    /// Inclusive statement execution wall.
    pub statement_ns: u64,
    /// Commit statement wall, nested within statement and transaction wall.
    pub commit_ns: u64,
    /// Successfully begun logical transactions.
    pub transactions: u64,
    /// Successfully begun mutation transactions.
    pub write_transactions: u64,
    /// Acknowledged mutation commits; not a count of observed sync system calls.
    pub write_commits: u64,
    /// Acknowledged commits, including read transactions.
    pub commits: u64,
    /// Definite rollbacks.
    pub rollbacks: u64,
    /// Complete attempted transaction wall including lock/commit work.
    pub transaction_ns: u64,
    /// Attempted final body INSERTs, including units later rolled back.
    pub sealed_inserts: u64,
    /// Body bytes submitted by those attempted final INSERTs.
    pub sealed_body_bytes: u64,
    /// Explicit checkpoint wall, outside SQL statement spans.
    pub checkpoint_ns: u64,
}
/// One explicit checkpoint result; pending frames remain visible.
#[derive(Clone, Copy, Debug)]
pub struct Checkpoint {
    /// SQLite reported an obstructed checkpoint.
    pub busy: bool,
    /// WAL frames before checkpoint, -1 when no WAL exists.
    pub log_frames: i64,
    /// Frames checkpointed, -1 when no WAL exists.
    pub checkpointed_frames: i64,
    /// Time spent performing the checkpoint.
    pub wall_ns: u64,
}
pub(crate) struct State {
    pub(crate) connection: Connection,
    pub(crate) quarantined: bool,
    pub(crate) work: RefCell<SqlWork>,
}
pub(crate) struct Session {
    pub(crate) state: Mutex<State>,
    pub(crate) writable: bool,
    pub(crate) profile: ConnectionProfile,
}
impl Session {
    pub(crate) fn connect(path: &Path, writable: bool, create: bool) -> Result<Self, BackendError> {
        if !cfg!(target_os = "macos") {
            return Err(BackendError::Integrity);
        }
        let flags = if writable {
            OpenFlags::SQLITE_OPEN_READ_WRITE
        } else {
            OpenFlags::SQLITE_OPEN_READ_ONLY
        } | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let connection = Connection::open_with_flags(path, flags).map_err(rows::error)?;
        connection
            .busy_timeout(std::time::Duration::ZERO)
            .map_err(rows::error)?;
        let work = RefCell::new(SqlWork::default());
        connection
            .set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)
            .map_err(rows::error)?;
        if create {
            query::run(&connection, "PRAGMA page_size=4096", vec![], &work)?;
        }
        if writable && create {
            let values = query::run(&connection, "PRAGMA journal_mode=WAL", vec![], &work)?;
            if values
                .first()
                .ok_or(BackendError::Integrity)?
                .get::<String>(0)?
                != "wal"
            {
                return Err(BackendError::Integrity);
            }
        }
        if !create {
            let mode = query::run(&connection, "PRAGMA journal_mode", vec![], &work)?
                .first()
                .ok_or(BackendError::Integrity)?
                .get::<String>(0)?;
            if mode != "wal" {
                return Err(BackendError::Integrity);
            }
        }
        for (name, value) in [
            ("synchronous", 2),
            ("foreign_keys", 1),
            ("fullfsync", 1),
            ("checkpoint_fullfsync", 1),
            ("wal_autocheckpoint", 1000),
            ("journal_size_limit", 4194304),
            ("cache_size", -2048),
            ("mmap_size", 0),
            ("temp_store", 2),
        ] {
            query::run(
                &connection,
                &format!("PRAGMA {name}={value}"),
                vec![],
                &work,
            )?;
        }
        let integer = |name| {
            query::run(&connection, &format!("PRAGMA {name}"), vec![], &work)?
                .first()
                .ok_or(BackendError::Integrity)?
                .get::<i64>(0)
        };
        let journal_mode = query::run(&connection, "PRAGMA journal_mode", vec![], &work)?
            .first()
            .ok_or(BackendError::Integrity)?
            .get::<String>(0)?;
        let compile_options = query::run(&connection, "PRAGMA compile_options", vec![], &work)?
            .iter()
            .map(|r| r.get::<String>(0))
            .collect::<Result<Vec<_>, _>>()?;
        let profile = ConnectionProfile {
            identity: "sqlite-wal-full-macos-fullfsync-v1",
            sqlite_version: rusqlite::version().to_owned(),
            platform: "macos",
            vfs_selection: "SQLite default; name not directly observed",
            compile_options,
            journal_mode,
            synchronous: integer("synchronous")?,
            foreign_keys: integer("foreign_keys")?,
            fullfsync: integer("fullfsync")?,
            checkpoint_fullfsync: integer("checkpoint_fullfsync")?,
            page_size: integer("page_size")?,
            wal_autocheckpoint: integer("wal_autocheckpoint")?,
            journal_size_limit: integer("journal_size_limit")?,
            cache_size: integer("cache_size")?,
            busy_timeout: integer("busy_timeout")?,
            mmap_size: integer("mmap_size")?,
            variable_limit: connection
                .limit(Limit::SQLITE_LIMIT_VARIABLE_NUMBER)
                .map_err(rows::error)? as usize,
            sql_length_limit: connection
                .limit(Limit::SQLITE_LIMIT_SQL_LENGTH)
                .map_err(rows::error)? as usize,
            column_limit: connection
                .limit(Limit::SQLITE_LIMIT_COLUMN)
                .map_err(rows::error)? as usize,
            length_limit: connection
                .limit(Limit::SQLITE_LIMIT_LENGTH)
                .map_err(rows::error)? as usize,
        };
        if profile.journal_mode != "wal"
            || profile.synchronous != 2
            || profile.foreign_keys != 1
            || profile.fullfsync != 1
            || profile.checkpoint_fullfsync != 1
            || profile.page_size != 4096
            || profile.wal_autocheckpoint != 1000
            || profile.journal_size_limit != 4194304
            || profile.cache_size != -2048
            || profile.busy_timeout != 0
            || profile.mmap_size != 0
            || profile.column_limit < 13
            || profile.length_limit < layerfs_storage::policy::SINGLETON_PACK_LIMIT + 1024
            || profile.sql_length_limit < 4096
            || profile.variable_limit < 6
        {
            return Err(BackendError::Integrity);
        }
        Ok(Self {
            state: Mutex::new(State {
                connection,
                quarantined: false,
                work,
            }),
            writable,
            profile,
        })
    }
    pub(crate) fn diagnostics(&self) -> Result<SqlWork, BackendError> {
        let s = self.state.try_lock().map_err(|_| BackendError::Busy)?;
        let w = *s.work.borrow();
        Ok(w)
    }
    pub(crate) fn checkpoint(&self) -> Result<Checkpoint, BackendError> {
        if !self.writable {
            return Err(BackendError::ReadOnly);
        }
        let mut s = self.state.try_lock().map_err(|_| BackendError::Busy)?;
        if s.quarantined {
            return Err(BackendError::Unknown);
        }
        let start = Instant::now();
        let rows = match query::run(
            &s.connection,
            "PRAGMA wal_checkpoint(TRUNCATE)",
            vec![],
            &s.work,
        ) {
            Ok(rows) => rows,
            Err(e) => {
                if e == BackendError::Unknown {
                    s.quarantined = true;
                }
                return Err(e);
            }
        };
        let r = rows.first().ok_or(BackendError::Integrity)?;
        let wall_ns = start.elapsed().as_nanos() as u64;
        s.work.borrow_mut().checkpoint_ns += wall_ns;
        Ok(Checkpoint {
            busy: r.get::<i64>(0)? != 0,
            log_frames: r.get(1)?,
            checkpointed_frames: r.get(2)?,
            wall_ns,
        })
    }
}
