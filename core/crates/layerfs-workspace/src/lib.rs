//! Workspace semantics, with immutable content access and separate overlay jobs.
//!
//! The initial surface binds authentic immutable roots and retains demand read
//! plans. Mutable filesystem operations, runtime/Commit and native FUSE remain
//! separate implementation slices.
#![forbid(unsafe_code)]

mod base;
mod cache;
mod client;
mod install;
mod workspace;

pub use base::{BaseRead, BaseStat, BaseView};
pub use client::{CanonicalClient, ClientWork};
pub use install::PreparedBase;
pub use workspace::{Workspace, WorkspaceError, WorkspaceResult};
