//! Linux filesystem projection; Workspace owns filesystem semantics.
#![forbid(unsafe_code)]

#[cfg(target_os = "linux")]
mod adapter;
mod mount;
#[cfg(target_os = "linux")]
mod replies;

pub use mount::{mount, mount_writable, MountError, MountFailure, MountHandle, MountPhase};
