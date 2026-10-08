//! Two captured passes that seal headers, typed values, fresh ranks and totals.
use super::{
    inodes::Builder,
    records::{
        add, change, context, key, number, Facts, Shared, Totals, CONTEXT, FRESH, HEADER, VALUE,
    },
};
use crate::{OverlayCapturedNamespace, OverlayOperationRecords, WorkspaceResult};
use layerfs_content::{
    filesystem::{FilesystemRead, PathName},
    object::inode_leaf::encode_inode_value,
    ContentError, ContentResult, EditRecordChange, EditRecordExpected, FinalizedConsumer,
};
use layerfs_overlay::{DirectoryEntry, Inode, InodeKind, PAGE_ROWS};
use layerfs_telemetry::timer::{Active, TimingScope};

/// The changes of one guarded job: at most PAGE_ROWS of them, flushed strictly
/// ordered by kind and then serial, and always before the next captured page
/// is requested. Never a namespace-sized collection.
#[derive(Default)]
struct Writer {
    headers: Vec<EditRecordChange>,
    values: Vec<EditRecordChange>,
    fresh: Vec<EditRecordChange>,
}
impl Writer {
    fn len(&self) -> usize {
        self.headers.len() + self.values.len() + self.fresh.len()
    }
    /// One inode row adds at most a header, a value and a fresh rank.
    fn reserve<P: OverlayOperationRecords + ?Sized>(
        &mut self,
        shared: &Shared<'_, P>,
        rows: usize,
    ) -> ContentResult<()> {
        if self.len() + rows > PAGE_ROWS {
            self.flush(shared)?;
        }
        Ok(())
    }
    fn flush<P: OverlayOperationRecords + ?Sized>(
        &mut self,
        shared: &Shared<'_, P>,
    ) -> ContentResult<()> {
        if self.len() == 0 {
            return Ok(());
        }
        let mut changes = Vec::with_capacity(self.len());
        changes.append(&mut self.headers);
        changes.append(&mut self.values);
        changes.append(&mut self.fresh);
        shared.borrow_mut().apply(changes)
    }
}
/// The unsealed context, guarded on absence: a second attempt in this scope
/// is refused by its original deciding record rather than merged.
pub(crate) fn begin<P: OverlayOperationRecords + ?Sized>(
    shared: &Shared<'_, P>,
    facts: Facts,
) -> ContentResult<Vec<u8>> {
    let initial = context(facts, Totals::default(), false);
    shared
        .borrow_mut()
        .apply(vec![change(CONTEXT, 0, initial.clone())])?;
    Ok(initial)
}
/// Replaces exactly the unsealed context with the declared totals.
pub(crate) fn seal<P: OverlayOperationRecords + ?Sized>(
    shared: &Shared<'_, P>,
    facts: Facts,
    totals: Totals,
    initial: Vec<u8>,
) -> ContentResult<Vec<u8>> {
    let sealed = context(facts, totals, true);
    shared.borrow_mut().apply(vec![EditRecordChange {
        key: key(CONTEXT, 0),
        expected: EditRecordExpected::ExactBytes(initial),
        value: Some(sealed.clone()),
    }])?;
    Ok(sealed)
}
/// One original captured page: its failure is retained, its window is checked.
fn page<P: OverlayOperationRecords + ?Sized, T>(
    shared: &Shared<'_, P>,
    original: WorkspaceResult<Vec<T>>,
) -> ContentResult<Vec<T>> {
    let page = original.map_err(|original| shared.borrow_mut().original(original))?;
    let mut owner = shared.borrow_mut();
    if page.len() > PAGE_ROWS {
        return Err(owner.content(ContentError::InvalidRecord("captured namespace page shape")));
    }
    owner.work.largest_page = owner.work.largest_page.max(page.len() as u64);
    Ok(page)
}
/// One pass over every captured name: each parent's exact change count becomes
/// its header unless the parent itself was created and removed in the capture.
pub(crate) fn names<P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized>(
    provider: &P,
    shared: &Shared<'_, P>,
    facts: Facts,
) -> ContentResult<u64> {
    let mut writer = Writer::default();
    let mut headers = 0_u64;
    let mut after: Option<(u64, Vec<u8>)> = None;
    let mut open: Option<(u64, u64)> = None;
    loop {
        shared.borrow().ready()?;
        add(&mut shared.borrow_mut().work.entry_pages, 1);
        let rows = page(
            shared,
            provider.captured_directory_entry_page(facts.reader, after.clone()),
        )?;
        // A short page ends a sealed sequence: no empty page job follows it.
        let last = rows.len() < PAGE_ROWS;
        for DirectoryEntry {
            parent,
            name,
            serial: _,
            inherited: _,
        } in rows
        {
            let ordered = match &after {
                None => parent != 0,
                Some((last, previous)) => (*last, previous.as_slice()) < (parent, name.as_slice()),
            };
            if !ordered {
                return Err(ContentError::InvalidRecord("captured name page order"));
            }
            // A name Content cannot represent ends the attempt with its error.
            PathName::from_bytes(&name)?;
            add(&mut shared.borrow_mut().work.entry_rows, 1);
            open = match open {
                Some((current, changes)) if current == parent => Some((
                    current,
                    changes.checked_add(1).ok_or(ContentError::LengthOverflow)?,
                )),
                Some((current, changes)) => {
                    headers += header(provider, shared, facts, &mut writer, current, changes)?;
                    Some((parent, 1))
                }
                None => Some((parent, 1)),
            };
            after = Some((parent, name));
        }
        if last {
            break;
        }
        // Every header this page finished is applied before the next page is
        // requested: nothing pending crosses a page boundary.
        writer.flush(shared)?;
    }
    if let Some((current, changes)) = open {
        headers += header(provider, shared, facts, &mut writer, current, changes)?;
    }
    writer.flush(shared)?;
    Ok(headers)
}
/// One captured inode point decides the finished parent. Only a parent created
/// above the floor and left with no link is dropped; a removed base directory
/// keeps its whiteouts, and a parent with no captured row is Content's to class.
fn header<P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized>(
    provider: &P,
    shared: &Shared<'_, P>,
    facts: Facts,
    writer: &mut Writer,
    parent: u64,
    changes: u64,
) -> ContentResult<u64> {
    shared.borrow().ready()?;
    add(&mut shared.borrow_mut().work.parent_points, 1);
    let inode = provider
        .captured_inode(facts.reader, parent)
        .map_err(|original| shared.borrow_mut().original(original))?;
    if inode.as_ref().is_some_and(|inode| inode.serial != parent) {
        return Err(ContentError::InvalidRecord("captured inode point"));
    }
    let tombstone = parent != facts.root_serial
        && inode
            .as_ref()
            .is_some_and(|inode| facts.reader.created_above(inode.born) && inode.nlink == 0);
    if tombstone {
        add(&mut shared.borrow_mut().work.headers_dropped, 1);
        return Ok(0);
    }
    writer.reserve(shared, 1)?;
    writer.headers.push(change(HEADER, parent, number(changes)));
    add(&mut shared.borrow_mut().work.headers_written, 1);
    Ok(1)
}
/// Whether the immutable base must hold this row's inode.
fn stored(facts: Facts, inode: &Inode) -> bool {
    inode.serial == facts.root_serial
        || (inode.nlink != 0 && !facts.reader.created_above(inode.born))
}
/// One pass over every captured inode in serial order. A page's base records
/// are one grouped demand; one file is under construction at a time.
pub(crate) fn values<P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized>(
    builder: &Builder<'_, '_, P>,
    consumer: &mut dyn FinalizedConsumer,
    timing: &TimingScope<'_, Active>,
    headers: u64,
) -> ContentResult<Totals> {
    let (provider, shared, facts) = (builder.provider, builder.shared, builder.facts);
    let client = builder.base.client();
    let mut base = FilesystemRead::new(client.as_ref(), builder.base.identity())?;
    let mut totals = Totals {
        headers,
        ..Totals::default()
    };
    let mut writer = Writer::default();
    let mut after = 0_u64;
    loop {
        shared.borrow().ready()?;
        add(&mut shared.borrow_mut().work.inode_pages, 1);
        let rows = page(shared, provider.captured_inode_page(facts.reader, after))?;
        let last = rows.len() < PAGE_ROWS;
        let mut demanded = Vec::with_capacity(rows.len());
        for inode in &rows {
            if inode.serial <= after {
                return Err(ContentError::InvalidRecord("captured inode page order"));
            }
            after = inode.serial;
            if stored(facts, inode) {
                demanded.push(inode.serial);
            }
        }
        let records = if demanded.is_empty() {
            Vec::new()
        } else {
            add(&mut shared.borrow_mut().work.base_lookups, 1);
            base.lookup_inodes(&demanded)?
        };
        let mut bases = demanded.iter().copied().zip(records);
        for inode in &rows {
            add(&mut shared.borrow_mut().work.inode_rows, 1);
            let root = inode.serial == facts.root_serial;
            if !root && inode.nlink == 0 {
                // Unlinked while held, removed, or created and removed: absent
                // from the result, so nothing is declared and nothing is read.
                add(&mut shared.borrow_mut().work.tombstones_skipped, 1);
                continue;
            }
            let fresh = !root && facts.reader.created_above(inode.born);
            let record = if fresh {
                None
            } else {
                match bases.next() {
                    Some((serial, record)) if serial == inode.serial => record,
                    _ => return Err(ContentError::InvalidRecord("captured base inode demand")),
                }
            };
            let value = builder.value(inode, record, fresh, consumer, timing)?;
            writer.reserve(shared, 3)?;
            if fresh {
                // A fresh directory nobody named still needs its empty page.
                if inode.kind == InodeKind::Directory
                    && !shared.borrow_mut().contains(key(HEADER, inode.serial))?
                {
                    writer.headers.push(change(HEADER, inode.serial, number(0)));
                    totals.headers += 1;
                    add(&mut shared.borrow_mut().work.headers_written, 1);
                }
                writer
                    .fresh
                    .push(change(FRESH, inode.serial, number(totals.fresh)));
                totals.fresh += 1;
                add(&mut shared.borrow_mut().work.fresh_serials, 1);
            }
            writer.values.push(change(
                VALUE,
                inode.serial,
                encode_inode_value(value).to_vec(),
            ));
            totals.values += 1;
            add(&mut shared.borrow_mut().work.values_written, 1);
        }
        // This page's values and ranks are applied before the next page is
        // requested: one captured page and its records are resident at a time.
        writer.flush(shared)?;
        if last {
            break;
        }
    }
    Ok(totals)
}
