//! Transient observations of already-owned service state; no ownership job.
use super::super::Service;
use super::super::{registry::bound, Failure};
use layerfs_protocol::daemon::{Reply, Request, WorkspaceToken};
use std::sync::Arc;

impl Service {
    pub(crate) fn operator_workspace(
        &self,
        directory: &str,
    ) -> Result<Arc<layerfs_store_service::store::BoundWorkspace>, Failure> {
        let entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
        entries
            .values()
            .find_map(|entry| match entry {
                super::super::registry::Entry::Bound(value)
                    if value
                        .kernel
                        .block()
                        .ready
                        .as_ref()
                        .is_some_and(|ready| ready.directory == directory) =>
                {
                    Some(value.workspace.clone())
                }
                _ => None,
            })
            .ok_or(Failure::Rejected(
                layerfs_protocol::daemon::ControlCode::Missing,
                "Workspace path absent",
            ))
    }

    /// The activity of the Workspace mounted at `directory`, the phase its
    /// admitted operation is in (empty when none runs) and when the
    /// activity began, in ms since the epoch; `None` when no Workspace is
    /// mounted there. It travels in the operator's changed-path answer, not
    /// in the Status record, whose bytes stay readable by other builds.
    pub(crate) fn operator_activity(
        &self,
        directory: &str,
    ) -> Option<(layerfs_protocol::daemon::Activity, &'static str, i64)> {
        let entries = self.entries.lock().ok()?;
        entries.values().find_map(|entry| match entry {
            super::super::registry::Entry::Bound(value)
                if value
                    .kernel
                    .block()
                    .ready
                    .as_ref()
                    .is_some_and(|ready| ready.directory == directory) =>
            {
                Some((value.activity, value.phase, value.since_ms))
            }
            _ => None,
        })
    }

    pub(crate) fn observed_store(&self) -> Arc<layerfs_store_service::store::Store> {
        self.store.clone()
    }
    pub(crate) fn observed_overlay(
        &self,
        token: WorkspaceToken,
    ) -> Result<layerfs_store_service::OwnerClient, Failure> {
        let entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
        let workspace = &bound(&entries, token)?.workspace;
        workspace
            .operation()
            .map(|operation| operation.overlay().clone())
            .map_err(Failure::Workspace)
    }
    #[cfg(target_os = "linux")]
    pub(crate) fn observed_kernel(
        &self,
        token: WorkspaceToken,
    ) -> Result<Option<layerfs_fuse::session::SessionObserver>, Failure> {
        use super::super::kernel::Kernel;
        let entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
        let binding = bound(&entries, token)?;
        Ok(match &binding.kernel {
            Kernel::Ready(attached) => Some(attached.session.observer()),
            Kernel::Leaving(leaving) => Some(leaving.observer.clone()),
            Kernel::Retained(kept) => kept.observer.clone(),
            Kernel::Unattached
            | Kernel::Attaching
            | Kernel::Exported(_)
            | Kernel::ExportLeaving(_)
            | Kernel::ExportKept(_) => None,
        })
    }
}

pub(crate) fn request_token(request: &Request) -> Option<WorkspaceToken> {
    match request {
        Request::Commit(control) => Some(control.token),
        Request::Unmount(control) => Some(control.token),
        Request::Status(token)
        | Request::Attach(token)
        | Request::Cleanup(token)
        | Request::ForceUnmount { token, .. } => Some(*token),
        _ => None,
    }
}
pub(crate) fn reply_token(reply: &Reply) -> Option<WorkspaceToken> {
    match reply {
        Reply::Bound { token, .. } | Reply::Unmounted(token) | Reply::Cleanup { token, .. } => {
            Some(*token)
        }
        Reply::Status(value) | Reply::Located(value) => Some(value.token),
        Reply::Ready(value) => Some(value.token),
        Reply::Retained(value) => Some(value.token),
        Reply::ForceUnmounted(value) => Some(value.token),
        Reply::Dirty(value) => Some(value.token),
        _ => None,
    }
}
