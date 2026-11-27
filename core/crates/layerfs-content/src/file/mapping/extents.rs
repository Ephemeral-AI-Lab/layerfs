//! Forward walk of a stored chunked mapping, one extent at a time.
//!
//! The walk visits the extents of one committed file in logical order. A
//! mapping page is read when the walk reaches it and dropped when the walk
//! leaves it, so a pass holds one decoded leaf and one stack entry per level,
//! never the mapping and never a payload byte. A subtree that ends at or
//! before the offset the caller asks for is skipped without reading its page.

use layerfs_telemetry::timer::{Active, TimingScope};

use crate::error::{ContentError, ContentResult};
use crate::file::mapping::codec::decode_node_with_context;
use crate::file::mapping::node::{ExtentNode, ExtentSlice, FileState, NodeSummary};
use crate::object::{AuthenticatedObjects, ObjectId};

/// The frontier of one forward pass: pending subtrees and the current leaf.
pub(crate) struct Walk {
    stack: Vec<(NodeSummary, bool, u64)>,
    leaf: std::vec::IntoIter<ExtentSlice>,
    position: u64,
}

impl Walk {
    /// A pass from the first extent of `state`'s tree.
    pub(crate) fn new(state: FileState) -> Self {
        Self {
            stack: vec![(
                NodeSummary {
                    id: state.mapping_root,
                    bytes: state.logical_len,
                    extents: state.extent_count,
                    level: state.tree_level,
                },
                true,
                0,
            )],
            leaf: Vec::new().into_iter(),
            position: 0,
        }
    }

    /// The next extent with its logical range. Subtrees that end at or before
    /// `start` are skipped unread; extents of the leaf that holds `start` are
    /// returned from that leaf's first one. `read` acquires one mapping page,
    /// given its identity and whether it is the tree's root.
    pub(crate) fn next(
        &mut self,
        start: u64,
        read: &mut dyn FnMut(ObjectId, bool) -> ContentResult<Vec<u8>>,
    ) -> ContentResult<Option<(u64, u64, ExtentSlice)>> {
        loop {
            if let Some(extent) = self.leaf.next() {
                let from = self.position;
                self.position = from
                    .checked_add(u64::from(extent.logical_length()))
                    .ok_or(ContentError::LengthOverflow)?;
                return Ok(Some((from, self.position, extent)));
            }
            let Some((summary, root, base)) = self.stack.pop() else {
                return Ok(None);
            };
            // A subtree that ends at or before the requested start cannot overlap
            // it, and neither can any subtree after it in logical order, so the
            // walk skips it without reading its page.
            if start != 0
                && base
                    .checked_add(summary.bytes)
                    .ok_or(ContentError::LengthOverflow)?
                    <= start
            {
                continue;
            }
            let node = decode_node_with_context(&read(summary.id, root)?, root)?;
            if node.level() != summary.level
                || node.logical_len() != summary.bytes
                || node.extent_count() != summary.extents
            {
                return Err(ContentError::InvalidRecord("mapping page summary"));
            }
            match node {
                ExtentNode::Leaf { extents, .. } => {
                    self.leaf = extents.into_iter();
                    self.position = base;
                }
                ExtentNode::Branch {
                    level, children, ..
                } => {
                    let child_level = level
                        .checked_sub(1)
                        .ok_or(ContentError::MappingDepthExceeded)?;
                    let mut previous_bytes = 0_u64;
                    let mut previous_extents = 0_u64;
                    let mut pending = Vec::with_capacity(children.len());
                    for child in &children {
                        let bytes = child
                            .cumulative_logical_end
                            .checked_sub(previous_bytes)
                            .ok_or(ContentError::InvalidRecord("mapping branch order"))?;
                        let extents = child
                            .cumulative_extent_end
                            .checked_sub(previous_extents)
                            .ok_or(ContentError::InvalidRecord("mapping branch order"))?;
                        pending.push((
                            NodeSummary {
                                id: child.child_object_id,
                                bytes,
                                extents,
                                level: child_level,
                            },
                            false,
                            base.checked_add(previous_bytes)
                                .ok_or(ContentError::LengthOverflow)?,
                        ));
                        previous_bytes = child.cumulative_logical_end;
                        previous_extents = child.cumulative_extent_end;
                    }
                    self.stack.extend(pending.into_iter().rev());
                }
            }
        }
    }
}

/// One extent of a committed file: `length` bytes at `offset` are the bytes
/// of chunk `chunk` from `chunk_offset`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Extent {
    /// Logical offset of the extent's first byte.
    pub offset: u64,
    /// Logical bytes the extent supplies.
    pub length: u32,
    /// Identity of the chunk object the bytes are read from.
    pub chunk: ObjectId,
    /// Offset of the extent's first byte inside that chunk's payload. An
    /// extent is its whole chunk only when this is zero and `length` is the
    /// payload's length; the mapping does not record the latter, so bytes
    /// that hash to `chunk` are the only proof.
    pub chunk_offset: u32,
}

/// Forward cursor over the extents of one committed chunked file.
pub struct ExtentCursor {
    state: FileState,
    walk: Walk,
    start: u64,
    pages: u64,
}

impl ExtentCursor {
    /// A cursor at the first extent of `state`'s file. Reads nothing.
    pub fn new(state: FileState) -> Self {
        Self {
            state,
            walk: Walk::new(state),
            start: 0,
            pages: 0,
        }
    }

    /// Repositions the cursor at the extent that holds `offset`; the next
    /// extent returned is that one. Reads nothing: the pages on the path are
    /// read by the next call, one per level.
    pub fn seek(&mut self, offset: u64) {
        self.walk = Walk::new(self.state);
        self.start = offset;
    }

    /// The next extent in logical order, or none at the end of the file.
    pub fn next(
        &mut self,
        reader: &dyn AuthenticatedObjects,
        scope: &TimingScope<'_, Active>,
    ) -> ContentResult<Option<Extent>> {
        loop {
            let pages = &mut self.pages;
            let found = self.walk.next(self.start, &mut |id, _| {
                *pages += 1;
                reader.read_canonical_scoped(id, scope.child("mapping.extents"))
            })?;
            let Some((from, to, slice)) = found else {
                return Ok(None);
            };
            // Extents of the sought leaf that end at or before the offset.
            if to <= self.start {
                continue;
            }
            return Ok(Some(Extent {
                offset: from,
                length: slice.logical_length(),
                chunk: slice.payload_object_id(),
                chunk_offset: slice.source_offset(),
            }));
        }
    }

    /// Mapping pages this cursor has read.
    pub const fn pages(&self) -> u64 {
        self.pages
    }
}
