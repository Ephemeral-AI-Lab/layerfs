//! Explicit Commit over the existing daemon constructor/publication owner.
use super::WorkspaceApi;
use crate::OperationFailure;
use layerfs_bridge::control::{
    CommitControl, ConflictReport, MergedCommit, Reply, Request, WorkspaceToken,
};
use layerfs_history::CommitOutcome;

/// The daemon's answer to one Workspace Commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitReply {
    /// Published against the head the Workspace was bound to, or forced.
    Committed(Box<CommitOutcome>),
    /// Merged with a moved head, then published.
    Merged(Box<MergedCommit>),
    /// Refused for conflicts; nothing was published.
    Conflict(Box<ConflictReport>),
}

impl WorkspaceApi<'_> {
    /// Requests one Commit and returns its original known publication/install result.
    /// The daemon captures the Workspace's shared published frontier and constructs
    /// it; this facade sends one request and infers nothing from the binding.
    /// A merged publication returns its outcome; a conflict is unexpected here.
    pub fn commit(
        &mut self,
        token: WorkspaceToken,
    ) -> Result<CommitOutcome, Box<OperationFailure>> {
        let request = Request::Commit(CommitControl {
            token,
            force: false,
            message: None,
        });
        match self.exchange(request.clone())? {
            Reply::Committed(outcome) => Ok(outcome),
            Reply::MergeCommitted(merged) => Ok(merged.outcome),
            reply => Err(OperationFailure::unexpected(request, reply)),
        }
    }

    /// Requests one Commit; `force` publishes over a moved head instead of
    /// merging, and `agent` and `call` are recorded on the Commit. Returns the publication, the merged publication or the
    /// conflict refusal.
    pub fn commit_with(
        &mut self,
        control: CommitControl,
    ) -> Result<CommitReply, Box<OperationFailure>> {
        let request = Request::Commit(control);
        match self.exchange(request.clone())? {
            Reply::Committed(outcome) => Ok(CommitReply::Committed(Box::new(outcome))),
            Reply::MergeCommitted(merged) => Ok(CommitReply::Merged(merged)),
            Reply::Conflict(report) => Ok(CommitReply::Conflict(report)),
            reply => Err(OperationFailure::unexpected(request, reply)),
        }
    }
}
