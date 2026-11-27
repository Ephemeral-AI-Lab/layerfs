//! The process lifecycle both backends use: starting and stopping a daemon
//! process on this kernel, its listener, search path and streams. The
//! `process` and `namespaces` backends build on it. Process groups and the
//! operating-system facts live in the one backend that runs on each OS;
//! `os` re-exports them for this module: `backend::process::macos` (the
//! `mount` table, `umount -f`) or `backend::namespaces::linux` (the
//! `mountinfo` table, a lazy `MNT_DETACH`). Process identity
//! (`LocalProcess`) is `layerfs_bridge::process` (layerfs-project#112).
//! `confined` is what an inspection read uses of the filesystem
//! (layerfs-project#178).
mod daemon;
pub(crate) mod listener;
pub(crate) mod search_path;
pub(crate) mod streams;
#[cfg(target_os = "linux")]
pub(crate) use crate::backend::namespaces::linux as os;
#[cfg(target_os = "macos")]
pub(crate) use crate::backend::process::macos as os;
#[cfg(target_os = "macos")]
pub use daemon::LocalCreateFailure;
#[cfg(target_os = "macos")]
pub(crate) use daemon::LOG;
pub use daemon::{Local, LocalPaths, LocalPhase};

/// The mount points at or under `directory` in this process's mount table,
/// deepest first (`mountinfo` on Linux, `mount`'s listing on macOS). Nothing
/// under `directory` is opened or `stat`ed, so a dead mount cannot hold the
/// caller: what a removal asks before it deletes a Sandbox's folder
/// (layerfs-project#180).
pub fn mounts_under(directory: &std::path::Path) -> std::io::Result<Vec<std::path::PathBuf>> {
    os::mounts::under(directory)
}
