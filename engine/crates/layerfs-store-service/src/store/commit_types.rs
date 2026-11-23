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
    /// The head the Commit must be made over (`--expected`): the node the
    /// Branch answers as its head, its head Commit or else its base Layer.
    /// Checked when the head is read and again inside the publication's
    /// own transaction; another head is refused `HeadMoved` and nothing is
    /// published. A forced Commit then displaces only this head.
    pub expected: Option<layerfs_history::NodeId>,
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
impl CommitFailure {
    /// Whether this failure leaves its outcome unknown. Before the Publish
    /// phase nothing in history can have changed (`commit_branch` is the
    /// only write that publishes), so only local uncertainty counts there,
    /// as it does after a known publication; an unanswered publication is
    /// uncertain on any unknown outcome.
    pub fn uncertain(&self) -> bool {
        match self.phase {
            CommitPhase::Publish if self.published.is_none() => self.error.uncertain(),
            _ => self.error.locally_uncertain(),
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
impl CommitError {
    /// Uncertainty about local state alone: the overlay engine, an owner job
    /// or the Workspace binding. A Store, Content or History failure is not
    /// here: before publication it cannot have changed history.
    pub fn locally_uncertain(&self) -> bool {
        match self {
            Self::Construction { original, .. } => original.locally_uncertain(),
            Self::Content {
                provider: Some(provider),
                ..
            } => matches!(
                provider.as_ref(),
                PortError::Poisoned | PortError::ReadAdmission(super::ReadAdmissionError::Poisoned)
            ),
            Self::Owner(error) => owner_uncertain(error),
            Self::Workspace(
                WorkspaceError::BindingPoisoned
                | WorkspaceError::Service(_)
                | WorkspaceError::Overlay(
                    layerfs_overlay::OverlayError::Uncertain { .. }
                    | layerfs_overlay::OverlayError::Quarantined,
                ),
            ) => true,
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
/// The exact original cause a Workspace service port carried, as the product
/// type it is: an owner job, the Store provider's failure, or a typed History
/// failure. A definite pre-effect History refusal (a `Busy` admission, for
/// one) must keep its own class here, so it is never read as an unknown
/// outcome (#647). Anything else stays a Workspace service failure.
pub fn service_cause(
    service: Box<dyn std::error::Error + Send + Sync>,
    label: ContentError,
) -> Result<CommitError, Box<dyn std::error::Error + Send + Sync>> {
    match service.downcast::<OwnerError>() {
        Ok(owner) => Ok(CommitError::Owner(*owner)),
        Err(other) => match other.downcast::<Arc<PortError>>() {
            Ok(port) => Ok(CommitError::Content {
                error: label,
                provider: Some(*port),
            }),
            Err(other) => match other.downcast::<HistoryError>() {
                Ok(history) => Ok(CommitError::History(*history)),
                Err(other) => Err(other),
            },
        },
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
