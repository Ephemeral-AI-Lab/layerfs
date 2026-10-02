//! The existing pooled-leaf representation selector over neutral physical access.
use std::time::Instant;

use layerfs_content::{ObjectId, ObjectRole};

use crate::{
    access::PackAccess,
    cas::{PoolCounters, SaveProfile},
    encoding::{
        delta::select::DepthCache, pool::PoolReader, DecompressionWorkspace, EncodedRecord,
    },
    error::{StorageError, StorageResult},
    pack::layout::PackLane,
    policy::StorageCapacities,
};

/// Caller-owned bounded pooled selection state, shared with its other storage work.
pub struct PooledSelectInput<'a> {
    /// Required metadata-domain placement and captured publication/private scope.
    pub access: &'a dyn PackAccess,
    /// Captured maximum eligible physical order.
    pub ceiling: i64,
    /// Frozen work/depth and encoding capacities.
    pub capacities: &'a StorageCapacities,
    /// Physically qualified chain-cost cache.
    pub depths: &'a mut DepthCache,
    /// Reused pooled physical/value reader.
    pub reader: &'a mut PoolReader,
    /// Existing decode arena, never duplicated by domain.
    pub decode: &'a mut DecompressionWorkspace,
    /// Pooled selection outcomes of this operation.
    pub counters: &'a mut PoolCounters,
    /// Existing disjoint work-cost profile.
    pub profile: &'a mut SaveProfile,
}

/// Selects the existing FULL or COPY/INSERT representation of a pooled body.
/// Ordinal assignment/group staging precede this call and remain the caller's.
/// Missing/ineligible/unprofitable bases choose FULL; acquired failures are terminal.
pub fn select_pooled(
    input: &mut PooledSelectInput<'_>,
    canonical_length: usize,
    body: &[u8],
    advisory: &[ObjectId],
) -> StorageResult<EncodedRecord> {
    if crate::encoding::pool::leaf::physical_length(canonical_length)? != body.len() {
        return Err(StorageError::Integrity("pooled canonical/body length"));
    }
    input.counters.leaves += 1;
    let started = Instant::now();
    let full = crate::encoding::pool::leaf::encode_full(body);
    SaveProfile::charge(&mut input.profile.full_ns, started);
    let full = full?;
    // One base acquisition, then one instruction trial. A missing or
    // ineligible base, or a losing comparison, stores the leaf in full.
    let started = Instant::now();
    let base = pool_base(input, advisory, canonical_length as u64, full.len() as u64);
    SaveProfile::charge(&mut input.profile.resolve.pooled_ns, started);
    let base = base?;
    let Some((base_id, base_body)) = base else {
        return Ok(pooled_full(
            input.counters,
            full,
            canonical_length,
            body.len(),
        ));
    };
    input.counters.trials += 1;
    let mut budget = crate::policy::METADATA_MATCH_BUDGET_BYTES;
    let started = Instant::now();
    let program = crate::encoding::pool::delta::build(base_id, &base_body, body, &mut budget);
    SaveProfile::charge(&mut input.profile.delta_ns, started);
    let program = program?;
    let Some(program) = program else {
        return Ok(pooled_full(
            input.counters,
            full,
            canonical_length,
            body.len(),
        ));
    };
    if program.len() >= full.len() {
        return Ok(pooled_full(
            input.counters,
            full,
            canonical_length,
            body.len(),
        ));
    }
    input.counters.delta_leaves += 1;
    Ok(crate::encoding::EncodedRecord {
        lane: PackLane::Ordinary,
        record: program,
        canonical_length,
        raw_length: body.len(),
        base: Some(base_id),
    })
}

fn pooled_full(
    counters: &mut PoolCounters,
    full: Vec<u8>,
    canonical_length: usize,
    raw_length: usize,
) -> EncodedRecord {
    counters.full_leaves += 1;
    EncodedRecord {
        lane: PackLane::Ordinary,
        record: full,
        canonical_length,
        raw_length,
        base: None,
    }
}

fn pool_base(
    input: &mut PooledSelectInput<'_>,
    advisory: &[ObjectId],
    target_canonical: u64,
    target_encoded: u64,
) -> StorageResult<Option<(ObjectId, Vec<u8>)>> {
    let depth_cap = input.capacities.metadata_delta_max_depth;
    if depth_cap == 0 {
        return Ok(None);
    }
    for id in advisory {
        let Some(location) = input.access.location(*id, input.ceiling)? else {
            continue;
        };
        if location.role != ObjectRole::InodeLeaf {
            continue;
        }
        // The walk reads each edge from its record through the owner's own
        // pooled reader, whose pack cache the acquisition below reuses.
        let pool = &mut *input.reader;
        let cost = input.depths.cost_of(
            input.access,
            input.decode,
            *id,
            |connection, workspace, location| pool.stored_base(connection, workspace, location),
        )?;
        let Some(cost) = cost else {
            continue;
        };
        if cost.depth >= depth_cap {
            continue;
        }
        // Both budgets are charged with what a read of the dependent would
        // actually pay: the chain's canonical sum from the depth walk, and -
        // because a record's width is what the reader charges - the base
        // chain's encoded bytes as the reader measured them plus this leaf's
        // own record width. Charging a worst-case per-record bound instead made
        // every accepted depth above fifteen unusable.
        let canonical = cost.canonical.saturating_add(target_canonical);
        if canonical > input.capacities.metadata_chain_canonical_limit {
            input.counters.work_exceeded = input.counters.work_exceeded.saturating_add(1);
            continue;
        }
        // The owner's own reader, not a fresh one per trial: its pack and
        // value caches are the point (a trial used to re-materialise the same
        // base packs), and its `chain_encoded_bytes` is the same charge a read
        // of the dependent will pay. What it must not do is serve a pack body
        // this save has since appended to, so every pack write releases the
        // reader's pack cache (see `write_pack`).
        let connection = input.access;
        let capacities = input.capacities;
        let body = input.reader.leaf_body(
            connection,
            capacities,
            input.ceiling,
            input.decode,
            location,
        )?;
        let encoded = input
            .reader
            .chain_encoded_bytes()
            .saturating_add(target_encoded);
        if encoded > input.capacities.metadata_chain_encoded_limit {
            input.counters.work_exceeded = input.counters.work_exceeded.saturating_add(1);
            continue;
        }
        return Ok(Some((*id, body)));
    }
    Ok(None)
}
