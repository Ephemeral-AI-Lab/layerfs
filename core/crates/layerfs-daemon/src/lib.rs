//! Daemon service ownership. SQL stays in the independently usable overlay crate.
//!
//! Initial service runs bounded typed jobs fairly and retains queue/reply credits.
//! Native FUSE, process/control and authenticated upstream assembly remain later
//! integration slices.
#![forbid(unsafe_code)]

mod overlay;
pub(crate) use overlay::commands;
pub(crate) use overlay::credits;
pub(crate) use overlay::owner;
pub(crate) use overlay::queue;

pub use commands::{Command, Response, ServiceClass};
pub use owner::{Completion, Owner, OwnerClient, OwnerConfig, OwnerError, Pending};
pub use queue::OwnerWork;
