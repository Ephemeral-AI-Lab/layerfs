//! Paged, save-owned deletion. A slot is released only after complete cleanup.
use crate::{
    error::{StorageError, StorageResult},
    policy::CLEANUP_PAGE_ROWS,
    sqlite::{ownership, write},
};
use rusqlite::Connection;
use std::sync::Mutex;

/// Counts established deletion work from one cleanup attempt.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CleanupReport {
    /// Locators removed.
    pub objects: u64,
    /// Packs removed, including their cascading value-group rows.
    pub packs: u64,
    /// Acknowledged deletion pages.
    pub pages: u64,
}

/// Removes only one definitely failed private save, without retrying a page.
pub fn abandon(
    connection: &Connection,
    save_id: i64,
    arbitration: &Mutex<()>,
) -> StorageResult<CleanupReport> {
    let mut report = CleanupReport::default();
    for kind in ["objects", "content_signatures", "object_packs"] {
        loop {
            let _guard = ownership::lock(arbitration)?;
            write::begin_immediate(connection)?;
            let private: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM saves WHERE save_id=?1 AND active_slot IS NOT NULL)",
                [save_id],
                |row| row.get(0),
            )?;
            if !private {
                return Err(StorageError::Integrity("cleanup save ownership"));
            }
            let removed=match kind {
                "objects" => connection.execute(
                    "DELETE FROM objects WHERE save_id=?1 AND object_id IN (SELECT object_id FROM objects WHERE save_id=?1 ORDER BY object_id LIMIT ?2)",
                    rusqlite::params![save_id,CLEANUP_PAGE_ROWS as i64],
                )?,
                "content_signatures" => connection.execute(
                    "DELETE FROM content_signatures WHERE slot IN (SELECT slot FROM content_signatures WHERE save_id=?1 ORDER BY slot LIMIT ?2)",
                    rusqlite::params![save_id,CLEANUP_PAGE_ROWS as i64],
                )?,
                // One pack per transaction bounds blob journal ownership even for
                // the existing singleton format; ordinary packs are <=256 KiB.
                _ => connection.execute(
                    "DELETE FROM object_packs WHERE pack_id=(SELECT pack_id FROM object_packs WHERE save_id=?1 ORDER BY pack_id LIMIT 1)", [save_id],
                )?,
            };
            if removed == 0 {
                write::rollback(connection)?;
                break;
            }
            write::commit(connection)?;
            report.pages += 1;
            match kind {
                "objects" => report.objects += removed as u64,
                "object_packs" => report.packs += removed as u64,
                _ => {}
            }
        }
    }
    let _guard = ownership::lock(arbitration)?;
    write::begin_immediate(connection)?;
    if connection.execute(
        "DELETE FROM saves WHERE save_id=?1 AND active_slot IS NOT NULL",
        [save_id],
    )? != 1
    {
        return Err(StorageError::Integrity("cleanup save cardinality"));
    }
    write::commit(connection)?;
    Ok(report)
}
