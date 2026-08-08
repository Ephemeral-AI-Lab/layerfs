//! Portable failure taxonomy for canonical M6.1 values.

use core::fmt;

/// Stable external outcomes: the immutable M6.0 base followed by M6.1's
/// sealed extension.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum OutcomeCode {
    Schema,
    Truncated,
    ExactEof,
    LogicalLength,
    TypedEdge,
    FileMode,
    Target,
    RootSentinel,
    ChildMode,
    Name,
    UnknownKind,
    OrderDuplicate,
    TypeDomain,
    OccupiedSameIdDifferentBytes,
    IdMismatch,
    Flags,
    Reserved,
    Path,
    IntegerOverflow,
    ChunkCap,
    CountCap,
    ChunkLength,
    PhysicalObjectCap,
    DigestUnavailable,
    DigestFailure,
    DigestWidth,
    DigestProtocol,
    SourceFailure,
    SinkRefused,
    ResourceRefused,
    Cancelled,
    Deadline,
}

impl OutcomeCode {
    /// Return the frozen machine-readable vector spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Schema => "S_SCHEMA",
            Self::Truncated => "S_TRUNCATED",
            Self::ExactEof => "S_EXACT_EOF",
            Self::LogicalLength => "S_LOGICAL_LENGTH",
            Self::TypedEdge => "S_TYPED_EDGE",
            Self::FileMode => "S_FILE_MODE",
            Self::Target => "S_TARGET",
            Self::RootSentinel => "S_ROOT_SENTINEL",
            Self::ChildMode => "S_CHILD_MODE",
            Self::Name => "S_NAME",
            Self::UnknownKind => "S_UNKNOWN_KIND",
            Self::OrderDuplicate => "S_ORDER_DUPLICATE",
            Self::TypeDomain => "S_TYPE_DOMAIN",
            Self::OccupiedSameIdDifferentBytes => "S_OCCUPIED_SAME_ID_DIFFERENT_BYTES",
            Self::IdMismatch => "S_ID_MISMATCH",
            Self::Flags => "S_FLAGS",
            Self::Reserved => "S_RESERVED",
            Self::Path => "S_PATH",
            Self::IntegerOverflow => "S_INTEGER_OVERFLOW",
            Self::ChunkCap => "S_CHUNK_CAP",
            Self::CountCap => "S_COUNT_CAP",
            Self::ChunkLength => "S_CHUNK_LENGTH",
            Self::PhysicalObjectCap => "S_PHYSICAL_OBJECT_CAP",
            Self::DigestUnavailable => "S_DIGEST_UNAVAILABLE",
            Self::DigestFailure => "S_DIGEST_FAILURE",
            Self::DigestWidth => "S_DIGEST_WIDTH",
            Self::DigestProtocol => "S_DIGEST_PROTOCOL",
            Self::SourceFailure => "S_SOURCE_FAILURE",
            Self::SinkRefused => "S_SINK_REFUSED",
            Self::ResourceRefused => "S_RESOURCE_REFUSED",
            Self::Cancelled => "S_CANCELLED",
            Self::Deadline => "S_DEADLINE",
        }
    }
}

/// Internal, typed structural and port failures.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CoreError {
    Schema,
    Truncated,
    TrailingBytes,
    LogicalLength,
    TypedEdge,
    FileMode,
    Target,
    RootSentinel,
    ChildMode,
    Name,
    UnknownKind,
    NonCanonicalOrder,
    TypeDomain,
    OccupiedSameIdDifferentBytes,
    IdMismatch,
    Flags,
    Reserved,
    Path,
    IntegerOverflow,
    ChunkCap,
    CountCap,
    ChunkLength,
    PhysicalObjectCap,
    DigestUnavailable,
    DigestFailure,
    DigestWidth,
    DigestProtocol,
    SourceFailure,
    SinkRefused,
    ResourceRefused,
    Cancelled,
    Deadline,
}

impl CoreError {
    /// Exhaustively map internal failures to their sealed external family.
    pub const fn outcome_code(self) -> OutcomeCode {
        match self {
            Self::Schema => OutcomeCode::Schema,
            Self::Truncated => OutcomeCode::Truncated,
            Self::TrailingBytes => OutcomeCode::ExactEof,
            Self::LogicalLength => OutcomeCode::LogicalLength,
            Self::TypedEdge => OutcomeCode::TypedEdge,
            Self::FileMode => OutcomeCode::FileMode,
            Self::Target => OutcomeCode::Target,
            Self::RootSentinel => OutcomeCode::RootSentinel,
            Self::ChildMode => OutcomeCode::ChildMode,
            Self::Name => OutcomeCode::Name,
            Self::UnknownKind => OutcomeCode::UnknownKind,
            Self::NonCanonicalOrder => OutcomeCode::OrderDuplicate,
            Self::TypeDomain => OutcomeCode::TypeDomain,
            Self::OccupiedSameIdDifferentBytes => OutcomeCode::OccupiedSameIdDifferentBytes,
            Self::IdMismatch => OutcomeCode::IdMismatch,
            Self::Flags => OutcomeCode::Flags,
            Self::Reserved => OutcomeCode::Reserved,
            Self::Path => OutcomeCode::Path,
            Self::IntegerOverflow => OutcomeCode::IntegerOverflow,
            Self::ChunkCap => OutcomeCode::ChunkCap,
            Self::CountCap => OutcomeCode::CountCap,
            Self::ChunkLength => OutcomeCode::ChunkLength,
            Self::PhysicalObjectCap => OutcomeCode::PhysicalObjectCap,
            Self::DigestUnavailable => OutcomeCode::DigestUnavailable,
            Self::DigestFailure => OutcomeCode::DigestFailure,
            Self::DigestWidth => OutcomeCode::DigestWidth,
            Self::DigestProtocol => OutcomeCode::DigestProtocol,
            Self::SourceFailure => OutcomeCode::SourceFailure,
            Self::SinkRefused => OutcomeCode::SinkRefused,
            Self::ResourceRefused => OutcomeCode::ResourceRefused,
            Self::Cancelled => OutcomeCode::Cancelled,
            Self::Deadline => OutcomeCode::Deadline,
        }
    }
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.outcome_code().as_str())
    }
}

impl std::error::Error for CoreError {}

pub type CoreResult<T> = Result<T, CoreError>;
