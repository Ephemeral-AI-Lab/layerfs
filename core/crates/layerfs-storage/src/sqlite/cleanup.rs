//! Bounded cleanup of a definitely failed, unpublished save.
//!
//! Cleanup authority comes from the baseline pack identifier captured under
//! exclusive ownership: only packs created after it belong to the failed attempt.
//! Deletion walks owned locators newest first, so every direct dependent is gone
//! before its base, and removes owned pack rows last. The attempt runs once; a
//! failure is surfaced and never retried by an outer wrapper or a destructor.
//! Each transaction it opens is bounded by the writer's own row and canonical-byte
//! limits, because the journal is held in memory. Both passes are paged: the
//! object rows and the pack rows (which carry the pack bodies) are deleted a
//! bounded page at a time, and a page that would cross a limit commits before the
//! next one starts.

use rusqlite::Connection;

use crate::error::{StorageError, StorageResult};
use crate::policy::{CLEANUP_PAGE_ROWS, TRANSACTION_CANONICAL_BYTES_LIMIT, TRANSACTION_ROW_LIMIT};
use crate::sqlite::write;

/// What one cleanup attempt removed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CleanupReport {
    /// Object rows removed.
    pub objects: u64,
    /// Pack rows removed.
    pub packs: u64,
    /// Deletion pages executed, across both passes.
    pub pages: u64,
}

/// Removes every object and pack this failed save created.
///
/// The caller rolls the open transaction back first; a rolled-back attempt leaves
/// only earlier acknowledged rows, and those are the ones this walks.
pub fn abandon(connection: &Connection, baseline_pack_id: i64) -> StorageResult<CleanupReport> {
    write::begin_immediate(connection)?;
    let mut report = CleanupReport::default();
    // The attempt is one cleanup, but not one unbounded transaction: the journal
    // is held in memory, so a transaction that deleted every owned row at once
    // would hold every modified page in RAM. The same row and canonical-byte
    // discipline the writer uses bounds each transaction here, and the deletion
    // order is unchanged - newest locator first, so a dependent is always gone
    // before its base.
    let mut open = write::TransactionState { rows: 1, bytes: 0 };
    loop {
        let mut statement = connection.prepare_cached(
            "SELECT object_id, canonical_length FROM objects WHERE pack_id > ?1 \
             ORDER BY pack_id DESC, group_number DESC, record_number DESC LIMIT ?2",
        )?;
        let rows: Vec<(Vec<u8>, i64)> = statement
            .query_map(
                rusqlite::params![baseline_pack_id, CLEANUP_PAGE_ROWS as i64],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?)),
            )?
            .collect::<Result<_, _>>()?;
        drop(statement);
        if rows.is_empty() {
            break;
        }
        let mut page_bytes = 0_u64;
        let placeholders = (1..=rows.len())
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!("DELETE FROM objects WHERE object_id IN ({placeholders})");
        let parameters: Vec<rusqlite::types::Value> = rows
            .iter()
            .map(|(id, length)| {
                page_bytes = page_bytes.saturating_add((*length).max(0) as u64);
                rusqlite::types::Value::Blob(id.clone())
            })
            .collect();
        let removed = connection.execute(&sql, rusqlite::params_from_iter(parameters))?;
        if removed == 0 || removed > rows.len() {
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
        open.rows = open.rows.saturating_add(removed as u64);
        open.bytes = open.bytes.saturating_add(page_bytes);
        if open.rows >= TRANSACTION_ROW_LIMIT || open.bytes >= TRANSACTION_CANONICAL_BYTES_LIMIT {
            // Commit what this page removed and continue the same attempt in a
            // fresh bounded transaction. A failure here is surfaced as it is; the
            // attempt is never retried and no outer wrapper resumes it.
            write::commit(connection)?;
            write::begin_immediate(connection)?;
            open = write::TransactionState { rows: 1, bytes: 0 };
        }
    }
    // The pack rows carry the pack bodies themselves, so this pass is paged and
    // charged exactly like the objects pass: the journal is held in memory, and
    // one statement that deleted every remaining pack at once would dirty roughly
    // one page per 4096 deleted bytes inside a single transaction.
    loop {
        let mut statement = connection.prepare_cached(
            "SELECT pack_id, length(data) FROM object_packs WHERE pack_id > ?1 \
             ORDER BY pack_id DESC LIMIT ?2",
        )?;
        let rows: Vec<(i64, i64)> = statement
            .query_map(
                rusqlite::params![baseline_pack_id, CLEANUP_PAGE_ROWS as i64],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )?
            .collect::<Result<_, _>>()?;
        drop(statement);
        if rows.is_empty() {
            break;
        }
        let mut page_bytes = 0_u64;
        let placeholders = (1..=rows.len())
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!("DELETE FROM object_packs WHERE pack_id IN ({placeholders})");
        let parameters: Vec<rusqlite::types::Value> = rows
            .iter()
            .map(|(id, length)| {
                page_bytes = page_bytes.saturating_add((*length).max(0) as u64);
                rusqlite::types::Value::Integer(*id)
            })
            .collect();
        let removed = connection.execute(&sql, rusqlite::params_from_iter(parameters))?;
        if removed == 0 || removed > rows.len() {
            return Err(StorageError::Integrity("cleanup pack delete cardinality"));
        }
        report.packs = report
            .packs
            .checked_add(removed as u64)
            .ok_or(StorageError::Integrity("cleanup accounting"))?;
        report.pages += 1;
        if report.pages > 1_000_000 {
            return Err(StorageError::Integrity("cleanup page budget"));
        }
        open.rows = open.rows.saturating_add(removed as u64);
        open.bytes = open.bytes.saturating_add(page_bytes);
        if open.rows >= TRANSACTION_ROW_LIMIT || open.bytes >= TRANSACTION_CANONICAL_BYTES_LIMIT {
            write::commit(connection)?;
            write::begin_immediate(connection)?;
            open = write::TransactionState { rows: 1, bytes: 0 };
        }
    }
    write::commit(connection)?;
    Ok(report)
}
