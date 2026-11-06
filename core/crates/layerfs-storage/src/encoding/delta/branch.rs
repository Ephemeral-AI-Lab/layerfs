//! Selection of a program-page record: whole, or a COPY/INSERT program against
//! its predecessor. Inode-table branches since #360 item 3; directory leaves and
//! branches and extent leaves and branches since layerfs-project #92.
//!
//! The candidate is the first advisory predecessor the construction carries: the
//! page this one replaces (`sorted/page.rs` for directory and inode pages) or the
//! stored page at the same level and logical position (extent pages), when it is
//! stored with this page's role. Its chain must be below the metadata depth cap
//! and, with this page on it, within the metadata chain budgets; otherwise the
//! base is the whole page at the root of that chain (#89), so a chain at its
//! limits restarts at depth one instead of storing the page whole. The root must
//! fit the budgets in the same way. Exactly one trial runs, with the pooled
//! producer and its match budget; the program is kept only when it is shorter
//! than the whole record, and for a directory page only when it saves at least a
//! quarter of it (`worth`). Every refusal stores the page whole, which
//! is what every Store before this form holds.

use layerfs_content::{ObjectId, ObjectRole};

use crate::encoding::delta::select::{self, note_depth, SelectInput};
use crate::encoding::full::EncodedRecord;
use crate::encoding::pool::{branch::BranchChain, delta};
use crate::error::StorageResult;
use crate::location::ObjectLocation;
use crate::pack::layout::PackLane;
use crate::policy::StorageCapacities;
use crate::save::SaveProfile;
use std::time::Instant;

/// What program-page selection did for one role class, apart from the payload
/// counts (#360 item 3, #92).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BranchCounters {
    /// Branches offered.
    pub branches: u64,
    /// Branches stored as a program against their predecessor.
    pub delta_branches: u64,
    /// Branches stored whole.
    pub full_branches: u64,
    /// Programs built.
    pub trials: u64,
    /// Predecessors whose chain reached the depth cap; their root is the base.
    pub depth_capped: u64,
    /// Bases refused for the chain budgets.
    pub work_exceeded: u64,
    /// Predecessors replaced by the whole branch at the root of their chain (#89).
    pub rebased: u64,
    /// Branches stored, by chain depth (whole is 0).
    pub depths: super::select::DepthHistogram,
}

impl BranchCounters {
    /// Adds `other` to these counts.
    pub fn accumulate(&mut self, other: &Self) {
        for (total, value) in [
            (&mut self.branches, other.branches),
            (&mut self.delta_branches, other.delta_branches),
            (&mut self.full_branches, other.full_branches),
            (&mut self.trials, other.trials),
            (&mut self.depth_capped, other.depth_capped),
            (&mut self.work_exceeded, other.work_exceeded),
            (&mut self.rebased, other.rebased),
        ] {
            *total = total.saturating_add(value);
        }
        super::select::add_depths(&mut self.depths, &other.depths);
    }
}

/// The counters of `role`'s class: inode-table branches, directory pages or
/// extent pages.
fn counters(counters: &mut super::select::DeltaCounters, role: ObjectRole) -> &mut BranchCounters {
    match role {
        ObjectRole::DirectoryLeaf | ObjectRole::DirectoryBranch => &mut counters.directory,
        ObjectRole::ExtentLeaf | ObjectRole::ExtentBranch => &mut counters.extent,
        _ => &mut counters.branch,
    }
}

/// Chooses the record of one program page of `role` whose whole record is
/// `full`.
pub(super) fn select(
    input: &mut SelectInput<'_>,
    role: ObjectRole,
    canonical: &[u8],
    advisory: &[ObjectId],
    full: EncodedRecord,
) -> StorageResult<EncodedRecord> {
    let class = counters(input.counters, role);
    class.branches = class.branches.saturating_add(1);
    if let Some(record) = trial(input, role, canonical, advisory, &full)? {
        return Ok(record);
    }
    let branch = counters(input.counters, role);
    branch.full_branches = branch.full_branches.saturating_add(1);
    note_depth(&mut branch.depths, 0);
    Ok(full)
}

fn trial(
    input: &mut SelectInput<'_>,
    role: ObjectRole,
    canonical: &[u8],
    advisory: &[ObjectId],
    full: &EncodedRecord,
) -> StorageResult<Option<EncodedRecord>> {
    let Some(base_id) = advisory.first().copied() else {
        return Ok(None);
    };
    let Some(location) = input.connection.location(base_id, i64::MAX)? else {
        return Ok(None);
    };
    if location.role != role {
        return Ok(None);
    }
    let capacities = input.capacities;
    let mut base_id = base_id;
    let (mut base, mut chain) = acquire(input, location)?;
    let capped = chain.depth >= capacities.metadata_delta_max_depth;
    if capped {
        let branch = counters(input.counters, role);
        branch.depth_capped = branch.depth_capped.saturating_add(1);
    }
    if (capped || !fits(capacities, chain, canonical, full)) && chain.depth > 0 {
        // The predecessor's chain is at its limits: the one trial runs against the
        // whole branch at its root instead (#89).
        let started = Instant::now();
        let root = select::chain_root(input.connection, input.packs, input.decode, base_id);
        SaveProfile::charge(&mut input.profile.resolve.cost_ns, started);
        let root = root?;
        (base, chain) = acquire(input, root)?;
        if chain.depth != 0 {
            return Err(crate::error::StorageError::Integrity("branch chain root"));
        }
        base_id = root.object_id;
        let branch = counters(input.counters, role);
        branch.rebased = branch.rebased.saturating_add(1);
    } else if capped {
        return Ok(None);
    }
    let branch = counters(input.counters, role);
    if !fits(capacities, chain, canonical, full) {
        branch.work_exceeded = branch.work_exceeded.saturating_add(1);
        return Ok(None);
    }
    branch.trials = branch.trials.saturating_add(1);
    let mut budget = crate::policy::METADATA_MATCH_BUDGET_BYTES;
    let started = Instant::now();
    let program = delta::build(base_id, &base, canonical, &mut budget);
    SaveProfile::charge(&mut input.profile.delta_ns, started);
    let Some(program) = program? else {
        return Ok(None);
    };
    if !worth(role, program.len(), full.record.len()) {
        return Ok(None);
    }
    let branch = counters(input.counters, role);
    branch.delta_branches = branch.delta_branches.saturating_add(1);
    note_depth(&mut branch.depths, chain.depth.saturating_add(1));
    Ok(Some(EncodedRecord {
        lane: PackLane::Ordinary,
        record: program,
        canonical_length: canonical.len(),
        raw_length: canonical.len(),
        base: Some(base_id),
    }))
}

/// The authenticated page of one stored program page and the chain work it
/// charged.
fn acquire(
    input: &mut SelectInput<'_>,
    location: ObjectLocation,
) -> StorageResult<(Vec<u8>, BranchChain)> {
    let started = Instant::now();
    let mut groups = crate::encoding::GroupCache::new();
    let base = input.pool.branch_canonical_with_caches(
        input.connection,
        input.capacities,
        i64::MAX,
        input.decode,
        location,
        Some(crate::encoding::pool::read::PooledCaches {
            encoded: input.packs,
            decoded: &mut groups,
        }),
    );
    SaveProfile::charge(&mut input.profile.resolve.acquire_ns, started);
    base
}

/// True when a program of `program` bytes is kept over the whole record of
/// `whole` bytes.
///
/// An inode-table branch or an extent page keeps any shorter program: its rows
/// are mostly object identities, which no group compression shortens, so the
/// uncompressed comparison is the stored one. A directory page keeps a program
/// only when it saves at least a quarter of the whole record (#92): both forms
/// are compressed inside the same ordinary group, and on a small directory whose
/// rows all changed (every name renamed over) the whole page's names compress
/// better than a program of short copies and literals that is only marginally
/// shorter, so the program would store more bytes and still lengthen the chain.
fn worth(role: ObjectRole, program: usize, whole: usize) -> bool {
    match role {
        ObjectRole::DirectoryLeaf | ObjectRole::DirectoryBranch => {
            program.saturating_mul(4) <= whole.saturating_mul(3)
        }
        _ => program < whole,
    }
}

/// True when `chain` with this page on it stays within the metadata budgets.
fn fits(
    capacities: &StorageCapacities,
    chain: BranchChain,
    canonical: &[u8],
    full: &EncodedRecord,
) -> bool {
    chain.canonical.saturating_add(canonical.len() as u64)
        <= capacities.metadata_chain_canonical_limit
        && chain.encoded.saturating_add(full.record.len() as u64)
            <= capacities.metadata_chain_encoded_limit
}
