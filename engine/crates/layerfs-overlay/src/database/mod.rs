//! Responsibility-scoped implementation modules and reexports.
pub(crate) mod connection;
pub(crate) mod profile;
pub(crate) mod startup;
pub(crate) mod statements;

pub(crate) mod physical;
// The audited module's own `#![allow(unsafe_code)]` admits it; a second
// outer attribute is a clippy duplication.
#[cfg(target_os = "windows")]
pub(crate) mod windows_identity;

pub(crate) mod accounting;
