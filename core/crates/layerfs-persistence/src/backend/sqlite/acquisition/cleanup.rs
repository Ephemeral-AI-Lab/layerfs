//! Bounded removal of one operation's working rows and of its record.
//!
//! Each job deletes the lowest remaining keys of the operation, entries first,
//! and moves exactly the removed rows' payload out of the held charge. The
//! shared tables and indexes are never dropped.
use super::accounting::{credit, owner};
use super::statements::{
    signed, unsigned, Failure, ABANDONED, DISCARD_ENTRIES, DISCARD_ENTRY_KEYS, DISCARD_ENTRY_TAIL,
    DISCARD_NATIVE, DISCARD_NATIVE_KEYS, DISCARD_NATIVE_TAIL, RELEASE, RELEASE_ABANDONED,
};
use crate::backend::{records::BackendError, Transaction};
use layerfs_storage::port::acquisition::{
    Abandoned, AcquisitionWork, Discarded, Disposal, Owner, Phase,
};

/// Removes at most `budget` rows through one statement; returns rows and bytes.
fn remove(
    tx: &Transaction<'_>,
    statement: &str,
    values: &[&dyn rusqlite::ToSql],
    bytes_bound: u64,
) -> Result<(i64, i64), Failure> {
    let removed = tx.borrowed(statement, values, bytes_bound)?;
    let mut bytes = 0_i64;
    for row in &removed {
        bytes += row.get::<i64>(0)?;
    }
    Ok((removed.len() as i64, bytes))
}

/// Select one inclusive endpoint after at most `budget` indexed keys. The
/// offset is bounded by this job (4095), and each acknowledged job removes
/// that prefix: it never grows with an enumeration cursor or population.
/// A short final prefix uses an indexed reverse seek. Both reads and DELETE
/// share this writable unit. SQLite still owns bounded RETURNING scratch.
fn remove_entries(
    tx: &Transaction<'_>,
    operation: i64,
    budget: i64,
) -> Result<(i64, i64), Failure> {
    if budget == 0 {
        return Ok((0, 0));
    }
    let mut last = tx.borrowed(DISCARD_ENTRY_KEYS, &[&operation, &(budget - 1)], 16)?;
    if last.is_empty() {
        last = tx.borrowed(DISCARD_ENTRY_TAIL, &[&operation], 8)?;
    }
    match last.first_mut() {
        Some(row) => {
            let parent: i64 = row.get(0)?;
            let name = row.take_bytes(1)?;
            remove(
                tx,
                DISCARD_ENTRIES,
                &[&operation, &parent, &name],
                16 + name.len() as u64,
            )
        }
        None => Ok((0, 0)),
    }
}

fn remove_native(tx: &Transaction<'_>, operation: i64, budget: i64) -> Result<(i64, i64), Failure> {
    if budget == 0 {
        return Ok((0, 0));
    }
    let mut last = tx.borrowed(DISCARD_NATIVE_KEYS, &[&operation, &(budget - 1)], 16)?;
    if last.is_empty() {
        last = tx.borrowed(DISCARD_NATIVE_TAIL, &[&operation], 8)?;
    }
    match last.first() {
        Some(row) => {
            let position: i64 = row.get(0)?;
            remove(tx, DISCARD_NATIVE, &[&operation, &position], 16)
        }
        None => Ok((0, 0)),
    }
}

fn discard_rows(tx: &Transaction<'_>, target: Owner, budget: i64) -> Result<Discarded, Failure> {
    let operation = signed(target.operation)?;
    let (entry_rows, entry_bytes) = remove_entries(tx, operation, budget)?;
    let (native_rows, native_bytes) = remove_native(tx, operation, budget - entry_rows)?;
    let (rows, bytes) = (entry_rows + native_rows, entry_bytes + native_bytes);
    let remaining_rows = credit(tx, target, rows, bytes)?;
    Ok(Discarded {
        rows: unsigned(rows)?,
        bytes: unsigned(bytes)?,
        remaining_rows,
    })
}

/// The provider has already matched this owner in the same transaction.
pub(crate) fn discard_validated(
    tx: &Transaction<'_>,
    target: Owner,
    budget: i64,
) -> Result<Discarded, Failure> {
    let discarded = discard_rows(tx, target, budget)?;
    maintain(tx, discarded.rows != 0, false)?;
    Ok(discarded)
}

fn release_record(tx: &Transaction<'_>, target: Owner, sql: &str) -> Result<(), Failure> {
    let released = tx.borrowed(
        sql,
        &[&signed(target.operation)?, &signed(target.epoch)?],
        16,
    )?;
    if released.is_empty() {
        return Err(Failure::Stale);
    }
    Ok(())
}

/// The charge snapshot was read with owner validation in this transaction.
pub(crate) fn release_validated(
    tx: &Transaction<'_>,
    target: Owner,
    work: AcquisitionWork,
) -> Result<(), Failure> {
    if work.held_rows != 0 {
        return Err(Failure::Held);
    }
    release_record(tx, target, RELEASE)?;
    maintain(tx, false, true)
}

/// One page-removal job per acknowledgment, including a combined final job.
fn maintain(tx: &Transaction<'_>, removed: bool, released: bool) -> Result<(), Failure> {
    if tx.reclamation_enabled() {
        if released {
            super::super::reclamation::job(tx, crate::RECLAMATION_PAGE_LIMIT)?;
        } else if removed {
            super::super::reclamation::job_when(
                tx,
                crate::RECLAMATION_PAGE_LIMIT,
                u64::from(crate::RECLAMATION_PAGE_LIMIT),
            )?;
        }
    }
    Ok(())
}

fn dispose(
    tx: &Transaction<'_>,
    target: Owner,
    budget: i64,
    release_sql: &str,
) -> Result<Disposal, Failure> {
    let discarded = discard_rows(tx, target, budget)?;
    let released = discarded.remaining_rows == 0;
    if released {
        release_record(tx, target, release_sql)?;
    }
    maintain(tx, discarded.rows != 0, released)?;
    Ok(Disposal {
        discarded,
        released,
    })
}

/// The provider has matched the live owner; deletion and release are atomic.
pub(crate) fn dispose_validated(
    tx: &Transaction<'_>,
    target: Owner,
    budget: i64,
) -> Result<Disposal, Failure> {
    dispose(tx, target, budget, RELEASE)
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
    // Abandoned disposal does not enter the provider's live-owner wrapper.
    owner(tx, target)?;
    Ok(dispose(tx, target, budget, RELEASE_ABANDONED)?.discarded)
}
