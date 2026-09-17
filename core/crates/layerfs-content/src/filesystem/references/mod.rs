//! Reference effects: bounded ordering records, runs and final inode rows.

pub mod backing;
pub mod merge;
pub mod record;
pub mod reduce;
pub mod release;
pub mod runs;

pub use backing::{FileBacking, OrderingBacking, OrderingRun};
pub use merge::{merge_runs, MergeWork, Run, RunReader};
pub use record::{Row, ROW_BYTES};
pub use reduce::{
    FinalChange, FinalRows, PendingState, ReferenceReducer, ReferenceWork, DEFAULT_BASE_BATCH,
    DEFAULT_MAXIMUM_PENDING,
};
pub use release::{release_zero_count, ReleaseWork};
pub use runs::{RunStore, DEFAULT_MERGE_BUFFER_BYTES};
