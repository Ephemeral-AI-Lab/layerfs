//! Linux namespace Sandboxes: each Sandbox is one daemon inside its own
//! mount, PID and IPC namespaces and its own cgroup v2 directory, on the one
//! kernel and one Store disk of the project's VM. Commands enter those
//! namespaces as a nonroot identity with no capabilities and no-new-privs,
//! and `cgroup.kill` ends everything the Sandbox started.
//!
//! The lifecycle is the local one (`crate::backend::native`): a private
//! directory per Sandbox, one daemon start and one listener wait, a record of
//! the daemon's exact process and its port. What this backend adds is outside
//! that daemon: the helper that builds the private root, the namespaces and
//! the cgroup. Nothing here is retried and no effect is undone implicitly.
mod cgroup;
mod create;
mod exec;
mod host;
mod paths;
mod provider;
mod sandbox;

pub use provider::NamespaceProvider;
pub use sandbox::Sandbox;
