//! Fixed typed slot access, exact scalar values and contiguous payload layout.

use super::declaration::{checkpoint_count, HEADER_BYTES};
use super::spool::{RowSpool, KIND_DIRECTORY, KIND_INODE, KIND_SERIAL, SPOOL_SLOT_BYTES};
use super::{InodeRowSource, SerialRowSource};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::input::InodeUpdate;
use crate::object::inode_leaf::{InodeKind, InodeValue};
use crate::object::ObjectId;

#[derive(Clone, Copy, Debug)]
pub(super) struct Slot {
    pub(super) key: u64,
    pub(super) offset: u64,
    pub(super) bytes: u64,
    pub(super) records: u32,
    pub(super) kind: u8,
}

impl RowSpool {
    pub(super) fn run(&self, kind: u8) -> (usize, usize) {
        match kind {
            KIND_DIRECTORY => (0, self.directory_rows),
            KIND_INODE => (self.directory_rows, self.inode_rows),
            _ => (self.directory_rows + self.inode_rows, self.new_rows),
        }
    }

    pub(super) fn slot(&self, kind: u8, index: usize) -> ContentResult<Option<Slot>> {
        if index >= self.written[usize::from(kind) - 1] {
            return Ok(None);
        }
        let (first, count) = self.run(kind);
        if index >= count {
            return Ok(None);
        }
        let ordinal = first
            .checked_add(index)
            .ok_or(ContentError::LengthOverflow)?;
        let at = u64::try_from(ordinal)
            .ok()
            .and_then(|i| i.checked_mul(SPOOL_SLOT_BYTES))
            .and_then(|i| i.checked_add(HEADER_BYTES))
            .ok_or(ContentError::LengthOverflow)?;
        let mut record = [0_u8; 32];
        let mut work = self.work.get();
        work.slot_reads = work.slot_reads.saturating_add(1);
        self.work.set(work);
        self.read_at(at, &mut record)?;
        let word = |from: usize| u64::from_be_bytes(record[from..from + 8].try_into().unwrap());
        let slot = Slot {
            key: word(0),
            offset: word(8),
            bytes: word(16),
            records: u32::from_be_bytes(record[24..28].try_into().unwrap()),
            kind: record[28],
        };
        let end = slot
            .offset
            .checked_add(slot.bytes)
            .ok_or(ContentError::LengthOverflow)?;
        if slot.kind != kind
            || record[29..] != [0; 3]
            || !super::serial_in_range(slot.key)
            || slot.offset < self.table_end
            || end > self.bytes
        {
            return Err(ContentError::InvalidRecord("row slot"));
        }
        match kind {
            KIND_DIRECTORY => {
                let count = u64::from(slot.records);
                let index_bytes = checkpoint_count(count) * 8;
                let minimum = index_bytes + count * 10;
                let maximum = index_bytes + count * 264;
                if slot.bytes < minimum || slot.bytes > maximum || end > self.directory_end {
                    return Err(ContentError::InvalidRecord("directory slot span"));
                }
            }
            KIND_INODE => {
                let expected = u64::try_from(index)
                    .ok()
                    .and_then(|i| i.checked_mul(65))
                    .and_then(|i| i.checked_add(self.directory_end))
                    .ok_or(ContentError::LengthOverflow)?;
                if slot.bytes != 65 || slot.records != 0 || slot.offset != expected {
                    return Err(ContentError::InvalidRecord("inode slot span"));
                }
            }
            _ => {
                if slot.bytes != 0 || slot.records != 0 || slot.offset < self.directory_end {
                    return Err(ContentError::InvalidRecord("fresh slot span"));
                }
            }
        }
        Ok(Some(slot))
    }

    pub(super) fn directory_slot(&self, index: usize) -> ContentResult<Option<Slot>> {
        let Some(slot) = self.slot(KIND_DIRECTORY, index)? else {
            return Ok(None);
        };
        let start = if index == 0 {
            self.table_end
        } else {
            let previous = self
                .slot(KIND_DIRECTORY, index - 1)?
                .ok_or(ContentError::InvalidRecord("directory predecessor"))?;
            if previous.key >= slot.key {
                return Err(ContentError::NonCanonicalOrdering);
            }
            previous
                .offset
                .checked_add(previous.bytes)
                .ok_or(ContentError::LengthOverflow)?
        };
        let end = slot
            .offset
            .checked_add(slot.bytes)
            .ok_or(ContentError::LengthOverflow)?;
        let expected_end = match self.slot(KIND_DIRECTORY, index + 1)? {
            Some(next) => {
                if next.key <= slot.key {
                    return Err(ContentError::NonCanonicalOrdering);
                }
                next.offset
            }
            None => self.directory_end,
        };
        if slot.offset != start || end != expected_end {
            return Err(ContentError::InvalidRecord("directory payload partition"));
        }
        Ok(Some(slot))
    }

    pub(super) fn find(&self, kind: u8, key: u64) -> ContentResult<Option<(usize, Slot)>> {
        let mut low = 0_usize;
        let mut high = self.written[usize::from(kind) - 1];
        while low < high {
            let middle = low + (high - low) / 2;
            let slot = self
                .slot(kind, middle)?
                .ok_or(ContentError::InvalidRecord("row slot"))?;
            match slot.key.cmp(&key) {
                std::cmp::Ordering::Less => low = middle + 1,
                std::cmp::Ordering::Greater => high = middle,
                std::cmp::Ordering::Equal => {
                    let slot = if kind == KIND_DIRECTORY {
                        self.directory_slot(middle)?
                            .ok_or(ContentError::InvalidRecord("row slot"))?
                    } else {
                        slot
                    };
                    return Ok(Some((middle, slot)));
                }
            }
        }
        Ok(None)
    }

    pub(super) fn publish_slot(&mut self, slot: Slot) -> ContentResult<()> {
        let seen = usize::from(slot.kind) - 1;
        let (first, _) = self.run(slot.kind);
        let ordinal = first
            .checked_add(self.written[seen])
            .ok_or(ContentError::LengthOverflow)?;
        let at = u64::try_from(ordinal)
            .ok()
            .and_then(|i| i.checked_mul(32))
            .and_then(|i| i.checked_add(HEADER_BYTES))
            .ok_or(ContentError::LengthOverflow)?;
        let mut bytes = [0_u8; 32];
        bytes[..8].copy_from_slice(&slot.key.to_be_bytes());
        bytes[8..16].copy_from_slice(&slot.offset.to_be_bytes());
        bytes[16..24].copy_from_slice(&slot.bytes.to_be_bytes());
        bytes[24..28].copy_from_slice(&slot.records.to_be_bytes());
        bytes[28] = slot.kind;
        self.write_at(at, &bytes)?;
        self.written[seen] += 1;
        self.last[usize::from(slot.kind)] = slot.key;
        Ok(())
    }

    pub(super) fn inode(&self, slot: &Slot) -> ContentResult<InodeUpdate> {
        let mut bytes = [0_u8; 65];
        self.read_at(slot.offset, &mut bytes)?;
        let kind = InodeKind::from_code(bytes[0])?;
        Ok(InodeUpdate {
            serial: slot.key,
            value: InodeValue {
                kind,
                namespace_ref_count: 0,
                content_root: ObjectId::from_bytes(&bytes[1..33])?,
                metadata_root: ObjectId::from_bytes(&bytes[33..65])?,
            },
        })
    }

    pub(super) fn validate_layout(&self) -> ContentResult<()> {
        let mut end = self.table_end;
        let mut bindings = 0_u64;
        let mut name_bytes = 0_u64;
        for index in 0..self.directory_rows {
            let slot = self
                .directory_slot(index)?
                .ok_or(ContentError::InvalidRecord("directory slot"))?;
            if slot.offset != end {
                return Err(ContentError::InvalidRecord("directory partition"));
            }
            end = slot
                .offset
                .checked_add(slot.bytes)
                .ok_or(ContentError::LengthOverflow)?;
            let header = self.slot_header(index, &slot)?;
            bindings = bindings
                .checked_add(u64::from(header.binding_count()))
                .ok_or(ContentError::LengthOverflow)?;
            name_bytes = name_bytes
                .checked_add(header.wire_name_bytes())
                .ok_or(ContentError::LengthOverflow)?;
        }
        if end != self.directory_end
            || bindings != self.bindings
            || name_bytes != self.wire_name_bytes
        {
            return Err(ContentError::InvalidRecord("spool directory totals"));
        }
        let mut previous = 0;
        for index in 0..self.inode_rows {
            let slot = self
                .slot(KIND_INODE, index)?
                .ok_or(ContentError::InvalidRecord("inode slot"))?;
            if slot.offset != end || slot.key <= previous {
                return Err(ContentError::InvalidRecord("inode partition"));
            }
            end = end.checked_add(65).ok_or(ContentError::LengthOverflow)?;
            previous = slot.key;
        }
        previous = 0;
        for index in 0..self.new_rows {
            let slot = self
                .slot(KIND_SERIAL, index)?
                .ok_or(ContentError::InvalidRecord("fresh slot"))?;
            if slot.key <= previous {
                return Err(ContentError::NonCanonicalOrdering);
            }
            previous = slot.key;
        }
        if end != self.bytes || self.file.metadata().map_err(|_| ContentError::Io)?.len() != end {
            return Err(ContentError::InvalidRecord("spool end"));
        }
        Ok(())
    }
}

pub(super) struct SpoolInodes<'a> {
    spool: &'a RowSpool,
    at: usize,
}
impl<'a> SpoolInodes<'a> {
    pub(super) fn new(spool: &'a RowSpool) -> Self {
        Self { spool, at: 0 }
    }
}
impl InodeRowSource for SpoolInodes<'_> {
    fn next_row(&mut self) -> ContentResult<Option<InodeUpdate>> {
        let Some(slot) = self.spool.slot(KIND_INODE, self.at)? else {
            return Ok(None);
        };
        let row = self.spool.inode(&slot)?;
        self.at += 1;
        Ok(Some(row))
    }
}

pub(super) struct SpoolSerials<'a> {
    spool: &'a RowSpool,
    at: usize,
}
impl<'a> SpoolSerials<'a> {
    pub(super) fn new(spool: &'a RowSpool) -> Self {
        Self { spool, at: 0 }
    }
}
impl SerialRowSource for SpoolSerials<'_> {
    fn next_row(&mut self) -> ContentResult<Option<u64>> {
        let Some(slot) = self.spool.slot(KIND_SERIAL, self.at)? else {
            return Ok(None);
        };
        self.at += 1;
        Ok(Some(slot.key))
    }
}
