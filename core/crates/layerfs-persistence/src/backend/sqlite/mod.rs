//! SQLite-only connection, transaction, query, schema and physical mechanics.
#[cfg(target_os = "macos")]
mod allocation;
pub(crate) mod connection;
pub(crate) mod metadata_allocation;
pub(crate) mod metadata_locations;
pub(crate) mod metadata_policy;
pub(crate) mod metadata_pooling;
pub(crate) mod metadata_signatures;
pub(crate) mod objects_read;
mod profile;
pub(crate) mod publish;
pub(crate) mod query;
pub(crate) mod rows;
pub(crate) mod schema;
pub(crate) mod statement_work;
pub(crate) mod transaction;
