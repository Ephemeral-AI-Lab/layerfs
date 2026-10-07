//! Non-destructive sealed membership passes; final rows restart after release.
use super::reduce::{FinalChange, ReferenceWork};
use super::{indexed::IndexedReducer, indexed_wire as wire, meaning, record::Row};
use crate::filesystem::inode::read::{lookup_many, InodeReadWork, InodeTable};
use crate::{AuthenticatedObjects, ContentError, ContentResult};
use std::collections::VecDeque;

pub(super) struct RowCursor {
    after: Option<[u8; 32]>,
    keys: std::vec::IntoIter<[u8; 32]>,
    seen: u64,
    finished: bool,
}
impl RowCursor {
    pub fn new() -> Self {
        Self {
            after: None,
            keys: Vec::new().into_iter(),
            seen: 0,
            finished: false,
        }
    }
    pub fn next(&mut self, reducer: &IndexedReducer<'_, '_>) -> ContentResult<Option<Row>> {
        if self.finished {
            return Ok(None);
        }
        if self.keys.len() == 0 {
            let keys = reducer.state.keys_after(wire::TOUCH, self.after)?;
            if keys.is_empty() {
                if self.seen != reducer.touched {
                    return Err(ContentError::InvalidRecord("filesystem touched membership"));
                }
                self.finished = true;
                return Ok(None);
            }
            for key in &keys {
                wire::serial(*key)?;
            }
            self.keys = keys.into_iter();
        }
        let key = self
            .keys
            .next()
            .ok_or(ContentError::InvalidRecord("filesystem touched window"))?;
        let row = reducer.required(wire::serial(key)?)?;
        self.seen = self
            .seen
            .checked_add(1)
            .ok_or(ContentError::LengthOverflow)?;
        if self.seen > reducer.touched {
            return Err(ContentError::InvalidRecord("filesystem touched membership"));
        }
        self.after = Some(key);
        Ok(Some(row))
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
            let mut wave = Vec::with_capacity(self.batch);
            while wave.len() < self.batch {
                match self.cursor.next(self.reducer)? {
                    Some(row) => wave.push(row),
                    None => break,
                }
            }
            if wave.is_empty() {
                return Ok(None);
            }
            let serials = wave
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
            for row in wave {
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
