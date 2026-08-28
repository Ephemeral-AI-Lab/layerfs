//! Native driver error boundary.

use std::{fmt, io};

#[derive(Debug)]
pub enum DriverError {
    Unsupported,
    NativeProtected,
    Conflict,
    VisibilityAmbiguous,
    DurabilityAmbiguous,
    Io(io::Error),
}

impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => f.write_str("native operation is unsupported"),
            Self::NativeProtected => f.write_str("native object is protected"),
            Self::Conflict => f.write_str("native object changed"),
            Self::VisibilityAmbiguous => f.write_str("native visibility is ambiguous"),
            Self::DurabilityAmbiguous => f.write_str("native durability is ambiguous"),
            Self::Io(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for DriverError {}

impl From<io::Error> for DriverError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub type Result<T> = std::result::Result<T, DriverError>;
