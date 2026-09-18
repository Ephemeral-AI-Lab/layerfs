//! One save mutation owner: the operation's state, its counters and the demands
//! it answers inside its own transaction.
//!
//! Ownership is acquired once with a single `BEGIN IMMEDIATE` attempt; a lost
//! write lock fails immediately instead of queueing. The owner keeps at most one
//! open tail per framing lane and writes packs and locators only inside its open
//! transaction. The responsibilities around that state live each in their own
//! file and extend this type from there: the pooled metadata lane
//! (`pool_lane`), framing, placement and sealing (`placement`), acquisition and
//! commit cadence (`lifecycle`), and representation selection (`selection`).

use rusqlite::Connection;

use layerfs_content::ObjectId;

use crate::cas::placement::PendingGroup;
use crate::cas::pool_lane::PoolCounters;
use crate::encoding::codec::{CompressionWorkspace, DecompressionWorkspace};
use crate::encoding::delta::candidates::Candidates;
use crate::encoding::delta::read::ChainCounters;
use crate::encoding::delta::select::{DeltaCounters, DepthCache};
use crate::error::StorageResult;
use crate::pack::placement::LanePlacement;
use crate::policy::StorageCapacities;
use crate::sqlite::lookup;
use crate::sqlite::write::TransactionState;
use std::collections::BTreeMap;

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
    /// Record-level objects newly written as a FULL representation.
    pub full_records: u64,
    /// Record-level objects newly written as a PREFIX representation.
    pub prefix_records: u64,
    /// `INSERT` statements issued for object rows.
    ///
    /// A statement counter, not a row counter: a multi-row `INSERT` of `k` rows
    /// is **one** statement. It is charged where the statement is issued, by the
    /// writer's own report, and it counts only the `objects` insert - pack
    /// writes, value-group inserts and transaction statements are already
    /// visible as `packs_created`, `pack_appends`, `pool.groups`, `transactions`
    /// and `commits`. Before batching, `statements == inserted` for a save that
    /// reuses nothing.
    pub statements: u64,
    /// Presence queries issued for offered objects' direct references.
    ///
    /// One charge per call into the paged presence lookup, which is the unit a
    /// wave-level batch removes: the lookup pages its own identifiers, so a call
    /// is one bounded query set however many references it carries.
    pub presence_queries: u64,
    /// Representation selection outcomes.
    pub delta: DeltaCounters,
    /// Work spent acquiring delta bases.
    pub chain: ChainCounters,
    /// Pooled metadata lane outcomes.
    pub pool: PoolCounters,
}

/// Exclusive writer state for one save operation.
pub struct MutationOwner {
    pub(super) connection: Connection,
    pub(super) capacities: StorageCapacities,
    pub(super) baseline_pack_id: i64,
    pub(super) next_pack_id: i64,
    /// Highest pack id this save created; becomes the publication watermark on
    /// acknowledgement and is left untouched on any failure.
    pub(super) ceiling: i64,
    pub(super) placement: [LanePlacement; 5],
    pub(super) groups: [PendingGroup; 5],
    /// Identities whose row this preparation wave has already written.
    ///
    /// A group is framed and placed as a whole, so one seal publishes rows for
    /// every member it holds - including members that arrived in earlier waves and
    /// are still waiting. The wave's membership snapshot is taken before any of
    /// that, so a member the seal just wrote would otherwise look absent for the
    /// rest of the wave and be offered a second time, which the `objects` primary
    /// key refuses. The lifetime is one wave: [`crate::cas::save::flush_batch`]
    /// clears it, and a row written in an earlier wave is already answered by that
    /// wave's own lookup. Bounded by the members one wave can seal, never by the
    /// size of the operation.
    pub(super) sealed_rows: Vec<ObjectId>,
    pub(super) transaction: TransactionState,
    pub(super) transaction_open: bool,
    pub(super) compression: CompressionWorkspace,
    pub(super) decompression: DecompressionWorkspace,
    pub(super) terminal: bool,
    pub(super) cleanup_attempted: bool,
    pub(super) quarantined: bool,
    pub(super) counters: OutcomeCounters,
    /// Admitted-FULL winner cache: owned by this operation, bounded and dropped
    /// with it, so a failed save can never leave a partly advanced cache usable.
    pub(super) candidates: Candidates,
    /// Bounded per-save dependency-depth cache.
    pub(super) depths: DepthCache,
    /// Pack bodies already read while acquiring delta bases in this operation.
    pub(super) pack_cache: BTreeMap<i64, Vec<u8>>,
    /// Work spent acquiring the base most recently resolved.
    pub(super) chain: ChainCounters,
    /// Work spent acquiring and reading every delta base of this operation.
    pub(super) chain_total: ChainCounters,
    /// Representation selection outcomes.
    pub(super) delta: DeltaCounters,
    /// Store-owned bounded ordered set of pooled value candidates.
    pub(super) pool_index: std::sync::Arc<std::sync::Mutex<crate::encoding::pool::PoolIndex>>,
    /// Bounded pooled-value reader used while synchronizing the index.
    pub(super) pool_reader: crate::encoding::pool::PoolReader,
    /// Ordinals this save assigned that the retained window does not answer yet.
    ///
    /// Bounded and reset per leaf; see `PENDING_VALUES_LIMIT`.
    pub(super) pending_values: BTreeMap<[u8; 73], u32>,
    /// Next ordinal this save may assign.
    pub(super) next_ordinal: Option<u32>,
    /// True once the index was synchronized inside this save.
    pub(super) pool_synced: bool,
    /// Pooled representation outcomes of this save.
    pub(super) pool: PoolCounters,
}

impl MutationOwner {
    /// Charges the presence queries a wave's availability check issued.
    pub fn note_presence_queries(&mut self, queries: u64) {
        self.counters.presence_queries = self.counters.presence_queries.saturating_add(queries);
    }

    /// Records one exact reuse occurrence.
    pub fn note_reuse(&mut self) {
        self.counters.reused += 1;
    }

    /// Reads objects inside this owner's transaction, with no ceiling.
    pub fn read_batch(&mut self, ids: &[ObjectId]) -> StorageResult<Vec<Vec<u8>>> {
        let mut groups = crate::encoding::GroupCache::new();
        let (values, _) = crate::cas::read::read_objects(
            &self.connection,
            ids,
            i64::MAX,
            &self.capacities,
            &mut self.decompression,
            &mut groups,
        )?;
        Ok(values)
    }

    /// Reconstructs one stored object, following and authenticating its chain.
    pub fn resolve_location(&mut self, location: lookup::ObjectLocation) -> StorageResult<Vec<u8>> {
        let value = {
            let mut groups = crate::encoding::GroupCache::new();
            let mut resolver = crate::encoding::delta::read::Resolver::new(
                &self.connection,
                i64::MAX,
                &self.capacities,
                &mut self.pack_cache,
                &mut groups,
                &mut self.decompression,
                &mut self.chain,
            );
            resolver.resolve_at(location)?.0
        };
        crate::encoding::delta::read::accumulate(&mut self.chain_total, self.chain);
        Ok(value)
    }
}
