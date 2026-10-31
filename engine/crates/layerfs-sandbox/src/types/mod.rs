//! Exact ordinary runtime identities, independent process/output knowledge and failures.
mod execution;
mod failure;
mod identity;
mod transfer;
pub use execution::{CommandIdentity, ExecInspection};
#[cfg(unix)] // read only by the (Unix) backend; the types themselves stay portable
pub(crate) use failure::STOPPED;
pub use failure::{ErrorDiagnostic, RuntimeError, WireFailure};
pub use identity::{ContainerId, ExecId};
pub use transfer::PendingRequest;
