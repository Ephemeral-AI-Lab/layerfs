//! A Normal Store's WAL checkpoints, off the commit path (owner
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
use crate::backend::error::BackendError;
use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;

/// Log length past which a commit wakes the thread: `journal_size_limit`.
const LOG_BYTES: u64 = 4_194_304;

pub(crate) struct Checkpointer {
    /// Held by the thread for one pass, and by an idle checkpoint.
    pass: Arc<Mutex<()>>,
    /// Set once the session's own connection was closed under quarantine
    /// (finding #2): this connection may then be SQLite's last, and it closes
    /// without the lane, so it must not checkpoint or remove `-wal`/`-shm`.
    quiet: Arc<AtomicBool>,
    wake: Option<mpsc::SyncSender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Checkpointer {
    /// Opens the checkpoint connection on the Store at `path`, which the
    /// session's own connection has already opened in WAL mode, and starts
    /// its thread. Failure refuses the session's open: without it the log
    /// would never be copied while the session lives.
    ///
    /// `synchronous` is the session profile's value. A checkpoint copies log
    /// frames into the database file, so it must synchronize exactly as the
    /// session does: under NORMAL a pass at 0 would mark frames copied that
    /// never reached the disk, and the writer would restart the log over them.
    ///
    /// The connection sets `checkpoint_fullfsync=0` and reads it back: a
    /// routine pass syncs the Store file with plain `fsync`, which on macOS
    /// does not flush the drive's cache (owner, 2026-11-23: F_FULLFSYNC on
    /// every pass pushed write-heavy suites past the 120 s test ceiling). The
    /// idle TRUNCATE and the last close run on the session's own connection,
    /// which sets `checkpoint_fullfsync=1` (`profile.rs`, finding #11); the
    /// window this leaves is in `docs/architecture/engine/store/sqlite.md`.
    /// Returns the read-back value.
    pub(crate) fn start(path: &Path, synchronous: i64) -> Result<(Self, i64), BackendError> {
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
            format!("PRAGMA synchronous={synchronous}"),
            "PRAGMA fullfsync=0".to_owned(),
            "PRAGMA checkpoint_fullfsync=0".to_owned(),
            "PRAGMA wal_autocheckpoint=0".to_owned(),
        ] {
            query::run(&connection, &pragma, vec![], &work)?;
        }
        let applied = query::run(&connection, "PRAGMA synchronous", vec![], &work)?
            .first()
            .ok_or(BackendError::Integrity)?
            .get::<i64>(0)?;
        if applied != synchronous {
            return Err(BackendError::Integrity);
        }
        let checkpoint_fullfsync =
            query::run(&connection, "PRAGMA checkpoint_fullfsync", vec![], &work)?
                .first()
                .ok_or(BackendError::Integrity)?
                .get::<i64>(0)?;
        if checkpoint_fullfsync != 0 {
            return Err(BackendError::Integrity);
        }
        let mode = query::run(&connection, "PRAGMA journal_mode", vec![], &work)?
            .first()
            .ok_or(BackendError::Integrity)?
            .get::<String>(0)?;
        if mode != "wal" {
            return Err(BackendError::Integrity);
        }
        let pass = Arc::new(Mutex::new(()));
        let quiet = Arc::new(AtomicBool::new(false));
        let (wake, woken) = mpsc::sync_channel::<()>(1);
        let held = pass.clone();
        let quietly = quiet.clone();
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
                if quietly.load(Ordering::Acquire)
                    && connection
                        .set_db_config(
                            rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
                            true,
                        )
                        .is_err()
                {
                    // As `State::drop`: retain it until process exit rather
                    // than tear the WAL down outside the lane.
                    std::mem::forget(connection);
                }
            })
            .map_err(|_| BackendError::Unknown)?;
        Ok((
            Self {
                pass,
                quiet,
                wake: Some(wake),
                thread: Some(thread),
            },
            checkpoint_fullfsync,
        ))
    }

    /// Makes this connection's eventual close skip SQLite's checkpoint and
    /// sidecar removal (finding #2): see `State::close_quarantined`.
    pub(crate) fn close_quietly(&self) {
        self.quiet.store(true, Ordering::Release);
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
    /// This connection is SQLite's last one only after a quarantined State
    /// closed the main connection, and that close set `quiet`, so this close
    /// then neither checkpoints nor removes the WAL and SHM files. Do not
    /// reacquire the directory lane here; a quarantined State may still
    /// retain it while this join completes.
    fn drop(&mut self) {
        self.wake.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
