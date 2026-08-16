use std::fmt;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CoreError {
    InvalidPath,
    PathLimitExceeded,
    InvalidObjectKind { tag: u8 },
    LengthOverflow,
    ObjectLimitExceeded,
    IdentityMismatch,
    UnexpectedEof,
    TrailingBytes,
    NonCanonicalOrdering,
    Unsupported,
    Io,
    InvalidIdentityLength { expected: usize, actual: usize },
    InvalidIdentityText,
    InvalidUtf8,
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
            Self::IdentityMismatch => {
                formatter.write_str("identity does not match canonical bytes")
            }
            Self::UnexpectedEof => formatter.write_str("unexpected end of input"),
            Self::TrailingBytes => formatter.write_str("trailing bytes"),
            Self::NonCanonicalOrdering => formatter.write_str("non-canonical ordering"),
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
        }
    }
}

impl std::error::Error for CoreError {}
