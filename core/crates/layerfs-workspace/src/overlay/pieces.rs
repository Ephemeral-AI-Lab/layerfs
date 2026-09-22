use crate::{backing::metadata_pages::PageRef, NodeAttributes, WorkspaceError};
use layerfs_bridge::contract::{Root, MAX_FILE};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PieceKind {
    Base,
    Local,
    Zero,
}
#[derive(Clone, Copy, Debug)]
pub struct Piece {
    pub kind: PieceKind,
    pub start: u64,
    pub length: u64,
    pub offset: u64,
    pub payload: u64,
    pub custody: PageRef,
}
impl Piece {
    pub fn value(self) -> [u8; 64] {
        let mut b = [0; 64];
        b[0] = match self.kind {
            PieceKind::Base => 0,
            PieceKind::Local => 1,
            PieceKind::Zero => 2,
        };
        b[8..16].copy_from_slice(&self.length.to_be_bytes());
        b[16..24].copy_from_slice(&self.offset.to_be_bytes());
        b[24..32].copy_from_slice(&self.payload.to_be_bytes());
        b[32..40].copy_from_slice(&self.custody.bytes());
        b
    }
    pub fn parse(start: u64, b: &[u8]) -> Result<Self, WorkspaceError> {
        if b.len() != 64 || b[0] > 2 || b[1..8].iter().chain(&b[40..]).any(|v| *v != 0) {
            return Err(WorkspaceError::Io);
        }
        let p = Self {
            kind: match b[0] {
                0 => PieceKind::Base,
                1 => PieceKind::Local,
                2 => PieceKind::Zero,
                _ => return Err(WorkspaceError::Io),
            },
            start,
            length: get(b, 8)?,
            offset: get(b, 16)?,
            payload: get(b, 24)?,
            custody: PageRef::parse(&b[32..40])?,
        };
        if p.length == 0
            || match p.kind {
                PieceKind::Base => p.payload != 0 || p.custody != PageRef::NULL,
                PieceKind::Local => p.payload == 0 || p.custody == PageRef::NULL,
                PieceKind::Zero => p.payload != 0 || p.custody != PageRef::NULL || p.offset != 0,
            }
            || p.offset.checked_add(p.length).is_none_or(|n| n > MAX_FILE)
            || p.start.checked_add(p.length).is_none_or(|n| n > MAX_FILE)
        {
            return Err(WorkspaceError::Io);
        }
        Ok(p)
    }
}
#[derive(Clone, Copy)]
pub struct Inode {
    pub captured: bool,
    pub fresh: bool,
    pub revision: u64,
    pub length: u64,
    pub base_length: u64,
    pub base: Root,
    pub metadata: Root,
    pub pieces: PageRef,
    pub generation: u64,
    pub mode: u32,
    pub seconds: i64,
    pub nanos: u32,
    pub replacement: u64,
    pub edits: u16,
    pub count: u16,
}
impl Inode {
    pub fn initial(attr: NodeAttributes, base: Root, metadata: Root) -> Self {
        Self {
            captured: false,
            fresh: false,
            revision: 0,
            length: attr.size,
            base_length: attr.size,
            base,
            metadata,
            pieces: PageRef::NULL,
            generation: 1,
            mode: attr.mode,
            seconds: attr.mtime_seconds,
            nanos: attr.mtime_nanoseconds,
            replacement: 0,
            edits: 0,
            count: 0,
        }
    }
    pub fn value(self) -> [u8; 160] {
        let mut b = [0; 160];
        b[24] = u8::from(self.captured);
        b[25] = u8::from(self.fresh);
        for (at, value) in [
            (0, self.revision),
            (8, self.length),
            (16, self.base_length),
            (104, self.generation),
            (128, self.replacement),
        ] {
            b[at..at + 8].copy_from_slice(&value.to_be_bytes());
        }
        b[32..64].copy_from_slice(&self.base);
        b[64..96].copy_from_slice(&self.metadata);
        b[96..104].copy_from_slice(&self.pieces.bytes());
        b[112..116].copy_from_slice(&self.mode.to_be_bytes());
        b[116..124].copy_from_slice(&self.seconds.to_be_bytes());
        b[124..128].copy_from_slice(&self.nanos.to_be_bytes());
        b[136..138].copy_from_slice(&self.edits.to_be_bytes());
        b[138..140].copy_from_slice(&self.count.to_be_bytes());
        b
    }
    pub fn parse(b: &[u8]) -> Result<Self, WorkspaceError> {
        if b.len() != 160
            || b[24] > 1
            || b[25] > 1
            || b[26..32].iter().chain(&b[140..]).any(|v| *v != 0)
        {
            return Err(WorkspaceError::Io);
        }
        let i = Self {
            captured: b[24] == 1,
            fresh: b[25] == 1,
            revision: get(b, 0)?,
            length: get(b, 8)?,
            base_length: get(b, 16)?,
            base: b[32..64].try_into().map_err(|_| WorkspaceError::Io)?,
            metadata: b[64..96].try_into().map_err(|_| WorkspaceError::Io)?,
            pieces: PageRef::parse(&b[96..104])?,
            generation: get(b, 104)?,
            mode: u32::from_be_bytes(b[112..116].try_into().map_err(|_| WorkspaceError::Io)?),
            seconds: i64::from_be_bytes(b[116..124].try_into().map_err(|_| WorkspaceError::Io)?),
            nanos: u32::from_be_bytes(b[124..128].try_into().map_err(|_| WorkspaceError::Io)?),
            replacement: get(b, 128)?,
            edits: u16::from_be_bytes([b[136], b[137]]),
            count: u16::from_be_bytes([b[138], b[139]]),
        };
        if i.revision == 0
            || i.generation == 0
            || i.mode & !0o777 != 0
            || (i.length == 0) != (i.count == 0)
            || (i.pieces == PageRef::NULL) != (i.count == 0)
            || i.length > MAX_FILE
            || i.base_length > MAX_FILE
            || i.nanos >= 1_000_000_000
            || i.count > 1024
            || i.edits > 256
            || i.replacement > 8 * 1024 * 1024
            || (i.fresh && i.metadata != [0; 32])
        {
            return Err(WorkspaceError::Io);
        }
        if i.captured {
            CapturedBase::parse(i.base)?;
        } else if i.fresh && (i.base != [0; 32] || i.base_length != 0 || i.replacement != i.length)
        {
            return Err(WorkspaceError::Io);
        }
        Ok(i)
    }
    pub fn attributes(self, mut a: NodeAttributes) -> NodeAttributes {
        a.size = self.length;
        a.mode = self.mode;
        a.mtime_seconds = self.seconds;
        a.mtime_nanoseconds = self.nanos;
        a
    }
}
pub fn get(b: &[u8], at: usize) -> Result<u64, WorkspaceError> {
    Ok(u64::from_be_bytes(
        b.get(at..at + 8)
            .ok_or(WorkspaceError::Io)?
            .try_into()
            .map_err(|_| WorkspaceError::Io)?,
    ))
}
pub fn splice(
    old: &[Piece],
    start: u64,
    end: u64,
    replacement: &[Piece],
    base_length: u64,
) -> Result<(Vec<Piece>, u16, u64), WorkspaceError> {
    let mut new = crate::backing::metadata_index::vector(1024)?;
    let mut position = 0u64;
    let mut push = |mut p: Piece| -> Result<(), WorkspaceError> {
        if p.length == 0 {
            return Ok(());
        }
        p.start = position;
        position = position
            .checked_add(p.length)
            .ok_or(WorkspaceError::Capacity)?;
        if position > MAX_FILE {
            return Err(WorkspaceError::Capacity);
        }
        if let Some(last) = new.last_mut() {
            let last: &mut Piece = last;
            if last.kind == p.kind
                && last.payload == p.payload
                && last.custody == p.custody
                && (p.kind == PieceKind::Zero
                    || last.offset.checked_add(last.length) == Some(p.offset))
            {
                last.length = last
                    .length
                    .checked_add(p.length)
                    .ok_or(WorkspaceError::Capacity)?;
                return Ok(());
            }
        }
        if new.len() == 1024 {
            return Err(WorkspaceError::Capacity);
        }
        new.push(p);
        Ok(())
    };
    for p in old {
        if p.start >= start {
            break;
        }
        let mut part = *p;
        part.length = part.length.min(start - p.start);
        push(part)?;
    }
    for piece in replacement {
        push(*piece)?;
    }
    for p in old {
        let stop = p.start.checked_add(p.length).ok_or(WorkspaceError::Io)?;
        if stop <= end {
            continue;
        }
        let mut part = *p;
        let skip = end.saturating_sub(p.start);
        if part.kind != PieceKind::Zero {
            part.offset = part.offset.checked_add(skip).ok_or(WorkspaceError::Io)?;
        }
        part.length -= skip;
        push(part)?;
    }
    let mut expected = 0;
    let mut pending = 0u64;
    let mut edits = 0u16;
    let mut bytes = 0u64;
    for p in &new {
        if p.kind != PieceKind::Base {
            pending = pending
                .checked_add(p.length)
                .ok_or(WorkspaceError::Capacity)?;
            bytes = bytes
                .checked_add(p.length)
                .ok_or(WorkspaceError::Capacity)?;
        } else {
            if p.offset < expected || p.offset + p.length > base_length {
                return Err(WorkspaceError::Io);
            }
            if p.offset != expected || pending != 0 {
                edits = edits.checked_add(1).ok_or(WorkspaceError::Capacity)?;
            }
            expected = p.offset + p.length;
            pending = 0;
        }
    }
    if expected != base_length || pending != 0 {
        edits = edits.checked_add(1).ok_or(WorkspaceError::Capacity)?;
    }
    if edits > 256 || bytes > 8 * 1024 * 1024 {
        return Err(WorkspaceError::Capacity);
    }
    Ok((new, edits, bytes))
}

#[derive(Clone, Copy)]
pub struct CapturedBase {
    pub root: PageRef,
    pub inode: u64,
    pub generation: u64,
    pub revision: u64,
}
impl CapturedBase {
    pub fn bytes(self) -> Root {
        let mut bytes = [0; 32];
        bytes[..8].copy_from_slice(&self.root.bytes());
        bytes[8..16].copy_from_slice(&self.inode.to_be_bytes());
        bytes[16..24].copy_from_slice(&self.generation.to_be_bytes());
        bytes[24..].copy_from_slice(&self.revision.to_be_bytes());
        bytes
    }
    pub fn parse(bytes: Root) -> Result<Self, WorkspaceError> {
        let value = Self {
            root: PageRef::parse(&bytes[..8])?,
            inode: get(&bytes, 8)?,
            generation: get(&bytes, 16)?,
            revision: get(&bytes, 24)?,
        };
        if value.root == PageRef::NULL
            || value.inode == 0
            || value.generation == 0
            || value.revision == 0
        {
            return Err(WorkspaceError::Io);
        }
        Ok(value)
    }
}
