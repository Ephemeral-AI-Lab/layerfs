//! Original component controls and read-only lost-reply recovery.
use crate::{BoundWorkspace, OperationFailure, WorkspaceApi};
use layerfs_bridge::control::{
    CleanupObservation, ReadyMount, Reply, Request, WorkspaceStatus, WorkspaceToken,
};
use layerfs_history::{BranchId, WorkspaceId};
impl WorkspaceApi<'_> {
    /// Prepares one logical Store/engine binding once. This is not a kernel mount.
    /// Lost delivery keeps the original request/unknown, with no automatic rebind.
    pub(crate) fn bind(
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

impl WorkspaceApi<'_> {
    /// Attaches one bound Workspace once. Ready means the kernel mount exists,
    /// the handshake completed and every receive loop is serving. A lost reply
    /// leaves the outcome unknown; observe it with `locate`, never by replay.
    pub(crate) fn attach(
        &mut self,
        token: WorkspaceToken,
    ) -> Result<ReadyMount, Box<OperationFailure>> {
        let request = Request::Attach(token);
        match self.exchange(request.clone())? {
            Reply::Ready(ready) => Ok(*ready),
            reply => Err(OperationFailure::unexpected(request, reply)),
        }
    }
    /// Observes one incarnation by identity alone, for a caller whose binding
    /// or attachment acknowledgement was lost. It binds and attaches nothing.
    pub(crate) fn locate(
        &mut self,
        workspace: WorkspaceId,
    ) -> Result<WorkspaceStatus, Box<OperationFailure>> {
        let request = Request::Locate(workspace);
        match self.exchange(request.clone())? {
            Reply::Located(status) => Ok(*status),
            reply => Err(OperationFailure::unexpected(request, reply)),
        }
    }
}

impl WorkspaceApi<'_> {
    /// Observes one allocated namespace once, including after its control
    /// binding was removed. Gone is physical retirement, distinct from Unmounted.
    pub(crate) fn cleanup(
        &mut self,
        token: WorkspaceToken,
    ) -> Result<CleanupObservation, Box<OperationFailure>> {
        let request = Request::Cleanup(token);
        match self.exchange(request.clone())? {
            Reply::Cleanup { state, .. } => Ok(state),
            reply => Err(OperationFailure::unexpected(request, reply)),
        }
    }
}
