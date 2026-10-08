//! Transient observations of already-owned service state; no ownership job.
use super::Service;
#[cfg(target_os = "linux")]
use super::{registry::bound, Failure};
use layerfs_bridge::control::{Reply, Request, WorkspaceToken};
use std::sync::Arc;

impl Service {
    pub(crate) fn observed_store(&self) -> Arc<crate::store::Store> {
        self.store.clone()
    }
    #[cfg(target_os = "linux")]
    pub(crate) fn observed_native(
        &self,
        token: WorkspaceToken,
    ) -> Result<Option<layerfs_fuse::session::SessionObserver>, Failure> {
        use super::native::Native;
        let entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
        let binding = bound(&entries, token)?;
        Ok(match &binding.native {
            Native::Ready(attached) => Some(attached.session.observer()),
            Native::Leaving(leaving) => Some(leaving.observer.clone()),
            Native::Retained(kept) => kept.observer.clone(),
            Native::Unattached | Native::Attaching => None,
        })
    }
}

pub(crate) fn request_token(request: &Request) -> Option<WorkspaceToken> {
    match request {
        Request::Commit(token)
        | Request::Status(token)
        | Request::Unmount(token)
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
        _ => None,
    }
}
