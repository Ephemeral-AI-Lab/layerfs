//! Authenticated bounded duplex channels and Store provisioning metadata.
#![forbid(unsafe_code)]
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
mod daemon_types;
mod daemon_wire;
pub mod initial_record;
