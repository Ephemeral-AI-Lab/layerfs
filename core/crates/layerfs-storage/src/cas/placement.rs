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
use crate::pack::layout::{EncodedGroup, PackLane, HEADER_LEN};
use crate::pack::{build_group, SelectedWrite};
use crate::policy::{BATCH_OBJECT_LIMIT, PACK_LIMIT};
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

struct QueuedGroup {
    group: EncodedGroup,
    members: Vec<PendingMember>,
}

/// Sealed groups waiting inside one wave; never retained across its COMMIT.
#[derive(Default)]
pub(super) struct QueuedGroups {
    lane: Option<PackLane>,
    groups: Vec<QueuedGroup>,
    body_bytes: usize,
    rows: usize,
}

impl QueuedGroups {
    fn holds(&self, id: ObjectId) -> bool {
        self.groups
            .iter()
            .any(|group| group.members.iter().any(|member| member.object_id == id))
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
            || self.queued.holds(id)
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
        let mut availability = Availability::default();
        for lane in lanes {
            self.seal_group(lane, &mut availability)?;
        }
        if ids.iter().any(|id| self.queued.holds(*id)) {
            self.flush_queued_groups(&mut availability)?;
        }
        Ok(())
    }

    /// Make a queued predecessor visible before representation selection reads it.
    pub(super) fn flush_queued_if_contains(
        &mut self,
        ids: &[ObjectId],
        availability: &mut Availability,
    ) -> StorageResult<()> {
        if ids.iter().any(|id| self.queued.holds(*id)) {
            self.flush_queued_groups(availability)?;
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
        if self.wave_held
            && matches!(
                lane,
                PackLane::Ordinary | PackLane::Native | PackLane::WholeFile
            )
            && group.body_size(lane)? <= PACK_LIMIT
            && pending.members.len() <= BATCH_OBJECT_LIMIT
        {
            self.queue_group(lane, group, pending, availability)?;
            SaveProfile::charge(&mut self.profile.diag.seal_total_ns, whole);
            return Ok(());
        }
        self.flush_queued_groups(availability)?;
        self.place_groups(
            lane,
            vec![QueuedGroup {
                group,
                members: pending.members,
            }],
            availability,
        )?;
        SaveProfile::charge(&mut self.profile.diag.seal_total_ns, whole);
        Ok(())
    }

    fn queue_group(
        &mut self,
        lane: PackLane,
        group: EncodedGroup,
        pending: PendingGroup,
        availability: &mut Availability,
    ) -> StorageResult<()> {
        let bytes = group.body_size(lane)?;
        let rows = pending.members.len();
        if self.queued.lane.is_some_and(|active| active != lane)
            || self.queued.body_bytes.saturating_add(bytes) > PACK_LIMIT
            || self.queued.rows.saturating_add(rows) > BATCH_OBJECT_LIMIT
        {
            self.flush_queued_groups(availability)?;
        }
        self.queued.lane = Some(lane);
        self.queued.body_bytes += bytes;
        self.queued.rows += rows;
        self.queued.groups.push(QueuedGroup {
            group,
            members: pending.members,
        });
        Ok(())
    }

    /// Place a bounded run of groups before any read, reuse, wave validation or COMMIT.
    pub(super) fn flush_queued_groups(
        &mut self,
        availability: &mut Availability,
    ) -> StorageResult<()> {
        let Some(lane) = self.queued.lane else {
            return Ok(());
        };
        if !self.wave_held {
            return Err(StorageError::Integrity("queued groups crossed wave"));
        }
        let queued = std::mem::take(&mut self.queued);
        let started = Instant::now();
        self.place_groups(lane, queued.groups, availability)?;
        SaveProfile::charge(&mut self.profile.diag.seal_total_ns, started);
        Ok(())
    }

    fn place_groups(
        &mut self,
        lane: PackLane,
        groups: Vec<QueuedGroup>,
        availability: &mut Availability,
    ) -> StorageResult<()> {
        let arbitration = std::sync::Arc::clone(&self.arbitration);
        let _guard = crate::sqlite::ownership::lock_unless_held(&arbitration, self.wave_held)?;
        if !self.transaction_open {
            self.begin_write()?;
        }
        let (encoded, members): (Vec<_>, Vec<_>) = groups
            .into_iter()
            .map(|group| (group.group, group.members))
            .unzip();
        let started = Instant::now();
        let writes =
            self.placement[lane.index()].select_many(lane, encoded, &mut self.next_pack_id);
        SaveProfile::charge(&mut self.profile.place_ns, started);
        let writes = writes?;
        let row_count = members.iter().map(Vec::len).sum::<usize>();
        if row_count as u64 + 4 > self.capacities.transaction_rows {
            return Err(StorageError::CapacityExceeded {
                what: "transaction rows",
                limit: self.capacities.transaction_rows,
                actual: row_count as u64 + 4,
            });
        }
        let mut rows = Vec::with_capacity(row_count);
        let mut counted = Vec::with_capacity(row_count);
        let mut member_groups = members.into_iter();
        for write in &writes {
            self.write_pack(write)?;
            let started = Instant::now();
            for placed in &write.placed {
                let members = member_groups
                    .next()
                    .ok_or(StorageError::Integrity("placed group count"))?;
                if placed.records != members.len() {
                    return Err(StorageError::Integrity("placed record count"));
                }
                for (record_number, member) in members.into_iter().enumerate() {
                    rows.push(ObjectRow {
                        object_id: member.object_id,
                        role: member.role.code(),
                        canonical_length: member.canonical_length,
                        pack_id: write.pack_id,
                        group_number: placed.group_number,
                        record_number,
                    });
                    counted.push(member);
                }
            }
            SaveProfile::charge(&mut self.profile.diag.rows_ns, started);
        }
        if member_groups.next().is_some() {
            return Err(StorageError::Integrity("placed group count"));
        }
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
        for member in counted {
            if member.base_object_id.is_some() {
                self.counters.prefix_records += 1;
            } else {
                self.counters.full_records += 1;
            }
            availability.inserted(member.object_id);
            self.sealed_rows.push(member.object_id);
            self.counters.inserted += 1;
            self.transaction.rows += 1;
            self.transaction.bytes += member.canonical_length as u64;
        }
        SaveProfile::charge(&mut self.profile.diag.members_ns, started);
        self.maybe_commit()?;
        Ok(())
    }

    pub(super) fn write_pack(&mut self, write: &SelectedWrite) -> StorageResult<()> {
        let whole = Instant::now();
        // Each flush inserts closed packs. Release this writer's cached pack
        // bodies after a write to bound retained memory; decoded values survive.
        self.pool_reader.release_packs();
        // Clear the selected pack's delta-reader entry too; it is normally
        // absent for a new pack, and this keeps the write path's cache bound.
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
