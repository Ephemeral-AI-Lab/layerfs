//! Namespace construction and Store Init/seal/install composition.
// Windows carries one audited unsafe FFI module (import host_id's
// GetFileInformationByHandle surface); forbid cannot be allowed back on, so
// it stays a hard deny there and the audited module opts in. Unix keeps the
// absolute forbid.
#![cfg_attr(not(windows), forbid(unsafe_code))]
#![cfg_attr(windows, deny(unsafe_code))]
#![deny(missing_docs)]
mod import;
pub mod store;
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
