//! Acknowledged Store/engine binding, explicitly distinct from native Ready.
use super::WorkspaceApi;
use crate::OperationFailure;
use layerfs_bridge::control::{Reply, Request, WorkspaceToken};
use layerfs_history::{BranchId, BranchSnapshot, WorkspaceId};

/// Original logical binding. It carries no mounted location or native readiness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundWorkspace {
    /// Exact acknowledged namespace/incarnation.
    pub token: WorkspaceToken,
    /// Coherent selected Branch/root snapshot, without a whole-tree scan.
    pub binding: BranchSnapshot,
}
impl WorkspaceApi<'_> {
    /// Prepares one logical Store/engine binding once. This is not a native mount.
    /// Lost delivery keeps the original request/unknown, with no automatic rebind.
    pub fn bind(
        &mut self,
        workspace: WorkspaceId,
        branch: BranchId,
    ) -> Result<BoundWorkspace, Box<OperationFailure>> {
        let request = Request::Mount { workspace, branch };
        match self.exchange(request.clone())? {
            Reply::Bound { token, binding } => Ok(BoundWorkspace { token, binding }),
            reply => Err(OperationFailure::unexpected(request, reply)),
        }
    }
}
