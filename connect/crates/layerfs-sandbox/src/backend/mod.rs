//! Every Sandbox backend, one folder each, named as `LAYERFS_SANDBOX` names
//! it: a Docker container, a native daemon process, or that process in Linux
//! namespaces and a cgroup. Callers reach them as `layerfs_sandbox::docker`,
//! `::native` and `::namespaces`; this folder's name is not public.
pub mod docker;
mod kind;
pub use kind::Backend;
#[cfg(target_os = "linux")]
pub mod namespaces;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod native;
