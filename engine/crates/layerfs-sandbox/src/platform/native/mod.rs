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
//! is read where it lives: [`macos`] supplies Darwin process facts, the
//! `mount` table and `umount -f` for the loopback NFS the macOS daemon serves;
//! [`linux`] supplies `/proc` facts, the `mountinfo` table and a lazy
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
#[cfg(target_os = "linux")]
pub(crate) mod linux;
pub(crate) mod listener;
#[cfg(target_os = "macos")]
pub(crate) mod macos;
pub(crate) mod process;
mod provider;
mod sandbox;
pub(crate) mod search;
pub(crate) mod streams;
pub use create::{Local, LocalCreateFailure, LocalPaths, LocalPhase};
#[cfg(target_os = "linux")]
pub(crate) use linux as os;
#[cfg(target_os = "macos")]
pub(crate) use macos as os;
pub use process::LocalProcess;
#[cfg(target_os = "linux")]
pub(crate) use provider::KERNEL_ROUTE;
pub use provider::{NativeLinuxProvider, NativeMacosProvider};
pub use sandbox::Sandbox;
