//! Public Store handle, the private save operation and the C1 handoff adapter.
//!
//! A Store is a path plus the policy it was opened with. `begin_save` reserves
//! one private save under the Store's persisted writer budget
//! (`max_concurrent_writes`, #216); `accept` takes finalized canonical objects
//! under bounded batch limits; `finish` completes every remaining write and
//! acknowledges the final transaction. Reads are independent bounded waves that
//! capture publication scope and a pack range ceiling once.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use layerfs_content::{ContentError, FinalizedConsumer, FinalizedObject, ObjectId};
use layerfs_telemetry::timer::TimingScope;

use crate::cas::batch::PendingBatch;
use crate::cas::owner::{MutationOwner, OutcomeCounters, SaveProfile};
use crate::cas::read::check_read_demand;
use crate::cas::{finish, read, save};
use crate::encoding::delta::candidates::Candidates;
use crate::encoding::delta::read::ChainCounters;
use crate::encoding::delta::select::DeltaCounters;
use crate::encoding::pool::PoolIndex;
use crate::encoding::DecompressionWorkspace;
use crate::error::{StorageError, StorageResult};
use crate::policy::{StorageCapacities, StoragePolicy};
use crate::sqlite::{connection, lookup, schema};

/// Result of one completed save operation.
///
/// Equality is deliberately manual: see [`SaveOutcome`]'s `PartialEq` below.
#[derive(Clone, Copy, Debug)]
pub struct SaveOutcome {
    /// Occurrences served by an exact existing row.
    pub reused: u64,
    /// Objects newly written.
    pub inserted: u64,
    /// Packs created by this operation.
    pub packs_created: u64,
    /// Bytes this operation handed to the engine for packs. See
    /// [`OutcomeCounters::pack_bytes_written`].
    pub pack_bytes_written: u64,
    /// Appends to packs this operation created.
    pub pack_appends: u64,
    /// Write transactions acknowledged with `COMMIT`.
    ///
    /// Holding a `SaveOutcome` at all *is* the acknowledgement: `finish` returns
    /// one only after its publication transaction committed, and every other outcome
    /// is a typed error. There is deliberately no boolean field for it - a field
    /// that can only ever hold one value cannot fail an assertion, and one used to
    /// sit here doing exactly that.
    pub commits: u64,
    /// Record-level objects newly written as a FULL representation.
    pub full_records: u64,
    /// Record-level objects newly written as a PREFIX representation.
    pub prefix_records: u64,
    /// `INSERT` statements issued for object rows.
    ///
    /// One per row before batching; one per bound chunk after. It is the counter
    /// an INSERT-batching change moves, and it is deliberately not a row count:
    /// `inserted` already is one.
    pub statements: u64,
    /// Presence queries issued for offered objects' direct references.
    pub presence_queries: u64,
    /// Representation selection outcomes.
    pub delta: DeltaCounters,
    /// Work spent acquiring delta bases.
    pub chain: ChainCounters,
    /// Pooled metadata lane outcomes.
    pub pool: crate::cas::PoolCounters,
    /// Nanosecond cost split of this operation's accept path.
    ///
    /// Seven disjoint buckets accumulated over the whole operation; see
    /// [`SaveProfile`]. It is reported work, not the accept span: the difference
    /// is the remainder the instrument does not name.
    pub profile: SaveProfile,
}

/// Two outcomes are equal when they describe the **same work**.
///
/// Every work counter participates; `profile` deliberately does not. It is a
/// wall-clock observation of the accept path, not a description of what the
/// operation did, so two runs of identical work never have equal profiles - and a
/// comparison that included them could never assert determinism. The integrated
/// recording-on/recording-off equality case is exactly that caller: with `profile`
/// in the comparison it reported "recording changed the operation" for a pair whose
/// every counter, root and read-back byte was identical, differing only in
/// nanoseconds. Compare `profile` explicitly when the durations are the question;
/// [`SaveProfile`] keeps its own derived equality for that.
impl PartialEq for SaveOutcome {
    fn eq(&self, other: &Self) -> bool {
        self.reused == other.reused
            && self.inserted == other.inserted
            && self.packs_created == other.packs_created
            && self.pack_appends == other.pack_appends
            && self.commits == other.commits
            && self.full_records == other.full_records
            && self.prefix_records == other.prefix_records
            && self.statements == other.statements
            && self.presence_queries == other.presence_queries
            && self.delta == other.delta
            && self.chain == other.chain
            && self.pool == other.pool
    }
}

impl Eq for SaveOutcome {}

impl From<OutcomeCounters> for SaveOutcome {
    fn from(counters: OutcomeCounters) -> Self {
        Self {
            reused: counters.reused,
            inserted: counters.inserted,
            packs_created: counters.packs_created,
            pack_bytes_written: counters.pack_bytes_written,
            pack_appends: counters.pack_appends,
            commits: counters.commits,
            full_records: counters.full_records,
            prefix_records: counters.prefix_records,
            statements: counters.statements,
            presence_queries: counters.presence_queries,
            delta: counters.delta,
            chain: counters.chain,
            pool: counters.pool,
            profile: counters.profile,
        }
    }
}

/// Counters describing one independent read wave.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StoreReadCounters {
    /// Objects returned.
    pub objects: u64,
    /// Pack bodies read.
    pub packs_read: u64,
    /// Membership pages issued.
    pub pages: u64,
    /// Retained-pack ceiling applied to every acquired location.
    pub ceiling: i64,
    /// Dependency edges followed.
    pub edges: u64,
    /// Longest dependency chain reconstructed.
    pub max_depth: u64,
    /// Canonical bytes reconstructed, including dependencies.
    pub canonical_bytes: u64,
    /// Ordinary-lane group bodies decompressed by this wave.
    pub group_decodes: u64,
    /// Pooled metadata reconstruction work, separate from ordinary-lane counts.
    pub pooled: crate::encoding::pool::PoolReadCounters,
    /// Connections this read opened.
    ///
    /// One `read_batch` call is one wave and opens one connection; a demand
    /// refused before the open reports nothing because the call returns an
    /// error. The figure is per wave, so an operation's connection lifetime is
    /// the sum over the waves it issued, which is what a caller that holds a
    /// provider across an operation reads back.
    pub opens: u64,
}

/// Cache observables of the connection **one save runs on**.
///
/// Owner: the save operation whose connection it describes. Bound: four integers,
/// fixed size, no allocation and no retained state. Live multiplicity: read on
/// demand, never stored. Lifetime: the read call. Release: nothing to release.
/// Every field is the engine's own answer for **that** connection - the pragmas
/// read back, never the values the profile was configured with - so a receipt can
/// state the profile a save actually ran under. It is a reading, never a knob: no
/// product path writes a pragma from it.
///
/// **What is deliberately absent: the page-cache spill counter.**
/// `SQLITE_DBSTATUS_CACHE_SPILL` is the observable that would say whether the
/// declared cache size is enough for the transaction shape the save produces, and
/// the product cannot read it: `rusqlite` exposes no safe wrapper (the 0.40.2
/// source declares no `db_status` binding), and this crate cannot call the FFI
/// itself - `#![deny(unsafe_code)]` with exactly one audited module
/// (`encoding::codec`), enforced by `core/tools/check_product_boundary.py`. A
/// second FFI site is an owner decision, so the field is absent rather than
/// faked, and the harness reads the status on a connection it owns instead
/// (recorded in this item's receipt).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SaveConnectionProfile {
    /// `PRAGMA page_size` of the store.
    pub page_size: i64,
    /// `PRAGMA cache_size`, natively signed (negative = KiB).
    pub cache_size: i64,
    /// `PRAGMA cache_spill`: 0 when spilling is off, otherwise the page
    /// threshold at which a full page cache may spill (SQLite's own encoding of
    /// the flag, not a boolean).
    pub cache_spill: i64,
    /// `PRAGMA mmap_size`.
    pub mmap_size: i64,
}

/// A content-addressed Store at one filesystem path.
#[derive(Clone, Debug)]
pub struct Store {
    path: PathBuf,
    policy: StoragePolicy,
    capacities: StorageCapacities,
    arbitration: Arc<Mutex<()>>,
    /// Store-owned bounded ordered set of pooled value candidates.
    ///
    /// It is disposable derivation: the catalogue is authoritative, a failed save
    /// invalidates it whole, and a reopened Store synchronizes it from the
    /// catalogue on first use.
    pool_index: Arc<Mutex<PoolIndex>>,
    /// Store-owned bounded content-signature index, persisted in
    /// `content_signatures`.
    ///
    /// **W2 (#188d).** Owned here rather than by a save, which is the whole point:
    /// a per-save index can only propose a base from the objects that same save
    /// admitted, so a cross-save match is impossible at any index size. `open`
    /// reads the table back; a save appends to it inside the transaction that
    /// publishes the objects the entries name; a failed save invalidates it whole.
    content_index: Arc<Mutex<Candidates>>,
}

impl Store {
    /// Creates a fresh Store with `policy`; an existing Store is never migrated.
    pub fn create(
        path: impl AsRef<Path>,
        policy: StoragePolicy,
        scope: TimingScope<'_>,
    ) -> StorageResult<Self> {
        scope.run(|_create| {
            let path = path.as_ref().to_path_buf();
            // An unsupported policy is rejected before the database file exists.
            let policy = policy.validated()?;
            let connection = connection::open(&path, true)?;
            let arbitration = crate::sqlite::ownership::arbitration(&path)?;
            let guard = crate::sqlite::ownership::lock(&arbitration)?;
            let stored = schema::create(&connection, policy)?;
            let capacities = StorageCapacities::from_policy(stored)?;
            // A Store this call just created has an empty index; the read is the
            // same one `open` performs and is bounded by the table, which is empty.
            let content_index = Candidates::load(&connection)?;
            drop(connection);
            drop(guard);
            Ok(Self {
                path,
                policy: stored,
                capacities,
                arbitration,
                pool_index: Arc::new(Mutex::new(PoolIndex::new())),
                content_index: Arc::new(Mutex::new(content_index)),
            })
        })
    }

    /// Opens an existing Store and validates its identity and policy.
    pub fn open(path: impl AsRef<Path>, scope: TimingScope<'_>) -> StorageResult<Self> {
        scope.run(|_open| {
            let path = path.as_ref().to_path_buf();
            let connection = connection::open(&path, false)?;
            let arbitration = crate::sqlite::ownership::arbitration(&path)?;
            let guard = crate::sqlite::ownership::lock(&arbitration)?;
            let stored = schema::validate(&connection, None)?;
            let capacities = StorageCapacities::from_policy(stored)?;
            // The bounded load: at most `candidates::SLOTS` rows of 32-byte
            // identity and 32-byte folded signature, read once for the lifetime of
            // this handle rather than once per save.
            let content_index = Candidates::load(&connection)?;
            drop(connection);
            drop(guard);
            Ok(Self {
                path,
                policy: stored,
                capacities,
                arbitration,
                pool_index: Arc::new(Mutex::new(PoolIndex::new())),
                content_index: Arc::new(Mutex::new(content_index)),
            })
        })
    }

    /// The default policy this slice creates and opens.
    pub const fn default_policy() -> StoragePolicy {
        StoragePolicy::frozen_default()
    }

    /// Persisted policy of this Store.
    ///
    /// This is the immutable construction profile. The writer budget is a
    /// separate persisted value: it is an admission setting, not a format
    /// property, and it can change while the Store stays the same Store.
    pub fn policy(&self) -> StoragePolicy {
        self.policy
    }

    /// The Store's authoritative writer budget.
    ///
    /// It is read from the Store file, never from a process-local copy, so every
    /// sandbox and process sharing this Store sees one number and a change is
    /// visible to the next [`Store::begin_save`] without reopening anything.
    pub fn max_concurrent_writes(&self) -> StorageResult<u8> {
        let connection = connection::open(&self.path, false)?;
        let _guard = crate::sqlite::ownership::lock(&self.arbitration)?;
        schema::max_concurrent_writes(&connection)
    }

    /// Sets the Store's writer budget for every later save.
    ///
    /// One persisted update, and the only supported way to change the setting.
    /// Retained private ownership is never released, rescanned or rewritten: a
    /// save that is already recorded keeps its slot and still counts against the
    /// new budget, so lowering the setting cannot make unresolved ownership
    /// reusable. A value outside `1..=MAX_CONCURRENT_WRITES_LIMIT` is refused
    /// rather than clamped.
    pub fn set_max_concurrent_writes(
        &self,
        writes: u8,
        scope: TimingScope<'_>,
    ) -> StorageResult<u8> {
        scope.run(|_configure| {
            let connection = connection::open(&self.path, false)?;
            let guard = crate::sqlite::ownership::lock(&self.arbitration)?;
            crate::sqlite::write::begin_immediate(&connection)?;
            match schema::set_max_concurrent_writes(&connection, writes) {
                Ok(applied) => {
                    crate::sqlite::write::commit(&connection)?;
                    drop(guard);
                    Ok(applied)
                }
                Err(refusal) => {
                    crate::sqlite::write::rollback(&connection)?;
                    drop(guard);
                    Err(refusal)
                }
            }
        })
    }

    /// Declared capacities of this Store.
    pub fn capacities(&self) -> StorageCapacities {
        self.capacities
    }

    /// Filesystem path of this Store.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Entries retained by the Store-owned pooled-value ordered set.
    ///
    /// Reported live capacity, not a test hook: the memory ledger needs the
    /// simultaneous size of the bounded derivation the Store owns.
    pub fn pool_index_entries(&self) -> usize {
        self.pool_index.lock().map(|index| index.len()).unwrap_or(0)
    }

    /// Occupied slots of the Store-owned content-signature index.
    ///
    /// Reported live occupancy, not a test hook: the memory ledger needs the
    /// simultaneous size of the bounded derivation the Store owns, and the
    /// declared bound only says what it *may* hold.
    pub fn content_index_entries(&self) -> usize {
        self.content_index
            .lock()
            .map(|index| index.entries())
            .unwrap_or(0)
    }

    /// Live bytes charged by the Store-owned content-signature index.
    pub fn content_index_bytes(&self) -> usize {
        self.content_index
            .lock()
            .map(|index| index.live_bytes())
            .unwrap_or(0)
    }

    /// Live bytes charged by the pooled-value ordered set.
    pub fn pool_index_bytes(&self) -> usize {
        self.pool_index
            .lock()
            .map(|index| index.live_bytes())
            .unwrap_or(0)
    }

    /// Reserves private ownership for one save under the Store's writer budget;
    /// database transactions arbitrate separately.
    pub fn begin_save(&self, scope: TimingScope<'_>) -> StorageResult<SaveOperation> {
        scope.run(|_acquire| {
            let connection = connection::open(&self.path, false)?;
            let owner = MutationOwner::acquire(
                connection,
                self.capacities,
                Arc::clone(&self.arbitration),
                Arc::clone(&self.pool_index),
                Arc::clone(&self.content_index),
            )?;
            Ok(SaveOperation {
                owner: Some(owner),
                batch: PendingBatch::new(self.capacities),
                policy: self.policy,
                capacities: self.capacities,
                finished: false,
                terminal: false,
            })
        })
    }

    /// Reads objects as one independent wave under a captured pack ceiling.
    pub fn read_batch(
        &self,
        ids: &[ObjectId],
        scope: TimingScope<'_>,
    ) -> StorageResult<(Vec<Vec<u8>>, StoreReadCounters)> {
        check_read_demand(ids, self.capacities.read_objects)?;
        scope.run(|read_scope| {
            let connection = connection::open(&self.path, false)?;
            // Publication scope excludes every private save. The retained-pack
            // ceiling is an additional range check, never visibility authority.
            let _guard = crate::sqlite::ownership::lock(&self.arbitration)?;
            crate::sqlite::ownership::scope(
                &connection,
                0,
                crate::sqlite::ownership::publication(&connection)?,
            )?;
            let ceiling = schema::retained_pack_ceiling(&connection)?;
            let mut workspace = read_scope
                .child("storage.decode")
                .run(|_| DecompressionWorkspace::new())?;
            let mut groups = crate::encoding::GroupCache::new();
            // One independent wave owns one pooled reader: this call is its whole
            // lifetime, so the reader is built here and dropped with the wave. The
            // operation-scoped reader is the session's (`ReadSession`).
            let mut pool = crate::encoding::pool::PoolReader::new();
            let (values, counters) = read_scope.child("storage.read").run(|_| {
                read::read_objects(
                    &connection,
                    ids,
                    ceiling,
                    &self.capacities,
                    &mut workspace,
                    &mut groups,
                    &mut pool,
                )
            })?;
            Ok((
                values,
                StoreReadCounters {
                    objects: counters.objects,
                    packs_read: counters.packs_read,
                    pages: counters.pages,
                    ceiling,
                    edges: counters.edges,
                    max_depth: counters.max_depth,
                    canonical_bytes: counters.canonical_bytes,
                    group_decodes: counters.group_decodes,
                    pooled: counters.pooled,
                    opens: 1,
                },
            ))
        })
    }

    /// Returns requested identifiers that are present, without locators.
    pub fn contains(
        &self,
        ids: &[ObjectId],
        scope: TimingScope<'_>,
    ) -> StorageResult<Vec<ObjectId>> {
        scope.run(|_contains| {
            // A batch lookup is a read wave like any other: its size is the
            // caller's declared resource and it is refused before a connection is
            // opened, so a caller cannot turn one question into unbounded query
            // work by passing a longer slice.
            check_read_demand(ids, self.capacities.read_objects)?;
            let connection = connection::open(&self.path, false)?;
            let _guard = crate::sqlite::ownership::lock(&self.arbitration)?;
            crate::sqlite::ownership::scope(
                &connection,
                0,
                crate::sqlite::ownership::publication(&connection)?,
            )?;
            let ceiling = schema::retained_pack_ceiling(&connection)?;
            lookup::present(&connection, ids, ceiling)
        })
    }
}

/// One independently owned save operation with bounded acceptance.
pub struct SaveOperation {
    owner: Option<MutationOwner>,
    batch: PendingBatch,
    policy: StoragePolicy,
    capacities: StorageCapacities,
    finished: bool,
    terminal: bool,
}

impl SaveOperation {
    /// Accepts one finalized object, flushing the wave when a bound is reached.
    ///
    /// This call creates no timing node. The caller owns the tree: a caller
    /// accepting a wave plans one region for the wave and accepts every object of
    /// it inside that region, which is what keeps a save's report bounded by its
    /// waves instead of by its object count. The earlier form started a node and
    /// added a `storage.batch` child on every single accept, so a save of a few
    /// hundred objects spent the recorder's node budget on one row per object and
    /// clipped its own detail - the telemetry contract forbids exactly that ("Do
    /// not add a node per object, syscall or delta edge").
    pub fn accept(&mut self, object: FinalizedObject) -> StorageResult<()> {
        let whole = std::time::Instant::now();
        if self.finished || self.terminal {
            return Err(StorageError::Aborted);
        }
        let result = match self.batch.push(object) {
            Ok(Some(drained)) => self.flush(drained),
            Ok(None) => Ok(()),
            Err(error) => Err(self.terminate(error)),
        };
        if let Some(owner) = self.owner.as_mut() {
            crate::cas::owner::SaveProfile::charge(
                &mut owner.profile.diag.accept_plumbing_ns,
                whole,
            );
        }
        result
    }

    /// Cache observables of this operation's own connection.
    ///
    /// The save owns its connection; a caller outside the crate cannot open it,
    /// so before this accessor the only way to answer "did this save's page cache
    /// spill?" was to replay the row shape on a harness-owned connection and
    /// argue by analogy. Read it before `finish`: the connection is released with
    /// the operation.
    pub fn connection_profile(&self) -> StorageResult<SaveConnectionProfile> {
        let owner = self.owner.as_ref().ok_or(StorageError::Aborted)?;
        let _guard = crate::sqlite::ownership::lock(&owner.arbitration)?;
        let connection = owner.connection();
        Ok(SaveConnectionProfile {
            page_size: connection::pragma_i64(connection, connection::Pragma::PageSize)?,
            cache_size: connection::pragma_i64(connection, connection::Pragma::CacheSize)?,
            cache_spill: connection::pragma_i64(connection, connection::Pragma::CacheSpill)?,
            mmap_size: connection::pragma_i64(connection, connection::Pragma::MmapSize)?,
        })
    }

    /// Flushes one preparation wave and prepares it for storage.
    fn flush(&mut self, objects: Vec<FinalizedObject>) -> StorageResult<()> {
        let Some(owner) = self.owner.as_mut() else {
            return Err(self.terminate(StorageError::Aborted));
        };
        match save::flush_batch(owner, objects) {
            Ok(()) => Ok(()),
            Err(error) => Err(self.terminate(error)),
        }
    }

    /// Drains the remaining batch and acknowledges storage completion.
    pub fn finish(mut self, scope: TimingScope<'_>) -> StorageResult<SaveOutcome> {
        // Three diagnostic totals, because the round-1 row leaves 255 of this
        // call's 257.5 ms in none of its named parts. See `DiagProfile`.
        let call_started = std::time::Instant::now();
        let mut drain_ns = 0_u64;
        let result = scope.run(|_finish| {
            let drain_started = std::time::Instant::now();
            let remaining = self.batch.drain();
            self.flush(remaining)?;
            drain_ns = drain_started.elapsed().as_nanos() as u64;
            let owner = self.owner.as_mut().ok_or(StorageError::Aborted)?;
            let counters = owner.finish()?;
            Ok(SaveOutcome::from(counters))
        });
        match result {
            Ok(mut outcome) => {
                self.finished = true;
                let drop_started = std::time::Instant::now();
                // **DIAGNOSTIC.** The owner is taken apart field by field and each
                // field is dropped in its own charged step, so the residual the
                // named groups cannot explain is what the `..` holds. Dropping the
                // fields in a different order from the struct's declaration cannot
                // change what the allocator is asked to free: every field is moved
                // out and dropped exactly once, and the owner was about to be
                // destroyed anyway.
                let mut release = crate::cas::owner::DiagProfile::default();
                if let Some(owner) = self.owner.take() {
                    let crate::cas::owner::MutationOwner {
                        connection,
                        compression,
                        decompression,
                        pool_reader,
                        pack_cache,
                        candidates,
                        pool_index,
                        groups,
                        placement,
                        ..
                    } = owner;
                    macro_rules! charged {
                        ($slot:ident, $value:expr) => {{
                            let started = std::time::Instant::now();
                            drop($value);
                            crate::cas::owner::SaveProfile::charge(&mut release.$slot, started);
                        }};
                    }
                    charged!(release_connection_ns, connection);
                    charged!(release_compression_ns, compression);
                    charged!(release_decompression_ns, decompression);
                    charged!(release_pool_reader_ns, pool_reader);
                    charged!(release_pack_cache_ns, pack_cache);
                    charged!(release_candidates_ns, candidates);
                    charged!(release_pool_index_ns, pool_index);
                    charged!(release_tails_ns, groups);
                    charged!(release_tails_ns, placement);
                    // Every other field drops here, at the end of this block: the
                    // residual is `finish_drop_ns` minus the named steps.
                }
                let drop_ns = drop_started.elapsed().as_nanos() as u64;
                outcome.profile.diag.accumulate(&release);
                outcome.profile.diag.finish_drain_ns = drain_ns;
                outcome.profile.diag.finish_drop_ns = drop_ns;
                outcome.profile.diag.finish_call_ns = call_started.elapsed().as_nanos() as u64;
                Ok(outcome)
            }
            Err(error) => Err(self.terminate(error)),
        }
    }

    /// Reads objects inside this save's own transaction, with no ceiling.
    ///
    /// Identities still held in the bounded pending batch are served from that
    /// state. An identity that is waiting in an unfinished write group has no row
    /// yet, so the group holding it is sealed first and the remainder is read
    /// through the open transaction; sealing is the owner's own write, happens
    /// inside this read's scope and takes the save's terminal boundary if it fails.
    /// A same-save read therefore observes every object `accept` acknowledged,
    /// without exposing any of it to unrelated readers.
    pub fn read_batch(
        &mut self,
        ids: &[ObjectId],
        scope: TimingScope<'_>,
    ) -> StorageResult<Vec<Vec<u8>>> {
        check_read_demand(ids, self.capacities.read_objects)?;
        // The caller's planned node is this read's one region: sealing the group
        // that holds a demanded identity and the query which follows it both happen
        // inside it. There is no inner child named after the caller's own node.
        scope.run(|_read| {
            let pending_bytes = ids
                .iter()
                .filter_map(|id| self.batch.pending_canonical(*id))
                .try_fold(0usize, |total, bytes| total.checked_add(bytes.len()))
                .ok_or(StorageError::Integrity("pending read byte accounting"))?;
            read::check_read_bytes(pending_bytes)?;
            let pending: Vec<(ObjectId, Vec<u8>)> = ids
                .iter()
                .filter_map(|id| {
                    self.batch
                        .pending_canonical(*id)
                        .map(|bytes| (*id, bytes.to_vec()))
                })
                .collect();
            let remaining: Vec<ObjectId> = ids
                .iter()
                .copied()
                .filter(|id| !pending.iter().any(|(candidate, _)| candidate == id))
                .collect();
            // Sealing is a preparation write, so it takes the same terminal boundary
            // as every other preparation write: a lost lock or a failed commit here
            // leaves an unproven transaction state that cleanup must resolve once.
            // The query that follows it is a query, and a missing object is a caller
            // error rather than a broken save, so it does not terminate anything.
            if !remaining.is_empty() {
                let owner = self.owner.as_mut().ok_or(StorageError::Aborted)?;
                if let Err(error) = owner.seal_pending(&remaining) {
                    return Err(self.terminate(error));
                }
            }
            let stored = if remaining.is_empty() {
                Vec::new()
            } else {
                let owner = self.owner.as_mut().ok_or(StorageError::Aborted)?;
                {
                    let _guard = crate::sqlite::ownership::lock(&owner.arbitration)?;
                    let lengths: std::collections::BTreeMap<_, _> =
                        lookup::locations(owner.connection(), &remaining, i64::MAX)?
                            .into_iter()
                            .map(|row| (row.object_id, row.canonical_length))
                            .collect();
                    let total = remaining.iter().try_fold(pending_bytes, |total, id| {
                        let length = lengths.get(id).ok_or(StorageError::ObjectMissing(*id))?;
                        total
                            .checked_add(*length)
                            .ok_or(StorageError::Integrity("read byte accounting"))
                    })?;
                    read::check_read_bytes(total)?;
                }
                owner.read_batch(&remaining)?
            };
            // Both sources are already in demand order - the pending batch is a
            // filtered copy of `ids` and so is the stored batch - so one pass over
            // `ids` with a cursor on each moves every value out exactly once. The
            // earlier form searched the pending vector per demand and cloned the
            // bytes it found, so a pending value was copied twice: once out of the
            // batch and once into the answer.
            let mut pending = pending.into_iter().peekable();
            let mut stored = stored.into_iter();
            let mut values = Vec::with_capacity(ids.len());
            for id in ids {
                if pending.peek().is_some_and(|(candidate, _)| candidate == id) {
                    let (_, bytes) = pending.next().ok_or(StorageError::Aborted)?;
                    values.push(bytes);
                } else {
                    values.push(stored.next().ok_or(StorageError::ObjectMissing(*id))?);
                }
            }
            if pending.next().is_some() || stored.next().is_some() {
                return Err(StorageError::Integrity("same-save read cardinality"));
            }
            Ok(values)
        })
    }

    /// Declared capacities of the Store this operation belongs to.
    pub fn capacities(&self) -> StorageCapacities {
        self.capacities
    }

    /// Persisted policy of the Store this operation belongs to.
    pub fn policy(&self) -> StoragePolicy {
        self.policy
    }

    /// Highest retained pack identifier before this operation started.
    pub fn baseline_pack_id(&self) -> StorageResult<i64> {
        self.owner
            .as_ref()
            .map(MutationOwner::baseline_pack_id)
            .ok_or(StorageError::Aborted)
    }

    /// Bytes retained by the open pack tail of every framing lane.
    pub fn retained_tail_bytes(&self) -> StorageResult<usize> {
        self.owner
            .as_ref()
            .ok_or(StorageError::Aborted)?
            .retained_tail_bytes()
    }

    /// Objects and canonical bytes currently waiting in the pending batch.
    pub fn pending(&self) -> (usize, u64) {
        (self.batch.len(), self.batch.canonical_bytes())
    }

    /// Representation selection outcomes so far.
    pub fn delta_counters(&self) -> DeltaCounters {
        self.owner
            .as_ref()
            .map(MutationOwner::delta_counters)
            .unwrap_or_default()
    }

    /// Pooled metadata lane outcomes so far.
    pub fn pool_counters(&self) -> crate::cas::PoolCounters {
        self.owner
            .as_ref()
            .map(MutationOwner::pool_counters)
            .unwrap_or_default()
    }

    /// Work spent acquiring delta bases so far.
    pub fn chain_counters(&self) -> ChainCounters {
        self.owner
            .as_ref()
            .map(MutationOwner::chain_counters)
            .unwrap_or_default()
    }

    /// Live bytes held by the bounded admitted-FULL winner cache.
    pub fn candidate_index_bytes(&self) -> usize {
        self.owner
            .as_ref()
            .map(MutationOwner::candidate_index_bytes)
            .unwrap_or(0)
    }

    /// Ends this operation with an explicit abort and one cleanup attempt.
    pub fn abort(mut self, scope: TimingScope<'_>) -> StorageResult<()> {
        let result = scope.run(|_abort| {
            self.terminal = true;
            match self.owner.as_mut() {
                Some(owner) => owner.abandon(),
                None => Ok(()),
            }
        });
        self.finished = true;
        self.owner = None;
        result
    }

    fn terminate(&mut self, error: StorageError) -> StorageError {
        self.terminal = true;
        let Some(owner) = self.owner.as_mut() else {
            return error;
        };
        finish::terminate(owner, error)
    }
}

impl Drop for SaveOperation {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        if let Some(owner) = self.owner.as_mut() {
            // One best-effort attempt; a previously attempted cleanup is never
            // repeated and a quarantined save is left untouched.
            let _ = owner.abandon();
        }
    }
}

/// Adapter that lets C1 feed a save operation directly.
///
/// C1 reports [`ContentError::OutputRejected`] and stops; the original storage
/// failure is retained here and returned once by the integrated caller, so no
/// detail is invented and the failure is never resent.
pub struct SaveHandoff<'a> {
    operation: &'a mut SaveOperation,
    failure: Option<StorageError>,
}

impl<'a> SaveHandoff<'a> {
    /// Wraps a save operation as a finalized-object consumer.
    pub fn new(operation: &'a mut SaveOperation) -> Self {
        Self {
            operation,
            failure: None,
        }
    }

    /// The retained storage failure, if one occurred.
    pub fn failure(&self) -> Option<&StorageError> {
        self.failure.as_ref()
    }

    /// Takes the retained storage failure.
    pub fn take_failure(&mut self) -> Option<StorageError> {
        self.failure.take()
    }

    /// The save operation this handoff feeds.
    pub fn operation(&mut self) -> &mut SaveOperation {
        self.operation
    }
}

impl FinalizedConsumer for SaveHandoff<'_> {
    fn accept(&mut self, object: FinalizedObject) -> Result<(), ContentError> {
        match self.operation.accept(object) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.failure = Some(error);
                Err(ContentError::OutputRejected)
            }
        }
    }
}
