//! The `project.remove_file` marker: one `path_removed` row per project and
//! path key, advanced in one write transaction with the project's epoch.

use super::position;
use super::rows::{cell, many, one, unsigned};
use super::transaction::Transaction;
use super::{layerstack, params};
use crate::backend::records::{Record as Row, Value};
use layerfs_history::error::{HistoryError, HistoryResult, Missing};
use layerfs_history::identity::{LayerStackId, NodeId};
use layerfs_history::path::trace_key;
use layerfs_history::records::{
    PathRemovalKind, PathRemovalOutcome, PathRemovalRecord, PathRemovalRequest,
};

const MARKER: &str = "paths_marker";

const MARKERS: &str = "paths_markers";

const NODE_MARKERS: &str = "paths_node_markers";

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

/// Inserts or advances one marker, without the UpToDate decision. The key is
/// the Store-salted trace hash of the stack and path (§4.3), never a
/// Store-independent hash of the path.
pub(crate) fn mark(
    tx: &Transaction<'_>,
    stack: LayerStackId,
    path: &[u8],
    trace_salt: &[u8; 32],
    frontier_serial: u64,
    removed_ms: i64,
) -> HistoryResult<()> {
    if path.is_empty() {
        return Err(HistoryError::InvalidInput("removed path"));
    }
    let key = trace_key(trace_salt, stack, path);
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
    trace_salt: &[u8; 32],
) -> HistoryResult<PathRemovalOutcome> {
    if layerstack::layer_stack(tx, request.stack)?.is_none() {
        return Err(HistoryError::Missing(Missing::LayerStack));
    }
    let key = trace_key(trace_salt, request.stack, &request.path);
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
    // Ordering audit (spec 5): SQLite's `BEGIN IMMEDIATE` already excludes
    // every other writer from the whole marker transaction, so reading the
    // frontier before the epoch bump cannot miss a Commit or Layer that
    // commits in between; PostgreSQL uses `history::admission::remove_path`,
    // which bumps the epoch before this read.
    let frontier = position::high_water(tx)?;
    mark(
        tx,
        request.stack,
        &request.path,
        trace_salt,
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

/// The `history_serial` of `node` with every marker of `stack`, in one
/// statement: one row per marker, or one row without a marker, each
/// carrying the serial; no row when the node does not exist.
pub(crate) fn node_markers(
    tx: &Transaction<'_>,
    stack: LayerStackId,
    node: NodeId,
) -> HistoryResult<Option<(u64, Vec<PathRemovalRecord>)>> {
    let rows = tx.query(NODE_MARKERS, [node.as_slice(), stack.as_slice()])?;
    let Some(first) = rows.first() else {
        return Ok(None);
    };
    let serial = unsigned(first, 0)?;
    let mut markers = Vec::with_capacity(rows.len());
    for row in &rows {
        if unsigned(row, 0)? != serial {
            return Err(HistoryError::Integrity("node position"));
        }
        if matches!(row.cells.get(1), Some(Value::Null)) {
            if rows.len() != 1 {
                return Err(HistoryError::Integrity("path marker"));
            }
            continue;
        }
        markers.push(PathRemovalRecord {
            path: cell(row, 1)?,
            frontier_serial: unsigned(row, 2)?,
            removed_ms: cell::<i64>(row, 3)?,
        });
    }
    Ok(Some((serial, markers)))
}
