use std::fmt;

use crate::identity::ObjectId;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CoreError {
    InvalidPath,
    PathLimitExceeded,
    InvalidObjectKind {
        tag: u8,
    },
    LengthOverflow,
    ObjectLimitExceeded,
    MissingObject,
    IdentityMismatch,
    LengthMismatch {
        expected: u64,
        actual: u64,
    },
    InvalidRange {
        start: u64,
        end: u64,
        length: u64,
    },
    BoundedResynchronization {
        scanned: u64,
        limit: u64,
    },
    UnexpectedEof,
    TrailingBytes,
    NonCanonicalOrdering,
    NonCanonicalPagePartition,
    Unsupported,
    Io,
    InvalidIdentityLength {
        expected: usize,
        actual: usize,
    },
    InvalidIdentityText,
    InvalidUtf8,
    NameCollision,
    PathNotFound,
    NotDirectory,
    RootMutation,
    InvalidRename,
    DeltaParentMismatch {
        expected: ObjectId,
        actual: ObjectId,
    },
    DeltaChildMismatch {
        expected: ObjectId,
        actual: ObjectId,
    },
    DeltaConflict,
    WrongLogicalRole,
    InvalidMappingTag {
        tag: u8,
    },
    UnsupportedMappingVersion {
        version: u16,
    },
    MappingDepthExceeded,
    MappingCycle,
    ChunkLengthMismatch,
    ChunkIdentityMismatch,
    AllocationBudgetExceeded,
    AllocationFailed,
    InvalidValidationReceipt,
    SchemaMigrationRequired,
    ValidationAuthorityUnavailable,
    PublicationConflict,
    AmbiguousDurability,
}

pub type CoreResult<T> = Result<T, CoreError>;

impl fmt::Display for CoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath => formatter.write_str("invalid canonical path"),
            Self::PathLimitExceeded => formatter.write_str("canonical path limit exceeded"),
            Self::InvalidObjectKind { tag } => write!(formatter, "invalid object kind {tag:#x}"),
            Self::LengthOverflow => formatter.write_str("length arithmetic overflow"),
            Self::ObjectLimitExceeded => formatter.write_str("object limit exceeded"),
            Self::MissingObject => formatter.write_str("object is missing"),
            Self::IdentityMismatch => {
                formatter.write_str("identity does not match canonical bytes")
            }
            Self::LengthMismatch { expected, actual } => {
                write!(
                    formatter,
                    "length mismatch: expected {expected}, got {actual}"
                )
            }
            Self::InvalidRange { start, end, length } => write!(
                formatter,
                "invalid range {start}..{end} for logical length {length}"
            ),
            Self::BoundedResynchronization { scanned, limit } => write!(
                formatter,
                "bounded resynchronization failed after scanning {scanned} bytes (limit {limit})"
            ),
            Self::UnexpectedEof => formatter.write_str("unexpected end of input"),
            Self::TrailingBytes => formatter.write_str("trailing bytes"),
            Self::NonCanonicalOrdering => formatter.write_str("non-canonical ordering"),
            Self::NonCanonicalPagePartition => formatter.write_str("non-canonical page partition"),
            Self::Unsupported => formatter.write_str("unsupported canonical bytes"),
            Self::Io => formatter.write_str("I/O error"),
            Self::InvalidIdentityLength { expected, actual } => {
                write!(
                    formatter,
                    "invalid identity length: expected {expected}, got {actual}"
                )
            }
            Self::InvalidIdentityText => formatter.write_str("invalid identity text"),
            Self::InvalidUtf8 => formatter.write_str("invalid UTF-8"),
            Self::NameCollision => formatter.write_str("directory name already exists"),
            Self::PathNotFound => formatter.write_str("path does not exist"),
            Self::NotDirectory => formatter.write_str("path component is not a directory"),
            Self::RootMutation => formatter.write_str("root cannot be added, removed, or replaced"),
            Self::InvalidRename => formatter.write_str("invalid rename"),
            Self::DeltaParentMismatch { expected, actual } => write!(
                formatter,
                "delta parent mismatch: expected {expected}, got {actual}"
            ),
            Self::DeltaChildMismatch { expected, actual } => write!(
                formatter,
                "delta child mismatch: expected {expected}, got {actual}"
            ),
            Self::DeltaConflict => formatter.write_str("delta entry conflicts with current tree"),
            Self::WrongLogicalRole => formatter.write_str("wrong logical object role"),
            Self::InvalidMappingTag { tag } => write!(formatter, "invalid mapping tag {tag:#x}"),
            Self::UnsupportedMappingVersion { version } => {
                write!(formatter, "unsupported mapping version {version}")
            }
            Self::MappingDepthExceeded => formatter.write_str("mapping depth exceeded"),
            Self::MappingCycle => formatter.write_str("mapping cycle detected"),
            Self::ChunkLengthMismatch => formatter.write_str("chunk length mismatch"),
            Self::ChunkIdentityMismatch => formatter.write_str("chunk identity mismatch"),
            Self::AllocationBudgetExceeded => formatter.write_str("allocation budget exceeded"),
            Self::AllocationFailed => formatter.write_str("allocation failed"),
            Self::InvalidValidationReceipt => formatter.write_str("invalid validation receipt"),
            Self::SchemaMigrationRequired => formatter.write_str("schema migration required"),
            Self::ValidationAuthorityUnavailable => {
                formatter.write_str("validation authority unavailable")
            }
            Self::PublicationConflict => formatter.write_str("publication conflict"),
            Self::AmbiguousDurability => formatter.write_str("ambiguous durability"),
        }
    }
}

impl std::error::Error for CoreError {}
