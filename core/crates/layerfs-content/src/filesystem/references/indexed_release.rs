//! Indexed FIFO and namespace-depth frames with guarded cursor acknowledgement.
use super::record::Row;
use super::release::ReleaseWork;
use super::{indexed::IndexedReducer, indexed_rows::RowCursor, indexed_wire as wire, meaning};
use crate::filesystem::directory::read::{list_after, DirectoryReadWork};
use crate::filesystem::inode::read::{lookup_many, InodeReadWork, InodeTable};
use crate::filesystem::objects::FilesystemObjects;
use crate::filesystem::sorted::finish::DirectoryRoot;
use crate::object::inode_leaf::{InodeKind, InodeValue};
use crate::{ConstructionRecordChange, ContentError, ContentResult};

struct Frontier {
    tail: u64,
    head: u64,
    depth: u64,
    work: ReleaseWork,
}

pub(super) fn release(
    objects: &FilesystemObjects<'_>,
    table: InodeTable,
    reducer: &mut IndexedReducer<'_, '_>,
    batch: usize,
    root: u64,
) -> ContentResult<(ReleaseWork, u64)> {
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
        for key in &keys {
            wire::serial(*key)?;
        }
        if !keys.is_empty() {
            for key in keys {
                frontier.transfer(objects, reducer, wire::serial(key)?)?;
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
    loop {
        let mut wave = Vec::with_capacity(batch.clamp(1, 64));
        while wave.len() < batch.clamp(1, 64) {
            match cursor.next(reducer)? {
                Some(row) => wave.push(row),
                None => break,
            }
        }
        if wave.is_empty() {
            break;
        }
        let serials = wave
            .iter()
            .filter(|row| matches!(row, Row::Effect { .. }))
            .map(|row| row.serial())
            .collect::<Vec<_>>();
        let bases = lookup_many(
            objects.reader(),
            table,
            &serials,
            &mut InodeReadWork::default(),
        )?;
        reads = reads.saturating_add(serials.len() as u64);
        let mut bases = bases.into_iter();
        for row in wave {
            let base = if matches!(row, Row::Effect { .. }) {
                bases.next().flatten()
            } else {
                None
            };
            let count = meaning::count(row, base)?;
            if count == 0 && row.serial() != root {
                let next = frontier
                    .tail
                    .checked_add(1)
                    .ok_or(ContentError::LengthOverflow)?;
                reducer.state.apply(vec![
                    wire::change(
                        wire::WORK,
                        next,
                        None,
                        Some(
                            wire::Item {
                                serial: row.serial(),
                                base,
                            }
                            .encode(),
                        ),
                    ),
                    wire::change(wire::NODE, row.serial(), None, Some(vec![1, 0])),
                ])?;
                frontier.tail = next;
            }
            reducer.work.serials_scanned = reducer.work.serials_scanned.saturating_add(1);
        }
    }
    Ok(reads)
}

impl Frontier {
    fn transfer(
        &mut self,
        objects: &FilesystemObjects<'_>,
        reducer: &mut IndexedReducer<'_, '_>,
        ordinal: u64,
    ) -> ContentResult<()> {
        if ordinal
            != self
                .head
                .checked_add(1)
                .ok_or(ContentError::LengthOverflow)?
            || ordinal > self.tail
        {
            return Err(ContentError::InvalidRecord("filesystem release FIFO"));
        }
        let old = reducer
            .state
            .read(wire::WORK, ordinal)?
            .ok_or(ContentError::InvalidRecord(
                "filesystem release work missing",
            ))?;
        let item = wire::Item::decode(&old)?;
        let row = reducer.required(item.serial)?;
        let merged = meaning::with_base(row, item.base)?;
        let value = merged
            .value()
            .ok_or(ContentError::InvalidRecord("released new inode"))?;
        let node =
            reducer
                .state
                .read(wire::NODE, item.serial)?
                .ok_or(ContentError::InvalidRecord(
                    "filesystem release node missing",
                ))?;
        if node != [1, 0] {
            return Err(ContentError::InvalidRecord("filesystem release node state"));
        }
        let mut changes = vec![wire::change(wire::WORK, ordinal, Some(old), None)];
        let mut depth = self.depth;
        if value.kind == InodeKind::Directory {
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
            changes.push(wire::change(
                wire::NODE,
                item.serial,
                Some(node),
                Some(vec![1, 1]),
            ));
        } else {
            changes.push(wire::change(
                wire::NODE,
                item.serial,
                Some(node),
                Some(vec![1, 2]),
            ));
        }
        changes.sort_by_key(|change| change.key);
        reducer.state.apply(changes)?;
        self.depth = depth;
        self.head = ordinal;
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
            objects.reader()
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
            objects.reader(),
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
