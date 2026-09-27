use super::page::{Kind, Page, PageRef, BODY_BYTES};
use crate::WorkspaceError;

const MAX_KEY: usize = 272;
const MAX_VALUE: usize = 512;

#[derive(Clone)]
pub struct Cell {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

#[derive(Clone)]
pub struct Child {
    pub max: Vec<u8>,
    pub page: PageRef,
}

pub enum Node {
    Leaf(Vec<Cell>),
    Branch(Vec<Child>),
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

    pub fn max(&self) -> &[u8] {
        match self {
            Self::Leaf(cells) => &cells.last().expect("nonempty node").key,
            Self::Branch(children) => &children.last().expect("nonempty node").max,
        }
    }

    pub fn body_len(&self) -> usize {
        match self {
            Self::Leaf(cells) => cells
                .iter()
                .map(|cell| 4 + cell.key.len() + cell.value.len())
                .sum(),
            Self::Branch(children) => children.iter().map(|child| 2 + child.max.len() + 16).sum(),
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>, WorkspaceError> {
        if self.len() == 0 || self.body_len() > BODY_BYTES || self.len() > u16::MAX as usize {
            return Err(WorkspaceError::Capacity);
        }
        let mut body = Vec::with_capacity(self.body_len());
        let mut previous: Option<&[u8]> = None;
        match self {
            Self::Leaf(cells) => {
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
                for child in children {
                    if child.max.is_empty()
                        || child.max.len() > MAX_KEY
                        || child.page.id == 0
                        || child.page.epoch == 0
                        || previous.is_some_and(|key| key >= child.max.as_slice())
                    {
                        return Err(WorkspaceError::InvalidInput);
                    }
                    body.extend_from_slice(&(child.max.len() as u16).to_be_bytes());
                    body.extend_from_slice(&child.max);
                    body.extend_from_slice(&child.page.id.to_be_bytes());
                    body.extend_from_slice(&child.page.epoch.to_be_bytes());
                    previous = Some(&child.max);
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
        if !matches!(kind, Kind::IndexLeaf | Kind::IndexBranch) {
            return Err(WorkspaceError::InvalidInput);
        }
        let body = page.verify(kind, incarnation, reference)?;
        let mut at = 0;
        let mut cells = Vec::new();
        let mut children = Vec::new();
        let mut previous: Option<Vec<u8>> = None;
        for _ in 0..page.records() {
            if at + 2 > body.len() {
                return Err(WorkspaceError::Io);
            }
            let key_len = u16::from_be_bytes([body[at], body[at + 1]]) as usize;
            at += 2;
            if key_len == 0 || key_len > MAX_KEY {
                return Err(WorkspaceError::Io);
            }
            if kind == Kind::IndexLeaf {
                if at + 2 > body.len() {
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
                if at + key_len + 16 > body.len() {
                    return Err(WorkspaceError::Io);
                }
                let max = body[at..at + key_len].to_vec();
                at += key_len;
                let id = u64::from_be_bytes(
                    body[at..at + 8]
                        .try_into()
                        .map_err(|_| WorkspaceError::Io)?,
                );
                let epoch = u64::from_be_bytes(
                    body[at + 8..at + 16]
                        .try_into()
                        .map_err(|_| WorkspaceError::Io)?,
                );
                at += 16;
                if id == 0 || epoch == 0 || previous.as_ref().is_some_and(|old| old >= &max) {
                    return Err(WorkspaceError::Io);
                }
                previous = Some(max.clone());
                children.push(Child {
                    max,
                    page: PageRef { id, epoch },
                });
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
