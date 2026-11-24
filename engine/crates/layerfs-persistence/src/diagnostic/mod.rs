//! Observational Store audits over one caller-owned database snapshot.
mod acquisition;
mod auxiliary;
mod catalog;
mod facts;
mod history;
mod lineage;
mod postgresql;
pub(crate) mod postgresql_contract;
mod postgresql_contract_expected;
mod project;
pub use project::{ProjectCoverage, CHECK as PROJECT_CHECK};
mod reader;
mod scan;
mod scratch;
mod sqlite;
mod sqlite_structure;
pub use sqlite_structure::sqlite_structure;
mod transaction;
pub use postgresql::connection as postgresql_connection;
pub(crate) use postgresql::with_cancellation;

mod mounts;
pub use mounts::{DoneAuditPage, DoneAuditRow, MountHolder};
mod observation;
pub(crate) use observation::emit;
pub use observation::{Observation, Options, Outcome, Resolver};
