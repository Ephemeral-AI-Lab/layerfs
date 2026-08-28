use crate::durable::commit_timers;
use crate::{CommitResult, Error, LayerFs, OperationCommitTimers, OperationId, Result, VersionRef};
use layerfs_core::{CanonicalPath, ObjectId};
use layerfs_materialization::{MaterializationDriver, OperationCounters};
use layerfs_working_store::BeginOperation;
use layerfs_workspace::{EndOperationReceipt, OperationWorkspace};
use std::path::Path;
use std::time::{Duration, Instant};

pub struct MaterializationCommitReceipt {
    pub operation_id: OperationId,
    pub candidate_root: ObjectId,
    pub outcome: CommitResult,
    pub cleanup: layerfs_workspace::Result<EndOperationReceipt>,
    pub acknowledgement: Option<layerfs_working_store::Result<bool>>,
    pub counters: OperationCounters,
    pub timers: OperationCommitTimers,
}

pub struct ManagedMaterializationCommitReceipt {
    pub operation_id: OperationId,
    pub candidate_root: ObjectId,
    pub outcome: Option<CommitResult>,
    pub cleanup: layerfs_workspace::Result<EndOperationReceipt>,
    pub acknowledgement: Option<layerfs_working_store::Result<bool>>,
    pub counters: OperationCounters,
    pub refresh_counters: OperationCounters,
    pub timers: OperationCommitTimers,
}

pub struct MaterializedOperation<'a> {
    fs: &'a LayerFs,
    admission: BeginOperation,
    workspace: OperationWorkspace<MaterializationDriver<'a>>,
    terminal: bool,
}

impl<'a> MaterializedOperation<'a> {
    pub(crate) fn new(
        fs: &'a LayerFs,
        admission: BeginOperation,
        workspace: OperationWorkspace<MaterializationDriver<'a>>,
    ) -> Self {
        Self {
            fs,
            admission,
            workspace,
            terminal: false,
        }
    }

    pub fn path(&self) -> &Path {
        self.workspace
            .paths()
            .expect("materialization always has custody")
            .view()
    }

    pub fn leases(&self) -> &layerfs_workspace::RuntimeLeases {
        self.workspace.leases()
    }

    pub fn managed_replace_range(
        &mut self,
        path: &str,
        start: u64,
        delete_len: u64,
        replacement: &[u8],
    ) -> Result<OperationCounters> {
        Ok(self.workspace.driver_mut().managed_replace_range(
            &CanonicalPath::new(path)?,
            start,
            delete_len,
            replacement,
        )?)
    }

    pub fn managed_rename(&mut self, from: &str, to: &str) -> Result<OperationCounters> {
        Ok(self
            .workspace
            .driver_mut()
            .managed_rename(&CanonicalPath::new(from)?, &CanonicalPath::new(to)?)?)
    }

    pub fn commit(mut self) -> Result<MaterializationCommitReceipt> {
        let complete = Instant::now();
        let freeze = self.workspace.freeze_observed(Duration::from_secs(30))?;
        let candidate_started = Instant::now();
        let candidate = self
            .workspace
            .driver()
            .candidate_root()
            .ok_or(Error::Workspace(
                layerfs_workspace::WorkspaceError::InvalidState,
            ))?;
        let finalized =
            self.workspace
                .finalize_candidate(self.admission.base.root(), candidate, Vec::new())?;
        let candidate_ns = freeze.driver_freeze_ns + candidate_started.elapsed().as_nanos();
        let working_commit = Instant::now();
        let result = self
            .fs
            .working
            .operation_commit(self.admission, finalized.into_working());
        let working_commit_ns = working_commit.elapsed().as_nanos();
        match result {
            Ok(outcome) => {
                let counters = self.workspace.driver().counters();
                self.terminal = true;
                let cleanup_started = Instant::now();
                let cleanup = self.workspace.cleanup();
                let cleanup_ns = cleanup_started.elapsed().as_nanos();
                let acknowledgement = if cleanup.is_ok() {
                    match outcome {
                        CommitResult::WorkingRecorded { record, .. } => {
                            Some(self.fs.working.acknowledge_operation(record))
                        }
                        CommitResult::Conflict { candidate, .. } => {
                            Some(self.fs.working.acknowledge_conflict(candidate))
                        }
                    }
                } else {
                    None
                };
                Ok(MaterializationCommitReceipt {
                    operation_id: self.admission.operation_id,
                    candidate_root: candidate,
                    outcome,
                    cleanup,
                    acknowledgement,
                    counters,
                    timers: commit_timers(
                        complete,
                        freeze.quiescence_ns,
                        candidate_ns,
                        working_commit_ns,
                        cleanup_ns,
                    ),
                })
            }
            Err(error) => {
                if self.workspace.discard().is_ok() {
                    self.terminal = self
                        .fs
                        .working
                        .discard_operation(self.admission.operation_id)
                        .is_ok();
                }
                Err(error.into())
            }
        }
    }

    pub fn discard(mut self) -> Result<()> {
        self.workspace.discard()?;
        self.fs
            .working
            .discard_operation(self.admission.operation_id)?;
        self.terminal = true;
        Ok(())
    }

    pub fn operation_id(&self) -> OperationId {
        self.admission.operation_id
    }
}

impl Drop for MaterializedOperation<'_> {
    fn drop(&mut self) {
        if !self.terminal
            && self.workspace.discard().is_ok()
            && self
                .fs
                .working
                .discard_operation(self.admission.operation_id)
                .is_ok()
        {
            self.terminal = true;
        }
    }
}

pub struct ManagedMaterializedOperation<'a> {
    fs: &'a LayerFs,
    admission: BeginOperation,
    workspace: OperationWorkspace<MaterializationDriver<'a>>,
    terminal: bool,
}

impl<'a> ManagedMaterializedOperation<'a> {
    pub(crate) fn new(
        fs: &'a LayerFs,
        admission: BeginOperation,
        workspace: OperationWorkspace<MaterializationDriver<'a>>,
    ) -> Self {
        Self {
            fs,
            admission,
            workspace,
            terminal: false,
        }
    }

    pub fn refresh_to(&mut self, target: VersionRef) -> Result<OperationCounters> {
        self.fs.working.validate_version_ref(target)?;
        Ok(self.workspace.driver_mut().refresh_to(target)?)
    }

    pub fn read(&self, path: &str, start: u64, length: usize) -> Result<Vec<u8>> {
        Ok(self
            .workspace
            .driver()
            .managed_read(&CanonicalPath::new(path)?, start, length)?)
    }

    pub fn commit(mut self) -> Result<ManagedMaterializationCommitReceipt> {
        let complete = Instant::now();
        let freeze = self.workspace.freeze_observed(Duration::from_secs(30))?;
        let candidate_started = Instant::now();
        let candidate = self
            .workspace
            .driver()
            .candidate_root()
            .ok_or(Error::Workspace(
                layerfs_workspace::WorkspaceError::InvalidState,
            ))?;
        let counters = self.workspace.driver().counters();
        let refresh_counters = self.workspace.driver().refresh_counters();
        if candidate == self.admission.base.root() {
            let candidate_ns = freeze.driver_freeze_ns + candidate_started.elapsed().as_nanos();
            let cleanup_started = Instant::now();
            let cleanup = self.workspace.cleanup();
            let cleanup_ns = cleanup_started.elapsed().as_nanos();
            cleanup.as_ref().map_err(|error| {
                Error::Workspace(match error {
                    layerfs_workspace::WorkspaceError::Busy => {
                        layerfs_workspace::WorkspaceError::Busy
                    }
                    _ => layerfs_workspace::WorkspaceError::InvalidState,
                })
            })?;
            self.fs
                .working
                .discard_operation(self.admission.operation_id)?;
            self.terminal = true;
            return Ok(ManagedMaterializationCommitReceipt {
                operation_id: self.admission.operation_id,
                candidate_root: candidate,
                outcome: None,
                cleanup,
                acknowledgement: None,
                counters,
                refresh_counters,
                timers: commit_timers(complete, freeze.quiescence_ns, candidate_ns, 0, cleanup_ns),
            });
        }
        let finalized =
            self.workspace
                .finalize_candidate(self.admission.base.root(), candidate, Vec::new())?;
        let candidate_ns = freeze.driver_freeze_ns + candidate_started.elapsed().as_nanos();
        let working_commit = Instant::now();
        let result = self
            .fs
            .working
            .operation_commit(self.admission, finalized.into_working());
        let working_commit_ns = working_commit.elapsed().as_nanos();
        match result {
            Ok(outcome) => {
                self.terminal = true;
                let cleanup_started = Instant::now();
                let cleanup = self.workspace.cleanup();
                let cleanup_ns = cleanup_started.elapsed().as_nanos();
                let acknowledgement = if cleanup.is_ok() {
                    match outcome {
                        CommitResult::WorkingRecorded { record, .. } => {
                            Some(self.fs.working.acknowledge_operation(record))
                        }
                        CommitResult::Conflict { candidate, .. } => {
                            Some(self.fs.working.acknowledge_conflict(candidate))
                        }
                    }
                } else {
                    None
                };
                Ok(ManagedMaterializationCommitReceipt {
                    operation_id: self.admission.operation_id,
                    candidate_root: candidate,
                    outcome: Some(outcome),
                    cleanup,
                    acknowledgement,
                    counters,
                    refresh_counters,
                    timers: commit_timers(
                        complete,
                        freeze.quiescence_ns,
                        candidate_ns,
                        working_commit_ns,
                        cleanup_ns,
                    ),
                })
            }
            Err(error) => {
                if self.workspace.discard().is_ok() {
                    self.terminal = self
                        .fs
                        .working
                        .discard_operation(self.admission.operation_id)
                        .is_ok();
                }
                Err(error.into())
            }
        }
    }

    pub fn discard(mut self) -> Result<()> {
        self.workspace.discard()?;
        self.fs
            .working
            .discard_operation(self.admission.operation_id)?;
        self.terminal = true;
        Ok(())
    }
}

impl Drop for ManagedMaterializedOperation<'_> {
    fn drop(&mut self) {
        if !self.terminal
            && self.workspace.discard().is_ok()
            && self
                .fs
                .working
                .discard_operation(self.admission.operation_id)
                .is_ok()
        {
            self.terminal = true;
        }
    }
}
