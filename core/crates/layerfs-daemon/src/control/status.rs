//! Coherent control metadata and a separately scoped indexed engine observation.
use super::{
    registry::{bound, Entry},
    Failure, Service, Success,
};
use crate::{Command, Response};
use layerfs_bridge::control::{
    ControlCode, LocalObservation, Reply, WorkspaceStatus, WorkspaceToken,
};
use layerfs_history::WorkspaceId;
impl Service {
    /// Observation by incarnation for a caller that holds no namespace. The
    /// registry supplies the token; nothing is replayed, bound or settled.
    pub(super) fn locate(&self, workspace: WorkspaceId) -> Result<Success, Failure> {
        let token = {
            let entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
            match entries.get(&workspace) {
                Some(Entry::Bound(value)) => WorkspaceToken {
                    workspace,
                    namespace: value.workspace.route().namespace(),
                },
                Some(Entry::Binding) => {
                    return Err(Failure::Rejected(
                        ControlCode::Busy,
                        "Workspace binding retained",
                    ))
                }
                None => return Err(Failure::Rejected(ControlCode::Missing, "Workspace absent")),
            }
        };
        let mut success = self.status(token)?;
        if let Reply::Status(status) = success.reply {
            success.reply = Reply::Located(status);
        }
        Ok(success)
    }
    pub(super) fn status(&self, token: WorkspaceToken) -> Result<Success, Failure> {
        let (workspace, binding, activity, epoch, published, native) = {
            let entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
            let value = bound(&entries, token)?;
            (
                value.workspace.clone(),
                value.snapshot.clone(),
                value.activity,
                value.epoch,
                value.published.clone(),
                // Copied with the control fields: maintained counters only.
                self.native.as_ref().map(|_| value.native.block()),
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
                native,
            })),
            completion,
            earlier: Vec::new(),
            commit: None,
            observation_failure,
            native: None,
        })
    }
}
