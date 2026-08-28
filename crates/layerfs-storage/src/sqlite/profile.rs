use crate::{map_sqlite_error, note_statement, EngineError, EngineResult};
use rusqlite::Connection;
use std::time::Duration;

pub(crate) const BUSY_TIMEOUT: Duration = Duration::ZERO;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SqliteProfile {
    pub journal_mode: String,
    pub synchronous: i64,
    pub temp_store: i64,
    pub mmap_size: i64,
    pub page_size: i64,
    pub cache_pages: i64,
    pub cache_spill_pages: i64,
}

pub(crate) fn configure_profile_counted(
    connection: &Connection,
    statements: &mut u64,
) -> EngineResult<SqliteProfile> {
    note_statement(statements)?;
    let journal_mode = connection
        .query_row("PRAGMA journal_mode=DELETE", [], |row| {
            row.get::<_, String>(0)
        })
        .map_err(map_sqlite_error)?;
    *statements = statements
        .checked_add(6)
        .ok_or(EngineError::CounterOverflow)?;
    connection
        .execute_batch(
            "PRAGMA synchronous=FULL; PRAGMA temp_store=FILE; PRAGMA mmap_size=0; PRAGMA cache_size=1280; PRAGMA cache_spill=ON; PRAGMA foreign_keys=ON;",
        )
        .map_err(map_sqlite_error)?;
    note_statement(statements)?;
    let synchronous = connection
        .query_row("PRAGMA synchronous", [], |row| row.get::<_, i64>(0))
        .map_err(map_sqlite_error)?;
    note_statement(statements)?;
    let temp_store = connection
        .query_row("PRAGMA temp_store", [], |row| row.get::<_, i64>(0))
        .map_err(map_sqlite_error)?;
    note_statement(statements)?;
    let mmap_size = connection
        .query_row("PRAGMA mmap_size", [], |row| row.get::<_, i64>(0))
        .map_err(map_sqlite_error)?;
    note_statement(statements)?;
    let page_size = connection
        .query_row("PRAGMA page_size", [], |row| row.get::<_, i64>(0))
        .map_err(map_sqlite_error)?;
    note_statement(statements)?;
    let cache_pages = connection
        .query_row("PRAGMA cache_size", [], |row| row.get::<_, i64>(0))
        .map_err(map_sqlite_error)?;
    note_statement(statements)?;
    let cache_spill_pages = connection
        .query_row("PRAGMA cache_spill", [], |row| row.get::<_, i64>(0))
        .map_err(map_sqlite_error)?;
    note_statement(statements)?;
    let foreign_keys = connection
        .query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
        .map_err(map_sqlite_error)?;
    let profile = SqliteProfile {
        journal_mode,
        synchronous,
        temp_store,
        mmap_size,
        page_size,
        cache_pages,
        cache_spill_pages,
    };
    if !profile.journal_mode.eq_ignore_ascii_case("DELETE")
        || profile.synchronous != 2
        || profile.temp_store != 1
        || profile.mmap_size != 0
        || profile.page_size != 4096
        || profile.cache_pages != 1280
        || profile.cache_spill_pages != 1280
        || foreign_keys != 1
    {
        return Err(EngineError::SqliteProfileMismatch(profile));
    }
    Ok(profile)
}
