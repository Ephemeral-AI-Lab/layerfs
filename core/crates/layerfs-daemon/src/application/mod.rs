//! Production direct-Store daemon startup/control, with no command supervisor.
mod cli;
mod config;
mod connection;
mod diagnostics;
mod failure;
mod filesystem;
mod owner;
mod serve;
pub use cli::main;
pub use diagnostics::resources::{ResourceDiagnosticError, ResourceFailure, ResourceObservation};
pub use failure::{ApplicationError, ConnectionFailure};
pub use owner::Application;
