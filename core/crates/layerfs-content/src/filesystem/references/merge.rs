//! Two-reader tiered merge with newest-value precedence.
//!
//! Runs are immutable and sorted. A merge reads the older and newer run with one
//! bounded buffer each and writes one output run, so simultaneous state is two
//! readers, one buffer and one output regardless of how many batches were spilled;
//! each spilled batch participates in at most `log2(batches)` merges. On a key
//! both runs hold, the newer row wins outright: a newer row already incorporates
//! the older effects, because the reducer re-reads and updates a spilled row
//! before it mutates it.

use crate::error::{ContentError, ContentResult};
use crate::filesystem::references::backing::OrderingRun;
use crate::filesystem::references::record::{Row, ROW_BYTES};

/// One immutable sorted run of ordering rows.
pub struct Run {
    /// The run's storage.
    pub handle: Box<dyn OrderingRun>,
    /// Rows in the run.
    pub count: u64,
    /// Smallest serial in the run.
    pub first: u64,
    /// Largest serial in the run.
    pub last: u64,
}

/// Work one merge or spill performed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MergeWork {
    /// Rows written to runs.
    pub rows_written: u64,
    /// Rows read back from runs.
    pub rows_read: u64,
    /// Runs created.
    pub runs_created: u64,
    /// Merges performed.
    pub merges: u64,
    /// Highest occupied tier.
    pub peak_level: usize,
    /// Largest number of live runs at once.
    pub peak_live_runs: usize,
    /// Largest run bytes held at once.
    pub peak_run_bytes: u64,
}

/// Buffered sequential row-reader state over one run, without the run itself.
///
/// The scan owns its buffer; the run's storage is borrowed per call. That split
/// is what lets the run store keep one reader per tier alive across lookups:
/// the buffer is allocated once, the cursor state is the scan's fields, and the
/// handle comes from whichever run the tier holds today.
pub struct RunScan {
    /// File offset of the next row to read.
    offset: u64,
    /// Rows the run still holds beyond `offset`.
    remaining: u64,
    /// The retained buffer, sized to whole rows.
    buffer: Vec<u8>,
    /// Valid bytes in `buffer`.
    filled: usize,
    /// Bytes of `buffer` already consumed.
    consumed: usize,
}

impl RunScan {
    /// A scan with one bounded buffer of `buffer_bytes`.
    ///
    /// The buffer is rounded to whole rows and is never reallocated; a restart
    /// only invalidates the bytes it holds.
    pub fn new(buffer_bytes: usize) -> Self {
        let rows = (buffer_bytes / ROW_BYTES).max(1);
        Self {
            offset: 0,
            remaining: 0,
            buffer: vec![0; rows * ROW_BYTES],
            filled: 0,
            consumed: 0,
        }
    }

    /// Positions the scan `offset` bytes into `run`, keeping the buffer.
    ///
    /// `offset` must be a whole number of rows within the run, so the first row
    /// this scan returns is the one at that position. Positioning invalidates
    /// the buffered bytes; it does not allocate.
    pub fn start(&mut self, run: &Run, offset: u64) -> ContentResult<()> {
        if offset % ROW_BYTES as u64 != 0 {
            return Err(ContentError::InvalidOrderingRecord("run seek offset"));
        }
        let skipped = offset / ROW_BYTES as u64;
        if skipped > run.count {
            return Err(ContentError::InvalidOrderingRecord("run seek past end"));
        }
        self.offset = offset;
        self.remaining = run.count - skipped;
        self.filled = 0;
        self.consumed = 0;
        Ok(())
    }

    /// Puts the last returned row back, so the next call returns it again.
    ///
    /// A scan that stops on a row it did not compare with its request leaves
    /// that row pending instead of consuming it.
    pub fn rewind(&mut self) {
        if self.consumed >= ROW_BYTES {
            self.consumed -= ROW_BYTES;
            self.offset -= ROW_BYTES as u64;
            self.remaining += 1;
        }
    }

    /// Next row in serial order, if any remains.
    ///
    /// The row is served from the retained buffer when it is already held and
    /// from one bounded `read_at` when it is not.
    pub fn next(&mut self, handle: &dyn OrderingRun) -> ContentResult<Option<Row>> {
        if self.remaining == 0 {
            return Ok(None);
        }
        if self.consumed == self.filled {
            let rows = self.remaining.min((self.buffer.len() / ROW_BYTES) as u64) as usize;
            let filled = rows * ROW_BYTES;
            handle.read_at(self.offset, &mut self.buffer[..filled])?;
            self.consumed = 0;
            self.filled = filled;
        }
        let bytes: &[u8; ROW_BYTES] = self.buffer[self.consumed..self.consumed + ROW_BYTES]
            .try_into()
            .map_err(|_| ContentError::UnexpectedEof)?;
        let row = Row::decode(bytes)?;
        self.consumed += ROW_BYTES;
        self.offset += ROW_BYTES as u64;
        self.remaining -= 1;
        Ok(Some(row))
    }

    /// File offset of the next row.
    pub const fn run_offset(&self) -> u64 {
        self.offset
    }

    /// Rows this scan has returned since it was positioned.
    pub const fn rows(&self) -> u64 {
        self.offset / ROW_BYTES as u64
    }
}

/// One sequential reader over a run with a bounded private buffer.
///
/// The reader is a fresh [`RunScan`] plus the run's handle, for callers - the
/// merge and visit paths - that read a whole run once and drop the reader. The
/// run store's lookup path uses `RunScan` directly so its buffer survives the
/// call.
pub struct RunReader<'a> {
    handle: &'a dyn OrderingRun,
    scan: RunScan,
}

impl<'a> RunReader<'a> {
    /// A reader over `run`, buffering whole rows only.
    pub fn new(run: &'a Run, buffer_bytes: usize) -> Self {
        let mut scan = RunScan::new(buffer_bytes);
        scan.remaining = run.count;
        Self {
            handle: run.handle.as_ref(),
            scan,
        }
    }

    /// Puts the last returned row back, so the next call returns it again.
    ///
    /// A scan that stops on a row it did not compare with its request leaves that
    /// row pending instead of consuming it.
    pub fn rewind(&mut self) {
        self.scan.rewind();
    }

    /// Next row in serial order, if any remains.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> ContentResult<Option<Row>> {
        self.scan.next(self.handle)
    }

    /// File offset of the next row.
    pub const fn run_offset(&self) -> u64 {
        self.scan.run_offset()
    }

    /// Rows this reader has returned.
    pub const fn rows(&self) -> u64 {
        self.scan.rows()
    }
}

/// Merges one older and one newer run into a fresh run.
///
/// Both inputs stay borrowed until the caller installs the finished output, so a
/// failed write leaves the previous tiers exactly as they were.
pub fn merge_runs(
    backing: &mut dyn crate::filesystem::references::backing::OrderingBacking,
    older: &Run,
    newer: &Run,
    buffer_bytes: usize,
    work: &mut MergeWork,
) -> ContentResult<Run> {
    let output_rows = older
        .count
        .checked_add(newer.count)
        .ok_or(ContentError::LengthOverflow)?;
    let mut handle = backing.create_run()?;
    work.runs_created = work.runs_created.saturating_add(1);
    let mut older_reader = RunReader::new(older, buffer_bytes);
    let mut newer_reader = RunReader::new(newer, buffer_bytes);
    let mut old = older_reader.next()?;
    let mut new = newer_reader.next()?;
    work.rows_read = work.rows_read.saturating_add(older.count + newer.count);
    let mut count = 0_u64;
    let mut first = None;
    let mut last = 0_u64;
    while old.is_some() || new.is_some() {
        let take_new = match (&old, &new) {
            (Some(old_row), Some(new_row)) => new_row.serial() <= old_row.serial(),
            (Some(_), None) => false,
            (None, Some(_)) => true,
            (None, None) => break,
        };
        let row = if take_new {
            let row = new.take().ok_or(ContentError::UnexpectedEof)?;
            if old
                .as_ref()
                .is_some_and(|entry| entry.serial() == row.serial())
            {
                old = older_reader.next()?;
            }
            new = newer_reader.next()?;
            row
        } else {
            let row = old.take().ok_or(ContentError::UnexpectedEof)?;
            old = older_reader.next()?;
            row
        };
        handle.append(&row.encode()?)?;
        first.get_or_insert(row.serial());
        last = row.serial();
        count = count.saturating_add(1);
        work.rows_written = work.rows_written.saturating_add(1);
    }
    handle.flush()?;
    work.merges = work.merges.saturating_add(1);
    if count > output_rows {
        return Err(ContentError::InvalidOrderingRecord("merge rows"));
    }
    Ok(Run {
        handle,
        count,
        first: first.ok_or(ContentError::InvalidOrderingRecord("empty merge"))?,
        last,
    })
}
