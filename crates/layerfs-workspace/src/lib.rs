//! Universal runtime boundary for one isolated LayerFS operation.

#![forbid(unsafe_code)]

mod direct;
mod driver;
mod error;
mod leases;
mod operation;
mod paths;
mod quiescence;
mod receipt;
mod workspace;

pub use direct::DirectDriver;
pub use driver::WorkspaceDriver;
pub use error::{Result, WorkspaceError};
pub use layerfs_working_store::{
    BeginOperation, BranchHead, BranchId, CommitResult, DiskNamespace, DiskTable, EngineCounters,
    IntegrityMode, LayerId, LayerStackId, OperationRecordRef, ScratchObservation, StorageError,
    VersionRef, WorkingCandidate, WorkingCandidateWrite, WorkingError, WorkingStore,
};
pub use leases::{LeaseGuard, LeaseKind, RuntimeLeases, RuntimeObservation};
pub use operation::{begin_operation, OperationId, OperationState, Presentation, WorkspaceTicket};
pub use paths::WorkspacePaths;
pub use receipt::{BeginOperationReceipt, EndOperationReceipt, FinalizedCandidate};
pub use workspace::{FreezeObservation, OperationWorkspace};

pub const COMPONENT: &str = "layerfs-workspace";
