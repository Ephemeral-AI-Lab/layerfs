use std::fmt;

#[derive(Debug)]
pub enum SdkError {
    Storage(layerfs_storage::StorageError),
    Workspace(layerfs_workspace::WorkspaceError),
    Monitor(layerfs_monitor::MonitorError),
    MissingLayer,
    MissingStack,
    MissingBranch,
    ActiveDependents,
}

impl fmt::Display for SdkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for SdkError {}

impl From<layerfs_storage::StorageError> for SdkError {
    fn from(value: layerfs_storage::StorageError) -> Self {
        Self::Storage(value)
    }
}

impl From<layerfs_workspace::WorkspaceError> for SdkError {
    fn from(value: layerfs_workspace::WorkspaceError) -> Self {
        Self::Workspace(value)
    }
}
