//! Namespace Init over the content, storage and history contracts.
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#[cfg(unix)]
mod batch;
mod error;
mod init;
#[cfg(unix)]
mod metadata;
#[cfg(unix)]
mod namespace;
#[cfg(unix)]
mod scan;
pub use error::{ProjectError, ProjectResult};
pub use init::{init, InitRequest, Initialized};
