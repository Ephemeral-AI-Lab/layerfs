//! Workspace semantics, with immutable content access and separate overlay jobs.
//!
//! The surface binds authentic immutable roots, composes the effective view and
//! performs ordinary namespace and byte operations as atomic owner jobs.
//! Lifetimes, runtime/Commit and native FUSE remain separate implementation slices.
#![forbid(unsafe_code)]

mod base;
mod construction;
mod mutation;
mod operations;
mod workspace;
pub(crate) use operations::attributes;

pub(crate) use base::cache;
pub(crate) use base::client;
pub(crate) use mutation::eval;
pub(crate) use mutation::facts;
pub(crate) use mutation::job;
pub(crate) use operations::namespace::create;
pub(crate) use operations::namespace::list;
pub(crate) use operations::types as operation;
pub(crate) use workspace::install;
mod ports;
pub(crate) use operations::namespace::remove;
pub(crate) use operations::namespace::rename;
pub(crate) use ports as port;
pub(crate) use workspace::serials;
pub(crate) use workspace::view;

pub(crate) use operations::file::write;

pub use base::{BaseRead, BaseStat, BaseView};
pub use cache::CanonicalCache;
pub use client::{CanonicalClient, ClientWork};
pub use construction::{
    CapturedFileAttempt, CapturedFileCustody, CapturedFileEdits, CapturedFileWork,
};
pub use construction::{EditBackingCustody, EditBackingWork, EditInputRefusal, IndexedEditRecords};
pub use construction::{
    EditBackingCustody as ConstructionBackingCustody, EditBackingWork as ConstructionBackingWork,
    EditInputRefusal as ConstructionInputRefusal, IndexedEditRecords as IndexedConstructionRecords,
};
pub use facts::{BaseFacts, Need};
pub use install::PreparedBase;
pub use job::{JobOutcome, NamespaceJob};
pub use list::ViewListing;
pub use operation::{Operation, Outcome, Position, Refusal, Time, WriteData};
pub use port::OverlayCapturedNamespace;
pub use port::{FileLengths, OverlayCapturedRuns, OverlayFileRead, OverlayJobs, OverlayRead};
pub use port::{
    OperationRecordApply, OperationRecordCopies, OperationRecordInputRefusal,
    OperationRecordRefusal, OperationRecordReply, OverlayOperationRecords,
};
pub use serials::{InodeSerials, SERIAL_REFILL};
pub use view::{SourceView, ViewStat};
pub use workspace::{Workspace, WorkspaceError, WorkspaceResult};
