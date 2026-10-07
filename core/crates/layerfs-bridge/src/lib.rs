//! Authenticated bounded native channels and Store provisioning metadata.
#![forbid(unsafe_code)]
#![deny(missing_docs)]
pub mod control;
#[cfg(feature = "native")]
pub mod native;
pub mod provision;
mod provision_wire;
mod wire;
