//! Linux namespace Sandboxes: each Sandbox is one daemon inside its own
//! mount, PID and IPC namespaces and its own cgroup v2 directory, on the one
//! kernel and one Store disk of the project's VM. Commands enter those
//! namespaces as a nonroot identity with no capabilities and no-new-privs,
//! and `cgroup.kill` ends everything the Sandbox started.
//!
//! The lifecycle is the local one, imported from `crate::local`
//! (namespaces is the local process with namespaces added: its
//! `Local`, `LocalPaths`, `LocalPhase`, `search`, `listener`, `streams` and
//! Linux `mounts`, with `LocalProcess` from `layerfs_bridge::process`): a
//! private directory per Sandbox, one daemon start and one listener wait, a
//! record of the daemon's exact process and its port. What this backend
//! adds is outside that daemon: the helper that builds the private root, the
//! namespaces and the cgroup. Nothing here is retried and no effect is
//! undone implicitly.
//!
//! Its adapter (stored name, capabilities) builds on every Unix host; the
//! backend itself on Linux only.
mod adapter;
#[cfg(target_os = "linux")]
mod cgroup;
#[cfg(target_os = "linux")]
mod create;
#[cfg(target_os = "linux")]
mod exec;
#[cfg(target_os = "linux")]
mod host;
#[cfg(target_os = "linux")]
mod paths;
#[cfg(target_os = "linux")]
mod provider;
#[cfg(target_os = "linux")]
pub mod rootfs;
#[cfg(target_os = "linux")]
mod sandbox;

pub(crate) use adapter::ADAPTER;
#[cfg(target_os = "linux")]
pub(crate) use cgroup::ExecGroup;
#[cfg(target_os = "linux")]
pub use provider::NamespaceProvider;
#[cfg(target_os = "linux")]
pub use rootfs::ImageRoot;
#[cfg(target_os = "linux")]
pub use sandbox::Sandbox;
