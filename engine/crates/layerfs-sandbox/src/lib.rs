//! Ordinary runtime operations, separate from filesystem admission and data ownership.
// unsafe only in backend/local/darwin.rs: macOS process facts come from
// proc_pidinfo, a system call (`ps` is setuid, which Seatbelt refuses)
#![deny(unsafe_code)]
#[cfg(unix)]
pub mod backend;
mod types;
pub use types::{
    CommandIdentity, ContainerId, ErrorDiagnostic, ExecId, ExecInspection, PendingRequest,
    RuntimeError, WireFailure,
};
