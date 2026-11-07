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
#[cfg(unix)]
mod failure;
#[cfg(unix)]
mod filesystem;
#[cfg(unix)]
mod owner;
#[cfg(unix)]
mod serve;
#[cfg(unix)]
pub use cli::main;
#[cfg(unix)]
pub use diagnostics::resources::{ResourceDiagnosticError, ResourceFailure, ResourceObservation};
#[cfg(unix)]
pub(crate) use diagnostics::{commit_parts, namespace_values, storage_values};
#[cfg(unix)]
pub use failure::{ApplicationError, ConnectionFailure};
#[cfg(unix)]
pub use owner::Application;
