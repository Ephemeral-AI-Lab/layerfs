//! What spans the control entities (#536; split by layerfs-project#101):
//! the worker's server, `inventory` (the relationships no record stores),
//! `doctor`, the home's conversions (`migrate`), and the machine, Store,
//! project and backup calls with the reads they need from other entities
//! (`Related`). The call machinery is `layerfs-api`'s (layerfs-project#107).
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod backup;
pub mod doctor;
#[doc(hidden)]
pub mod inventory;
#[cfg(feature = "machine")]
pub mod machine;
#[cfg(feature = "machine")]
mod machine_force;
pub mod migrate;
pub mod project;
pub mod store;
pub mod worker_server;
pub use layerfs_project::store::{
    projects, AddFailure, AddRequest, AddedProject, CreateRequest, InitError, InitFailure,
    InitRequest, InstallError, InstallFailure, InstallWork, Installed, ProjectApi, SealedProject,
};
pub use layerfs_project::{InitCounters, InitCounts, InitStep, ProjectError, SourceRefusal};
pub use layerfs_store_service::methods::call;
