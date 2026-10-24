//! Namespace Init over the content, storage and history contracts.
#![forbid(unsafe_code)]
#![deny(missing_docs)]
mod import;
pub use error::{ProjectError, ProjectResult, RetainedAcquisition, SourceRefusal};
#[cfg(unix)]
pub(crate) use import::backing;
#[cfg(unix)]
pub(crate) use import::batch;
pub(crate) use import::error;
#[cfg(unix)]
pub(crate) use import::files;
pub(crate) use import::init;
#[cfg(unix)]
pub(crate) use import::namespace;
pub use import::observe::{InitCounters, InitCounts, InitStep};
#[cfg(unix)]
pub(crate) use import::scan;
pub use init::{init, init_observed, InitRequest, Initialized};

pub(crate) use import::work as namespace_work;
pub use namespace_work::NamespaceWork;
