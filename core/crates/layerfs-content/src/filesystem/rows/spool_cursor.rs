//! Exact scalar traversal of one issued directory span.

use super::declaration::checkpoint_count;
use super::spool::RowSpool;
use super::spool_slots::Slot;
use super::{BindingRowSource, DirectoryCompletion, DirectoryHeader};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::path::PathName;

pub(super) struct SpoolBindings<'a> {
    spool: &'a RowSpool,
    header: DirectoryHeader,
    slot: Slot,
    data_start: u64,
    end: u64,
    at: u64,
    seen: u32,
    wire_bytes: u64,
    previous: Option<PathName>,
    eof: bool,
    failure: Option<ContentError>,
}

impl<'a> SpoolBindings<'a> {
    pub(super) fn new(
        spool: &'a RowSpool,
        header: DirectoryHeader,
        slot: Slot,
    ) -> ContentResult<Self> {
        let data_start = slot
            .offset
            .checked_add(checkpoint_count(u64::from(slot.records)) * 8)
            .ok_or(ContentError::LengthOverflow)?;
        let end = slot
            .offset
            .checked_add(slot.bytes)
            .ok_or(ContentError::LengthOverflow)?;
        if data_start > end {
            return Err(ContentError::InvalidRecord("directory data span"));
        }
        Ok(Self {
            spool,
            header,
            slot,
            data_start,
            end,
            at: data_start,
            seen: 0,
            wire_bytes: 0,
            previous: None,
            eof: false,
            failure: None,
        })
    }

    fn next(&mut self) -> ContentResult<Option<(PathName, Option<u64>)>> {
        if self.eof {
            return Ok(None);
        }
        if self.seen == self.header.binding_count() {
            if self.at != self.end || self.wire_bytes != self.header.wire_name_bytes() {
                return Err(ContentError::InvalidRecord("directory completion"));
            }
            self.eof = true;
            return Ok(None);
        }
        if self.seen != 0 && self.seen % 16 == 0 {
            let checkpoint = self
                .spool
                .checkpoint(&self.slot, u64::from(self.seen / 16))?;
            if checkpoint != self.at - self.data_start {
                return Err(ContentError::InvalidRecord("directory checkpoint ordinal"));
            }
        }
        let (name, child, next) = self.spool.decode_binding(self.at, self.end)?;
        if self
            .previous
            .as_ref()
            .is_some_and(|previous| previous >= &name)
        {
            return Err(ContentError::NonCanonicalOrdering);
        }
        let bytes = self
            .wire_bytes
            .checked_add(10 + name.as_bytes().len() as u64)
            .ok_or(ContentError::LengthOverflow)?;
        if bytes > self.header.wire_name_bytes() {
            return Err(ContentError::InvalidRecord("directory byte count"));
        }
        self.at = next;
        self.seen += 1;
        self.wire_bytes = bytes;
        self.previous = Some(name.clone());
        Ok(Some((name, child)))
    }
}

impl BindingRowSource for SpoolBindings<'_> {
    fn next_binding(&mut self) -> ContentResult<Option<(PathName, Option<u64>)>> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = self.next();
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }
    fn finish(&mut self) -> ContentResult<DirectoryCompletion> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if !self.eof {
            self.failure = Some(ContentError::IncompleteOperation);
            return Err(ContentError::IncompleteOperation);
        }
        Ok(DirectoryCompletion::finished(self.header))
    }
}

impl RowSpool {
    pub(super) fn slot_header(&self, index: usize, slot: &Slot) -> ContentResult<DirectoryHeader> {
        let count = u64::from(slot.records);
        let record_bytes = slot
            .bytes
            .checked_sub(checkpoint_count(count) * 8)
            .ok_or(ContentError::InvalidRecord("directory index span"))?;
        let wire_bytes = record_bytes
            .checked_add(count)
            .ok_or(ContentError::LengthOverflow)?;
        self.authority.header(
            slot.key,
            u64::try_from(index).map_err(|_| ContentError::LengthOverflow)?,
            slot.records,
            wire_bytes,
        )
    }

    pub(super) fn checkpoint(&self, slot: &Slot, block: u64) -> ContentResult<u64> {
        if block == 0 {
            return Ok(0);
        }
        let checkpoints = checkpoint_count(u64::from(slot.records));
        if block > checkpoints {
            return Err(ContentError::InvalidRecord("directory checkpoint"));
        }
        let at = slot
            .offset
            .checked_add((block - 1) * 8)
            .ok_or(ContentError::LengthOverflow)?;
        let mut bytes = [0; 8];
        let mut work = self.work.get();
        work.checkpoint_reads = work.checkpoint_reads.saturating_add(1);
        self.work.set(work);
        self.read_at(at, &mut bytes)?;
        let offset = u64::from_be_bytes(bytes);
        let data_bytes = slot
            .bytes
            .checked_sub(checkpoints * 8)
            .ok_or(ContentError::InvalidRecord("directory index span"))?;
        if offset == 0 || offset >= data_bytes {
            return Err(ContentError::InvalidRecord("directory checkpoint span"));
        }
        Ok(offset)
    }

    pub(super) fn decode_binding(
        &self,
        at: u64,
        end: u64,
    ) -> ContentResult<(PathName, Option<u64>, u64)> {
        if at >= end {
            return Err(ContentError::UnexpectedEof);
        }
        let mut length = [0];
        self.read_at(at, &mut length)?;
        let length = usize::from(length[0]);
        if length == 0 {
            return Err(ContentError::InvalidRecord("directory row name"));
        }
        let next = at
            .checked_add(9 + length as u64)
            .ok_or(ContentError::LengthOverflow)?;
        if next > end {
            return Err(ContentError::UnexpectedEof);
        }
        let mut bytes = [0_u8; 263];
        self.read_at(at + 1, &mut bytes[..length + 8])?;
        let name = PathName::from_bytes(&bytes[..length])?;
        let serial = u64::from_be_bytes(bytes[length..length + 8].try_into().unwrap());
        if serial != 0 && !super::serial_in_range(serial) {
            return Err(ContentError::InvalidRecord("inode serial"));
        }
        let mut work = self.work.get();
        work.bindings_decoded = work.bindings_decoded.saturating_add(1);
        self.work.set(work);
        Ok((name, (serial != 0).then_some(serial), next))
    }
}
