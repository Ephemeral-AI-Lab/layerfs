//! Ordinary explicit Commit: shared preparation followed by one composite command.
use super::completion::CommitAttempt;
use crate::*;
use layerfs_bridge::contract::{HistoryCommand, Operation, HISTORY_RESULT_BYTES, MAX_OPERATION_MS};
use std::{sync::atomic::Ordering, time::Instant};
impl Workspace {
    pub fn commit(&self, deadline: Instant) -> Result<CommitReport, WorkspaceError> {
        if self.inner.access != WorkspaceAccess::LocalEdit {
            return Err(WorkspaceError::ReadOnly);
        }
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(WorkspaceError::Deadline)?;
        if remaining.as_millis() == 0 || remaining.as_millis() > u128::from(MAX_OPERATION_MS) {
            return Err(WorkspaceError::InvalidInput);
        }
        let _operation = self.begin(false, deadline)?;
        let mut first_remote = Some(self.begin(true, deadline)?);
        let submission = self.capture_submission(true, deadline)?;
        let attempt = match CommitAttempt::reserve(self, &submission, None) {
            Ok(attempt) => attempt,
            Err(error) => return Err(submission.fail(error)),
        };
        submission.commit_claimed.store(true, Ordering::Release);
        if submission.commit.set(attempt.clone()).is_err() {
            return Err(submission.fail(WorkspaceError::Io));
        }
        if let Err(error) = attempt.phase(&submission, CommitPhase::Preparing) {
            return Err(attempt.fail(&submission, error));
        }
        let changes = match self.prepare_changes(&submission, deadline, &mut first_remote) {
            Ok(changes) => changes,
            Err(error) => return Err(attempt.fail(&submission, submission.fail(error))),
        };
        let response = (|| {
            attempt.phase(&submission, CommitPhase::CompositeCommit)?;
            let remote = first_remote
                .take()
                .map_or_else(|| self.begin(true, deadline), Ok)?;
            let response = self.remote_call(
                (self.inner.store, submission.capture()?.generation),
                Operation::HistoryCommand(HistoryCommand::Commit(changes)),
                &mut &[][..],
                HISTORY_RESULT_BYTES as u64,
                &mut std::io::sink(),
                deadline,
            );
            drop(remote);
            response
        })();
        match self.complete_commit_response(&submission, &attempt, response, deadline) {
            Ok(report) => Ok(report),
            Err(error) => Err(attempt.fail(&submission, error)),
        }
    }
}
