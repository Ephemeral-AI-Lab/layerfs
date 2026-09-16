//! Complete-file construction, chunking, extent mapping and logical reads.
//!
//! Entry module: declarations and re-exports only.

pub mod cdc;
pub mod edit;
pub mod mapping;

mod content;
mod read;
mod view;

pub use content::{
    classify, construct_bytes, construct_stream, encode_whole_file, encode_whole_file_payload,
    inspect, whole_file_payload, ConstructedFile, FileContent, WHOLE_CANONICAL_OVERHEAD,
};
pub use edit::{
    apply_edits, Edit, EditFrontier, EditRequest, EditSource, EditStream, Replacements,
    MAXIMUM_EDITS_PER_OPERATION,
};
pub use mapping::{ExtentBuilder, ReadCounters};
pub use read::{read_all, read_all_bounded, read_range};
pub use view::FileView;
