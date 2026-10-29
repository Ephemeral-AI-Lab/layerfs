//! Responsibility-scoped implementation modules and reexports.
pub(crate) mod config;
pub(crate) mod handles;
pub(crate) mod open;
#[cfg(feature = "postgresql")]
pub(crate) mod postgresql;
pub(crate) mod seal;
