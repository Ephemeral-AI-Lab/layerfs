//! The bounded reader for one file's length-indexed extent sequence.
//!
//! The cursor holds one page path and one leaf's records, so a read or a Commit
//! walk visits `O(H + touched leaves)` index pages and never materializes a
//! file's whole extent list. It checks the operation deadline between page
//! reads. Levels are relative: a branch directly above the leaves declares 1
//! and every branch declares exactly one below its parent, so the root declares
//! the tree's height.
use super::{
    metadata_pages::{self, PageRef, PieceRecord},
    metadata_pieces::PieceStore,
    segments::Window,
};
use crate::{overlay::pieces::Piece, WorkspaceError};
use layerfs_bridge::contract::MAX_FILE;
use std::time::Instant;

/// The declared structural ceiling: the root's level never exceeds it.
const MAX_HEIGHT: u8 = metadata_pages::LEVEL_LIMIT;

/// The arena's own page reads have no root owner to charge; this is the bound
/// one page operation may take on its own.
fn clock(deadline: Instant) -> Result<(), WorkspaceError> {
    crate::backing::payload::clock(deadline).map_err(|_| WorkspaceError::Deadline)
}

/// A bounded ordered cursor over one file's extents, positioned at `offset`.
pub fn cursor<'a, S: PieceStore + ?Sized>(
    store: &'a S,
    root: PageRef,
    offset: u64,
    length: u64,
    window: &mut Window,
    deadline: Instant,
) -> Result<Cursor<'a, S>, WorkspaceError> {
    Cursor::seek(store, root, offset, length, window, deadline)
}

/// The extent of `root` that covers `offset`, with its derived logical start.
pub fn piece_at<S: PieceStore + ?Sized>(
    store: &S,
    root: PageRef,
    offset: u64,
    length: u64,
    window: &mut Window,
    deadline: Instant,
) -> Result<(u64, Piece), WorkspaceError> {
    Cursor::seek(store, root, offset, length, window, deadline)?.piece_at(offset, window)
}

/// A bounded ordered cursor over one file's extents. The Workspace's backing
/// operations are deadline-bounded, so the cursor carries the operation's own
/// deadline and checks it between its page reads.
pub struct Cursor<'a, S: PieceStore + ?Sized> {
    store: &'a S,
    deadline: Instant,
    stack: [Frame; MAX_HEIGHT as usize],
    depth: usize,
    leaf: Vec<PieceRecord>,
    at: usize,
    point: u64,
}
/// One branch page above the current leaf: the page itself, its declared
/// level, how many children it holds and which one the walk is inside.
#[derive(Clone, Copy)]
struct Frame {
    page: PageRef,
    level: u8,
    count: u16,
    index: u16,
}
impl Frame {
    const EMPTY: Self = Self {
        page: PageRef::NULL,
        level: 0,
        count: 0,
        index: 0,
    };
}
impl<'a, S: PieceStore + ?Sized> Cursor<'a, S> {
    /// Positions a cursor at `offset`, which must be inside a sequence that
    /// covers `length` bytes.
    pub fn seek(
        store: &'a S,
        root: PageRef,
        offset: u64,
        length: u64,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<Self, WorkspaceError> {
        let mut cursor = Self {
            store,
            deadline,
            stack: [Frame::EMPTY; MAX_HEIGHT as usize],
            depth: 0,
            leaf: Vec::new(),
            at: 0,
            point: 0,
        };
        if root == PageRef::NULL {
            return if offset == 0 && length == 0 {
                Ok(cursor)
            } else {
                Err(WorkspaceError::Io)
            };
        }
        let mut page = root;
        let mut lower = 0u64;
        loop {
            if cursor.depth == MAX_HEIGHT as usize {
                return Err(WorkspaceError::Io);
            }
            let bytes = store.read(page, window)?;
            if metadata_pages::PageKind::of(&bytes)? != metadata_pages::PageKind::Pieces {
                return Err(WorkspaceError::Io);
            }
            if bytes[48] == 0 {
                let records = metadata_pages::decode_pieces_leaf(
                    store.incarnation(),
                    page,
                    &bytes,
                    MAX_FILE,
                )?;
                let mut at = 0;
                while at < records.len() {
                    let end = lower
                        .checked_add(records[at].length)
                        .ok_or(WorkspaceError::Io)?;
                    if end > offset {
                        break;
                    }
                    lower = end;
                    at += 1;
                }
                if at == records.len() && offset != length {
                    return Err(WorkspaceError::Io);
                }
                cursor.leaf = records;
                cursor.at = at;
                cursor.point = lower;
                return Ok(cursor);
            }
            let (level, children) =
                metadata_pages::decode_pieces_branch(store.incarnation(), page, &bytes, MAX_FILE)?;
            // Levels are relative: a branch one above the leaves declares 1,
            // and every branch declares exactly one below its parent. The root
            // therefore declares the tree's height, never more than the
            // declared ceiling.
            if level == 0 || usize::from(level) > usize::from(MAX_HEIGHT) {
                return Err(WorkspaceError::Io);
            }
            if cursor.depth > 0 && level != cursor.stack[cursor.depth - 1].level.saturating_sub(1) {
                return Err(WorkspaceError::Io);
            }
            let mut selected = children.len() - 1;
            for (index, child) in children.iter().enumerate() {
                if offset < lower + child.length {
                    selected = index;
                    break;
                }
            }
            lower = children[..selected].iter().try_fold(lower, |sum, c| {
                sum.checked_add(c.length).ok_or(WorkspaceError::Io)
            })?;
            cursor.stack[cursor.depth] = Frame {
                page,
                level,
                count: children.len() as u16,
                index: selected as u16,
            };
            cursor.depth += 1;
            page = children[selected].page;
        }
    }
    /// The next extent in sequence order, with its derived logical start.
    pub fn next(&mut self, window: &mut Window) -> Result<Option<(u64, Piece)>, WorkspaceError> {
        loop {
            clock(self.deadline)?;
            if self.at < self.leaf.len() {
                let record = self.leaf[self.at];
                let start = self.point;
                self.point = self
                    .point
                    .checked_add(record.length)
                    .ok_or(WorkspaceError::Io)?;
                self.at += 1;
                return Ok(Some((start, Piece::from_record(record)?)));
            }
            if !self.advance(window)? {
                return Ok(None);
            }
        }
    }
    /// The extent that covers `offset`, with its derived logical start.
    pub fn piece_at(
        mut self,
        offset: u64,
        window: &mut Window,
    ) -> Result<(u64, Piece), WorkspaceError> {
        clock(self.deadline)?;
        let (start, piece) = self.next(window)?.ok_or(WorkspaceError::Io)?;
        if offset < start || offset >= start.checked_add(piece.length).ok_or(WorkspaceError::Io)? {
            return Err(WorkspaceError::Io);
        }
        Ok((start, piece))
    }
    /// Moves to the next leaf, if the sequence holds one.
    fn advance(&mut self, window: &mut Window) -> Result<bool, WorkspaceError> {
        clock(self.deadline)?;
        while self.depth > 0 {
            self.depth -= 1;
            let frame = self.stack[self.depth];
            let next = frame.index.saturating_add(1);
            if next < frame.count {
                self.stack[self.depth].index = next;
                return self.step_in(window).map(|()| true);
            }
        }
        Ok(false)
    }
    /// Rebuilds the walk under the child the frame at the current depth now
    /// selects. Every ancestor is re-read for its children's lengths, so the
    /// new leaf's logical start is accumulated, not remembered.
    fn step_in(&mut self, window: &mut Window) -> Result<(), WorkspaceError> {
        let mut lower = 0u64;
        for height in 0..=self.depth {
            let frame = self.stack[height];
            let bytes = self.store.read(frame.page, window)?;
            if metadata_pages::PageKind::of(&bytes)? != metadata_pages::PageKind::Pieces
                || bytes[48] == 0
            {
                return Err(WorkspaceError::Io);
            }
            let (level, children) = metadata_pages::decode_pieces_branch(
                self.store.incarnation(),
                frame.page,
                &bytes,
                MAX_FILE,
            )?;
            if level != frame.level || children.len() != usize::from(frame.count) {
                return Err(WorkspaceError::Io);
            }
            let index = usize::from(frame.index);
            let child = children.get(index).ok_or(WorkspaceError::Io)?;
            lower = children[..index].iter().try_fold(lower, |sum, c| {
                sum.checked_add(c.length).ok_or(WorkspaceError::Io)
            })?;
            if height == self.depth {
                // The frames above stay; the subtree below is rebuilt.
                self.depth += 1;
                return self.enter(child.page, lower, window);
            }
        }
        Err(WorkspaceError::Io)
    }
    /// Loads the leftmost leaf of the subtree at `page`, which starts at
    /// `lower`, pushing a frame for every branch level it passes.
    fn enter(
        &mut self,
        mut page: PageRef,
        lower: u64,
        window: &mut Window,
    ) -> Result<(), WorkspaceError> {
        loop {
            clock(self.deadline)?;
            if self.depth >= MAX_HEIGHT as usize {
                return Err(WorkspaceError::Io);
            }
            let bytes = self.store.read(page, window)?;
            if metadata_pages::PageKind::of(&bytes)? != metadata_pages::PageKind::Pieces {
                return Err(WorkspaceError::Io);
            }
            if bytes[48] == 0 {
                self.leaf = metadata_pages::decode_pieces_leaf(
                    self.store.incarnation(),
                    page,
                    &bytes,
                    MAX_FILE,
                )?;
                self.at = 0;
                self.point = lower;
                return Ok(());
            }
            let (level, children) = metadata_pages::decode_pieces_branch(
                self.store.incarnation(),
                page,
                &bytes,
                MAX_FILE,
            )?;
            if level == 0 || usize::from(level) > usize::from(MAX_HEIGHT) {
                return Err(WorkspaceError::Io);
            }
            if self.depth > 0 && level != self.stack[self.depth - 1].level.saturating_sub(1) {
                return Err(WorkspaceError::Io);
            }
            self.stack[self.depth] = Frame {
                page,
                level,
                count: children.len() as u16,
                index: 0,
            };
            self.depth += 1;
            page = children[0].page;
            // The first child starts exactly where its parent's subtree does.
        }
    }
}
