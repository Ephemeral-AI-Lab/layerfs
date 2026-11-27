//! Namespace construction and Store Init/seal/install composition.
#![forbid(unsafe_code)]
#![deny(missing_docs)]
mod import;
pub mod store;
pub use error::{ProjectError, ProjectResult, RetainedAcquisition, SourceRefusal};
#[cfg(unix)]
pub(crate) use import::backing;
#[cfg(unix)]
pub(crate) use import::batch;
pub(crate) use import::error;
#[cfg(unix)]
pub(crate) use import::files;
#[cfg(unix)]
pub(crate) use import::namespace;
pub use import::observe::{InitCounters, InitCounts, InitStep, NamespaceWork};
pub(crate) use import::run as init;
#[cfg(unix)]
pub(crate) use import::scan;
pub use init::{init, init_observed, InitRequest, Initialized};
