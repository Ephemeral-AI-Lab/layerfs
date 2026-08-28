use crate::driver::DriverError;
use layerfs_core::{CoreError, ObjectId};
use layerfs_workspace::StorageError;
use std::fmt;

#[derive(Debug)]
pub enum VfsError {
    Core(CoreError),
    Storage(StorageError),
    Driver(DriverError),
    Io(std::io::Error),
    WorkspaceBusy,
    ExternalDirtyConflict,
    ExternalHardLinkBoundary,
    NativeProtected,
    CommittedCleanup {
        root: ObjectId,
        error: Box<VfsError>,
    },
    InvalidState,
    Indeterminate,
    IncompleteDerived,
}

pub type VfsResult<T> = Result<T, VfsError>;

impl From<CoreError> for VfsError {
    fn from(value: CoreError) -> Self {
        Self::Core(value)
    }
}

impl From<StorageError> for VfsError {
    fn from(value: StorageError) -> Self {
        Self::Storage(value)
    }
}

impl From<DriverError> for VfsError {
    fn from(value: DriverError) -> Self {
        if matches!(value, DriverError::NativeProtected) {
            Self::NativeProtected
        } else {
            Self::Driver(value)
        }
    }
}

impl From<std::io::Error> for VfsError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl fmt::Display for VfsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for VfsError {}
