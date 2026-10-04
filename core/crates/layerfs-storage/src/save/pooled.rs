//! Existing pooled value assignment and COPY/INSERT selection through the ports.
use super::{source::WaveSource, state::State};
use crate::{
    encoding::{
        pool::{delta, leaf, value_group},
        EncodedRecord,
    },
    error::{StorageError, StorageResult},
    pack::layout::PackLane,
    policy::{METADATA_INDEX_VALUES, ORDINAL_BLOCK_LEAVES, ORDINAL_RESERVE_AFTER},
    source::Source,
};
use layerfs_content::{
    inode_leaf::{PoolingLeaf, INODE_VALUE_BYTES},
    FinalizedObject, ObjectId, ObjectRole,
};
use std::collections::{BTreeMap, BTreeSet};
impl State<'_> {
    pub(super) fn sync_pool(&mut self) -> StorageResult<()> {
        if self.pool_synced {
            return Ok(());
        }
        self.storage.source.refresh_window();
        let capacities = self.storage.capacities();
        self.pool_index.sync(
            &self.storage.source,
            &capacities,
            i64::MAX,
            &mut self.pool,
            &mut self.decode,
        )?;
        self.pool_synced = true;
        Ok(())
    }
    pub(super) fn select_pooled(
        &mut self,
        object: &FinalizedObject,
        advisory: &[ObjectId],
    ) -> StorageResult<(EncodedRecord, Vec<u32>)> {
        self.sync_pool()?;
        let input = PoolingLeaf::decode(object.canonical())?;
        let mut seen = BTreeSet::new();
        let unknown: Vec<_> = input
            .rows()
            .iter()
            .filter_map(|row| seen.insert(row.value).then_some(row.value))
            .collect();
        let capacities = self.storage.capacities();
        let source = WaveSource {
            fetch: &self.storage.source,
            packer: &self.packer,
            signatures: &self.signatures,
        };
        let known = self.pool_index.find(
            &source,
            &capacities,
            i64::MAX,
            &mut self.pool,
            &mut self.decode,
            &unknown,
        )?;
        let fresh_count = unknown
            .iter()
            .filter(|value| !known.contains_key(*value))
            .count();
        if fresh_count > 0
            && fresh_count as u64 > self.ordinal_end.saturating_sub(self.next_ordinal)
        {
            let block = if self.ordinal_reservations < ORDINAL_RESERVE_AFTER {
                fresh_count
            } else {
                fresh_count.saturating_mul(ORDINAL_BLOCK_LEAVES)
            };
            self.reserve_ordinals(block, 1)?;
        }
        let mut memo = BTreeMap::<[u8; INODE_VALUE_BYTES], u32>::new();
        let mut fresh = Vec::new();
        let mut ordinals = Vec::with_capacity(input.rows().len());
        let mut first = None;
        for row in input.rows() {
            let ordinal =
                if let Some(ordinal) = memo.get(&row.value).or_else(|| known.get(&row.value)) {
                    self.pool_stats.reused_values += 1;
                    *ordinal
                } else {
                    if self.next_ordinal >= self.ordinal_end {
                        return Err(StorageError::Integrity(
                            "metadata ordinal reservation exhausted",
                        ));
                    }
                    let ordinal = u32::try_from(self.next_ordinal)
                        .map_err(|_| StorageError::Integrity("metadata ordinal maximum"))?;
                    self.next_ordinal += 1;
                    first.get_or_insert(ordinal);
                    memo.insert(row.value, ordinal);
                    fresh.push(row.value);
                    self.pool_stats.new_values += 1;
                    ordinal
                };
            ordinals.push(ordinal);
        }
        if let Some(first) = first {
            if self.pool_index.len() + fresh.len() > METADATA_INDEX_VALUES {
                self.pool_index.advance_window(first);
                self.window_change = Some(first);
            }
            for (number, values) in fresh.chunks(crate::policy::VALUES_PER_GROUP).enumerate() {
                let first = first
                    .checked_add((number * crate::policy::VALUES_PER_GROUP) as u32)
                    .ok_or(StorageError::Integrity("metadata ordinal maximum"))?;
                let canonical = values
                    .iter()
                    .map(value_group::canonical_value)
                    .collect::<StorageResult<Vec<_>>>()?;
                let started = std::time::Instant::now();
                let group = value_group::build(&canonical, &mut self.compression);
                super::SaveProfile::charge(&mut self.profile.group_ns, started);
                let group = group?;
                self.packer
                    .add_values(first, group, &mut self.next_pack, self.pack_end)?;
                self.pool.release_packs();
                self.pool_index.note_group(first, values)?;
                self.pool_stats.groups += 1;
            }
        }
        let body = input.body(&ordinals)?;
        let started = std::time::Instant::now();
        let full = leaf::encode_full(&body);
        super::SaveProfile::charge(&mut self.profile.full_ns, started);
        let full = full?;
        self.pool_stats.leaves += 1;
        if let Some((id, base)) =
            self.pooled_base(advisory, object.canonical_len() as u64, full.len() as u64)?
        {
            self.pool_stats.trials += 1;
            let mut budget = crate::policy::METADATA_MATCH_BUDGET_BYTES;
            let started = std::time::Instant::now();
            let program = delta::build(id, &base, &body, &mut budget);
            super::SaveProfile::charge(&mut self.profile.delta_ns, started);
            if let Some(program) = program? {
                if program.len() < full.len() {
                    self.pool_stats.delta_leaves += 1;
                    return Ok((
                        EncodedRecord {
                            lane: PackLane::Ordinary,
                            record: program,
                            canonical_length: object.canonical_len(),
                            raw_length: body.len(),
                            base: Some(id),
                        },
                        ordinals,
                    ));
                }
            }
        }
        self.pool_stats.full_leaves += 1;
        Ok((
            EncodedRecord {
                lane: PackLane::Ordinary,
                record: full,
                canonical_length: object.canonical_len(),
                raw_length: body.len(),
                base: None,
            },
            ordinals,
        ))
    }
    fn pooled_base(
        &mut self,
        advisory: &[ObjectId],
        canonical: u64,
        encoded: u64,
    ) -> StorageResult<Option<(ObjectId, Vec<u8>)>> {
        let capacities = self.storage.capacities();
        let cap = capacities.metadata_delta_max_depth;
        if cap == 0 {
            return Ok(None);
        }
        let source = WaveSource {
            fetch: &self.storage.source,
            packer: &self.packer,
            signatures: &self.signatures,
        };
        for id in advisory {
            let Some(location) = source.location(*id, i64::MAX)? else {
                continue;
            };
            if location.role != ObjectRole::InodeLeaf {
                continue;
            }
            let reader = &mut self.pool;
            let Some(cost) = self.depths.cost_of(
                &source,
                &mut self.decode,
                *id,
                |source, workspace, location| reader.stored_base(source, workspace, location),
            )?
            else {
                continue;
            };
            if cost.depth >= cap {
                continue;
            }
            if cost.canonical.saturating_add(canonical) > capacities.metadata_chain_canonical_limit
            {
                self.pool_stats.work_exceeded += 1;
                continue;
            }
            let body =
                self.pool
                    .leaf_body(&source, &capacities, i64::MAX, &mut self.decode, location)?;
            if self.pool.chain_encoded_bytes().saturating_add(encoded)
                > capacities.metadata_chain_encoded_limit
            {
                self.pool_stats.work_exceeded += 1;
                continue;
            }
            return Ok(Some((*id, body)));
        }
        Ok(None)
    }
}
