//! Known-edit construction: checked input, bounded frontier and final emission.
//!
//! Entry module: declarations and re-exports only.

mod apply;
mod compare;
mod concat;
mod draft_codec;
mod draft_port;
mod draft_record;
mod draft_resident;
mod finish;
mod input;
mod no_draft;
mod split;
mod temporary;
mod tree;

pub use apply::{apply_edits, apply_edits_with_state, EditRequest};
pub use compare::{compare_replacements, NoOpVerdict, COMPARE_WINDOW_BYTES};
pub use concat::coalesce_adjacent;
pub use draft_port::{DraftCapacity, DraftJob, DraftScope, DraftState, DraftStats};
pub use draft_record::DraftRecord;
pub use draft_resident::ResidentDrafts;
pub use finish::{emit_empty_representation, EmittedRoot};
pub use input::{Edit, EditSequence, EditSource, ReplacementReader};
pub use input::{Plan, Segment};
pub use no_draft::{apply_edits_without_drafts, NoDraft};
pub use split::slice_of;
pub use tree::{EditCounters, EditObjects, EDIT_DEFERRED_LIMIT};
