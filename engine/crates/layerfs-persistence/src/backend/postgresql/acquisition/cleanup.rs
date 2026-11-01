//! Bounded removal of one operation's working rows and of its record.
//!
//! Each job deletes the lowest remaining keys of the operation, entries first,
//! and moves exactly the removed rows' payload out of the held charge. The
//! shared tables and indexes are never dropped. Removed rows' space is the
//! server's to reuse; there is no file to reclaim.
use super::reads::get;
use super::writes::credit;
use crate::backend::{
    postgresql::session::transaction::Transaction,
    records::BackendError,
    sqlite::acquisition::statements::{signed, unsigned, Failure},
};
use layerfs_storage::port::acquisition::{
    Abandoned, AcquisitionWork, Discarded, Disposal, Owner, Phase,
};
use postgres::types::ToSql;

const ENTRY_KEY: &str = "SELECT parent_position,name FROM init_entry WHERE operation_id=$1::bigint ORDER BY parent_position,name LIMIT 1 OFFSET $2::bigint";
const ENTRY_TAIL: &str = "SELECT parent_position,name FROM init_entry WHERE operation_id=$1::bigint ORDER BY parent_position DESC,name DESC LIMIT 1";
const DISCARD_ENTRIES: &str = "DELETE FROM init_entry WHERE operation_id=$1::bigint AND (parent_position,name)<=($2::bigint,$3::bytea) RETURNING (octet_length(name)+coalesce(octet_length(host_path),0)+coalesce(octet_length(host_identity),0)+32+coalesce(octet_length(content_root),0))::bigint";
const HOST_FILE_KEY: &str = "SELECT canonical_position FROM init_host_file WHERE operation_id=$1::bigint ORDER BY canonical_position LIMIT 1 OFFSET $2::bigint";
const HOST_FILE_TAIL: &str = "SELECT canonical_position FROM init_host_file WHERE operation_id=$1::bigint ORDER BY canonical_position DESC LIMIT 1";
const DISCARD_HOST_FILES: &str = "DELETE FROM init_host_file WHERE operation_id=$1::bigint AND canonical_position<=$2::bigint RETURNING (octet_length(host_path)+60+coalesce(octet_length(file_root),0))::bigint";
const RELEASE: &str = "DELETE FROM init_operation WHERE operation_id=$1::bigint AND owner_epoch=$2::bigint AND held_rows=0 RETURNING operation_id";
const ABANDONED: &str = "SELECT operation_id,owner_epoch,phase,held_rows,held_bytes FROM init_operation WHERE operation_id>$1::bigint AND owner_epoch<>$2::bigint ORDER BY operation_id LIMIT $3::bigint";

/// Removes rows through one statement; returns their count and bytes.
fn remove(
    tx: &Transaction<'_>,
    sql: &str,
    values: &[&(dyn ToSql + Sync)],
    bound: u64,
) -> Result<(i64, i64), Failure> {
    let removed = tx.typed(sql, values, bound)?;
    let mut bytes = 0_i64;
    for row in &removed {
        bytes += get::<i64>(row, 0)?;
    }
    Ok((removed.len() as i64, bytes))
}

/// The inclusive last key of at most `budget` lowest keys: an indexed offset
/// bounded by one job, or the tail when fewer remain.
fn last_key(
    tx: &Transaction<'_>,
    key: &str,
    tail: &str,
    operation: i64,
    budget: i64,
) -> Result<Option<postgres::Row>, Failure> {
    let mut last = tx.typed(key, &[&operation, &(budget - 1)], 16)?;
    if last.is_empty() {
        last = tx.typed(tail, &[&operation], 8)?;
    }
    Ok(last.into_iter().next())
}

/// One removal job; the provider has matched the owner in this transaction.
pub(super) fn discard(
    tx: &Transaction<'_>,
    target: Owner,
    budget: i64,
) -> Result<Discarded, Failure> {
    let operation = signed(target.operation)?;
    let (mut rows, mut bytes) = (0, 0);
    if budget != 0 {
        if let Some(row) = last_key(tx, ENTRY_KEY, ENTRY_TAIL, operation, budget)? {
            let (parent, name): (i64, Vec<u8>) = (get(&row, 0)?, get(&row, 1)?);
            (rows, bytes) = remove(
                tx,
                DISCARD_ENTRIES,
                &[&operation, &parent, &name],
                16 + name.len() as u64,
            )?;
        }
    }
    if budget - rows != 0 {
        if let Some(row) = last_key(tx, HOST_FILE_KEY, HOST_FILE_TAIL, operation, budget - rows)? {
            let position: i64 = get(&row, 0)?;
            let (host_rows, host_bytes) =
                remove(tx, DISCARD_HOST_FILES, &[&operation, &position], 16)?;
            rows += host_rows;
            bytes += host_bytes;
        }
    }
    let remaining_rows = credit(tx, target, rows, bytes)?;
    Ok(Discarded {
        rows: unsigned(rows)?,
        bytes: unsigned(bytes)?,
        remaining_rows,
    })
}

fn release_record(tx: &Transaction<'_>, target: Owner) -> Result<(), Failure> {
    let (operation, epoch) = (signed(target.operation)?, signed(target.epoch)?);
    if tx.typed(RELEASE, &[&operation, &epoch], 16)?.is_empty() {
        return Err(Failure::Stale);
    }
    Ok(())
}

/// The charge snapshot was read with the owner check in this transaction.
pub(super) fn release(
    tx: &Transaction<'_>,
    target: Owner,
    work: AcquisitionWork,
) -> Result<(), Failure> {
    if work.held_rows != 0 {
        return Err(Failure::Held);
    }
    release_record(tx, target)
}

/// One removal job; the record goes with the operation's last row, atomically.
pub(super) fn dispose(
    tx: &Transaction<'_>,
    target: Owner,
    budget: i64,
) -> Result<Disposal, Failure> {
    let discarded = discard(tx, target, budget)?;
    let released = discarded.remaining_rows == 0;
    if released {
        release_record(tx, target)?;
    }
    Ok(Disposal {
        discarded,
        released,
    })
}

pub(super) fn abandoned(
    tx: &Transaction<'_>,
    current_epoch: u64,
    after_operation: Option<u64>,
    limit: i64,
) -> Result<Vec<Abandoned>, Failure> {
    let after = after_operation.map_or(Ok(0), signed)?;
    let rows = tx.typed(ABANDONED, &[&after, &signed(current_epoch)?, &limit], 24)?;
    rows.iter()
        .map(|row| {
            let phase = u8::try_from(get::<i64>(row, 2)?)
                .ok()
                .and_then(Phase::from_code)
                .ok_or(BackendError::Integrity)?;
            Ok(Abandoned {
                owner: Owner {
                    operation: unsigned(get(row, 0)?)?,
                    epoch: unsigned(get(row, 1)?)?,
                },
                phase,
                held_rows: unsigned(get(row, 3)?)?,
                held_bytes: unsigned(get(row, 4)?)?,
            })
        })
        .collect()
}
