use crate::{
    worker::WorkspaceWorker, CreateWorkspaceSession, EndWorkspaceMode, Workspace,
    WorkspaceCommitResult, WorkspaceDetail, WorkspaceDiff, WorkspaceEndResult, WorkspaceError,
    WorkspaceProjection, WorkspaceResult, WorkspaceSession, WorkspaceSessionId, WorkspaceSummary,
    Workspaces,
};
use layerfs_storage::{CommitId, RefOutcome, Result, StorageError};
use std::sync::Arc;
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceState {
    Active,
    Committed,
    Discarded,
    Ended,
}

impl Workspace {
    #[doc(hidden)]
    pub fn commit(&mut self) -> Result<RefOutcome<CommitId>> {
        self.ensure_active()?;
        let candidate = self.build_candidate()?;
        let outcome = self.branch.commit_candidate(
            self.branch_id,
            self.expected_head,
            self.base_root,
            candidate,
        )?;
        self.state = WorkspaceState::Committed;
        Ok(outcome)
    }

    #[doc(hidden)]
    pub fn discard(&mut self) -> Result<()> {
        if self.state == WorkspaceState::Committed {
            return Err(StorageError::InvalidInput("workspace committed"));
        }
        self.clear_spool()?;
        self.state = WorkspaceState::Discarded;
        Ok(())
    }

    pub(crate) fn end_clean(&mut self) -> Result<()> {
        self.clear_spool()?;
        self.state = WorkspaceState::Ended;
        Ok(())
    }

    pub(crate) fn ensure_active(&self) -> Result<()> {
        if self.state == WorkspaceState::Active {
            Ok(())
        } else {
            Err(StorageError::InvalidInput("workspace inactive"))
        }
    }
}

impl Workspaces {
    pub fn create_workspace_session(
        &self,
        request: CreateWorkspaceSession,
    ) -> WorkspaceResult<WorkspaceSession> {
        self.prune_retained()?;
        if !request.placement.root().is_absolute() {
            return Err(WorkspaceError::InvalidPlacement);
        }
        let branches = self
            .branches
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        let mut matches =
            branches
                .iter()
                .filter_map(|store| match store.branch(request.branch_id) {
                    Ok(Some(_)) => Some(Ok(store.clone())),
                    Ok(None) => None,
                    Err(error) => Some(Err(WorkspaceError::Storage(error))),
                });
        let branch = matches
            .next()
            .transpose()?
            .ok_or(WorkspaceError::NotFound)?;
        if matches.next().is_some() {
            return Err(WorkspaceError::InvalidPlacement);
        }
        let id = WorkspaceSessionId::new();
        let state = self.runtime_root.join("workspaces").join(id.to_string());
        std::fs::create_dir_all(&state)?;
        let workspace = Workspace::open(branch, request.branch_id, state.join("spool"))?;
        let projection = request.projection.unwrap_or({
            if matches!(
                request.placement,
                crate::WorkspacePlacement::Container { .. }
            ) || cfg!(target_os = "linux")
            {
                WorkspaceProjection::Fuse
            } else {
                WorkspaceProjection::Materialize
            }
        });
        let worker = Arc::new(WorkspaceWorker::new(
            id,
            request.clone(),
            projection,
            workspace,
        ));
        let handle = match crate::projection::attach(&worker) {
            Ok(handle) => handle,
            Err(error) => {
                let _ = std::fs::remove_dir_all(&state);
                return Err(error);
            }
        };
        *worker
            .projection_handle
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)? = Some(handle);
        let session = session(&worker)?;
        self.sessions
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .insert(id, crate::registry::SessionRecord::Active(worker));
        Ok(session)
    }

    pub fn commit_workspace_session(
        &self,
        id: WorkspaceSessionId,
    ) -> WorkspaceResult<WorkspaceCommitResult> {
        let worker = self.worker(id)?;
        if worker.has_executions()? {
            return Err(WorkspaceError::WorkspaceBusy);
        }
        crate::projection::pause(&worker)?;
        let result = (|| {
            let _quiesced = worker.quiesce()?;
            crate::projection::capture(&worker)?;
            let mut workspace = worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?;
            let previous_head = workspace.expected_head;
            workspace
                .commit()
                .map(|outcome| WorkspaceCommitResult::from_outcome(previous_head, outcome))
                .or_else(WorkspaceError::from_commit)
        })();
        match &result {
            Ok(WorkspaceCommitResult::Created { .. } | WorkspaceCommitResult::UpToDate { .. }) => {
                let read_only = crate::projection::make_read_only(&worker);
                let resumed = crate::projection::resume(&worker);
                read_only?;
                resumed?;
            }
            _ => crate::projection::resume(&worker)?,
        }
        result
    }

    pub fn end_workspace_session(
        &self,
        id: WorkspaceSessionId,
        mode: EndWorkspaceMode,
    ) -> WorkspaceResult<WorkspaceEndResult> {
        let worker = self.worker(id)?;
        if worker.has_executions()? {
            return Err(WorkspaceError::WorkspaceBusy);
        }
        crate::projection::pause(&worker)?;
        let result = (|| {
            let _quiesced = worker.quiesce()?;
            let active = worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?
                .state
                == WorkspaceState::Active;
            if mode == EndWorkspaceMode::Clean && active && crate::projection::is_dirty(&worker)? {
                return Err(WorkspaceError::WorkspaceDirty);
            }
            let mut workspace = worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?;
            let state = workspace
                .spool
                .parent()
                .ok_or(WorkspaceError::InvalidPlacement)?
                .to_owned();
            let discarded = mode == EndWorkspaceMode::Discard;
            match mode {
                EndWorkspaceMode::Discard => {
                    workspace.discard()?;
                    workspace.state = WorkspaceState::Ended;
                }
                EndWorkspaceMode::Clean => workspace.end_clean()?,
            }
            drop(workspace);
            crate::projection::end(&worker)?;
            if state.exists() {
                std::fs::remove_dir_all(state)?;
            }
            Ok(WorkspaceEndResult {
                session_id: id,
                discarded,
            })
        })();
        if result.is_err() {
            crate::projection::resume(&worker)?;
            return result;
        }
        let result = result?;
        let retained = {
            let workspace = worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?;
            crate::registry::RetainedSession {
                session: session_locked(&worker, &workspace),
                mutation_generation: workspace.mutation_generation,
                ended_at: SystemTime::now(),
            }
        };
        self.sessions
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .insert(id, crate::registry::SessionRecord::Retained(retained));
        self.prune_retained()?;
        Ok(result)
    }

    pub fn sessions(&self) -> WorkspaceResult<Vec<WorkspaceSummary>> {
        self.prune_retained()?;
        self.sessions
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .values()
            .map(|record| match record {
                crate::registry::SessionRecord::Active(worker) => summary(worker),
                crate::registry::SessionRecord::Retained(retained) => {
                    Ok(crate::registry::retained_summary(retained))
                }
            })
            .collect()
    }

    pub fn session(&self, id: WorkspaceSessionId) -> WorkspaceResult<WorkspaceDetail> {
        self.prune_retained()?;
        let record = self
            .sessions
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .get(&id)
            .cloned()
            .ok_or(WorkspaceError::NotFound)?;
        let executions = self.execution_summaries(id)?;
        match record {
            crate::registry::SessionRecord::Active(worker) => {
                let workspace = worker
                    .workspace
                    .lock()
                    .map_err(|_| WorkspaceError::WorkspaceBusy)?;
                Ok(WorkspaceDetail {
                    session: session_locked(&worker, &workspace),
                    mutation_generation: workspace.mutation_generation,
                    executions,
                })
            }
            crate::registry::SessionRecord::Retained(retained) => Ok(WorkspaceDetail {
                session: retained.session,
                mutation_generation: retained.mutation_generation,
                executions,
            }),
        }
    }

    pub fn diff(&self, id: WorkspaceSessionId) -> WorkspaceResult<WorkspaceDiff> {
        self.prune_retained()?;
        let record = self
            .sessions
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .get(&id)
            .cloned()
            .ok_or(WorkspaceError::NotFound)?;
        match record {
            crate::registry::SessionRecord::Active(worker) => {
                let dirty = crate::projection::is_dirty(&worker)?;
                let workspace = worker
                    .workspace
                    .lock()
                    .map_err(|_| WorkspaceError::WorkspaceBusy)?;
                Ok(WorkspaceDiff {
                    session_id: id,
                    dirty,
                    mutation_generation: workspace.mutation_generation,
                })
            }
            crate::registry::SessionRecord::Retained(retained) => Ok(WorkspaceDiff {
                session_id: id,
                dirty: false,
                mutation_generation: retained.mutation_generation,
            }),
        }
    }
}

fn session(worker: &WorkspaceWorker) -> WorkspaceResult<WorkspaceSession> {
    let workspace = worker
        .workspace
        .lock()
        .map_err(|_| WorkspaceError::WorkspaceBusy)?;
    Ok(session_locked(worker, &workspace))
}

fn session_locked(worker: &WorkspaceWorker, workspace: &Workspace) -> WorkspaceSession {
    WorkspaceSession {
        id: worker.id,
        branch_id: workspace.branch_id,
        pinned_head: workspace.expected_head,
        placement: worker.request.placement.clone(),
        projection: worker.projection,
        state: workspace.state,
    }
}

fn summary(worker: &Arc<WorkspaceWorker>) -> WorkspaceResult<WorkspaceSummary> {
    let dirty = crate::projection::is_dirty(worker)?;
    let workspace = worker
        .workspace
        .lock()
        .map_err(|_| WorkspaceError::WorkspaceBusy)?;
    Ok(WorkspaceSummary {
        id: worker.id,
        branch_id: workspace.branch_id,
        pinned_head: workspace.expected_head,
        state: workspace.state,
        dirty,
    })
}
