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
//!
//! A caller that reads several ascending ranges of the same file - the retained
//! runs of one known edit are the case this exists for - uses [`RangeCursor`]
//! instead of calling [`read_range`] once per range. The cursor keeps the mapping
//! pages it has retained, so later ranges reuse those pages while they remain
//! cached. Eviction may require a later grouped demand; current navigation owns
//! its pages independently of that disposable retained cache.

use std::io::Write;
use std::ops::Range;

use layerfs_telemetry::timer::{Active, TimingScope};

use crate::error::{ContentError, ContentResult};
use crate::file::cdc;
use crate::file::mapping::codec::{decode_chunk_payload, decode_node_with_context};
use crate::file::mapping::types::{ExtentNode, ExtentSlice, FileState};
use crate::object::{AuthenticatedObjects, ObjectId};

use super::{CheckedPage, PageCache};

/// Distinct payloads a single wave may hold.
pub const READ_WAVE_OBJECTS: usize = 32;
/// Declared byte ceiling of one wave.
///
/// Both halves of the wave are enforced, not just the count: the objects a wave
/// demands are checked against [`cdc::MAXIMUM_CHUNK_BYTES`] as they are decoded,
/// and the bytes it serves are charged against this figure and refused when they
/// exceed it. A provider that hands back an oversized payload under a chunk role
/// is therefore refused at the wave boundary instead of being copied through,
/// which is what makes the object bound and the byte bound the same claim.
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
    cache: &'a mut PageCache,
    demands: Vec<Demand>,
    distinct: Vec<ObjectId>,
    counters: ReadCounters,
}

impl<'a, 'r, 's> Wave<'a, 'r, 's> {
    fn new(
        reader: &'a dyn AuthenticatedObjects,
        sink: &'a mut dyn Write,
        scope: &'s TimingScope<'r, Active>,
        cache: &'a mut PageCache,
    ) -> Self {
        Self {
            reader,
            sink,
            scope,
            cache,
            demands: Vec::with_capacity(READ_WAVE_OBJECTS * 2),
            distinct: Vec::with_capacity(READ_WAVE_OBJECTS),
            counters: ReadCounters::default(),
        }
    }

    /// Acquires one level's pages, serving what the cache already holds.
    ///
    /// All hits gain a batch owner before retained-cache eviction. Each missing
    /// identity is acquired once, even when it is demanded under two contexts.
    /// Provider vectors move into their first demand; only duplicate demands
    /// need another owned copy. Complete validation precedes any retention.
    fn pages(&mut self, nodes: &[&Frontier]) -> ContentResult<Vec<CheckedPage>> {
        if nodes.len() > READ_NAVIGATION_WAVE {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "mapping.navigation_pages",
                limit: READ_NAVIGATION_WAVE as u64,
                actual: nodes.len() as u64,
            });
        }
        let mut pages: Vec<Option<CheckedPage>> = Vec::new();
        pages
            .try_reserve_exact(nodes.len())
            .map_err(|_| ContentError::ResourceUnavailable {
                what: "mapping.navigation_owners",
            })?;
        for node in nodes {
            pages.push(match self.cache.get(node.id, node.root) {
                Some(canonical) => {
                    Some(PageCache::decode_owned(PageCache::copy_page(canonical)?, node.root)?.0)
                }
                None => None,
            });
        }
        // A hit from another context owns the same immutable canonical bytes;
        // the receiving context is independently checked below.
        for index in 0..nodes.len() {
            if pages[index].is_none() {
                if let Some(hit) = nodes
                    .iter()
                    .enumerate()
                    .position(|(other, node)| node.id == nodes[index].id && pages[other].is_some())
                {
                    pages[index] = Some(
                        pages[hit]
                            .as_ref()
                            .ok_or(ContentError::InvalidRecord("navigation hit owner"))?
                            .copy_for(nodes[index].root)?,
                    );
                }
            }
        }
        let mut ids = Vec::new();
        ids.try_reserve_exact(nodes.len())
            .map_err(|_| ContentError::ResourceUnavailable {
                what: "mapping.navigation_ids",
            })?;
        for (node, page) in nodes.iter().zip(&pages) {
            if page.is_none() && !ids.contains(&node.id) {
                ids.push(node.id);
            }
        }
        if !ids.is_empty() {
            let values = self
                .reader
                .read_canonical_batch_scoped(&ids, self.scope.child("mapping.navigate"))?;
            if values.len() != ids.len() {
                return Err(ContentError::BatchCardinality {
                    requested: ids.len(),
                    returned: values.len(),
                });
            }
            for (id, canonical) in ids.iter().copied().zip(values) {
                let first = nodes
                    .iter()
                    .position(|node| node.id == id)
                    .ok_or(ContentError::InvalidRecord("navigation identity"))?;
                // The original supplied owner is checked before any duplicate
                // copy. A copy preserves facts or checks its different context.
                pages[first] = Some(PageCache::decode_owned(canonical, nodes[first].root)?.0);
                for index in first + 1..nodes.len() {
                    if nodes[index].id == id {
                        pages[index] = Some(
                            pages[first]
                                .as_ref()
                                .ok_or(ContentError::InvalidRecord("navigation supplied owner"))?
                                .copy_for(nodes[index].root)?,
                        );
                    }
                }
            }
            self.counters.nodes_read = self.counters.nodes_read.saturating_add(ids.len() as u64);
            self.counters.node_batches_read = self.counters.node_batches_read.saturating_add(1);
            self.counters.max_node_batch = self.counters.max_node_batch.max(ids.len() as u64);
        }
        for (node, page) in nodes.iter().zip(&pages) {
            page.as_ref()
                .ok_or(ContentError::InvalidRecord("navigation checked owner"))?
                .check_summary(node.level, node.expected_root)?;
        }
        let mut result = Vec::new();
        result
            .try_reserve_exact(nodes.len())
            .map_err(|_| ContentError::ResourceUnavailable {
                what: "mapping.navigation_result",
            })?;
        for (node, page) in nodes.iter().zip(pages) {
            let page = page.ok_or(ContentError::InvalidRecord("navigation result owner"))?;
            if self.cache.get(node.id, node.root).is_none() {
                // Every hit is already owned above. The hint drops old retained
                // owners before this copy; retain independently enforces count.
                self.cache.make_room_for(1);
                self.cache.retain(node.id, page.copy_for(node.root)?)?;
            }
            result.push(page);
        }
        Ok(result)
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
        // Every demanded object is a CHUNK; its role decoder owns the role
        // check, and the wave owns the size check: an oversized payload is refused
        // here rather than sliced, so `READ_WAVE_BYTES` is a bound on bytes this
        // read actually acquired.
        let mut payloads: Vec<&[u8]> = Vec::with_capacity(values.len());
        let mut wave_bytes = 0_usize;
        for value in &values {
            let inner = crate::object::decode_bytes_object(value)?;
            let payload = decode_chunk_payload(inner)?;
            if payload.len() > cdc::MAXIMUM_CHUNK_BYTES {
                return Err(ContentError::ObjectLimitExceeded {
                    limit: cdc::MAXIMUM_CHUNK_BYTES,
                    actual: payload.len(),
                });
            }
            wave_bytes = wave_bytes.saturating_add(payload.len());
            payloads.push(payload);
        }
        if wave_bytes > READ_WAVE_BYTES {
            return Err(ContentError::ObjectLimitExceeded {
                limit: READ_WAVE_BYTES,
                actual: wave_bytes,
            });
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
    let mut cache = PageCache::new();
    let mut wave = Wave::new(reader, sink, scope, &mut cache);
    traverse(state, &range, &mut wave)?;
    wave.flush()?;
    if wave.counters.payload_bytes_read != requested {
        return Err(ContentError::InvalidRecord("mapping coverage"));
    }
    Ok(wave.counters)
}

/// Ordered reader of ascending ranges of one chunked file.
///
/// Each range is served by the same bounded traversal [`read_range`] uses, but the
/// pages that traversal acquires may be retained: a page shared by several ranges
/// is reused while cached, and eviction may cause later grouped reacquisition.
/// Ranges must be ascending and inside the file, and each one is served exactly or
/// refused - never partly served under an `Ok`.
pub struct RangeCursor<'a, 'r, 's> {
    reader: &'a dyn AuthenticatedObjects,
    state: FileState,
    scope: &'s TimingScope<'r, Active>,
    counters: ReadCounters,
    /// Position the range before the active one ended at.
    segment_start: u64,
}

impl<'a, 'r, 's> RangeCursor<'a, 'r, 's> {
    /// Opens a cursor over `state`, memoizing its pages in `pages`.
    ///
    /// The cache is the caller's, so a cursor can be handed the pages an earlier
    /// pass of the same operation acquired and can leave its own for the pass that
    /// follows it.
    pub fn new(
        reader: &'a dyn AuthenticatedObjects,
        state: FileState,
        pages: &mut PageCache,
        scope: &'s TimingScope<'r, Active>,
    ) -> ContentResult<Self> {
        let _ = pages;
        Ok(Self {
            reader,
            state,
            scope,
            counters: ReadCounters::default(),
            segment_start: 0,
        })
    }

    /// Reads the next range, appending it to `sink`.
    ///
    /// The range must start at or after the previous one's end. The sink is a
    /// parameter rather than a field so the caller can read what it has collected
    /// between two ranges without the cursor holding a borrow across both.
    pub fn read_segment(
        &mut self,
        range: Range<u64>,
        sink: &mut dyn Write,
        pages: &mut PageCache,
    ) -> ContentResult<ReadCounters> {
        if range.start > range.end
            || range.end > self.state.logical_len
            || range.start < self.segment_start
        {
            return Err(ContentError::InvalidRange {
                start: range.start,
                end: range.end,
                length: self.state.logical_len,
            });
        }
        self.segment_start = range.end;
        if range.start == range.end {
            return Ok(self.counters);
        }
        let requested = range.end - range.start;
        let mut wave = Wave::new(self.reader, sink, self.scope, pages);
        traverse(self.state, &range, &mut wave)?;
        wave.flush()?;
        let segment = wave.counters;
        if segment.payload_bytes_read != requested {
            return Err(ContentError::InvalidRecord("mapping coverage"));
        }
        merge(&mut self.counters, segment);
        Ok(self.counters)
    }
}

/// Adds one traversal's work to a cursor's running totals.
fn merge(total: &mut ReadCounters, wave: ReadCounters) {
    total.nodes_read = total.nodes_read.saturating_add(wave.nodes_read);
    total.node_batches_read = total
        .node_batches_read
        .saturating_add(wave.node_batches_read);
    total.max_node_batch = total.max_node_batch.max(wave.max_node_batch);
    total.payload_ids_read = total.payload_ids_read.saturating_add(wave.payload_ids_read);
    total.payload_batches_read = total
        .payload_batches_read
        .saturating_add(wave.payload_batches_read);
    total.max_payload_batch = total.max_payload_batch.max(wave.max_payload_batch);
    total.payload_bytes_read = total
        .payload_bytes_read
        .saturating_add(wave.payload_bytes_read);
}

/// Walks the mapping tree one level per bounded navigation wave.
///
/// The recursion the previous form used is unrolled into a frontier so that one
/// level's demands become one grouped call: the frontier holds the nodes still to
/// reach, in logical order, and each wave of at most READ_NAVIGATION_WAVE pages is
/// acquired, decoded and released before the next. A child whose subtree starts
/// at or after range.end is never demanded - that is where the recursive form
/// stopped descending - and neither is any node after it, because the frontier is
/// in logical order. Every comparison is in absolute file offsets: a page's child
/// summaries are cumulative from the page's own start, so each is rebased on the
/// frontier's origin before it is compared with the range.
fn traverse(
    state: FileState,
    range: &Range<u64>,
    wave: &mut Wave<'_, '_, '_>,
) -> ContentResult<()> {
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
            let nodes: Vec<&Frontier> = chunk.iter().collect();
            let pages = wave.pages(&nodes)?;
            for (node, canonical) in chunk.iter().zip(pages.iter()) {
                let page = decode_node_with_context(canonical.canonical(), node.root)?;
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
                        let mut previous_extents = 0;
                        for child in children {
                            // A child's cumulative end is cumulative within *this*
                            // page, so it is relative to the page's own origin while
                            // the range and the frontier carry absolute offsets. A
                            // page whose origin is zero - the root, and every page of
                            // a one-level tree - hides the difference; a non-root
                            // branch of a deeper tree does not, and reading it through
                            // the relative value prunes children the range covers.
                            let end = node
                                .origin
                                .checked_add(child.cumulative_logical_end)
                                .ok_or(ContentError::LengthOverflow)?;
                            let expected_bytes = end
                                .checked_sub(previous)
                                .ok_or(ContentError::NonCanonicalOrdering)?;
                            let expected_extents = child
                                .cumulative_extent_end
                                .checked_sub(previous_extents)
                                .ok_or(ContentError::NonCanonicalOrdering)?;
                            previous_extents = child.cumulative_extent_end;
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
                                expected_root: Some((expected_bytes, expected_extents)),
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
