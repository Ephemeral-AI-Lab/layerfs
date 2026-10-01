//! Monotone depth-bounded mapping discovery, independent of retained cache.
use crate::error::{ContentError, ContentResult};

use super::read::Frontier;
use super::{ChildDescriptor, ExtentNode, ExtentSlice, FileState, MAX_ENTRIES, MAX_LEVEL};
use std::ops::Range;

struct Frame {
    children: Vec<ChildDescriptor>,
    next: usize,
    origin: u64,
    child_level: u8,
}

pub(super) struct Leaf {
    pub position: u64,
    pub next: usize,
    pub end: u64,
    pub extents: Vec<ExtentSlice>,
}

pub(super) struct Frontiers {
    pub nodes: [Frontier; super::READ_NAVIGATION_WAVE],
    pub len: usize,
}

impl Leaf {
    pub fn new(origin: u64, length: u64, extents: Vec<ExtentSlice>) -> ContentResult<Self> {
        checked_capacity("mapping.cursor_extents", extents.capacity(), MAX_ENTRIES)?;
        Ok(Self {
            position: origin,
            next: 0,
            end: origin
                .checked_add(length)
                .ok_or(ContentError::LengthOverflow)?,
            extents,
        })
    }
}

/// One selected mapping continuation, with no level-width queue.
pub(super) struct Navigation {
    root: Frontier,
    pending: Option<Frontier>,
    frames: Vec<Frame>,
    pub leaf: Option<Leaf>,
}

impl Navigation {
    pub fn new(state: FileState) -> ContentResult<Self> {
        let root = Frontier {
            id: state.mapping_root,
            root: true,
            level: state.tree_level,
            origin: 0,
            expected_root: Some((state.logical_len, state.extent_count)),
        };
        let mut frames = Vec::new();
        frames
            .try_reserve_exact(usize::from(MAX_LEVEL) + 1)
            .map_err(|_| ContentError::ResourceUnavailable {
                what: "mapping.cursor_frames",
            })?;
        checked_capacity(
            "mapping.cursor_frames",
            frames.capacity(),
            usize::from(MAX_LEVEL) + 1,
        )?;
        Ok(Self {
            root,
            pending: Some(root),
            frames,
            leaf: None,
        })
    }

    /// Discover at most32 leaves, keeping the first out-of-range child unconsumed.
    pub fn next_wave(
        &mut self,
        range: &Range<u64>,
        mut branch: impl FnMut(Frontier) -> ContentResult<ExtentNode>,
    ) -> ContentResult<Frontiers> {
        let mut result = Frontiers {
            nodes: [self.root; super::READ_NAVIGATION_WAVE],
            len: 0,
        };
        while result.len < result.nodes.len() {
            let Some(node) = self.next_leaf(range, &mut branch)? else {
                break;
            };
            result.nodes[result.len] = node;
            result.len += 1;
        }
        Ok(result)
    }

    fn next_leaf(
        &mut self,
        range: &Range<u64>,
        branch: &mut impl FnMut(Frontier) -> ContentResult<ExtentNode>,
    ) -> ContentResult<Option<Frontier>> {
        loop {
            let node = match self.pending.take() {
                Some(node) => node,
                None => {
                    let Some(frame) = self.frames.last_mut() else {
                        return Ok(None);
                    };
                    if frame.next == frame.children.len() {
                        self.frames.pop();
                        continue;
                    }
                    let child = frame.children[frame.next];
                    let (prior_bytes, prior_extents) = if frame.next == 0 {
                        (0, 0)
                    } else {
                        let prior = frame.children[frame.next - 1];
                        (prior.cumulative_logical_end, prior.cumulative_extent_end)
                    };
                    let origin = frame
                        .origin
                        .checked_add(prior_bytes)
                        .ok_or(ContentError::LengthOverflow)?;
                    let end = frame
                        .origin
                        .checked_add(child.cumulative_logical_end)
                        .ok_or(ContentError::LengthOverflow)?;
                    if origin >= range.end {
                        return Ok(None);
                    }
                    frame.next += 1;
                    if end <= range.start {
                        continue;
                    }
                    Frontier {
                        id: child.child_object_id,
                        root: false,
                        level: frame.child_level,
                        origin,
                        expected_root: Some((
                            child
                                .cumulative_logical_end
                                .checked_sub(prior_bytes)
                                .ok_or(ContentError::NonCanonicalOrdering)?,
                            child
                                .cumulative_extent_end
                                .checked_sub(prior_extents)
                                .ok_or(ContentError::NonCanonicalOrdering)?,
                        )),
                    }
                }
            };
            if node.level == 0 {
                return Ok(Some(node));
            }
            if node.level > MAX_LEVEL || self.frames.len() > usize::from(MAX_LEVEL) {
                return Err(ContentError::MappingDepthExceeded);
            }
            let ExtentNode::Branch { children, .. } = branch(node)? else {
                return Err(ContentError::WrongLogicalRole);
            };
            checked_capacity("mapping.cursor_children", children.capacity(), MAX_ENTRIES)?;
            self.frames.push(Frame {
                children,
                next: 0,
                origin: node.origin,
                child_level: node.level - 1,
            });
        }
    }
}

fn checked_capacity(what: &'static str, actual: usize, limit: usize) -> ContentResult<()> {
    if actual > limit {
        return Err(ContentError::BoundedCapacityExceeded {
            what,
            limit: limit as u64,
            actual: actual as u64,
        });
    }
    Ok(())
}
