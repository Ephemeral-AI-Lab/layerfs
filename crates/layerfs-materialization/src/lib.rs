//! Host filesystem observations used by the Phase 0 evaluation harness.
//!
//! This crate exposes typed projection ownership and observations, while
//! platform-specific mechanics stay behind this boundary.

#![deny(unsafe_code)]

mod capture;
pub mod driver;
mod error;
mod host_driver;
mod host_environment;
mod managed_edit;
mod materialize;
mod operation_driver;
mod portable;
mod refresh;
mod topology;

pub(crate) use topology::topology_edge_key;

pub use error::{VfsError, VfsResult};
pub use host_driver::{host_driver, HostDriver};
pub use host_environment::{probe, HostEnvironment, ProbeError};
pub use operation_driver::MaterializationDriver;
pub use portable::{
    NativeOperationCounters, NativeRoute, OperationCounters, RootId, OPERATION_Q_BOUND_BYTES,
};

#[cfg(target_os = "macos")]
pub mod apfs;

pub const COMPONENT: &str = "layerfs-materialization";
