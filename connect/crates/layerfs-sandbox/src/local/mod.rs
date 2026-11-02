//! Shared local-process lifecycle: starting and stopping a daemon process on
//! this kernel, its listener, search path, streams, process groups and the
//! platform facts (`os`): `macos` supplies Darwin process facts, the `mount`
//! table and `umount -f`; `linux` supplies `/proc` facts, the `mountinfo`
//! table and a lazy `MNT_DETACH`. The host_process and namespaces backends
//! build on it.
mod create;
pub(crate) mod groups;
pub(crate) mod listener;
#[path = "os/mod.rs"]
pub(crate) mod os_impl;
pub(crate) mod process;
pub(crate) mod search;
pub(crate) mod streams;
pub(crate) use create::KERNEL_ROUTE;
pub use create::{Local, LocalCreateFailure, LocalPaths, LocalPhase};
#[cfg(target_os = "linux")]
pub(crate) use os_impl::linux as os;
#[cfg(target_os = "macos")]
pub(crate) use os_impl::macos as os;
pub use process::LocalProcess;
