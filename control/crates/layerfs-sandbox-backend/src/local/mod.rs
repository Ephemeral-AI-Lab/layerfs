//! Shared local-process lifecycle: starting and stopping a daemon process on
//! this kernel, its listener, search path, streams, process groups and the
//! platform facts (`os`): `macos` supplies the `mount` table and
//! `umount -f`; `linux` supplies the `mountinfo` table and a lazy
//! `MNT_DETACH`. Process identity (`LocalProcess`) is
//! `layerfs_bridge::process` (layerfs-project#112). The host_process and
//! namespaces backends build on it. `confined` is what an inspection read
//! uses of the filesystem (layerfs-project#178).
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod confined;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod create;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) mod groups;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) mod listener;
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "os/mod.rs"]
pub(crate) mod os_impl;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) mod process;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) mod search;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) mod streams;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use create::{Local, LocalCreateFailure, LocalPaths, LocalPhase};
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::{Local, LocalPaths};
#[cfg(target_os = "linux")]
pub(crate) use os_impl::linux as os;
#[cfg(target_os = "macos")]
pub(crate) use os_impl::macos as os;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use process::LocalProcess;
#[cfg(target_os = "windows")]
pub use layerfs_bridge::process::LocalProcess;

/// The mount points at or under `directory` in this process mount table,
/// deepest first (`mountinfo` on Linux, `mount` listing on macOS). Nothing
/// under `directory` is opened or `stat`ed, so a dead mount cannot hold
/// the caller: what a removal asks before it deletes a Sandbox folder
/// (layerfs-project#180). Unix only: the Windows in-process mount
/// answers from its own registry.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn mounts_under(directory: &std::path::Path) -> std::io::Result<Vec<std::path::PathBuf>> {
    os::mounts::under(directory)
}
