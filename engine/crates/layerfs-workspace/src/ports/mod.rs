//! Service boundaries; declarations and reexports only.
mod captured_namespace;
mod captured_runs;
mod digests;
mod files;
pub use captured_namespace::OverlayCapturedNamespace;
pub use captured_runs::OverlayCapturedRuns;
pub use digests::{FileDigestRow, FileDigests, FILE_DIGEST_WINDOW};
mod lengths;
mod operation_record;
mod overlay;
pub use files::OverlayFileRead;
pub use lengths::FileLengths;
pub use operation_record::{
    OperationRecordApply, OperationRecordCopies, OperationRecordInputRefusal,
    OperationRecordRefusal, OperationRecordReply, OperationRecordWindow, OverlayOperationRecords,
};
pub use overlay::{OverlayJobs, OverlayRead};
