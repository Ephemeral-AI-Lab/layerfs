//! Extent-tree mapping: typed fields, canonical codec, streaming build and read.
//!
//! Entry module: declarations and re-exports only.

mod build;
mod codec;
mod predecessor;
mod read;
mod types;

pub(crate) use build::build_streaming_with_predecessor;
pub use build::{build_streaming, emit_empty_leaf, emit_file_state, ExtentBuilder, MappingBuild};
pub use codec::{
    chunk_canonical_len, decode_chunk_payload, decode_file_state, decode_node,
    decode_node_with_context, encode_chunk_object, encode_file_state, encode_node, profile_id,
    CHUNK_MAGIC,
};
pub use predecessor::PredecessorBase;
pub(crate) use predecessor::PredecessorCursor;
pub use read::{
    read_range, PageCache, RangeCursor, ReadCounters, READ_NAVIGATION_CACHE_PAGES,
    READ_NAVIGATION_WAVE, READ_WAVE_BYTES, READ_WAVE_OBJECTS,
};
pub use types::{
    ChildDescriptor, ExtentNode, ExtentSlice, FileState, NodeSummary, MAX_ENTRIES, MAX_LEVEL,
    MAX_NODE_OBJECT_BYTES, MIN_ENTRIES,
};
