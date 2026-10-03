//! Unchanged representation selection and lane group limits.
use super::{seal::Member, source::WaveSource, state::State};
use crate::{
    encoding::{
        delta::{
            candidates::signature,
            select::{select, SelectInput},
        },
        raw_payload,
    },
    error::{StorageError, StorageResult},
    pack::{assemble, layout::PackLane},
    policy::{GROUP_CANONICAL_BYTES_LIMIT, GROUP_TARGET},
    source::Source,
};
use layerfs_content::{FinalizedObject, ObjectId, ObjectRole};
impl State<'_> {
    pub(super) fn offer(&mut self, object: &FinalizedObject) -> StorageResult<()> {
        if object.role() == ObjectRole::InodeLeaf {
            return Err(StorageError::UnsupportedPolicy {
                field: "pooled metadata admission",
            });
        }
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
        let advisory: Vec<ObjectId> = object.predecessors().ids().collect();
        self.packer
            .flush_if_contains(&advisory, &mut self.next_pack, self.pack_end)?;
        let mut physical_candidates = advisory.clone();
        let sig = if PackLane::for_role(object.role()) == PackLane::WholeFile {
            let sig = signature(raw_payload(object.canonical(), object.role())?);
            let proposed = self.candidates.find(object.id(), &sig);
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
            self.register_ready()?;
        }
        let view = WaveSource {
            fetch: &self.storage.source,
            packer: &self.packer,
            signatures: &self.signatures,
        };
        let arbitration = std::sync::Mutex::new(());
        let capacities = self.storage.capacities();
        let mut input = SelectInput {
            connection: &view,
            arbitration: &arbitration,
            wave_held: true,
            capacities: &capacities,
            candidates: &mut self.candidates,
            depths: &mut self.depths,
            packs: &mut self.packs,
            pool: &mut self.pool,
            decode: &mut self.decode,
            chain: &mut self.chain,
            chain_total: &mut self.chain_total,
            signature: sig,
            counters: &mut self.delta,
            profile: &mut self.profile,
        };
        let record = select(
            &mut input,
            object.id(),
            object.canonical(),
            object.role(),
            &advisory,
            &mut self.compression,
        )?;
        let lane = record.lane;
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
            prefix: record.base.is_some(),
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
}
