//! Reference effects: retained bindings, effect totals and final typed rows.
//!
//! New inodes take their count from the bindings the operation actually retained
//! for them, so an alias added in one directory and another in a second directory
//! reach the same count without a journal replay. Existing inodes keep their
//! stored count and take the signed effect the merge observed, so aliases outside
//! the changed paths survive untouched. Additions are observed before removals are
//! released, so a move never drops an inode to a spurious zero.
//!
//! Base records are read at the end, in bounded waves, only for the serials whose
//! effect rows need them; an inode whose count is unchanged and whose value was
//! not supplied produces no row at all.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::error::{ContentError, ContentResult};
use crate::filesystem::inode::read::{lookup_many, InodeReadWork, InodeTable};
use crate::filesystem::references::backing::OrderingBacking;
use crate::filesystem::references::record::{Row, ROW_BYTES};
use crate::filesystem::references::runs::{RunStore, DEFAULT_MERGE_BUFFER_BYTES};
use crate::object::inode_leaf::InodeValue;
use crate::object::AuthenticatedObjects;

/// Records the reducer may hold in memory before it must spill.
pub const DEFAULT_MAXIMUM_PENDING: usize = 4_096;
/// Base records one read wave may request.
pub const DEFAULT_BASE_BATCH: usize = 32;

/// Work the reducer performed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReferenceWork {
    /// Reference-row insertions and updates. This is work, not cardinality: a
    /// serial registered by the caller and then observed once counts twice.
    pub rows_touched: u64,
    /// Rows spilled to runs.
    pub rows_spilled: u64,
    /// Base records read.
    pub base_records_read: u64,
    /// Base read waves.
    pub base_waves: u64,
    /// Final inode values emitted.
    pub final_values: u64,
    /// Final removals emitted.
    pub final_removals: u64,
    /// Serials collected once, in order, for the zero-count scan. This is the
    /// operation's only touched-serial collection: one `u64` per touched inode.
    pub serials_scanned: u64,
    /// Largest simultaneous pending rows.
    pub peak_pending: usize,
    /// Merge and spill work.
    pub runs: crate::filesystem::references::merge::MergeWork,
}

/// Bounded pending state plus the tiered runs that back it.
pub struct ReferenceReducer<'r, 'b> {
    maximum_pending: usize,
    pending: BTreeMap<u64, Row>,
    declared_new: BTreeSet<u64>,
    runs: RunStore<'r, 'b>,
    work: ReferenceWork,
}

impl<'r, 'b> ReferenceReducer<'r, 'b> {
    /// A reducer bounded by `maximum_pending` in-memory rows.
    pub fn new(
        maximum_pending: usize,
        backing: Option<&'r mut (dyn OrderingBacking + 'b)>,
        merge_buffer: usize,
        ordering_bytes: u64,
    ) -> Self {
        Self {
            maximum_pending: maximum_pending.max(1),
            pending: BTreeMap::new(),
            declared_new: BTreeSet::new(),
            runs: RunStore::new(backing, merge_buffer.max(ROW_BYTES), ordering_bytes),
            work: ReferenceWork::default(),
        }
    }

    /// Work performed so far.
    pub fn work(&self) -> ReferenceWork {
        let mut work = self.work;
        work.runs = self.runs.work();
        work.peak_pending = self.work.peak_pending.max(self.pending.len());
        work
    }

    /// Declares one serial as newly allocated by the caller's allocator.
    pub fn declare_new(&mut self, serial: u64) -> ContentResult<()> {
        if serial == 0 {
            return Err(ContentError::InvalidRecord("inode serial"));
        }
        if self.declared_new.contains(&serial) {
            return Err(ContentError::InvalidRecord("duplicate new inode"));
        }
        self.declared_new.insert(serial);
        Ok(())
    }

    /// True when the caller declared this serial as newly allocated.
    pub fn is_new(&self, serial: u64) -> bool {
        self.declared_new.contains(&serial)
    }

    /// Records one retained final binding to `serial`.
    pub fn note_retained_binding(&mut self, serial: u64) -> ContentResult<()> {
        let row = self.entry(serial)?;
        match row {
            Row::Count { count, .. } => {
                *count = count.checked_add(1).ok_or(ContentError::LengthOverflow)?
            }
            Row::Effect { delta, .. } => {
                *delta = delta.checked_add(1).ok_or(ContentError::LengthOverflow)?
            }
        }
        Ok(())
    }

    /// Records that one binding to `serial` was removed by this operation.
    pub fn note_removed_binding(&mut self, serial: u64) -> ContentResult<()> {
        let row = self.entry(serial)?;
        match row {
            Row::Count { .. } => Err(ContentError::InvalidRecord("new inode removal")),
            Row::Effect { delta, .. } => {
                *delta = delta.checked_sub(1).ok_or(ContentError::LengthOverflow)?;
                Ok(())
            }
        }
    }

    /// Records the caller's typed final value for `serial`.
    pub fn note_value(&mut self, serial: u64, value: InodeValue) -> ContentResult<()> {
        let row = self.entry(serial)?;
        match row {
            Row::Count {
                value: existing, ..
            }
            | Row::Effect {
                value: existing, ..
            } => *existing = Some(value),
        }
        Ok(())
    }

    /// True when any pending or spilled row mentions `serial`.
    pub fn is_touched(&mut self, serial: u64) -> ContentResult<bool> {
        if self.pending.contains_key(&serial) {
            return Ok(true);
        }
        Ok(self.runs.find(serial)?.is_some())
    }

    /// The newest pending state of `serial`, without creating a row for it.
    pub fn state(&mut self, serial: u64) -> ContentResult<Option<PendingState>> {
        let row = match self.pending.get(&serial) {
            Some(row) => Some(*row),
            None => self.runs.find(serial)?,
        };
        Ok(row.map(|row| match row {
            Row::Count { value, count, .. } => PendingState::New { value, count },
            Row::Effect { value, delta, .. } => PendingState::Existing { value, delta },
        }))
    }

    /// Every serial this reducer holds a row for, in ascending order.
    ///
    /// One `u64` per touched inode, taken from the consolidated snapshot; the
    /// caller uses it to find the inodes whose derived count reached zero before
    /// the final stream is built.
    pub fn touched_serials(&mut self, _batch: usize) -> ContentResult<Vec<u64>> {
        self.runs.consolidate()?;
        let mut serials = Vec::new();
        let mut pending = self.pending.keys().copied().peekable();
        let mut last: Option<u64> = None;
        self.runs.visit_newest_first(|row| {
            let serial = row.serial();
            while let Some(next) = pending.peek().copied() {
                if next <= serial {
                    if last != Some(next) {
                        serials.push(next);
                        last = Some(next);
                    }
                    pending.next();
                } else {
                    break;
                }
            }
            if last != Some(serial) {
                serials.push(serial);
                last = Some(serial);
            }
            Ok(true)
        })?;
        for next in pending {
            if last != Some(next) {
                serials.push(next);
                last = Some(next);
            }
        }
        Ok(serials)
    }

    /// Charges the one collection the operation performs over touched serials.
    pub fn note_serials_scanned(&mut self, serials: u64) {
        self.work.serials_scanned = self.work.serials_scanned.saturating_add(serials);
    }

    /// Rows currently held in memory.
    pub fn pending_rows(&self) -> usize {
        self.pending.len()
    }

    /// Releases every run this reducer created.
    pub fn release(&mut self) -> ContentResult<()> {
        self.runs.release()
    }

    fn entry(&mut self, serial: u64) -> ContentResult<&mut Row> {
        if !self.pending.contains_key(&serial) {
            let row = match self.runs.find(serial)? {
                Some(row) => row,
                None if self.declared_new.contains(&serial) => Row::Count {
                    serial,
                    value: None,
                    count: 0,
                },
                None => Row::Effect {
                    serial,
                    value: None,
                    delta: 0,
                },
            };
            if self.pending.len() >= self.maximum_pending {
                self.runs.spill(&self.pending)?;
                self.work.rows_spilled = self
                    .work
                    .rows_spilled
                    .saturating_add(self.pending.len() as u64);
                self.pending.clear();
            }
            self.pending.insert(serial, row);
            self.work.rows_touched = self.work.rows_touched.saturating_add(1);
            self.work.peak_pending = self.work.peak_pending.max(self.pending.len());
        }
        self.pending
            .get_mut(&serial)
            .ok_or(ContentError::InvalidOrderingRecord("pending row"))
    }

    /// Streams the final typed rows in serial order.
    ///
    /// The pending map is merged with the spilled runs (pending is newest) and
    /// each effect row's base record is read in a bounded wave. The returned
    /// stream owns the final run and must be consumed before the operation ends.
    pub fn finish<'a>(
        &mut self,
        reader: &'a dyn AuthenticatedObjects,
        table: InodeTable,
        base_batch: usize,
        root_serial: u64,
    ) -> ContentResult<FinalRows<'a>> {
        self.runs.consolidate()?;
        let work = self.work();
        let pending = std::mem::take(&mut self.pending);
        let run_count = self.runs.single_run().map_or(0, |run| run.count);
        let handle = self.runs.take_single_handle();
        FinalRows::new(
            reader,
            table,
            pending,
            handle,
            run_count,
            base_batch.max(1),
            root_serial,
            work,
        )
    }
}

/// The newest known state of one inode in the reducer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PendingState {
    /// A newly allocated inode.
    New {
        /// Typed value, once supplied.
        value: Option<InodeValue>,
        /// Bindings retained so far.
        count: u64,
    },
    /// An existing inode.
    Existing {
        /// Typed value, when the caller supplied one.
        value: Option<InodeValue>,
        /// Signed effect total observed so far.
        delta: i64,
    },
}

/// One streamed final inode change.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FinalChange {
    /// Inode serial.
    pub serial: u64,
    /// Final typed value, or `None` when the inode is absent from the new tree.
    pub value: Option<InodeValue>,
}

/// The final typed rows of one operation, streamed in serial order.
pub struct FinalRows<'r> {
    reader: &'r dyn AuthenticatedObjects,
    table: InodeTable,
    memory: std::vec::IntoIter<Row>,
    memory_next: Option<Row>,
    run: Option<Box<dyn crate::filesystem::references::backing::OrderingRun>>,
    run_remaining: u64,
    run_offset: u64,
    run_buffer: Vec<u8>,
    run_filled: usize,
    run_consumed: usize,
    run_next: Option<Row>,
    lookahead: VecDeque<Row>,
    base_batch: usize,
    root_serial: u64,
    work: ReferenceWork,
    finished: bool,
}

impl<'r> FinalRows<'r> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        reader: &'r dyn AuthenticatedObjects,
        table: InodeTable,
        pending: BTreeMap<u64, Row>,
        run: Option<Box<dyn crate::filesystem::references::backing::OrderingRun>>,
        run_rows: u64,
        base_batch: usize,
        root_serial: u64,
        work: ReferenceWork,
    ) -> ContentResult<Self> {
        let mut rows = Self {
            reader,
            table,
            memory: pending.into_values().collect::<Vec<_>>().into_iter(),
            memory_next: None,
            run: None,
            run_remaining: 0,
            run_offset: 0,
            run_buffer: Vec::new(),
            run_filled: 0,
            run_consumed: 0,
            run_next: None,
            lookahead: VecDeque::new(),
            base_batch,
            root_serial,
            work,
            finished: false,
        };
        if let Some(handle) = run {
            rows.run_remaining = run_rows;
            rows.run_buffer = vec![0; base_batch.max(1) * ROW_BYTES * 4];
            rows.run = Some(handle);
        }
        rows.memory_next = rows.memory.next();
        rows.run_next = rows.read_run_row()?;
        Ok(rows)
    }

    /// Work performed so far.
    pub const fn work(&self) -> ReferenceWork {
        self.work
    }

    /// Next final change in serial order, or `None` at the end.
    pub fn next_change(&mut self) -> ContentResult<Option<FinalChange>> {
        if self.finished {
            return Ok(None);
        }
        if self.lookahead.is_empty() && !self.fill_wave()? {
            self.finished = true;
            return Ok(None);
        }
        let row = self
            .lookahead
            .pop_front()
            .ok_or(ContentError::InvalidOrderingRecord("wave"))?;
        self.finish_row(row)
    }

    /// Reads up to one batch of effective rows into the lookahead queue.
    fn fill_wave(&mut self) -> ContentResult<bool> {
        let mut wave = Vec::with_capacity(self.base_batch);
        while wave.len() < self.base_batch {
            let next = match (&self.memory_next, &self.run_next) {
                (Some(memory), Some(run)) => {
                    if memory.serial() <= run.serial() {
                        self.memory_next.take()
                    } else {
                        self.run_next.take()
                    }
                }
                (Some(_), None) => self.memory_next.take(),
                (None, Some(_)) => self.run_next.take(),
                (None, None) => None,
            };
            let Some(row) = next else {
                break;
            };
            if self.memory_next.is_none() {
                self.memory_next = self.memory.next();
            }
            // Pending rows are newer than every run row; a run row for the same
            // serial is superseded and must not be yielded again.
            if self
                .run_next
                .as_ref()
                .is_some_and(|run| run.serial() == row.serial())
            {
                self.run_next = self.read_run_row()?;
            }
            if self.run_next.is_none() {
                self.run_next = self.read_run_row()?;
            }
            wave.push(row);
        }
        if wave.is_empty() {
            return Ok(false);
        }
        // One bounded authenticated wave supplies every base record this batch
        // needs; rows that carry a typed value need no base record at all.
        // Every effect row needs its stored base count, because the caller's
        // typed value is never trusted for the reference count.
        let mut serials = Vec::new();
        for row in &wave {
            if matches!(row, Row::Effect { .. }) {
                serials.push(row.serial());
            }
        }
        let mut bases = Vec::new();
        if !serials.is_empty() {
            bases = lookup_many(
                self.reader,
                self.table,
                &serials,
                &mut InodeReadWork::default(),
            )?;
            self.work.base_records_read = self
                .work
                .base_records_read
                .saturating_add(serials.len() as u64);
            self.work.base_waves = self.work.base_waves.saturating_add(1);
        }
        let mut base_iter = bases.into_iter();
        for row in wave {
            let base = if matches!(row, Row::Effect { .. }) {
                base_iter.next().flatten()
            } else {
                None
            };
            self.lookahead.push_back(row);
            let position = self.lookahead.len() - 1;
            if let Row::Effect {
                serial,
                value,
                delta,
            } = self.lookahead[position]
            {
                // The stored record supplies the base count; a supplied typed
                // value supplies kind, content and metadata only.
                let base = base.ok_or(ContentError::InvalidRecord("effect inode record"))?;
                let merged = InodeValue {
                    kind: value.map_or(base.kind, |value| value.kind),
                    namespace_ref_count: base.namespace_ref_count,
                    content_root: value.map_or(base.content_root, |value| value.content_root),
                    metadata_root: value.map_or(base.metadata_root, |value| value.metadata_root),
                };
                self.lookahead[position] = Row::Effect {
                    serial,
                    value: Some(merged),
                    delta,
                };
            }
        }
        Ok(true)
    }

    fn finish_row(&mut self, row: Row) -> ContentResult<Option<FinalChange>> {
        match row {
            Row::Count {
                serial,
                value,
                count,
            } => {
                if count == 0 && serial != self.root_serial {
                    // A newly allocated identity with no retained binding is a
                    // disconnected record, not a silent removal. The root
                    // directory is the one inode whose count is zero by contract.
                    return Err(ContentError::InvalidRecord("new inode without binding"));
                }
                let value = value.ok_or(ContentError::InvalidRecord("new inode value"))?;
                self.work.final_values = self.work.final_values.saturating_add(1);
                Ok(Some(FinalChange {
                    serial,
                    value: Some(InodeValue {
                        namespace_ref_count: count,
                        ..value
                    }),
                }))
            }
            Row::Effect {
                serial,
                value,
                delta,
            } => {
                let base = value.ok_or(ContentError::InvalidRecord("effect inode record"))?;
                let count = i128::from(base.namespace_ref_count) + i128::from(delta);
                if count <= 0 && serial == self.root_serial {
                    // The root directory is the one inode whose count is zero by
                    // contract; it is retained, never released.
                    self.work.final_values = self.work.final_values.saturating_add(1);
                    return Ok(Some(FinalChange {
                        serial,
                        value: Some(InodeValue {
                            namespace_ref_count: 0,
                            ..base
                        }),
                    }));
                }
                if count <= 0 {
                    self.work.final_removals = self.work.final_removals.saturating_add(1);
                    return Ok(Some(FinalChange {
                        serial,
                        value: None,
                    }));
                }
                self.work.final_values = self.work.final_values.saturating_add(1);
                Ok(Some(FinalChange {
                    serial,
                    value: Some(InodeValue {
                        namespace_ref_count: count as u64,
                        ..base
                    }),
                }))
            }
        }
    }

    fn read_run_row(&mut self) -> ContentResult<Option<Row>> {
        if self.run_remaining == 0 {
            return Ok(None);
        }
        if self.run_consumed == self.run_filled {
            let rows = self
                .run_remaining
                .min((self.run_buffer.len() / ROW_BYTES) as u64) as usize;
            let filled = rows * ROW_BYTES;
            let handle = self
                .run
                .as_ref()
                .ok_or(ContentError::InvalidOrderingRecord("run handle"))?;
            handle.read_at(self.run_offset, &mut self.run_buffer[..filled])?;
            self.run_consumed = 0;
            self.run_filled = filled;
        }
        let bytes: &[u8; ROW_BYTES] = self.run_buffer
            [self.run_consumed..self.run_consumed + ROW_BYTES]
            .try_into()
            .map_err(|_| ContentError::UnexpectedEof)?;
        let row = Row::decode(bytes)?;
        self.run_consumed += ROW_BYTES;
        self.run_offset += ROW_BYTES as u64;
        self.run_remaining -= 1;
        Ok(Some(row))
    }
}

/// The default merge buffer for one reducer's runs.
pub const fn default_merge_buffer() -> usize {
    DEFAULT_MERGE_BUFFER_BYTES
}
