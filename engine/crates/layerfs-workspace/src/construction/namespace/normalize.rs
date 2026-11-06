//! Two captured passes that seal headers, typed values, fresh ranks and totals.
use super::{
    inodes::Builder,
    records::{
        add, change, context, key, number, Facts, Shared, Totals, CONTEXT, FRESH, HEADER, VALUE,
    },
};
use crate::construction::outcome::nanos;
use crate::{
    BaseView, FileDigestRow, OverlayCapturedNamespace, OverlayOperationRecords, WorkspaceResult,
    FILE_DIGEST_WINDOW,
};
use layerfs_content::{
    filesystem::{FilesystemRead, PathName},
    object::inode_leaf::{encode_inode_value, InodeKind as CanonicalKind},
    ContentError, ContentResult, EditRecordChange, EditRecordExpected, FinalizedConsumer, ObjectId,
};
use layerfs_overlay::{DirectoryEntry, Inode, InodeKind, PAGE_ROWS, RECORD_ROWS};
use layerfs_telemetry::timer::{Active, TimingScope};
use std::time::Instant;

/// The changes of one guarded job: at most RECORD_ROWS of them, flushed strictly
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
        if self.len() + rows > RECORD_ROWS {
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
/// Most same-path predecessors one attempt remembers: 16 bytes each, so at most
/// 1 MiB. A name past the bound is not looked up and offers its file no base:
/// the hint is advisory, and the bound keeps it from growing with the capture.
const SAME_PATH_HINTS: usize = 65_536;
/// The regular file the base tree holds at the path a fresh file now takes
/// (#88). `rm f` then writing `f`, as `git checkout` does, replaces a file
/// with no rename, so no predecessor is recorded; the file that held its name
/// is the closest earlier version there is. Pairs of (serial, base serial),
/// sorted by serial, the first name of a serial kept.
pub(crate) struct SamePath(Vec<(u64, u64)>);
impl SamePath {
    fn of(&self, serial: u64) -> Option<u64> {
        self.0
            .binary_search_by_key(&serial, |(fresh, _)| *fresh)
            .ok()
            .map(|at| self.0[at].1)
    }
}
/// The base serial of the regular file `parent` binds as `name` in the base,
/// for a name the capture binds to `serial` over a binding the lower view
/// held. A name the base does not bind, under a parent it does not hold as a
/// directory, or bound to another kind or to `serial` itself offers nothing;
/// any other base failure is the attempt's own.
fn same_path<P: OverlayOperationRecords + ?Sized>(
    shared: &Shared<'_, P>,
    base: &BaseView,
    parent: u64,
    name: &PathName,
    serial: u64,
) -> ContentResult<Option<u64>> {
    add(&mut shared.borrow_mut().work.same_path_lookups, 1);
    match base.child(parent, name) {
        Ok(found) if found.value.kind == CanonicalKind::RegularFile && found.serial != serial => {
            Ok(Some(found.serial))
        }
        Ok(_) | Err(ContentError::PathNotFound) | Err(ContentError::WrongLogicalRole) => Ok(None),
        Err(original) => Err(shared.borrow_mut().content(original)),
    }
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
/// A name bound over one the lower view held also asks the base which file
/// held it, for the values pass to offer as a delta base (`SamePath`).
pub(crate) fn names<P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized>(
    provider: &P,
    shared: &Shared<'_, P>,
    facts: Facts,
    base: &BaseView,
) -> ContentResult<(u64, SamePath)> {
    let mut writer = Writer::default();
    let mut headers = 0_u64;
    let mut hints = Vec::new();
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
            serial,
            inherited,
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
            let path = PathName::from_bytes(&name)?;
            add(&mut shared.borrow_mut().work.entry_rows, 1);
            if let (Some(serial), true) = (serial, inherited && hints.len() < SAME_PATH_HINTS) {
                if let Some(held) = same_path(shared, base, parent, &path, serial)? {
                    hints.push((serial, held));
                }
            }
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
    // Stable: a serial bound at two such names keeps the first, in name order.
    hints.sort_by_key(|(serial, _)| *serial);
    hints.dedup_by_key(|(serial, _)| *serial);
    Ok((headers, SamePath(hints)))
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
/// Records the pending fresh files' digests in the Store, one window.
fn record_digests<P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized>(
    builder: &Builder<'_, '_, P>,
    pending: &mut Vec<FileDigestRow>,
) -> ContentResult<()> {
    if let (Some(digests), false) = (builder.digests, pending.is_empty()) {
        digests
            .record_file_digests(pending)
            .map_err(|original| builder.shared.borrow_mut().original(original))?;
        add(
            &mut builder.shared.borrow_mut().work.digests_recorded,
            pending.len() as u64,
        );
    }
    pending.clear();
    Ok(())
}
/// One pass over every captured inode in serial order. A page's base records
/// are one grouped demand; one file is under construction at a time. A fresh
/// file is offered the predecessor a rename recorded, else the file the base
/// held at its path (`same`).
pub(crate) fn values<P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized>(
    builder: &Builder<'_, '_, P>,
    consumer: &mut dyn FinalizedConsumer,
    timing: &TimingScope<'_, Active>,
    headers: u64,
    same: &SamePath,
) -> ContentResult<Totals> {
    let (provider, shared, facts) = (builder.provider, builder.shared, builder.facts);
    let client = builder.base.client();
    let mut base = FilesystemRead::new(client.as_ref(), builder.base.identity())?;
    let mut totals = Totals {
        headers,
        ..Totals::default()
    };
    let mut writer = Writer::default();
    let mut pending = Vec::with_capacity(FILE_DIGEST_WINDOW);
    let mut after = 0_u64;
    loop {
        shared.borrow().ready()?;
        add(&mut shared.borrow_mut().work.inode_pages, 1);
        let started = Instant::now();
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
        // The overlay's digests of this page's fresh files, one demand.
        let followed: Vec<u64> = match builder.digests {
            None => Vec::new(),
            Some(_) => rows
                .iter()
                .filter(|inode| {
                    inode.kind == InodeKind::File
                        && inode.nlink != 0
                        && inode.serial != facts.root_serial
                        && facts.reader.created_above(inode.born)
                })
                .map(|inode| inode.serial)
                .collect(),
        };
        let digests = if followed.is_empty() {
            Vec::new()
        } else {
            provider
                .captured_digests(facts.reader, &followed)
                .map_err(|original| shared.borrow_mut().original(original))?
        };
        if digests.len() != followed.len() {
            return Err(ContentError::InvalidRecord("captured digest page"));
        }
        // The files a rename may have put in place of others: their
        // predecessors are offered as delta bases (#352 F-063), a small
        // file's whole (#88) as a chunked file's chunks.
        let replacing: Vec<u64> = rows
            .iter()
            .filter(|inode| {
                inode.kind == InodeKind::File
                    && inode.nlink != 0
                    && inode.serial != facts.root_serial
                    && facts.reader.created_above(inode.born)
            })
            .map(|inode| inode.serial)
            .collect();
        let predecessors = if replacing.is_empty() {
            Vec::new()
        } else {
            provider
                .captured_predecessors(facts.reader, &replacing)
                .map_err(|original| shared.borrow_mut().original(original))?
        };
        if predecessors.len() != replacing.len() {
            return Err(ContentError::InvalidRecord("captured predecessor page"));
        }
        let mut predecessors = replacing.into_iter().zip(predecessors).peekable();
        // The saved roots that already hold those bytes, one Store demand
        // per window.
        let contents: Vec<([u8; 32], u64)> = digests
            .iter()
            .flatten()
            .map(|captured| (captured.digest, captured.length))
            .collect();
        let mut saved = Vec::with_capacity(contents.len());
        if let Some(store) = builder.digests {
            for window in contents.chunks(FILE_DIGEST_WINDOW) {
                let roots = store
                    .file_roots(window)
                    .map_err(|original| shared.borrow_mut().original(original))?;
                if roots.len() != window.len() {
                    return Err(ContentError::InvalidRecord("saved file root page"));
                }
                saved.extend(roots);
            }
        }
        let mut saved = saved.into_iter();
        let mut digests = followed.into_iter().zip(digests).peekable();
        add(&mut shared.borrow_mut().work.inode_page_ns, nanos(started));
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
            if inode.kind == InodeKind::Directory {
                let mut owner = shared.borrow_mut();
                if owner.directories.len() < crate::base::REMEMBERED_DIRECTORIES {
                    owner
                        .directories
                        .push((inode.serial, inode.entries, inode.subdirs));
                }
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
            let captured = digests
                .next_if(|(serial, _)| *serial == inode.serial)
                .and_then(|(_, captured)| captured);
            let reuse = match captured {
                Some(_) => saved
                    .next()
                    .ok_or(ContentError::InvalidRecord("saved file root page"))?
                    .map(|root| ObjectId::from_bytes(&root))
                    .transpose()?,
                None => None,
            };
            let predecessor = predecessors
                .next_if(|(serial, _)| *serial == inode.serial)
                .and_then(|(_, predecessor)| predecessor)
                .or_else(|| fresh.then(|| same.of(inode.serial)).flatten());
            if predecessor.is_some() && inode.kind == InodeKind::File && reuse.is_none() {
                add(&mut shared.borrow_mut().work.predecessors_offered, 1);
            }
            let value =
                builder.value(inode, record, fresh, reuse, predecessor, consumer, timing)?;
            // A root built from all of this file's bytes is recorded.
            if let (Some(captured), None) = (captured, reuse) {
                pending.push(FileDigestRow {
                    root: value.content_root.to_bytes(),
                    length: captured.length,
                    digest: captured.digest,
                });
                if pending.len() == FILE_DIGEST_WINDOW {
                    record_digests(builder, &mut pending)?;
                }
            }
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
    record_digests(builder, &mut pending)?;
    Ok(totals)
}
