//! Daemon service ownership. SQL stays in the independently usable overlay crate.
//!
//! Initial service runs bounded typed jobs fairly and retains queue/reply credits.
//! Native FUSE, process/control and authenticated upstream assembly remain later
//! integration slices.
#![forbid(unsafe_code)]

mod commands;
mod credits;
mod owner;
mod queue;

pub use commands::{Command, Response, ServiceClass};
pub use owner::{Completion, Owner, OwnerClient, OwnerConfig, OwnerError, Pending};
pub use queue::OwnerWork;
