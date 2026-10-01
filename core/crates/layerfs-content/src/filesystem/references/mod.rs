//! Reference effects: bounded ordering records, runs and final inode rows.

pub mod backing;
pub mod merge;
pub mod record;
pub mod reduce;
pub mod release;
pub mod runs;
mod seek;
mod tier_stream;

pub use backing::{FileBacking, OrderingBacking, OrderingRun};
pub use merge::{merge_runs, MergeWork, Run, RunReader};
pub use record::{Row, ROW_BYTES};
pub use reduce::{
    FinalChange, FinalRows, PendingState, ReferenceReducer, ReferenceWork, DEFAULT_BASE_BATCH,
    DEFAULT_MAXIMUM_PENDING,
};
pub use release::{release_zero_count, ReleaseWork};
pub use runs::{RunStore, DEFAULT_MERGE_BUFFER_BYTES};

pub(crate) mod count_reduce;
pub(crate) mod release_state;
pub use count_reduce::{
    canonical_base_working_bytes, canonical_count_control_working_bytes,
    canonical_final_working_bytes, canonical_zero_working_bytes,
};
pub use release_state::canonical_release_working_bytes;
