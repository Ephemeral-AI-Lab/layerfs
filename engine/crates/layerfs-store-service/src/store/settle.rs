//! One exact local failure disposition, never a guessed history cleanup.
use super::{BoundWorkspace, CommitFailure};
use crate::OwnerError;

impl BoundWorkspace {
    /// One original job of the Commit thread. Admission is a readiness wait
    /// before the single attempt, as for a filesystem request: a Workspace
    /// busy with other callers delays the Commit and does not refuse it.
    pub(super) fn job<T>(
        &self,
        command: impl FnOnce(&crate::OwnerClient, layerfs_workspace::Route) -> Result<T, crate::OwnerError>
            + Send,
    ) -> Result<T, OwnerError> {
        command(&self.owner, self.route())
    }
    pub(super) fn settle_failure(&self, mut failure: CommitFailure) -> Box<CommitFailure> {
        if failure.published.is_some() || failure.error.uncertain() {
            return Box::new(failure);
        }
        let Some(capture) = failure.capture else {
            failure.locally_settled = true;
            return Box::new(failure);
        };
        match self.job(|job_client: &crate::OwnerClient, job_route| {
            job_client.resolve_failed(job_route, capture)
        }) {
            Ok(()) => failure.locally_settled = true,
            Err(error) => failure.local_error = Some(error),
        }
        Box::new(failure)
    }
}
