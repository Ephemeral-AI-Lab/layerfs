//! Working transfer progress persistence.

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::legacy_store::{
    begin_product_transaction, commit_product_state, rollback_product_transaction, Engine,
};
use crate::full::record_id::RequestId;
use crate::full::transfer::batch::{StoredTransferState, SyncTransferCounters};
use rusqlite::{params, OptionalExtension};

impl Engine {
    #[allow(clippy::too_many_arguments)]
    pub fn product_record_transfer_state(
        &self,
        owner_request_id: RequestId,
        request_id: RequestId,
        batch_sequence: u64,
        direction: &str,
        cursor: &[u8],
        complete: bool,
        counters: SyncTransferCounters,
    ) -> EngineResult<bool> {
        if !matches!(direction, "fetch" | "push") || !(40..=41_008).contains(&cursor.len()) {
            return Err(EngineError::InvalidRecord("transfer state"));
        }
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            let sequence =
                i64::try_from(batch_sequence).map_err(|_| EngineError::CounterOverflow)?;
            let incumbent = connection
                .query_row(
                    "SELECT owner_request_id, direction FROM layerfs_transfer_state
                     WHERE request_id = ?1 AND batch_sequence = ?2",
                    params![request_id.as_bytes(), sequence],
                    |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()
                .map_err(map_sqlite_error)?;
            if incumbent.as_ref().is_some_and(|value| {
                value.0.as_slice() != owner_request_id.as_bytes() || value.1 != direction
            }) {
                return Err(EngineError::InvalidRecord("transfer request direction"));
            }
            connection
                .execute(
                    "INSERT INTO layerfs_transfer_state
                     (owner_request_id, request_id, batch_sequence, direction, cursor, state,
                      unique_bytes, resumed_bytes, retransmitted_bytes)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                     ON CONFLICT(request_id, batch_sequence) DO UPDATE SET
                       cursor = excluded.cursor,
                       state = excluded.state,
                       unique_bytes = excluded.unique_bytes,
                       resumed_bytes = excluded.resumed_bytes,
                       retransmitted_bytes = excluded.retransmitted_bytes",
                    params![
                        owner_request_id.as_bytes(),
                        request_id.as_bytes(),
                        sequence,
                        direction,
                        cursor,
                        if complete { "complete" } else { "transferring" },
                        i64::try_from(counters.unique_bytes)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        i64::try_from(counters.resumed_bytes)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        i64::try_from(counters.retransmitted_bytes)
                            .map_err(|_| EngineError::CounterOverflow)?,
                    ],
                )
                .map_err(map_sqlite_error)?;
            let reconciliation = format!(
                "SELECT EXISTS(SELECT 1 FROM layerfs_transfer_state \
                 WHERE request_id = ?1 AND batch_sequence = {sequence})"
            );
            commit_product_state(
                self,
                &mut connection,
                &reconciliation,
                request_id.as_bytes(),
            )
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_latest_transfer_state(
        &self,
        request_id: RequestId,
        direction: &str,
    ) -> EngineResult<Option<StoredTransferState>> {
        if !matches!(direction, "fetch" | "push") {
            return Err(EngineError::InvalidRecord("transfer direction"));
        }
        let connection = self.lock_connection()?;
        connection
            .query_row(
                "SELECT batch_sequence, cursor, state, unique_bytes, resumed_bytes,
                        retransmitted_bytes
                 FROM layerfs_transfer_state
                 WHERE request_id = ?1 AND direction = ?2
                 ORDER BY batch_sequence DESC LIMIT 1",
                params![request_id.as_bytes(), direction],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                },
            )
            .optional()
            .map_err(map_sqlite_error)?
            .map(
                |(batch_sequence, cursor, state, unique, resumed, retransmitted)| {
                    Ok(StoredTransferState {
                        batch_sequence: u64::try_from(batch_sequence)
                            .map_err(|_| EngineError::InvalidRecord("transfer sequence"))?,
                        cursor,
                        complete: state == "complete",
                        counters: SyncTransferCounters {
                            unique_bytes: u64::try_from(unique)
                                .map_err(|_| EngineError::InvalidRecord("transfer bytes"))?,
                            resumed_bytes: u64::try_from(resumed)
                                .map_err(|_| EngineError::InvalidRecord("transfer bytes"))?,
                            retransmitted_bytes: u64::try_from(retransmitted)
                                .map_err(|_| EngineError::InvalidRecord("transfer bytes"))?,
                        },
                    })
                },
            )
            .transpose()
    }

    pub fn product_clear_transfer_state(
        &self,
        request_id: RequestId,
        direction: &str,
    ) -> EngineResult<bool> {
        if !matches!(direction, "fetch" | "push") {
            return Err(EngineError::InvalidRecord("transfer direction"));
        }
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            connection
                .execute(
                    "DELETE FROM layerfs_transfer_state
                     WHERE request_id = ?1 AND direction = ?2",
                    params![request_id.as_bytes(), direction],
                )
                .map_err(map_sqlite_error)?;
            commit_product_state(
                self,
                &mut connection,
                "SELECT NOT EXISTS(SELECT 1 FROM layerfs_transfer_state
                 WHERE request_id = ?1)",
                request_id.as_bytes(),
            )
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }

    pub fn product_clear_transfer_state_owner(
        &self,
        owner_request_id: RequestId,
        direction: &str,
    ) -> EngineResult<bool> {
        if !matches!(direction, "fetch" | "push") {
            return Err(EngineError::InvalidRecord("transfer direction"));
        }
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            connection
                .execute(
                    "DELETE FROM layerfs_transfer_state
                     WHERE owner_request_id = ?1 AND direction = ?2",
                    params![owner_request_id.as_bytes(), direction],
                )
                .map_err(map_sqlite_error)?;
            commit_product_state(
                self,
                &mut connection,
                "SELECT NOT EXISTS(SELECT 1 FROM layerfs_transfer_state
                 WHERE owner_request_id = ?1)",
                owner_request_id.as_bytes(),
            )
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }
}
