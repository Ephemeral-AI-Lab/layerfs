//! Namespace Init over the content, storage and history contracts.
#![forbid(unsafe_code)]
#![deny(missing_docs)]
mod import;
pub use error::{ProjectError, ProjectResult};
#[cfg(unix)]
pub(crate) use import::batch;
pub(crate) use import::error;
pub(crate) use import::init;
#[cfg(unix)]
pub(crate) use import::metadata;
#[cfg(unix)]
pub(crate) use import::namespace;
#[cfg(unix)]
pub(crate) use import::scan;
pub use init::{init, InitRequest, Initialized};

pub(crate) use import::work as namespace_work;
pub use namespace_work::NamespaceWork;
