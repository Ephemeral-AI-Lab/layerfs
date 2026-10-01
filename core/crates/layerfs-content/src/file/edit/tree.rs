//! Stored-node localized edit: split, concatenate and unchanged-subtree reuse.
//!
//! One edit is four operations over summaries: split the immutable base at the
//! replacement start, split the remainder at the end of the deleted range,
//! concatenate the replacement subtree between them, and emit only the nodes that
//! actually changed. A summary is either a **stored subtree** (identity plus
//! checked size, extent and level, read only when a split or join needs its
//! boundary) or an **owned unfinished node** created by this operation.
//!
//! This is the reference algorithm, not a mapping rebuild: an untouched subtree is
//! referenced by its stored identity and its bytes are never read again, so a
//! localized edit costs the affected paths and the join boundaries, never the whole
//! retained mapping.

use super::temporary::TemporarySummary;
use crate::error::{ContentError, ContentResult};
use crate::file::mapping::{
    decode_node_with_context, encode_node, ChildDescriptor, ExtentNode, ExtentSlice, NodeSummary,
    PageCache, MAX_ENTRIES, MAX_LEVEL,
};
use crate::object::{
    AuthenticatedObjects, FinalizedConsumer, FinalizedObject, ObjectId, ObjectRole,
};

/// Aggregate framed metadata ceiling for drafts and associated ownership facts.
///
/// Bodies, references, counts, jobs and publication facts share this ceiling.
/// The supplied native profile keeps them on metadata-only backing; the explicit
/// compatibility profile retains them with a separate container allowance.
pub const EDIT_DEFERRED_LIMIT: usize = 8 * 1024 * 1024 - 1;
/// Work one localized edit actually performed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EditCounters {
    /// Stored or owned nodes read.
    pub nodes_read: u64,
    /// Mapping nodes this operation created and published.
    ///
    /// One per distinct mapping object the commit walk emits, so it equals the
    /// number of mapping objects the consumer received. A draft a later split or
    /// join superseded is never counted, because it is never emitted.
    pub nodes_created: u64,
    /// Payload objects created by this operation.
    pub payloads_created: u64,
    /// Payload bytes created.
    pub payload_bytes: u64,
    /// Peak unfinished-body logical charge, excluding associated metadata and spare capacity.
    pub peak_deferred_bytes: usize,
    /// Peak admitted aggregate draft/ownership metadata, independent of payload bytes.
    pub peak_draft_metadata_bytes: usize,
}

/// Namespace tag of one operation-local draft key.
///
/// A key is not an identity: the identity of a mapping page is the digest of its
/// canonical bytes, and those bytes do not exist until the page is final. Until
/// then a page this operation built is addressed by a tag plus a counter, so the
/// mapping is injective, reversible in constant time and needs no table. No
/// stored object carries the tag - a stored identity is such a digest, and this
/// tag is not one.
const DRAFT_KEY_TAG: [u8; 24] = *b"layerfs-edit-draft-key\0\0";

enum DraftOwner<'a> {
    Resident(super::draft_resident::ResidentDrafts),
    Supplied(&'a mut dyn super::draft_port::DraftState),
}
impl DraftOwner<'_> {
    fn state(&mut self) -> &mut dyn super::draft_port::DraftState {
        match self {
            Self::Resident(state) => state,
            Self::Supplied(state) => *state,
        }
    }
    fn stats(&self) -> super::draft_port::DraftStats {
        match self {
            Self::Resident(state) => super::draft_port::DraftState::stats(state),
            Self::Supplied(state) => state.stats(),
        }
    }
}

/// Localized boundary work over one admitted draft authority, not growing maps.
pub struct EditObjects<'a> {
    reader: &'a dyn AuthenticatedObjects,
    consumer: &'a mut dyn FinalizedConsumer,
    pages: &'a mut PageCache,
    owner: DraftOwner<'a>,
    selected: Option<ObjectId>,
    next_key: u32,
    counters: EditCounters,
}
impl<'a> EditObjects<'a> {
    /// Explicit resident compatibility authority for independent C1 callers.
    pub fn new(
        reader: &'a dyn AuthenticatedObjects,
        consumer: &'a mut dyn FinalizedConsumer,
        pages: &'a mut PageCache,
    ) -> Self {
        Self {
            reader,
            consumer,
            pages,
            owner: DraftOwner::Resident(super::draft_resident::ResidentDrafts::new()),
            selected: None,
            next_key: 0,
            counters: EditCounters::default(),
        }
    }
    /// The same split/join algorithm over a captured supplied metadata authority.
    pub fn supplied(
        reader: &'a dyn AuthenticatedObjects,
        consumer: &'a mut dyn FinalizedConsumer,
        pages: &'a mut PageCache,
        state: &'a mut dyn super::draft_port::DraftState,
    ) -> ContentResult<Self> {
        state.capacity()?.validated()?;
        Ok(Self {
            reader,
            consumer,
            pages,
            owner: DraftOwner::Supplied(state),
            selected: None,
            next_key: 0,
            counters: EditCounters::default(),
        })
    }
    /// Acknowledged construction work, including full associated metadata peak.
    pub fn counters(&self) -> EditCounters {
        EditCounters {
            peak_deferred_bytes: self.owner.stats().peak_deferred_body_bytes,
            peak_draft_metadata_bytes: self.owner.stats().peak_bytes,
            ..self.counters
        }
    }
    /// Current admitted metadata, including parent/jobs/resolution/emission owners.
    pub fn charged_bytes(&self) -> usize {
        self.owner.stats().bytes
    }
    /// One boundary node, with exact summary/context checks.
    pub fn load_node(&mut self, summary: NodeSummary, root: bool) -> ContentResult<ExtentNode> {
        let mut pending_page = None;
        let node = if let Some(canonical) = self.pages.get(summary.id, root) {
            decode_node_with_context(canonical, root)?
        } else {
            self.counters.nodes_read = self.counters.nodes_read.saturating_add(1);
            match self.owner.state().get(summary.id)? {
                Some(record) => record.node(root)?,
                None => {
                    let canonical = self.reader.read_canonical(summary.id)?;
                    let (page, node) = PageCache::decode_owned(canonical, root)?;
                    pending_page = Some(page);
                    node
                }
            }
        };
        if node.level() != summary.level
            || node.logical_len() != summary.bytes
            || node.extent_count() != summary.extents
        {
            return Err(ContentError::InvalidRecord("extent summary"));
        }
        // Supplied coverage is part of admission: a grammar-valid page with a
        // mismatched parent summary must not mutate or evict the retained cache.
        if let Some(page) = pending_page {
            self.pages.retain(summary.id, page)?;
        }
        Ok(node)
    }
    /// Store one private decoded draft; no canonical encoding or hash occurs here.
    pub(crate) fn hold_node(&mut self, node: &ExtentNode) -> ContentResult<TemporarySummary> {
        let id = self.next_draft_key()?;
        self.owner
            .state()
            .hold(id, super::draft_record::DraftRecord::Node(node.clone()))?;
        self.temporary(NodeSummary {
            id,
            bytes: node.logical_len(),
            extents: node.extent_count(),
            level: node.level(),
        })
    }
    fn hold_page(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.owner
            .state()
            .hold(object.id(), super::draft_record::DraftRecord::Page(object))
    }
    /// Transfer the exact selected root and drain advancing zero/revived jobs.
    pub(crate) fn settle(&mut self, live: &TemporarySummary) -> ContentResult<()> {
        self.owner.state().select_root(self.selected, live.id)?;
        self.selected = Some(live.id);
        while let Some(job) = self.owner.state().next_job()? {
            self.owner.state().retire_job(job)?;
        }
        Ok(())
    }
    /// Acquire a moved temporary name before exposing it to boundary work.
    pub(crate) fn temporary(&mut self, summary: NodeSummary) -> ContentResult<TemporarySummary> {
        if std::mem::size_of::<TemporarySummary>() != std::mem::size_of::<NodeSummary>() {
            return Err(ContentError::InvalidRecord(
                "draft temporary compiled shape",
            ));
        }
        self.owner.state().retain_temporaries(&[summary.id])?;
        Ok(TemporarySummary(summary))
    }
    fn child_temporaries(
        &mut self,
        children: &[ChildDescriptor],
        level: u8,
    ) -> ContentResult<Vec<TemporarySummary>> {
        let summaries = child_summaries(children, level)?;
        for page in summaries.chunks(32) {
            let mut ids = [summaries[0].id; 32];
            for (slot, summary) in ids.iter_mut().zip(page) {
                *slot = summary.id;
            }
            self.owner.state().retain_temporaries(&ids[..page.len()])?;
        }
        Ok(summaries.into_iter().map(TemporarySummary).collect())
    }
    fn release(&mut self, summary: TemporarySummary) -> ContentResult<()> {
        self.owner.state().supersede(summary.id, self.selected)?;
        if self.selected == Some(summary.id) {
            self.selected = None;
        }
        Ok(())
    }
    fn transfer_children(&mut self, children: Vec<TemporarySummary>) -> ContentResult<()> {
        for page in children.chunks(32) {
            let mut ids = [children[0].id; 32];
            for (slot, summary) in ids.iter_mut().zip(page) {
                *slot = summary.id;
            }
            self.owner.state().release_temporaries(&ids[..page.len()])?;
        }
        Ok(())
    }
    fn next_draft_key(&mut self) -> ContentResult<ObjectId> {
        let sequence = self.next_key;
        self.next_key = sequence
            .checked_add(1)
            .ok_or(ContentError::LengthOverflow)?;
        let mut bytes = [0; 32];
        bytes[..DRAFT_KEY_TAG.len()].copy_from_slice(&DRAFT_KEY_TAG);
        bytes[DRAFT_KEY_TAG.len()..].copy_from_slice(&u64::from(sequence).to_be_bytes());
        ObjectId::from_bytes(&bytes)
    }
    /// Payload ownership moves directly to the consumer; scratch never sees it.
    pub fn publish_payload(&mut self, object: FinalizedObject) -> ContentResult<ObjectId> {
        let id = object.id();
        let length = object.canonical_len();
        self.consumer.accept(object)?;
        self.counters.payloads_created = self.counters.payloads_created.saturating_add(1);
        self.counters.payload_bytes = self.counters.payload_bytes.saturating_add(length as u64);
        Ok(id)
    }
    /// Retire required logical metadata before emitting the file state.
    pub(crate) fn finish(&mut self, mapping: TemporarySummary) -> ContentResult<ObjectId> {
        let summary = mapping.summary();
        let root_id = self.commit(mapping)?;
        crate::file::mapping::emit_file_state(
            self.consumer,
            NodeSummary {
                id: root_id,
                ..summary
            },
        )
    }
    /// Children-first publication, with exact paged duplicate/resolution facts.
    pub(crate) fn commit(&mut self, root: TemporarySummary) -> ContentResult<ObjectId> {
        self.settle(&root)?;
        self.owner.state().release_temporaries(&[root.id])?;
        let result = self.commit_node(root.summary(), true, 0);
        if result.is_err() {
            self.owner.state().abandon();
        }
        let id = result?;
        self.owner.state().finish()?;
        Ok(id)
    }
    fn commit_node(
        &mut self,
        summary: NodeSummary,
        root: bool,
        depth: u8,
    ) -> ContentResult<ObjectId> {
        if depth > MAX_LEVEL {
            return Err(ContentError::MappingDepthExceeded);
        }
        if let Some(id) = self.owner.state().resolved(summary.id)? {
            return Ok(id);
        }
        let Some(record) = self.owner.state().get(summary.id)? else {
            return Ok(summary.id);
        };
        let object = match record {
            super::draft_record::DraftRecord::Page(object) => {
                if object.role() == ObjectRole::ExtentBranch {
                    let node = decode_node_with_context(object.canonical(), root)?;
                    if let ExtentNode::Branch {
                        level, children, ..
                    } = node
                    {
                        for child in child_summaries(&children, level - 1)? {
                            self.commit_node(child, false, depth + 1)?;
                        }
                    }
                }
                object
            }
            super::draft_record::DraftRecord::Node(mut node) => {
                if let ExtentNode::Branch {
                    level, children, ..
                } = &mut node
                {
                    for (index, child) in child_summaries(children, *level - 1)?
                        .into_iter()
                        .enumerate()
                    {
                        children[index].child_object_id =
                            self.commit_node(child, false, depth + 1)?;
                    }
                }
                let role = if node.level() == 0 {
                    ObjectRole::ExtentLeaf
                } else {
                    ObjectRole::ExtentBranch
                };
                FinalizedObject::new(role, encode_node(&node, root)?)?
                    .with_references(node.references())
            }
        };
        let id = object.id();
        if self.owner.state().begin_emission(summary.id, id)? {
            self.consumer.accept(object)?;
            self.counters.nodes_created = self.counters.nodes_created.saturating_add(1);
        }
        self.owner.state().accepted(summary.id, id)?;
        Ok(id)
    }
}

/// Borrowed consumer view that routes every object into one [`EditObjects`].
pub struct DeferredSink<'a, 'b> {
    objects: &'b mut EditObjects<'a>,
}

impl<'a, 'b> DeferredSink<'a, 'b> {
    /// Wraps the operation state as a consumer.
    pub fn new(objects: &'b mut EditObjects<'a>) -> Self {
        Self { objects }
    }
}

impl FinalizedConsumer for DeferredSink<'_, '_> {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        // A payload named by a surviving extent is reachable the moment it is
        // created; a page is held as this operation's unfinished node and is
        // published only if the final tree reaches it.
        match object.role() {
            ObjectRole::Chunk => {
                self.objects.publish_payload(object)?;
                Ok(())
            }
            _ => self.objects.hold_page(object),
        }
    }
}

/// Summaries of a branch's children, recovering each child's own totals.
pub fn child_summaries(children: &[ChildDescriptor], level: u8) -> ContentResult<Vec<NodeSummary>> {
    let mut prior_bytes = 0_u64;
    let mut prior_extents = 0_u64;
    let mut summaries = Vec::with_capacity(children.len());
    for child in children {
        summaries.push(NodeSummary {
            id: child.child_object_id,
            bytes: child
                .cumulative_logical_end
                .checked_sub(prior_bytes)
                .ok_or(ContentError::NonCanonicalOrdering)?,
            extents: child
                .cumulative_extent_end
                .checked_sub(prior_extents)
                .ok_or(ContentError::NonCanonicalOrdering)?,
            level,
        });
        prior_bytes = child.cumulative_logical_end;
        prior_extents = child.cumulative_extent_end;
    }
    Ok(summaries)
}

/// Merges adjoining slices of one payload, which the canonical partition forbids
/// from staying separate.
///
/// The pairwise rule lives in [`crate::file::edit::coalesce_adjacent`] and is used
/// here rather than repeated: a payload's logical length is bounded by the chunk
/// grammar, so a refused merge is a pair that is not contiguous, never an overflow.
pub fn coalesce(extents: &mut Vec<ExtentSlice>) -> ContentResult<()> {
    let mut index = 1;
    while index < extents.len() {
        match crate::file::edit::coalesce_adjacent(extents[index - 1], extents[index]) {
            Some(merged) => {
                extents[index - 1] = merged;
                extents.remove(index);
            }
            None => index += 1,
        }
    }
    Ok(())
}

/// Releases the unfinished nodes of a subtree the caller will not use.
///
/// A replaced range is a zero-candidate already covered by an exact detached job.
/// Final root transfer at settle retires it only when all real links are zero;
/// shared drafts and the prior selected root remain owned until that boundary.
pub(crate) fn discard(
    objects: &mut EditObjects<'_>,
    summary: Option<TemporarySummary>,
) -> ContentResult<()> {
    if let Some(summary) = summary {
        objects.release(summary)?;
    }
    Ok(())
}

/// Splits one subtree at `offset`, in its own coordinates.
///
/// `root_context` says whether this subtree is the whole tree, which decides the
/// page-partition checks; it is not the position of the edit.
pub(crate) fn split(
    objects: &mut EditObjects<'_>,
    root: TemporarySummary,
    offset: u64,
    root_context: bool,
) -> ContentResult<(Option<TemporarySummary>, Option<TemporarySummary>)> {
    if offset > root.bytes {
        return Err(ContentError::InvalidRange {
            start: offset,
            end: offset,
            length: root.bytes,
        });
    }
    if offset == 0 {
        return Ok((None, Some(root)));
    }
    if offset == root.bytes {
        return Ok((Some(root), None));
    }
    // An interior split replaces this node with the two halves it returns, so the
    // draft it may hold is superseded and released. Every child it was built from
    // stays live: it is re-referenced by one of the halves or by the recursion.
    let node = objects.load_node(root.summary(), root_context)?;
    let child_pins = match &node {
        ExtentNode::Branch {
            level, children, ..
        } => Some(objects.child_temporaries(children, level - 1)?),
        ExtentNode::Leaf { .. } => None,
    };
    objects.release(root)?;
    match node {
        ExtentNode::Leaf { extents, .. } => {
            let mut left = Vec::new();
            let mut right = Vec::new();
            let mut logical = 0_u64;
            for extent in extents {
                let end = logical
                    .checked_add(u64::from(extent.logical_length()))
                    .ok_or(ContentError::LengthOverflow)?;
                if end <= offset {
                    left.push(extent);
                } else if logical >= offset {
                    right.push(extent);
                } else {
                    let left_len = u32::try_from(offset - logical)
                        .map_err(|_| ContentError::LengthOverflow)?;
                    left.push(ExtentSlice::new(
                        extent.payload_object_id(),
                        extent.source_offset(),
                        left_len,
                    )?);
                    right.push(ExtentSlice::new(
                        extent.payload_object_id(),
                        extent
                            .source_offset()
                            .checked_add(left_len)
                            .ok_or(ContentError::LengthOverflow)?,
                        extent.logical_length() - left_len,
                    )?);
                }
                logical = end;
            }
            Ok((
                Some(emit_leaf(objects, left)?),
                Some(emit_leaf(objects, right)?),
            ))
        }
        ExtentNode::Branch { children, .. } => {
            let index = children.partition_point(|child| child.cumulative_logical_end < offset);
            let before = if index == 0 {
                0
            } else {
                children[index - 1].cumulative_logical_end
            };
            let mut summaries = child_pins.unwrap();
            let suffix = summaries.split_off(index + 1);
            let child = summaries
                .pop()
                .ok_or(ContentError::InvalidRecord("split child index"))?;
            let (child_left, child_right) = split(objects, child, offset - before, false)?;
            let prefix = root_from_children(objects, summaries)?;
            let suffix = root_from_children(objects, suffix)?;
            Ok((
                concat_optional(objects, prefix, child_left)?,
                concat_optional(objects, child_right, suffix)?,
            ))
        }
    }
}

/// Joins two optional subtrees, keeping both sides when either is absent.
pub(crate) fn concat_optional(
    objects: &mut EditObjects<'_>,
    left: Option<TemporarySummary>,
    right: Option<TemporarySummary>,
) -> ContentResult<Option<TemporarySummary>> {
    match (left, right) {
        (None, value) | (value, None) => Ok(value),
        (Some(left), Some(right)) => concat(objects, left, right).map(Some),
    }
}

/// Joins two subtrees, which may have different heights.
pub(crate) fn concat(
    objects: &mut EditObjects<'_>,
    left: TemporarySummary,
    right: TemporarySummary,
) -> ContentResult<TemporarySummary> {
    concat_inner(
        objects,
        JoinSide::summary(left),
        JoinSide::summary(right),
        0,
    )
}

/// One side of a join: its summary, and the node when the caller already has it.
///
/// A caller that has just decoded a node under its **non-root** context can hand
/// it here instead of letting the join decode it again under the weaker root
/// context. The stricter check is kept and the duplicate read disappears; a side
/// without a node is loaded exactly as before.
struct JoinSide {
    summary: TemporarySummary,
    node: Option<ExtentNode>,
}

impl JoinSide {
    /// A side that has not been decoded yet.
    fn summary(summary: TemporarySummary) -> Self {
        Self {
            summary,
            node: None,
        }
    }

    /// A side the caller already decoded and checked.
    fn decoded(summary: TemporarySummary, node: ExtentNode) -> Self {
        Self {
            summary,
            node: Some(node),
        }
    }

    /// The decoded node, from the caller when it has one.
    fn take(&mut self, objects: &mut EditObjects<'_>, root: bool) -> ContentResult<ExtentNode> {
        match self.node.take() {
            Some(node) => Ok(node),
            None => objects.load_node(self.summary.summary(), root),
        }
    }
}

fn concat_inner(
    objects: &mut EditObjects<'_>,
    mut left: JoinSide,
    mut right: JoinSide,
    depth: u8,
) -> ContentResult<TemporarySummary> {
    if depth > MAX_LEVEL {
        return Err(ContentError::MappingDepthExceeded);
    }
    if left.summary.level == right.summary.level {
        let left_node = left.take(objects, true)?;
        let right_node = right.take(objects, true)?;
        let mut left_pins = match &left_node {
            ExtentNode::Branch {
                level, children, ..
            } => objects.child_temporaries(children, level - 1)?,
            ExtentNode::Leaf { .. } => Vec::new(),
        };
        let right_pins = match &right_node {
            ExtentNode::Branch {
                level, children, ..
            } => objects.child_temporaries(children, level - 1)?,
            ExtentNode::Leaf { .. } => Vec::new(),
        };
        // A join replaces both inputs: a merged page when they are leaves, and a
        // rebuilt page when they are branches. Their children stay live.
        objects.release(left.summary)?;
        objects.release(right.summary)?;
        return match (left_node, right_node) {
            (ExtentNode::Leaf { mut extents, .. }, ExtentNode::Leaf { extents: other, .. }) => {
                extents.extend(other);
                coalesce(&mut extents)?;
                root_from_extents(objects, extents)
            }
            (ExtentNode::Branch { .. }, ExtentNode::Branch { .. }) => {
                left_pins.extend(right_pins);
                root_from_children(objects, left_pins)?
                    .ok_or(ContentError::InvalidRecord("empty branch concat"))
            }
            _ => Err(ContentError::WrongLogicalRole),
        };
    }
    if left.summary.level > right.summary.level {
        let ExtentNode::Branch {
            level, children, ..
        } = left.take(objects, true)?
        else {
            return Err(ContentError::WrongLogicalRole);
        };
        let mut summaries = objects.child_temporaries(&children, level - 1)?;
        // The taller side is dismantled into a rebuilt prefix and one descending
        // boundary; every child it held stays live in one of the two.
        objects.release(left.summary)?;
        let last = summaries
            .pop()
            .ok_or(ContentError::InvalidRecord("empty branch"))?;
        // The boundary child is checked under its **non-root** context, the
        // stricter one, and the decoded node travels into the join instead of
        // being read and decoded a second time.
        let last_node = objects.load_node(last.summary(), false)?;
        let prefix = root_from_children(objects, summaries)?;
        let boundary = concat_inner(
            objects,
            JoinSide::decoded(last, last_node),
            right,
            depth + 1,
        )?;
        return match prefix {
            None => Ok(boundary),
            Some(prefix) if prefix.level == boundary.level => concat_inner(
                objects,
                JoinSide::summary(prefix),
                JoinSide::summary(boundary),
                depth + 1,
            ),
            Some(prefix) if prefix.level.checked_add(1) == Some(boundary.level) => {
                let ExtentNode::Branch {
                    level, children, ..
                } = objects.load_node(boundary.summary(), true)?
                else {
                    return Err(ContentError::WrongLogicalRole);
                };
                let mut summaries = objects.child_temporaries(&children, level - 1)?;
                // The loaded page is superseded by the branch that re-hosts its
                // children; the prefix becomes the first of those children and
                // stays live.
                objects.release(boundary)?;
                summaries.insert(0, prefix);
                root_from_children(objects, summaries)?
                    .ok_or(ContentError::InvalidRecord("empty concat"))
            }
            Some(prefix) if boundary.level.checked_add(1) == Some(prefix.level) => {
                let ExtentNode::Branch {
                    level, children, ..
                } = objects.load_node(prefix.summary(), true)?
                else {
                    return Err(ContentError::WrongLogicalRole);
                };
                let mut summaries = objects.child_temporaries(&children, level - 1)?;
                objects.release(prefix)?;
                summaries.push(boundary);
                root_from_children(objects, summaries)?
                    .ok_or(ContentError::InvalidRecord("empty concat"))
            }
            Some(_) => Err(ContentError::InvalidRecord("left concat levels")),
        };
    }
    let ExtentNode::Branch {
        level, children, ..
    } = right.take(objects, true)?
    else {
        return Err(ContentError::WrongLogicalRole);
    };
    let mut summaries = objects.child_temporaries(&children, level - 1)?;
    // Mirror of the taller-left case: the taller side is dismantled into a
    // descending boundary and one rebuilt suffix.
    objects.release(right.summary)?;
    if summaries.is_empty() {
        return Err(ContentError::InvalidRecord("empty branch"));
    }
    let first = summaries.remove(0);
    // As above: the stricter non-root check stays, and the join reuses the node.
    let first_node = objects.load_node(first.summary(), false)?;
    let boundary = concat_inner(
        objects,
        left,
        JoinSide::decoded(first, first_node),
        depth + 1,
    )?;
    let suffix = root_from_children(objects, summaries)?;
    match suffix {
        None => Ok(boundary),
        Some(suffix) if suffix.level == boundary.level => concat_inner(
            objects,
            JoinSide::summary(boundary),
            JoinSide::summary(suffix),
            depth + 1,
        ),
        Some(suffix) if suffix.level.checked_add(1) == Some(boundary.level) => {
            let ExtentNode::Branch {
                level, children, ..
            } = objects.load_node(boundary.summary(), true)?
            else {
                return Err(ContentError::WrongLogicalRole);
            };
            let mut summaries = objects.child_temporaries(&children, level - 1)?;
            // The loaded page is superseded by the branch that re-hosts its
            // children; the suffix becomes the last of those children and stays
            // live.
            objects.release(boundary)?;
            summaries.push(suffix);
            root_from_children(objects, summaries)?
                .ok_or(ContentError::InvalidRecord("empty concat"))
        }
        Some(suffix) if boundary.level.checked_add(1) == Some(suffix.level) => {
            let ExtentNode::Branch {
                level, children, ..
            } = objects.load_node(suffix.summary(), true)?
            else {
                return Err(ContentError::WrongLogicalRole);
            };
            let mut summaries = objects.child_temporaries(&children, level - 1)?;
            // The suffix page is rebuilt around the boundary, which stays live as
            // its first child.
            objects.release(suffix)?;
            summaries.insert(0, boundary);
            root_from_children(objects, summaries)?
                .ok_or(ContentError::InvalidRecord("empty concat"))
        }
        Some(_) => Err(ContentError::InvalidRecord("right concat levels")),
    }
}

/// Root for a whole list of extents, half-partitioning when the page is full.
pub(crate) fn root_from_extents(
    objects: &mut EditObjects<'_>,
    extents: Vec<ExtentSlice>,
) -> ContentResult<TemporarySummary> {
    if extents.len() <= MAX_ENTRIES {
        return emit_leaf(objects, extents);
    }
    let half = extents.len() / 2;
    let left = emit_leaf(objects, extents[..half].to_vec())?;
    let right = emit_leaf(objects, extents[half..].to_vec())?;
    root_from_children(objects, vec![left, right])?
        .ok_or(ContentError::InvalidRecord("empty extent root"))
}

/// Root for a whole list of children, collapsing one child and half-partitioning
/// when the page is full.
pub(crate) fn root_from_children(
    objects: &mut EditObjects<'_>,
    mut children: Vec<TemporarySummary>,
) -> ContentResult<Option<TemporarySummary>> {
    if children.is_empty() {
        return Ok(None);
    }
    if children.len() == 1 {
        return Ok(children.pop());
    }
    if children.len() <= MAX_ENTRIES {
        return emit_branch(objects, children).map(Some);
    }
    let half = children.len() / 2;
    let right_children = children.split_off(half);
    let left = emit_branch(objects, children)?;
    let right = emit_branch(objects, right_children)?;
    emit_branch(objects, vec![left, right]).map(Some)
}

/// Emits one leaf page from its extents.
pub(crate) fn emit_leaf(
    objects: &mut EditObjects<'_>,
    extents: Vec<ExtentSlice>,
) -> ContentResult<TemporarySummary> {
    let bytes = extents.iter().try_fold(0_u64, |sum, extent| {
        sum.checked_add(u64::from(extent.logical_length()))
            .ok_or(ContentError::LengthOverflow)
    })?;
    objects.hold_node(&ExtentNode::Leaf {
        subtree_logical_bytes: bytes,
        extents,
    })
}

/// Emits one branch page from its children.
pub(crate) fn emit_branch(
    objects: &mut EditObjects<'_>,
    children: Vec<TemporarySummary>,
) -> ContentResult<TemporarySummary> {
    if children.is_empty() {
        return Err(ContentError::InvalidRecord("empty branch"));
    }
    let level = children[0]
        .level
        .checked_add(1)
        .ok_or(ContentError::MappingDepthExceeded)?;
    if children.iter().any(|child| child.level + 1 != level) {
        return Err(ContentError::InvalidRecord("mixed branch levels"));
    }
    let mut bytes = 0_u64;
    let mut extents = 0_u64;
    let mut descriptors = Vec::with_capacity(children.len());
    for child in &children {
        bytes = bytes
            .checked_add(child.bytes)
            .ok_or(ContentError::LengthOverflow)?;
        extents = extents
            .checked_add(child.extents)
            .ok_or(ContentError::LengthOverflow)?;
        descriptors.push(ChildDescriptor {
            cumulative_logical_end: bytes,
            cumulative_extent_end: extents,
            child_object_id: child.id,
        });
    }
    let root = objects.hold_node(&ExtentNode::Branch {
        level,
        subtree_logical_bytes: bytes,
        subtree_extent_count: extents,
        children: descriptors,
    })?;
    objects.transfer_children(children)?;
    Ok(root)
}
