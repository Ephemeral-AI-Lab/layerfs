//! Responsibility-scoped implementation modules and reexports.
pub(crate) mod connection;
pub(crate) mod profile;
pub(crate) mod startup;
pub(crate) mod statements;

pub(crate) mod physical;
#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
pub(crate) mod windows_identity;

pub(crate) mod accounting;
