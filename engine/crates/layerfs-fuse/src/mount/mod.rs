//! Linux mount profile, first-party mount controls and negotiation facts.
mod profile;
pub(crate) mod syscalls;

pub(crate) use profile::negotiate;
pub use profile::Negotiation;
// `private_namespace` is for the daemon entry only (prototype for #3/#499).
pub use syscalls::{control_root, private_namespace, AbortWrite, MountEntry, CONTROL};
