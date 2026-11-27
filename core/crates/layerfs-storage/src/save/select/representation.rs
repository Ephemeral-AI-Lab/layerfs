//! Unchanged representation selection and lane group limits.
use super::super::{prepare::source::WaveSource, publish::seal::Member, state::State, SaveProfile};
use crate::{
    encoding::{
        delta::{
            candidates::signature,
            record::{prefix_cannot_win, HEADER_BYTES},
            select::{acquires, prepare_full, select_after_trial, SelectInput},
        },
        encode_prefix, raw_payload, EncodedRecord,
    },
    error::{StorageError, StorageResult},
    location::Source,
    pack::{assemble, layout::PackLane},
    policy::{GROUP_CANONICAL_BYTES_LIMIT, GROUP_TARGET},
};
use layerfs_content::{FinalizedObject, ObjectId, ObjectRole};
impl State<'_> {
    /// Selects and places one new object. With `base`, the object was held
    /// until this Save published `base`, its ring proposal
    /// (`save/deferred.rs`), which is then tried after the declared
    /// candidates as the ring's proposal would be.
    pub(crate) fn offer(
        &mut self,
        object: &FinalizedObject,
        base: Option<ObjectId>,
    ) -> StorageResult<()> {
        for reference in object.references() {
            if !self.packer.pending(*reference)
                && self.packer.location(*reference).is_none()
                && self
                    .storage
                    .source
                    .location(*reference, i64::MAX)?
                    .is_none()
            {
                return Err(StorageError::MissingDependency {
                    object: object.id(),
                    reference: *reference,
                });
            }
        }
        let mut advisory: Vec<ObjectId> = object.predecessors().ids().collect();
        let declared = advisory.len();
        advisory.extend(base);
        self.packer
            .flush_if_contains(&advisory, &mut self.next_pack, self.pack_end)?;
        let mut physical_candidates = advisory.clone();
        let mut full = None;
        let mut ring_lost = false;
        let sig = if PackLane::for_role(object.role()) == PackLane::WholeFile {
            let sig = signature(raw_payload(object.canonical(), object.role())?);
            // An unloaded ring means this Save has inserted nothing, so what it
            // would propose is no object pending here; selection loads it if
            // its fallback needs it (#352 record 034, S1-b).
            let mut proposed = match self.candidates.needs_load() || base.is_some() {
                true => None,
                false => self.candidates.propose(object.id(), &sig),
            };
            // A proposal this Save still holds would be sealed and published
            // below before selection may use it. When no PREFIX record can beat
            // the payload's FULL record, selection answers FULL whatever the
            // base, so the proposal is dropped instead of publishing one wave
            // per similar file (layerfs-project #67). The FULL record is
            // prepared once and handed to selection.
            if proposed
                .is_some_and(|id| self.packer.pending(id) || self.packer.location(id).is_some())
            {
                let record = prepare_full(
                    object.canonical(),
                    object.role(),
                    &self.storage.capacities(),
                    &mut self.compression,
                    &mut self.profile,
                )?;
                if prefix_cannot_win(record.lane, record.record.len())
                    || self.declared_acquired(object.role(), &advisory[..declared])?
                {
                    // A declared candidate selection acquires is tried before
                    // the ring, which it then never asks (#810).
                    proposed = None;
                } else if let Some(id) = proposed {
                    // A base this Save stored FULL and still keeps is tried
                    // here, before anything is published (#810). A losing
                    // trial needs no base: FULL is stored now. A winning one
                    // holds the object until the wave's own publication has
                    // published the base.
                    match self.pending_trial(object, id, &record)? {
                        Some(false) => {
                            proposed = None;
                            ring_lost = true;
                        }
                        Some(true) if self.deferred.hold(object, id) => return Ok(()),
                        _ => {}
                    }
                }
                full = Some(record);
            }
            let mut needed = advisory.clone();
            needed.extend(proposed);
            physical_candidates.extend(proposed);
            self.packer.seal_pending(
                &needed,
                &mut self.compression,
                &mut self.next_pack,
                self.pack_end,
            )?;
            if let Some(id) = proposed {
                self.storage.source.locate(&[id])?;
            }
            Some(sig)
        } else {
            None
        };
        // A pending base must have an acknowledged first-wins representation
        // before selection charges its chain cost. A concurrent winner may carry
        // a deeper PREFIX chain than this producer's private FULL copy.
        if physical_candidates
            .iter()
            .any(|id| self.packer.location(*id).is_some())
        {
            self.flush_signatures()?;
            self.register_ready(false)?;
        }
        let view = WaveSource {
            fetch: &self.storage.source,
            packer: &self.packer,
            signatures: &self.signatures,
            values: &self.values,
        };
        let capacities = self.storage.capacities();
        let mut input = SelectInput {
            connection: &view,
            capacities: &capacities,
            candidates: &mut self.candidates,
            depths: &mut self.depths,
            losing: &mut self.losing,
            packs: &mut self.packs,
            pool: &mut self.pool,
            decode: &mut self.decode,
            chain: &mut self.chain,
            chain_total: &mut self.chain_total,
            signature: sig,
            full,
            counters: &mut self.delta,
            profile: &mut self.profile,
        };
        let record = if object.role() == ObjectRole::InodeLeaf {
            self.select_pooled(object, &advisory)?
        } else {
            select_after_trial(
                &mut input,
                object.id(),
                object.canonical(),
                object.role(),
                &advisory,
                &mut self.compression,
                ring_lost,
            )?
        };
        let lane = record.lane;
        // A whole-file object stored FULL may be the next similar file's ring
        // proposal while it is still unpublished; its bytes are kept for that
        // trial (`pending_trial`).
        if lane == PackLane::WholeFile && record.base.is_none() {
            self.stored_full.keep(object.id(), object.canonical());
        }
        let body = assemble::framed_length(std::slice::from_ref(&record.record))?;
        let body_limit = crate::encoding::lane_body_limit(lane, &capacities);
        if body > body_limit && lane != PackLane::Singleton {
            return Err(StorageError::CapacityExceeded {
                what: "owner.record_body",
                limit: body_limit as u64,
                actual: body as u64,
            });
        }
        let group = &self.packer.groups[lane.index()];
        let occupied = !group.records.is_empty();
        if occupied
            && (lane == PackLane::Singleton
                || assemble::framed_group_length(
                    group.records.len() + 1,
                    group.payload + record.record.len(),
                )? > GROUP_TARGET
                || group.canonical.saturating_add(object.canonical_len())
                    > GROUP_CANONICAL_BYTES_LIMIT as usize)
        {
            self.packer.seal(
                lane,
                &mut self.compression,
                &mut self.next_pack,
                self.pack_end,
            )?;
        }
        let mut references = object.references().to_vec();
        references.extend(record.base);
        references.sort_unstable();
        references.dedup();
        let group = &mut self.packer.groups[lane.index()];
        group.payload += record.record.len();
        group.canonical += object.canonical_len();
        group.records.push(record.record);
        group.members.push(Member {
            id: object.id(),
            role: object.role(),
            length: object.canonical_len(),
            references,
            base: record.base,
        });
        if lane == PackLane::Singleton {
            self.packer.seal(
                lane,
                &mut self.compression,
                &mut self.next_pack,
                self.pack_end,
            )?;
        }
        Ok(())
    }

    /// Whether this Save's one trial of `object` against `base`, an object
    /// it stored FULL and has not published, would keep the PREFIX record
    /// rather than `full`; `None` when that is not decided here.
    ///
    /// The trial is the one selection would run once `base` was published: a
    /// FULL base is its own chain root at depth zero, so no re-basing applies,
    /// and the PREFIX record is kept only when it is shorter than `full`. A
    /// base whose bytes are no longer kept, a role without payload deltas, or
    /// a pair near a chain budget is not decided here: it is published first,
    /// as before. A losing trial stores FULL with no publication. A concurrent
    /// Save that publishes the same base first with a deeper chain could only
    /// have re-based that trial onto another root; FULL is then stored without
    /// it. A winning trial runs again once the base is published, against
    /// whatever representation won, under the same depth and work budgets.
    fn pending_trial(
        &mut self,
        object: &FinalizedObject,
        base: ObjectId,
        full: &EncodedRecord,
    ) -> StorageResult<Option<bool>> {
        let capacities = self.storage.capacities();
        if capacities.delta_depth_for_role(object.role()) == 0 {
            return Ok(None);
        }
        let Some(canonical) = self.stored_full.get(base) else {
            return Ok(None);
        };
        let chained = (canonical.len() + object.canonical_len() + HEADER_BYTES) as u64;
        if chained > capacities.chain_canonical_limit || chained > capacities.chain_encoded_limit {
            return Ok(None);
        }
        let started = std::time::Instant::now();
        let prefix = encode_prefix(
            object.canonical(),
            object.role(),
            base,
            raw_payload(&canonical, object.role())?,
            &capacities,
            &mut self.compression,
            &mut self.profile,
        );
        SaveProfile::charge(&mut self.profile.delta_ns, started);
        Ok(Some(prefix?.record.len() < full.record.len()))
    }

    /// True when selection of a `role` object will acquire one of its
    /// declared candidates `advisory`, all of them published before this
    /// Save, so it never asks the similarity ring.
    ///
    /// The probe is selection's own (`encoding::delta::select::acquires`) on
    /// the same view, with its outcome counts discarded: selection counts
    /// them when it probes again. A declared candidate this Save still holds
    /// is not decided here.
    fn declared_acquired(
        &mut self,
        role: ObjectRole,
        advisory: &[ObjectId],
    ) -> StorageResult<bool> {
        if advisory.is_empty()
            || advisory
                .iter()
                .any(|id| self.packer.pending(*id) || self.packer.location(*id).is_some())
        {
            return Ok(false);
        }
        let view = WaveSource {
            fetch: &self.storage.source,
            packer: &self.packer,
            signatures: &self.signatures,
            values: &self.values,
        };
        let capacities = self.storage.capacities();
        let mut counters = crate::encoding::delta::select::DeltaCounters::default();
        let mut input = SelectInput {
            connection: &view,
            capacities: &capacities,
            candidates: &mut self.candidates,
            depths: &mut self.depths,
            losing: &mut self.losing,
            packs: &mut self.packs,
            pool: &mut self.pool,
            decode: &mut self.decode,
            chain: &mut self.chain,
            chain_total: &mut self.chain_total,
            signature: None,
            full: None,
            counters: &mut counters,
            profile: &mut self.profile,
        };
        acquires(&mut input, role, advisory)
    }
}
