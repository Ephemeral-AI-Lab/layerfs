//! Length-indexed piece page codec: extent records and child references.
//!
//! Relocated from `backing/metadata_pages.rs` by slice 3.0 of the phase-3
//! plan. The shared 4 KiB header, `PageRef` and `PageKind` stay there and are
//! imported here, so both specializations read one header definition.
use crate::backing::metadata_pages::{
    body_used, open_page, seal, PageKind, PageRef, HEADER, LEVEL_LIMIT, PAGE,
};
use crate::WorkspaceError;
use layerfs_bridge::contract::MAX_FILE;
/// One piece record of a length-indexed piece page; a leaf holds a whole
/// number of these in its payload area.
pub const RECORD: usize = 32;
/// Largest logical length one extent may describe. An extent's start is
/// derived from its position in the sequence, so it is not stored.
pub const MAX_EXTENT: u64 = (1 << 24) - 1;
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
                .is_none_or(|end| end > MAX_FILE)
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
/// Encodes one branch page. A branch holds at least one child: ordinary packing
/// gives it two or more, and a fold that collapses a node lifts its single
/// remaining child inside a page of the node's own level rather than leaving the
/// tree with children at two depths.
pub fn encode_pieces_branch(
    incarnation: [u8; 32],
    r: PageRef,
    level: u8,
    children: &[ChildRef],
    bytes: &mut [u8],
) -> Result<(), WorkspaceError> {
    if bytes.len() != PAGE || children.is_empty() || level == 0 {
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
    let mut records = crate::backing::metadata_index::vector(used / RECORD)?;
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
    let mut children = crate::backing::metadata_index::vector(used / ChildRef::BYTES)?;
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
    if children.is_empty() {
        return Err(WorkspaceError::Io);
    }
    Ok((level, children))
}
