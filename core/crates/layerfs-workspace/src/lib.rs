//! Workspace semantics, with immutable content access and separate overlay jobs.
//!
//! The surface binds authentic immutable roots, composes the effective view and
//! performs ordinary namespace operations as atomic owner jobs. Payload streams,
//! lifetimes, runtime/Commit and native FUSE remain separate implementation slices.
#![forbid(unsafe_code)]

mod attributes;
mod base;
mod cache;
mod client;
mod create;
mod eval;
mod facts;
mod install;
mod job;
mod list;
mod mutate;
mod operation;
mod port;
mod remove;
mod rename;
mod serials;
mod view;
mod workspace;

pub use base::{BaseRead, BaseStat, BaseView};
pub use client::{CanonicalClient, ClientWork};
pub use facts::{BaseFacts, Need};
pub use install::PreparedBase;
pub use job::{JobOutcome, NamespaceJob};
pub use list::ViewListing;
pub use operation::{Operation, Outcome, Refusal, Time};
pub use port::{FileLengths, OverlayJobs, OverlayRead};
pub use serials::{InodeSerials, SERIAL_REFILL};
pub use view::{SourceView, ViewStat};
pub use workspace::{Workspace, WorkspaceError, WorkspaceResult};
