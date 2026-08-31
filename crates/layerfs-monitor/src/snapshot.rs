use crate::{DedupAnalysis, OperationReceipt};
use layerfs_layerstack_store::{LayerStackStore, StoreError};
use layerfs_workspace::{WorkspaceError, WorkspaceSummary};
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatabaseSnapshot {
    pub location: PathBuf,
    pub database_bytes: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MonitorSnapshot {
    pub database: DatabaseSnapshot,
    pub workspaces: Vec<WorkspaceSummary>,
    pub operations: Vec<OperationReceipt>,
    pub last_analysis: Option<DedupAnalysis>,
}

#[derive(Debug)]
pub enum MonitorError {
    Store(StoreError),
    Workspace(WorkspaceError),
    Integrity(&'static str),
}

pub type MonitorResult<T> = std::result::Result<T, MonitorError>;

impl From<StoreError> for MonitorError {
    fn from(value: StoreError) -> Self {
        Self::Store(value)
    }
}

impl From<WorkspaceError> for MonitorError {
    fn from(value: WorkspaceError) -> Self {
        Self::Workspace(value)
    }
}

impl std::fmt::Display for MonitorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for MonitorError {}

pub(crate) fn database_snapshot(store: &LayerStackStore) -> MonitorResult<DatabaseSnapshot> {
    let storage = store.storage_snapshot()?;
    Ok(DatabaseSnapshot {
        location: store.path().to_owned(),
        database_bytes: storage.database_bytes,
    })
}
