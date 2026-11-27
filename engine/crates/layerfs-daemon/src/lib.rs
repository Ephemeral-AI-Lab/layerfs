//! The daemon: Workspaces (FUSE, kernel Attach/Ready, Commit, unmount) over
//! the Store service crate's Store and overlay. Store calls are not served
//! here; they run in the SDK's process (#352 F-020).
//!
//! Each Workspace owns one overlay connection behind one lock; a caller runs
//! its own short typed call under it and keeps exact failure custody and reply tickets.
//! Direct Store ports provide immutable data and conditional history transitions.
//! Control composes kernel Attach/Ready/unmount over the Fuse session owner;
//! command launch and supervision stay with the Sandbox or external executor.
#![forbid(unsafe_code)]

mod operator;
mod ports;
pub use ports::{request as mount_request, Mounted};
pub mod telemetry;
pub use layerfs_store_service::store::{self, install as shared_install, open as bootstrap};

pub use layerfs_store_service::{
    JobWork, OriginalInput, Owner, OwnerClient, OwnerConfig, OwnerError, OwnerStart, OwnerWork,
    ServiceClass, WorkspacePanic,
};

pub mod workspace;

pub mod application;
pub mod install;
