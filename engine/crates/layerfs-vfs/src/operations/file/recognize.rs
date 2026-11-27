//! Recognition of committed content in the bytes of one write, before its
//! first owner visit.
//!
//! A pass reads the write's own bytes and mapping pages of the file's own
//! committed content. It reads no overlay row and no committed chunk. A whole
//! chunk of the write whose identity equals the committed chunk's is claimed;
//! every other byte is stored as before. A pass that finds nothing, cannot
//! read a page or is not made at all changes nothing about the write.
//!
//! A chunk that crosses the end of one request is carried: the identity of
//! its first bytes is kept, never the bytes, and the next request that begins
//! where that one ended completes it. Its claim then reaches back over the
//! bytes the earlier request stored, and the engine honours that part only
//! while nothing else was published to the file in between.
//!
//! A program that grows a file moves its tail from the end backward, so its
//! requests arrive in falling offsets and the chunk is cut the other way
//! round: its end was written first. A request therefore also keeps the
//! bytes of the chunk whose beginning lies before it, at most one chunk. The
//! request that ends where that one began holds the beginning; the identity
//! of both parts is then the proof, and the claim reaches past the request's
//! end under the same rule.
use crate::{BaseView, FileLengths};
use layerfs_content::file::cdc::{cut_candidates, MAXIMUM_CHUNK_BYTES, MINIMUM_CHUNK_BYTES};
use layerfs_content::file::mapping::{chunk_id, ChunkIdentity, Extent, ExtentCursor, FileState};
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, MINIMUM_SMALL_FILE_THRESHOLD_BYTES,
};
use layerfs_overlay::{ClaimedRange, WriteClaims, CELL_BYTES, RUN_BYTES, WRITE_CLAIMS};
use layerfs_telemetry::timer::{Active, Timing, TimingScope};
use std::sync::Arc;

/// A write shorter than one minimum chunk holds no whole chunk.
pub const RECOGNIZE_BYTES: usize = MINIMUM_CHUNK_BYTES;
/// Consecutive bytes of requests that claimed nothing after which a file's
/// writes are no longer examined.
pub const UNMATCHED_BYTES: u64 = 1 << 20;
/// Leading bytes of a request a search scans. Any window of two maximum
/// chunks of matching content holds one whole chunk.
const SEARCH_BYTES: usize = 2 * MAXIMUM_CHUNK_BYTES;
/// How far before the written offset a search without an earlier claim starts.
const SEARCH_BACK: u64 = 16 * MAXIMUM_CHUNK_BYTES as u64;
/// Committed extents one search compares, from each place it starts.
const SEARCH_EXTENTS: usize = 64;
/// Identities one search computes.
const SEARCH_HASHES: usize = 8;
/// Cut candidates one search keeps for each pair alignment.
const CANDIDATES: usize = 128;
/// Consecutive committed chunks that differ before a followed run ends.
const FOLLOW_MISSES: usize = 2;

/// Whether a file's committed content can supply a claim.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Gate {
    #[default]
    Unknown,
    /// No committed content, or none in chunks.
    Never,
    Chunked {
        root: [u8; 32],
        state: FileState,
    },
}
/// What the writes of one file have established: fixed size, no bytes. The
/// default knows nothing. It belongs to one base; another base starts over.
#[derive(Clone, Debug, Default)]
pub struct WriteRun {
    base: [u8; 32],
    gate: Gate,
    /// File offset less source offset of the last claimed chunk.
    delta: Option<i64>,
    /// Where the last range of the last claim begins and ends in the file,
    /// and where it ends in its source.
    file_start: u64,
    file_end: u64,
    source_end: u64,
    unmatched: u64,
    carry: Option<Box<Carry>>,
    back: Option<Box<Back>>,
}
/// The end of a committed chunk whose beginning lies before the request
/// that wrote it. Only the request that ends where that one began can
/// complete it.
#[derive(Clone, Debug)]
struct Back {
    /// The request that stored the bytes: where it began and ended.
    stored: (u64, u64),
    extent: Extent,
    /// The chunk's last bytes: the first bytes of that request.
    bytes: Arc<[u8]>,
    /// Where that request's claim right after the chunk ends in the file.
    reach: u64,
    sequence: Option<i64>,
}
/// A committed chunk whose first bytes ended one request. Only the request
/// that begins where that one ended can complete it.
#[derive(Clone, Debug)]
struct Carry {
    /// The request that stored the first bytes: where it began and ended.
    stored: (u64, u64),
    /// Where the chunk begins in the file.
    file: u64,
    extent: Extent,
    /// The chunk's identity so far: its bytes up to the end of that request.
    identity: ChunkIdentity,
    /// The revision of the publication that stored them, once it is known.
    sequence: Option<i64>,
}
/// The result of one pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Recognition {
    Decided(Option<WriteClaims>),
    /// A committed page or fact the pass needs is not in memory. Nothing was
    /// recorded; the same pass over an admitted reader decides.
    NeedsBase,
}
/// Cumulative work of recognition passes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RecognizeWork {
    pub passes: u64,
    pub needs_base: u64,
    /// Passes an admitted reader could not finish. Each stored its write.
    pub failed: u64,
    pub scanned_bytes: u64,
    pub hashed_bytes: u64,
    pub extents: u64,
    pub pages: u64,
    pub claims: u64,
    pub claimed_bytes: u64,
}
impl RecognizeWork {
    /// Adds the work of other passes to this total.
    pub fn add(&mut self, other: &Self) {
        self.passes += other.passes;
        self.needs_base += other.needs_base;
        self.failed += other.failed;
        self.scanned_bytes += other.scanned_bytes;
        self.hashed_bytes += other.hashed_bytes;
        self.extents += other.extents;
        self.pages += other.pages;
        self.claims += other.claims;
        self.claimed_bytes += other.claimed_bytes;
    }
}
impl WriteRun {
    /// Whether a write of `length` bytes over `base` is worth a pass. Reads
    /// nothing.
    pub fn wants(&self, base: &BaseView, length: usize) -> bool {
        length >= RECOGNIZE_BYTES
            && (self.base != base.identity().0.to_bytes()
                || (self.gate != Gate::Never && self.unmatched < UNMATCHED_BYTES))
    }
    /// Bytes the carried chunks add to a run at most, for the fixed
    /// allowance: one identity, and the end of one chunk.
    pub const CARRY_BYTES: usize =
        std::mem::size_of::<Carry>() + std::mem::size_of::<Back>() + MAXIMUM_CHUNK_BYTES;
    /// Records the publication of the write of `[offset, end)` that this run
    /// examined last: the bytes a carried chunk left in that write are the
    /// stored ones from `revision`. Any other write records nothing.
    pub fn published(&mut self, offset: u64, end: u64, revision: i64) {
        if let Some(carry) = &mut self.carry {
            if carry.stored == (offset, end) && carry.sequence.is_none() {
                carry.sequence = Some(revision);
            }
        }
        if let Some(back) = &mut self.back {
            if back.stored == (offset, end) && back.sequence.is_none() {
                back.sequence = Some(revision);
            }
        }
    }
    /// One pass over the bytes of a write at `offset` of file `serial`,
    /// through the client of `base`. A memory-only client that lacks a page
    /// answers [`Recognition::NeedsBase`] and records nothing.
    pub fn recognize(
        &mut self,
        base: &BaseView,
        serial: u64,
        offset: u64,
        data: &[u8],
        work: &mut RecognizeWork,
    ) -> Recognition {
        let identity = base.identity().0.to_bytes();
        let mut next = self.clone();
        if next.base != identity {
            next = Self {
                base: identity,
                ..Self::default()
            };
        }
        work.passes += 1;
        let pass = Timing::disabled("write.recognize", |scope| {
            next.pass(base, serial, offset, data, work, scope)
        });
        match pass.0 {
            Ok((ranges, after)) => {
                let gate = next.gate;
                *self = next;
                let Gate::Chunked { root, .. } = gate else {
                    return Recognition::Decided(None);
                };
                work.claims += ranges.len() as u64;
                work.claimed_bytes += ranges.iter().map(|range| range.length).sum::<u64>();
                Recognition::Decided((!ranges.is_empty()).then_some(WriteClaims {
                    source: root,
                    after,
                    ranges,
                }))
            }
            Err(_) if base.client().is_resident() => {
                work.needs_base += 1;
                Recognition::NeedsBase
            }
            Err(_) => {
                // A reader that cannot answer is charged like bytes that
                // match nothing, so a failing Store is not asked for long.
                if self.base != identity {
                    *self = Self {
                        base: identity,
                        ..Self::default()
                    };
                }
                self.unmatched = self.unmatched.saturating_add(data.len() as u64);
                (self.carry, self.back) = (None, None);
                work.failed += 1;
                Recognition::Decided(None)
            }
        }
    }
    fn pass(
        &mut self,
        base: &BaseView,
        serial: u64,
        offset: u64,
        data: &[u8],
        work: &mut RecognizeWork,
        scope: &TimingScope<'_, Active>,
    ) -> ContentResult<(Vec<ClaimedRange>, Option<i64>)> {
        // A carried chunk serves the request that follows its own, or none.
        let (carry, back) = (self.carry.take(), self.back.take());
        if self.gate == Gate::Unknown {
            self.gate = gate(base, serial)?;
        }
        let Gate::Chunked { state, .. } = self.gate else {
            return Ok((Vec::new(), None));
        };
        let client = base.client();
        let mut pass = Pass {
            cursor: ExtentCursor::new(state),
            client: client.as_ref(),
            scope,
            offset,
            data,
            ranges: Vec::new(),
            leading: None,
            work,
        };
        let (mut at, mut searched, mut crossing) = (0, false, None);
        let mut after = match carry {
            Some(carry) => pass.complete(self, *carry)?,
            None => None,
        };
        if after.is_some() {
            at = pass
                .ranges
                .last()
                .map_or(0, |range| (range.offset + range.length - offset) as usize);
        }
        loop {
            if let Some(delta) = self.delta {
                (at, crossing) = pass.follow(delta, at)?;
            }
            // One search a request, and none once the known shift has
            // matched in it: a second shift in the same bytes is found by
            // the next request.
            if searched || !pass.ranges.is_empty() || data.len() - at < RECOGNIZE_BYTES {
                break;
            }
            searched = true;
            match pass.search(self, at)? {
                Some((delta, start)) => (self.delta, at) = (Some(delta), start),
                None => break,
            }
        }
        pass.work.pages += pass.cursor.pages();
        // One claim names one earlier publication: a request completes a
        // chunk at its beginning or at its end, not both.
        if let (None, Some((start, extent)), Some(back)) = (after, crossing, back) {
            after = pass.join(start, &extent, &back);
            if after.is_some() {
                crossing = None;
            }
        }
        if let Some((start, extent)) = crossing {
            self.carry = pass.carry(start, extent);
        }
        self.back = pass.back();
        match pass.ranges.last() {
            Some(last) => {
                self.file_start = last.offset;
                self.file_end = last.offset + last.length;
                self.source_end = last.source_offset + last.length;
                self.unmatched = 0;
            }
            None => self.unmatched = self.unmatched.saturating_add(data.len() as u64),
        }
        Ok((pass.ranges, after))
    }
    /// Whether a chunk found at `file` that names `source` keeps the source
    /// moving forward through the file after the last claim.
    fn admits(&self, delta: i64, file: u64, source: u64) -> bool {
        match self.delta {
            None => true,
            Some(last) => last == delta || (file >= self.file_end && source >= self.source_end),
        }
    }
}
/// The committed file of `serial` in `base`, when its content is in chunks.
fn gate(base: &BaseView, serial: u64) -> ContentResult<Gate> {
    let value = match base.inode(serial) {
        Ok(found) => found.value,
        Err(ContentError::PathNotFound) => return Ok(Gate::Never),
        Err(error) => return Err(error),
    };
    if value.kind != InodeKind::RegularFile {
        return Ok(Gate::Never);
    }
    // A length below the smallest whole-file threshold is never in chunks:
    // the root object of such a file is its payload and is not read here.
    let length = base.client().file_length(value.content_root).map_err(|_| {
        ContentError::ProviderFailure {
            what: "recognizer file length",
        }
    })?;
    if length < MINIMUM_SMALL_FILE_THRESHOLD_BYTES {
        return Ok(Gate::Never);
    }
    let root = value.content_root.to_bytes();
    Ok(match base.content(root)?.state()? {
        Some(state) => Gate::Chunked { root, state },
        None => Gate::Never,
    })
}
struct Pass<'a, 's> {
    cursor: ExtentCursor,
    client: &'a dyn AuthenticatedObjects,
    scope: &'a TimingScope<'s, Active>,
    offset: u64,
    data: &'a [u8],
    ranges: Vec<ClaimedRange>,
    /// The committed chunk that holds the request's first byte and begins
    /// before it, including the first run established by a search.
    leading: Option<Extent>,
    work: &'a mut RecognizeWork,
}
impl Pass<'_, '_> {
    fn next(&mut self) -> ContentResult<Option<Extent>> {
        let extent = self.cursor.next(self.client, self.scope)?;
        self.work.extents += u64::from(extent.is_some());
        Ok(extent)
    }
    /// Whether bytes `[start, end)` of the write are the whole chunk of
    /// `extent`: equal identity is the proof, and a slice of a chunk has none.
    fn equal(&mut self, extent: &Extent, start: usize, end: usize) -> bool {
        if extent.chunk_offset != 0 {
            return false;
        }
        self.work.hashed_bytes += (end - start) as u64;
        chunk_id(&self.data[start..end]).is_ok_and(|id| id == extent.chunk)
    }
    /// Claims the chunk at `start`; false when the write can hold no more
    /// ranges. A chunk that continues the last range in the file and in the
    /// source extends it.
    fn claim(&mut self, start: usize, extent: &Extent) -> bool {
        let range = ClaimedRange {
            offset: self.offset + start as u64,
            length: u64::from(extent.length),
            source_offset: extent.offset,
        };
        let full = self.ranges.len() == WRITE_CLAIMS;
        match self.ranges.last_mut() {
            Some(last)
                if last.offset + last.length == range.offset
                    && last.source_offset + last.length == range.source_offset =>
            {
                last.length += range.length;
            }
            _ if full => return false,
            _ => self.ranges.push(range),
        }
        true
    }
    /// Completes a carried chunk with the first bytes of this request. When
    /// its identity is the committed chunk's, the claim begins with the
    /// bytes the earlier request stored: the chunk's own, and before them the
    /// end of that request's claim that was not whole cells. Returns the
    /// revision those stored bytes belong to.
    fn complete(&mut self, run: &WriteRun, carry: Carry) -> ContentResult<Option<i64>> {
        let rest = carry.identity.remaining();
        let shift = i128::from(carry.file) - i128::from(carry.extent.offset);
        let (Some(sequence), true) = (
            carry.sequence,
            carry.stored.1 == self.offset
                && rest <= self.data.len()
                && run.delta.map(i128::from) == Some(shift),
        ) else {
            return Ok(None);
        };
        let mut identity = carry.identity;
        identity.update(&self.data[..rest])?;
        self.work.hashed_bytes += rest as u64;
        if identity.finish()? != carry.extent.chunk {
            return Ok(None);
        }
        let mut from = carry.file;
        if run.file_end == carry.file && run.source_end == carry.extent.offset {
            from = (from - from % CELL_BYTES as u64).max(run.file_start);
        }
        // Never before the request that stored the bytes, nor further back
        // than the engine accepts.
        let from = from
            .max(carry.stored.0)
            .max(self.offset.saturating_sub(RUN_BYTES as u64));
        let Ok(source_offset) = u64::try_from(i128::from(from) - shift) else {
            return Ok(None);
        };
        self.ranges.push(ClaimedRange {
            offset: from,
            length: self.offset + rest as u64 - from,
            source_offset,
        });
        Ok(Some(sequence))
    }
    /// Completes the chunk of `extent`, which begins at `start` and continues
    /// past this request, with the bytes the request above it stored. When
    /// the identity of both parts is the committed chunk's, the claim ends
    /// in those stored bytes: with the chunk, and after it with the beginning
    /// of that request's claim that was not whole cells. Returns the revision
    /// those stored bytes belong to.
    fn join(&mut self, start: usize, extent: &Extent, back: &Back) -> Option<i64> {
        let end = self.offset + self.data.len() as u64;
        let head = self.data.len() - start;
        let whole = (back.extent == *extent
            && back.stored.0 == end
            && head + back.bytes.len() == extent.length as usize)
            .then_some(back.sequence)??;
        let mut identity = ChunkIdentity::new(extent.length as usize).ok()?;
        identity.update(&self.data[start..]).ok()?;
        identity.update(&back.bytes).ok()?;
        self.work.hashed_bytes += u64::from(extent.length);
        if identity.finish().ok()? != extent.chunk || !self.claim(start, extent) {
            return None;
        }
        // The cell the chunk ends in is whole only with the bytes after it,
        // which that request claimed: never past its claim or past what the
        // engine accepts.
        let last = self.ranges.last_mut()?;
        let to = last.offset + last.length;
        let cell = to.div_ceil(CELL_BYTES as u64) * CELL_BYTES as u64;
        if cell <= back.reach && cell <= end + RUN_BYTES as u64 {
            last.length += cell - to;
        }
        Some(whole)
    }
    /// The first bytes of this request when they are the end of a committed
    /// chunk that begins before it and the claim begins right after them.
    fn back(&mut self) -> Option<Box<Back>> {
        let (extent, first) = (self.leading?, self.ranges.first()?);
        let tail = usize::try_from(first.offset.checked_sub(self.offset)?).ok()?;
        let chunk = extent.offset + u64::from(extent.length);
        if tail == 0 || tail >= extent.length as usize || first.source_offset != chunk {
            return None;
        }
        Some(Box::new(Back {
            stored: (self.offset, self.offset + self.data.len() as u64),
            extent,
            bytes: Arc::from(&self.data[..tail]),
            reach: first.offset + first.length,
            sequence: None,
        }))
    }
    /// The identity so far of the chunk of `extent`, whose bytes begin at
    /// `start` and continue past this request.
    fn carry(&mut self, start: usize, extent: Extent) -> Option<Box<Carry>> {
        let mut identity = ChunkIdentity::new(extent.length as usize).ok()?;
        identity.update(&self.data[start..]).ok()?;
        self.work.hashed_bytes += (self.data.len() - start) as u64;
        Some(Box::new(Carry {
            stored: (self.offset, self.offset + self.data.len() as u64),
            file: self.offset + start as u64,
            extent,
            identity,
            sequence: None,
        }))
    }
    /// Compares the committed chunks that lie, shifted by `delta`, wholly
    /// inside the bytes from `from`. Returns where the first chunk of the
    /// final run of differing chunks begins, or the length of the bytes; and
    /// the whole chunk that begins in the bytes and continues past them,
    /// when the run reached it.
    fn follow(
        &mut self,
        delta: i64,
        from: usize,
    ) -> ContentResult<(usize, Option<(usize, Extent)>)> {
        let first = i128::from(self.offset) + from as i128;
        // A search starts at its first whole match. Also visit the chunk
        // holding the request's first byte so a backward writer can carry
        // its tail into the next request. No earlier bytes are claimed here.
        let scan = if self.ranges.is_empty() {
            self.leading = None;
            i128::from(self.offset)
        } else {
            first
        };
        let source = u64::try_from((scan - i128::from(delta)).max(0)).unwrap_or(u64::MAX);
        self.cursor.seek(source);
        let (mut misses, mut resume, mut crossing) = (0, None, None);
        while let Some(extent) = self.next()? {
            let file = i128::from(extent.offset) + i128::from(delta);
            // The extent that holds the first byte may begin before it.
            if file < first {
                if file < i128::from(self.offset)
                    && file + i128::from(extent.length) > i128::from(self.offset)
                    && extent.chunk_offset == 0
                {
                    self.leading = Some(extent);
                }
                continue;
            }
            let start = (file - i128::from(self.offset)) as usize;
            let end = start.saturating_add(extent.length as usize);
            if end > self.data.len() {
                if start < self.data.len() && misses == 0 && extent.chunk_offset == 0 {
                    crossing = Some((start, extent));
                }
                break;
            }
            if self.equal(&extent, start, end) {
                if !self.claim(start, &extent) {
                    break;
                }
                (misses, resume) = (0, None);
            } else {
                resume.get_or_insert(start);
                misses += 1;
                if misses == FOLLOW_MISSES {
                    break;
                }
            }
        }
        Ok((resume.unwrap_or(self.data.len()), crossing))
    }
    /// Looks for one committed chunk in the bytes from `from`: a chunk that
    /// begins and ends where the frozen scanner could have cut, has the
    /// length of a nearby committed extent, and its identity. Returns the
    /// shift and where the chunk begins.
    fn search(&mut self, run: &WriteRun, from: usize) -> ContentResult<Option<(i64, usize)>> {
        let window = &self.data[from..self.data.len().min(from + SEARCH_BYTES)];
        let first = self.offset + from as u64;
        // Ends by the parity of the chunk's first byte in the window.
        let mut ends = [Vec::new(), Vec::new()];
        for (parity, ends) in ends.iter_mut().enumerate() {
            cut_candidates(&window[parity..], &mut |at| {
                if ends.len() < CANDIDATES {
                    ends.push(at + parity);
                }
            });
        }
        self.work.scanned_bytes += 2 * window.len() as u64;
        // A chunk begins where the one before it ended, or with the bytes.
        let mut starts: Vec<usize> = ends.iter().flatten().copied().collect();
        starts.push(0);
        starts.sort_unstable();
        starts.dedup();
        // Committed extents after the last claim, then those around the
        // written offset when they lie further on.
        let near = first.saturating_sub(SEARCH_BACK);
        let hints = match run.delta {
            Some(_) if first >= run.file_end => [
                Some(run.source_end),
                Some(near).filter(|near| *near > run.source_end),
            ],
            _ => [Some(near), None],
        };
        let mut hashes = 0;
        for hint in hints.into_iter().flatten() {
            self.cursor.seek(hint);
            for _ in 0..SEARCH_EXTENTS {
                let Some(extent) = self.next()? else {
                    break;
                };
                let length = extent.length as usize;
                for &start in &starts {
                    let end = start + length;
                    if end > window.len() {
                        break;
                    }
                    // A chunk cut at the maximum size ends where no scanner
                    // chose: nothing but its identity would point at it, so
                    // a search leaves it to the run that follows a shift.
                    if ends[start % 2].binary_search(&end).is_err() {
                        continue;
                    }
                    let file = first + start as u64;
                    let Ok(delta) = i64::try_from(i128::from(file) - i128::from(extent.offset))
                    else {
                        continue;
                    };
                    if !run.admits(delta, file, extent.offset) {
                        continue;
                    }
                    if hashes == SEARCH_HASHES {
                        return Ok(None);
                    }
                    hashes += 1;
                    if self.equal(&extent, from + start, from + end) {
                        return Ok(Some((delta, from + start)));
                    }
                }
            }
        }
        Ok(None)
    }
}
