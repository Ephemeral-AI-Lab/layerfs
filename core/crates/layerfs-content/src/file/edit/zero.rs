//! On-demand positive zero evidence over exact authenticated stored ranges.
use crate::file::mapping::{
    decode_chunk_payload, decode_node_with_context, ExtentNode, NodeSummary, PageCache, MIN_ENTRIES,
};
use crate::{AuthenticatedObjects, ContentError, ContentResult, ObjectId};
use layerfs_telemetry::timer::{Active, TimingScope};
use std::{collections::BTreeMap, ops::Range};

// This is an immutable work memo, not an input limit or growing operation set.
// Eviction repeats real demands; diverse fragmentation still pays its visits.
const WITNESSES: usize = 64;
#[derive(Clone, Copy)]
enum Witness {
    Chunk {
        length: usize,
    },
    Mapping {
        bytes: u64,
        extents: u64,
        level: u8,
        nonroot: bool,
    },
}
#[derive(Default)]
pub(super) struct ZeroEvidence {
    positive: BTreeMap<ObjectId, Witness>,
}
impl ZeroEvidence {
    fn remember(&mut self, id: ObjectId, witness: Witness) {
        if self.positive.len() == WITNESSES && !self.positive.contains_key(&id) {
            self.positive.clear();
        }
        self.positive.insert(id, witness);
    }
    fn summary(node: &ExtentNode, summary: NodeSummary) -> ContentResult<()> {
        if node.logical_len() != summary.bytes
            || node.extent_count() != summary.extents
            || node.level() != summary.level
        {
            return Err(ContentError::InvalidRecord("zero mapping summary"));
        }
        Ok(())
    }
    pub fn mapping(
        &mut self,
        reader: &dyn AuthenticatedObjects,
        pages: &mut PageCache,
        summary: NodeSummary,
        range: Range<u64>,
        root: bool,
        scope: &TimingScope<'_, Active>,
    ) -> ContentResult<bool> {
        if range.start > range.end || range.end > summary.bytes {
            return Err(ContentError::InvalidRange {
                start: range.start,
                end: range.end,
                length: summary.bytes,
            });
        }
        if range.is_empty() {
            return Ok(true);
        }
        if let Some(witness) = self.positive.get(&summary.id) {
            return match *witness {
                Witness::Mapping {
                    bytes,
                    extents,
                    level,
                    nonroot,
                } if bytes == summary.bytes
                    && extents == summary.extents
                    && level == summary.level
                    && (root || nonroot) =>
                {
                    Ok(true)
                }
                _ => Err(ContentError::InvalidRecord("zero mapping witness")),
            };
        }
        let node = if let Some(canonical) = pages.get(summary.id, root) {
            let node = decode_node_with_context(canonical, root)?;
            Self::summary(&node, summary)?;
            node
        } else {
            let canonical =
                reader.read_canonical_scoped(summary.id, scope.child("mapping.zero_navigate"))?;
            let node = decode_node_with_context(&canonical, root)?;
            Self::summary(&node, summary)?;
            // Invalid summaries cannot evict or poison the existing memo.
            pages.make_room_for(1);
            pages.insert(summary.id, root, canonical);
            node
        };
        let nonroot = node.entry_count() >= MIN_ENTRIES;
        match node {
            ExtentNode::Leaf { extents, .. } => {
                let mut before = 0u64;
                for extent in extents {
                    let end = before
                        .checked_add(u64::from(extent.logical_length()))
                        .ok_or(ContentError::LengthOverflow)?;
                    let from = range.start.max(before);
                    let to = range.end.min(end);
                    if from < to {
                        let low = u64::from(extent.source_offset())
                            .checked_add(from - before)
                            .ok_or(ContentError::LengthOverflow)?;
                        let high = low
                            .checked_add(to - from)
                            .ok_or(ContentError::LengthOverflow)?;
                        if !self.chunk(reader, extent.payload_object_id(), low..high, scope)? {
                            return Ok(false);
                        }
                    }
                    before = end;
                    if before >= range.end {
                        break;
                    }
                }
            }
            ExtentNode::Branch {
                children, level, ..
            } => {
                let mut before = 0u64;
                let mut previous_extents = 0u64;
                for child in children {
                    let end = child.cumulative_logical_end;
                    let from = range.start.max(before);
                    let to = range.end.min(end);
                    if from < to {
                        let child_summary = NodeSummary {
                            id: child.child_object_id,
                            bytes: end
                                .checked_sub(before)
                                .ok_or(ContentError::NonCanonicalOrdering)?,
                            extents: child
                                .cumulative_extent_end
                                .checked_sub(previous_extents)
                                .ok_or(ContentError::NonCanonicalOrdering)?,
                            level: level
                                .checked_sub(1)
                                .ok_or(ContentError::MappingDepthExceeded)?,
                        };
                        if !self.mapping(
                            reader,
                            pages,
                            child_summary,
                            from - before..to - before,
                            false,
                            scope,
                        )? {
                            return Ok(false);
                        }
                    }
                    before = end;
                    previous_extents = child.cumulative_extent_end;
                    if before >= range.end {
                        break;
                    }
                }
            }
        }
        // A partial overlap proves only that overlap. It never certifies the
        // rest of the node or a payload outside the requested byte domain.
        if range.start == 0 && range.end == summary.bytes {
            self.remember(
                summary.id,
                Witness::Mapping {
                    bytes: summary.bytes,
                    extents: summary.extents,
                    level: summary.level,
                    nonroot,
                },
            );
        }
        Ok(true)
    }
    fn chunk(
        &mut self,
        reader: &dyn AuthenticatedObjects,
        id: ObjectId,
        range: Range<u64>,
        scope: &TimingScope<'_, Active>,
    ) -> ContentResult<bool> {
        let low = usize::try_from(range.start).map_err(|_| ContentError::LengthOverflow)?;
        let high = usize::try_from(range.end).map_err(|_| ContentError::LengthOverflow)?;
        if let Some(witness) = self.positive.get(&id) {
            return match *witness {
                Witness::Chunk { length } if low <= high && high <= length => Ok(true),
                _ => Err(ContentError::InvalidRecord("zero chunk witness")),
            };
        }
        let canonical = reader.read_canonical_scoped(id, scope.child("mapping.zero_payload"))?;
        let value = crate::object::decode_bytes_object(&canonical)?;
        let payload = decode_chunk_payload(value)?;
        let selected = payload
            .get(low..high)
            .ok_or(ContentError::InvalidRecord("zero chunk range"))?;
        if selected.iter().any(|byte| *byte != 0) {
            return Ok(false);
        }
        if payload.iter().all(|byte| *byte == 0) {
            self.remember(
                id,
                Witness::Chunk {
                    length: payload.len(),
                },
            );
        }
        Ok(true)
    }
}
