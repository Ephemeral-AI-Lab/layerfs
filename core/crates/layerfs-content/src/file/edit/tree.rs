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

use std::collections::BTreeMap;

use crate::error::{ContentError, ContentResult};
use crate::file::mapping::{
    decode_file_state, decode_node_with_context, encode_file_state, encode_node, ChildDescriptor,
    ExtentNode, ExtentSlice, FileState, NodeSummary, MAX_ENTRIES, MAX_LEVEL,
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
    /// Nodes created by this operation.
    pub nodes_created: u64,
    /// Payload objects created by this operation.
    pub payloads_created: u64,
    /// Payload bytes created.
    pub payload_bytes: u64,
    /// Largest number of deferred node bytes held at once.
    pub peak_deferred_bytes: usize,
}

/// The operation's private object set: deferred unfinished nodes plus the
/// already-stored objects it reads through.
pub struct EditObjects<'a> {
    reader: &'a dyn AuthenticatedObjects,
    consumer: &'a mut dyn FinalizedConsumer,
    deferred: BTreeMap<ObjectId, Vec<u8>>,
    charged: usize,
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
            deferred: BTreeMap::new(),
            charged: 0,
            counters: EditCounters::default(),
        }
    }

    /// Work performed so far.
    pub const fn counters(&self) -> EditCounters {
        self.counters
    }

    /// Reads one authenticated canonical object, preferring this operation's own
    /// unfinished nodes over the stored generation.
    pub fn read(&self, id: ObjectId) -> ContentResult<Vec<u8>> {
        match self.deferred.get(&id) {
            Some(bytes) => Ok(bytes.clone()),
            None => self.reader.read_canonical(id),
        }
    }

    /// Reads and decodes one mapping page under its root/non-root context.
    pub fn load_node(&mut self, summary: NodeSummary, root: bool) -> ContentResult<ExtentNode> {
        self.counters.nodes_read = self.counters.nodes_read.saturating_add(1);
        let canonical = self.read(summary.id)?;
        let node = decode_node_with_context(&canonical, root)?;
        if node.level() != summary.level
            || node.logical_len() != summary.bytes
            || node.extent_count() != summary.extents
        {
            return Err(ContentError::InvalidRecord("extent summary"));
        }
        Ok(node)
    }

    /// Encodes one node, keeps it as this operation's unfinished state and returns
    /// its summary. The node is published only if the final tree reaches it.
    pub fn hold_node(&mut self, node: &ExtentNode) -> ContentResult<NodeSummary> {
        let canonical = encode_node(node)?;
        let id = ObjectId::for_bytes(&canonical);
        let charged = canonical
            .len()
            .checked_add(DEFERRED_OBJECT_OVERHEAD)
            .ok_or(ContentError::LengthOverflow)?;
        let next = self
            .charged
            .checked_add(charged)
            .ok_or(ContentError::LengthOverflow)?;
        if next > EDIT_DEFERRED_LIMIT {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "edit.deferred_nodes",
                limit: EDIT_DEFERRED_LIMIT as u64,
                actual: next as u64,
            });
        }
        if let Some(prior) = self.deferred.get(&id) {
            if prior != &canonical {
                return Err(ContentError::IdentityMismatch);
            }
        } else {
            self.deferred.insert(id, canonical);
        }
        self.charged = next;
        self.counters.nodes_created = self.counters.nodes_created.saturating_add(1);
        self.counters.peak_deferred_bytes = self.counters.peak_deferred_bytes.max(next);
        Ok(NodeSummary {
            id,
            bytes: node.logical_len(),
            extents: node.extent_count(),
            level: node.level(),
        })
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
        self.commit(mapping)?;
        let root = emit_file_state(self.consumer, mapping)?;
        Ok(root)
    }

    /// Publishes every unfinished node the final tree reaches, children first.
    ///
    /// A node that a later split or join overwrote is simply never reached, so no
    /// speculative object is ever emitted and no prune pass is needed.
    pub fn commit(&mut self, root: NodeSummary) -> ContentResult<()> {
        let mut visited = std::collections::BTreeSet::new();
        self.commit_node(root, true, &mut visited)
    }

    fn commit_node(
        &mut self,
        summary: NodeSummary,
        root: bool,
        visited: &mut std::collections::BTreeSet<ObjectId>,
    ) -> ContentResult<()> {
        if !visited.insert(summary.id) {
            return Ok(());
        }
        let Some(canonical) = self.deferred.get(&summary.id).cloned() else {
            // A stored subtree: its objects are already published.
            return Ok(());
        };
        let node = decode_node_with_context(&canonical, root)?;
        if let ExtentNode::Branch {
            level, children, ..
        } = &node
        {
            for child in child_summaries(children, level - 1)? {
                self.commit_node(child, false, visited)?;
            }
        }
        let role = match node {
            ExtentNode::Leaf { .. } => ObjectRole::ExtentLeaf,
            ExtentNode::Branch { .. } => ObjectRole::ExtentBranch,
        };
        let references = node.references();
        let object = FinalizedObject::new(role, canonical)?.with_references(references);
        self.consumer.accept(object)?;
        Ok(())
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
        match object.role() {
            ObjectRole::Chunk => {
                self.objects.publish_payload(object)?;
                Ok(())
            }
            _ => {
                let node = decode_node_with_context(object.canonical(), true)?;
                self.objects.hold_node(&node)?;
                Ok(())
            }
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
pub fn coalesce(extents: &mut Vec<ExtentSlice>) -> ContentResult<()> {
    let mut index = 1;
    while index < extents.len() {
        let previous = extents[index - 1];
        let current = extents[index];
        if previous.payload_object_id() == current.payload_object_id()
            && previous
                .source_offset()
                .checked_add(previous.logical_length())
                == Some(current.source_offset())
        {
            extents[index - 1] = ExtentSlice::new(
                previous.payload_object_id(),
                previous.source_offset(),
                previous
                    .logical_length()
                    .checked_add(current.logical_length())
                    .ok_or(ContentError::LengthOverflow)?,
            )?;
            extents.remove(index);
        } else {
            index += 1;
        }
    }
    Ok(())
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
    let node = objects.load_node(root, root_context)?;
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

/// Encodes and publishes the file state of a finished edit.
pub fn emit_file_state(
    consumer: &mut dyn FinalizedConsumer,
    mapping: NodeSummary,
) -> ContentResult<ObjectId> {
    let state = FileState {
        logical_len: mapping.bytes,
        extent_count: mapping.extents,
        tree_level: mapping.level,
        profile_id: crate::file::mapping::profile_id(),
        mapping_root: mapping.id,
    };
    let object = FinalizedObject::new(ObjectRole::FileState, encode_file_state(state)?)?
        .with_references(vec![mapping.id]);
    let id = object.id();
    consumer.accept(object)?;
    Ok(id)
}
