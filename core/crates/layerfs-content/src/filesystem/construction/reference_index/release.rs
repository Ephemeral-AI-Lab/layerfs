//! Indexed FIFO and namespace-depth frames with guarded cursor acknowledgement.
use super::reduce::{IndexedReducer, RELEASE_ROWS};
use super::{super::reference::meaning, rows::RowCursor, wire};
use crate::filesystem::construction::reference::record::Row;
use crate::filesystem::construction::reference::release::ReleaseWork;
use crate::filesystem::objects::FilesystemObjects;
use crate::filesystem::tree::directory::read::{list_after, DirectoryReadWork};
use crate::filesystem::tree::inode::read::{lookup_many, InodeReadWork, InodeTable};
use crate::filesystem::tree::pages::finish::DirectoryRoot;
use crate::object::inode_leaf::{InodeKind, InodeValue};
use crate::{ConstructionRecordChange, ContentError, ContentResult};

/// Zero rows of a seed wave one job queues: a work item and a node each,
/// inside the 64 changes of one guarded job.
const QUEUE_ROWS: usize = 32;
/// Most seed rows sharing one base-inode lookup. Each level needs at most one
/// page per serial, so at most 256 canonical pages are read at a time.
const SEED_READ_ROWS: usize = 256;

struct Frontier {
    tail: u64,
    head: u64,
    depth: u64,
    work: ReleaseWork,
}

pub(crate) fn release(
    objects: &FilesystemObjects<'_>,
    table: InodeTable,
    reducer: &mut IndexedReducer<'_, '_>,
    batch: usize,
    root: u64,
) -> ContentResult<(ReleaseWork, u64)> {
    // The seed pass reads stored rows: every held meaning is written first.
    reducer.flush()?;
    let mut frontier = Frontier {
        tail: 0,
        head: 0,
        depth: 0,
        work: ReleaseWork::default(),
    };
    let reads = seed(objects, table, reducer, batch, root, &mut frontier)?;
    // Membership was sealed for the complete seed pass. New rows, including
    // lower serials, are now allowed; the later final pass starts from None.
    loop {
        let keys = reducer.state.first_keys(wire::WORK)?;
        let ordinals = keys
            .into_iter()
            .map(wire::serial)
            .collect::<ContentResult<Vec<_>>>()?;
        if !ordinals.is_empty() {
            for ordinals in ordinals.chunks(RELEASE_ROWS) {
                frontier.transfer(objects, reducer, ordinals)?;
            }
            continue;
        }
        if frontier.head != frontier.tail {
            return Err(ContentError::InvalidRecord(
                "filesystem release work missing",
            ));
        }
        if frontier.depth == 0 {
            break;
        }
        frontier.page(objects, table, reducer)?;
    }
    Ok((frontier.work, reads))
}

fn seed(
    objects: &FilesystemObjects<'_>,
    table: InodeTable,
    reducer: &mut IndexedReducer<'_, '_>,
    batch: usize,
    root: u64,
    frontier: &mut Frontier,
) -> ContentResult<u64> {
    let mut cursor = RowCursor::new();
    let mut reads = 0u64;
    let batch = batch.clamp(1, 64);
    loop {
        let rows = cursor.window(reducer, batch, SEED_READ_ROWS)?;
        if rows.is_empty() {
            break;
        }
        let serials = rows
            .iter()
            .filter(|row| matches!(row, Row::Effect { .. }))
            .map(|row| row.serial())
            .collect::<Vec<_>>();
        let bases = lookup_many(
            objects.base_reader(),
            table,
            &serials,
            &mut InodeReadWork::default(),
        )?;
        reads = reads.saturating_add(serials.len() as u64);
        let mut bases = bases.into_iter();
        for wave in rows.chunks(batch) {
            // Keep the guarded jobs and FIFO order of each backing wave.
            let mut queued = Vec::new();
            let mut tail = frontier.tail;
            for &row in wave {
                let base = if matches!(row, Row::Effect { .. }) {
                    bases.next().flatten()
                } else {
                    None
                };
                let count = meaning::count(row, base)?;
                if count == 0 && row.serial() != root {
                    if queued.len() == 2 * QUEUE_ROWS {
                        frontier.queue(reducer, &mut queued, tail)?;
                    }
                    tail = tail.checked_add(1).ok_or(ContentError::LengthOverflow)?;
                    let item = wire::Item {
                        serial: row.serial(),
                        base,
                    };
                    queued.push(wire::change(wire::WORK, tail, None, Some(item.encode())));
                    queued.push(wire::change(
                        wire::NODE,
                        row.serial(),
                        None,
                        Some(vec![1, 0]),
                    ));
                }
                reducer.work.serials_scanned = reducer.work.serials_scanned.saturating_add(1);
            }
            frontier.queue(reducer, &mut queued, tail)?;
        }
    }
    Ok(reads)
}

impl Frontier {
    /// Publishes the queued work items and nodes of a seed wave as one job.
    fn queue(
        &mut self,
        reducer: &IndexedReducer<'_, '_>,
        queued: &mut Vec<ConstructionRecordChange>,
        tail: u64,
    ) -> ContentResult<()> {
        if queued.is_empty() {
            return Ok(());
        }
        let mut changes = std::mem::take(queued);
        changes.sort_by_key(|change| change.key);
        reducer.state.apply(changes)?;
        self.tail = tail;
        Ok(())
    }
    /// Moves at most [`RELEASE_ROWS`] queued items, in FIFO order, to the
    /// released state: their work records are one read job, their rows and
    /// nodes a second, and every deletion, node step and new frame one guarded
    /// job. The first refusal in item order is the one returned, and a
    /// refusal applies nothing of the job.
    fn transfer(
        &mut self,
        objects: &FilesystemObjects<'_>,
        reducer: &mut IndexedReducer<'_, '_>,
        ordinals: &[u64],
    ) -> ContentResult<()> {
        // An item that cannot be read ends the job after the items before it
        // were checked: an earlier item's refusal still comes first.
        let mut later = None;
        let mut head = self.head;
        let mut keys = Vec::with_capacity(ordinals.len());
        for ordinal in ordinals {
            match head.checked_add(1) {
                Some(next) if next == *ordinal && next <= self.tail => {}
                Some(_) => later = Some(ContentError::InvalidRecord("filesystem release FIFO")),
                None => later = Some(ContentError::LengthOverflow),
            }
            if later.is_some() {
                break;
            }
            head = *ordinal;
            keys.push(wire::key(wire::WORK, *ordinal));
        }
        let mut items = Vec::with_capacity(keys.len());
        if !keys.is_empty() {
            for (ordinal, old) in ordinals.iter().zip(reducer.state.read_keys(&keys)?) {
                let item = old
                    .ok_or(ContentError::InvalidRecord(
                        "filesystem release work missing",
                    ))
                    .and_then(|old| Ok((wire::Item::decode(&old)?, old)));
                match item {
                    Ok((item, old)) => items.push((*ordinal, item, Some(old))),
                    Err(error) => {
                        later = Some(error);
                        break;
                    }
                }
            }
        }
        let serials = items
            .iter()
            .map(|(_, item, _)| item.serial)
            .collect::<Vec<_>>();
        let mut changes = Vec::with_capacity(3 * items.len());
        let mut depth = self.depth;
        reducer.read_rows(&serials, Some(wire::NODE), |index, row, node| {
            let (ordinal, item, old) = &mut items[index];
            let merged = meaning::with_base(row?, item.base)?;
            let value = merged
                .value()
                .ok_or(ContentError::InvalidRecord("released new inode"))?;
            let node = node.ok_or(ContentError::InvalidRecord(
                "filesystem release node missing",
            ))?;
            if node != [1, 0] {
                return Err(ContentError::InvalidRecord("filesystem release node state"));
            }
            changes.push(wire::change(wire::WORK, *ordinal, old.take(), None));
            let state = if value.kind == InodeKind::Directory {
                let accepted = item
                    .base
                    .is_none_or(|base| base.content_root != value.content_root);
                if accepted {
                    objects.accepted_reader()?;
                }
                depth = depth.checked_add(1).ok_or(ContentError::LengthOverflow)?;
                let frame = wire::Frame {
                    serial: item.serial,
                    root: value.content_root,
                    accepted,
                    finished: false,
                    after: None,
                };
                changes.push(wire::change(wire::FRAME, depth, None, Some(frame.encode())));
                1
            } else {
                2
            };
            changes.push(wire::change(
                wire::NODE,
                item.serial,
                Some(node),
                Some(vec![1, state]),
            ));
            Ok(())
        })?;
        if let Some(error) = later {
            return Err(error);
        }
        changes.sort_by_key(|change| change.key);
        reducer.state.apply(changes)?;
        self.depth = depth;
        self.head = head;
        self.work.peak_depth = self
            .work
            .peak_depth
            .max(usize::try_from(depth).map_err(|_| ContentError::LengthOverflow)?);
        Ok(())
    }
    fn page(
        &mut self,
        objects: &FilesystemObjects<'_>,
        table: InodeTable,
        reducer: &mut IndexedReducer<'_, '_>,
    ) -> ContentResult<()> {
        let mut old =
            reducer
                .state
                .read(wire::FRAME, self.depth)?
                .ok_or(ContentError::InvalidRecord(
                    "filesystem release frame missing",
                ))?;
        let mut frame = wire::Frame::decode(&old)?;
        if frame.finished {
            let node = reducer.state.read(wire::NODE, frame.serial)?.ok_or(
                ContentError::InvalidRecord("filesystem release node missing"),
            )?;
            if node != [1, 1] {
                return Err(ContentError::InvalidRecord("filesystem release node state"));
            }
            reducer.state.apply(vec![
                wire::change(wire::FRAME, self.depth, Some(old), None),
                wire::change(wire::NODE, frame.serial, Some(node), Some(vec![1, 2])),
            ])?;
            self.depth = self
                .depth
                .checked_sub(1)
                .ok_or(ContentError::LengthOverflow)?;
            return Ok(());
        }
        let reader = if frame.accepted {
            objects.accepted_reader()?
        } else {
            objects.base_reader()
        };
        let page = list_after(
            reader,
            DirectoryRoot(frame.root),
            frame.after.as_ref(),
            64,
            crate::filesystem::limits::MAXIMUM_PAGE_BYTES,
            &mut DirectoryReadWork::default(),
        )?;
        self.work.pages = self.work.pages.saturating_add(1);
        self.work.entries = self.work.entries.saturating_add(page.entries.len() as u64);
        if page.entries.is_empty() {
            if page.continuation.is_some() {
                return Err(ContentError::InvalidRecord(
                    "filesystem release continuation",
                ));
            }
            frame.finished = true;
            reducer.state.apply(vec![wire::change(
                wire::FRAME,
                self.depth,
                Some(old),
                Some(frame.encode()),
            )])?;
            return Ok(());
        }
        let serials = page
            .entries
            .iter()
            .map(|(_, serial)| *serial)
            .collect::<Vec<_>>();
        let bases = lookup_many(
            objects.base_reader(),
            table,
            &serials,
            &mut InodeReadWork::default(),
        )?;
        self.work.base_records = self.work.base_records.saturating_add(serials.len() as u64);
        let last = page.entries.len() - 1;
        for (index, (name, serial)) in page.entries.into_iter().enumerate() {
            if frame.after.as_ref().is_some_and(|after| after >= &name) {
                return Err(ContentError::NonCanonicalOrdering);
            }
            let base = bases[index].ok_or(ContentError::InvalidRecord("released child"))?;
            let transition = reducer.prepare_removed(serial)?;
            let row = meaning::with_base(transition.row, Some(base))?;
            let value = row
                .value()
                .ok_or(ContentError::InvalidRecord("released child"))?;
            let count = meaning::count(transition.row, Some(base))?;
            let before_count = meaning::count(transition.before, Some(base))?;
            frame.after = Some(name);
            frame.finished = index == last && page.continuation.is_none();
            let after = frame.encode();
            let mut extra = vec![wire::change(
                wire::FRAME,
                self.depth,
                Some(old),
                Some(after.clone()),
            )];
            let tail = self.child(
                reducer,
                serial,
                base,
                value.kind,
                (before_count, count),
                &mut extra,
            )?;
            // Row decrement, touched guard, child work publication and exact name
            // progress are one all-guards-before-effects job. No consumed input
            // is deleted first and a refusal is never replayed.
            reducer.apply_transition(transition, extra)?;
            self.tail = tail;
            old = after;
            self.work.released = self.work.released.saturating_add(1);
        }
        self.work.traversed_directories = self.work.traversed_directories.saturating_add(1);
        Ok(())
    }
    fn child(
        &self,
        reducer: &IndexedReducer<'_, '_>,
        serial: u64,
        base: InodeValue,
        kind: InodeKind,
        counts: (u64, u64),
        changes: &mut Vec<ConstructionRecordChange>,
    ) -> ContentResult<u64> {
        if counts.1 != 0 || kind != InodeKind::Directory {
            return Ok(self.tail);
        }
        if let Some(node) = reducer.state.read(wire::NODE, serial)? {
            if node.len() != 2 || node[0] != 1 || node[1] > 2 {
                return Err(ContentError::InvalidRecord("filesystem release node state"));
            }
            // Already transferred/queued means no second traversal of the same
            // directory identity; its count row still records every decrement.
            changes.push(wire::change(
                wire::NODE,
                serial,
                Some(node.clone()),
                Some(node),
            ));
            return Ok(self.tail);
        }
        if counts.0 == 0 {
            return Err(ContentError::InvalidRecord(
                "filesystem release node missing",
            ));
        }
        let tail = self
            .tail
            .checked_add(1)
            .ok_or(ContentError::LengthOverflow)?;
        changes.push(wire::change(
            wire::WORK,
            tail,
            None,
            Some(
                wire::Item {
                    serial,
                    base: Some(base),
                }
                .encode(),
            ),
        ));
        changes.push(wire::change(wire::NODE, serial, None, Some(vec![1, 0])));
        Ok(tail)
    }
}
