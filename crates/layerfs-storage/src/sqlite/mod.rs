//! SQLite connection, profile, and admission mechanics.

pub mod admission;
pub mod connection;
pub mod profile;

pub(crate) use admission::{preflight_schema, schema_shape, SchemaState};
#[cfg(test)]
pub(crate) use connection::initial_verified_scrub;
pub(crate) use connection::{
    add_retained_scrub_counters, add_verification_progress_counters, clear_known_trusted_history,
    inspect_store_id_readonly, trusted_history,
};
pub(crate) use connection::{CommitDispatch, ConnectionGuard};
pub use profile::SqliteProfile;
pub(crate) use profile::{configure_profile_counted, BUSY_TIMEOUT};
