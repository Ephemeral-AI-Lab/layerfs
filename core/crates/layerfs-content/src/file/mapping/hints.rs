//! Positional predecessor pages for a rebuilt or edited extent tree (#92).
//!
//! A new mapping page is offered, as its advisory delta base, the stored page of
//! an earlier version of the same file at the same level whose logical range
//! holds the new page's first byte. The correspondence is positional, as the
//! chunk correspondence of [`super::predecessor`] is: the two trees partition one
//! logical file, the builder fills pages from the start, and an edit rewrites
//! only the pages on the paths it touches, so the page at the same level and
//! offset is the page the new one was split, shifted or extended from. Storage
//! decides whether the base is acquired and worth a COPY/INSERT program; a wrong
//! hint costs a trial, never correctness.
//!
//! Only branch pages are read: a leaf's identity is known from its parent. Two
//! forms share one descent:
//!
//! * [`page_at`] descends lazily through the caller's own page reader, for an edit
//!   that has already read the paths it changes;
//! * [`PageHints`] collects the branch levels of the earlier version once, bounded
//!   by [`PAGE_HINT_BRANCH_READS`], for the streaming builder, which has no
//!   reader of its own. Pages past the bound get no hint.

use crate::error::{ContentError, ContentResult};
use crate::file::mapping::codec::decode_node_with_context;
use crate::file::mapping::types::{ExtentNode, FileState};
use crate::object::ObjectId;

/// Branch pages one [`PageHints::collect`] reads before it stops collecting.
pub const PAGE_HINT_BRANCH_READS: usize = 64;

/// Reads one stored mapping page's canonical bytes; `true` marks the root.
pub type PageRead<'r> = &'r mut dyn FnMut(ObjectId, bool) -> ContentResult<Vec<u8>>;

/// The stored page at `level` holding logical offset `start` in the mapping
/// `state` opens, or `None` when that tree has no such page.
///
/// The descent reads the branch pages on one path, root first, through `read`.
pub fn page_at(
    state: &FileState,
    level: u8,
    start: u64,
    read: PageRead<'_>,
) -> ContentResult<Option<ObjectId>> {
    if level > state.tree_level || start >= state.logical_len {
        return Ok(None);
    }
    let mut id = state.mapping_root;
    let mut current = state.tree_level;
    let mut offset = start;
    let mut root = true;
    while current > level {
        let node = decode_node_with_context(&read(id, root)?, root)?;
        let ExtentNode::Branch {
            level: stored,
            children,
            ..
        } = node
        else {
            return Err(ContentError::InvalidRecord("mapping hint level"));
        };
        if stored != current {
            return Err(ContentError::InvalidRecord("mapping hint level"));
        }
        let mut before = 0_u64;
        let Some(child) = children.iter().find(|child| {
            let holds = child.cumulative_logical_end > offset;
            if !holds {
                before = child.cumulative_logical_end;
            }
            holds
        }) else {
            return Ok(None);
        };
        offset -= before;
        id = child.child_object_id;
        current -= 1;
        root = false;
    }
    Ok(Some(id))
}

/// The branch levels of one stored mapping, collected for positional hints.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PageHints {
    /// Level and identity of the stored root.
    root: Option<(u8, ObjectId)>,
    /// Per level below the root: `(start, end, page)` in logical order. A level
    /// is a contiguous prefix of that level's pages.
    levels: Vec<Vec<(u64, u64, ObjectId)>>,
}

impl PageHints {
    /// Collects the pages of the mapping `state` opens, reading at most
    /// [`PAGE_HINT_BRANCH_READS`] branch pages, level by level from the root.
    pub fn collect(state: &FileState, read: PageRead<'_>) -> ContentResult<Self> {
        let mut hints = Self {
            root: Some((state.tree_level, state.mapping_root)),
            levels: vec![Vec::new(); usize::from(state.tree_level)],
        };
        let mut parents = vec![(0_u64, state.logical_len, state.mapping_root)];
        let mut reads = 0_usize;
        for level in (1..=state.tree_level).rev() {
            let mut children = Vec::new();
            for (start, _, id) in parents {
                if reads == PAGE_HINT_BRANCH_READS {
                    break;
                }
                reads += 1;
                let root = level == state.tree_level;
                let node = decode_node_with_context(&read(id, root)?, root)?;
                let ExtentNode::Branch {
                    level: stored,
                    children: descriptors,
                    ..
                } = node
                else {
                    return Err(ContentError::InvalidRecord("mapping hint level"));
                };
                if stored != level {
                    return Err(ContentError::InvalidRecord("mapping hint level"));
                }
                let mut before = start;
                for child in descriptors {
                    let end = start
                        .checked_add(child.cumulative_logical_end)
                        .ok_or(ContentError::LengthOverflow)?;
                    children.push((before, end, child.child_object_id));
                    before = end;
                }
            }
            hints.levels[usize::from(level) - 1] = children.clone();
            parents = children;
        }
        Ok(hints)
    }

    /// The collected page at `level` holding logical offset `start`.
    pub fn at(&self, level: u8, start: u64) -> Option<ObjectId> {
        let (root_level, root) = self.root?;
        if level == root_level {
            return Some(root);
        }
        let pages = self.levels.get(usize::from(level))?;
        let index = pages.partition_point(|(_, end, _)| *end <= start);
        pages
            .get(index)
            .filter(|(from, _, _)| *from <= start)
            .map(|(_, _, id)| *id)
    }
}
