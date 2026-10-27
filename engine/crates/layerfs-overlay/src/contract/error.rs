//! Original errors and exact failure knowledge, with no operation replay.
use std::{fmt, io, mem::ManuallyDrop, sync::Mutex};

/// Original checked-close refusal and its still-owned connection. Quarantine
/// cannot replay close through Connection's destructor. The failed daemon/slot
/// retains this custody; OS process exit ultimately releases the connection.
pub struct CloseFailure {
    pub cause: rusqlite::Error,
    _connection: Mutex<ManuallyDrop<rusqlite::Connection>>,
    _open: Mutex<ManuallyDrop<crate::open::OpenState>>,
}
impl CloseFailure {
    pub(crate) fn retain(
        (connection, cause): (rusqlite::Connection, rusqlite::Error),
        open: crate::open::OpenState,
    ) -> Self {
        Self {
            cause,
            _connection: Mutex::new(ManuallyDrop::new(connection)),
            _open: Mutex::new(ManuallyDrop::new(open)),
        }
    }
}
impl fmt::Debug for CloseFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CloseFailure")
            .field("cause", &self.cause)
            .field("connection", &"retained; no close replay")
            .finish()
    }
}

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
    Consolidating,
    ReplyAttemptsPending,
    BaseSourcesPending,
    Quarantined,
    Close(Box<CloseFailure>),
    /// A write I/O error on an identity-verified full device, returned only
    /// after the entire atomic job has been rolled back. Keeps SQLite's cause.
    NoSpace {
        cause: Box<OverlayError>,
    },
    Uncertain {
        cause: Box<OverlayError>,
        completion: Option<Box<OverlayError>>,
    },
    UnsupportedPlatform,
}
impl fmt::Display for OverlayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for OverlayError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(cause) => Some(cause),
            Self::Sql(cause) => Some(cause),
            Self::Close(failure) => Some(&failure.cause),
            Self::NoSpace { cause } | Self::Uncertain { cause, .. } => Some(cause.as_ref()),
            _ => None,
        }
    }
}
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
    /// Identifies a direct SQLite capacity error, without asserting rollback.
    /// Outcome knowledge belongs to the caller. An uncertain wrapper is never
    /// classified from its nested cause, even when that cause is SQLite Full.
    pub fn is_sqlite_full(&self) -> bool {
        matches!(self, Self::Sql(rusqlite::Error::SqliteFailure(error, _))
            if error.code == rusqlite::ErrorCode::DiskFull)
    }
    /// Known no-space outcomes of an atomic mutation. Never unwraps an
    /// uncertain result, even if its retained cause is a capacity failure.
    pub fn is_no_space(&self) -> bool {
        self.is_sqlite_full() || matches!(self, Self::NoSpace { .. })
    }
    pub(crate) fn capacity_io(&self) -> bool {
        matches!(self, Self::Sql(rusqlite::Error::SqliteFailure(error, _))
            if matches!(error.extended_code, rusqlite::ffi::SQLITE_IOERR_WRITE
                | rusqlite::ffi::SQLITE_IOERR_TRUNCATE))
    }
    pub(crate) fn unsafe_database_state(&self) -> bool {
        matches!(self, Self::Sql(rusqlite::Error::SqliteFailure(error, _))
            if matches!(error.code, rusqlite::ErrorCode::SystemIoFailure
                | rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase))
    }
}
