//! Logical terminal control through the existing daemon lifetime owner.
use super::WorkspaceApi;
use crate::{operation::exchange, OperationFailure};
use layerfs_bridge::control::{Reply, Request, WorkspaceToken};

impl WorkspaceApi<'_> {
    /// Requests one explicit normal terminal unmount. The daemon probes the
    /// kernel reversibly: Busy leaves the Workspace Ready and usable. Success is
    /// acknowledged only after detach, every loop join, daemon-work drain,
    /// native-owner revocation and logical Close. Retained custody is returned
    /// as its own typed cause; this facade adds no teardown shortcut.
    pub fn unmount(&mut self, token: WorkspaceToken) -> Result<(), Box<OperationFailure>> {
        let request = Request::Unmount(token);
        match exchange(self.control, request.clone())? {
            Reply::Unmounted(_) => Ok(()),
            reply => Err(OperationFailure::unexpected(request, reply)),
        }
    }
}
