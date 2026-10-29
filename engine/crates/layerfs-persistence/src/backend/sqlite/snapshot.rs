//! One consistent copy of a live Store to a new file (`VACUUM INTO`, #510).
//!
//! The copy runs on its own read-only connection, so the shared session and
//! its writer stay free: writers in this and other processes continue under
//! WAL while `VACUUM INTO` reads one snapshot in one read transaction; the
//! result is the Store exactly as of that transaction. It rewrites each
//! table and index as one contiguous run of pages, which a ranged reader of
//! the pushed file fetches in few requests (#510). Every Store table is
//! WITHOUT ROWID or keys its rows by an `INTEGER PRIMARY KEY`, which a
//! vacuum keeps, and `sqlite_sequence` is copied (rowid audit on #510).
//! Page size, `auto_vacuum`, `user_version` and `application_id` carry over;
//! the output is a rollback-journal file, so the copy is then switched to
//! WAL, the mode every Store open requires. While the copy runs, the calling
//! thread reports its growing size. A busy source is reported, never retried.
use super::session::connection::{Session, SqlWork};
use super::session::query;
use super::session::rows;
use crate::{backend::records::BackendError, SealedStore};
use layerfs_storage::port::PersistenceError;
use rusqlite::{Connection, OpenFlags};
use std::cell::RefCell;
use std::path::Path;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

/// How often the copy's size is reported while `VACUUM INTO` runs.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

fn integer(c: &Connection, sql: &str, work: &RefCell<SqlWork>) -> Result<i64, PersistenceError> {
    Ok(query::run(c, sql, vec![], work)?
        .first()
        .ok_or(PersistenceError::Malformed)?
        .get::<i64>(0)?)
}

fn refused(error: impl std::fmt::Display) -> PersistenceError {
    PersistenceError::Refused {
        status: error.to_string(),
    }
}

impl Session {
    /// Copies this Store to `destination`, which must not exist. The file is
    /// left closed with no sidecar, in WAL mode with the source's schema
    /// version, ready for the sealed-Store install. On failure an incomplete
    /// destination file may remain; it is the caller's to remove.
    /// `progress(done, total)` is called with the bytes written so far and
    /// an upper bound of the copy's size (the source's pages), then once
    /// with the copy's final size as both.
    pub(crate) fn snapshot(
        &self,
        destination: &Path,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<SealedStore, PersistenceError> {
        // VACUUM INTO itself refuses an existing output file; this names it first
        if std::fs::symlink_metadata(destination).is_ok() {
            return Err(refused("the snapshot destination exists"));
        }
        let target = destination
            .to_str()
            .ok_or_else(|| refused("the snapshot path is not UTF-8"))?
            .to_owned();
        let work = RefCell::new(SqlWork::default());
        let source = Connection::open_with_flags(
            &self.path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(rows::error)?;
        source.busy_timeout(Duration::ZERO).map_err(rows::error)?;
        // Disposable copy: VACUUM INTO writes its output with the source
        // connection's synchronous setting, so nothing is synced.
        query::run(&source, "PRAGMA synchronous=0", vec![], &work)?;
        let page_size = u64::try_from(integer(&source, "PRAGMA page_size", &work)?)
            .map_err(|_| PersistenceError::Malformed)?;
        let pages = u64::try_from(integer(&source, "PRAGMA page_count", &work)?)
            .map_err(|_| PersistenceError::Malformed)?;
        let bound = pages * page_size;
        let (done, finished) = mpsc::channel::<()>();
        let copied = std::thread::scope(|scope| {
            let copy = scope.spawn(move || {
                let result = source.execute("VACUUM INTO ?1", [&target]);
                let _ = done.send(());
                (source, result)
            });
            while let Err(RecvTimeoutError::Timeout) = finished.recv_timeout(PROGRESS_EVERY) {
                let written = std::fs::metadata(destination).map_or(0, |m| m.len());
                progress(written.min(bound), bound);
            }
            copy.join()
        });
        let (source, vacuumed) = copied.map_err(|_| PersistenceError::Uncertain)?;
        if let Err(error) = vacuumed {
            let message = error.to_string();
            return Err(match rows::error(error) {
                BackendError::Busy => PersistenceError::Busy,
                BackendError::Capacity => BackendError::Capacity.into(),
                _ => refused(format!("VACUUM INTO: {message}")),
            });
        }
        source.close().map_err(|(_, error)| rows::error(error))?;
        let copy = Connection::open_with_flags(
            destination,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(rows::error)?;
        copy.busy_timeout(Duration::ZERO).map_err(rows::error)?;
        query::run(&copy, "PRAGMA synchronous=0", vec![], &work)?;
        let mode = query::run(&copy, "PRAGMA journal_mode=wal", vec![], &work)?
            .first()
            .ok_or(PersistenceError::Malformed)?
            .get::<String>(0)?;
        if mode != "wal" {
            return Err(PersistenceError::Malformed);
        }
        #[cfg(target_os = "macos")]
        super::lifecycle::file_control::disable_persistent_wal(&copy)?;
        if integer(&copy, "PRAGMA wal_checkpoint(TRUNCATE)", &work)? != 0 {
            return Err(PersistenceError::Busy);
        }
        let kept = [
            ("user_version", self.schema_version()),
            ("application_id", super::schema::APPLICATION_ID),
            ("page_size", self.profile.page_size),
            ("auto_vacuum", self.profile.auto_vacuum),
        ];
        for (pragma, expected) in kept {
            if integer(&copy, &format!("PRAGMA {pragma}"), &work)? != expected {
                return Err(BackendError::Integrity.into());
            }
        }
        copy.close().map_err(|(_, error)| rows::error(error))?;
        for suffix in ["-wal", "-shm", "-journal"] {
            let mut sidecar = destination.as_os_str().to_os_string();
            sidecar.push(suffix);
            match std::fs::symlink_metadata(sidecar) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Ok(_) => return Err(refused("snapshot sidecar remains after close")),
                Err(error) => return Err(refused(error)),
            }
        }
        let bytes = std::fs::metadata(destination).map_err(refused)?.len();
        progress(bytes, bytes);
        Ok(SealedStore {
            path: destination.to_path_buf(),
            profile: self.profile.persistence,
            sqlite_version: self.profile.sqlite_version.clone(),
            bytes,
        })
    }
}
