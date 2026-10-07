//! SQLite-only connection, transaction, query, schema and physical mechanics.
pub(crate) mod acquisition;
pub(crate) mod connection;
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod file_control;
pub(crate) mod metadata_allocation;
pub(crate) mod metadata_locations;
pub(crate) mod metadata_policy;
pub(crate) mod metadata_pooling;
pub(crate) mod metadata_signatures;
pub(crate) mod objects_read;
pub(crate) mod objects_selection;
pub(crate) mod prepared;
mod profile;
pub(crate) mod publish;
pub(crate) mod query;
pub(crate) mod reclamation;
pub(crate) mod rows;
pub(crate) mod schema;
pub(crate) mod seal;
pub(crate) mod statement_work;
pub(crate) mod transaction;
pub(crate) mod unit_io;
pub(crate) mod unit_layout;
mod units_publish;
mod units_read;
