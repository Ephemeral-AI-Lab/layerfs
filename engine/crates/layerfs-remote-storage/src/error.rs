//! What a remote request reports. Only `Uncertain` and `PartsLeft` say a
//! sent request's outcome is unknown; nothing is resent for either.
use std::fmt;

/// Why a remote operation did not complete.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RemoteError {
    /// Bad input or configuration; nothing was sent.
    Invalid(String),
    /// The provider could not be reached; no request byte was sent.
    Unavailable(String),
    /// The provider answered with a failure status and error code.
    Refused { status: u16, code: String },
    /// The named file is absent from the remote.
    Missing,
    /// The file is larger than this provider or layout allows.
    TooLarge { bytes: u64, max: u64 },
    /// Downloaded bytes do not match their recorded size or sha256.
    Integrity(String),
    /// A local file operation failed.
    Local(String),
    /// A request was sent and its outcome is unknown. It is never resent.
    Uncertain(String),
    /// A multipart upload failed and its abort failed too: its parts may
    /// remain on the remote under `upload`.
    PartsLeft {
        cause: Box<RemoteError>,
        upload: String,
        abort: Box<RemoteError>,
    },
    /// Not built in this phase (for example TLS endpoints).
    Unsupported(String),
}

impl fmt::Display for RemoteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(why) => write!(f, "invalid remote request: {why}"),
            Self::Unavailable(why) => write!(f, "remote unavailable (nothing sent): {why}"),
            Self::Refused { status, code } => write!(f, "remote refused: HTTP {status} {code}"),
            Self::Missing => f.write_str("remote file missing"),
            Self::TooLarge { bytes, max } => {
                write!(f, "file of {bytes} bytes exceeds the remote's {max}")
            }
            Self::Integrity(why) => write!(f, "remote file integrity: {why}"),
            Self::Local(why) => write!(f, "local file: {why}"),
            Self::Uncertain(why) => write!(f, "remote outcome unknown (not resent): {why}"),
            Self::PartsLeft {
                cause,
                upload,
                abort,
            } => write!(
                f,
                "{cause}; abort of multipart upload {upload} failed ({abort}), its parts may remain"
            ),
            Self::Unsupported(why) => write!(f, "unsupported: {why}"),
        }
    }
}

impl std::error::Error for RemoteError {}

/// A local file error, with what was being done.
pub(crate) fn local(doing: &str, error: std::io::Error) -> RemoteError {
    RemoteError::Local(format!("{doing}: {error}"))
}
