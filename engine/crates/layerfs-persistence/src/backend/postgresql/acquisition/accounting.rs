//! Owner checks and exact row/payload-byte charges of one operation, shared by
//! every write unit (placement included).
use super::reads::get;
use crate::acquisition::failure::{signed, unsigned, Failure};
use crate::backend::{error::BackendError, postgresql::session::transaction::Transaction};
use layerfs_storage::port::acquisition::{AcquisitionWork, Owner, Phase};

/// The owner row's statement key (`history::statements::qualified`); its
/// text names the schema, so it also runs alone as a fenced autocommit read
/// (`reads::work`, #818 I3a).
const OWNER: &str = "acquisition_owner";
const CHARGE: &str = "UPDATE init_operation SET held_rows=held_rows+$3::bigint,held_bytes=held_bytes+$4::bigint,peak_rows=greatest(peak_rows,held_rows+$3::bigint),peak_bytes=greatest(peak_bytes,held_bytes+$4::bigint) WHERE operation_id=$1::bigint AND owner_epoch=$2::bigint RETURNING held_rows";
const CREDIT: &str = "UPDATE init_operation SET held_rows=held_rows-$3::bigint,held_bytes=held_bytes-$4::bigint,removed_rows=removed_rows+$3::bigint,removed_bytes=removed_bytes+$4::bigint WHERE operation_id=$1::bigint AND owner_epoch=$2::bigint RETURNING held_rows";

pub(super) fn identity(owner: Owner) -> Result<(i64, i64), Failure> {
    Ok((signed(owner.operation)?, signed(owner.epoch)?))
}

/// The operation's phase and charges; `Stale` when the owner is not live.
pub(super) fn owner(
    tx: &Transaction<'_>,
    owner: Owner,
) -> Result<(Phase, AcquisitionWork), Failure> {
    let (operation, epoch) = identity(owner)?;
    let rows = tx.typed(&super::reads::owned(tx, OWNER)?, &[&operation, &epoch], 16)?;
    decoded(rows.first().ok_or(Failure::Stale)?, 0)
}

/// The owner row's phase and charges from its seven columns at `first`:
/// `Integrity` for an unknown phase or a negative count.
pub(super) fn decoded(
    row: &postgres::Row,
    first: usize,
) -> Result<(Phase, AcquisitionWork), Failure> {
    let phase = u8::try_from(get::<i64>(row, first)?)
        .ok()
        .and_then(Phase::from_code)
        .ok_or(BackendError::Integrity)?;
    let mut counts = [0_u64; 6];
    for (index, count) in counts.iter_mut().enumerate() {
        *count = unsigned(get::<i64>(row, first + index + 1)?)?;
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

fn adjust(
    tx: &Transaction<'_>,
    sql: &str,
    owner: Owner,
    rows: i64,
    bytes: i64,
) -> Result<u64, Failure> {
    let (operation, epoch) = identity(owner)?;
    let updated = tx.typed(sql, &[&operation, &epoch, &rows, &bytes], 32)?;
    unsigned(get::<i64>(updated.first().ok_or(Failure::Stale)?, 0)?)
}

/// Adds signed row and byte deltas; returns the rows now held.
pub(super) fn charge(
    tx: &Transaction<'_>,
    owner: Owner,
    rows: i64,
    bytes: i64,
) -> Result<u64, Failure> {
    adjust(tx, CHARGE, owner, rows, bytes)
}

/// Moves removed rows and bytes out of the held charge; returns rows now held.
pub(super) fn credit(
    tx: &Transaction<'_>,
    owner: Owner,
    rows: i64,
    bytes: i64,
) -> Result<u64, Failure> {
    adjust(tx, CREDIT, owner, rows, bytes)
}
