//! The pooled metadata lane: ordinals, value groups, index sync, pooled bases.
//!
//! One canonical inode leaf is admitted here: its distinct values are assigned
//! ordinals (from the save's own memo, the retained index window, or a fresh
//! assignment), new values are framed into value groups and placed, and the leaf
//! body is stored in full or as a COPY/INSERT delta against one acquired base.
//! Everything this module keeps is bounded by the operation: the ordinal memo is
//! reset per leaf, the index is synchronized once per save, and the reader it
//! borrows is the owner's own.

use layerfs_content::{FinalizedObject, ObjectId, ObjectRole};

use crate::cas::owner::{MutationOwner, SaveProfile};
use crate::error::{StorageError, StorageResult};
use crate::pack::layout::PackLane;
use crate::sqlite::lookup;
use std::collections::BTreeMap;
use std::time::Instant;

/// Distinct values the pooled lane's per-save ordinal memo may hold at once.
///
/// Owner: one save operation's pooled lane. Bound: this many `(value, ordinal)`
/// entries. Live multiplicity: one map per save. Lifetime: one leaf - the map is
/// reset once that leaf's value groups are written, because the catalogue rows
/// and the retained index window answer those values from then on. Release:
/// cleared per leaf, and dropped with the operation. Worst case is therefore one
/// leaf's worth of values: `POOLED_LEAF_ROWS_LIMIT` (100) entries of a 73-byte
/// key, a `u32` value and a `BTreeMap` node share, about 12 KiB. A leaf that
/// somehow offered more fails closed rather than growing the map.
const PENDING_VALUES_LIMIT: usize = crate::policy::POOLED_LEAF_ROWS_LIMIT;

/// What the pooled metadata lane actually did for one save operation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PoolCounters {
    /// Canonical inode leaves admitted.
    pub leaves: u64,
    /// Values that reused an existing ordinal.
    pub reused_values: u64,
    /// Values that received a new ordinal.
    pub new_values: u64,
    /// Value groups written.
    pub groups: u64,
    /// Pooled leaves stored against a direct base.
    pub delta_leaves: u64,
    /// Pooled leaves stored in full.
    pub full_leaves: u64,
    /// Delta trials attempted.
    pub trials: u64,
    /// Candidates refused because the resulting chain would exceed its budget.
    pub work_exceeded: u64,
}

impl MutationOwner {
    /// Admits one canonical inode leaf through the pooled metadata lane.
    ///
    /// Ordinals are assigned in first-encounter order, new values are grouped and
    /// written as their own packs, and the leaf body is stored in full or as a
    /// COPY/INSERT delta against one acquired base. The catalogue row and the leaf
    /// row share the save's transaction, so a group is visible exactly when the
    /// leaf that refers to it is.
    pub(super) fn select_pooled(
        &mut self,
        object: &FinalizedObject,
        advisory: &[ObjectId],
    ) -> StorageResult<crate::encoding::EncodedRecord> {
        use layerfs_content::inode_leaf::{pooled_body, INODE_VALUE_BYTES};
        let started = Instant::now();
        let synced = self.sync_pool_index();
        SaveProfile::charge(&mut self.profile.resolve.pooled_ns, started);
        synced?;
        let leaf = layerfs_content::inode_leaf::InodeLeaf::decode(object.canonical())?;
        // One batch demand for every distinct value of this leaf that the save's
        // own memo cannot answer, in first-encounter order. Asking per row cost
        // one point query per row; the index is a batch API.
        let mut unknown: Vec<[u8; INODE_VALUE_BYTES]> = Vec::new();
        let mut seen: std::collections::BTreeSet<[u8; INODE_VALUE_BYTES]> =
            std::collections::BTreeSet::new();
        for row in &leaf.rows {
            if !self.pending_values.contains_key(&row.value) && seen.insert(row.value) {
                unknown.push(row.value);
            }
        }
        let known: BTreeMap<[u8; INODE_VALUE_BYTES], u32> = if unknown.is_empty() {
            BTreeMap::new()
        } else {
            let arbitration = std::sync::Arc::clone(&self.arbitration);
            let _guard = crate::sqlite::ownership::lock_unless_held(&arbitration, self.wave_held)?;
            let mut index = self
                .pool_index
                .lock()
                .map_err(|_| StorageError::Integrity("pool index lock"))?;
            // The owner's own ceiling, not an unbounded one: a catalogue row
            // belonging to a pack this save has not published (and did not
            // create) is refused here rather than resolved.
            let started = Instant::now();
            let found = index.find(
                &self.connection,
                &self.capacities,
                self.ceiling,
                &mut self.pool_reader,
                &mut self.decompression,
                &unknown,
            );
            SaveProfile::charge(&mut self.profile.resolve.pooled_ns, started);
            found?
        };
        let fresh_count = unknown
            .iter()
            .filter(|value| !known.contains_key(*value))
            .count();
        if fresh_count != 0 {
            let arbitration = std::sync::Arc::clone(&self.arbitration);
            let _guard = crate::sqlite::ownership::lock_unless_held(&arbitration, self.wave_held)?;
            // Bounded commits re-acquire the save's transaction, so it is normally
            // already open; only start one when this caller arrives without it.
            if !self.transaction_open {
                self.begin_write()?;
            }
            self.next_ordinal = Some(u64::from(crate::sqlite::ownership::reserve_ordinals(
                &self.connection,
                fresh_count,
            )?));
            // **The reservation is its own step.** An aborted save's ordinal
            // reservations are never reused - a value the catalogue handed out is
            // not handed out again, whatever happened to the save that asked for it
            // - so this statement is acknowledged here rather than with the wave
            // that happens to contain it. A wave's transaction is closed at this
            // point and reopened by the next write, which is exactly the boundary
            // every seal used to draw.
            self.commit_reservation()?;
        }
        let mut ordinals = Vec::with_capacity(leaf.rows.len());
        let mut fresh: Vec<[u8; INODE_VALUE_BYTES]> = Vec::new();
        for row in &leaf.rows {
            let ordinal = match self.pending_values.get(&row.value) {
                Some(ordinal) => {
                    self.pool.reused_values += 1;
                    *ordinal
                }
                None => match known.get(&row.value).copied() {
                    Some(ordinal) => {
                        self.pool.reused_values += 1;
                        ordinal
                    }
                    None => {
                        let ordinal = self.assign_ordinal()?;
                        if self.pending_values.len() >= PENDING_VALUES_LIMIT {
                            return Err(StorageError::Integrity("metadata pending values"));
                        }
                        self.pending_values.insert(row.value, ordinal);
                        fresh.push(row.value);
                        self.pool.new_values += 1;
                        ordinal
                    }
                },
            };
            ordinals.push(ordinal);
        }
        self.write_value_groups(&fresh)?;
        // The leaf's value groups are catalogue rows in this save's own open
        // transaction, and the retained index window was told about them by
        // `note_group`, so the memo has nothing left to answer: it is reset once
        // per leaf. That is what bounds it - a leaf holds at most
        // `POOLED_LEAF_ROWS_LIMIT` rows - instead of accumulating one entry per
        // distinct value of the whole operation. A value whose window entry was
        // evicted in the meantime resolves exactly as it does after a cold start:
        // the index misses it and a duplicate physical value is stored, which the
        // window's own eviction semantics already allow.
        self.pending_values.clear();
        let body = pooled_body(object.canonical(), &ordinals)?;
        self.pool.leaves += 1;
        let started = Instant::now();
        let full = crate::encoding::pool::leaf::encode_full(&body);
        SaveProfile::charge(&mut self.profile.full_ns, started);
        let full = full?;
        // One base acquisition, then one instruction trial. A missing or
        // ineligible base, or a losing comparison, stores the leaf in full.
        let started = Instant::now();
        let base = self.pool_base(advisory, object.canonical_len() as u64, full.len() as u64);
        SaveProfile::charge(&mut self.profile.resolve.pooled_ns, started);
        let base = base?;
        let Some((base_id, base_body)) = base else {
            return Ok(self.pooled_full(full, object.canonical_len(), body.len()));
        };
        self.pool.trials += 1;
        let mut budget = crate::policy::METADATA_MATCH_BUDGET_BYTES;
        let started = Instant::now();
        let program = crate::encoding::pool::delta::build(base_id, &base_body, &body, &mut budget);
        SaveProfile::charge(&mut self.profile.delta_ns, started);
        let program = program?;
        let Some(program) = program else {
            return Ok(self.pooled_full(full, object.canonical_len(), body.len()));
        };
        if program.len() >= full.len() {
            return Ok(self.pooled_full(full, object.canonical_len(), body.len()));
        }
        self.pool.delta_leaves += 1;
        Ok(crate::encoding::EncodedRecord {
            lane: PackLane::Ordinary,
            record: program,
            canonical_length: object.canonical_len(),
            raw_length: body.len(),
            base: Some(base_id),
        })
    }

    /// The pooled lane's FULL alternative, counted once.
    ///
    /// Every path that declines a COPY/INSERT program - no base, no program, a
    /// program that is not smaller, or a refused chain - stores the same FULL
    /// record, so the fallback exists once.
    fn pooled_full(
        &mut self,
        full: Vec<u8>,
        canonical_length: usize,
        raw_length: usize,
    ) -> crate::encoding::EncodedRecord {
        self.pool.full_leaves += 1;
        crate::encoding::EncodedRecord {
            lane: PackLane::Ordinary,
            record: full,
            canonical_length,
            raw_length,
            base: None,
        }
    }

    /// Synchronizes the Store-owned index with the catalogue once per save.
    fn sync_pool_index(&mut self) -> StorageResult<()> {
        if self.pool_synced {
            return Ok(());
        }
        let arbitration = std::sync::Arc::clone(&self.arbitration);
        let _guard = crate::sqlite::ownership::lock_unless_held(&arbitration, self.wave_held)?;
        let mut index = self
            .pool_index
            .lock()
            .map_err(|_| StorageError::Integrity("pool index lock"))?;
        // `self.ceiling` already includes every pack this save created, so the
        // recurrence reads this operation's own groups and refuses any row from a
        // pack that is not published to it.
        index.sync(
            &self.connection,
            &self.capacities,
            self.ceiling,
            &mut self.pool_reader,
            &mut self.decompression,
        )?;
        self.pool_synced = true;
        Ok(())
    }

    /// Acknowledges the ordinal reservation's own transaction.
    ///
    /// The wave flag is cleared for this one call because the reservation is not
    /// part of the wave's transaction: `maybe_commit` is the product's only commit
    /// site, and this is a step, not a nested write.
    fn commit_reservation(&mut self) -> StorageResult<()> {
        let held = self.wave_held;
        self.wave_held = false;
        let result = self.maybe_commit();
        self.wave_held = held;
        result
    }

    /// Next ordinal this save may assign, read from the catalogue once.
    fn assign_ordinal(&mut self) -> StorageResult<u32> {
        let next = match self.next_ordinal {
            Some(next) => next,
            None => return Err(StorageError::Integrity("metadata ordinal reservation")),
        };
        let assigned = next
            .checked_add(1)
            .ok_or(StorageError::Integrity("metadata ordinal maximum"))?;
        self.next_ordinal = Some(assigned);
        u32::try_from(next).map_err(|_| StorageError::Integrity("metadata ordinal maximum"))
    }

    /// Builds and places the value groups of every new value of one leaf.
    fn write_value_groups(&mut self, fresh: &[[u8; 73]]) -> StorageResult<()> {
        if fresh.is_empty() {
            return Ok(());
        }
        let first = self
            .next_ordinal
            .ok_or(StorageError::Integrity("metadata ordinal cursor"))?
            .checked_sub(fresh.len() as u64)
            .ok_or(StorageError::Integrity("metadata ordinal cursor"))?;
        let first = u32::try_from(first)
            .map_err(|_| StorageError::Integrity("metadata ordinal maximum"))?;
        let mut built = Vec::new();
        for (number, chunk) in fresh.chunks(crate::policy::VALUES_PER_GROUP).enumerate() {
            let canonical = chunk
                .iter()
                .map(crate::encoding::pool::value_group::canonical_value)
                .collect::<StorageResult<Vec<_>>>()?;
            let started = Instant::now();
            let group =
                crate::encoding::pool::value_group::build(&canonical, &mut self.compression);
            SaveProfile::charge(&mut self.profile.group_ns, started);
            let group = group?;
            built.push((
                first + (number * crate::policy::VALUES_PER_GROUP) as u32,
                group,
            ));
        }
        let lane = PackLane::PooledMetadata;
        let encoded: Vec<crate::pack::layout::EncodedGroup> =
            built.iter().map(|(_, group)| group.group.clone()).collect();
        let arbitration = std::sync::Arc::clone(&self.arbitration);
        let _guard = crate::sqlite::ownership::lock_unless_held(&arbitration, self.wave_held)?;
        if !self.transaction_open {
            self.begin_write()?;
        }
        let started = Instant::now();
        let writes =
            self.placement[lane.index()].select_many(lane, encoded, &mut self.next_pack_id);
        SaveProfile::charge(&mut self.profile.place_ns, started);
        let writes = writes?;
        let mut next_group = 0_usize;
        for write in &writes {
            self.write_pack(write)?;
            for placed in &write.placed {
                let (first_ordinal, group) = built
                    .get(next_group)
                    .ok_or(StorageError::Integrity("placed value group count"))?;
                next_group += 1;
                let started = Instant::now();
                let inserted = crate::sqlite::pool::insert_group(
                    &self.connection,
                    &crate::sqlite::pool::ValueGroupRow {
                        first_ordinal: *first_ordinal,
                        count: group.count,
                        pack_id: write.pack_id,
                        group_number: placed.group_number,
                        digest: group.digest,
                    },
                );
                SaveProfile::charge(&mut self.profile.sql_ns, started);
                inserted?;
                self.transaction.rows += 1;
                self.transaction.bytes += group.body.len() as u64;
                self.pool.groups += 1;
            }
        }
        if next_group != built.len() {
            return Err(StorageError::Integrity("value group placement count"));
        }
        // The index learns about ordinals this save assigned without re-reading
        // the catalogue it just wrote to.
        {
            let mut index = self
                .pool_index
                .lock()
                .map_err(|_| StorageError::Integrity("pool index lock"))?;
            // The groups were built from `fresh` in order, so the values are handed
            // over as one cursor over that slice rather than re-derived per group.
            let window_start = crate::sqlite::pool::window_start(&self.connection)?;
            index.advance_window(window_start);
            let mut offset = 0_usize;
            for (first_ordinal, group) in &built {
                let end = offset
                    .checked_add(group.count)
                    .ok_or(StorageError::Integrity("metadata group values"))?;
                let values = fresh
                    .get(offset..end)
                    .ok_or(StorageError::Integrity("metadata group values"))?;
                if *first_ordinal >= window_start {
                    index.note_group(*first_ordinal, values)?;
                }
                offset = end;
            }
            if offset != fresh.len() {
                return Err(StorageError::Integrity("metadata group coverage"));
            }
        }
        self.maybe_commit()
    }

    /// First acquired pooled-leaf base, with its physical body.
    ///
    /// The base must already be stored: a leaf this same save accepted but has not
    /// sealed is not read for a trial, and an absent or too-deep base selects a
    /// full leaf by policy. A base whose chain plus the dependent leaf would exceed
    /// the chain budgets is refused for the same reason the payload lane refuses
    /// one: no stored object may depend on bytes a later read could not reconstruct.
    fn pool_base(
        &mut self,
        advisory: &[ObjectId],
        target_canonical: u64,
        target_encoded: u64,
    ) -> StorageResult<Option<(ObjectId, Vec<u8>)>> {
        let arbitration = std::sync::Arc::clone(&self.arbitration);
        let _guard = crate::sqlite::ownership::lock_unless_held(&arbitration, self.wave_held)?;
        let depth_cap = self.capacities.metadata_delta_max_depth;
        if depth_cap == 0 {
            return Ok(None);
        }
        for id in advisory {
            let Some(location) = lookup::location(&self.connection, *id, self.ceiling)? else {
                continue;
            };
            if location.role != ObjectRole::InodeLeaf {
                continue;
            }
            // The walk reads each edge from its record through the owner's own
            // pooled reader, whose pack cache the acquisition below reuses.
            let pool = &mut self.pool_reader;
            let cost = self.depths.cost_of(
                &self.connection,
                &mut self.decompression,
                *id,
                |connection, workspace, location| pool.stored_base(connection, workspace, location),
            )?;
            let Some(cost) = cost else {
                continue;
            };
            if cost.depth >= depth_cap {
                continue;
            }
            // Both budgets are charged with what a read of the dependent would
            // actually pay: the chain's canonical sum from the depth walk, and -
            // because a record's width is what the reader charges - the base
            // chain's encoded bytes as the reader measured them plus this leaf's
            // own record width. Charging a worst-case per-record bound instead made
            // every accepted depth above fifteen unusable.
            let canonical = cost.canonical.saturating_add(target_canonical);
            if canonical > self.capacities.metadata_chain_canonical_limit {
                self.pool.work_exceeded = self.pool.work_exceeded.saturating_add(1);
                continue;
            }
            // The owner's own reader, not a fresh one per trial: its pack and
            // value caches are the point (a trial used to re-materialise the same
            // base packs), and its `chain_encoded_bytes` is the same charge a read
            // of the dependent will pay. What it must not do is serve a pack body
            // this save has since appended to, so every pack write releases the
            // reader's pack cache (see `write_pack`).
            let connection = &self.connection;
            let capacities = &self.capacities;
            let body = self.pool_reader.leaf_body(
                connection,
                capacities,
                self.ceiling,
                &mut self.decompression,
                location,
            )?;
            let encoded = self
                .pool_reader
                .chain_encoded_bytes()
                .saturating_add(target_encoded);
            if encoded > self.capacities.metadata_chain_encoded_limit {
                self.pool.work_exceeded = self.pool.work_exceeded.saturating_add(1);
                continue;
            }
            return Ok(Some((*id, body)));
        }
        Ok(None)
    }

    /// Pooled lane outcomes of this operation.
    pub fn pool_counters(&self) -> PoolCounters {
        self.pool
    }
}
