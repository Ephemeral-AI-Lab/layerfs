use crate::{
    worker::WorkspaceWorker, CreateWorkspaceSession, EndWorkspaceMode, Workspace,
    WorkspaceCommitResult, WorkspaceCommitStatus, WorkspaceDetail, WorkspaceDiff,
    WorkspaceEndResult, WorkspaceError, WorkspaceFileRangeEdit, WorkspaceId, WorkspacePlacement,
    WorkspaceProjection, WorkspaceResult, WorkspaceSession, WorkspaceSummary, Workspaces,
};
use layerfs_layerstack_store::{
    CommitOutcome, ObjectBuffer, Result, StoreError as StorageError, WorkspaceCommitPhase,
};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommitTransition {
    Checkpointed,
    Refreshed,
    InstallationFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceState {
    Active,
    Committed,
    Discarded,
    Ended,
    BrokenCleanup,
}

#[cfg(test)]
thread_local! {
    static INJECT_INSTALL_FAILURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static INJECT_PARTIAL_INSTALL_FAILURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

impl Workspace {
    pub(crate) fn commit(&mut self) -> Result<(CommitOutcome, CommitTransition)> {
        if let Some((outcome, expected_base, refresh)) = self.pending_publication {
            let transition = self.transition_committed(outcome, expected_base, refresh)?;
            return Ok((outcome, transition));
        }
        if self.state != WorkspaceState::Active {
            return Err(StorageError::InvalidInput("workspace inactive"));
        }
        if let Some(mut resolution) = self.resolution.take() {
            if let Err(error) = resolution.invalidate_if_mutated(self) {
                self.resolution = Some(resolution);
                return Err(error);
            }
            if resolution.unresolved() != 0 {
                self.resolution = Some(resolution);
                return Err(StorageError::InvalidInput(
                    "unresolved reconciliation conflict",
                ));
            }
            let choices = match resolution.choices() {
                Ok(choices) => choices,
                Err(error) => {
                    self.resolution = Some(resolution);
                    return Err(error);
                }
            };
            let expected_base = resolution.prepared.current_layer_id;
            let candidate = match self.build_candidate(crate::changes::CandidatePurpose::Preview) {
                Ok(candidate) => candidate,
                Err(error) => {
                    self.resolution = Some(resolution);
                    return Err(error);
                }
            };
            let root = candidate.built.root_id;
            let outcome = self.store.commit_workspace_reconciliation(
                self.workspace_id,
                &resolution.prepared,
                candidate.built,
                &choices,
            );
            if outcome.is_err() {
                self.note_retained_stage(root);
                self.resolution = Some(resolution);
            }
            let outcome = outcome?;
            self.pending_stage = None;
            let transition = self.transition_committed(outcome, expected_base, true)?;
            return Ok((outcome, transition));
        }
        let mut branch = self
            .store
            .branch(self.branch_id)?
            .ok_or(StorageError::NotFound("Branch"))?;
        // Carry the frozen expected context into conditional publication. The
        // Store may observe a newer head, but only after retaining this complete
        // candidate stage for explicit retry/discard.
        branch.head_commit_id = self.expected_head;
        branch.base_layer_id = self.expected_base;
        #[cfg(feature = "test-instrumentation")]
        if consume_verification_fault(
            self.branch_id,
            VerificationFault::Candidate,
            self.live.spool_bytes,
        ) {
            crate::changes::inject_candidate_failure_once();
        }
        // Streaming admission occurs while constructing files, before the final
        // candidate is returned to the Store publication entrypoint.
        #[cfg(feature = "test-instrumentation")]
        layerfs_layerstack_store::verification_candidate(self.branch_id, 0);
        let generation = if let Some(remote) = &self.remote {
            remote
                .backing
                .lock()
                .map_err(|_| StorageError::Integrity("live backing lock"))?
                .frozen_generation()?
        } else {
            self.live.mutation_generation
        };
        let (candidate, admission) = if generation == 0 {
            (
                ObjectBuffer::new(&self.reader)?.finish(self.base_root, 0)?,
                self.store.workspace_admission(self.workspace_id)?,
            )
        } else {
            let prepared = self.build_candidate(crate::changes::CandidatePurpose::Commit)?;
            self.pending_checkpoint = Some(prepared.checkpoint);
            (
                prepared.built,
                prepared
                    .admission
                    .ok_or(StorageError::Integrity("Commit admission handoff"))?,
            )
        };
        let expected_base = self.expected_base;
        let root = candidate.root_id;
        let outcome = self.store.commit_workspace_candidate(
            self.workspace_id,
            &branch,
            self.base_root,
            expected_base,
            candidate,
            admission,
        );
        if outcome.is_err() {
            self.note_retained_stage(root);
        }
        let outcome = outcome?;
        self.pending_stage = None;
        let transition = self.transition_committed(outcome, expected_base, false)?;
        Ok((outcome, transition))
    }

    fn note_retained_stage(&mut self, candidate_root: layerfs_content::ObjectId) {
        self.pending_stage = match self.store.workspace_stage(self.workspace_id) {
            Ok(Some(stage)) => Some(stage.root_id),
            Ok(None) => None,
            // Preserve the operation's original error while conservatively
            // freezing mutation until explicit discard can resolve uncertainty.
            Err(_) => Some(candidate_root),
        };
        if self.pending_stage.is_none() {
            self.pending_checkpoint = None;
        }
    }

    fn transition_committed(
        &mut self,
        outcome: CommitOutcome,
        expected_base: layerfs_layerstack_store::LayerId,
        refresh: bool,
    ) -> Result<CommitTransition> {
        if self.remote.is_some() {
            if refresh {
                return Err(StorageError::InvalidInput(
                    "remote reconciliation installation",
                ));
            }
            self.pending_publication = Some((outcome, expected_base, false));
            return Ok(CommitTransition::Checkpointed);
        }
        if matches!(outcome, CommitOutcome::UpToDate { .. }) && self.live.mutation_generation == 0 {
            let started = Instant::now();
            self.retire_spool_segments();
            layerfs_layerstack_store::note_workspace_commit_phase(
                WorkspaceCommitPhase::Checkpoint,
                elapsed_ns(started),
            );
            self.pending_stage = None;
            return Ok(if refresh {
                CommitTransition::Refreshed
            } else {
                CommitTransition::Checkpointed
            });
        }
        // Publication already succeeded. Retain its immutable identity until
        // installation and cleanup finish so retry cannot create another Commit.
        self.pending_publication = Some((outcome, expected_base, refresh));
        let started = Instant::now();
        #[cfg(test)]
        if INJECT_INSTALL_FAILURE.with(|inject| inject.replace(false)) {
            self.presentation_failed = true;
            return Ok(CommitTransition::InstallationFailed);
        }
        let installed = if refresh {
            self.refresh_reconciled(outcome, expected_base)
        } else {
            self.install_checkpoint(outcome, expected_base)
        };
        layerfs_layerstack_store::note_workspace_commit_phase(
            WorkspaceCommitPhase::Checkpoint,
            elapsed_ns(started),
        );
        if installed.is_err() {
            self.presentation_failed = true;
            return Ok(CommitTransition::InstallationFailed);
        }
        self.pending_publication = None;
        Ok(if refresh {
            CommitTransition::Refreshed
        } else {
            CommitTransition::Checkpointed
        })
    }

    fn refresh_reconciled(
        &mut self,
        outcome: CommitOutcome,
        expected_base: layerfs_layerstack_store::LayerId,
    ) -> Result<()> {
        let (expected_head, root) = match outcome {
            CommitOutcome::Committed {
                commit_id, root_id, ..
            } => (Some(commit_id), root_id),
            CommitOutcome::UpToDate { root_id } => (self.expected_head, root_id),
        };
        let mut committed = Self::from_snapshot(
            crate::cow_tree::WorkspaceSnapshot {
                store: self.store.clone(),
                workspace_id: self.workspace_id,
                branch_id: self.branch_id,
                expected_head,
                expected_base,
                root,
                reader: self
                    .store
                    .snapshot_reader(root)
                    .with_read_metrics_from(&self.reader),
            },
            &self.spool,
            self.live.policy,
        )?;
        // Held read plans retain old segments and their admission charge across refresh.
        let mut backing = std::mem::take(&mut self.backing);
        self.clear_spool()?;
        backing.current = None;
        backing.metrics = Default::default();
        committed.backing = backing;
        *self = committed;
        self.retire_spool_segments();
        Ok(())
    }

    fn install_checkpoint(
        &mut self,
        outcome: CommitOutcome,
        expected_base: layerfs_layerstack_store::LayerId,
    ) -> Result<()> {
        let (head, root) = match outcome {
            CommitOutcome::Committed {
                commit_id, root_id, ..
            } => (Some(commit_id), root_id),
            CommitOutcome::UpToDate { root_id } => (self.expected_head, root_id),
        };
        let checkpoint = self
            .pending_checkpoint
            .take()
            .ok_or(StorageError::Integrity("missing published checkpoint"))?;
        let result = (|| {
            if checkpoint.root != root || checkpoint.generation != self.live.mutation_generation {
                return Err(StorageError::Integrity("checkpoint publication identity"));
            }
            // Validate every handoff before changing any live backing. Retry also
            // accepts already installed nodes because paths, attributes and IDs stay stable.
            checkpoint.visit(|id, inode, _, attr| {
                self.live
                    .validate_checkpoint_record(id, inode, attr)
                    .map_err(crate::live_error)
            })?;
            let reader = self
                .store
                .snapshot_reader(root)
                .with_read_metrics_from(&self.reader);
            let namespace = layerfs_content::filesystem::namespace(
                &layerfs_layerstack_store::CoreReader(&reader),
                root,
            )?;
            // The descriptor remains readable if unlink succeeds but a later step
            // fails. Each installed node then owns canonical backing; retries skip its spool.
            checkpoint.visit(|id, inode, content, attr| {
                self.live
                    .install_checkpoint_record(id, inode, content, attr)
                    .map_err(crate::live_error)?;
                #[cfg(test)]
                if INJECT_PARTIAL_INSTALL_FAILURE.with(|inject| inject.replace(false)) {
                    return Err(StorageError::Integrity(
                        "injected partial checkpoint installation",
                    ));
                }
                Ok(())
            })?;
            self.live
                .finish_checkpoint(root)
                .map_err(crate::live_error)?;
            self.retire_spool_segments();
            self.reader = reader;
            self.expected_head = head;
            self.expected_base = expected_base;
            self.base_root = root;
            self.base_inodes =
                layerfs_content::tree::inode::InodeTableRoot(namespace.inode_table_root);
            self.directory_lookup_cache = Default::default();
            self.capture = crate::capture::CaptureState::default();
            self.resolution = None;
            self.state = WorkspaceState::Active;
            Ok(())
        })();
        if result.is_err() {
            self.pending_checkpoint = Some(checkpoint);
        }
        result
    }

    #[doc(hidden)]
    pub fn discard(&mut self) -> Result<()> {
        if self.state == WorkspaceState::Committed {
            return Err(StorageError::InvalidInput("workspace committed"));
        }
        if self.pending_stage.is_some() {
            self.store.discard_workspace_stage(self.workspace_id)?;
        }
        self.pending_stage = None;
        self.clear_spool()?;
        self.pending_publication = None;
        self.pending_checkpoint = None;
        self.state = WorkspaceState::Discarded;
        Ok(())
    }

    pub(crate) fn end_clean(&mut self) -> Result<()> {
        if self.pending_stage.is_some() || self.pending_publication.is_some() {
            return Err(StorageError::InvalidInput("workspace completion pending"));
        }
        self.clear_spool()?;
        self.state = WorkspaceState::Ended;
        Ok(())
    }

    pub(crate) fn ensure_active(&self) -> Result<()> {
        if self.state == WorkspaceState::Active
            && self.pending_stage.is_none()
            && self.pending_publication.is_none()
        {
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
            workspace.physical_spool_snapshot();
        let (open_spool_files, spool_segment_bytes) = if let Some(remote) = &workspace.remote {
            let backing = remote
                .backing
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?;
            (backing.spool.segments.len(), backing.spool.bytes)
        } else {
            (workspace.backing.segments.len(), workspace.backing.bytes)
        };
        Ok(VerificationWorkspaceState {
            spool_bytes: workspace.live.spool_bytes,
            spool_peak_bytes: workspace.live.spool_bytes_peak,
            physical_spool_allocated_bytes: physical_current,
            physical_spool_peak_bytes: physical_peak,
            physical_spool_observation_errors: physical_errors,
            physical_spool_observation_count: physical_observations,
            mutation_generation: workspace.live.mutation_generation,
            open_spool_files,
            spool_segment_bytes,
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
                workspace_id: id.bytes(),
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
        if let Some(host) = worker.host_runtime()? {
            return self.commit_host_session(&worker, &host);
        }
        let _operation = worker
            .lifecycle
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        let _timing = layerfs_layerstack_store::begin_workspace_commit(match worker.projection {
            WorkspaceProjection::Fuse => layerfs_layerstack_store::CaptureMode::Live,
            WorkspaceProjection::Materialize => layerfs_layerstack_store::CaptureMode::Materialized,
        })?;
        let remote = worker
            .workspace
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .remote
            .clone();
        if remote.is_none() && worker.has_executions()? {
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
        let quiesced = if remote.is_some() {
            worker.quiesce()
        } else {
            worker.wait_for_writers().and_then(|()| worker.quiesce())
        };
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
            let completion_pending = {
                let workspace = worker
                    .workspace
                    .lock()
                    .map_err(|_| WorkspaceError::WorkspaceBusy)?;
                workspace.pending_stage.is_some() || workspace.pending_publication.is_some()
            };
            if !completion_pending {
                let started = Instant::now();
                let captured = crate::projection::capture(&worker);
                layerfs_layerstack_store::note_workspace_commit_phase(
                    WorkspaceCommitPhase::Capture,
                    elapsed_ns(started),
                );
                captured?;
            }
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
                    .map(|result| (result, CommitTransition::Checkpointed)),
            };
            let observations = workspace.reader.read_metrics_snapshot().and_then(|after| {
                layerfs_layerstack_store::note_workspace_commit_reads(commit_read_before, after)
            });
            match (committed, observations) {
                (Ok((result @ WorkspaceCommitResult::Created { .. }, _)), Err(_))
                | (Ok((result @ WorkspaceCommitResult::UpToDate { .. }, _)), Err(_)) => {
                    workspace.presentation_failed = true;
                    Ok((result, CommitTransition::InstallationFailed))
                }
                (result, Ok(())) => result,
                (_, Err(error)) => Err(error.into()),
            }
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
                CommitTransition::Checkpointed => {
                    let started = Instant::now();
                    let installed = crate::live_backing::install_checkpoint(&worker.workspace);
                    layerfs_layerstack_store::note_workspace_commit_phase(
                        WorkspaceCommitPhase::Checkpoint,
                        elapsed_ns(started),
                    );
                    let started = Instant::now();
                    let resumed = installed.and_then(|()| crate::projection::resume(&worker));
                    layerfs_layerstack_store::note_workspace_commit_phase(
                        WorkspaceCommitPhase::Resume,
                        elapsed_ns(started),
                    );
                    resumed
                }
                CommitTransition::InstallationFailed => Err(WorkspaceError::InvalidExecution),
                CommitTransition::Refreshed => {
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

    /// Ordinary Commit against the installed host authority.
    ///
    /// The attempt coordinator owns one unresolved attempt and captures its own
    /// snapshot, so no projection freeze, writer wait, quiesce, kernel cache
    /// flush, legacy capture or checkpoint installation is involved, and no
    /// whole-duration lifecycle lock is held: ordinary commands, FUSE callbacks
    /// and SDK ranges continue against the same live authority while this
    /// construction runs.
    fn commit_host_session(
        &self,
        worker: &Arc<WorkspaceWorker>,
        host: &Arc<crate::host_runtime::HostRuntime>,
    ) -> WorkspaceResult<WorkspaceCommitStatus> {
        let _timing = layerfs_layerstack_store::begin_workspace_commit(
            layerfs_layerstack_store::CaptureMode::Live,
        )?;
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
        // Bounded maintenance of the previously published context before this
        // attempt starts. A retained attempt reports remaining work and is left
        // to its own authoritative resolver below.
        host.maintain()?;
        crate::projection::record_write_metrics(worker)?;
        let previous_head = host.published_head()?;
        let completion = match host.commit(|| {
            let started = Instant::now();
            let snapshot = host.operations.host.snapshot()?;
            layerfs_layerstack_store::note_workspace_commit_phase(
                layerfs_layerstack_store::WorkspaceCommitPhase::Capture,
                elapsed_ns(started),
            );
            Ok(snapshot)
        }) {
            Ok(completion) => completion,
            Err(error) => {
                return Ok(WorkspaceCommitStatus {
                    result: match error {
                        layerfs_layerstack_store::StoreError::StoreBusy => {
                            WorkspaceCommitResult::Busy
                        }
                        layerfs_layerstack_store::StoreError::CommitHeadMoved {
                            expected,
                            actual,
                        } => WorkspaceCommitResult::HeadMoved { expected, actual },
                        error => return Err(error.into()),
                    },
                    presentation_failed: false,
                })
            }
        };
        let result = if completion.receipt.up_to_date {
            WorkspaceCommitResult::UpToDate {
                head: previous_head,
            }
        } else {
            WorkspaceCommitResult::Created {
                previous_head,
                commit_id: completion.receipt.head_after.ok_or(
                    layerfs_layerstack_store::StoreError::Integrity("publication head"),
                )?,
            }
        };
        if let Some(error) = &completion.cleanup_error {
            // The publication is known and is reported as such; the retained
            // receipt stays charged and is acknowledged by the next Commit or by
            // the authoritative End/Discard resolution.
            eprintln!("layerfs-workspace: retained publication receipt cleanup: {error}");
        }
        let observations = worker
            .workspace
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .reader
            .read_metrics_snapshot()
            .and_then(|after| {
                layerfs_layerstack_store::note_workspace_commit_reads(commit_read_before, after)
            });
        if let Err(error) = observations {
            // The publication is complete and must not be re-reported as failed.
            eprintln!("layerfs-workspace: host Commit read observation: {error}");
        }
        Ok(WorkspaceCommitStatus {
            result,
            presentation_failed: false,
        })
    }

    pub fn recover_workspace_presentation(
        &self,
        id: WorkspaceId,
    ) -> WorkspaceResult<WorkspaceSession> {
        let worker = self.worker(id)?;
        let _operation = worker
            .lifecycle
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        if worker.has_executions()?
            || !worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?
                .presentation_failed
        {
            return Err(WorkspaceError::InvalidExecution);
        }
        // A failed resume may already have ended the projection. Its control
        // channel cannot be paused again; attach below creates a fresh owner.
        let attached = worker
            .projection_handle
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .is_some();
        if attached {
            crate::projection::pause(&worker)?;
        }
        let _quiesced = worker.quiesce()?;
        crate::projection::end(&worker)?;
        {
            let mut workspace = worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?;
            if let Some((outcome, expected_base, refresh)) = workspace.pending_publication {
                if workspace.transition_committed(outcome, expected_base, refresh)?
                    == CommitTransition::InstallationFailed
                {
                    return Err(WorkspaceError::InvalidExecution);
                }
            }
        }
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
        if let Some(host) = worker.host_runtime()? {
            // The installed authority already owns live bytes: one atomic SDK
            // scope, no freeze/capture/refresh, and no whole-duration lifecycle
            // lock, so an ordinary range edit completes while a Commit is in
            // flight. Same-workspace/same-path validation is above.
            return host.edit(&path, &edits).map_err(WorkspaceError::from);
        }
        let _operation = worker
            .lifecycle
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        let remote = worker
            .workspace
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .remote
            .clone();
        if let Some(remote) = remote {
            return remote.edit(&path, edits);
        }
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
            if workspace.live.nodes[&node].pins != 0 {
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
        let _operation = worker
            .lifecycle
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        let _ending = worker.begin_end()?;
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
        // Discard must remain usable after a backing write failure. FREEZE
        // flushes pending bytes and rejects a failed owner; shutdown below
        // closes admission and releases those bytes without publishing them.
        let host = worker.host_runtime()?;
        if let Some(host) = &host {
            // The host authority is settled before the mounted consumer is asked
            // to stop, and its attempt/raw owners are resolved below, after the
            // verified unmount. A Discard that still cannot resolve its exact
            // publication stays observable and retryable instead of erasing it.
            if let Err(error) = host.recover_sdk() {
                if mode == EndWorkspaceMode::Clean {
                    return Err(error.into());
                }
                // Discard retires the exact pending owner in `after_detach`,
                // which needs no control call, so a failed recovery is not fatal.
            }
        }
        if host.is_none() {
            if mode == EndWorkspaceMode::Clean {
                crate::projection::pause(&worker)?;
            }
        }
        let _quiesced = match worker.quiesce() {
            Ok(quiesced) => quiesced,
            Err(error) => {
                if host.is_none() {
                    crate::projection::resume(&worker)?;
                }
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
                let has_unpublished_state = {
                    let workspace = worker
                        .workspace
                        .lock()
                        .map_err(|_| WorkspaceError::WorkspaceBusy)?;
                    workspace.resolution.is_some() || workspace.pending_stage.is_some()
                };
                if has_unpublished_state || crate::projection::is_dirty(&worker)? {
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
                if host.is_none() {
                    crate::projection::resume(&worker)?;
                }
                return Err(error);
            }
        };
        if let Some(host) = &host {
            if mode == EndWorkspaceMode::Clean {
                host.maintain()?;
            }
        }
        crate::projection::record_read_metrics(&worker)?;
        if let Err(error) = crate::projection::end(&worker) {
            if let Ok(mut workspace) = worker.workspace.lock() {
                workspace.state = WorkspaceState::BrokenCleanup;
                if let Ok(mut remote) = worker.remote.lock() {
                    *remote = None;
                }
            }
            return Err(error);
        }
        let finalized = (|| {
            let mut workspace = worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?;
            workspace.remote = None;
            *worker
                .remote
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)? = None;
            if let Some(host) = &host {
                // Verified unmount is complete. This is the one authoritative
                // resolution point for the Commit attempt and SDK scope.
                host.after_detach()?;
                drop(worker.take_host_runtime()?);
            }
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
                if let Ok(mut remote) = worker.remote.lock() {
                    *remote = None;
                }
            }
            return Err(error);
        }
        let retained = {
            let workspace = worker
                .workspace
                .lock()
                .map_err(|_| WorkspaceError::WorkspaceBusy)?;
            crate::registry::RetainedSession {
                session: session_locked(&worker, &workspace),
                mutation_generation: workspace.live.mutation_generation,
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
                if let Some((session, generation)) = remote_session(&worker)? {
                    return Ok(WorkspaceDetail {
                        session,
                        mutation_generation: generation,
                        executions,
                    });
                }
                if let Some(host) = worker.host_runtime()? {
                    return Ok(WorkspaceDetail {
                        session: host_session(&worker, &host)?,
                        mutation_generation: host.generation()?,
                        executions,
                    });
                }
                let generation = crate::live_backing::generation(&worker)?;
                let workspace = worker
                    .workspace
                    .lock()
                    .map_err(|_| WorkspaceError::WorkspaceBusy)?;
                Ok(WorkspaceDetail {
                    session: session_locked(&worker, &workspace),
                    mutation_generation: generation,
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
                if let Some(host) = worker.host_runtime()? {
                    return Ok(WorkspaceDiff {
                        session_id: id,
                        dirty: host.is_dirty()?,
                        mutation_generation: host.generation()?,
                    });
                }
                let generation = crate::live_backing::generation(&worker)?;
                let dirty = if worker.projection == WorkspaceProjection::Fuse {
                    generation != 0
                } else {
                    crate::projection::is_dirty(&worker)?
                };
                Ok(WorkspaceDiff {
                    session_id: id,
                    dirty,
                    mutation_generation: generation,
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
    if let Some((session, _)) = remote_session(worker)? {
        return Ok(session);
    }
    if let Some(host) = worker.host_runtime()? {
        return host_session(worker, &host);
    }
    let workspace = worker
        .workspace
        .lock()
        .map_err(|_| WorkspaceError::WorkspaceBusy)?;
    Ok(session_locked(worker, &workspace))
}

/// Session identity and pinned head for a host-authority Workspace. The pinned
/// head is the published context, never the legacy `Workspace.expected_head`.
fn host_session(
    worker: &WorkspaceWorker,
    host: &crate::host_runtime::HostRuntime,
) -> WorkspaceResult<WorkspaceSession> {
    let pinned_head = host.published_head()?;
    let state = worker
        .workspace
        .lock()
        .map_err(|_| WorkspaceError::WorkspaceBusy)?
        .state;
    Ok(WorkspaceSession {
        id: worker.id,
        branch_id: worker.request.branch_id,
        layer_stack_id: worker.identity.layer_stack_id,
        layer_stack_name: worker.identity.layer_stack_name.clone(),
        branch_name: worker.identity.branch_name.clone(),
        pinned_head,
        placement: worker.request.placement.clone(),
        projection: worker.projection,
        state,
    })
}

fn remote_session(worker: &WorkspaceWorker) -> WorkspaceResult<Option<(WorkspaceSession, u64)>> {
    let remote = worker
        .remote
        .lock()
        .map_err(|_| WorkspaceError::WorkspaceBusy)?
        .clone();
    let Some(remote) = remote else {
        return Ok(None);
    };
    let (generation, _, _, head) = remote.observe()?;
    Ok(Some((
        WorkspaceSession {
            id: worker.id,
            branch_id: worker.request.branch_id,
            layer_stack_id: worker.identity.layer_stack_id,
            layer_stack_name: worker.identity.layer_stack_name.clone(),
            branch_name: worker.identity.branch_name.clone(),
            pinned_head: head,
            placement: worker.request.placement.clone(),
            projection: worker.projection,
            state: WorkspaceState::Active,
        },
        generation,
    )))
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

pub(crate) fn summary(worker: &Arc<WorkspaceWorker>) -> WorkspaceResult<WorkspaceSummary> {
    if let Some((session, generation)) = remote_session(worker)? {
        return Ok(WorkspaceSummary {
            id: session.id,
            branch_id: session.branch_id,
            layer_stack_id: session.layer_stack_id,
            layer_stack_name: session.layer_stack_name,
            branch_name: session.branch_name,
            pinned_head: session.pinned_head,
            state: session.state,
            dirty: generation != 0,
        });
    }
    if let Some(host) = worker.host_runtime()? {
        let session = host_session(worker, &host)?;
        let state = worker
            .workspace
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .state;
        return Ok(WorkspaceSummary {
            id: session.id,
            branch_id: session.branch_id,
            layer_stack_id: session.layer_stack_id,
            layer_stack_name: session.layer_stack_name,
            branch_name: session.branch_name,
            pinned_head: session.pinned_head,
            state,
            dirty: host.is_dirty()? || state == WorkspaceState::BrokenCleanup,
        });
    }
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
        dirty: dirty || workspace.state == WorkspaceState::BrokenCleanup,
    })
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

    #[test]
    fn refreshed_workspace_keeps_held_segment_charge_until_read_finishes() {
        let (root, workspaces, branch, store) = fixture("refresh-segment-charge");
        drop(workspaces);
        let mut workspace = Workspace::open_with_policy(
            store,
            branch,
            root.join("spool"),
            crate::ResourcePolicy {
                max_spool_bytes: 4,
                ..crate::ResourcePolicy::default()
            },
        )
        .unwrap();
        let file = lookup_path(&mut workspace, "file").unwrap();
        workspace.write(file, 0, b"held").unwrap();
        let read = workspace.read_plan(file, 0, 4).unwrap();
        let (outcome, _) = workspace.commit().unwrap();
        workspace
            .refresh_reconciled(outcome, workspace.expected_base)
            .unwrap();
        assert_eq!(workspace.backing.bytes, 4);
        assert!(workspace.backing.current.is_none());
        let file = lookup_path(&mut workspace, "file").unwrap();
        assert!(workspace.write(file, 0, b"x").is_err());
        assert_eq!(read.read().unwrap(), b"held");
        workspace.write(file, 0, b"x").unwrap();
        assert_eq!(workspace.backing.bytes, 1);
        workspace.discard().unwrap();
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn owner_drop_discards_only_its_workspace_without_branch_exclusion() {
        let (root, first, branch, store) = fixture("owner-drop-isolation");
        let second = Workspaces::new(root.join("second-runtime"), store.clone()).unwrap();
        let request = |mount: &str| CreateWorkspaceSession {
            branch_id: branch,
            placement: crate::WorkspacePlacement::Host {
                root: root.join(mount),
            },
            projection: Some(WorkspaceProjection::Materialize),
        };
        let created = first
            .create_workspace_session(request("first-mount"))
            .unwrap();
        let concurrent = second
            .create_workspace_session(request("second-mount"))
            .unwrap();
        let before_root = store.pin_branch(branch).unwrap().root;
        let before_commits = store.store_counts().unwrap().commits;
        prepend(&first, created.id).unwrap();
        let state = first
            .runtime_root
            .join("workspaces")
            .join(created.id.to_string());
        // Projection diagnostics belong to this exact state directory, not spool/.
        std::fs::write(state.join("mountinfo.txt"), b"owned projection diagnostic").unwrap();
        assert!(root.join("first-mount").exists());
        drop(first);
        assert!(!root.join("first-mount").exists());
        assert!(!state.exists());
        assert_eq!(store.pin_branch(branch).unwrap().root, before_root);
        assert_eq!(store.store_counts().unwrap().commits, before_commits);
        assert_eq!(
            std::fs::read(root.join("second-mount/file")).unwrap(),
            b"abcdef"
        );
        second
            .end_workspace_session(concurrent.id, EndWorkspaceMode::Clean)
            .unwrap();
        drop(second);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn published_install_failure_recovers_without_second_commit() {
        let (root, workspaces, branch, store) = fixture("published-install-recovery");
        let session = session(&root, &workspaces, branch);
        prepend(&workspaces, session.id).unwrap();
        let commits_before = store.store_counts().unwrap().commits;
        INJECT_INSTALL_FAILURE.with(|inject| inject.set(true));
        let status = workspaces
            .commit_workspace_session_with_status(session.id)
            .unwrap();
        assert!(matches!(
            status.result,
            WorkspaceCommitResult::Created { .. }
        ));
        assert!(status.presentation_failed);
        assert_eq!(store.store_counts().unwrap().commits, commits_before + 1);
        assert!(workspaces
            .worker(session.id)
            .unwrap()
            .workspace
            .lock()
            .unwrap()
            .pending_publication
            .is_some());

        workspaces
            .recover_workspace_presentation(session.id)
            .unwrap();
        assert_eq!(std::fs::read(root.join("mount/file")).unwrap(), b"Pabcdef");
        assert_eq!(store.store_counts().unwrap().commits, commits_before + 1);
        assert!(matches!(
            workspaces.commit_workspace_session(session.id).unwrap(),
            WorkspaceCommitResult::UpToDate { .. }
        ));
        assert_eq!(store.store_counts().unwrap().commits, commits_before + 1);
        workspaces
            .end_workspace_session(session.id, EndWorkspaceMode::Clean)
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn continuing_workspace_installs_its_returned_snapshot_after_later_publication() {
        let (root, workspaces, branch_id, store) = fixture("returned-snapshot-race");
        drop(workspaces);
        let mut first =
            Workspace::open(store.clone(), branch_id, root.join("first-spool")).unwrap();
        let first_file = lookup_path(&mut first, "file").unwrap();
        first.write(first_file, 0, b"first").unwrap();
        let expected = store.branch(branch_id).unwrap().unwrap();
        let prepared = first
            .build_candidate(crate::changes::CandidatePurpose::Preview)
            .unwrap();
        first.pending_checkpoint = Some(prepared.checkpoint);
        let candidate = prepared.built;
        let first_outcome = store
            .commit_workspace_candidate(
                first.workspace_id,
                &expected,
                first.base_root,
                first.expected_base,
                candidate,
                store.workspace_admission(first.workspace_id).unwrap(),
            )
            .unwrap();
        let first_root = match first_outcome {
            CommitOutcome::Committed { root_id, .. } => root_id,
            CommitOutcome::UpToDate { .. } => panic!("first edit must publish"),
        };

        let mut second =
            Workspace::open(store.clone(), branch_id, root.join("second-spool")).unwrap();
        let second_file = lookup_path(&mut second, "file").unwrap();
        second.write(second_file, 0, b"second").unwrap();
        second.commit().unwrap();
        assert_ne!(store.pin_branch(branch_id).unwrap().root, first_root);

        let first_base = first.expected_base;
        assert_eq!(
            first
                .transition_committed(first_outcome, first_base, false)
                .unwrap(),
            CommitTransition::Checkpointed
        );
        assert_eq!(first.base_root, first_root);
        assert_eq!(first.read(first_file, 0, 16).unwrap(), b"firstf");
        drop(first);
        drop(second);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn head_movement_retains_stage_and_freezes_mutation_until_discard() {
        let (root, workspaces, branch_id, store) = fixture("retained-stage-freeze");
        drop(workspaces);
        let mut winner =
            Workspace::open(store.clone(), branch_id, root.join("winner-spool")).unwrap();
        let mut stale =
            Workspace::open(store.clone(), branch_id, root.join("stale-spool")).unwrap();
        let winner_file = lookup_path(&mut winner, "file").unwrap();
        winner.write(winner_file, 0, b"winner").unwrap();
        winner.commit().unwrap();

        let stale_file = lookup_path(&mut stale, "file").unwrap();
        stale.write(stale_file, 0, b"stale").unwrap();
        assert!(matches!(
            stale.commit(),
            Err(StorageError::CommitHeadMoved { .. })
        ));
        let staged = store.workspace_stage(stale.workspace_id).unwrap().unwrap();
        assert_eq!(stale.pending_stage, Some(staged.root_id));
        assert!(stale.write(stale_file, 0, b"blocked").is_err());
        stale.discard().unwrap();
        assert!(store.workspace_stage(stale.workspace_id).unwrap().is_none());
        drop(winner);
        drop(stale);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn partial_checkpoint_retry_preserves_identity_aliases_and_pinned_spools() {
        let (root, workspaces, branch, store) = fixture("checkpoint-identities");
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
            .map(|(_, node)| workspace.live.nodes[node].canonical.unwrap())
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
        let commits_before = store.store_counts().unwrap().commits;
        INJECT_PARTIAL_INSTALL_FAILURE.with(|inject| inject.set(true));
        let (published, failed) = workspace.commit().unwrap();
        assert_eq!(failed, CommitTransition::InstallationFailed);
        assert!(workspace.pending_checkpoint.is_some());
        assert!(workspace.write(files[0].1, 0, b"blocked").is_err());
        let (outcome, transition) = workspace.commit().unwrap();
        assert_eq!(outcome, published);
        assert_eq!(store.store_counts().unwrap().commits, commits_before + 1);
        assert!(workspace.pending_checkpoint.is_none());
        assert!(matches!(outcome, CommitOutcome::Committed { .. }));
        assert_eq!(transition, CommitTransition::Checkpointed);
        assert_eq!(
            workspace.lookup(crate::ROOT, b"group").unwrap().node,
            directory
        );
        for (index, (name, node)) in files.iter().enumerate() {
            assert_eq!(
                workspace.lookup(directory, name.as_bytes()).unwrap().node,
                *node
            );
            assert_eq!(workspace.live.nodes[node].canonical, Some(inodes[index]));
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
                workspace.live.nodes[node].data,
                crate::cow_tree::Data::File(crate::cow_tree::FileData::Base { .. })
            ));
        }
        assert_eq!(
            workspace.lookup(directory, b"alias").unwrap().node,
            files[0].1
        );
        assert_eq!(workspace.attr(files[0].1).unwrap().links, 2);
        assert!(workspace.live.nodes[&orphan].paths.is_empty());
        assert_eq!(workspace.live.nodes[&orphan].pins, 1);
        assert_eq!(workspace.read(orphan, 0, 64).unwrap(), b"pinned-data");
        assert_eq!(workspace.live.spool_bytes, 11);
        assert_eq!(workspace.backing.segments.len(), 1);
        assert!(workspace.live.dirty.is_empty() && workspace.live.mutation_paths.is_empty());
        assert_eq!(workspace.live.mutation_generation, 0);
        workspace.unpin(orphan).unwrap();
        assert_eq!(workspace.live.spool_bytes, 0);
        assert!(workspace.backing.segments.is_empty());
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
                workspace.live.nodes.clone(),
                workspace.live.dirty.clone(),
                workspace.live.mutation_generation,
                workspace.live.mutation_paths.clone(),
                workspace.live.spool_bytes,
                workspace.live.inline_bytes,
                workspace.live.piece_allocation_bytes,
            )
        };
        let branch_before = store.pin_branch(branch).unwrap().root;
        crate::projection::inject_refresh_failure_once();
        assert!(prepend(&workspaces, session.id).is_err());
        {
            let workspace = worker.workspace.lock().unwrap();
            assert_eq!(workspace.live.nodes, before.0);
            assert_eq!(workspace.live.dirty, before.1);
            assert_eq!(workspace.live.mutation_generation, before.2);
            assert_eq!(workspace.live.mutation_paths, before.3);
            assert_eq!(workspace.live.spool_bytes, before.4);
            assert_eq!(workspace.live.inline_bytes, before.5);
            assert_eq!(workspace.live.piece_allocation_bytes, before.6);
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
                workspace.live.nodes.clone(),
                workspace.live.dirty.clone(),
                workspace.live.mutation_generation,
                workspace.live.mutation_paths.clone(),
                workspace.live.spool_bytes,
                workspace.live.inline_bytes,
                workspace.live.piece_allocation_bytes,
                store.pin_branch(branch).unwrap().root,
            )
        };
        assert!(matches!(
            prepend(&workspaces, session.id),
            Err(WorkspaceError::WorkspaceBusy)
        ));
        {
            let workspace = worker.workspace.lock().unwrap();
            assert_eq!(workspace.live.nodes, before.0);
            assert_eq!(workspace.live.dirty, before.1);
            assert_eq!(workspace.live.mutation_generation, before.2);
            assert_eq!(workspace.live.mutation_paths, before.3);
            assert_eq!(workspace.live.spool_bytes, before.4);
            assert_eq!(workspace.live.inline_bytes, before.5);
            assert_eq!(workspace.live.piece_allocation_bytes, before.6);
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
                workspace.live.nodes.clone(),
                workspace.live.dirty.clone(),
                workspace.live.mutation_generation,
                workspace.live.spool_bytes,
                workspace.live.inline_bytes,
                workspace.live.piece_allocation_bytes,
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
            assert_eq!(workspace.live.nodes, before.0);
            assert_eq!(workspace.live.dirty, before.1);
            assert_eq!(workspace.live.mutation_generation, before.2);
            assert_eq!(workspace.live.spool_bytes, before.3);
            assert_eq!(workspace.live.inline_bytes, before.4);
            assert_eq!(workspace.live.piece_allocation_bytes, before.5);
        }
        assert_eq!(store.pin_branch(branch).unwrap().root, before.6);
        assert_eq!(std::fs::read(root.join("mount/file")).unwrap(), b"abcdef");
        assert!(worker.projection_handle.lock().unwrap().is_some());
        workspaces
            .end_workspace_session(session.id, EndWorkspaceMode::Clean)
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    // ---- Host authority routes -------------------------------------------------

    /// A Host-placement session carrying the same authority production mounts
    /// on Linux. `mount_host` and the client kernel-coherence path are
    /// Linux-only, so the authority is installed directly here; every route
    /// below is the unmodified production route for that authority.
    fn host_authority_session(
        root: &std::path::Path,
        workspaces: &Workspaces,
        branch: layerfs_layerstack_store::BranchId,
    ) -> (
        WorkspaceSession,
        Arc<crate::host_runtime::HostRuntime>,
        layerfs_fuse::host_client::HostClient,
    ) {
        let session = workspaces
            .create_workspace_session(CreateWorkspaceSession {
                branch_id: branch,
                placement: crate::WorkspacePlacement::Host {
                    root: root.join("mount"),
                },
                projection: Some(WorkspaceProjection::Materialize),
            })
            .unwrap();
        let worker = workspaces.worker(session.id).unwrap();
        let runtime = crate::projection::start_host_authority(&worker, true).unwrap();
        worker.install_host_runtime(runtime.clone()).unwrap();
        let client = runtime.server.host_owner().unwrap();
        (session, runtime, client)
    }

    /// Read one published root's exact bytes through a fresh reader.
    fn committed_file(store: &LayerStackStore, root: layerfs_content::ObjectId) -> Vec<u8> {
        use layerfs_content::{filesystem, CanonicalPath};
        use layerfs_layerstack_store::CoreReader;
        let reader = store.snapshot_reader(root);
        let resolved = filesystem::resolve(
            &CoreReader(&reader),
            root,
            &CanonicalPath::new("file").unwrap(),
            &mut filesystem::LogicalCounters::default(),
        )
        .unwrap();
        let content = layerfs_content::file::content::FileContentRoot(resolved.record.content_root);
        let length = layerfs_content::file::content::length(&CoreReader(&reader), content).unwrap();
        let mut bytes = Vec::new();
        layerfs_content::file::content::read_range(
            &CoreReader(&reader),
            content,
            0..length,
            &mut bytes,
        )
        .unwrap();
        bytes
    }

    #[test]
    fn host_authority_commit_in_flight_keeps_live_operations_and_owned_cut() {
        use layerfs_fuse::FilesystemPort;
        let (root, workspaces, branch, store) = fixture("host-commit-in-flight");
        let (session, runtime, client) = host_authority_session(&root, &workspaces, branch);
        let node = client.lookup(crate::ROOT, b"file").unwrap().node;
        assert_eq!(client.read(node, 0, 16).unwrap(), b"abcdef");
        client.write(node, 0, b"A").unwrap();
        assert_eq!(client.read(node, 0, 16).unwrap(), b"Abcdef");
        assert!(workspaces.diff(session.id).unwrap().dirty);
        let cut = runtime.generation().unwrap();
        let (at_build, resume) = crate::host_runtime::arm_build_latch();
        let committed = std::thread::scope(|scope| {
            let building =
                scope.spawn(|| workspaces.commit_workspace_session_with_status(session.id));
            // The attempt owns its captured snapshot and is mid-construction
            // while these ordinary operations run against the same authority.
            at_build
                .recv_timeout(std::time::Duration::from_secs(60))
                .unwrap();
            // Unresolved attempt state is visible as pending lifecycle work.
            assert!(workspaces.diff(session.id).unwrap().dirty);
            assert!(workspaces.sessions().unwrap()[0].dirty);
            // A live write and read through the mounted port complete while the
            // Commit is in flight and see the new bytes immediately.
            assert_eq!(client.write(node, 1, b"Z").unwrap(), 1);
            assert_eq!(client.read(node, 0, 16).unwrap(), b"AZcdef");
            resume.send(()).unwrap();
            building.join().unwrap().unwrap()
        });
        assert!(matches!(
            committed.result,
            WorkspaceCommitResult::Created { .. }
        ));
        assert!(!committed.presentation_failed);
        // C1 is the owned cut: it holds the capture boundary, not the later write.
        assert_eq!(runtime.covered_sequence().unwrap(), cut);
        let c1 = store.pin_branch(branch).unwrap().root;
        assert_eq!(committed_file(&store, c1), b"Abcdef");
        assert_eq!(client.read(node, 0, 16).unwrap(), b"AZcdef");
        // The later write is still live and is included by the next Commit; the
        // non-resetting host sequence is what makes this session dirty.
        let later = runtime.generation().unwrap();
        assert!(later > cut);
        assert!(workspaces.diff(session.id).unwrap().dirty);
        assert!(matches!(
            workspaces.commit_workspace_session(session.id).unwrap(),
            WorkspaceCommitResult::Created { .. }
        ));
        let c2 = store.pin_branch(branch).unwrap().root;
        assert_eq!(committed_file(&store, c2), b"AZcdef");
        assert_eq!(runtime.covered_sequence().unwrap(), later);
        assert!(!workspaces.diff(session.id).unwrap().dirty);
        workspaces
            .end_workspace_session(session.id, EndWorkspaceMode::Clean)
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
        println!("HOST IN-FLIGHT PASS: owned C1 cut excludes a write that completed during construction, C2 includes it, live port read/write never blocked, published coverage == live sequence");
    }

    #[test]
    fn host_authority_dirty_tracks_covered_sequence_not_nonzero_generation() {
        use layerfs_fuse::FilesystemPort;
        let (root, workspaces, branch, _store) = fixture("host-dirty-coverage");
        let (session, runtime, client) = host_authority_session(&root, &workspaces, branch);
        let node = client.lookup(crate::ROOT, b"file").unwrap().node;
        let pinned = session.pinned_head;
        assert!(!workspaces.diff(session.id).unwrap().dirty);
        client.write(node, 0, b"X").unwrap();
        assert!(workspaces.diff(session.id).unwrap().dirty);
        assert!(matches!(
            workspaces.commit_workspace_session(session.id).unwrap(),
            WorkspaceCommitResult::Created { .. }
        ));
        let generation = runtime.generation().unwrap();
        assert!(generation > 0, "the host sequence never resets");
        assert_eq!(runtime.covered_sequence().unwrap(), generation);
        let detail = workspaces.session(session.id).unwrap();
        assert_eq!(detail.mutation_generation, generation);
        assert!(
            detail.session.pinned_head.is_some() && detail.session.pinned_head != pinned,
            "the pinned head follows the published context"
        );
        // A committed session with a nonzero sequence is clean.
        assert!(!workspaces.diff(session.id).unwrap().dirty);
        assert!(!workspaces.sessions().unwrap()[0].dirty);
        // A no-op Commit advances coverage instead of leaving a dirty session.
        assert!(matches!(
            workspaces.commit_workspace_session(session.id).unwrap(),
            WorkspaceCommitResult::UpToDate { .. }
        ));
        assert!(!workspaces.diff(session.id).unwrap().dirty);
        workspaces
            .end_workspace_session(session.id, EndWorkspaceMode::Clean)
            .unwrap();
        // A later write is the only thing that makes Clean End refuse.
        let (session, runtime, client) = host_authority_session(&root, &workspaces, branch);
        let node = client.lookup(crate::ROOT, b"file").unwrap().node;
        client.write(node, 0, b"Y").unwrap();
        assert!(matches!(
            workspaces.end_workspace_session(session.id, EndWorkspaceMode::Clean),
            Err(WorkspaceError::WorkspaceDirty)
        ));
        // The refused End allocated nothing: the session still commits and ends.
        assert!(matches!(
            workspaces.commit_workspace_session(session.id).unwrap(),
            WorkspaceCommitResult::Created { .. }
        ));
        assert!(!runtime.is_dirty().unwrap());
        workspaces
            .end_workspace_session(session.id, EndWorkspaceMode::Clean)
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
        println!("HOST DIRTY PASS: coverage-relative dirtiness, monotonic sequence, published pinned head, no-op Commit stays clean, Clean End refuses only outstanding work");
    }

    #[test]
    fn host_authority_discard_resolves_publication_instead_of_erasing_it() {
        use layerfs_fuse::FilesystemPort;
        use layerfs_layerstack_store::WorkspacePublicationAttempt;
        let (root, workspaces, branch, store) = fixture("host-discard-resolution");
        let (session, runtime, client) = host_authority_session(&root, &workspaces, branch);
        let node = client.lookup(crate::ROOT, b"file").unwrap().node;
        let branch_before = store.branch(branch).unwrap().unwrap();
        let root_before = store.pin_branch(branch).unwrap().root;
        client.write(node, 0, b"X").unwrap();
        let covered = runtime.generation().unwrap();
        let commits_before = store.store_counts().unwrap().commits;
        // The publication transaction commits and then its reply is lost, so the
        // caller cannot know whether this exact attempt published.
        layerfs_layerstack_store::set_transaction_failure_at(Some(u64::MAX - 3));
        let lost = workspaces.commit_workspace_session(session.id);
        layerfs_layerstack_store::set_transaction_failure_at(None);
        assert!(lost.is_err());
        let published_root = store.pin_branch(branch).unwrap().root;
        assert_eq!(committed_file(&store, published_root), b"Xbcdef");
        assert_eq!(store.store_counts().unwrap().commits, commits_before + 1);
        assert!(runtime.is_dirty().unwrap());
        let attempt = WorkspacePublicationAttempt::new(
            session.id.bytes(),
            &branch_before,
            root_before,
            published_root,
            store.branch(branch).unwrap().unwrap().base_layer_id,
            covered,
        );
        assert!(matches!(
            store.resolve_workspace_publication(&attempt).unwrap(),
            layerfs_layerstack_store::WorkspacePublicationResolution::Published(_)
        ));
        // Discard must resolve that exact receipt: the published Commit stays on
        // the branch and the receipt is acknowledged, never erased as unknown.
        let ended = workspaces
            .end_workspace_session(session.id, EndWorkspaceMode::Discard)
            .unwrap();
        assert!(ended.discarded);
        assert_eq!(store.pin_branch(branch).unwrap().root, published_root);
        assert_eq!(store.store_counts().unwrap().commits, commits_before + 1);
        assert_eq!(committed_file(&store, published_root), b"Xbcdef");
        // Acknowledged by the Discard resolution: nothing is left to delete.
        assert!(!store.acknowledge_workspace_publication(&attempt).unwrap());
        assert!(!root.join("mount").exists());

        // An outcome that authoritative state cannot resolve is retained instead
        // of being discarded: a sibling Workspace moves the branch after this
        // attempt's publication transaction rolled back.
        let (session, _runtime, client) = host_authority_session(&root, &workspaces, branch);
        let node = client.lookup(crate::ROOT, b"file").unwrap().node;
        client.write(node, 0, b"L").unwrap();
        layerfs_layerstack_store::set_transaction_failure_at(Some(u64::MAX - 1));
        let rolled_back = workspaces.commit_workspace_session(session.id);
        layerfs_layerstack_store::set_transaction_failure_at(None);
        assert!(rolled_back.is_err());
        let stage = store.workspace_stage(session.id.bytes()).unwrap();
        assert!(stage.is_some(), "the exact stage is retained for retry");
        let second = Workspaces::new(root.join("second-runtime"), store.clone()).unwrap();
        let sibling = second
            .create_workspace_session(CreateWorkspaceSession {
                branch_id: branch,
                placement: crate::WorkspacePlacement::Host {
                    root: root.join("sibling-mount"),
                },
                projection: Some(WorkspaceProjection::Materialize),
            })
            .unwrap();
        prepend(&second, sibling.id).unwrap();
        assert!(matches!(
            second.commit_workspace_session(sibling.id).unwrap(),
            WorkspaceCommitResult::Created { .. }
        ));
        let sibling_root = store.pin_branch(branch).unwrap().root;
        assert_ne!(sibling_root, published_root);
        assert!(matches!(
            workspaces.end_workspace_session(session.id, EndWorkspaceMode::Discard),
            Err(WorkspaceError::Storage(_))
        ));
        assert_eq!(
            store
                .workspace_stage(session.id.bytes())
                .unwrap()
                .map(|s| s.root_id),
            stage.map(|s| s.root_id),
            "an unresolvable publication keeps its exact retained stage"
        );
        assert_eq!(store.pin_branch(branch).unwrap().root, sibling_root);
        assert_eq!(
            workspaces
                .worker(session.id)
                .unwrap()
                .workspace
                .lock()
                .unwrap()
                .state,
            WorkspaceState::BrokenCleanup
        );
        drop((second, client, sibling));
        drop(workspaces);
        std::fs::remove_dir_all(root).unwrap();
        println!("HOST DISCARD PASS: lost reply resolved from the retained receipt with published history preserved; unresolvable attempt retained with its stage and branch rather than erased");
    }

    /// The audit's superseded expectation: a retained stage after a moved branch
    /// keeps CAS and exact explicit-discard resolution, but it no longer freezes
    /// ordinary mutation. The legacy `Workspace::commit` test above still covers
    /// the retained local Materialize route, whose own live mirror is separate.
    #[test]
    fn host_authority_head_movement_retains_stage_without_freezing_mutation() {
        use layerfs_fuse::FilesystemPort;
        let (root, workspaces, branch, store) = fixture("host-head-moved");
        let (session, runtime, client) = host_authority_session(&root, &workspaces, branch);
        let node = client.lookup(crate::ROOT, b"file").unwrap().node;
        client.write(node, 0, b"A").unwrap();
        // A sibling Workspace on the same branch publishes first: no
        // lifetime-exclusive branch lease, and this attempt's CAS now conflicts.
        let second = Workspaces::new(root.join("second-runtime"), store.clone()).unwrap();
        let sibling = second
            .create_workspace_session(CreateWorkspaceSession {
                branch_id: branch,
                placement: crate::WorkspacePlacement::Host {
                    root: root.join("sibling-mount"),
                },
                projection: Some(WorkspaceProjection::Materialize),
            })
            .unwrap();
        prepend(&second, sibling.id).unwrap();
        assert!(matches!(
            second.commit_workspace_session(sibling.id).unwrap(),
            WorkspaceCommitResult::Created { .. }
        ));
        let sibling_root = store.pin_branch(branch).unwrap().root;
        assert!(matches!(
            workspaces.commit_workspace_session(session.id).unwrap(),
            WorkspaceCommitResult::HeadMoved { .. }
        ));
        let stage = store
            .workspace_stage(session.id.bytes())
            .unwrap()
            .expect("the conflicting candidate stage is retained for explicit retry");
        assert_eq!(store.pin_branch(branch).unwrap().root, sibling_root);
        // The retained stage no longer freezes ordinary mutation or status.
        assert_eq!(client.write(node, 1, b"Y").unwrap(), 1);
        assert_eq!(client.read(node, 0, 16).unwrap(), b"AYcdef");
        assert!(workspaces.diff(session.id).unwrap().dirty);
        assert!(runtime.generation().unwrap() > 0);
        assert!(matches!(
            workspaces.end_workspace_session(session.id, EndWorkspaceMode::Clean),
            Err(WorkspaceError::WorkspaceDirty)
        ));
        // Explicit Discard resolves the known-not-published attempt: the exact
        // stage disappears, the winning branch is untouched, and the live bytes
        // that arrived while the stage was retained are never published.
        assert!(
            workspaces
                .end_workspace_session(session.id, EndWorkspaceMode::Discard)
                .unwrap()
                .discarded
        );
        assert!(store.workspace_stage(session.id.bytes()).unwrap().is_none());
        assert_eq!(store.pin_branch(branch).unwrap().root, sibling_root);
        assert_eq!(committed_file(&store, sibling_root), b"Pabcdef");
        drop((second, client, sibling));
        drop(workspaces);
        std::fs::remove_dir_all(root).unwrap();
        println!("HOST HEAD-MOVED PASS: retained stage {stage:?}, CAS and explicit discard preserved, ordinary mutation/status continued while it was retained");
    }

    /// Real mounted Linux host authority: the production `attach` path plus a
    /// held Commit, an ordinary SDK splice and live kernel reads on one inode.
    #[test]
    #[cfg(all(target_os = "linux", feature = "host-fuse"))]
    fn mounted_host_authority_holds_commit_while_sdk_edit_and_live_read_proceed() {
        use std::os::unix::fs::{FileExt, MetadataExt};
        if std::env::var_os("LAYERFS_HOST_AUTHORITY_MOUNT").is_none() {
            return;
        }
        let (root, workspaces, branch, store) = fixture("host-mounted");
        std::fs::create_dir_all(root.join("mount")).unwrap();
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
        assert_eq!(std::fs::read(&file).unwrap(), b"abcdef");
        // An ordinary kernel write through the mount precedes the Commit, so the
        // owned capture is a real change and not a no-op publication.
        let handle = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&file)
            .unwrap();
        handle.write_at(b"A", 0).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"Abcdef");
        let worker = workspaces.worker(session.id).unwrap();
        let runtime = worker.host_runtime().unwrap().unwrap();
        let cut = runtime.generation().unwrap();
        let (at_build, resume) = crate::host_runtime::arm_build_latch();
        let committed = std::thread::scope(|scope| {
            let building =
                scope.spawn(|| workspaces.commit_workspace_session_with_status(session.id));
            at_build
                .recv_timeout(std::time::Duration::from_secs(60))
                .unwrap();
            assert!(workspaces.diff(session.id).unwrap().dirty);
            // Ordinary SDK splice against the same inode while Commit constructs.
            workspaces
                .edit_workspace_file_range(WorkspaceFileRangeEdit {
                    workspace_id: session.id,
                    path: "file".into(),
                    start: 0,
                    delete_len: 0,
                    replacement: crate::WorkspaceFileReplacement::Inline(b"P".to_vec()),
                })
                .unwrap();
            assert_eq!(std::fs::read(&file).unwrap(), b"PAbcdef");
            handle.write_at(b"Z", 1).unwrap();
            assert_eq!(std::fs::read(&file).unwrap(), b"PZbcdef");
            assert_eq!(std::fs::metadata(&file).unwrap().ino(), inode);
            resume.send(()).unwrap();
            building.join().unwrap().unwrap()
        });
        assert!(matches!(
            committed.result,
            WorkspaceCommitResult::Created { .. }
        ));
        assert_eq!(runtime.covered_sequence().unwrap(), cut);
        let c1 = store.pin_branch(branch).unwrap().root;
        assert_eq!(committed_file(&store, c1), b"Abcdef");
        assert!(matches!(
            workspaces.commit_workspace_session(session.id).unwrap(),
            WorkspaceCommitResult::Created { .. }
        ));
        let c2 = store.pin_branch(branch).unwrap().root;
        assert_eq!(committed_file(&store, c2), b"PZbcdef");
        assert_eq!(std::fs::metadata(&file).unwrap().ino(), inode);
        assert!(!workspaces.diff(session.id).unwrap().dirty);
        // The mounted consumer must own no descriptor on this mount to unmount.
        drop(handle);
        workspaces
            .end_workspace_session(session.id, EndWorkspaceMode::Clean)
            .unwrap();
        assert!(!is_mounted_mountpoint(&root.join("mount")));
        std::fs::remove_dir_all(root).unwrap();
        println!("MOUNTED HOST AUTHORITY PASS: production attach, held Commit, concurrent SDK splice with stable inode, C1 excludes the splice while C2 includes it, verified unmount");
    }

    /// Real container-placement host authority on the benchmark's daemon mount
    /// route (`LAYERFS_FUSE_TRANSPORT=daemon`): the in-container daemon mounts
    /// through the one installed host authority instead of the legacy owner.
    #[cfg(unix)]
    #[test]
    fn container_daemon_route_mounts_through_the_host_authority() {
        let Ok(container) = std::env::var("LAYERFS_TEST_CONTAINER") else {
            return;
        };
        let exec = |script: &str| -> String {
            let output = std::process::Command::new("docker")
                .args(["exec", &container, "/bin/sh", "-c", script])
                .output()
                .expect("docker exec");
            assert!(
                output.status.success(),
                "docker exec failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        };
        let hex_capability = exec("od -An -tx1 -v /run/layerfs/capability | tr -d ' \\n'");
        assert_eq!(
            hex_capability.len(),
            64,
            "daemon capability: {hex_capability}"
        );
        let mut capability = [0_u8; 32];
        for (index, byte) in capability.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex_capability[index * 2..index * 2 + 2], 16).unwrap();
        }
        let published = {
            let output = std::process::Command::new("docker")
                .args(["port", &container, "41273"])
                .output()
                .expect("docker port");
            assert!(output.status.success(), "docker port failed");
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .expect("published daemon port")
                .trim()
                .to_owned()
        };
        let endpoint = published
            .parse::<std::net::SocketAddr>()
            .expect("published daemon endpoint");
        let owner = layerfs_daemon::connect_tcp(endpoint, capability).expect("daemon owner");
        let binding = crate::ContainerBinding {
            id: crate::ContainerId(container.clone()),
            owner,
            fuse_host: "host.docker.internal".to_owned(),
        };
        let root = std::env::temp_dir().join(format!(
            "layerfs-lifecycle-container-daemon-{}-{}",
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
        let workspaces =
            Workspaces::new_with_container(root.join("runtime"), store.clone(), binding).unwrap();
        // The in-container daemon validates that a Workspace root lives under
        // its own /workspace tree.
        let mount = "/workspace/layerfs-container-daemon";
        let session = workspaces
            .create_workspace_session(CreateWorkspaceSession {
                branch_id: branch,
                placement: crate::WorkspacePlacement::Container {
                    container_id: crate::ContainerId(container.clone()),
                    root: std::path::PathBuf::from(mount),
                },
                projection: Some(WorkspaceProjection::Fuse),
            })
            .unwrap();
        let worker = workspaces.worker(session.id).unwrap();
        assert!(
            worker.host_runtime().unwrap().is_some(),
            "daemon mount route must own the installed host authority"
        );
        assert!(
            worker.remote.lock().unwrap().is_none(),
            "daemon mount route must not create the legacy remote owner"
        );
        // The in-container daemon mounted this path through the host authority.
        assert_eq!(exec(&format!("cat {mount}/file")), "abcdef");
        let inode_before = exec(&format!("stat -c %i {mount}/file"));
        exec(&format!(
            "printf Z | dd of={mount}/file bs=1 seek=0 conv=notrunc status=none"
        ));
        assert_eq!(exec(&format!("cat {mount}/file")), "Zbcdef");
        let committed = workspaces.commit_workspace_session(session.id).unwrap();
        assert!(matches!(committed, WorkspaceCommitResult::Created { .. }));
        let published_root = store.pin_branch(branch).unwrap().root;
        assert_eq!(committed_file(&store, published_root), b"Zbcdef");
        assert_eq!(exec(&format!("stat -c %i {mount}/file")), inode_before);
        workspaces
            .end_workspace_session(session.id, EndWorkspaceMode::Clean)
            .unwrap();
        assert_eq!(exec(&format!("findmnt -rn -M {mount} | wc -l")), "0");
        std::fs::remove_dir_all(root).unwrap();
        println!(
            "CONTAINER DAEMON HOST AUTHORITY PASS: benchmark daemon mount route owns one host \
             authority, in-container write captured as Created, inode stable, verified unmount"
        );
    }

    /// Real container-placement host authority: a host-side session whose
    /// Workspace root lives inside a Linux container must mount through the one
    /// installed host authority (no legacy remote owner), capture an ordinary
    /// in-container write, keep the inode and unmount cleanly. Requires a
    /// running container with /dev/fuse, SYS_ADMIN and the helper binary; gated
    /// by `LAYERFS_TEST_CONTAINER`.
    #[cfg(unix)]
    #[test]
    fn container_placement_mounts_through_the_host_authority() {
        let Ok(container) = std::env::var("LAYERFS_TEST_CONTAINER") else {
            return;
        };
        let mount = "/var/tmp/layerfs-container-authority";
        let exec = |script: &str| -> String {
            let output = std::process::Command::new("docker")
                .args(["exec", &container, "/bin/sh", "-c", script])
                .output()
                .expect("docker exec");
            assert!(
                output.status.success(),
                "docker exec failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        };
        let (root, workspaces, branch, store) = fixture("container-host-authority");
        let session = workspaces
            .create_workspace_session(CreateWorkspaceSession {
                branch_id: branch,
                placement: crate::WorkspacePlacement::Container {
                    container_id: crate::ContainerId(container.clone()),
                    root: std::path::PathBuf::from(mount),
                },
                projection: Some(WorkspaceProjection::Fuse),
            })
            .unwrap();
        let worker = workspaces.worker(session.id).unwrap();
        assert!(
            worker.host_runtime().unwrap().is_some(),
            "container placement must own the installed host authority"
        );
        assert!(
            worker.remote.lock().unwrap().is_none(),
            "container placement must not create the legacy remote owner"
        );
        assert_eq!(exec(&format!("cat {mount}/file")), "abcdef");
        let inode_before = exec(&format!("stat -c %i {mount}/file"));
        // An ordinary kernel write from inside the container.
        exec(&format!(
            "printf Z | dd of={mount}/file bs=1 seek=0 conv=notrunc status=none"
        ));
        assert_eq!(exec(&format!("cat {mount}/file")), "Zbcdef");
        let committed = workspaces.commit_workspace_session(session.id).unwrap();
        assert!(matches!(committed, WorkspaceCommitResult::Created { .. }));
        let published = store.pin_branch(branch).unwrap().root;
        assert_eq!(committed_file(&store, published), b"Zbcdef");
        assert_eq!(exec(&format!("stat -c %i {mount}/file")), inode_before);
        assert!(!workspaces.diff(session.id).unwrap().dirty);
        workspaces
            .end_workspace_session(session.id, EndWorkspaceMode::Clean)
            .unwrap();
        assert_eq!(exec(&format!("findmnt -rn -M {mount} | wc -l")), "0");
        std::fs::remove_dir_all(root).unwrap();
        println!(
            "CONTAINER HOST AUTHORITY PASS: container placement owns one host authority, \
             in-container write captured as Created, inode stable, verified unmount"
        );
    }

    #[cfg(all(target_os = "linux", feature = "host-fuse"))]
    fn is_mounted_mountpoint(mountpoint: &std::path::Path) -> bool {
        use std::os::unix::ffi::OsStrExt;
        let mut encoded = Vec::new();
        for byte in mountpoint.as_os_str().as_bytes() {
            match byte {
                b' ' => encoded.extend_from_slice(br"\040"),
                b'\t' => encoded.extend_from_slice(br"\011"),
                b'\n' => encoded.extend_from_slice(br"\012"),
                b'\\' => encoded.extend_from_slice(br"\134"),
                byte => encoded.push(*byte),
            }
        }
        std::fs::read("/proc/self/mountinfo")
            .map(|mountinfo| {
                mountinfo
                    .split(|byte| *byte == b'\n')
                    .any(|line| line.split(|byte| *byte == b' ').nth(4) == Some(encoded.as_slice()))
            })
            .unwrap_or(false)
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
    pub spool_segment_bytes: u64,
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
