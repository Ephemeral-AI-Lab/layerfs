//! Native Commit outcomes and writable observations on the daemon control route.
use crate::lifecycle::{failure_code, Selected};
use layerfs_bridge::contract as wire;
use layerfs_workspace as native;
use std::time::Instant;

pub(crate) fn commit(
    selected: &Selected,
    workspace: &native::Workspace,
    deadline: Instant,
) -> Result<wire::Response, wire::Failure> {
    let identity = selected.id.as_bytes().to_vec();
    let before = workspace.status().map_err(|error| cause(&error))?;
    let outcome = match workspace.commit(deadline) {
        Ok(report) => wire::WorkspaceCommitOutcome::Completed(wire::WorkspaceCommitReportWire {
            generation: report.generation,
            stage_token: report.stage_token,
            outcome: report.outcome,
            revision: report.revision,
        }),
        Err(native::WorkspaceError::Commit(failure)) => {
            wire::WorkspaceCommitOutcome::Failed(Box::new(wire::WorkspaceCommitFailureWire {
                generation: failure.generation,
                phase: commit_phase(failure.phase),
                disposition: commit_disposition(failure.disposition),
                cause: cause(&failure.cause),
                known_stage: failure
                    .stage
                    .as_ref()
                    .map(|selector| selector.stage().clone()),
                observed_stage: failure.observed_stage.clone(),
                known_outcome: failure.known_outcome.clone(),
                observed_outcome: failure.observed_outcome.clone(),
                installed_revision: failure.installed_revision,
            }))
        }
        Err(native::WorkspaceError::Stage(failure)) => {
            // Capture already owns G even if reserving its Commit attempt failed.
            // These records describe that capture; they are not stage capabilities.
            wire::WorkspaceCommitOutcome::Failed(Box::new(wire::WorkspaceCommitFailureWire {
                generation: failure.generation,
                phase: wire::WorkspaceCommitPhase::Preparing,
                disposition: if failure.disposition == native::StageFailureDisposition::Unknown {
                    wire::WorkspaceCommitFailureDisposition::Unknown
                } else {
                    wire::WorkspaceCommitFailureDisposition::KnownBeforeCommit
                },
                cause: cause(&failure.cause),
                known_stage: failure
                    .known_stage
                    .as_ref()
                    .map(|selector| selector.stage().clone()),
                observed_stage: failure.observed_stage.clone(),
                known_outcome: None,
                observed_outcome: None,
                installed_revision: None,
            }))
        }
        Err(error) => {
            let mut failure = cause(&error);
            // Failure bookkeeping can itself be unavailable after capture or
            // publication. Only an unchanged native observation proves refusal.
            if !workspace.status().is_ok_and(|after| {
                after.generation == before.generation && after.submission == before.submission
            }) {
                failure.unknown = true;
            }
            return Err(failure);
        }
    };
    let result = wire::WorkspaceCommitWire {
        workspace: identity,
        incarnation: selected.incarnation,
        outcome,
    };
    // A conversion refusal after native entry must not imply that Commit aborted.
    result.validate().map_err(|mut failure| {
        failure.unknown = true;
        failure
    })?;
    Ok(wire::Response::WorkspaceCommit(Box::new(result)))
}

pub(crate) fn status(
    status: wire::WorkspaceStatusWire,
    local: native::WorkspaceStatus,
) -> Result<wire::Response, wire::Failure> {
    let result = wire::WorkspaceWritableStatusWire {
        status,
        generation: local.generation,
        revision: local.revision,
        dirty_inodes: local.dirty_inodes as u64,
        submission: local
            .submission
            .map(|submission| wire::WorkspaceSubmissionWire {
                generation: submission.generation,
                captured_revision: submission.captured_revision,
                dirty_inodes: submission.dirty_inodes as u64,
                phase: stage_phase(submission.phase),
                inode: submission.inode,
                saved_files: submission.saved_files,
                saved_metadata: submission.saved_metadata,
                stage_token: submission.stage_token,
                candidate_root: submission.candidate_root,
                failure: submission.failure.map(stage_disposition),
                failure_phase: submission.failure_phase.map(stage_phase),
                commit: submission
                    .commit
                    .map(|commit| wire::WorkspaceCommitStatusWire {
                        phase: commit_phase(commit.phase),
                        known_root: commit.known_root,
                        known_head: commit.known_head,
                        installed_revision: commit.installed_revision,
                        failure: commit.failure.map(commit_disposition),
                    }),
            }),
    };
    result.validate()?;
    Ok(wire::Response::WorkspaceWritableStatus(Box::new(result)))
}

fn cause(error: &native::WorkspaceError) -> wire::Failure {
    match error {
        native::WorkspaceError::Service(failure) => failure.clone(),
        native::WorkspaceError::Stage(failure) => cause(&failure.cause),
        native::WorkspaceError::Commit(failure) => cause(&failure.cause),
        native::WorkspaceError::Backing(backing) => wire::Failure {
            code: failure_code(error),
            unknown: !backing.accounting_complete,
            cleanup: backing.cleanup_failed.then_some(wire::Code::Io),
            history: None,
        },
        _ => failure_code(error).into(),
    }
}

fn commit_phase(phase: native::CommitPhase) -> wire::WorkspaceCommitPhase {
    match phase {
        native::CommitPhase::Preparing => wire::WorkspaceCommitPhase::Preparing,
        native::CommitPhase::CommitStaged => wire::WorkspaceCommitPhase::CommitStaged,
        native::CommitPhase::CompositeCommit => wire::WorkspaceCommitPhase::CompositeCommit,
        native::CommitPhase::Reconcile => wire::WorkspaceCommitPhase::Reconcile,
        native::CommitPhase::Complete => wire::WorkspaceCommitPhase::Complete,
    }
}

fn commit_disposition(
    disposition: native::CommitFailureDisposition,
) -> wire::WorkspaceCommitFailureDisposition {
    match disposition {
        native::CommitFailureDisposition::KnownBeforeCommit => {
            wire::WorkspaceCommitFailureDisposition::KnownBeforeCommit
        }
        native::CommitFailureDisposition::Unknown => {
            wire::WorkspaceCommitFailureDisposition::Unknown
        }
        native::CommitFailureDisposition::KnownCommitLocalFailure => {
            wire::WorkspaceCommitFailureDisposition::KnownCommitLocalFailure
        }
    }
}

fn stage_phase(phase: native::StagePhase) -> wire::WorkspaceStagePhase {
    match phase {
        native::StagePhase::Captured => wire::WorkspaceStagePhase::Captured,
        native::StagePhase::FileSave => wire::WorkspaceStagePhase::FileSave,
        native::StagePhase::MetadataSave => wire::WorkspaceStagePhase::MetadataSave,
        native::StagePhase::StageChanges => wire::WorkspaceStagePhase::StageChanges,
        native::StagePhase::Staged => wire::WorkspaceStagePhase::Staged,
        native::StagePhase::Failed => wire::WorkspaceStagePhase::Failed,
        native::StagePhase::LocalBookkeeping => wire::WorkspaceStagePhase::LocalBookkeeping,
    }
}

fn stage_disposition(
    disposition: native::StageFailureDisposition,
) -> wire::WorkspaceStageFailureDisposition {
    match disposition {
        native::StageFailureDisposition::KnownBeforeStage => {
            wire::WorkspaceStageFailureDisposition::KnownBeforeStage
        }
        native::StageFailureDisposition::Unknown => wire::WorkspaceStageFailureDisposition::Unknown,
        native::StageFailureDisposition::KnownStageLocalFailure => {
            wire::WorkspaceStageFailureDisposition::KnownStageLocalFailure
        }
    }
}
