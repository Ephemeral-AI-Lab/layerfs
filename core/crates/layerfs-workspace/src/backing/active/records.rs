use super::extents::Extent;
use crate::{NodeKind, WorkspaceError};
use layerfs_bridge::contract::{Root, MAX_FILE};

pub const INODE_BYTES: usize = 416;
pub const NAMESPACE_BYTES: usize = 16;

#[derive(Clone, Copy, Debug)]
pub struct HotInode {
    pub revision: u64,
    pub generation: u64,
    pub length: u64,
    pub kind: NodeKind,
    pub fresh: bool,
    /// 0 empty, 1 up to four inline extents, 2 ordered `E` records.
    pub storage: u8,
    pub mode: u32,
    pub seconds: i64,
    pub nanos: u32,
    pub base: Root,
    pub metadata: Root,
    pub inline: [Option<Extent>; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NamespaceRecord {
    pub serial: u64,
    pub kind: NodeKind,
    pub tombstone: bool,
}

fn kind_byte(kind: NodeKind) -> u8 {
    match kind {
        NodeKind::File => 0,
        NodeKind::Directory => 1,
        NodeKind::Symlink => 2,
    }
}
fn parse_kind(byte: u8) -> Result<NodeKind, WorkspaceError> {
    match byte {
        0 => Ok(NodeKind::File),
        1 => Ok(NodeKind::Directory),
        2 => Ok(NodeKind::Symlink),
        _ => Err(WorkspaceError::Io),
    }
}

pub fn inode_key(serial: u64) -> [u8; 9] {
    let mut key = [0; 9];
    key[0] = b'I';
    key[1..].copy_from_slice(&serial.to_be_bytes());
    key
}
pub fn dirty_key(generation: u64, serial: u64) -> [u8; 17] {
    let mut key = [0; 17];
    key[0] = b'D';
    key[1..9].copy_from_slice(&generation.to_be_bytes());
    key[9..].copy_from_slice(&serial.to_be_bytes());
    key
}
pub fn namespace_key(parent: u64, name: &[u8]) -> Result<Vec<u8>, WorkspaceError> {
    if parent == 0
        || name.is_empty()
        || name.len() > 255
        || name.iter().any(|byte| *byte == 0 || *byte == b'/')
    {
        return Err(WorkspaceError::InvalidInput);
    }
    let mut key = Vec::with_capacity(9 + name.len());
    key.push(b'N');
    key.extend_from_slice(&parent.to_be_bytes());
    key.extend_from_slice(name);
    Ok(key)
}

impl HotInode {
    pub fn value(self) -> Result<[u8; INODE_BYTES], WorkspaceError> {
        self.validate()?;
        let mut bytes = [0; INODE_BYTES];
        bytes[..8].copy_from_slice(&self.revision.to_be_bytes());
        bytes[8..16].copy_from_slice(&self.generation.to_be_bytes());
        bytes[16..24].copy_from_slice(&self.length.to_be_bytes());
        bytes[24] = kind_byte(self.kind);
        bytes[25] = u8::from(self.fresh);
        bytes[26] = self.storage;
        bytes[27] = self.inline.iter().filter(|slot| slot.is_some()).count() as u8;
        bytes[28..32].copy_from_slice(&self.mode.to_be_bytes());
        bytes[32..40].copy_from_slice(&self.seconds.to_be_bytes());
        bytes[40..44].copy_from_slice(&self.nanos.to_be_bytes());
        bytes[48..80].copy_from_slice(&self.base);
        bytes[80..112].copy_from_slice(&self.metadata);
        for (index, extent) in self.inline.iter().enumerate() {
            if let Some(extent) = extent {
                let at = 160 + index * 64;
                bytes[at..at + 8].copy_from_slice(&extent.start.to_be_bytes());
                bytes[at + 8..at + 64].copy_from_slice(&extent.value()?);
            }
        }
        Ok(bytes)
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, WorkspaceError> {
        if bytes.len() != INODE_BYTES
            || bytes[25] > 1
            || bytes[44..48]
                .iter()
                .chain(&bytes[112..160])
                .any(|b| *b != 0)
        {
            return Err(WorkspaceError::Io);
        }
        let number = |at: usize| -> Result<u64, WorkspaceError> {
            Ok(u64::from_be_bytes(
                bytes[at..at + 8]
                    .try_into()
                    .map_err(|_| WorkspaceError::Io)?,
            ))
        };
        let mut inline = [None; 4];
        let count = bytes[27] as usize;
        if count > 4 {
            return Err(WorkspaceError::Io);
        }
        for (index, slot) in inline.iter_mut().enumerate() {
            let at = 160 + index * 64;
            if index < count {
                let start = number(at)?;
                let key = Extent::key(1, start);
                *slot = Some(Extent::parse(&key, &bytes[at + 8..at + 64], 1)?);
            } else if bytes[at..at + 64].iter().any(|b| *b != 0) {
                return Err(WorkspaceError::Io);
            }
        }
        let inode = Self {
            revision: number(0)?,
            generation: number(8)?,
            length: number(16)?,
            kind: parse_kind(bytes[24])?,
            fresh: bytes[25] != 0,
            storage: bytes[26],
            mode: u32::from_be_bytes(bytes[28..32].try_into().map_err(|_| WorkspaceError::Io)?),
            seconds: i64::from_be_bytes(bytes[32..40].try_into().map_err(|_| WorkspaceError::Io)?),
            nanos: u32::from_be_bytes(bytes[40..44].try_into().map_err(|_| WorkspaceError::Io)?),
            base: bytes[48..80].try_into().map_err(|_| WorkspaceError::Io)?,
            metadata: bytes[80..112].try_into().map_err(|_| WorkspaceError::Io)?,
            inline,
        };
        inode.validate()?;
        Ok(inode)
    }

    fn validate(&self) -> Result<(), WorkspaceError> {
        let mode_mask = if self.kind == NodeKind::Directory {
            0o1777
        } else {
            0o777
        };
        if self.revision == 0
            || self.generation == 0
            || self.length > MAX_FILE
            || self.nanos >= 1_000_000_000
            || self.mode & !mode_mask != 0
            || self.storage > 2
            || (self.kind == NodeKind::Directory && self.length != 0)
            || (self.storage == 0 && self.length != 0)
            || (self.storage == 2 && self.inline.iter().any(Option::is_some))
            || (self.storage != 1 && self.inline.iter().any(Option::is_some))
        {
            return Err(WorkspaceError::Io);
        }
        let mut end = 0;
        let mut seen_none = false;
        for extent in self.inline {
            match extent {
                Some(extent) if self.storage == 1 && !seen_none && extent.start == end => {
                    end = extent.end;
                    extent.value()?;
                }
                Some(_) => return Err(WorkspaceError::Io),
                None => seen_none = true,
            }
        }
        if self.storage == 1 && (end != self.length || self.length == 0) {
            return Err(WorkspaceError::Io);
        }
        Ok(())
    }
}

impl NamespaceRecord {
    pub fn value(self) -> Result<[u8; NAMESPACE_BYTES], WorkspaceError> {
        if self.serial == 0 && !self.tombstone {
            return Err(WorkspaceError::InvalidInput);
        }
        let mut bytes = [0; NAMESPACE_BYTES];
        bytes[..8].copy_from_slice(&self.serial.to_be_bytes());
        bytes[8] = kind_byte(self.kind);
        bytes[9] = u8::from(self.tombstone);
        Ok(bytes)
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, WorkspaceError> {
        if bytes.len() != NAMESPACE_BYTES
            || bytes[9] > 1
            || bytes[10..].iter().any(|byte| *byte != 0)
        {
            return Err(WorkspaceError::Io);
        }
        let record = Self {
            serial: u64::from_be_bytes(bytes[..8].try_into().map_err(|_| WorkspaceError::Io)?),
            kind: parse_kind(bytes[8])?,
            tombstone: bytes[9] != 0,
        };
        if record.serial == 0 && !record.tombstone {
            return Err(WorkspaceError::Io);
        }
        Ok(record)
    }
}
