//! Typed history record decoding from shared backend columns.
use super::{bindings::Parameters, transaction::Transaction};
use crate::backend::records::{FromValue, Record as Row};
use layerfs_content::ObjectId;
use layerfs_history::{
    error::{HistoryError, HistoryResult},
    identity::{BranchId, CommitId, HistoryName, LayerId, LayerStackId, StageToken, WorkspaceId},
    records::{BranchRecord, CommitRecord, LayerRecord, LayerStackRecord, StageRecord},
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

/// Reads one required Workspace incarnation column.
pub(crate) fn workspace_column(row: &Row, index: usize) -> HistoryResult<WorkspaceId> {
    let bytes: Vec<u8> = cell(row, index)?;
    WorkspaceId::from_slice(&bytes)
}

/// Reads one required stage token column.
pub(crate) fn token_column(row: &Row, index: usize) -> HistoryResult<StageToken> {
    let token: i64 = cell(row, index)?;
    if token <= 0 {
        return Err(HistoryError::Integrity("stage token"));
    }
    StageToken::new(token as u64)
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
    })
}

/// Decodes one Commit row.
pub(crate) fn commit_row(row: &Row) -> HistoryResult<CommitRecord> {
    Ok(CommitRecord {
        id: required(commit_column(row, 0)?, "Commit identity")?,
        stack: required(stack_column(row, 1)?, "LayerStack identity")?,
        root: object(row, 2)?,
        parent: commit_column(row, 3)?,
        base_layer: required(layer_column(row, 4)?, "Layer identity")?,
    })
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
    })
}

/// Decodes one stage row, in the catalog's column order.
pub(crate) fn stage_row(row: &Row) -> HistoryResult<StageRecord> {
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
