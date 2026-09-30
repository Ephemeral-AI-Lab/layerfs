//! Exact ordered claim seal and bounded acknowledged retirement transactions.

use layerfs_content::filesystem::state::{ClaimKey, ClaimLedger, ClaimSeal};
use rusqlite::Connection;

use crate::error::{StorageError, StorageResult};

use super::claim_index;
use super::phased::{Attempt, AttemptKind, ClaimPhase, Phased};
use super::profile;

pub(crate) fn seal(
    connection: &Connection,
    state: &mut Phased,
) -> StorageResult<(ClaimSeal, Option<ClaimKey>)> {
    crate::sqlite::write::begin_immediate(connection)?;
    let result = (|| {
        claim_index::verify(connection, state)?;
        let maximum = claim_index::maximum(connection, state)?;
        if maximum.is_none() != (state.records == 0) {
            return Err(StorageError::Integrity(
                "construction scratch claim terminal count",
            ));
        }
        let mut ledger = ClaimLedger::new(state.claims.clone())?;
        while ledger.records() < state.records {
            let count = (state.records - ledger.records()).min(128) as usize;
            let rows = claim_index::read(connection, state, ledger.last(), count)?;
            if rows.len() != count {
                return Err(StorageError::Integrity(
                    "construction scratch claim seal cardinality",
                ));
            }
            ledger.acknowledge(&rows)?;
        }
        if ledger.last() != maximum || ledger.encoded_bytes() != state.records * 32 {
            return Err(StorageError::Integrity(
                "construction scratch claim seal EOF",
            ));
        }
        let seal = ledger.seal();
        state.proposed_seal = Some((seal.clone(), maximum));
        if connection.execute("UPDATE session_owner SET claim_state=1,claim_remaining=?1,claim_digest=?2,claim_max=?3 WHERE id=1 AND claim_state=0 AND claim_records=?1 AND claim_scope=?4",
            rusqlite::params![state.records as i64, seal.digest().as_slice(), maximum.as_ref().map(|key| key.as_bytes().as_slice()), state.claims.as_bytes().as_slice()])? != 1 {
            return Err(StorageError::Integrity("construction scratch claim seal acknowledgement"));
        }
        Ok((seal, maximum))
    })();
    profile::finish_transaction(connection, result)
}

pub(crate) struct Retirement {
    pub(crate) remaining: u64,
    pub(crate) after: Option<ClaimKey>,
    pub(crate) phase: ClaimPhase,
}

pub(crate) fn retire_window(
    connection: &Connection,
    state: &mut Phased,
) -> StorageResult<Retirement> {
    crate::sqlite::write::begin_immediate(connection)?;
    let result = (|| {
        claim_index::verify(connection, state)?;
        let count = state.remaining.min(128) as usize;
        let rows = claim_index::read(connection, state, state.after, count)?;
        if rows.len() != count {
            return Err(StorageError::Integrity(
                "construction scratch retirement cardinality",
            ));
        }
        let remaining = state.remaining - count as u64;
        let after = rows.last().map(|record| record.key()).or(state.after);
        if after.is_some_and(|key| state.maximum.is_none_or(|maximum| key > maximum))
            || (remaining == 0 && after != state.maximum)
        {
            return Err(StorageError::Integrity(
                "construction scratch retirement terminal key",
            ));
        }
        let mut serials = [0; 128];
        for (slot, row) in serials.iter_mut().zip(&rows) {
            *slot = row.key().serial();
        }
        state.attempt = Some(Attempt {
            kind: AttemptKind::Retire,
            before: state.remaining,
            proposed: remaining,
            serials,
            count,
        });
        let mut statement =
            connection.prepare("DELETE FROM exclusive_claims WHERE key=?1 AND class=x'01'")?;
        for row in &rows {
            if statement.execute([row.key().as_bytes().as_slice()])? != 1 {
                return Err(StorageError::Integrity(
                    "construction scratch retirement affected rows",
                ));
            }
        }
        drop(statement);
        if remaining == 0 && !claim_index::empty(connection)? {
            return Err(StorageError::Integrity(
                "construction scratch retirement EOF",
            ));
        }
        let phase = if remaining == 0 {
            ClaimPhase::Retired
        } else {
            ClaimPhase::Retiring
        };
        if connection.execute("UPDATE session_owner SET claim_state=?1,claim_remaining=?2,claim_after=?3 WHERE id=1 AND claim_state=?4 AND claim_remaining=?5 AND claim_scope=?6 AND claim_digest=?7",
            rusqlite::params![phase as u8, remaining as i64, after.as_ref().map(|key| key.as_bytes().as_slice()), state.phase as u8, state.remaining as i64, state.claims.as_bytes().as_slice(), state.seal.as_ref().unwrap().digest().as_slice()])? != 1 {
            return Err(StorageError::Integrity("construction scratch retirement acknowledgement"));
        }
        Ok(Retirement {
            remaining,
            after,
            phase,
        })
    })();
    profile::finish_transaction(connection, result)
}
