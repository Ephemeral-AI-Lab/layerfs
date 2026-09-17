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

/// Verifies that an already open connection carries the declared profile.
///
/// `Store::open` applies the profile; a connection that reaches an owner by
/// any other route is re-verified here instead of being trusted, so a
/// connection with another journal mode, synchronous setting, foreign-key
/// enforcement or busy timeout is refused before the first write. Verification
/// reads the pragmas back and never rewrites them.
pub fn verify_profile(connection: &Connection) -> StorageResult<()> {
    let journal: String = connection.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
    if !journal.eq_ignore_ascii_case("memory") {
        return Err(StorageError::Integrity("journal mode"));
    }
    if pragma_i64(connection, Pragma::Synchronous)? != 0 {
        return Err(StorageError::Integrity("synchronous mode"));
    }
    if pragma_i64(connection, Pragma::ForeignKeys)? != 1 {
        return Err(StorageError::Integrity("foreign key enforcement"));
    }
    if pragma_i64(connection, Pragma::BusyTimeout)? != 0 {
        return Err(StorageError::Integrity("busy timeout"));
    }
    Ok(())
}

/// The integer pragmas the product reads.
///
/// The set is closed on purpose: a pragma is never addressed through a
/// caller-supplied string, so no caller input can reach SQL text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pragma {
    /// `PRAGMA application_id`: the schema's application identity.
    ApplicationId,
    /// `PRAGMA user_version`: the schema's version.
    UserVersion,
    /// `PRAGMA synchronous`: the durability level; the profile is OFF (0).
    Synchronous,
    /// `PRAGMA temp_store`: the temporary storage; the profile is MEMORY (2).
    TempStore,
    /// `PRAGMA foreign_keys`: constraint enforcement; the profile is ON (1).
    ForeignKeys,
    /// `PRAGMA busy_timeout`: the wait before a locked Store fails; 0.
    BusyTimeout,
    /// `PRAGMA page_size`: the engine's page size, read for evidence only.
    PageSize,
    /// `PRAGMA cache_size`: the engine's page cache, read for evidence only.
    CacheSize,
    /// `PRAGMA mmap_size`: the memory-mapped I/O limit, read for evidence only.
    MmapSize,
}

impl Pragma {
    /// The pragma's literal SQL text.
    const fn sql(self) -> &'static str {
        match self {
            Self::ApplicationId => "PRAGMA application_id",
            Self::UserVersion => "PRAGMA user_version",
            Self::Synchronous => "PRAGMA synchronous",
            Self::TempStore => "PRAGMA temp_store",
            Self::ForeignKeys => "PRAGMA foreign_keys",
            Self::BusyTimeout => "PRAGMA busy_timeout",
            Self::PageSize => "PRAGMA page_size",
            Self::CacheSize => "PRAGMA cache_size",
            Self::MmapSize => "PRAGMA mmap_size",
        }
    }
}

/// Reads one declared integer pragma.
pub fn pragma_i64(connection: &Connection, pragma: Pragma) -> StorageResult<i64> {
    Ok(connection.query_row(pragma.sql(), [], |row| row.get(0))?)
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
