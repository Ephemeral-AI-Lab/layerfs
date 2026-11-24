//! Durable host connection and recorded work; no busy retry or fallback.
use super::profile;
use super::query;
use super::rows;
use crate::backend::records::BackendError;
use crate::backend::sqlite::lane::writer_admission::Writer;
use crate::backend::sqlite::lifecycle::identity::FileIdentity;
use crate::SqlitePersistenceProfile;
use rusqlite::{limits::Limit, Connection, OpenFlags};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};
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
    /// FULL is 2 (Durable); NORMAL is 1 (Normal).
    pub synchronous: i64,
    /// Foreign keys must be enabled.
    pub foreign_keys: i64,
    /// macOS full-sync selection.
    pub fullfsync: i64,
    /// macOS checkpoint full-sync selection.
    pub checkpoint_fullfsync: i64,
    /// The same setting read back from the checkpoint thread's connection,
    /// which runs the routine PASSIVE passes of a Normal session: 0, plain
    /// `fsync`; the idle and last-close checkpoints run on this session's
    /// connection (`checkpoint_fullfsync` above). `None` when the session has
    /// no checkpoint thread (read-only and diagnostic sessions).
    pub checkpoint_thread_checkpoint_fullfsync: Option<i64>,
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
    /// Successfully completed single-statement implicit read snapshots. These
    /// have no explicit BEGIN/COMMIT and are excluded from those counters.
    pub read_snapshots: u64,
    /// Definite rollbacks.
    pub rollbacks: u64,
    /// Complete attempted transaction wall including lock/commit work.
    pub transaction_ns: u64,
    /// Complete attempted single-statement read wall, including session lock,
    /// prepare, execution, typed mapping and cursor/lease release.
    pub read_snapshot_ns: u64,
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
    /// PostgreSQL only (zero on SQLite): client-server exchanges that waited
    /// for the server's reply: each prepare, execution, simple-query script
    /// (`BEGIN` with its settings, `COMMIT`, `ROLLBACK`), and a binary COPY's
    /// start and finish. A new connection's handshake is not included; it is
    /// counted in `connects`.
    pub round_trips: u64,
    /// PostgreSQL only: statements prepared on the server (statement-cache
    /// misses, and each COPY's unnamed statement), each one round trip.
    pub prepares: u64,
    /// PostgreSQL only: new server connections opened by the session,
    /// including its publication-lane connection.
    pub connects: u64,
}
pub(crate) struct State {
    pub(crate) connection: super::lifecycle::ConnectionSlot,
    /// The Store path whose lane a writable session takes around its last
    /// close; `None` for read-only and diagnostic sessions, whose close cannot
    /// checkpoint or remove `-wal`/`-shm` and so takes no lane.
    pub(crate) close_lane: Option<PathBuf>,
    pub(crate) quarantined: bool,
    pub(crate) work: RefCell<SqlWork>,
    pub(crate) writer_hold: Option<crate::backend::sqlite::lane::writer_admission::Writer>,
}
pub(crate) struct Session {
    pub(crate) diagnostic: bool,
    /// A writable Normal session's checkpoint thread; dropped first, so
    /// it ends before the session's own connection closes.
    pub(crate) background: Option<super::background::Checkpointer>,
    pub(crate) admission: crate::backend::admission::Admission,
    pub(crate) state: Mutex<State>,
    pub(crate) writable: bool,
    pub(crate) profile: ConnectionProfile,
    pub(crate) layout: crate::SqlitePackLayout,
    pub(crate) acquisition: crate::SqliteAcquisitionSchema,
    /// The `(dev, ino)` identity the path named when this handle opened (§10).
    /// `None` on a platform without one; the fence then has nothing to compare.
    pub(crate) identity: Option<FileIdentity>,
    pub(crate) path: PathBuf,
}
/// The path of the Store's SQLite sidecar `suffix` (`-wal`, `-shm`).
fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

/// The Store open rule (#776 C3, S6). A writable open runs PRAGMA and schema
/// statements that write, so it takes the same local-volume lane a
/// transaction takes: another opener waits for it instead of meeting
/// SQLite's Busy (#534, #647). A read-only open takes it only when the Store
/// has no `-shm`: its first read creates the sidecars by path and runs
/// SQLite's WAL recovery, so unadmitted it could leave sidecars beside the
/// file a compaction is installing, or recover under an admitted writer's
/// transaction. With `-shm` there, another connection holds the Store and the
/// open maps it without the lane. Returns the lane, if taken, and the file
/// the path names before the open; it must still name it after the first
/// read, or the open may have reached a file a compaction replaced meanwhile
/// (#776 C3).
fn admit_open(
    path: &Path,
    writable: bool,
) -> Result<(Option<Writer>, Option<FileIdentity>), BackendError> {
    let writer = if writable || !sidecar(path, "-shm").exists() {
        Some(Writer::acquire(path)?)
    } else {
        None
    };
    let before = FileIdentity::capture(path).map_err(|_| BackendError::Integrity)?;
    Ok((writer, before))
}

/// A raw read-only connection to the Store at `path` opened under the open
/// rule, for a reader outside a Session's connection: the structure probe, a
/// compaction's leftover-work check, a snapshot's source. The lane (taken
/// only when the Store has no `-shm`) is held through the connection's first
/// read, which creates the sidecars, and is returned for the caller to
/// release; an error exit closes the connection under it. A `-shm` that
/// appears or goes between the check and that read is benign: the identity
/// check here and a compaction's re-check before its rename catch a replaced
/// file or a sidecar left beside one. `error` maps SQLite's errors, so a
/// caller can classify the first read's. Returns the file it opened.
pub(crate) fn open_admitted_read_only(
    path: &Path,
    error: &mut dyn FnMut(rusqlite::Error) -> BackendError,
) -> Result<(Connection, Option<Writer>, Option<FileIdentity>), BackendError> {
    let (writer, before) = admit_open(path, false)?;
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(&mut *error)?;
    connection
        .busy_timeout(std::time::Duration::ZERO)
        .map_err(&mut *error)?;
    connection
        .query_row("PRAGMA schema_version", [], |_| Ok(()))
        .map_err(&mut *error)?;
    if FileIdentity::capture(path).map_err(|_| BackendError::Integrity)? != before {
        return Err(BackendError::Stale);
    }
    Ok((connection, writer, before))
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
        Self::connect_mode(
            path,
            writable,
            create,
            selected,
            creation_layout,
            creation_acquisition,
            false,
        )
    }
    pub(crate) fn connect_diagnostic(path: &Path) -> Result<Self, BackendError> {
        Self::connect_mode(
            path,
            false,
            false,
            SqlitePersistenceProfile::Normal,
            crate::SqlitePackLayout::Monolithic,
            crate::SqliteAcquisitionSchema::Absent,
            true,
        )
    }
    fn connect_mode(
        path: &Path,
        writable: bool,
        create: bool,
        selected: SqlitePersistenceProfile,
        creation_layout: crate::SqlitePackLayout,
        creation_acquisition: crate::SqliteAcquisitionSchema,
        diagnostic: bool,
    ) -> Result<Self, BackendError> {
        if !cfg!(any(target_os = "macos", target_os = "linux")) {
            return Err(BackendError::Integrity);
        }
        // The lane, when the open rule takes it, is released below, after
        // the last step that can fail, before this session serves its first
        // call.
        let (writer, before) = admit_open(path, writable)?;
        let flags = if writable {
            OpenFlags::SQLITE_OPEN_READ_WRITE
        } else {
            OpenFlags::SQLITE_OPEN_READ_ONLY
        } | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let connection = Connection::open_with_flags(path, flags).map_err(rows::error)?;
        connection.set_prepared_statement_cache_capacity(64);
        connection
            .busy_timeout(std::time::Duration::ZERO)
            .map_err(rows::error)?;
        let work = RefCell::new(SqlWork::default());
        connection
            .set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)
            .map_err(rows::error)?;
        if diagnostic {
            // These are private read-connection limits, not source format,
            // journal, checkpoint or synchronous-policy changes.
            connection.execute_batch("PRAGMA query_only=ON; PRAGMA cache_size=-2048; PRAGMA mmap_size=0; PRAGMA temp_store=FILE").map_err(rows::error)?;
        } else {
            profile::apply(&connection, create, selected, creation_acquisition, &work)?;
        }
        let (layout, acquisition) = if create {
            (creation_layout, creation_acquisition)
        } else {
            let stored = query::run(&connection, "PRAGMA user_version", vec![], &work)?
                .first()
                .ok_or(BackendError::Integrity)?
                .get::<i64>(0)?;
            // Versions 10 to 12 used the retired host acquisition schema and are refused.
            // Versions 1 to 6 kept inode values in pooled packs behind an
            // ordinal catalogue. Versions 16 to 18 and 22 to 24 (schema
            // version 1) kept one row per inode value and binding hint;
            // schema version 2 (#91) keeps them in blocks. Versions 36 to 38
            // and 42 to 44 (schema version 2) record no Commit attempt;
            // schema version 3 (#776 R1) does. Such a Store is refused here,
            // never misread or converted.
            let acquisition = match stored {
                56..=58 => crate::SqliteAcquisitionSchema::Absent,
                62..=64 => crate::SqliteAcquisitionSchema::Tables,
                16..=18 | 22..=24 => return Err(BackendError::SchemaVersion1),
                36..=38 | 42..=44 => return Err(BackendError::SchemaVersion2),
                _ => {
                    return Err(if diagnostic {
                        BackendError::UnsupportedSchemaVersion(stored)
                    } else {
                        BackendError::Integrity
                    })
                }
            };
            let layout = match stored - acquisition.version_offset() {
                56 => crate::SqlitePackLayout::Monolithic,
                57 => crate::SqlitePackLayout::GroupRows,
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
        let mut profile = ConnectionProfile {
            persistence: selected,
            pack_layout: layout,
            acquisition,
            identity: selected.identity(),
            sqlite_version: rusqlite::version().to_owned(),
            platform: std::env::consts::OS,
            vfs_selection: "SQLite default; name not directly observed",
            compile_options,
            journal_mode,
            synchronous: integer("synchronous")?,
            foreign_keys: integer("foreign_keys")?,
            fullfsync: integer("fullfsync")?,
            checkpoint_fullfsync: integer("checkpoint_fullfsync")?,
            checkpoint_thread_checkpoint_fullfsync: None,
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
        if !diagnostic {
            profile::check(&profile)?;
        }
        super::thread_work::add(&work.borrow());
        let background = match (writable && selected.background_checkpoint())
            .then(|| super::background::Checkpointer::start(path, selected.synchronous()))
            .transpose()?
        {
            Some((background, checkpoint_fullfsync)) => {
                profile.checkpoint_thread_checkpoint_fullfsync = Some(checkpoint_fullfsync);
                Some(background)
            }
            None => None,
        };
        // §10: the record every later transaction is fenced against.
        let identity = FileIdentity::capture(path).map_err(|_| BackendError::Integrity)?;
        if identity != before {
            return Err(BackendError::Stale);
        }
        // The lane goes only once nothing above can fail: an error exit drops
        // the raw connection, whose close may be SQLite's last (a checkpoint
        // and the removal of `-wal`/`-shm`), and it must close under the lane
        // as `State::drop` does (#776 S5).
        if let Some(writer) = writer {
            writer.release().map_err(|_| BackendError::Unknown)?;
        }
        Ok(Self {
            diagnostic,
            background,
            admission: crate::backend::admission::Admission::default(),
            path: path.to_path_buf(),
            state: Mutex::new(State {
                connection: super::lifecycle::ConnectionSlot::new(connection),
                close_lane: writable.then(|| path.to_path_buf()),
                quarantined: false,
                work,
                writer_hold: None,
            }),
            writable,
            profile,
            layout,
            acquisition,
            identity,
        })
    }
    /// The session state. A thread that panicked while holding it left the
    /// connection in an unknown state (a transaction may still be open), so
    /// a poisoned lock is recovered as a quarantined state: the next caller
    /// refuses `Unknown` and closes that connection under any writer it
    /// still holds, instead of every later call failing on the poisoned lock
    /// while the lane or a write transaction stays held (#776 S3).
    pub(crate) fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| {
            self.state.clear_poison();
            let mut state = poisoned.into_inner();
            state.quarantined = true;
            state
        })
    }
    /// Stored schema version: the pack layout's, offset by the acquisition tables.
    pub(crate) fn schema_version(&self) -> i64 {
        self.layout.version() + self.acquisition.version_offset()
    }
    pub(crate) fn diagnostics(&self) -> Result<SqlWork, BackendError> {
        // Counters are read under the state lock alone: a held publication
        // lane owns the admission between its transactions, and a reader of
        // counters runs no SQL, so it must not queue behind it (#352 F-018).
        let s = self.state.lock().map_err(|_| BackendError::Unknown)?;
        let w = *s.work.borrow();
        Ok(w)
    }
    /// A raw read-only connection to this Session's Store
    /// ([`open_admitted_read_only`]), for work that must not occupy the
    /// Session's own connection. A quarantined Session is refused `Unknown`,
    /// and the connection must reach the file this Session opened, or
    /// `Stale`. The caller releases the returned lane once it is back.
    pub(crate) fn admitted_read_only(&self) -> Result<(Connection, Option<Writer>), BackendError> {
        if self.state().quarantined {
            return Err(BackendError::Unknown);
        }
        self.check_identity()?;
        let (connection, writer, opened) = open_admitted_read_only(&self.path, &mut rows::error)?;
        if self.identity.is_some() && opened != self.identity {
            return Err(BackendError::Stale);
        }
        Ok((connection, writer))
    }
    /// §10: the Store path must still name the file this handle opened, or the
    /// call is `StaleStore` before it reads or writes anything.
    pub(crate) fn check_identity(&self) -> Result<(), BackendError> {
        let Some(expected) = self.identity else {
            return Ok(());
        };
        match expected.names(&self.path) {
            Ok(true) => Ok(()),
            Ok(false) => Err(BackendError::Stale),
            // A removed Store is as stale as a replaced one: nothing this
            // handle opened is left at the path to act on.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(BackendError::Stale),
            Err(_) => Err(BackendError::Integrity),
        }
    }
}
