//! Typed history record decoding from shared backend columns.
use super::{bindings::Parameters, transaction::Transaction};
use crate::backend::records::{FromValue, Record as Row};
use layerfs_content::ObjectId;
use layerfs_history::{
    error::{HistoryError, HistoryResult},
    identity::{BranchId, CommitId, HistoryName, LayerId, LayerStackId, NodeId},
    records::{BranchRecord, CommitMeta, CommitRecord, CommitVia, LayerRecord, LayerStackRecord},
};
/// Reads one column through the engine's own conversion, typed on failure.
pub(crate) fn cell<T: FromValue>(row: &Row, index: usize) -> HistoryResult<T> {
    row.get(index)
        .map_err(|_| HistoryError::Integrity("catalog row"))
}

/// Reads one required 32-byte root identity column.
pub(crate) fn object(row: &Row, index: usize) -> HistoryResult<ObjectId> {
    let bytes: Vec<u8> = cell(row, index)?;
    ObjectId::from_bytes(&bytes).map_err(|_| HistoryError::Integrity("root identity"))
}

/// Reads one required tagged identity column.
macro_rules! identity_column {
    ($name:ident, $type:ident) => {
        /// Reads one optional tagged identity column.
        pub(crate) fn $name(row: &Row, index: usize) -> HistoryResult<Option<$type>> {
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

/// Reads one required history name column.
pub(crate) fn name_column(row: &Row, index: usize) -> HistoryResult<HistoryName> {
    let name: String = cell(row, index)?;
    HistoryName::new(&name).map_err(|_| HistoryError::Integrity("history name"))
}

/// Decodes one stack row: identity, name, scope, profile, head Layer.
pub(crate) fn stack_row(row: &Row) -> HistoryResult<LayerStackRecord> {
    Ok(LayerStackRecord {
        id: required(stack_column(row, 0)?, "LayerStack identity")?,
        name: name_column(row, 1)?,
        scope: object(row, 2)?,
        profile: object(row, 3)?,
        head_layer: required(layer_column(row, 4)?, "Layer identity")?,
    })
}

/// Decodes one Branch row.
pub(crate) fn branch_row(row: &Row) -> HistoryResult<BranchRecord> {
    Ok(BranchRecord {
        id: required(branch_column(row, 0)?, "Branch identity")?,
        stack: required(stack_column(row, 1)?, "LayerStack identity")?,
        name: name_column(row, 2)?,
        base_layer: required(layer_column(row, 3)?, "Layer identity")?,
        head_commit: commit_column(row, 4)?,
        forked_from: cell::<Option<Vec<u8>>>(row, 5)?
            .map(|bytes| NodeId::from_slice(&bytes))
            .transpose()?,
    })
}

/// Decodes one Commit row.
pub(crate) fn commit_row(row: &Row) -> HistoryResult<CommitRecord> {
    let (lww, units) = super::facts::decode(&cell::<Vec<u8>>(row, 15)?)?;
    let record = CommitRecord {
        id: required(commit_column(row, 0)?, "Commit identity")?,
        stack: required(stack_column(row, 1)?, "LayerStack identity")?,
        root: object(row, 2)?,
        parent: commit_column(row, 3)?,
        base_layer: required(layer_column(row, 4)?, "Layer identity")?,
        second_parent: cell::<Option<Vec<u8>>>(row, 5)?
            .map(|bytes| NodeId::from_slice(&bytes))
            .transpose()?,
        meta: CommitMeta {
            branch: branch_column(row, 6)?,
            via: CommitVia::from_code(
                u8::try_from(cell::<i64>(row, 7)?)
                    .map_err(|_| HistoryError::Integrity("Commit via"))?,
            )?,
            message: cell(row, 8)?,
            merged_from: cell::<Option<Vec<u8>>>(row, 9)?
                .map(|bytes| NodeId::from_slice(&bytes))
                .transpose()?,
            time_ms: cell(row, 10)?,
            statistics: layerfs_history::CommitStatistics {
                logical: cell::<i64>(row, 11)? as u64,
                new_canonical: cell::<i64>(row, 12)? as u64,
                new_stored: cell::<i64>(row, 13)? as u64,
                new_metadata: cell::<i64>(row, 14)? as u64,
            },
            lww,
            lww_truncated: false,
            units,
            displaced: commit_column(row, 16)?,
        },
    };
    record.meta.check()?;
    if CommitId::derive(
        record.root,
        record.parent,
        record.base_layer,
        record.second_parent,
        &record.meta,
    ) != record.id
    {
        return Err(HistoryError::Integrity("Commit v3 identity"));
    }
    Ok(record)
}

/// Decodes one Layer row.
pub(crate) fn layer_row(row: &Row) -> HistoryResult<LayerRecord> {
    Ok(LayerRecord {
        id: required(layer_column(row, 0)?, "Layer identity")?,
        stack: required(stack_column(row, 1)?, "LayerStack identity")?,
        parent: layer_column(row, 2)?,
        root: object(row, 3)?,
        source_branch: branch_column(row, 4)?,
        source_commit: commit_column(row, 5)?,
        logical: cell::<i64>(row, 6)? as u64,
        overwrote_count: unsigned(row, 7)?,
    })
}

/// Runs one statement expecting at most one row and decodes it.
pub(crate) fn one<T>(
    tx: &Transaction<'_>,
    statement: &str,
    parameters: impl Parameters,
    decode: impl FnOnce(&Row) -> HistoryResult<T>,
) -> HistoryResult<Option<T>> {
    let rows = tx.query(statement, parameters)?;
    if rows.len() > 1 {
        return Err(HistoryError::Integrity("catalog row count"));
    }
    rows.first().map(decode).transpose()
}
/// Decodes every row in the statement's order.
pub(crate) fn many<T>(
    tx: &Transaction<'_>,
    statement: &str,
    parameters: impl Parameters,
    decode: impl FnMut(&Row) -> HistoryResult<T>,
) -> HistoryResult<Vec<T>> {
    tx.query(statement, parameters)?
        .iter()
        .map(decode)
        .collect()
}
/// Reads one required integer column as a checked unsigned value.
pub(crate) fn unsigned(row: &Row, index: usize) -> HistoryResult<u64> {
    let value: i64 = cell(row, index)?;
    u64::try_from(value).map_err(|_| HistoryError::Integrity("catalog counter"))
}

/// Preserves domain errors from the transaction facade.
pub(crate) fn sql(error: HistoryError) -> HistoryError {
    error
}
