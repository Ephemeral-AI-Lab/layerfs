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

use std::collections::BTreeMap;

use crate::error::{ContentError, ContentResult};
use crate::filesystem::references::backing::OrderingBacking;
use crate::filesystem::references::merge::{merge_runs, MergeWork, Run, RunReader};
use crate::filesystem::references::record::{Row, ROW_BYTES};

/// Default bytes of one merge buffer.
pub const DEFAULT_MERGE_BUFFER_BYTES: usize = 16 * 1024;
/// Largest tier index the store can address.
pub const MAXIMUM_LEVELS: usize = 32;

/// Tiered runs over one caller-supplied backing.
pub struct RunStore<'a> {
    backing: Option<&'a mut dyn OrderingBacking>,
    levels: Vec<Option<Run>>,
    merge_buffer: usize,
    work: MergeWork,
}

impl<'a> RunStore<'a> {
    /// A store over optional `backing` with a bounded merge buffer.
    ///
    /// Without backing the store can still answer lookups and merges for state a
    /// caller holds in memory; the first spill fails explicitly, because growing
    /// without a declared owner is not a fallback.
    pub fn new(backing: Option<&'a mut dyn OrderingBacking>, merge_buffer: usize) -> Self {
        Self {
            backing,
            levels: Vec::new(),
            merge_buffer: merge_buffer.max(ROW_BYTES),
            work: MergeWork::default(),
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
    fn backing(&mut self) -> ContentResult<&mut (dyn OrderingBacking + 'a)> {
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
        for older in self.levels[..level].iter().flatten() {
            let backing = self
                .backing
                .as_deref_mut()
                .ok_or(ContentError::ResourceUnavailable {
                    what: "ordering backing",
                })?;
            run = merge_runs(backing, older, &run, self.merge_buffer, &mut self.work)?;
        }
        for slot in &mut self.levels[..level] {
            *slot = None;
        }
        self.levels[level] = Some(run);
        self.work.peak_live_runs = self.work.peak_live_runs.max(self.live_runs());
        self.work.peak_run_bytes = self.work.peak_run_bytes.max(self.run_bytes());
        Ok(())
    }

    /// Finds the newest run row for `serial`, if any tier holds one.
    pub fn find(&self, serial: u64) -> ContentResult<Option<Row>> {
        for run in self.levels.iter().flatten() {
            if serial < run.first || serial > run.last {
                continue;
            }
            let mut reader = RunReader::new(run, self.merge_buffer);
            while let Some(row) = reader.next()? {
                if row.serial() == serial {
                    return Ok(Some(row));
                }
                if row.serial() > serial {
                    break;
                }
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
            return Ok(());
        }
        let mut sources = self
            .levels
            .iter()
            .enumerate()
            .filter_map(|(index, run)| run.as_ref().map(|run| (index, run)))
            .collect::<Vec<_>>();
        sources.sort_by_key(|(index, _)| std::cmp::Reverse(*index));
        let mut combined: Option<Run> = None;
        for (_, run) in sources {
            let backing = self
                .backing
                .as_deref_mut()
                .ok_or(ContentError::ResourceUnavailable {
                    what: "ordering backing",
                })?;
            combined = match combined {
                None => Some(Run {
                    handle: copy_run(backing, run, self.merge_buffer, &mut self.work)?,
                    count: run.count,
                    first: run.first,
                    last: run.last,
                }),
                Some(newer) => Some(merge_runs(
                    backing,
                    run,
                    &newer,
                    self.merge_buffer,
                    &mut self.work,
                )?),
            };
        }
        self.levels.clear();
        if let Some(run) = combined {
            self.levels.push(Some(run));
        }
        self.work.peak_live_runs = self.work.peak_live_runs.max(self.live_runs());
        self.work.peak_run_bytes = self.work.peak_run_bytes.max(self.run_bytes());
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
        slot.take().map(|run| run.handle)
    }

    /// Releases every run this store created.
    pub fn release(&mut self) -> ContentResult<()> {
        for slot in &mut self.levels {
            *slot = None;
        }
        match self.backing.as_deref_mut() {
            Some(backing) => backing.release(),
            None => Ok(()),
        }
    }
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
