//! Logical terminal control through the existing daemon lifetime owner.
use super::WorkspaceApi;
use crate::{operation::exchange, OperationFailure};
use layerfs_bridge::control::{Reply, Request, WorkspaceToken};

impl WorkspaceApi<'_> {
    /// Requests one explicit terminal unmount. The daemon must establish its
    /// actual native/engine lifetime fences; this facade adds no teardown shortcut.
    /// Current pre-S8 service supplies logical Close only, never native Ready.
    pub fn unmount(&mut self, token: WorkspaceToken) -> Result<(), Box<OperationFailure>> {
        let request = Request::Unmount(token);
        match exchange(self.control, request.clone())? {
            Reply::Unmounted(_) => Ok(()),
            reply => Err(OperationFailure::unexpected(request, reply)),
        }
    }
}
