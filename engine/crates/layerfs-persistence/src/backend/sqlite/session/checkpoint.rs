//! One idle WAL checkpoint on a live shared session (#86): when the last
//! call or Sandbox in a process releases a Store it keeps open, its
//! write-ahead log is copied into the Store file and truncated to zero
//! bytes, so a quiesced Store's size is its file. The session stays open;
//! nothing is closed or sealed here.
use super::connection::Session;
use super::query;
use crate::backend::records::BackendError;

impl Session {
    /// Runs `PRAGMA wal_checkpoint(TRUNCATE)` once. `Ok(true)`: the log is
    /// empty (it was, or it is now). `Ok(false)`: it could not complete now
    /// and is left as it is (its frames stay valid; the last close checkpoints
    /// them): this thread holds the publication lane, another process holds
    /// the writer lane (Linux, tried without waiting), or a reader in another
    /// process still reads a snapshot in the log (`busy`). Nothing is retried
    /// and no busy handler waits. Short holds: the FIFO turn and the
    /// connection lock are held for this one statement only.
    pub(crate) fn checkpoint_idle(&self) -> Result<bool, BackendError> {
        if !self.writable {
            return Err(BackendError::ReadOnly);
        }
        // A stat of the log's path opens no descriptor on SQLite's files.
        if wal_bytes(&self.path)? == 0 {
            return Ok(true);
        }
        let turn = self
            .admission
            .enter(layerfs_telemetry::timer::waits::Wait::Writer)?;
        if turn.nested {
            return Ok(false);
        }
        let mut state = self.state.lock().map_err(|_| BackendError::Unknown)?;
        super::thread_work::attributed(&mut state, |state| {
            if state.quarantined {
                return Err(BackendError::Unknown);
            }
            if !state.connection.is_autocommit() {
                state.quarantined = true;
                return Err(BackendError::Unknown);
            }
            // A TRUNCATE checkpoint takes SQLite's WAL write lock: a writer of
            // another process entering meanwhile would fail Busy, so the
            // writers' lane is taken first, or the checkpoint is left.
            let Some(writer) =
                crate::backend::sqlite::lane::writer_admission::Writer::try_acquire(&self.path)?
            else {
                return Ok(false);
            };
            state.writer_hold = Some(writer);
            let result = query::run(
                &state.connection,
                "PRAGMA wal_checkpoint(TRUNCATE)",
                vec![],
                &state.work,
            );
            if !state.connection.is_autocommit() {
                state.quarantined = true;
            }
            state.finish_writer()?;
            let rows = result?;
            let busy = rows.first().ok_or(BackendError::Integrity)?.get::<i64>(0)?;
            Ok(busy == 0)
        })
    }
}

/// The length of the Store's `-wal` file; zero when there is none.
fn wal_bytes(path: &std::path::Path) -> Result<u64, BackendError> {
    let mut wal = path.as_os_str().to_os_string();
    wal.push("-wal");
    match std::fs::symlink_metadata(wal) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(_) => Err(BackendError::Integrity),
    }
}
