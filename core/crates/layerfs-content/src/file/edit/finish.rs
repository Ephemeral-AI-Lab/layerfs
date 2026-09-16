//! Final emission of one known edit: the root, and only the root.
//!
//! Every child object is emitted before its parent, and the file state is derived
//! from facts the frontier already established. The empty representation is the
//! same defined form complete-file construction uses for a zero-length file.

use crate::error::ContentResult;
use crate::file::mapping::{self, MappingBuild};
use crate::object::{FinalizedConsumer, ObjectId};

/// Root emitted by one edit operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmittedRoot {
    /// Identity that opens the result.
    pub root: ObjectId,
    /// Logical length of the result.
    pub logical_len: u64,
    /// Chunk objects emitted for replacements.
    pub chunks: u64,
    /// Mapping pages emitted.
    pub nodes: u64,
}

/// Emits the defined empty file representation.
pub fn emit_empty_representation(
    consumer: &mut dyn FinalizedConsumer,
) -> ContentResult<EmittedRoot> {
    let mut build = MappingBuild::default();
    let leaf = mapping::emit_empty_leaf(consumer, &mut build)?;
    let root = mapping::emit_file_state(consumer, leaf)?;
    Ok(EmittedRoot {
        root,
        logical_len: 0,
        chunks: build.chunks,
        nodes: build.nodes + 1,
    })
}

/// Emits the file state of a finished mapping.
pub fn emit_file_state(
    consumer: &mut dyn FinalizedConsumer,
    build: MappingBuild,
) -> ContentResult<EmittedRoot> {
    let mapping_root = build
        .root
        .ok_or(crate::error::ContentError::WrongLogicalRole)?;
    let root = mapping::emit_file_state(consumer, mapping_root)?;
    Ok(EmittedRoot {
        root,
        logical_len: mapping_root.bytes,
        chunks: build.chunks,
        nodes: build.nodes + 1,
    })
}
