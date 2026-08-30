use crate::{
    worker::WorkspaceWorker, CreateWorkspaceSession, EndWorkspaceMode, Workspace,
    WorkspaceCommitResult, WorkspaceDetail, WorkspaceDiff, WorkspaceEndResult, WorkspaceError,
    WorkspaceId, WorkspaceProjection, WorkspaceResult, WorkspaceSession, WorkspaceSummary,
    Workspaces,
};
use layerfs_storage::{Result, StorageError};
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
    pub fn commit(&mut self) -> Result<(layerfs_branch_store::CommitOutcome, bool)> {
        self.ensure_active()?;
        if let Some(mut resolution) = self.resolution.take() {
            resolution.invalidate_if_mutated(self)?;
            if resolution.unresolved() != 0 {
                self.resolution = Some(resolution);
                return Err(StorageError::InvalidInput(
                    "unresolved reconciliation conflict",
                ));
            }
            let choices = resolution.choices()?;
            let candidate = match self.build_candidate() {
                Ok(candidate) => candidate,
                Err(error) => {
                    self.resolution = Some(resolution);
                    return Err(error);
                }
            };
            let outcome = self.branch.commit_reconciliation(
                self.parent.clone(),
                &resolution.prepared,
                candidate,
                &choices,
            );
            if outcome.is_err() {
                self.resolution = Some(resolution);
            }
            let outcome = outcome?;
            let reloaded = self.reload_committed(outcome);
            return Ok((outcome, reloaded));
        }
        let candidate = self.build_candidate()?;
        let outcome = self.branch.commit_candidate(
            self.branch_id,
            self.expected_head,
            self.expected_base,
            self.base_root,
            candidate,
            self.expected_base,
            false,
        )?;
        let reloaded = self.reload_committed(outcome);
        Ok((outcome, reloaded))
    }

    fn reload_committed(&mut self, outcome: layerfs_branch_store::CommitOutcome) -> bool {
        let committed = Self::open_with_policy(
            self.branch.clone(),
            self.parent.clone(),
            self.branch_id,
            self.spool.clone(),
            self.policy,
        );
        if let Ok(mut committed) = committed {
            let _ = self.clear_spool();
            committed.state = WorkspaceState::Committed;
            *self = committed;
            return true;
        }
        self.expected_head = match outcome {
            layerfs_branch_store::CommitOutcome::Created { commit_id, .. } => Some(commit_id),
            layerfs_branch_store::CommitOutcome::UpToDate { head } => head,
        };
        self.resolution = None;
        self.state = WorkspaceState::Committed;
        false
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
        if !request.placement.root().is_absolute() || request.placement.root().parent().is_none() {
            return Err(WorkspaceError::InvalidPlacement);
        }
        let identity = self.workspace_identity(request.branch_id)?;
        let lease = self.acquire_lease(request.branch_id)?;
        let id = WorkspaceId::new();
        let state = self.runtime_root.join("workspaces").join(id.to_string());
        std::fs::create_dir_all(&state)?;
        let workspace = Workspace::open(
            self.branch.clone(),
            self.parent.clone(),
            request.branch_id,
            state.join("spool"),
        )?;
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
            identity,
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
        lease.keep();
        Ok(session)
    }

    pub fn commit_workspace_session(
        &self,
        id: WorkspaceId,
    ) -> WorkspaceResult<WorkspaceCommitResult> {
        let worker = self.worker(id)?;
        if worker.has_executions()? {
            return Ok(WorkspaceCommitResult::Busy);
        }
        crate::projection::pause(&worker)?;
        let _quiesced = match worker.quiesce() {
            Ok(quiesced) => quiesced,
            Err(WorkspaceError::WorkspaceBusy) => {
                crate::projection::resume(&worker)?;
                return Ok(WorkspaceCommitResult::Busy);
            }
            Err(error) => {
                crate::projection::resume(&worker)?;
                return Err(error);
            }
        };
        let result = (|| {
            crate::projection::capture(&worker)?;
            let mut workspace = worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?;
            match workspace.commit() {
                Ok((outcome, reloaded)) => {
                    Ok((WorkspaceCommitResult::from_outcome(outcome), reloaded))
                }
                Err(error) => WorkspaceError::from_commit(error).map(|result| (result, false)),
            }
        })();
        match &result {
            Ok((
                WorkspaceCommitResult::Created { .. } | WorkspaceCommitResult::UpToDate { .. },
                reloaded,
            )) => {
                let presentation = if *reloaded {
                    crate::projection::refresh(&worker)
                        .and_then(|()| crate::projection::make_read_only(&worker))
                } else {
                    crate::projection::end(&worker)
                };
                if presentation.is_err() {
                    let _ = crate::projection::end(&worker);
                }
            }
            _ => crate::projection::resume(&worker)?,
        }
        match result {
            Ok((result, _)) => Ok(result),
            Err(WorkspaceError::WorkspaceBusy) => Ok(WorkspaceCommitResult::Busy),
            Err(error) => Err(error),
        }
    }

    pub fn end_workspace_session(
        &self,
        id: WorkspaceId,
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
            if mode == EndWorkspaceMode::Clean && active {
                let has_resolution = worker
                    .workspace
                    .lock()
                    .map_err(|_| WorkspaceError::WorkspaceBusy)?
                    .resolution
                    .is_some();
                if has_resolution || crate::projection::is_dirty(&worker)? {
                    return Err(WorkspaceError::WorkspaceDirty);
                }
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
        self.release_lease(worker.request.branch_id);
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

    pub fn session(&self, id: WorkspaceId) -> WorkspaceResult<WorkspaceDetail> {
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

    pub fn diff(&self, id: WorkspaceId) -> WorkspaceResult<WorkspaceDiff> {
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

pub(crate) fn session(worker: &WorkspaceWorker) -> WorkspaceResult<WorkspaceSession> {
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
        layer_stack_id: worker.identity.layer_stack_id,
        layer_stack_name: worker.identity.layer_stack_name.clone(),
        branch_name: worker.identity.branch_name.clone(),
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
        layer_stack_id: worker.identity.layer_stack_id,
        layer_stack_name: worker.identity.layer_stack_name.clone(),
        branch_name: worker.identity.branch_name.clone(),
        pinned_head: workspace.expected_head,
        state: workspace.state,
        dirty,
    })
}
