use super::state::Workspace;
use crate::{ReferenceScope, WorkspaceError, WorkspaceStatus};
use std::{fs, sync::atomic::Ordering};

/// One projection binding. Dropping it never claims that kernel detach succeeded.
pub struct MountLease {
    workspace: Workspace,
    finished: bool,
}
impl Workspace {
    pub fn reserve_mount(&self) -> Result<MountLease, WorkspaceError> {
        let mut state = self.state()?;
        self.available(&state)?;
        if state.mounted || state.active > 0 {
            return Err(WorkspaceError::Busy);
        }
        state.mounted = true;
        Ok(MountLease {
            workspace: self.clone(),
            finished: false,
        })
    }
    pub fn status(&self) -> Result<WorkspaceStatus, WorkspaceError> {
        let state = self.state()?;
        Ok(WorkspaceStatus {
            mounted: state.mounted,
            stopping: self.inner.stopping.load(Ordering::Acquire),
            closed: state.closed,
            active_operations: state.active,
            nodes: state.nodes.len(),
            handles: state.handles.len(),
            projection_handles: state
                .handles
                .iter()
                .filter(|handle| handle.scope == ReferenceScope::Projection)
                .count(),
            cookies: state.cookies.len(),
            accounted_bytes: self.host.budget.used(),
        })
    }
    pub fn close_clean(&self) -> Result<(), WorkspaceError> {
        {
            let state = self.state()?;
            if state.closed {
                return Err(WorkspaceError::Closed);
            }
            if state.mounted
                || state.active > 0
                || !state.handles.is_empty()
                || state
                    .nodes
                    .iter()
                    .any(|node| node.lookups > 0 || node.projection_lookups > 0)
            {
                return Err(WorkspaceError::Busy);
            }
            self.inner.stopping.store(true, Ordering::Release);
        }
        // A failed leaf removal keeps registry/count/table ownership for inspection.
        fs::remove_dir(&self.inner.mount_path)?;
        let mut state = self.state()?;
        state.closed = true;
        state.nodes = Vec::new();
        state.handles = Vec::new();
        state.cookies = Vec::new();
        state.tables = None;
        drop(state);
        self.host
            .registry
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .retain(|entry| entry.incarnation != self.inner.incarnation);
        Ok(())
    }
}
impl MountLease {
    pub fn stop_admission(&mut self) {
        self.workspace.inner.stopping.store(true, Ordering::Release);
    }
    /// Call only after the projection has detached and joined its callback loops.
    /// Open semantic handles remain owned; detached kernel lookup references end.
    pub fn finish(&mut self) -> Result<(), WorkspaceError> {
        if self.finished {
            return Err(WorkspaceError::Closed);
        }
        let mut state = self.workspace.state()?;
        if state.active > 0
            || state
                .handles
                .iter()
                .any(|handle| handle.scope == ReferenceScope::Projection)
        {
            return Err(WorkspaceError::Busy);
        }
        state.mounted = false;
        for node in &mut state.nodes {
            node.projection_lookups = 0;
        }
        state.collect(self.workspace.inner.root.serial);
        self.workspace
            .inner
            .stopping
            .store(false, Ordering::Release);
        self.finished = true;
        Ok(())
    }
}
