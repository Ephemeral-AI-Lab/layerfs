//! Production direct-Store daemon startup/control, with no command supervisor.
// The daemon application is the unix daemon (FUSE serving, uid checks,
// config guards); Windows reaches Stores in-process (#352 F-020) and this
// crate contributes its install/types surface only.
#[cfg(unix)]
mod bodies;
#[cfg(unix)]
mod cli;
#[cfg(unix)]
mod config;
#[cfg(unix)]
mod connection;
#[cfg(unix)]
mod diagnostics;
mod failure;
mod filesystem;
#[cfg(unix)]
#[doc(hidden)]
pub mod internals;
#[cfg(any(unix, windows))]
mod owner;
mod retained;
#[cfg(any(unix, windows))]
mod serve;
#[cfg(unix)]
pub use cli::main;
#[cfg(unix)]
pub use diagnostics::resources::{ResourceDiagnosticError, ResourceFailure, ResourceObservation};
#[cfg(unix)]
pub(crate) use diagnostics::{commit_parts, namespace_values, storage_values};
pub use failure::{ApplicationError, ConnectionFailure};
pub use owner::Application;
pub use retained::{RetainedFailure, RetainedSnapshot};
