//! Existing Bridge/C5 typed failure grammar; no guessed observation or message parsing.
use crate::wire::Snapshot;
use layerfs_bridge::contract::*;
use layerfs_history::{
    error::{HistoryError, StageDisposition},
    records::StageRecord,
};
fn stage(r: &StageRecord) -> StageWire {
    StageWire {
        workspace: r.workspace.to_bytes(),
        token: r.token.value(),
        stack: r.stack.to_bytes(),
        branch: r.branch.to_bytes(),
        expected_head: r.expected_head.map(|h| h.to_bytes()),
        expected_base: r.expected_base.to_bytes(),
        expected_root: *r.expected_root.as_bytes(),
        construction_base_root: *r.construction_base_root.as_bytes(),
        intended_commit_base: r.intended_commit_base.to_bytes(),
        candidate_root: *r.candidate_root.as_bytes(),
        profile: *r.profile.as_bytes(),
        scope: *r.scope.as_bytes(),
        generation: r.generation,
    }
}
pub fn branch_moved(expected: &Snapshot, actual: &Snapshot) -> Failure {
    let mut f = Failure::from(Code::HeadMoved);
    f.history = Some(Box::new(HistoryFailure {
        conflict: Some(HistoryConflict::BranchMoved {
            expected_head: expected.head,
            actual_head: actual.head,
            expected_base: expected.base,
            actual_base: actual.base,
        }),
        stage: StageObservation::Unobserved,
    }));
    f
}
pub fn catalog(error: HistoryError) -> Failure {
    let conflict = match &error {
        HistoryError::HeadMoved(s) => Some(HistoryConflict::BranchMoved {
            expected_head: s.expected_head.map(|h| h.to_bytes()),
            actual_head: s.actual_head.map(|h| h.to_bytes()),
            expected_base: s.expected_base.to_bytes(),
            actual_base: s.actual_base.to_bytes(),
        }),
        HistoryError::StackMoved { expected, actual } => Some(HistoryConflict::StackMoved {
            expected: expected.to_bytes(),
            actual: actual.to_bytes(),
        }),
        HistoryError::BaseMismatch {
            commit_base,
            branch_base,
        } => Some(HistoryConflict::BaseMismatch {
            commit_base: commit_base.to_bytes(),
            branch_base: branch_base.to_bytes(),
        }),
        HistoryError::StageChanged { expected, actual } => Some(HistoryConflict::StageChanged {
            expected: expected.value(),
            actual: actual.map(|t| t.value()),
        }),
        _ => None,
    };
    let code = match error {
        HistoryError::WithStage {
            cause,
            stage: observed,
        } => {
            let mut f = catalog(*cause);
            let ctx = f.history.get_or_insert_with(Default::default);
            ctx.stage = match observed {
                StageDisposition::Absent(w) => StageObservation::Absent(w.to_bytes()),
                StageDisposition::Retained(r) => StageObservation::Retained(Box::new(stage(&r))),
                StageDisposition::AcknowledgedUnknown(r) => {
                    StageObservation::AcknowledgedUnknown(Box::new(stage(&r)))
                }
            };
            return f;
        }
        HistoryError::InvalidInput(_) => Code::InvalidInput,
        HistoryError::Missing(_) | HistoryError::NotInHistory(_) => Code::NotFound,
        HistoryError::Unsupported(_) => Code::Unsupported,
        HistoryError::Busy => Code::Busy,
        HistoryError::OwnershipUnavailable => Code::Ownership,
        HistoryError::Capacity(_) => Code::Capacity,
        HistoryError::Integrity(_) => Code::Integrity,
        HistoryError::HeadMoved(_)
        | HistoryError::BaseMismatch { .. }
        | HistoryError::StackMoved { .. } => Code::HeadMoved,
        HistoryError::StageChanged { .. } => Code::StageChanged,
        HistoryError::ContinuityUnavailable => Code::ContinuityUnavailable,
        HistoryError::UnknownOutcome => Code::Unknown,
    };
    let mut f = Failure::from(code);
    if conflict.is_some() {
        f.history = Some(Box::new(HistoryFailure {
            conflict,
            stage: StageObservation::Unobserved,
        }));
    }
    f
}
