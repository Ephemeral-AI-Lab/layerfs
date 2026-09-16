//! Bounded cleanup of a definitely failed, unpublished save.
//!
//! Cleanup authority comes from the baseline pack identifier captured under
//! exclusive ownership: only packs created after it belong to the failed attempt.
//! Deletion walks owned locators newest first, so every direct dependent is gone
//! before its base, and removes owned pack rows last. The attempt runs once; a
//! failure is surfaced and never retried by an outer wrapper or a destructor.

use rusqlite::Connection;

use crate::error::{StorageError, StorageResult};
use crate::policy::CLEANUP_PAGE_ROWS;
use crate::sqlite::write;

/// What one cleanup attempt removed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CleanupReport {
    /// Object rows removed.
    pub objects: u64,
    /// Pack rows removed.
    pub packs: u64,
    /// Deletion pages executed.
    pub pages: u64,
}

/// Removes every object and pack this failed save created.
///
/// The caller rolls the open transaction back first; a rolled-back attempt leaves
/// only earlier acknowledged rows, and those are the ones this walks.
pub fn abandon(connection: &Connection, baseline_pack_id: i64) -> StorageResult<CleanupReport> {
    write::begin_immediate(connection)?;
    let mut report = CleanupReport::default();
    loop {
        let mut statement = connection.prepare_cached(
            "SELECT object_id FROM objects WHERE pack_id > ?1 \
             ORDER BY pack_id DESC, group_number DESC, record_number DESC LIMIT ?2",
        )?;
        let ids: Vec<Vec<u8>> = statement
            .query_map(
                rusqlite::params![baseline_pack_id, CLEANUP_PAGE_ROWS as i64],
                |row| row.get::<_, Vec<u8>>(0),
            )?
            .collect::<Result<_, _>>()?;
        drop(statement);
        if ids.is_empty() {
            break;
        }
        let placeholders = (1..=ids.len())
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!("DELETE FROM objects WHERE object_id IN ({placeholders})");
        let parameters: Vec<rusqlite::types::Value> = ids
            .iter()
            .map(|id| rusqlite::types::Value::Blob(id.clone()))
            .collect();
        let removed = connection.execute(&sql, rusqlite::params_from_iter(parameters))?;
        if removed == 0 || removed > ids.len() {
            return Err(StorageError::Integrity("cleanup delete cardinality"));
        }
        report.objects = report
            .objects
            .checked_add(removed as u64)
            .ok_or(StorageError::Integrity("cleanup accounting"))?;
        report.pages += 1;
        if report.pages > 1_000_000 {
            return Err(StorageError::Integrity("cleanup page budget"));
        }
    }
    let packs = connection.execute(
        "DELETE FROM object_packs WHERE pack_id > ?1",
        [baseline_pack_id],
    )?;
    report.packs = packs as u64;
    write::commit(connection)?;
    Ok(report)
}
