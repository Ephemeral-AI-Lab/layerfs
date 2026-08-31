#[derive(Debug)]
pub enum SdkError {
    Store(layerfs_layerstack_store::StoreError),
    Workspace(layerfs_workspace::WorkspaceError),
    Monitor(layerfs_monitor::MonitorError),
    InvalidRequest(&'static str),
}

pub type Result<T> = std::result::Result<T, SdkError>;

impl std::fmt::Display for SdkError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for SdkError {}

impl From<layerfs_layerstack_store::StoreError> for SdkError {
    fn from(value: layerfs_layerstack_store::StoreError) -> Self {
        Self::Store(value)
    }
}

impl From<layerfs_workspace::WorkspaceError> for SdkError {
    fn from(value: layerfs_workspace::WorkspaceError) -> Self {
        Self::Workspace(value)
    }
}

impl From<layerfs_monitor::MonitorError> for SdkError {
    fn from(value: layerfs_monitor::MonitorError) -> Self {
        Self::Monitor(value)
    }
}

impl From<std::io::Error> for SdkError {
    fn from(_value: std::io::Error) -> Self {
        Self::InvalidRequest("I/O")
    }
}
