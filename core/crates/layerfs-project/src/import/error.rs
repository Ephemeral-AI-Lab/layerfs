//! Typed construction, storage, history and host I/O failures.
use layerfs_content::ContentError;
use layerfs_history::HistoryError;
use layerfs_storage::StorageError;
use std::{fmt, io};
/// Result of a namespace initialization operation.
pub type ProjectResult<T> = Result<T, ProjectError>;
/// A refused or failed initialization. Previously registered objects remain valid.
#[derive(Debug)]
pub enum ProjectError {
    /// The source or frozen file metadata is invalid.
    InvalidInput,
    /// The source contains a kind the native importer cannot construct.
    Unsupported,
    /// A named construction/allocation bound was exceeded.
    Capacity,
    /// The host-provided operation deadline expired.
    Deadline,
    /// A constructor worker could not complete its handoff.
    WorkerUnavailable,
    /// Canonical construction failed.
    Content(ContentError),
    /// Storage failed, retaining uncertainty in its original typed carrier.
    Storage(StorageError),
    /// Catalog publication/allocation failed with its exact typed context.
    History(HistoryError),
    /// Host I/O failed.
    Io(io::Error),
    /// Scratch cleanup failed after the stated construction result.
    Cleanup {
        /// Original construction failure, when one occurred.
        cause: Option<Box<ProjectError>>,
        /// Cleanup I/O failure.
        error: io::Error,
    },
}
impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "namespace Init: {self:?}")
    }
}
impl std::error::Error for ProjectError {}
impl From<io::Error> for ProjectError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
#[cfg(unix)]
pub(crate) fn content(error: ContentError) -> ProjectError {
    ProjectError::Content(error)
}
#[cfg(unix)]
pub(crate) fn storage(error: StorageError) -> ProjectError {
    ProjectError::Storage(error)
}
