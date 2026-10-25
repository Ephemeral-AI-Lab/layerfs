//! Namespace Init over the content, storage and history contracts.
#![deny(unsafe_code)]
#![deny(missing_docs)]
mod import;
pub use error::{ProjectError, ProjectResult, RetainedAcquisition, SourceRefusal};
pub(crate) use import::backing;
pub(crate) use import::batch;
pub(crate) use import::error;
pub(crate) use import::files;
pub(crate) use import::init;
pub(crate) use import::namespace;
pub use import::observe::{InitCounters, InitCounts, InitStep};
pub(crate) use import::scan;
pub use init::{init, init_observed, InitRequest, Initialized};

pub(crate) use import::work as namespace_work;
pub use namespace_work::NamespaceWork;
