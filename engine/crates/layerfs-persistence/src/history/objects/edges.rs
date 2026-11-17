//! The delta edges of one mark wave's payload records, from bounded pack reads
//! (§6.2).
//!
//! A file payload has no logical references, so the mark needs only its
//! record's base and stored width. Both sit in the record's first bytes and in
//! its group's directory, so this reads those ranges and never the payload: a
//! mark costs a few hundred bytes per pack instead of the pack. Compressed
//! groups and the `Ordinary` lane answer `None` and keep the decoding reader.
//!
//! The first read of a wave is one call for the heads of many packs. A pack
//! whose directory and record headers sit in its head, the common shape of a
//! pack of large records, costs no statement of its own; only a pack that
//! needs more is acquired on its own.
use super::{storage, ClosureSource};
use layerfs_content::ObjectId;
use layerfs_history::error::{HistoryError, HistoryResult};
use layerfs_storage::{
    encoding::delta::edge::{
        has_sparse_edges, sparse_directory, sparse_edge, RecordDirectory, Sparse,
    },
    encoding::delta::read::ChainEdge,
    location::{ObjectLocation, PackInfo},
    pack::layout::{self, GroupView, PackHeader},
    port::{
        AcquiredPackRead, PackRange, PackReadChoice, PackReadPlan, PersistenceError,
        PACK_READ_PREFIX_BYTES,
    },
    source::Source,
    StorageError,
};
use std::collections::BTreeMap;

/// After the head: a wider directory, then record headers. No pack needs more.
const ROUNDS: usize = 4;
/// Bytes of each pack the first read asks for: the directory of a pack of up
/// to about sixty groups, its first group's record directory and first record
/// header. Small on purpose: on a cold cache each further page of a pack is
/// one more random read. A pack with a wider directory is read again.
const HEAD_BYTES: usize = 1024;
/// Packs whose heads one call reads.
const HEAD_PACKS: usize = 256;
/// Ranges one acquisition selects; far below the port's own limits.
const BATCH_RANGES: usize = 1024;
/// Bytes one acquisition selects.
const BATCH_BYTES: usize = 512 * 1024;

/// The pack bytes held so far, by pack offset.
///
/// Pieces never overlap or touch: `add` joins a new piece to its neighbours.
/// So the one piece that can hold a range is the last one starting at or
/// before it, and a lookup is one ordered-map step, whatever the pack holds.
#[derive(Default)]
struct Pieces(BTreeMap<usize, Vec<u8>>);

impl Pieces {
    fn held(&self, offset: usize, length: usize) -> Option<&[u8]> {
        let (at, bytes) = self.0.range(..=offset).next_back()?;
        let from = offset - at;
        bytes.get(from..from.checked_add(length)?)
    }

    /// Adds the bytes at `offset`. A pack is immutable, so where a new piece
    /// overlaps a held one both carry the same bytes and either copy serves.
    fn add(&mut self, offset: usize, bytes: Vec<u8>) {
        let (mut start, mut joined) = (offset, bytes);
        let before = self
            .0
            .range(..=start)
            .next_back()
            .map(|(at, held)| (*at, held.len()))
            .filter(|(at, length)| at + length >= start);
        if let Some((at, length)) = before {
            let mut held = self.0.remove(&at).unwrap_or_default();
            held.extend_from_slice(joined.get(at + length - start..).unwrap_or(&[]));
            (start, joined) = (at, held);
        }
        while let Some(at) = self
            .0
            .range(start..=start + joined.len())
            .next()
            .map(|(at, _)| *at)
        {
            let held = self.0.remove(&at).unwrap_or_default();
            joined.extend_from_slice(held.get(start + joined.len() - at..).unwrap_or(&[]));
        }
        self.0.insert(start, joined);
    }
}

/// One record's state after a round.
#[derive(Clone, Copy)]
enum Answer {
    Edge(ChainEdge),
    Encoded,
    Pending,
}

/// Sorts `needs` and joins the ranges that overlap or touch.
fn merged(mut needs: Vec<PackRange>) -> Vec<PackRange> {
    needs.sort_unstable_by_key(|range| (range.offset, range.length));
    let mut merged: Vec<PackRange> = Vec::with_capacity(needs.len());
    for range in needs {
        match merged.last_mut() {
            Some(last) if range.offset <= last.offset + last.length => {
                last.length = last.length.max(range.offset + range.length - last.offset);
            }
            _ => merged.push(range),
        }
    }
    merged
}

/// Answers every item the held bytes can, and lists the ranges still needed,
/// sorted and disjoint.
///
/// `items` are in locator order, so a group's records are adjacent and its
/// directory is parsed once for all of them, not once per record.
fn resolve(
    header: PackHeader,
    views: &[GroupView],
    items: &[ObjectLocation],
    held: &Pieces,
    answers: &mut [Answer],
) -> Result<Vec<PackRange>, StorageError> {
    let mut needs: Vec<PackRange> = Vec::new();
    let mut group: Option<(usize, Option<RecordDirectory>)> = None;
    for (item, answer) in items.iter().zip(answers.iter_mut()) {
        if !matches!(answer, Answer::Pending) {
            continue;
        }
        let view = *views
            .get(item.group_number)
            .ok_or(StorageError::Integrity("group ordinal"))?;
        if !has_sparse_edges(header.lane, view) {
            *answer = Answer::Encoded;
            continue;
        }
        if group.as_ref().map(|(number, _)| *number) != Some(item.group_number) {
            let directory =
                match sparse_directory(view, |offset, length| held.held(offset, length))? {
                    Sparse::Ready(directory) => Some(directory),
                    Sparse::Need(range) => {
                        needs.push(range);
                        None
                    }
                };
            group = Some((item.group_number, directory));
        }
        let Some((_, Some(directory))) = &group else {
            continue;
        };
        match sparse_edge(header.lane, view, directory, item, |offset, length| {
            held.held(offset, length)
        })? {
            Sparse::Ready(edge) => *answer = Answer::Edge(edge),
            Sparse::Need(range) => needs.push(range),
        }
    }
    Ok(merged(needs))
}

/// One acquisition's ranges, chosen before it starts.
struct Ranges {
    pack_id: i64,
    ranges: Option<Vec<PackRange>>,
}

impl PackReadPlan for Ranges {
    fn select(
        &mut self,
        info: PackInfo,
        _prefix: &[u8],
    ) -> Result<PackReadChoice, PersistenceError> {
        if info.pack_id != self.pack_id {
            return Err(PersistenceError::Malformed);
        }
        self.ranges
            .take()
            .map(PackReadChoice::Ranges)
            .ok_or(PersistenceError::Malformed)
    }
}

/// One acquisition of `ranges`, counted in the mark's bytes.
fn acquire(
    source: &ClosureSource<'_>,
    pack_id: i64,
    ranges: Vec<PackRange>,
) -> HistoryResult<Vec<(PackRange, Vec<u8>)>> {
    let mut plan = Ranges {
        pack_id,
        ranges: Some(ranges),
    };
    let read = source
        .mechanics
        .read_scoped_pack(pack_id, &mut plan)
        .map_err(StorageError::from)
        .map_err(storage)?;
    let AcquiredPackRead::Units(units) = read else {
        return Err(HistoryError::Integrity("mark edge acquisition"));
    };
    source.note_scoped_body(
        pack_id,
        u64::try_from(units.acquired_bytes()).unwrap_or(u64::MAX),
        u64::try_from(units.info().length).unwrap_or(u64::MAX),
    );
    Ok(units.into_parts().2)
}

/// Acquires `needs`, sorted and disjoint ranges of pack `pack_id`, in bounded
/// batches, and adds them to `held`.
fn acquire_all(
    source: &ClosureSource<'_>,
    pack_id: i64,
    needs: Vec<PackRange>,
    held: &mut Pieces,
) -> HistoryResult<()> {
    let mut acquired = Vec::new();
    let mut flush = |batch: &mut Vec<PackRange>| -> HistoryResult<()> {
        if batch.is_empty() {
            return Ok(());
        }
        acquired.extend(acquire(source, pack_id, std::mem::take(batch))?);
        Ok(())
    };
    let mut batch = Vec::new();
    let mut bytes = 0;
    for range in needs {
        if batch.len() == BATCH_RANGES || bytes + range.length > BATCH_BYTES {
            flush(&mut batch)?;
            bytes = 0;
        }
        bytes += range.length;
        batch.push(range);
    }
    flush(&mut batch)?;
    for (range, bytes) in acquired {
        held.add(range.offset, bytes);
    }
    Ok(())
}

/// The record directories of `groups`, uncompressed groups of pack `pack_id`
/// in pack order, from bounded reads: each record's stored extent without the
/// records themselves.
pub(super) fn group_directories(
    source: &ClosureSource<'_>,
    pack_id: i64,
    groups: &[GroupView],
) -> HistoryResult<Vec<RecordDirectory>> {
    let mut held = Pieces::default();
    // The head, then a directory wider than the head: no group needs more.
    for _ in 0..3 {
        let mut needs = Vec::new();
        let mut directories = Vec::with_capacity(groups.len());
        for view in groups {
            match sparse_directory(*view, |offset, length| held.held(offset, length))
                .map_err(storage)?
            {
                Sparse::Ready(directory) => directories.push(directory),
                Sparse::Need(range) => needs.push(range),
            }
        }
        if needs.is_empty() {
            return Ok(directories);
        }
        acquire_all(source, pack_id, merged(needs), &mut held)?;
    }
    Err(HistoryError::Integrity("mark group directory"))
}

/// The edges of `items`, all records of pack `pack_id`, given the pack's
/// declared length and head: `None` where the record's group is encoded and
/// must take the decoding reader.
fn pack_edges(
    source: &ClosureSource<'_>,
    pack_id: i64,
    items: &[ObjectLocation],
    length: usize,
    head: Vec<u8>,
) -> HistoryResult<Vec<Option<ChainEdge>>> {
    let control = layout::parse_control_header(&head, length).map_err(storage)?;
    let head = if head.len() < control.body_offset {
        source
            .mechanics
            .pack_prefix(pack_id, PACK_READ_PREFIX_BYTES)
            .map_err(StorageError::from)
            .map_err(storage)?
            .filter(|(wide, _)| *wide == length)
            .ok_or(HistoryError::Integrity("mark pack directory"))?
            .1
    } else {
        head
    };
    let header = layout::parse_directory_header(&head, length).map_err(storage)?;
    let views =
        layout::directory_group_views(&head[..header.body_offset], header).map_err(storage)?;
    // The head's bytes past the directory are body bytes this mark read.
    source.note_scoped_body(
        pack_id,
        u64::try_from(head.len() - header.body_offset).unwrap_or(u64::MAX),
        u64::try_from(length).unwrap_or(u64::MAX),
    );
    let mut held = Pieces::default();
    held.add(0, head);
    let mut answers = vec![Answer::Pending; items.len()];
    for _ in 0..=ROUNDS {
        let needs = resolve(header, &views, items, &held, &mut answers).map_err(storage)?;
        if needs.is_empty() {
            source.keep_layout(pack_id, views);
            return answers
                .into_iter()
                .map(|answer| match answer {
                    Answer::Edge(edge) => Ok(Some(edge)),
                    Answer::Encoded => Ok(None),
                    Answer::Pending => Err(HistoryError::Integrity("mark edge read")),
                })
                .collect();
        }
        acquire_all(source, pack_id, needs, &mut held)?;
    }
    Err(HistoryError::Integrity("mark edge read"))
}

/// The header-read edges of one wave: every object of `wave` whose role names
/// no other object and whose record sits in an uncompressed payload group.
///
/// `wave` is in locator order, so each pack's records are adjacent. An object
/// absent from the result takes the decoding reader, exactly as before.
pub(super) fn wave_edges(
    source: &ClosureSource<'_>,
    wave: &[ObjectId],
) -> HistoryResult<BTreeMap<ObjectId, ChainEdge>> {
    let mut packs: Vec<(i64, Vec<ObjectLocation>)> = Vec::new();
    for id in wave {
        let Some(location) = source.location(*id, i64::MAX).map_err(storage)? else {
            continue;
        };
        if location.role.has_references() {
            continue;
        }
        match packs.last_mut() {
            Some((pack_id, items)) if *pack_id == location.pack_id => items.push(location),
            _ => packs.push((location.pack_id, vec![location])),
        }
    }
    let mut edges = BTreeMap::new();
    for batch in packs.chunks(HEAD_PACKS) {
        let ids: Vec<i64> = batch.iter().map(|(pack_id, _)| *pack_id).collect();
        let mut heads = Vec::with_capacity(ids.len());
        source
            .mechanics
            .pack_prefixes(&ids, HEAD_BYTES, &mut heads)
            .map_err(StorageError::from)
            .map_err(storage)?;
        if heads.len() != batch.len() {
            return Err(HistoryError::Integrity("mark pack heads"));
        }
        for ((pack_id, items), (length, head)) in batch.iter().zip(heads) {
            let answers = pack_edges(source, *pack_id, items, length, head)?;
            for (item, edge) in items.iter().zip(answers) {
                if let Some(edge) = edge {
                    edges.insert(item.object_id, edge);
                }
            }
        }
    }
    Ok(edges)
}
