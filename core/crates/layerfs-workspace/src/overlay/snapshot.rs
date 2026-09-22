//! Private capture consumed by the production staging operation.
use crate::{
    backing::{
        metadata::{MetadataCharge, MetadataHost, ProgressFund, RootOwner, ESCROW},
        metadata_pages::PageRef,
    },
    *,
};
use layerfs_bridge::contract::{StageObservation, StageWire};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::Instant,
};
pub(crate) struct StageIdentity {
    pub stage: OnceLock<StageWire>,
    _charge: MetadataCharge,
}
pub(crate) struct Captured {
    pub root: Arc<RootOwner>,
    pub context: Arc<crate::runtime::state::BranchContext>,
    pub generation: u64,
    pub revision: u64,
    pub count: usize,
    pub directories: usize,
    pub fresh_files: usize,
    pub fresh_symlinks: usize,
    pub names: usize,
    pub name_bytes: usize,
}
pub(crate) type SavedInode = InodeSaveObservation;
pub(crate) struct SubmissionState {
    pub status: SubmissionStatus,
    pub result_slot: Option<usize>,
    pub pending: Option<SavedInode>,
    pub source_failure: Option<WorkspaceError>,
    /// Directory serials this submission declared as new, so a completed Commit
    /// forgets exactly the declarations the canonical state accepted. A directory
    /// a later generation created is not one of them and stays undeclared.
    pub declared: Vec<u64>,
}
pub(crate) struct Submission {
    pub captured: OnceLock<Captured>,
    pub identity: Arc<StageIdentity>,
    pub state: Mutex<SubmissionState>,
    pub results: [Arc<RootOwner>; 2],
    pub fund: Arc<ProgressFund>,
    pub failure: OnceLock<Arc<StageFailure>>,
    pub commit_claimed: AtomicBool,
    pub commit: OnceLock<Arc<crate::commit::completion::CommitAttempt>>,
    pub observed_stage: OnceLock<StageWire>,
    failure_charge: Mutex<Option<MetadataCharge>>,
    _permit: FrozenPermit,
    _charge: MetadataCharge,
}
struct FrozenPermit(Arc<AtomicBool>);
impl Drop for FrozenPermit {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
impl Submission {
    fn reserve(
        workspace: &Workspace,
        host: &Arc<MetadataHost>,
    ) -> Result<Arc<Self>, WorkspaceError> {
        workspace
            .host
            .frozen
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| WorkspaceError::Busy)?;
        let permit = FrozenPermit(workspace.host.frozen.clone());
        let charge = host.memory(32768)?;
        let failure_charge = host.memory(4096)?;
        let identity = Arc::new(StageIdentity {
            stage: OnceLock::new(),
            _charge: host.memory(1024)?,
        });
        let fund = ProgressFund::new(host)?;
        let results = host.result_roots(
            workspace
                .inner
                .arena
                .as_ref()
                .ok_or(WorkspaceError::Unsupported)?,
            &fund,
        )?;
        Ok(Arc::new(Self {
            captured: OnceLock::new(),
            identity,
            state: Mutex::new(SubmissionState {
                status: SubmissionStatus {
                    generation: 0,
                    captured_revision: 0,
                    dirty_inodes: 0,
                    phase: StagePhase::Captured,
                    inode: None,
                    saved_files: 0,
                    saved_metadata: 0,
                    stage_token: None,
                    candidate_root: None,
                    failure: None,
                    failure_phase: None,
                    commit: None,
                },
                result_slot: None,
                pending: None,
                source_failure: None,
                declared: Vec::new(),
            }),
            results,
            fund,
            failure: OnceLock::new(),
            commit_claimed: AtomicBool::new(false),
            commit: OnceLock::new(),
            observed_stage: OnceLock::new(),
            failure_charge: Mutex::new(Some(failure_charge)),
            _permit: permit,
            _charge: charge,
        }))
    }
    pub fn capture(&self) -> Result<&Captured, WorkspaceError> {
        self.captured.get().ok_or(WorkspaceError::Io)
    }
    pub fn status(&self) -> Result<SubmissionStatus, WorkspaceError> {
        Ok(self.state.lock().map_err(|_| WorkspaceError::Io)?.status)
    }
    pub fn phase(&self, phase: StagePhase, inode: Option<u64>) -> Result<(), WorkspaceError> {
        let mut s = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        s.status.phase = phase;
        s.status.inode = inode;
        Ok(())
    }
    pub fn fail(&self, cause: WorkspaceError) -> WorkspaceError {
        let Ok(captured) = self.capture() else {
            return cause;
        };
        let (phase, source_failure, pending) = self
            .state
            .lock()
            .map_or((StagePhase::LocalBookkeeping, None, None), |s| {
                (s.status.phase, s.source_failure.clone(), s.pending)
            });
        let (unknown, mut observed) = match &cause {
            WorkspaceError::Service(failure) => {
                let observed = failure
                    .history
                    .as_ref()
                    .and_then(|history| match &history.stage {
                        StageObservation::Retained(stage)
                        | StageObservation::AcknowledgedUnknown(stage) => Some((**stage).clone()),
                        _ => None,
                    });
                let uncertain = failure.unknown
                    || failure.history.as_ref().is_some_and(|history| {
                        matches!(history.stage, StageObservation::AcknowledgedUnknown(_))
                    });
                (uncertain, observed)
            }
            WorkspaceError::Backing(failure) => (!failure.accounting_complete, None),
            _ => (false, None),
        };
        if observed.is_none() {
            observed = self.observed_stage.get().cloned();
        }
        let known_stage = self.identity.stage.get().map(|_| StageSelector {
            identity: self.identity.clone(),
        });
        let disposition = if known_stage.is_some() {
            StageFailureDisposition::KnownStageLocalFailure
        } else if unknown {
            StageFailureDisposition::Unknown
        } else {
            StageFailureDisposition::KnownBeforeStage
        };
        let Ok(mut reserved) = self.failure_charge.lock() else {
            return cause;
        };
        let Some(charge) = reserved.take() else {
            return cause;
        };
        drop(reserved);
        let failure = Arc::new(StageFailure {
            generation: captured.generation,
            phase,
            disposition,
            cause,
            observed_stage: observed,
            known_stage,
            source_failure,
            pending,
            _charge: charge,
        });
        let _ = self.failure.set(failure.clone());
        if let Ok(mut state) = self.state.lock() {
            state.status.phase = StagePhase::Failed;
            state.status.failure_phase = Some(phase);
            state.status.failure = Some(disposition);
            if let Some(stage) = self.identity.stage.get() {
                state.status.stage_token = Some(stage.token);
                state.status.candidate_root = Some(stage.candidate_root);
            }
        }
        WorkspaceError::Stage(failure)
    }
    pub fn acknowledged(&self, stage: StageWire) -> Result<StageSelector, WorkspaceError> {
        self.identity
            .stage
            .set(stage)
            .map_err(|_| WorkspaceError::Io)?;
        let result = self.identity.stage.get().ok_or(WorkspaceError::Io)?;
        let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        state.status.phase = StagePhase::Staged;
        state.status.inode = None;
        state.status.stage_token = Some(result.token);
        state.status.candidate_root = Some(result.candidate_root);
        Ok(StageSelector {
            identity: self.identity.clone(),
        })
    }
    pub fn result_root(&self) -> Result<Option<&Arc<RootOwner>>, WorkspaceError> {
        let slot = self
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .result_slot;
        Ok(slot.map(|slot| &self.results[slot]))
    }
    pub fn result_ref(&self) -> Result<PageRef, WorkspaceError> {
        self.result_root()?
            .map_or(Ok(PageRef::NULL), |root| root.root())
    }
}
impl Workspace {
    pub(crate) fn capture_submission(
        &self,
        allow_clean: bool,
        deadline: Instant,
    ) -> Result<Arc<Submission>, WorkspaceError> {
        if self.inner.access != WorkspaceAccess::LocalEdit {
            return Err(WorkspaceError::ReadOnly);
        }
        let initial_clean = {
            let state = self.state()?;
            self.available(&state)?;
            if state.submission.is_some() {
                return Err(WorkspaceError::Busy);
            }
            if state.dirty_inodes == 0 && !allow_clean {
                return Err(WorkspaceError::InvalidInput);
            }
            (state.dirty_inodes == 0).then_some(state.generation)
        };
        self.maintain_backing(deadline)?;
        let host = self
            .host
            .metadata
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        let submission = Submission::reserve(self, host)?;
        let clean_root = match initial_clean
            .map(|generation| {
                host.clean_capture_root(
                    self.inner
                        .arena
                        .as_ref()
                        .ok_or(WorkspaceError::Unsupported)?,
                    generation,
                )
            })
            .transpose()
        {
            Ok(root) => root,
            Err(error) => {
                host.remove_empty_result_roots(&submission.results)?;
                return Err(error);
            }
        };
        let result = (|| -> Result<(), WorkspaceError> {
            crate::backing::payload::clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
            let mut state = self.state()?;
            self.available(&state)?;
            if state.submission.is_some() {
                return Err(WorkspaceError::Busy);
            }
            if initial_clean.is_some() != (state.dirty_inodes == 0)
                || initial_clean.is_some_and(|generation| generation != state.generation)
            {
                return Err(WorkspaceError::Busy);
            }
            let generation = state.generation;
            let next = generation
                .checked_add(1)
                .filter(|g| *g <= i64::MAX as u64)
                .ok_or(WorkspaceError::Capacity)?;
            let revision = state.revision;
            let next_revision = revision.checked_add(1).ok_or(WorkspaceError::Capacity)?;
            let root = clean_root
                .as_ref()
                .cloned()
                .or_else(|| state.overlay.clone())
                .ok_or(WorkspaceError::Io)?;
            let context = state.branch.clone().ok_or(WorkspaceError::Unsupported)?;
            if clean_root.is_none() {
                let completion = state.completion.as_ref().ok_or(WorkspaceError::Io)?;
                if completion.generation != generation || completion.bytes != ESCROW {
                    return Err(WorkspaceError::Io);
                }
            } else if state.completion.is_some() {
                return Err(WorkspaceError::Io);
            }
            let count = state.dirty_inodes;
            let directories = state.dirty_directories;
            let fresh_files = state.fresh_files;
            let fresh_symlinks = state.fresh_symlinks;
            let names = state.directory_names;
            let name_bytes = state.directory_bytes;
            state.frontier_bytes(
                count,
                directories,
                fresh_files,
                fresh_symlinks,
                names,
                name_bytes,
            )?;
            {
                let mut s = submission.state.lock().map_err(|_| WorkspaceError::Io)?;
                s.status.generation = generation;
                s.status.captured_revision = revision;
                s.status.dirty_inodes = count;
            }
            let completion = if let Some(root) = &clean_root {
                root.take_completion(generation)?
                    .ok_or(WorkspaceError::Io)?
            } else {
                state.completion.take().ok_or(WorkspaceError::Io)?
            };
            submission.fund.install(completion)?;
            if submission
                .captured
                .set(Captured {
                    root,
                    context,
                    generation,
                    revision,
                    count,
                    directories,
                    fresh_files,
                    fresh_symlinks,
                    names,
                    name_bytes,
                })
                .is_err()
            {
                unreachable!("one capture per reserved submission")
            }
            if let Some(root) = &clean_root {
                state.overlay = Some(root.clone());
            }
            state.generation = next;
            state.revision = next_revision;
            state.dirty_inodes = 0;
            state.dirty_directories = 0;
            state.fresh_files = 0;
            state.fresh_symlinks = 0;
            state.directory_names = 0;
            state.directory_bytes = 0;
            state.fresh.clear();
            state.submission = Some(submission.clone());
            Ok(())
        })();
        if let Err(error) = result {
            if let Some(root) = &clean_root {
                host.release_clean_capture_root(root)?;
            }
            host.remove_empty_result_roots(&submission.results)?;
            return Err(error);
        }
        Ok(submission)
    }
}
