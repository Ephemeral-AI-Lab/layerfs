//! Operation-owned edit drafts, reference custody and final emission.

use std::collections::{BTreeMap, BTreeSet};

use super::tree::child_summaries;
use crate::error::{ContentError, ContentResult};
use crate::file::mapping::{
    decode_node_with_context, encode_node, ChildDescriptor, ExtentNode, ExtentSlice, NodeSummary,
    PageCache,
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
    /// Mapping pages an earlier pass of this operation already acquired.
    pages: &'a mut PageCache,
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
    ///
    /// `pages` is the operation's shared mapping-page memo: a page the comparison
    /// pass acquired is served from it here instead of being demanded again.
    pub fn new(
        reader: &'a dyn AuthenticatedObjects,
        consumer: &'a mut dyn FinalizedConsumer,
        pages: &'a mut PageCache,
    ) -> Self {
        Self {
            reader,
            consumer,
            pages,
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
        // The memo is consulted before the charge and before the demand: a page an
        // earlier pass of this operation acquired is neither read nor demanded
        // again, so `nodes_read` keeps counting what this operation actually read.
        if let Some(canonical) = self.pages.get(summary.id, root) {
            let node = decode_node_with_context(canonical, root)?;
            if node.level() != summary.level
                || node.logical_len() != summary.bytes
                || node.extent_count() != summary.extents
            {
                return Err(ContentError::InvalidRecord("extent summary"));
            }
            return Ok(node);
        }
        self.counters.nodes_read = self.counters.nodes_read.saturating_add(1);
        let (node, canonical) = match self.drafts.get(&summary.id) {
            Some(Draft::Node(node)) => (node.clone(), None),
            Some(Draft::Page(object)) => {
                (decode_node_with_context(object.canonical(), root)?, None)
            }
            None => {
                let canonical = self.reader.read_canonical(summary.id)?;
                let node = decode_node_with_context(&canonical, root)?;
                (node, Some(canonical))
            }
        };
        if node.level() != summary.level
            || node.logical_len() != summary.bytes
            || node.extent_count() != summary.extents
        {
            return Err(ContentError::InvalidRecord("extent summary"));
        }
        if let Some(canonical) = canonical {
            self.pages.make_room_for(1);
            self.pages.insert(summary.id, root, canonical);
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
    pub(super) fn release(&mut self, id: ObjectId) {
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
                let canonical = encode_node(&node, root)?;
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
