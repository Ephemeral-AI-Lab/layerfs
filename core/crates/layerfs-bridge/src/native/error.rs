//! Transport failures retain original crypto/I/O and failed close evidence.
use std::{fmt, io};

/// One native channel failure, without retries or inferred product disposition.
#[derive(Debug)]
pub enum ChannelError {
    /// Original socket failure.
    Io(io::Error),
    /// Original Noise failure.
    Noise(snow::Error),
    /// Peer/header/handshake input was invalid.
    Invalid(&'static str),
    /// A caller record exceeds the processing window; no send was attempted.
    RecordLimit,
    /// The physical Noise nonce space is exhausted.
    NonceExhausted,
    /// Either direction previously failed or explicitly closed this connection.
    Quarantined,
    /// Connection quarantine also failed to interrupt socket I/O.
    CloseFailed {
        /// Original channel operation failure.
        original: Box<ChannelError>,
        /// Exact shutdown failure; shutdown is not retried.
        close: io::Error,
    },
}
impl From<io::Error> for ChannelError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<snow::Error> for ChannelError {
    fn from(error: snow::Error) -> Self {
        Self::Noise(error)
    }
}
impl fmt::Display for ChannelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => error.fmt(f),
            Self::Noise(error) => error.fmt(f),
            Self::Invalid(what) => write!(f, "invalid native channel: {what}"),
            Self::RecordLimit => f.write_str("native record processing window exceeded"),
            Self::NonceExhausted => f.write_str("native Noise nonce exhausted"),
            Self::Quarantined => f.write_str("native channel quarantined"),
            Self::CloseFailed { original, close } => {
                write!(f, "{original}; shutdown also failed: {close}")
            }
        }
    }
}
impl std::error::Error for ChannelError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Noise(e) => Some(e),
            Self::CloseFailed { original, .. } => Some(original),
            _ => None,
        }
    }
}
/// Exact native operation result.
pub type ChannelResult<T> = Result<T, ChannelError>;
