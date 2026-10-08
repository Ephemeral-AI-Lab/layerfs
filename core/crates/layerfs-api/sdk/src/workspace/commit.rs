//! Explicit Commit over the existing daemon constructor/publication owner.
use super::WorkspaceApi;
use crate::OperationFailure;
use layerfs_bridge::control::{Reply, Request, WorkspaceToken};
use layerfs_history::CommitStagedOutcome;

impl WorkspaceApi<'_> {
    /// Requests one Commit and returns its original known publication/install result.
    /// The daemon captures the Workspace's shared published frontier and constructs
    /// it; this facade sends one request and infers nothing from the binding.
    pub fn commit(
        &mut self,
        token: WorkspaceToken,
    ) -> Result<CommitStagedOutcome, Box<OperationFailure>> {
        let request = Request::Commit(token);
        match self.exchange(request.clone())? {
            Reply::Committed(outcome) => Ok(outcome),
            reply => Err(OperationFailure::unexpected(request, reply)),
        }
    }
}
