//! Bounded extent traversal and ordered payload demand.
//!
//! Navigation reads one mapping page per visited node as a one-ID batch. Payload
//! reads are grouped: every distinct payload demanded inside a bounded wave is
//! acquired by a single batch call, then each demand is served from the wave in
//! logical order. A payload demanded more than once is read once and borrowed;
//! it is never cloned per demand. The wave is released before the next one, so a
//! long read never holds the whole file payload.

use std::io::Write;
use std::ops::Range;

use crate::error::{ContentError, ContentResult};
use crate::file::cdc;
use crate::file::mapping::codec::{decode_chunk_payload, decode_node_with_context};
use crate::file::mapping::types::{ExtentNode, ExtentSlice, FileState};
use crate::object::{AuthenticatedObjects, ObjectId};

/// Distinct payloads a single wave may hold.
pub const READ_WAVE_OBJECTS: usize = 32;
/// Declared byte ceiling of one wave: the object bound at the largest chunk.
pub const READ_WAVE_BYTES: usize = READ_WAVE_OBJECTS * cdc::MAXIMUM_CHUNK_BYTES;

/// Work performed by a logical read.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReadCounters {
    /// Mapping pages read.
    pub nodes_read: u64,
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

struct Wave<'a> {
    reader: &'a dyn AuthenticatedObjects,
    sink: &'a mut dyn Write,
    demands: Vec<Demand>,
    distinct: Vec<ObjectId>,
    counters: ReadCounters,
}

impl<'a> Wave<'a> {
    fn new(reader: &'a dyn AuthenticatedObjects, sink: &'a mut dyn Write) -> Self {
        Self {
            reader,
            sink,
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
        let values = self.reader.read_canonical_batch(&self.distinct)?;
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

/// Reads `range` of a chunked file, emitting bytes in logical order.
///
/// The file state's declared length and extent count are the root page's own
/// summary, and a read checks that the page it actually decoded agrees with them
/// before it traverses anything. The traversal is then checked against what it
/// emitted: a range is served exactly or refused, never partly served under an
/// `Ok`. Both checks are on bytes this read already acquired, so they add no
/// read.
pub fn read_range(
    reader: &dyn AuthenticatedObjects,
    state: FileState,
    range: Range<u64>,
    sink: &mut dyn Write,
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
    let mut wave = Wave::new(reader, sink);
    descend(
        reader,
        state.mapping_root,
        true,
        state.tree_level,
        0,
        &range,
        &mut wave,
        Some((state.logical_len, state.extent_count)),
    )?;
    wave.flush()?;
    if wave.counters.payload_bytes_read != requested {
        return Err(ContentError::InvalidRecord("mapping coverage"));
    }
    Ok(wave.counters)
}

/// Traverses one page, checking the root against the file state it was opened from.
#[allow(clippy::too_many_arguments)]
fn descend(
    reader: &dyn AuthenticatedObjects,
    id: ObjectId,
    root: bool,
    level: u8,
    origin: u64,
    range: &Range<u64>,
    wave: &mut Wave<'_>,
    expected_root: Option<(u64, u64)>,
) -> ContentResult<()> {
    let canonical = reader.read_canonical(id)?;
    wave.counters.nodes_read = wave.counters.nodes_read.saturating_add(1);
    let node = decode_node_with_context(&canonical, root)?;
    if node.level() != level {
        return Err(ContentError::InvalidRecord("mapping level"));
    }
    if let Some((logical_len, extent_count)) = expected_root {
        if node.logical_len() != logical_len || node.extent_count() != extent_count {
            return Err(ContentError::InvalidRecord("mapping coverage"));
        }
    }
    match node {
        ExtentNode::Leaf { extents, .. } => {
            let mut position = origin;
            for extent in extents {
                position = push_extent(extent, position, range, wave)?;
            }
            Ok(())
        }
        ExtentNode::Branch { children, .. } => {
            let mut previous = origin;
            for child in children {
                let end = child.cumulative_logical_end;
                if end <= range.start {
                    previous = end;
                    continue;
                }
                if previous >= range.end {
                    return Ok(());
                }
                descend(
                    reader,
                    child.child_object_id,
                    false,
                    level - 1,
                    previous,
                    range,
                    wave,
                    None,
                )?;
                previous = end;
            }
            Ok(())
        }
    }
}

fn push_extent(
    extent: ExtentSlice,
    position: u64,
    range: &Range<u64>,
    wave: &mut Wave<'_>,
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
