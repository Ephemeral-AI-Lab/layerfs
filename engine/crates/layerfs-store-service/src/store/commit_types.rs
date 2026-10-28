//! Original Commit knowledge, publication outcome and local ownership receipts.
use super::PortError;
use crate::OwnerError;
use layerfs_content::{
    filesystem::FilesystemUpdateCounters,
    merge::{Conflict, LwwDecision},
    ContentError, ObjectId,
};
use layerfs_history::{
    BranchSnapshot, CommitId, CommitOutcome, CommitRequest, HistoryError, WorkspaceId,
};
use layerfs_overlay::{Capture, CapturedReader, OperationOwner};
use layerfs_storage::{Diagnostics, StorageError, WriteOutcome};
use layerfs_workspace::Route;
use layerfs_workspace::{
    CapturedNamespaceCustody, CapturedNamespaceWork, PreparedBase, WorkspaceError,
};
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
    /// Previously conflicted files still hold marker blocks; nothing is
    /// published.
    Markers(Vec<String>),
    /// The head moved again after the merge, by more paths than the
    /// in-lane merge takes; nothing is published (#355).
    LaneMerge {
        paths: u64,
        limit: u64,
    },
}
/// What the product constructor's one namespace attempt acquired, counted and
/// released. A named reader or owner here was acquired and is not known
/// released: it is the caller's exact custody, never released on Drop.
#[derive(Debug, Default)]
pub struct CapturedConstruction {
    /// The producer's counted adapter work, once it has run.
    pub work: Option<CapturedNamespaceWork>,
    /// Content's filesystem work of a successful update.
    pub counters: Option<FilesystemUpdateCounters>,
    pub reader: Option<CapturedReader>,
    pub operation: Option<OperationOwner>,
    /// A failed attempt's record and file custody while its owners are kept.
    pub custody: Option<Box<CapturedNamespaceCustody>>,
    /// Releases known done, in attempt order: reader, then owner. Each
    /// receipt is dropped once read.
    pub released: Vec<ReleasedOwner>,
    /// The submission failure that ended the release sequence.
    pub release_error: Option<OwnerError>,
}
/// One owner of a namespace attempt whose release is known done.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleasedOwner {
    Reader,
    Operation,
}
impl CapturedConstruction {
    /// True while an acquired reader or operation owner is not known released.
    pub const fn retained(&self) -> bool {
        self.reader.is_some() || self.operation.is_some()
    }
}
/// What one Workspace Commit asks for besides the Workspace itself.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommitOptions {
    /// Publish over a moved head instead of merging.
    pub force: bool,
    /// Description of the captured filesystem change.
    pub message: Option<String>,
}
/// A Commit merged with a Branch head that moved since the Workspace bound.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MergeReport {
    /// Head the Workspace was bound to.
    pub from: Option<CommitId>,
    /// Head it merged with.
    pub head: Option<CommitId>,
    /// Conflicting paths; empty for a clean merge.
    pub conflicts: Vec<Conflict>,
    /// `.lww` decisions.
    pub lww: Vec<LwwDecision>,
    /// Paths a conflicted merge changed in the Workspace view, at most
    /// 4,096 in path order.
    pub written: Vec<String>,
    /// True when `written` may omit changed paths: the comparison stopped at
    /// its bound (#355 F-041). False only for a complete list.
    pub written_truncated: bool,
    /// Paths the in-lane step decided when the head moved again after the
    /// merge; `None` when it did not (#355).
    pub lane_paths: Option<usize>,
}
/// Known history publication followed by a known paired local base install,
/// or a conflicted merge installed locally with nothing published.
#[derive(Debug)]
pub struct CommitSuccess {
    /// The publication; `None` when the merge conflicted.
    pub history: Option<CommitOutcome>,
    /// The merge with a moved head, when one ran.
    pub merge: Option<MergeReport>,
    pub saved: WriteOutcome,
    pub storage: Diagnostics,
    /// `None` when an unchanged Workspace answered UpToDate without one.
    pub capture: Option<Capture>,
    /// The product constructor's receipt; None under a caller's constructor.
    pub namespace: Option<CapturedConstruction>,
    /// The Workspace's own captured root and the different root installed
    /// in its place (a merge, clean or with markers); the kernel may cache
    /// what changed between them.
    pub replaced: Option<(ObjectId, ObjectId)>,
}
/// No part of an original failed attempt is inferred from a subsequent read.
#[derive(Debug)]
pub struct CommitFailure {
    pub workspace: WorkspaceId,
    pub route: Route,
    pub binding: Option<BranchSnapshot>,
    pub phase: CommitPhase,
    pub error: CommitError,
    pub intent: Option<CommitRequest>,
    pub capture: Option<Capture>,
    pub prepared: Option<PreparedBase>,
    pub saved: Option<WriteOutcome>,
    pub storage: Option<Diagnostics>,
    /// Some means publication is known even if local install failed.
    pub published: Option<CommitOutcome>,
    pub local_error: Option<OwnerError>,
    /// False means capture/unknown custody remains; another Commit is refused.
    pub locally_settled: bool,
    /// The product constructor's receipt, when its closure ran.
    pub namespace: Option<CapturedConstruction>,
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
            prepared: None,
            saved: None,
            storage: None,
            published: None,
            local_error: None,
            locally_settled: false,
            namespace: None,
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
    pub fn uncertain(&self) -> bool {
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
                    PortError::ConcurrentDemand => false,
                    PortError::ReadAdmission(error) => {
                        matches!(error, super::ReadAdmissionError::Poisoned)
                    }
                },
            ),
            Self::History(error) => error.unknown(),
            Self::Owner(error) => owner_uncertain(error),
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
pub fn owner_uncertain(error: &OwnerError) -> bool {
    match error {
        OwnerError::Unattempted { .. } => false,
        OwnerError::Attempted(failure) => owner_uncertain(&failure.cause),
        OwnerError::Retained(cause) => owner_uncertain(cause),
        OwnerError::Overlay(
            layerfs_overlay::OverlayError::Uncertain { .. }
            | layerfs_overlay::OverlayError::Quarantined,
        ) => true,
        OwnerError::WorkspacePanicked(_) => true,
        OwnerError::Workspace(error) | OwnerError::Install { cause: error, .. } => matches!(
            error.as_ref(),
            WorkspaceError::Overlay(
                layerfs_overlay::OverlayError::Uncertain { .. }
                    | layerfs_overlay::OverlayError::Quarantined
            ) | WorkspaceError::BindingPoisoned
                | WorkspaceError::Service(_)
        ),
        _ => false,
    }
}
