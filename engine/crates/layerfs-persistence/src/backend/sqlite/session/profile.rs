//! Explicit immutable connection profile; no automatic Store conversion.
use super::connection::{ConnectionProfile, SqlWork};
use super::query;
use crate::{backend::records::BackendError, SqlitePersistenceProfile};
use rusqlite::Connection;
use std::cell::RefCell;
impl SqlitePersistenceProfile {
    pub(crate) fn identity(self) -> &'static str {
        match self {
            Self::Durable => "sqlite-wal-full-v2",
            Self::Disposable => "sqlite-wal-off-v2",
        }
    }
    pub(crate) fn journal(self) -> &'static str {
        "wal"
    }
    pub(crate) fn synchronous(self) -> i64 {
        match self {
            Self::Durable => 2,
            Self::Disposable => 0,
        }
    }
    pub(crate) fn fullfsync(self) -> i64 {
        i64::from(self == Self::Durable)
    }
    /// Pages after which a COMMIT checkpoints the log itself. Disposable
    /// leaves it to its session's checkpoint thread, off the commit path
    /// (`background.rs`; owner approval, 2026-11-07).
    pub(crate) fn wal_autocheckpoint(self) -> i64 {
        match self {
            Self::Durable => 1000,
            Self::Disposable => 0,
        }
    }
}
pub(crate) fn apply(
    c: &Connection,
    create: bool,
    selected: SqlitePersistenceProfile,
    acquisition: crate::SqliteAcquisitionSchema,
    w: &RefCell<SqlWork>,
) -> Result<(), BackendError> {
    // WAL selection and acquisition setup can write before schema creation.
    // Apply the selected synchronization policy before any such operation.
    for (name, value) in [
        ("synchronous", selected.synchronous()),
        ("fullfsync", selected.fullfsync()),
    ] {
        query::run(c, &format!("PRAGMA {name}={value}"), vec![], w)?;
    }
    if create {
        query::run(c, "PRAGMA page_size=4096", vec![], w)?;
        if acquisition == crate::SqliteAcquisitionSchema::Tables {
            // Select pointer-map support before schema/WAL creation. Opening
            // an existing Store never changes its physical format.
            query::run(c, "PRAGMA auto_vacuum=INCREMENTAL", vec![], w)?;
        }
    } else {
        let mode = query::run(c, "PRAGMA journal_mode", vec![], w)?
            .first()
            .ok_or(BackendError::Integrity)?
            .get::<String>(0)?;
        if mode != "wal" {
            return Err(BackendError::Integrity);
        }
    }
    if create {
        let mode = query::run(
            c,
            &format!("PRAGMA journal_mode={}", selected.journal()),
            vec![],
            w,
        )?
        .first()
        .ok_or(BackendError::Integrity)?
        .get::<String>(0)?;
        if mode != selected.journal() {
            return Err(BackendError::Integrity);
        }
    }
    for (name, value) in [
        ("foreign_keys", 1),
        ("checkpoint_fullfsync", 1),
        ("wal_autocheckpoint", selected.wal_autocheckpoint()),
        ("journal_size_limit", 4194304),
        ("cache_size", -2048),
        ("mmap_size", 0),
        ("temp_store", 2),
    ] {
        query::run(c, &format!("PRAGMA {name}={value}"), vec![], w)?;
    }
    Ok(())
}
pub(crate) fn check(p: &ConnectionProfile) -> Result<(), BackendError> {
    let selected = p.persistence;
    if p.journal_mode != selected.journal()
        || p.synchronous != selected.synchronous()
        || p.foreign_keys != 1
        || p.fullfsync != selected.fullfsync()
        || p.checkpoint_fullfsync != 1
        || p.page_size != 4096
        || !matches!(p.auto_vacuum, 0 | 2)
        || (p.auto_vacuum == 2 && p.acquisition != crate::SqliteAcquisitionSchema::Tables)
        || p.wal_autocheckpoint != selected.wal_autocheckpoint()
        || p.journal_size_limit != 4194304
        || p.cache_size != -2048
        || p.busy_timeout != 0
        || p.mmap_size != 0
        || p.temp_store != 2
        || p.column_limit < 13
        || p.length_limit < layerfs_storage::policy::SINGLETON_PACK_LIMIT + 1024
        || p.sql_length_limit < 4096
        || p.variable_limit < 6
    {
        return Err(BackendError::Integrity);
    }
    Ok(())
}
