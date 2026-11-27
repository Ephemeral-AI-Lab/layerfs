//! Process process runtime: each Sandbox is one daemon process on this kernel,
//! started from a private directory, with no container, image or Engine.
//! It runs on macOS only (`ProcessProvider`): Linux starts no process
//! Sandbox (#533 O27), and a process record found there is refused with the
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
//! Its adapter (stored name `"process"`, capabilities) builds on every Unix
//! host; the backend itself on macOS only.
mod adapter;
pub(crate) mod instance;
#[cfg(target_os = "macos")]
pub(crate) mod macos;
#[cfg(target_os = "macos")]
mod provider;
pub(crate) use adapter::ADAPTER;
#[cfg(target_os = "macos")]
pub use instance::handle::Sandbox;
#[cfg(target_os = "macos")]
pub use provider::ProcessProvider;
