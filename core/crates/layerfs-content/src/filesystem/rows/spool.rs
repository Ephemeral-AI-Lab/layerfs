//! A charged, file-backed row spool: fixed-stride slots, payloads behind them.
//!
//! The spool is where a prepared namespace lives between the transport that
//! delivered it and the operation that applies it. It stores the rows themselves
//! and nothing else: one fixed 32-byte slot per row - the row's key, the offset of
//! its payload, its length and how many bindings it carries - followed by the
//! payload region. Because the slot table is fixed-stride and grouped by kind in
//! key order, a lookup is a binary search over slots read straight from the file,
//! and a pass is one sequential walk of a kind's slots. The resident cost of either
//! is one slot and one row, whatever the row count.
//!
//! **The slot table is sized from the declared totals, before the first row is
//! written.** A spool is created for an update that already stated how many
//! directory rows, typed values and fresh serials it holds, so the payload region
//! begins at a known offset and the spool never has to grow a resident index or
//! rewrite a slot to make room. [`RowSpool::seal`] refuses an update that wrote
//! fewer rows than it declared, and a row beyond the declared totals is refused
//! where it is written.
//!
//! **Every byte is charged, and the file is removed rather than leaked.** A spool
//! owns a declared capacity; the slot table is reserved when the spool is created
//! and every payload against it as it is written, so a row that would exceed the
//! capacity fails before it grows. Dropping the spool removes its file, and
//! [`RowSpool::cleanup`] is the checked form of the same act.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use super::{DirectoryRowSource, InodeRowSource, RowSource, SerialRowSource};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::input::{DirectoryUpdate, InodeUpdate};
use crate::filesystem::path::PathName;
use crate::object::inode_leaf::{InodeKind, InodeValue};
use crate::object::ObjectId;

/// Bytes one row occupies in the slot table.
pub const SPOOL_SLOT_BYTES: u64 = 32;
/// Bytes the spool header occupies before the slot table.
const SPOOL_HEADER_BYTES: u64 = 48;
const KIND_DIRECTORY: u8 = 1;
const KIND_INODE: u8 = 2;
const KIND_SERIAL: u8 = 3;

/// One file-backed, charged spool of prepared rows.
pub struct RowSpool {
    path: PathBuf,
    file: File,
    directory_rows: usize,
    inode_rows: usize,
    new_rows: usize,
    slots: usize,
    bytes: u64,
    capacity: u64,
    sealed: bool,
    /// Highest key written in each kind's run. A run is searched by slot, so a
    /// key that does not rise would make a later binary search answer the wrong
    /// row; the invariant is enforced where the row is written.
    last: [u64; 4],
}

impl RowSpool {
    /// Creates an empty spool for an update with these declared row totals.
    ///
    /// The slot table is reserved immediately, so the payload region's first
    /// offset is known before the first row is written.
    pub fn create(
        path: PathBuf,
        directory_rows: usize,
        inode_rows: usize,
        new_rows: usize,
        capacity: u64,
    ) -> ContentResult<Self> {
        let table = u64::try_from(directory_rows + inode_rows + new_rows)
            .ok()
            .and_then(|rows| rows.checked_mul(SPOOL_SLOT_BYTES))
            .and_then(|bytes| bytes.checked_add(SPOOL_HEADER_BYTES))
            .ok_or(ContentError::LengthOverflow)?;
        if table > capacity {
            return Err(ContentError::ResourceUnavailable {
                what: "prepared row spool",
            });
        }
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| ContentError::Io)?;
        let mut header = [0_u8; SPOOL_HEADER_BYTES as usize];
        header[..8].copy_from_slice(&(directory_rows as u64).to_be_bytes());
        header[8..16].copy_from_slice(&(inode_rows as u64).to_be_bytes());
        header[16..24].copy_from_slice(&(new_rows as u64).to_be_bytes());
        file.write_all(&header).map_err(|_| ContentError::Io)?;
        file.set_len(table).map_err(|_| ContentError::Io)?;
        Ok(Self {
            path,
            file,
            directory_rows,
            inode_rows,
            new_rows,
            slots: 0,
            bytes: table,
            capacity,
            sealed: false,
            last: [0; 4],
        })
    }

    /// The file this spool owns.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Bytes this spool currently holds, including its reserved slot table.
    pub fn held_bytes(&self) -> u64 {
        self.bytes
    }

    /// Directory rows written so far.
    pub fn written_directories(&self) -> usize {
        self.slots.min(self.directory_rows)
    }

    /// Typed inode values written so far.
    pub fn written_inodes(&self) -> usize {
        self.slots
            .saturating_sub(self.directory_rows)
            .min(self.inode_rows)
    }

    /// Fresh serials written so far.
    pub fn written_new(&self) -> usize {
        self.slots
            .saturating_sub(self.directory_rows + self.inode_rows)
            .min(self.new_rows)
    }

    /// Writes one directory row's final bindings.
    pub fn push_directory(&mut self, row: &DirectoryUpdate) -> ContentResult<()> {
        if self.slots >= self.directory_rows {
            return Err(ContentError::InvalidRecord("directory row count"));
        }
        let mut payload = Vec::new();
        payload.extend_from_slice(&row.parent.to_be_bytes());
        let count = u32::try_from(row.changes.len()).map_err(|_| ContentError::LengthOverflow)?;
        payload.extend_from_slice(&count.to_be_bytes());
        for (name, serial) in &row.changes {
            let bytes = name.as_bytes();
            let length = u8::try_from(bytes.len()).map_err(|_| ContentError::LengthOverflow)?;
            payload.push(length);
            payload.extend_from_slice(bytes);
            payload.extend_from_slice(&serial.unwrap_or(0).to_be_bytes());
        }
        self.write_slot(KIND_DIRECTORY, row.parent, row.changes.len(), &payload)
    }

    /// Writes one typed final inode value.
    pub fn push_inode(&mut self, row: &InodeUpdate) -> ContentResult<()> {
        if self.written_inodes() >= self.inode_rows || self.slots < self.directory_rows {
            return Err(ContentError::InvalidRecord("inode row count"));
        }
        super::serial_in_range(row.serial)
            .then_some(())
            .ok_or(ContentError::InvalidRecord("inode serial"))?;
        let mut payload = Vec::with_capacity(73);
        payload.extend_from_slice(&row.serial.to_be_bytes());
        payload.push(row.value.kind.code());
        payload.extend_from_slice(row.value.content_root.as_bytes());
        payload.extend_from_slice(row.value.metadata_root.as_bytes());
        self.write_slot(KIND_INODE, row.serial, 0, &payload)
    }

    /// Writes one serial the caller's allocator just created.
    pub fn push_serial(&mut self, serial: u64) -> ContentResult<()> {
        if self.written_new() >= self.new_rows || self.slots < self.directory_rows + self.inode_rows
        {
            return Err(ContentError::InvalidRecord("new inode row count"));
        }
        super::serial_in_range(serial)
            .then_some(())
            .ok_or(ContentError::InvalidRecord("new inode serial"))?;
        self.write_slot(KIND_SERIAL, serial, 0, &serial.to_be_bytes())
    }

    /// Reserves a payload and records its slot at the end of its kind's run.
    fn write_slot(
        &mut self,
        kind: u8,
        key: u64,
        records: usize,
        payload: &[u8],
    ) -> ContentResult<()> {
        let seen = usize::from(kind);
        if self.slots > 0 && self.last[seen] >= key {
            return Err(ContentError::NonCanonicalOrdering);
        }
        let length = payload.len() as u64;
        let end = self
            .bytes
            .checked_add(length)
            .ok_or(ContentError::LengthOverflow)?;
        if end > self.capacity {
            return Err(ContentError::ResourceUnavailable {
                what: "prepared row spool",
            });
        }
        let offset = self.bytes;
        let slot = SPOOL_HEADER_BYTES + self.slots as u64 * SPOOL_SLOT_BYTES;
        let mut record = [0_u8; SPOOL_SLOT_BYTES as usize];
        record[..8].copy_from_slice(&key.to_be_bytes());
        record[8..16].copy_from_slice(&offset.to_be_bytes());
        record[16..24].copy_from_slice(&length.to_be_bytes());
        record[24..28].copy_from_slice(&(records as u32).to_be_bytes());
        record[28] = kind;
        self.file
            .seek(SeekFrom::Start(slot))
            .and_then(|_| self.file.write_all(&record))
            .and_then(|_| self.file.seek(SeekFrom::Start(offset)))
            .and_then(|_| self.file.write_all(payload))
            .map_err(|_| ContentError::Io)?;
        self.slots += 1;
        self.bytes = end;
        self.last[seen] = key;
        Ok(())
    }

    /// Refuses a spool that does not hold every row its update declared.
    pub fn seal(&mut self) -> ContentResult<()> {
        if self.slots
            != self
                .directory_rows
                .saturating_add(self.inode_rows)
                .saturating_add(self.new_rows)
        {
            return Err(ContentError::InvalidRecord("prepared row count"));
        }
        self.file.flush().map_err(|_| ContentError::Io)?;
        self.sealed = true;
        Ok(())
    }

    /// True once every declared row has been written.
    pub fn is_sealed(&self) -> bool {
        self.sealed
    }

    /// Removes the spool's file. A failure to remove it is reported.
    pub fn cleanup(&mut self) -> ContentResult<()> {
        if self.path.as_os_str().is_empty() {
            return Ok(());
        }
        std::fs::remove_file(&self.path).map_err(|_| ContentError::Io)?;
        self.path.clear();
        Ok(())
    }

    /// Reads one slot of a kind's run, or `None` past its end.
    fn slot(&self, kind: u8, index: usize) -> ContentResult<Option<Slot>> {
        let (start, count) = self.run(kind);
        if index >= count {
            return Ok(None);
        }
        let at = SPOOL_HEADER_BYTES + (start + index) as u64 * SPOOL_SLOT_BYTES;
        let mut record = [0_u8; SPOOL_SLOT_BYTES as usize];
        let mut file = &self.file;
        file.seek(SeekFrom::Start(at))
            .and_then(|_| file.read_exact(&mut record))
            .map_err(|_| ContentError::Io)?;
        let word = |from: usize| u64::from_be_bytes(record[from..from + 8].try_into().unwrap());
        let slot = Slot {
            key: word(0),
            offset: word(8),
            bytes: word(16),
            records: u32::from_be_bytes(record[24..28].try_into().unwrap()),
            kind: record[28],
        };
        if slot.kind != kind || slot.offset + slot.bytes > self.bytes {
            return Err(ContentError::InvalidRecord("row slot"));
        }
        Ok(Some(slot))
    }

    /// The slot run one kind owns, as `(first, count)`.
    const fn run(&self, kind: u8) -> (usize, usize) {
        match kind {
            KIND_DIRECTORY => (0, self.directory_rows),
            KIND_INODE => (self.directory_rows, self.inode_rows),
            _ => (self.directory_rows + self.inode_rows, self.new_rows),
        }
    }

    /// Decodes the payload of one slot.
    fn read(&self, slot: &Slot) -> ContentResult<Vec<u8>> {
        let length = usize::try_from(slot.bytes).map_err(|_| ContentError::LengthOverflow)?;
        let mut payload = vec![0_u8; length];
        let mut file = &self.file;
        file.seek(SeekFrom::Start(slot.offset))
            .and_then(|_| file.read_exact(&mut payload))
            .map_err(|_| ContentError::Io)?;
        Ok(payload)
    }

    /// Decodes one directory row from its payload.
    fn directory(&self, slot: &Slot) -> ContentResult<DirectoryUpdate> {
        let payload = self.read(slot)?;
        let mut reader = PayloadReader::new(&payload);
        let parent = reader.word()?;
        let count = reader.count()?;
        if count != slot.records {
            return Err(ContentError::InvalidRecord("directory row count"));
        }
        let mut changes = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let length = reader.byte()? as usize;
            let name = PathName::from_bytes(reader.take(length)?)
                .map_err(|_| ContentError::InvalidRecord("directory row name"))?;
            let serial = reader.word()?;
            changes.push((name, (serial != 0).then_some(serial)));
        }
        if !reader.is_empty() {
            return Err(ContentError::InvalidRecord("directory row"));
        }
        Ok(DirectoryUpdate { parent, changes })
    }

    /// Decodes one typed inode value from its payload.
    fn inode(&self, slot: &Slot) -> ContentResult<InodeUpdate> {
        let payload = self.read(slot)?;
        let mut reader = PayloadReader::new(&payload);
        let serial = reader.word()?;
        let kind = InodeKind::from_code(reader.byte()?)
            .map_err(|_| ContentError::InvalidRecord("inode kind"))?;
        let content_root = ObjectId::from_bytes(reader.take(32)?)
            .map_err(|_| ContentError::InvalidRecord("content root"))?;
        let metadata_root = ObjectId::from_bytes(reader.take(32)?)
            .map_err(|_| ContentError::InvalidRecord("metadata root"))?;
        if !reader.is_empty() {
            return Err(ContentError::InvalidRecord("inode row"));
        }
        Ok(InodeUpdate {
            serial,
            value: InodeValue {
                kind,
                namespace_ref_count: 0,
                content_root,
                metadata_root,
            },
        })
    }

    /// The slot of one key in a kind's run, by binary search over the run.
    fn find(&self, kind: u8, key: u64) -> ContentResult<Option<Slot>> {
        let (_, count) = self.run(kind);
        let mut low = 0_usize;
        let mut high = count;
        while low < high {
            let middle = low + (high - low) / 2;
            let slot = self
                .slot(kind, middle)?
                .ok_or(ContentError::InvalidRecord("row slot"))?;
            match slot.key.cmp(&key) {
                std::cmp::Ordering::Less => low = middle + 1,
                std::cmp::Ordering::Greater => high = middle,
                std::cmp::Ordering::Equal => return Ok(Some(slot)),
            }
        }
        Ok(None)
    }
}

impl RowSource for RowSpool {
    fn directory_rows(&self) -> usize {
        self.directory_rows
    }
    fn inode_rows(&self) -> usize {
        self.inode_rows
    }
    fn new_rows(&self) -> usize {
        self.new_rows
    }
    fn directories(&self) -> ContentResult<Box<dyn DirectoryRowSource + '_>> {
        Ok(Box::new(SpoolDirectories { spool: self, at: 0 }))
    }
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>> {
        Ok(Box::new(SpoolInodes { spool: self, at: 0 }))
    }
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>> {
        Ok(Box::new(SpoolSerials { spool: self, at: 0 }))
    }
    fn directory_for(&self, parent: u64) -> ContentResult<Option<DirectoryUpdate>> {
        match self.find(KIND_DIRECTORY, parent)? {
            Some(slot) => Ok(Some(self.directory(&slot)?)),
            None => Ok(None),
        }
    }
    fn value_for(&self, serial: u64) -> ContentResult<Option<InodeValue>> {
        match self.find(KIND_INODE, serial)? {
            Some(slot) => Ok(Some(self.inode(&slot)?.value)),
            None => Ok(None),
        }
    }
    fn new_position(&self, serial: u64) -> ContentResult<Option<usize>> {
        let (_, count) = self.run(KIND_SERIAL);
        let mut low = 0_usize;
        let mut high = count;
        while low < high {
            let middle = low + (high - low) / 2;
            let slot = self
                .slot(KIND_SERIAL, middle)?
                .ok_or(ContentError::InvalidRecord("row slot"))?;
            match slot.key.cmp(&serial) {
                std::cmp::Ordering::Less => low = middle + 1,
                std::cmp::Ordering::Greater => high = middle,
                std::cmp::Ordering::Equal => return Ok(Some(middle)),
            }
        }
        Ok(None)
    }
}

impl Drop for RowSpool {
    fn drop(&mut self) {
        if !self.path.as_os_str().is_empty() {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// One row's entry in the spool's slot table.
#[derive(Clone, Copy, Debug)]
struct Slot {
    key: u64,
    offset: u64,
    bytes: u64,
    records: u32,
    kind: u8,
}

struct SpoolDirectories<'a> {
    spool: &'a RowSpool,
    at: usize,
}

impl DirectoryRowSource for SpoolDirectories<'_> {
    fn next_row(&mut self) -> ContentResult<Option<DirectoryUpdate>> {
        match self.spool.slot(KIND_DIRECTORY, self.at)? {
            Some(slot) => {
                self.at += 1;
                Ok(Some(self.spool.directory(&slot)?))
            }
            None => Ok(None),
        }
    }
}

struct SpoolInodes<'a> {
    spool: &'a RowSpool,
    at: usize,
}

impl InodeRowSource for SpoolInodes<'_> {
    fn next_row(&mut self) -> ContentResult<Option<InodeUpdate>> {
        match self.spool.slot(KIND_INODE, self.at)? {
            Some(slot) => {
                self.at += 1;
                Ok(Some(self.spool.inode(&slot)?))
            }
            None => Ok(None),
        }
    }
}

struct SpoolSerials<'a> {
    spool: &'a RowSpool,
    at: usize,
}

impl SerialRowSource for SpoolSerials<'_> {
    fn next_row(&mut self) -> ContentResult<Option<u64>> {
        match self.spool.slot(KIND_SERIAL, self.at)? {
            Some(slot) => {
                self.at += 1;
                Ok(Some(slot.key))
            }
            None => Ok(None),
        }
    }
}

/// A bounded reader over one decoded payload.
struct PayloadReader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> PayloadReader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }
    fn take(&mut self, count: usize) -> ContentResult<&'a [u8]> {
        let end = self
            .at
            .checked_add(count)
            .ok_or(ContentError::LengthOverflow)?;
        let slice = self
            .bytes
            .get(self.at..end)
            .ok_or(ContentError::InvalidRecord("row payload"))?;
        self.at = end;
        Ok(slice)
    }
    fn byte(&mut self) -> ContentResult<u8> {
        Ok(self.take(1)?[0])
    }
    fn word(&mut self) -> ContentResult<u64> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn count(&mut self) -> ContentResult<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn is_empty(&self) -> bool {
        self.at == self.bytes.len()
    }
}
