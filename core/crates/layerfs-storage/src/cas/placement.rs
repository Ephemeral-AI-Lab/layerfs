//! Framing, lane placement, group sealing and the open tails.
//!
//! One lane holds at most one unframed group at a time. A member waiting here has
//! no row yet, so an identity that is still pending is answered from this bounded
//! state or its group is sealed on demand - the owner never retains a second copy
//! of a waiting payload. Placement and the locator rows it produces share the
//! save's open transaction, so a group is visible exactly when its records are.

use layerfs_content::{ObjectId, ObjectRole};

use crate::cas::owner::{MutationOwner, SaveProfile};

use crate::cas::dependencies::Availability;
use crate::error::{StorageError, StorageResult};
use crate::pack::layout::{PackLane, HEADER_LEN};
use crate::pack::{build_group, SelectedWrite};
use crate::sqlite::write::{self, ObjectRow};
use std::time::Instant;

/// One record waiting for its group to be framed and placed.
///
/// Only identifiers and lengths are retained, never the payload: a group is bounded
/// by its framed bytes, so retaining canonical bytes could hold far more than the
/// group target whenever records compress well. An identity that is still waiting
/// here has no row yet, which is why the owner can seal its group on demand and let
/// the ordinary verified storage path answer for it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PendingMember {
    pub(super) object_id: ObjectId,
    pub(super) role: ObjectRole,
    pub(super) canonical_length: usize,
    pub(super) base_object_id: Option<ObjectId>,
}

#[derive(Default)]
pub(super) struct PendingGroup {
    pub(super) records: Vec<Vec<u8>>,
    pub(super) members: Vec<PendingMember>,
    /// Record payload bytes only. The group's shared framing - one count and one
    /// end offset per record - is projected through `framed_group_length` when the
    /// seal decision is made, so nothing here counts it once per record.
    pub(super) payload_len: usize,
    pub(super) canonical_bytes: usize,
}

impl PendingGroup {
    fn clear(&mut self) {
        self.records.clear();
        self.members.clear();
        self.payload_len = 0;
        self.canonical_bytes = 0;
    }
}

impl MutationOwner {
    /// True when this owner accepted `id` into an unfinished group.
    ///
    /// Such an identity has no row yet but is available to the same operation, so
    /// a later dependent must resolve it from this bounded state instead of
    /// querying storage for bytes that only exist in memory.
    pub fn pending_member(&self, id: ObjectId) -> bool {
        self.groups
            .iter()
            .any(|group| group.members.iter().any(|member| member.object_id == id))
    }

    /// Bytes retained by the open pack tails of every lane.
    pub fn retained_tail_bytes(&self) -> StorageResult<usize> {
        PackLane::ALL.iter().try_fold(0usize, |total, lane| {
            total
                .checked_add(self.placement[lane.index()].retained_bytes()?)
                .ok_or(StorageError::Integrity("retained tail accounting"))
        })
    }

    /// Frames and places the open group holding any of `ids`.
    ///
    /// A member of an unfinished group has no row yet: its group is framed but not
    /// placed, so no locator exists for it. Rather than retain a second copy of every
    /// waiting payload, the owner seals that group on demand and lets the ordinary
    /// verified storage path answer for the identity. Retaining the canonical bytes
    /// instead would be unbounded in practice: a group is bounded by its *framed*
    /// bytes, so highly compressible records could hold far more canonical data than
    /// the group target. The cost here is packing granularity for the caller that
    /// needs the identity, never correctness, and each identity is sealed at most
    /// once: afterwards its row exists and the membership lookup finds it.
    pub fn seal_pending(&mut self, ids: &[ObjectId]) -> StorageResult<()> {
        let mut lanes: Vec<PackLane> = Vec::new();
        for id in ids {
            for lane in PackLane::ALL {
                let holds = self.groups[lane.index()]
                    .members
                    .iter()
                    .any(|member| member.object_id == *id);
                if holds && !lanes.contains(&lane) {
                    lanes.push(lane);
                }
            }
        }
        if lanes.is_empty() {
            return Ok(());
        }
        let mut availability = Availability::default();
        for lane in lanes {
            self.seal_group(lane, &mut availability)?;
        }
        Ok(())
    }

    /// Frames and places the lane's pending group, then records its locators.
    pub fn seal_group(
        &mut self,
        lane: PackLane,
        availability: &mut Availability,
    ) -> StorageResult<()> {
        let index = lane.index();
        if self.groups[index].records.is_empty() {
            return Ok(());
        }
        let whole = Instant::now();
        let mut pending = std::mem::take(&mut self.groups[index]);
        let started = Instant::now();
        let framed = build_group(lane, &pending.records, Some(&mut self.compression));
        SaveProfile::charge(&mut self.profile.group_ns, started);
        let group = match framed {
            Ok(group) => group,
            Err(error) => {
                pending.clear();
                return Err(error);
            }
        };
        let arbitration = std::sync::Arc::clone(&self.arbitration);
        let _guard = crate::sqlite::ownership::lock_unless_held(&arbitration, self.wave_held)?;
        if !self.transaction_open {
            self.begin_write()?;
        }
        let started = Instant::now();
        let writes = self.placement[index].select_many(lane, vec![group], &mut self.next_pack_id);
        SaveProfile::charge(&mut self.profile.place_ns, started);
        let writes = writes?;
        let write = writes
            .first()
            .ok_or(StorageError::Integrity("placement produced no write"))?;
        let placed = write
            .placed
            .first()
            .copied()
            .ok_or(StorageError::Integrity("placement produced no group"))?;
        if placed.records != pending.members.len() {
            return Err(StorageError::Integrity("placed record count"));
        }
        // Ordinary groups obey the preparation byte bound. A pre-existing
        // singleton representation can own one larger canonical object, bounded
        // by CANONICAL_LIMIT and SINGLETON_PACK_LIMIT, never a group of them.
        if pending.members.len() as u64 + 4 > self.capacities.transaction_rows {
            return Err(StorageError::CapacityExceeded {
                what: "transaction rows",
                limit: self.capacities.transaction_rows,
                actual: pending.members.len() as u64 + 4,
            });
        }
        self.write_pack(write)?;
        // The group's rows are inserted together: one statement per chunk the
        // engine's own limits allow, not one statement per row (P2-2). The same
        // rows, in the same order, with the same counters charged.
        let started = Instant::now();
        let rows: Vec<ObjectRow> = pending
            .members
            .iter()
            .enumerate()
            .map(|(record_number, member)| ObjectRow {
                object_id: member.object_id,
                role: member.role.code(),
                canonical_length: member.canonical_length,
                pack_id: write.pack_id,
                group_number: placed.group_number,
                record_number,
            })
            .collect();
        SaveProfile::charge(&mut self.profile.diag.rows_ns, started);
        let started = Instant::now();
        let statements = write::insert_objects(&self.connection, &rows);
        SaveProfile::charge(&mut self.profile.sql_ns, started);
        SaveProfile::charge(&mut self.profile.diag.insert_objects_ns, started);
        let statements = statements?;
        if self.wave_held {
            // The wave validates every row it writes in one call at its end. The
            // rows this check compares against belong to *other* saves and cannot
            // change while this transaction holds the Store's write lock, so
            // deferring to the end of the wave is the same check with one query
            // set instead of one per seal - and a collision now rolls the whole
            // wave back instead of leaving earlier seals committed.
            self.wave_rows.extend_from_slice(&rows);
        } else {
            let started = Instant::now();
            self.validate_candidates(&rows)?;
            SaveProfile::charge(&mut self.profile.diag.validate_ns, started);
        }
        self.counters.statements = self.counters.statements.saturating_add(statements);
        let started = Instant::now();
        for member in &pending.members {
            if member.base_object_id.is_some() {
                self.counters.prefix_records += 1;
            } else {
                self.counters.full_records += 1;
            }
            availability.inserted(member.object_id);
            // The wave's membership snapshot predates this seal, so the row just
            // written is recorded for the rest of the wave; see `sealed_rows`.
            self.sealed_rows.push(member.object_id);
            self.counters.inserted += 1;
            self.transaction.rows += 1;
            self.transaction.bytes += member.canonical_length as u64;
        }
        SaveProfile::charge(&mut self.profile.diag.members_ns, started);
        pending.clear();
        self.maybe_commit()?;
        SaveProfile::charge(&mut self.profile.diag.seal_total_ns, whole);
        Ok(())
    }

    pub(super) fn write_pack(&mut self, write: &SelectedWrite) -> StorageResult<()> {
        let whole = Instant::now();
        // A write adds a directory entry and a body to one pack, so a cache that
        // holds that pack's bytes is describing a directory with fewer entries
        // than the pack now has: a read of a group the write added would ask the
        // old directory for it and be refused with `Integrity("group ordinal")`.
        // The pooled reader's decoded values survive - an ordinal's value is
        // written once and never moves, and neither does a body - but its pack
        // cache does not.
        self.pool_reader.release_packs();
        // The delta reader's pack cache holds whole pack bytes and is keyed by pack
        // id, so an append leaves a stale directory behind. Reading a group ordinal
        // that the append added then asks the *old* directory for it, which has
        // fewer groups, and `pack::layout::group_view` refuses it with
        // `Integrity("group ordinal")` -- a stored row that is correct, read
        // through bytes that are not. Dropping the entry costs one re-read and
        // cannot be skipped: the pack is append-only, so the cached copy is valid
        // only until the next append.
        self.pack_cache.remove(&write.pack_id);
        let started = Instant::now();
        let written = if write.created {
            write::insert_pack(&self.connection, write.pack_id, write)
        } else {
            write::append_pack(&self.connection, write.pack_id, write)
        };
        SaveProfile::charge(&mut self.profile.sql_ns, started);
        written?;
        let submitted = (write.bodies.len() + write.directory.len() + HEADER_LEN) as u64;
        self.counters.pack_bytes_written =
            self.counters.pack_bytes_written.saturating_add(submitted);
        if write.created {
            self.counters.packs_created += 1;
        } else {
            self.counters.pack_appends += 1;
        }
        self.ceiling = self.ceiling.max(write.pack_id);
        // The save's pack ceiling is read in exactly one place - `publish`, which
        // folds it into the retained range in the transaction that publishes the
        // save - and only a write that creates a pack can raise it. Re-asserting
        // it on every append writes a value the row already holds.
        if write.created {
            self.connection.execute(
                "UPDATE saves SET pack_ceiling = MAX(pack_ceiling, ?2) WHERE save_id = ?1 AND active_slot IS NOT NULL",
                [self.save_id, write.pack_id],
            )?;
        }
        self.transaction.rows += 1;
        // The bytes this write actually hands to the engine: its bodies, its
        // directory entries and the control area. It used to be the assembled
        // length of the whole pack, which is not a byte any transaction submitted.
        self.transaction.bytes += (write.bodies.len() + write.directory.len() + HEADER_LEN) as u64;
        SaveProfile::charge(&mut self.profile.diag.write_pack_total_ns, whole);
        Ok(())
    }
}
