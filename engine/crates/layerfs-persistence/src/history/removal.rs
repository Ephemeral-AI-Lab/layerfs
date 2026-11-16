//! Removal of one LayerStack (#506): a project's rows and lineage go in one
//! write transaction. Store-wide packs, objects, file digests and the stack's
//! allocator scope stay; keeping the scope keeps its serials from being reused.
use super::layerstack;
use super::params;
use super::transaction::Transaction;
use layerfs_history::error::{HistoryError, HistoryResult, Missing};
use layerfs_history::identity::{BranchId, LayerStackId};

const RETIRE_INSERT: &str = "branch_retire_insert";

// Shared with project.delete: an upsert that only advances an existing row.
const EPOCH_BUMP: &str = "layerstack_bump_project_epoch";

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

/// The marker transaction of `branch.remove` (spec 4 / 4.4): one live
/// Branch, one `branch_retired` row inserted at most once, and the project
/// epoch bumped only by that insert. A repeat is `NoSuchBranch` because the
/// filtered lookup no longer sees the Branch; a concurrent remove that won
/// between that read and the insert makes the insert a no-op, which is `Busy`.
pub(crate) fn mark_branch_removed(
    tx: &Transaction<'_>,
    branch: BranchId,
    removed_ms: i64,
) -> HistoryResult<()> {
    let record =
        super::branch::branch(tx, branch)?.ok_or(HistoryError::Missing(Missing::Branch))?;
    let inserted = tx.execute(RETIRE_INSERT, params![branch.as_slice(), removed_ms])?;
    if inserted == 0 {
        return Err(HistoryError::Busy);
    }
    tx.execute(EPOCH_BUMP, [record.stack.as_slice()])?;
    Ok(())
}
