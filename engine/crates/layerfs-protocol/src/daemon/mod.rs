//! The daemon control wire: one record family per call (hello, mount,
//! Commit, unmount, forced teardown, history, capacity), each with its codec;
//! every record is re-exported here.
mod capacity;
mod commit;
mod forced;
mod hello;
mod history;
mod mount;
mod reply;
pub(crate) mod request;
mod unmount;

pub use capacity::{
    AdmissionModel, CapacityModel, RetainedFailureModel, RetainedModel, OPERATION, RECLAIM,
};
pub use commit::{CommitControl, ConflictReport, MergedCommit, TEXT_FIELD_LIMIT};
pub use forced::{
    AbortDisposition, CommitKnowledge, DetachDisposition, ForceUnmounted, ForcedCleanup,
    ForcedFacts, ForcedOutcome,
};
pub use hello::{DaemonPhase, DaemonStatus, HelloRequest};
pub use mount::{
    MountPhase, MountReceipt, MountStatus, MountWork, ReadyMount, TeardownCustody, TeardownStage,
    MOUNT_DIRECTORY_LIMIT,
};
pub use request::{
    Activity, Answer, Call, CleanupObservation, ControlCode, ControlRefusal, LocalObservation,
    MountPlace, Reply, Request, WorkspaceStatus, WorkspaceToken, HISTORY_WINDOW,
};
pub use unmount::{DirtyReport, UnmountControl};
