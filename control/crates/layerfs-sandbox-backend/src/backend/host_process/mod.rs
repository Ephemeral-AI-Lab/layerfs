//! Native process runtime: each Sandbox is one daemon process on this kernel,
//! started from a private directory, with no container, image or Engine.
//! It runs on macOS only (`HostProcessProvider`): Linux starts no native
//! Sandbox (#533 O27), and a native record found there is refused with the
//! records-only `sandbox remove` as its fix (RV3 FAIL 3).
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
//! host; the backend itself on macOS only.
mod adapter;
#[cfg(target_os = "macos")]
mod exec;
#[cfg(target_os = "macos")]
mod provider;
#[cfg(target_os = "macos")]
pub(crate) mod sandbox;
pub(crate) use adapter::ADAPTER;
#[cfg(target_os = "macos")]
pub use provider::HostProcessProvider;
#[cfg(target_os = "macos")]
pub use sandbox::Sandbox;
