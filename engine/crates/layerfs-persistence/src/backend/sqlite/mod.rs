//! SQLite-only connection, transaction, query, schema and physical mechanics.
pub(crate) mod acquisition;
mod admission;
pub(crate) mod connection;
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod file_control;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) mod headroom;
#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
mod windows_lane;
#[cfg(target_os = "windows")]
pub use windows_lane::windows_volume_filesystem;
pub(crate) mod lane;
pub(crate) mod metadata_allocation;
pub(crate) mod metadata_locations;
pub(crate) mod metadata_policy;
pub(crate) mod metadata_signatures;
pub(crate) mod metadata_values;
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
#[cfg(target_os = "macos")]
mod seal_allocation;
pub(crate) mod statement_work;
pub(crate) mod thread_work;
pub(crate) mod transaction;
pub(crate) mod unit_io;
pub(crate) mod unit_layout;
mod units_publish;
mod units_read;
pub(crate) mod writer_admission;
