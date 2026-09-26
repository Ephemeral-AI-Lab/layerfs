//! One ordered COW page index, shared by inode, generation-dirty and piece roots.
use super::{
    metadata::{Arena, RootOwner},
    metadata_pages::{
        self, Cell, PageData, PageKind, PageRef, PieceRecord, HEADER, MAX_CELLS, MIN_BODY, PAGE,
    },
    segments::Window,
};
use crate::{
    overlay::{directories::Directory, pieces::Inode},
    WorkspaceError,
};
use std::{sync::Arc, time::Instant};
pub fn vector<T>(capacity: usize) -> Result<Vec<T>, WorkspaceError> {
    let mut out = Vec::new();
    out.try_reserve_exact(capacity)
        .map_err(|_| WorkspaceError::Capacity)?;
    if out.capacity() > capacity {
        return Err(WorkspaceError::Capacity);
    }
    Ok(out)
}
/// Every page reference one already-encoded page body holds: the child pages of
/// a branch, and — for a piece leaf — the custody page of each `Local` extent.
/// A responsibility edge must be counted once for every field that names it, or
/// a page whose only reference is such a field never reaches zero and is never
/// freed.
pub fn edges_raw(
    incarnation: [u8; 32],
    r: PageRef,
    bytes: &[u8],
) -> Result<Vec<PageRef>, WorkspaceError> {
    match PageKind::of(bytes)? {
        PageKind::Cells => cells_edges(&PageData::decode(incarnation, r, bytes)?),
        PageKind::Pieces => {
            if bytes[48] == 0 {
                let records = metadata_pages::decode_pieces_leaf(
                    incarnation,
                    r,
                    bytes,
                    layerfs_bridge::contract::MAX_FILE,
                )?;
                let mut refs = vector(records.len())?;
                for record in records {
                    if record.kind == PieceRecord::LOCAL {
                        refs.push(record.custody);
                    }
                }
                Ok(refs)
            } else {
                let (_, children) = metadata_pages::decode_pieces_branch(
                    incarnation,
                    r,
                    bytes,
                    layerfs_bridge::contract::MAX_FILE,
                )?;
                let mut refs = vector(children.len())?;
                for child in children {
                    refs.push(child.page);
                }
                Ok(refs)
            }
        }
    }
}
fn cells_edges(page: &PageData) -> Result<Vec<PageRef>, WorkspaceError> {
    let mut refs = vector(page.cells.len() * 2)?;
    for cell in &page.cells {
        let r = if page.level > 0 {
            PageRef::parse(cell.value())?
        } else if cell.key_len == 9 && cell.key()[0] == b'I' {
            Inode::parse(cell.value())?.pieces
        } else if cell.key_len == 9 && cell.key()[0] == b'N' {
            let directory = Directory::parse(cell.value())?;
            if directory.tombstones != PageRef::NULL {
                refs.push(directory.tombstones);
            }
            directory.entries
        } else if cell.key_len >= 2 && cell.key()[0] == b'E' && cell.value_len == 16 {
            crate::overlay::directories::entry_serial(cell.value())?;
            PageRef::NULL
        } else if cell.key_len >= 2 && cell.key()[0] == b'T' && cell.value() == [1] {
            crate::overlay::directories::tombstone(cell.key())?;
            PageRef::NULL
        } else if (cell.key_len == 9 && cell.key()[0] == b'R' && cell.value_len == 80)
            || (cell.key_len == 17 && cell.key()[0] == b'D' && cell.value() == [1])
        {
            // A result record and a dirty marker name no page of their own.
            PageRef::NULL
        } else {
            return Err(WorkspaceError::Io);
        };
        if r != PageRef::NULL {
            refs.push(r)
        }
    }
    Ok(refs)
}
pub fn edges(page: &PageData) -> Result<Vec<PageRef>, WorkspaceError> {
    cells_edges(page)
}
/// The declared level ceiling of one metadata key kind. Every metadata key is a
/// one-byte kind followed by its identity, except a name kind's cursor bound,
/// which may carry the kind byte alone and no name at all.
pub(crate) fn key_limit(key: &[u8]) -> Result<u8, WorkspaceError> {
    match key.first() {
        Some(b'E' | b'T') if key.len() <= 256 => Ok(3),
        Some(b'D') if key.len() == 17 => Ok(2),
        Some(b'I' | b'N' | b'R') if key.len() == 9 => Ok(2),
        _ => Err(WorkspaceError::Io),
    }
}
/// The level ceiling of one key that is written into a page, where a name kind
/// must carry at least one name byte: a bare kind byte is only a cursor bound.
pub(crate) fn stored_key_limit(key: &[u8]) -> Result<u8, WorkspaceError> {
    let limit = key_limit(key)?;
    if limit == 3 && key.len() < 2 {
        return Err(WorkspaceError::Io);
    }
    Ok(limit)
}
fn check_index(cells: &[Cell], level: u8, limit: u8) -> Result<(), WorkspaceError> {
    if level > limit {
        return Err(WorkspaceError::Io);
    }
    for cell in cells {
        if stored_key_limit(cell.key())? != limit {
            return Err(WorkspaceError::Io);
        }
        if level == 0 {
            let valid = match cell.key()[0] {
                b'E' => cell.value_len == 16,
                b'T' => cell.value_len == 1 && cell.value() == [1] && cell.key_len >= 2,
                b'I' => cell.value_len == 160,
                b'N' => cell.value_len == 128,
                b'R' => cell.value_len == 80,
                b'D' => cell.value() == [1],
                _ => false,
            };
            if !valid {
                return Err(WorkspaceError::Io);
            }
        }
    }
    Ok(())
}
impl Arena {
    pub fn find(
        &self,
        mut root: PageRef,
        key: &[u8],
        window: &mut Window,
        deadline: Instant,
    ) -> Result<Option<Cell>, WorkspaceError> {
        if root == PageRef::NULL {
            return Ok(None);
        }
        let limit = key_limit(key)?;
        let mut expected = None;
        let mut maximum: Option<Cell> = None;
        for _ in 0..=limit {
            let page = self.load(root, window, deadline)?;
            check_index(&page.cells, page.level, limit)?;
            if expected.is_some_and(|level| level != page.level || page.body() < MIN_BODY)
                || maximum
                    .as_ref()
                    .is_some_and(|c| page.cells.last().is_none_or(|last| last.key() != c.key()))
            {
                return Err(WorkspaceError::Io);
            }
            if page.level == 0 {
                return Ok(page.cells.into_iter().find(|c| c.key() == key));
            }
            let at = page
                .cells
                .iter()
                .position(|c| c.key() >= key)
                .unwrap_or(page.cells.len() - 1);
            maximum = Some(page.cells[at].clone());
            root = PageRef::parse(page.cells[at].value())?;
            if root == PageRef::NULL {
                return Err(WorkspaceError::Io);
            }
            expected = Some(page.level - 1);
        }
        Err(WorkspaceError::Io)
    }
}
impl Arena {
    /// One file's extent that covers `offset`, with its derived logical start.
    pub fn piece_at(
        self: &Arc<Self>,
        root: PageRef,
        offset: u64,
        length: u64,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<(u64, crate::overlay::pieces::Piece), WorkspaceError> {
        crate::backing::metadata_pieces::piece_at(
            self.clone(),
            root,
            offset,
            length,
            window,
            deadline,
        )
    }
    /// A bounded ordered cursor over one file's extents. The cursor owns the
    /// arena handle it reads through, so a caller can keep one walk across
    /// several transport pulls.
    pub fn cursor(
        self: &Arc<Self>,
        root: PageRef,
        offset: u64,
        length: u64,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<crate::backing::metadata_pieces::Cursor<Arc<Self>>, WorkspaceError> {
        crate::backing::metadata_pieces::cursor(
            self.clone(),
            root,
            offset,
            length,
            window,
            deadline,
        )
    }
}
impl RootOwner {
    pub fn update(
        &self,
        root: PageRef,
        updates: Vec<Cell>,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<PageRef, WorkspaceError> {
        let limit = key_limit(updates.first().ok_or(WorkspaceError::InvalidInput)?.key())?;
        if updates.len() > if limit == 3 { 1 } else { 4 }
            || updates
                .windows(2)
                .any(|pair| pair[0].key() >= pair[1].key())
        {
            return Err(WorkspaceError::Capacity);
        }
        check_index(&updates, 0, limit)?;
        let result = self.update_node(root, None, updates, limit, window, deadline)?;
        if result.len() == 1 {
            return PageRef::parse(result[0].value());
        }
        let level = self
            .arena
            .load(PageRef::parse(result[0].value())?, window, deadline)?
            .level
            .checked_add(1)
            .ok_or(WorkspaceError::Capacity)?;
        if level > limit {
            return Err(WorkspaceError::Capacity);
        }
        self.write_page(
            PageData {
                level,
                cells: result,
            },
            window,
            deadline,
        )
    }
    fn update_node(
        &self,
        root: PageRef,
        expected: Option<u8>,
        updates: Vec<Cell>,
        limit: u8,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<Vec<Cell>, WorkspaceError> {
        let page = if root == PageRef::NULL {
            PageData {
                level: 0,
                cells: vector(0)?,
            }
        } else {
            self.arena.load(root, window, deadline)?
        };
        check_index(&page.cells, page.level, limit)?;
        if expected.is_some_and(|n| n != page.level || page.body() < MIN_BODY) {
            return Err(WorkspaceError::Io);
        }
        let level = page.level;
        let mut cells = vector(page.cells.len() + updates.len() + 2)?;
        if level == 0 {
            let mut old = page.cells.into_iter().peekable();
            let mut changes = updates.into_iter().peekable();
            while old.peek().is_some() || changes.peek().is_some() {
                match (old.peek(), changes.peek()) {
                    (Some(a), Some(b)) if a.key() < b.key() => {
                        cells.push(old.next().ok_or(WorkspaceError::Io)?)
                    }
                    (Some(a), Some(b)) if a.key() == b.key() => {
                        old.next();
                        cells.push(changes.next().ok_or(WorkspaceError::Io)?);
                    }
                    (_, Some(_)) => cells.push(changes.next().ok_or(WorkspaceError::Io)?),
                    (Some(_), None) => cells.push(old.next().ok_or(WorkspaceError::Io)?),
                    _ => break,
                }
            }
        } else {
            let last = page.cells.len() - 1;
            let update_count = updates.len();
            let mut changes = updates.into_iter().peekable();
            for (index, cell) in page.cells.into_iter().enumerate() {
                let mut subset = vector(update_count)?;
                while changes
                    .peek()
                    .is_some_and(|change| index == last || change.key() <= cell.key())
                {
                    subset.push(changes.next().ok_or(WorkspaceError::Io)?);
                }
                if subset.is_empty() {
                    cells.push(cell)
                } else {
                    let child = PageRef::parse(cell.value())?;
                    for updated in
                        self.update_node(child, Some(level - 1), subset, limit, window, deadline)?
                    {
                        cells.push(updated)
                    }
                }
            }
            if changes.peek().is_some() {
                return Err(WorkspaceError::Io);
            }
        }
        let used: usize = cells.iter().map(Cell::size).sum();
        let mut pages = vector(2)?;
        if used <= PAGE - HEADER && cells.len() <= MAX_CELLS {
            let max = cells.last().ok_or(WorkspaceError::Io)?.clone();
            let r = self.write_page(PageData { level, cells }, window, deadline)?;
            pages.push(Cell::new(max.key(), &r.bytes())?);
        } else {
            let mut bytes = 0;
            let mut cut = 0;
            for c in &cells {
                if bytes >= used / 2 {
                    break;
                }
                bytes += c.size();
                cut += 1;
            }
            let mut right = vector(cells.len() - cut)?;
            right.extend(cells.drain(cut..));
            for part in [cells, right] {
                let data = PageData { level, cells: part };
                if data.body() < MIN_BODY {
                    return Err(WorkspaceError::Io);
                }
                let max = data.cells.last().ok_or(WorkspaceError::Io)?.clone();
                let r = self.write_page(data, window, deadline)?;
                pages.push(Cell::new(max.key(), &r.bytes())?);
            }
        }
        Ok(pages)
    }
}
impl Arena {
    /// Bounded successor lookup, without retaining a vector of frontier records.
    pub fn next(
        &self,
        root: PageRef,
        lower: &[u8],
        exclusive: bool,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<Option<Cell>, WorkspaceError> {
        if root == PageRef::NULL {
            return Ok(None);
        }
        self.next_in(root, None, lower, exclusive, window, deadline)
    }
    fn next_in(
        &self,
        root: PageRef,
        parent: Option<(u8, &[u8])>,
        lower: &[u8],
        exclusive: bool,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<Option<Cell>, WorkspaceError> {
        let page = self.load(root, window, deadline)?;
        check_index(&page.cells, page.level, key_limit(lower)?)?;
        if parent.is_some_and(|(n, key)| {
            n != page.level
                || page.body() < MIN_BODY
                || page.cells.last().is_none_or(|last| last.key() != key)
        }) {
            return Err(WorkspaceError::Io);
        }
        if page.level == 0 {
            return Ok(page.cells.into_iter().find(|cell| {
                if exclusive {
                    cell.key() > lower
                } else {
                    cell.key() >= lower
                }
            }));
        }
        for cell in page.cells.iter().filter(|cell| {
            if exclusive {
                cell.key() > lower
            } else {
                cell.key() >= lower
            }
        }) {
            if let Some(found) = self.next_in(
                PageRef::parse(cell.value())?,
                Some((page.level - 1, cell.key())),
                lower,
                exclusive,
                window,
                deadline,
            )? {
                return Ok(Some(found));
            }
        }
        Ok(None)
    }
}
