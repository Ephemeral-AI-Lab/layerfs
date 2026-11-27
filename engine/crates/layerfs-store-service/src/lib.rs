//! The Store engine: typed reads, file changes and Branch history over the
//! global Store, in the process that calls them.
//!
//! The SDK's fs, branch and project methods and the operator Store calls run
//! on this engine in the SDK's own process, against the Store it opened
//! (`StoreService`); this is their only route (#352 F-020). The engine is
//! organised by mechanism: `filesystem`, `history` and `lookup` know no SDK
//! method names; `families/` is the SDK facade, one folder per call family,
//! one file per public Store method and their method table,
//! [`families::call`]. `store` opens and installs the Store; the daemon uses
//! `store` and `workspace` for its Workspace Commits. This crate has no FUSE,
//! socket or Sandbox. Each Workspace owns one overlay connection behind one
//! lock (`workspace::owner`); a caller runs its own short typed call under it
//! and keeps exact failure custody.
#![forbid(unsafe_code)]

pub mod families;
pub mod filesystem;
pub mod history;
pub mod lookup;
mod service;
pub mod store;
pub mod telemetry;
pub mod workspace;

pub use owner::{OriginalInput, Owner, OwnerClient, OwnerConfig, OwnerError, WorkspacePanic};
pub use service::{OpenRefusal, Retained, StoreService, CALLS};
pub use workspace::owner::class::ServiceClass;
pub use workspace::owner::jobs as owner;
pub use workspace::owner::operations::hold::{MaintenanceHold, MutationHold};
pub use workspace::owner::roster::startup::OwnerStart;
pub use workspace::owner::work::{JobWork, OwnerWork};

#[doc(hidden)]
pub use crate::families as methods;
