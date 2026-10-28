//! Pooled leaf selection: ordinal assignment for inode values and COPY/INSERT
//! selection against a stored base, through the ports.
//!
//! A value keeps the ordinal it already has in a base leaf the caller named.
//! A value no base holds gets a new ordinal from this Save's reserved block and
//! becomes one stored row, published before the leaf that names it. Nothing is
//! rebuilt from history: the only memory of earlier values is the base leaf
//! itself and a bounded memo of what this Save assigned.
use super::{source::WaveSource, state::State};
use crate::{
    encoding::{
        pool::{delta, leaf},
        EncodedRecord,
    },
    error::{StorageError, StorageResult},
    pack::layout::PackLane,
    policy::METADATA_INDEX_VALUES,
    source::Source,
};
use layerfs_content::{
    inode_leaf::{PoolingLeaf, INODE_VALUE_BYTES},
    FinalizedObject, ObjectId, ObjectRole,
};
use std::collections::{BTreeMap, BTreeSet};

/// One stored leaf the caller named as a likely predecessor.
struct Base {
    id: ObjectId,
    body: Vec<u8>,
    /// Chain depth and canonical work of reading it, as the depth walk charged.
    depth: u8,
    chain_canonical: u64,
    chain_encoded: u64,
}

/// Candidate filter of one value: eight bytes of each of its two digests.
fn fingerprint(value: &[u8; INODE_VALUE_BYTES]) -> u64 {
    let word = |from: usize| {
        let mut bytes = [0_u8; 8];
        bytes.copy_from_slice(&value[from..from + 8]);
        u64::from_le_bytes(bytes)
    };
    word(INODE_VALUE_BYTES - 64) ^ word(INODE_VALUE_BYTES - 32).rotate_left(29) ^ word(0)
}

impl State<'_> {
    pub(super) fn select_pooled(
        &mut self,
        object: &FinalizedObject,
        advisory: &[ObjectId],
    ) -> StorageResult<EncodedRecord> {
        let input = PoolingLeaf::decode(object.canonical())?;
        let mut known = BTreeMap::<[u8; INODE_VALUE_BYTES], u32>::new();
        let bases = self.pooled_bases(advisory, &mut known)?;
        // Values no base holds may still be ones this Save already assigned,
        // which is the common case while a whole tree is first saved.
        let mut candidates = BTreeSet::new();
        for row in input.rows() {
            if !known.contains_key(&row.value) {
                if let Some(ordinal) = self.value_memo.get(&fingerprint(&row.value)) {
                    candidates.insert(*ordinal);
                }
            }
        }
        if !candidates.is_empty() {
            let ordinals: Vec<u32> = candidates.into_iter().collect();
            let source = WaveSource {
                fetch: &self.storage.source,
                packer: &self.packer,
                signatures: &self.signatures,
                values: &self.values,
            };
            for (ordinal, value) in ordinals.iter().zip(source.inode_values(&ordinals)?) {
                known.entry(value).or_insert(*ordinal);
            }
        }
        let fresh_count = input
            .rows()
            .iter()
            .filter(|row| !known.contains_key(&row.value))
            .map(|row| row.value)
            .collect::<BTreeSet<_>>()
            .len();
        if fresh_count > 0
            && fresh_count as u64 > self.ordinal_end.saturating_sub(self.next_ordinal)
        {
            self.reserve_ordinals(fresh_count.max(self.storage.reservations.ordinals))?;
        }
        let mut ordinals = Vec::with_capacity(input.rows().len());
        for row in input.rows() {
            let ordinal = if let Some(ordinal) = known.get(&row.value) {
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
                known.insert(row.value, ordinal);
                self.values.insert(ordinal, row.value);
                // The memo is dropped whole at its bound: the cost of a miss is
                // one more stored row for a repeated value, never a failure.
                if self.value_memo.len() >= METADATA_INDEX_VALUES {
                    self.value_memo.clear();
                }
                self.value_memo
                    .entry(fingerprint(&row.value))
                    .or_insert(ordinal);
                self.pool_stats.new_values += 1;
                ordinal
            };
            ordinals.push(ordinal);
        }
        let body = input.body(&ordinals)?;
        let started = std::time::Instant::now();
        let full = leaf::encode_full(&body);
        super::SaveProfile::charge(&mut self.profile.full_ns, started);
        let full = full?;
        self.pool_stats.leaves += 1;
        let capacities = self.storage.capacities();
        let cap = capacities.metadata_delta_max_depth;
        for base in &bases {
            if base.depth >= cap {
                self.pool_stats.depth_capped += 1;
                continue;
            }
            if base
                .chain_canonical
                .saturating_add(object.canonical_len() as u64)
                > capacities.metadata_chain_canonical_limit
                || base.chain_encoded.saturating_add(full.len() as u64)
                    > capacities.metadata_chain_encoded_limit
            {
                self.pool_stats.work_exceeded += 1;
                continue;
            }
            self.pool_stats.trials += 1;
            let mut budget = crate::policy::METADATA_MATCH_BUDGET_BYTES;
            let started = std::time::Instant::now();
            let program = delta::build(base.id, &base.body, &body, &mut budget);
            super::SaveProfile::charge(&mut self.profile.delta_ns, started);
            if let Some(program) = program? {
                if program.len() < full.len() {
                    self.pool_stats.delta_leaves += 1;
                    let depth = base.depth.saturating_add(1);
                    crate::encoding::delta::select::note_depth(&mut self.pool_stats.depths, depth);
                    return Ok(EncodedRecord {
                        lane: PackLane::Ordinary,
                        record: program,
                        canonical_length: object.canonical_len(),
                        raw_length: body.len(),
                        base: Some(base.id),
                    });
                }
            }
            break;
        }
        self.pool_stats.full_leaves += 1;
        crate::encoding::delta::select::note_depth(&mut self.pool_stats.depths, 0);
        Ok(EncodedRecord {
            lane: PackLane::Ordinary,
            record: full,
            canonical_length: object.canonical_len(),
            raw_length: body.len(),
            base: None,
        })
    }
    /// Reads every stored inode leaf the caller named, authenticated, and adds
    /// the ordinal of each of its values to `known`. A base too deep to carry
    /// another delta still gives its rows their ordinals.
    fn pooled_bases(
        &mut self,
        advisory: &[ObjectId],
        known: &mut BTreeMap<[u8; INODE_VALUE_BYTES], u32>,
    ) -> StorageResult<Vec<Base>> {
        let capacities = self.storage.capacities();
        let source = WaveSource {
            fetch: &self.storage.source,
            packer: &self.packer,
            signatures: &self.signatures,
            values: &self.values,
        };
        let mut bases = Vec::new();
        for id in advisory {
            let Some(location) = source.location(*id, i64::MAX)? else {
                continue;
            };
            if location.role != ObjectRole::InodeLeaf {
                continue;
            }
            let reader = &mut self.pool;
            let groups = &mut self.groups;
            let packs = &mut self.packs;
            let Some(cost) = self.depths.cost_of(
                &source,
                &mut self.decode,
                *id,
                |source, workspace, location| {
                    // The pool reader keeps the encoded account of a pooled chain
                    // (`chain_encoded` below), so the walk measures no width.
                    reader
                        .stored_base_with_caches(
                            source,
                            workspace,
                            location,
                            Some(crate::encoding::pool::read::PooledCaches {
                                encoded: &mut *packs,
                                decoded: &mut *groups,
                            }),
                        )
                        .map(|base| crate::encoding::delta::read::ChainEdge { base, width: 0 })
                },
            )?
            else {
                continue;
            };
            let (body, canonical) = self.pool.leaf_parts_with_caches(
                &source,
                &capacities,
                i64::MAX,
                &mut self.decode,
                location,
                Some(crate::encoding::pool::read::PooledCaches {
                    encoded: &mut self.packs,
                    decoded: &mut self.groups,
                }),
            )?;
            let stored = PoolingLeaf::decode(&canonical)?;
            for (row, pooled) in stored.rows().iter().zip(leaf::rows(&body)?) {
                known.entry(row.value).or_insert(pooled.ordinal);
            }
            bases.push(Base {
                id: *id,
                body,
                depth: cost.depth,
                chain_canonical: cost.canonical,
                chain_encoded: self.pool.chain_encoded_bytes(),
            });
        }
        Ok(bases)
    }
}
