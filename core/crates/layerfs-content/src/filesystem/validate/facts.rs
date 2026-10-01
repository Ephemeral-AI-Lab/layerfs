//! Existing admitted positive/absence memo and semantic lookup accounting.
use super::{charge_inode, ValidationWork, ALLOCATION_CHECK_BATCH};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::inode::read::{lookup_many, lookup_many_owned, InodeReadWork, InodeTable};
use crate::filesystem::state::{BaseFact, FactScope, FactState, GraphMemory, GraphMemoryLease};
use crate::object::inode_leaf::InodeValue;
use crate::object::{AuthenticatedObjects, CanonicalBudget};
use std::collections::{BTreeMap, BTreeSet};

/// Memoizes authenticated base records and absence within a resource-sized
/// window. Eviction changes physical reads, never the logical demand charge.
pub(super) struct ValidationState {
    records: BTreeMap<u64, InodeValue>,
    absent: BTreeSet<u64>,
    limit: usize,
    hot: Option<HotOwner>,
}

impl ValidationState {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            records: BTreeMap::new(),
            absent: BTreeSet::new(),
            limit: limit.max(1),
            hot: None,
        }
    }

    fn make_room(&mut self) {
        if self.records.len() + self.absent.len() >= self.limit {
            self.records.clear();
            self.absent.clear();
        }
    }

    pub(super) fn known(&self, serial: u64) -> bool {
        self.records.contains_key(&serial) || self.absent.contains(&serial)
    }

    /// One checked <=64 missing slice. Logical demand charges stay at semantic sites.
    pub(super) fn prefetch_wave(
        &mut self,
        reader: &dyn AuthenticatedObjects,
        table: InodeTable,
        serials: &[u64],
        work: &mut ValidationWork,
    ) -> ContentResult<()> {
        if serials.len() > ALLOCATION_CHECK_BATCH {
            return Err(ContentError::ObjectLimitExceeded {
                limit: ALLOCATION_CHECK_BATCH,
                actual: serials.len(),
            });
        }
        if serials.is_empty() {
            return Ok(());
        }
        work.prefetch.lookup_calls = work.prefetch.lookup_calls.saturating_add(1);
        work.prefetch.submitted_serials = work
            .prefetch
            .submitted_serials
            .saturating_add(serials.len() as u64);
        work.prefetch.max_missing = work.prefetch.max_missing.max(serials.len() as u64);
        let mut inode = InodeReadWork::default();
        let outcome = lookup_many(reader, table, serials, &mut inode);
        // Successful acquisition/decode work remains visible on a later error.
        work.read_waves = work.read_waves.saturating_add(inode.read_waves);
        work.inode_pages_read = work.inode_pages_read.saturating_add(inode.pages_read);
        let found = outcome?;
        work.prefetch.max_answers = work.prefetch.max_answers.max(found.len() as u64);
        for (serial, value) in serials.iter().copied().zip(found) {
            self.make_room();
            match value {
                Some(value) => {
                    self.records.insert(serial, value);
                }
                None => {
                    self.absent.insert(serial);
                }
            }
        }
        Ok(())
    }

    /// One base record, answered from the memo when it is known.
    pub(super) fn lookup_optional(
        &mut self,
        reader: &dyn AuthenticatedObjects,
        table: InodeTable,
        serial: u64,
        work: &mut ValidationWork,
    ) -> ContentResult<Option<InodeValue>> {
        if let Some(value) = self.records.get(&serial) {
            // A memoized record was found by an earlier demand, so its charge is
            // the same one `charge_inode` makes for a found serial: one demand.
            work.objects_read = work.objects_read.saturating_add(1);
            work.inode_demands = work.inode_demands.saturating_add(1);
            return Ok(Some(*value));
        }
        if self.absent.contains(&serial) {
            // The same charge a descent would have made for a serial the base does
            // not hold (`lookup_many` charges one demand per serial answered at a
            // leaf, absence included), so the physical read is what this removes
            // and the accounting is what it keeps.
            work.objects_read = work.objects_read.saturating_add(1);
            work.inode_demands = work.inode_demands.saturating_add(1);
            return Ok(None);
        }
        let mut inode = InodeReadWork::default();
        let found = lookup_many(reader, table, &[serial], &mut inode)?;
        charge_inode(work, inode);
        let value = found.into_iter().next().flatten();
        self.make_room();
        match value {
            Some(value) => {
                self.records.insert(serial, value);
            }
            None => {
                self.absent.insert(serial);
            }
        }
        Ok(value)
    }

    /// One base record that must exist.
    pub(super) fn lookup_one(
        &mut self,
        reader: &dyn AuthenticatedObjects,
        table: InodeTable,
        serial: u64,
        work: &mut ValidationWork,
    ) -> ContentResult<InodeValue> {
        self.lookup_optional(reader, table, serial, work)?
            .ok_or(ContentError::InvalidRecord("missing base inode"))
    }
}

const HOT_FACTS: usize = 8;
struct HotFacts {
    scope: FactScope,
    entries: Vec<BaseFact>,
    next: usize,
    canonical: CanonicalBudget,
}
struct HotOwner {
    value: Box<HotFacts>,
    _memory: GraphMemoryLease,
}
impl std::ops::Deref for HotOwner {
    type Target = HotFacts;
    fn deref(&self) -> &HotFacts {
        &self.value
    }
}
impl std::ops::DerefMut for HotOwner {
    fn deref_mut(&mut self) -> &mut HotFacts {
        &mut self.value
    }
}
/// Actual fixed hot8 owner/layout charge shared with native namespace admission.
/// Canonical returned data uses its separate32MiB compatibility budget.
pub const fn validation_fact_working_bytes() -> usize {
    std::mem::size_of::<ValidationState>()
        + std::mem::size_of::<HotFacts>()
        + std::mem::size_of::<HotOwner>()
        + HOT_FACTS * std::mem::size_of::<BaseFact>()
        + 4 * std::mem::size_of::<usize>()
}
/// Exact64-answer/result and insertion-array admission during supplied prefetch.
pub const fn validation_fact_prefetch_working_bytes() -> usize {
    ALLOCATION_CHECK_BATCH
        * (std::mem::size_of::<BaseFact>() + std::mem::size_of::<Option<InodeValue>>())
}
/// Actual fixed scalar parent declaration/marking source window.
pub const fn validation_parent_working_bytes() -> usize {
    128 * std::mem::size_of::<u64>()
}
impl ValidationState {
    pub(super) fn reserve_working<S: FactState + ?Sized>(
        &mut self,
        state: &mut S,
        bytes: usize,
    ) -> ContentResult<GraphMemoryLease> {
        let scope = &self
            .hot
            .as_ref()
            .ok_or(ContentError::InvalidOrderingRecord(
                "validation supplied working",
            ))?
            .scope;
        state.fact_memory(scope)?.reserve(bytes)
    }
    pub(super) fn supplied(scope: FactScope, memory: GraphMemory) -> ContentResult<Self> {
        let lease = memory.reserve(validation_fact_working_bytes())?;
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(HOT_FACTS)
            .map_err(|_| ContentError::ResourceUnavailable {
                what: "validation.hot_facts",
            })?;
        if entries.capacity() > HOT_FACTS {
            return Err(ContentError::InvalidOrderingRecord(
                "validation hot capacity",
            ));
        }
        Ok(Self {
            records: BTreeMap::new(),
            absent: BTreeSet::new(),
            limit: HOT_FACTS,
            hot: Some(HotOwner {
                value: Box::new(HotFacts {
                    scope,
                    entries,
                    next: 0,
                    canonical: CanonicalBudget::compatibility(),
                }),
                _memory: lease,
            }),
        })
    }
    fn selected_hot(&self, table: InodeTable) -> ContentResult<&HotFacts> {
        let hot = self
            .hot
            .as_ref()
            .ok_or(ContentError::InvalidOrderingRecord(
                "validation supplied facts",
            ))?;
        if hot.scope.subject().table() != Some(table) {
            return Err(ContentError::InvalidOrderingRecord(
                "validation fact base table",
            ));
        }
        Ok(hot)
    }
    fn cache(&mut self, fact: BaseFact) {
        let hot = self.hot.as_mut().unwrap().value.as_mut();
        if let Some(old) = hot.entries.iter_mut().find(|old| old.serial == fact.serial) {
            *old = fact;
            return;
        }
        if hot.entries.len() < HOT_FACTS {
            hot.entries.push(fact);
        } else {
            hot.entries[hot.next] = fact;
            hot.next = (hot.next + 1) % HOT_FACTS;
        }
    }
    pub(super) fn known_supplied<S: FactState + ?Sized>(
        &mut self,
        state: &mut S,
        table: InodeTable,
        serial: u64,
    ) -> ContentResult<bool> {
        let hot = self.selected_hot(table)?;
        if hot.entries.iter().any(|fact| fact.serial == serial) {
            return Ok(true);
        }
        let scope = hot.scope.clone();
        let fact = state.fact_get(&scope, serial)?;
        if let Some(fact) = fact {
            if fact.serial != serial {
                return Err(ContentError::InvalidOrderingRecord("fact selected answer"));
            }
            BaseFact::decode(serial, &fact.encode_value()?)?;
            self.cache(fact);
            Ok(true)
        } else {
            Ok(false)
        }
    }
    pub(super) fn lookup_supplied<S: FactState + ?Sized>(
        &mut self,
        state: &mut S,
        reader: &dyn AuthenticatedObjects,
        table: InodeTable,
        serial: u64,
        work: &mut ValidationWork,
    ) -> ContentResult<Option<InodeValue>> {
        if self.known_supplied(state, table, serial)? {
            work.objects_read = work.objects_read.saturating_add(1);
            work.inode_demands = work.inode_demands.saturating_add(1);
            return Ok(self
                .hot
                .as_ref()
                .unwrap()
                .entries
                .iter()
                .find(|f| f.serial == serial)
                .unwrap()
                .value);
        }
        let scope = self.selected_hot(table)?.scope.clone();
        let _window = state
            .fact_memory(&scope)?
            .reserve(std::mem::size_of::<BaseFact>() + std::mem::size_of::<Option<InodeValue>>())?;
        let mut inode = InodeReadWork::default();
        let result = lookup_many_owned(
            reader,
            table,
            &[serial],
            &mut inode,
            &self.hot.as_ref().unwrap().canonical,
        );
        charge_inode(work, inode);
        let value = result?.into_iter().next().flatten();
        let fact = BaseFact { serial, value };
        state.fact_insert(&scope, &[fact])?;
        self.cache(fact);
        Ok(value)
    }
    pub(super) fn prefetch_supplied<S: FactState + ?Sized>(
        &mut self,
        state: &mut S,
        reader: &dyn AuthenticatedObjects,
        table: InodeTable,
        serials: &[u64],
        work: &mut ValidationWork,
    ) -> ContentResult<()> {
        if serials.len() > ALLOCATION_CHECK_BATCH {
            return Err(ContentError::ObjectLimitExceeded {
                limit: ALLOCATION_CHECK_BATCH,
                actual: serials.len(),
            });
        }
        if serials.is_empty() {
            return Ok(());
        }
        let scope = self.selected_hot(table)?.scope.clone();
        let _window = state
            .fact_memory(&scope)?
            .reserve(validation_fact_prefetch_working_bytes())?;
        work.prefetch.lookup_calls = work.prefetch.lookup_calls.saturating_add(1);
        work.prefetch.submitted_serials = work
            .prefetch
            .submitted_serials
            .saturating_add(serials.len() as u64);
        work.prefetch.max_missing = work.prefetch.max_missing.max(serials.len() as u64);
        let mut inode = InodeReadWork::default();
        let result = lookup_many_owned(
            reader,
            table,
            serials,
            &mut inode,
            &self.hot.as_ref().unwrap().canonical,
        );
        work.read_waves = work.read_waves.saturating_add(inode.read_waves);
        work.inode_pages_read = work.inode_pages_read.saturating_add(inode.pages_read);
        let found = result?;
        work.prefetch.max_answers = work.prefetch.max_answers.max(found.len() as u64);
        if found.len() != serials.len() {
            return Err(ContentError::InvalidOrderingRecord(
                "base fact answer count",
            ));
        }
        let mut records = [BaseFact {
            serial: 0,
            value: None,
        }; ALLOCATION_CHECK_BATCH];
        for (i, (serial, value)) in serials.iter().copied().zip(found).enumerate() {
            records[i] = BaseFact { serial, value };
        }
        state.fact_insert(&scope, &records[..serials.len()])?;
        for fact in &records[..serials.len()] {
            self.cache(*fact);
        }
        Ok(())
    }
}
