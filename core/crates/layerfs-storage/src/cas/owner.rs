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
use std::time::Instant;

/// Nanosecond cost split of one save operation's accept path.
///
/// Seven disjoint buckets, each charged at the call site that does that kind of
/// work. The profile is an **aggregate over the whole operation**, never a span
/// per object: a stride1 row accepts about 10^5 objects and the recorder refuses a
/// node per object, so the cost accumulates into seven `u64` fields and is
/// published once per state. Charging costs one `Instant::now()` pair per site.
///
/// Every charged interval is disjoint from every other: no bucket contains
/// another, and a bucket's sites are the only places that kind of work happens.
/// The instrument measures the accept path, so the caller's own per-object work
/// (presence validation, group assembly outside the codec, `raw_payload`) is
/// deliberately outside all seven and is reported as the remainder.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SaveProfile {
    /// The disjoint parts of resolution.
    pub resolve: ResolveProfile,
    /// FULL representation encode: the ordinary lane's tree-role and payload
    /// frames, and the pooled lane's leaf body.
    pub full_ns: u64,
    /// Delta representation encode: ordinary prefix frames and pooled
    /// COPY/INSERT program construction.
    pub delta_ns: u64,
    /// Group codec: ordinary group framing and pooled value-group compression.
    pub group_ns: u64,
    /// Pack placement: lane selection and the write it produces.
    pub place_ns: u64,
    /// SQL: object rows, pack bodies, value-group rows, the content-signature
    /// flush and the publication watermark.
    pub sql_ns: u64,
    /// Transaction cadence: `COMMIT`, `ROLLBACK`, and the `BEGIN IMMEDIATE` that
    /// restarts a bounded transaction.
    pub commit_ns: u64,
    /// Diagnostic span totals of the regions the seven buckets do **not** charge.
    ///
    /// Every field is a **total** of a named region, so a region that contains one
    /// or more of the seven buckets contains their charges too and the residue the
    /// region leaves unnamed is the difference. It is a measurement of the
    /// instrument, never an eighth bucket: nothing here is added into
    /// [`total_ns`](Self::total_ns), no decision reads it, and a build without it
    /// runs the same work. It exists because the campaign's own remainder (31 % of
    /// the accept span) was unattributable by name, and a remainder cannot be
    /// optimized or defended while nothing says what is in it.
    pub diag: DiagProfile,
    /// Occurrences of the exact-reuse verification whose identity had already been
    /// verified once earlier in this same operation (B1's population).
    ///
    /// A **count**, not a duration, and deliberately not an eighth time bucket:
    /// [`total_ns`](Self::total_ns) still sums the seven durations, so the split
    /// published beside this figure is unchanged by it. It exists because "how
    /// much of `resolve.reuse` is a repeat of work this operation already did"
    /// cannot be answered from any duration - a first verification and a repeat
    /// cost the same - and a treatment that memoizes a verification has to know
    /// its population before it is pre-registered. It is zero unless the
    /// measurement-only probe is enabled (see `cas::owner::reuse_probe`), so an
    /// ordinary save is unaffected; `SaveOutcome`'s equality already excludes this
    /// whole struct, so enabling it cannot make a determinism comparison fail.
    pub reuse_repeat: u64,
    /// Payload records this save stored verbatim rather than as a codec frame.
    ///
    /// A **count**, like [`reuse_repeat`](Self::reuse_repeat) and deliberately not
    /// an eighth time bucket: [`total_ns`](Self::total_ns) still sums the seven
    /// durations, so the split published beside this figure is unchanged by it. It
    /// exists because "the probe stopped compressing" and "the store stopped
    /// writing frames" are different claims, and only the second one is the
    /// treatment: a count taken from each record's own tag says which records a
    /// reader will take the stored path for.
    pub stored_records: u64,
}

/// Diagnostic span totals of the accept path's uncharged regions.
///
/// One field per named region, each a **sum of nanoseconds over the operation**,
/// charged with the same `Instant` pair discipline as the seven buckets. Regions
/// nest: `seal_total_ns` contains `group_ns`, `place_ns`, `write_pack_total_ns`,
/// `sql_ns` (the object insert), `validate_ns`, `rows_ns`, `members_ns` and
/// `commit_total_ns`. Reading a residue therefore means subtracting the children
/// from the parent, and the analysis that does so publishes both numbers.
///
/// These are diagnostics. They are published beside the profile and never folded
/// into it; `SaveOutcome`'s equality ignores them exactly as it ignores `profile`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DiagProfile {
    /// Whole `maybe_commit` call on every step.
    pub commit_total_ns: u64,
    /// `BEGIN IMMEDIATE` and the pack-watermark read of every step.
    pub begin_ns: u64,
    /// Whole `seal_group` call.
    pub seal_total_ns: u64,
    /// Whole `offer` call, selection included.
    pub offer_total_ns: u64,
    /// Whole `write_pack` call.
    pub write_pack_total_ns: u64,
    /// Whole `validate_candidates` call.
    pub validate_ns: u64,
    /// The per-row candidate queries inside `validate_candidates`.
    pub collision_query_ns: u64,
    /// `ObjectRow` vector construction for one seal.
    pub rows_ns: u64,
    /// The member/counter loop that follows one seal's insert.
    pub members_ns: u64,
    /// Whole `SaveOperation::accept` call, seen from the Store handle.
    pub accept_plumbing_ns: u64,
    /// Whole `flush_batch` call.
    pub flush_batch_ns: u64,
    /// The wave's locator query and presence seed inside `flush_batch`.
    pub wave_ns: u64,
    /// Whole `finish_inner` call.
    pub finish_total_ns: u64,
    /// The publication statement inside `finish_inner`.
    pub publish_ns: u64,
    /// The final `flush(remaining)` drain inside `SaveOperation::finish`.
    ///
    /// `span_finish_ns` is 257.5 ms on the round-1 row while `finish_inner` is
    /// 1.2 ms, and the drain was bounded from above at 0.8 ms by subtraction. The
    /// three parts of that span therefore had to be charged rather than inferred:
    /// this one, [`finish_drop_ns`](Self::finish_drop_ns) and
    /// [`finish_call_ns`](Self::finish_call_ns).
    pub finish_drain_ns: u64,
    /// Dropping the save's owner: connection, codec workspaces, retained pack
    /// tails, and the save's private candidate and pool index clones.
    pub finish_drop_ns: u64,
    /// The whole `SaveOperation::finish` call, from entry to return.
    pub finish_call_ns: u64,
    /// The object-row `INSERT` of one seal, separate from the pack write that
    /// shares its bucket.
    pub insert_objects_ns: u64,
    /// Closing the save's connection.
    pub release_connection_ns: u64,
    /// Releasing the compression workspace's zstd context.
    pub release_compression_ns: u64,
    /// Releasing the decompression workspace's zstd context.
    pub release_decompression_ns: u64,
    /// Releasing the pooled reader's pack and value caches.
    pub release_pool_reader_ns: u64,
    /// Releasing the delta reader's pack cache.
    pub release_pack_cache_ns: u64,
    /// Releasing this save's private clone of the content-signature index.
    pub release_candidates_ns: u64,
    /// Releasing this save's private clone of the pooled value index.
    pub release_pool_index_ns: u64,
    /// Releasing the retained open pack tails of every lane.
    pub release_tails_ns: u64,
    /// The stored-frame probe: the bounded-prefix compression that decides whether
    /// a payload is worth compressing at all.
    ///
    /// A region **inside** [`SaveProfile::full_ns`](crate::cas::owner::SaveProfile),
    /// like every other total here: the probe runs inside the FULL encode the
    /// caller charges, so `full_ns` contains this charge and the difference is the
    /// work the probe did not do. It exists because the treatment it belongs to
    /// trades a whole-payload codec call for a bounded one, and the *count* of
    /// those calls is worth nothing without the width of what each one compresses:
    /// the sample is `STORED_PROBE_BYTES` wide and the payload it stands for is
    /// not, so a saving predicted from the sample alone would be the round-8
    /// mistake with the sign reversed.
    pub probe_ns: u64,
}

impl DiagProfile {
    /// Adds another diagnostic profile's totals into this one.
    pub fn accumulate(&mut self, other: &Self) {
        macro_rules! add {
            ($($field:ident),+ $(,)?) => {
                $( self.$field = self.$field.saturating_add(other.$field); )+
            };
        }
        add!(
            commit_total_ns,
            begin_ns,
            seal_total_ns,
            offer_total_ns,
            write_pack_total_ns,
            validate_ns,
            collision_query_ns,
            rows_ns,
            members_ns,
            accept_plumbing_ns,
            flush_batch_ns,
            wave_ns,
            finish_total_ns,
            publish_ns,
            finish_drain_ns,
            finish_drop_ns,
            finish_call_ns,
            insert_objects_ns,
            release_connection_ns,
            release_compression_ns,
            release_decompression_ns,
            release_pool_reader_ns,
            release_pack_cache_ns,
            release_candidates_ns,
            release_pool_index_ns,
            release_tails_ns,
            probe_ns,
        );
    }
}

/// The disjoint parts of [`SaveProfile::resolve`].
///
/// Resolution is the save's largest bucket and its parts are different kinds of
/// work with different treatments: a chain walk that only measures depth
/// (`eligible`), a chain walk that reconstructs a base (`acquire`), a walk that
/// records a new object's cost (`cost`), the exact-reuse verification
/// (`reuse`), and the pooled lane's own lookups (`pooled`). `resolve_ns()` is
/// their sum, so the top-level bucket is unchanged and the parts cannot overlap
/// it or each other.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResolveProfile {
    /// Candidate eligibility: the `depth_of` edge walk, which reads each edge
    /// through `ChainBases` and is charged to no other counter in the product.
    pub eligible_ns: u64,
    /// Base acquisition: the chain rebuild that produces the offered bytes a
    /// prefix frame is taken against.
    pub acquire_ns: u64,
    /// The post-trial cost walk that records the admitted object's own depth.
    pub cost_ns: u64,
    /// Exact-reuse verification: stored-object reconstruction, the identity
    /// re-hash that authenticates it, and the byte comparison.
    pub reuse_ns: u64,
    /// The pooled lane's value lookup, base acquisition and index synchronization.
    pub pooled_ns: u64,
}

impl ResolveProfile {
    /// Adds another resolve profile's parts into this one.
    pub fn accumulate(&mut self, other: &Self) {
        self.eligible_ns = self.eligible_ns.saturating_add(other.eligible_ns);
        self.acquire_ns = self.acquire_ns.saturating_add(other.acquire_ns);
        self.cost_ns = self.cost_ns.saturating_add(other.cost_ns);
        self.reuse_ns = self.reuse_ns.saturating_add(other.reuse_ns);
        self.pooled_ns = self.pooled_ns.saturating_add(other.pooled_ns);
    }

    /// Sum of the five parts.
    pub fn total_ns(&self) -> u64 {
        [
            self.eligible_ns,
            self.acquire_ns,
            self.cost_ns,
            self.reuse_ns,
            self.pooled_ns,
        ]
        .into_iter()
        .fold(0_u64, u64::saturating_add)
    }
}

impl SaveProfile {
    /// Resolution: the sum of [`ResolveProfile`]'s disjoint parts.
    pub fn resolve_ns(&self) -> u64 {
        self.resolve.total_ns()
    }

    /// Adds another profile's buckets into this one.
    pub fn accumulate(&mut self, other: &Self) {
        self.diag.accumulate(&other.diag);
        self.resolve.accumulate(&other.resolve);
        self.reuse_repeat = self.reuse_repeat.saturating_add(other.reuse_repeat);
        self.stored_records = self.stored_records.saturating_add(other.stored_records);
        self.full_ns = self.full_ns.saturating_add(other.full_ns);
        self.delta_ns = self.delta_ns.saturating_add(other.delta_ns);
        self.group_ns = self.group_ns.saturating_add(other.group_ns);
        self.place_ns = self.place_ns.saturating_add(other.place_ns);
        self.sql_ns = self.sql_ns.saturating_add(other.sql_ns);
        self.commit_ns = self.commit_ns.saturating_add(other.commit_ns);
    }

    /// Sum of the seven buckets.
    ///
    /// This is charged work, not the accept path: the difference between it and
    /// `storage.accept_loop` is the remainder the instrument does not name.
    pub fn total_ns(&self) -> u64 {
        [
            self.resolve_ns(),
            self.full_ns,
            self.delta_ns,
            self.group_ns,
            self.place_ns,
            self.sql_ns,
            self.commit_ns,
        ]
        .into_iter()
        .fold(0_u64, u64::saturating_add)
    }

    /// Charges `slot` with the nanoseconds elapsed since `started`.
    ///
    /// One `Instant::now()` pair per site. The caller owns the `started` reading
    /// so a charged interval can wrap a `?` without either charging twice or
    /// losing the charge, and the pair is deliberately not an RAII guard: a guard
    /// that charges on drop would charge an error path whose profile is never
    /// published, at the cost of an extra branch per object.
    pub(crate) fn charge(slot: &mut u64, started: Instant) {
        *slot = slot.saturating_add(started.elapsed().as_nanos() as u64);
    }
}

/// Measurement-only population probe for the exact-reuse verification.
///
/// `#205`'s B1 candidate is "do not re-verify what this save already verified".
/// Its size cannot be read from any duration, because a first verification and a
/// repeat cost the same: the population - how many reuse occurrences find an
/// identity this operation has already reconstructed and compared equal - has to
/// be counted. This type counts it, and does nothing else.
///
/// It is **off by default** and enabled only by
/// `LAYERFS_STORAGE_REUSE_PROBE`, which exists so the mechanism can be sized
/// before any treatment is pre-registered. Enabled, it records every identity the
/// reuse verification has completed for and counts the occurrences that find one
/// already recorded. It changes no decision, no byte, no row and no root: the
/// verification still runs in full on every occurrence, and the probe only
/// observes what the result was. With the variable unset the owner holds `None`
/// and the accept path pays one `Option` test per reuse occurrence.
#[derive(Debug, Default)]
pub struct ReuseProbe {
    /// Identities whose exact-reuse verification has completed in this operation.
    ///
    /// Bounded by the identities one operation verifies, and retained only while
    /// the probe is enabled. The count itself lives on `SaveProfile::reuse_repeat`,
    /// pushed by the caller that observes the return value, so there is exactly one
    /// place a repeat is counted.
    verified: std::collections::BTreeSet<ObjectId>,
}

impl ReuseProbe {
    /// Whether the measurement-only probe is enabled for this process.
    pub fn enabled() -> bool {
        std::env::var("LAYERFS_STORAGE_REUSE_PROBE").is_ok_and(|value| value == "1")
    }

    /// Records one completed verification, returning whether it was a repeat.
    ///
    /// `true` means this operation had already reconstructed and re-authenticated
    /// this identity earlier, which is exactly the occurrence a whole-operation
    /// memo could have answered from its own record.
    pub fn observe(&mut self, id: ObjectId) -> bool {
        !self.verified.insert(id)
    }
}

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
    /// Bytes this operation actually handed to the engine for packs.
    ///
    /// The increment of every pack write: bodies, the directory entries the write
    /// added and one control area each. It is the figure the pack-append rewrite
    /// moves and it cannot be inferred from the finished store - a rewrite leaves
    /// the same bytes behind that a single write would - so it is charged where
    /// the bytes are submitted. Before the reserved-directory framing it counted
    /// the assembled length of the whole pack on every append, which is what
    /// `#219` measured at 2,292,865,337 bytes for 302,023,232 persisted.
    pub pack_bytes_written: u64,
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
    /// Nanosecond cost split of the accept path.
    pub profile: SaveProfile,
}

/// Private writer state for one save operation.
pub struct MutationOwner {
    pub(super) connection: Connection,
    pub(crate) arbitration: std::sync::Arc<std::sync::Mutex<()>>,
    pub(super) save_id: i64,
    pub(super) published_pool: std::sync::Arc<std::sync::Mutex<crate::encoding::pool::PoolIndex>>,
    pub(super) published_candidates: std::sync::Arc<std::sync::Mutex<Candidates>>,
    pub(super) capacities: StorageCapacities,
    pub(super) baseline_pack_id: i64,
    pub(super) next_pack_id: i64,
    /// `store_policy.next_pack_id` as this connection last read or wrote it.
    ///
    /// The watermark only has to be written back when it moved: `begin_write`
    /// re-reads it under the write lock, and only `LanePlacement` allocating a
    /// pack moves it. Writing the unchanged value back is a statement whose
    /// result the row already holds, on every step, for the whole operation.
    pub(super) committed_pack_id: i64,
    /// Highest pack id this save created; contributes to the retained range on
    /// publication. Visibility is determined by the save row, not this ceiling.
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
    /// True while this owner holds the Store's arbitration for a whole wave.
    ///
    /// A wave keeps **one** write transaction open across every seal it performs,
    /// and SQLite admits one writer per Store file, so the whole wave has to hold
    /// the arbitration: a second writer that slipped in between two of its seals
    /// would take the write lock and fail on `BEGIN IMMEDIATE` - `busy_timeout` is
    /// 0 by design, so a locked Store is a definite failure and never a wait -
    /// instead of waiting for a step it can measure.
    ///
    /// The arbitration is a plain `Mutex`, so it is taken once and every nested
    /// acquisition inside the wave is a no-op. The flag is a field of the owner
    /// rather than a thread-local, so two Stores on one thread cannot confuse each
    /// other's state.
    pub(super) wave_held: bool,
    pub(super) compression: CompressionWorkspace,
    pub(super) decompression: DecompressionWorkspace,
    pub(super) terminal: bool,
    pub(super) cleanup_attempted: bool,
    pub(super) quarantined: bool,
    pub(super) counters: OutcomeCounters,
    /// Nanosecond cost split of this operation's accept path.
    pub(super) profile: SaveProfile,
    /// Measurement-only reuse-repeat probe; `None` unless explicitly enabled.
    ///
    /// See [`ReuseProbe`]. It is not part of the persisted policy, is never
    /// enabled by a default, and observes the verification without changing it.
    pub(super) probe: Option<ReuseProbe>,
    /// Store-owned bounded content-signature index.
    ///
    /// **W2 (#188d).** It is shared with the `Store` and persisted in
    /// `content_signatures`, so it outlives this operation: an object one save
    /// admitted is a candidate the next save can be offered. The `Store` also
    /// keeps it across `open`, which is what makes the correspondence cross-save
    /// rather than merely cross-operation. A failed save invalidates it whole - it
    /// is disposable derivation and the table is authoritative.
    pub(super) candidates: std::sync::Arc<std::sync::Mutex<Candidates>>,
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
    /// Object rows this wave has written and not yet validated.
    ///
    /// A wave's seals all share one transaction, so the collision check they each
    /// ran can run once for the whole wave instead: the rows it compares against
    /// belong to *other* saves and cannot change while this transaction holds the
    /// Store's write lock. Cleared at the start of every wave and drained by the
    /// one validation the wave performs.
    pub(super) wave_rows: Vec<crate::sqlite::write::ObjectRow>,
    /// Ordinals this save assigned that the retained window does not answer yet.
    ///
    /// Bounded and reset per leaf; see `PENDING_VALUES_LIMIT`.
    pub(super) pending_values: BTreeMap<[u8; 73], u32>,
    /// Next ordinal this save may assign.
    pub(super) next_ordinal: Option<u64>,
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

    /// Runs one preparation wave under a single write transaction.
    ///
    /// **The step is the wave.** The multi-writer rule is that a write transaction
    /// never outlives the step that opened it under the arbitration lock, so every
    /// step commits before that lock is released; batching stays inside a step.
    /// This makes the step one preparation wave - the bounded unit the caller
    /// already offers - instead of one seal, and the transaction is acknowledged
    /// here, before the lock is released. The other writer waits for a wave rather
    /// than for a seal; both are bounded, and the wave's measured width (3.7 ms on
    /// this row) is inside the p99 the pair probe already tolerated.
    pub(super) fn with_wave<T>(
        &mut self,
        body: impl FnOnce(&mut Self) -> StorageResult<T>,
    ) -> StorageResult<T> {
        let arbitration = std::sync::Arc::clone(&self.arbitration);
        let _guard = crate::sqlite::ownership::lock(&arbitration)?;
        self.wave_held = true;
        let result = body(self);
        self.wave_held = false;
        match result {
            Ok(value) => {
                self.maybe_commit()?;
                Ok(value)
            }
            Err(error) => Err(error),
        }
    }

    /// Reads objects inside this owner's transaction, with no ceiling.
    ///
    /// The pooled reader is this operation's own (`self.pool_reader`), the same one
    /// the pooled lane and the depth walk use, so a pack a later read of this save
    /// demands is not copied again. Its pack cache is released on every pack write
    /// (`write_pack`), which is what makes that lifetime sound while the save runs.
    pub fn read_batch(&mut self, ids: &[ObjectId]) -> StorageResult<Vec<Vec<u8>>> {
        let arbitration = std::sync::Arc::clone(&self.arbitration);
        let _guard = crate::sqlite::ownership::lock_unless_held(&arbitration, self.wave_held)?;
        let mut groups = crate::encoding::GroupCache::new();
        let (values, _) = crate::cas::read::read_objects(
            &self.connection,
            ids,
            i64::MAX,
            &self.capacities,
            &mut self.decompression,
            &mut groups,
            &mut self.pool_reader,
        )?;
        Ok(values)
    }

    /// Reconstructs one stored object, following and authenticating its chain.
    ///
    /// Both body caches are this operation's: the ordinary-lane pack cache
    /// (`self.pack_cache`) and the pooled reader (`self.pool_reader`). A pooled
    /// reader built here instead would be discarded with the one leaf it resolved,
    /// and the next leaf of the same save would copy the same packs again. The
    /// pooled reader's pack cache is released on every pack write, so no body it
    /// retains can predate a write to the pack it came from.
    pub fn resolve_location(&mut self, location: lookup::ObjectLocation) -> StorageResult<Vec<u8>> {
        let arbitration = std::sync::Arc::clone(&self.arbitration);
        let _guard = crate::sqlite::ownership::lock_unless_held(&arbitration, self.wave_held)?;
        let value = {
            let mut groups = crate::encoding::GroupCache::new();
            let mut resolver = crate::encoding::delta::read::Resolver::new(
                &self.connection,
                i64::MAX,
                &self.capacities,
                crate::encoding::delta::read::BodyCaches {
                    packs: &mut self.pack_cache,
                    pool: &mut self.pool_reader,
                },
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
