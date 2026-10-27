//! Coherent control metadata and a separately scoped indexed engine observation.
use super::{
    registry::{bound, Entry},
    Failure, Service, Success,
};
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
        let (workspace, binding, activity, epoch, published, kernel) = {
            let entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
            let value = bound(&entries, token)?;
            (
                value.workspace.clone(),
                value.snapshot.clone(),
                value.activity,
                value.epoch,
                value.published.clone(),
                // Copied with the control fields: maintained counters only.
                self.kernel.as_ref().map(|_| value.kernel.block()),
            )
        };
        let observed = self.job(
            workspace.route(),
            |job_client: &crate::OwnerClient, job_route| job_client.state(job_route),
        );
        let (local, observation_failure) = match observed {
            Ok(state) => (
                Some(LocalObservation {
                    revision: state.revision,
                    active: state.active.number(),
                    captured: state.captured.map(|generation| generation.number()),
                    captured_revision: state.captured_revision,
                    base_root: state.base_root,
                    dirty_inodes: state.dirty_inodes,
                    dirty_directory_entries: state.dirty_directory_entries,
                    closed: state.closed,
                    base_readers: state.base_readers,
                }),
                None,
            ),
            Err(error) => (None, Some(Box::new(error))),
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
                kernel,
            })),
            commit: None,
            observation_failure,
            kernel: None,
        })
    }
}
