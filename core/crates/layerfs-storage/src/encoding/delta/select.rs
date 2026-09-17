//! Candidate eligibility, one prefix trial and complete-cost selection.
//!
//! A missing object starts from a prepared compressed FULL alternative. A PREFIX
//! trial happens only when exactly one eligible candidate is acquired under the
//! role's depth, work and memory policy, and the selection compares the complete
//! framed record cost including the base identity. A candidate that is absent or
//! ineligible selects FULL by policy; a codec, allocation or read failure is
//! returned as a failure and never becomes an alternative representation.

use std::collections::BTreeMap;

use rusqlite::Connection;

use layerfs_content::{ObjectId, ObjectRole};

use crate::encoding::codec::{CompressionWorkspace, DecompressionWorkspace};
use crate::encoding::delta::candidates::{signature, Candidates};
use crate::encoding::delta::read::{ChainCounters, Resolver};
use crate::encoding::full::{encode_full, encode_prefix, raw_payload, EncodedRecord};
use crate::error::{StorageError, StorageResult};
use crate::pack::layout::PackLane;
use crate::policy::StorageCapacities;
use crate::sqlite::lookup;
use layerfs_content::MAXIMUM_DELTA_MAX_DEPTH;

/// Live entries of the per-save chain-depth cache.
const DEPTH_CACHE_ENTRIES: usize = 4_096;

/// What selection actually did for one object.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DeltaCounters {
    /// Objects whose FULL alternative was prepared.
    pub prepared_full: u64,
    /// PREFIX trials attempted.
    pub trials: u64,
    /// Objects stored as a PREFIX record.
    pub prefix_selected: u64,
    /// Trials that lost the cost comparison and stored FULL.
    pub full_losses: u64,
    /// Objects for which no candidate was supplied or found.
    pub no_candidate: u64,
    /// Supplied or cached candidates that were absent from storage.
    pub absent_candidates: u64,
    /// Candidates present but ineligible under the depth or role policy.
    pub ineligible_candidates: u64,
    /// Candidates refused because the resulting chain would exceed its budget.
    pub work_exceeded: u64,
}

/// Depth and canonical cost of one object's dependency chain.
///
/// `canonical` is exactly what a read of that object charges its chain budget: the
/// object itself and every dependency, so a producer can refuse to create a
/// dependency a later read could not reconstruct.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChainCost {
    /// Dependency edges below this object.
    pub depth: u8,
    /// Canonical bytes of this object and its dependencies.
    pub canonical: u64,
}

/// Bounded per-save cache of dependency depths and chain costs.
#[derive(Debug, Default)]
pub struct DepthCache {
    costs: BTreeMap<ObjectId, ChainCost>,
}

impl DepthCache {
    /// Empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Live entries held.
    pub fn len(&self) -> usize {
        self.costs.len()
    }

    /// True when no cost is cached yet.
    pub fn is_empty(&self) -> bool {
        self.costs.is_empty()
    }

    /// Depth of `id` in its dependency chain, or `None` when it is not stored.
    pub fn depth_of(&mut self, connection: &Connection, id: ObjectId) -> StorageResult<Option<u8>> {
        Ok(self.cost_of(connection, id)?.map(|cost| cost.depth))
    }

    /// Depth and canonical cost of `id`'s chain, or `None` when it is not stored.
    ///
    /// The walk is iterative and bounded by the profile maximum; a stored chain
    /// longer than that is corrupt and is reported rather than absorbed.
    pub fn cost_of(
        &mut self,
        connection: &Connection,
        id: ObjectId,
    ) -> StorageResult<Option<ChainCost>> {
        let mut path: Vec<(ObjectId, u64)> = Vec::new();
        let mut current = id;
        let cost = loop {
            if let Some(cost) = self.costs.get(&current).copied() {
                break cost;
            }
            let Some(location) = lookup::location(connection, current, i64::MAX)? else {
                return Ok(None);
            };
            path.push((current, location.canonical_length as u64));
            if path.len() > usize::from(MAXIMUM_DELTA_MAX_DEPTH) {
                return Err(StorageError::Integrity("stored dependency chain depth"));
            }
            match location.base_object_id {
                Some(base) => current = base,
                None => {
                    break ChainCost {
                        depth: 0,
                        canonical: 0,
                    }
                }
            }
        };
        // The walk stopped either at the chain root (`cost` zero, which is already
        // the root's own depth) or at an already known cost (that object's depth).
        // Recording walks back up the path, so the deepest element keeps the depth
        // it actually has and each ancestor below it adds exactly one edge.
        let mut level = cost;
        let mut result = cost;
        for (position, (id, own)) in path.iter().rev().enumerate() {
            let depth = cost
                .depth
                .saturating_add(u8::try_from(position).unwrap_or(u8::MAX));
            level = ChainCost {
                depth,
                canonical: level.canonical.saturating_add(*own),
            };
            self.record(*id, level);
            result = level;
        }
        Ok(Some(result))
    }

    /// Records the depth and chain cost of one identity.
    pub fn record(&mut self, id: ObjectId, cost: ChainCost) {
        if self.costs.len() >= DEPTH_CACHE_ENTRIES {
            // Bounded live capacity: the cache is dropped whole rather than grown.
            self.costs.clear();
        }
        self.costs.insert(id, cost);
    }
}

/// Everything one selection needs besides the object itself.
pub struct SelectInput<'a> {
    /// Open write connection: candidate lookups and base reads use it.
    pub connection: &'a Connection,
    /// Accepted capacities: frames, depths and chain budgets.
    pub capacities: &'a StorageCapacities,
    /// Admitted-FULL winner cache.
    pub candidates: &'a mut Candidates,
    /// Bounded chain-depth cache.
    pub depths: &'a mut DepthCache,
    /// Pack bodies this operation already read, bounded by
    /// [`crate::policy::DEPENDENCY_PACK_CACHE_BYTES`] and released wholesale when
    /// the next body would cross it.
    pub packs: &'a mut BTreeMap<i64, Vec<u8>>,
    /// Decode workspace for base reconstruction.
    pub decode: &'a mut DecompressionWorkspace,
    /// Work performed while acquiring the base just resolved.
    pub chain: &'a mut ChainCounters,
    /// Work performed while acquiring every base of this operation.
    ///
    /// A resolver reports one chain, so the operation's total is accumulated here
    /// as each acquisition completes; `chain` keeps the value of the chain just
    /// resolved, which is what the budget check below compares against.
    pub chain_total: &'a mut ChainCounters,
    /// Selection outcomes.
    pub counters: &'a mut DeltaCounters,
}

/// Chooses the physical representation of one canonical object.
///
/// `advisory` is the caller's bounded, explicitly declared candidate list in
/// preference order. A `WHOLE_FILE` object considers that list first and falls
/// back to the admitted-FULL winner cache only when no listed candidate is
/// acquired. A `CHUNK` object considers the first eligible listed candidate and
/// never the cache: its correspondence is already known by the caller.
pub fn select(
    input: &mut SelectInput<'_>,
    canonical: &[u8],
    role: ObjectRole,
    advisory: &[ObjectId],
    encode: &mut CompressionWorkspace,
) -> StorageResult<EncodedRecord> {
    if matches!(
        role,
        ObjectRole::ExtentLeaf
            | ObjectRole::ExtentBranch
            | ObjectRole::FileState
            | ObjectRole::InodeLeaf
    ) {
        return encode_full(canonical, role, input.capacities, encode);
    }
    let full = encode_full(canonical, role, input.capacities, encode)?;
    input.counters.prepared_full = input.counters.prepared_full.saturating_add(1);
    let lane = full.lane;
    let depth_cap = input.capacities.delta_depth_for_role(role);
    if depth_cap == 0 {
        if lane == PackLane::WholeFile {
            input.candidates.insert(
                ObjectId::for_bytes(canonical),
                signature(raw_payload(canonical, role)?),
            );
        }
        return Ok(full);
    }
    let raw = raw_payload(canonical, role)?;
    let candidate = match role {
        // The caller already knows the correspondence, so exactly the first
        // supplied candidate is considered - but only after the same eligibility
        // decision the whole-file lane applies. An absent or ineligible one
        // selects FULL by policy; it is never acquired and never fails the save.
        ObjectRole::Chunk => match advisory.first().copied() {
            Some(id) => match probe(input, id, role, depth_cap)? {
                true => Some(id),
                false => None,
            },
            None => None,
        },
        _ => match acquisition(input, role, advisory, depth_cap)? {
            Some(id) => Some(id),
            None => {
                let found = input
                    .candidates
                    .find(ObjectId::for_bytes(canonical), &signature(raw));
                match found {
                    Some(id) => match probe(input, id, role, depth_cap)? {
                        true => Some(id),
                        false => None,
                    },
                    None => None,
                }
            }
        },
    };
    let Some(base_id) = candidate else {
        input.counters.no_candidate = input.counters.no_candidate.saturating_add(1);
        if lane == PackLane::WholeFile {
            input
                .candidates
                .insert(ObjectId::for_bytes(canonical), signature(raw));
        }
        return Ok(full);
    };
    // Exactly one trial: the acquired base is read once, authenticated, and used
    // for one prefix frame. Nothing here retries with another candidate.
    let base = acquire(input, base_id)?;
    // A dependency that could not be reconstructed is never created: the chain
    // budget covers the base chain *and* the dependent. Refusing here stores FULL
    // by policy, so no stored object depends on bytes a later read could not
    // reconstruct.
    let chained = input
        .chain
        .canonical_bytes
        .saturating_add(canonical.len() as u64);
    let encoded = input
        .chain
        .encoded_bytes
        .saturating_add(canonical.len() as u64);
    if chained > input.capacities.chain_canonical_limit
        || encoded > input.capacities.chain_encoded_limit
    {
        input.counters.work_exceeded = input.counters.work_exceeded.saturating_add(1);
        if lane == PackLane::WholeFile {
            input
                .candidates
                .insert(ObjectId::for_bytes(canonical), signature(raw));
        }
        return Ok(full);
    }
    let base_raw = raw_payload(&base, role)?;
    input.counters.trials = input.counters.trials.saturating_add(1);
    let prefix = encode_prefix(canonical, role, base_id, base_raw, input.capacities, encode)?;
    if prefix.record.len() < full.record.len() {
        let base_cost = input
            .depths
            .cost_of(input.connection, base_id)?
            .ok_or(StorageError::Integrity("selected base is not stored"))?;
        input.depths.record(
            ObjectId::for_bytes(canonical),
            ChainCost {
                depth: base_cost.depth.saturating_add(1),
                canonical: base_cost.canonical.saturating_add(canonical.len() as u64),
            },
        );
        input.counters.prefix_selected = input.counters.prefix_selected.saturating_add(1);
        Ok(prefix)
    } else {
        input.counters.full_losses = input.counters.full_losses.saturating_add(1);
        if lane == PackLane::WholeFile {
            input
                .candidates
                .insert(ObjectId::for_bytes(canonical), signature(raw));
        }
        Ok(full)
    }
}

/// Decides one supplied candidate: present, of this role, and under the cap.
///
/// Every rejected candidate is counted, whether it was absent or present but
/// ineligible; neither ever becomes a failure of the operation.
fn probe(
    input: &mut SelectInput<'_>,
    id: ObjectId,
    role: ObjectRole,
    depth_cap: u8,
) -> StorageResult<bool> {
    if eligible(input, id, role, depth_cap)? {
        return Ok(true);
    }
    input.counters.ineligible_candidates = input.counters.ineligible_candidates.saturating_add(1);
    Ok(false)
}

fn acquisition(
    input: &mut SelectInput<'_>,
    role: ObjectRole,
    advisory: &[ObjectId],
    depth_cap: u8,
) -> StorageResult<Option<ObjectId>> {
    for id in advisory {
        if probe(input, *id, role, depth_cap)? {
            return Ok(Some(*id));
        }
    }
    Ok(None)
}

fn eligible(
    input: &mut SelectInput<'_>,
    id: ObjectId,
    role: ObjectRole,
    depth_cap: u8,
) -> StorageResult<bool> {
    let Some(location) = lookup::location(input.connection, id, i64::MAX)? else {
        input.counters.absent_candidates = input.counters.absent_candidates.saturating_add(1);
        return Ok(false);
    };
    if location.role != role {
        return Ok(false);
    }
    let Some(depth) = input.depths.depth_of(input.connection, id)? else {
        input.counters.absent_candidates = input.counters.absent_candidates.saturating_add(1);
        return Ok(false);
    };
    Ok(depth < depth_cap)
}

fn acquire(input: &mut SelectInput<'_>, id: ObjectId) -> StorageResult<Vec<u8>> {
    let value = {
        let mut resolver = Resolver::new(
            input.connection,
            i64::MAX,
            input.capacities,
            input.packs,
            input.decode,
            input.chain,
        );
        resolver.resolve_dependency(id)?
    };
    crate::encoding::delta::read::accumulate(input.chain_total, *input.chain);
    Ok(value)
}
