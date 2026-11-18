//! Nodes of one stack by a range of their tagged identity: how a caller
//! resolves a prefix of a Commit or Layer identity in one bounded read of
//! the stack's identity index, without paging history.

use super::params;
use super::rows::{cell, one, unsigned};
use super::transaction::Transaction;
use layerfs_history::error::{HistoryError, HistoryResult};
use layerfs_history::identity::{LayerStackId, NodeId, COMMIT_TAG, LAYER_TAG};

const COMMITS: &str = "commit_identity_range";

const LAYERS: &str = "layerstack_identity_range";

/// The first node of `stack` whose identity lies in `low..=high`, in
/// identity order, and how many nodes lie there. Both bounds carry the tag
/// of one kind: Commits and Layers are read from their own tables.
pub(crate) fn nodes_between(
    tx: &Transaction<'_>,
    stack: LayerStackId,
    low: &[u8; 33],
    high: &[u8; 33],
) -> HistoryResult<(Option<NodeId>, u64)> {
    let statement = match (low[0], high[0]) {
        (COMMIT_TAG, COMMIT_TAG) => COMMITS,
        (LAYER_TAG, LAYER_TAG) => LAYERS,
        _ => return Err(HistoryError::InvalidInput("history node identity range")),
    };
    let found = one(
        tx,
        statement,
        params![stack.as_slice(), low.as_slice(), high.as_slice()],
        |row| {
            let id: Vec<u8> = cell(row, 0)?;
            Ok((NodeId::from_slice(&id)?, unsigned(row, 1)?))
        },
    )?;
    Ok(match found {
        Some((first, count)) => (Some(first), count),
        None => (None, 0),
    })
}
