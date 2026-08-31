#![forbid(unsafe_code)]

mod collector;
mod dedup;
mod operation;
mod snapshot;

pub use collector::Monitor;
pub use dedup::{CandidateTotals, DedupAnalysis};
pub use operation::{
    CandidateStats, OperationFamily, OperationId, OperationOutcome, OperationReceipt,
    SemanticOperation, TimingFragment,
};
pub(crate) use snapshot::database_snapshot;
pub use snapshot::{DatabaseSnapshot, MonitorError, MonitorResult, MonitorSnapshot};
