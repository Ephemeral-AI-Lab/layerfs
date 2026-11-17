//! The delta edges of one mark wave's payload records, from bounded pack reads
//! (§6.2).
//!
//! A file payload has no logical references, so the mark needs only its
//! record's base and stored width. Both sit in the record's first bytes and in
//! its group's directory, so this reads those ranges and never the payload: a
//! mark costs a few hundred bytes per pack instead of the pack. Compressed
//! groups and the `Ordinary` lane answer `None` and keep the decoding reader.
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
    port::{AcquiredPackRead, PackRange, PackReadChoice, PackReadPlan, PersistenceError},
    source::Source,
    StorageError,
};
use std::collections::BTreeMap;

/// Count, then a wider directory, then record headers: no pack needs more.
const ROUNDS: usize = 4;
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

/// Selects the ranges a round needs. The first round has no directory yet, so
/// it derives its ranges from the prefix the acquisition hands it.
struct EdgeRanges<'a> {
    pack_id: i64,
    items: &'a [ObjectLocation],
    held: &'a Pieces,
    answers: &'a mut [Answer],
    layout: Option<(PackHeader, Vec<GroupView>)>,
    ranges: Option<Vec<PackRange>>,
}

impl PackReadPlan for EdgeRanges<'_> {
    fn select(
        &mut self,
        info: PackInfo,
        prefix: &[u8],
    ) -> Result<PackReadChoice, PersistenceError> {
        if info.pack_id != self.pack_id {
            return Err(PersistenceError::Malformed);
        }
        if let Some(ranges) = self.ranges.take() {
            return Ok(PackReadChoice::Ranges(ranges));
        }
        let header = layout::parse_directory_header(prefix, info.length)
            .map_err(|_| PersistenceError::Malformed)?;
        let views = layout::directory_group_views(prefix, header)
            .map_err(|_| PersistenceError::Malformed)?;
        let mut ranges = resolve(header, &views, self.items, self.held, self.answers)
            .map_err(|_| PersistenceError::Malformed)?;
        if ranges.is_empty() {
            // Every group of this pack is encoded. An acquisition must select
            // something, and the first body byte always exists.
            ranges.push(PackRange {
                offset: header.body_offset,
                length: 1,
            });
        }
        ranges.truncate(BATCH_RANGES);
        self.layout = Some((header, views));
        Ok(PackReadChoice::Ranges(ranges))
    }
}

/// One acquisition of `plan`'s ranges, added to `held` and to the mark's bytes.
fn acquire(
    source: &ClosureSource<'_>,
    pack_id: i64,
    plan: &mut EdgeRanges<'_>,
) -> HistoryResult<Vec<(PackRange, Vec<u8>)>> {
    let read = source
        .mechanics
        .read_scoped_pack(pack_id, plan)
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
        let mut plan = EdgeRanges {
            pack_id,
            items: &[],
            held: &Pieces::default(),
            answers: &mut [],
            layout: None,
            ranges: Some(std::mem::take(batch)),
        };
        acquired.extend(acquire(source, pack_id, &mut plan)?);
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

/// The edges of `items`, all records of pack `pack_id`: `None` where the
/// record's group is encoded and must take the decoding reader.
fn pack_edges(
    source: &ClosureSource<'_>,
    pack_id: i64,
    items: &[ObjectLocation],
) -> HistoryResult<Vec<Option<ChainEdge>>> {
    let mut held = Pieces::default();
    let mut answers = vec![Answer::Pending; items.len()];
    let pieces = {
        let mut plan = EdgeRanges {
            pack_id,
            items,
            held: &held,
            answers: &mut answers,
            layout: None,
            ranges: None,
        };
        let pieces = acquire(source, pack_id, &mut plan)?;
        (plan.layout, pieces)
    };
    let (Some((header, views)), first) = pieces else {
        return Err(HistoryError::Integrity("mark edge directory"));
    };
    for (range, bytes) in first {
        held.add(range.offset, bytes);
    }
    for _ in 0..ROUNDS {
        let needs = resolve(header, &views, items, &held, &mut answers).map_err(storage)?;
        if needs.is_empty() {
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
/// `wave` is in locator order, so each pack's records are adjacent and one
/// pack is read at a time. An object absent from the result takes the decoding
/// reader, exactly as before.
pub(super) fn wave_edges(
    source: &ClosureSource<'_>,
    wave: &[ObjectId],
) -> HistoryResult<BTreeMap<ObjectId, ChainEdge>> {
    let mut edges = BTreeMap::new();
    let mut items: Vec<ObjectLocation> = Vec::new();
    let mut flush = |items: &mut Vec<ObjectLocation>| -> HistoryResult<()> {
        let Some(first) = items.first() else {
            return Ok(());
        };
        for (item, edge) in items.iter().zip(pack_edges(source, first.pack_id, items)?) {
            if let Some(edge) = edge {
                edges.insert(item.object_id, edge);
            }
        }
        items.clear();
        Ok(())
    };
    for id in wave {
        let Some(location) = source.location(*id, i64::MAX).map_err(storage)? else {
            continue;
        };
        if location.role.has_references() {
            continue;
        }
        if items
            .first()
            .is_some_and(|first| first.pack_id != location.pack_id)
        {
            flush(&mut items)?;
        }
        items.push(location);
    }
    flush(&mut items)?;
    Ok(edges)
}
