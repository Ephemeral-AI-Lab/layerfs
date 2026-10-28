//! Typed construction, storage, history, acquisition and host I/O failures.
use layerfs_content::ContentError;
use layerfs_history::HistoryError;
use layerfs_storage::port::acquisition::{AcquisitionError, AcquisitionWork, Owner};
use layerfs_storage::port::PersistenceError;
use layerfs_storage::StorageError;
use std::{fmt, io, path::PathBuf};
/// Result of a namespace initialization operation.
pub type ProjectResult<T> = Result<T, ProjectError>;
/// A refused or failed initialization. Previously registered objects remain valid.
#[derive(Debug)]
pub enum ProjectError {
    /// A source entry was refused: `reason` names the check it failed and
    /// `path` the host entry.
    InvalidInput {
        /// The check the entry failed.
        reason: SourceRefusal,
        /// Host path of the refused entry.
        path: PathBuf,
    },
    /// The source contains a kind the host importer cannot construct.
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
/// Why Init refused a source entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceRefusal {
    /// The source is a symbolic link or not a directory.
    NotDirectory,
    /// Permission bits portable metadata cannot hold (set-user-ID or
    /// set-group-ID on any entry, the sticky bit on a regular file); the
    /// entry's `0o7777` bits.
    Mode(u32),
    /// Modification-time nanoseconds outside `0..1_000_000_000`.
    Mtime,
    /// The entry changed while Init read it, or a later path of one file
    /// no longer matches the first.
    Changed,
}
impl fmt::Display for SourceRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotDirectory => f.write_str("the source is not a directory"),
            Self::Mode(mode) => write!(
                f,
                "mode {mode:04o} has bits LayerFS cannot store (set-user-ID, set-group-ID, or sticky on a file)"
            ),
            Self::Mtime => f.write_str("the modification time is out of range"),
            Self::Changed => f.write_str("the entry changed while Init read it"),
        }
    }
}
impl ProjectError {
    /// A refusal of the source entry at `path`.
    pub(crate) fn refused(reason: SourceRefusal, path: impl Into<PathBuf>) -> Self {
        Self::InvalidInput {
            reason,
            path: path.into(),
        }
    }
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
fn storage_unknown(error: &StorageError) -> bool {
    match error {
        StorageError::UnknownOutcome { .. } => true,
        StorageError::CleanupFailed { original, cleanup } => {
            storage_unknown(original) || storage_unknown(cleanup)
        }
        _ => false,
    }
}
pub(crate) fn content(error: ContentError) -> ProjectError {
    ProjectError::Content(error)
}
pub(crate) fn storage(error: StorageError) -> ProjectError {
    ProjectError::Storage(error)
}
pub(crate) fn acquisition(error: AcquisitionError) -> ProjectError {
    ProjectError::Acquisition(error)
}
/// A backing answer that contradicts what this operation itself wrote.
pub(crate) fn malformed() -> ProjectError {
    ProjectError::Acquisition(AcquisitionError::Persistence(PersistenceError::Malformed))
}
