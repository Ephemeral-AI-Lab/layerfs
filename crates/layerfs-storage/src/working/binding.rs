//! Legacy private Operation base resolution.

use crate::error::{EngineError, EngineResult};
use crate::full::branch::read::{read_branch_ancestry, BranchHead, VersionRef};
use rusqlite::Connection;

pub(crate) fn effective_branch_base(
    connection: &Connection,
    head: BranchHead,
) -> EngineResult<VersionRef> {
    if let Some(operation_version_id) = head.operation_version_id {
        return Ok(VersionRef::OperationVersion {
            branch_id: head.branch_id,
            operation_version_id,
            root: head.root,
        });
    }
    let ancestry = read_branch_ancestry(connection, head.branch_id)?
        .ok_or(EngineError::InvalidRecord("Branch ancestry"))?;
    if let Some(operation_version_id) = ancestry.fork_operation_version_id {
        Ok(VersionRef::OperationVersion {
            branch_id: ancestry
                .immediate_parent_branch_id
                .ok_or(EngineError::InvalidRecord("child Branch parent"))?,
            operation_version_id,
            root: ancestry.fork_root,
        })
    } else {
        Ok(VersionRef::Layer {
            layer_stack_id: ancestry.origin_layer_stack_id,
            layer_id: ancestry.origin_layer_id,
            root: ancestry.fork_root,
        })
    }
}
