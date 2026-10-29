//! One consistent copy of a live Store to a new file (SQLite online backup).
//!
//! The copy runs on its own read-only connection, so the shared session and
//! its writer stay free: writers in this and other processes continue under
//! WAL while the copy reads one snapshot. That connection opens one read
//! transaction before the first backup step and holds it across all of
//! them, so later commits by other connections never restart the copy; the
//! result is the Store exactly as of that transaction. Steps of
//! [`STEP_PAGES`] report the bytes copied so far (#506 E4). A busy or
//! locked step is reported, never retried.
use super::session::connection::Session;
use super::session::query;
use super::session::rows;
use crate::{backend::records::BackendError, SealedStore};
use layerfs_storage::port::PersistenceError;
use rusqlite::{backup::Backup, backup::StepResult, Connection, OpenFlags};
use std::{fs::OpenOptions, path::Path};

/// Pages one backup step copies before it reports progress.
pub(crate) const STEP_PAGES: i32 = 1024;

impl Session {
    /// Copies this Store to `destination`, which must not exist. The file is
    /// left closed with no sidecar, in WAL mode with the source's schema
    /// version, ready for the sealed-Store install. On failure an incomplete
    /// destination file may remain; it is the caller's to remove.
    /// `progress(done, total)` is called after each step with the bytes
    /// copied and the copy's whole size.
    pub(crate) fn snapshot(
        &self,
        destination: &Path,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<SealedStore, PersistenceError> {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .map_err(|e| PersistenceError::Refused {
                status: e.kind().to_string(),
            })?;
        let work = std::cell::RefCell::new(Default::default());
        let source = Connection::open_with_flags(
            &self.path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(rows::error)?;
        source
            .busy_timeout(std::time::Duration::ZERO)
            .map_err(rows::error)?;
        let mut copy = Connection::open_with_flags(
            destination,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(rows::error)?;
        copy.busy_timeout(std::time::Duration::ZERO)
            .map_err(rows::error)?;
        // Disposable copy: no sync, same page size; WAL so the copied header
        // keeps the journal mode every Store open requires.
        for pragma in ["synchronous=0", "page_size=4096"] {
            query::run(&copy, &format!("PRAGMA {pragma}"), vec![], &work)?;
        }
        let mode = query::run(&copy, "PRAGMA journal_mode=wal", vec![], &work)?
            .first()
            .ok_or(PersistenceError::Malformed)?
            .get::<String>(0)?;
        if mode != "wal" {
            return Err(PersistenceError::Malformed);
        }
        #[cfg(target_os = "macos")]
        super::lifecycle::file_control::disable_persistent_wal(&copy)?;
        // the read transaction every step shares: one snapshot of the source
        let page_size = query::run(&source, "PRAGMA page_size", vec![], &work)?
            .first()
            .ok_or(PersistenceError::Malformed)?
            .get::<i64>(0)?;
        let page_size = u64::try_from(page_size).map_err(|_| PersistenceError::Malformed)?;
        query::run(&source, "BEGIN", vec![], &work)?;
        query::run(&source, "SELECT count(*) FROM sqlite_schema", vec![], &work)?;
        {
            let backup = Backup::new(&source, &mut copy).map_err(rows::error)?;
            loop {
                let step = backup.step(STEP_PAGES).map_err(rows::error)?;
                let done = backup.progress();
                let pages = u64::try_from(done.pagecount).unwrap_or(0);
                let left = u64::try_from(done.remaining).unwrap_or(0);
                progress((pages - left.min(pages)) * page_size, pages * page_size);
                match step {
                    StepResult::Done => break,
                    StepResult::More => continue,
                    StepResult::Busy | StepResult::Locked => return Err(PersistenceError::Busy),
                    _ => return Err(PersistenceError::Uncertain),
                }
            }
        }
        query::run(&source, "COMMIT", vec![], &work)?;
        source.close().map_err(|(_, error)| rows::error(error))?;
        let checkpoint = query::run(&copy, "PRAGMA wal_checkpoint(TRUNCATE)", vec![], &work)?;
        if checkpoint
            .first()
            .ok_or(PersistenceError::Malformed)?
            .get::<i64>(0)?
            != 0
        {
            return Err(PersistenceError::Busy);
        }
        let version = query::run(&copy, "PRAGMA user_version", vec![], &work)?
            .first()
            .ok_or(PersistenceError::Malformed)?
            .get::<i64>(0)?;
        if version != self.schema_version() {
            return Err(BackendError::Integrity.into());
        }
        copy.close().map_err(|(_, error)| rows::error(error))?;
        for suffix in ["-wal", "-shm", "-journal"] {
            let mut sidecar = destination.as_os_str().to_os_string();
            sidecar.push(suffix);
            match std::fs::symlink_metadata(sidecar) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Ok(_) => {
                    return Err(PersistenceError::Refused {
                        status: "snapshot sidecar remains after close".into(),
                    })
                }
                Err(error) => {
                    return Err(PersistenceError::Refused {
                        status: error.to_string(),
                    })
                }
            }
        }
        let bytes = std::fs::metadata(destination)
            .map_err(|error| PersistenceError::Refused {
                status: error.to_string(),
            })?
            .len();
        Ok(SealedStore {
            path: destination.to_path_buf(),
            profile: self.profile.persistence,
            sqlite_version: self.profile.sqlite_version.clone(),
            bytes,
        })
    }
}
