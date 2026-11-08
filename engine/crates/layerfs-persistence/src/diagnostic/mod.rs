//! Observational Store audits over one caller-owned database snapshot.
mod acquisition;
mod auxiliary;
mod catalog;
mod facts;
mod history;
mod lineage;
#[cfg(feature = "postgresql")]
mod postgresql;
#[cfg(feature = "postgresql")]
mod postgresql_contract;
#[cfg(feature = "postgresql")]
mod postgresql_contract_expected;
mod reader;
mod scan;
mod scratch;
mod sqlite;
mod sqlite_structure;
pub use sqlite_structure::sqlite_structure;
mod transaction;
#[cfg(feature = "postgresql")]
pub use postgresql::connection as postgresql_connection;
#[cfg(feature = "postgresql")]
pub(crate) use postgresql::with_cancellation;

mod observation;
pub(crate) use observation::emit;
pub use observation::{Observation, Options, Outcome, Resolver};
