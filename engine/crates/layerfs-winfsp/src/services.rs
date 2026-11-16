//! The bridge between WinFsp callbacks and the mount services: the
//! context one mounted file system carries, reached from a callback's
//! FileSystem handle.
use layerfs_mount_ports::ports::{Fence, MountServices, RequestServices};
use std::sync::Arc;

/// What one mounted Workspace carries into every callback.
pub struct Context {
    pub services: Arc<dyn MountServices>,
    pub fence: Fence,
}

impl Context {
    /// Starts one request's services, as each callback does.
    pub fn request(&self) -> Result<Arc<dyn RequestServices>, String> {
        self.services
            .request(&self.fence)
            .map_err(|error| format!("mount services: {error}"))
    }
}
