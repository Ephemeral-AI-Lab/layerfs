//! The delta edges of one mark wave's payload records, from bounded pack reads
//! (§6.2).
//!
//! A file payload has no logical references, so the mark needs only its
//! record's base and stored width. Both sit in the record's first bytes and in
//! its group's directory, so this reads those ranges and never the payload: a
//! mark costs a few hundred bytes per pack instead of the pack. Compressed
//! groups and the `Ordinary` lane answer `None` and keep the decoding reader.
//!
//! Reads are per wave, not per pack: one call for the heads of many packs,
//! then at most a few calls for whatever ranges those packs still need. A
//! pack costs no statement and no round trip of its own.
use super::ids::IdMap;
use super::{storage, ClosureSource};
use layerfs_content::ObjectId;
use layerfs_history::error::{HistoryError, HistoryResult};
use layerfs_storage::{
    encoding::delta::edge::{
        has_sparse_edges, sparse_directory, sparse_edge, RecordDirectory, Sparse,
    },
    encoding::delta::read::ChainEdge,
    location::ObjectLocation,
    pack::layout::{self, GroupView, PackHeader},
    port::{PackRange, PACK_READ_PREFIX_BYTES},
    source::Source,
    StorageError,
};
use std::collections::{BTreeMap, VecDeque};

/// After the head: a wider directory, then record headers. No pack needs more.
const ROUNDS: usize = 4;
/// Bytes of each pack the first read asks for: the directory of a pack of up
/// to about twenty groups, its first group's record directory and first
/// record header. A pack with a wider directory is read again.
///
/// The size is chosen so the read never leaves the pack row's own page in a
/// SQLite Store. A row too large for its 4,096-byte page keeps at least its
/// first 489 bytes there and the rest in a chain of further pages; the
/// columns before the body take at most 49 of those. Up to 440 bytes of body
/// are therefore always on the page the row lookup has already loaded, and
/// one byte more costs every such pack another page read: a random one on a
/// cold cache. A PostgreSQL Store reads its first stored piece either way.
const HEAD_BYTES: usize = 432;
/// Packs whose heads one call reads.
const HEAD_PACKS: usize = 256;
/// Ranges one call asks for, over all its packs.
const BATCH_RANGES: usize = 1024;
/// Bytes one call asks for, over all its packs.
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

/// One request of a round: the pack, its declared length, and the ranges.
type Request = (i64, usize, Vec<PackRange>);

/// Reads every request's ranges in bounded calls and returns each request's
/// bytes, range by range.
fn read_ranges(
    source: &ClosureSource<'_>,
    requests: &[Request],
) -> HistoryResult<Vec<Vec<Vec<u8>>>> {
    let mut bodies: Vec<Vec<Vec<u8>>> = requests.iter().map(|_| Vec::new()).collect();
    let mut call: Vec<(i64, Vec<PackRange>)> = Vec::new();
    let mut owners: Vec<usize> = Vec::new();
    let mut flush =
        |call: &mut Vec<(i64, Vec<PackRange>)>, owners: &mut Vec<usize>| -> HistoryResult<()> {
            if call.is_empty() {
                return Ok(());
            }
            let mut read = Vec::with_capacity(call.len());
            source
                .mechanics
                .pack_ranges(call, &mut read)
                .map_err(StorageError::from)
                .map_err(storage)?;
            if read.len() != call.len() {
                return Err(HistoryError::Integrity("mark range read"));
            }
            for ((owner, (_, asked)), got) in owners.drain(..).zip(call.drain(..)).zip(read) {
                let exact = got.len() == asked.len()
                    && asked
                        .iter()
                        .zip(&got)
                        .all(|(range, bytes)| bytes.len() == range.length);
                if !exact {
                    return Err(HistoryError::Integrity("mark range read"));
                }
                bodies[owner].extend(got);
            }
            Ok(())
        };
    let (mut count, mut bytes) = (0, 0);
    for (index, (pack_id, _, ranges)) in requests.iter().enumerate() {
        for range in ranges {
            if count > 0 && (count == BATCH_RANGES || bytes + range.length > BATCH_BYTES) {
                flush(&mut call, &mut owners)?;
                (count, bytes) = (0, 0);
            }
            match call.last_mut() {
                Some((_, list)) if owners.last() == Some(&index) => list.push(*range),
                _ => {
                    call.push((*pack_id, vec![*range]));
                    owners.push(index);
                }
            }
            count += 1;
            bytes += range.length;
        }
    }
    flush(&mut call, &mut owners)?;
    Ok(bodies)
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
        let request = [(pack_id, usize::MAX, merged(needs))];
        let mut read = 0u64;
        for (range, bytes) in request[0]
            .2
            .iter()
            .zip(read_ranges(source, &request)?.remove(0))
        {
            read += bytes.len() as u64;
            held.add(range.offset, bytes);
        }
        source.note_scoped_body(pack_id, read, u64::MAX);
    }
    Err(HistoryError::Integrity("mark group directory"))
}

/// One pack's records in a wave, and what the wave has read of the pack.
struct PackWork<'a> {
    pack_id: i64,
    length: usize,
    items: &'a [ObjectLocation],
    header: PackHeader,
    views: Vec<GroupView>,
    held: Pieces,
    answers: Vec<Answer>,
    settled: bool,
}

impl<'a> PackWork<'a> {
    /// The pack's layout from its declared length and head. A directory wider
    /// than the head is read again, whole.
    fn open(
        source: &ClosureSource<'_>,
        pack_id: i64,
        items: &'a [ObjectLocation],
        length: usize,
        head: Vec<u8>,
    ) -> HistoryResult<Self> {
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
        Ok(Self {
            pack_id,
            length,
            items,
            header,
            views,
            held,
            answers: vec![Answer::Pending; items.len()],
            settled: false,
        })
    }
}

/// Answers every record of `work`: each round resolves what the held bytes
/// can and reads, in one bounded set of calls, what all the packs still need.
fn settle(source: &ClosureSource<'_>, work: &mut [PackWork<'_>]) -> HistoryResult<()> {
    for round in 0..=ROUNDS {
        let mut requests: Vec<Request> = Vec::new();
        let mut owners = Vec::new();
        for (index, pack) in work.iter_mut().enumerate() {
            if pack.settled {
                continue;
            }
            let needs = resolve(
                pack.header,
                &pack.views,
                pack.items,
                &pack.held,
                &mut pack.answers,
            )
            .map_err(storage)?;
            if needs.is_empty() {
                pack.settled = true;
            } else {
                requests.push((pack.pack_id, pack.length, needs));
                owners.push(index);
            }
        }
        if requests.is_empty() {
            return Ok(());
        }
        if round == ROUNDS {
            break;
        }
        let bodies = read_ranges(source, &requests)?;
        for ((owner, (pack_id, length, ranges)), read) in
            owners.into_iter().zip(requests).zip(bodies)
        {
            let mut total = 0u64;
            for (range, bytes) in ranges.iter().zip(read) {
                total += bytes.len() as u64;
                work[owner].held.add(range.offset, bytes);
            }
            source.note_scoped_body(pack_id, total, u64::try_from(length).unwrap_or(u64::MAX));
        }
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
) -> HistoryResult<IdMap<ChainEdge>> {
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
    let mut edges = IdMap::default();
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
        let mut work = Vec::with_capacity(batch.len());
        for ((pack_id, items), (length, head)) in batch.iter().zip(heads) {
            work.push(PackWork::open(source, *pack_id, items, length, head)?);
        }
        settle(source, &mut work)?;
        for pack in work {
            for (item, answer) in pack.items.iter().zip(pack.answers) {
                match answer {
                    Answer::Edge(edge) => {
                        edges.insert(item.object_id, edge);
                    }
                    Answer::Encoded => {}
                    Answer::Pending => return Err(HistoryError::Integrity("mark edge read")),
                }
            }
            source.keep_layout(pack.pack_id, pack.views);
        }
    }
    Ok(edges)
}

/// The directory prefixes of the packs a kept-waste pass still has to read,
/// handed out in pack order and read a bounded batch of packs per call.
pub(super) struct Prefixes<'a, 'b> {
    source: &'a ClosureSource<'b>,
    packs: Vec<i64>,
    next: usize,
    held: VecDeque<(usize, Vec<u8>)>,
}

impl<'a, 'b> Prefixes<'a, 'b> {
    /// `packs` in the order [`Self::take`] will be asked for them.
    pub(super) fn new(source: &'a ClosureSource<'b>, packs: Vec<i64>) -> Self {
        Self {
            source,
            packs,
            next: 0,
            held: VecDeque::new(),
        }
    }

    /// The declared length and directory prefix of `pack_id`, which must be
    /// the next pack of the list.
    pub(super) fn take(&mut self, pack_id: i64) -> HistoryResult<(usize, Vec<u8>)> {
        if self.packs.get(self.next) != Some(&pack_id) {
            return Err(HistoryError::Integrity("mark pack directory"));
        }
        if self.held.is_empty() {
            let batch = &self.packs[self.next..(self.next + HEAD_PACKS).min(self.packs.len())];
            let mut read = Vec::with_capacity(batch.len());
            self.source
                .mechanics
                .pack_prefixes(batch, PACK_READ_PREFIX_BYTES, &mut read)
                .map_err(StorageError::from)
                .map_err(storage)?;
            if read.len() != batch.len() {
                return Err(HistoryError::Integrity("mark pack directory"));
            }
            self.held = read.into();
        }
        self.next += 1;
        self.held
            .pop_front()
            .ok_or(HistoryError::Integrity("mark pack directory"))
    }
}
