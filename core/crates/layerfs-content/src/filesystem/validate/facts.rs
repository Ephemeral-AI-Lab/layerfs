//! Existing admitted positive/absence memo and semantic lookup accounting.
use super::{charge_inode, ValidationWork, ALLOCATION_CHECK_BATCH};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::inode::read::{lookup_many, InodeReadWork, InodeTable};
use crate::object::inode_leaf::InodeValue;
use crate::object::AuthenticatedObjects;
use std::collections::{BTreeMap, BTreeSet};

/// Memoizes authenticated base records and absence within a resource-sized
/// window. Eviction changes physical reads, never the logical demand charge.
pub(super) struct ValidationState {
    records: BTreeMap<u64, InodeValue>,
    absent: BTreeSet<u64>,
    limit: usize,
}

impl ValidationState {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            records: BTreeMap::new(),
            absent: BTreeSet::new(),
            limit: limit.max(1),
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
