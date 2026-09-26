//! The bounded ordered reader for one file's length-indexed extent sequence.
//!
//! A cursor holds one page path and one leaf's records, so a read or a Commit
//! walk visits `O(H + L)` index pages for `H` the tree's height and `L` the
//! leaves it reaches, and never materializes a file's whole extent list. It
//! checks the operation deadline between page reads.
//!
//! Levels are relative: a branch directly above the leaves declares 1 and every
//! branch declares exactly one below its parent, so the root declares the tree's
//! height. Every frame remembers the logical start and length of the child it
//! selected, so advancing to the next leaf subtracts those remembered lengths
//! instead of re-reading the path from the root: one page read per frame the step
//! passes through, plus one per level the following descent enters. Summed over a
//! whole walk that is `O(H + L)`, not one path read per leaf.
use super::{
    metadata_pages::{self, PageRef, PieceRecord, PAGE},
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

/// A shared or borrowed store handle is a store: the cursor owns whichever
/// handle its caller holds, so one Commit phase can keep a walk across its
/// transport pulls without borrowing the host it reads through.
impl<S: PieceStore + ?Sized> PieceStore for &S {
    fn read(&self, page: PageRef, window: &mut Window) -> Result<[u8; PAGE], WorkspaceError> {
        (**self).read(page, window)
    }
    fn write<T>(
        &self,
        level: u8,
        window: &mut Window,
        encode: impl FnOnce(PageRef, &mut [u8]) -> Result<T, WorkspaceError>,
    ) -> Result<T, WorkspaceError> {
        (**self).write(level, window, encode)
    }
    fn incarnation(&self) -> [u8; 32] {
        (**self).incarnation()
    }
}
impl<S: PieceStore + ?Sized> PieceStore for std::sync::Arc<S> {
    fn read(&self, page: PageRef, window: &mut Window) -> Result<[u8; PAGE], WorkspaceError> {
        (**self).read(page, window)
    }
    fn write<T>(
        &self,
        level: u8,
        window: &mut Window,
        encode: impl FnOnce(PageRef, &mut [u8]) -> Result<T, WorkspaceError>,
    ) -> Result<T, WorkspaceError> {
        (**self).write(level, window, encode)
    }
    fn incarnation(&self) -> [u8; 32] {
        (**self).incarnation()
    }
}

/// A bounded ordered cursor over one file's extents, positioned at `offset`.
/// The cursor owns its store handle, so a Commit phase can hold one across its
/// transport pulls instead of seeking the same path again for every frame.
pub fn cursor<S: PieceStore>(
    store: S,
    root: PageRef,
    offset: u64,
    length: u64,
    window: &mut Window,
    deadline: Instant,
) -> Result<Cursor<S>, WorkspaceError> {
    Cursor::seek(store, root, offset, length, window, deadline)
}

/// The extent of `root` that covers `offset`, with its derived logical start.
pub fn piece_at<S: PieceStore>(
    store: S,
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
pub struct Cursor<S: PieceStore> {
    store: S,
    deadline: Instant,
    stack: [Frame; MAX_HEIGHT as usize],
    depth: usize,
    leaf: Vec<PieceRecord>,
    at: usize,
    point: u64,
}
/// One branch page above the current leaf: the page itself, its declared level,
/// how many children it holds, which one the walk is inside, and the logical
/// start and length of that child's subtree. The two positions are what let the
/// walk step to the next child without re-reading the path above it.
#[derive(Clone, Copy)]
struct Frame {
    page: PageRef,
    level: u8,
    count: u16,
    index: u16,
    start: u64,
    step: u64,
}
impl Frame {
    const EMPTY: Self = Self {
        page: PageRef::NULL,
        level: 0,
        count: 0,
        index: 0,
        start: 0,
        step: 0,
    };
}
impl<S: PieceStore> Cursor<S> {
    /// Positions a cursor at `offset`, which must be inside a sequence that
    /// covers `length` bytes.
    pub fn seek(
        store: S,
        root: PageRef,
        offset: u64,
        length: u64,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<Self, WorkspaceError> {
        if offset > length {
            return Err(WorkspaceError::Io);
        }
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
            let bytes = cursor.store.read(page, window)?;
            if metadata_pages::PageKind::of(&bytes)? != metadata_pages::PageKind::Pieces {
                return Err(WorkspaceError::Io);
            }
            if bytes[48] == 0 {
                let records = metadata_pages::decode_pieces_leaf(
                    cursor.store.incarnation(),
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
            let (level, children) = metadata_pages::decode_pieces_branch(
                cursor.store.incarnation(),
                page,
                &bytes,
                MAX_FILE,
            )?;
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
            let mut start = lower;
            for (index, child) in children.iter().enumerate() {
                let end = lower.checked_add(child.length).ok_or(WorkspaceError::Io)?;
                if offset < end || index + 1 == children.len() {
                    selected = index;
                    start = lower;
                    break;
                }
                lower = end;
            }
            let child = children[selected];
            cursor.stack[cursor.depth] = Frame {
                page,
                level,
                count: children.len() as u16,
                index: selected as u16,
                start,
                step: child.length,
            };
            cursor.depth += 1;
            page = child.page;
            lower = start;
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
    /// Moves to the next leaf, if the sequence holds one. Every frame already
    /// remembers where its selected child starts and how long it is, so the
    /// first frame with another child is the whole decision: no page above this
    /// leaf is read to find it.
    fn advance(&mut self, window: &mut Window) -> Result<bool, WorkspaceError> {
        clock(self.deadline)?;
        while self.depth > 0 {
            self.depth -= 1;
            let frame = self.stack[self.depth];
            let next = frame.index.saturating_add(1);
            if next < frame.count {
                let start = frame
                    .start
                    .checked_add(frame.step)
                    .ok_or(WorkspaceError::Io)?;
                self.stack[self.depth].index = next;
                return self.step_in(start, window).map(|()| true);
            }
        }
        Ok(false)
    }
    /// Selects the frame's current child one more time and descends the leftmost
    /// path under it, which starts at `start`. Only the page whose child the
    /// step enters is re-read; each level below it is read once as the descent
    /// passes through.
    fn step_in(&mut self, start: u64, window: &mut Window) -> Result<(), WorkspaceError> {
        let frame = self.stack[self.depth];
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
        let child = *children.get(index).ok_or(WorkspaceError::Io)?;
        self.stack[self.depth] = Frame {
            index: frame.index,
            start,
            step: child.length,
            ..frame
        };
        self.depth += 1;
        self.load(child.page, start, window)
    }
    /// Loads the leftmost leaf of the subtree at `page`, which starts at
    /// `lower`, pushing a frame for every branch level it passes.
    fn load(
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
            let child = children[0];
            self.stack[self.depth] = Frame {
                page,
                level,
                count: children.len() as u16,
                index: 0,
                start: lower,
                step: child.length,
            };
            self.depth += 1;
            // The first child starts exactly where its parent's subtree does,
            // so the logical start carries over unchanged.
            page = child.page;
        }
    }
}
