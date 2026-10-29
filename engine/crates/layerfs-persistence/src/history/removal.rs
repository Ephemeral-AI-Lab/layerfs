//! Removal of one LayerStack (#506): a project's rows and lineage go in one
//! write transaction. Store-wide packs, objects, file digests and the stack's
//! allocator scope stay; keeping the scope keeps its serials from being reused.
use super::layerstack;
use super::transaction::Transaction;
use layerfs_history::error::{HistoryError, HistoryResult, Missing};
use layerfs_history::identity::LayerStackId;

/// In the order they run: the totals read the rows they subtract, and the
/// run-keyed lineage is found through the Commit lineage that names its runs.
/// Foreign keys are deferred, so the rows themselves go in any order.
const STATEMENTS: [&str; 9] = [
    "removal_totals",
    "removal_absorbed",
    "removal_reach",
    "removal_commit_lineage",
    "removal_layer_lineage",
    "removal_branches",
    "removal_commits",
    "removal_layers",
    "removal_stack",
];

/// Removes `stack`; an unknown stack is `Missing(LayerStack)`.
pub(crate) fn remove_layerstack(tx: &Transaction<'_>, stack: LayerStackId) -> HistoryResult<()> {
    if layerstack::layer_stack(tx, stack)?.is_none() {
        return Err(HistoryError::Missing(Missing::LayerStack));
    }
    for key in STATEMENTS {
        tx.execute(key, [stack.as_slice()])?;
    }
    Ok(())
}
