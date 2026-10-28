//! Transient observations of already-owned service state; no ownership job.
use super::Service;
use super::{registry::bound, Failure};
use layerfs_bridge::control::{Reply, Request, WorkspaceToken};
use std::sync::Arc;

impl Service {
    pub(crate) fn operator_workspace(
        &self,
        directory: &str,
    ) -> Result<Arc<crate::store::BoundWorkspace>, Failure> {
        let entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
        entries
            .values()
            .find_map(|entry| match entry {
                super::registry::Entry::Bound(value)
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
                layerfs_bridge::control::ControlCode::Missing,
                "Workspace path absent",
            ))
    }

    pub(crate) fn observed_store(&self) -> Arc<crate::store::Store> {
        self.store.clone()
    }
    pub(crate) fn observed_overlay(
        &self,
        token: WorkspaceToken,
    ) -> Result<crate::OwnerClient, Failure> {
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
        use super::kernel::Kernel;
        let entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
        let binding = bound(&entries, token)?;
        Ok(match &binding.kernel {
            Kernel::Ready(attached) => Some(attached.session.observer()),
            Kernel::Leaving(leaving) => Some(leaving.observer.clone()),
            Kernel::Retained(kept) => kept.observer.clone(),
            Kernel::Unattached | Kernel::Attaching => None,
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
