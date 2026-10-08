//! Logical terminal control through the existing daemon lifetime owner.
use super::WorkspaceApi;
use crate::OperationFailure;
use layerfs_bridge::control::{ForcedOutcome, Reply, Request, WorkspaceToken};

impl WorkspaceApi<'_> {
    /// Requests one explicit normal terminal unmount. The daemon probes the
    /// kernel reversibly: Busy leaves the Workspace Ready and usable. Success is
    /// acknowledged only after detach, every loop join, daemon-work drain,
    /// native-owner revocation and logical Close. Retained custody is returned
    /// as its own typed cause; this facade adds no teardown shortcut.
    pub fn unmount(&mut self, token: WorkspaceToken) -> Result<(), Box<OperationFailure>> {
        let request = Request::Unmount(token);
        match self.exchange(request.clone())? {
            Reply::Unmounted(_) => Ok(()),
            reply => Err(OperationFailure::unexpected(request, reply)),
        }
    }
    /// Requests one explicit forced terminal unmount. `relinquish_unknown` is
    /// required and has no default: it alone permits the daemon to close over
    /// stopped unknown Commit custody, which the outcome still reports as
    /// unknown. Success returns the daemon's abort, detach, Commit-knowledge
    /// and cleanup dispositions. A refusal before any effect is a remote
    /// cause; retained custody, with its forced facts, is its own typed cause.
    /// This facade signals no process and never replays the request.
    pub fn force_unmount(
        &mut self,
        token: WorkspaceToken,
        relinquish_unknown: bool,
    ) -> Result<ForcedOutcome, Box<OperationFailure>> {
        let request = Request::ForceUnmount {
            token,
            relinquish_unknown,
        };
        match self.exchange(request.clone())? {
            Reply::ForceUnmounted(closed) => Ok(closed.outcome),
            reply => Err(OperationFailure::unexpected(request, reply)),
        }
    }
}
