//! One exclusive owner connection for one disposable Workspace file.
use crate::{
    metrics, DatabaseProfile, DatabaseWork, OverlayError, OverlayResult, ProfileConfig,
    StatementKind,
};
use rusqlite::{Connection, ToSql};
use std::{
    cell::{Cell, RefCell},
    path::Path,
    sync::Arc,
};

/// Daemon-local engine of one Workspace. The daemon runs short jobs on it one
/// at a time, each on its calling thread under the Workspace lock.
pub struct Overlay {
    pub(crate) identity: u64,
    pub(crate) connection: Connection,
    pub(crate) open: RefCell<crate::open::OpenState>,
    pub(crate) work: RefCell<DatabaseWork>,
    pub(crate) payload_work: Cell<crate::PayloadWork>,
    pub(crate) quarantined: Cell<bool>,
    /// Connection-local readiness hints, never namespace/owner data. False is
    /// established only by an exact empty ready query; enqueue/release marks
    /// possible work. A rolled-back enqueue can leave only a false positive.
    pub(crate) maintenance_ready: Cell<bool>,
    /// Runtime state of the one bound Workspace. Atomic jobs snapshot this fixed
    /// value; known rollback restores it and unknown outcomes retain custody.
    pub(crate) workspace: Cell<Option<crate::WorkspaceState>>,
    /// Whether any namespace of this engine holds an orphan. Set when one
    /// is inserted, cleared by the job deleting the last one, and restored
    /// when a job fails, so it is exact:
    /// while false no orphan row and no orphan-domain inode row exists.
    pub(crate) orphan_seen: Cell<bool>,
    /// Whether this engine has ever written a reference piece. Set before
    /// the only statement that inserts one and restored when a job fails, so
    /// while it is false no piece row exists and no path issues a piece
    /// statement. It is never cleared.
    pub(crate) pieces_seen: Cell<bool>,
    /// Whether the running atomic job may still finish one orphan's release
    /// itself. Set when a job starts, cleared by the first last reference
    /// that uses it and by every maintenance step, so one job does at most
    /// one maintenance step's rows of inline reclamation.
    pub(crate) release_step: Cell<bool>,
    /// What the running atomic job has asked of this connection.
    pub(super) transaction: Cell<Transaction>,
    /// Reply tickets of this engine's publications, shared with the threads
    /// that attempt replies.
    pub(crate) tickets: Arc<crate::ReplyTickets>,
    /// Tickets issued by the running atomic job, withdrawn if it fails.
    pub(crate) issued: RefCell<Vec<i64>>,
    pub(super) profile: DatabaseProfile,
    pub(super) allocation: crate::database::physical::Allocation,
}
/// An atomic job starts its SQLite transaction at its first writing statement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Transaction {
    None,
    /// The job has executed no writing statement; its reads ran in autocommit.
    Wanted,
    /// BEGIN is executing for the job's first writing statement.
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
    /// Copies a closed empty template into a fresh owned file and opens it.
    /// Schema DDL is paid by template creation; settings and identities are
    /// verified on each connection. A failed copy/open retains its artifact.
    pub fn copy_template_observed(
        path: &Path,
        config: ProfileConfig,
        template: &Path,
    ) -> crate::Creation {
        crate::database::startup::copy_template(path, config, template)
    }

    /// Checked close of the sole disposable connection. It performs no sync.
    /// Callers establish all request/capture/handle fences before consuming it.
    /// The path is retained on failure; close never authorizes an implicit replay.
    pub fn close_database(self) -> OverlayResult<()> {
        self.close_database_file().map(drop)
    }
    /// Checked close retaining only the physical file identity, for an immutable
    /// template observed after readiness. The returned handle has no connection.
    pub fn close_database_file(self) -> OverlayResult<crate::ClosedFile> {
        self.connection.flush_prepared_statement_cache();
        let open = self.open.into_inner();
        self.connection
            .close()
            .map_err(|refused| OverlayError::Uncertain {
                cause: Box::new(OverlayError::Close(Box::new(crate::CloseFailure::retain(
                    refused, open,
                )))),
                completion: None,
            })?;
        Ok(crate::ClosedFile {
            allocation: self.allocation,
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
    /// Whether an uncertain outcome closed this engine to every later job and
    /// maintenance step. It is never cleared.
    pub fn quarantined(&self) -> bool {
        self.quarantined.get()
    }
    /// Whether a maintenance turn may find work: the connection-local hints
    /// that live `maintain` consults. False is exact; true can
    /// be a false positive that the next turn clears.
    pub fn maintenance_pending(&self) -> bool {
        self.maintenance_ready.get()
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
    /// Nonrecycled identity for exact typed token validation, not routing.
    pub const fn engine(&self) -> u64 {
        self.identity
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
        let wanted = matches!(self.transaction.get(), Transaction::Wanted);
        let writing = || {
            if wanted {
                self.begin()
            } else {
                Ok(())
            }
        };
        let result = metrics::query(
            &self.connection,
            &self.work,
            kind,
            metrics::Query {
                sql,
                params,
                bound_bytes: bytes,
                cached,
                before_write: Some(&writing),
            },
            decode,
        );
        result.map_err(|cause| self.failed(cause))
    }

    /// One writing statement, with its original counted outcome.
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
    /// Whether the running job has passed admission and begun its
    /// transaction: true from its first writing statement on.
    pub(crate) fn writing(&self) -> bool {
        self.transaction.get() == Transaction::Begun
    }
    /// The failure of a write that is not a statement. An unsafe database
    /// state quarantines the connection, as it does for a statement.
    pub(crate) fn failed(&self, cause: OverlayError) -> OverlayError {
        if !cause.unsafe_database_state() {
            return cause;
        }
        // Only a writing job can defer quarantine for capacity classification.
        // Its caller must still establish a whole-job rollback before returning
        // NoSpace. Read failures and non-capacity I/O stay uncertain.
        if self.writing() && cause.capacity_io() {
            match self.allocation.device_full() {
                Ok(true) => return cause,
                Ok(false) => {}
                Err(observation) => {
                    self.quarantined.set(true);
                    return OverlayError::Uncertain {
                        cause: Box::new(cause),
                        completion: Some(Box::new(observation)),
                    };
                }
            }
        }
        self.quarantined.set(true);
        OverlayError::Uncertain {
            cause: Box::new(cause),
            completion: None,
        }
    }
    pub(crate) fn atomic<T>(&self, job: impl FnOnce() -> OverlayResult<T>) -> OverlayResult<T> {
        self.transaction(job)
    }
    pub(crate) fn atomic_cleanup<T>(
        &self,
        job: impl FnOnce() -> OverlayResult<T>,
    ) -> OverlayResult<T> {
        self.transaction(job)
    }
    /// The engine's reply tickets, for the threads that attempt replies.
    pub fn reply_tickets(&self) -> Arc<crate::ReplyTickets> {
        self.tickets.clone()
    }
    /// Issues the reply ticket of the running atomic job's publication.
    pub(crate) fn issue(&self, publication: crate::Publication) {
        self.tickets.issue(publication);
        if self.transaction.get() != Transaction::None {
            self.issued.borrow_mut().push(publication.revision);
        }
    }
    /// Allocation-call counters without performing filesystem or SQL I/O.
    pub fn allocation_work(&self) -> crate::AllocationWork {
        self.allocation.work()
    }
    pub fn allocation(&self) -> OverlayResult<crate::AllocationState> {
        self.available()?;
        match self.allocation.state() {
            Ok(state) => Ok(state),
            Err(cause) => {
                self.quarantined.set(true);
                Err(OverlayError::Uncertain {
                    cause: Box::new(cause),
                    completion: None,
                })
            }
        }
    }
    /// Runs one job atomically. Reads before the job's first writing statement
    /// execute in autocommit: this connection is the only one, holds the
    /// exclusive lock and runs one job at a time, so they see the same state.
    /// A job with no SQL writes begins and commits no SQLite transaction.
    fn transaction<T>(&self, job: impl FnOnce() -> OverlayResult<T>) -> OverlayResult<T> {
        self.available()?;
        if self.transaction.get() != Transaction::None {
            return Err(OverlayError::Invalid("nested transaction"));
        }
        self.transaction.set(Transaction::Wanted);
        self.release_step.set(true);
        let (orphans, pieces, mount) = (
            self.orphan_seen.get(),
            self.pieces_seen.get(),
            self.open.borrow().mount,
        );
        let workspace = self.workspace.get();
        let result = job();
        let issued = std::mem::take(&mut *self.issued.borrow_mut());
        let begun = self.transaction.get() == Transaction::Begun;
        let failed = match result {
            Ok(value) if begun => match self.execute(StatementKind::Commit, "COMMIT", &[], 0) {
                Ok(_) => {
                    self.transaction.set(Transaction::None);
                    self.open.borrow_mut().commit();
                    return Ok(value);
                }
                Err(cause) if cause.is_sqlite_full() || cause.capacity_io() => cause,
                Err(cause) => {
                    self.transaction.set(Transaction::None);
                    self.quarantined.set(true);
                    return Err(OverlayError::Uncertain {
                        cause: Box::new(cause),
                        completion: None,
                    });
                }
            },
            Ok(value) => {
                self.transaction.set(Transaction::None);
                self.open.borrow_mut().commit();
                return Ok(value);
            }
            Err(cause) => self.failed(cause),
        };
        self.transaction.set(Transaction::None);
        if self.quarantined.get() {
            return Err(failed);
        }
        if begun && !self.connection.is_autocommit() {
            // One rollback attempt, only when SQLite has not already rolled
            // back the transaction. A failed rollback retains both causes.
            if let Err(completion) = self.execute(StatementKind::Rollback, "ROLLBACK", &[], 0) {
                self.quarantined.set(true);
                return Err(OverlayError::Uncertain {
                    cause: Box::new(failed),
                    completion: Some(Box::new(completion)),
                });
            }
        }
        // Hints and reply tickets revert only after known whole-job failure,
        // including a capacity failure at COMMIT, never on an unknown outcome.
        self.orphan_seen.set(orphans);
        self.pieces_seen.set(pieces);
        self.open.borrow_mut().rollback();
        self.open.borrow_mut().mount = mount;
        self.workspace.set(workspace);
        for revision in issued {
            self.tickets.withdraw(revision);
        }
        if begun {
            if let Err(observation) = self.allocation.state() {
                self.quarantined.set(true);
                return Err(OverlayError::Uncertain {
                    cause: Box::new(failed),
                    completion: Some(Box::new(observation)),
                });
            }
        }
        if failed.capacity_io() {
            Err(OverlayError::NoSpace {
                cause: Box::new(failed),
            })
        } else {
            Err(failed)
        }
    }
    /// Admission and BEGIN for the first writing statement of an atomic job,
    /// before that statement executes. A failure is returned by that statement
    /// and leaves no transaction open.
    fn begin(&self) -> OverlayResult<()> {
        let Transaction::Wanted = self.transaction.get() else {
            return Ok(());
        };
        // BEGIN IMMEDIATE is itself a writing statement.
        self.transaction.set(Transaction::Beginning);
        let begun = self.execute(StatementKind::Begin, "BEGIN IMMEDIATE", &[], 0);
        self.transaction.set(match begun {
            Ok(_) => Transaction::Begun,
            Err(_) => Transaction::Wanted,
        });
        begun.map(|_| ())
    }
}
pub(crate) fn integer(value: u64) -> OverlayResult<i64> {
    i64::try_from(value).map_err(|_| OverlayError::Invalid("SQLite signed identity/offset"))
}

pub(crate) fn unsigned(row: &rusqlite::Row<'_>, column: usize) -> rusqlite::Result<u64> {
    let value: i64 = row.get(column)?;
    u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(column, value))
}
