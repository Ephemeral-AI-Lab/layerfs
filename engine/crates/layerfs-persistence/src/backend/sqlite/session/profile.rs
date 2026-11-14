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
/// Prototype switches (FUSE profile probe, not a product contract), from
/// `LAYERFS_PROBE_SQLITE`, a comma list of `name=value`: `page=<bytes>`
/// selects the page size of a Store this process creates and lets an open
/// accept that size; `mmap=<bytes>` and `cache=<KiB>` set `mmap_size` and a
/// negative `cache_size` on read-only Store connections only;
/// `locate=<flags>` joined by `+` changes multi-id locates: `typed` decodes
/// rows in place instead of through owned records, `bucket` pads the id list
/// to a power of two (at least 16) so one prepared statement serves many
/// demands, `split` reads `object_location` alone and then each distinct
/// pack descriptor once. Unset, none applies and the profile is unchanged.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Probe {
    pub(crate) page_size: i64,
    pub(crate) mmap: Option<i64>,
    pub(crate) cache_kib: Option<i64>,
    pub(crate) locate_typed: bool,
    pub(crate) locate_bucket: bool,
    pub(crate) locate_split: bool,
}
pub(crate) fn probe() -> Probe {
    static PROBE: std::sync::OnceLock<Probe> = std::sync::OnceLock::new();
    *PROBE.get_or_init(|| {
        let value = std::env::var("LAYERFS_PROBE_SQLITE").unwrap_or_default();
        let get = |name: &str| {
            value.split(',').find_map(|part| {
                let (key, v) = part.trim().split_once('=')?;
                (key == name).then(|| v.trim().to_owned())
            })
        };
        let number = |name: &str| get(name).and_then(|v| v.parse::<i64>().ok());
        let locate = get("locate").unwrap_or_default();
        let flag = |name: &str| locate.split('+').any(|part| part == name);
        Probe {
            page_size: number("page").unwrap_or(4096),
            mmap: number("mmap"),
            cache_kib: number("cache"),
            locate_typed: flag("typed"),
            locate_bucket: flag("bucket"),
            locate_split: flag("split"),
        }
    })
}
/// Prototype: the probe's settings for a read-only Store connection.
pub(crate) fn apply_reader(c: &Connection, w: &RefCell<SqlWork>) -> Result<(), BackendError> {
    let probe = probe();
    if let Some(bytes) = probe.mmap {
        query::run(c, &format!("PRAGMA mmap_size={bytes}"), vec![], w)?;
    }
    if let Some(kib) = probe.cache_kib {
        query::run(c, &format!("PRAGMA cache_size=-{kib}"), vec![], w)?;
    }
    Ok(())
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
        let page = format!("PRAGMA page_size={}", probe().page_size);
        query::run(c, &page, vec![], w)?;
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
pub(crate) fn check(p: &ConnectionProfile, reader: bool) -> Result<(), BackendError> {
    let selected = p.persistence;
    let probe = probe();
    let (cache, mmap) = match reader {
        true => (
            probe.cache_kib.map_or(-2048, |kib| -kib),
            probe.mmap.unwrap_or(0),
        ),
        false => (-2048, 0),
    };
    if p.journal_mode != selected.journal()
        || p.synchronous != selected.synchronous()
        || p.foreign_keys != 1
        || p.fullfsync != selected.fullfsync()
        || p.checkpoint_fullfsync != 1
        || p.page_size != probe.page_size
        || !matches!(p.auto_vacuum, 0 | 2)
        || (p.auto_vacuum == 2 && p.acquisition != crate::SqliteAcquisitionSchema::Tables)
        || p.wal_autocheckpoint != selected.wal_autocheckpoint()
        || p.journal_size_limit != 4194304
        || p.cache_size != cache
        || p.busy_timeout != 0
        || p.mmap_size != mmap
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
