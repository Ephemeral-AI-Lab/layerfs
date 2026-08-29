use crate::dedup::DedupAnalysis;
use crate::resource::ProcessSnapshot;
use crate::{BranchStoreId, OperationId, OperationReceipt};
use layerfs_storage::{BranchId, BranchRecord, StoreStorageSnapshot};
use layerfs_workspace::{WorkspaceSessionId, WorkspaceSummary};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatabaseSnapshot {
    pub role: String,
    pub location: String,
    pub storage: StoreStorageSnapshot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MonitorScope {
    Databases,
    Dedup { route: Option<BranchStoreId> },
    Workspace(Option<WorkspaceSessionId>),
    Branch(BranchId),
    Operation(Option<OperationId>),
    Process,
}

#[derive(Clone, Debug)]
pub enum MonitorSnapshot {
    Databases(Vec<DatabaseSnapshot>),
    Dedup(Vec<(BranchStoreId, DedupAnalysis)>),
    Workspaces(Vec<WorkspaceSummary>),
    Branch(Option<BranchRecord>),
    Operations(Vec<OperationReceipt>),
    Process {
        process_id: u32,
        resident_bytes: Option<u64>,
        available_parallelism: usize,
    },
}

impl From<ProcessSnapshot> for MonitorSnapshot {
    fn from(value: ProcessSnapshot) -> Self {
        Self::Process {
            process_id: value.process_id,
            resident_bytes: value.resident_bytes,
            available_parallelism: value.available_parallelism,
        }
    }
}

#[derive(Debug)]
pub enum MonitorError {
    Storage(layerfs_storage::StorageError),
    Workspace(layerfs_workspace::WorkspaceError),
    Io(std::io::Error),
    NotFound,
    Integrity(&'static str),
}

pub type MonitorResult<T> = Result<T, MonitorError>;

impl fmt::Display for MonitorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for MonitorError {}

impl From<layerfs_storage::StorageError> for MonitorError {
    fn from(value: layerfs_storage::StorageError) -> Self {
        Self::Storage(value)
    }
}

impl From<layerfs_workspace::WorkspaceError> for MonitorError {
    fn from(value: layerfs_workspace::WorkspaceError) -> Self {
        Self::Workspace(value)
    }
}

impl From<std::io::Error> for MonitorError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}
