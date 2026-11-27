//! Workspace semantics, with immutable content access and separate overlay jobs.
//!
//! The surface binds authentic immutable roots, composes the effective view and
//! performs ordinary namespace and byte operations as atomic owner jobs.
//! Lifetimes, runtime/Commit and kernel FUSE remain separate implementation slices.
#![forbid(unsafe_code)]

mod base;
pub(crate) mod binding;
mod construction;
pub(crate) mod job;
mod operations;
pub(crate) use operations::attributes;

pub(crate) use base::cache;
pub(crate) use base::fetch as client;
pub(crate) use binding::install;
pub(crate) use job::eval;
pub(crate) use job::facts;
pub(crate) use operations::namespace::create;
pub(crate) use operations::namespace::list;
pub(crate) use operations::outcomes as operation;
pub mod ports;
pub mod requests;
pub(crate) use binding::serials;
pub(crate) use binding::view;
pub(crate) use operations::namespace::remove;
pub(crate) use operations::namespace::rename;
pub(crate) use ports as port;

pub(crate) use operations::file::write;

pub use base::{BaseContent, BaseRead, BaseStat, BaseView, DIRECTORY_COUNT_CAPACITY};
pub use binding::{Workspace, WorkspaceError, WorkspaceResult};
pub use cache::{CanonicalCache, ConstructedObjects};
pub use client::{CanonicalClient, ClientWork};
pub use construction::{
    CapturedFileAttempt, CapturedFileCustody, CapturedFileEdits, CapturedFileWork,
};
pub use construction::{
    CapturedNamespace, CapturedNamespaceAttempt, CapturedNamespaceCustody, CapturedNamespaceWork,
};
pub use construction::{EditBackingCustody, EditBackingWork, EditInputRefusal, IndexedEditRecords};
pub use construction::{
    EditBackingCustody as ConstructionBackingCustody, EditBackingWork as ConstructionBackingWork,
    EditInputRefusal as ConstructionInputRefusal, IndexedEditRecords as IndexedConstructionRecords,
};
pub use facts::{BaseFacts, Need, VisitFacts};
pub use install::PreparedBase;
pub use job::plan::{MutationInputFailure, MutationPlan, MutationStage};
pub use job::run::{JobOutcome, NamespaceJob};
pub use list::ViewListing;
pub use operation::{Operation, Outcome, Position, Refusal, Time, WriteData};
pub use operations::file::recognize::{
    Recognition, RecognizeWork, WriteRun, RECOGNIZE_BYTES, UNMATCHED_BYTES,
};
pub use operations::kernel::directory::{
    KernelDirectoryBatch, KernelDirectoryEntry, KernelDirectoryListing, KernelDirectoryRead,
    KernelDirectoryVisit, KernelDirectoryWindow,
};
pub use operations::kernel::lookup::{
    Decided, KernelReadDecision, KernelReadFailure, KernelReadOperation, KernelReadOutcome,
    KernelReadValue,
};
pub use operations::kernel::mutation::KernelMutationOutcome;
pub use operations::kernel::read::{KernelDataVisit, KernelWindow};
pub use operations::kernel::visit::{
    KernelInput, KernelMutationVisit, KernelReadVisit, KernelVisitRequest,
};
pub use port::OverlayCapturedNamespace;
pub use port::{FileDigestRow, FileDigests, FILE_DIGEST_WINDOW};
pub use port::{FileLengths, OverlayCapturedRuns, OverlayFileRead, OverlayJobs, OverlayRead};
pub use port::{
    OperationRecordApply, OperationRecordCopies, OperationRecordInputRefusal,
    OperationRecordRefusal, OperationRecordReply, OperationRecordWindow, OverlayOperationRecords,
};
pub use serials::{InodeSerials, SERIAL_REFILL};
pub use view::{SourceView, ViewStat};

pub use binding::Route;
