//! Embedded connection profile: no WAL, no synchronous flush, no busy waiting.
//!
//! The selected profile is MEMORY journal, `synchronous = OFF`, memory temporary
//! storage and a zero busy timeout. The RAM rollback journal is retained, so a
//! runtime transaction still aborts atomically; nothing here adds crash
//! durability, and no product path calls `fsync`, `fdatasync`, `sync_all` or
//! `sync_data`. A locked Store is a definite failure, not a wait.

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

use crate::error::{StorageError, StorageResult};

/// Opens or creates the Store database and applies the profile.
pub fn open(path: &Path, create: bool) -> StorageResult<Connection> {
    let flags = if create {
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    };
    let connection = Connection::open_with_flags(path, flags)?;
    configure(&connection)?;
    Ok(connection)
}

/// Applies the declared pragma profile to an already open connection.
pub fn configure(connection: &Connection) -> StorageResult<()> {
    // `journal_mode` returns the resulting mode; MEMORY keeps rollback atomicity
    // without a disk journal and is not WAL.
    let journal: String =
        connection.query_row("PRAGMA journal_mode = MEMORY", [], |row| row.get(0))?;
    if !journal.eq_ignore_ascii_case("memory") {
        return Err(StorageError::Integrity("journal mode"));
    }
    connection.execute_batch("PRAGMA synchronous = OFF; PRAGMA temp_store = MEMORY;")?;
    connection.execute_batch("PRAGMA foreign_keys = ON;")?;
    let foreign_keys: i64 = connection.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
    if foreign_keys != 1 {
        return Err(StorageError::Integrity("foreign key enforcement"));
    }
    connection.busy_timeout(Duration::ZERO)?;
    Ok(())
}

/// Reads one integer pragma.
pub fn pragma_i64(connection: &Connection, name: &str) -> StorageResult<i64> {
    let sql = format!("PRAGMA {name}");
    Ok(connection.query_row(&sql, [], |row| row.get(0))?)
}

/// True when the engine reports that this failure is a lost write lock.
pub fn is_busy(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(inner, _)
            if matches!(
                inner.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            )
    )
}

/// Maps a lost ownership attempt to its explicit failure.
pub fn ownership_error(error: rusqlite::Error) -> StorageError {
    if is_busy(&error) {
        StorageError::OwnershipUnavailable
    } else {
        StorageError::Engine(error)
    }
}
