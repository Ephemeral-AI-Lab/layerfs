//! One bounded DirectoryRoots producer and its sealed point/cursor consumers.
use crate::error::{ContentError, ContentResult};
use crate::object::ObjectId;

use super::{
    IndexedState, StateCursor, StateKey, StateLedger, StateRecord, StateScope, StateSeal,
    STATE_MAX_PAGE_RECORDS,
};

pub(crate) struct DirectoryRoots<'a> {
    state: &'a mut dyn IndexedState,
    scope: StateScope,
    ledger: StateLedger,
    pending: Vec<StateRecord>,
    declared: u64,
    seal: Option<StateSeal>,
    release_attempted: bool,
}

impl<'a> DirectoryRoots<'a> {
    pub(crate) fn new(
        state: &'a mut dyn IndexedState,
        scope: StateScope,
        declared: usize,
    ) -> ContentResult<Self> {
        state.capacity(&scope)?.check_requested(declared)?;
        let mut pending = Vec::new();
        pending
            .try_reserve_exact(declared.min(STATE_MAX_PAGE_RECORDS))
            .map_err(|_| ContentError::ResourceUnavailable {
                what: "indexed_state.directory_root_window",
            })?;
        if pending.capacity() > declared.min(STATE_MAX_PAGE_RECORDS) {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "indexed_state.directory_root_capacity",
                limit: declared.min(STATE_MAX_PAGE_RECORDS) as u64,
                actual: pending.capacity() as u64,
            });
        }
        Ok(Self {
            state,
            ledger: StateLedger::new(scope.clone()),
            scope,
            pending,
            declared: u64::try_from(declared).map_err(|_| ContentError::LengthOverflow)?,
            seal: None,
            release_attempted: false,
        })
    }

    pub(crate) fn append(&mut self, serial: u64, root: ObjectId) -> ContentResult<()> {
        if self.seal.is_some() || self.release_attempted {
            return Err(ContentError::InvalidOrderingRecord(
                "directory roots closed",
            ));
        }
        let actual = self
            .ledger
            .records()
            .checked_add(self.pending.len() as u64)
            .and_then(|count| count.checked_add(1))
            .ok_or(ContentError::LengthOverflow)?;
        if actual > self.declared {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "indexed_state.declared_records",
                limit: self.declared,
                actual,
            });
        }
        let record = StateRecord::directory_root(&self.scope, serial, root)?;
        let last = self
            .pending
            .last()
            .map(|record| record.key())
            .or(self.ledger.last());
        if last.is_some_and(|last| last >= record.key()) {
            return Err(ContentError::InvalidOrderingRecord("directory roots order"));
        }
        if self.pending.len() == STATE_MAX_PAGE_RECORDS {
            self.flush()?;
        }
        self.pending.push(record);
        Ok(())
    }

    fn flush(&mut self) -> ContentResult<()> {
        if !self.pending.is_empty() {
            self.ledger.validate_append(&self.pending)?;
            self.state.append(&self.scope, &self.pending)?;
            self.ledger.acknowledge(&self.pending)?;
            self.pending.clear();
        }
        Ok(())
    }

    pub(crate) fn seal(&mut self) -> ContentResult<()> {
        self.flush()?;
        self.pending = Vec::new();
        let expected = self.ledger.seal();
        if self.state.seal(&self.scope)? != expected {
            return Err(ContentError::InvalidOrderingRecord("directory roots seal"));
        }
        self.seal = Some(expected);
        Ok(())
    }

    fn known_seal(&self) -> ContentResult<&StateSeal> {
        self.seal
            .as_ref()
            .ok_or(ContentError::InvalidOrderingRecord(
                "directory roots unsealed",
            ))
    }

    pub(crate) fn cursor(&mut self) -> ContentResult<StateCursor<'_>> {
        let seal = self.known_seal()?.clone();
        Ok(StateCursor::new(self.state, seal))
    }

    pub(crate) fn get(&mut self, serial: u64) -> ContentResult<Option<ObjectId>> {
        let seal = self.known_seal()?.clone();
        let key = StateKey::directory_root(&self.scope, serial)?;
        let record = self.state.get(&seal, key)?;
        match record {
            Some(record) if record.key() != key => Err(ContentError::InvalidOrderingRecord(
                "directory roots point key",
            )),
            record => Ok(record.map(|record| record.root())),
        }
    }

    /// Logical completion only; native session cleanup remains its caller's job.
    pub(crate) fn release(&mut self) -> ContentResult<()> {
        if self.release_attempted {
            return Ok(());
        }
        self.release_attempted = true;
        self.pending = Vec::new();
        self.state.release(&self.scope)
    }
}
