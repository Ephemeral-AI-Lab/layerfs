//! One ordered COW page index, shared by inode, generation-dirty and piece roots.
use super::{
    metadata::{Arena, RootOwner},
    metadata_pages::{Cell, PageData, PageRef, HEADER, MAX_CELLS, MIN_BODY, PAGE},
    segments::Window,
};
use crate::{
    overlay::{
        directories::Directory,
        pieces::{Inode, Piece, PieceKind},
    },
    WorkspaceError,
};
use std::time::Instant;
pub fn vector<T>(capacity: usize) -> Result<Vec<T>, WorkspaceError> {
    let mut out = Vec::new();
    out.try_reserve_exact(capacity)
        .map_err(|_| WorkspaceError::Capacity)?;
    if out.capacity() > capacity {
        return Err(WorkspaceError::Capacity);
    }
    Ok(out)
}
pub fn edges(page: &PageData) -> Result<Vec<PageRef>, WorkspaceError> {
    let mut refs = vector(page.cells.len())?;
    for cell in &page.cells {
        let r = if page.level > 0 {
            PageRef::parse(cell.value())?
        } else if cell.key_len == 9 && cell.key()[0] == b'I' {
            Inode::parse(cell.value())?.pieces
        } else if cell.key_len == 9 && cell.key()[0] == b'N' {
            Directory::parse(cell.value())?.entries
        } else if cell.key_len >= 2 && cell.key()[0] == b'E' && cell.value_len == 16 {
            crate::overlay::directories::entry_serial(cell.value())?;
            PageRef::NULL
        } else if cell.key_len == 8 && cell.value_len == 64 {
            let piece = Piece::parse(
                u64::from_be_bytes(cell.key().try_into().map_err(|_| WorkspaceError::Io)?),
                cell.value(),
            )?;
            if piece.kind == PieceKind::Local {
                piece.custody
            } else {
                PageRef::NULL
            }
        } else if (cell.key_len == 9 && cell.key()[0] == b'R' && cell.value_len == 80)
            || (cell.key_len == 17 && cell.key()[0] == b'D' && cell.value() == [1])
        {
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
fn index_limit(key: &[u8]) -> Result<u8, WorkspaceError> {
    match key.first() {
        Some(b'E') if key.len() <= 256 => Ok(3),
        Some(b'D') if key.len() == 17 => Ok(2),
        Some(b'I' | b'N' | b'R') if key.len() == 9 => Ok(2),
        _ => Err(WorkspaceError::Io),
    }
}
fn check_index(cells: &[Cell], level: u8, limit: u8) -> Result<(), WorkspaceError> {
    if level > limit {
        return Err(WorkspaceError::Io);
    }
    for cell in cells {
        if index_limit(cell.key())? != limit || (limit == 3 && cell.key_len < 2) {
            return Err(WorkspaceError::Io);
        }
        if level == 0 {
            let valid = match cell.key()[0] {
                b'E' => cell.value_len == 16,
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
        let limit = index_limit(key)?;
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
    pub fn pieces(
        &self,
        root: PageRef,
        count: u16,
        length: u64,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<Vec<Piece>, WorkspaceError> {
        let mut out = vector(1024)?;
        if root != PageRef::NULL {
            self.collect_pieces(root, None, &mut out, window, deadline)?;
        }
        if out.len() != usize::from(count) {
            return Err(WorkspaceError::Io);
        }
        let mut position = 0u64;
        for p in &out {
            if p.start != position {
                return Err(WorkspaceError::Io);
            }
            position = position.checked_add(p.length).ok_or(WorkspaceError::Io)?;
        }
        if position != length {
            return Err(WorkspaceError::Io);
        }
        Ok(out)
    }
    fn collect_pieces(
        &self,
        r: PageRef,
        expected: Option<u8>,
        out: &mut Vec<Piece>,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<(), WorkspaceError> {
        let page = self.load(r, window, deadline)?;
        if page.level > 1 || expected.is_some_and(|n| n != page.level || page.body() < MIN_BODY) {
            return Err(WorkspaceError::Io);
        }
        for cell in page.cells {
            if page.level == 0 {
                if out.len() == 1024 {
                    return Err(WorkspaceError::Capacity);
                }
                out.push(Piece::parse(
                    u64::from_be_bytes(cell.key().try_into().map_err(|_| WorkspaceError::Io)?),
                    cell.value(),
                )?);
            } else {
                self.collect_pieces(
                    PageRef::parse(cell.value())?,
                    Some(page.level - 1),
                    out,
                    window,
                    deadline,
                )?;
                let maximum =
                    u64::from_be_bytes(cell.key().try_into().map_err(|_| WorkspaceError::Io)?);
                if out.last().is_none_or(|p| p.start + p.length - 1 != maximum) {
                    return Err(WorkspaceError::Io);
                }
            }
        }
        Ok(())
    }
    pub fn piece_at(
        &self,
        mut root: PageRef,
        offset: u64,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<Piece, WorkspaceError> {
        let key = offset.to_be_bytes();
        let mut expected = None;
        let mut coverage: Option<(u64, u64)> = None;
        for _ in 0..2 {
            let page = self.load(root, window, deadline)?;
            if expected.is_some_and(|n| n != page.level || page.body() < MIN_BODY) || page.level > 1
            {
                return Err(WorkspaceError::Io);
            }
            if page.level == 0 {
                if let Some((start, end)) = coverage {
                    let first = page.cells.first().ok_or(WorkspaceError::Io)?;
                    let last = page.cells.last().ok_or(WorkspaceError::Io)?;
                    let first = Piece::parse(
                        u64::from_be_bytes(first.key().try_into().map_err(|_| WorkspaceError::Io)?),
                        first.value(),
                    )?;
                    let last = Piece::parse(
                        u64::from_be_bytes(last.key().try_into().map_err(|_| WorkspaceError::Io)?),
                        last.value(),
                    )?;
                    if first.start != start || last.start + last.length - 1 != end {
                        return Err(WorkspaceError::Io);
                    }
                }
                let cell = page
                    .cells
                    .iter()
                    .rev()
                    .find(|c| c.key() <= key.as_slice())
                    .ok_or(WorkspaceError::Io)?;
                let p = Piece::parse(
                    u64::from_be_bytes(cell.key().try_into().map_err(|_| WorkspaceError::Io)?),
                    cell.value(),
                )?;
                if offset >= p.start + p.length {
                    return Err(WorkspaceError::Io);
                }
                return Ok(p);
            }
            let at = page
                .cells
                .iter()
                .position(|c| c.key() >= key.as_slice())
                .unwrap_or(page.cells.len() - 1);
            let start = if at == 0 {
                0
            } else {
                u64::from_be_bytes(
                    page.cells[at - 1]
                        .key()
                        .try_into()
                        .map_err(|_| WorkspaceError::Io)?,
                )
                .checked_add(1)
                .ok_or(WorkspaceError::Io)?
            };
            let end = u64::from_be_bytes(
                page.cells[at]
                    .key()
                    .try_into()
                    .map_err(|_| WorkspaceError::Io)?,
            );
            coverage = Some((start, end));
            root = PageRef::parse(page.cells[at].value())?;
            expected = Some(page.level - 1);
        }
        Err(WorkspaceError::Io)
    }
}
impl RootOwner {
    pub fn build_pieces(
        &self,
        pieces: &[Piece],
        window: &mut Window,
        deadline: Instant,
    ) -> Result<PageRef, WorkspaceError> {
        if pieces.is_empty() {
            return Ok(PageRef::NULL);
        }
        let leaves = pieces.len().div_ceil(52);
        let each = pieces.len().div_ceil(leaves);
        let mut branches = vector(leaves)?;
        for chunk in pieces.chunks(each) {
            let mut cells = vector(chunk.len())?;
            for p in chunk {
                cells.push(Cell::new(&p.start.to_be_bytes(), &p.value())?)
            }
            let last = chunk.last().ok_or(WorkspaceError::Io)?;
            let maximum = (last.start + last.length - 1).to_be_bytes();
            let r = self.write_page(PageData { level: 0, cells }, window, deadline)?;
            branches.push(Cell::new(&maximum, &r.bytes())?);
        }
        if branches.len() == 1 {
            return PageRef::parse(branches[0].value());
        }
        self.write_page(
            PageData {
                level: 1,
                cells: branches,
            },
            window,
            deadline,
        )
    }
    pub fn update(
        &self,
        root: PageRef,
        updates: Vec<Cell>,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<PageRef, WorkspaceError> {
        let limit = index_limit(updates.first().ok_or(WorkspaceError::InvalidInput)?.key())?;
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
        check_index(&page.cells, page.level, index_limit(lower)?)?;
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
