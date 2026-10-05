//! Keyed cell page codec: cells, page bodies and the metadata key grammar.
//!
//! Relocated from `backing/metadata_pages.rs` by slice 3.0 of the phase-3
//! plan. The shared 4 KiB header, `PageRef` and `PageKind` stay there and are
//! imported here, so both specializations read one header definition.
use crate::backing::metadata_pages::{
    body_used, open_page, seal, PageKind, PageRef, HEADER, LEVEL_LIMIT, MAX_CELLS, PAGE,
};
use crate::WorkspaceError;
#[derive(Clone)]
pub struct Cell {
    bytes: [u8; 272],
    pub key_len: usize,
    pub value_len: usize,
}
impl Cell {
    pub fn new(key: &[u8], value: &[u8]) -> Result<Self, WorkspaceError> {
        if key.is_empty() || key.len() > 256 || value.len() > 160 || key.len() + value.len() > 272 {
            return Err(WorkspaceError::Io);
        }
        let mut c = Self {
            bytes: [0; 272],
            key_len: key.len(),
            value_len: value.len(),
        };
        c.bytes[..key.len()].copy_from_slice(key);
        c.bytes[key.len()..key.len() + value.len()].copy_from_slice(value);
        Ok(c)
    }
    pub fn key(&self) -> &[u8] {
        &self.bytes[..self.key_len]
    }
    pub fn value(&self) -> &[u8] {
        &self.bytes[self.key_len..self.key_len + self.value_len]
    }
    pub fn size(&self) -> usize {
        4 + self.key_len + self.value_len
    }
}
pub struct PageData {
    pub level: u8,
    pub cells: Vec<Cell>,
}
impl PageData {
    pub fn body(&self) -> usize {
        self.cells.iter().map(Cell::size).sum()
    }
    pub fn encode(
        &self,
        incarnation: [u8; 32],
        r: PageRef,
        bytes: &mut [u8],
    ) -> Result<(), WorkspaceError> {
        let mut raw = [0; PAGE];
        self.encode_raw(incarnation, r, &mut raw)?;
        bytes.copy_from_slice(&raw);
        Ok(())
    }
    /// The same encoding into the page buffer itself, without an intermediate copy.
    pub fn encode_raw(
        &self,
        incarnation: [u8; 32],
        r: PageRef,
        bytes: &mut [u8],
    ) -> Result<(), WorkspaceError> {
        if bytes.len() != PAGE
            || self.level > LEVEL_LIMIT
            || self.cells.is_empty()
            || self.cells.len() > MAX_CELLS
            || self.body() > PAGE - HEADER
        {
            return Err(WorkspaceError::Capacity);
        }
        bytes.copy_from_slice(&open_page(
            PageKind::Cells,
            self.level,
            self.body(),
            incarnation,
            r,
        )?);
        bytes[50..52].copy_from_slice(&(self.cells.len() as u16).to_be_bytes());
        bytes[52..54].copy_from_slice(&(self.body() as u16).to_be_bytes());
        let mut at = HEADER;
        let mut previous: Option<&[u8]> = None;
        for c in &self.cells {
            if previous.is_some_and(|p| p >= c.key()) {
                return Err(WorkspaceError::Io);
            }
            previous = Some(c.key());
            if self.level > 0 && c.value_len != 8 {
                return Err(WorkspaceError::Io);
            }
            bytes[at..at + 2].copy_from_slice(&(c.key_len as u16).to_be_bytes());
            bytes[at + 2..at + 4].copy_from_slice(&(c.value_len as u16).to_be_bytes());
            at += 4;
            bytes[at..at + c.key_len].copy_from_slice(c.key());
            at += c.key_len;
            bytes[at..at + c.value_len].copy_from_slice(c.value());
            at += c.value_len;
        }
        seal(bytes);
        Ok(())
    }
    pub fn decode(incarnation: [u8; 32], r: PageRef, b: &[u8]) -> Result<Self, WorkspaceError> {
        let (level, cells) = decode_cells_raw(incarnation, r, b)?;
        Ok(Self { level, cells })
    }
}
/// Decodes one already-encoded keyed-cell page body, without copying it first.
pub fn decode_cells_raw(
    incarnation: [u8; 32],
    r: PageRef,
    b: &[u8],
) -> Result<(u8, Vec<Cell>), WorkspaceError> {
    if b.len() != PAGE
        || PageKind::of(b)? != PageKind::Cells
        || b[8..40] != incarnation
        || b[40..48] != r.bytes()
        || b[48] > LEVEL_LIMIT
    {
        return Err(WorkspaceError::Io);
    }
    let used = body_used(b)?;
    let count = u16::from_be_bytes([b[50], b[51]]) as usize;
    if count == 0 || count > MAX_CELLS || usize::from(u16::from_be_bytes([b[52], b[53]])) != used {
        return Err(WorkspaceError::Io);
    }
    let mut cells = crate::backing::metadata_index::vector(count)?;
    let mut at = HEADER;
    for _ in 0..count {
        if at + 4 > HEADER + used {
            return Err(WorkspaceError::Io);
        }
        let k = u16::from_be_bytes([b[at], b[at + 1]]) as usize;
        let v = u16::from_be_bytes([b[at + 2], b[at + 3]]) as usize;
        at += 4;
        if at + k + v > HEADER + used {
            return Err(WorkspaceError::Io);
        }
        let cell = Cell::new(&b[at..at + k], &b[at + k..at + k + v])?;
        if cells.last().is_some_and(|p: &Cell| p.key() >= cell.key()) || (b[48] > 0 && v != 8) {
            return Err(WorkspaceError::Io);
        }
        cells.push(cell);
        at += k + v;
    }
    if at != HEADER + used || b[at..].iter().any(|x| *x != 0) {
        return Err(WorkspaceError::Io);
    }
    Ok((b[48], cells))
}
pub fn inode_key(inode: u64) -> [u8; 9] {
    let mut key = [b'I'; 9];
    key[1..].copy_from_slice(&inode.to_be_bytes());
    key
}
pub fn dirty_key(generation: u64, inode: u64) -> [u8; 17] {
    let mut key = [b'D'; 17];
    key[1..9].copy_from_slice(&generation.to_be_bytes());
    key[9..].copy_from_slice(&inode.to_be_bytes());
    key
}
pub fn result_key(inode: u64) -> [u8; 9] {
    let mut key = [b'R'; 9];
    key[1..].copy_from_slice(&inode.to_be_bytes());
    key
}

pub fn namespace_key(inode: u64) -> [u8; 9] {
    let mut key = [b'N'; 9];
    key[1..].copy_from_slice(&inode.to_be_bytes());
    key
}
pub fn entry_key(name: &[u8]) -> Result<Vec<u8>, WorkspaceError> {
    if name.len() > 255 {
        return Err(WorkspaceError::InvalidInput);
    }
    let mut key = crate::backing::metadata_index::vector(name.len() + 1)?;
    key.push(b'E');
    key.extend_from_slice(name);
    Ok(key)
}
