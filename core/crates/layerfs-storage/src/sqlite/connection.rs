//! Embedded connection profile: no WAL, no synchronous flush, no busy waiting.
//!
//! The selected profile is MEMORY journal, `synchronous = OFF`, memory temporary
//! storage, a page cache declared in pages and a zero busy timeout. The RAM rollback journal is retained, so a
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
    let arbitration = super::ownership::arbitration(path)?;
    let _guard = super::ownership::lock(&arbitration)?;
    configure(&connection)?;
    super::ownership::initialize_scope(&connection)?;
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
    apply_cache_size(connection)?;
    connection.busy_timeout(Duration::ZERO)?;
    Ok(())
}

/// Declares the page cache in the engine's own unit: pages, not bytes.
///
/// The engine's default (`cache_size = -2000` KiB) is a byte count that is 512
/// pages only at the 4096-byte page the engine assumes. A Store created with a
/// wider page keeps the same bytes and therefore a fraction of the pages, and a
/// statement that seeks a random leaf then reads, journals and modifies a whole
/// page instead of a row — measured at 6.63 us per row insert against 4.20 us for
/// the narrow default, and removed by declaring the page count
/// ([`crate::policy::STORE_CACHE_PAGES`]).
///
/// The value is derived from the page size the **connection's own file** reports,
/// so a Store created at any page size gets the declared count of pages and no
/// Store is starved by a wider page. It is read back rather than assumed, and a
/// file whose page size the engine cannot report is refused.
pub fn apply_cache_size(connection: &Connection) -> StorageResult<()> {
    let page = pragma_i64(connection, Pragma::PageSize)?;
    if page <= 0 {
        return Err(StorageError::Integrity("page size"));
    }
    let kib = crate::policy::STORE_CACHE_PAGES.saturating_mul(page) / 1024;
    connection.execute_batch(&format!("PRAGMA cache_size = -{kib}"))?;
    if pragma_i64(connection, Pragma::CacheSize)? != -kib {
        return Err(StorageError::Integrity("page cache size"));
    }
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
    // The cache is declared in pages and derived from this file's page size, so
    // the check reads both back: a connection left with the engine's byte default
    // is refused exactly as one with another journal mode is.
    let page = pragma_i64(connection, Pragma::PageSize)?;
    let declared = crate::policy::STORE_CACHE_PAGES.saturating_mul(page) / 1024;
    if pragma_i64(connection, Pragma::CacheSize)? != -declared {
        return Err(StorageError::Integrity("page cache size"));
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
    /// `PRAGMA cache_spill`: whether a full page cache may spill; read for
    /// evidence, set by the profile when the profile declares it.
    CacheSpill,
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
            Self::CacheSpill => "PRAGMA cache_spill",
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
