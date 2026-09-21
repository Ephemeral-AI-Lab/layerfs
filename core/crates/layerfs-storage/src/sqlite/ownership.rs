//! Native file arbitration and save-scoped publication. No lock spans an upload.
//!
//! Private save ownership is the Store's configured writer budget (#216), not a
//! fixed slot pair: `store_policy.max_concurrent_writes` is read inside the
//! allocation transaction, so the one authoritative number is shared by every
//! sandbox and process using the file. The slot a save records is drawn from the
//! whole supported slot space, which keeps the rows a higher earlier setting
//! produced valid after the budget is lowered.
use std::{
    path::Path,
    sync::{Arc, Mutex, MutexGuard, OnceLock, Weak},
};

use rusqlite::Connection;

use crate::error::{StorageError, StorageResult};
use crate::sqlite::{schema, write};

const STORE_SLOTS: usize = 64;
type Registry = Vec<((u64, u64), Weak<Mutex<()>>)>;
static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();

pub(crate) fn arbitration(path: &Path) -> StorageResult<Arc<Mutex<()>>> {
    #[cfg(unix)]
    let key = {
        use std::os::unix::fs::MetadataExt;
        let metadata =
            std::fs::metadata(path).map_err(|_| StorageError::Integrity("Store file identity"))?;
        (metadata.dev(), metadata.ino())
    };
    #[cfg(not(unix))]
    return Err(StorageError::UnsupportedPolicy {
        field: "native Store file arbitration",
    });
    #[cfg(unix)]
    {
        let mut registry = REGISTRY
            .get_or_init(|| Mutex::new(Vec::new()))
            .lock()
            .map_err(|_| StorageError::Integrity("Store arbitration registry"))?;
        registry.retain(|(_, owner)| owner.strong_count() != 0);
        if let Some(owner) = registry
            .iter()
            .find_map(|(identity, owner)| (*identity == key).then(|| owner.upgrade()).flatten())
        {
            return Ok(owner);
        }
        if registry.len() >= STORE_SLOTS {
            return Err(StorageError::CapacityExceeded {
                what: "native Store handles",
                limit: STORE_SLOTS as u64,
                actual: registry.len() as u64 + 1,
            });
        }
        let owner = Arc::new(Mutex::new(()));
        registry.push((key, Arc::downgrade(&owner)));
        Ok(owner)
    }
}

pub(crate) fn lock(owner: &Mutex<()>) -> StorageResult<MutexGuard<'_, ()>> {
    owner
        .lock()
        .map_err(|_| StorageError::Integrity("Store arbitration"))
}

pub(crate) fn initialize_scope(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch(
        "CREATE TEMP TABLE layerfs_read_scope (save_id INTEGER NOT NULL, publication INTEGER NOT NULL);
         INSERT INTO layerfs_read_scope VALUES (0, 9223372036854775807);",
    )?;
    Ok(())
}

pub(crate) fn scope(connection: &Connection, save: i64, publication: i64) -> StorageResult<()> {
    if connection.execute(
        "UPDATE temp.layerfs_read_scope SET save_id = ?1, publication = ?2",
        [save, publication],
    )? != 1
    {
        return Err(StorageError::Integrity("read scope cardinality"));
    }
    Ok(())
}

pub(crate) fn publication(connection: &Connection) -> StorageResult<i64> {
    Ok(connection.query_row(
        "SELECT publication_sequence FROM store_policy WHERE id = 1",
        [],
        |row| row.get(0),
    )?)
}

/// Reserves one private save under the Store's persisted writer budget.
///
/// The budget is read inside this transaction, so two processes cannot admit
/// more writers than the Store allows between them. A retained owner - a save
/// whose cleanup or outcome is unresolved - is counted like any other live
/// owner and keeps its slot whatever the budget is now, which is why a lowered
/// setting can never make ownership reusable. The slot is the lowest free one
/// inside `1..=budget`; the upper bound is the budget and the shipped CHECK still
/// constrains the whole supported space.
pub(crate) fn acquire(connection: &Connection) -> StorageResult<(i64, i64)> {
    write::begin_immediate(connection)?;
    let budget = i64::from(schema::max_concurrent_writes(connection)?);
    let live: i64 = connection.query_row(
        "SELECT COUNT(*) FROM saves WHERE active_slot IS NOT NULL",
        [],
        |row| row.get(0),
    )?;
    if live >= budget {
        write::rollback(connection)?;
        return Err(StorageError::OwnershipUnavailable);
    }
    let slot: Option<i64> = connection.query_row(
        "WITH RECURSIVE candidate(slot) AS ( \
           SELECT 1 UNION ALL SELECT slot + 1 FROM candidate WHERE slot < ?1 \
         ) \
         SELECT MIN(slot) FROM candidate \
         WHERE NOT EXISTS (SELECT 1 FROM saves WHERE active_slot = candidate.slot)",
        [budget],
        |row| row.get(0),
    )?;
    // `live < budget` distinct slots exist in a space of `budget` values, so a
    // free slot inside it always remains; a NULL here means the invariant broke.
    let Some(slot) = slot else {
        write::rollback(connection)?;
        return Err(StorageError::Integrity("save slot allocation"));
    };
    let visible = publication(connection)?;
    connection.execute("INSERT INTO saves (active_slot) VALUES (?1)", [slot])?;
    let save = connection.last_insert_rowid();
    write::commit(connection)?;
    scope(connection, save, visible)?;
    Ok((save, visible))
}

pub(crate) fn publish(connection: &Connection, save: i64) -> StorageResult<()> {
    connection.execute(
        "UPDATE store_policy SET publication_sequence = publication_sequence + 1,
         retained_pack_ceiling = MAX(retained_pack_ceiling, (SELECT pack_ceiling FROM saves WHERE save_id = ?1))
         WHERE id = 1 AND publication_sequence < 9223372036854775807", [save],
    )?;
    if connection.changes() != 1 {
        return Err(StorageError::Integrity("publication sequence exhausted"));
    }
    if connection.execute(
        "UPDATE saves SET active_slot = NULL,
         publication = (SELECT publication_sequence FROM store_policy WHERE id = 1)
         WHERE save_id = ?1 AND active_slot IS NOT NULL AND publication IS NULL",
        [save],
    )? != 1
    {
        return Err(StorageError::Integrity("save publication ownership"));
    }
    Ok(())
}

pub(crate) fn next_pack(connection: &Connection) -> StorageResult<i64> {
    Ok(connection.query_row(
        "SELECT next_pack_id FROM store_policy WHERE id = 1",
        [],
        |row| row.get(0),
    )?)
}

pub(crate) fn advance_pack(connection: &Connection, next: i64) -> StorageResult<()> {
    if connection.execute(
        "UPDATE store_policy SET next_pack_id = ?1 WHERE id = 1 AND next_pack_id <= ?1",
        [next],
    )? != 1
    {
        return Err(StorageError::Integrity("pack allocation"));
    }
    Ok(())
}

pub(crate) fn reserve_ordinals(connection: &Connection, count: usize) -> StorageResult<u32> {
    let first: i64 = connection.query_row(
        "SELECT next_ordinal FROM store_policy WHERE id = 1",
        [],
        |row| row.get(0),
    )?;
    let end = first
        .checked_add(count as i64)
        .filter(|end| *end <= u32::MAX as i64 + 1)
        .ok_or(StorageError::Integrity("metadata ordinal maximum"))?;
    connection.execute(
        "UPDATE store_policy SET next_ordinal = ?1,
         metadata_window_start = CASE WHEN metadata_window_values + ?2 > 131072 THEN ?3 ELSE metadata_window_start END,
         metadata_window_values = CASE WHEN metadata_window_values + ?2 > 131072 THEN ?2 ELSE metadata_window_values + ?2 END
         WHERE id = 1",
        [end, count as i64, first],
    )?;
    u32::try_from(first).map_err(|_| StorageError::Integrity("metadata ordinal maximum"))
}
