//! Every Sandbox backend, one folder each, named as `LAYERFS_SANDBOX` names
//! it: a `process` daemon process (macOS), or that process in Linux
//! namespaces and a cgroup (`namespaces`). Each folder holds everything
//! only that backend uses, its operating-system facts included: `process`
//! has the process groups and `macos/`, `namespaces` has `linux/`, the
//! command filter, the network and the root. Callers reach them as
//! `layerfs_sandbox_backend::process` and `::namespaces`; this folder's
//! name is not public. Each folder's `ADAPTER` is reached through
//! `adapter()`, the one match (G2).
mod adapter;
mod kind;
pub(crate) use adapter::adapter;
pub use kind::Backend;
pub mod namespaces;
pub mod process;
