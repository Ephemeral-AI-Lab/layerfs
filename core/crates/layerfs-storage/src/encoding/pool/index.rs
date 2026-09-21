//! The bounded Store-owned ordered set of pooled value candidates.
//!
//! The set is a standard-library `BTreeSet<(fingerprint, ordinal)>`: the
//! fingerprint is only a candidate filter and full value bytes decide equality.
//! It retains at most 131,072 entries and resets wholesale when a group would
//! exceed that, which reproduces the reference window exactly: eviction causes a
//! duplicate physical value, never a lost one. A failure invalidates the whole set
//! so a partly advanced index can never be reused.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::Connection;

use layerfs_content::inode_leaf::{decode_pooled_value, INODE_VALUE_BYTES};
use layerfs_content::ObjectId;

use crate::encoding::codec::DecompressionWorkspace;
use crate::encoding::pool::read::PoolReader;
use crate::error::{StorageError, StorageResult};
use crate::policy::{StorageCapacities, METADATA_INDEX_VALUES, VALUES_PER_GROUP};
use crate::sqlite::pool;

/// Bounded ordered set of `(fingerprint, ordinal)` candidates.
#[derive(Clone, Debug, Default)]
pub struct PoolIndex {
    entries: BTreeSet<(i64, u32)>,
    next: u64,
    retained_bytes: usize,
}

impl PoolIndex {
    /// Empty, unsynchronized index.
    pub fn new() -> Self {
        Self {
            entries: BTreeSet::new(),
            next: 1,
            retained_bytes: 0,
        }
    }

    /// Retained candidate entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when no candidate is retained.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Live bytes charged by the retained entries.
    pub fn live_bytes(&self) -> usize {
        self.retained_bytes
    }

    /// Drops every entry and re-syncs from the catalogue on the next call.
    ///
    /// Called when a save fails: the catalogue is authoritative and the index is
    /// disposable derivation, so invalidation is a reset, never a repair.
    pub fn invalidate(&mut self) {
        self.entries.clear();
        self.retained_bytes = 0;
        self.next = 1;
    }

    /// Records one group this save assigned, with the values it stored.
    ///
    /// The catalogue rows are written in the same transaction, so inserting the
    /// values here keeps both the retained window and the cursor exact without a
    /// second read, and applies the same whole-window reset as a synchronized
    /// group.
    pub fn note_group(
        &mut self,
        first_ordinal: u32,
        values: &[[u8; INODE_VALUE_BYTES]],
    ) -> StorageResult<()> {
        if u64::from(first_ordinal) < self.next || values.is_empty() {
            return Err(StorageError::Integrity("metadata index chronology"));
        }
        if self.entries.len() + values.len() > METADATA_INDEX_VALUES {
            self.entries.clear();
            self.retained_bytes = 0;
        }
        for (position, value) in values.iter().enumerate() {
            let ordinal = first_ordinal
                .checked_add(position as u32)
                .ok_or(StorageError::Integrity("metadata ordinal"))?;
            self.insert(value, ordinal);
        }
        self.next = u64::from(first_ordinal)
            .checked_add(
                u64::try_from(values.len())
                    .map_err(|_| StorageError::Integrity("metadata ordinal"))?,
            )
            .ok_or(StorageError::Integrity("metadata ordinal maximum"))?;
        Ok(())
    }

    /// Synchronizes the retained window with the persisted catalogue.
    pub fn sync(
        &mut self,
        connection: &Connection,
        capacities: &StorageCapacities,
        ceiling: i64,
        reader: &mut PoolReader,
        workspace: &mut DecompressionWorkspace,
    ) -> StorageResult<()> {
        let start = pool::window_start(connection)?;
        self.advance_window(start);
        let Ok(from) = u32::try_from(self.next) else {
            return Ok(());
        };
        pool::for_each_group(connection, Some(from), |row| {
            if u64::from(row.first_ordinal) < self.next
                || row.count == 0
                || row.count > VALUES_PER_GROUP
            {
                return Err(StorageError::Integrity("metadata catalogue overlap/range"));
            }
            let values = reader.group_values(connection, capacities, ceiling, workspace, &row)?;
            self.note_group(row.first_ordinal, &values)
        })?;
        Ok(())
    }

    /// Discards candidates below the allocator's bounded reserved-value window.
    pub fn advance_window(&mut self, start: u32) {
        self.entries.retain(|(_, ordinal)| *ordinal >= start);
        self.retained_bytes = self.entries.len() * (std::mem::size_of::<(i64, u32)>() + 8);
        self.next = self.next.max(u64::from(start));
    }

    fn insert(&mut self, value: &[u8; INODE_VALUE_BYTES], ordinal: u32) {
        let key = (fingerprint(value), ordinal);
        if self.entries.insert(key) {
            self.retained_bytes = self
                .retained_bytes
                .saturating_add(std::mem::size_of::<(i64, u32)>() + 8);
        }
    }

    /// Smallest ordinal whose authenticated value equals each wanted value.
    ///
    /// A fingerprint match is only a candidate: every candidate's group is read
    /// and the full 73 bytes are compared. The smallest matching ordinal wins, so
    /// the same value always resolves to the ordinal the reference would choose.
    pub fn find(
        &mut self,
        connection: &Connection,
        capacities: &StorageCapacities,
        ceiling: i64,
        reader: &mut PoolReader,
        workspace: &mut DecompressionWorkspace,
        values: &[[u8; INODE_VALUE_BYTES]],
    ) -> StorageResult<BTreeMap<[u8; INODE_VALUE_BYTES], u32>> {
        let mut found = BTreeMap::new();
        if values.is_empty() {
            return Ok(found);
        }
        // An ordered set, not a vector with a linear `contains`: every retained
        // entry can carry the same fingerprint, and a linear dedup inside the loop
        // is quadratic in the candidate count. The set both dedups and orders, so
        // the ordinal walk below is unchanged.
        let mut candidates: BTreeSet<u32> = BTreeSet::new();
        for value in values {
            let fingerprint = fingerprint(value);
            for (_candidate, ordinal) in self
                .entries
                .range((fingerprint, 0)..=(fingerprint, u32::MAX))
            {
                candidates.insert(*ordinal);
            }
        }
        let mut ordinal_values: Option<(u32, Vec<[u8; INODE_VALUE_BYTES]>)> = None;
        for ordinal in candidates {
            let needed = ordinal_values.as_ref().is_none_or(|(first, values)| {
                u64::from(ordinal) >= u64::from(*first) + values.len() as u64
            });
            if needed {
                let row = pool::group_for(connection, ordinal)?
                    .ok_or(StorageError::Integrity("metadata ordinal missing"))?;
                let raw = reader.group_values(connection, capacities, ceiling, workspace, &row)?;
                ordinal_values = Some((row.first_ordinal, raw));
            }
            let (first, raw) = ordinal_values
                .as_ref()
                .ok_or(StorageError::Integrity("metadata group cache"))?;
            let value = raw[(ordinal - *first) as usize];
            if values.contains(&value) {
                found.entry(value).or_insert(ordinal);
            }
            if found.len() == values.len() {
                break;
            }
        }
        Ok(found)
    }
}

/// Candidate filter of one value: the low eight digest bytes.
fn fingerprint(value: &[u8; INODE_VALUE_BYTES]) -> i64 {
    let canonical = ObjectId::for_bytes(value);
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&canonical.as_bytes()[..8]);
    i64::from_le_bytes(bytes)
}

/// Convenience for callers that hold canonical value objects.
pub fn value_of(canonical: &[u8]) -> StorageResult<[u8; INODE_VALUE_BYTES]> {
    Ok(decode_pooled_value(canonical)?)
}
