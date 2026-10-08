//! Explicit terminal namespace observation, independent of registry retention.
use super::{Failure, Service, Success};
use crate::{Command, OwnerError, Response};
use layerfs_bridge::control::{CleanupObservation, Reply, WorkspaceToken};
use layerfs_overlay::CleanupState;

impl Service {
    pub(super) fn cleanup(&self, token: WorkspaceToken) -> Result<Success, Failure> {
        let done = self
            .owner
            .try_submit(
                None,
                Command::ObserveCleanup {
                    namespace: token.namespace,
                    incarnation: token.workspace.to_bytes(),
                },
            )
            .map_err(|(cause, command)| {
                Failure::Owner(OwnerError::Unattempted {
                    cause: Box::new(cause),
                    command: Box::new(command),
                })
            })?
            .wait()
            .map_err(Failure::Owner)?;
        let state = match done.result() {
            Ok(Response::CleanupState(state)) => match state {
                CleanupState::Live => CleanupObservation::Live,
                CleanupState::Held => CleanupObservation::Held,
                CleanupState::Queued => CleanupObservation::Queued,
                CleanupState::Gone => CleanupObservation::Gone,
            },
            _ => return Err(Failure::Completion(Box::new(done))),
        };
        // Drop the caller's completion reference before sending the copied
        // observation. The publisher may still transiently hold its original
        // Arc/credit; this is not a zero-credit or slot-release fence.
        drop(done);
        Ok(Success::reply(Reply::Cleanup { token, state }))
    }
}
