//! One bounded DirectoryRoots producer and its sealed point/cursor consumers.
use crate::error::{ContentError, ContentResult};
use crate::object::ObjectId;

use super::{
    GraphMemory, GraphMemoryLease, IndexedState, RootCursor, StateKey, StateLedger, StateRecord,
    StateScope, StateSeal, StateTable, STATE_MAX_PAGE_RECORDS,
};

pub(crate) struct DirectoryRoots {
    value: Option<Box<DirectoryRootsData>>,
    release_attempted: bool,
    _memory: Option<GraphMemoryLease>,
}
pub(crate) struct DirectoryRootsData {
    scope: StateScope,
    ledger: StateLedger,
    pending: Vec<StateRecord>,
    declared: u64,
    seal: Option<StateSeal>,
}
impl std::ops::Deref for DirectoryRoots {
    type Target = DirectoryRootsData;
    fn deref(&self) -> &Self::Target {
        self.value.as_ref().unwrap()
    }
}
impl std::ops::DerefMut for DirectoryRoots {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value.as_mut().unwrap()
    }
}
impl DirectoryRoots {
    pub(crate) fn new<S: IndexedState + ?Sized>(
        state: &mut S,
        scope: StateScope,
        declared: usize,
    ) -> ContentResult<Self> {
        Self::new_admitted(state, scope, declared, None)
    }
    pub(crate) fn new_with_memory<S: IndexedState + ?Sized>(
        state: &mut S,
        scope: StateScope,
        declared: usize,
        memory: GraphMemory,
    ) -> ContentResult<Self> {
        Self::new_admitted(state, scope, declared, Some(memory))
    }
    fn new_admitted<S: IndexedState + ?Sized>(
        state: &mut S,
        scope: StateScope,
        declared: usize,
        memory: Option<GraphMemory>,
    ) -> ContentResult<Self> {
        if scope.table() != StateTable::DirectoryRoots {
            return Err(ContentError::InvalidOrderingRecord("state table"));
        }
        state.capacity(&scope)?.check_requested(declared)?;
        let _memory = memory
            .map(|memory| memory.reserve(directory_roots_working_bytes(declared)))
            .transpose()?;
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
            value: Some(Box::new(DirectoryRootsData {
                ledger: StateLedger::new(scope.clone()),
                scope,
                pending,
                declared: u64::try_from(declared).map_err(|_| ContentError::LengthOverflow)?,
                seal: None,
            })),
            release_attempted: false,
            _memory,
        })
    }

    pub(crate) fn append<S: IndexedState + ?Sized>(
        &mut self,
        state: &mut S,
        serial: u64,
        root: ObjectId,
    ) -> ContentResult<()> {
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
            self.flush(state)?;
        }
        self.pending.push(record);
        Ok(())
    }

    fn flush<S: IndexedState + ?Sized>(&mut self, state: &mut S) -> ContentResult<()> {
        let data = self.value.as_mut().unwrap();
        if !data.pending.is_empty() {
            data.ledger.validate_append(&data.pending)?;
            state.append(&data.scope, &data.pending)?;
            data.ledger.acknowledge(&data.pending)?;
            data.pending.clear();
        }
        Ok(())
    }

    pub(crate) fn seal<S: IndexedState + ?Sized>(&mut self, state: &mut S) -> ContentResult<()> {
        self.flush(state)?;
        self.pending = Vec::new();
        if let Some(memory) = self._memory.as_mut() {
            let append = memory.bytes() - directory_roots_working_bytes(0);
            if append != 0 {
                drop(memory.split(append)?);
            }
        }
        let expected = self.ledger.seal();
        if state.seal(&self.scope)? != expected {
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

    pub(crate) fn scan(&self) -> ContentResult<RootCursor> {
        Ok(RootCursor::new(
            self.seal
                .as_ref()
                .ok_or(ContentError::InvalidOrderingRecord(
                    "directory roots unsealed",
                ))?
                .clone(),
        ))
    }
    pub(crate) fn get<S: IndexedState + ?Sized>(
        &mut self,
        state: &mut S,
        serial: u64,
    ) -> ContentResult<Option<ObjectId>> {
        let seal = self.known_seal()?.clone();
        let key = StateKey::directory_root(&self.scope, serial)?;
        let record = state.get(&seal, key)?;
        match record {
            Some(record) if record.key() != key => Err(ContentError::InvalidOrderingRecord(
                "directory roots point key",
            )),
            record => Ok(record.map(|record| record.root())),
        }
    }

    /// Logical completion only; native session cleanup remains its caller's job.
    pub(crate) fn release<S: IndexedState + ?Sized>(&mut self, state: &mut S) -> ContentResult<()> {
        if self.release_attempted {
            return Ok(());
        }
        self.release_attempted = true;
        let scope = self.scope.clone();
        self.value = None;
        self._memory = None;
        state.release(&scope)
    }
}

/// Actual fixed Roots coordinator plus its bounded source-order append capacity.
pub const fn directory_roots_working_bytes(declared: usize) -> usize {
    std::mem::size_of::<DirectoryRoots>()
        + std::mem::size_of::<DirectoryRootsData>()
        + if declared < 128 { declared } else { 128 } * std::mem::size_of::<StateRecord>()
}
/// Exact nonborrowing scan control, separate from each held returned page.
pub const fn directory_root_cursor_working_bytes() -> usize {
    std::mem::size_of::<RootCursor>()
}
