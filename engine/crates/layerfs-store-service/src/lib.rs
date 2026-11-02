//! The Store engine: typed reads, file changes and Branch history over the
//! global Store, in the process that calls them.
//!
//! The SDK's fs, branch and project methods (layerfs-api) and the operator
//! Store calls run on this engine in the SDK's own process, against the Store
//! it opened (`StoreService`); this is their only route (#352 F-020). The
//! engine is organised by mechanism (`read`, `write`, `history`) and knows no
//! SDK method names. The daemon uses the same `store` and `overlay` for its
//! Workspace Commits. This crate has no FUSE, socket or
//! Sandbox. Each Workspace owns one overlay connection behind one lock; a
//! caller runs its own short typed call under it and keeps exact failure
//! custody.
#![forbid(unsafe_code)]

pub mod bootstrap;
pub mod history;
pub mod install;
pub mod kernel_guard;
pub mod operator;
pub mod overlay;
pub mod read;
pub mod service;
pub mod store;
mod store_service;
pub mod telemetry;
pub mod write;

pub use overlay::owner;
pub use owner::{OriginalInput, Owner, OwnerClient, OwnerConfig, OwnerError, WorkspacePanic};
pub use service::observations::JobWork;
pub use service::owner_work::OwnerWork;
pub use service::results::ServiceClass;
pub use service::startup::OwnerStart;
pub use store_service::{OpenRefusal, Retained, StoreService, CALLS};
