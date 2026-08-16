//! Unencoded logical file content over immutable CAS chunks.

use std::io::{Cursor, Read};
use std::ops::Range;

use crate::cas::{InMemoryCas, PutOutcome};
use crate::cdc::FastCdc;
use crate::limits::MAX_CHILD_REFERENCES;
use crate::{ChunkId, CoreError, CoreResult};

pub const MAX_REJOIN_WINDOW_BYTES: u64 = 1024 * 1024;
const REJOIN_CONFIRM_CHUNKS: usize = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChunkReference {
    id: ChunkId,
    length: u64,
}

impl ChunkReference {
    pub const fn new(id: ChunkId, length: u64) -> Self {
        Self { id, length }
    }

    pub const fn id(self) -> ChunkId {
        self.id
    }

    pub const fn length(self) -> u64 {
        self.length
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogicalFile {
    chunks: Vec<ChunkReference>,
    length: u64,
}

impl LogicalFile {
    pub fn from_chunks(cas: &InMemoryCas, chunks: Vec<ChunkReference>) -> CoreResult<Self> {
        if chunks.len() > MAX_CHILD_REFERENCES {
            return Err(CoreError::ObjectLimitExceeded);
        }

        let mut length = 0_u64;
        for &reference in &chunks {
            validate_chunk(cas, reference)?;
            length = length
                .checked_add(reference.length)
                .ok_or(CoreError::LengthOverflow)?;
        }
        Self::from_authenticated_chunks(chunks, length)
    }

    fn from_authenticated_chunks(chunks: Vec<ChunkReference>, length: u64) -> CoreResult<Self> {
        if chunks.len() > MAX_CHILD_REFERENCES {
            return Err(CoreError::ObjectLimitExceeded);
        }
        let actual_length = chunks.iter().try_fold(0_u64, |length, reference| {
            length
                .checked_add(reference.length)
                .ok_or(CoreError::LengthOverflow)
        })?;
        if actual_length != length {
            return Err(CoreError::LengthMismatch {
                expected: length,
                actual: actual_length,
            });
        }
        Ok(Self { chunks, length })
    }

    pub const fn length(&self) -> u64 {
        self.length
    }

    pub const fn is_empty(&self) -> bool {
        self.length == 0
    }

    pub fn chunks(&self) -> &[ChunkReference] {
        &self.chunks
    }

    pub fn read_range(&self, cas: &InMemoryCas, range: Range<u64>) -> CoreResult<RangeRead> {
        if range.start > range.end || range.end > self.length {
            return Err(CoreError::InvalidRange {
                start: range.start,
                end: range.end,
                length: self.length,
            });
        }
        if range.start == range.end {
            return Ok(RangeRead {
                bytes: Vec::new(),
                chunks_read: 0,
            });
        }

        let requested = range
            .end
            .checked_sub(range.start)
            .ok_or(CoreError::LengthOverflow)?;
        let capacity = usize::try_from(requested).map_err(|_| CoreError::LengthOverflow)?;
        let mut bytes = Vec::with_capacity(capacity);
        let mut offset = 0_u64;
        let mut chunks_read = 0_usize;

        for &reference in &self.chunks {
            let chunk_end = offset
                .checked_add(reference.length)
                .ok_or(CoreError::LengthOverflow)?;
            if offset >= range.end {
                break;
            }
            if chunk_end <= range.start {
                offset = chunk_end;
                continue;
            }

            let chunk = validate_chunk(cas, reference)?;
            let chunk_start = range.start.saturating_sub(offset);
            let chunk_end = range.end.min(chunk_end) - offset;
            let chunk_start =
                usize::try_from(chunk_start).map_err(|_| CoreError::LengthOverflow)?;
            let chunk_end = usize::try_from(chunk_end).map_err(|_| CoreError::LengthOverflow)?;
            bytes.extend_from_slice(&chunk[chunk_start..chunk_end]);
            chunks_read = chunks_read
                .checked_add(1)
                .ok_or(CoreError::LengthOverflow)?;
            offset = offset
                .checked_add(reference.length)
                .ok_or(CoreError::LengthOverflow)?;
        }

        Ok(RangeRead { bytes, chunks_read })
    }

    pub fn replace_range(
        &self,
        cas: &mut InMemoryCas,
        range: Range<u64>,
        replacement: &[u8],
    ) -> CoreResult<EditResult> {
        if range.start > range.end || range.end > self.length {
            return Err(CoreError::InvalidRange {
                start: range.start,
                end: range.end,
                length: self.length,
            });
        }
        if range.start == range.end && replacement.is_empty() {
            return Ok(EditResult {
                file: self.clone(),
                counters: EditCounters::default(),
            });
        }

        let replacement_length =
            u64::try_from(replacement.len()).map_err(|_| CoreError::LengthOverflow)?;
        let deleted_length = range
            .end
            .checked_sub(range.start)
            .ok_or(CoreError::LengthOverflow)?;
        let length_without_deleted = self
            .length
            .checked_sub(deleted_length)
            .ok_or(CoreError::LengthOverflow)?;
        let new_length = length_without_deleted
            .checked_add(replacement_length)
            .ok_or(CoreError::LengthOverflow)?;
        let offsets = self.chunk_start_offsets()?;
        let scan_start_index = self.scan_start_index(&offsets, range.start);
        let scan_start_offset = offsets[scan_start_index];
        let suffix_start_index = self.suffix_start_index(&offsets, range.end);
        let probe_end_index = self.rejoin_probe_end(suffix_start_index)?;
        let probe_end_offset = offsets[probe_end_index];

        let old_prefix_length = range
            .start
            .checked_sub(scan_start_offset)
            .ok_or(CoreError::LengthOverflow)?;
        let old_probe_length = probe_end_offset
            .checked_sub(range.end)
            .ok_or(CoreError::LengthOverflow)?;
        let scan_length = old_prefix_length
            .checked_add(replacement_length)
            .and_then(|length| length.checked_add(old_probe_length))
            .ok_or(CoreError::LengthOverflow)?;
        let scan_capacity = usize::try_from(scan_length).map_err(|_| CoreError::LengthOverflow)?;
        let mut scan_input = Vec::with_capacity(scan_capacity);
        append_old_range(
            self,
            cas,
            &offsets,
            scan_start_offset..range.start,
            &mut scan_input,
        )?;
        scan_input.extend_from_slice(replacement);
        append_old_range(
            self,
            cas,
            &offsets,
            range.end..probe_end_offset,
            &mut scan_input,
        )?;

        let (scanned, mut counters) = scan_and_store(cas, &scan_input)?;
        let (new_chunks, retained_scanned, suffix_index) =
            if suffix_start_index == self.chunks.len() {
                let mut chunks = self.chunks[..scan_start_index].to_vec();
                chunks.extend(scanned.iter().map(|chunk| chunk.reference));
                (chunks, scanned.len(), None)
            } else {
                let rejoin = find_rejoin(
                    &scanned,
                    &self.chunks,
                    &offsets,
                    range,
                    scan_start_offset,
                    replacement_length,
                )
                .ok_or(CoreError::BoundedResynchronization {
                    scanned: counters.cdc_bytes_scanned,
                    limit: MAX_REJOIN_WINDOW_BYTES,
                })?;
                let mut chunks = scanned[..rejoin.scanned_index]
                    .iter()
                    .map(|chunk| chunk.reference)
                    .collect::<Vec<_>>();
                let mut prefix = self.chunks[..scan_start_index].to_vec();
                prefix.append(&mut chunks);
                chunks = prefix;
                chunks.extend_from_slice(&self.chunks[rejoin.old_index..]);
                (chunks, rejoin.scanned_index, Some(rejoin.old_index))
            };

        counters.chunks_reused = 0;
        counters.chunks_created = 0;
        add_counter(&mut counters.chunks_reused, scan_start_index as u64)?;
        if let Some(old_index) = suffix_index {
            let reused = self
                .chunks
                .len()
                .checked_sub(old_index)
                .ok_or(CoreError::LengthOverflow)?;
            add_counter(&mut counters.chunks_reused, reused as u64)?;
        }
        for chunk in scanned.iter().take(retained_scanned) {
            match chunk.outcome {
                PutOutcome::Inserted => add_counter(&mut counters.chunks_created, 1)?,
                PutOutcome::Reused => add_counter(&mut counters.chunks_reused, 1)?,
            }
        }

        let file = Self::from_authenticated_chunks(new_chunks, new_length)?;
        Ok(EditResult { file, counters })
    }

    pub fn full_replace<R: Read>(cas: &mut InMemoryCas, reader: R) -> CoreResult<EditResult> {
        let (scanned, counters) = scan_and_store_reader(cas, reader)?;
        let length = scanned.iter().try_fold(0_u64, |length, chunk| {
            length
                .checked_add(chunk.reference.length)
                .ok_or(CoreError::LengthOverflow)
        })?;
        let chunks = scanned.iter().map(|chunk| chunk.reference).collect();
        let file = Self::from_authenticated_chunks(chunks, length)?;
        Ok(EditResult { file, counters })
    }

    fn chunk_start_offsets(&self) -> CoreResult<Vec<u64>> {
        let capacity = self
            .chunks
            .len()
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        let mut offsets = Vec::with_capacity(capacity);
        offsets.push(0_u64);
        for &reference in &self.chunks {
            let next = offsets
                .last()
                .copied()
                .ok_or(CoreError::LengthOverflow)?
                .checked_add(reference.length)
                .ok_or(CoreError::LengthOverflow)?;
            offsets.push(next);
        }
        Ok(offsets)
    }

    fn scan_start_index(&self, offsets: &[u64], position: u64) -> usize {
        if position == self.length {
            return if self.chunks.is_empty() {
                0
            } else {
                self.chunks.len() - 1
            };
        }
        self.chunks
            .iter()
            .enumerate()
            .find(|(index, _)| position < offsets[index + 1])
            .map_or(0, |(index, _)| index)
    }

    fn suffix_start_index(&self, offsets: &[u64], position: u64) -> usize {
        offsets
            .iter()
            .enumerate()
            .find(|(_, &offset)| offset >= position)
            .map_or(self.chunks.len(), |(index, _)| index)
    }

    fn rejoin_probe_end(&self, start: usize) -> CoreResult<usize> {
        let mut end = start;
        let mut bytes = 0_u64;
        while end < self.chunks.len() {
            let next = bytes
                .checked_add(self.chunks[end].length)
                .ok_or(CoreError::LengthOverflow)?;
            if next > MAX_REJOIN_WINDOW_BYTES && end > start {
                break;
            }
            bytes = next;
            end = end.checked_add(1).ok_or(CoreError::LengthOverflow)?;
        }
        Ok(end)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EditCounters {
    pub cdc_bytes_scanned: u64,
    pub chunks_reused: u64,
    pub chunks_created: u64,
    pub bytes_hashed: u64,
    pub bytes_delivered: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditResult {
    file: LogicalFile,
    counters: EditCounters,
}

impl EditResult {
    pub fn file(&self) -> &LogicalFile {
        &self.file
    }

    pub fn into_file(self) -> LogicalFile {
        self.file
    }

    pub const fn counters(&self) -> EditCounters {
        self.counters
    }
}

#[derive(Clone, Copy)]
struct ScannedChunk {
    reference: ChunkReference,
    outcome: PutOutcome,
    start: u64,
}

fn scan_and_store(
    cas: &mut InMemoryCas,
    input: &[u8],
) -> CoreResult<(Vec<ScannedChunk>, EditCounters)> {
    scan_and_store_reader(cas, Cursor::new(input))
}

fn scan_and_store_reader<R: Read>(
    cas: &mut InMemoryCas,
    reader: R,
) -> CoreResult<(Vec<ScannedChunk>, EditCounters)> {
    let mut scanned = Vec::new();
    let mut counters = EditCounters::default();
    let cdc_counters = FastCdc::new().scan(reader, |bytes| {
        let length = u64::try_from(bytes.len()).map_err(|_| CoreError::LengthOverflow)?;
        let start = counters.bytes_hashed;
        let (id, outcome) = cas.put_chunk(bytes)?;
        counters.bytes_hashed = counters
            .bytes_hashed
            .checked_add(length)
            .ok_or(CoreError::LengthOverflow)?;
        match outcome {
            PutOutcome::Inserted => add_counter(&mut counters.chunks_created, 1)?,
            PutOutcome::Reused => add_counter(&mut counters.chunks_reused, 1)?,
        }
        scanned.push(ScannedChunk {
            reference: ChunkReference::new(id, length),
            outcome,
            start,
        });
        Ok(())
    })?;
    counters.cdc_bytes_scanned = cdc_counters.bytes_scanned;
    counters.bytes_delivered = counters.cdc_bytes_scanned;
    Ok((scanned, counters))
}

fn append_old_range(
    file: &LogicalFile,
    cas: &InMemoryCas,
    offsets: &[u64],
    range: Range<u64>,
    output: &mut Vec<u8>,
) -> CoreResult<()> {
    if range.start > range.end || range.end > file.length {
        return Err(CoreError::InvalidRange {
            start: range.start,
            end: range.end,
            length: file.length,
        });
    }
    for (index, &reference) in file.chunks.iter().enumerate() {
        let chunk_start = offsets[index];
        let chunk_end = offsets[index + 1];
        let start = range.start.max(chunk_start);
        let end = range.end.min(chunk_end);
        if start >= end {
            continue;
        }
        let bytes = validate_chunk(cas, reference)?;
        let local_start = usize::try_from(
            start
                .checked_sub(chunk_start)
                .ok_or(CoreError::LengthOverflow)?,
        )
        .map_err(|_| CoreError::LengthOverflow)?;
        let local_end = usize::try_from(
            end.checked_sub(chunk_start)
                .ok_or(CoreError::LengthOverflow)?,
        )
        .map_err(|_| CoreError::LengthOverflow)?;
        output.extend_from_slice(&bytes[local_start..local_end]);
    }
    Ok(())
}

struct Rejoin {
    scanned_index: usize,
    old_index: usize,
}

fn find_rejoin(
    scanned: &[ScannedChunk],
    old_chunks: &[ChunkReference],
    offsets: &[u64],
    range: Range<u64>,
    scan_start_offset: u64,
    replacement_length: u64,
) -> Option<Rejoin> {
    let changed_prefix = range.start.checked_sub(scan_start_offset)?;
    let changed_prefix = changed_prefix.checked_add(replacement_length)?;
    let suffix_start = offsets.iter().position(|&offset| offset >= range.end)?;
    let mut probe_end = suffix_start;
    let mut probe_bytes = 0_u64;
    while probe_end < old_chunks.len() {
        let next = probe_bytes.checked_add(old_chunks[probe_end].length)?;
        if next > MAX_REJOIN_WINDOW_BYTES && probe_end > suffix_start {
            break;
        }
        probe_bytes = next;
        probe_end = probe_end.checked_add(1)?;
    }
    for old_index in suffix_start..probe_end {
        let old_relative = offsets[old_index].checked_sub(range.end)?;
        let expected_start = changed_prefix.checked_add(old_relative)?;
        let Some(scanned_index) = scanned
            .iter()
            .position(|chunk| chunk.start == expected_start)
        else {
            continue;
        };
        let confirmations = REJOIN_CONFIRM_CHUNKS.min(probe_end - old_index);
        let matches = (0..confirmations).all(|offset| {
            let Some(scanned_chunk) = scanned.get(scanned_index + offset) else {
                return false;
            };
            scanned_chunk.reference == old_chunks[old_index + offset]
        });
        if matches {
            return Some(Rejoin {
                scanned_index,
                old_index,
            });
        }
    }
    None
}

fn add_counter(counter: &mut u64, amount: u64) -> CoreResult<()> {
    *counter = counter
        .checked_add(amount)
        .ok_or(CoreError::LengthOverflow)?;
    Ok(())
}

fn validate_chunk(cas: &InMemoryCas, reference: ChunkReference) -> CoreResult<&[u8]> {
    let bytes = cas.get(reference.id)?;
    let actual = u64::try_from(bytes.len()).map_err(|_| CoreError::LengthOverflow)?;
    if actual != reference.length {
        return Err(CoreError::LengthMismatch {
            expected: reference.length,
            actual,
        });
    }
    Ok(bytes)
}

#[derive(Debug, Eq, PartialEq)]
pub struct RangeRead {
    bytes: Vec<u8>,
    chunks_read: usize,
}

impl RangeRead {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub const fn chunks_read(&self) -> usize {
        self.chunks_read
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::cas::InMemoryCas;
    use crate::cdc::FastCdc;
    use crate::chunk_id;

    fn cas_and_chunks() -> (InMemoryCas, Vec<ChunkReference>) {
        let mut cas = InMemoryCas::new();
        let mut references = Vec::new();
        for bytes in [b"abcd".as_slice(), b"efgh", b"ijkl", b"mnop"] {
            let (id, _) = cas.put_chunk(bytes).unwrap();
            references.push(ChunkReference::new(id, bytes.len() as u64));
        }
        (cas, references)
    }

    #[test]
    fn reconstructs_exact_bytes_from_ordered_chunks() {
        let (cas, references) = cas_and_chunks();
        let file = LogicalFile::from_chunks(&cas, references).unwrap();
        let read = file.read_range(&cas, 0..file.length()).unwrap();
        assert_eq!(read.bytes(), b"abcdefghijklmnop");
        assert_eq!(read.chunks_read(), 4);
    }

    #[test]
    fn reads_only_chunks_overlapping_a_cross_chunk_range() {
        let (cas, references) = cas_and_chunks();
        let file = LogicalFile::from_chunks(&cas, references).unwrap();
        let read = file.read_range(&cas, 5..11).unwrap();
        assert_eq!(read.bytes(), b"fghijk");
        assert_eq!(read.chunks_read(), 2);
    }

    #[test]
    fn empty_and_eof_ranges_are_explicit() {
        let cas = InMemoryCas::new();
        let empty = LogicalFile::from_chunks(&cas, Vec::new()).unwrap();
        assert!(empty.is_empty());
        assert_eq!(empty.read_range(&cas, 0..0).unwrap().bytes(), b"");
        assert_eq!(
            empty.read_range(&cas, 0..1),
            Err(CoreError::InvalidRange {
                start: 0,
                end: 1,
                length: 0,
            })
        );

        let (cas, references) = cas_and_chunks();
        let file = LogicalFile::from_chunks(&cas, references).unwrap();
        let empty = file.read_range(&cas, 5..5).unwrap();
        assert_eq!(empty.bytes(), b"");
        assert_eq!(empty.chunks_read(), 0);
        let eof = file.read_range(&cas, 16..16).unwrap();
        assert_eq!(eof.bytes(), b"");
        assert_eq!(eof.chunks_read(), 0);
        assert_eq!(
            file.read_range(&cas, 16..17),
            Err(CoreError::InvalidRange {
                start: 16,
                end: 17,
                length: 16,
            })
        );
        assert_eq!(
            file.read_range(&cas, Range { start: 9, end: 8 }),
            Err(CoreError::InvalidRange {
                start: 9,
                end: 8,
                length: 16,
            })
        );
    }

    #[test]
    fn rejects_invalid_declared_lengths() {
        let (cas, references) = cas_and_chunks();
        let invalid = vec![ChunkReference::new(references[0].id(), 3)];
        assert_eq!(
            LogicalFile::from_chunks(&cas, invalid),
            Err(CoreError::LengthMismatch {
                expected: 3,
                actual: 4,
            })
        );
    }

    #[test]
    fn rejects_missing_chunks() {
        let cas = InMemoryCas::new();
        let missing = vec![ChunkReference::new(chunk_id(b"missing"), 7)];
        assert_eq!(
            LogicalFile::from_chunks(&cas, missing),
            Err(CoreError::MissingObject)
        );
    }

    fn generated_input(length: usize) -> Vec<u8> {
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        (0..length)
            .map(|_| {
                state ^= state.wrapping_shl(7);
                state ^= state.wrapping_shr(9);
                state ^= state.wrapping_shl(8);
                state as u8
            })
            .collect()
    }

    fn file_for(data: &[u8]) -> (InMemoryCas, LogicalFile) {
        let mut cas = InMemoryCas::new();
        let mut references = Vec::new();
        FastCdc::new()
            .scan(Cursor::new(data), |chunk| {
                let (id, _) = cas.put_chunk(chunk)?;
                references.push(ChunkReference::new(id, chunk.len() as u64));
                Ok(())
            })
            .unwrap();
        let file = LogicalFile::from_chunks(&cas, references).unwrap();
        (cas, file)
    }

    fn all_bytes(cas: &InMemoryCas, file: &LogicalFile) -> Vec<u8> {
        file.read_range(cas, 0..file.length()).unwrap().into_bytes()
    }

    #[test]
    fn bounded_edits_reconstruct_and_reuse_authenticated_chunks() {
        let data = generated_input(2 * 1024 * 1024);
        let (mut cas, file) = file_for(&data);

        let mut expected = data.clone();
        expected.splice(500_000..500_001, [0xa5]);
        let middle = file
            .replace_range(&mut cas, 500_000..500_001, &[0xa5])
            .unwrap();
        let actual = all_bytes(&cas, middle.file());
        let first_difference = actual
            .iter()
            .zip(&expected)
            .position(|(actual, expected)| actual != expected);
        assert_eq!(
            actual.len(),
            expected.len(),
            "first diff: {first_difference:?}"
        );
        assert_eq!(
            &actual[499_990..500_010],
            &expected[499_990..500_010],
            "first diff: {first_difference:?}"
        );
        assert!(middle.counters().cdc_bytes_scanned < file.length());
        assert!(middle.counters().chunks_reused > 0);
        assert_eq!(
            middle.counters().cdc_bytes_scanned,
            middle.counters().bytes_hashed
        );
        assert_eq!(
            middle.counters().cdc_bytes_scanned,
            middle.counters().bytes_delivered
        );

        let mut expected = data.clone();
        expected.splice(0..0, b"prefix".iter().copied());
        let prepend = file.replace_range(&mut cas, 0..0, b"prefix").unwrap();
        assert_eq!(all_bytes(&cas, prepend.file()), expected);
        assert!(prepend.counters().chunks_reused > 0);

        let mut expected = data.clone();
        expected.splice(data.len()..data.len(), b"suffix".iter().copied());
        let append = file
            .replace_range(&mut cas, file.length()..file.length(), b"suffix")
            .unwrap();
        assert_eq!(all_bytes(&cas, append.file()), expected);
        assert!(append.counters().cdc_bytes_scanned < file.length());

        let mut expected = data.clone();
        expected.truncate(700_000);
        let truncated = file
            .replace_range(&mut cas, 700_000..file.length(), &[])
            .unwrap();
        assert_eq!(all_bytes(&cas, truncated.file()), expected);
        assert!(truncated.counters().cdc_bytes_scanned < file.length());

        let eof = file
            .replace_range(&mut cas, file.length()..file.length(), &[])
            .unwrap();
        assert_eq!(eof.file(), &file);
        assert_eq!(eof.counters(), EditCounters::default());
    }

    #[test]
    fn full_replace_is_a_separate_streaming_path() {
        let data = generated_input(100_000);
        let mut cas = InMemoryCas::new();
        let result = LogicalFile::full_replace(&mut cas, Cursor::new(data.clone())).unwrap();
        assert_eq!(all_bytes(&cas, result.file()), data);
        assert_eq!(result.counters().cdc_bytes_scanned, 100_000);
        assert_eq!(
            result.counters().cdc_bytes_scanned,
            result.counters().bytes_hashed
        );
        assert_eq!(
            result.counters().cdc_bytes_scanned,
            result.counters().bytes_delivered
        );
        assert!(result.counters().chunks_created > 0);
    }

    #[test]
    fn bounded_rejoin_failure_is_typed() {
        let data = b"small file";
        let (mut cas, file) = file_for(data);
        let error = file
            .replace_range(&mut cas, 0..0, b"prepend")
            .expect_err("a one-chunk suffix cannot prove a rejoin");
        assert_eq!(
            error,
            CoreError::BoundedResynchronization {
                scanned: 17,
                limit: MAX_REJOIN_WINDOW_BYTES,
            }
        );
    }
}
