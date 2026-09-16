//! Typed storage failures, including the unknown-persistence-outcome boundary.
//!
//! There is one attempt per operation. A failure is returned to the caller
//! unchanged; a failure that leaves the persistence outcome unproven is reported
//! as [`StorageError::UnknownOutcome`] and never triggers a resend, a polling
//! loop or destructive cleanup on a guess.

use std::fmt;

use layerfs_content::{ContentError, ObjectId};

/// Failures produced by the physical storage component.
#[derive(Debug)]
pub enum StorageError {
    /// A canonical construction, framing or read check failed.
    Content(ContentError),
    /// The embedded engine reported a failure for one attempted statement.
    Engine(rusqlite::Error),
    /// The requested object is not stored.
    ObjectMissing(ObjectId),
    /// Stored bytes differ from the bytes offered under the same identity.
    Collision(ObjectId),
    /// A direct logical dependency is not stored and not in the current batch.
    MissingDependency {
        /// The object whose reference is unresolved.
        object: ObjectId,
        /// The unresolved direct reference.
        reference: ObjectId,
    },
    /// A location lies outside the retained-pack ceiling of this read.
    VisibilityCeiling {
        /// Pack the record lives in.
        pack_id: i64,
        /// Highest pack this read may observe.
        ceiling: i64,
    },
    /// Another writer holds the Store's write ownership.
    OwnershipUnavailable,
    /// The requested policy or profile is not implemented by this slice.
    UnsupportedPolicy {
        /// The rejected field or profile.
        field: &'static str,
    },
    /// A physical framing, locator or cardinality invariant failed.
    Integrity(&'static str),
    /// A declared bound would be exceeded.
    CapacityExceeded {
        /// The bounded resource.
        what: &'static str,
        /// The declared limit.
        limit: u64,
        /// The requested or observed size.
        actual: u64,
    },
    /// Acknowledgement was not established, so the write may have committed.
    UnknownOutcome {
        /// The failure that prevented acknowledgement.
        original: Box<StorageError>,
    },
    /// The failed save's cleanup did not complete; both errors are retained.
    CleanupFailed {
        /// The failure that ended the operation.
        original: Box<StorageError>,
        /// The failure observed while cleaning up.
        cleanup: Box<StorageError>,
    },
    /// The operation was ended by an explicit abort before success.
    Aborted,
}

impl StorageError {
    /// True when this failure leaves the persistence outcome unproven.
    pub fn is_unknown_outcome(&self) -> bool {
        matches!(self, Self::UnknownOutcome { .. })
    }
}

impl fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Content(error) => write!(formatter, "content: {error}"),
            Self::Engine(error) => write!(formatter, "engine: {error}"),
            Self::ObjectMissing(id) => write!(formatter, "object {id} is not stored"),
            Self::Collision(id) => write!(formatter, "identity collision for {id}"),
            Self::MissingDependency { object, reference } => {
                write!(formatter, "{object} depends on missing {reference}")
            }
            Self::VisibilityCeiling { pack_id, ceiling } => {
                write!(
                    formatter,
                    "pack {pack_id} beyond retained ceiling {ceiling}"
                )
            }
            Self::OwnershipUnavailable => formatter.write_str("save ownership unavailable"),
            Self::UnsupportedPolicy { field } => {
                write!(formatter, "unsupported storage policy: {field}")
            }
            Self::Integrity(what) => write!(formatter, "storage integrity: {what}"),
            Self::CapacityExceeded {
                what,
                limit,
                actual,
            } => {
                write!(
                    formatter,
                    "bounded capacity {what} limit {limit} exceeded by {actual}"
                )
            }
            Self::UnknownOutcome { original } => {
                write!(formatter, "unknown persistence outcome after: {original}")
            }
            Self::CleanupFailed { original, cleanup } => {
                write!(formatter, "cleanup failed after {original}: {cleanup}")
            }
            Self::Aborted => formatter.write_str("save aborted before success"),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<ContentError> for StorageError {
    fn from(error: ContentError) -> Self {
        Self::Content(error)
    }
}

impl From<rusqlite::Error> for StorageError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Engine(error)
    }
}

/// Result alias for this component.
pub type StorageResult<T> = Result<T, StorageError>;
