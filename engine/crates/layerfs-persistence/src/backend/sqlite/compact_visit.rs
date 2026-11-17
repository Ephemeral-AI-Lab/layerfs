//! §6.3 visited scratch: a SQLite compaction's visited set is a temp-file
//! table, streamed while the mark walks the object closure.
//!
//! One row per discovered object is written to `<store>.compact-visited`
//! (journal off, no sidecars) instead of leaving the set to a second pass
//! over an in-memory map. The §7 build attaches the same file as `scratch`
//! and joins `object_location` against it, and the `none`/abort paths delete
//! it with the rest of the artifact set.
use layerfs_content::ObjectId;
use layerfs_history::error::{HistoryError, HistoryResult};
use rusqlite::{params, Connection, OpenFlags};
use std::path::{Path, PathBuf};

/// The rows a mark streams into the compaction's visited scratch.
pub(crate) struct VisitSink {
    path: PathBuf,
    connection: Option<Connection>,
    finished: bool,
}

fn sidecars(path: &Path) -> [PathBuf; 3] {
    let with = |suffix: &str| {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        PathBuf::from(name)
    };
    [with("-journal"), with("-wal"), with("-shm")]
}

fn db_error(_error: rusqlite::Error) -> HistoryError {
    HistoryError::Integrity("visited scratch")
}

fn io_error(error: std::io::Error) -> HistoryError {
    match error.raw_os_error() {
        Some(28) | Some(122) | Some(27) | Some(69) => HistoryError::Capacity("disk space"),
        _ => HistoryError::Integrity("visited scratch I/O"),
    }
}

impl VisitSink {
    /// Creates the scratch beside the Store. The caller has already refused a
    /// recorded artifact; an existing name here is an unrecorded leftover.
    pub(crate) fn create(path: &Path) -> HistoryResult<Self> {
        if std::fs::symlink_metadata(path).is_ok() {
            return Err(HistoryError::Exists("staged compaction artifact"));
        }
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(db_error)?;
        connection
            .busy_timeout(std::time::Duration::ZERO)
            .map_err(db_error)?;
        // The scratch is disposable: `journal_mode=OFF` leaves no journal of
        // its own, so the artifact set has no sidecars to judge.
        connection
            .execute_batch(
                "PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF;\
CREATE TABLE visited (\
 object_id BLOB PRIMARY KEY CHECK(length(object_id)=32),\
 pack_id INTEGER NOT NULL\
) STRICT, WITHOUT ROWID;\
CREATE TABLE live_pack (pack_id INTEGER PRIMARY KEY) STRICT, WITHOUT ROWID;\
BEGIN IMMEDIATE;",
            )
            .map_err(db_error)?;
        let sink = Self {
            path: path.to_path_buf(),
            connection: Some(connection),
            finished: false,
        };
        Ok(sink)
    }

    /// Streams one reached object and its locating pack as it is discovered.
    pub(crate) fn visit(&self, id: ObjectId, pack_id: i64) -> HistoryResult<()> {
        let connection = self
            .connection
            .as_ref()
            .ok_or(HistoryError::Integrity("visited scratch"))?;
        connection
            .prepare_cached("INSERT OR IGNORE INTO visited(object_id,pack_id) VALUES(?1,?2)")
            .map_err(db_error)?
            .execute(params![id.as_bytes().as_slice(), pack_id])
            .map_err(db_error)?;
        Ok(())
    }

    /// Derives the retained packs, commits, closes the connection and refuses
    /// a sidecar.
    pub(crate) fn finish(mut self) -> HistoryResult<u64> {
        let connection = self
            .connection
            .take()
            .ok_or(HistoryError::Integrity("visited scratch"))?;
        // The retained packs are the visited rows' packs: one ordered
        // statement at the end instead of one insert per reached object.
        connection
            .execute_batch(
                "INSERT INTO live_pack(pack_id) \
SELECT DISTINCT pack_id FROM visited ORDER BY pack_id; COMMIT",
            )
            .map_err(db_error)?;
        connection.close().map_err(|(_, error)| db_error(error))?;
        for sidecar in sidecars(&self.path) {
            if std::fs::symlink_metadata(&sidecar).is_ok() {
                return Err(HistoryError::Integrity("visited scratch sidecar"));
            }
        }
        self.finished = true;
        let bytes = std::fs::metadata(&self.path).map_err(io_error)?.len();
        Ok(bytes)
    }
}

impl Drop for VisitSink {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        self.connection.take();
        let _ = std::fs::remove_file(&self.path);
        for sidecar in sidecars(&self.path) {
            let _ = std::fs::remove_file(sidecar);
        }
    }
}
