//! Typed content-construction and canonical-read failures.
//!
//! Every variant is produced by exactly one check. There is no error-driven
//! alternate algorithm: a failed construction, codec or framing check returns
//! its error to the caller and that operation is over.

use std::fmt;

/// Failures produced by canonical framing, construction and logical reads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContentError {
    /// A configuration value is outside the explicitly implemented profile.
    UnsupportedPolicy {
        /// The rejected policy field.
        field: &'static str,
    },
    /// A checked length or offset computation overflowed.
    LengthOverflow,
    /// The input ended before the declared structure did.
    UnexpectedEof,
    /// Bytes remained after the declared structure.
    TrailingBytes,
    /// The canonical envelope magic or kind is not the supported one.
    UnsupportedFraming,
    /// The mapping grammar version is not the supported one.
    UnsupportedMappingVersion {
        /// The version found in the value.
        version: u16,
    },
    /// A mapping node carried an unknown role tag.
    InvalidMappingTag {
        /// The tag found in the value.
        tag: u8,
    },
    /// The canonical value belongs to another logical role.
    WrongLogicalRole,
    /// A structural invariant failed; the label names the check.
    InvalidRecord(&'static str),
    /// A declared bound would be exceeded.
    ObjectLimitExceeded {
        /// The bound that would be exceeded, in bytes.
        limit: usize,
        /// The size that was requested or found.
        actual: usize,
    },
    /// The bytes do not hash to the ID they were supplied under.
    IdentityMismatch,
    /// The provider does not hold the requested object.
    MissingObject,
    /// A raw identity had the wrong width.
    InvalidIdentityLength {
        /// Required width in bytes.
        expected: usize,
        /// Supplied width in bytes.
        actual: usize,
    },
    /// Identity text was not lowercase/uppercase hexadecimal of the right width.
    InvalidIdentityText,
    /// A logical range is outside the file or inverted.
    InvalidRange {
        /// Requested start offset.
        start: u64,
        /// Requested exclusive end offset.
        end: u64,
        /// Logical length of the file.
        length: u64,
    },
    /// A node page was outside the canonical entry partition.
    NonCanonicalPagePartition,
    /// Branch summaries were not strictly increasing.
    NonCanonicalOrdering,
    /// A node's recorded byte total disagreed with its entries.
    LengthMismatch {
        /// Recorded total.
        expected: u64,
        /// Total computed from the entries.
        actual: u64,
    },
    /// The mapping tree is deeper than the frozen grammar allows.
    MappingDepthExceeded,
    /// A read or write through the supplied sink failed.
    Io,
    /// The supplied bounded consumer refused the object.
    OutputRejected,
    /// A declared bounded capacity was exceeded.
    BoundedCapacityExceeded {
        /// The bounded resource, such as `construction.whole_file`.
        what: &'static str,
        /// The declared limit.
        limit: u64,
        /// The requested or observed size.
        actual: u64,
    },
    /// A declared edit is not applicable to the base it addresses.
    InvalidEdit {
        /// The label names the failed check.
        what: &'static str,
    },
    /// The provider returned a batch of the wrong cardinality.
    BatchCardinality {
        /// Number of identifiers requested.
        requested: usize,
        /// Number of values returned.
        returned: usize,
    },
    /// A logical path or name is not a canonical one.
    InvalidPath,
    /// A logical path or name exceeds a declared byte or component bound.
    PathLimitExceeded,
    /// A logical name or path is not valid UTF-8.
    InvalidUtf8,
    /// The operation tried to mutate the filesystem root itself.
    RootMutation,
    /// The stored profile is not one this implementation is required to support.
    UnsupportedProfile {
        /// The profile field or grammar that was rejected.
        what: &'static str,
    },
    /// The supplied identity is outside the operation's allocation scope.
    ScopeMismatch {
        /// The scope field that disagreed.
        what: &'static str,
    },
    /// The operation's declared ordering or scratch resource is unavailable.
    ResourceUnavailable {
        /// The resource that could not be provided.
        what: &'static str,
    },
    /// A caller-supplied ordering record, run or cursor was malformed.
    InvalidOrderingRecord(&'static str),
    /// The operation was cancelled or its final completion did not run.
    IncompleteOperation,
}

impl fmt::Display for ContentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPolicy { field } => {
                write!(formatter, "unsupported construction policy field: {field}")
            }
            Self::LengthOverflow => formatter.write_str("length overflow"),
            Self::UnexpectedEof => formatter.write_str("unexpected end of input"),
            Self::TrailingBytes => formatter.write_str("trailing bytes"),
            Self::UnsupportedFraming => formatter.write_str("unsupported canonical framing"),
            Self::UnsupportedMappingVersion { version } => {
                write!(formatter, "unsupported mapping version {version}")
            }
            Self::InvalidMappingTag { tag } => write!(formatter, "invalid mapping tag {tag}"),
            Self::WrongLogicalRole => formatter.write_str("wrong logical role"),
            Self::InvalidRecord(what) => write!(formatter, "invalid record: {what}"),
            Self::ObjectLimitExceeded { limit, actual } => {
                write!(formatter, "object limit {limit} exceeded by {actual}")
            }
            Self::IdentityMismatch => formatter.write_str("object identity mismatch"),
            Self::MissingObject => formatter.write_str("object not available"),
            Self::InvalidIdentityLength { expected, actual } => {
                write!(formatter, "identity width {actual}, expected {expected}")
            }
            Self::InvalidIdentityText => formatter.write_str("invalid identity text"),
            Self::InvalidRange { start, end, length } => {
                write!(formatter, "range {start}..{end} outside length {length}")
            }
            Self::NonCanonicalPagePartition => formatter.write_str("non-canonical page partition"),
            Self::NonCanonicalOrdering => formatter.write_str("non-canonical ordering"),
            Self::LengthMismatch { expected, actual } => {
                write!(
                    formatter,
                    "length mismatch: recorded {expected}, actual {actual}"
                )
            }
            Self::MappingDepthExceeded => formatter.write_str("mapping depth exceeded"),
            Self::Io => formatter.write_str("content I/O failure"),
            Self::OutputRejected => formatter.write_str("finalized output rejected"),
            Self::BoundedCapacityExceeded {
                what,
                limit,
                actual,
            } => {
                write!(
                    formatter,
                    "bounded capacity {what} limit {limit} exceeded by {actual}"
                )
            }
            Self::InvalidEdit { what } => write!(formatter, "invalid edit: {what}"),
            Self::BatchCardinality {
                requested,
                returned,
            } => {
                write!(
                    formatter,
                    "batch returned {returned} of {requested} objects"
                )
            }
            Self::InvalidPath => formatter.write_str("invalid canonical path"),
            Self::PathLimitExceeded => formatter.write_str("path or name limit exceeded"),
            Self::InvalidUtf8 => formatter.write_str("invalid UTF-8"),
            Self::RootMutation => formatter.write_str("root mutation"),
            Self::UnsupportedProfile { what } => {
                write!(formatter, "unsupported profile: {what}")
            }
            Self::ScopeMismatch { what } => write!(formatter, "scope mismatch: {what}"),
            Self::ResourceUnavailable { what } => {
                write!(
                    formatter,
                    "ordering or scratch resource unavailable: {what}"
                )
            }
            Self::InvalidOrderingRecord(what) => {
                write!(formatter, "invalid ordering record: {what}")
            }
            Self::IncompleteOperation => formatter.write_str("incomplete operation"),
        }
    }
}

impl std::error::Error for ContentError {}

/// Result alias for this component.
pub type ContentResult<T> = Result<T, ContentError>;
