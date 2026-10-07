//! Public native-control vocabulary; no immutable-content data service.
use layerfs_history::{
    error::MovedState, BranchId, BranchSnapshot, CommitHistoryRequest, CommitRecord,
    CommitStagedOutcome, ForkRequest, PageResult, WorkspaceId,
};
/// History processing window; authenticated continuation covers arbitrarily many pages.
pub const HISTORY_WINDOW: u16 = 32;
/// Exact daemon-local namespace plus authority incarnation. Never redirect a stale token.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkspaceToken {
    /// Authority-selected Workspace incarnation.
    pub workspace: WorkspaceId,
    /// Original local namespace returned by the owner; never reused by this daemon.
    pub namespace: i64,
}
/// One explicitly requested control operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Request {
    /// Prepare the Store/engine binding. FUSE attachment/readiness is a later S8 step.
    Mount {
        /// New authority incarnation.
        workspace: WorkspaceId,
        /// Selected Branch.
        branch: BranchId,
    },
    /// Capture and run the daemon-supplied Content producer, Save and conditional publish.
    Commit(WorkspaceToken),
    /// Observe maintained local state without reading or refreshing the Store.
    Status(WorkspaceToken),
    /// Terminally close a quiescent local binding and transfer cleanup to the owner.
    Unmount(WorkspaceToken),
    /// Create a Branch through the daemon's direct History port.
    Fork(ForkRequest),
    /// Read one bounded anchored ancestry page through the daemon.
    History(CommitHistoryRequest),
}
/// Correlation for one attempted request on an authenticated connection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Call {
    /// Nonzero original correlation.
    pub id: u64,
    /// Exact original command.
    pub request: Request,
}
/// Last acknowledged control-owner disposition, copied with its installed binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Activity {
    /// No admitted control Commit or terminal transition.
    Idle,
    /// One admitted Commit is running; this does not predict its result.
    Committing,
    /// Terminal close is admitted; no new Commit may enter.
    Closing,
    /// An original unknown must retain custody; normal Commit/unmount are refused.
    Uncertain,
    /// Publication is known but local installation did not complete normally.
    LocalFailure,
}
/// One indexed engine state-row observation, with its own revision scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalObservation {
    /// Original acknowledged local mutation revision.
    pub revision: i64,
    /// Active engine generation.
    pub active: i64,
    /// Retained capture generation, if any.
    pub captured: Option<i64>,
    /// Frozen capture revision, if any.
    pub captured_revision: Option<i64>,
    /// Engine-installed immutable root at this row observation.
    pub base_root: [u8; 32],
    /// Maintained dirty inode count, without scanning.
    pub dirty_inodes: u64,
    /// Maintained dirty binding count, without scanning.
    pub dirty_directory_entries: u64,
    /// Original logical terminal state.
    pub closed: bool,
    /// Maintained base-source ownership count.
    pub base_readers: u64,
}
/// Scoped bounded observations; registry metadata and engine row have distinct epochs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceStatus {
    /// Exact selected incarnation/namespace.
    pub token: WorkspaceToken,
    /// Last binding acknowledged by the control owner, never a Branch refresh.
    pub binding: BranchSnapshot,
    /// Disposition copied coherently with that binding.
    pub activity: Activity,
    /// Control-owner state epoch; saturation does not reject operations.
    pub epoch: u64,
    /// True when the diagnostic epoch can no longer increase.
    pub epoch_saturated: bool,
    /// Known publication retained after a local failure, when available.
    pub published: Option<CommitStagedOutcome>,
    /// Separately scoped indexed engine observation taken after the control copy.
    pub local: Option<LocalObservation>,
    /// Original engine observation refusal when local fields are unavailable.
    pub local_failure: Option<ControlRefusal>,
}
/// Typed refusal knowledge; the daemon retains its original detailed cause.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlCode {
    /// Original write/admission contention; earlier acknowledged Save waves may remain.
    Busy,
    /// Exact conditional Branch conflict, carried in moved.
    HeadMoved,
    /// Selected route or history record is absent.
    Missing,
    /// Declared control or owner capacity refused admission.
    Capacity,
    /// Invalid command or stale namespace token.
    Invalid,
    /// Definite failure; retained context is described by the original phase.
    Failed,
    /// An original outcome is uncertain and cannot be replayed or guessed.
    Unknown,
}
/// Original refused control operation, independent of message text parsing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlRefusal {
    /// Deciding typed category.
    pub code: ControlCode,
    /// Original operation phase, bounded control metadata.
    pub phase: String,
    /// Exact expected/actual Branch state for HeadMoved.
    pub moved: Option<MovedState>,
    /// Original known publication, including when local install failed.
    pub published: Option<CommitStagedOutcome>,
    /// Original bounded cause description; never parsed to infer the category.
    pub detail: String,
}
/// Original command result. Bound means engine/Store preparation, not kernel readiness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Reply {
    /// Newly bound engine namespace and exact selected history snapshot.
    Bound {
        /// Original namespace token.
        token: WorkspaceToken,
        /// Exact selected binding.
        binding: BranchSnapshot,
    },
    /// Original known history outcome after paired local installation.
    Committed(CommitStagedOutcome),
    /// Bounded local observation with explicit owner scopes.
    Status(Box<WorkspaceStatus>),
    /// Logical terminal close acknowledged; physical cleanup remains owner-driven.
    Unmounted(WorkspaceToken),
    /// Original acknowledged new Branch.
    Forked(BranchSnapshot),
    /// Bounded immutable ancestry page and authenticated continuation.
    History(PageResult<CommitRecord>),
    /// Original typed refusal.
    Refused(ControlRefusal),
}
/// Correlated original reply; transport failure does not reverse its effects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Answer {
    /// Exact original request correlation.
    pub id: u64,
    /// Original result.
    pub reply: Reply,
}
