//! The plugin's error carrier: stable LayerFS code, message and structured
//! data. Tier 0 is Rust-only, so this is a plain Rust error; the
//! cross-language `#[rutis::service]` mapping is out of scope.

use layerfs_bridge::control::SdkRefusal;
use serde_json::Value;
use std::fmt;

/// One failed LayerFS call.
#[derive(Debug)]
pub enum LayerFsError {
    /// A LayerFS refusal, with its stable code.
    Refused {
        code: String,
        message: String,
        data: Value,
    },
    /// Invalid configuration or arguments.
    Invalid(String),
    /// A platform or filesystem refusal.
    Unsupported(String),
    /// Local I/O.
    Io(std::io::Error),
    /// A discovery record or manifest could not be decoded.
    Control(String),
    /// A blocking worker did not finish.
    Join(String),
}

impl LayerFsError {
    /// A refusal with an already known code.
    pub fn refused(code: impl Into<String>, message: impl Into<String>, data: Value) -> Self {
        Self::Refused {
            code: code.into(),
            message: message.into(),
            data,
        }
    }

    /// Invalid input before any effect.
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }

    /// A platform or filesystem refusal.
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::Unsupported(message.into())
    }

    /// The stable code a caller handles.
    pub fn code(&self) -> &str {
        match self {
            Self::Refused { code, .. } => code,
            Self::Invalid(_) => "InvalidInput",
            Self::Unsupported(_) => "Unsupported",
            Self::Io(_) | Self::Control(_) | Self::Join(_) => "Failed",
        }
    }
}

impl From<SdkRefusal> for LayerFsError {
    fn from(refusal: SdkRefusal) -> Self {
        let data = crate::value::to_json(&refusal.detail);
        Self::Refused {
            code: refusal.code,
            message: refusal.message,
            data,
        }
    }
}

impl From<std::io::Error> for LayerFsError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl fmt::Display for LayerFsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused { code, message, .. } => write!(f, "{code}: {message}"),
            Self::Invalid(message) => write!(f, "InvalidInput: {message}"),
            Self::Unsupported(message) => write!(f, "Unsupported: {message}"),
            Self::Io(error) => write!(f, "Io: {error}"),
            Self::Control(message) => write!(f, "Control: {message}"),
            Self::Join(message) => write!(f, "Failed: {message}"),
        }
    }
}

impl std::error::Error for LayerFsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}
