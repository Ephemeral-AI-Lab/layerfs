//! Immutable metadata page format. Page identities include a checked reuse epoch.
use crate::WorkspaceError;
use layerfs_bridge::contract::MAX_FILE;
use sha2::{Digest, Sha256};
pub const PAGE: usize = 4096;
pub const HEADER: usize = 128;
pub const MAX_CELLS: usize = 128;
pub const MIN_BODY: usize = 1024;
/// Declared format of the payload area of one page. A page states its own
/// version, so an older format is refused by its reader instead of being
/// reinterpreted under newer rules.
pub const FORMAT_CELLS: u8 = 0;
pub const FORMAT_PIECES: u8 = 1;
/// One piece record of a length-indexed piece page; a leaf holds a whole
/// number of these in its payload area.
pub const RECORD: usize = 32;
/// Largest logical length one extent may describe. An extent's start is
/// derived from its position in the sequence, so it is not stored.
pub const MAX_EXTENT: u64 = (1 << 24) - 1;
/// Declared structural ceiling of one page tree.
pub const LEVEL_LIMIT: u8 = 7;
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
/// One packed extent of a length-indexed piece leaf. `offset` is the byte offset
/// inside the extent's own origin: a canonical base or a private payload.
#[derive(Clone, Copy, Debug)]
pub struct PieceRecord {
    pub kind: u8,
    pub length: u64,
    pub offset: u64,
    pub payload: u64,
    pub custody: PageRef,
}
impl PieceRecord {
    pub const BASE: u8 = 0;
    pub const LOCAL: u8 = 1;
    pub const ZERO: u8 = 2;
    pub fn bytes(self) -> [u8; RECORD] {
        let mut b = [0; RECORD];
        b[0] = self.kind;
        b[1..4].copy_from_slice(&(self.length as u32).to_be_bytes()[1..]);
        b[4..12].copy_from_slice(&self.offset.to_be_bytes());
        b[12..20].copy_from_slice(&self.payload.to_be_bytes());
        b[20..28].copy_from_slice(&self.custody.bytes());
        b
    }
    /// Parses one record. `bounded` is the logical length of the sequence the
    /// record belongs to, so an extent can never describe bytes beyond it.
    pub fn parse(b: &[u8], bounded: u64) -> Result<Self, WorkspaceError> {
        if b.len() != RECORD || b[0] > Self::ZERO || b[28..].iter().any(|v| *v != 0) {
            return Err(WorkspaceError::Io);
        }
        let value = Self {
            kind: b[0],
            length: u64::from(u32::from_be_bytes([0, b[1], b[2], b[3]])),
            offset: u64::from_be_bytes(b[4..12].try_into().map_err(|_| WorkspaceError::Io)?),
            payload: u64::from_be_bytes(b[12..20].try_into().map_err(|_| WorkspaceError::Io)?),
            custody: PageRef::parse(&b[20..28])?,
        };
        let shaped = match value.kind {
            Self::BASE => value.payload == 0 && value.custody == PageRef::NULL,
            Self::LOCAL => value.payload != 0 && value.custody != PageRef::NULL,
            _ => value.payload == 0 && value.custody == PageRef::NULL && value.offset == 0,
        };
        if !shaped
            || value.length == 0
            || value.length > MAX_EXTENT
            || value.length > bounded
            || value
                .offset
                .checked_add(value.length)
                .is_none_or(|end| end > MAX_EXTENT.max((1 << 32) - 1))
        {
            return Err(WorkspaceError::Io);
        }
        Ok(value)
    }
}
/// One child reference of a length-indexed branch page: the child page and the
/// logical byte length of everything the child covers. A seek subtracts those
/// lengths, so an insertion moves a suffix without rewriting its pages.
#[derive(Clone, Copy, Debug)]
pub struct ChildRef {
    pub page: PageRef,
    pub length: u64,
}
impl ChildRef {
    pub const BYTES: usize = 16;
    pub fn bytes(self) -> [u8; Self::BYTES] {
        let mut b = [0; Self::BYTES];
        b[..8].copy_from_slice(&self.page.bytes());
        b[8..].copy_from_slice(&self.length.to_be_bytes());
        b
    }
    pub fn parse(b: &[u8]) -> Result<Self, WorkspaceError> {
        if b.len() != Self::BYTES {
            return Err(WorkspaceError::Io);
        }
        let value = Self {
            page: PageRef::parse(&b[..8])?,
            length: u64::from_be_bytes(b[8..].try_into().map_err(|_| WorkspaceError::Io)?),
        };
        if value.page == PageRef::NULL || value.length == 0 {
            return Err(WorkspaceError::Io);
        }
        Ok(value)
    }
}
/// The declared kind of one page. A cells page carries keyed metadata cells; a
/// pieces page carries the length-indexed extent sequence of one file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageKind {
    Cells,
    Pieces,
}
impl PageKind {
    fn format(self) -> u8 {
        match self {
            Self::Cells => FORMAT_CELLS,
            Self::Pieces => FORMAT_PIECES,
        }
    }
    fn declare(self, bytes: &mut [u8]) {
        bytes[49] = self.format();
        bytes[54] = u8::from(self == Self::Pieces);
    }
    /// The declared format of one page, without decoding its body.
    pub fn of(bytes: &[u8]) -> Result<Self, WorkspaceError> {
        verify(bytes)?;
        if &bytes[..8] != b"LFSWMTA1" || bytes[55] != 0 {
            return Err(WorkspaceError::Io);
        }
        match (bytes[49], bytes[54]) {
            (FORMAT_CELLS, 0) => Ok(Self::Cells),
            (FORMAT_PIECES, 1) => Ok(Self::Pieces),
            // An unknown version, or a body that does not match the version it
            // declares, is refused here rather than reinterpreted.
            _ => Err(WorkspaceError::Io),
        }
    }
}
fn open_page(
    kind: PageKind,
    level: u8,
    used: usize,
    incarnation: [u8; 32],
    r: PageRef,
) -> Result<[u8; PAGE], WorkspaceError> {
    if used == 0 || used > PAGE - HEADER || level > LEVEL_LIMIT {
        return Err(WorkspaceError::Capacity);
    }
    let mut bytes = [0; PAGE];
    bytes[..8].copy_from_slice(b"LFSWMTA1");
    bytes[8..40].copy_from_slice(&incarnation);
    bytes[40..48].copy_from_slice(&r.bytes());
    bytes[48] = level;
    kind.declare(&mut bytes);
    bytes[56..64].copy_from_slice(&(used as u64).to_be_bytes());
    Ok(bytes)
}
/// The shared declared byte count of one page body.
pub fn body_used(bytes: &[u8]) -> Result<usize, WorkspaceError> {
    let used = usize::try_from(u64::from_be_bytes(
        bytes[56..64].try_into().map_err(|_| WorkspaceError::Io)?,
    ))
    .map_err(|_| WorkspaceError::Io)?;
    if used == 0 || used > PAGE - HEADER || bytes[64..96].iter().any(|x| *x != 0) {
        return Err(WorkspaceError::Io);
    }
    Ok(used)
}
pub fn encode_pieces_leaf(
    incarnation: [u8; 32],
    r: PageRef,
    records: &[PieceRecord],
    bytes: &mut [u8],
) -> Result<(), WorkspaceError> {
    if bytes.len() != PAGE {
        return Err(WorkspaceError::Capacity);
    }
    bytes.copy_from_slice(&open_page(
        PageKind::Pieces,
        0,
        records.len() * RECORD,
        incarnation,
        r,
    )?);
    let mut at = HEADER;
    for record in records {
        bytes[at..at + RECORD].copy_from_slice(&record.bytes());
        at += RECORD;
    }
    seal(bytes);
    Ok(())
}
pub fn encode_pieces_branch(
    incarnation: [u8; 32],
    r: PageRef,
    level: u8,
    children: &[ChildRef],
    bytes: &mut [u8],
) -> Result<(), WorkspaceError> {
    if bytes.len() != PAGE || children.len() < 2 || level == 0 {
        return Err(WorkspaceError::Capacity);
    }
    bytes.copy_from_slice(&open_page(
        PageKind::Pieces,
        level,
        children.len() * ChildRef::BYTES,
        incarnation,
        r,
    )?);
    let mut at = HEADER;
    for child in children {
        bytes[at..at + ChildRef::BYTES].copy_from_slice(&child.bytes());
        at += ChildRef::BYTES;
    }
    seal(bytes);
    Ok(())
}
/// The logical byte length of one decoded piece page, from its own header
/// fields: a leaf sums its records, a branch its child lengths.
pub fn piece_body_length(bytes: &[u8]) -> Result<u64, WorkspaceError> {
    let kind = PageKind::of(bytes)?;
    if kind != PageKind::Pieces {
        return Err(WorkspaceError::Io);
    }
    let level = bytes[48];
    let used = body_used(bytes)?;
    let bounded = MAX_FILE;
    let mut total = 0u64;
    let mut at = HEADER;
    if level == 0 {
        if used % RECORD != 0 {
            return Err(WorkspaceError::Io);
        }
        while at < HEADER + used {
            let record = PieceRecord::parse(&bytes[at..at + RECORD], bounded)?;
            total = total.checked_add(record.length).ok_or(WorkspaceError::Io)?;
            at += RECORD;
        }
    } else {
        if level > LEVEL_LIMIT || used % ChildRef::BYTES != 0 {
            return Err(WorkspaceError::Io);
        }
        while at < HEADER + used {
            let child = ChildRef::parse(&bytes[at..at + ChildRef::BYTES])?;
            total = total.checked_add(child.length).ok_or(WorkspaceError::Io)?;
            at += ChildRef::BYTES;
        }
    }
    if total == 0 || total > bounded {
        return Err(WorkspaceError::Io);
    }
    Ok(total)
}
/// The records of one piece leaf, in sequence order.
pub fn decode_pieces_leaf(
    incarnation: [u8; 32],
    r: PageRef,
    b: &[u8],
    bounded: u64,
) -> Result<Vec<PieceRecord>, WorkspaceError> {
    if b.len() != PAGE
        || PageKind::of(b)? != PageKind::Pieces
        || b[48] != 0
        || b[8..40] != incarnation
        || b[40..48] != r.bytes()
    {
        return Err(WorkspaceError::Io);
    }
    let used = body_used(b)?;
    if used % RECORD != 0 || b[HEADER + used..].iter().any(|x| *x != 0) {
        return Err(WorkspaceError::Io);
    }
    let mut records = super::metadata_index::vector(used / RECORD)?;
    let mut at = HEADER;
    while at < HEADER + used {
        records.push(PieceRecord::parse(&b[at..at + RECORD], bounded)?);
        at += RECORD;
    }
    Ok(records)
}
/// The level and children of one piece branch page.
pub fn decode_pieces_branch(
    incarnation: [u8; 32],
    r: PageRef,
    b: &[u8],
    bounded: u64,
) -> Result<(u8, Vec<ChildRef>), WorkspaceError> {
    if b.len() != PAGE
        || PageKind::of(b)? != PageKind::Pieces
        || b[8..40] != incarnation
        || b[40..48] != r.bytes()
    {
        return Err(WorkspaceError::Io);
    }
    let level = b[48];
    let used = body_used(b)?;
    if level == 0 || level > LEVEL_LIMIT || used % ChildRef::BYTES != 0 {
        return Err(WorkspaceError::Io);
    }
    let mut children = super::metadata_index::vector(used / ChildRef::BYTES)?;
    let mut at = HEADER;
    let mut total = 0u64;
    while at < HEADER + used {
        let child = ChildRef::parse(&b[at..at + ChildRef::BYTES])?;
        total = total
            .checked_add(child.length)
            .filter(|total| *total <= bounded)
            .ok_or(WorkspaceError::Io)?;
        children.push(child);
        at += ChildRef::BYTES;
    }
    if children.len() < 2 {
        return Err(WorkspaceError::Io);
    }
    Ok((level, children))
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
    Ok((b[48], cells))
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
    let mut key = super::metadata_index::vector(name.len() + 1)?;
    key.push(b'E');
    key.extend_from_slice(name);
    Ok(key)
}
