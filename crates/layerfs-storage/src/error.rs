use crate::SqliteProfile;
use layerfs_core::{CoreError, ObjectId};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SqliteErrorKind {
    Busy,
    Locked,
    PermissionDenied,
    NoSpace,
    Corrupt,
    ReadOnly,
    Constraint,
    Io,
    Other,
}

#[derive(Debug, Eq, PartialEq)]
pub enum EngineError {
    Core(CoreError),
    Sqlite {
        kind: SqliteErrorKind,
        message: String,
    },
    MissingObject(ObjectId),
    MissingRoot(ObjectId),
    MissingDelta(ObjectId),
    IdentityMismatch {
        expected: ObjectId,
        actual: ObjectId,
    },
    MalformedObject {
        id: ObjectId,
        cause: CoreError,
    },
    ImmutableConflict(&'static str, ObjectId),
    InvalidRange {
        start: u64,
        end: u64,
        length: u64,
    },
    ShortRead {
        expected: u64,
        actual: u64,
    },
    ParentMismatch {
        expected: Option<ObjectId>,
        actual: Option<ObjectId>,
    },
    SchemaMismatch,
    ProfileMismatch,
    SqliteProfileMismatch(SqliteProfile),
    InvalidRecord(&'static str),
    InvalidTransaction,
    PublicationConflict,
    UnresolvedGenerationResidue {
        generation: u64,
    },
    AmbiguousDurability,
    CounterOverflow,
    InjectedFailure(&'static str),
}

impl fmt::Display for EngineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Core(error) => error.fmt(formatter),
            Self::Sqlite { kind, message } => write!(formatter, "SQLite {kind:?}: {message}"),
            Self::MissingObject(id) => write!(formatter, "object {id} is missing"),
            Self::MissingRoot(id) => write!(formatter, "root {id} is missing"),
            Self::MissingDelta(id) => write!(formatter, "delta {id} is missing"),
            Self::IdentityMismatch { expected, actual } => {
                write!(
                    formatter,
                    "identity mismatch: expected {expected}, got {actual}"
                )
            }
            Self::MalformedObject { id, cause } => {
                write!(formatter, "object {id} is malformed: {cause}")
            }
            Self::ImmutableConflict(kind, id) => {
                write!(formatter, "immutable {kind} {id} conflicts")
            }
            Self::InvalidRange { start, end, length } => {
                write!(
                    formatter,
                    "invalid range {start}..{end} for length {length}"
                )
            }
            Self::ShortRead { expected, actual } => {
                write!(formatter, "short read: expected {expected}, got {actual}")
            }
            Self::ParentMismatch { expected, actual } => {
                write!(
                    formatter,
                    "parent mismatch: expected {expected:?}, got {actual:?}"
                )
            }
            Self::SchemaMismatch => formatter.write_str("SQLite schema marker mismatch"),
            Self::ProfileMismatch => formatter.write_str("SQLite profile mismatch"),
            Self::SqliteProfileMismatch(profile) => {
                write!(formatter, "SQLite profile mismatch: {profile:?}")
            }
            Self::InvalidRecord(name) => write!(formatter, "invalid durable {name} record"),
            Self::InvalidTransaction => formatter.write_str("capture transaction is not active"),
            Self::PublicationConflict => {
                formatter.write_str("publication expected ref does not match")
            }
            Self::UnresolvedGenerationResidue { generation } => {
                write!(
                    formatter,
                    "unresolved Store generation {generation} residue"
                )
            }
            Self::AmbiguousDurability => formatter.write_str("publication outcome is ambiguous"),
            Self::CounterOverflow => formatter.write_str("counter arithmetic overflow"),
            Self::InjectedFailure(point) => write!(formatter, "injected failure at {point}"),
        }
    }
}

impl std::error::Error for EngineError {}

impl From<CoreError> for EngineError {
    fn from(error: CoreError) -> Self {
        Self::Core(error)
    }
}

pub type EngineResult<T> = Result<T, EngineError>;
pub type StorageError = EngineError;

pub(crate) fn engine_step(step: &'static str, error: EngineError) -> EngineError {
    match error {
        EngineError::Sqlite { kind, message } => EngineError::Sqlite {
            kind,
            message: format!("{step}: {message}"),
        },
        error => error,
    }
}

pub(crate) fn io_engine_error(error: std::io::Error) -> EngineError {
    EngineError::Sqlite {
        kind: SqliteErrorKind::Io,
        message: error.to_string(),
    }
}

pub(crate) fn sqlite_error_kind(error: &rusqlite::Error) -> SqliteErrorKind {
    match error {
        rusqlite::Error::SqliteFailure(error, _) => match error.code {
            rusqlite::ErrorCode::DatabaseBusy => SqliteErrorKind::Busy,
            rusqlite::ErrorCode::DatabaseLocked => SqliteErrorKind::Locked,
            rusqlite::ErrorCode::PermissionDenied => SqliteErrorKind::PermissionDenied,
            rusqlite::ErrorCode::DiskFull => SqliteErrorKind::NoSpace,
            rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase => {
                SqliteErrorKind::Corrupt
            }
            rusqlite::ErrorCode::ReadOnly => SqliteErrorKind::ReadOnly,
            rusqlite::ErrorCode::ConstraintViolation => SqliteErrorKind::Constraint,
            rusqlite::ErrorCode::SystemIoFailure => SqliteErrorKind::Io,
            _ => SqliteErrorKind::Other,
        },
        rusqlite::Error::BlobSizeError => SqliteErrorKind::Io,
        _ => SqliteErrorKind::Other,
    }
}

pub(crate) fn map_sqlite_error(error: rusqlite::Error) -> EngineError {
    let kind = sqlite_error_kind(&error);
    EngineError::Sqlite {
        kind,
        message: error.to_string(),
    }
}
