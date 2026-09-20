//! Representation selection and its counters.
//!
//! One offered object becomes one record: a pooled inode leaf is admitted through
//! the pooled metadata lane, every other object is decided by the delta selector
//! against the caller's advisory candidates. A FULL alternative is a policy
//! outcome, never error recovery: a failed acquisition, codec call or allocation
//! fails the operation.

use layerfs_content::{FinalizedObject, ObjectId, ObjectRole};

use crate::cas::dependencies::Availability;
use crate::cas::owner::MutationOwner;
use crate::cas::placement::PendingMember;
use crate::encoding::delta::read::ChainCounters;
use crate::encoding::delta::select::{select, DeltaCounters, SelectInput};
use crate::error::{StorageError, StorageResult};
use crate::pack::layout::PackLane;
use crate::policy::GROUP_TARGET;

impl MutationOwner {
    /// Prepares and places one missing object.
    ///
    /// `advisory` is the caller's bounded, explicitly declared candidate list in
    /// preference order. Selection may store FULL by policy when no candidate is
    /// acquired, but a failed acquisition, codec call or allocation fails the
    /// operation: a stored FULL alternative is a policy outcome, never error
    /// recovery.
    pub fn offer(
        &mut self,
        object: &FinalizedObject,
        advisory: &[ObjectId],
        availability: &mut Availability,
    ) -> StorageResult<()> {
        if self.terminal {
            return Err(StorageError::Aborted);
        }
        let queries = {
            let _guard = crate::sqlite::ownership::lock(&self.arbitration)?;
            availability.validate(&self.connection, object, i64::MAX, |id| {
                self.pending_member(id)
            })?
        };
        self.counters.presence_queries = self.counters.presence_queries.saturating_add(queries);
        let record = self.select_record(object, advisory)?;
        let lane = record.lane;
        let index = lane.index();
        let body = crate::pack::assemble::framed_length(std::slice::from_ref(&record.record))?;
        let body_limit = crate::encoding::lane_body_limit(lane, &self.capacities);
        if body > body_limit && lane != PackLane::Singleton {
            return Err(StorageError::CapacityExceeded {
                what: "owner.record_body",
                limit: body_limit as u64,
                actual: body as u64,
            });
        }
        let occupied = !self.groups[index].records.is_empty();
        let must_seal = match lane {
            PackLane::WholeFile | PackLane::PooledMetadata | PackLane::Singleton => occupied,
            PackLane::Ordinary | PackLane::Native => {
                // The seal decision is the framed length the group *would* have,
                // projected through the same identity that frames it: the count,
                // its end offsets and the payload, not a per-record framed length
                // summed once per record.
                occupied
                    && crate::pack::assemble::framed_group_length(
                        self.groups[index].records.len() + 1,
                        self.groups[index].payload_len + record.record.len(),
                    )? > GROUP_TARGET
            }
        };
        if must_seal
            || (occupied
                && self.groups[index]
                    .canonical_bytes
                    .saturating_add(object.canonical_len())
                    > self.capacities.batch_bytes as usize)
        {
            self.seal_group(lane, availability)?;
        }
        let group = &mut self.groups[index];
        group.canonical_bytes = group
            .canonical_bytes
            .checked_add(object.canonical_len())
            .ok_or(StorageError::Integrity("group canonical bytes"))?;
        group.payload_len = group
            .payload_len
            .checked_add(record.record.len())
            .ok_or(StorageError::Integrity("group framing"))?;
        group.records.push(record.record);
        group.members.push(PendingMember {
            object_id: object.id(),
            role: object.role(),
            canonical_length: object.canonical_len(),
            base_object_id: record.base,
        });
        availability.inserted(object.id());
        if matches!(
            lane,
            PackLane::WholeFile | PackLane::PooledMetadata | PackLane::Singleton
        ) {
            self.seal_group(lane, availability)?;
        }
        Ok(())
    }

    fn select_record(
        &mut self,
        object: &FinalizedObject,
        advisory: &[ObjectId],
    ) -> StorageResult<crate::encoding::EncodedRecord> {
        if object.role() == ObjectRole::InodeLeaf {
            return self.select_pooled(object, advisory);
        }
        // The index is Store-owned so that it survives the save; the lock is held
        // for this one selection and never across a call into the codec or the
        // database, so a save's own exclusivity is what orders the writers.
        let mut candidates = self
            .candidates
            .lock()
            .map_err(|_| StorageError::Integrity("candidate index lock"))?;
        let mut input = SelectInput {
            connection: &self.connection,
            arbitration: &self.arbitration,
            capacities: &self.capacities,
            candidates: &mut candidates,
            depths: &mut self.depths,
            packs: &mut self.pack_cache,
            pool: &mut self.pool_reader,
            decode: &mut self.decompression,
            chain: &mut self.chain,
            chain_total: &mut self.chain_total,
            counters: &mut self.delta,
            profile: &mut self.profile,
        };
        select(
            &mut input,
            object.id(),
            object.canonical(),
            object.role(),
            advisory,
            &mut self.compression,
        )
    }

    /// Representation selection outcomes of this operation.
    pub fn delta_counters(&self) -> DeltaCounters {
        self.delta
    }

    /// Work spent acquiring delta bases in this operation.
    ///
    /// Every chain the operation acquired, not the last one it resolved.
    pub fn chain_counters(&self) -> ChainCounters {
        self.chain_total
    }

    /// Live bytes held by the bounded content-signature index.
    pub fn candidate_index_bytes(&self) -> usize {
        self.candidates
            .lock()
            .map(|index| index.live_bytes())
            .unwrap_or(0)
    }
}
