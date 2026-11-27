//! Known-edit construction: checked input, bounded frontier and final emission.
//!
//! Entry module: declarations and re-exports only.

mod apply;
mod backing;
pub(crate) mod coalesce;
mod finish;
mod input;
pub(crate) mod noop;
mod objects;
mod runs;
mod source;
mod tree;
mod zero;

pub use apply::{
    apply_edits, apply_edits_backed, apply_indexed_edits_view_backed, EditRequest,
    IndexedEditRequest,
};
pub use backing::{
    EditRecordApply, EditRecordChange, EditRecordExpected, EditRecordKey, IndexedEditBacking,
    IndexedRecordPage,
};
pub use coalesce::coalesce_adjacent;
pub use finish::{emit_empty_representation, EmittedRoot};
pub use input::{Edit, EditSequence, EditSource, IndexedEditSource, ReplacementReader};
pub use input::{Plan, Segment};
pub use noop::{NoOpVerdict, COMPARE_WINDOW_BYTES};
pub use objects::{EditCounters, EditObjects, EDIT_DEFERRED_LIMIT};
