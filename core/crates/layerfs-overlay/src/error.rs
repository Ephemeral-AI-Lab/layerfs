//! Original errors and exact failure knowledge, with no operation replay.
use std::{fmt, io};

pub type OverlayResult<T> = Result<T, OverlayError>;
#[derive(Debug)]
pub enum OverlayError {
    Io(io::Error),
    Sql(rusqlite::Error),
    Invalid(&'static str),
    Missing,
    Stale,
    Closed,
    CaptureInFlight,
    ReplyAttemptsPending,
    Quarantined,
    Uncertain {
        cause: Box<OverlayError>,
        completion: Option<rusqlite::Error>,
    },
    UnsupportedPlatform,
}
impl fmt::Display for OverlayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for OverlayError {}
impl From<rusqlite::Error> for OverlayError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sql(value)
    }
}
impl From<io::Error> for OverlayError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl OverlayError {
    pub(crate) fn unsafe_database_state(&self) -> bool {
        matches!(self, Self::Sql(rusqlite::Error::SqliteFailure(error, _))
            if matches!(error.code, rusqlite::ErrorCode::SystemIoFailure
                | rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase))
    }
}
