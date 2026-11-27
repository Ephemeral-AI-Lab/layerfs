//! The tables machines and Sandboxes read: how a Sandbox runs on a machine
//! (`backend`), PostgreSQL URL hosts and the retired platforms (every
//! build), and the platform profiles, options, reach rules and support table
//! (host builds only).
#[cfg(unix)]
pub mod backend;
#[cfg(all(feature = "machine", unix))]
pub(crate) mod options;
#[cfg(unix)]
pub mod postgresql_url;
#[cfg(all(feature = "machine", unix))]
pub(crate) mod profile;
#[cfg(all(feature = "machine", unix))]
pub(crate) mod reach;
pub mod removed;
#[cfg(all(feature = "machine", unix))]
pub(crate) mod support;
