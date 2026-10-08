//! One daemon-owned disposable SQLite database, with typed namespaced jobs.
//!
//! The engine owns SQL and profiling. Filesystem validation, scheduling, native
//! replies and process lifetimes belong to its callers. Windows bound each job;
//! they do not limit total files, changes, payload or Workspace lifetime.
#![forbid(unsafe_code)]

mod contract;
mod database;
mod diagnostics;
mod lifetime;
mod maintenance;
mod namespace;
mod payload;
pub(crate) use contract::error;
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
pub(crate) use maintenance::reclaim;

pub use close::CleanupState;
pub use compound::SourceRows;
pub use contract::custody::{CapturedReader, FileRead, LookupOwner, OpenFile, OperationOwner};
pub use contract::indexed_operation_record::{
    indexed_operation_record_changes_bytes, IndexedOperationRecordApply,
    IndexedOperationRecordChange, IndexedOperationRecordKey, IndexedOperationRecordScope,
    OperationRecordExpectedValue,
};
pub use contract::native::{NativeDecision, NativeMount, NativeMountState, NativeObservation};
pub use contract::native_directory::{
    NativeCookie, NativeCookiePlan, NativeDirectory, NativeDirectoryCursor, NativeDirectoryPage,
    NativeDirectoryRead,
};
pub use database::accounting::{Resources, StoredCounts};
pub use database::allocation::{
    AllocationState, AllocationWork, CLEANUP_HEADROOM, MUTATION_GROWTH,
};
pub use database::startup::{Creation, CreationWork};
pub use db::Overlay;
pub use diagnostics::payload::PayloadWork;
pub use error::{OverlayError, OverlayResult};
pub use maintenance::{MaintenanceCursor, MaintenanceStep};
pub use metrics::{DatabaseWork, StatementKind, StatementWork};
pub use payload::captured_types::{
    CapturedGap, CapturedRunCursor, CapturedRunReply, CapturedRunStep, CapturedRunWork,
};
pub use profile::{DatabaseProfile, ProfileConfig};
pub use reclaim::ReclaimStep;
pub use types::{
    BaseSource, Binding, Capture, Cell, Changes, DirectoryEntry, DirectoryEntryChange,
    DirectoryEntryWindow, Generation, Inode, InodeKind, Lease, LeaseKind, LocalRead, NameLayers,
    OperationRecord, PayloadWrite, Publication, Route, WorkspaceState, CELL_BYTES,
    COMPOUND_DIRECTORY_ENTRIES, COMPOUND_INODES, MASK_BYTES, OPERATION_RECORD_BYTES, PAGE_ROWS,
    READ_WINDOW, WRITE_WINDOW,
};
