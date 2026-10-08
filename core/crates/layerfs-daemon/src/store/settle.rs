//! One exact local failure disposition, never a guessed history cleanup.
use super::{BoundWorkspace, CommitFailure};
use crate::{Command, Completion, OwnerError, Response};
use std::sync::atomic::Ordering;

impl BoundWorkspace {
    /// One original job of the Commit thread. Admission is a readiness wait
    /// before the single attempt, as for a filesystem request: a Workspace
    /// busy with other callers delays the Commit and does not refuse it.
    pub(super) fn job(&self, command: Command) -> Result<Completion, OwnerError> {
        self.owner
            .submit_waiting(Some(self.route()), command)
            .map_err(|(cause, command)| OwnerError::Unattempted {
                cause: Box::new(cause),
                command: Box::new(command),
            })?
            .wait()
    }
    pub(super) fn settle_failure(&self, mut failure: CommitFailure) -> Box<CommitFailure> {
        if failure.published.is_some() || failure.error.uncertain() {
            return Box::new(failure);
        }
        let Some(capture) = failure.capture else {
            failure.locally_settled = true;
            self.committing.store(false, Ordering::Release);
            return Box::new(failure);
        };
        match self.job(Command::ResolveFailed(capture)) {
            Ok(done) => {
                failure.locally_settled = matches!(done.result(), Ok(Response::Done));
                failure.local = Some(done);
            }
            Err(error) => failure.local_error = Some(error),
        }
        if failure.locally_settled {
            self.committing.store(false, Ordering::Release);
        }
        Box::new(failure)
    }
}
