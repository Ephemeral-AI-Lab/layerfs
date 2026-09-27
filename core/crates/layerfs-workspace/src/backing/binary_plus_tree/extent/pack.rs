//! Balanced branch packing and level-preserving collapse for extent pages.
use super::splice::{Level, PieceStore};
use crate::{
    backing::{
        metadata_index::vector,
        metadata_pages::{self, ChildRef, PageRef},
        segments::Window,
    },
    WorkspaceError,
};

// The codec accepts 248 children. This smaller target bounds repeated
// child-ownership I/O while keeping the existing full leaf packing.
const BRANCH_CHILDREN: usize = 32;
const MAX_HEIGHT: u8 = metadata_pages::LEVEL_LIMIT;

/// Answers one rebuilt node at its own declared level.
///
/// `children` are the node's rebuilt children, every one of them at
/// `level - 1`. `pack_level` writes the branch pages above them, which answers
/// at `level` as soon as it has two children to place. A fold can leave a node
/// with one child, or with none at all, when the interval it replaced covered
/// the whole node: the collapse is repaired here rather than refused.
///
/// Nothing to place is answered as an empty level, so the parent drops the
/// node. A single child is lifted inside one branch page of its own level, so
/// every path from the root to a leaf keeps the same depth: a sibling that did
/// not collapse must not end up beside a page one level shallower. The lifted
/// page replaces the node one for one, so a collapse never adds a page to the
/// tree and never grows its height. Rebalancing the collapsed child against a
/// sibling would also restore the two-child shape at the cost of reading and
/// rewriting that sibling's children, and it can still leave a node with one
/// child when the whole tree holds only three of them; preserving the level is
/// what the cursor and every later splice actually require.
pub(super) fn lift<S: PieceStore + ?Sized>(
    store: &S,
    mut children: Vec<ChildRef>,
    level: u8,
    sponsor: PageRef,
    window: &mut Window,
) -> Result<Level, WorkspaceError> {
    if level == 0 {
        return Err(WorkspaceError::Io);
    }
    if children.len() > 1 {
        // A parent can accept several replacement pages at this level. Only
        // the root may keep packing until it becomes one page.
        return pack_once(store, children, level - 1, sponsor, window);
    }
    let Some(child) = children.pop() else {
        return Ok(Level {
            pages: Vec::new(),
            level,
        });
    };
    if child.length == 0 {
        return Err(WorkspaceError::Io);
    }
    let incarnation = store.incarnation();
    let page = store.write(level, sponsor, window, |r, bytes| {
        metadata_pages::encode_pieces_branch(incarnation, r, level, &[child], bytes).map(|()| r)
    })?;
    Ok(Level {
        pages: vec![ChildRef {
            page,
            length: child.length,
        }],
        level,
    })
}

/// Writes the branch levels above `lower`, whose pages all declare
/// `child_level`. The result is one page at the level above them, or, when the
/// children collapse to a single page, that page at its own level.
///
/// Chunk boundaries are balanced rather than greedy: 249 pages split into 125
/// and 124, never 248 and a single child. A level of two or more pages therefore
/// becomes branches of two or more children, which is what `lift` relies on to
/// tell a normal rebuild from a collapsed node.
pub(super) fn pack_level<S: PieceStore + ?Sized>(
    store: &S,
    mut lower: Vec<ChildRef>,
    mut child_level: u8,
    mut sponsor: PageRef,
    window: &mut Window,
) -> Result<Level, WorkspaceError> {
    while lower.len() > 1 {
        let packed = pack_once(store, lower, child_level, sponsor, window)?;
        lower = packed.pages;
        child_level = packed.level;
        sponsor = PageRef::NULL;
    }
    Ok(Level {
        pages: lower,
        level: child_level,
    })
}

fn pack_once<S: PieceStore + ?Sized>(
    store: &S,
    lower: Vec<ChildRef>,
    child_level: u8,
    sponsor: PageRef,
    window: &mut Window,
) -> Result<Level, WorkspaceError> {
    let branch = child_level.checked_add(1).ok_or(WorkspaceError::Capacity)?;
    if branch > MAX_HEIGHT {
        return Err(WorkspaceError::Capacity);
    }
    let chunks = lower.len().div_ceil(BRANCH_CHILDREN);
    let base = lower.len() / chunks;
    let extra = lower.len() % chunks;
    let mut upper = vector(chunks)?;
    let mut at = 0usize;
    for index in 0..chunks {
        let take = base + usize::from(index < extra);
        let chunk = &lower[at..at + take];
        at += take;
        let length = chunk.iter().try_fold(0u64, |sum, c| {
            sum.checked_add(c.length).ok_or(WorkspaceError::Io)
        })?;
        let candidate = if chunks == 1 { sponsor } else { PageRef::NULL };
        let page = store.write(branch, candidate, window, |r, bytes| {
            metadata_pages::encode_pieces_branch(store.incarnation(), r, branch, chunk, bytes)
                .map(|()| r)
        })?;
        upper.push(ChildRef { page, length });
    }
    Ok(Level {
        pages: upper,
        level: branch,
    })
}
