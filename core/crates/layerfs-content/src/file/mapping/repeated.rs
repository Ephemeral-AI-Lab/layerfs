//! Uniform extent runs preserve the ordinary builder's canonical lookahead.
use super::build::{add, emit_node, Pending};
use super::{
    encode_chunk_object, ChildDescriptor, ExtentBuilder, ExtentNode, ExtentSlice, NodeSummary,
    MAX_ENTRIES, MAX_LEVEL,
};
use crate::{
    AdvisoryPredecessors, ContentError, ContentResult, FinalizedConsumer, FinalizedObject,
    ObjectId, ObjectRole, PredecessorProvenance,
};

#[derive(Clone, Copy)]
enum Entry {
    Extent(ExtentSlice),
    Child(NodeSummary),
}

impl ExtentBuilder {
    /// Appends `count` identical chunks, accepting the payload once and reusing
    /// complete identical mapping pages. The unfinished suffix at every level
    /// and the final root match ordinary streaming of these chunks exactly.
    /// Work depends on boundary pages and tree height, not the repeat count.
    pub fn push_repeated_chunk(
        &mut self,
        raw: &[u8],
        count: u64,
        consumer: &mut dyn FinalizedConsumer,
    ) -> ContentResult<Option<ObjectId>> {
        self.push_repeated_chunk_with_predecessor(raw, count, None, consumer)
    }

    /// The same canonical repeated run with the caller's unchanged-prefix hint.
    /// Advisory provenance changes no chunk identity, partition or repeat count.
    pub fn push_repeated_chunk_with_predecessor(
        &mut self,
        raw: &[u8],
        count: u64,
        predecessor: Option<ObjectId>,
        consumer: &mut dyn FinalizedConsumer,
    ) -> ContentResult<Option<ObjectId>> {
        if count == 0 {
            return Ok(None);
        }
        let mut object = FinalizedObject::new(ObjectRole::Chunk, encode_chunk_object(raw)?)?;
        if let Some(predecessor) = predecessor {
            let mut hints = AdvisoryPredecessors::new();
            hints.push(predecessor, PredecessorProvenance::UnchangedPrefix)?;
            object = object.with_predecessors(hints);
        }
        let id = object.id();
        let extent = ExtentSlice::new(id, 0, raw.len() as u32)?;
        let bytes = (raw.len() as u64)
            .checked_mul(count)
            .ok_or(ContentError::LengthOverflow)?;
        self.build.logical_len = add(self.build.logical_len, bytes)?;
        consumer.accept(object)?;
        self.build.chunks = add(self.build.chunks, 1)?;
        self.repeat(0, Entry::Extent(extent), count, consumer)?;
        Ok(Some(id))
    }

    fn repeat(
        &mut self,
        level: usize,
        entry: Entry,
        mut count: u64,
        consumer: &mut dyn FinalizedConsumer,
    ) -> ContentResult<()> {
        if count == 0 {
            return Ok(());
        }
        if level > usize::from(MAX_LEVEL) {
            return Err(ContentError::MappingDepthExceeded);
        }
        while self.levels.len() <= level {
            self.levels.push(Pending::Children(Vec::new()));
        }
        // Flush the old possibly mixed prefix through the ordinary path. Each
        // iteration consumes a full page of that prefix; it is bounded by U.
        let mut old = self.levels[level].len();
        while old != 0 && count != 0 {
            let take = count.min((self.flush_at + 1 - self.levels[level].len()) as u64);
            self.extend(level, entry, take as usize)?;
            count -= take;
            if self.levels[level].len() > self.flush_at {
                self.flush_streaming(consumer, level)?;
                old = old.saturating_sub(MAX_ENTRIES);
            } else {
                return Ok(());
            }
        }
        let total = add(self.levels[level].len() as u64, count)?;
        let pages = total
            .saturating_sub(self.flush_at as u64)
            .div_ceil(MAX_ENTRIES as u64);
        let tail = total - pages * MAX_ENTRIES as u64;
        // Remaining entries are all the same. Only one canonical full page is
        // necessary, even if ordinary streaming emits it billions of times.
        match &mut self.levels[level] {
            Pending::Extents(entries) => entries.clear(),
            Pending::Children(entries) => entries.clear(),
        }
        self.extend(level, entry, tail as usize)?;
        if pages != 0 {
            let node = uniform_node(entry, level)?;
            let summary = emit_node(consumer, &node, &mut self.build, false)?;
            self.repeat(level + 1, Entry::Child(summary), pages, consumer)?;
        }
        Ok(())
    }

    fn extend(&mut self, level: usize, entry: Entry, count: usize) -> ContentResult<()> {
        match (&mut self.levels[level], entry) {
            (Pending::Extents(entries), Entry::Extent(extent)) => {
                entries.extend(std::iter::repeat(extent).take(count))
            }
            (Pending::Children(entries), Entry::Child(child)) => {
                entries.extend(std::iter::repeat(child).take(count))
            }
            _ => return Err(ContentError::InvalidRecord("repeated builder role")),
        }
        self.note_pending();
        Ok(())
    }
}

fn uniform_node(entry: Entry, level: usize) -> ContentResult<ExtentNode> {
    match entry {
        Entry::Extent(extent) => Ok(ExtentNode::Leaf {
            subtree_logical_bytes: u64::from(extent.logical_length()) * MAX_ENTRIES as u64,
            extents: vec![extent; MAX_ENTRIES],
        }),
        Entry::Child(child) => {
            let mut bytes = 0;
            let mut extents = 0;
            let mut children = Vec::with_capacity(MAX_ENTRIES);
            for _ in 0..MAX_ENTRIES {
                bytes = add(bytes, child.bytes)?;
                extents = add(extents, child.extents)?;
                children.push(ChildDescriptor {
                    cumulative_logical_end: bytes,
                    cumulative_extent_end: extents,
                    child_object_id: child.id,
                });
            }
            Ok(ExtentNode::Branch {
                level: level as u8,
                subtree_logical_bytes: bytes,
                subtree_extent_count: extents,
                children,
            })
        }
    }
}
