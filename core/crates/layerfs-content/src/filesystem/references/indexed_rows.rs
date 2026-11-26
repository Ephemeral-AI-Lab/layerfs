//! Non-destructive sealed membership passes; final rows restart after release.
use super::reduce::{FinalChange, ReferenceWork};
use super::{
    indexed::{IndexedReducer, PASS_ROWS},
    indexed_wire as wire, meaning,
    record::Row,
};
use crate::filesystem::inode::read::{lookup_many, InodeReadWork, InodeTable};
use crate::{AuthenticatedObjects, ContentError, ContentResult};
use std::collections::VecDeque;

/// Most final rows held before the first of them is emitted. Each level of
/// their base lookup reads at most 128 canonical pages in one demand.
const FINAL_READ_ROWS: usize = 128;

pub(super) struct RowCursor {
    after: Option<[u8; 32]>,
    keys: std::vec::IntoIter<[u8; 32]>,
    seen: u64,
    /// The window in hand held fewer than 64 keys: no key follows it.
    short: bool,
    finished: bool,
}
impl RowCursor {
    pub fn new() -> Self {
        Self {
            after: None,
            keys: Vec::new().into_iter(),
            seen: 0,
            short: false,
            finished: false,
        }
    }
    /// Reads whole backing waves into one window for a shared base lookup.
    /// Both passes have sealed touched membership. The seed writes only work
    /// items and nodes, so those writes do not change the rows read here.
    pub fn window(
        &mut self,
        reducer: &IndexedReducer<'_, '_>,
        batch: usize,
        limit: usize,
    ) -> ContentResult<Vec<Row>> {
        let capacity = limit / batch * batch;
        let mut rows = Vec::with_capacity(capacity);
        while rows.len() < capacity {
            let wave = self.wave(reducer, batch)?;
            let short = wave.len() < batch;
            rows.extend(wave);
            if short {
                break;
            }
        }
        Ok(rows)
    }
    /// The next rows of the pass, at most `limit` of them and none once it
    /// has ended. The rows of one job are read together; no row beyond the
    /// wave is read, so a caller that writes between waves is never behind.
    fn wave(&mut self, reducer: &IndexedReducer<'_, '_>, limit: usize) -> ContentResult<Vec<Row>> {
        let mut rows = Vec::with_capacity(limit.min(64));
        while rows.len() < limit && !self.finished {
            if self.keys.len() == 0 {
                // A window of fewer than 64 keys held every remaining key, so
                // the pass ends on it without asking for one more.
                let keys = if self.short {
                    Vec::new()
                } else {
                    reducer.state.keys_after(wire::TOUCH, self.after)?
                };
                self.short = keys.len() < 64;
                if keys.is_empty() {
                    if self.seen != reducer.touched {
                        return Err(ContentError::InvalidRecord("filesystem touched membership"));
                    }
                    self.finished = true;
                    break;
                }
                for key in &keys {
                    wire::serial(*key)?;
                }
                self.keys = keys.into_iter();
            }
            let take = (limit - rows.len()).min(PASS_ROWS).min(self.keys.len());
            let mut serials = [0_u64; PASS_ROWS];
            let mut last = self.after;
            for slot in &mut serials[..take] {
                let key = self
                    .keys
                    .next()
                    .ok_or(ContentError::InvalidRecord("filesystem touched window"))?;
                *slot = wire::serial(key)?;
                last = Some(key);
            }
            for row in reducer.required_many(&serials[..take])? {
                self.seen = self
                    .seen
                    .checked_add(1)
                    .ok_or(ContentError::LengthOverflow)?;
                if self.seen > reducer.touched {
                    return Err(ContentError::InvalidRecord("filesystem touched membership"));
                }
                rows.push(row);
            }
            self.after = last;
        }
        Ok(rows)
    }
}

pub(super) struct IndexedRows<'s, 'b, 'r> {
    reducer: &'s IndexedReducer<'s, 'b>,
    cursor: RowCursor,
    reader: &'r dyn AuthenticatedObjects,
    table: InodeTable,
    batch: usize,
    root: u64,
    rows: VecDeque<Row>,
    work: ReferenceWork,
    terminal: bool,
}
impl<'s, 'b, 'r> IndexedRows<'s, 'b, 'r> {
    pub fn new(
        reducer: &'s IndexedReducer<'s, 'b>,
        reader: &'r dyn AuthenticatedObjects,
        table: InodeTable,
        batch: usize,
        root: u64,
    ) -> Self {
        Self {
            reducer,
            cursor: RowCursor::new(),
            reader,
            table,
            batch: batch.clamp(1, 64),
            root,
            rows: VecDeque::new(),
            work: reducer.work,
            terminal: false,
        }
    }
    pub fn work(&self) -> ReferenceWork {
        self.work
    }
    pub fn next_change(&mut self) -> ContentResult<Option<FinalChange>> {
        if self.terminal {
            return Ok(None);
        }
        let result = self.next_inner();
        if result.as_ref().map_or(true, |row| row.is_none()) {
            self.terminal = true;
        }
        result
    }
    fn next_inner(&mut self) -> ContentResult<Option<FinalChange>> {
        if self.rows.is_empty() {
            let window = self
                .cursor
                .window(self.reducer, self.batch, FINAL_READ_ROWS)?;
            if window.is_empty() {
                return Ok(None);
            }
            let serials = window
                .iter()
                .filter(|row| matches!(row, Row::Effect { .. }))
                .map(|row| row.serial())
                .collect::<Vec<_>>();
            let bases = lookup_many(
                self.reader,
                self.table,
                &serials,
                &mut InodeReadWork::default(),
            )?;
            if !serials.is_empty() {
                self.work.base_records_read = self
                    .work
                    .base_records_read
                    .saturating_add(serials.len() as u64);
                self.work.base_waves = self.work.base_waves.saturating_add(1);
            }
            let mut bases = bases.into_iter();
            for row in window {
                let base = if matches!(row, Row::Effect { .. }) {
                    bases.next().flatten()
                } else {
                    None
                };
                self.rows.push_back(meaning::with_base(row, base)?);
            }
        }
        let row = self
            .rows
            .pop_front()
            .ok_or(ContentError::InvalidRecord("filesystem final wave"))?;
        Ok(Some(meaning::finish(row, self.root, &mut self.work)?))
    }
}
