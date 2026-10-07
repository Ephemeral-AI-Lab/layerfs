//! Authenticated bounded native channels and Store provisioning metadata.
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#[cfg(feature = "native")]
pub mod native;
pub mod provision;
