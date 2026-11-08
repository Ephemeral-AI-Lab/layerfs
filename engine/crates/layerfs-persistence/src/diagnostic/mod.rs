//! Observational Store audits over one caller-owned database snapshot.
mod auxiliary;
mod history;
mod lineage;
#[cfg(feature = "postgresql")]
mod postgresql;
mod reader;
mod scan;
mod scratch;
mod sqlite;
mod transaction;
#[cfg(feature = "postgresql")]
pub use postgresql::connection as postgresql_connection;

mod observation;
pub(crate) use observation::emit;
pub use observation::{Observation, Options, Outcome, Resolver};
