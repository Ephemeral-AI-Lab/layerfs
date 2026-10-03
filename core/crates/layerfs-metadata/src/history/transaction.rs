//! One coherent transaction per history operation; immediate catalog contention.
use super::{bindings::Parameters, open::PgHistory};
use crate::{client::Client, params::Param};
use layerfs_history::{HistoryError, HistoryResult};
use layerfs_storage::port::MetadataError;
use postgres::Row;
use std::sync::TryLockError;
pub(crate) struct Transaction<'a> {
    client: &'a Client,
    writable: bool,
}
impl Transaction<'_> {
    pub(crate) fn query(&self, sql: &str, parameters: impl Parameters) -> HistoryResult<Vec<Row>> {
        self.client
            .query(sql, parameters.into_params(), self.writable)
            .map_err(error)
    }
    pub(crate) fn execute(&self, sql: &str, parameters: impl Parameters) -> HistoryResult<usize> {
        self.query(sql, parameters).map(|rows| rows.len())
    }
}
/// Driver statuses retain definite refusal, contention and uncertainty classes.
pub(crate) fn error(error: MetadataError) -> HistoryError {
    match error {
        MetadataError::Uncertain => HistoryError::UnknownOutcome,
        MetadataError::Missing | MetadataError::Malformed => {
            HistoryError::Integrity("catalog storage")
        }
        MetadataError::Refused { status } => match status.as_str() {
            "55P03" | "40001" | "40P01" => HistoryError::Busy,
            "25006" => HistoryError::ContinuityUnavailable,
            "54000" | "53100" | "53200" => HistoryError::Capacity("catalog storage"),
            "42P01" | "42703" => HistoryError::Integrity("catalog table set"),
            _ => HistoryError::Integrity("catalog constraint"),
        },
    }
}
impl PgHistory {
    fn transact<T>(
        &self,
        writable: bool,
        body: impl FnOnce(&Transaction<'_>) -> HistoryResult<T>,
    ) -> HistoryResult<T> {
        let mut state = match self.state.try_lock() {
            Ok(state) => state,
            Err(TryLockError::WouldBlock) => return Err(HistoryError::Busy),
            Err(TryLockError::Poisoned(_)) => return Err(HistoryError::UnknownOutcome),
        };
        if state.quarantined {
            return Err(HistoryError::UnknownOutcome);
        }
        if writable && !self.writable {
            return Err(HistoryError::ContinuityUnavailable);
        }
        let tx = Transaction {
            client: &self.client,
            writable,
        };
        let begin = if writable {
            include_str!("../../sql/queries/history/begin_write.sql")
        } else {
            include_str!("../../sql/queries/history/begin_read.sql")
        };
        let result = (|| {
            tx.query(begin, Vec::<Param>::new())?;
            // Serialize only C5 authorities in this schema. NOWAIT never retries.
            if writable {
                tx.query(
                    include_str!("../../sql/queries/history/lock.sql"),
                    Vec::<Param>::new(),
                )?;
            }
            let value = body(&tx)?;
            tx.query(
                include_str!("../../sql/queries/history/commit.sql"),
                Vec::<Param>::new(),
            )?;
            Ok(value)
        })();
        if result.as_ref().is_err_and(HistoryError::unknown) {
            state.quarantined = true;
            return result;
        }
        if result.is_err()
            && tx
                .query(
                    include_str!("../../sql/queries/history/rollback.sql"),
                    Vec::<Param>::new(),
                )
                .is_err()
        {
            state.quarantined = true;
            return Err(HistoryError::UnknownOutcome);
        }
        result
    }
    pub(crate) fn read<T>(
        &self,
        body: impl FnOnce(&Transaction<'_>) -> HistoryResult<T>,
    ) -> HistoryResult<T> {
        self.transact(false, body)
    }
    pub(crate) fn write<T>(
        &self,
        body: impl FnOnce(&Transaction<'_>) -> HistoryResult<T>,
    ) -> HistoryResult<T> {
        self.transact(true, body)
    }
}
