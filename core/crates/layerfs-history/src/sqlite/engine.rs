//! Generic host participation; C5 has no C2 ownership or allocator dependency.
use crate::{HistoryError, HistoryResult};
use rusqlite::{limits::Limit, Connection, OptionalExtension};
use std::sync::Arc;
/// Required host boundary for explicitly participating constructors.
/// Native Server's private implementation borrows its actual established guard.
/// This callback itself is not evidence about a linked allocator or heap fit.
pub trait EngineParticipation: Send + Sync {
    /// Validate the continuing host participation before a required native boundary.
    fn validate(&self) -> HistoryResult<()>;
}
pub(crate) type Participation = Option<Arc<dyn EngineParticipation>>;
pub(crate) fn validate(engine: Option<&dyn EngineParticipation>) -> HistoryResult<()> {
    match engine {
        Some(engine) => engine.validate(),
        None => Ok(()),
    }
}
const LIMITS: &[(Limit, i32)] = &[
    (Limit::SQLITE_LIMIT_LENGTH, 65536),
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
pub(crate) fn configure(connection: &Connection, reference: bool) -> HistoryResult<()> {
    let journal: String = connection
        .query_row("PRAGMA journal_mode=MEMORY", [], |r| r.get(0))
        .map_err(super::rows::sql)?;
    if !journal.eq_ignore_ascii_case("memory") {
        return Err(HistoryError::Unsupported("participating catalog journal"));
    }
    connection
        .execute_batch("PRAGMA cache_size=-2048; PRAGMA mmap_size=0;")
        .map_err(super::rows::sql)?;
    for &(kind, value) in LIMITS {
        connection
            .set_limit(kind, value)
            .map_err(super::rows::sql)?;
    }
    verify(connection, reference)
}
pub(crate) fn verify(connection: &Connection, reference: bool) -> HistoryResult<()> {
    for (sql, expected) in [
        ("PRAGMA cache_size", -2048),
        ("PRAGMA synchronous", 0),
        ("PRAGMA temp_store", 2),
        ("PRAGMA foreign_keys", 1),
        ("PRAGMA busy_timeout", 0),
    ] {
        let value: i64 = connection
            .query_row(sql, [], |r| r.get(0))
            .map_err(super::rows::sql)?;
        if value != expected {
            return Err(HistoryError::Integrity(
                "participating catalog connection profile",
            ));
        }
    }
    let mmap: Option<i64> = connection
        .query_row("PRAGMA mmap_size", [], |r| r.get(0))
        .optional()
        .map_err(super::rows::sql)?;
    // The separately identified in-memory schema reference has no mapped-file API.
    if mmap != Some(0) && !(reference && mmap.is_none()) {
        return Err(HistoryError::Integrity("participating catalog mmap"));
    }
    let journal: String = connection
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .map_err(super::rows::sql)?;
    if !journal.eq_ignore_ascii_case("memory") {
        return Err(HistoryError::Integrity("participating catalog journal"));
    }
    for &(kind, value) in LIMITS {
        if connection.limit(kind).map_err(super::rows::sql)? != value {
            return Err(HistoryError::Unsupported(
                "participating catalog connection limit",
            ));
        }
    }
    Ok(())
}
