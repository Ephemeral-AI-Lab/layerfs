//! Authenticated bounded duplex channels and Store provisioning metadata,
//! and the local process identity the home and the drivers share (`process`).
// unsafe only in process/darwin.rs (macOS process facts come from
// proc_pidinfo, a system call: `ps` is setuid, which Seatbelt refuses)
#![deny(unsafe_code)]
#![deny(missing_docs)]
pub mod control;
#[cfg(feature = "duplex")]
pub mod duplex;
pub mod provision;
mod provision_wire;
mod wire;

mod control_forced;
mod control_history;
mod control_merge;
mod control_mount;
mod control_reply;
mod control_request;
mod control_sdk;
mod control_types;
mod control_unmount;
mod sdk_merge_values;
pub mod sdk_values;

pub mod daemon_setup;

pub use store::{DaemonStore, StoreAuthority};
pub use support::{KernelRoute, StoreKind};
mod daemon_types;
mod daemon_wire;
pub mod initial_record;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod process;
pub mod store;
pub mod support;
