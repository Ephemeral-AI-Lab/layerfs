//! The `project.remove_file` marker: one `path_removed` row per project and
//! path key, advanced in one write transaction with the project's epoch.

use super::position;
use super::rows::{cell, many, one, unsigned};
use super::transaction::Transaction;
use super::{layerstack, params};
use crate::backend::records::Record as Row;
use layerfs_history::error::{HistoryError, HistoryResult, Missing};
use layerfs_history::identity::LayerStackId;
use layerfs_history::path::path_key;
use layerfs_history::records::{
    PathRemovalKind, PathRemovalOutcome, PathRemovalRecord, PathRemovalRequest,
};

const MARKER: &str = "paths_marker";

const MARKERS: &str = "paths_markers";

const UPSERT: &str = "paths_upsert";

const ADVANCE_EPOCH: &str = "paths_advance_epoch";

/// One marker row decoded from the Store.
fn marker(row: &Row) -> HistoryResult<PathRemovalRecord> {
    Ok(PathRemovalRecord {
        path: cell(row, 0)?,
        frontier_serial: unsigned(row, 1)?,
        removed_ms: cell::<i64>(row, 2)?,
    })
}

/// Inserts or advances one marker, without the UpToDate decision.
pub(crate) fn mark(
    tx: &Transaction<'_>,
    stack: LayerStackId,
    path: &[u8],
    frontier_serial: u64,
    removed_ms: i64,
) -> HistoryResult<()> {
    if path.is_empty() {
        return Err(HistoryError::InvalidInput("removed path"));
    }
    let key = path_key(path);
    let frontier =
        i64::try_from(frontier_serial).map_err(|_| HistoryError::Capacity("path frontier"))?;
    tx.execute(
        UPSERT,
        params![stack.as_slice(), key.as_slice(), path, frontier, removed_ms],
    )?;
    Ok(())
}

/// Applies one `project.remove_file` marker, or reports it current.
pub(crate) fn remove_path(
    tx: &Transaction<'_>,
    request: &PathRemovalRequest,
) -> HistoryResult<PathRemovalOutcome> {
    if layerstack::layer_stack(tx, request.stack)?.is_none() {
        return Err(HistoryError::Missing(Missing::LayerStack));
    }
    let key = path_key(&request.path);
    let stored = one(
        tx,
        MARKER,
        [request.stack.as_slice(), key.as_slice()],
        marker,
    )?;
    let stack_high = position::stack_high_water(tx, request.stack)?;
    if let Some(stored) = &stored {
        if stored.path != request.path {
            return Err(HistoryError::Integrity("path marker key"));
        }
        if stack_high <= stored.frontier_serial {
            return Ok(PathRemovalOutcome {
                kind: PathRemovalKind::UpToDate,
                stack: request.stack,
                path: stored.path.clone(),
                frontier_serial: stored.frontier_serial,
                removed_ms: stored.removed_ms,
            });
        }
    }
    // The new frontier is the Store high-water, read in this transaction.
    let frontier = position::high_water(tx)?;
    mark(
        tx,
        request.stack,
        &request.path,
        frontier,
        request.removed_ms,
    )?;
    let advanced = tx.execute(ADVANCE_EPOCH, [request.stack.as_slice()])?;
    if advanced != 1 {
        return Err(HistoryError::Conflict("project epoch"));
    }
    Ok(PathRemovalOutcome {
        kind: PathRemovalKind::Removed,
        stack: request.stack,
        path: request.path.clone(),
        frontier_serial: frontier,
        removed_ms: request.removed_ms,
    })
}

/// Every marker of one Stack.
pub(crate) fn markers(
    tx: &Transaction<'_>,
    stack: LayerStackId,
) -> HistoryResult<Vec<PathRemovalRecord>> {
    many(tx, MARKERS, [stack.as_slice()], marker)
}
