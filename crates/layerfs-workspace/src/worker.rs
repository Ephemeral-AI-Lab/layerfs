use crate::{CreateWorkspaceSession, Workspace, WorkspaceError, WorkspaceId, WorkspaceProjection};
use std::sync::{Arc, Condvar, Mutex};

pub(crate) struct WorkspaceWorker {
    pub(crate) id: WorkspaceId,
    pub(crate) request: CreateWorkspaceSession,
    pub(crate) projection: WorkspaceProjection,
    pub(crate) identity: WorkspaceIdentity,
    pub(crate) workspace: Arc<Mutex<Workspace>>,
    pub(crate) projection_handle: Mutex<Option<crate::projection::ProjectionHandle>>,
    admission: Mutex<Admission>,
    drained: Condvar,
}

#[derive(Clone)]
pub(crate) struct WorkspaceIdentity {
    pub(crate) layer_stack_id: layerfs_storage::LayerStackId,
    pub(crate) layer_stack_name: layerfs_storage::EntityName,
    pub(crate) branch_name: layerfs_storage::EntityName,
}

#[derive(Default)]
struct Admission {
    accepting: bool,
    callbacks: u32,
    writers: u32,
    executions: u32,
}

impl WorkspaceWorker {
    pub(crate) fn new(
        id: WorkspaceId,
        request: CreateWorkspaceSession,
        projection: WorkspaceProjection,
        identity: WorkspaceIdentity,
        workspace: Workspace,
    ) -> Self {
        Self {
            id,
            request,
            projection,
            identity,
            workspace: Arc::new(Mutex::new(workspace)),
            projection_handle: Mutex::new(None),
            admission: Mutex::new(Admission {
                accepting: true,
                ..Admission::default()
            }),
            drained: Condvar::new(),
        }
    }

    pub(crate) fn enter_callback(&self) -> Result<Callback<'_>, WorkspaceError> {
        let mut admission = self
            .admission
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        if !admission.accepting {
            return Err(WorkspaceError::WorkspaceBusy);
        }
        admission.callbacks += 1;
        Ok(Callback { worker: self })
    }

    pub(crate) fn note_writer(&self, opened: bool) -> Result<(), WorkspaceError> {
        let mut admission = self
            .admission
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        if opened {
            admission.writers += 1;
        } else {
            admission.writers = admission
                .writers
                .checked_sub(1)
                .ok_or(WorkspaceError::WorkspaceBusy)?;
        }
        Ok(())
    }

    pub(crate) fn note_execution(&self, started: bool) -> Result<(), WorkspaceError> {
        let mut admission = self
            .admission
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        if started {
            if !admission.accepting {
                return Err(WorkspaceError::WorkspaceBusy);
            }
            admission.executions += 1;
        } else {
            admission.executions = admission
                .executions
                .checked_sub(1)
                .ok_or(WorkspaceError::WorkspaceBusy)?;
        }
        Ok(())
    }

    pub(crate) fn has_executions(&self) -> Result<bool, WorkspaceError> {
        Ok(self
            .admission
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .executions
            != 0)
    }

    pub(crate) fn quiesce(&self) -> Result<Quiesced<'_>, WorkspaceError> {
        let mut admission = self
            .admission
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        admission.accepting = false;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while admission.callbacks != 0 {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                admission.accepting = true;
                return Err(WorkspaceError::WorkspaceBusy);
            }
            let (next, timeout) = self
                .drained
                .wait_timeout(admission, remaining)
                .map_err(|_| WorkspaceError::WorkspaceBusy)?;
            admission = next;
            if timeout.timed_out() && admission.callbacks != 0 {
                admission.accepting = true;
                return Err(WorkspaceError::WorkspaceBusy);
            }
        }
        if admission.writers != 0 || admission.executions != 0 {
            admission.accepting = true;
            return Err(WorkspaceError::WorkspaceBusy);
        }
        Ok(Quiesced { worker: self })
    }
}

pub(crate) struct Callback<'a> {
    worker: &'a WorkspaceWorker,
}

impl Drop for Callback<'_> {
    fn drop(&mut self) {
        if let Ok(mut admission) = self.worker.admission.lock() {
            admission.callbacks -= 1;
            if admission.callbacks == 0 {
                self.worker.drained.notify_all();
            }
        }
    }
}

pub(crate) struct Quiesced<'a> {
    worker: &'a WorkspaceWorker,
}

impl Drop for Quiesced<'_> {
    fn drop(&mut self) {
        if let Ok(mut admission) = self.worker.admission.lock() {
            admission.accepting = true;
            self.worker.drained.notify_all();
        }
    }
}
