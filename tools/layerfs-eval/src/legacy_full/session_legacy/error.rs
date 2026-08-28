use layerfs_core::{CoreError, ObjectId};
use layerfs_materialization::driver::DriverError;
use layerfs_storage::EngineError;
use std::fmt;

#[derive(Debug)]
pub enum VfsError {
    Core(CoreError),
    Engine(EngineError),
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
impl From<EngineError> for VfsError {
    fn from(value: EngineError) -> Self {
        Self::Engine(value)
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Core(error) => write!(f, "core: {error:?}"),
            Self::Engine(error) => write!(f, "storage: {error}"),
            Self::Driver(error) => write!(f, "projection: {error:?}"),
            Self::Io(error) => write!(f, "I/O: {error}"),
            Self::WorkspaceBusy => f.write_str("workspace busy"),
            Self::ExternalDirtyConflict => f.write_str("external dirty conflict"),
            Self::ExternalHardLinkBoundary => f.write_str("external hard-link boundary"),
            Self::NativeProtected => f.write_str("native object protected"),
            Self::CommittedCleanup { root, error } => {
                write!(f, "root {root} committed; cleanup failed: {error}")
            }
            Self::InvalidState => f.write_str("invalid state"),
            Self::Indeterminate => f.write_str("indeterminate"),
            Self::IncompleteDerived => f.write_str("incomplete derived state"),
        }
    }
}
impl std::error::Error for VfsError {}
