//! Linux namespace Sandboxes: each Sandbox is one daemon inside its own
//! mount, PID and IPC namespaces and its own cgroup v2 directory, on the one
//! kernel and one Store disk of the project's VM. Commands enter those
//! namespaces as a nonroot identity with no capabilities and no-new-privs,
//! and `cgroup.kill` ends everything the Sandbox started.
//!
//! The lifecycle is the local one, imported from `crate::shared`
//! (namespaces is the local process with namespaces added: its
//! `Local`, `LocalPaths`, `LocalPhase`, `search_path`, `listener`, `streams`,
//! and its own Linux `mounts` in `linux/`, with `LocalProcess` from `layerfs_os::process`): a
//! private directory per Sandbox, one daemon start and one listener wait, a
//! record of the daemon's exact process and its port. What this backend
//! adds is outside that daemon: the helper that builds the private root, the
//! namespaces and the cgroup. Nothing here is retried and no effect is
//! undone implicitly.
//!
//! Its adapter (stored name, capabilities) builds on every Unix host; the
//! backend itself on Linux only.
mod adapter;
pub(crate) mod instance;
pub mod linux;
pub mod network;
#[cfg(target_os = "linux")]
mod provider;
pub mod rootfs;
#[cfg(target_os = "linux")]
pub(crate) mod seccomp;

pub(crate) use adapter::ADAPTER;

#[cfg(target_os = "linux")]
pub(crate) use instance::cgroup::ExecGroup;
#[cfg(target_os = "linux")]
pub use instance::cgroup::{give_controllers, leaf, split};
#[cfg(target_os = "linux")]
pub use instance::exec::NO_HELPER_COPY;
#[cfg(target_os = "linux")]
pub use instance::handle::Sandbox;
pub use instance::spec::{HelperSpec, SpecNetwork, RESOLVER_FILES, SANDBOX_MOUNTS};
#[cfg(target_os = "linux")]
pub use provider::NamespaceProvider;
#[cfg(target_os = "linux")]
pub use rootfs::ImageRoot;
#[cfg(target_os = "linux")]
pub use seccomp::{deny_user_namespaces, OneThread};
