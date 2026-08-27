//! Universal runtime boundary for one isolated LayerFS operation.

#![forbid(unsafe_code)]

mod direct;
mod driver;
mod leases;
mod operation;
mod quiescence;
mod receipt;
mod workspace;

pub use direct::DirectDriver;
pub use driver::WorkspaceDriver;
pub use layerfs_working_store::{
    BeginOperation, BranchHead, BranchId, CommitResult, DiskNamespace, DiskTable, EngineCounters,
    IntegrityMode, LayerId, LayerStackId, OperationRecordRef, ScratchObservation, StorageError,
    VersionRef, WorkingCandidate, WorkingCandidateWrite, WorkingError, WorkingStore,
};
pub use leases::{LeaseGuard, LeaseKind, RuntimeLeases, RuntimeObservation};
pub use operation::{OperationId, OperationState, Presentation, WorkspaceTicket};
pub use receipt::{BeginOperationReceipt, EndOperationReceipt, FinalizedCandidate};
#[cfg(feature = "test-hooks")]
#[doc(hidden)]
pub use workspace::inject_remove_owned_failure_for_test;
pub use workspace::{FreezeObservation, OperationWorkspace, WorkspacePaths};

pub fn begin_operation(
    working: &layerfs_working_store::WorkingStore,
    expected: layerfs_working_store::BranchHead,
    presentation: Presentation,
) -> layerfs_working_store::Result<(layerfs_working_store::BeginOperation, WorkspaceTicket)> {
    let admission = working.begin_operation(expected)?;
    let ticket = WorkspaceTicket::from_admission(&admission, presentation);
    Ok((admission, ticket))
}

use std::fmt;

pub const COMPONENT: &str = "layerfs-workspace";

#[derive(Debug)]
pub enum WorkspaceError {
    Busy,
    Driver(String),
    InvalidState,
    OwnershipMismatch,
    ResourceExhausted,
    Timeout,
    Io(std::io::Error),
}

impl fmt::Display for WorkspaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy => formatter.write_str("workspace is busy"),
            Self::Driver(error) => write!(formatter, "workspace driver failed: {error}"),
            Self::InvalidState => formatter.write_str("invalid workspace state"),
            Self::OwnershipMismatch => formatter.write_str("workspace ownership mismatch"),
            Self::ResourceExhausted => formatter.write_str("workspace resource limit exceeded"),
            Self::Timeout => formatter.write_str("workspace quiescence timed out"),
            Self::Io(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for WorkspaceError {}

impl From<std::io::Error> for WorkspaceError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub type Result<T> = std::result::Result<T, WorkspaceError>;
