//! Typed construction, storage, history, acquisition and host I/O failures.
use layerfs_content::ContentError;
use layerfs_history::HistoryError;
use layerfs_storage::port::acquisition::{AcquisitionError, AcquisitionWork, Owner};
use layerfs_storage::port::PersistenceError;
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
    /// The acquisition backing's files are the source or lie inside it;
    /// nothing was begun.
    BackingInsideSource,
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
    /// One acquisition backing unit failed with its exact typed outcome.
    Acquisition(AcquisitionError),
    /// Host I/O failed.
    Io(io::Error),
    /// Removing acquisition working state failed after the stated result.
    /// Nothing was published by this Init.
    Cleanup {
        /// Original construction failure, when one occurred.
        cause: Option<Box<ProjectError>>,
        /// Deciding cleanup failure: the first unit that did not complete.
        error: AcquisitionError,
        /// Working state the operation still owns; nothing here was retried.
        retained: RetainedAcquisition,
    },
    /// A Store, history or backing outcome is unknown. Nothing further was
    /// attempted: the operation's working state is exactly as that unit left it.
    Uncertain {
        /// The failure whose outcome is unknown.
        cause: Box<ProjectError>,
        /// The operation whose record and working rows were left in place.
        retained: RetainedAcquisition,
    },
}
/// Acquisition working state one Init still owns after it returned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetainedAcquisition {
    /// The operation and the provider session that began it.
    pub owner: Owner,
    /// Exact logical charges it still holds, when the backing reported them
    /// after the failure. Absent when that one read was refused or not attempted.
    pub work: Option<AcquisitionWork>,
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
impl ProjectError {
    /// True when a persistence outcome behind this failure is unknown.
    pub(crate) fn unknown_outcome(&self) -> bool {
        match self {
            Self::Storage(error) => storage_unknown(error),
            Self::History(error) => error.unknown(),
            Self::Acquisition(AcquisitionError::Persistence(error)) => {
                *error == PersistenceError::Uncertain
            }
            _ => false,
        }
    }
}
#[cfg(unix)]
fn storage_unknown(error: &StorageError) -> bool {
    match error {
        StorageError::UnknownOutcome { .. } => true,
        StorageError::CleanupFailed { original, cleanup } => {
            storage_unknown(original) || storage_unknown(cleanup)
        }
        _ => false,
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
#[cfg(unix)]
pub(crate) fn acquisition(error: AcquisitionError) -> ProjectError {
    ProjectError::Acquisition(error)
}
/// A backing answer that contradicts what this operation itself wrote.
#[cfg(unix)]
pub(crate) fn malformed() -> ProjectError {
    ProjectError::Acquisition(AcquisitionError::Persistence(PersistenceError::Malformed))
}
