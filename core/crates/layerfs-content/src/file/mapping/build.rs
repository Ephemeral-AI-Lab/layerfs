//! Streaming construction of the extent tree.
//!
//! Chunks and retained slices are offered to one owned, decoded, unfinished
//! builder. Full pages are sealed as soon as they can no longer change, and the
//! file state is emitted last from facts already established. Only unfinished
//! boundary pages are retained, so construction - and an edit - never holds the
//! whole mapping. The same builder serves complete-file construction and known
//! edits, so both produce the identical canonical partition for identical
//! extents.

use std::io::Read;

use crate::error::{ContentError, ContentResult};
use crate::file::cdc::FastCdc;
use crate::file::mapping::codec::{
    encode_chunk_object, encode_file_state, encode_node, profile_id,
};
use crate::file::mapping::types::{
    ChildDescriptor, ExtentNode, ExtentSlice, FileState, NodeSummary, MAX_ENTRIES, MAX_LEVEL,
};
use crate::object::{
    AdvisoryPredecessors, FinalizedConsumer, FinalizedObject, ObjectId, ObjectRole,
    PredecessorProvenance,
};
use crate::policy::ConstructionCapacities;

use super::predecessor::PredecessorCursor;

/// Facts established while building; no root reread is required afterwards.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MappingBuild {
    /// Mapping root, or `None` when the input produced no extents at all.
    pub root: Option<NodeSummary>,
    /// Logical bytes scanned.
    pub logical_len: u64,
    /// Extents in the tree.
    pub extent_count: u64,
    /// Level of the mapping root.
    pub tree_level: u8,
    /// Chunk objects emitted.
    pub chunks: u64,
    /// Largest number of decoded entries the builder held at once.
    pub peak_pending: usize,
    /// Mapping pages emitted.
    pub nodes: u64,
}

enum Pending {
    Extents(Vec<ExtentSlice>),
    Children(Vec<NodeSummary>),
}

impl Pending {
    fn len(&self) -> usize {
        match self {
            Self::Extents(entries) => entries.len(),
            Self::Children(entries) => entries.len(),
        }
    }
}

/// One owned decoded unfinished builder.
///
/// It holds only boundary pages: a level is flushed as soon as it can no longer
/// change, so the retained entry count is bounded by the height and the page
/// capacity rather than by the file length or the number of edits.
pub struct ExtentBuilder {
    levels: Vec<Pending>,
    flush_at: usize,
    build: MappingBuild,
    peak_pending: usize,
}

impl ExtentBuilder {
    /// Empty builder under the declared capacities.
    pub fn new(capacities: &ConstructionCapacities) -> Self {
        let flush_at = capacities.stream_flush_entries.max(MAX_ENTRIES + 1);
        Self {
            levels: vec![Pending::Extents(Vec::with_capacity(flush_at + 1))],
            flush_at,
            build: MappingBuild::default(),
            peak_pending: 0,
        }
    }

    /// Logical bytes offered so far.
    pub const fn logical_len(&self) -> u64 {
        self.build.logical_len
    }

    /// Extents offered so far.
    pub const fn extent_count(&self) -> u64 {
        self.build.extent_count
    }

    /// Chunk objects emitted so far.
    pub const fn chunks(&self) -> u64 {
        self.build.chunks
    }

    /// Mapping pages emitted so far.
    pub const fn nodes(&self) -> u64 {
        self.build.nodes
    }

    /// Largest number of decoded entries held at once.
    pub const fn peak_pending(&self) -> usize {
        self.peak_pending
    }

    /// Entries currently retained before their page is sealed.
    pub fn pending_entries(&self) -> usize {
        self.levels.iter().map(Pending::len).sum()
    }

    /// Encodes `raw` into one chunk object and appends it as an extent.
    ///
    /// `predecessor` is the retained payload this run continues, when the caller
    /// knows one. It is advisory only: C2 decides whether the hint is acquired and
    /// worth encoding against.
    pub fn push_chunk(
        &mut self,
        raw: &[u8],
        predecessor: Option<ObjectId>,
        consumer: &mut dyn FinalizedConsumer,
    ) -> ContentResult<ObjectId> {
        let mut object = FinalizedObject::new(ObjectRole::Chunk, encode_chunk_object(raw)?)?;
        if let Some(predecessor) = predecessor {
            let mut predecessors = AdvisoryPredecessors::new();
            predecessors.push(predecessor, PredecessorProvenance::UnchangedPrefix)?;
            object = object.with_predecessors(predecessors);
        }
        let id = object.id();
        consumer.accept(object)?;
        self.build.chunks = add(self.build.chunks, 1)?;
        self.push_extent(ExtentSlice::new(id, 0, raw.len() as u32)?, consumer)?;
        Ok(id)
    }

    /// Appends one retained slice of an already-stored payload.
    ///
    /// Two contiguous slices of the same payload are merged, because the canonical
    /// extent partition forbids that pair: a deletion inside one chunk would
    /// otherwise produce a page whose identity depends on how the caller split its
    /// edits.
    pub fn push_extent(
        &mut self,
        extent: ExtentSlice,
        consumer: &mut dyn FinalizedConsumer,
    ) -> ContentResult<()> {
        let _ = &consumer;
        self.build.logical_len = add(self.build.logical_len, u64::from(extent.logical_length()))?;
        match &mut self.levels[0] {
            Pending::Extents(extents) => match extents
                .last()
                .copied()
                .and_then(|previous| crate::file::edit::coalesce_adjacent(previous, extent))
            {
                Some(merged) => {
                    if let Some(last) = extents.last_mut() {
                        *last = merged;
                    }
                }
                None => extents.push(extent),
            },
            Pending::Children(_) => return Err(ContentError::InvalidRecord("builder level zero")),
        }
        self.note_pending();
        self.flush_streaming(consumer, 0)
    }

    fn note_pending(&mut self) {
        let pending = self.pending_entries();
        if pending > self.peak_pending {
            self.peak_pending = pending;
        }
    }

    /// Completes the tree, emitting every remaining page child before parent.
    pub fn finish(mut self, consumer: &mut dyn FinalizedConsumer) -> ContentResult<MappingBuild> {
        if self.build.logical_len == 0 {
            self.build.peak_pending = self.peak_pending;
            return Ok(self.build);
        }
        let root = self.finish_levels(consumer)?;
        self.build.root = Some(root);
        self.build.extent_count = root.extents;
        self.build.tree_level = root.level;
        self.build.peak_pending = self.peak_pending;
        Ok(self.build)
    }

    /// Flushes every level that is over the streaming bound, lowest first.
    ///
    /// A flush removes `MAX_ENTRIES` entries from one level and adds one to the
    /// level above, so the cascade continues upward for exactly as long as some
    /// level exceeds the bound: no level accumulates entries for the length of the
    /// stream, and the retained entry count stays a function of the height and the
    /// page capacity rather than of the file. The canonical partition is untouched:
    /// a flush emits a full page and `finish_levels` still partitions whatever
    /// remains, including the half-partition of an exactly-overfull level.
    fn flush_streaming(
        &mut self,
        consumer: &mut dyn FinalizedConsumer,
        level: usize,
    ) -> ContentResult<()> {
        let mut current = level;
        while self.levels[current].len() > self.flush_at {
            // A streaming flush emits a full page that a higher level will hold,
            // so it is never the tree's root page.
            let summary = self.emit_prefix(consumer, current, MAX_ENTRIES, false)?;
            self.push_summary(current + 1, summary)?;
            if self.levels[current].len() <= self.flush_at {
                // This level is under its bound again; only the one above can have
                // been pushed over it.
                current += 1;
                if current > usize::from(MAX_LEVEL) {
                    return Err(ContentError::MappingDepthExceeded);
                }
            }
        }
        Ok(())
    }

    fn finish_levels(
        &mut self,
        consumer: &mut dyn FinalizedConsumer,
    ) -> ContentResult<NodeSummary> {
        let mut level = 0;
        loop {
            let higher_nonempty = self
                .levels
                .iter()
                .skip(level + 1)
                .any(|pending| pending.len() != 0);
            let len = self.levels[level].len();
            if !higher_nonempty && len <= MAX_ENTRIES {
                if level > 0 && len == 1 {
                    if let Pending::Children(children) = &self.levels[level] {
                        return Ok(children[0]);
                    }
                }
                return self.emit_prefix(consumer, level, len, true);
            }
            if len != 0 {
                let first = if len > MAX_ENTRIES { len / 2 } else { len };
                let summary = self.emit_prefix(consumer, level, first, false)?;
                self.push_summary(level + 1, summary)?;
                continue;
            }
            level += 1;
            if level >= self.levels.len() {
                return Err(ContentError::InvalidRecord("empty rope builder"));
            }
        }
    }

    fn emit_prefix(
        &mut self,
        consumer: &mut dyn FinalizedConsumer,
        level: usize,
        count: usize,
        root: bool,
    ) -> ContentResult<NodeSummary> {
        let node = match &mut self.levels[level] {
            Pending::Extents(entries) => {
                let entries: Vec<ExtentSlice> = entries.drain(..count).collect();
                let bytes = entries.iter().try_fold(0_u64, |sum, entry| {
                    add(sum, u64::from(entry.logical_length()))
                })?;
                ExtentNode::Leaf {
                    subtree_logical_bytes: bytes,
                    extents: entries,
                }
            }
            Pending::Children(entries) => {
                let entries: Vec<NodeSummary> = entries.drain(..count).collect();
                let mut bytes = 0_u64;
                let mut extents = 0_u64;
                let children = entries
                    .iter()
                    .map(|entry| {
                        bytes = add(bytes, entry.bytes)?;
                        extents = add(extents, entry.extents)?;
                        Ok(ChildDescriptor {
                            cumulative_logical_end: bytes,
                            cumulative_extent_end: extents,
                            child_object_id: entry.id,
                        })
                    })
                    .collect::<ContentResult<Vec<_>>>()?;
                ExtentNode::Branch {
                    level: level as u8,
                    subtree_logical_bytes: bytes,
                    subtree_extent_count: extents,
                    children,
                }
            }
        };
        emit_node(consumer, &node, &mut self.build, root)
    }

    fn push_summary(&mut self, level: usize, summary: NodeSummary) -> ContentResult<()> {
        if usize::from(summary.level) + 1 != level {
            return Err(ContentError::InvalidRecord("rope builder level"));
        }
        while self.levels.len() <= level {
            self.levels
                .push(Pending::Children(Vec::with_capacity(MAX_ENTRIES + 1)));
        }
        match &mut self.levels[level] {
            Pending::Children(children) => children.push(summary),
            Pending::Extents(_) => return Err(ContentError::InvalidRecord("rope builder role")),
        }
        Ok(())
    }
}

/// Builds the extent tree for `source`, emitting every finalized object in
/// child-before-parent order.
pub fn build_streaming<R: Read>(
    capacities: &ConstructionCapacities,
    source: R,
    consumer: &mut dyn FinalizedConsumer,
) -> ContentResult<MappingBuild> {
    build_streaming_with_predecessor(capacities, source, None, consumer)
}

/// Builds the extent tree for `source`, offering every chunk the stored payload
/// the base mapping holds at that chunk's own byte range.
///
/// The cursor is advanced by the builder's own logical position, which is the
/// chunk's start offset: the chunk at offset `n` of the result is offered the base
/// extent covering offset `n` of the base. A cursor that has run out of
/// correspondence offers nothing, and the construction is then byte for byte the
/// one without a base.
pub(crate) fn build_streaming_with_predecessor<R: Read>(
    capacities: &ConstructionCapacities,
    source: R,
    predecessor: Option<&mut PredecessorCursor<'_, '_, '_>>,
    consumer: &mut dyn FinalizedConsumer,
) -> ContentResult<MappingBuild> {
    let mut builder = ExtentBuilder::new(capacities);
    let mut predecessor = predecessor;
    let scanned = FastCdc::new().scan(source, |chunk| {
        let start = builder.logical_len();
        let hint = match predecessor.as_mut() {
            Some(cursor) => cursor.hint(start, chunk.len() as u32)?,
            None => None,
        };
        builder.push_chunk(chunk, hint, consumer).map(|_| ())
    })?;
    if scanned.bytes_scanned != builder.logical_len() {
        return Err(ContentError::InvalidRecord("chunk accounting"));
    }
    builder.finish(consumer)
}

/// Emits the defined empty mapping page and returns its summary.
pub fn emit_empty_leaf(
    consumer: &mut dyn FinalizedConsumer,
    build: &mut MappingBuild,
) -> ContentResult<NodeSummary> {
    emit_node(
        consumer,
        &ExtentNode::Leaf {
            subtree_logical_bytes: 0,
            extents: Vec::new(),
        },
        build,
        true,
    )
}

/// Emits the file-state root for an already-finished mapping.
pub fn emit_file_state(
    consumer: &mut dyn FinalizedConsumer,
    mapping_root: NodeSummary,
) -> ContentResult<ObjectId> {
    let state = FileState {
        logical_len: mapping_root.bytes,
        extent_count: mapping_root.extents,
        tree_level: mapping_root.level,
        profile_id: profile_id(),
        mapping_root: mapping_root.id,
    };
    let object = FinalizedObject::new(ObjectRole::FileState, encode_file_state(state)?)?
        .with_references(vec![mapping_root.id]);
    let id = object.id();
    consumer.accept(object)?;
    Ok(id)
}

fn emit_node(
    consumer: &mut dyn FinalizedConsumer,
    node: &ExtentNode,
    build: &mut MappingBuild,
    root: bool,
) -> ContentResult<NodeSummary> {
    if node.level() > MAX_LEVEL {
        return Err(ContentError::MappingDepthExceeded);
    }
    let role = match node {
        ExtentNode::Leaf { .. } => ObjectRole::ExtentLeaf,
        ExtentNode::Branch { .. } => ObjectRole::ExtentBranch,
    };
    let object =
        FinalizedObject::new(role, encode_node(node, root)?)?.with_references(node.references());
    let summary = NodeSummary {
        id: object.id(),
        bytes: node.logical_len(),
        extents: node.extent_count(),
        level: node.level(),
    };
    consumer.accept(object)?;
    build.nodes = add(build.nodes, 1)?;
    Ok(summary)
}

fn add(left: u64, right: u64) -> ContentResult<u64> {
    left.checked_add(right).ok_or(ContentError::LengthOverflow)
}
