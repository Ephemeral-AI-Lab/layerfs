//! Selected disposable profile, startup readback and physical observations.
use crate::{DatabaseWork, OverlayError, OverlayResult, StatementKind};
use rusqlite::Connection;
use std::cell::RefCell;

/// Daemon-wide resource settings selected before open.
#[derive(Clone, Copy, Debug)]
pub struct ProfileConfig {
    /// Suggested aggregate pager KiB; not a process/OS resident ceiling.
    pub pager_kib: u32,
    /// Optional explicitly selected physical database quota. None uses SQLite's
    /// format ceiling, without a LayerFS total-state limit. Device headroom and
    /// per-job resource admission are separate obligations.
    pub max_pages: Option<u32>,
}
impl Default for ProfileConfig {
    fn default() -> Self {
        Self {
            pager_kib: 2048,
            max_pages: None,
        }
    }
}
/// Actual read-back engine/build settings. No crash durability is claimed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatabaseProfile {
    pub schema_version: i64,
    pub sqlite_version: String,
    pub compile_options: Vec<String>,
    pub journal_mode: String,
    pub locking_mode: String,
    pub synchronous: i64,
    pub mmap_size: i64,
    pub cache_size: i64,
    pub page_size: i64,
    pub max_pages: i64,
    pub explicit_page_quota: Option<u32>,
    pub foreign_keys: i64,
    pub busy_timeout: i64,
    pub temp_store: i64,
    pub auto_vacuum: i64,
}
pub(crate) fn initialize(
    c: &Connection,
    config: ProfileConfig,
    work: &RefCell<DatabaseWork>,
    configuration_calls: &mut u64,
) -> OverlayResult<DatabaseProfile> {
    let max_pages = config.max_pages.unwrap_or(u32::MAX - 1);
    if config.pager_kib == 0 || max_pages < 32 || max_pages == u32::MAX {
        return Err(OverlayError::Invalid("profile resource settings"));
    }
    *configuration_calls += 1;
    c.busy_timeout(std::time::Duration::ZERO)?;
    *configuration_calls += 1;
    c.set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)?;
    crate::diagnostics::startup::batch(c, work, &format!(
        "PRAGMA page_size=4096; PRAGMA auto_vacuum=NONE; PRAGMA journal_mode=MEMORY; PRAGMA synchronous=OFF;
         PRAGMA locking_mode=EXCLUSIVE; PRAGMA mmap_size=0; PRAGMA foreign_keys=ON;
         PRAGMA temp_store=FILE; PRAGMA cache_size=-{}; PRAGMA max_page_count={};",
        config.pager_kib, max_pages
    ))?;
    let integer = |name: &str| readback(c, work, &format!("PRAGMA {name}"), |r| r.get::<_, i64>(0));
    let text = |name: &str| {
        readback(c, work, &format!("PRAGMA {name}"), |r| {
            r.get::<_, String>(0)
        })
    };
    let p = DatabaseProfile {
        schema_version: 0,
        sqlite_version: rusqlite::version().to_owned(),
        compile_options: crate::metrics::query(
            c,
            work,
            StatementKind::Startup,
            crate::metrics::Query {
                sql: "PRAGMA compile_options",
                params: &[],
                bound_bytes: 0,
                cached: false,
            },
            |r| r.get(0),
        )?,
        journal_mode: text("journal_mode")?,
        locking_mode: text("locking_mode")?,
        synchronous: integer("synchronous")?,
        mmap_size: integer("mmap_size")?,
        cache_size: integer("cache_size")?,
        page_size: integer("page_size")?,
        max_pages: integer("max_page_count")?,
        explicit_page_quota: config.max_pages,
        foreign_keys: integer("foreign_keys")?,
        busy_timeout: integer("busy_timeout")?,
        temp_store: integer("temp_store")?,
        auto_vacuum: integer("auto_vacuum")?,
    };
    // Physical growth derivation is pinned to these actual SQLite builds.
    // A new system/bundled version requires source review and qualification.
    if !matches!(p.sqlite_version.as_str(), "3.51.0" | "3.53.2") {
        return Err(OverlayError::Invalid(
            "unqualified SQLite physical growth profile",
        ));
    }
    if p.journal_mode != "memory"
        || p.locking_mode != "exclusive"
        || p.synchronous != 0
        || p.mmap_size != 0
        || p.cache_size != -i64::from(config.pager_kib)
        || p.page_size != 4096
        || p.max_pages != i64::from(max_pages)
        || p.foreign_keys != 1
        || p.busy_timeout != 0
        || p.temp_store != 1
        || p.auto_vacuum != 0
    {
        return Err(OverlayError::Invalid("profile readback differs"));
    }
    Ok(p)
}

pub(crate) fn readback<T>(
    connection: &Connection,
    work: &RefCell<DatabaseWork>,
    sql: &str,
    decode: impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
) -> OverlayResult<T> {
    let mut values = crate::metrics::query(
        connection,
        work,
        StatementKind::Startup,
        crate::metrics::Query {
            sql,
            params: &[],
            bound_bytes: 0,
            cached: false,
        },
        decode,
    )?;
    if values.len() != 1 {
        return Err(OverlayError::Invalid("startup readback cardinality"));
    }
    Ok(values.remove(0))
}
