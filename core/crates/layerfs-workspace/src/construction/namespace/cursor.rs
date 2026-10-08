//! The sealed streamed row source: record windows, captured names and points.
use super::records::{add, key, read_number, serial, verify, Shared, Totals, FRESH, HEADER, VALUE};
use crate::construction::outcome::CapturedNamespaceWork;
use crate::{OverlayCapturedNamespace, OverlayOperationRecords};
use layerfs_content::{
    filesystem::{
        DirectoryChangeLookup, DirectoryChangeSource, DirectoryHeader, DirectoryHeaderSource,
        InodeRowSource, InodeUpdate, PathName, SerialRowSource, StreamedRowSource,
    },
    object::inode_leaf::{decode_inode_value, InodeValue},
    ContentError, ContentResult,
};
use layerfs_overlay::{CapturedReader, DirectoryEntry, PAGE_ROWS};
use std::cell::RefCell;

/// One fixed window of sealed point answers, one slot per serial residue.
/// A sealed record never changes, so a retained answer cannot go stale; a
/// refused or undecodable read is never retained.
struct Memo<T: Copy>(RefCell<Vec<Option<(u64, T)>>>);
impl<T: Copy> Memo<T> {
    fn new() -> Self {
        Self(RefCell::new(vec![None; PAGE_ROWS]))
    }
    fn slot(serial: u64) -> usize {
        (serial % PAGE_ROWS as u64) as usize
    }
    fn get(&self, serial: u64) -> Option<T> {
        self.0.borrow()[Self::slot(serial)]
            .filter(|(known, _)| *known == serial)
            .map(|(_, answer)| answer)
    }
    fn set(&self, serial: u64, answer: T) {
        self.0.borrow_mut()[Self::slot(serial)] = Some((serial, answer));
    }
}
/// Every pass and point answers the same sealed rows: headers, values and
/// fresh ranks from this attempt's records, names from the retained reader.
/// One window of at most PAGE_ROWS is resident per open pass and per kind of
/// point answer, whatever the size of the change.
pub(crate) struct Cursor<'s, 'a, P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized> {
    provider: &'a P,
    shared: &'s Shared<'a, P>,
    reader: CapturedReader,
    context: &'s [u8],
    headers: usize,
    values: usize,
    fresh: usize,
    /// Recent header, value and fresh-rank answers, absence included.
    header: Memo<Option<u64>>,
    value: Memo<Option<InodeValue>>,
    rank: Memo<Option<usize>>,
}
impl<'s, 'a, P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized> Cursor<'s, 'a, P> {
    pub fn new(
        provider: &'a P,
        shared: &'s Shared<'a, P>,
        reader: CapturedReader,
        context: &'s [u8],
        totals: Totals,
    ) -> ContentResult<Self> {
        verify(shared, context)?;
        let rows = |total: u64| usize::try_from(total).map_err(|_| ContentError::LengthOverflow);
        Ok(Self {
            provider,
            shared,
            reader,
            context,
            headers: rows(totals.headers)?,
            values: rows(totals.values)?,
            fresh: rows(totals.fresh)?,
            header: Memo::new(),
            value: Memo::new(),
            rank: Memo::new(),
        })
    }
    fn count(&self, counter: impl FnOnce(&mut CapturedNamespaceWork) -> &mut u64) {
        add(counter(&mut self.shared.borrow_mut().work), 1);
    }
    fn fail(&self, original: ContentError) -> ContentError {
        self.shared.borrow_mut().content(original)
    }
    fn record(&self, kind: u32, serial: u64) -> ContentResult<Option<Vec<u8>>> {
        self.shared.borrow_mut().get(key(kind, serial))
    }
    fn header(&self, parent: u64) -> ContentResult<Option<u64>> {
        if let Some(rows) = self.header.get(parent) {
            self.shared.borrow().ready()?;
            return Ok(rows);
        }
        let rows = match self.record(HEADER, parent)? {
            Some(raw) => Some(
                read_number(&raw, "captured namespace header").map_err(|error| self.fail(error))?,
            ),
            None => None,
        };
        self.header.set(parent, rows);
        Ok(rows)
    }
    fn value(&self, serial: u64) -> ContentResult<Option<InodeValue>> {
        if let Some(value) = self.value.get(serial) {
            self.shared.borrow().ready()?;
            return Ok(value);
        }
        let value = match self.record(VALUE, serial)? {
            Some(raw) => Some(decode_inode_value(&raw).map_err(|error| self.fail(error))?),
            None => None,
        };
        self.value.set(serial, value);
        Ok(value)
    }
    fn rank(&self, serial: u64) -> ContentResult<Option<usize>> {
        if let Some(rank) = self.rank.get(serial) {
            self.shared.borrow().ready()?;
            return Ok(rank);
        }
        let rank = match self.record(FRESH, serial)? {
            Some(raw) => Some(
                read_number(&raw, "captured namespace fresh rank")
                    .and_then(|rank| {
                        usize::try_from(rank).map_err(|_| ContentError::LengthOverflow)
                    })
                    .map_err(|error| self.fail(error))?,
            ),
            None => None,
        };
        self.rank.set(serial, rank);
        Ok(rank)
    }
    /// One original parent-local page of the retained reader.
    fn names(&self, parent: u64, after: Option<Vec<u8>>) -> ContentResult<Vec<DirectoryEntry>> {
        self.shared.borrow().ready()?;
        self.count(|work| &mut work.change_pages);
        let page = self
            .provider
            .captured_directory_entries(self.reader, parent, after)
            .map_err(|original| self.shared.borrow_mut().original(original))?;
        if page.len() > PAGE_ROWS {
            return Err(self.fail(ContentError::InvalidRecord("captured namespace page shape")));
        }
        let mut owner = self.shared.borrow_mut();
        owner.work.largest_page = owner.work.largest_page.max(page.len() as u64);
        Ok(page)
    }
}
/// One ordered pass over one record kind's serials, a key window at a time.
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
    fn next<P: OverlayOperationRecords + ?Sized>(
        &mut self,
        shared: &Shared<'_, P>,
    ) -> ContentResult<Option<u64>> {
        if self.window.len() == 0 && !self.ended {
            let keys = shared.borrow_mut().keys_after(self.kind, self.after)?;
            if keys.len() > PAGE_ROWS {
                return Err(shared
                    .borrow_mut()
                    .content(ContentError::InvalidRecord("captured namespace key window")));
            }
            match keys.last() {
                Some(last) => self.after = Some(*last),
                None => self.ended = true,
            }
            self.window = keys.into_iter();
        }
        match self.window.next() {
            Some(raw) => serial(&raw)
                .map(Some)
                .map_err(|error| shared.borrow_mut().content(error)),
            None => Ok(None),
        }
    }
}
struct Headers<'c, 's, 'a, P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized> {
    cursor: &'c Cursor<'s, 'a, P>,
    keys: Keys,
}
impl<P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized> DirectoryHeaderSource
    for Headers<'_, '_, '_, P>
{
    fn next_row(&mut self) -> ContentResult<Option<DirectoryHeader>> {
        let Some(parent) = self.keys.next(self.cursor.shared)? else {
            return Ok(None);
        };
        // The same point the lookups use, so a pass cannot answer differently;
        // its answer stays for the checks that follow this row.
        let change_rows = self.cursor.header(parent)?.ok_or_else(|| {
            self.cursor.fail(ContentError::InvalidRecord(
                "missing captured namespace header",
            ))
        })?;
        self.cursor.count(|work| &mut work.header_rows);
        Ok(Some(DirectoryHeader {
            parent,
            change_rows,
        }))
    }
}
struct Changes<'c, 's, 'a, P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized> {
    cursor: &'c Cursor<'s, 'a, P>,
    parent: u64,
    window: std::vec::IntoIter<DirectoryEntry>,
    after: Option<Vec<u8>>,
    ended: bool,
}
impl<P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized> DirectoryChangeSource
    for Changes<'_, '_, '_, P>
{
    fn next_row(&mut self) -> ContentResult<Option<(PathName, Option<u64>)>> {
        if self.window.len() == 0 && !self.ended {
            let page = self.cursor.names(self.parent, self.after.clone())?;
            // A short page ends a sealed sequence: no empty page job follows.
            self.ended = page.len() < PAGE_ROWS;
            self.window = page.into_iter();
        }
        let Some(row) = self.window.next() else {
            return Ok(None);
        };
        if row.parent != self.parent
            || self
                .after
                .as_deref()
                .is_some_and(|after| after >= row.name.as_slice())
        {
            return Err(self
                .cursor
                .fail(ContentError::InvalidRecord("captured name cursor order")));
        }
        let name = PathName::from_bytes(&row.name).map_err(|error| self.cursor.fail(error))?;
        self.after = Some(row.name);
        self.cursor.count(|work| &mut work.change_rows);
        Ok(Some((name, row.serial)))
    }
}
struct Values<'c, 's, 'a, P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized> {
    cursor: &'c Cursor<'s, 'a, P>,
    keys: Keys,
}
impl<P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized> InodeRowSource
    for Values<'_, '_, '_, P>
{
    fn next_row(&mut self) -> ContentResult<Option<InodeUpdate>> {
        let Some(serial) = self.keys.next(self.cursor.shared)? else {
            return Ok(None);
        };
        let value = self.cursor.value(serial)?.ok_or_else(|| {
            self.cursor.fail(ContentError::InvalidRecord(
                "missing captured namespace value",
            ))
        })?;
        self.cursor.count(|work| &mut work.value_rows);
        Ok(Some(InodeUpdate { serial, value }))
    }
}
struct Fresh<'c, 's, 'a, P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized> {
    cursor: &'c Cursor<'s, 'a, P>,
    keys: Keys,
}
impl<P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized> SerialRowSource
    for Fresh<'_, '_, '_, P>
{
    fn next_row(&mut self) -> ContentResult<Option<u64>> {
        let serial = self.keys.next(self.cursor.shared)?;
        if serial.is_some() {
            self.cursor.count(|work| &mut work.fresh_rows);
        }
        Ok(serial)
    }
}
impl<P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized> StreamedRowSource
    for Cursor<'_, '_, P>
{
    fn directory_rows(&self) -> usize {
        self.headers
    }
    fn inode_rows(&self) -> usize {
        self.values
    }
    fn new_rows(&self) -> usize {
        self.fresh
    }
    fn directory_headers(&self) -> ContentResult<Box<dyn DirectoryHeaderSource + '_>> {
        verify(self.shared, self.context)?;
        self.count(|work| &mut work.header_opens);
        Ok(Box::new(Headers {
            cursor: self,
            keys: Keys::new(HEADER),
        }))
    }
    fn directory_header(&self, parent: u64) -> ContentResult<Option<DirectoryHeader>> {
        self.count(|work| &mut work.header_points);
        Ok(self.header(parent)?.map(|change_rows| DirectoryHeader {
            parent,
            change_rows,
        }))
    }
    fn directory_changes(&self, parent: u64) -> ContentResult<Box<dyn DirectoryChangeSource + '_>> {
        self.count(|work| &mut work.change_opens);
        // A parent without a sealed header has no declared sequence: names the
        // reader still holds under a dropped parent are never served.
        if self.header(parent)?.is_none() {
            return Err(self.fail(ContentError::InvalidRecord(
                "captured namespace changes without header",
            )));
        }
        Ok(Box::new(Changes {
            cursor: self,
            parent,
            window: Vec::new().into_iter(),
            after: None,
            ended: false,
        }))
    }
    fn directory_change(
        &self,
        parent: u64,
        name: &PathName,
    ) -> ContentResult<DirectoryChangeLookup> {
        self.count(|work| &mut work.change_points);
        if self.header(parent)?.is_none() {
            return Ok(DirectoryChangeLookup::Unchanged);
        }
        let row = self
            .provider
            .captured_directory_entry(self.reader, parent, name.as_bytes())
            .map_err(|original| self.shared.borrow_mut().original(original))?;
        match row {
            None => Ok(DirectoryChangeLookup::Unchanged),
            Some(row) if row.parent != parent || row.name != name.as_bytes() => {
                Err(self.fail(ContentError::InvalidRecord("captured name point")))
            }
            Some(row) => Ok(row
                .serial
                .map_or(DirectoryChangeLookup::Removed, DirectoryChangeLookup::Bound)),
        }
    }
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>> {
        verify(self.shared, self.context)?;
        self.count(|work| &mut work.value_opens);
        Ok(Box::new(Values {
            cursor: self,
            keys: Keys::new(VALUE),
        }))
    }
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>> {
        verify(self.shared, self.context)?;
        self.count(|work| &mut work.fresh_opens);
        Ok(Box::new(Fresh {
            cursor: self,
            keys: Keys::new(FRESH),
        }))
    }
    fn value_for(&self, serial: u64) -> ContentResult<Option<InodeValue>> {
        self.count(|work| &mut work.value_points);
        self.value(serial)
    }
    fn new_position(&self, serial: u64) -> ContentResult<Option<usize>> {
        self.count(|work| &mut work.fresh_points);
        self.rank(serial)
    }
}
