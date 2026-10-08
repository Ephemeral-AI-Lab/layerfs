//! Service boundaries; declarations and reexports only.
mod captured_namespace;
mod captured_runs;
mod files;
pub use captured_namespace::OverlayCapturedNamespace;
pub use captured_runs::OverlayCapturedRuns;
mod lengths;
mod operation_record;
mod overlay;
pub use files::OverlayFileRead;
pub use lengths::FileLengths;
pub use operation_record::{
    OperationRecordApply, OperationRecordCopies, OperationRecordInputRefusal,
    OperationRecordRefusal, OperationRecordReply, OverlayOperationRecords,
};
pub use overlay::{OverlayJobs, OverlayRead};
