//! One exact C5 CommitStaged attempt, retaining remote outcomes across local failures.
use crate::{
    backing::{
        metadata::{MetadataCharge, RootOwner},
        metadata_index::vector,
    },
    overlay::snapshot::Submission,
    runtime::state::BranchContext,
    *,
};
use layerfs_bridge::contract::{
    BranchSnapshotWire, BranchWire, CommitOutcomeWire, HistoryCommand, HistoryResult, Operation,
    Response, StageObservation, HISTORY_RESULT_BYTES, MAX_OPERATION_MS,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::Instant,
};
pub(crate) struct CommitAttempt {
    pub root: Arc<RootOwner>,
    pub next: Mutex<Option<BranchContext>>,
    pub status: Mutex<CommitStatus>,
    pub known: OnceLock<CommitOutcomeWire>,
    pub observed: OnceLock<CommitOutcomeWire>,
    received: AtomicBool,
    pub failure: OnceLock<Arc<CommitFailure>>,
    pub stage: Option<StageSelector>,
    failure_charge: Mutex<Option<MetadataCharge>>,
    _charge: MetadataCharge,
}
impl CommitAttempt {
    pub(crate) fn reserve(
        workspace: &Workspace,
        submission: &Submission,
        selector: Option<&StageSelector>,
    ) -> Result<Arc<Self>, WorkspaceError> {
        let host = workspace
            .host
            .metadata
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        let charge = host.memory(2048)?;
        let failure_charge = host.memory(4096)?;
        let context = &submission.capture()?.context;
        let mut name = vector(context.branch.name.len())?;
        name.extend_from_slice(&context.branch.name);
        let next = BranchContext::new(
            BranchSnapshotWire {
                branch: BranchWire {
                    name,
                    branch: context.branch.branch,
                    stack: context.branch.stack,
                    base_layer: context.branch.base_layer,
                    head_commit: context.branch.head_commit,
                },
                head_root: context.head_root,
                base_root: context.base_root,
                effective_root: context.effective_root,
                root_serial: context.root_serial,
                scope: context.scope,
                profile: context.profile,
            },
            &workspace.host.budget,
        )?;
        let root = host.reconciliation_root(&submission.capture()?.root.arena, &submission.fund)?;
        Ok(Arc::new(Self {
            root,
            next: Mutex::new(Some(next)),
            stage: selector.cloned(),
            status: Mutex::new(CommitStatus {
                phase: CommitPhase::Preparing,
                known_root: None,
                known_head: None,
                installed_revision: None,
                failure: None,
            }),
            known: OnceLock::new(),
            observed: OnceLock::new(),
            received: AtomicBool::new(false),
            failure: OnceLock::new(),
            failure_charge: Mutex::new(Some(failure_charge)),
            _charge: charge,
        }))
    }
    pub fn phase(&self, submission: &Submission, phase: CommitPhase) -> Result<(), WorkspaceError> {
        let mut status = self.status.lock().map_err(|_| WorkspaceError::Io)?;
        status.phase = phase;
        let value = *status;
        drop(status);
        submission
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .status
            .commit = Some(value);
        Ok(())
    }
    pub(crate) fn fail(&self, submission: &Submission, cause: WorkspaceError) -> WorkspaceError {
        let Ok(captured) = submission.capture() else {
            return cause;
        };
        let status = self.status.lock().map_or(
            CommitStatus {
                phase: CommitPhase::Reconcile,
                known_root: None,
                known_head: None,
                installed_revision: None,
                failure: None,
            },
            |s| *s,
        );
        let known_outcome = self.known.get().cloned();
        let unknown = match &cause {
            WorkspaceError::Service(f) => {
                f.unknown
                    || f.history.as_ref().is_some_and(|h| {
                        matches!(h.stage, StageObservation::AcknowledgedUnknown(_))
                    })
            }
            WorkspaceError::Stage(f) => f.disposition == StageFailureDisposition::Unknown,
            _ => self.received.load(Ordering::Acquire),
        };
        let disposition = if known_outcome.is_some() {
            CommitFailureDisposition::KnownCommitLocalFailure
        } else if unknown {
            CommitFailureDisposition::Unknown
        } else {
            CommitFailureDisposition::KnownBeforeCommit
        };
        let Ok(mut charge) = self.failure_charge.lock() else {
            return cause;
        };
        let Some(charge) = charge.take() else {
            return cause;
        };
        let observed_stage = match &cause {
            WorkspaceError::Service(f) => {
                f.history.as_ref().and_then(|history| match &history.stage {
                    StageObservation::Retained(stage)
                    | StageObservation::AcknowledgedUnknown(stage) => Some((**stage).clone()),
                    _ => None,
                })
            }
            WorkspaceError::Stage(f) => f.observed_stage.clone(),
            _ => None,
        };
        let failure = Arc::new(CommitFailure {
            generation: captured.generation,
            phase: status.phase,
            disposition,
            cause,
            stage: self.stage.clone(),
            observed_stage,
            known_outcome,
            observed_outcome: self.observed.get().cloned(),
            installed_revision: status.installed_revision,
            _charge: charge,
        });
        let _ = self.failure.set(failure.clone());
        if let Ok(mut s) = self.status.lock() {
            s.failure = Some(disposition);
        }
        if let Ok(mut s) = submission.state.lock() {
            s.status.commit = Some(CommitStatus {
                failure: Some(disposition),
                ..status
            });
        }
        WorkspaceError::Commit(failure)
    }
}
impl Workspace {
    pub fn commit_staged(
        &self,
        selector: &StageSelector,
        deadline: Instant,
    ) -> Result<CommitReport, WorkspaceError> {
        if self.inner.access != WorkspaceAccess::LocalEdit {
            return Err(WorkspaceError::ReadOnly);
        }
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(WorkspaceError::Deadline)?;
        if remaining.as_millis() == 0 || remaining.as_millis() > u128::from(MAX_OPERATION_MS) {
            return Err(WorkspaceError::InvalidInput);
        }
        let _operation = self.begin(false, deadline)?;
        let submission = {
            let state = self.state()?;
            self.available(&state)?;
            let submission = state
                .submission
                .as_ref()
                .ok_or(WorkspaceError::InvalidInput)?;
            if !Arc::ptr_eq(&submission.identity, &selector.identity) {
                return Err(WorkspaceError::InvalidInput);
            }
            super::save::validate_stage(submission, self.inner.incarnation, selector.stage())?;
            let status = submission.status()?;
            if status.phase != StagePhase::Staged || status.failure.is_some() {
                return Err(WorkspaceError::Busy);
            }
            submission
                .commit_claimed
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .map_err(|_| WorkspaceError::Busy)?;
            submission.clone()
        };
        let remote = match self.begin(true, deadline) {
            Ok(remote) => remote,
            Err(error) => {
                submission.commit_claimed.store(false, Ordering::Release);
                return Err(error);
            }
        };
        let attempt = match CommitAttempt::reserve(self, &submission, Some(selector)) {
            Ok(attempt) => attempt,
            Err(error) => {
                submission.commit_claimed.store(false, Ordering::Release);
                return Err(error);
            }
        };
        if submission.commit.set(attempt.clone()).is_err() {
            return Err(WorkspaceError::Io);
        }
        match self.complete_staged(&submission, &attempt, remote, deadline) {
            Ok(report) => Ok(report),
            Err(error) => Err(attempt.fail(&submission, error)),
        }
    }
    fn complete_staged(
        &self,
        submission: &Submission,
        attempt: &CommitAttempt,
        remote: crate::runtime::state::OperationGuard,
        deadline: Instant,
    ) -> Result<CommitReport, WorkspaceError> {
        attempt.phase(submission, CommitPhase::CommitStaged)?;
        let stage = attempt.stage.as_ref().ok_or(WorkspaceError::Io)?.stage();
        let response = self.remote_call(
            (self.inner.store, stage.generation),
            Operation::HistoryCommand(HistoryCommand::CommitStaged {
                workspace: stage.workspace,
                token: stage.token,
            }),
            &mut &[][..],
            HISTORY_RESULT_BYTES as u64,
            &mut std::io::sink(),
            deadline,
        );
        drop(remote);
        self.complete_commit_response(submission, attempt, response, deadline)
    }
    pub(crate) fn complete_commit_response(
        &self,
        submission: &Submission,
        attempt: &CommitAttempt,
        response: Result<Response, WorkspaceError>,
        deadline: Instant,
    ) -> Result<CommitReport, WorkspaceError> {
        let response = response?;
        attempt.received.store(true, Ordering::Release);
        let Response::History(result) = response else {
            return Err(WorkspaceError::InvalidInput);
        };
        let HistoryResult::Committed(outcome) = *result else {
            return Err(WorkspaceError::InvalidInput);
        };
        attempt
            .observed
            .set(outcome.clone())
            .map_err(|_| WorkspaceError::Io)?;
        let (root, head) = validate_outcome(submission, attempt.stage.as_ref(), &outcome)?;
        attempt
            .known
            .set(outcome.clone())
            .map_err(|_| WorkspaceError::Io)?;
        {
            let mut s = attempt.status.lock().map_err(|_| WorkspaceError::Io)?;
            s.known_root = Some(root);
            s.known_head = head;
        }
        attempt.phase(submission, CommitPhase::Reconcile)?;
        // The candidate's 64 pages already belong to it. All later releases of
        // mixed successor/fund descendants must return to ordinary quota.
        submission.fund.finish()?;
        let revision = self.reconcile_commit(submission, attempt, &outcome, deadline)?;
        attempt.phase(submission, CommitPhase::Complete)?;
        let retained = {
            let mut state = self.state()?;
            if state
                .submission
                .as_ref()
                .is_none_or(|s| !std::ptr::eq(s.as_ref(), submission))
            {
                return Err(WorkspaceError::Io);
            }
            state.submission.take()
        };
        drop(retained);
        Ok(CommitReport {
            generation: submission.capture()?.generation,
            stage_token: attempt
                .stage
                .as_ref()
                .map(|selector| selector.stage().token),
            outcome,
            revision,
        })
    }
}
fn validate_outcome(
    submission: &Submission,
    selector: Option<&StageSelector>,
    outcome: &CommitOutcomeWire,
) -> Result<([u8; 32], Option<[u8; 33]>), WorkspaceError> {
    let captured = submission.capture()?;
    let context = &captured.context;
    let candidate = selector.map(|selector| selector.stage().candidate_root);
    match outcome {
        CommitOutcomeWire::Committed(commit)
            if commit.stack == context.branch.stack
                && commit.parent == context.branch.head_commit
                && commit.base_layer == context.branch.base_layer
                && commit.root != context.effective_root
                && candidate.is_none_or(|root| root == commit.root)
                && Some(commit.commit) != context.branch.head_commit =>
        {
            Ok((commit.root, Some(commit.commit)))
        }
        CommitOutcomeWire::UpToDate { head, root }
            if *head == context.branch.head_commit
                && *root == context.effective_root
                && candidate.is_none_or(|candidate| candidate == *root) =>
        {
            Ok((*root, *head))
        }
        _ => Err(WorkspaceError::InvalidInput),
    }
}
