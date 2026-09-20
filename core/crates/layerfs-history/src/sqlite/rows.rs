//! Row codecs and shared statement helpers for the native provider.
//!
//! Every byte column is decoded through the crate's typed identities, so a
//! stored value with the wrong width or the wrong tag is an integrity failure
//! rather than a silently accepted record. No helper here builds SQL text from
//! caller data: every statement in this module tree is a literal, and every
//! caller value is a bound parameter.

use crate::error::{HistoryError, HistoryResult};
use crate::identity::{
    BranchId, CommitId, HistoryName, LayerId, LayerStackId, StageToken, WorkspaceId,
};
use crate::records::{BranchRecord, CommitRecord, LayerRecord, LayerStackRecord, StageRecord};
use layerfs_content::ObjectId;
use rusqlite::{DropBehavior, Error as SqlError, Row, Transaction, TransactionBehavior};
use std::sync::{MutexGuard, TryLockError};

use super::open::Provider;
use super::SqliteCatalog;

/// Maps one engine failure onto the product's typed failure.
///
/// A constraint violation is an integrity failure (deferred checks surface at
/// `COMMIT`, which is why this mapping is used there too). Contention is `Busy`.
/// A read-only engine is a continuity refusal. Anything else is an unknown
/// persistence outcome: it is reported as such and never repaired, replayed or
/// reinterpreted.
pub(crate) fn sql(error: SqlError) -> HistoryError {
    if let SqlError::FromSqlConversionFailure(_, _, boxed) = error {
        return match boxed.downcast::<HistoryError>() {
            Ok(history) => *history,
            Err(_) => HistoryError::Integrity("catalog row"),
        };
    }
    match &error {
        SqlError::SqliteFailure(failure, _) => match failure.code {
            rusqlite::ErrorCode::ConstraintViolation => {
                HistoryError::Integrity("catalog constraint")
            }
            rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked => {
                HistoryError::Busy
            }
            rusqlite::ErrorCode::ReadOnly => HistoryError::ContinuityUnavailable,
            rusqlite::ErrorCode::DiskFull | rusqlite::ErrorCode::TooBig => {
                HistoryError::Capacity("catalog storage")
            }
            rusqlite::ErrorCode::DatabaseCorrupt
            | rusqlite::ErrorCode::NotADatabase
            | rusqlite::ErrorCode::SchemaChanged => HistoryError::Integrity("catalog storage"),
            _ => HistoryError::UnknownOutcome,
        },
        SqlError::IntegralValueOutOfRange(_, _)
        | SqlError::InvalidColumnType(_, _, _)
        | SqlError::InvalidColumnIndex(_)
        | SqlError::InvalidColumnName(_)
        | SqlError::QueryReturnedNoRows
        | SqlError::ExecuteReturnedResults
        | SqlError::InvalidQuery => HistoryError::Integrity("catalog row"),
        _ => HistoryError::UnknownOutcome,
    }
}

/// Carries a typed decode failure out through the engine's row closure.
///
/// The closure `query_row` accepts can only return an engine error, so a typed
/// failure travels as a boxed cause and [`sql`] unwraps it. That keeps the typed
/// class intact instead of collapsing every decode problem into one code.
pub(crate) fn to_sql(error: HistoryError) -> SqlError {
    SqlError::FromSqlConversionFailure(0, rusqlite::types::Type::Blob, Box::new(error))
}

impl SqliteCatalog {
    /// Takes the provider cell without waiting, then runs one short transaction.
    ///
    /// The cell is taken with a non-blocking attempt, so contention is an
    /// immediate `Busy` refusal rather than a wait. A write requested from a
    /// handle that holds no authority is refused before any statement runs, and
    /// a typed refusal inside `body` rolls the transaction back and is returned
    /// unchanged.
    pub(crate) fn transact<T>(
        &self,
        writable: bool,
        body: impl FnOnce(&Transaction<'_>) -> HistoryResult<T>,
    ) -> HistoryResult<T> {
        let mut provider: MutexGuard<'_, Provider> = match self.state.try_lock() {
            Ok(guard) => guard,
            Err(TryLockError::WouldBlock) => return Err(HistoryError::Busy),
            Err(TryLockError::Poisoned(_)) => return Err(HistoryError::UnknownOutcome),
        };
        if provider.quarantined {
            return Err(HistoryError::UnknownOutcome);
        }
        if writable && !provider.writable {
            return Err(HistoryError::ContinuityUnavailable);
        }
        let behavior = if writable {
            TransactionBehavior::Immediate
        } else {
            TransactionBehavior::Deferred
        };
        let result = (|| {
            let mut transaction = provider
                .connection
                .transaction_with_behavior(behavior)
                .map_err(sql)?;
            // Never let RAII guess rollback after an unknown body/COMMIT outcome.
            transaction.set_drop_behavior(DropBehavior::Ignore);
            let result = body(&transaction).and_then(|value| {
                if writable {
                    transaction.execute_batch("COMMIT").map_err(sql)?;
                }
                Ok(value)
            });
            match result {
                Err(error) if error.unknown() => Err(error),
                result => {
                    if !transaction.is_autocommit() && transaction.rollback().is_err() {
                        return Err(HistoryError::UnknownOutcome);
                    }
                    result
                }
            }
        })();
        if result.as_ref().is_err_and(|error| error.unknown()) {
            // Retain one bounded connection until this catalog is dropped. It
            // cannot publish more work or expose pending rows as committed.
            provider.quarantined = true;
        }
        result
    }

    /// Runs one coherent read-only metadata transaction.
    pub(crate) fn read<T>(
        &self,
        body: impl FnOnce(&Transaction<'_>) -> HistoryResult<T>,
    ) -> HistoryResult<T> {
        self.transact(false, body)
    }

    /// Runs one metadata transaction that may publish a transition.
    pub(crate) fn write<T>(
        &self,
        body: impl FnOnce(&Transaction<'_>) -> HistoryResult<T>,
    ) -> HistoryResult<T> {
        self.transact(true, body)
    }
}

/// Reads one column through the engine's own conversion, typed on failure.
pub(crate) fn cell<T: rusqlite::types::FromSql>(row: &Row<'_>, index: usize) -> HistoryResult<T> {
    row.get(index).map_err(sql)
}

/// Reads one required 32-byte root identity column.
pub(crate) fn object(row: &Row<'_>, index: usize) -> HistoryResult<ObjectId> {
    let bytes: Vec<u8> = cell(row, index)?;
    ObjectId::from_bytes(&bytes).map_err(|_| HistoryError::Integrity("root identity"))
}

/// Reads one required tagged identity column.
macro_rules! identity_column {
    ($name:ident, $type:ident) => {
        /// Reads one optional tagged identity column.
        pub(crate) fn $name(row: &Row<'_>, index: usize) -> HistoryResult<Option<$type>> {
            let bytes: Option<Vec<u8>> = cell(row, index)?;
            match bytes {
                None => Ok(None),
                Some(bytes) => Ok(Some($type::from_slice(&bytes)?)),
            }
        }
    };
}

identity_column!(branch_column, BranchId);
identity_column!(commit_column, CommitId);
identity_column!(layer_column, LayerId);
identity_column!(stack_column, LayerStackId);

pub(crate) fn required<T>(value: Option<T>, what: &'static str) -> HistoryResult<T> {
    value.ok_or(HistoryError::Integrity(what))
}

/// Reads one required Workspace incarnation column.
pub(crate) fn workspace_column(row: &Row<'_>, index: usize) -> HistoryResult<WorkspaceId> {
    let bytes: Vec<u8> = cell(row, index)?;
    WorkspaceId::from_slice(&bytes)
}

/// Reads one required stage token column.
pub(crate) fn token_column(row: &Row<'_>, index: usize) -> HistoryResult<StageToken> {
    let token: i64 = cell(row, index)?;
    if token <= 0 {
        return Err(HistoryError::Integrity("stage token"));
    }
    StageToken::new(token as u64)
}

/// Reads one required history name column.
pub(crate) fn name_column(row: &Row<'_>, index: usize) -> HistoryResult<HistoryName> {
    let name: String = cell(row, index)?;
    HistoryName::new(&name).map_err(|_| HistoryError::Integrity("history name"))
}

/// Decodes one stack row: identity, name, scope, profile, head Layer.
pub(crate) fn stack_row(row: &Row<'_>) -> HistoryResult<LayerStackRecord> {
    Ok(LayerStackRecord {
        id: required(stack_column(row, 0)?, "LayerStack identity")?,
        name: name_column(row, 1)?,
        scope: object(row, 2)?,
        profile: object(row, 3)?,
        head_layer: required(layer_column(row, 4)?, "Layer identity")?,
    })
}

/// Decodes one Branch row.
pub(crate) fn branch_row(row: &Row<'_>) -> HistoryResult<BranchRecord> {
    Ok(BranchRecord {
        id: required(branch_column(row, 0)?, "Branch identity")?,
        stack: required(stack_column(row, 1)?, "LayerStack identity")?,
        name: name_column(row, 2)?,
        base_layer: required(layer_column(row, 3)?, "Layer identity")?,
        head_commit: commit_column(row, 4)?,
    })
}

/// Decodes one Commit row.
pub(crate) fn commit_row(row: &Row<'_>) -> HistoryResult<CommitRecord> {
    Ok(CommitRecord {
        id: required(commit_column(row, 0)?, "Commit identity")?,
        stack: required(stack_column(row, 1)?, "LayerStack identity")?,
        root: object(row, 2)?,
        parent: commit_column(row, 3)?,
        base_layer: required(layer_column(row, 4)?, "Layer identity")?,
    })
}

/// Decodes one Layer row.
pub(crate) fn layer_row(row: &Row<'_>) -> HistoryResult<LayerRecord> {
    Ok(LayerRecord {
        id: required(layer_column(row, 0)?, "Layer identity")?,
        stack: required(stack_column(row, 1)?, "LayerStack identity")?,
        parent: layer_column(row, 2)?,
        root: object(row, 3)?,
        source_branch: branch_column(row, 4)?,
        source_commit: commit_column(row, 5)?,
    })
}

/// Decodes one stage row, in the catalog's column order.
pub(crate) fn stage_row(row: &Row<'_>) -> HistoryResult<StageRecord> {
    let generation: i64 = cell(row, 12)?;
    Ok(StageRecord {
        workspace: workspace_column(row, 0)?,
        token: token_column(row, 1)?,
        stack: required(stack_column(row, 2)?, "LayerStack identity")?,
        branch: required(branch_column(row, 3)?, "Branch identity")?,
        expected_head: commit_column(row, 4)?,
        expected_base: required(layer_column(row, 5)?, "Layer identity")?,
        expected_root: object(row, 6)?,
        construction_base_root: object(row, 7)?,
        intended_commit_base: required(layer_column(row, 8)?, "Layer identity")?,
        candidate_root: object(row, 9)?,
        profile: object(row, 10)?,
        scope: object(row, 11)?,
        generation: u64::try_from(generation)
            .map_err(|_| HistoryError::Integrity("stage generation"))?,
    })
}

/// Runs one statement expecting at most one row and decodes it.
pub(crate) fn one<T>(
    tx: &Transaction<'_>,
    statement: &str,
    parameters: impl rusqlite::Params,
    decode: impl FnOnce(&Row<'_>) -> HistoryResult<T>,
) -> HistoryResult<Option<T>> {
    use rusqlite::OptionalExtension;
    tx.query_row(statement, parameters, |row| decode(row).map_err(to_sql))
        .optional()
        .map_err(sql)
}

/// Runs one statement and decodes every row in the order the engine returns.
pub(crate) fn many<T>(
    tx: &Transaction<'_>,
    statement: &str,
    parameters: impl rusqlite::Params,
    mut decode: impl FnMut(&Row<'_>) -> HistoryResult<T>,
) -> HistoryResult<Vec<T>> {
    let mut prepared = tx.prepare(statement).map_err(sql)?;
    let rows = prepared
        .query_map(parameters, |row| decode(row).map_err(to_sql))
        .map_err(sql)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(sql)
}

/// Reads one required integer column as a checked unsigned value.
pub(crate) fn unsigned(row: &Row<'_>, index: usize) -> HistoryResult<u64> {
    let value: i64 = cell(row, index)?;
    u64::try_from(value).map_err(|_| HistoryError::Integrity("catalog counter"))
}
