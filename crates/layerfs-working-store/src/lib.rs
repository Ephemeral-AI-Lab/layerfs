//! Host-recoverable Branch and Operation authority over one Working StorageId.

#![forbid(unsafe_code)]

mod checkout;
mod compaction;
mod store;

mod branch;
mod layer_candidate;
mod layered;
mod operation;

pub use layerfs_storage::integrity::IntegrityMode;
pub use layerfs_storage::scratch::{DiskNamespace, DiskTable, ScratchObservation};
pub use layerfs_storage::{
    BranchHead, BranchId, BranchPushOutcome, BranchRollbackPublication, ChildMergePublication,
    LayerCandidate, LayerId, LayerStackHead, LayerStackId, OperationId, OperationRecordRef,
    OperationVersionId, PreservedOperationCandidate, RecoverableOperation,
    RecoverableOperationState, VersionRef,
};
pub use layerfs_storage::{EngineCounters, StorageError};

pub use branch::{BranchRollbackResult, ChildMergeResult};
pub use layer_candidate::LayerPreparationResult;
pub use operation::{
    BeginOperation, CommitResult, WorkingCandidate, WorkingCandidateWrite, WorkingTrustedCandidate,
};
pub use store::{Result, WorkingError, WorkingStore};

pub const COMPONENT: &str = "layerfs-working-store";
