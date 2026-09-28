//! Indexed node grammar v2: typed leaves, fixed fences and tagged targets.
//!
//! A branch cell carries the exclusive upper key of its child's partition and a
//! tagged target. A cold target names an immutable physical node; a hot target
//! names one slot of the selected directory, so replacing the referenced node
//! leaves every ancestor record unchanged.
use super::page::{Kind, Page, PageRef, BODY_BYTES};
use crate::WorkspaceError;

pub const MAX_KEY: usize = 272;
pub const MAX_VALUE: usize = 512;
pub const TARGET_BYTES: usize = 17;
pub const HOT_SLOTS: usize = 64;
pub const MAX_LEVEL: u8 = 7;
pub const TAG_COLD: u8 = 0;
pub const TAG_HOT: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    pub tag: u8,
    pub first: u64,
    pub epoch: u64,
}

impl Target {
    pub fn cold(page: PageRef) -> Self {
        Self {
            tag: TAG_COLD,
            first: page.id,
            epoch: page.epoch,
        }
    }

    pub fn hot(slot: usize, epoch: u64) -> Result<Self, WorkspaceError> {
        if slot >= HOT_SLOTS || epoch == 0 {
            return Err(WorkspaceError::InvalidInput);
        }
        Ok(Self {
            tag: TAG_HOT,
            first: slot as u64,
            epoch,
        })
    }

    pub fn encode(self) -> Result<[u8; TARGET_BYTES], WorkspaceError> {
        let valid = match self.tag {
            TAG_COLD => self.first != 0,
            TAG_HOT => self.first < HOT_SLOTS as u64,
            _ => false,
        };
        if !valid || self.epoch == 0 {
            return Err(WorkspaceError::InvalidInput);
        }
        let mut bytes = [0; TARGET_BYTES];
        bytes[0] = self.tag;
        bytes[1..9].copy_from_slice(&self.first.to_be_bytes());
        bytes[9..17].copy_from_slice(&self.epoch.to_be_bytes());
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, WorkspaceError> {
        if bytes.len() != TARGET_BYTES {
            return Err(WorkspaceError::Io);
        }
        let number = |at: usize| -> Result<u64, WorkspaceError> {
            Ok(u64::from_be_bytes(
                bytes[at..at + 8]
                    .try_into()
                    .map_err(|_| WorkspaceError::Io)?,
            ))
        };
        let target = Self {
            tag: bytes[0],
            first: number(1)?,
            epoch: number(9)?,
        };
        target.encode().map_err(|_| WorkspaceError::Io)?;
        Ok(target)
    }

    pub fn cold_page(self) -> Result<PageRef, WorkspaceError> {
        if self.tag != TAG_COLD {
            return Err(WorkspaceError::Io);
        }
        Ok(PageRef {
            id: self.first,
            epoch: self.epoch,
        })
    }

    pub fn hot_slot(self) -> Result<usize, WorkspaceError> {
        if self.tag != TAG_HOT {
            return Err(WorkspaceError::Io);
        }
        usize::try_from(self.first)
            .ok()
            .filter(|slot| *slot < HOT_SLOTS)
            .ok_or(WorkspaceError::Io)
    }
}

#[derive(Clone)]
pub struct Cell {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

/// One branch cell: the exclusive upper key of the child's partition, empty
/// only for a final child that inherits its parent's bound, plus its target.
#[derive(Clone)]
pub struct Child {
    pub fence: Vec<u8>,
    pub target: Target,
}

pub enum Node {
    Leaf(Vec<Cell>),
    Branch(Vec<Child>),
}

fn cell_bytes(cell: &Cell) -> usize {
    4 + cell.key.len() + cell.value.len()
}

fn child_bytes(child: &Child) -> usize {
    2 + child.fence.len() + TARGET_BYTES
}

impl Node {
    pub fn kind(&self) -> Kind {
        match self {
            Self::Leaf(_) => Kind::IndexLeaf,
            Self::Branch(_) => Kind::IndexBranch,
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Self::Leaf(cells) => cells.len(),
            Self::Branch(children) => children.len(),
        }
    }

    pub fn body_len(&self) -> usize {
        match self {
            Self::Leaf(cells) => cells.iter().map(cell_bytes).sum(),
            Self::Branch(children) => children.iter().map(child_bytes).sum(),
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>, WorkspaceError> {
        if self.len() == 0 || self.body_len() > BODY_BYTES || self.len() > u16::MAX as usize {
            return Err(WorkspaceError::Capacity);
        }
        let mut body = Vec::with_capacity(self.body_len());
        match self {
            Self::Leaf(cells) => {
                let mut previous: Option<&[u8]> = None;
                for cell in cells {
                    if cell.key.is_empty()
                        || cell.key.len() > MAX_KEY
                        || cell.value.len() > MAX_VALUE
                        || previous.is_some_and(|key| key >= cell.key.as_slice())
                    {
                        return Err(WorkspaceError::InvalidInput);
                    }
                    body.extend_from_slice(&(cell.key.len() as u16).to_be_bytes());
                    body.extend_from_slice(&(cell.value.len() as u16).to_be_bytes());
                    body.extend_from_slice(&cell.key);
                    body.extend_from_slice(&cell.value);
                    previous = Some(&cell.key);
                }
            }
            Self::Branch(children) => {
                let mut previous: Option<&[u8]> = None;
                let last = children.len() - 1;
                for (at, child) in children.iter().enumerate() {
                    if child.fence.len() > MAX_KEY || (at != last && child.fence.is_empty()) {
                        return Err(WorkspaceError::InvalidInput);
                    }
                    if !child.fence.is_empty()
                        && previous.is_some_and(|fence| fence >= child.fence.as_slice())
                    {
                        return Err(WorkspaceError::InvalidInput);
                    }
                    if !child.fence.is_empty() {
                        previous = Some(&child.fence);
                    }
                    body.extend_from_slice(&(child.fence.len() as u16).to_be_bytes());
                    body.extend_from_slice(&child.fence);
                    body.extend_from_slice(&child.target.encode()?);
                }
            }
        }
        Ok(body)
    }

    pub fn decode(
        page: &Page,
        incarnation: [u8; 32],
        reference: PageRef,
        kind: Kind,
    ) -> Result<Self, WorkspaceError> {
        if !kind.indexed() {
            return Err(WorkspaceError::InvalidInput);
        }
        let body = page.verify(kind, incarnation, reference)?;
        let mut at = 0;
        let mut cells = Vec::new();
        let mut children = Vec::new();
        let mut previous: Option<Vec<u8>> = None;
        let count = page.records() as usize;
        for position in 0..count {
            if at + 2 > body.len() {
                return Err(WorkspaceError::Io);
            }
            let key_len = u16::from_be_bytes([body[at], body[at + 1]]) as usize;
            at += 2;
            if key_len > MAX_KEY {
                return Err(WorkspaceError::Io);
            }
            if kind == Kind::IndexLeaf {
                if key_len == 0 || at + 2 > body.len() {
                    return Err(WorkspaceError::Io);
                }
                let value_len = u16::from_be_bytes([body[at], body[at + 1]]) as usize;
                at += 2;
                if value_len > MAX_VALUE || at + key_len + value_len > body.len() {
                    return Err(WorkspaceError::Io);
                }
                let key = body[at..at + key_len].to_vec();
                at += key_len;
                let value = body[at..at + value_len].to_vec();
                at += value_len;
                if previous.as_ref().is_some_and(|old| old >= &key) {
                    return Err(WorkspaceError::Io);
                }
                previous = Some(key.clone());
                cells.push(Cell { key, value });
            } else {
                if at + key_len + TARGET_BYTES > body.len() {
                    return Err(WorkspaceError::Io);
                }
                let fence = body[at..at + key_len].to_vec();
                at += key_len;
                let target = Target::decode(&body[at..at + TARGET_BYTES])?;
                at += TARGET_BYTES;
                let final_child = position + 1 == count;
                if fence.is_empty() && !final_child {
                    return Err(WorkspaceError::Io);
                }
                if !fence.is_empty() && previous.as_ref().is_some_and(|old| old >= &fence) {
                    return Err(WorkspaceError::Io);
                }
                if !fence.is_empty() {
                    previous = Some(fence.clone());
                }
                children.push(Child { fence, target });
            }
        }
        if at != body.len() {
            return Err(WorkspaceError::Io);
        }
        Ok(if kind == Kind::IndexLeaf {
            Self::Leaf(cells)
        } else {
            Self::Branch(children)
        })
    }
}
