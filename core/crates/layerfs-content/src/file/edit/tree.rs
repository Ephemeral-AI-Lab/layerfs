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

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{ContentError, ContentResult};
use crate::file::mapping::{
    decode_file_state, decode_node_with_context, encode_node, ChildDescriptor, ExtentNode,
    ExtentSlice, FileState, NodeSummary, MAX_ENTRIES, MAX_LEVEL,
};
use crate::object::{
    AuthenticatedObjects, FinalizedConsumer, FinalizedObject, ObjectId, ObjectRole,
};

/// Largest encoded bytes one operation may hold for its own unfinished nodes.
///
/// Nodes are tiny relative to the payload they map (one 128-entry page per four
/// megabytes of chunked content), so this ceiling only ever binds on pathological
/// input, and it binds by failing the operation rather than by dropping state.
pub const EDIT_DEFERRED_LIMIT: usize = 8 * 1024 * 1024 - 1;
/// Charged overhead per deferred object beyond its encoded bytes.
const DEFERRED_OBJECT_OVERHEAD: usize = 128;

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
    /// Largest number of unfinished-node bytes held at once.
    pub peak_deferred_bytes: usize,
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

/// One unfinished node this operation owns.
///
/// A draft holds exactly one representation and never two. A page the canonical
/// builder emitted is already framed, immutable, identified and referenced, so it
/// is held as the finalized object it is: final emission is a move, and its bytes
/// are decoded only when a split or join actually needs its boundary. A node this
/// operation built from decoded parts is held decoded and encoded exactly once,
/// in [`EditObjects::commit_node`], after it is proven final.
enum Draft {
    /// A page the canonical builder already produced.
    Page(FinalizedObject),
    /// A node this operation built, held decoded.
    Node(ExtentNode),
}

impl Draft {
    /// Bytes this draft charges against the operation's unfinished-node ceiling.
    ///
    /// A held page charges its canonical bytes; a held decoded node charges the
    /// decoded entries it actually occupies, which is the number that bounds the
    /// operation's memory.
    fn charge(&self) -> usize {
        match self {
            Self::Page(object) => object
                .canonical_len()
                .saturating_add(DEFERRED_OBJECT_OVERHEAD),
            Self::Node(node) => {
                let entries = match node {
                    ExtentNode::Leaf { extents, .. } => extents
                        .len()
                        .saturating_mul(std::mem::size_of::<ExtentSlice>()),
                    ExtentNode::Branch { children, .. } => children
                        .len()
                        .saturating_mul(std::mem::size_of::<ChildDescriptor>()),
                };
                std::mem::size_of::<ExtentNode>()
                    .saturating_add(entries)
                    .saturating_add(DEFERRED_OBJECT_OVERHEAD)
            }
        }
    }
}

/// The operation's private object set: unfinished nodes plus the already-stored
/// objects it reads through.
pub struct EditObjects<'a> {
    reader: &'a dyn AuthenticatedObjects,
    consumer: &'a mut dyn FinalizedConsumer,
    drafts: BTreeMap<ObjectId, Draft>,
    /// Draft children each live draft still references.
    parent_refs: BTreeMap<ObjectId, u32>,
    /// Drafts no live draft references.
    detached: BTreeSet<ObjectId>,
    committed: BTreeMap<ObjectId, ObjectId>,
    charged: usize,
    next_key: u32,
    counters: EditCounters,
}

impl<'a> EditObjects<'a> {
    /// Empty operation state over `reader`, publishing to `consumer`.
    pub fn new(
        reader: &'a dyn AuthenticatedObjects,
        consumer: &'a mut dyn FinalizedConsumer,
    ) -> Self {
        Self {
            reader,
            consumer,
            drafts: BTreeMap::new(),
            parent_refs: BTreeMap::new(),
            detached: BTreeSet::new(),
            committed: BTreeMap::new(),
            charged: 0,
            next_key: 0,
            counters: EditCounters::default(),
        }
    }

    /// Work performed so far.
    pub const fn counters(&self) -> EditCounters {
        self.counters
    }

    /// Bytes currently held for this operation's unfinished nodes.
    pub const fn charged_bytes(&self) -> usize {
        self.charged
    }

    /// Reads and decodes one mapping page under its root/non-root context.
    pub fn load_node(&mut self, summary: NodeSummary, root: bool) -> ContentResult<ExtentNode> {
        self.counters.nodes_read = self.counters.nodes_read.saturating_add(1);
        let node = match self.drafts.get(&summary.id) {
            Some(Draft::Node(node)) => node.clone(),
            Some(Draft::Page(object)) => decode_node_with_context(object.canonical(), root)?,
            None => {
                let canonical = self.reader.read_canonical(summary.id)?;
                decode_node_with_context(&canonical, root)?
            }
        };
        if node.level() != summary.level
            || node.logical_len() != summary.bytes
            || node.extent_count() != summary.extents
        {
            return Err(ContentError::InvalidRecord("extent summary"));
        }
        Ok(node)
    }

    /// Holds one node this operation built, decoded, under a fresh draft key.
    ///
    /// The node is not encoded and not hashed here: neither its identity nor its
    /// bytes are decided until the commit walk proves it final. A node that a
    /// later split or join replaces is released and never encoded at all.
    pub fn hold_node(&mut self, node: &ExtentNode) -> ContentResult<NodeSummary> {
        let id = self.next_draft_key()?;
        let draft = Draft::Node(node.clone());
        self.charge_bytes(draft.charge())?;
        self.drafts.insert(id, draft);
        self.detached.insert(id);
        self.retain_children(node)?;
        Ok(NodeSummary {
            id,
            bytes: node.logical_len(),
            extents: node.extent_count(),
            level: node.level(),
        })
    }

    /// Holds one page the canonical builder already emitted.
    ///
    /// The page's bytes, identity and references are final when the builder emits
    /// it, so holding it costs one move and publishing it later costs nothing: no
    /// decode, encode or hash is repeated for a page the builder produced.
    fn hold_page(&mut self, object: FinalizedObject) -> ContentResult<()> {
        let id = object.id();
        if self.drafts.contains_key(&id) {
            return Ok(());
        }
        let charge = object
            .canonical_len()
            .saturating_add(DEFERRED_OBJECT_OVERHEAD);
        // A branch page names the pages it was built from: those are this
        // operation's drafts, and the parent reference is what keeps them alive.
        if object.role() == ObjectRole::ExtentBranch {
            let node = decode_node_with_context(object.canonical(), true)?;
            self.retain_children(&node)?;
        }
        self.charge_bytes(charge)?;
        self.drafts.insert(id, Draft::Page(object));
        self.detached.insert(id);
        Ok(())
    }

    /// Records that a parent being held references each of its draft children.
    fn retain_children(&mut self, node: &ExtentNode) -> ContentResult<()> {
        let ExtentNode::Branch {
            level, children, ..
        } = node
        else {
            return Ok(());
        };
        for child in child_summaries(children, level.saturating_sub(1))? {
            if self.drafts.contains_key(&child.id) {
                *self.parent_refs.entry(child.id).or_insert(0) += 1;
                self.detached.remove(&child.id);
            }
        }
        Ok(())
    }

    /// Releases every draft the operation still holds but its result does not
    /// reference.
    ///
    /// Called once per edit, after the result of that edit is known. Only drafts
    /// with no live draft parent are candidates - the set is maintained as the
    /// operation runs and stays at the size of what is in flight - so this is a
    /// release of what the boundary work already disconnected, not a walk of the
    /// retained tree and not a prune pass over the mapping.
    pub fn settle(&mut self, live: NodeSummary) {
        // One pass collects every candidate the set holds, instead of restarting
        // a linear scan from the beginning after each release - which was
        // quadratic in the detached set. Releasing a draft can detach its
        // children, so the passes repeat until the set only holds the result;
        // each pass strictly reduces the number of live drafts, so the number of
        // passes is bounded by the tree's depth.
        while !self.detached.is_empty() {
            let pending: Vec<ObjectId> = self
                .detached
                .iter()
                .copied()
                .filter(|candidate| *candidate != live.id)
                .collect();
            if pending.is_empty() {
                return;
            }
            for id in pending {
                self.release(id);
            }
        }
    }

    /// Releases one draft and detaches the children it was the last reference to.
    fn release_draft_children(&mut self, draft: &Draft) {
        let children: Vec<ObjectId> = match draft {
            Draft::Node(node) => self.child_ids(node).unwrap_or_default(),
            Draft::Page(object) => {
                if object.role() != ObjectRole::ExtentBranch {
                    Vec::new()
                } else {
                    decode_node_with_context(object.canonical(), true)
                        .ok()
                        .and_then(|node| self.child_ids(&node).ok())
                        .unwrap_or_default()
                }
            }
        };
        for child in children {
            let referenced = self.parent_refs.get(&child).copied().unwrap_or(0);
            if referenced <= 1 {
                self.parent_refs.remove(&child);
                if self.drafts.contains_key(&child) {
                    self.detached.insert(child);
                }
            } else {
                self.parent_refs.insert(child, referenced - 1);
            }
        }
    }

    fn child_ids(&self, node: &ExtentNode) -> ContentResult<Vec<ObjectId>> {
        let ExtentNode::Branch {
            level, children, ..
        } = node
        else {
            return Ok(Vec::new());
        };
        Ok(child_summaries(children, level.saturating_sub(1))?
            .into_iter()
            .map(|child| child.id)
            .collect())
    }

    /// Releases one draft this operation owns, releasing its charge.
    ///
    /// A node a split or a join superseded is unreachable from the final mapping,
    /// so it is never emitted and holding it would only bound the operation by the
    /// number of edits rather than by the tree it ends up with. Releasing a
    /// summary that was already published, or that belongs to the stored
    /// generation, does nothing.
    fn release(&mut self, id: ObjectId) {
        if let Some(draft) = self.drafts.remove(&id) {
            self.charged = self.charged.saturating_sub(draft.charge());
            self.detached.remove(&id);
            self.release_draft_children(&draft);
        }
    }

    fn next_draft_key(&mut self) -> ContentResult<ObjectId> {
        let sequence = self.next_key;
        self.next_key = sequence
            .checked_add(1)
            .ok_or(ContentError::LengthOverflow)?;
        let mut bytes = [0_u8; 32];
        bytes[..DRAFT_KEY_TAG.len()].copy_from_slice(&DRAFT_KEY_TAG);
        bytes[DRAFT_KEY_TAG.len()..].copy_from_slice(&u64::from(sequence).to_be_bytes());
        ObjectId::from_bytes(&bytes)
    }

    fn charge_bytes(&mut self, charge: usize) -> ContentResult<()> {
        let next = self
            .charged
            .checked_add(charge)
            .ok_or(ContentError::LengthOverflow)?;
        if next > EDIT_DEFERRED_LIMIT {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "edit.deferred_nodes",
                limit: EDIT_DEFERRED_LIMIT as u64,
                actual: next as u64,
            });
        }
        self.charged = next;
        self.counters.peak_deferred_bytes = self.counters.peak_deferred_bytes.max(next);
        Ok(())
    }

    /// Publishes one payload object immediately: a payload named by a surviving
    /// extent is reachable by construction.
    pub fn publish_payload(&mut self, object: FinalizedObject) -> ContentResult<ObjectId> {
        let id = object.id();
        let length = object.canonical_len();
        self.consumer.accept(object)?;
        self.counters.payloads_created = self.counters.payloads_created.saturating_add(1);
        self.counters.payload_bytes = self.counters.payload_bytes.saturating_add(length as u64);
        Ok(id)
    }

    /// Publishes every unfinished node the final tree reaches, children first,
    /// then the file state that opens it.
    ///
    /// The state is last because it depends on the mapping root, and the mapping
    /// root on its children: a consumer never sees a parent before its child.
    pub fn finish(&mut self, mapping: NodeSummary) -> ContentResult<ObjectId> {
        let mut published = BTreeSet::new();
        let root_id = self.commit_node(mapping, true, &mut published)?;
        let root = crate::file::mapping::emit_file_state(
            self.consumer,
            NodeSummary {
                id: root_id,
                ..mapping
            },
        )?;
        Ok(root)
    }

    /// Publishes every unfinished node the final tree reaches, children first, and
    /// returns the mapping's real identity.
    ///
    /// A node that a later split or join overwrote is simply never reached, so no
    /// speculative object is ever emitted and no prune pass is needed.
    pub fn commit(&mut self, root: NodeSummary) -> ContentResult<ObjectId> {
        let mut published = BTreeSet::new();
        self.commit_node(root, true, &mut published)
    }

    /// Publishes one reached node and returns its real identity.
    ///
    /// A stored subtree is already published and is returned unchanged. A draft is
    /// proven final by being reached from the final mapping: its children are
    /// committed first, its descriptor identities are patched to the children's
    /// real identities, and only then is it encoded, hashed and published - once.
    fn commit_node(
        &mut self,
        summary: NodeSummary,
        root: bool,
        published: &mut BTreeSet<ObjectId>,
    ) -> ContentResult<ObjectId> {
        let Some(draft) = self.drafts.remove(&summary.id) else {
            // Either a stored subtree, or a draft another reference already
            // committed during this walk: both answer with their identity.
            return Ok(self
                .committed
                .get(&summary.id)
                .copied()
                .unwrap_or(summary.id));
        };
        self.charged = self.charged.saturating_sub(draft.charge());
        let id = match draft {
            Draft::Page(object) => {
                if object.role() == ObjectRole::ExtentBranch {
                    // A branch page publishes its children before itself, so its
                    // descriptors are read once here; a leaf page needs no decode.
                    let node = decode_node_with_context(object.canonical(), root)?;
                    if let ExtentNode::Branch {
                        level, children, ..
                    } = &node
                    {
                        for child in child_summaries(children, level.saturating_sub(1))? {
                            self.commit_node(child, false, published)?;
                        }
                    }
                }
                let id = object.id();
                if published.insert(id) {
                    self.consumer.accept(object)?;
                    self.counters.nodes_created = self.counters.nodes_created.saturating_add(1);
                }
                id
            }
            Draft::Node(mut node) => {
                if let ExtentNode::Branch {
                    level, children, ..
                } = &mut node
                {
                    // This page holds its children by draft key; the descriptor
                    // takes the child's real identity before this page is encoded.
                    let summaries = child_summaries(children, level.saturating_sub(1))?;
                    for (index, child) in summaries.into_iter().enumerate() {
                        let committed = self.commit_node(child, false, published)?;
                        children[index].child_object_id = committed;
                    }
                }
                let role = match node {
                    ExtentNode::Leaf { .. } => ObjectRole::ExtentLeaf,
                    ExtentNode::Branch { .. } => ObjectRole::ExtentBranch,
                };
                let canonical = encode_node(&node)?;
                let references = node.references();
                let object = FinalizedObject::new(role, canonical)?.with_references(references);
                let id = object.id();
                if published.insert(id) {
                    self.consumer.accept(object)?;
                    self.counters.nodes_created = self.counters.nodes_created.saturating_add(1);
                }
                id
            }
        };
        self.committed.insert(summary.id, id);
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
/// A replaced range is not part of the result, so nothing it holds can ever be
/// published: the draft the split created for it is dropped here instead of being
/// carried until the operation ends. Only that node is released - the pages it was
/// split from are shared with the halves that survive.
pub fn discard(objects: &mut EditObjects<'_>, summary: Option<NodeSummary>) {
    if let Some(summary) = summary {
        objects.release(summary.id);
    }
}

/// Splits one subtree at `offset`, in its own coordinates.
///
/// `root_context` says whether this subtree is the whole tree, which decides the
/// page-partition checks; it is not the position of the edit.
pub fn split(
    objects: &mut EditObjects<'_>,
    root: NodeSummary,
    offset: u64,
    root_context: bool,
) -> ContentResult<(Option<NodeSummary>, Option<NodeSummary>)> {
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
    let node = objects.load_node(root, root_context)?;
    objects.release(root.id);
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
        ExtentNode::Branch {
            level, children, ..
        } => {
            let index = children.partition_point(|child| child.cumulative_logical_end < offset);
            let before = if index == 0 {
                0
            } else {
                children[index - 1].cumulative_logical_end
            };
            let summaries = child_summaries(&children, level - 1)?;
            let child = *summaries
                .get(index)
                .ok_or(ContentError::InvalidRecord("split child index"))?;
            let (child_left, child_right) = split(objects, child, offset - before, false)?;
            let prefix = root_from_children(objects, summaries[..index].to_vec())?;
            let suffix = root_from_children(objects, summaries[index + 1..].to_vec())?;
            Ok((
                concat_optional(objects, prefix, child_left)?,
                concat_optional(objects, child_right, suffix)?,
            ))
        }
    }
}

/// Joins two optional subtrees, keeping both sides when either is absent.
pub fn concat_optional(
    objects: &mut EditObjects<'_>,
    left: Option<NodeSummary>,
    right: Option<NodeSummary>,
) -> ContentResult<Option<NodeSummary>> {
    match (left, right) {
        (None, value) | (value, None) => Ok(value),
        (Some(left), Some(right)) => concat(objects, left, right).map(Some),
    }
}

/// Joins two subtrees, which may have different heights.
pub fn concat(
    objects: &mut EditObjects<'_>,
    left: NodeSummary,
    right: NodeSummary,
) -> ContentResult<NodeSummary> {
    concat_inner(objects, left, right, 0)
}

fn concat_inner(
    objects: &mut EditObjects<'_>,
    left: NodeSummary,
    right: NodeSummary,
    depth: u8,
) -> ContentResult<NodeSummary> {
    if depth > MAX_LEVEL {
        return Err(ContentError::MappingDepthExceeded);
    }
    if left.level == right.level {
        let left_node = objects.load_node(left, true)?;
        let right_node = objects.load_node(right, true)?;
        // A join replaces both inputs: a merged page when they are leaves, and a
        // rebuilt page when they are branches. Their children stay live.
        objects.release(left.id);
        objects.release(right.id);
        return match (left_node, right_node) {
            (ExtentNode::Leaf { mut extents, .. }, ExtentNode::Leaf { extents: other, .. }) => {
                extents.extend(other);
                coalesce(&mut extents)?;
                root_from_extents(objects, extents)
            }
            (
                ExtentNode::Branch {
                    level, children, ..
                },
                ExtentNode::Branch {
                    children: other, ..
                },
            ) => {
                let mut summaries = child_summaries(&children, level - 1)?;
                summaries.extend(child_summaries(&other, level - 1)?);
                root_from_children(objects, summaries)?
                    .ok_or(ContentError::InvalidRecord("empty branch concat"))
            }
            _ => Err(ContentError::WrongLogicalRole),
        };
    }
    if left.level > right.level {
        let ExtentNode::Branch {
            level, children, ..
        } = objects.load_node(left, true)?
        else {
            return Err(ContentError::WrongLogicalRole);
        };
        let summaries = child_summaries(&children, level - 1)?;
        // The taller side is dismantled into a rebuilt prefix and one descending
        // boundary; every child it held stays live in one of the two.
        objects.release(left.id);
        let (last, prefix) = summaries
            .split_last()
            .ok_or(ContentError::InvalidRecord("empty branch"))?;
        let last = *last;
        objects.load_node(last, false)?;
        let prefix = root_from_children(objects, prefix.to_vec())?;
        let boundary = concat_inner(objects, last, right, depth + 1)?;
        return match prefix {
            None => Ok(boundary),
            Some(prefix) if prefix.level == boundary.level => {
                concat_inner(objects, prefix, boundary, depth + 1)
            }
            Some(prefix) if prefix.level.checked_add(1) == Some(boundary.level) => {
                let ExtentNode::Branch {
                    level, children, ..
                } = objects.load_node(boundary, true)?
                else {
                    return Err(ContentError::WrongLogicalRole);
                };
                let mut summaries = child_summaries(&children, level - 1)?;
                // The loaded page is superseded by the branch that re-hosts its
                // children; the prefix becomes the first of those children and
                // stays live.
                objects.release(boundary.id);
                summaries.insert(0, prefix);
                root_from_children(objects, summaries)?
                    .ok_or(ContentError::InvalidRecord("empty concat"))
            }
            Some(prefix) if boundary.level.checked_add(1) == Some(prefix.level) => {
                let ExtentNode::Branch {
                    level, children, ..
                } = objects.load_node(prefix, true)?
                else {
                    return Err(ContentError::WrongLogicalRole);
                };
                let mut summaries = child_summaries(&children, level - 1)?;
                summaries.push(boundary);
                root_from_children(objects, summaries)?
                    .ok_or(ContentError::InvalidRecord("empty concat"))
            }
            Some(_) => Err(ContentError::InvalidRecord("left concat levels")),
        };
    }
    let ExtentNode::Branch {
        level, children, ..
    } = objects.load_node(right, true)?
    else {
        return Err(ContentError::WrongLogicalRole);
    };
    let summaries = child_summaries(&children, level - 1)?;
    // Mirror of the taller-left case: the taller side is dismantled into a
    // descending boundary and one rebuilt suffix.
    objects.release(right.id);
    let (first, suffix) = summaries
        .split_first()
        .ok_or(ContentError::InvalidRecord("empty branch"))?;
    let first = *first;
    objects.load_node(first, false)?;
    let boundary = concat_inner(objects, left, first, depth + 1)?;
    let suffix = root_from_children(objects, suffix.to_vec())?;
    match suffix {
        None => Ok(boundary),
        Some(suffix) if suffix.level == boundary.level => {
            concat_inner(objects, boundary, suffix, depth + 1)
        }
        Some(suffix) if suffix.level.checked_add(1) == Some(boundary.level) => {
            let ExtentNode::Branch {
                level, children, ..
            } = objects.load_node(boundary, true)?
            else {
                return Err(ContentError::WrongLogicalRole);
            };
            let mut summaries = child_summaries(&children, level - 1)?;
            // The loaded page is superseded by the branch that re-hosts its
            // children; the suffix becomes the last of those children and stays
            // live.
            objects.release(boundary.id);
            summaries.push(suffix);
            root_from_children(objects, summaries)?
                .ok_or(ContentError::InvalidRecord("empty concat"))
        }
        Some(suffix) if boundary.level.checked_add(1) == Some(suffix.level) => {
            let ExtentNode::Branch {
                level, children, ..
            } = objects.load_node(suffix, true)?
            else {
                return Err(ContentError::WrongLogicalRole);
            };
            let mut summaries = child_summaries(&children, level - 1)?;
            // The suffix page is rebuilt around the boundary, which stays live as
            // its first child.
            objects.release(suffix.id);
            summaries.insert(0, boundary);
            root_from_children(objects, summaries)?
                .ok_or(ContentError::InvalidRecord("empty concat"))
        }
        Some(_) => Err(ContentError::InvalidRecord("right concat levels")),
    }
}

/// Root for a whole list of extents, half-partitioning when the page is full.
pub fn root_from_extents(
    objects: &mut EditObjects<'_>,
    extents: Vec<ExtentSlice>,
) -> ContentResult<NodeSummary> {
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
pub fn root_from_children(
    objects: &mut EditObjects<'_>,
    children: Vec<NodeSummary>,
) -> ContentResult<Option<NodeSummary>> {
    if children.is_empty() {
        return Ok(None);
    }
    if children.len() == 1 {
        return Ok(Some(children[0]));
    }
    if children.len() <= MAX_ENTRIES {
        return emit_branch(objects, children).map(Some);
    }
    let half = children.len() / 2;
    let left = emit_branch(objects, children[..half].to_vec())?;
    let right = emit_branch(objects, children[half..].to_vec())?;
    emit_branch(objects, vec![left, right]).map(Some)
}

/// Emits one leaf page from its extents.
pub fn emit_leaf(
    objects: &mut EditObjects<'_>,
    extents: Vec<ExtentSlice>,
) -> ContentResult<NodeSummary> {
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
pub fn emit_branch(
    objects: &mut EditObjects<'_>,
    children: Vec<NodeSummary>,
) -> ContentResult<NodeSummary> {
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
    for child in children {
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
    objects.hold_node(&ExtentNode::Branch {
        level,
        subtree_logical_bytes: bytes,
        subtree_extent_count: extents,
        children: descriptors,
    })
}

/// Reads the file state behind a root and returns it with its mapping summary.
pub fn read_state(
    reader: &dyn AuthenticatedObjects,
    root: ObjectId,
) -> ContentResult<(FileState, NodeSummary)> {
    let canonical = reader.read_canonical(root)?;
    let state = decode_file_state(&canonical)?;
    Ok((
        state,
        NodeSummary {
            id: state.mapping_root,
            bytes: state.logical_len,
            extents: state.extent_count,
            level: state.tree_level,
        },
    ))
}
