//! One Workspace's disposable SQLite database and typed atomic jobs.
//!
//! The engine owns SQL and profiling. Filesystem validation, scheduling, kernel
//! replies and process lifetimes belong to its callers. Windows bound each job;
//! they do not limit total files, changes, payload or Workspace lifetime.
#![deny(unsafe_code)]

mod contract;
mod database;
mod diagnostics;
mod lifetime;
mod maintenance;
mod namespace;
mod open;
mod payload;
pub(crate) use contract::error;
pub(crate) use contract::types::ReferenceKind;
pub(crate) use database::connection as db;
pub(crate) use lifetime::close;
pub(crate) use namespace::compound;
pub(crate) use namespace::inode;
pub(crate) use payload::cells;
pub(crate) use payload::layers;

pub(crate) use diagnostics::metrics;

pub(crate) use contract::types;
pub(crate) use database::profile;
pub(crate) use database::statements as sql;
pub(crate) use maintenance::work as reclaim;

pub use close::CleanupState;
pub use compound::SourceRows;
pub use contract::custody::{CapturedReader, FileRead, LookupOwner, OpenFile, OperationOwner};
pub use contract::indexed_operation_record::{
    indexed_operation_record_changes_bytes, IndexedOperationRecordApply,
    IndexedOperationRecordChange, IndexedOperationRecordKey, IndexedOperationRecordScope,
    OperationRecordExpectedValue,
};
pub use contract::kernel::{
    KernelApplied, KernelDecision, KernelEffect, KernelMount, KernelMountState, KernelObservation,
};
pub use contract::kernel_directory::{
    KernelCookieOffer, KernelDirectory, KernelDirectoryCursor, KernelDirectoryPage,
};
pub use database::accounting::{Resources, StoredCounts};
pub use database::physical::{AllocationState, AllocationWork, ClosedFile};
pub use database::startup::{Creation, CreationWork};
#[cfg(target_os = "windows")]
pub use database::windows_identity::{
    windows_allocation_size, windows_handle_identity, windows_path_identity,
};
pub use db::Overlay;
pub use diagnostics::payload::PayloadWork;
pub use error::{CloseFailure, OverlayError, OverlayResult};
pub use lifetime::tickets::{ReplyTickets, Settled};
pub use maintenance::{MaintenanceCursor, MaintenanceStep};
pub use metrics::{DatabaseWork, StatementKind, StatementWork};
pub use open::{OpenDisposals, OpenStateCounts};
pub use payload::captured_types::{
    CapturedGap, CapturedRunCursor, CapturedRunReply, CapturedRunStep, CapturedRunWork,
};
pub use payload::digest::CapturedDigest;
pub use profile::{DatabaseProfile, ProfileConfig};
pub use reclaim::ReclaimStep;
pub use types::{
    BaseSource, Binding, Capture, Cell, Changes, ClaimedRange, DirectoryEntry,
    DirectoryEntryChange, DirectoryEntryWindow, Generation, Inode, InodeKind, LocalRead,
    NameLayers, PayloadWrite, Publication, ReferenceRange, WorkspaceState, WriteClaims, CELL_BYTES,
    COMPOUND_DIRECTORY_ENTRIES, COMPOUND_INODES, MASK_BYTES, OPERATION_RECORD_BYTES, PAGE_ROWS,
    READ_REFERENCES, READ_WINDOW, RUN_BYTES, WRITE_CLAIMS, WRITE_WINDOW,
};
