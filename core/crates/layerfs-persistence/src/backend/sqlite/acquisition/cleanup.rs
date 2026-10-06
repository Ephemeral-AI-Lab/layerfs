//! Bounded removal of one operation's working rows and of its record.
//!
//! Each job deletes the lowest remaining keys of the operation, entries first,
//! and moves exactly the removed rows' payload out of the held charge. The
//! shared tables and indexes are never dropped.
use super::accounting::{credit, owner};
use super::statements::{
    signed, unsigned, Failure, ABANDONED, DISCARD_ENTRIES, DISCARD_NATIVE, RELEASE,
    RELEASE_ABANDONED,
};
use crate::backend::{records::BackendError, Transaction};
use layerfs_storage::port::acquisition::{Abandoned, Discarded, Owner, Phase};

/// Removes at most `budget` rows through one statement; returns rows and bytes.
fn remove(
    tx: &Transaction<'_>,
    statement: &str,
    operation: i64,
    budget: i64,
) -> Result<(i64, i64), Failure> {
    if budget == 0 {
        return Ok((0, 0));
    }
    let removed = tx.borrowed(statement, &[&operation, &budget], 16)?;
    let mut bytes = 0_i64;
    for row in &removed {
        bytes += row.get::<i64>(0)?;
    }
    Ok((removed.len() as i64, bytes))
}

pub(crate) fn discard(
    tx: &Transaction<'_>,
    target: Owner,
    budget: i64,
) -> Result<Discarded, Failure> {
    // Stale before any row is touched: the keys below carry no epoch.
    owner(tx, target)?;
    let operation = signed(target.operation)?;
    let (entry_rows, entry_bytes) = remove(tx, DISCARD_ENTRIES, operation, budget)?;
    let (native_rows, native_bytes) = remove(tx, DISCARD_NATIVE, operation, budget - entry_rows)?;
    let (rows, bytes) = (entry_rows + native_rows, entry_bytes + native_bytes);
    let remaining_rows = credit(tx, target, rows, bytes)?;
    Ok(Discarded {
        rows: unsigned(rows)?,
        bytes: unsigned(bytes)?,
        remaining_rows,
    })
}

pub(crate) fn release(tx: &Transaction<'_>, target: Owner) -> Result<(), Failure> {
    let (_, work) = owner(tx, target)?;
    if work.held_rows != 0 {
        return Err(Failure::Held);
    }
    let released = tx.borrowed(
        RELEASE,
        &[&signed(target.operation)?, &signed(target.epoch)?],
        16,
    )?;
    if released.is_empty() {
        return Err(Failure::Stale);
    }
    Ok(())
}

pub(crate) fn abandoned(
    tx: &Transaction<'_>,
    current_epoch: u64,
    after_operation: Option<u64>,
    limit: i64,
) -> Result<Vec<Abandoned>, Failure> {
    let rows = tx.borrowed(
        ABANDONED,
        &[
            &after_operation.map_or(Ok(0), signed)?,
            &signed(current_epoch)?,
            &limit,
        ],
        24,
    )?;
    rows.iter()
        .map(|row| {
            let phase = u8::try_from(row.get::<i64>(2)?)
                .ok()
                .and_then(Phase::from_code)
                .ok_or(BackendError::Integrity)?;
            Ok(Abandoned {
                owner: Owner {
                    operation: unsigned(row.get::<i64>(0)?)?,
                    epoch: unsigned(row.get::<i64>(1)?)?,
                },
                phase,
                held_rows: unsigned(row.get::<i64>(3)?)?,
                held_bytes: unsigned(row.get::<i64>(4)?)?,
            })
        })
        .collect()
}

/// One removal job for an abandoned operation; its record goes with its last row.
pub(crate) fn discard_abandoned(
    tx: &Transaction<'_>,
    target: Owner,
    budget: i64,
) -> Result<Discarded, Failure> {
    let discarded = discard(tx, target, budget)?;
    if discarded.remaining_rows == 0 {
        let released = tx.borrowed(
            RELEASE_ABANDONED,
            &[&signed(target.operation)?, &signed(target.epoch)?],
            16,
        )?;
        if released.is_empty() {
            return Err(Failure::Stale);
        }
    }
    Ok(discarded)
}
