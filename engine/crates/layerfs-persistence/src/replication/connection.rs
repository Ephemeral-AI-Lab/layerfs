//! Opening the Store and a bundle, taking the Store's writer lane, and
//! beginning and ending a write transaction.
use super::ChangeError;
use crate::backend::sqlite::lane::writer_admission::Writer;
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

/// Opens `path` with no busy handler, so a locked file is refused at once: one
/// attempt, never a wait (rusqlite installs a 5 s busy handler by default).
pub(super) fn open(path: &Path, flags: OpenFlags) -> Result<Connection, ChangeError> {
    let c = Connection::open_with_flags(path, flags | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    c.busy_timeout(std::time::Duration::ZERO)?;
    Ok(c)
}

/// Opens the Store at `path` with the Store's own connection settings under
/// the Disposable profile, the only one that executes: WAL, synchronous OFF.
/// A Store that is not in WAL mode is refused.
pub(super) fn open_store(path: &Path, flags: OpenFlags) -> Result<Connection, ChangeError> {
    let c = open(path, flags)?;
    let work = std::cell::RefCell::new(crate::SqlWork::default());
    // The acquisition schema is read only when a Store is created.
    crate::backend::sqlite::session::profile::apply(
        &c,
        false,
        crate::SqlitePersistenceProfile::Disposable,
        crate::SqliteAcquisitionSchema::Absent,
        &work,
    )
    .map_err(|error| ChangeError::Store(error.into()))?;
    Ok(c)
}

/// Takes the writer lane of the Store at `path`: the lane each of the Store's
/// writers takes for every write transaction and writable open and close. One
/// non-blocking attempt: while another writer holds it, [`ChangeError::Busy`].
/// The caller declares the lane before its connection, so the connection,
/// whose close can checkpoint, is dropped while the lane is still held.
pub(super) fn lane(path: &Path) -> Result<Writer, ChangeError> {
    match Writer::try_acquire(path) {
        Ok(Some(writer)) => Ok(writer),
        Ok(None) => Err(ChangeError::Busy),
        Err(error) => Err(ChangeError::Store(error.into())),
    }
}

/// Begins the write transaction: one attempt. A connection that bypasses the
/// writer lane and holds SQLite's write lock makes it [`ChangeError::Busy`].
pub(super) fn begin(c: &Connection) -> Result<(), ChangeError> {
    c.execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| match error.sqlite_error_code() {
            Some(rusqlite::ErrorCode::DatabaseBusy) => ChangeError::Busy,
            _ => error.into(),
        })
}

/// Commits `outcome`'s transaction or rolls it back.
pub(super) fn commit<T>(c: &Connection, outcome: Result<T, ChangeError>) -> Result<T, ChangeError> {
    let value = match outcome {
        Ok(value) => value,
        Err(error) => {
            let _ = c.execute_batch("ROLLBACK");
            return Err(error);
        }
    };
    if let Err(error) = c.execute_batch("COMMIT") {
        let _ = c.execute_batch("ROLLBACK");
        return Err(match error.sqlite_error_code() {
            Some(rusqlite::ErrorCode::ConstraintViolation) => {
                ChangeError::Integrity("a foreign key is unsatisfied at commit")
            }
            _ => error.into(),
        });
    }
    Ok(value)
}

/// [`commit`], then checkpoints so the Store file alone holds every change. A
/// checkpoint that another reader blocks, or that fails, is reported as
/// [`ChangeError::NotCheckpointed`], because the change has already committed.
pub(super) fn finish<T>(c: &Connection, outcome: Result<T, ChangeError>) -> Result<T, ChangeError> {
    let value = commit(c, outcome)?;
    // The first column is 1 when a reader kept the checkpoint from completing.
    match c.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| {
        r.get::<_, i64>(0)
    }) {
        Ok(0) => Ok(value),
        Ok(_) => Err(ChangeError::NotCheckpointed(None)),
        Err(error) => Err(ChangeError::NotCheckpointed(Some(error))),
    }
}
