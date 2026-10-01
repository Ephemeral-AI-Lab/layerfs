//! Explicit participating connection shape; page-cache targets are not heap admission.
use crate::{StorageError, StorageResult};
use rusqlite::{limits::Limit, Connection};
/// Closed actual native factory role, independent of Store or engine credit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionClass {
    /// Store constructor and native Save connection, configured2MiB page-cache target.
    ContentWrite,
    /// Canonical read session, configured1MiB page-cache target.
    ContentRead,
}
impl ConnectionClass {
    /// Exact configured page-cache KiB target; this is not native allocation admission.
    pub const fn cache_kib(self) -> i64 {
        match self {
            Self::ContentWrite => 2048,
            Self::ContentRead => 1024,
        }
    }
}
const LIMITS: &[(Limit, i32)] = &[
    (Limit::SQLITE_LIMIT_LENGTH, 16 * 1024 * 1024 + 8192),
    (Limit::SQLITE_LIMIT_SQL_LENGTH, 65536),
    (Limit::SQLITE_LIMIT_COLUMN, 64),
    (Limit::SQLITE_LIMIT_EXPR_DEPTH, 256),
    (Limit::SQLITE_LIMIT_COMPOUND_SELECT, 32),
    (Limit::SQLITE_LIMIT_FUNCTION_ARG, 32),
    (Limit::SQLITE_LIMIT_ATTACHED, 0),
    (Limit::SQLITE_LIMIT_LIKE_PATTERN_LENGTH, 4096),
    (Limit::SQLITE_LIMIT_VARIABLE_NUMBER, 1024),
    (Limit::SQLITE_LIMIT_TRIGGER_DEPTH, 32),
    (Limit::SQLITE_LIMIT_WORKER_THREADS, 0),
];
/// Apply exact safe limits before object/schema work, with actual readback.
pub(crate) fn configure(connection: &Connection, class: ConnectionClass) -> StorageResult<()> {
    connection.pragma_update(None, "cache_size", -class.cache_kib())?;
    connection.execute_batch("PRAGMA mmap_size=0;")?;
    for &(kind, value) in LIMITS {
        connection.set_limit(kind, value)?;
    }
    verify(connection, class)
}
/// Runtime profile validation only; a foreign setting is never silently restored.
pub(crate) fn verify(connection: &Connection, class: ConnectionClass) -> StorageResult<()> {
    let cache: i64 = connection.query_row("PRAGMA cache_size", [], |r| r.get(0))?;
    let mmap: i64 = connection.query_row("PRAGMA mmap_size", [], |r| r.get(0))?;
    if cache != -class.cache_kib() || mmap != 0 {
        return Err(StorageError::Integrity(
            "participating SQLite cache/mmap profile",
        ));
    }
    for &(kind, value) in LIMITS {
        if connection.limit(kind)? != value {
            return Err(StorageError::UnsupportedPolicy {
                field: "participating SQLite connection limit",
            });
        }
    }
    Ok(())
}
