//! The sealed streamed row source: record windows, captured names and points.
use super::records::{add, key, read_number, serial, verify, Shared, Totals, FRESH, HEADER, VALUE};
use crate::construction::outcome::{nanos, CapturedNamespaceWork};
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
use std::{cell::RefCell, time::Instant};

/// Windows of sealed answers each Cursor holds per record kind. A window is
/// at most PAGE_ROWS rows of one key-ordered range and answers every serial
/// in that range, absence included, so a sweep in serial order reads each
/// sealed record once and a point near a recent one reads nothing. Held only
/// by this Cursor, whatever the size of the change.
const WINDOWS: usize = 16;
/// One key-ordered range of sealed answers: every serial in `from..=to`
/// that has a record is in `rows`; any other serial in the range has none.
#[derive(Clone)]
struct Window<T> {
    from: u64,
    to: u64,
    rows: Vec<(u64, T)>,
}
impl<T: Copy> Window<T> {
    fn answer(&self, serial: u64) -> Option<Option<T>> {
        (self.from..=self.to).contains(&serial).then(|| {
            self.rows
                .binary_search_by_key(&serial, |(known, _)| *known)
                .ok()
                .map(|index| self.rows[index].1)
        })
    }
}
/// A fixed ring of windows. A sealed record never changes, so a retained
/// answer cannot go stale; a refused or undecodable window is never retained.
struct Windows<T>(RefCell<(Vec<Window<T>>, usize)>);
impl<T: Copy> Windows<T> {
    fn new() -> Self {
        Self(RefCell::new((Vec::with_capacity(WINDOWS), 0)))
    }
    fn answer(&self, serial: u64) -> Option<Option<T>> {
        self.0
            .borrow()
            .0
            .iter()
            .find_map(|window| window.answer(serial))
    }
    fn keep(&self, window: Window<T>) {
        let (windows, next) = &mut *self.0.borrow_mut();
        if windows.len() < WINDOWS {
            windows.push(window);
        } else {
            windows[*next] = window;
            *next = (*next + 1) % WINDOWS;
        }
    }
}
fn header_rows(raw: &[u8]) -> ContentResult<u64> {
    read_number(raw, "captured namespace header")
}
fn fresh_rank(raw: &[u8]) -> ContentResult<usize> {
    read_number(raw, "captured namespace fresh rank")
        .and_then(|rank| usize::try_from(rank).map_err(|_| ContentError::LengthOverflow))
}
/// Every pass and point answers the same sealed rows: headers, values and
/// fresh ranks from this attempt's records, names from the retained reader.
/// One window of at most PAGE_ROWS rows is read per job, and at most WINDOWS
/// of them per kind are held, whatever the size of the change.
pub(crate) struct Cursor<'s, 'a, P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized> {
    provider: &'a P,
    shared: &'s Shared<'a, P>,
    reader: CapturedReader,
    context: &'s [u8],
    headers: usize,
    values: usize,
    fresh: usize,
    /// Recent header, value and fresh-rank ranges, absence included.
    header: Windows<u64>,
    value: Windows<InodeValue>,
    rank: Windows<usize>,
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
            header: Windows::new(),
            value: Windows::new(),
            rank: Windows::new(),
        })
    }
    fn count(&self, counter: impl FnOnce(&mut CapturedNamespaceWork) -> &mut u64) {
        add(counter(&mut self.shared.borrow_mut().work), 1);
    }
    /// Charges one captured name read's time to the update.
    fn charge(&self, started: Instant) {
        add(
            &mut self.shared.borrow_mut().work.update_capture_ns,
            nanos(started),
        );
    }
    fn fail(&self, original: ContentError) -> ContentError {
        self.shared.borrow_mut().content(original)
    }
    /// The window of `kind` after `after` (None: from the first serial), as
    /// one job: the rows must be ordered past `after` and decodable, and a
    /// window that does not end the kind must hold a row.
    fn load<T: Copy>(
        &self,
        kind: u32,
        after: Option<u64>,
        decode: fn(&[u8]) -> ContentResult<T>,
    ) -> ContentResult<(Window<T>, bool)> {
        let (entries, ended) = self
            .shared
            .borrow_mut()
            .entries_after(kind, after.map(|serial| key(kind, serial).key))?;
        if entries.len() > PAGE_ROWS || (entries.is_empty() && !ended) {
            return Err(self.fail(ContentError::InvalidRecord(
                "captured namespace record window",
            )));
        }
        let from = match after {
            Some(after) => after.checked_add(1).ok_or(ContentError::LengthOverflow)?,
            None => 0,
        };
        let mut rows = Vec::with_capacity(entries.len());
        let mut previous = after;
        for (raw, value) in entries {
            let serial = serial(&raw).map_err(|error| self.fail(error))?;
            if previous.is_some_and(|previous| previous >= serial) {
                return Err(self.fail(ContentError::InvalidRecord(
                    "captured namespace window order",
                )));
            }
            rows.push((serial, decode(&value).map_err(|error| self.fail(error))?));
            previous = Some(serial);
        }
        let to = match (ended, previous) {
            (true, _) | (false, None) => u64::MAX,
            (false, Some(last)) => last,
        };
        Ok((Window { from, to, rows }, ended))
    }
    /// One sealed answer: from a held window, else from the window that
    /// starts at `serial`, which is then held.
    fn point<T: Copy>(
        &self,
        windows: &Windows<T>,
        kind: u32,
        serial: u64,
        decode: fn(&[u8]) -> ContentResult<T>,
    ) -> ContentResult<Option<T>> {
        if let Some(answer) = windows.answer(serial) {
            self.shared.borrow().ready()?;
            return Ok(answer);
        }
        let (window, _) = self.load(kind, serial.checked_sub(1), decode)?;
        let answer = window.answer(serial).ok_or(ContentError::InvalidRecord(
            "captured namespace window range",
        ))?;
        windows.keep(window);
        Ok(answer)
    }
    fn header(&self, parent: u64) -> ContentResult<Option<u64>> {
        self.point(&self.header, HEADER, parent, header_rows)
    }
    fn value(&self, serial: u64) -> ContentResult<Option<InodeValue>> {
        self.point(&self.value, VALUE, serial, decode_inode_value)
    }
    fn rank(&self, serial: u64) -> ContentResult<Option<usize>> {
        self.point(&self.rank, FRESH, serial, fresh_rank)
    }
    /// One original parent-local page of the retained reader.
    fn names(&self, parent: u64, after: Option<Vec<u8>>) -> ContentResult<Vec<DirectoryEntry>> {
        self.shared.borrow().ready()?;
        self.count(|work| &mut work.change_pages);
        let started = Instant::now();
        let page = self
            .provider
            .captured_directory_entries(self.reader, parent, after);
        self.charge(started);
        let page = page.map_err(|original| self.shared.borrow_mut().original(original))?;
        if page.len() > PAGE_ROWS {
            return Err(self.fail(ContentError::InvalidRecord("captured namespace page shape")));
        }
        let mut owner = self.shared.borrow_mut();
        owner.work.largest_page = owner.work.largest_page.max(page.len() as u64);
        Ok(page)
    }
}
/// One ordered pass over one record kind, a window at a time. Each window
/// it reads is also held for the points that follow its rows.
struct Pass<T> {
    kind: u32,
    decode: fn(&[u8]) -> ContentResult<T>,
    rows: std::vec::IntoIter<(u64, T)>,
    after: Option<u64>,
    ended: bool,
}
impl<T: Copy> Pass<T> {
    fn new(kind: u32, decode: fn(&[u8]) -> ContentResult<T>) -> Self {
        Self {
            kind,
            decode,
            rows: Vec::new().into_iter(),
            after: None,
            ended: false,
        }
    }
    fn next<P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized>(
        &mut self,
        cursor: &Cursor<'_, '_, P>,
        windows: &Windows<T>,
    ) -> ContentResult<Option<(u64, T)>> {
        loop {
            if let Some(row) = self.rows.next() {
                return Ok(Some(row));
            }
            if self.ended {
                return Ok(None);
            }
            // A window that ends the kind is the last: no empty window job
            // follows it.
            let (window, ended) = cursor.load(self.kind, self.after, self.decode)?;
            self.ended = ended;
            self.after = window.rows.last().map(|(serial, _)| *serial).or(self.after);
            self.rows = window.rows.clone().into_iter();
            windows.keep(window);
        }
    }
}
struct Headers<'c, 's, 'a, P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized> {
    cursor: &'c Cursor<'s, 'a, P>,
    pass: Pass<u64>,
}
impl<P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized> DirectoryHeaderSource
    for Headers<'_, '_, '_, P>
{
    fn next_row(&mut self) -> ContentResult<Option<DirectoryHeader>> {
        // The pass's window is held, so the points that follow this row
        // answer from the same sealed rows.
        let Some((parent, change_rows)) = self.pass.next(self.cursor, &self.cursor.header)? else {
            return Ok(None);
        };
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
    pass: Pass<InodeValue>,
}
impl<P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized> InodeRowSource
    for Values<'_, '_, '_, P>
{
    fn next_row(&mut self) -> ContentResult<Option<InodeUpdate>> {
        let Some((serial, value)) = self.pass.next(self.cursor, &self.cursor.value)? else {
            return Ok(None);
        };
        self.cursor.count(|work| &mut work.value_rows);
        Ok(Some(InodeUpdate { serial, value }))
    }
}
struct Fresh<'c, 's, 'a, P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized> {
    cursor: &'c Cursor<'s, 'a, P>,
    pass: Pass<usize>,
    /// The dense rank the next fresh serial must carry.
    next: usize,
}
impl<P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized> SerialRowSource
    for Fresh<'_, '_, '_, P>
{
    fn next_row(&mut self) -> ContentResult<Option<u64>> {
        let Some((serial, rank)) = self.pass.next(self.cursor, &self.cursor.rank)? else {
            return Ok(None);
        };
        if rank != self.next {
            return Err(self
                .cursor
                .fail(ContentError::InvalidRecord("captured namespace fresh rank")));
        }
        self.next += 1;
        self.cursor.count(|work| &mut work.fresh_rows);
        Ok(Some(serial))
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
            pass: Pass::new(HEADER, header_rows),
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
        let started = Instant::now();
        let row = self
            .provider
            .captured_directory_entry(self.reader, parent, name.as_bytes());
        self.charge(started);
        let row = row.map_err(|original| self.shared.borrow_mut().original(original))?;
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
            pass: Pass::new(VALUE, decode_inode_value),
        }))
    }
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>> {
        verify(self.shared, self.context)?;
        self.count(|work| &mut work.fresh_opens);
        Ok(Box::new(Fresh {
            cursor: self,
            pass: Pass::new(FRESH, fresh_rank),
            next: 0,
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
    fn are_new(&self, serials: &[u64]) -> ContentResult<Vec<bool>> {
        add(
            &mut self.shared.borrow_mut().work.fresh_points,
            serials.len() as u64,
        );
        if serials.len() > PAGE_ROWS {
            return Err(self.fail(ContentError::InvalidRecord(
                "captured namespace point window",
            )));
        }
        serials
            .iter()
            .map(|serial| Ok(self.rank(*serial)?.is_some()))
            .collect()
    }
}
