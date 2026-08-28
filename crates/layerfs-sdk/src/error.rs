use std::fmt;

#[derive(Debug)]
pub enum Error {
    Core(layerfs_core::CoreError),
    Working(layerfs_working_store::WorkingError),
    Workspace(layerfs_workspace::WorkspaceError),
    Sync(layerfs_sync::SyncError),
    Materialization(layerfs_materialization::VfsError),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for Error {}

impl From<layerfs_core::CoreError> for Error {
    fn from(error: layerfs_core::CoreError) -> Self {
        Self::Core(error)
    }
}

impl From<layerfs_working_store::WorkingError> for Error {
    fn from(error: layerfs_working_store::WorkingError) -> Self {
        Self::Working(error)
    }
}

impl From<layerfs_workspace::WorkspaceError> for Error {
    fn from(error: layerfs_workspace::WorkspaceError) -> Self {
        Self::Workspace(error)
    }
}

impl From<layerfs_sync::SyncError> for Error {
    fn from(error: layerfs_sync::SyncError) -> Self {
        Self::Sync(error)
    }
}

impl From<layerfs_materialization::VfsError> for Error {
    fn from(error: layerfs_materialization::VfsError) -> Self {
        Self::Materialization(error)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
