//! Preserve original failure knowledge while sending a bounded control summary.
use super::Failure;
use crate::{
    store::{owner_uncertain, BindError, CommitError, PortError},
    OwnerError,
};
use layerfs_bridge::control::{ControlCode, ControlRefusal};
use layerfs_history::{error::MovedState, HistoryError};
use layerfs_overlay::OverlayError;
use layerfs_storage::StorageError;
impl Failure {
    pub(super) fn wire(&self) -> ControlRefusal {
        let (code, moved, phase, detail, published) = match self {
            Self::Custody(refusal) => return refusal.as_ref().clone(),
            Self::Rejected(code, detail) => {
                (*code, None, "admission".into(), (*detail).into(), None)
            }
            Self::Native(failed) => (
                failed.code,
                None,
                failed.phase.into(),
                super::native::bounded(&failed.detail),
                None,
            ),
            Self::Retained(custody) => (
                ControlCode::Unknown,
                None,
                "native teardown".into(),
                custody.detail.clone(),
                None,
            ),
            Self::Poisoned => (
                ControlCode::Unknown,
                None,
                "registry".into(),
                "control owner poisoned".into(),
                None,
            ),
            Self::History(error) => {
                let (code, moved) = history(error);
                (code, moved, "history".into(), error.to_string(), None)
            }
            Self::Workspace(error) => (
                ControlCode::Unknown,
                None,
                "binding".into(),
                error.to_string(),
                None,
            ),
            Self::Owner(error) => (
                owner(error),
                None,
                "owner submission".into(),
                owner_detail(error),
                None,
            ),
            Self::Completion(done) => {
                let (code, detail) = completion(done);
                (code, None, "owner completion".into(), detail, None)
            }
            Self::Bind(failed) => {
                let (code, moved, detail) = match &failed.error {
                    BindError::History(error) => {
                        let (code, moved) = history(error);
                        (code, moved, error.to_string())
                    }
                    BindError::Read(error) => {
                        (port(Some(error.as_ref()), false), None, error.to_string())
                    }
                    BindError::MissingBranch(id) => {
                        (ControlCode::Missing, None, format!("Branch {id} missing"))
                    }
                    BindError::Owner(error) => (owner(error), None, owner_detail(error)),
                    BindError::Completion(done) => {
                        let (code, detail) = completion(done);
                        (code, None, detail)
                    }
                    BindError::Content { error, provider } => (
                        port(
                            provider.as_deref(),
                            matches!(error, layerfs_content::ContentError::ProviderFailure { .. }),
                        ),
                        None,
                        error.to_string(),
                    ),
                };
                (
                    code,
                    moved,
                    format!("bind {:?}", failed.phase),
                    detail,
                    None,
                )
            }
            Self::Commit(failed) => {
                let (mut code, moved, mut detail) = commit(&failed.error);
                // A settled failure whose reader or owner was not released
                // still leaves custody; the reply must not read as settled.
                if code != ControlCode::Unknown
                    && failed
                        .namespace
                        .as_ref()
                        .is_some_and(|kept| kept.retained())
                {
                    code = ControlCode::Unknown;
                    detail = format!("captured namespace custody retained after: {detail}");
                }
                (
                    code,
                    moved,
                    format!("Commit {:?}", failed.phase),
                    super::native::bounded(&detail),
                    failed.published.clone(),
                )
            }
            Self::After { cause, original } => {
                let mut refusal = cause.wire();
                refusal.phase = "control settlement".into();
                refusal.published = original
                    .commit
                    .as_ref()
                    .map(|commit| commit.history.clone())
                    .or(refusal.published);
                return refusal;
            }
        };
        ControlRefusal {
            code,
            moved,
            phase,
            detail,
            published,
        }
    }
    /// True when this failure itself carries unresolved custody. Retained
    /// native custody stays in the registry entry, not in the reply's carrier.
    pub(crate) fn uncertain(&self) -> bool {
        !matches!(self, Self::Retained(_)) && self.wire().code == ControlCode::Unknown
    }
}
fn history(error: &HistoryError) -> (ControlCode, Option<MovedState>) {
    match error.cause() {
        HistoryError::Busy => (ControlCode::Busy, None),
        HistoryError::HeadMoved(value) => (ControlCode::HeadMoved, Some(**value)),
        HistoryError::UnknownOutcome => (ControlCode::Unknown, None),
        HistoryError::Missing(_) => (ControlCode::Missing, None),
        HistoryError::InvalidInput(_) => (ControlCode::Invalid, None),
        HistoryError::Capacity(_) => (ControlCode::Capacity, None),
        _ => (ControlCode::Failed, None),
    }
}
fn storage(error: &StorageError) -> ControlCode {
    if error.is_unknown_outcome() {
        ControlCode::Unknown
    } else if matches!(error, StorageError::Busy) {
        ControlCode::Busy
    } else {
        ControlCode::Failed
    }
}
fn port(error: Option<&PortError>, opaque: bool) -> ControlCode {
    match error {
        Some(PortError::Storage(error)) => storage(error),
        Some(PortError::History(error)) => history(error).0,
        Some(PortError::Poisoned) => ControlCode::Unknown,
        Some(PortError::ConcurrentDemand) => ControlCode::Busy,
        Some(PortError::ReadAdmission(error)) => match error {
            crate::store::ReadAdmissionError::Capacity => ControlCode::Capacity,
            crate::store::ReadAdmissionError::Poisoned => ControlCode::Unknown,
            crate::store::ReadAdmissionError::InvalidLimits => ControlCode::Invalid,
            _ => ControlCode::Failed,
        },
        None if opaque => ControlCode::Unknown,
        None => ControlCode::Failed,
    }
}
fn commit(error: &CommitError) -> (ControlCode, Option<MovedState>, String) {
    if let CommitError::History(error) = error {
        let (code, moved) = history(error);
        return (code, moved, error.to_string());
    }
    let code = if error.uncertain() {
        ControlCode::Unknown
    } else {
        match error {
            CommitError::InFlight => ControlCode::Busy,
            CommitError::Storage(error) | CommitError::Construction { storage: error, .. } => {
                storage(error)
            }
            CommitError::Owner(error) => owner(error),
            _ => ControlCode::Failed,
        }
    };
    let detail = match error {
        CommitError::Storage(error) | CommitError::Construction { storage: error, .. } => {
            error.to_string()
        }
        CommitError::Content { error, .. } => error.to_string(),
        CommitError::Owner(error) => owner_detail(error),
        CommitError::Completion(done) => completion(done).1,
        CommitError::Workspace(error) => error.to_string(),
        CommitError::InFlight => "Commit already active".into(),
        CommitError::Context(reason) => (*reason).into(),
        CommitError::History(error) => error.to_string(),
    };
    (code, None, detail)
}
fn owner(error: &OwnerError) -> ControlCode {
    if owner_uncertain(error) {
        return ControlCode::Unknown;
    }
    match error {
        OwnerError::Unattempted { cause, .. } => owner(cause),
        OwnerError::AdmissionFull => ControlCode::Capacity,
        OwnerError::InvalidAdmission => ControlCode::Invalid,
        OwnerError::Overlay(
            OverlayError::CaptureInFlight
            | OverlayError::Consolidating
            | OverlayError::ReplyAttemptsPending
            | OverlayError::BaseSourcesPending,
        ) => ControlCode::Busy,
        OwnerError::Overlay(OverlayError::Missing | OverlayError::Closed) => ControlCode::Missing,
        OwnerError::Overlay(OverlayError::Stale | OverlayError::Invalid(_)) => ControlCode::Invalid,
        _ => ControlCode::Failed,
    }
}
fn owner_detail(error: &OwnerError) -> String {
    match error {
        OwnerError::Unattempted { cause, .. } => owner_detail(cause),
        OwnerError::Install { cause, .. } => cause.to_string(),
        other => other.to_string(),
    }
}
fn completion(done: &crate::Completion) -> (ControlCode, String) {
    match done.result() {
        Err(error) => (owner(error), owner_detail(error)),
        Ok(_) => (ControlCode::Unknown, "unexpected owner reply".into()),
    }
}
