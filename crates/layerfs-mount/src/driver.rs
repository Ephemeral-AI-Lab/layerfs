use crate::workspace::{MountedLifecycle, MountedWorkspace};
use layerfs_workspace::{Presentation, Result, WorkspaceDriver, WorkspaceError};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct MountDriver {
    workspace: Arc<Mutex<MountedWorkspace>>,
    view: PathBuf,
}

impl MountDriver {
    pub fn new(workspace: MountedWorkspace, view: PathBuf) -> Self {
        Self {
            workspace: Arc::new(Mutex::new(workspace)),
            view,
        }
    }

    pub fn shared_workspace(&self) -> Arc<Mutex<MountedWorkspace>> {
        self.workspace.clone()
    }
}

impl WorkspaceDriver for MountDriver {
    fn presentation(&self) -> Presentation {
        Presentation::Mount
    }

    fn view_path(&self) -> Option<&Path> {
        Some(&self.view)
    }

    fn freeze(&mut self) -> Result<()> {
        self.workspace
            .lock()
            .map_err(|_| WorkspaceError::InvalidState)?
            .shutdown()
            .map(drop)
            .map_err(|_| WorkspaceError::InvalidState)
    }

    fn cleanup(&mut self) -> Result<()> {
        let mut workspace = self
            .workspace
            .lock()
            .map_err(|_| WorkspaceError::InvalidState)?;
        if workspace.lifecycle() != MountedLifecycle::Closed {
            return Err(WorkspaceError::InvalidState);
        }
        workspace
            .release_kernel_cache_ownership()
            .map_err(|_| WorkspaceError::InvalidState)
    }
}
