//! Native process runtime: each Sandbox is one daemon process on this kernel,
//! started from a private directory, with no container, image or Engine.
//! Only macOS starts native Sandboxes (`HostProcessProvider`); on Linux this
//! module only reopens, stops and deletes a native Sandbox from a record
//! written before 2.0 (item 1.10 would remove that build).
//!
//! It is for a machine that is itself the isolation boundary (a microVM, a
//! CI job, a developer's own Mac). It isolates nothing: the daemon and its
//! commands share this kernel's processes, network, users and filesystem with
//! everything else on it. Commands still run as their own nonroot identity
//! where the platform allows it, and what they leave running ends with the
//! Sandbox. One attempt per operation, as in the Docker runtime; nothing is
//! retried.
//!
//! Its adapter (stored name `"native"`, capabilities) builds on every Unix
//! host; the backend itself on macOS and Linux only.
mod adapter;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod exec;
#[cfg(target_os = "macos")]
mod provider;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) mod sandbox;
pub(crate) use adapter::ADAPTER;
#[cfg(target_os = "macos")]
pub use provider::HostProcessProvider;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use sandbox::Sandbox;
