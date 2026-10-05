//! One persistent ordered walk over a keyed page tree.
//!
//! A bounded successor lookup descends from the root for every key it answers,
//! so an ordered walk of a whole tree reads one root path per key and one
//! reached leaf per step. This cursor holds the path it is on instead: leaving
//! a leaf steps to the next child of the nearest branch that still has one, and
//! entering the next leaf is a descent from there. A walk that reaches `L`
//! leaves under a tree of height `H` therefore reads each of those leaves once
//! and each branch page once on the way in and once on the way out - `O(H + L)`
//! page reads rather than `O(H * L)` - and never materializes a page list.
//!
//! Levels and fences are checked on the way down exactly as the successor
//! lookup checks them, so a cursor cannot walk a tree the reader would refuse.
use super::update::{check_index, key_limit};
use crate::{
    backing::{
        metadata::Arena,
        metadata_index::vector,
        metadata_pages::{Cell, PageData, PageRef, MIN_BODY},
        segments::Window,
    },
    WorkspaceError,
};
use std::{sync::Arc, time::Instant};

/// One open branch level: the page itself and which of its children the walk is
/// inside. A walk keeps one frame per level, so leaving a leaf costs one step
/// per branch it passes and never a re-read of the path above it.
struct Frame {
    page: PageData,
    at: usize,
}

/// One ordered, resumable walk over the keys of one keyed page tree. The cursor
/// owns the arena handle it reads through, so a caller can keep one walk across
/// several transport pulls instead of seeking the same path again for every
/// record.
pub struct KeyCursor {
    arena: Arc<Arena>,
    frames: Vec<Frame>,
    leaf: Option<PageData>,
    at: usize,
    limit: u8,
    deadline: Instant,
    leaves: u64,
    reads: u64,
}
impl KeyCursor {
    /// The next key in order, or `None` once the tree is exhausted.
    pub fn next(&mut self, window: &mut Window) -> Result<Option<Cell>, WorkspaceError> {
        loop {
            if let Some(leaf) = &self.leaf {
                if self.at < leaf.cells.len() {
                    let cell = leaf.cells[self.at].clone();
                    self.at += 1;
                    return Ok(Some(cell));
                }
            }
            // The leaf is exhausted. Step to the next child of the nearest
            // branch that still has one; a branch page left behind is never
            // read again.
            loop {
                let Some(frame) = self.frames.last_mut() else {
                    self.leaf = None;
                    return Ok(None);
                };
                frame.at += 1;
                if frame.at < frame.page.cells.len() {
                    break;
                }
                self.frames.pop();
            }
            let child = {
                let frame = self.frames.last().ok_or(WorkspaceError::Io)?;
                PageRef::parse(frame.page.cells[frame.at].value())?
            };
            self.open(child, window)?;
        }
    }
    /// Leaf pages this walk has read. One reached leaf is one read, however
    /// many keys it carries.
    pub fn leaves(&self) -> u64 {
        self.leaves
    }
    /// Every page this walk has read, branch levels included.
    pub fn reads(&self) -> u64 {
        self.reads
    }
    /// Whether the walk has passed the last key the tree holds.
    pub fn exhausted(&self) -> bool {
        self.leaf.is_none()
    }
    fn read(&mut self, r: PageRef, window: &mut Window) -> Result<PageData, WorkspaceError> {
        crate::backing::payload::clock(self.deadline).map_err(|_| WorkspaceError::Deadline)?;
        self.reads += 1;
        self.arena.load(r, window, self.deadline)
    }
    /// Descends to the leftmost leaf of one subtree, keeping every branch page
    /// the descent read.
    fn open(&mut self, root: PageRef, window: &mut Window) -> Result<(), WorkspaceError> {
        let mut r = root;
        loop {
            let page = self.read(r, window)?;
            check_index(&page.cells, page.level, self.limit)?;
            if page.level == 0 {
                self.leaves += 1;
                self.at = 0;
                self.leaf = Some(page);
                return Ok(());
            }
            r = PageRef::parse(page.cells.first().ok_or(WorkspaceError::Io)?.value())?;
            self.frames.push(Frame { page, at: 0 });
        }
    }
    /// Descends to the first leaf that can hold a key at or above `lower`, and
    /// positions the walk at the first such key.
    fn seek(
        &mut self,
        root: PageRef,
        lower: &[u8],
        window: &mut Window,
    ) -> Result<(), WorkspaceError> {
        let mut r = root;
        let mut fence: Option<(u8, Vec<u8>)> = None;
        loop {
            let page = self.read(r, window)?;
            check_index(&page.cells, page.level, self.limit)?;
            if let Some((level, key)) = &fence {
                if page.level != *level
                    || page.body() < MIN_BODY
                    || page
                        .cells
                        .last()
                        .is_none_or(|last| last.key() != key.as_slice())
                {
                    return Err(WorkspaceError::Io);
                }
            }
            if page.level == 0 {
                self.leaves += 1;
                self.at = page
                    .cells
                    .iter()
                    .position(|cell| cell.key() >= lower)
                    .unwrap_or(page.cells.len());
                self.leaf = Some(page);
                return Ok(());
            }
            let at = page
                .cells
                .iter()
                .position(|cell| cell.key() >= lower)
                .unwrap_or(page.cells.len() - 1);
            let child = PageRef::parse(page.cells[at].value())?;
            fence = Some((page.level - 1, page.cells[at].key().to_vec()));
            self.frames.push(Frame { page, at });
            r = child;
        }
    }
}
impl Arena {
    /// One persistent ordered cursor over the keys of `root`, positioned at the
    /// first key that is not below `lower`. `lower` is any key of the tree's own
    /// kind, including a bare kind byte for the beginning of that kind.
    pub fn key_cursor(
        self: &Arc<Self>,
        root: PageRef,
        lower: &[u8],
        window: &mut Window,
        deadline: Instant,
    ) -> Result<KeyCursor, WorkspaceError> {
        let mut cursor = KeyCursor {
            arena: self.clone(),
            frames: vector(8)?,
            leaf: None,
            at: 0,
            limit: key_limit(lower)?,
            deadline,
            leaves: 0,
            reads: 0,
        };
        if root != PageRef::NULL {
            cursor.seek(root, lower, window)?;
        }
        Ok(cursor)
    }
}
