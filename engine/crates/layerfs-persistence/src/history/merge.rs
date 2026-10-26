//! Branch merge: one new Branch from two sides, in one transaction.
//!
//! Both sides must still be at the nodes the caller merged; otherwise the
//! request is `HeadMoved` (a Branch) or `StackMoved` (main) and nothing is
//! written. A fast-forward creates the Branch at an existing node; a merge
//! creates it with one new Commit whose second parent is the right side.
//! Neither side changes.

use super::params;
use super::transaction::Transaction;
use layerfs_history::error::{HistoryError, HistoryResult, Missing, MovedState};
use layerfs_history::identity::NodeId;
use layerfs_history::records::{BranchMergeRequest, BranchSnapshot, MergeHead, MergeSide};

use super::rows::{one, sql};

const BRANCH_NAME_TAKEN: &str = "branch_branch_name_taken";
const INSERT_BRANCH: &str = "branch_insert_branch";

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
) -> HistoryResult<BranchSnapshot> {
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
    if super::branch::branch(tx, request.branch)?.is_some() {
        return Err(HistoryError::Integrity("Branch identity"));
    }
    if one(
        tx,
        BRANCH_NAME_TAKEN,
        params![request.stack.as_slice(), request.name.as_str()],
        |_| Ok(()),
    )?
    .is_some()
    {
        return Err(HistoryError::Integrity("Branch name"));
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
    super::branch::branch_snapshot(tx, request.branch)?
        .ok_or(HistoryError::Integrity("Branch insertion"))
}
