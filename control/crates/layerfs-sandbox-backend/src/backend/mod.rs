//! Every Sandbox backend, one folder each, named as `LAYERFS_SANDBOX` names
//! it: a native ("host_process") daemon process, or that process in Linux
//! namespaces and a cgroup. Callers reach them as
//! `layerfs_sandbox_backend::host_process` and `::namespaces`; this folder's
//! name is not public. Each folder's `ADAPTER` is reached through
//! `adapter()`, the one match (G2).
mod adapter;
mod kind;
pub(crate) use adapter::adapter;
pub use kind::Backend;
pub mod host_process;
pub mod namespaces;
