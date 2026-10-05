//! One daemon-owned disposable SQLite database, with typed namespaced jobs.
//!
//! The engine owns SQL and profiling. Filesystem validation, scheduling, native
//! replies and process lifetimes belong to its callers. Windows bound each job;
//! they do not limit total files, changes, payload or Workspace lifetime.
#![forbid(unsafe_code)]

mod close;
mod db;
mod error;
mod generation;
mod inode;
mod metrics;
mod payload;
mod profile;
mod reclaim;
mod scratch;
mod sql;
mod types;
mod workspace;

pub use close::CleanupState;
pub use db::Overlay;
pub use error::{OverlayError, OverlayResult};
pub use metrics::{DatabaseWork, StatementKind, StatementWork};
pub use profile::{DatabaseProfile, ProfileConfig};
pub use reclaim::ReclaimStep;
pub use types::{
    Capture, Cell, Dentry, Generation, Inode, InodeKind, Lease, LeaseKind, Publication, Route,
    ScratchRecord, WorkspaceState, CELL_BYTES, MASK_BYTES, PAGE_ROWS, SCRATCH_BYTES,
};
