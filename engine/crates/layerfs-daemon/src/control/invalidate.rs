//! Kernel cache invalidation after a Commit installed a root other than the
//! Workspace's own (a clean merge, or a merge with markers). The FUSE contract
//! skips invalidation only for an install that changes nothing the kernel
//! showed; this one sends it before the caller's answer.
use super::{Failure, Service};
use crate::store::BoundWorkspace;
use layerfs_bridge::control::WorkspaceToken;
use layerfs_content::ObjectId;

impl Service {
    /// One notification per changed inode and per rebound name the Workspace
    /// showed. A Workspace with no kernel connection has nothing cached.
    pub(super) fn invalidate(
        &self,
        token: WorkspaceToken,
        workspace: &BoundWorkspace,
        shown: ObjectId,
        installed: ObjectId,
    ) -> Result<(), Failure> {
        #[cfg(target_os = "linux")]
        {
            use super::{kernel::Kernel, registry::bound};
            use crate::store::Stale;
            use layerfs_bridge::control::ControlCode;
            let invalidator = {
                let entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
                match &bound(&entries, token)?.kernel {
                    Kernel::Ready(attached) => attached.session.invalidator(),
                    _ => None,
                }
            };
            let Some(invalidator) = invalidator else {
                return Ok(());
            };
            workspace
                .stale_after_install(shown, installed, |stale| match stale {
                    Stale::Inode(serial) => invalidator.inode(serial),
                    Stale::Entry { parent, name } => invalidator.entry(parent, name),
                })
                .map_err(|_| {
                    Failure::Rejected(
                        ControlCode::Unknown,
                        "kernel cache invalidation after a merged install failed",
                    )
                })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (token, workspace, shown, installed);
            Ok(())
        }
    }
}
