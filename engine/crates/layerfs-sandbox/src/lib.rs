//! Ordinary runtime operations, separate from filesystem admission and data ownership.
// unsafe only in backend/macos/darwin.rs: macOS process facts come from
// proc_pidinfo, a system call (`ps` is setuid, which Seatbelt refuses)
#![deny(unsafe_code)]
#[cfg(unix)]
pub mod backend;
pub mod provider;
mod types;
#[cfg(unix)]
pub use backend::{Runtime, Sandbox};
pub use types::{
    CommandIdentity, ContainerId, ErrorDiagnostic, ExecId, ExecInspection, ExecRequest,
    PendingRequest, RuntimeError, WireFailure,
};
