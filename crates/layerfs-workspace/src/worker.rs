use crate::{CreateWorkspaceSession, Workspace, WorkspaceError, WorkspaceId, WorkspaceProjection};
use std::sync::{Arc, Condvar, Mutex};

pub(crate) struct WorkspaceWorker {
    pub(crate) id: WorkspaceId,
    pub(crate) request: CreateWorkspaceSession,
    pub(crate) projection: WorkspaceProjection,
    pub(crate) identity: WorkspaceIdentity,
    pub(crate) workspace: Arc<Mutex<Workspace>>,
    pub(crate) lifecycle: Mutex<()>,
    pub(crate) remote: Mutex<Option<crate::live_backing::RemoteWorkspace>>,
    // The one installed host authority, shared by the mounted client, SDK ranges,
    // status and lifecycle. Container placement keeps its `remote` owner instead.
    host: Mutex<Option<Arc<crate::host_runtime::HostRuntime>>>,
    pub(crate) projection_handle: Mutex<Option<crate::projection::ProjectionHandle>>,
    admission: Mutex<Admission>,
    drained: Condvar,
}

#[derive(Clone)]
pub(crate) struct WorkspaceIdentity {
    pub(crate) layer_stack_id: layerfs_layerstack_store::LayerStackId,
    pub(crate) layer_stack_name: layerfs_layerstack_store::EntityName,
    pub(crate) branch_name: layerfs_layerstack_store::EntityName,
}

#[derive(Default)]
struct Admission {
    accepting: bool,
    closing: bool,
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
            lifecycle: Mutex::new(()),
            projection_handle: Mutex::new(None),
            remote: Mutex::new(None),
            host: Mutex::new(None),
            admission: Mutex::new(Admission {
                accepting: true,
                ..Admission::default()
            }),
            drained: Condvar::new(),
        }
    }

    /// The installed host authority, when this session owns one. Reading the
    /// slot is a short lock; callers never hold it across an operation.
    pub(crate) fn host_runtime(
        &self,
    ) -> Result<Option<Arc<crate::host_runtime::HostRuntime>>, WorkspaceError> {
        Ok(self
            .host
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .clone())
    }

    /// Install the authority constructed before projection attachment. The
    /// mounted client, SDK, status and lifecycle all reach this one owner. It is
    /// available wherever an attachment can own one: the Linux host-FUSE mount
    /// and the container placement whose helper mounts through the same
    /// authority.
    pub(crate) fn install_host_runtime(
        &self,
        runtime: Arc<crate::host_runtime::HostRuntime>,
    ) -> Result<(), WorkspaceError> {
        let mut slot = self
            .host
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        if slot.is_some() {
            return Err(WorkspaceError::InvalidExecution);
        }
        *slot = Some(runtime);
        Ok(())
    }

    /// Remove the authority once End/Discard has settled it. Callers hold the
    /// returned owner until its shutdown is acknowledged.
    pub(crate) fn take_host_runtime(
        &self,
    ) -> Result<Option<Arc<crate::host_runtime::HostRuntime>>, WorkspaceError> {
        Ok(self
            .host
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .take())
    }

    #[cfg(test)]
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

    #[cfg(test)]
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
            self.drained.notify_all();
        }
        Ok(())
    }

    pub(crate) fn note_execution(&self, started: bool) -> Result<(), WorkspaceError> {
        let mut admission = self
            .admission
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        if started {
            if admission.closing {
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

    pub(crate) fn begin_end(&self) -> Result<Ending<'_>, WorkspaceError> {
        let mut admission = self
            .admission
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        if admission.closing || admission.executions != 0 {
            return Err(WorkspaceError::WorkspaceBusy);
        }
        admission.closing = true;
        Ok(Ending { worker: self })
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
        Ok(Quiesced { worker: self })
    }

    pub(crate) fn wait_for_writers(&self) -> Result<(), WorkspaceError> {
        let mut admission = self
            .admission
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while admission.writers != 0 {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(WorkspaceError::WorkspaceBusy);
            }
            let (next, timeout) = self
                .drained
                .wait_timeout(admission, remaining)
                .map_err(|_| WorkspaceError::WorkspaceBusy)?;
            admission = next;
            if timeout.timed_out() && admission.writers != 0 {
                return Err(WorkspaceError::WorkspaceBusy);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) struct Callback<'a> {
    worker: &'a WorkspaceWorker,
}

#[cfg(test)]
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

pub(crate) struct Ending<'a> {
    worker: &'a WorkspaceWorker,
}
impl Drop for Ending<'_> {
    fn drop(&mut self) {
        if let Ok(mut admission) = self.worker.admission.lock() {
            admission.closing = false;
        }
    }
}
