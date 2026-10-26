//! Ordinary runtime operations, separate from filesystem admission and data ownership.
#![forbid(unsafe_code)]
#[cfg(unix)]
pub mod backend;
mod types;
pub use types::{
    CommandIdentity, ContainerId, ErrorDiagnostic, ExecId, ExecInspection, PendingRequest,
    RuntimeError, WireFailure,
};
