//! SQLite-only connection, transaction, query, schema and physical mechanics.
pub(crate) mod acquisition;
#[cfg(target_os = "macos")]
mod allocation;
#[cfg(target_os = "macos")]
mod allocation_owner;
pub(crate) mod connection;
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
pub(crate) mod rows;
pub(crate) mod schema;
#[cfg(target_os = "macos")]
mod segment_io;
pub(crate) mod segment_layout;
#[cfg(target_os = "macos")]
mod segment_owner;
mod segment_publish;
pub(crate) mod segment_read;
pub(crate) mod statement_work;
pub(crate) mod transaction;
pub(crate) mod unit_io;
pub(crate) mod unit_layout;
mod units_publish;
mod units_read;
