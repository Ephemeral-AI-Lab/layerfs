//! Branch merge: one new Branch from two sides, or one side merged into an
//! existing Branch, in one transaction.
//!
//! Both sides must still be at the nodes the caller merged; otherwise the
//! request is `HeadMoved` (a Branch) or `StackMoved` (main) and nothing is
//! written. A fast-forward puts the Branch at an existing node; a merge
//! gives it one new Commit whose second parent is the right side. A merge
//! into a new Branch changes neither side; a merge into an existing Branch
//! moves that Branch, the left side, and nothing else.

use super::super::params;
use super::super::statement::transaction::Transaction;
use layerfs_history::error::{HistoryError, HistoryResult, Missing, MovedState};
use layerfs_history::identity::NodeId;
use layerfs_history::records::{BranchMergeRequest, BranchSnapshot, MergeHead, MergeSide};

use super::super::statement::rows::sql;

const INSERT_BRANCH: &str = "branch_insert_branch";

const MOVE_BRANCH: &str = "branch_move_branch";

/// Where a merge's result goes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Into {
    /// A new Branch, `request.branch` and `request.name`.
    New,
    /// The existing Branch `request.branch`, which is the left side.
    Existing,
}

/// The node one side is at now.
fn side_node(
    tx: &Transaction<'_>,
    request: &BranchMergeRequest,
    side: MergeSide,
) -> HistoryResult<NodeId> {
    match side {
        MergeSide::Main => {
            let stack = super::layerstack::layer_stack(tx, request.stack)?
                .ok_or(HistoryError::Missing(Missing::LayerStack))?;
            Ok(NodeId::Layer(stack.head_layer))
        }
        MergeSide::Branch(id) => {
            let branch =
                super::branch::branch(tx, id)?.ok_or(HistoryError::Missing(Missing::Branch))?;
            if branch.stack != request.stack {
                return Err(HistoryError::Integrity("Branch ownership"));
            }
            Ok(branch
                .head_commit
                .map_or(NodeId::Layer(branch.base_layer), NodeId::Commit))
        }
    }
}

pub(crate) fn merge_branch(
    tx: &Transaction<'_>,
    request: &BranchMergeRequest,
    into: Into,
) -> HistoryResult<BranchSnapshot> {
    if into == Into::Existing && request.sides[0].0 != MergeSide::Branch(request.branch) {
        return Err(HistoryError::InvalidInput("merge target"));
    }
    // Spec 5: the project epoch opens before the side, name and head reads.
    // Every read below is filtered (a retired stack or Branch is missing), so
    // a removal that committed before this line is visible, and one that
    // commits after it changes the epoch `confirm` re-checks; a removal in
    // between is never invisible.
    if super::layerstack::layer_stack(tx, request.stack)?.is_none() {
        return Err(HistoryError::Missing(Missing::LayerStack));
    }
    let fence = crate::gc::fence::open(tx, request.stack)?;
    for (side, expected) in request.sides {
        let actual = side_node(tx, request, side)?;
        if actual == expected {
            continue;
        }
        return Err(match (side, expected, actual) {
            (MergeSide::Main, NodeId::Layer(expected), NodeId::Layer(actual)) => {
                HistoryError::StackMoved { expected, actual }
            }
            (MergeSide::Branch(id), _, _) => {
                let branch =
                    super::branch::branch(tx, id)?.ok_or(HistoryError::Missing(Missing::Branch))?;
                let (expected_head, expected_base) = match expected {
                    NodeId::Commit(commit) => (Some(commit), branch.base_layer),
                    NodeId::Layer(layer) => (None, layer),
                };
                HistoryError::HeadMoved(Box::new(MovedState {
                    expected_head,
                    actual_head: branch.head_commit,
                    expected_base,
                    actual_base: branch.base_layer,
                }))
            }
            _ => HistoryError::Integrity("merge side"),
        });
    }
    if into == Into::New {
        if super::branch::branch_any(tx, request.branch)?.is_some() {
            return Err(super::branch::taken(tx, request.stack, &request.name)?);
        }
        if let Some(state) = super::branch::name_taken(tx, request.stack, &request.name)? {
            return Err(HistoryError::NameTaken(Box::new(state)));
        }
    }
    if let (Into::Existing, MergeHead::Node(node)) = (into, &request.head) {
        // A Branch moves to an existing node only forward: the node holds
        // everything the Branch held.
        if !super::super::lineage::ancestry::is_ancestor(tx, request.sides[0].1, *node)? {
            return Err(HistoryError::InvalidInput("fast-forward target"));
        }
    }
    let (base, head) = match &request.head {
        MergeHead::Node(NodeId::Layer(layer)) => (*layer, None),
        MergeHead::Node(NodeId::Commit(commit)) => {
            let record = super::commit::commit(tx, *commit)?
                .ok_or(HistoryError::Missing(Missing::Commit))?;
            (record.base_layer, Some(*commit))
        }
        MergeHead::Commit(merge) => (merge.base, None),
    };
    for node in [Some(NodeId::Layer(base)), head.map(NodeId::Commit)]
        .into_iter()
        .flatten()
    {
        let stack = match node {
            NodeId::Layer(id) => {
                super::layerstack::layer(tx, id)?
                    .ok_or(HistoryError::Missing(Missing::Layer))?
                    .stack
            }
            NodeId::Commit(id) => {
                super::commit::commit(tx, id)?
                    .ok_or(HistoryError::Missing(Missing::Commit))?
                    .stack
            }
        };
        if stack != request.stack {
            return Err(HistoryError::Integrity("merge node ownership"));
        }
    }
    let mut head_commit = head;
    if let MergeHead::Commit(merge) = &request.head {
        let record = super::commit::new_commit(
            tx,
            request.stack,
            request.branch,
            merge.root,
            merge.parent,
            merge.base,
            Some(merge.second_parent),
            &merge.meta,
        )?;
        head_commit = Some(record.id);
    }
    match into {
        Into::New => {
            tx.execute(
                INSERT_BRANCH,
                params![
                    request.branch.as_slice(),
                    request.stack.as_slice(),
                    request.name.as_str(),
                    base.as_slice(),
                    head_commit.map(|id| id.to_bytes().to_vec()),
                    request.sides[0].1.as_slice().to_vec(),
                ],
            )
            .map_err(sql)?;
        }
        Into::Existing => {
            // The sides check above read this Branch at the node the caller
            // merged, in this transaction: the move replaces exactly that.
            let moved = tx
                .execute(
                    MOVE_BRANCH,
                    params![
                        base.as_slice(),
                        head_commit.map(|id| id.to_bytes().to_vec()),
                        request.branch.as_slice(),
                    ],
                )
                .map_err(sql)?;
            if moved != 1 {
                return Err(HistoryError::Integrity("Branch move"));
            }
        }
    }
    let snapshot = super::branch::branch_snapshot(tx, request.branch)?
        .ok_or(HistoryError::Integrity("Branch insertion"))?;
    fence.confirm(tx)?;
    Ok(snapshot)
}
