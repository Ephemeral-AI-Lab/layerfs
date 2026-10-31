//! Native process runtime: each Sandbox is one daemon process on this kernel,
//! started from a private directory, with no container, image or Engine.
//!
//! It is for a machine that is itself the isolation boundary (a microVM, a
//! CI job, a developer's own Mac). It isolates nothing: the daemon and its
//! commands share this kernel's processes, network, users and filesystem with
//! everything else on it. Commands still run as their own nonroot identity
//! where the platform allows it, and what they leave running ends with the
//! Sandbox. One attempt per operation, as in the Docker runtime; nothing is
//! retried.
//!
//! The lifecycle here is one implementation for both platforms. What differs
//! is read where it lives: [`super::macos`] supplies Darwin process facts, the
//! `mount` table and `umount -f` for the loopback NFS the macOS daemon serves;
//! [`super::linux`] supplies `/proc` facts, the `mountinfo` table and a lazy
//! `MNT_DETACH` for the FUSE mount the root daemon serves.
//!
//! The daemon and its commands start with one search path and nothing else of
//! this process's environment: on Linux a container's, whatever this process
//! has; on macOS this process's own `PATH`, since a native command is this
//! user's own tool (Homebrew's `/opt/homebrew/bin`, `~/.local/bin`, a version
//! manager's `node`) and no image supplies it.
mod create;
mod exec;
mod groups;
pub(crate) mod process;
mod provider;
mod sandbox;
pub(crate) mod search;
#[cfg(target_os = "linux")]
pub(crate) use super::linux as platform;
#[cfg(target_os = "macos")]
pub(crate) use super::macos as platform;
pub use create::{Local, LocalCreateFailure, LocalPaths, LocalPhase};
pub use exec::LocalExit;
pub use process::LocalProcess;
pub use provider::{
    NativeLinuxInstance, NativeLinuxProvider, NativeMacosInstance, NativeMacosProvider,
};
pub use sandbox::Sandbox;
