//! Workspace lifecycle errors.

use std::fmt;

#[derive(Debug)]
pub enum WorkspaceError {
    Busy,
    InvalidState,
    OwnershipMismatch,
    ResourceExhausted,
    Timeout,
    Io(std::io::Error),
}

impl fmt::Display for WorkspaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy => formatter.write_str("workspace is busy"),
            Self::InvalidState => formatter.write_str("invalid workspace state"),
            Self::OwnershipMismatch => formatter.write_str("workspace ownership mismatch"),
            Self::ResourceExhausted => formatter.write_str("workspace resource limit exceeded"),
            Self::Timeout => formatter.write_str("workspace quiescence timed out"),
            Self::Io(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for WorkspaceError {}

impl From<std::io::Error> for WorkspaceError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub type Result<T> = std::result::Result<T, WorkspaceError>;
