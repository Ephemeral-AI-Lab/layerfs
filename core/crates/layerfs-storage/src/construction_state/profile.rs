//! Closed private SQLite profile and exact readback, without a global heap claim.

use rusqlite::{limits::Limit, Connection, OpenFlags};

use crate::error::{StorageError, StorageResult};

use super::{native::RESERVED_BYTES, plan::Plan, status::ScratchProfile};

pub(crate) const APPLICATION_ID: i64 = 0x4c46_4353;
pub(crate) const ROW_LIMIT: u64 = 65_536;
pub(crate) const RECORD_BYTES: u64 = 63;
const SCHEMA: &str = include_str!("../../sql/construction_scratch.sql");
const PHASED_SCHEMA: &str = include_str!("../../sql/construction_phased.sql");
const SITES_SCHEMA: &str = include_str!("../../sql/construction_sites.sql");

const LIMITS: &[(Limit, i32)] = &[
    (Limit::SQLITE_LIMIT_LENGTH, 65_536),
    (Limit::SQLITE_LIMIT_SQL_LENGTH, 8_192),
    (Limit::SQLITE_LIMIT_COLUMN, 16),
    (Limit::SQLITE_LIMIT_EXPR_DEPTH, 64),
    (Limit::SQLITE_LIMIT_COMPOUND_SELECT, 1),
    (Limit::SQLITE_LIMIT_VDBE_OP, 10_000),
    (Limit::SQLITE_LIMIT_FUNCTION_ARG, 8),
    (Limit::SQLITE_LIMIT_ATTACHED, 0),
    (Limit::SQLITE_LIMIT_LIKE_PATTERN_LENGTH, 0),
    (Limit::SQLITE_LIMIT_VARIABLE_NUMBER, 8),
    (Limit::SQLITE_LIMIT_TRIGGER_DEPTH, 0),
    (Limit::SQLITE_LIMIT_WORKER_THREADS, 0),
];

pub(crate) fn open(path: &std::path::Path) -> StorageResult<Connection> {
    // Native ownership has already created this exact empty file. No CREATE or
    // open-time adoption/migration is permitted here.
    Ok(Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?)
}

pub(crate) fn initialize(
    connection: &Connection,
    header: &[u8],
    plan: Plan,
    claim_scope: Option<&[u8; 81]>,
    site_scope: Option<&[u8; 89]>,
) -> StorageResult<ScratchProfile> {
    let journal: String =
        connection.query_row("PRAGMA journal_mode = MEMORY", [], |row| row.get(0))?;
    if !journal.eq_ignore_ascii_case("memory") {
        return Err(StorageError::UnsupportedPolicy {
            field: "construction scratch journal",
        });
    }
    connection.execute_batch("PRAGMA synchronous=OFF; PRAGMA temp_store=MEMORY; PRAGMA foreign_keys=ON; PRAGMA cache_size=-512; PRAGMA mmap_size=0;")?;
    connection.busy_timeout(std::time::Duration::ZERO)?;
    for (limit, value) in LIMITS {
        connection.set_limit(*limit, *value)?;
        if connection.limit(*limit)? != *value {
            return Err(StorageError::UnsupportedPolicy {
                field: "construction scratch connection limit",
            });
        }
    }
    crate::sqlite::write::begin_immediate(connection)?;
    let schema = (|| {
        let affected = match plan {
            Plan::Legacy => {
                connection.execute_batch(SCHEMA)?;
                connection.execute(
                    "INSERT INTO session_owner VALUES (1,?1,NULL,0,0,0,NULL)",
                    [header],
                )?
            }
            Plan::ClaimsThenRoots {
                directories,
                bindings,
            } => {
                let scope = claim_scope
                    .ok_or(StorageError::Integrity("construction scratch phased scope"))?;
                connection.execute_batch(PHASED_SCHEMA)?;
                connection.execute("INSERT INTO session_owner VALUES (1,?1,NULL,0,0,0,NULL,?2,?3,?4,0,0,0,NULL,NULL,NULL)",
                    rusqlite::params![header, directories as i64, bindings as i64, scope.as_slice()])?
            }
            Plan::SitesThenRoots {
                directories,
                bindings,
                ..
            } => {
                let scope =
                    site_scope.ok_or(StorageError::Integrity("construction scratch site scope"))?;
                connection.execute_batch(SITES_SCHEMA)?;
                let affected = connection.execute(
                    "INSERT INTO session_owner VALUES(1,?1,NULL,0,0,0,NULL,?2,?3)",
                    rusqlite::params![header, directories as i64, bindings as i64],
                )?;
                if affected != 1 {
                    return Err(StorageError::Integrity(
                        "construction scratch site root owner",
                    ));
                }
                connection.execute(
                    "INSERT INTO site_owner VALUES(1,?1,0,0,0,NULL,NULL,NULL,NULL,NULL)",
                    [scope.as_slice()],
                )?
            }
        };
        if affected != 1 {
            return Err(StorageError::Integrity(
                "construction scratch header insertion",
            ));
        }
        Ok(())
    })();
    finish_write(connection, schema)?;
    let pages: i64 = connection.query_row("PRAGMA max_page_count=4096", [], |row| row.get(0))?;
    if pages != (RESERVED_BYTES / 4096) as i64 {
        return Err(StorageError::UnsupportedPolicy {
            field: "construction scratch logical page limit",
        });
    }
    readback(connection, plan.version())
}

pub(crate) fn finish_write(
    connection: &Connection,
    result: StorageResult<()>,
) -> StorageResult<()> {
    finish_transaction(connection, result)
}

pub(crate) fn finish_transaction<T>(
    connection: &Connection,
    result: StorageResult<T>,
) -> StorageResult<T> {
    match result {
        Ok(value) => crate::sqlite::write::commit(connection).map(|()| value),
        Err(original) => match crate::sqlite::write::rollback(connection) {
            Ok(()) => Err(original),
            Err(cleanup) => Err(StorageError::UnknownOutcome {
                original: Box::new(StorageError::CleanupFailed {
                    original: Box::new(original),
                    cleanup: Box::new(cleanup),
                }),
            }),
        },
    }
}

pub(crate) fn readback(connection: &Connection, version: u16) -> StorageResult<ScratchProfile> {
    let read =
        |sql: &str| -> StorageResult<i64> { Ok(connection.query_row(sql, [], |row| row.get(0))?) };
    let profile = ScratchProfile {
        application_id: read("PRAGMA application_id")?,
        version: read("PRAGMA user_version")?,
        page_size: read("PRAGMA page_size")?,
        cache_size: read("PRAGMA cache_size")?,
        mmap_size: read("PRAGMA mmap_size")?,
        max_page_count: read("PRAGMA max_page_count")?,
        synchronous: read("PRAGMA synchronous")?,
        temp_store: read("PRAGMA temp_store")?,
        foreign_keys: read("PRAGMA foreign_keys")?,
        busy_timeout: read("PRAGMA busy_timeout")?,
    };
    let journal: String = connection.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
    if profile
        != (ScratchProfile {
            application_id: APPLICATION_ID,
            version: i64::from(version),
            page_size: 4096,
            cache_size: -512,
            mmap_size: 0,
            max_page_count: 4096,
            synchronous: 0,
            temp_store: 2,
            foreign_keys: 1,
            busy_timeout: 0,
        })
        || !journal.eq_ignore_ascii_case("memory")
    {
        return Err(StorageError::UnsupportedPolicy {
            field: "construction scratch profile readback",
        });
    }
    for (limit, value) in LIMITS {
        if connection.limit(*limit)? != *value {
            return Err(StorageError::UnsupportedPolicy {
                field: "construction scratch connection limit",
            });
        }
    }
    Ok(profile)
}
