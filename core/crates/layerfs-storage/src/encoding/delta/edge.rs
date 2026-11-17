//! A payload record's dependency edge from bounded reads of its pack.
//!
//! [`super::read::ChainBases::edge`] borrows the record's complete encoded
//! group, so a walk that wants only edges pays for every payload byte it
//! passes. An uncompressed group keeps everything an edge needs at offsets its
//! own framing names: the record count and directory at the group's head, then
//! the record's first bytes. [`sparse_directory`] and [`sparse_edge`] read
//! those from whatever the caller already holds and name the next range it
//! lacks, so the caller decides how to batch the reads. No payload byte is
//! read, and so none is authenticated here: a consumer that reconstructs the
//! record still does that.
//!
//! A group's directory is validated and parsed once into a
//! [`RecordDirectory`]; each record's range is then one step. Asking the
//! framing reader for every record of a group instead scans the directory once
//! per record.
use super::read::ChainEdge;
use super::record;
use crate::error::{StorageError, StorageResult};
use crate::location::ObjectLocation;
use crate::pack::layout::{record_range, GroupCodec, GroupView, PackLane};
use crate::policy::RECORD_COUNT_LIMIT;
use crate::port::PackRange;

/// Bytes at a group's head a walk asks for first: the record count, the
/// directory of a group of up to 100 records, and usually its first header.
pub const HEAD_BYTES: usize = 512;

/// One uncompressed group's record directory, validated whole exactly as
/// [`crate::encoding::framed_record`] validates it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordDirectory {
    /// Record `n` spans `bounds[n]..bounds[n + 1]` of the group.
    bounds: Vec<usize>,
}

impl RecordDirectory {
    /// The directory of a complete uncompressed group body.
    pub fn of(group: &[u8]) -> StorageResult<Self> {
        let count = count_of(group.get(..4), group.len())?;
        let ends = group
            .get(4..4 + 4 * count)
            .ok_or(StorageError::Integrity("group record directory"))?;
        Self::parse(count, ends, group.len())
    }

    fn parse(count: usize, ends: &[u8], group_length: usize) -> StorageResult<Self> {
        // The one validator of a directory: counts, ordering and the exact
        // record area. What follows only copies offsets it accepted.
        record_range(count, ends, group_length, 0)?;
        let area = 4 + ends.len();
        let mut bounds = Vec::with_capacity(count + 1);
        bounds.push(area);
        for end in ends.chunks_exact(4) {
            let mut field = [0; 4];
            field.copy_from_slice(end);
            bounds.push(area + u32::from_le_bytes(field) as usize);
        }
        Ok(Self { bounds })
    }

    /// The records the group holds.
    pub fn count(&self) -> usize {
        self.bounds.len() - 1
    }

    /// The group-relative range of record `number`.
    pub fn range(&self, number: usize) -> StorageResult<(usize, usize)> {
        match (self.bounds.get(number), self.bounds.get(number + 1)) {
            (Some(start), Some(end)) => Ok((*start, *end)),
            _ => Err(StorageError::Integrity("group record directory")),
        }
    }
}

fn count_of(field: Option<&[u8]>, group_length: usize) -> StorageResult<usize> {
    let mut count = [0; 4];
    count.copy_from_slice(field.ok_or(StorageError::Integrity("group framing"))?);
    let count = u32::from_le_bytes(count) as usize;
    if !(1..=RECORD_COUNT_LIMIT).contains(&count) || 4 + 4 * count > group_length {
        return Err(StorageError::Integrity("group record directory"));
    }
    Ok(count)
}

/// What the bytes a caller holds answer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Sparse<T> {
    /// The answer.
    Ready(T),
    /// The pack range to acquire before asking again.
    Need(PackRange),
}

/// Whether a record of the group `view` of a `lane` pack has an edge these
/// readers can report. A compressed group and the role-dependent `Ordinary`
/// lane take the decoding reader.
pub fn has_sparse_edges(lane: PackLane, view: GroupView) -> bool {
    lane != PackLane::Ordinary && view.codec == GroupCodec::Raw
}

/// The record directory of the uncompressed group `view`, from the pack bytes
/// `held` returns.
///
/// `held(offset, length)` returns exactly `length` bytes at pack offset
/// `offset`, or `None` when the caller does not hold all of them.
pub fn sparse_directory<'a>(
    view: GroupView,
    held: impl Fn(usize, usize) -> Option<&'a [u8]>,
) -> StorageResult<Sparse<RecordDirectory>> {
    let length = view
        .end
        .checked_sub(view.start)
        .filter(|length| *length >= 4)
        .ok_or(StorageError::Integrity("group framing"))?;
    let need = |length: usize| {
        Sparse::Need(PackRange {
            offset: view.start,
            length,
        })
    };
    let Some(count) = held(view.start, 4) else {
        return Ok(need(length.min(HEAD_BYTES)));
    };
    let count = count_of(Some(count), length)?;
    let Some(ends) = held(view.start + 4, 4 * count) else {
        return Ok(need((4 + 4 * count + record::HEADER_BYTES).min(length)));
    };
    RecordDirectory::parse(count, ends, length).map(Sparse::Ready)
}

/// The edge of the record `location` names in the group `view` of a `lane`
/// pack, exactly as [`super::read::ChainBases::edge`] reports it, from the
/// group's `directory` and the pack bytes `held` returns.
pub fn sparse_edge<'a>(
    lane: PackLane,
    view: GroupView,
    directory: &RecordDirectory,
    location: &ObjectLocation,
    held: impl Fn(usize, usize) -> Option<&'a [u8]>,
) -> StorageResult<Sparse<ChainEdge>> {
    if !has_sparse_edges(lane, view) {
        return Err(StorageError::Integrity("encoded record edge"));
    }
    if lane == PackLane::Singleton && location.record_number != 0 {
        return Err(StorageError::Integrity("singleton record ordinal"));
    }
    let (start, end) = directory.range(location.record_number)?;
    let width = end - start;
    let header = width.min(record::HEADER_BYTES);
    let Some(bytes) = held(view.start + start, header) else {
        return Ok(Sparse::Need(PackRange {
            offset: view.start + start,
            length: header,
        }));
    };
    Ok(Sparse::Ready(ChainEdge {
        base: record::header_base(lane, bytes, width, location.canonical_length)?,
        width: width as u64,
    }))
}
