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

use super::references::{Child as ChildDescriptor, Node as ExtentNode, Summary as NodeSummary};
use crate::error::{ContentError, ContentResult};
use crate::file::mapping::{ExtentSlice, MAX_ENTRIES, MAX_LEVEL};

pub(super) use super::engine::{DeferredSink, Engine as EditObjects};

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
pub fn discard(objects: &mut EditObjects<'_>, summary: Option<NodeSummary>) -> ContentResult<()> {
    if let Some(summary) = summary {
        objects.release(summary.id)?;
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
    // An interior split replaces this node with the two halves it returns, so the
    // draft it may hold is superseded and released. Every child it was built from
    // stays live: it is re-referenced by one of the halves or by the recursion.
    let node = objects.load_node(root, root_context)?;
    match node {
        ExtentNode::Leaf { extents, .. } => {
            // Payload slices own no draft mapping children. Release the
            // superseded leaf before allocating its halves; a parent-referenced
            // leaf remains retained by the counted release transition.
            objects.release(root.id)?;
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
            // A singleton prefix/suffix is a bare child summary, so it does not
            // supply a new counted parent. Keep the original parent until both
            // concatenations have consumed their repeated child references.
            let halves = (
                concat_optional(objects, prefix, child_left)?,
                concat_optional(objects, child_right, suffix)?,
            );
            objects.release(root.id)?;
            Ok(halves)
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
    summary: NodeSummary,
    node: Option<ExtentNode>,
}

impl JoinSide {
    /// A side that has not been decoded yet.
    const fn summary(summary: NodeSummary) -> Self {
        Self {
            summary,
            node: None,
        }
    }

    /// A side the caller already decoded and checked.
    const fn decoded(summary: NodeSummary, node: ExtentNode) -> Self {
        Self {
            summary,
            node: Some(node),
        }
    }

    /// The decoded node, from the caller when it has one.
    fn take(&mut self, objects: &mut EditObjects<'_>, root: bool) -> ContentResult<ExtentNode> {
        match self.node.take() {
            Some(node) => Ok(node),
            None => objects.load_node(self.summary, root),
        }
    }
}

fn concat_inner(
    objects: &mut EditObjects<'_>,
    mut left: JoinSide,
    mut right: JoinSide,
    depth: u8,
) -> ContentResult<NodeSummary> {
    if depth > MAX_LEVEL {
        return Err(ContentError::MappingDepthExceeded);
    }
    if left.summary.level == right.summary.level {
        let left_node = left.take(objects, true)?;
        let right_node = right.take(objects, true)?;
        // A join replaces both inputs: a merged page when they are leaves, and a
        // rebuilt page when they are branches. Their children stay live.
        objects.release(left.summary.id)?;
        if right.summary.id != left.summary.id {
            objects.release(right.summary.id)?;
        }
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
    if left.summary.level > right.summary.level {
        let ExtentNode::Branch {
            level, children, ..
        } = left.take(objects, true)?
        else {
            return Err(ContentError::WrongLogicalRole);
        };
        let summaries = child_summaries(&children, level - 1)?;
        // The taller side is dismantled into a rebuilt prefix and one descending
        // boundary; every child it held stays live in one of the two.
        let (last, prefix) = summaries
            .split_last()
            .ok_or(ContentError::InvalidRecord("empty branch"))?;
        let last = *last;
        // The boundary child is checked under its **non-root** context, the
        // stricter one, and the decoded node travels into the join instead of
        // being read and decoded a second time.
        let last_node = objects.load_node(last, false)?;
        let prefix = root_from_children(objects, prefix.to_vec())?;
        let boundary = concat_inner(
            objects,
            JoinSide::decoded(last, last_node),
            right,
            depth + 1,
        )?;
        objects.release(left.summary.id)?;
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
                } = objects.load_node(boundary, true)?
                else {
                    return Err(ContentError::WrongLogicalRole);
                };
                let mut summaries = child_summaries(&children, level - 1)?;
                // The loaded page is superseded by the branch that re-hosts its
                // children; the prefix becomes the first of those children and
                // stays live.
                objects.release(boundary.id)?;
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
    } = right.take(objects, true)?
    else {
        return Err(ContentError::WrongLogicalRole);
    };
    let summaries = child_summaries(&children, level - 1)?;
    // Mirror of the taller-left case: the taller side is dismantled into a
    // descending boundary and one rebuilt suffix.
    let (first, suffix) = summaries
        .split_first()
        .ok_or(ContentError::InvalidRecord("empty branch"))?;
    let first = *first;
    // As above: the stricter non-root check stays, and the join reuses the node.
    let first_node = objects.load_node(first, false)?;
    let boundary = concat_inner(
        objects,
        left,
        JoinSide::decoded(first, first_node),
        depth + 1,
    )?;
    let suffix = root_from_children(objects, suffix.to_vec())?;
    objects.release(right.summary.id)?;
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
            } = objects.load_node(boundary, true)?
            else {
                return Err(ContentError::WrongLogicalRole);
            };
            let mut summaries = child_summaries(&children, level - 1)?;
            // The loaded page is superseded by the branch that re-hosts its
            // children; the suffix becomes the last of those children and stays
            // live.
            objects.release(boundary.id)?;
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
            objects.release(suffix.id)?;
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
