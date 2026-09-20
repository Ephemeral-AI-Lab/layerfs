//! Scope-wide inode allocation with a checked high-water bound.
//!
//! One row per scope holds how many serials that scope has consumed. A
//! reservation is a half-open range starting after the high-water mark, and the
//! mark advances in the same transaction that returns the range, so a serial is
//! never handed out twice. Consumption is unconditional: a reservation that is
//! never used, a stage that is discarded and an operation that fails afterwards
//! all leave the mark where they found it plus their own count. There is no
//! recycling and no way to recover unused numbers by scanning roots.
//!
//! The terminal value of the high-water column is a real endpoint, not an
//! overflow: a request whose range would pass `i64::MAX` is a checked `Capacity`
//! refusal, and the columns stay inside the engine's signed 64-bit integers.

use crate::error::{HistoryError, HistoryResult};
use crate::identity::CatalogId;
use crate::records::{Reservation, ReserveRequest, MAXIMUM_INODE_RESERVATION};
use rusqlite::Transaction;

use super::rows::{cell, one, sql, unsigned};

const SCOPE_BY_ID: &str = "SELECT highwater, authority_id FROM scope_allocator \
     WHERE scope_id = ?1";

const INSERT_SCOPE: &str = "INSERT INTO scope_allocator (scope_id, highwater, authority_id) \
     VALUES (?1, 0, ?2)";

const ADVANCE_SCOPE: &str = "UPDATE scope_allocator SET highwater = ?1 \
     WHERE scope_id = ?2 AND highwater = ?3";

/// Consumes one checked half-open inode range for a scope.
pub(crate) fn reserve_inodes(
    tx: &Transaction<'_>,
    catalog: CatalogId,
    request: &ReserveRequest,
) -> HistoryResult<Reservation> {
    if request.count == 0 || request.count > MAXIMUM_INODE_RESERVATION {
        return Err(HistoryError::InvalidInput("inode reservation"));
    }
    let existing = one(
        tx,
        SCOPE_BY_ID,
        [request.scope.as_bytes().as_slice()],
        |row| Ok((unsigned(row, 0)?, cell::<Vec<u8>>(row, 1)?)),
    )?;
    let highwater: u64 = match existing {
        None => {
            tx.execute(
                INSERT_SCOPE,
                rusqlite::params![request.scope.as_bytes().as_slice(), catalog.as_slice()],
            )
            .map_err(sql)?;
            0
        }
        Some((highwater, authority)) => {
            if authority.as_slice() != catalog.as_slice() {
                return Err(HistoryError::Integrity("scope authority"));
            }
            highwater
        }
    };
    let end = highwater
        .checked_add(request.count)
        .filter(|end| *end <= i64::MAX as u64)
        .ok_or(HistoryError::Capacity("inode serials"))?;
    let advanced = tx
        .execute(
            ADVANCE_SCOPE,
            rusqlite::params![
                i64::try_from(end).map_err(|_| HistoryError::Capacity("inode serials"))?,
                request.scope.as_bytes().as_slice(),
                i64::try_from(highwater)
                    .map_err(|_| HistoryError::Integrity("scope high-water"))?
            ],
        )
        .map_err(sql)?;
    if advanced != 1 {
        return Err(HistoryError::Integrity("scope high-water"));
    }
    Ok(Reservation {
        scope: request.scope,
        start: highwater + 1,
        count: request.count,
    })
}
