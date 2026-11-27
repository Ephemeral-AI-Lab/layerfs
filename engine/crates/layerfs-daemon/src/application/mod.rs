//! Production direct-Store daemon startup/control, with no command supervisor.
pub(crate) mod connection;
mod failure;
#[doc(hidden)]
pub mod internals;
pub(crate) mod observed;
pub(crate) mod startup;
pub use connection::retained::{RetainedFailure, RetainedSnapshot};
pub use failure::{ApplicationError, ConnectionFailure};
pub(crate) use observed::records::{commit_parts, namespace_values, storage_values};
pub use observed::resources::{ResourceDiagnosticError, ResourceFailure, ResourceObservation};
pub use startup::cli::main;
pub use startup::lifecycle::Application;
