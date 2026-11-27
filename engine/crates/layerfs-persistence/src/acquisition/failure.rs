//! One acquisition unit's failure before it is reported through the port,
//! and the signed/unsigned conversions both engines' units share.
use crate::backend::error::{BackendError, OutcomeError};
use layerfs_storage::port::{acquisition::AcquisitionError, PersistenceError};

/// One unit's failure before it is reported through the port.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Failure {
    Backend(BackendError),
    /// The Store was created without the acquisition tables.
    Unavailable,
    /// The operation still holds working rows.
    Held,
    Stale,
    Bounds,
    Changed(u64),
}
impl From<BackendError> for Failure {
    fn from(error: BackendError) -> Self {
        Self::Backend(error)
    }
}
impl OutcomeError for Failure {
    fn uncertain(&self) -> bool {
        *self == Self::Backend(BackendError::Unknown)
    }
}
impl From<Failure> for AcquisitionError {
    fn from(failure: Failure) -> Self {
        match failure {
            Failure::Backend(error) => Self::Persistence(error.into()),
            Failure::Unavailable => Self::Persistence(PersistenceError::BackendUnavailable),
            Failure::Held => Self::Persistence(PersistenceError::Refused {
                status: "acquisition working rows remain".to_owned(),
            }),
            Failure::Stale => Self::Stale,
            Failure::Bounds => Self::Bounds,
            Failure::Changed(position) => Self::Changed { position },
        }
    }
}

/// A position as the engine's signed integer; larger values are out of bounds.
pub(crate) fn signed(value: u64) -> Result<i64, Failure> {
    i64::try_from(value).map_err(|_| Failure::Bounds)
}
/// A stored non-negative integer.
pub(crate) fn unsigned(value: i64) -> Result<u64, Failure> {
    u64::try_from(value).map_err(|_| Failure::Backend(BackendError::Integrity))
}
