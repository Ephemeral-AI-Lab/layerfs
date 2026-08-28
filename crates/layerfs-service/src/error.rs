use layerfs_durable_store::DurableError;
use layerfs_sync::SyncError;
use std::fmt;

#[derive(Debug)]
pub enum ServiceError {
    Authentication,
    StorageIdentity,
    InvalidConfiguration,
    Durable(DurableError),
    Sync(SyncError),
    Io(std::io::Error),
    Wire(String),
}

impl fmt::Display for ServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ServiceError {}

impl From<DurableError> for ServiceError {
    fn from(value: DurableError) -> Self {
        Self::Durable(value)
    }
}

impl From<SyncError> for ServiceError {
    fn from(value: SyncError) -> Self {
        Self::Sync(value)
    }
}

impl From<std::io::Error> for ServiceError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub type Result<T> = std::result::Result<T, ServiceError>;
