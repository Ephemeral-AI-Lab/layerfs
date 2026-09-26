use crate::{
    backing::metadata_pages::{PageRef, PieceRecord, MAX_EXTENT},
    NodeAttributes, NodeKind, WorkspaceError,
};
use layerfs_bridge::contract::{Root, MAX_FILE};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PieceKind {
    Base,
    Local,
    Zero,
}
/// One extent of a file's length-indexed sequence. `offset` is the byte offset
/// inside the extent's own origin — a canonical base or a private payload — and
/// the logical start of the extent is derived from its position in the sequence.
#[derive(Clone, Copy, Debug)]
pub struct Piece {
    pub kind: PieceKind,
    pub length: u64,
    pub offset: u64,
    pub payload: u64,
    pub custody: PageRef,
}
impl Piece {
    /// The stored form of this extent. A zero-length extent is refused by the
    /// record parser, so callers never store one.
    pub fn record(self) -> PieceRecord {
        PieceRecord {
            kind: match self.kind {
                PieceKind::Base => PieceRecord::BASE,
                PieceKind::Local => PieceRecord::LOCAL,
                PieceKind::Zero => PieceRecord::ZERO,
            },
            length: self.length,
            offset: self.offset,
            payload: self.payload,
            custody: self.custody,
        }
    }
    pub fn from_record(record: PieceRecord) -> Result<Self, WorkspaceError> {
        let piece = Self {
            kind: match record.kind {
                PieceRecord::BASE => PieceKind::Base,
                PieceRecord::LOCAL => PieceKind::Local,
                PieceRecord::ZERO => PieceKind::Zero,
                _ => return Err(WorkspaceError::Io),
            },
            length: record.length,
            offset: record.offset,
            payload: record.payload,
            custody: record.custody,
        };
        let shaped = match piece.kind {
            PieceKind::Base => piece.payload == 0 && piece.custody == PageRef::NULL,
            PieceKind::Local => piece.payload != 0 && piece.custody != PageRef::NULL,
            PieceKind::Zero => {
                piece.payload == 0 && piece.custody == PageRef::NULL && piece.offset == 0
            }
        };
        if !shaped || piece.length == 0 || piece.length > MAX_EXTENT {
            return Err(WorkspaceError::Io);
        }
        Ok(piece)
    }
    /// One canonical base extent covering `length` bytes from the file start.
    pub fn base(length: u64) -> Self {
        Self {
            kind: PieceKind::Base,
            length,
            offset: 0,
            payload: 0,
            custody: PageRef::NULL,
        }
    }
}
#[derive(Clone, Copy)]
pub struct Inode {
    pub captured: bool,
    pub fresh: bool,
    pub symlink: bool,
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
}
impl Inode {
    pub fn initial(attr: NodeAttributes, base: Root, metadata: Root) -> Self {
        Self {
            captured: false,
            fresh: false,
            symlink: attr.kind == NodeKind::Symlink,
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
        }
    }
    pub fn value(self) -> [u8; 160] {
        let mut b = [0; 160];
        b[24] = u8::from(self.captured);
        b[25] = u8::from(self.fresh);
        b[26] = u8::from(self.symlink);
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
        b
    }
    pub fn parse(b: &[u8]) -> Result<Self, WorkspaceError> {
        if b.len() != 160
            || b[24] > 1
            || b[25] > 1
            || b[26] > 1
            || b[27..32].iter().chain(&b[140..]).any(|v| *v != 0)
        {
            return Err(WorkspaceError::Io);
        }
        let i = Self {
            captured: b[24] == 1,
            fresh: b[25] == 1,
            symlink: b[26] == 1,
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
        };
        if i.revision == 0
            || i.generation == 0
            || i.mode & !0o777 != 0
            || (i.length == 0) != (i.pieces == PageRef::NULL)
            || i.length > MAX_FILE
            || i.base_length > MAX_FILE
            || i.nanos >= 1_000_000_000
            || i.replacement > MAX_FILE
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
        if i.symlink
            && (!i.fresh
                || i.captured
                || i.mode != 0o777
                || i.length > layerfs_bridge::contract::SYMLINK_TARGET_BYTES as u64
                || i.edits != u16::from(i.length > 0))
        {
            return Err(WorkspaceError::Io);
        }
        Ok(i)
    }
    pub fn constructs_file(self) -> bool {
        self.fresh && !self.captured && !self.symlink
    }
    pub fn kind(self) -> NodeKind {
        if self.symlink {
            NodeKind::Symlink
        } else {
            NodeKind::File
        }
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
