//! Selected disposable profile, startup readback and physical observations.
use crate::{OverlayError, OverlayResult};
use rusqlite::Connection;

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
}
pub(crate) fn initialize(c: &Connection, config: ProfileConfig) -> OverlayResult<DatabaseProfile> {
    let max_pages = config.max_pages.unwrap_or(u32::MAX - 1);
    if config.pager_kib == 0 || max_pages < 32 || max_pages == u32::MAX {
        return Err(OverlayError::Invalid("profile resource settings"));
    }
    c.busy_timeout(std::time::Duration::ZERO)?;
    c.set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)?;
    c.execute_batch(&format!(
        "PRAGMA page_size=4096; PRAGMA journal_mode=MEMORY; PRAGMA synchronous=OFF;
         PRAGMA locking_mode=EXCLUSIVE; PRAGMA mmap_size=0; PRAGMA foreign_keys=ON;
         PRAGMA temp_store=FILE; PRAGMA cache_size=-{}; PRAGMA max_page_count={};",
        config.pager_kib, max_pages
    ))?;
    let integer = |name: &str| c.query_row(&format!("PRAGMA {name}"), [], |r| r.get(0));
    let text = |name: &str| c.query_row(&format!("PRAGMA {name}"), [], |r| r.get(0));
    let p = DatabaseProfile {
        schema_version: 0,
        sqlite_version: rusqlite::version().to_owned(),
        compile_options: c
            .prepare("PRAGMA compile_options")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?,
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
    };
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
    {
        return Err(OverlayError::Invalid("profile readback differs"));
    }
    Ok(p)
}
