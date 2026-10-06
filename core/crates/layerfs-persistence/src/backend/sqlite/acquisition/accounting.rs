//! Owner checks and exact row/payload-byte charges of one operation.
//!
//! A row's payload is its variable bytes: name, native path, native identity
//! and object identities. The same amounts are charged when a row is written,
//! adjusted when it changes and read back from the engine when it is removed,
//! so an operation whose rows are all gone holds exactly zero.
use super::statements::{unsigned, Failure, CHARGE, CREDIT, OWNER};
use crate::backend::{records::BackendError, Transaction};
use layerfs_storage::port::acquisition::{AcquisitionWork, NewEntry, Owner, Phase};

/// Bytes of one stored object identity.
pub(crate) const ROOT_BYTES: i64 = 32;
/// Bytes of one stored native identity with its evidence.
pub(crate) const NATIVE_BYTES: i64 = 60;

/// Payload bytes of one entry row as it is inserted.
pub(crate) fn entry_bytes(entry: &NewEntry) -> i64 {
    let stored_path = match entry.kind {
        layerfs_content::inode_leaf::InodeKind::Directory => entry.native_path.as_deref(),
        _ => None,
    };
    let unplaced_native = entry.position.is_none() && entry.native.is_some();
    entry.key.name.len() as i64
        + stored_path.map_or(0, |path| path.len() as i64)
        + if unplaced_native { NATIVE_BYTES } else { 0 }
        + ROOT_BYTES
        + entry.target_root.map_or(0, |_| ROOT_BYTES)
}

/// Payload bytes of one first-path native row as it is inserted.
pub(crate) fn native_bytes(path: &[u8]) -> i64 {
    path.len() as i64 + NATIVE_BYTES
}

fn identity(owner: Owner) -> Result<[i64; 2], Failure> {
    Ok([
        super::statements::signed(owner.operation)?,
        super::statements::signed(owner.epoch)?,
    ])
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
