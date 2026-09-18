//! Framing, lane placement, group sealing and the open tails.
//!
//! One lane holds at most one unframed group at a time. A member waiting here has
//! no row yet, so an identity that is still pending is answered from this bounded
//! state or its group is sealed on demand - the owner never retains a second copy
//! of a waiting payload. Placement and the locator rows it produces share the
//! save's open transaction, so a group is visible exactly when its records are.

use layerfs_content::{ObjectId, ObjectRole};

use crate::cas::owner::MutationOwner;

use crate::cas::dependencies::Availability;
use crate::error::{StorageError, StorageResult};
use crate::pack::layout::PackLane;
use crate::pack::{build_group, SelectedWrite};
use crate::sqlite::write::{self, ObjectRow};

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
}

impl PendingGroup {
    fn clear(&mut self) {
        self.records.clear();
        self.members.clear();
        self.payload_len = 0;
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
                .checked_add(self.placement[lane.index()].retained_bytes(*lane)?)
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
        let mut pending = std::mem::take(&mut self.groups[index]);
        let group = match build_group(lane, &pending.records, Some(&mut self.compression)) {
            Ok(group) => group,
            Err(error) => {
                pending.clear();
                return Err(error);
            }
        };
        let writes =
            self.placement[index].select_many(lane, vec![group], &mut self.next_pack_id)?;
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
        self.write_pack(write)?;
        for (record_number, member) in pending.members.iter().enumerate() {
            if member.base_object_id.is_some() {
                self.counters.prefix_records += 1;
            } else {
                self.counters.full_records += 1;
            }
            let statements = write::insert_object(
                &self.connection,
                &ObjectRow {
                    object_id: member.object_id,
                    role: member.role.code(),
                    canonical_length: member.canonical_length,
                    base_object_id: member.base_object_id,
                    pack_id: write.pack_id,
                    group_number: placed.group_number,
                    record_number,
                },
            )?;
            self.counters.statements = self.counters.statements.saturating_add(statements);
            availability.inserted(member.object_id);
            self.counters.inserted += 1;
            self.transaction.rows += 1;
            self.transaction.bytes += member.canonical_length as u64;
        }
        pending.clear();
        self.maybe_commit()
    }

    pub(super) fn write_pack(&mut self, write: &SelectedWrite) -> StorageResult<()> {
        if write.created {
            write::insert_pack(&self.connection, write.pack_id, &write.bytes)?;
            self.counters.packs_created += 1;
        } else {
            write::append_pack(&self.connection, write.pack_id, &write.bytes)?;
            self.counters.pack_appends += 1;
        }
        self.ceiling = self.ceiling.max(write.pack_id);
        self.transaction.rows += 1;
        self.transaction.bytes += write.bytes.len() as u64;
        Ok(())
    }
}
