//! The merge plan's rows as the streamed source of the backed update route.
//!
//! The rows are the decided changes, kept in the merge's records (#355):
//! each changed directory's names in chunks of at most [`CHUNK_BYTES`] under
//! its serial, a header with their count, and values and fresh ranks under
//! their serials, which order the records as the update reads them. A pass
//! holds one window of keys or one chunk. Point lookups keep one header, one
//! chunk and one window of fresh serials, which also answers which serials
//! between them are not fresh; the update asks in near serial and name
//! order, so most answers need no read.

use std::cell::RefCell;

use super::codec::Cursor;
use super::spill::{first, pair, single, Spill, CHANGES, CHUNK_BYTES, FIRST, FRESH, HEADER, VALUE};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::construction::input::{
    DirectoryChangeLookup, DirectoryChangeSource, DirectoryHeader, DirectoryHeaderSource,
    InodeRowSource, PreparedDirectoryStreams, SerialRowSource, StreamedRowSource,
};
use crate::filesystem::{FilesystemResources, FilesystemRootId, InodeScope, InodeUpdate, PathName};
use crate::object::inode_leaf::{decode_inode_value, InodeValue};

fn malformed() -> ContentError {
    ContentError::InvalidRecord("merge plan record")
}

/// Rows the plan declares.
#[derive(Clone, Copy, Default)]
pub(crate) struct PlanCounts {
    pub(super) headers: usize,
    pub(super) values: usize,
    pub(super) fresh: usize,
}

/// Writes one directory's changed names, in name order, as chunks.
pub(crate) struct ChangeWriter {
    parent: u64,
    count: u64,
    chunks: u64,
    chunk: Vec<u8>,
    first: Vec<u8>,
}

impl ChangeWriter {
    pub(crate) fn new(parent: u64) -> Self {
        Self {
            parent,
            count: 0,
            chunks: 0,
            chunk: Vec::new(),
            first: Vec::new(),
        }
    }

    pub(crate) fn push(
        &mut self,
        spill: &Spill<'_>,
        name: &PathName,
        binding: Option<u64>,
    ) -> ContentResult<()> {
        let name = name.as_bytes();
        if self.chunk.len() + name.len() + 11 > CHUNK_BYTES {
            self.flush(spill)?;
        }
        if self.chunk.is_empty() {
            self.first = name.to_vec();
        }
        let length = u16::try_from(name.len()).map_err(|_| malformed())?;
        self.chunk.extend_from_slice(&length.to_be_bytes());
        self.chunk.extend_from_slice(name);
        match binding {
            None => self.chunk.push(0),
            Some(serial) => {
                self.chunk.push(1);
                self.chunk.extend_from_slice(&serial.to_be_bytes());
            }
        }
        self.count += 1;
        Ok(())
    }

    /// True when no name was pushed.
    pub(crate) fn is_empty(&self) -> bool {
        self.count == 0
    }

    fn flush(&mut self, spill: &Spill<'_>) -> ContentResult<()> {
        if self.chunk.is_empty() {
            return Ok(());
        }
        let key = pair(self.parent, self.chunks);
        spill.insert(FIRST, key, std::mem::take(&mut self.first))?;
        spill.insert(CHANGES, key, std::mem::take(&mut self.chunk))?;
        self.chunks += 1;
        Ok(())
    }

    /// Writes the last chunk and the header; a parent written twice is refused.
    pub(crate) fn finish(
        mut self,
        spill: &Spill<'_>,
        counts: &mut PlanCounts,
    ) -> ContentResult<()> {
        self.flush(spill)?;
        let mut header = self.count.to_be_bytes().to_vec();
        header.extend_from_slice(&self.chunks.to_be_bytes());
        if spill.get(HEADER, single(self.parent))?.is_some() {
            return Err(ContentError::InvalidRecord("merge directory written twice"));
        }
        spill.insert(HEADER, single(self.parent), header)?;
        counts.headers += 1;
        Ok(())
    }
}

type Change = (PathName, Option<u64>);
/// A parent's change count and chunk count.
type Header = (u64, u64);

fn decode_changes(bytes: &[u8]) -> ContentResult<Vec<Change>> {
    let mut cursor = Cursor(bytes);
    let mut out = Vec::new();
    while !cursor.0.is_empty() {
        let length = u16::from_be_bytes(cursor.take(2)?.try_into().map_err(|_| malformed())?);
        let name = std::str::from_utf8(cursor.take(length as usize)?).map_err(|_| malformed())?;
        let binding = match cursor.byte()? {
            0 => None,
            1 => Some(cursor.number()?),
            _ => return Err(malformed()),
        };
        out.push((PathName::new(name)?, binding));
    }
    Ok(out)
}

/// The plan rows of one merge, read from its records.
pub(super) struct PlanRows<'s, 'r> {
    spill: &'s Spill<'r>,
    base: FilesystemRootId,
    scope: InodeScope,
    root_serial: u64,
    counts: PlanCounts,
    /// The chunk last read by a point lookup: parent, chunk and its names.
    memo: RefCell<Option<(u64, u64, Vec<Change>)>>,
    /// The last header asked for, absence included.
    header: RefCell<Option<(u64, Option<Header>)>>,
    /// Fresh serials after `start` up to the last held; all of them when the
    /// window reached the end.
    fresh: RefCell<FreshWindow>,
}

/// A window of consecutive fresh serials and their ranks.
#[derive(Default)]
struct FreshWindow {
    start: Option<u64>,
    serials: Vec<(u64, Option<usize>)>,
    complete: bool,
    loaded: bool,
}

impl FreshWindow {
    /// Whether the window decides `serial`.
    fn covers(&self, serial: u64) -> bool {
        self.loaded
            && self.start.is_none_or(|start| serial > start)
            && (self.complete || self.serials.last().is_some_and(|(last, _)| serial <= *last))
    }
}

impl<'s, 'r> PlanRows<'s, 'r> {
    pub(super) fn new(
        spill: &'s Spill<'r>,
        base: FilesystemRootId,
        scope: InodeScope,
        root_serial: u64,
        counts: PlanCounts,
    ) -> Self {
        Self {
            spill,
            base,
            scope,
            root_serial,
            counts,
            memo: RefCell::new(None),
            header: RefCell::new(None),
            fresh: RefCell::new(FreshWindow::default()),
        }
    }

    /// The fresh rank of a serial, from the window, reading a new one first
    /// when it does not decide the serial.
    fn rank(&self, serial: u64) -> ContentResult<Option<usize>> {
        let mut window = self.fresh.borrow_mut();
        if !window.covers(serial) {
            let start = serial.checked_sub(1);
            let keys = self.spill.keys_after(FRESH, start.map(single))?;
            *window = FreshWindow {
                start,
                complete: keys.len() < 64,
                serials: keys.iter().map(|key| (first(key), None)).collect(),
                loaded: true,
            };
        }
        let Ok(index) = window
            .serials
            .binary_search_by_key(&serial, |(held, _)| *held)
        else {
            return Ok(None);
        };
        if let Some(rank) = window.serials[index].1 {
            return Ok(Some(rank));
        }
        let bytes = self
            .spill
            .get(FRESH, single(serial))?
            .ok_or_else(malformed)?;
        let mut cursor = Cursor(&bytes);
        let rank = usize::try_from(cursor.number()?).map_err(|_| malformed())?;
        cursor.end()?;
        window.serials[index].1 = Some(rank);
        Ok(Some(rank))
    }

    /// Change count and chunk count of a parent with a header.
    fn header(&self, parent: u64) -> ContentResult<Option<Header>> {
        if let Some((held, header)) = *self.header.borrow() {
            if held == parent {
                return Ok(header);
            }
        }
        let header = match self.spill.get(HEADER, single(parent))? {
            None => None,
            Some(bytes) => {
                let mut cursor = Cursor(&bytes);
                let header = (cursor.number()?, cursor.number()?);
                cursor.end()?;
                Some(header)
            }
        };
        *self.header.borrow_mut() = Some((parent, header));
        Ok(header)
    }

    /// The chunk of `parent`'s names that would hold `name`.
    fn chunk_for(&self, parent: u64, chunks: u64, name: &PathName) -> ContentResult<u64> {
        // The held chunk, or the one after it, when the name falls there:
        // names asked in order step through the chunks one read each.
        if let Some((held, chunk, changes)) = &*self.memo.borrow() {
            if *held == parent && changes.first().is_some_and(|(low, _)| low <= name) {
                if changes.last().is_some_and(|(high, _)| name <= high) || chunk + 1 == chunks {
                    return Ok(*chunk);
                }
                if self.first_name(parent, chunk + 1)?.as_slice() > name.as_bytes() {
                    return Ok(*chunk);
                }
            }
        }
        // The last chunk whose first name is not after `name`.
        let (mut low, mut high) = (0u64, chunks);
        while high - low > 1 {
            let middle = low + (high - low) / 2;
            if self.first_name(parent, middle)?.as_slice() <= name.as_bytes() {
                low = middle;
            } else {
                high = middle;
            }
        }
        Ok(low)
    }

    fn chunk(&self, parent: u64, chunk: u64) -> ContentResult<Vec<Change>> {
        let bytes = self
            .spill
            .get(CHANGES, pair(parent, chunk))?
            .ok_or_else(malformed)?;
        decode_changes(&bytes)
    }

    fn first_name(&self, parent: u64, chunk: u64) -> ContentResult<Vec<u8>> {
        self.spill
            .get(FIRST, pair(parent, chunk))?
            .ok_or_else(malformed)
    }
}

/// A pass over one record kind's keys, a window at a time.
struct Keys {
    kind: u32,
    window: std::vec::IntoIter<[u8; 32]>,
    after: Option<[u8; 32]>,
    ended: bool,
}

impl Keys {
    fn new(kind: u32) -> Self {
        Self {
            kind,
            window: Vec::new().into_iter(),
            after: None,
            ended: false,
        }
    }

    fn next(&mut self, spill: &Spill<'_>) -> ContentResult<Option<[u8; 32]>> {
        loop {
            if let Some(key) = self.window.next() {
                self.after = Some(key);
                return Ok(Some(key));
            }
            if self.ended {
                return Ok(None);
            }
            let keys = spill.keys_after(self.kind, self.after)?;
            self.ended = keys.is_empty();
            self.window = keys.into_iter();
        }
    }
}

struct Headers<'p, 's, 'r> {
    rows: &'p PlanRows<'s, 'r>,
    keys: Keys,
}

impl DirectoryHeaderSource for Headers<'_, '_, '_> {
    fn next_row(&mut self) -> ContentResult<Option<DirectoryHeader>> {
        let Some(key) = self.keys.next(self.rows.spill)? else {
            return Ok(None);
        };
        let parent = first(&key);
        let (count, _) = self.rows.header(parent)?.ok_or_else(malformed)?;
        Ok(Some(DirectoryHeader {
            parent,
            change_rows: count,
        }))
    }
}

struct Changes<'p, 's, 'r> {
    rows: &'p PlanRows<'s, 'r>,
    parent: u64,
    chunks: u64,
    read: u64,
    window: std::vec::IntoIter<Change>,
}

impl DirectoryChangeSource for Changes<'_, '_, '_> {
    fn next_row(&mut self) -> ContentResult<Option<Change>> {
        loop {
            if let Some(change) = self.window.next() {
                return Ok(Some(change));
            }
            if self.read == self.chunks {
                return Ok(None);
            }
            self.window = self.rows.chunk(self.parent, self.read)?.into_iter();
            self.read += 1;
        }
    }
}

struct Values<'p, 's, 'r> {
    rows: &'p PlanRows<'s, 'r>,
    keys: Keys,
}

impl InodeRowSource for Values<'_, '_, '_> {
    fn next_row(&mut self) -> ContentResult<Option<InodeUpdate>> {
        let Some(key) = self.keys.next(self.rows.spill)? else {
            return Ok(None);
        };
        let serial = first(&key);
        let value = self.rows.value_for(serial)?.ok_or_else(malformed)?;
        Ok(Some(InodeUpdate { serial, value }))
    }
}

struct Fresh<'p, 's, 'r> {
    rows: &'p PlanRows<'s, 'r>,
    keys: Keys,
}

impl SerialRowSource for Fresh<'_, '_, '_> {
    fn next_row(&mut self) -> ContentResult<Option<u64>> {
        Ok(self.keys.next(self.rows.spill)?.map(|key| first(&key)))
    }
}

impl StreamedRowSource for PlanRows<'_, '_> {
    fn directory_rows(&self) -> usize {
        self.counts.headers
    }
    fn inode_rows(&self) -> usize {
        self.counts.values
    }
    fn new_rows(&self) -> usize {
        self.counts.fresh
    }
    fn directory_headers(&self) -> ContentResult<Box<dyn DirectoryHeaderSource + '_>> {
        Ok(Box::new(Headers {
            rows: self,
            keys: Keys::new(HEADER),
        }))
    }
    fn directory_header(&self, parent: u64) -> ContentResult<Option<DirectoryHeader>> {
        Ok(self.header(parent)?.map(|(count, _)| DirectoryHeader {
            parent,
            change_rows: count,
        }))
    }
    fn directory_changes(&self, parent: u64) -> ContentResult<Box<dyn DirectoryChangeSource + '_>> {
        let chunks = self.header(parent)?.map_or(0, |(_, chunks)| chunks);
        Ok(Box::new(Changes {
            rows: self,
            parent,
            chunks,
            read: 0,
            window: Vec::new().into_iter(),
        }))
    }
    fn directory_change(
        &self,
        parent: u64,
        name: &PathName,
    ) -> ContentResult<DirectoryChangeLookup> {
        let Some((_, chunks)) = self.header(parent)? else {
            return Ok(DirectoryChangeLookup::Unchanged);
        };
        if chunks == 0 {
            return Ok(DirectoryChangeLookup::Unchanged);
        }
        let low = self.chunk_for(parent, chunks, name)?;
        let mut memo = self.memo.borrow_mut();
        if !matches!(&*memo, Some((p, c, _)) if *p == parent && *c == low) {
            *memo = Some((parent, low, self.chunk(parent, low)?));
        }
        let changes = &memo.as_ref().expect("memo").2;
        Ok(match changes.binary_search_by(|(key, _)| key.cmp(name)) {
            Err(_) => DirectoryChangeLookup::Unchanged,
            Ok(index) => changes[index]
                .1
                .map_or(DirectoryChangeLookup::Removed, DirectoryChangeLookup::Bound),
        })
    }
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>> {
        Ok(Box::new(Values {
            rows: self,
            keys: Keys::new(VALUE),
        }))
    }
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>> {
        Ok(Box::new(Fresh {
            rows: self,
            keys: Keys::new(FRESH),
        }))
    }
    fn value_for(&self, serial: u64) -> ContentResult<Option<InodeValue>> {
        self.spill
            .get(VALUE, single(serial))?
            .map(|bytes| decode_inode_value(&bytes))
            .transpose()
    }
    fn new_position(&self, serial: u64) -> ContentResult<Option<usize>> {
        self.rank(serial)
    }
    fn is_new(&self, serial: u64) -> ContentResult<bool> {
        let mut window = self.fresh.borrow_mut();
        if !window.covers(serial) {
            drop(window);
            self.rank(serial)?;
            window = self.fresh.borrow_mut();
        }
        Ok(window
            .serials
            .binary_search_by_key(&serial, |(held, _)| *held)
            .is_ok())
    }
    fn are_new(&self, serials: &[u64]) -> ContentResult<Vec<bool>> {
        serials.iter().map(|serial| self.is_new(*serial)).collect()
    }
}

impl PreparedDirectoryStreams for PlanRows<'_, '_> {
    fn base(&self) -> Option<FilesystemRootId> {
        Some(self.base)
    }
    fn scope(&self) -> InodeScope {
        self.scope
    }
    fn root_serial(&self) -> u64 {
        self.root_serial
    }
    fn resources(&self) -> FilesystemResources {
        FilesystemResources::default()
    }
}
