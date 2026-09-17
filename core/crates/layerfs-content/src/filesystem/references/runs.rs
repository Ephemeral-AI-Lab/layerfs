//! Bounded pending state and the tiered run store that backs it.
//!
//! The reducer holds at most `max_pending` rows in memory. Crossing that bound is
//! planned work, not a recovery path: the pending map is written as one sorted run
//! and merged into the tiered structure, which keeps live runs logarithmic in the
//! number of spills. A caller that supplies no backing and cannot hold the
//! pending state fails before the row is inserted, so no operation silently grows
//! without a declared owner.
//!
//! The store never rewrites the accumulated prefix: a spill writes its own batch
//! and merges it into the first free tier, so each batch participates in at most
//! `log2(batches)` merges.
//!
//! **One byte owner.** This store is the operation's ordering owner: the pending
//! rows it is about to spill, every live run and the output a merge is about to
//! create are all reserved against one declared ceiling before that storage is
//! created. Obsolete runs are dropped as soon as their rows are merged, which is
//! what returns their bytes; nothing counts only the final run while older files
//! are still owned. The physical owner is the backing, whose own ceiling is
//! enforced when a run appends.

use std::collections::BTreeMap;

use crate::error::{ContentError, ContentResult};
use crate::filesystem::references::backing::OrderingBacking;
use crate::filesystem::references::merge::{merge_runs, MergeWork, Run, RunReader};
use crate::filesystem::references::record::{Row, ROW_BYTES};

/// Default bytes of one merge buffer.
pub const DEFAULT_MERGE_BUFFER_BYTES: usize = 16 * 1024;
/// Default ordering bytes one operation may own across pending rows and runs.
pub const DEFAULT_ORDERING_BYTES: u64 = 64 * 1024 * 1024;
/// Largest tier index the store can address.
pub const MAXIMUM_LEVELS: usize = 32;

/// Tiered runs over one caller-supplied backing.
pub struct RunStore<'r, 'b> {
    backing: Option<&'r mut (dyn OrderingBacking + 'b)>,
    levels: Vec<Option<Run>>,
    /// One resumable lookup scan per tier, parallel to `levels`.
    scans: Vec<LookupScan>,
    merge_buffer: usize,
    limit: u64,
    /// Bytes of the pending rows the reducer is about to spill.
    pending_bytes: u64,
    /// Bytes of the run inputs a merge holds while its output is written.
    ///
    /// A merge reads two runs and writes a third, and all three exist at once.
    /// Taking the inputs out of `levels` before merging used to drop them from
    /// the declared set, so the ceiling never covered the bytes the operation
    /// really owned at that moment.
    merge_input_bytes: u64,
    work: MergeWork,
}

impl<'r, 'b> RunStore<'r, 'b> {
    /// A store over optional `backing` with a bounded merge buffer.
    ///
    /// Without backing the store can still answer lookups and merges for state a
    /// caller holds in memory; the first spill fails explicitly, because growing
    /// without a declared owner is not a fallback.
    pub fn new(
        backing: Option<&'r mut (dyn OrderingBacking + 'b)>,
        merge_buffer: usize,
        limit: u64,
    ) -> Self {
        Self {
            backing,
            levels: Vec::new(),
            scans: Vec::new(),
            merge_buffer: merge_buffer.max(ROW_BYTES),
            limit,
            pending_bytes: 0,
            merge_input_bytes: 0,
            work: MergeWork::default(),
        }
    }

    /// Checks that the backing can hold the declared ordering ceiling.
    ///
    /// The ceiling is this operation's promise to itself. If the physical owner's
    /// own ceiling is smaller, the promise is unenforceable and the operation
    /// fails before it starts rather than discovering it at the first spill.
    pub fn check_capacity(&self) -> ContentResult<()> {
        let Some(capacity) = self
            .backing
            .as_deref()
            .and_then(|backing| backing.capacity_bytes())
        else {
            return Ok(());
        };
        if capacity < self.limit {
            return Err(ContentError::ResourceUnavailable {
                what: "ordering backing capacity",
            });
        }
        Ok(())
    }

    /// The declared ordering ceiling.
    pub const fn limit_bytes(&self) -> u64 {
        self.limit
    }

    /// Drops every tier's scan position, keeping the tiers themselves.
    ///
    /// Called whenever a tier's run is replaced: the position belongs to the run
    /// that was scanned, not to the tier.
    fn reset_scans(&mut self) {
        self.scans.clear();
    }

    /// Bytes of the pending rows currently charged to this operation.
    pub const fn pending_bytes(&self) -> u64 {
        self.pending_bytes
    }

    /// Charges the pending map the reducer is about to spill.
    pub fn charge_pending(&mut self, rows: u64) -> ContentResult<()> {
        let bytes = rows
            .checked_mul(ROW_BYTES as u64)
            .ok_or(ContentError::LengthOverflow)?;
        // The pending rows are part of the same owned set: a spill is only allowed
        // when the pending bytes and the run they become both fit.
        self.reserve(bytes.saturating_mul(2))?;
        self.pending_bytes = bytes;
        Ok(())
    }

    /// Bytes the operation owns right now: live runs, spilled-but-unmerged
    /// inputs, the pending rows about to be spilled and any reserved output.
    pub fn owned_bytes(&self) -> u64 {
        self.run_bytes()
            .saturating_add(self.pending_bytes)
            .saturating_add(self.merge_input_bytes)
    }

    /// Reserves `bytes` of storage this operation is about to create.
    pub fn reserve(&mut self, bytes: u64) -> ContentResult<()> {
        let owned = self.owned_bytes();
        let next = owned
            .checked_add(bytes)
            .ok_or(ContentError::LengthOverflow)?;
        if next > self.limit {
            return Err(ContentError::ObjectLimitExceeded {
                limit: usize::try_from(self.limit).unwrap_or(usize::MAX),
                actual: usize::try_from(next).unwrap_or(usize::MAX),
            });
        }
        self.work.peak_run_bytes = self.work.peak_run_bytes.max(next);
        self.note_physical_peak();
        Ok(())
    }

    /// Folds the physical owner's reported peak into this store's own observation.
    fn note_physical_peak(&mut self) {
        if let Some(backing) = self.backing.as_deref() {
            self.work.peak_run_bytes = self.work.peak_run_bytes.max(backing.peak_bytes());
        }
    }

    /// Work performed so far.
    pub const fn work(&self) -> MergeWork {
        self.work
    }

    /// True when no run exists yet.
    pub fn is_empty(&self) -> bool {
        self.levels.iter().all(Option::is_none)
    }

    /// Live runs.
    pub fn live_runs(&self) -> usize {
        self.levels.iter().flatten().count()
    }

    /// The declared backing, when the caller supplied one.
    fn backing(&mut self) -> ContentResult<&mut (dyn OrderingBacking + 'b)> {
        match self.backing.as_deref_mut() {
            Some(backing) => Ok(backing),
            None => Err(ContentError::ResourceUnavailable {
                what: "ordering backing",
            }),
        }
    }

    /// Bytes currently held by live runs.
    pub fn run_bytes(&self) -> u64 {
        self.levels
            .iter()
            .flatten()
            .map(|run| run.count * ROW_BYTES as u64)
            .sum()
    }

    /// Writes `pending` as one run and merges it into the tiers.
    ///
    /// Level 0 is the newest data, so a lookup stops at the first tier that holds
    /// the key.
    pub fn spill(&mut self, pending: &BTreeMap<u64, Row>) -> ContentResult<()> {
        if pending.is_empty() {
            return Ok(());
        }
        // Reserve the batch this spill is about to write before creating it.
        self.charge_pending(pending.len() as u64)?;
        self.reserve(self.pending_bytes)?;
        let level = self
            .levels
            .iter()
            .position(Option::is_none)
            .unwrap_or(self.levels.len());
        if level >= MAXIMUM_LEVELS {
            return Err(ContentError::ResourceUnavailable {
                what: "ordering tiers",
            });
        }
        if level == self.levels.len() {
            self.levels.push(None);
        }
        self.reset_scans();
        self.work.peak_level = self.work.peak_level.max(level);
        let mut handle = self.backing()?.create_run()?;
        self.work.runs_created = self.work.runs_created.saturating_add(1);
        let mut first = None;
        let mut last = 0_u64;
        for row in pending.values() {
            handle.append(&row.encode()?)?;
            first.get_or_insert(row.serial());
            last = row.serial();
            self.work.rows_written = self.work.rows_written.saturating_add(1);
        }
        handle.flush()?;
        let mut run = Run {
            handle,
            count: pending.len() as u64,
            first: first.ok_or(ContentError::InvalidOrderingRecord("spill rows"))?,
            last,
        };
        // The tiers this batch merges into are taken out of the store first, so
        // each obsolete run is dropped (its bytes and its file returned) as soon
        // as its rows have been merged into the surviving run.
        let mut older_runs = Vec::new();
        for slot in &mut self.levels[..level] {
            if let Some(older) = slot.take() {
                self.merge_input_bytes = self
                    .merge_input_bytes
                    .saturating_add(older.count * ROW_BYTES as u64);
                older_runs.push(older);
            }
        }
        self.merge_input_bytes = self
            .merge_input_bytes
            .saturating_add(run.count * ROW_BYTES as u64);
        for older in older_runs {
            // The merge output coexists with both inputs, so it is reserved
            // before it is written.
            let output_rows = older
                .count
                .checked_add(run.count)
                .ok_or(ContentError::LengthOverflow)?;
            self.reserve(
                output_rows
                    .checked_mul(ROW_BYTES as u64)
                    .ok_or(ContentError::LengthOverflow)?,
            )?;
            let backing = self
                .backing
                .as_deref_mut()
                .ok_or(ContentError::ResourceUnavailable {
                    what: "ordering backing",
                })?;
            run = merge_runs(backing, &older, &run, self.merge_buffer, &mut self.work)?;
            // The older input is dropped here: its bytes leave the owned set
            // with it, and the surviving run still counts the newer input.
            self.merge_input_bytes = self
                .merge_input_bytes
                .saturating_sub(older.count * ROW_BYTES as u64);
        }
        self.merge_input_bytes = self
            .merge_input_bytes
            .saturating_sub(run.count * ROW_BYTES as u64);
        // Obsolete tiers were dropped above, which returned their bytes and
        // removed their files; only the surviving run stays owned.
        for slot in &mut self.levels[..level] {
            *slot = None;
        }
        self.levels[level] = Some(run);
        self.reset_scans();
        self.pending_bytes = 0;
        self.work.peak_live_runs = self.work.peak_live_runs.max(self.live_runs());
        self.work.peak_run_bytes = self
            .work
            .peak_run_bytes
            .max(self.run_bytes() + self.pending_bytes);
        self.note_physical_peak();
        Ok(())
    }

    /// Finds the newest run row for `serial`, if any tier holds one.
    ///
    /// Each tier keeps the position its scan reached, so an ascending sweep reads
    /// a run once instead of re-reading it per serial. A request at or above the
    /// position continues the scan; a request below it starts the run again,
    /// because a scan that resumed past a row never compared it. Rows read here
    /// are charged to `rows_read`, which is what makes the reported work describe
    /// the operation instead of only its spills.
    pub fn find(&mut self, serial: u64) -> ContentResult<Option<Row>> {
        for index in 0..self.levels.len() {
            let Some(run) = self.levels[index].as_ref() else {
                continue;
            };
            if serial < run.first || serial > run.last {
                continue;
            }
            while self.scans.len() <= index {
                self.scans.push(LookupScan::default());
            }
            let scan = self.scans[index];
            let mut scan = if scan.total == 0 {
                LookupScan {
                    total: run.count,
                    ..scan
                }
            } else {
                scan
            };
            // A request the cursor has already passed needs this run from the
            // front: the rows in between were never compared with it. With no
            // recorded resume point the cursor cannot answer anything, so the scan
            // also starts where it can compare rows in order - which is the front.
            let from = match scan.resume {
                Some(resume) if serial >= resume => scan.offset,
                Some(_) => 0,
                None => 0,
            };
            let mut reader = RunReader::seek_from(run, self.merge_buffer, from)?;
            let mut found = None;
            let mut resume = None;
            while let Some(row) = reader.next()? {
                self.work.rows_read = self.work.rows_read.saturating_add(1);
                if row.serial() == serial {
                    found = Some(row);
                    break;
                }
                if row.serial() > serial {
                    // This row was not compared with the request: leave it at the
                    // cursor so a later request still sees it.
                    reader.rewind();
                    resume = Some(row.serial());
                    break;
                }
                resume = Some(row.serial().saturating_add(1));
            }
            if found.is_some() {
                // The run is sorted and holds one row per serial, so the row after
                // the one that matched is above this serial.
                resume = Some(serial.saturating_add(1));
            }
            scan.offset = reader.run_offset();
            scan.resume = resume;
            self.scans[index] = scan;
            if let Some(row) = found {
                return Ok(Some(row));
            }
        }
        Ok(None)
    }

    /// Merges every live run into the single newest level.
    ///
    /// Oldest-first order is what makes a newer row win a shared key; the result
    /// is installed at level zero, and a later spill still finds level one free
    /// and merges against it, so precedence is unchanged. With one live run this
    /// is a no-op and no bytes are rewritten.
    pub fn consolidate(&mut self) -> ContentResult<()> {
        if self.levels.iter().flatten().count() <= 1 {
            self.note_physical_peak();
            return Ok(());
        }
        // Every live run is moved out of the store, newest tier first, so each
        // input is dropped (its bytes and file returned) once the combined run has
        // consumed it. Iterating newest first is what keeps the merge honest: each
        // step merges one older tier *under* the rows the newer tiers already
        // contributed, so a newer row always wins a shared key.
        let mut sources = Vec::new();
        for index in 0..self.levels.len() {
            if let Some(run) = self.levels[index].take() {
                self.merge_input_bytes = self
                    .merge_input_bytes
                    .saturating_add(run.count * ROW_BYTES as u64);
                sources.push(run);
            }
        }
        let mut combined: Option<Run> = None;
        for run in sources {
            let consumed = run.count * ROW_BYTES as u64;
            let output_rows = run
                .count
                .checked_add(combined.as_ref().map_or(0, |run| run.count))
                .ok_or(ContentError::LengthOverflow)?;
            self.reserve(
                output_rows
                    .checked_mul(ROW_BYTES as u64)
                    .ok_or(ContentError::LengthOverflow)?,
            )?;
            let backing = self
                .backing
                .as_deref_mut()
                .ok_or(ContentError::ResourceUnavailable {
                    what: "ordering backing",
                })?;
            combined = match combined {
                None => Some(Run {
                    handle: copy_run(backing, &run, self.merge_buffer, &mut self.work)?,
                    count: run.count,
                    first: run.first,
                    last: run.last,
                }),
                Some(newer) => Some(merge_runs(
                    backing,
                    &run,
                    &newer,
                    self.merge_buffer,
                    &mut self.work,
                )?),
            };
            self.merge_input_bytes = self.merge_input_bytes.saturating_sub(consumed);
        }
        // Dropping the input tiers here returns their bytes; the consolidated run
        // is the only one left owned.
        self.levels.clear();
        self.reset_scans();
        if let Some(run) = combined {
            self.levels.push(Some(run));
        }
        self.work.peak_live_runs = self.work.peak_live_runs.max(self.live_runs());
        self.work.peak_run_bytes = self.work.peak_run_bytes.max(self.run_bytes());
        self.note_physical_peak();
        Ok(())
    }

    /// The single live run, when the store holds exactly one.
    pub fn single_run(&self) -> Option<&Run> {
        let mut live = self.levels.iter().flatten();
        let first = live.next()?;
        live.next().is_none().then_some(first)
    }

    /// Reads every live run with newest-row precedence, without consuming it.
    ///
    /// Levels are scanned newest first, so a serial that two levels hold is
    /// emitted once, from the newer one, and the older duplicate is skipped. One
    /// reader per live tier is bounded by the declared merge buffer.
    pub fn visit_newest_first(
        &self,
        mut visitor: impl FnMut(Row) -> ContentResult<bool>,
    ) -> ContentResult<()> {
        let mut readers = self
            .levels
            .iter()
            .flatten()
            .map(|run| RunReader::new(run, self.merge_buffer))
            .collect::<Vec<_>>();
        let mut current = readers
            .iter_mut()
            .map(|reader| reader.next())
            .collect::<ContentResult<Vec<_>>>()?;
        let mut last: Option<u64> = None;
        loop {
            let mut best: Option<(usize, u64)> = None;
            for (index, row) in current.iter().enumerate() {
                if let Some(row) = row {
                    let serial = row.serial();
                    match best {
                        None => best = Some((index, serial)),
                        Some((_, previous)) if serial < previous => best = Some((index, serial)),
                        _ => {}
                    }
                }
            }
            let Some((index, serial)) = best else {
                return Ok(());
            };
            let row = current[index].take().ok_or(ContentError::UnexpectedEof)?;
            current[index] = readers[index].next()?;
            if last == Some(serial) {
                continue;
            }
            last = Some(serial);
            if !visitor(row)? {
                return Ok(());
            }
        }
    }

    /// Moves the single live run's storage out of the store.
    ///
    /// The row stream keeps reading that run after the store and its backing are
    /// gone; on this profile the handle stays valid because the run is an open
    /// descriptor, not a pathname lookup.
    pub fn take_single_handle(
        &mut self,
    ) -> Option<Box<dyn crate::filesystem::references::backing::OrderingRun>> {
        let slot = self.levels.iter_mut().find(|slot| slot.is_some())?;
        let handle = slot.take().map(|run| run.handle);
        self.reset_scans();
        handle
    }

    /// Releases every run this store created.
    pub fn release(&mut self) -> ContentResult<()> {
        for slot in &mut self.levels {
            *slot = None;
        }
        self.reset_scans();
        match self.backing.as_deref_mut() {
            Some(backing) => backing.release(),
            None => Ok(()),
        }
    }
}

/// One tier's resumable lookup scan.
///
/// `find` is called once per touched serial, once per released child and again
/// for every row state the reducer samples. Each run is sorted and the reducer's
/// demands ascend, so the scan keeps its position: a request the tier already
/// settled is answered without a read, one beyond the position continues the
/// scan, and one behind it starts the scan again. That makes an ascending sweep
/// cost one pass over a run's rows instead of one pass per serial.
///
/// `offset` is where the scan stopped, which is always a row boundary, and
/// `resume` is the smallest serial that position may still answer. A reader over
/// the tier's run is built per call from `offset`; that rebuild is a real cost in
/// an ascending sweep and is recorded here rather than described as free.
///
/// A `settled` field used to sit beside these. It was never read, so the doc that
/// claimed a request below it "needs no read at all" described behaviour the code
/// did not have, and `resume` is the field that actually decides.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct LookupScan {
    /// File offset of the next row to examine.
    offset: u64,
    /// Smallest serial the cursor at `offset` may still answer. `None` means the
    /// row at the cursor was not read, so nothing below the cursor is safe.
    resume: Option<u64>,
    /// Rows in the run.
    total: u64,
}

/// Copies one run into a fresh handle so a merge never aliases its own input.
fn copy_run(
    backing: &mut dyn OrderingBacking,
    run: &Run,
    buffer_bytes: usize,
    work: &mut MergeWork,
) -> ContentResult<Box<dyn crate::filesystem::references::backing::OrderingRun>> {
    let mut handle = backing.create_run()?;
    work.runs_created = work.runs_created.saturating_add(1);
    let mut reader = RunReader::new(run, buffer_bytes);
    work.rows_read = work.rows_read.saturating_add(run.count);
    while let Some(row) = reader.next()? {
        handle.append(&row.encode()?)?;
        work.rows_written = work.rows_written.saturating_add(1);
    }
    handle.flush()?;
    Ok(handle)
}

/// Reads every live run with newest-row precedence, without consuming it.
///
/// Levels are scanned newest first, so a serial that two levels hold is emitted
/// once, from the newer one, and the older duplicate is skipped. Readers are
/// bounded: one per live tier, each with the declared merge buffer.
/// Reads every row of one run in serial order.
pub fn visit_run(
    run: &Run,
    buffer_bytes: usize,
    mut visitor: impl FnMut(Row) -> ContentResult<()>,
) -> ContentResult<()> {
    let mut reader = RunReader::new(run, buffer_bytes);
    while let Some(row) = reader.next()? {
        visitor(row)?;
    }
    Ok(())
}
