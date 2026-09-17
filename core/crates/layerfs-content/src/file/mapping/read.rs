//! Bounded extent traversal and ordered payload demand.
//!
//! Navigation is batched by level: every mapping page the requested range can
//! reach at one level is demanded in one provider call, so a read issues one
//! navigation call per bounded level wave instead of one point call per visited
//! page. Payload reads are grouped the same way: every distinct payload demanded
//! inside a bounded wave is acquired by one call, then each demand is served from
//! the wave in logical order. A payload demanded more than once is read once and
//! borrowed; it is never cloned per demand. The wave is released before the next
//! one, so a long read never holds the whole file payload.

use std::io::Write;
use std::ops::Range;

use layerfs_telemetry::timer::{Active, TimingScope};

use crate::error::{ContentError, ContentResult};
use crate::file::cdc;
use crate::file::mapping::codec::{decode_chunk_payload, decode_node_with_context};
use crate::file::mapping::types::{ExtentNode, ExtentSlice, FileState};
use crate::object::{AuthenticatedObjects, ObjectId};

/// Distinct payloads a single wave may hold.
pub const READ_WAVE_OBJECTS: usize = 32;
/// Declared byte ceiling of one wave: the object bound at the largest chunk.
pub const READ_WAVE_BYTES: usize = READ_WAVE_OBJECTS * cdc::MAXIMUM_CHUNK_BYTES;
/// Navigation pages one level wave may hold at once.
///
/// A level is demanded in waves of this many pages, so the pages themselves are
/// capped at READ_NAVIGATION_WAVE * MAX_NODE_OBJECT_BYTES. The frontier retains
/// one 56-byte entry per mapping node the range still has to reach at the next
/// level; the same read visits those nodes and pays for them either way.
pub const READ_NAVIGATION_WAVE: usize = 32;

/// Work performed by a logical read.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReadCounters {
    /// Mapping pages read.
    pub nodes_read: u64,
    /// Navigation provider calls issued.
    pub node_batches_read: u64,
    /// Largest single navigation call.
    pub max_node_batch: u64,
    /// Distinct payload objects read.
    pub payload_ids_read: u64,
    /// Payload batch calls issued.
    pub payload_batches_read: u64,
    /// Largest single payload batch.
    pub max_payload_batch: u64,
    /// Payload bytes emitted to the sink.
    pub payload_bytes_read: u64,
}

/// One ordered payload demand inside a wave.
struct Demand {
    id: ObjectId,
    source_offset: u32,
    length: u32,
}

/// One mapping node the traversal has still to reach.
struct Frontier {
    id: ObjectId,
    root: bool,
    level: u8,
    origin: u64,
    expected_root: Option<(u64, u64)>,
}

struct Wave<'a, 'r, 's> {
    reader: &'a dyn AuthenticatedObjects,
    sink: &'a mut dyn Write,
    scope: &'s TimingScope<'r, Active>,
    demands: Vec<Demand>,
    distinct: Vec<ObjectId>,
    counters: ReadCounters,
}

impl<'a, 'r, 's> Wave<'a, 'r, 's> {
    fn new(
        reader: &'a dyn AuthenticatedObjects,
        sink: &'a mut dyn Write,
        scope: &'s TimingScope<'r, Active>,
    ) -> Self {
        Self {
            reader,
            sink,
            scope,
            demands: Vec::with_capacity(READ_WAVE_OBJECTS * 2),
            distinct: Vec::with_capacity(READ_WAVE_OBJECTS),
            counters: ReadCounters::default(),
        }
    }

    fn push(&mut self, id: ObjectId, source_offset: u32, length: u32) -> ContentResult<()> {
        if !self.distinct.contains(&id) {
            if self.distinct.len() >= READ_WAVE_OBJECTS {
                self.flush()?;
            }
            self.distinct.push(id);
        }
        self.demands.push(Demand {
            id,
            source_offset,
            length,
        });
        if self.demands.len() >= READ_WAVE_OBJECTS * 4 {
            self.flush()?;
        }
        Ok(())
    }

    fn flush(&mut self) -> ContentResult<()> {
        if self.demands.is_empty() {
            return Ok(());
        }
        let values = self
            .reader
            .read_canonical_batch_scoped(&self.distinct, self.scope.child("mapping.payload"))?;
        if values.len() != self.distinct.len() {
            return Err(ContentError::BatchCardinality {
                requested: self.distinct.len(),
                returned: values.len(),
            });
        }
        // Every demanded object is a CHUNK; its role decoder owns the check.
        let mut payloads: Vec<&[u8]> = Vec::with_capacity(values.len());
        for value in &values {
            let inner = crate::object::decode_bytes_object(value)?;
            payloads.push(decode_chunk_payload(inner)?);
        }
        self.counters.payload_batches_read = self.counters.payload_batches_read.saturating_add(1);
        self.counters.payload_ids_read = self
            .counters
            .payload_ids_read
            .saturating_add(self.distinct.len() as u64);
        self.counters.max_payload_batch = self
            .counters
            .max_payload_batch
            .max(self.distinct.len() as u64);
        for demand in &self.demands {
            let index = self
                .distinct
                .iter()
                .position(|candidate| *candidate == demand.id)
                .ok_or(ContentError::InvalidRecord("demand index"))?;
            let payload = payloads[index];
            let start = demand.source_offset as usize;
            let end = start
                .checked_add(demand.length as usize)
                .ok_or(ContentError::LengthOverflow)?;
            let slice = payload
                .get(start..end)
                .ok_or(ContentError::InvalidRecord("extent outside payload"))?;
            self.sink.write_all(slice).map_err(|_| ContentError::Io)?;
            self.counters.payload_bytes_read = self
                .counters
                .payload_bytes_read
                .saturating_add(slice.len() as u64);
        }
        self.demands.clear();
        self.distinct.clear();
        Ok(())
    }
}

/// Reads a range of a chunked file, emitting bytes in logical order.
///
/// The file state's declared length and extent count are the root page's own
/// summary, and a read checks that the page it actually decoded agrees with them
/// before it traverses anything. The traversal is then checked against what it
/// emitted: a range is served exactly or refused, never partly served under an
/// Ok. Both checks are on bytes this read already acquired, so they add no read.
pub fn read_range(
    reader: &dyn AuthenticatedObjects,
    state: FileState,
    range: Range<u64>,
    sink: &mut dyn Write,
    scope: &TimingScope<'_, Active>,
) -> ContentResult<ReadCounters> {
    if range.start > range.end || range.end > state.logical_len {
        return Err(ContentError::InvalidRange {
            start: range.start,
            end: range.end,
            length: state.logical_len,
        });
    }
    if range.start == range.end {
        return Ok(ReadCounters::default());
    }
    let requested = range.end - range.start;
    let mut wave = Wave::new(reader, sink, scope);
    traverse(state, &range, &mut wave)?;
    wave.flush()?;
    if wave.counters.payload_bytes_read != requested {
        return Err(ContentError::InvalidRecord("mapping coverage"));
    }
    Ok(wave.counters)
}

/// Walks the mapping tree one level per bounded navigation wave.
///
/// The recursion the previous form used is unrolled into a frontier so that one
/// level's demands become one grouped call: the frontier holds the nodes still to
/// reach, in logical order, and each wave of at most READ_NAVIGATION_WAVE pages is
/// acquired, decoded and released before the next. A child whose subtree starts
/// at or after range.end is never demanded - that is where the recursive form
/// stopped descending - and neither is any node after it, because the frontier is
/// in logical order.
fn traverse(
    state: FileState,
    range: &Range<u64>,
    wave: &mut Wave<'_, '_, '_>,
) -> ContentResult<()> {
    let reader = wave.reader;
    let scope = wave.scope;
    let mut level = vec![Frontier {
        id: state.mapping_root,
        root: true,
        level: state.tree_level,
        origin: 0,
        expected_root: Some((state.logical_len, state.extent_count)),
    }];
    let mut depth = 0_u8;
    while !level.is_empty() {
        if depth > crate::file::mapping::types::MAX_LEVEL {
            return Err(ContentError::MappingDepthExceeded);
        }
        depth = depth.saturating_add(1);
        let mut next: Vec<Frontier> = Vec::new();
        let mut finished = false;
        for chunk in level.chunks(READ_NAVIGATION_WAVE) {
            let ids: Vec<ObjectId> = chunk.iter().map(|node| node.id).collect();
            let pages =
                reader.read_canonical_batch_scoped(&ids, scope.child("mapping.navigate"))?;
            if pages.len() != ids.len() {
                return Err(ContentError::BatchCardinality {
                    requested: ids.len(),
                    returned: pages.len(),
                });
            }
            wave.counters.nodes_read = wave.counters.nodes_read.saturating_add(ids.len() as u64);
            wave.counters.node_batches_read = wave.counters.node_batches_read.saturating_add(1);
            wave.counters.max_node_batch = wave.counters.max_node_batch.max(ids.len() as u64);
            for (node, canonical) in chunk.iter().zip(&pages) {
                let page = decode_node_with_context(canonical, node.root)?;
                if page.level() != node.level {
                    return Err(ContentError::InvalidRecord("mapping level"));
                }
                if let Some((logical_len, extent_count)) = node.expected_root {
                    if page.logical_len() != logical_len || page.extent_count() != extent_count {
                        return Err(ContentError::InvalidRecord("mapping coverage"));
                    }
                }
                match page {
                    ExtentNode::Leaf { extents, .. } => {
                        let mut position = node.origin;
                        for extent in extents {
                            position = push_extent(extent, position, range, wave)?;
                        }
                    }
                    ExtentNode::Branch { children, .. } => {
                        let child_level = node
                            .level
                            .checked_sub(1)
                            .ok_or(ContentError::MappingDepthExceeded)?;
                        let mut previous = node.origin;
                        for child in children {
                            let end = child.cumulative_logical_end;
                            if end <= range.start {
                                previous = end;
                                continue;
                            }
                            if previous >= range.end {
                                finished = true;
                                break;
                            }
                            next.push(Frontier {
                                id: child.child_object_id,
                                root: false,
                                level: child_level,
                                origin: previous,
                                expected_root: None,
                            });
                            previous = end;
                        }
                    }
                }
                if finished {
                    break;
                }
            }
            if finished {
                break;
            }
        }
        level = next;
    }
    Ok(())
}

fn push_extent(
    extent: ExtentSlice,
    position: u64,
    range: &Range<u64>,
    wave: &mut Wave<'_, '_, '_>,
) -> ContentResult<u64> {
    let end = position
        .checked_add(u64::from(extent.logical_length()))
        .ok_or(ContentError::LengthOverflow)?;
    if end <= range.start || position >= range.end {
        return Ok(end);
    }
    let local_start = range.start.max(position) - position;
    let local_end = range.end.min(end) - position;
    let source = u64::from(extent.source_offset())
        .checked_add(local_start)
        .ok_or(ContentError::LengthOverflow)?;
    let length = local_end - local_start;
    if length == 0 {
        return Err(ContentError::InvalidRecord("empty extent demand"));
    }
    let source_offset = u32::try_from(source).map_err(|_| ContentError::LengthOverflow)?;
    let demand_length = u32::try_from(length).map_err(|_| ContentError::LengthOverflow)?;
    wave.push(extent.payload_object_id(), source_offset, demand_length)?;
    Ok(end)
}
