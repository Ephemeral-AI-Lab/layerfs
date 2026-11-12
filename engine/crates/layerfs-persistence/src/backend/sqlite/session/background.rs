//! A Disposable Store's WAL checkpoints, off the commit path (owner
//! approval, 2026-11-07). The writing connection runs with
//! `wal_autocheckpoint=0`, so a COMMIT only appends its frames to the log.
//! When a write leaves the log longer than `journal_size_limit` (about the
//! 1000 pages the automatic checkpoint used), it wakes this session's
//! checkpoint thread, which copies the log into the Store file with
//! `PRAGMA wal_checkpoint(PASSIVE)` on its own connection while the writer
//! goes on. A passive checkpoint never takes the WAL write lock and never
//! waits, so no writer, reader or other process meets `Busy` from it; once
//! it has copied every frame, the next write starts the log again from its
//! beginning, which keeps the log bounded. One that cannot finish (a reader
//! still in the log) leaves its frames valid, and the next wake, the idle
//! checkpoint (#86) or the last close copies them. Each wake is a new pass
//! over the log, never a replay of a failed operation.
use super::{query, rows};
use crate::backend::records::BackendError;
use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use std::sync::{mpsc, Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;

/// Log length past which a commit wakes the thread: `journal_size_limit`.
const LOG_BYTES: u64 = 4_194_304;

pub(crate) struct Checkpointer {
    /// Held by the thread for one pass, and by an idle checkpoint.
    pass: Arc<Mutex<()>>,
    wake: Option<mpsc::SyncSender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Checkpointer {
    /// Opens the checkpoint connection on the Store at `path`, which the
    /// session's own connection has already opened in WAL mode, and starts
    /// its thread. Failure refuses the session's open: without it the log
    /// would never be copied while the session lives.
    pub(crate) fn start(path: &Path) -> Result<Self, BackendError> {
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(rows::error)?;
        connection
            .busy_timeout(std::time::Duration::ZERO)
            .map_err(rows::error)?;
        connection
            .set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)
            .map_err(rows::error)?;
        let work = std::cell::RefCell::new(super::connection::SqlWork::default());
        for pragma in [
            "PRAGMA synchronous=0",
            "PRAGMA fullfsync=0",
            "PRAGMA wal_autocheckpoint=0",
        ] {
            query::run(&connection, pragma, vec![], &work)?;
        }
        let mode = query::run(&connection, "PRAGMA journal_mode", vec![], &work)?
            .first()
            .ok_or(BackendError::Integrity)?
            .get::<String>(0)?;
        if mode != "wal" {
            return Err(BackendError::Integrity);
        }
        let pass = Arc::new(Mutex::new(()));
        let (wake, woken) = mpsc::sync_channel::<()>(1);
        let held = pass.clone();
        let thread = std::thread::Builder::new()
            .name("layerfs-store-checkpoint".to_owned())
            .spawn(move || {
                while woken.recv().is_ok() {
                    let _pass = held.lock().unwrap_or_else(|e| e.into_inner());
                    // Its outcome changes nothing the session reports: frames
                    // left in the log stay valid for the next pass.
                    let _ =
                        query::run(&connection, "PRAGMA wal_checkpoint(PASSIVE)", vec![], &work);
                }
            })
            .map_err(|_| BackendError::Unknown)?;
        Ok(Self {
            pass,
            wake: Some(wake),
            thread: Some(thread),
        })
    }

    /// After a write commits: wakes the thread when the log at `path` is
    /// longer than [`LOG_BYTES`]. A wake already pending covers this one.
    pub(crate) fn committed(&self, path: &Path) {
        if super::checkpoint::wal_bytes(path).is_ok_and(|bytes| bytes > LOG_BYTES) {
            if let Some(wake) = &self.wake {
                let _ = wake.try_send(());
            }
        }
    }

    /// Waits for a pass running now and keeps the next one from starting
    /// while the guard lives: an idle checkpoint is not left `Busy` by it.
    pub(crate) fn hold(&self) -> MutexGuard<'_, ()> {
        self.pass.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Drop for Checkpointer {
    /// Ends the thread after its current pass and closes its connection,
    /// before the session's own connection closes.
    /// This connection is therefore never SQLite's last one: State still owns
    /// the main connection. Do not reacquire its directory lane here; a
    /// quarantined State may already retain it while this join completes.
    fn drop(&mut self) {
        self.wake.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
