//! Durable host connection and recorded work; no busy retry or fallback.
use super::{profile, query, rows};
use crate::backend::records::BackendError;
use crate::SqlitePersistenceProfile;
use rusqlite::{limits::Limit, Connection, OpenFlags};
use std::{cell::RefCell, path::Path, sync::Mutex, time::Instant};
/// The selected SQLite settings and actual runtime capabilities.
#[derive(Clone, Debug)]
pub struct ConnectionProfile {
    /// Explicit selected completion/durability contract.
    pub persistence: SqlitePersistenceProfile,
    /// Actual physical schema selected explicitly or from the supported stored version.
    pub pack_layout: crate::SqlitePackLayout,
    /// Whether the stored schema version carries the acquisition tables.
    pub acquisition: crate::SqliteAcquisitionSchema,
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
    /// FULL is 2; OFF is 0 for Disposable.
    pub synchronous: i64,
    /// Foreign keys must be enabled.
    pub foreign_keys: i64,
    /// macOS full-sync selection.
    pub fullfsync: i64,
    /// macOS checkpoint full-sync selection.
    pub checkpoint_fullfsync: i64,
    /// Database page bytes.
    pub page_size: i64,
    /// Stored page-reclamation mode: 0 for retained Stores, 2 for new acquisition Stores.
    pub auto_vacuum: i64,
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
    /// Memory temporary storage selection, 2.
    pub temp_store: i64,
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
    /// Actual statement full-scan steps, including failed attempts.
    pub fullscan_steps: u64,
    /// Actual statement sort operations.
    pub sorts: u64,
    /// Rows inserted into automatic query indexes.
    pub autoindex_rows: u64,
    /// Automatic statement reprepares observed, without an application retry.
    pub reprepares: u64,
    /// Successfully decoded returned rows, including prefixes of failed statements.
    pub returned_rows: u64,
    /// Input binding bytes; integer scalars count eight.
    pub bound_bytes: u64,
    /// Accumulated prepare, execution/counter and lease-return wall. Shared
    /// leases pay prepare/return once per window; transaction and operation
    /// clocks retain surrounding work.
    pub statement_ns: u64,
    /// Prepare/cache checkout, bind/query creation, next (step plus DONE reset),
    /// row mapping, explicit cursor drop, status counters, cached statement drop.
    /// Only the ordinary measured query wrapper populates these seven phases.
    pub statement_phases: [super::statement_work::StatementPhaseWork; 7],
    /// The same seven phase observations restricted to COMMIT statements.
    pub commit_phases: [super::statement_work::StatementPhaseWork; 7],
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
    /// Attempted read-only incremental BLOB opens.
    pub blob_open_calls: u64,
    /// Attempted moves of an owned read BLOB cursor within one acquisition call.
    pub blob_reopen_calls: u64,
    /// Attempted exact offset/length BLOB reads, including failures.
    pub blob_read_calls: u64,
    /// Requested bytes of those reads, not VFS/device bytes.
    pub blob_requested_bytes: u64,
    /// Bytes returned by successful exact reads.
    pub blob_read_bytes: u64,
    /// Inclusive actual BLOB read-call wall; hashing/planning are excluded.
    pub blob_read_ns: u64,
    /// Attempted checked closes of incremental BLOB handles.
    pub blob_close_calls: u64,
    /// Successful shared bounded main-file physical reservations (not SQL statements).
    pub preallocation_calls: u64,
    /// Bytes reserved without changing logical length.
    pub preallocation_bytes: u64,
    /// Inclusive reservation/custody/source-close wall.
    pub preallocation_ns: u64,
    /// Checked temporary descriptor close wall nested in reservation.
    pub preallocation_close_ns: u64,
    /// Explicit checkpoint wall, outside SQL statement spans.
    pub checkpoint_ns: u64,
}
/// Actual allocation descriptor identity observed during completion; not ownership.
#[derive(Clone, Copy, Debug)]
pub struct AllocationIdentity {
    /// Descriptor number at this observation; never a stable handle capability.
    pub descriptor: i32,
    /// Filesystem device identity.
    pub device: u64,
    /// File inode identity.
    pub inode: u64,
    /// Logical bytes observed before release.
    pub logical_bytes: u64,
}
#[derive(Clone, Copy)]
pub(crate) struct AllocationRelease {
    pub before: u64,
    pub after: u64,
    pub source: Option<AllocationIdentity>,
    pub transfer_ns: u64,
    pub scratch_close_ns: u64,
    pub source_close_ns: u64,
}
/// One explicit checkpoint result; pending frames remain visible.
#[derive(Clone, Copy, Debug)]
pub struct Checkpoint {
    /// Profile whose completion work was performed.
    pub persistence: SqlitePersistenceProfile,
    /// Whether the selected profile required an actual WAL checkpoint.
    pub wal_checkpoint_performed: bool,
    /// SQLite reported an obstructed checkpoint.
    pub busy: bool,
    /// WAL frames before checkpoint, -1 when no WAL exists.
    pub log_frames: i64,
    /// Frames checkpointed, -1 when no WAL exists.
    pub checkpointed_frames: i64,
    /// Time spent performing the checkpoint and allocation release.
    pub wall_ns: u64,
    /// Physical main-file allocation before unused-extents release; absent when busy.
    pub allocation_before_bytes: Option<u64>,
    /// Physical main-file allocation after unused-extents release; absent when busy.
    pub allocation_after_bytes: Option<u64>,
    /// Exact descriptor/file identity used for unused-extents release.
    pub allocation_source: Option<AllocationIdentity>,
    /// F_TRANSFEREXTENTS call wall, nested in completion.
    pub allocation_transfer_ns: u64,
    /// Scratch descriptor close wall, nested in completion.
    pub allocation_scratch_close_ns: u64,
    /// Checked on-demand allocation descriptor close wall, nested in completion.
    pub allocation_source_close_ns: u64,
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
    pub(crate) layout: crate::SqlitePackLayout,
    pub(crate) acquisition: crate::SqliteAcquisitionSchema,
    #[cfg(target_os = "macos")]
    pub(crate) allocation: Option<super::allocation_owner::AllocationOwner>,
}
impl Session {
    pub(crate) fn connect(
        path: &Path,
        writable: bool,
        create: bool,
        selected: SqlitePersistenceProfile,
        creation_layout: crate::SqlitePackLayout,
        creation_acquisition: crate::SqliteAcquisitionSchema,
    ) -> Result<Self, BackendError> {
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
        profile::apply(&connection, create, selected, creation_acquisition, &work)?;
        let (layout, acquisition) = if create {
            (creation_layout, creation_acquisition)
        } else {
            let stored = query::run(&connection, "PRAGMA user_version", vec![], &work)?
                .first()
                .ok_or(BackendError::Integrity)?
                .get::<i64>(0)?;
            let acquisition = match stored {
                1..=3 => crate::SqliteAcquisitionSchema::Absent,
                4..=6 => crate::SqliteAcquisitionSchema::Tables,
                _ => return Err(BackendError::Integrity),
            };
            let layout = match stored - acquisition.version_offset() {
                1 => crate::SqlitePackLayout::Monolithic,
                2 => crate::SqlitePackLayout::GroupRows,
                _ => crate::SqlitePackLayout::GroupRowsIndexed,
            };
            (layout, acquisition)
        };
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
            persistence: selected,
            pack_layout: layout,
            acquisition,
            identity: selected.identity(),
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
            auto_vacuum: integer("auto_vacuum")?,
            wal_autocheckpoint: integer("wal_autocheckpoint")?,
            journal_size_limit: integer("journal_size_limit")?,
            cache_size: integer("cache_size")?,
            busy_timeout: integer("busy_timeout")?,
            mmap_size: integer("mmap_size")?,
            temp_store: integer("temp_store")?,
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
        profile::check(&profile)?;
        #[cfg(target_os = "macos")]
        let allocation = if writable {
            Some(super::allocation_owner::AllocationOwner::open(
                path,
                selected == SqlitePersistenceProfile::Disposable,
            )?)
        } else {
            None
        };
        Ok(Self {
            #[cfg(target_os = "macos")]
            allocation,
            state: Mutex::new(State {
                connection,
                quarantined: false,
                work,
            }),
            writable,
            profile,
            layout,
            acquisition,
        })
    }
    /// Stored schema version: the pack layout's, offset by the acquisition tables.
    pub(crate) fn schema_version(&self) -> i64 {
        self.layout.version() + self.acquisition.version_offset()
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
        let result = (|| {
            let wal_checkpoint_performed =
                self.profile.persistence == SqlitePersistenceProfile::Durable;
            let (busy, log_frames, checkpointed_frames) = if wal_checkpoint_performed {
                let rows = query::run(
                    &s.connection,
                    "PRAGMA wal_checkpoint(TRUNCATE)",
                    vec![],
                    &s.work,
                )?;
                let r = rows.first().ok_or(BackendError::Integrity)?;
                (r.get::<i64>(0)? != 0, r.get::<i64>(1)?, r.get::<i64>(2)?)
            } else {
                (false, -1, -1)
            };
            let allocation: Option<AllocationRelease> = if busy {
                None
            } else {
                #[cfg(target_os = "macos")]
                {
                    Some(
                        self.allocation
                            .as_ref()
                            .ok_or(BackendError::Integrity)?
                            .release()?,
                    )
                }
                #[cfg(not(target_os = "macos"))]
                {
                    return Err(BackendError::Integrity);
                }
            };
            Ok(Checkpoint {
                persistence: self.profile.persistence,
                wal_checkpoint_performed,
                busy,
                log_frames,
                checkpointed_frames,
                wall_ns: 0,
                allocation_before_bytes: allocation.map(|p| p.before),
                allocation_after_bytes: allocation.map(|p| p.after),
                allocation_source: allocation.and_then(|p| p.source),
                allocation_transfer_ns: allocation.map_or(0, |p| p.transfer_ns),
                allocation_scratch_close_ns: allocation.map_or(0, |p| p.scratch_close_ns),
                allocation_source_close_ns: allocation.map_or(0, |p| p.source_close_ns),
            })
        })();
        let wall_ns = start.elapsed().as_nanos() as u64;
        s.work.borrow_mut().checkpoint_ns += wall_ns;
        if result.as_ref().err() == Some(&BackendError::Unknown) {
            s.quarantined = true;
        }
        result.map(|mut report| {
            report.wall_ns = wall_ns;
            report
        })
    }
}
