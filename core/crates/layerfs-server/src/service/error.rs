//! Preserve the distinctions exposed by C1/C2; never parse diagnostic strings.
use layerfs_bridge::contract::{Code, Failure};
use layerfs_content::ContentError as C;
use layerfs_storage::StorageError as S;
pub fn content(e: C) -> Failure {
    match e {
        C::MissingObject => Code::MissingObject,
        C::PathNotFound => Code::PathNotFound,
        C::ProviderFailure { .. } | C::OutputRejected => Code::Provider,
        C::BoundedCapacityExceeded { .. }
        | C::ObjectLimitExceeded { .. }
        | C::ResourceUnavailable { .. }
        | C::PathLimitExceeded => Code::Capacity,
        C::UnsupportedProfile { .. } | C::UnsupportedPolicy { .. } => Code::Unsupported,
        C::Io => Code::Io,
        C::IdentityMismatch => Code::Integrity,
        _ => Code::InvalidInput,
    }
    .into()
}
pub fn storage(e: S) -> Failure {
    match e {
        S::Content(e) => content(e),
        S::ObjectMissing(_) | S::MissingDependency { .. } => Code::MissingObject.into(),
        S::OwnershipUnavailable | S::UninspectedState { .. } => Code::Ownership.into(),
        S::UnsupportedPolicy { .. } => Code::Unsupported.into(),
        S::CapacityExceeded { .. } => Code::Capacity.into(),
        S::UnknownOutcome { original } => {
            let mut f = storage(*original);
            f.unknown = true;
            f
        }
        S::CleanupFailed { original, cleanup } => {
            let mut f = storage(*original);
            let c = storage(*cleanup);
            f.unknown |= c.unknown;
            f.cleanup = Some(c.code);
            f
        }
        S::Engine(_) => Code::Provider.into(),
        S::Collision(_) | S::VisibilityCeiling { .. } | S::Unpublished(_) | S::Integrity(_) => {
            Code::Integrity.into()
        }
        S::Aborted => Code::InvalidInput.into(),
    }
}

use crate::service::records::stage_wire;
use layerfs_bridge::contract::{HistoryConflict, HistoryFailure, StageObservation};
use layerfs_history::{CommitId, HistoryError};
/// Maps one typed history failure onto its wire class without parsing a message.
pub(crate) fn catalog(error: HistoryError) -> Failure {
    let conflict = match &error {
        HistoryError::HeadMoved(state) => Some(HistoryConflict::BranchMoved {
            expected_head: state.expected_head.map(CommitId::to_bytes),
            actual_head: state.actual_head.map(CommitId::to_bytes),
            expected_base: state.expected_base.to_bytes(),
            actual_base: state.actual_base.to_bytes(),
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
            actual: actual.map(|token| token.value()),
        }),
        _ => None,
    };
    let code = match error {
        HistoryError::WithStage { cause, stage } => {
            let mut failure = catalog(*cause);
            let context = failure.history.get_or_insert_with(Default::default);
            context.stage = match stage {
                layerfs_history::error::StageDisposition::Absent(workspace) => {
                    StageObservation::Absent(workspace.to_bytes())
                }
                layerfs_history::error::StageDisposition::Retained(stage) => {
                    StageObservation::Retained(Box::new(stage_wire(&stage)))
                }
                layerfs_history::error::StageDisposition::AcknowledgedUnknown(stage) => {
                    StageObservation::AcknowledgedUnknown(Box::new(stage_wire(&stage)))
                }
            };
            return failure;
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
    let mut result = Failure::from(code);
    if conflict.is_some() {
        result.history = Some(Box::new(HistoryFailure {
            conflict,
            stage: StageObservation::Unobserved,
        }));
    }
    result
}
