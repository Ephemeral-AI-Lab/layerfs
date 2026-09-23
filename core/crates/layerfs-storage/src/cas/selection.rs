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
use crate::policy::{GROUP_CANONICAL_BYTES_LIMIT, GROUP_TARGET};

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
        let whole = std::time::Instant::now();
        if self.terminal {
            return Err(StorageError::Aborted);
        }
        let queries = {
            let arbitration = std::sync::Arc::clone(&self.arbitration);
            let _guard = crate::sqlite::ownership::lock_unless_held(&arbitration, self.wave_held)?;
            availability.validate(&self.connection, object, i64::MAX, |id| {
                self.pending_member(id)
            })?
        };
        self.counters.presence_queries = self.counters.presence_queries.saturating_add(queries);
        self.flush_queued_if_contains(advisory, availability)?;
        // **Only a placed row can be a delta base.** The whole-file lane's groups
        // are no longer sealed per record, so a member this operation has already
        // admitted is still waiting in an open group when a later offer asks
        // whether it can be a base. It has no row, and a selection that asks
        // storage would find nothing and silently lose the edge the winner cache
        // proposes. The group is therefore placed *before* selection is asked for
        // a representation for an object that could name one of its members: the
        // caller's advisory list first, then whatever the cache proposes for this
        // exact payload.
        //
        // The signature is computed here rather than there because this is the
        // only place that can act on the answer; it is handed to the selection, so
        // it is still computed exactly once per object, as it was when only the
        // selection computed it.
        let prehashed = self.pending_base_for(object, advisory)?;
        let record = self.select_record(object, advisory, prehashed)?;
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
            // The whole-file lane seals on the framed length it would write, like
            // the ordinary and native lanes: its groups carry many records now, so
            // a group is a framing unit rather than a single record. The pooled and
            // singleton lanes still hold exactly one record by grammar.
            PackLane::PooledMetadata | PackLane::Singleton => occupied,
            PackLane::Ordinary | PackLane::Native | PackLane::WholeFile => {
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
                    > GROUP_CANONICAL_BYTES_LIMIT as usize)
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
        if matches!(lane, PackLane::PooledMetadata | PackLane::Singleton) {
            self.seal_group(lane, availability)?;
        }
        crate::cas::owner::SaveProfile::charge(&mut self.profile.diag.offer_total_ns, whole);
        Ok(())
    }

    /// Places the open whole-file group when a later offer could name a member.
    ///
    /// Returns the offered payload's own signature when it had to be computed -
    /// the whole-file lane is the only lane whose records enter the winner cache -
    /// and `None` otherwise, so a selection that does not need it pays nothing.
    fn pending_base_for(
        &mut self,
        object: &FinalizedObject,
        advisory: &[ObjectId],
    ) -> StorageResult<Option<[u64; 8]>> {
        if PackLane::for_role(object.role()) != PackLane::WholeFile {
            return Ok(None);
        }
        // A listed predecessor is a base candidate this caller has already named,
        // and the cache can propose another. Both are questions only a placed row
        // can answer, so both are asked before selection runs.
        let listed = advisory.iter().any(|id| self.pending_member(*id));
        let raw = crate::encoding::raw_payload(object.canonical(), object.role())?;
        let signature = crate::encoding::delta::candidates::signature(raw);
        let proposed = {
            let candidates = self
                .candidates
                .lock()
                .map_err(|_| StorageError::Integrity("candidate index lock"))?;
            candidates.find(object.id(), &signature)
        };
        let proposed_pending = proposed.is_some_and(|id| self.pending_member(id));
        if listed || proposed_pending {
            let mut needed = advisory.to_vec();
            if let Some(id) = proposed {
                needed.push(id);
            }
            self.seal_pending(&needed)?;
        }
        Ok(Some(signature))
    }

    fn select_record(
        &mut self,
        object: &FinalizedObject,
        advisory: &[ObjectId],
        signature: Option<[u64; 8]>,
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
            wave_held: self.wave_held,
            capacities: &self.capacities,
            candidates: &mut candidates,
            depths: &mut self.depths,
            packs: &mut self.pack_cache,
            pool: &mut self.pool_reader,
            decode: &mut self.decompression,
            chain: &mut self.chain,
            chain_total: &mut self.chain_total,
            signature,
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
