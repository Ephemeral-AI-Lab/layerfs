//! Inode value blocks: the values of consecutive ordinals in one row keyed
//! by its first ordinal; the ordinals inside are implicit.
//!
//! Body, format 1 (little-endian unless a varint):
//!
//! | Bytes | Field |
//! | --- | --- |
//! | 1 | format, `1` |
//! | 1 | entry count `n`, 1..=255: ordinals `first ..= first + n - 1` |
//! | 1 | distinct modes `m`, 0..=255 |
//! | 4 × m | the modes, each the value's four mode bytes as stored |
//! | … | `n` entries |
//! | 4 | CRC32C of every byte before it |
//!
//! An entry starts with a tag that is the value's byte 0 for the two
//! canonical forms (inode_leaf.rs): the root form (bit 7 clear) stores a
//! LEB128 reference count, the 32-byte content root and the 32-byte
//! attribute root; the inline form (bit 7 set, zero padding) stores the
//! count, the content root, a mode-table index, the mtime seconds as a
//! zig-zag LEB128 difference from the previous inline entry's (0 before the
//! first) and the four nanosecond bytes. Any other 73 bytes (and a byte 0
//! of `0xff`) take tag `0xff` and are stored whole. Decoding rebuilds the
//! exact 73 bytes, so no object identity depends on this layout.
use super::frame::{self, Reader, BODY_LIMIT, CRC_BYTES};
use crate::backend::records::BackendError;
use layerfs_content::inode_leaf::INODE_VALUE_BYTES;
use layerfs_storage::location::InodeValueRow;

const FORMAT: u8 = 1;
const RAW: u8 = 0xff;
const HEADER: usize = 3;
const MAXIMUM_ENTRIES: usize = 255;
const MAXIMUM_MODES: usize = 255;

/// One encoded block: ordinals `first ..= last`.
pub(crate) struct Block {
    pub(crate) first: u32,
    pub(crate) last: u32,
    pub(crate) body: Vec<u8>,
}

/// The block being filled.
struct Open {
    first: u32,
    count: usize,
    modes: Vec<[u8; 4]>,
    previous_mtime: i64,
    entries: Vec<u8>,
}

impl Open {
    fn new(first: u32) -> Self {
        Self {
            first,
            count: 0,
            modes: Vec::new(),
            previous_mtime: 0,
            entries: Vec::with_capacity(BODY_LIMIT),
        }
    }
    fn size(&self) -> usize {
        HEADER + 4 * self.modes.len() + self.entries.len() + CRC_BYTES
    }
    /// Appends `value` when it fits; false leaves the block unchanged.
    fn push(&mut self, value: &[u8; INODE_VALUE_BYTES]) -> bool {
        if self.count == MAXIMUM_ENTRIES {
            return false;
        }
        let inline = value[0] & 0x80 != 0 && value[0] != RAW && value[57..].iter().all(|b| *b == 0);
        let root_form = value[0] & 0x80 == 0;
        let mut entry = Vec::with_capacity(INODE_VALUE_BYTES + 1);
        let mut new_mode = false;
        let mut mtime = self.previous_mtime;
        if inline || root_form {
            entry.push(value[0]);
            let count = u64::from_be_bytes(value[1..9].try_into().unwrap_or_default());
            frame::put_varint(&mut entry, count);
            entry.extend_from_slice(&value[9..41]);
            if root_form {
                entry.extend_from_slice(&value[41..73]);
            } else {
                let mode: [u8; 4] = value[41..45].try_into().unwrap_or_default();
                let index = match self.modes.iter().position(|m| *m == mode) {
                    Some(index) => index,
                    None if self.modes.len() < MAXIMUM_MODES => {
                        new_mode = true;
                        self.modes.len()
                    }
                    None => return false,
                };
                entry.push(index as u8);
                mtime = i64::from_be_bytes(value[45..53].try_into().unwrap_or_default());
                frame::put_varint(
                    &mut entry,
                    frame::zigzag(mtime.wrapping_sub(self.previous_mtime)),
                );
                entry.extend_from_slice(&value[53..57]);
            }
        } else {
            entry.push(RAW);
            entry.extend_from_slice(value);
        }
        let grown = self.size() + entry.len() + if new_mode { 4 } else { 0 };
        if self.count > 0 && grown > BODY_LIMIT {
            return false;
        }
        if new_mode {
            self.modes
                .push(value[41..45].try_into().unwrap_or_default());
        }
        self.previous_mtime = mtime;
        self.entries.extend_from_slice(&entry);
        self.count += 1;
        true
    }
    fn finish(self) -> Block {
        let mut body = Vec::with_capacity(self.size());
        body.push(FORMAT);
        body.push(self.count as u8);
        body.push(self.modes.len() as u8);
        for mode in &self.modes {
            body.extend_from_slice(mode);
        }
        body.extend_from_slice(&self.entries);
        frame::seal(&mut body);
        Block {
            first: self.first,
            last: self.first + (self.count as u32 - 1),
            body,
        }
    }
}

/// Packs distinct ascending ordinals' values into blocks, in order: a new
/// block starts at a gap in the ordinals or when the current one is full.
pub(crate) fn encode(rows: &[InodeValueRow]) -> Result<Vec<Block>, BackendError> {
    let mut blocks = Vec::new();
    let mut open: Option<Open> = None;
    let mut previous: Option<u32> = None;
    for row in rows {
        if row.ordinal == 0 || previous.is_some_and(|p| p >= row.ordinal) {
            return Err(BackendError::Integrity);
        }
        let follows = previous.is_some_and(|p| p + 1 == row.ordinal);
        previous = Some(row.ordinal);
        if let Some(block) = open.as_mut() {
            if follows && block.push(&row.value) {
                continue;
            }
        }
        if let Some(block) = open.take() {
            blocks.push(block.finish());
        }
        let mut block = Open::new(row.ordinal);
        if !block.push(&row.value) {
            return Err(BackendError::Integrity);
        }
        open = Some(block);
    }
    if let Some(block) = open {
        blocks.push(block.finish());
    }
    Ok(blocks)
}

/// The values of ordinals `first ..= last`, from one block's body.
pub(crate) fn decode(
    first: u32,
    last: u32,
    body: &[u8],
) -> Result<Vec<[u8; INODE_VALUE_BYTES]>, BackendError> {
    if body.len() > BODY_LIMIT || last < first {
        return Err(BackendError::Integrity);
    }
    let mut reader = Reader::new(frame::open(body)?);
    let count = usize::from(reader.byte().and_then(|format| match format {
        FORMAT => reader.byte(),
        _ => Err(BackendError::Integrity),
    })?);
    if count == 0 || (last - first) as usize + 1 != count {
        return Err(BackendError::Integrity);
    }
    let modes: Vec<&[u8]> = (0..reader.byte()?)
        .map(|_| reader.take(4))
        .collect::<Result<_, _>>()?;
    let mut previous_mtime = 0i64;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        let mut value = [0u8; INODE_VALUE_BYTES];
        let tag = reader.byte()?;
        if tag == RAW {
            value.copy_from_slice(reader.take(INODE_VALUE_BYTES)?);
            values.push(value);
            continue;
        }
        value[0] = tag;
        value[1..9].copy_from_slice(&reader.varint()?.to_be_bytes());
        value[9..41].copy_from_slice(reader.take(32)?);
        if tag & 0x80 == 0 {
            value[41..73].copy_from_slice(reader.take(32)?);
        } else {
            let mode = modes
                .get(usize::from(reader.byte()?))
                .ok_or(BackendError::Integrity)?;
            value[41..45].copy_from_slice(mode);
            previous_mtime = previous_mtime.wrapping_add(frame::unzigzag(reader.varint()?));
            value[45..53].copy_from_slice(&previous_mtime.to_be_bytes());
            value[53..57].copy_from_slice(reader.take(4)?);
        }
        values.push(value);
    }
    reader.finished()?;
    Ok(values)
}

/// Picks the asked values out of blocks in ascending first-ordinal order,
/// in ordinal order; a block that overlaps the one before it is refused.
pub(crate) fn pick(
    ordinals: &[u32],
    blocks: impl IntoIterator<Item = Result<(u32, u32, Vec<u8>), BackendError>>,
    out: &mut Vec<InodeValueRow>,
) -> Result<(), BackendError> {
    let mut asked = ordinals.iter().copied().peekable();
    let mut end = 0u32;
    for block in blocks {
        let (first, last, body) = block?;
        if first <= end {
            return Err(BackendError::Integrity);
        }
        end = last;
        let values = decode(first, last, &body)?;
        while let Some(ordinal) = asked.next_if(|o| *o <= last) {
            if ordinal >= first {
                out.push(InodeValueRow {
                    ordinal,
                    value: values[(ordinal - first) as usize],
                });
            }
        }
    }
    Ok(())
}
