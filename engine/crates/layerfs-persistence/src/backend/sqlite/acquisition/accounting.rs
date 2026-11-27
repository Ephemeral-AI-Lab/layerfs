//! Owner checks and exact row/payload-byte charges of one operation; the
//! byte amounts are `acquisition::payload`'s.
use super::statements::{CHARGE, CREDIT, OWNER};
use crate::acquisition::failure::{signed, unsigned, Failure};
use crate::backend::{error::BackendError, Transaction};
use layerfs_storage::port::acquisition::{AcquisitionWork, Owner, Phase};

fn identity(owner: Owner) -> Result<[i64; 2], Failure> {
    Ok([signed(owner.operation)?, signed(owner.epoch)?])
}

/// The operation's phase and charges; `Stale` when the owner is not live.
pub(crate) fn owner(
    tx: &Transaction<'_>,
    owner: Owner,
) -> Result<(Phase, AcquisitionWork), Failure> {
    let [operation, epoch] = identity(owner)?;
    let rows = tx.borrowed(OWNER, &[&operation, &epoch], 16)?;
    let row = rows.first().ok_or(Failure::Stale)?;
    let phase = u8::try_from(row.get::<i64>(0)?)
        .ok()
        .and_then(Phase::from_code)
        .ok_or(BackendError::Integrity)?;
    let mut counts = [0_u64; 6];
    for (index, count) in counts.iter_mut().enumerate() {
        *count = unsigned(row.get::<i64>(index + 1)?)?;
    }
    let [held_rows, held_bytes, peak_rows, peak_bytes, removed_rows, removed_bytes] = counts;
    Ok((
        phase,
        AcquisitionWork {
            held_rows,
            held_bytes,
            peak_rows,
            peak_bytes,
            removed_rows,
            removed_bytes,
        },
    ))
}

/// Adds signed row and byte deltas; returns the rows now held.
pub(crate) fn charge(
    tx: &Transaction<'_>,
    owner: Owner,
    rows: i64,
    bytes: i64,
) -> Result<u64, Failure> {
    let [operation, epoch] = identity(owner)?;
    let updated = tx.borrowed(CHARGE, &[&operation, &epoch, &rows, &bytes], 32)?;
    unsigned(updated.first().ok_or(Failure::Stale)?.get::<i64>(0)?)
}

/// Moves removed rows and bytes out of the held charge; returns rows now held.
pub(crate) fn credit(
    tx: &Transaction<'_>,
    owner: Owner,
    rows: i64,
    bytes: i64,
) -> Result<u64, Failure> {
    let [operation, epoch] = identity(owner)?;
    let updated = tx.borrowed(CREDIT, &[&operation, &epoch, &rows, &bytes], 32)?;
    unsigned(updated.first().ok_or(Failure::Stale)?.get::<i64>(0)?)
}
