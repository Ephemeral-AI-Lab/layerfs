//! Bounded original Workspace observations, never a hidden Store refresh.
use super::WorkspaceApi;
use crate::OperationFailure;
use layerfs_protocol::provision::install::{Reply, Request, WorkspaceStatus, WorkspaceToken};

impl WorkspaceApi<'_> {
    /// Observes existing maintained state once. No unknown outcome is settled.
    pub fn status(
        &mut self,
        token: WorkspaceToken,
    ) -> Result<WorkspaceStatus, Box<OperationFailure>> {
        let request = Request::Status(token);
        match self.exchange(request.clone())? {
            Reply::Status(status) => Ok(*status),
            reply => Err(OperationFailure::unexpected(request, reply)),
        }
    }
}
