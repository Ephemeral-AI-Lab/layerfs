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

/// One sequential reader over a run with a bounded private buffer.
pub struct RunReader<'a> {
    handle: &'a dyn OrderingRun,
    offset: u64,
    remaining: u64,
    buffer: Vec<u8>,
    filled: usize,
    consumed: usize,
}

impl<'a> RunReader<'a> {
    /// A reader over `run`, buffering whole rows only.
    pub fn new(run: &'a Run, buffer_bytes: usize) -> Self {
        let rows = (buffer_bytes / ROW_BYTES).max(1);
        Self {
            handle: run.handle.as_ref(),
            offset: 0,
            remaining: run.count,
            buffer: vec![0; rows * ROW_BYTES],
            filled: 0,
            consumed: 0,
        }
    }

    /// Next row in serial order, if any remains.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> ContentResult<Option<Row>> {
        if self.remaining == 0 {
            return Ok(None);
        }
        if self.consumed == self.filled {
            let rows = self.remaining.min((self.buffer.len() / ROW_BYTES) as u64) as usize;
            let filled = rows * ROW_BYTES;
            self.handle
                .read_at(self.offset, &mut self.buffer[..filled])?;
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
