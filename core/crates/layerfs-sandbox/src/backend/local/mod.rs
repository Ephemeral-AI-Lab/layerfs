//! Local process runtime: each Sandbox is one daemon process on this kernel,
//! started from a private directory, with no container, image or Engine.
//!
//! It is for a machine that is itself the isolation boundary (a microVM, a
//! CI job). It isolates nothing: the daemon and its commands share this
//! kernel's processes, network, users and filesystem with everything else on
//! it. Commands still run as their own nonroot identity, and what they leave
//! running ends with the Sandbox. One attempt per operation, as in the Docker
//! runtime; nothing is retried. Linux only: it reads `/proc` and detaches
//! mounts.
mod create;
mod exec;
mod groups;
mod mounts;
mod process;
mod sandbox;
pub use create::{Local, LocalCreateFailure, LocalPaths, LocalPhase};
pub use exec::LocalExit;
pub use process::LocalProcess;
pub use sandbox::Sandbox;
