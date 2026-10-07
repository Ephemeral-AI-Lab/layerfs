//! Coherent control metadata and a separately scoped indexed engine observation.
use super::{registry::bound, Failure, Service, Success};
use crate::{Command, Response};
use layerfs_bridge::control::{LocalObservation, Reply, WorkspaceStatus, WorkspaceToken};
impl Service {
    pub(super) fn status(&self, token: WorkspaceToken) -> Result<Success, Failure> {
        let (workspace, binding, activity, epoch, published) = {
            let entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
            let value = bound(&entries, token)?;
            (
                value.workspace.clone(),
                value.snapshot.clone(),
                value.activity,
                value.epoch,
                value.published.clone(),
            )
        };
        let observed = self.job(workspace.route(), Command::State);
        let (local, completion, observation_failure) = match observed {
            Ok(done) => match done.result() {
                Ok(Response::State(state)) => {
                    let local = LocalObservation {
                        revision: state.revision,
                        active: state.active.number(),
                        captured: state.captured.map(|generation| generation.number()),
                        captured_revision: state.captured_revision,
                        base_root: state.base_root,
                        dirty_inodes: state.dirty_inodes,
                        dirty_directory_entries: state.dirty_directory_entries,
                        closed: state.closed,
                        base_readers: state.base_readers,
                    };
                    (Some(local), Some(done), None)
                }
                _ => (
                    None,
                    None,
                    Some(Box::new(Failure::Completion(Box::new(done)))),
                ),
            },
            Err(error) => (None, None, Some(Box::new(error))),
        };
        let local_failure = observation_failure.as_ref().map(|error| error.wire());
        Ok(Success {
            reply: Reply::Status(Box::new(WorkspaceStatus {
                token,
                binding,
                activity,
                epoch,
                epoch_saturated: epoch == u64::MAX,
                published,
                local,
                local_failure,
            })),
            completion,
            commit: None,
            observation_failure,
        })
    }
}
