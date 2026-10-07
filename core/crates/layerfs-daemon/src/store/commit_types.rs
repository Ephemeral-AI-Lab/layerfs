//! Original Commit knowledge, publication outcome and local ownership receipts.
use super::PortError;
use crate::{Completion, OwnerError};
use layerfs_content::ContentError;
use layerfs_history::{
    BranchSnapshot, CommitStagedOutcome, HistoryError, StageRequest, WorkspaceId,
};
use layerfs_overlay::{Capture, Route};
use layerfs_storage::{Diagnostics, StorageError, WriteOutcome};
use layerfs_workspace::{PreparedBase, WorkspaceError};
use std::{fmt, sync::Arc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitPhase {
    Admission,
    Capture,
    Begin,
    Construct,
    Finish,
    Validate,
    Publish,
    Install,
}
#[derive(Debug)]
pub enum CommitError {
    InFlight,
    Context(&'static str),
    Storage(StorageError),
    Construction {
        original: Box<CommitError>,
        storage: StorageError,
    },
    Content {
        error: ContentError,
        provider: Option<Arc<PortError>>,
    },
    History(HistoryError),
    Workspace(WorkspaceError),
    Owner(OwnerError),
    Completion(Box<Completion>),
}
/// Known history publication followed by a known paired local base install.
#[derive(Debug)]
pub struct CommitSuccess {
    pub history: CommitStagedOutcome,
    pub saved: WriteOutcome,
    pub storage: Diagnostics,
    pub capture: Capture,
    pub captured: Completion,
    pub installed: Completion,
}
/// No part of an original failed attempt is inferred from a subsequent read.
#[derive(Debug)]
pub struct CommitFailure {
    pub workspace: WorkspaceId,
    pub route: Route,
    pub binding: Option<BranchSnapshot>,
    pub phase: CommitPhase,
    pub error: CommitError,
    pub intent: Option<StageRequest>,
    pub capture: Option<Capture>,
    pub captured: Option<Completion>,
    pub prepared: Option<PreparedBase>,
    pub saved: Option<WriteOutcome>,
    pub storage: Option<Diagnostics>,
    /// Some means publication is known even if local install failed.
    pub published: Option<CommitStagedOutcome>,
    /// Original install or definite-failure resolution completion.
    pub local: Option<Completion>,
    pub local_error: Option<OwnerError>,
    /// False means capture/unknown custody remains; another Commit is refused.
    pub locally_settled: bool,
}
impl CommitFailure {
    pub(super) fn new(error: CommitError, workspace: WorkspaceId, route: Route) -> Self {
        Self {
            workspace,
            route,
            binding: None,
            phase: CommitPhase::Admission,
            error,
            intent: None,
            capture: None,
            captured: None,
            prepared: None,
            saved: None,
            storage: None,
            published: None,
            local: None,
            local_error: None,
            locally_settled: false,
        }
    }
}
impl fmt::Display for CommitFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Store Commit at {:?}: {:?}", self.phase, self.error)
    }
}
impl std::error::Error for CommitFailure {}

impl CommitError {
    pub(crate) fn uncertain(&self) -> bool {
        match self {
            Self::Storage(error) => error.is_unknown_outcome(),
            Self::Construction { original, storage } => {
                storage.is_unknown_outcome()
                    || (!matches!(original.as_ref(), Self::Content { provider: None, .. })
                        && original.uncertain())
            }
            Self::Content { error, provider } => provider.as_ref().map_or_else(
                || {
                    matches!(
                        error,
                        ContentError::ProviderFailure { .. } | ContentError::OutputRejected
                    )
                },
                |e| match e.as_ref() {
                    PortError::Storage(e) => e.is_unknown_outcome(),
                    PortError::History(e) => e.unknown(),
                    PortError::Poisoned => true,
                },
            ),
            Self::History(error) => error.unknown(),
            Self::Owner(error) => owner_uncertain(error),
            Self::Completion(done) => done.result().as_ref().err().is_none_or(owner_uncertain),
            Self::Workspace(WorkspaceError::BindingPoisoned) => true,
            Self::Workspace(WorkspaceError::Service(_)) => true,
            Self::Workspace(WorkspaceError::Overlay(
                layerfs_overlay::OverlayError::Uncertain { .. }
                | layerfs_overlay::OverlayError::Quarantined,
            )) => true,
            _ => false,
        }
    }
}
impl From<ContentError> for CommitError {
    fn from(error: ContentError) -> Self {
        Self::Content {
            error,
            provider: None,
        }
    }
}
impl From<StorageError> for CommitError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}
impl From<WorkspaceError> for CommitError {
    fn from(error: WorkspaceError) -> Self {
        Self::Workspace(error)
    }
}
pub(crate) fn owner_uncertain(error: &OwnerError) -> bool {
    match error {
        OwnerError::Unattempted { .. } => false,
        OwnerError::Overlay(
            layerfs_overlay::OverlayError::Uncertain { .. }
            | layerfs_overlay::OverlayError::Quarantined,
        ) => true,
        OwnerError::Disconnected | OwnerError::WorkerPanicked => true,
        _ => false,
    }
}
