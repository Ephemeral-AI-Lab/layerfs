//! Exact underlying operation failures at the host adapter boundary.
use std::fmt;

/// Host runtime failures, without replay or guessed publication recovery.
#[derive(Debug)]
pub enum RuntimeError {
    /// Application authority denied this exact request.
    Denied,
    /// Malformed adapter configuration or request.
    Invalid(&'static str),
    /// No unused session slot; nothing has begun.
    AdmissionUnavailable,
    /// Wrong runtime/peer/Workspace or stale Save capability.
    StaleCapability,
    /// This capability's terminal operation was already attempted.
    AlreadyAttempted,
    /// A terminal uncertainty/cleanup failure still owns its slot.
    RetainedCustody,
    /// Exact owning content failure.
    Content(layerfs_content::ContentError),
    /// Exact owning storage failure, including nested original/cleanup errors.
    Storage(std::sync::Arc<layerfs_storage::StorageError>),
    /// Exact owning history failure.
    History(layerfs_history::HistoryError),
    /// Application reply sink failed after any already delivered objects.
    Reply(std::io::Error),
}

impl From<layerfs_content::ContentError> for RuntimeError {
    fn from(error: layerfs_content::ContentError) -> Self {
        Self::Content(error)
    }
}
impl From<layerfs_storage::StorageError> for RuntimeError {
    fn from(error: layerfs_storage::StorageError) -> Self {
        Self::Storage(std::sync::Arc::new(error))
    }
}
impl From<layerfs_history::HistoryError> for RuntimeError {
    fn from(error: layerfs_history::HistoryError) -> Self {
        Self::History(error)
    }
}
impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Denied => f.write_str("runtime authority denied request"),
            Self::Invalid(what) => write!(f, "invalid runtime input: {what}"),
            Self::AdmissionUnavailable => f.write_str("runtime session admission unavailable"),
            Self::StaleCapability => f.write_str("stale or foreign runtime capability"),
            Self::AlreadyAttempted => f.write_str("runtime operation already attempted"),
            Self::RetainedCustody => f.write_str("runtime uncertain completion retains custody"),
            Self::Content(error) => error.fmt(f),
            Self::Storage(error) => error.fmt(f),
            Self::History(error) => error.fmt(f),
            Self::Reply(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for RuntimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Content(error) => Some(error),
            Self::Storage(error) => Some(error),
            Self::History(error) => Some(error),
            Self::Reply(error) => Some(error),
            _ => None,
        }
    }
}
/// Typed result for one attempted runtime operation.
pub type RuntimeResult<T> = Result<T, RuntimeError>;
