//! The Store history position: the `history_meta.next_history_serial` counter
//! a `remove_file` frontier names, and the serial each Commit and Layer
//! records.
//!
//! Every new Commit takes `next_history_serial` and advances the counter in
//! the same write transaction; a Layer that republishes a Commit carries that
//! Commit's serial. The high-water a marker stores is `next_history_serial -
//! 1`, read in the marker transaction, so a node created concurrently is
//! ordered by its serial and never by a later read.

use super::super::params;
use super::super::statement::rows::{one, unsigned};
use super::super::statement::transaction::Transaction;
use layerfs_history::error::{HistoryError, HistoryResult};
use layerfs_history::identity::{LayerStackId, NodeId};

const HIGH_WATER: &str = "position_high_water";

const NEXT_SERIAL: &str = "position_next_serial";

const STACK_HIGH_WATER: &str = "position_stack_high_water";

const NODE_SERIAL: &str = "position_node_serial";

/// The Store high-water: `next_history_serial - 1`.
pub(crate) fn high_water(tx: &Transaction<'_>) -> HistoryResult<u64> {
    one(tx, HIGH_WATER, params![], |row| unsigned(row, 0))?
        .ok_or(HistoryError::Integrity("history position"))
}

/// Takes the next Store serial, advancing `next_history_serial` in this
/// transaction.
pub(crate) fn next_serial(tx: &Transaction<'_>) -> HistoryResult<u64> {
    one(tx, NEXT_SERIAL, params![], |row| unsigned(row, 0))?
        .ok_or(HistoryError::Integrity("history position"))
}

/// The largest `history_serial` in one Stack, for the `UpToDate` test.
pub(crate) fn stack_high_water(tx: &Transaction<'_>, stack: LayerStackId) -> HistoryResult<u64> {
    one(tx, STACK_HIGH_WATER, [stack.as_slice()], |row| {
        unsigned(row, 0)
    })?
    .ok_or(HistoryError::Integrity("history position"))
}

/// The `history_serial` one Commit or Layer records, when it exists.
pub(crate) fn node_serial(tx: &Transaction<'_>, node: NodeId) -> HistoryResult<Option<u64>> {
    one(tx, NODE_SERIAL, [node.as_slice()], |row| unsigned(row, 0))
}
