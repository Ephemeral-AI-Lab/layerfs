//! Commit terminals and writable Status, using the existing C5 payload grammars.
use super::{
    control::{put_status, take_status},
    history_failure::{decode_contextual_failure, encode_contextual_failure},
    metadata::{put_optional, take_optional},
    response::{put_commit_outcome, put_stage, take_commit_outcome, take_stage},
    Decoder, Encoder,
};
use crate::contract::*;

pub(super) fn put_workspace_commit(
    e: &mut Encoder,
    result: &WorkspaceCommitWire,
) -> Result<(), Failure> {
    result.validate()?;
    e.blob(&result.workspace)?;
    e.put(&result.incarnation)?;
    match &result.outcome {
        WorkspaceCommitOutcome::Completed(report) => {
            e.u8(0)?;
            e.u64(report.generation)?;
            put_optional(e, report.stage_token.map(u64::to_be_bytes).as_ref())?;
            put_commit_outcome(e, &report.outcome)?;
            e.u64(report.revision)?;
        }
        WorkspaceCommitOutcome::Failed(failure) => {
            e.u8(1)?;
            e.u64(failure.generation)?;
            e.u8(failure.phase as u8)?;
            e.u8(failure.disposition as u8)?;
            e.blob(&encode_contextual_failure(&failure.cause)?)?;
            for stage in [&failure.known_stage, &failure.observed_stage] {
                e.u8(u8::from(stage.is_some()))?;
                if let Some(stage) = stage {
                    put_stage(e, stage)?;
                }
            }
            for outcome in [&failure.known_outcome, &failure.observed_outcome] {
                e.u8(u8::from(outcome.is_some()))?;
                if let Some(outcome) = outcome {
                    put_commit_outcome(e, outcome)?;
                }
            }
            put_optional(e, failure.installed_revision.map(u64::to_be_bytes).as_ref())?;
        }
    }
    Ok(())
}
pub(super) fn take_workspace_commit(d: &mut Decoder<'_>) -> Result<WorkspaceCommitWire, Failure> {
    let workspace = d.blob(WORKSPACE_ID_BYTES)?;
    let incarnation = d.root()?;
    let outcome = match d.u8()? {
        0 => WorkspaceCommitOutcome::Completed(WorkspaceCommitReportWire {
            generation: d.u64()?,
            stage_token: take_optional::<8>(d)?.map(u64::from_be_bytes),
            outcome: take_commit_outcome(d)?,
            revision: d.u64()?,
        }),
        1 => WorkspaceCommitOutcome::Failed(Box::new(WorkspaceCommitFailureWire {
            generation: d.u64()?,
            phase: commit_phase(d.u8()?)?,
            disposition: commit_disposition(d.u8()?)?,
            cause: decode_contextual_failure(&d.blob(HISTORY_FAILURE_BYTES)?)?,
            known_stage: optional_stage(d)?,
            observed_stage: optional_stage(d)?,
            known_outcome: optional_outcome(d)?,
            observed_outcome: optional_outcome(d)?,
            installed_revision: take_optional::<8>(d)?.map(u64::from_be_bytes),
        })),
        _ => return Err(Code::InvalidInput.into()),
    };
    let result = WorkspaceCommitWire {
        workspace,
        incarnation,
        outcome,
    };
    result.validate()?;
    Ok(result)
}
fn optional_stage(d: &mut Decoder<'_>) -> Result<Option<StageWire>, Failure> {
    match d.u8()? {
        0 => Ok(None),
        1 => Ok(Some(take_stage(d)?)),
        _ => Err(Code::InvalidInput.into()),
    }
}
fn optional_outcome(d: &mut Decoder<'_>) -> Result<Option<CommitOutcomeWire>, Failure> {
    match d.u8()? {
        0 => Ok(None),
        1 => Ok(Some(take_commit_outcome(d)?)),
        _ => Err(Code::InvalidInput.into()),
    }
}
pub(super) fn put_writable_status(
    e: &mut Encoder,
    status: &WorkspaceWritableStatusWire,
) -> Result<(), Failure> {
    status.validate()?;
    put_status(e, &status.status)?;
    e.u64(status.generation)?;
    e.u64(status.revision)?;
    e.u64(status.dirty_inodes)?;
    e.u8(u8::from(status.submission.is_some()))?;
    if let Some(submission) = &status.submission {
        e.u64(submission.generation)?;
        e.u64(submission.captured_revision)?;
        e.u64(submission.dirty_inodes)?;
        e.u8(submission.phase as u8)?;
        put_optional(e, submission.inode.map(u64::to_be_bytes).as_ref())?;
        e.u16(submission.saved_files)?;
        e.u16(submission.saved_metadata)?;
        put_optional(e, submission.stage_token.map(u64::to_be_bytes).as_ref())?;
        put_optional(e, submission.candidate_root.as_ref())?;
        e.u8(submission.failure.map_or(0, |value| value as u8))?;
        e.u8(submission.failure_phase.map_or(0, |value| value as u8))?;
        e.u8(u8::from(submission.commit.is_some()))?;
        if let Some(commit) = &submission.commit {
            e.u8(commit.phase as u8)?;
            put_optional(e, commit.known_root.as_ref())?;
            put_optional(e, commit.known_head.as_ref())?;
            put_optional(e, commit.installed_revision.map(u64::to_be_bytes).as_ref())?;
            e.u8(commit.failure.map_or(0, |value| value as u8))?;
        }
    }
    Ok(())
}
pub(super) fn take_writable_status(
    d: &mut Decoder<'_>,
) -> Result<WorkspaceWritableStatusWire, Failure> {
    let status = take_status(d)?;
    let generation = d.u64()?;
    let revision = d.u64()?;
    let dirty_inodes = d.u64()?;
    let submission = match d.u8()? {
        0 => None,
        1 => Some(WorkspaceSubmissionWire {
            generation: d.u64()?,
            captured_revision: d.u64()?,
            dirty_inodes: d.u64()?,
            phase: stage_phase(d.u8()?)?,
            inode: take_optional::<8>(d)?.map(u64::from_be_bytes),
            saved_files: d.u16()?,
            saved_metadata: d.u16()?,
            stage_token: take_optional::<8>(d)?.map(u64::from_be_bytes),
            candidate_root: take_optional::<32>(d)?,
            failure: match d.u8()? {
                0 => None,
                value => Some(stage_disposition(value)?),
            },
            failure_phase: match d.u8()? {
                0 => None,
                value => Some(stage_phase(value)?),
            },
            commit: match d.u8()? {
                0 => None,
                1 => Some(WorkspaceCommitStatusWire {
                    phase: commit_phase(d.u8()?)?,
                    known_root: take_optional::<32>(d)?,
                    known_head: take_optional::<33>(d)?,
                    installed_revision: take_optional::<8>(d)?.map(u64::from_be_bytes),
                    failure: match d.u8()? {
                        0 => None,
                        value => Some(commit_disposition(value)?),
                    },
                }),
                _ => return Err(Code::InvalidInput.into()),
            },
        }),
        _ => return Err(Code::InvalidInput.into()),
    };
    let result = WorkspaceWritableStatusWire {
        status,
        generation,
        revision,
        dirty_inodes,
        submission,
    };
    result.validate()?;
    Ok(result)
}
fn commit_phase(value: u8) -> Result<WorkspaceCommitPhase, Failure> {
    Ok(match value {
        1 => WorkspaceCommitPhase::Preparing,
        2 => WorkspaceCommitPhase::CommitStaged,
        3 => WorkspaceCommitPhase::CompositeCommit,
        4 => WorkspaceCommitPhase::Reconcile,
        5 => WorkspaceCommitPhase::Complete,
        _ => return Err(Code::InvalidInput.into()),
    })
}
fn commit_disposition(value: u8) -> Result<WorkspaceCommitFailureDisposition, Failure> {
    Ok(match value {
        1 => WorkspaceCommitFailureDisposition::KnownBeforeCommit,
        2 => WorkspaceCommitFailureDisposition::Unknown,
        3 => WorkspaceCommitFailureDisposition::KnownCommitLocalFailure,
        _ => return Err(Code::InvalidInput.into()),
    })
}
fn stage_phase(value: u8) -> Result<WorkspaceStagePhase, Failure> {
    Ok(match value {
        1 => WorkspaceStagePhase::Captured,
        2 => WorkspaceStagePhase::FileSave,
        3 => WorkspaceStagePhase::MetadataSave,
        4 => WorkspaceStagePhase::StageChanges,
        5 => WorkspaceStagePhase::Staged,
        6 => WorkspaceStagePhase::Failed,
        7 => WorkspaceStagePhase::LocalBookkeeping,
        _ => return Err(Code::InvalidInput.into()),
    })
}
fn stage_disposition(value: u8) -> Result<WorkspaceStageFailureDisposition, Failure> {
    Ok(match value {
        1 => WorkspaceStageFailureDisposition::KnownBeforeStage,
        2 => WorkspaceStageFailureDisposition::Unknown,
        3 => WorkspaceStageFailureDisposition::KnownStageLocalFailure,
        _ => return Err(Code::InvalidInput.into()),
    })
}
