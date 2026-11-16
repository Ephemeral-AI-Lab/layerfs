//! The Windows control operations: the Request dispatch over the
//! twin registry (#454). Every arm mirrors its unix face: admission
//! first, then the effect; the mount arms answer through WinFsp.
use super::registry_windows::Service;
use super::{Failure, Success};
use layerfs_bridge::control::{Reply, Request};
use layerfs_store_service::store::CommitOptions;

impl Service {
    /// Runs one control operation on the Windows registry.
    pub fn execute_control(&self, request: &Request) -> Result<Success, Failure> {
        match request {
            Request::Mount { .. } | Request::MountNamed { .. } => Err(Failure::Rejected(
                layerfs_bridge::control::ControlCode::Invalid,
                "Windows mount lands with the serve face batch (#454)",
            )),
            Request::Commit(control) => {
                let token = control.token;
                let success = self.commit_ready(token.workspace, &CommitOptions {
                    force: control.force,
                    message: control.message.clone(),
                })?;
                let reply = match (&success.history, &success.merge) {
                    (Some(outcome), None) => Reply::Committed(outcome.clone()),
                    (Some(outcome), Some(merge)) => Reply::MergeCommitted(Box::new(layerfs_bridge::control::MergedCommit {
                        outcome: outcome.clone(),
                        from: merge.from,
                        head: merge.head,
                        lww: merge.lww.clone(),
                    })),
                    (None, merge) => Reply::Conflict(Box::new(layerfs_bridge::control::ConflictReport {
                        head: merge.as_ref().and_then(|merge| merge.head),
                        base: merge.as_ref().and_then(|merge| merge.from),
                        markers: false,
                        conflicts: merge.as_ref().map(|m| m.conflicts.clone()).unwrap_or_default(),
                        lww: merge.as_ref().map(|m| m.lww.clone()).unwrap_or_default(),
                        written: merge.as_ref().map(|m| m.written.clone()).unwrap_or_default(),
                        truncated: false,
                    })),
                };
                let _ = &reply;
                Ok(Success {
                    reply,
                    commit: Some(success),
                    observation_failure: None,
                    kernel: None,
                })
            }
            Request::Unmount(control) => {
                let token = control.token;
                self.unmount(token.workspace)?;
                Ok(Success {
                    reply: Reply::Unmounted(token),
                    commit: None,
                    observation_failure: None,
                    kernel: None,
                })
            }
            Request::Attach(token) => {
                let _ = token;
                super::attach_windows::attach_winfsp()
            }
            other => {
                let _ = other;
                Err(Failure::Rejected(
                    layerfs_bridge::control::ControlCode::Invalid,
                    "the Windows control face answers the mount lifecycle today (#454)",
                ))
            }
        }
    }
}
