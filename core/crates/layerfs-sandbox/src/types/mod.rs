//! Exact ordinary runtime identities, independent process/output knowledge and failures.
mod execution;
mod failure;
mod identity;
mod transfer;
pub use execution::{CommandIdentity, ExecInspection};
pub use failure::{ErrorDiagnostic, RuntimeError, WireFailure};
pub use identity::{ContainerId, ExecId};
pub use transfer::PendingRequest;
