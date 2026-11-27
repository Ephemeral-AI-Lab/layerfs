//! Observational Store audits over one caller-owned database snapshot.
pub use check::project::{ProjectCoverage, CHECK as PROJECT_CHECK};
mod reader;
mod scratch;
pub use super::backend::postgresql::server::reach::ConnectionStep;
pub use super::backend::sqlite::diagnostic::structure::sqlite_structure;
pub(crate) mod view;
pub use super::backend::postgresql::diagnostic::run::connection as postgresql_connection;
pub(crate) use super::backend::postgresql::diagnostic::run::with_cancellation;

pub use check::mounts::{DoneAuditPage, DoneAuditRow, MountHolder};
pub(crate) mod check;
mod observation;
pub(crate) use observation::emit;
pub use observation::{Observation, Options, Outcome, Resolver};
