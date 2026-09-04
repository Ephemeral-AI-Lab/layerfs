use crate::{
    worker::WorkspaceWorker, CreateWorkspaceSession, EndWorkspaceMode, Workspace,
    WorkspaceCommitResult, WorkspaceCommitStatus, WorkspaceDetail, WorkspaceDiff,
    WorkspaceEndResult, WorkspaceError, WorkspaceFileRangeEdit, WorkspaceId, WorkspacePlacement,
    WorkspaceProjection, WorkspaceResult, WorkspaceSession, WorkspaceSummary, Workspaces,
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
        #[cfg(feature = "test-instrumentation")]
        if consume_verification_fault(
            self.branch_id,
            VerificationFault::Candidate,
            self.spool_bytes,
        ) {
            crate::changes::inject_candidate_failure_once();
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
        let mut nodes = std::collections::HashMap::new();
        let mut canonical_nodes = std::collections::HashMap::new();
        let mut obsolete_spools = Vec::new();
        let mut retained_spool_nodes = std::collections::BTreeSet::new();
        let mut retained_spool_bytes = 0_u64;
        let mut retained_inline_bytes = 0_u64;
        let mut retained_piece_allocation_bytes = 0_u64;
        #[cfg(all(test, target_os = "macos"))]
        let mut diagnostic_nodes = 0_usize;
        #[cfg(all(test, target_os = "macos"))]
        rebase_memory_checkpoint("rebase-start", self.nodes.len(), 0, committed.nodes.len())?;
        // Keep the original Workspace intact until every fallible validation
        // succeeds, without cloning its entire node/path/spool graph up front.
        for (&id, old) in &self.nodes {
            if old.paths.is_empty() {
                if old.pins != 0 {
                    let mut retained = old.clone();
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
                            .saturating_add(pieces.logical_allocation_charge()?);
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
            #[cfg(test)]
            assert!(
                committed
                    .nodes
                    .values()
                    .filter(|node| !matches!(node.data, crate::cow_tree::Data::Directory(_)))
                    .count()
                    <= 1,
                "rebase must stage at most one non-directory node"
            );
            let mut rebased = if fresh_attr.kind == crate::cow_tree::Kind::Directory {
                // Ancestors remain available for subsequent path resolution.
                committed
                    .nodes
                    .get(&fresh_id)
                    .ok_or(StorageError::Integrity("committed Workspace node"))?
                    .clone()
            } else {
                let node = committed
                    .nodes
                    .remove(&fresh_id)
                    .ok_or(StorageError::Integrity("committed Workspace node"))?;
                if let Some(inode) = node.canonical {
                    committed.canonical_nodes.remove(&inode);
                }
                node
            };
            rebased.paths = old.paths.clone();
            rebased.pins = old.pins;
            if let Some(inode) = rebased.canonical {
                if canonical_nodes.insert(inode, id).is_some() {
                    return Err(StorageError::Integrity("committed Workspace inode"));
                }
            }
            if let crate::cow_tree::Data::File(crate::cow_tree::FileData::Edited {
                spool, ..
            }) = &old.data
            {
                if !self.open_spools.contains_key(&id) {
                    return Err(StorageError::Integrity("spool descriptor"));
                }
                obsolete_spools.push((id, spool.clone()));
            }
            nodes.insert(id, rebased);
            #[cfg(all(test, target_os = "macos"))]
            {
                diagnostic_nodes += 1;
                if diagnostic_nodes % 10000 == 0 {
                    rebase_memory_checkpoint(
                        "rebase-progress",
                        self.nodes.len(),
                        nodes.len(),
                        committed.nodes.len(),
                    )?;
                }
            }
        }
        #[cfg(all(test, target_os = "macos"))]
        rebase_memory_checkpoint(
            "rebase-validated",
            self.nodes.len(),
            nodes.len(),
            committed.nodes.len(),
        )?;
        self.open_spools
            .retain(|node, _| retained_spool_nodes.contains(node));
        for (node, spool) in obsolete_spools {
            self.remove_spool_if_exists(node, &spool)?;
        }
        self.reader = committed.reader.clone();
        self.expected_head = expected_head;
        self.expected_base = committed.expected_base;
        self.base_root = committed.base_root;
        self.base_inodes = committed.base_inodes;
        self.nodes = nodes;
        self.canonical_nodes = canonical_nodes;
        self.spool_bytes = retained_spool_bytes;
        self.spool_bytes_peak = retained_spool_bytes;
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
        committed.physical_spool = std::mem::take(&mut self.physical_spool);
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
    #[cfg(feature = "test-instrumentation")]
    pub fn verification_workspace_state(
        &self,
        id: WorkspaceId,
    ) -> WorkspaceResult<VerificationWorkspaceState> {
        let worker = self.worker(id)?;
        let workspace = worker
            .workspace
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        let (physical_current, physical_peak, physical_errors, physical_observations) =
            workspace.physical_spool.borrow().snapshot();
        Ok(VerificationWorkspaceState {
            spool_bytes: workspace.spool_bytes,
            spool_peak_bytes: workspace.spool_bytes_peak,
            physical_spool_allocated_bytes: physical_current,
            physical_spool_peak_bytes: physical_peak,
            physical_spool_observation_errors: physical_errors,
            physical_spool_observation_count: physical_observations,
            mutation_generation: workspace.mutation_generation,
            open_spool_files: workspace.open_spools.len(),
        })
    }

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
        self.commit_workspace_session_with_status(id)
            .map(|status| status.result)
    }

    pub fn commit_workspace_session_with_status(
        &self,
        id: WorkspaceId,
    ) -> WorkspaceResult<WorkspaceCommitStatus> {
        let worker = self.worker(id)?;
        let _timing = layerfs_layerstack_store::begin_workspace_commit(match worker.projection {
            WorkspaceProjection::Fuse => layerfs_layerstack_store::CaptureMode::Live,
            WorkspaceProjection::Materialize => layerfs_layerstack_store::CaptureMode::Materialized,
        })?;
        if worker.has_executions()? {
            return Ok(WorkspaceCommitStatus {
                result: WorkspaceCommitResult::Busy,
                presentation_failed: false,
            });
        }
        if worker
            .workspace
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .presentation_failed
        {
            return Err(WorkspaceError::InvalidExecution);
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
                if let Err(error) = crate::projection::resume(&worker) {
                    let _ = crate::projection::end(&worker);
                    worker
                        .workspace
                        .lock()
                        .map_err(|_| WorkspaceError::WorkspaceBusy)?
                        .presentation_failed = true;
                    return Err(error);
                }
                return Ok(WorkspaceCommitStatus {
                    result: WorkspaceCommitResult::Busy,
                    presentation_failed: false,
                });
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
            workspace.note_commit_edit_state()?;
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
        #[cfg(feature = "test-instrumentation")]
        if matches!(&result, Ok((WorkspaceCommitResult::Created { .. }, _)))
            && consume_verification_fault(
                worker.request.branch_id,
                VerificationFault::PresentationResume,
                0,
            )
        {
            crate::projection::inject_resume_failure_once();
        }
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
            worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?
                .presentation_failed = true;
            return match result {
                Ok((result @ WorkspaceCommitResult::Created { .. }, _))
                | Ok((result @ WorkspaceCommitResult::UpToDate { .. }, _)) => {
                    Ok(WorkspaceCommitStatus {
                        result,
                        presentation_failed: true,
                    })
                }
                _ => Err(error),
            };
        }
        match result {
            Ok((result, _)) => Ok(WorkspaceCommitStatus {
                result,
                presentation_failed: false,
            }),
            Err(WorkspaceError::WorkspaceBusy) => Ok(WorkspaceCommitStatus {
                result: WorkspaceCommitResult::Busy,
                presentation_failed: false,
            }),
            Err(error) => Err(error),
        }
    }

    pub fn recover_workspace_presentation(
        &self,
        id: WorkspaceId,
    ) -> WorkspaceResult<WorkspaceSession> {
        let worker = self.worker(id)?;
        if worker.has_executions()?
            || !worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?
                .presentation_failed
        {
            return Err(WorkspaceError::InvalidExecution);
        }
        crate::projection::pause(&worker)?;
        let _quiesced = worker.quiesce()?;
        crate::projection::end(&worker)?;
        let handle = crate::projection::attach(&worker, self.daemon_mount_owner()?)?;
        *worker
            .projection_handle
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)? = Some(handle);
        let mut workspace = worker
            .workspace
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        workspace.presentation_failed = false;
        Ok(session_locked(&worker, &workspace))
    }

    pub fn edit_workspace_file_range(&self, edit: WorkspaceFileRangeEdit) -> WorkspaceResult<()> {
        self.edit_workspace_file_ranges(vec![edit])
    }

    pub fn start_workspace_resource_sample(
        &self,
        id: WorkspaceId,
    ) -> WorkspaceResult<layerfs_daemon::ResourceSampleClock> {
        let worker = self.worker(id)?;
        let WorkspacePlacement::Container { container_id, .. } = &worker.request.placement else {
            return Err(WorkspaceError::InvalidPlacement);
        };
        let owner = self
            .daemon_mount_owner()?
            .filter(|owner| owner.accepts(container_id))
            .ok_or(WorkspaceError::InvalidPlacement)?;
        owner.start_resource_sample(id).map_err(Into::into)
    }

    pub fn finish_workspace_resource_sample(
        &self,
        id: WorkspaceId,
        t0_unix_ns: u64,
        t3_unix_ns: u64,
        uncertainty_ns: u64,
    ) -> WorkspaceResult<layerfs_daemon::protocol::CgroupResourceSample> {
        let owner = self
            .daemon_mount_owner()?
            .ok_or(WorkspaceError::InvalidPlacement)?;
        owner
            .finish_resource_sample(id, t0_unix_ns, t3_unix_ns, uncertainty_ns)
            .map_err(Into::into)
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
            if workspace.nodes[&node].pins != 0 {
                return Err(WorkspaceError::WorkspaceBusy);
            }
            let checkpoint = workspace.edit_checkpoint(node)?;
            let result = workspace.edit_many(
                node,
                edits
                    .into_iter()
                    .map(|edit| (edit.start, edit.delete_len, edit.replacement))
                    .collect(),
            );
            Ok((result, checkpoint, node))
        })();
        let (result, checkpoint, node) = match result {
            Ok(value) => value,
            Err(error) => {
                crate::projection::resume(&worker)?;
                return Err(error);
            }
        };
        if let Err(error) = result {
            worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?
                .restore_edit(checkpoint)?;
            crate::projection::resume(&worker)?;
            return Err(error.into());
        }
        match crate::projection::refresh_file(&worker, node, self.daemon_mount_owner()?) {
            Ok(()) => Ok(()),
            Err(refresh_error) => {
                let restored = worker
                    .workspace
                    .lock()
                    .map_err(|_| WorkspaceError::WorkspaceBusy)?
                    .restore_edit(checkpoint);
                if restored.is_err()
                    || crate::projection::refresh_file(&worker, node, self.daemon_mount_owner()?)
                        .is_err()
                {
                    if let Ok(mut workspace) = worker.workspace.lock() {
                        workspace.presentation_failed = true;
                    }
                }
                Err(refresh_error)
            }
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
        let workspace = worker
            .workspace
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        let state = workspace.state;
        let presentation_failed = workspace.presentation_failed;
        drop(workspace);
        if state == WorkspaceState::BrokenCleanup
            || (presentation_failed && mode == EndWorkspaceMode::Clean)
        {
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

#[cfg(all(test, target_os = "macos"))]
fn rebase_memory_checkpoint(
    phase: &str,
    old_nodes: usize,
    new_nodes: usize,
    staged_nodes: usize,
) -> Result<()> {
    let Ok(root) = std::env::var("LAYERFS_REBASE_DIAGNOSTIC_ROOT") else {
        return Ok(());
    };
    let output = std::process::Command::new("/bin/ps")
        .args(["-p", &std::process::id().to_string(), "-o", "rss="])
        .output()?;
    if !output.status.success() {
        return Err(StorageError::Integrity("diagnostic ps failed"));
    }
    let rss = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .map_err(|_| StorageError::Integrity("diagnostic RSS"))?
        * 1024;
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let elapsed = START.get_or_init(Instant::now).elapsed().as_nanos();
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::path::Path::new(&root).join("memory.jsonl"))?;
    writeln!(file, "{{\"phase\":\"{phase}\",\"pid\":{},\"elapsed_ns\":{elapsed},\"old_nodes\":{old_nodes},\"new_nodes\":{new_nodes},\"staged_nodes\":{staged_nodes},\"rss_bytes\":{rss}}}", std::process::id())?;
    file.flush()?;
    if matches!(phase, "after-load" | "after-rebase") {
        for (tool, args) in [
            ("vmmap", vec!["-summary"]),
            ("heap", vec!["-s", "--noContent"]),
        ] {
            let output = std::process::Command::new(format!("/usr/bin/{tool}"))
                .args(args)
                .arg(std::process::id().to_string())
                .output()?;
            std::fs::write(
                std::path::Path::new(&root).join(format!("{phase}-{tool}.txt")),
                &output.stdout,
            )?;
            std::fs::write(
                std::path::Path::new(&root).join(format!("{phase}-{tool}.stderr.txt")),
                &output.stderr,
            )?;
            if !output.status.success() {
                return Err(StorageError::Integrity(
                    "diagnostic native memory tool failed",
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use layerfs_layerstack_store::{
        EntityName, LayerStackInitialization, LayerStackStore, LocalForkSource,
    };

    fn fixture(
        label: &str,
    ) -> (
        std::path::PathBuf,
        Workspaces,
        layerfs_layerstack_store::BranchId,
        LayerStackStore,
    ) {
        let root = std::env::temp_dir().join(format!(
            "layerfs-lifecycle-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("source");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("file"), b"abcdef").unwrap();
        let store = LayerStackStore::create(root.join("store.sqlite")).unwrap();
        let layer = store
            .initialize_layerstack(
                EntityName::new("project").unwrap(),
                LayerStackInitialization::Directory(source),
            )
            .unwrap()
            .genesis_layer_id;
        let branch = store
            .fork_branch(
                EntityName::new("main").unwrap(),
                LocalForkSource::Layer { layer_id: layer },
            )
            .unwrap();
        let workspaces = Workspaces::new(root.join("runtime"), store.clone()).unwrap();
        (root, workspaces, branch, store)
    }

    fn session(
        root: &std::path::Path,
        workspaces: &Workspaces,
        branch: layerfs_layerstack_store::BranchId,
    ) -> WorkspaceSession {
        workspaces
            .create_workspace_session(CreateWorkspaceSession {
                branch_id: branch,
                placement: crate::WorkspacePlacement::Host {
                    root: root.join("mount"),
                },
                projection: Some(WorkspaceProjection::Materialize),
            })
            .unwrap()
    }

    fn prepend(workspaces: &Workspaces, id: WorkspaceId) -> WorkspaceResult<()> {
        workspaces.edit_workspace_file_range(WorkspaceFileRangeEdit {
            workspace_id: id,
            path: "file".into(),
            start: 0,
            delete_len: 0,
            replacement: crate::WorkspaceFileReplacement::Inline(b"P".to_vec()),
        })
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "Explicit retained-Store diagnostic; no workload or preparation"]
    fn diagnose_rebase_retained_committed_store() {
        let root = std::path::PathBuf::from(
            std::env::var("LAYERFS_REBASE_DIAGNOSTIC_ROOT").expect("diagnostic root"),
        );
        rebase_memory_checkpoint("before-open", 0, 0, 0).unwrap();
        let store = LayerStackStore::connect(root.join("store.sqlite")).unwrap();
        let branch = std::fs::read_to_string(root.join("branch-id"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let mut workspace = Workspace::open(store.clone(), branch, root.join("spool")).unwrap();
        rebase_memory_checkpoint("after-open", workspace.nodes.len(), 0, 0).unwrap();
        let wide = workspace.lookup(crate::ROOT, b"wide").unwrap().node;
        let regular = workspace.lookup(crate::ROOT, b"regular").unwrap().node;
        let mut spine = workspace.lookup(crate::ROOT, b"spine").unwrap().node;
        for depth in 1..=128 {
            spine = workspace
                .lookup(spine, format!("d{depth:03}").as_bytes())
                .unwrap()
                .node;
        }
        let mut loaded = 0;
        for shard in 0..500 {
            for ordinal in 0..64 {
                workspace
                    .lookup(wide, format!("s{shard:03}-f{ordinal:03}.dat").as_bytes())
                    .unwrap();
                loaded += 1;
            }
            let directory = workspace
                .lookup(regular, format!("s{shard:03}").as_bytes())
                .unwrap()
                .node;
            for ordinal in 64..199 {
                workspace
                    .lookup(directory, format!("f{ordinal:03}.dat").as_bytes())
                    .unwrap();
                loaded += 1;
            }
            workspace
                .lookup(spine, format!("s{shard:03}.dat").as_bytes())
                .unwrap();
            loaded += 1;
            if loaded % 10000 == 0 {
                rebase_memory_checkpoint("load-progress", workspace.nodes.len(), 0, 0).unwrap();
            }
        }
        assert_eq!(loaded, 100000);
        rebase_memory_checkpoint("after-load", workspace.nodes.len(), 0, 0).unwrap();
        workspace
            .rebase_committed(CommitOutcome::UpToDate {
                root_id: workspace.base_root,
            })
            .unwrap();
        rebase_memory_checkpoint("after-rebase", workspace.nodes.len(), 0, 0).unwrap();
        drop(workspace);
        drop(store);
        rebase_memory_checkpoint("after-drop", 0, 0, 0).unwrap();
    }

    #[test]
    fn rebase_streams_nodes_preserving_identity_aliases_and_pinned_spools() {
        let (root, workspaces, branch, store) = fixture("streamed-rebase");
        drop(workspaces);
        let mut workspace = Workspace::open(store.clone(), branch, root.join("spool")).unwrap();
        let directory = workspace.mkdir(crate::ROOT, b"group", 0o750).unwrap().node;
        let mut files = Vec::new();
        for index in 0..32 {
            let name = format!("f{index:02}");
            let node = workspace
                .create_file(directory, name.as_bytes(), 0o640)
                .unwrap()
                .node;
            workspace.write(node, 0, b"before").unwrap();
            files.push((name, node));
        }
        workspace.link(files[0].1, directory, b"alias").unwrap();
        workspace.commit().unwrap();
        let inodes = files
            .iter()
            .map(|(_, node)| workspace.nodes[node].canonical.unwrap())
            .collect::<Vec<_>>();
        for (index, (_, node)) in files.iter().enumerate() {
            workspace
                .write(*node, 0, format!("after-{index:02}").as_bytes())
                .unwrap();
            workspace.set_mtime(*node, 1700000007, 23).unwrap();
        }
        let orphan = workspace
            .create_file(directory, b"held", 0o600)
            .unwrap()
            .node;
        workspace.write(orphan, 0, b"pinned-data").unwrap();
        workspace.pin(orphan, false).unwrap();
        workspace.unlink(directory, b"held", false).unwrap();
        let (outcome, transition) = workspace.commit().unwrap();
        assert!(matches!(outcome, CommitOutcome::Committed { .. }));
        assert_eq!(transition, CommitTransition::Rebased);
        assert_eq!(
            workspace.lookup(crate::ROOT, b"group").unwrap().node,
            directory
        );
        for (index, (name, node)) in files.iter().enumerate() {
            assert_eq!(
                workspace.lookup(directory, name.as_bytes()).unwrap().node,
                *node
            );
            assert_eq!(workspace.nodes[node].canonical, Some(inodes[index]));
            assert_eq!(
                workspace.read(*node, 0, 64).unwrap(),
                format!("after-{index:02}").as_bytes()
            );
            let attr = workspace.attr(*node).unwrap();
            assert_eq!(
                (attr.mode, attr.mtime_seconds, attr.mtime_nanoseconds),
                (0o640, 1700000007, 23)
            );
            assert!(matches!(
                workspace.nodes[node].data,
                crate::cow_tree::Data::File(crate::cow_tree::FileData::Base { .. })
            ));
        }
        assert_eq!(
            workspace.lookup(directory, b"alias").unwrap().node,
            files[0].1
        );
        assert_eq!(workspace.attr(files[0].1).unwrap().links, 2);
        assert!(workspace.nodes[&orphan].paths.is_empty());
        assert_eq!(workspace.nodes[&orphan].pins, 1);
        assert_eq!(workspace.read(orphan, 0, 64).unwrap(), b"pinned-data");
        assert_eq!(workspace.spool_bytes, 11);
        assert_eq!(workspace.open_spools.len(), 1);
        assert!(workspace.dirty.is_empty() && workspace.mutation_paths.is_empty());
        assert_eq!(workspace.mutation_generation, 0);
        workspace.unpin(orphan).unwrap();
        assert_eq!(workspace.spool_bytes, 0);
        assert!(workspace.open_spools.is_empty());
        drop(workspace);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg(all(target_os = "linux", feature = "host-fuse"))]
    fn live_fuse_owner_edit_invalidates_warm_pages_and_rolls_back_resume_failure() {
        if std::env::var_os("LAYERFS_LIVE_FUSE").is_none() {
            return;
        }
        use std::os::unix::fs::MetadataExt;
        let (root, workspaces, branch, store) = fixture("live-invalidation");
        let session = workspaces
            .create_workspace_session(CreateWorkspaceSession {
                branch_id: branch,
                placement: crate::WorkspacePlacement::Host {
                    root: root.join("mount"),
                },
                projection: Some(WorkspaceProjection::Fuse),
            })
            .unwrap();
        let file = root.join("mount/file");
        let inode = std::fs::metadata(&file).unwrap().ino();
        let branch_before = store.pin_branch(branch).unwrap().root;
        assert_eq!(std::fs::read(&file).unwrap(), b"abcdef");
        crate::projection::inject_resume_failure_once();
        assert!(prepend(&workspaces, session.id).is_err());
        assert_eq!(std::fs::read(&file).unwrap(), b"abcdef");
        assert_eq!(store.pin_branch(branch).unwrap().root, branch_before);
        assert!(
            !workspaces
                .worker(session.id)
                .unwrap()
                .workspace
                .lock()
                .unwrap()
                .presentation_failed
        );
        prepend(&workspaces, session.id).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"Pabcdef");
        assert_eq!(std::fs::metadata(&file).unwrap().ino(), inode);
        assert_eq!(std::fs::metadata(&file).unwrap().len(), 7);
        workspaces
            .edit_workspace_file_range(WorkspaceFileRangeEdit {
                workspace_id: session.id,
                path: "file".into(),
                start: 1,
                delete_len: 6,
                replacement: crate::WorkspaceFileReplacement::Inline(b"Q".to_vec()),
            })
            .unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"PQ");
        assert_eq!(std::fs::metadata(&file).unwrap().ino(), inode);
        workspaces
            .end_workspace_session(session.id, EndWorkspaceMode::Discard)
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_projection_refresh_restores_exact_state_and_retry_once() {
        let (root, workspaces, branch, store) = fixture("refresh-rollback");
        let session = session(&root, &workspaces, branch);
        let worker = workspaces.worker(session.id).unwrap();
        let before = {
            let workspace = worker.workspace.lock().unwrap();
            (
                workspace.nodes.clone(),
                workspace.dirty.clone(),
                workspace.mutation_generation,
                workspace.mutation_paths.clone(),
                workspace.spool_bytes,
                workspace.inline_bytes,
                workspace.piece_allocation_bytes,
            )
        };
        let branch_before = store.pin_branch(branch).unwrap().root;
        crate::projection::inject_refresh_failure_once();
        assert!(prepend(&workspaces, session.id).is_err());
        {
            let workspace = worker.workspace.lock().unwrap();
            assert_eq!(workspace.nodes, before.0);
            assert_eq!(workspace.dirty, before.1);
            assert_eq!(workspace.mutation_generation, before.2);
            assert_eq!(workspace.mutation_paths, before.3);
            assert_eq!(workspace.spool_bytes, before.4);
            assert_eq!(workspace.inline_bytes, before.5);
            assert_eq!(workspace.piece_allocation_bytes, before.6);
        }
        assert_eq!(std::fs::read(root.join("mount/file")).unwrap(), b"abcdef");
        assert_eq!(store.pin_branch(branch).unwrap().root, branch_before);
        assert!(worker.projection_handle.lock().unwrap().is_some());
        prepend(&workspaces, session.id).unwrap();
        assert_eq!(std::fs::read(root.join("mount/file")).unwrap(), b"Pabcdef");
        workspaces
            .end_workspace_session(session.id, EndWorkspaceMode::Discard)
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pinned_target_rejects_owner_edit_without_state_change() {
        let (root, workspaces, branch, store) = fixture("pinned-edit");
        let session = session(&root, &workspaces, branch);
        let worker = workspaces.worker(session.id).unwrap();
        let node = {
            let mut workspace = worker.workspace.lock().unwrap();
            let node = lookup_path(&mut workspace, "file").unwrap();
            workspace.pin(node, false).unwrap();
            node
        };
        let before = {
            let workspace = worker.workspace.lock().unwrap();
            (
                workspace.nodes.clone(),
                workspace.dirty.clone(),
                workspace.mutation_generation,
                workspace.mutation_paths.clone(),
                workspace.spool_bytes,
                workspace.inline_bytes,
                workspace.piece_allocation_bytes,
                store.pin_branch(branch).unwrap().root,
            )
        };
        assert!(matches!(
            prepend(&workspaces, session.id),
            Err(WorkspaceError::WorkspaceBusy)
        ));
        {
            let workspace = worker.workspace.lock().unwrap();
            assert_eq!(workspace.nodes, before.0);
            assert_eq!(workspace.dirty, before.1);
            assert_eq!(workspace.mutation_generation, before.2);
            assert_eq!(workspace.mutation_paths, before.3);
            assert_eq!(workspace.spool_bytes, before.4);
            assert_eq!(workspace.inline_bytes, before.5);
            assert_eq!(workspace.piece_allocation_bytes, before.6);
        }
        assert_eq!(store.pin_branch(branch).unwrap().root, before.7);
        assert!(worker.projection_handle.lock().unwrap().is_some());
        assert_eq!(std::fs::read(root.join("mount/file")).unwrap(), b"abcdef");
        worker.workspace.lock().unwrap().unpin(node).unwrap();
        workspaces
            .end_workspace_session(session.id, EndWorkspaceMode::Clean)
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn busy_commit_with_resume_failure_requires_explicit_presentation_recovery() {
        let (root, workspaces, branch, _) = fixture("busy-resume");
        let session = session(&root, &workspaces, branch);
        let worker = workspaces.worker(session.id).unwrap();
        worker.note_writer(true).unwrap();
        crate::projection::inject_resume_failure_once();
        assert!(matches!(
            workspaces.commit_workspace_session(session.id),
            Err(WorkspaceError::Io(_))
        ));
        assert!(worker.workspace.lock().unwrap().presentation_failed);
        worker.note_writer(false).unwrap();
        assert_eq!(
            workspaces
                .recover_workspace_presentation(session.id)
                .unwrap()
                .state,
            WorkspaceState::Active
        );
        workspaces
            .end_workspace_session(session.id, EndWorkspaceMode::Clean)
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn callback_and_execution_reject_owner_edit_without_state_change() {
        let (root, workspaces, branch, store) = fixture("admission-busy");
        let session = session(&root, &workspaces, branch);
        let worker = workspaces.worker(session.id).unwrap();
        let before = {
            let workspace = worker.workspace.lock().unwrap();
            (
                workspace.nodes.clone(),
                workspace.dirty.clone(),
                workspace.mutation_generation,
                workspace.spool_bytes,
                workspace.inline_bytes,
                workspace.piece_allocation_bytes,
                store.pin_branch(branch).unwrap().root,
            )
        };
        let callback = worker.enter_callback().unwrap();
        assert!(matches!(
            prepend(&workspaces, session.id),
            Err(WorkspaceError::WorkspaceBusy)
        ));
        drop(callback);
        worker.note_execution(true).unwrap();
        assert!(matches!(
            prepend(&workspaces, session.id),
            Err(WorkspaceError::WorkspaceBusy)
        ));
        worker.note_execution(false).unwrap();
        {
            let workspace = worker.workspace.lock().unwrap();
            assert_eq!(workspace.nodes, before.0);
            assert_eq!(workspace.dirty, before.1);
            assert_eq!(workspace.mutation_generation, before.2);
            assert_eq!(workspace.spool_bytes, before.3);
            assert_eq!(workspace.inline_bytes, before.4);
            assert_eq!(workspace.piece_allocation_bytes, before.5);
        }
        assert_eq!(store.pin_branch(branch).unwrap().root, before.6);
        assert_eq!(std::fs::read(root.join("mount/file")).unwrap(), b"abcdef");
        assert!(worker.projection_handle.lock().unwrap().is_some());
        workspaces
            .end_workspace_session(session.id, EndWorkspaceMode::Clean)
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(feature = "test-instrumentation")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerificationFault {
    Candidate,
    PresentationResume,
    ShortAppend,
    NoSpace,
}
#[cfg(feature = "test-instrumentation")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerificationWorkspaceState {
    pub spool_bytes: u64,
    pub spool_peak_bytes: u64,
    pub physical_spool_allocated_bytes: Option<u64>,
    pub physical_spool_peak_bytes: Option<u64>,
    pub physical_spool_observation_errors: u64,
    pub physical_spool_observation_count: u64,
    pub mutation_generation: u64,
    pub open_spool_files: usize,
}
#[cfg(feature = "test-instrumentation")]
#[derive(Clone, Debug)]
pub struct VerificationFaultReceipt {
    pub branch: layerfs_layerstack_store::BranchId,
    pub fault: VerificationFault,
    pub hit_count: u64,
    pub spool_bytes_before: u64,
}
#[cfg(feature = "test-instrumentation")]
static VERIFICATION_FAULT: std::sync::Mutex<Option<VerificationFaultReceipt>> =
    std::sync::Mutex::new(None);
#[cfg(feature = "test-instrumentation")]
static VERIFICATION_FAULT_ARMED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
#[cfg(feature = "test-instrumentation")]
pub fn arm_verification_fault(
    branch: layerfs_layerstack_store::BranchId,
    fault: VerificationFault,
) -> WorkspaceResult<()> {
    let mut state = VERIFICATION_FAULT
        .lock()
        .map_err(|_| WorkspaceError::WorkspaceBusy)?;
    if state.is_some() {
        return Err(WorkspaceError::WorkspaceBusy);
    }
    *state = Some(VerificationFaultReceipt {
        branch,
        fault,
        hit_count: 0,
        spool_bytes_before: 0,
    });
    VERIFICATION_FAULT_ARMED.store(true, std::sync::atomic::Ordering::Release);
    Ok(())
}
#[cfg(feature = "test-instrumentation")]
pub fn take_verification_fault_receipt() -> WorkspaceResult<Option<VerificationFaultReceipt>> {
    VERIFICATION_FAULT_ARMED.store(false, std::sync::atomic::Ordering::Release);
    Ok(VERIFICATION_FAULT
        .lock()
        .map_err(|_| WorkspaceError::WorkspaceBusy)?
        .take())
}
#[cfg(feature = "test-instrumentation")]
pub(crate) fn consume_verification_fault(
    branch: layerfs_layerstack_store::BranchId,
    fault: VerificationFault,
    spool_bytes: u64,
) -> bool {
    if !VERIFICATION_FAULT_ARMED.load(std::sync::atomic::Ordering::Acquire) {
        return false;
    }
    let Ok(mut state) = VERIFICATION_FAULT.lock() else {
        return false;
    };
    let Some(receipt) = state.as_mut() else {
        return false;
    };
    if receipt.branch != branch || receipt.fault != fault || receipt.hit_count != 0 {
        return false;
    }
    receipt.hit_count = 1;
    receipt.spool_bytes_before = spool_bytes;
    VERIFICATION_FAULT_ARMED.store(false, std::sync::atomic::Ordering::Release);
    true
}

#[cfg(all(test, feature = "test-instrumentation"))]
#[test]
fn verification_fault_scope_is_one_shot() {
    let branch = layerfs_layerstack_store::BranchId::new();
    let other = layerfs_layerstack_store::BranchId::new();
    assert!(!consume_verification_fault(
        branch,
        VerificationFault::ShortAppend,
        0
    ));
    arm_verification_fault(branch, VerificationFault::ShortAppend).unwrap();
    assert!(!consume_verification_fault(
        other,
        VerificationFault::ShortAppend,
        0
    ));
    assert!(std::thread::spawn(move || consume_verification_fault(
        branch,
        VerificationFault::ShortAppend,
        4096
    ))
    .join()
    .unwrap());
    assert!(!consume_verification_fault(
        branch,
        VerificationFault::ShortAppend,
        8192
    ));
    let receipt = take_verification_fault_receipt().unwrap().unwrap();
    assert_eq!((receipt.hit_count, receipt.spool_bytes_before), (1, 4096));
    assert!(take_verification_fault_receipt().unwrap().is_none());
}
