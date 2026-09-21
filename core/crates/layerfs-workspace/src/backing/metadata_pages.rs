//! Immutable metadata page format. Page identities include a checked reuse epoch.
use crate::WorkspaceError;
use sha2::{Digest, Sha256};
pub const PAGE: usize = 4096;
pub const HEADER: usize = 128;
pub const MAX_CELLS: usize = 128;
pub const MIN_BODY: usize = 1024;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PageRef {
    pub slot: u32,
    pub epoch: u32,
}
impl PageRef {
    pub const NULL: Self = Self { slot: 0, epoch: 0 };
    pub fn bytes(self) -> [u8; 8] {
        let mut b = [0; 8];
        b[..4].copy_from_slice(&self.slot.to_be_bytes());
        b[4..].copy_from_slice(&self.epoch.to_be_bytes());
        b
    }
    pub fn parse(b: &[u8]) -> Result<Self, WorkspaceError> {
        if b.len() != 8 {
            return Err(WorkspaceError::Io);
        }
        let r = Self {
            slot: u32::from_be_bytes(b[..4].try_into().map_err(|_| WorkspaceError::Io)?),
            epoch: u32::from_be_bytes(b[4..].try_into().map_err(|_| WorkspaceError::Io)?),
        };
        if (r.slot == 0) != (r.epoch == 0) || r.slot > 65536 {
            return Err(WorkspaceError::Io);
        }
        Ok(r)
    }
}
#[derive(Clone)]
pub struct Cell {
    key: [u8; 17],
    value: [u8; 160],
    pub key_len: usize,
    pub value_len: usize,
}
impl Cell {
    pub fn new(key: &[u8], value: &[u8]) -> Result<Self, WorkspaceError> {
        if key.is_empty() || key.len() > 17 || value.len() > 160 {
            return Err(WorkspaceError::Io);
        }
        let mut c = Self {
            key: [0; 17],
            value: [0; 160],
            key_len: key.len(),
            value_len: value.len(),
        };
        c.key[..key.len()].copy_from_slice(key);
        c.value[..value.len()].copy_from_slice(value);
        Ok(c)
    }
    pub fn key(&self) -> &[u8] {
        &self.key[..self.key_len]
    }
    pub fn value(&self) -> &[u8] {
        &self.value[..self.value_len]
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
        if bytes.len() != PAGE
            || self.level > 7
            || self.cells.is_empty()
            || self.cells.len() > MAX_CELLS
            || self.body() > PAGE - HEADER
        {
            return Err(WorkspaceError::Capacity);
        }
        bytes.fill(0);
        bytes[..8].copy_from_slice(b"LFSWMTA1");
        bytes[8..40].copy_from_slice(&incarnation);
        bytes[40..48].copy_from_slice(&r.bytes());
        bytes[48] = self.level;
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
        verify(b)?;
        if &b[..8] != b"LFSWMTA1"
            || b[8..40] != incarnation
            || b[40..48] != r.bytes()
            || b[48] > 7
            || b[49] != 0
            || b[54..96].iter().any(|x| *x != 0)
        {
            return Err(WorkspaceError::Io);
        }
        let count = u16::from_be_bytes([b[50], b[51]]) as usize;
        let used = u16::from_be_bytes([b[52], b[53]]) as usize;
        if count == 0 || count > MAX_CELLS || used > PAGE - HEADER {
            return Err(WorkspaceError::Io);
        }
        let mut cells = super::metadata_index::vector(count)?;
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
        Ok(Self {
            level: b[48],
            cells,
        })
    }
}
pub fn seal(bytes: &mut [u8]) {
    bytes[96..128].fill(0);
    let hash = Sha256::digest(&*bytes);
    bytes[96..128].copy_from_slice(&hash);
}
pub fn verify(bytes: &[u8]) -> Result<(), WorkspaceError> {
    if bytes.len() != PAGE {
        return Err(WorkspaceError::Io);
    }
    let mut hasher = Sha256::new();
    hasher.update(&bytes[..96]);
    hasher.update([0; 32]);
    hasher.update(&bytes[128..]);
    if hasher.finalize().as_slice() != &bytes[96..128] {
        return Err(WorkspaceError::Io);
    }
    Ok(())
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
