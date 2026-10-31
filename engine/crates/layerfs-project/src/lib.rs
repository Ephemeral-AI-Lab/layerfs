//! Namespace construction and Store Init/seal/install composition.
// deny, not upstream's forbid: the audited Windows identity module (owner
// direction 2026-10-25) lives in this crate (core/AGENTS.md).
#![deny(unsafe_code)]
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
