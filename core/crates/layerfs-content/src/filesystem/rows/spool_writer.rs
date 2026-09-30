//! One streaming directory producer and fixed-width typed payload writes.

use super::declaration::checkpoint_count;
use super::resident_cursor::binding_bytes;
use super::spool::{RowSpool, KIND_DIRECTORY, KIND_INODE, KIND_SERIAL};
use super::spool_slots::Slot;
use crate::error::{ContentError, ContentResult};
use crate::filesystem::input::{DirectoryUpdate, InodeUpdate};
use crate::filesystem::path::PathName;

pub(super) struct DirectoryWrite {
    parent: u64,
    count: u32,
    written: u32,
    start: u64,
    data_start: u64,
    wire_bytes: u64,
    previous: Option<PathName>,
}

impl RowSpool {
    /// Starts one directory, admitting its index and minimum records first.
    pub fn begin_directory(&mut self, parent: u64, bindings: u32) -> ContentResult<()> {
        self.ensure_writable()?;
        if self.active.is_some() {
            return Err(ContentError::IncompleteOperation);
        }
        if self.written[1] != 0 || self.written[2] != 0 {
            return Err(ContentError::InvalidRecord("directory phase finished"));
        }
        self.check_next(KIND_DIRECTORY, parent)?;
        let count = u64::from(bindings);
        if let Some(declared) = self.declaration {
            if self
                .bindings
                .checked_add(count)
                .ok_or(ContentError::LengthOverflow)?
                > declared.bindings
                || self
                    .wire_name_bytes
                    .checked_add(count * 11)
                    .ok_or(ContentError::LengthOverflow)?
                    > declared.wire_name_bytes
            {
                return Err(ContentError::InvalidRecord("declared binding count"));
            }
        }
        let index_bytes = checkpoint_count(count)
            .checked_mul(8)
            .ok_or(ContentError::LengthOverflow)?;
        let data_start = self
            .bytes
            .checked_add(index_bytes)
            .ok_or(ContentError::LengthOverflow)?;
        let minimum_end = data_start
            .checked_add(count * 10)
            .ok_or(ContentError::LengthOverflow)?;
        self.check_capacity(minimum_end)?;
        let start = self.bytes;
        if index_bytes != 0 {
            if let Err(error) = self.file.set_len(data_start).map_err(|_| ContentError::Io) {
                self.failure = Some(error.clone());
                return Err(error);
            }
        }
        self.bytes = data_start;
        self.active = Some(DirectoryWrite {
            parent,
            count: bindings,
            written: 0,
            start,
            data_start,
            wire_bytes: 0,
            previous: None,
        });
        Ok(())
    }

    /// Writes one checked full name and final child under the active row.
    pub fn push_binding(&mut self, name: &PathName, child: Option<u64>) -> ContentResult<()> {
        self.ensure_writable()?;
        let result = self.write_binding(name, child);
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }

    fn write_binding(&mut self, name: &PathName, child: Option<u64>) -> ContentResult<()> {
        let row = self
            .active
            .as_ref()
            .ok_or(ContentError::InvalidRecord("directory not started"))?;
        if row.written >= row.count {
            return Err(ContentError::InvalidRecord("directory binding count"));
        }
        if row
            .previous
            .as_ref()
            .is_some_and(|previous| previous >= name)
        {
            return Err(ContentError::NonCanonicalOrdering);
        }
        if child.is_some_and(|serial| !super::serial_in_range(serial)) {
            return Err(ContentError::InvalidRecord("inode serial"));
        }
        let name_bytes = name.as_bytes();
        let length = u8::try_from(name_bytes.len()).map_err(|_| ContentError::LengthOverflow)?;
        let wire_bytes = row
            .wire_bytes
            .checked_add(10 + name_bytes.len() as u64)
            .ok_or(ContentError::LengthOverflow)?;
        if self.declaration.is_some_and(|declared| {
            self.wire_name_bytes
                .checked_add(wire_bytes)
                .is_none_or(|bytes| bytes > declared.wire_name_bytes)
        }) {
            return Err(ContentError::InvalidRecord("declared binding bytes"));
        }
        let end = self
            .bytes
            .checked_add(9 + name_bytes.len() as u64)
            .ok_or(ContentError::LengthOverflow)?;
        let remaining = u64::from(row.count - row.written - 1);
        self.check_capacity(
            end.checked_add(remaining * 10)
                .ok_or(ContentError::LengthOverflow)?,
        )?;
        let checkpoint = if row.written != 0 && row.written % 16 == 0 {
            Some((
                row.start + 8 * u64::from(row.written / 16 - 1),
                self.bytes - row.data_start,
            ))
        } else {
            None
        };
        if let Some((at, relative)) = checkpoint {
            self.write_at(at, &relative.to_be_bytes())?;
        }
        let mut record = [0_u8; 264];
        record[0] = length;
        record[1..1 + name_bytes.len()].copy_from_slice(name_bytes);
        record[1 + name_bytes.len()..9 + name_bytes.len()]
            .copy_from_slice(&child.unwrap_or(0).to_be_bytes());
        self.write_at(self.bytes, &record[..9 + name_bytes.len()])?;
        self.bytes = end;
        let row = self
            .active
            .as_mut()
            .ok_or(ContentError::IncompleteOperation)?;
        row.written += 1;
        row.wire_bytes = wire_bytes;
        row.previous = Some(name.clone());
        Ok(())
    }

    /// Publishes only an exactly completed directory; a partial row has no slot.
    pub fn end_directory(
        &mut self,
        parent: u64,
        bindings: u32,
        wire_name_bytes: u64,
    ) -> ContentResult<()> {
        self.ensure_writable()?;
        let result = self.finish_directory(parent, bindings, wire_name_bytes);
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }

    fn finish_directory(
        &mut self,
        parent: u64,
        bindings: u32,
        wire_name_bytes: u64,
    ) -> ContentResult<()> {
        let row = self
            .active
            .as_ref()
            .ok_or(ContentError::InvalidRecord("directory not started"))?;
        if row.parent != parent
            || row.count != bindings
            || row.written != bindings
            || row.wire_bytes != wire_name_bytes
        {
            return Err(ContentError::InvalidRecord("directory completion"));
        }
        let all_bindings = self
            .bindings
            .checked_add(u64::from(bindings))
            .ok_or(ContentError::LengthOverflow)?;
        let all_bytes = self
            .wire_name_bytes
            .checked_add(wire_name_bytes)
            .ok_or(ContentError::LengthOverflow)?;
        if self
            .declaration
            .is_some_and(|d| all_bindings > d.bindings || all_bytes > d.wire_name_bytes)
        {
            return Err(ContentError::InvalidRecord("declared binding totals"));
        }
        let slot = Slot {
            key: parent,
            offset: row.start,
            bytes: self.bytes - row.start,
            records: bindings,
            kind: KIND_DIRECTORY,
        };
        self.publish_slot(slot)?;
        self.active = None;
        self.bindings = all_bindings;
        self.wire_name_bytes = all_bytes;
        self.directory_end = self.bytes;
        Ok(())
    }

    /// Explicit resident compatibility write through the same scalar producer.
    pub fn push_directory(&mut self, row: &DirectoryUpdate) -> ContentResult<()> {
        row.check()?;
        if row
            .changes
            .iter()
            .any(|(_, child)| child.is_some_and(|serial| !super::serial_in_range(serial)))
        {
            return Err(ContentError::InvalidRecord("inode serial"));
        }
        let count = u32::try_from(row.changes.len()).map_err(|_| ContentError::LengthOverflow)?;
        let wire_bytes = binding_bytes(&row.changes)?;
        let payload = checkpoint_count(u64::from(count))
            .checked_mul(8)
            .and_then(|bytes| {
                wire_bytes
                    .checked_sub(u64::from(count))
                    .and_then(|names| bytes.checked_add(names))
            })
            .ok_or(ContentError::LengthOverflow)?;
        self.check_capacity(
            self.bytes
                .checked_add(payload)
                .ok_or(ContentError::LengthOverflow)?,
        )?;
        self.begin_directory(row.parent, count)?;
        for (name, child) in &row.changes {
            self.push_binding(name, *child)?;
        }
        self.end_directory(row.parent, count, wire_bytes)
    }

    /// Appends one65-byte typed value; its serial lives only in the slot.
    pub fn push_inode(&mut self, row: &InodeUpdate) -> ContentResult<()> {
        self.ensure_scalar_phase()?;
        self.check_next(KIND_INODE, row.serial)?;
        let mut bytes = [0_u8; 65];
        bytes[0] = row.value.kind.code();
        bytes[1..33].copy_from_slice(row.value.content_root.as_bytes());
        bytes[33..65].copy_from_slice(row.value.metadata_root.as_bytes());
        self.append_scalar(KIND_INODE, row.serial, &bytes)
    }

    /// Publishes one fresh serial from its slot key, without a duplicate payload.
    pub fn push_serial(&mut self, serial: u64) -> ContentResult<()> {
        self.ensure_scalar_phase()?;
        self.check_next(KIND_SERIAL, serial)?;
        self.append_scalar(KIND_SERIAL, serial, &[])
    }

    fn ensure_scalar_phase(&self) -> ContentResult<()> {
        self.ensure_writable()?;
        if self.active.is_some() || self.written[0] != self.directory_rows {
            return Err(ContentError::IncompleteOperation);
        }
        Ok(())
    }

    fn append_scalar(&mut self, kind: u8, key: u64, bytes: &[u8]) -> ContentResult<()> {
        let end = self
            .bytes
            .checked_add(bytes.len() as u64)
            .ok_or(ContentError::LengthOverflow)?;
        self.check_capacity(end)?;
        let slot = Slot {
            key,
            offset: self.bytes,
            bytes: bytes.len() as u64,
            records: 0,
            kind,
        };
        let result = self.write_at(self.bytes, bytes).and_then(|()| {
            self.bytes = end;
            self.publish_slot(slot)
        });
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }

    fn check_next(&self, kind: u8, key: u64) -> ContentResult<()> {
        let index = usize::from(kind) - 1;
        let (_, count) = self.run(kind);
        if self.written[index] >= count {
            return Err(ContentError::InvalidRecord(match kind {
                KIND_DIRECTORY => "directory row count",
                KIND_INODE => "inode row count",
                _ => "new inode row count",
            }));
        }
        if !super::serial_in_range(key) {
            return Err(ContentError::InvalidRecord("inode serial"));
        }
        if self.written[index] != 0 && self.last[usize::from(kind)] >= key {
            return Err(ContentError::NonCanonicalOrdering);
        }
        Ok(())
    }

    fn check_capacity(&self, end: u64) -> ContentResult<()> {
        if end > self.capacity {
            return Err(ContentError::ResourceUnavailable {
                what: "prepared row spool",
            });
        }
        Ok(())
    }
}
