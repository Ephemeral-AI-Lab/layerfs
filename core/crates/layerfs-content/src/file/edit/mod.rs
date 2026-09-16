//! Known-edit construction: checked input, bounded frontier and final emission.
//!
//! Entry module: declarations and re-exports only.

mod apply;
mod compare;
mod concat;
mod finish;
mod frontier;
mod input;
mod split;

pub use apply::{apply_edits, EditRequest};
pub use compare::{compare_replacements, NoOpVerdict, COMPARE_WINDOW_BYTES};
pub use concat::coalesce_adjacent;
pub use finish::{emit_empty_representation, EmittedRoot};
pub use frontier::EditFrontier;
pub use input::{Edit, EditSource, EditStream, ReplacementReader, MAXIMUM_EDITS_PER_OPERATION};
pub use input::{Plan, Replacements, Segment};
pub use split::slice_of;
