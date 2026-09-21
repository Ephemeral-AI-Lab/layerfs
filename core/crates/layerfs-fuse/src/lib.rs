//! Linux read and existing-file write projection; Workspace owns filesystem semantics.
#![forbid(unsafe_code)]

#[cfg(target_os = "linux")]
mod adapter;
mod mount;
#[cfg(target_os = "linux")]
mod replies;

pub use mount::{mount, mount_writable, MountError, MountHandle};
