//! Prospectively allocated resident compatibility for the same exact table.
use crate::error::{ContentError, ContentResult};

use super::{
    IndexedState, PageLimit, StateCapacity, StateKey, StateLedger, StatePage, StateRecord,
    StateScope, StateSeal, StateTable, DIRECTORY_ROOT_RECORD_BYTES,
};

/// Explicitly admitted logical compatibility table with fallible native storage.
/// Its population capacity is caller declared and makes no physical-memory claim.
pub struct ResidentState {
    scope: StateScope,
    capacity: StateCapacity,
    records: Vec<StateRecord>,
    ledger: StateLedger,
    seal: Option<StateSeal>,
    released: bool,
}

impl ResidentState {
    /// Exact caller-declared compatibility shape; no new 64 KiB population clamp.
    pub fn new(scope: StateScope, maximum_records: usize) -> ContentResult<Self> {
        if scope.table() != StateTable::DirectoryRoots {
            return Err(ContentError::InvalidOrderingRecord("state table"));
        }
        let count = u64::try_from(maximum_records).map_err(|_| ContentError::LengthOverflow)?;
        let bytes = count
            .checked_mul(DIRECTORY_ROOT_RECORD_BYTES as u64)
            .ok_or(ContentError::LengthOverflow)?;
        let mut records = Vec::new();
        records.try_reserve_exact(maximum_records).map_err(|_| {
            ContentError::ResourceUnavailable {
                what: "indexed_state.resident_records",
            }
        })?;
        if records.capacity() > maximum_records {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "indexed_state.resident_capacity",
                limit: count,
                actual: records.capacity() as u64,
            });
        }
        Ok(Self {
            ledger: StateLedger::new(scope.clone()),
            scope,
            capacity: StateCapacity::new(count, bytes)?,
            records,
            seal: None,
            released: false,
        })
    }

    fn check_scope(&self, scope: &StateScope) -> ContentResult<()> {
        if self.released || scope != &self.scope {
            return Err(ContentError::InvalidOrderingRecord("state authority"));
        }
        Ok(())
    }

    fn check_seal(&self, seal: &StateSeal) -> ContentResult<()> {
        self.check_scope(seal.scope())?;
        if self.seal.as_ref() != Some(seal) {
            return Err(ContentError::InvalidOrderingRecord(
                "state sealed selection",
            ));
        }
        Ok(())
    }
}

impl IndexedState for ResidentState {
    fn capacity(&self, scope: &StateScope) -> ContentResult<StateCapacity> {
        self.check_scope(scope)?;
        Ok(self.capacity)
    }

    fn append(&mut self, scope: &StateScope, records: &[StateRecord]) -> ContentResult<()> {
        self.check_scope(scope)?;
        if self.seal.is_some() {
            return Err(ContentError::InvalidOrderingRecord("state sealed mutation"));
        }
        self.ledger.validate_append(records)?;
        let count = self
            .records
            .len()
            .checked_add(records.len())
            .ok_or(ContentError::LengthOverflow)?;
        self.capacity.check_requested(count)?;
        // The exact resident capacity was acquired before canonical effects.
        self.records.extend_from_slice(records);
        self.ledger.acknowledge(records)
    }

    fn seal(&mut self, scope: &StateScope) -> ContentResult<StateSeal> {
        self.check_scope(scope)?;
        if self.seal.is_some() {
            return Err(ContentError::InvalidOrderingRecord("state already sealed"));
        }
        let seal = self.ledger.seal();
        self.seal = Some(seal.clone());
        Ok(seal)
    }

    fn get(&mut self, seal: &StateSeal, key: StateKey) -> ContentResult<Option<StateRecord>> {
        self.check_seal(seal)?;
        StateKey::decode(&self.scope, key.as_bytes())?;
        Ok(self
            .records
            .binary_search_by_key(&key, |record| record.key())
            .ok()
            .map(|index| self.records[index]))
    }

    fn page(
        &mut self,
        seal: &StateSeal,
        after: Option<StateKey>,
        limit: PageLimit,
    ) -> ContentResult<StatePage> {
        self.check_seal(seal)?;
        if let Some(key) = after {
            StateKey::decode(&self.scope, key.as_bytes())?;
        }
        let start = self
            .records
            .partition_point(|record| after.is_some_and(|key| record.key() <= key));
        let available = self.records.len() - start;
        if available != 0 && limit.fitting_records() == 0 {
            limit.check_records(1)?;
        }
        let count = available.min(limit.fitting_records());
        let mut records = Vec::new();
        records
            .try_reserve_exact(count)
            .map_err(|_| ContentError::ResourceUnavailable {
                what: "indexed_state.resident_page",
            })?;
        records.extend_from_slice(&self.records[start..start + count]);
        let page = StatePage::after(seal.clone(), after, records, count == available)?;
        page.check_limit(limit)?;
        Ok(page)
    }

    fn release(&mut self, scope: &StateScope) -> ContentResult<()> {
        if scope != &self.scope {
            return Err(ContentError::InvalidOrderingRecord(
                "state release selection",
            ));
        }
        if !self.released {
            self.released = true;
            self.records = Vec::new();
        }
        Ok(())
    }
}
