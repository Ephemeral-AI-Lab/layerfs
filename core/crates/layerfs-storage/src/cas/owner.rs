//! One save mutation owner: cursors, placement state and the write transaction.
//!
//! Ownership is acquired once with a single `BEGIN IMMEDIATE` attempt; a lost
//! write lock fails immediately instead of queueing. The owner tracks the pack and
//! group cursors it established at acquisition, keeps at most one open tail per
//! framing lane, and writes packs and locators only inside its open transaction.

use rusqlite::Connection;

use layerfs_content::{FinalizedObject, ObjectId, ObjectRole};

use crate::cas::dependencies::Availability;
use crate::encoding::codec::{CompressionWorkspace, DecompressionWorkspace, GROUP_LIMIT};
use crate::encoding::encode_full;
use crate::error::{StorageError, StorageResult};
use crate::pack::assemble::framed_length;
use crate::pack::layout::PackLane;
use crate::pack::placement::LanePlacement;
use crate::pack::{build_group, SelectedWrite};
use crate::policy::{StorageCapacities, StoragePolicy};
use crate::sqlite::lookup;
use crate::sqlite::write::{self, ObjectRow, TransactionState};

/// Soft group target: a group is sealed once the next record would pass it.
const GROUP_TARGET: usize = 48 * 1024;

/// Largest framed body one record may occupy in `lane`.
///
/// The compact whole-file lane stores a single record per group and is bounded by
/// its own frame limit, not by the multi-record ordinary group ceiling.
fn lane_body_limit(capacities: &StorageCapacities, lane: PackLane) -> usize {
    match lane {
        PackLane::WholeFile => capacities.whole_file_frame_limit + 1,
        PackLane::Ordinary | PackLane::Native => GROUP_LIMIT,
    }
}

/// One record waiting for its group to be framed and placed.
///
/// Only identifiers and lengths are retained, never the payload: a group is bounded
/// by its framed bytes, so retaining canonical bytes could hold far more than the
/// group target whenever records compress well. An identity that is still waiting
/// here has no row yet, which is why the owner can seal its group on demand and let
/// the ordinary verified storage path answer for it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingMember {
    object_id: ObjectId,
    role: ObjectRole,
    canonical_length: usize,
}

#[derive(Default)]
struct PendingGroup {
    records: Vec<Vec<u8>>,
    members: Vec<PendingMember>,
    framed_len: usize,
}

impl PendingGroup {
    fn clear(&mut self) {
        self.records.clear();
        self.members.clear();
        self.framed_len = 0;
    }
}

/// Counters describing what one save operation actually did.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OutcomeCounters {
    /// Occurrences served by an exact existing row.
    pub reused: u64,
    /// Objects newly written.
    pub inserted: u64,
    /// Packs created.
    pub packs_created: u64,
    /// Appends to a pack this save created.
    pub pack_appends: u64,
    /// Write transactions started.
    pub transactions: u64,
    /// Write transactions acknowledged with `COMMIT`.
    pub commits: u64,
}

/// Exclusive writer state for one save operation.
pub struct MutationOwner {
    connection: Connection,
    capacities: StorageCapacities,
    baseline_pack_id: i64,
    next_pack_id: i64,
    /// Highest pack id this save created; becomes the publication watermark on
    /// acknowledgement and is left untouched on any failure.
    ceiling: i64,
    placement: [LanePlacement; 3],
    groups: [PendingGroup; 3],
    transaction: TransactionState,
    transaction_open: bool,
    compression: CompressionWorkspace,
    decompression: DecompressionWorkspace,
    terminal: bool,
    cleanup_attempted: bool,
    quarantined: bool,
    counters: OutcomeCounters,
}

impl MutationOwner {
    /// Acquires ownership once and establishes the operation's cursors.
    pub fn acquire(
        connection: Connection,
        policy: StoragePolicy,
        capacities: StorageCapacities,
    ) -> StorageResult<Self> {
        // Ownership first: the baseline and cursor are only meaningful when no
        // other writer can publish a pack between reading them and using them.
        write::begin_immediate(&connection)?;
        let baseline_pack_id = lookup::highest_pack_id(&connection)?;
        let published = crate::sqlite::schema::retained_pack_ceiling(&connection)?;
        if published != baseline_pack_id {
            // Undeleted packs from a save whose cleanup did not complete. Writing
            // now would have to guess their ownership, so the save is refused.
            return Err(StorageError::UninspectedState {
                ceiling: published,
                highest_pack_id: baseline_pack_id,
            });
        }
        let next_pack_id = baseline_pack_id
            .checked_add(1)
            .ok_or(StorageError::Integrity("pack identifier overflow"))?;
        let compression = CompressionWorkspace::new()?;
        let decompression = DecompressionWorkspace::new()?;
        let _ = policy;
        Ok(Self {
            connection,
            capacities,
            baseline_pack_id,
            next_pack_id,
            ceiling: baseline_pack_id,
            placement: [
                LanePlacement::new(),
                LanePlacement::new(),
                LanePlacement::new(),
            ],
            groups: [
                PendingGroup::default(),
                PendingGroup::default(),
                PendingGroup::default(),
            ],
            transaction: TransactionState { rows: 1, bytes: 0 },
            transaction_open: true,
            compression,
            decompression,
            terminal: false,
            cleanup_attempted: false,
            quarantined: false,
            counters: OutcomeCounters {
                transactions: 1,
                ..OutcomeCounters::default()
            },
        })
    }

    /// Highest retained pack identifier before this operation.
    pub fn baseline_pack_id(&self) -> i64 {
        self.baseline_pack_id
    }

    /// Connection held by this owner; same-save reads use it directly.
    pub fn connection(&self) -> &Connection {
        &self.connection
    }

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

    /// Records one exact reuse occurrence.
    pub fn note_reuse(&mut self) {
        self.counters.reused += 1;
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

    /// Reads objects inside this owner's transaction, with no ceiling.
    pub fn read_batch(&mut self, ids: &[ObjectId]) -> StorageResult<Vec<Vec<u8>>> {
        let (values, _) = crate::cas::read::read_objects(
            &self.connection,
            ids,
            i64::MAX,
            &mut self.decompression,
        )?;
        Ok(values)
    }

    /// Reads the stored canonical bytes at one location inside this owner.
    pub fn stored_canonical(&mut self, location: lookup::ObjectLocation) -> StorageResult<Vec<u8>> {
        crate::cas::membership::stored_canonical(
            &self.connection,
            location,
            &mut self.decompression,
        )
    }

    /// Prepares and places one missing object.
    pub fn offer(
        &mut self,
        object: FinalizedObject,
        availability: &mut Availability,
    ) -> StorageResult<()> {
        if self.terminal {
            return Err(StorageError::Aborted);
        }
        availability.validate(&self.connection, &object, i64::MAX, |id| {
            self.pending_member(id)
        })?;
        let record = encode_full(object.canonical(), object.role(), &mut self.compression)?;
        let lane = record.lane;
        let index = lane.index();
        let body = framed_length(std::slice::from_ref(&record.record))?;
        let body_limit = lane_body_limit(&self.capacities, lane);
        if body > body_limit {
            return Err(StorageError::CapacityExceeded {
                what: "owner.record_body",
                limit: body_limit as u64,
                actual: body as u64,
            });
        }
        let occupied = !self.groups[index].records.is_empty();
        let must_seal = match lane {
            PackLane::WholeFile => occupied,
            PackLane::Ordinary | PackLane::Native => {
                occupied && self.groups[index].framed_len.saturating_add(body) > GROUP_TARGET
            }
        };
        if must_seal {
            self.seal_group(lane, availability)?;
        }
        let group = &mut self.groups[index];
        group.framed_len = group
            .framed_len
            .checked_add(body)
            .ok_or(StorageError::Integrity("group framing"))?;
        group.records.push(record.record);
        group.members.push(PendingMember {
            object_id: object.id(),
            role: object.role(),
            canonical_length: object.canonical_len(),
        });
        availability.inserted(object.id());
        if lane == PackLane::WholeFile {
            self.seal_group(lane, availability)?;
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
            write::insert_object(
                &self.connection,
                &ObjectRow {
                    object_id: member.object_id,
                    role: member.role.code(),
                    canonical_length: member.canonical_length,
                    base_object_id: None,
                    pack_id: write.pack_id,
                    group_number: placed.group_number,
                    record_number,
                },
            )?;
            availability.inserted(member.object_id);
            self.counters.inserted += 1;
            self.transaction.rows += 1;
            self.transaction.bytes += member.canonical_length as u64;
        }
        pending.clear();
        self.maybe_commit()
    }

    fn write_pack(&mut self, write: &SelectedWrite) -> StorageResult<()> {
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

    /// Commits and lazily restarts the shared write transaction when it is full.
    pub fn maybe_commit(&mut self) -> StorageResult<()> {
        if self.transaction_open
            && (self.transaction.rows >= self.capacities.transaction_rows
                || self.transaction.bytes >= self.capacities.transaction_bytes)
        {
            write::commit(&self.connection)?;
            // Clear the flag before the re-acquire: if the lock is lost here, no
            // transaction is open and cleanup must proceed to the deletion pass
            // instead of trying to roll back a transaction that does not exist.
            self.transaction_open = false;
            self.counters.commits += 1;
            write::begin_immediate(&self.connection)?;
            self.transaction_open = true;
            self.transaction = TransactionState { rows: 1, bytes: 0 };
            self.counters.transactions += 1;
        }
        Ok(())
    }

    /// Completes every remaining group and acknowledges the final transaction.
    ///
    /// An operation whose last transaction holds no write at all is released with
    /// `ROLLBACK`: no `COMMIT` is issued for an empty write.
    pub fn finish(&mut self) -> StorageResult<OutcomeCounters> {
        if self.terminal {
            return Err(StorageError::Aborted);
        }
        match self.finish_inner() {
            Ok(counters) => Ok(counters),
            Err(error) => {
                self.terminal = true;
                Err(error)
            }
        }
    }

    fn finish_inner(&mut self) -> StorageResult<OutcomeCounters> {
        let mut availability = Availability::default();
        for lane in PackLane::ALL {
            self.seal_group(lane, &mut availability)?;
        }
        let pending_rows = self.transaction.rows.saturating_sub(1);
        let has_pending = pending_rows > 0 || self.transaction.bytes > 0;
        // A save that created no pack has nothing to publish. It releases its
        // acquisition with ROLLBACK: no COMMIT is issued for an empty write.
        let publishes = self.ceiling > self.baseline_pack_id;
        if !self.transaction_open {
            return Ok(self.counters);
        }
        if !has_pending && !publishes {
            write::rollback(&self.connection)?;
            self.transaction_open = false;
            return Ok(self.counters);
        }
        if !has_pending {
            // An earlier bounded commit already published this save's last packs.
            // The watermark still needs its own acknowledgement, so the empty
            // acquisition is released and a dedicated final transaction carries
            // the publication. It is the last thing this save does; if it is lost,
            // the packs stay unpublished and the Store is uninspected rather than
            // silently exposed.
            write::rollback(&self.connection)?;
            write::begin_immediate(&self.connection)?;
            self.counters.transactions += 1;
        }
        // The watermark names the packs this save created. It advances only here,
        // so either the save's output and its watermark both become visible, or
        // neither does.
        crate::sqlite::schema::advance_retained_pack_ceiling(&self.connection, self.ceiling)?;
        write::commit(&self.connection)?;
        self.counters.commits += 1;
        self.transaction_open = false;
        Ok(self.counters)
    }

    /// Ends a failed operation with exactly one cleanup attempt.
    pub fn abandon(&mut self) -> StorageResult<()> {
        if self.cleanup_attempted || self.quarantined {
            return Ok(());
        }
        self.cleanup_attempted = true;
        self.terminal = true;
        if self.transaction_open {
            write::rollback(&self.connection)?;
            self.transaction_open = false;
        }
        crate::sqlite::cleanup::abandon(&self.connection, self.baseline_pack_id)?;
        Ok(())
    }

    /// Marks the operation terminal without touching storage.
    pub fn mark_terminal(&mut self) {
        self.terminal = true;
    }

    /// Marks an unproven outcome: affected writes stop and nothing is deleted.
    pub fn quarantine(&mut self) {
        self.terminal = true;
        self.quarantined = true;
    }
}
