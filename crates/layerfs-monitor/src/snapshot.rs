use crate::dedup::DedupAnalysis;
use crate::resource::ProcessSnapshot;
use crate::OperationReceipt;
use layerfs_storage::StoreStorageSnapshot;
use layerfs_workspace::WorkspaceSummary;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatabaseSnapshot {
    pub role: String,
    pub location: String,
    pub storage: StoreStorageSnapshot,
}

#[derive(Clone, Debug)]
pub struct MonitorSnapshot {
    pub databases: Vec<DatabaseSnapshot>,
    pub workspaces: Vec<WorkspaceSummary>,
    pub operations: Vec<OperationReceipt>,
    pub dedup: Option<DedupAnalysis>,
    pub process_id: u32,
    pub resident_bytes: Option<u64>,
    pub available_parallelism: usize,
}

impl MonitorSnapshot {
    pub(crate) fn with_process(mut self, process: ProcessSnapshot) -> Self {
        self.process_id = process.process_id;
        self.resident_bytes = process.resident_bytes;
        self.available_parallelism = process.available_parallelism;
        self
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

impl std::fmt::Display for MonitorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
