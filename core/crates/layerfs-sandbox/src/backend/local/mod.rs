//! Local process runtime: each Sandbox is one daemon process on this kernel,
//! started from a private directory, with no container, image or Engine.
//!
//! It is for a machine that is itself the isolation boundary (a microVM, a
//! CI job). It isolates nothing: the daemon and its commands share this
//! kernel's processes, network, users and filesystem with everything else on
//! it. Commands still run as their own nonroot identity, and what they leave
//! running ends with the Sandbox. One attempt per operation, as in the Docker
//! runtime; nothing is retried. Linux reads `/proc` and detaches mounts;
//! macOS (the native route, whose daemon serves NFS) reads the same facts
//! with `proc_pidinfo`, `sysctl` and `mount`, and forces mounts off with
//! `umount -f`.
//!
//! The daemon and its commands start with one search path and nothing else of
//! this process's environment: on Linux a container's, whatever this process
//! has; on macOS this process's own `PATH`, since a native command is this
//! user's own tool (Homebrew's `/opt/homebrew/bin`, `~/.local/bin`, a version
//! manager's `node`) and no image supplies it.
mod create;
#[cfg(target_os = "macos")]
mod darwin;
mod exec;
mod groups;
mod mounts;
mod process;
mod sandbox;
mod search;
pub use create::{Local, LocalCreateFailure, LocalPaths, LocalPhase};
pub use exec::LocalExit;
pub use process::LocalProcess;
pub use sandbox::Sandbox;
