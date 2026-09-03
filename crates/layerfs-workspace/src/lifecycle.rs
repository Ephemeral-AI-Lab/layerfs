use crate::{
    worker::WorkspaceWorker, CreateWorkspaceSession, EndWorkspaceMode, Workspace,
    WorkspaceCommitResult, WorkspaceDetail, WorkspaceDiff, WorkspaceEndResult, WorkspaceError,
    WorkspaceFileRangeEdit, WorkspaceId, WorkspaceProjection, WorkspaceResult, WorkspaceSession,
    WorkspaceSummary, Workspaces,
};
use layerfs_layerstack_store::{
    CommitOutcome, Result, StoreError as StorageError, WorkspaceCommitPhase,
};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommitTransition {
    Rebased,
    RebasedRefresh,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceState {
    Active,
    Committed,
    Discarded,
    Ended,
    BrokenCleanup,
}

impl Workspace {
    pub(crate) fn commit(&mut self) -> Result<(CommitOutcome, CommitTransition)> {
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
            let outcome =
                self.store
                    .commit_reconciliation(&resolution.prepared, candidate, &choices);
            if outcome.is_err() {
                self.resolution = Some(resolution);
            }
            let outcome = outcome?;
            let transition = self.transition_committed(outcome, true)?;
            return Ok((outcome, transition));
        }
        let branch = self
            .store
            .branch(self.branch_id)?
            .ok_or(StorageError::NotFound("Branch"))?;
        if branch.head_commit_id != self.expected_head || branch.base_layer_id != self.expected_base
        {
            return Err(StorageError::CommitHeadMoved {
                expected: self.expected_head,
                actual: branch.head_commit_id,
            });
        }
        if self.mutation_generation == 0 {
            return Ok((
                CommitOutcome::UpToDate {
                    root_id: self.base_root,
                },
                CommitTransition::Rebased,
            ));
        }
        let candidate = self.build_candidate()?;
        let outcome =
            self.store
                .commit_candidate(&branch, self.base_root, self.expected_base, candidate)?;
        let transition = self.transition_committed(outcome, false)?;
        Ok((outcome, transition))
    }

    fn transition_committed(
        &mut self,
        outcome: CommitOutcome,
        refresh: bool,
    ) -> Result<CommitTransition> {
        let started = Instant::now();
        let rebased = if refresh {
            self.reload_committed(outcome)
        } else {
            self.rebase_committed(outcome)
        };
        layerfs_layerstack_store::note_workspace_commit_phase(
            WorkspaceCommitPhase::InPlaceRebase,
            elapsed_ns(started),
        );
        rebased?;
        Ok(if refresh {
            CommitTransition::RebasedRefresh
        } else {
            CommitTransition::Rebased
        })
    }

    fn rebase_committed(&mut self, outcome: CommitOutcome) -> Result<()> {
        let expected_head = match outcome {
            CommitOutcome::Committed { commit_id, .. } => Some(commit_id),
            CommitOutcome::UpToDate { .. } => self.expected_head,
        };
        let pinned = self.store.pin_branch(self.branch_id)?;
        if pinned.branch.head_commit_id != expected_head {
            return Err(StorageError::Integrity("committed Workspace head"));
        }
        static REBASE_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let rebase_spool = self
            .spool
            .parent()
            .ok_or(StorageError::InvalidInput("Workspace spool"))?
            .join(format!(
                "rebase-spool-{}",
                REBASE_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
        let mut committed = Self::from_snapshot(
            crate::cow_tree::WorkspaceSnapshot {
                store: self.store.clone(),
                branch_id: self.branch_id,
                expected_head,
                expected_base: pinned.branch.base_layer_id,
                root: pinned.root,
                reader: pinned.reader.with_read_metrics_from(&self.reader),
            },
            &rebase_spool,
            self.policy,
        )?;
        let current = self
            .nodes
            .iter()
            .map(|(id, node)| (*id, node.clone()))
            .collect::<Vec<_>>();
        let mut nodes = std::collections::HashMap::new();
        let mut canonical_nodes = std::collections::HashMap::new();
        let mut obsolete_spools = Vec::new();
        let mut retained_spool_nodes = std::collections::BTreeSet::new();
        let mut retained_spool_bytes = 0_u64;
        let mut retained_inline_bytes = 0_u64;
        let mut retained_piece_allocation_bytes = 0_u64;
        for (id, old) in current {
            if old.paths.is_empty() {
                if old.pins != 0 {
                    let mut retained = old;
                    retained.canonical = None;
                    if let crate::cow_tree::Data::File(crate::cow_tree::FileData::Edited {
                        spool_high_water,
                        pieces,
                        ..
                    }) = &retained.data
                    {
                        if !self.open_spools.contains_key(&id) {
                            return Err(StorageError::Integrity("spool descriptor"));
                        }
                        retained_spool_nodes.insert(id);
                        retained_spool_bytes =
                            retained_spool_bytes.saturating_add(*spool_high_water);
                        retained_inline_bytes =
                            retained_inline_bytes.saturating_add(pieces.inline_len());
                        retained_piece_allocation_bytes = retained_piece_allocation_bytes
                            .saturating_add(pieces.allocation_bytes()?);
                    }
                    nodes.insert(id, retained);
                }
                continue;
            }
            let fresh_id = if id == crate::ROOT {
                crate::ROOT
            } else {
                lookup_path(&mut committed, old.paths.first().expect("nonempty paths"))?
            };
            for path in old.paths.iter().skip(1) {
                if lookup_path(&mut committed, path)? != fresh_id {
                    return Err(StorageError::Integrity("committed hard-link identity"));
                }
            }
            let old_attr = self.attr(id)?;
            let fresh_attr = committed.attr(fresh_id)?;
            if old_attr.kind != fresh_attr.kind
                || old_attr.size != fresh_attr.size
                || old_attr.mode != fresh_attr.mode
                || old_attr.links != fresh_attr.links
                || old_attr.mtime_seconds != fresh_attr.mtime_seconds
                || old_attr.mtime_nanoseconds != fresh_attr.mtime_nanoseconds
            {
                return Err(StorageError::Integrity("committed Workspace presentation"));
            }
            let mut rebased = committed
                .nodes
                .get(&fresh_id)
                .ok_or(StorageError::Integrity("committed Workspace node"))?
                .clone();
            rebased.paths = old.paths;
            rebased.pins = old.pins;
            if let Some(inode) = rebased.canonical {
                if canonical_nodes.insert(inode, id).is_some() {
                    return Err(StorageError::Integrity("committed Workspace inode"));
                }
            }
            if let crate::cow_tree::Data::File(crate::cow_tree::FileData::Edited {
                spool, ..
            }) = old.data
            {
                if !self.open_spools.contains_key(&id) {
                    return Err(StorageError::Integrity("spool descriptor"));
                }
                obsolete_spools.push(spool);
            }
            nodes.insert(id, rebased);
        }
        self.open_spools
            .retain(|node, _| retained_spool_nodes.contains(node));
        for spool in obsolete_spools {
            if spool.exists() {
                std::fs::remove_file(spool)?;
            }
        }
        self.reader = committed.reader.clone();
        self.expected_head = expected_head;
        self.expected_base = committed.expected_base;
        self.base_root = committed.base_root;
        self.base_inodes = committed.base_inodes;
        self.nodes = nodes;
        self.canonical_nodes = canonical_nodes;
        self.spool_bytes = retained_spool_bytes;
        self.inline_bytes = retained_inline_bytes;
        self.piece_allocation_bytes = retained_piece_allocation_bytes;
        self.mutation_generation = 0;
        self.mutation_paths.clear();
        self.dirty.clear();
        self.capture = crate::capture::CaptureState::default();
        self.resolution = None;
        self.state = WorkspaceState::Active;
        let _ = std::fs::remove_dir_all(rebase_spool);
        Ok(())
    }

    fn reload_committed(&mut self, outcome: CommitOutcome) -> Result<()> {
        let expected_head = match outcome {
            CommitOutcome::Committed { commit_id, .. } => Some(commit_id),
            CommitOutcome::UpToDate { .. } => self.expected_head,
        };
        let pinned = self.store.pin_branch(self.branch_id)?;
        if pinned.branch.head_commit_id != expected_head {
            return Err(StorageError::Integrity("committed Workspace head"));
        }
        let metrics = self.reader.clone();
        let spool = self.spool.clone();
        self.clear_spool()?;
        let mut committed = Self::from_snapshot(
            crate::cow_tree::WorkspaceSnapshot {
                store: self.store.clone(),
                branch_id: self.branch_id,
                expected_head,
                expected_base: pinned.branch.base_layer_id,
                root: pinned.root,
                reader: pinned.reader.with_read_metrics_from(&metrics),
            },
            &spool,
            self.policy,
        )?;
        committed.state = WorkspaceState::Active;
        *self = committed;
        Ok(())
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
        let lease = self.acquire_lease(request.branch_id)?;
        let pinned = self.store.pin_branch(request.branch_id)?;
        let identity = crate::worker::WorkspaceIdentity {
            layer_stack_id: pinned.layer_stack.id,
            layer_stack_name: pinned.layer_stack.name.clone(),
            branch_name: pinned.branch.name.clone(),
        };
        let id = WorkspaceId::new();
        let state = self.runtime_root.join("workspaces").join(id.to_string());
        std::fs::create_dir_all(&state)?;
        let workspace = Workspace::from_snapshot(
            crate::cow_tree::WorkspaceSnapshot {
                store: self.store.clone(),
                branch_id: request.branch_id,
                expected_head: pinned.branch.head_commit_id,
                expected_base: pinned.branch.base_layer_id,
                root: pinned.root,
                reader: pinned.reader,
            },
            &state.join("spool"),
            crate::ResourcePolicy::default(),
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
            lease,
        ));
        let handle = match crate::projection::attach(&worker, self.daemon_mount_owner()?) {
            Ok(handle) => handle,
            Err(error) => {
                let _ = std::fs::remove_dir_all(&state);
                return Err(error);
            }
        };
        #[cfg(debug_assertions)]
        if std::env::var("LAYERFS_WORKSPACE_INJECT_POST_ATTACH_FAILURE").as_deref() == Ok("1") {
            drop(handle);
            std::fs::remove_dir_all(&state)?;
            return Err(WorkspaceError::InvalidPlacement);
        }
        *worker
            .projection_handle
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)? = Some(handle);
        let (create_read, cache_rows, cache_bytes) = worker
            .workspace
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .reader
            .take_create_metrics()?;
        if matches!(
            &request.placement,
            crate::WorkspacePlacement::Container { .. }
        ) {
            layerfs_layerstack_store::note_workspace_create_snapshot(
                create_read,
                cache_rows,
                cache_bytes,
            )?;
        }
        let session = session(&worker)?;
        self.sessions
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .insert(id, crate::registry::SessionRecord::Active(worker));
        Ok(session)
    }

    pub fn commit_workspace_session(
        &self,
        id: WorkspaceId,
    ) -> WorkspaceResult<WorkspaceCommitResult> {
        let worker = self.worker(id)?;
        let _timing = layerfs_layerstack_store::begin_workspace_commit(match worker.projection {
            WorkspaceProjection::Fuse => layerfs_layerstack_store::CaptureMode::Live,
            WorkspaceProjection::Materialize => layerfs_layerstack_store::CaptureMode::Materialized,
        })?;
        if worker.has_executions()? {
            return Ok(WorkspaceCommitResult::Busy);
        }
        let commit_read_before = worker
            .workspace
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .reader
            .read_metrics_snapshot()?;
        let started = Instant::now();
        let paused = crate::projection::pause(&worker);
        layerfs_layerstack_store::note_workspace_commit_phase(
            WorkspaceCommitPhase::PauseFence,
            elapsed_ns(started),
        );
        paused?;
        if let Err(error) = crate::projection::record_write_metrics(&worker) {
            let _ = crate::projection::resume(&worker);
            return Err(error);
        }
        let started = Instant::now();
        let quiesced = worker.wait_for_writers().and_then(|()| worker.quiesce());
        layerfs_layerstack_store::note_workspace_commit_phase(
            WorkspaceCommitPhase::Quiesce,
            elapsed_ns(started),
        );
        let _quiesced = match quiesced {
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
            let started = Instant::now();
            let captured = crate::projection::capture(&worker);
            layerfs_layerstack_store::note_workspace_commit_phase(
                WorkspaceCommitPhase::Capture,
                elapsed_ns(started),
            );
            captured?;
            let mut workspace = worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?;
            let previous_head = workspace.expected_head;
            let committed = match workspace.commit() {
                Ok((outcome, transition)) => Ok((
                    WorkspaceCommitResult::from_outcome(outcome, previous_head),
                    transition,
                )),
                Err(error) => WorkspaceError::from_commit(error)
                    .map(|result| (result, CommitTransition::Rebased)),
            };
            let commit_read_after = workspace.reader.read_metrics_snapshot()?;
            layerfs_layerstack_store::note_workspace_commit_reads(
                commit_read_before,
                commit_read_after,
            )?;
            committed
        })();
        let presentation = match &result {
            Ok((
                WorkspaceCommitResult::Created { .. } | WorkspaceCommitResult::UpToDate { .. },
                transition,
            )) => match transition {
                CommitTransition::Rebased => {
                    let started = Instant::now();
                    let resumed = crate::projection::resume(&worker);
                    layerfs_layerstack_store::note_workspace_commit_phase(
                        WorkspaceCommitPhase::Resume,
                        elapsed_ns(started),
                    );
                    resumed
                }
                CommitTransition::RebasedRefresh => {
                    let started = Instant::now();
                    let refreshed = crate::projection::refresh(&worker, self.daemon_mount_owner()?);
                    layerfs_layerstack_store::note_workspace_commit_phase(
                        WorkspaceCommitPhase::Resume,
                        elapsed_ns(started),
                    );
                    refreshed
                }
            },
            _ => {
                let started = Instant::now();
                let resumed = crate::projection::resume(&worker);
                layerfs_layerstack_store::note_workspace_commit_phase(
                    WorkspaceCommitPhase::Resume,
                    elapsed_ns(started),
                );
                resumed
            }
        };
        if let Err(error) = presentation {
            let _ = crate::projection::end(&worker);
            return Err(error);
        }
        match result {
            Ok((result, _)) => Ok(result),
            Err(WorkspaceError::WorkspaceBusy) => Ok(WorkspaceCommitResult::Busy),
            Err(error) => Err(error),
        }
    }

    pub fn edit_workspace_file_range(&self, edit: WorkspaceFileRangeEdit) -> WorkspaceResult<()> {
        self.edit_workspace_file_ranges(vec![edit])
    }

    pub fn edit_workspace_file_ranges(
        &self,
        edits: Vec<WorkspaceFileRangeEdit>,
    ) -> WorkspaceResult<()> {
        let first = edits.first().ok_or(WorkspaceError::InvalidExecution)?;
        if edits
            .iter()
            .any(|edit| edit.workspace_id != first.workspace_id || edit.path != first.path)
        {
            return Err(WorkspaceError::InvalidExecution);
        }
        let workspace_id = first.workspace_id;
        let path = first.path.clone();
        let worker = self.worker(workspace_id)?;
        if worker.has_executions()? {
            return Err(WorkspaceError::WorkspaceBusy);
        }
        crate::projection::pause(&worker)?;
        let quiesced = worker.wait_for_writers().and_then(|()| worker.quiesce());
        let _quiesced = match quiesced {
            Ok(value) => value,
            Err(error) => {
                crate::projection::resume(&worker)?;
                return Err(error);
            }
        };
        if let Err(error) = crate::projection::capture(&worker) {
            crate::projection::resume(&worker)?;
            return Err(error);
        }
        let result = (|| {
            let mut workspace = worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?;
            let node = lookup_path(&mut workspace, &path)?;
            workspace.edit_many(
                node,
                edits
                    .into_iter()
                    .map(|edit| (edit.start, edit.delete_len, edit.replacement))
                    .collect(),
            )?;
            Ok(())
        })();
        if result.is_err() {
            crate::projection::resume(&worker)?;
            return result;
        }
        crate::projection::refresh(&worker, self.daemon_mount_owner()?)
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
        let state = worker
            .workspace
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .state;
        if state == WorkspaceState::BrokenCleanup {
            return Err(WorkspaceError::InvalidPlacement);
        }
        crate::projection::pause(&worker)?;
        let _quiesced = match worker.quiesce() {
            Ok(quiesced) => quiesced,
            Err(error) => {
                crate::projection::resume(&worker)?;
                return Err(error);
            }
        };
        let validated = (|| {
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
            let workspace = worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?;
            let state = workspace
                .spool
                .parent()
                .ok_or(WorkspaceError::InvalidPlacement)?
                .to_owned();
            let discarded = mode == EndWorkspaceMode::Discard;
            Ok((state, discarded))
        })();
        let (state, discarded) = match validated {
            Ok(validated) => validated,
            Err(error) => {
                crate::projection::resume(&worker)?;
                return Err(error);
            }
        };
        crate::projection::record_read_metrics(&worker)?;
        if let Err(error) = crate::projection::end(&worker) {
            if let Ok(mut workspace) = worker.workspace.lock() {
                workspace.state = WorkspaceState::BrokenCleanup;
            }
            return Err(error);
        }
        let finalized = (|| {
            let mut workspace = worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?;
            match mode {
                EndWorkspaceMode::Discard => {
                    workspace.discard()?;
                    workspace.state = WorkspaceState::Ended;
                }
                EndWorkspaceMode::Clean => workspace.end_clean()?,
            }
            drop(workspace);
            if state.exists() {
                std::fs::remove_dir_all(state)?;
            }
            Ok(())
        })();
        if let Err(error) = finalized {
            if let Ok(mut workspace) = worker.workspace.lock() {
                workspace.state = WorkspaceState::BrokenCleanup;
            }
            return Err(error);
        }
        worker
            .lease
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .take();
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
        Ok(WorkspaceEndResult {
            session_id: id,
            discarded,
        })
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

fn lookup_path(workspace: &mut Workspace, path: &str) -> Result<crate::NodeId> {
    let mut node = crate::ROOT;
    for component in path
        .as_bytes()
        .split(|byte| *byte == b'/')
        .filter(|component| !component.is_empty())
    {
        node = workspace.lookup_node(node, component)?;
    }
    Ok(node)
}

fn elapsed_ns(started: Instant) -> u64 {
    started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
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
