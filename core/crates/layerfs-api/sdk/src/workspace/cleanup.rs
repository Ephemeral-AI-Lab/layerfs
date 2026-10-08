//! Exact post-terminal cleanup observation; no unmount or maintenance replay.
use super::WorkspaceApi;
use crate::OperationFailure;
use layerfs_bridge::control::{CleanupObservation, Reply, Request, WorkspaceToken};

impl WorkspaceApi<'_> {
    /// Observes one allocated namespace once, including after its control
    /// binding was removed. Gone is physical retirement, distinct from Unmounted.
    pub fn cleanup(
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
