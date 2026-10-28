//! Explicit terminal namespace observation, independent of registry retention.
use super::{Failure, Service, Success};
use layerfs_bridge::control::{CleanupObservation, Reply, WorkspaceToken};
use layerfs_overlay::CleanupState;

impl Service {
    pub(super) fn cleanup(&self, token: WorkspaceToken) -> Result<Success, Failure> {
        let done = self
            .owner
            .observe_cleanup(None, token.namespace, token.workspace.to_bytes())
            .map_err(Failure::Owner)?;
        let state = match done {
            CleanupState::Live => CleanupObservation::Live,
            CleanupState::Held => CleanupObservation::Held,
            CleanupState::Queued => CleanupObservation::Queued,
            CleanupState::Gone => CleanupObservation::Gone,
        };
        Ok(Success::reply(Reply::Cleanup { token, state }))
    }
}
