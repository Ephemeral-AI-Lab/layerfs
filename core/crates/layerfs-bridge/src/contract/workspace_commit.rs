//! Explicit Workspace Commit and writable observations; stage records are not capabilities.
use super::{
    control::check_workspace_identity,
    history::{check_commit, check_stage, serial, tag},
    Code, CommitOutcomeWire, Failure, Root, StageWire, WorkspaceStatusWire, MAX_OPERATION_MS,
};

pub const WORKSPACE_COMMIT_OPCODE: u8 = 14;
pub const WORKSPACE_COMMIT_MAX_MS: u32 = MAX_OPERATION_MS;
pub const WORKSPACE_COMMIT_REQUEST_BYTES: usize = 124;
pub const WORKSPACE_COMMIT_RESULT_BYTES: usize = 1590;
pub const WORKSPACE_WRITABLE_STATUS_RESULT_BYTES: usize = 725;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceCommitWire {
    pub workspace: Vec<u8>,
    pub incarnation: Root,
    pub outcome: WorkspaceCommitOutcome,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceCommitOutcome {
    Completed(WorkspaceCommitReportWire),
    Failed(Box<WorkspaceCommitFailureWire>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceCommitReportWire {
    pub generation: u64,
    pub stage_token: Option<u64>,
    pub outcome: CommitOutcomeWire,
    pub revision: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum WorkspaceCommitPhase {
    Preparing = 1,
    CommitStaged = 2,
    CompositeCommit = 3,
    Reconcile = 4,
    Complete = 5,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum WorkspaceCommitFailureDisposition {
    KnownBeforeCommit = 1,
    Unknown = 2,
    KnownCommitLocalFailure = 3,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceCommitFailureWire {
    pub generation: u64,
    pub phase: WorkspaceCommitPhase,
    pub disposition: WorkspaceCommitFailureDisposition,
    pub cause: Failure,
    /// A known local stage observation, never a remotely usable StageSelector.
    pub known_stage: Option<StageWire>,
    /// An observed service record, which may not identify this capture.
    pub observed_stage: Option<StageWire>,
    pub known_outcome: Option<CommitOutcomeWire>,
    pub observed_outcome: Option<CommitOutcomeWire>,
    pub installed_revision: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceWritableStatusWire {
    pub status: WorkspaceStatusWire,
    pub generation: u64,
    pub revision: u64,
    pub dirty_inodes: u64,
    pub submission: Option<WorkspaceSubmissionWire>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum WorkspaceStagePhase {
    Captured = 1,
    FileSave = 2,
    MetadataSave = 3,
    StageChanges = 4,
    Staged = 5,
    Failed = 6,
    LocalBookkeeping = 7,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum WorkspaceStageFailureDisposition {
    KnownBeforeStage = 1,
    Unknown = 2,
    KnownStageLocalFailure = 3,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkspaceSubmissionWire {
    pub generation: u64,
    pub captured_revision: u64,
    pub dirty_inodes: u64,
    pub phase: WorkspaceStagePhase,
    pub inode: Option<u64>,
    pub saved_files: u16,
    pub saved_metadata: u16,
    pub stage_token: Option<u64>,
    pub candidate_root: Option<Root>,
    pub failure: Option<WorkspaceStageFailureDisposition>,
    pub failure_phase: Option<WorkspaceStagePhase>,
    pub commit: Option<WorkspaceCommitStatusWire>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkspaceCommitStatusWire {
    pub phase: WorkspaceCommitPhase,
    pub known_root: Option<Root>,
    pub known_head: Option<[u8; 33]>,
    pub installed_revision: Option<u64>,
    pub failure: Option<WorkspaceCommitFailureDisposition>,
}

impl WorkspaceCommitWire {
    pub fn validate(&self) -> Result<(), Failure> {
        check_workspace_identity(&self.workspace, &self.incarnation)?;
        match &self.outcome {
            WorkspaceCommitOutcome::Completed(report) => {
                serial(report.generation)?;
                if let Some(token) = report.stage_token {
                    serial(token)?;
                }
                if report.revision == 0 {
                    return Err(Code::InvalidInput.into());
                }
                check_commit_outcome(&report.outcome)
            }
            WorkspaceCommitOutcome::Failed(failure) => {
                serial(failure.generation)?;
                if failure.cause.code == Code::Unknown && !failure.cause.unknown {
                    return Err(Code::InvalidInput.into());
                }
                if let Some(stage) = &failure.known_stage {
                    check_stage(stage)?;
                    if stage.workspace != self.incarnation || stage.generation != failure.generation
                    {
                        return Err(Code::InvalidInput.into());
                    }
                }
                if let Some(stage) = &failure.observed_stage {
                    check_stage(stage)?;
                }
                if let Some(outcome) = &failure.known_outcome {
                    check_commit_outcome(outcome)?;
                }
                if let Some(outcome) = &failure.observed_outcome {
                    check_commit_outcome(outcome)?;
                }
                let valid = match failure.disposition {
                    WorkspaceCommitFailureDisposition::KnownBeforeCommit => {
                        failure.known_outcome.is_none()
                            && failure.observed_outcome.is_none()
                            && failure.installed_revision.is_none()
                    }
                    WorkspaceCommitFailureDisposition::Unknown => {
                        failure.known_outcome.is_none() && failure.installed_revision.is_none()
                    }
                    WorkspaceCommitFailureDisposition::KnownCommitLocalFailure => {
                        failure.known_outcome.is_some()
                            && failure.known_outcome == failure.observed_outcome
                    }
                };
                if !valid || failure.installed_revision == Some(0) {
                    return Err(Code::InvalidInput.into());
                }
                Ok(())
            }
        }
    }
}
impl WorkspaceWritableStatusWire {
    pub fn validate(&self) -> Result<(), Failure> {
        self.status.validate()?;
        serial(self.generation)?;
        if self.status.closed && (self.dirty_inodes != 0 || self.submission.is_some()) {
            return Err(Code::InvalidInput.into());
        }
        if let Some(submission) = &self.submission {
            serial(submission.generation)?;
            if submission.generation.checked_add(1) != Some(self.generation)
                || submission.captured_revision >= self.revision
                || submission.failure.is_some() != submission.failure_phase.is_some()
            {
                return Err(Code::InvalidInput.into());
            }
            if let Some(inode) = submission.inode {
                serial(inode)?;
            }
            if let Some(token) = submission.stage_token {
                serial(token)?;
            }
            if let Some(commit) = &submission.commit {
                if let Some(head) = &commit.known_head {
                    tag(head, 0x12)?;
                }
                if commit
                    .installed_revision
                    .is_some_and(|revision| revision == 0 || revision > self.revision)
                {
                    return Err(Code::InvalidInput.into());
                }
            }
        }
        Ok(())
    }
}
pub(crate) fn check_commit_outcome(outcome: &CommitOutcomeWire) -> Result<(), Failure> {
    match outcome {
        CommitOutcomeWire::Committed(record) => check_commit(record),
        CommitOutcomeWire::UpToDate {
            head: Some(head), ..
        } => tag(head, 0x12),
        CommitOutcomeWire::UpToDate { head: None, .. } => Ok(()),
    }
}
