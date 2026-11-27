//! One Workspace's disposable SQLite database and typed atomic jobs.
//!
//! The engine owns SQL and profiling. Filesystem validation, scheduling, kernel
//! replies and process lifetimes belong to its callers. Windows bound each job;
//! they do not limit total files, changes, payload or Workspace lifetime.
#![forbid(unsafe_code)]

pub(crate) mod backing;
mod contract;
pub(crate) mod custody;
pub(crate) mod generation;
pub(crate) mod kernel;
mod namespace;
mod open;
mod payload;
pub(crate) mod reclaim;
pub(crate) use backing::close;
pub(crate) use backing::connection as db;
pub(crate) use contract::error;
pub(crate) use contract::types::ReferenceKind;
pub(crate) use namespace::compound;
pub(crate) use namespace::inode;
pub(crate) use payload::cells::codec as cells;
pub(crate) use payload::layers;

pub(crate) use backing::metrics;

pub(crate) use backing::profile;
pub(crate) use backing::statements as sql;
pub(crate) use contract::types;

pub use backing::accounting::{Resources, StoredCounts};
pub use backing::physical::{AllocationState, AllocationWork, ClosedFile};
pub use backing::startup::{Creation, CreationWork};
pub use close::CleanupState;
pub use compound::SourceRows;
pub use contract::custody::{CapturedReader, FileRead, LookupOwner, OpenFile, OperationOwner};
pub use contract::kernel::{
    KernelApplied, KernelDecision, KernelEffect, KernelLink, KernelMount, KernelMountState,
    KernelObservation,
};
pub use contract::kernel_directory::{
    KernelCookieOffer, KernelDirectory, KernelDirectoryCursor, KernelDirectoryPage,
};
pub use contract::operation_record::{
    indexed_operation_record_changes_bytes, IndexedOperationRecordApply,
    IndexedOperationRecordChange, IndexedOperationRecordKey, IndexedOperationRecordScope,
    OperationRecordExpectedValue,
};
pub use db::Overlay;
pub use error::{CloseFailure, OverlayError, OverlayResult};
pub use generation::captured::cursor::{
    CapturedGap, CapturedRunCursor, CapturedRunReply, CapturedRunStep, CapturedRunWork,
};
pub use generation::captured::digest::CapturedDigest;
pub use generation::tickets::{ReplyTickets, Settled};
pub use metrics::{DatabaseWork, StatementKind, StatementWork};
pub use open::{OpenDisposals, OpenStateCounts};
pub use payload::work::PayloadWork;
pub use profile::{DatabaseProfile, ProfileConfig};
pub use reclaim::turn::ReclaimStep;
pub use reclaim::{MaintenanceCursor, MaintenanceStep};
pub use types::{
    BaseSource, Binding, Capture, Cell, Changes, ClaimedRange, DirectoryEntry,
    DirectoryEntryChange, DirectoryEntryWindow, Generation, Inode, InodeKind, LocalRead,
    NameLayers, PayloadWrite, Publication, ReferenceRange, WorkspaceState, WriteClaims, CELL_BYTES,
    COMPOUND_DIRECTORY_ENTRIES, COMPOUND_INODES, MASK_BYTES, OPERATION_RECORD_BYTES, PAGE_ROWS,
    READ_REFERENCES, READ_WINDOW, RECORD_ROWS, RUN_BYTES, WRITE_CLAIMS, WRITE_WINDOW,
};
