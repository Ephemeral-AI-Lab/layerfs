//! Known-edit construction: checked input, bounded frontier and final emission.
//!
//! Entry module: declarations and re-exports only.

mod apply;
mod backing;
mod compare;
mod concat;
mod draft_codec;
mod engine;
mod finish;
mod input;
mod objects;
mod references;
mod resolution;
mod runs;
mod source;
mod split;
mod state;
mod tree;
mod zero;

pub use apply::{
    apply_edits, apply_edits_backed, apply_indexed_edits_view_backed, EditRequest,
    IndexedEditRequest,
};
pub use backing::{
    EditRecordApply, EditRecordChange, EditRecordExpected, EditRecordKey, IndexedEditBacking,
};
pub use compare::{compare_replacements, NoOpVerdict, COMPARE_WINDOW_BYTES};
pub use concat::coalesce_adjacent;
pub use finish::{emit_empty_representation, EmittedRoot};
pub use input::{Edit, EditSequence, EditSource, IndexedEditSource, ReplacementReader};
pub use input::{Plan, Segment};
pub use objects::{EditCounters, EditObjects, EDIT_DEFERRED_LIMIT};
pub use split::slice_of;
