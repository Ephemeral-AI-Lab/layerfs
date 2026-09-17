//! Public Store handle, the exclusive save operation and the C1 handoff adapter.
//!
//! A Store is a path plus the policy it was opened with. `begin_save` acquires
//! exclusive write ownership once; `accept` takes finalized canonical objects
//! under bounded batch limits; `finish` completes every remaining write and
//! acknowledges the final transaction. Reads are independent bounded waves that
//! capture their retained-pack ceiling once.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use layerfs_content::{ContentError, FinalizedConsumer, FinalizedObject, ObjectId};
use layerfs_telemetry::timer::TimingScope;

use crate::cas::batch::PendingBatch;
use crate::cas::owner::{MutationOwner, OutcomeCounters};
use crate::cas::{finish, read, save};
use crate::encoding::delta::read::ChainCounters;
use crate::encoding::delta::select::DeltaCounters;
use crate::encoding::pool::PoolIndex;
use crate::encoding::DecompressionWorkspace;
use crate::error::{StorageError, StorageResult};
use crate::policy::{StorageCapacities, StoragePolicy};
use crate::sqlite::{connection, lookup, schema};

/// Result of one completed save operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SaveOutcome {
    /// Occurrences served by an exact existing row.
    pub reused: u64,
    /// Objects newly written.
    pub inserted: u64,
    /// Packs created by this operation.
    pub packs_created: u64,
    /// Appends to packs this operation created.
    pub pack_appends: u64,
    /// Write transactions acknowledged with `COMMIT`.
    ///
    /// Holding a `SaveOutcome` at all *is* the acknowledgement: `finish` returns
    /// one only after the watermark transaction committed, and every other outcome
    /// is a typed error. There is deliberately no boolean field for it - a field
    /// that can only ever hold one value cannot fail an assertion, and one used to
    /// sit here doing exactly that.
    pub commits: u64,
    /// Record-level objects newly written as a FULL representation.
    pub full_records: u64,
    /// Record-level objects newly written as a PREFIX representation.
    pub prefix_records: u64,
    /// Representation selection outcomes.
    pub delta: DeltaCounters,
    /// Work spent acquiring delta bases.
    pub chain: ChainCounters,
    /// Pooled metadata lane outcomes.
    pub pool: crate::cas::owner::PoolCounters,
}

impl From<OutcomeCounters> for SaveOutcome {
    fn from(counters: OutcomeCounters) -> Self {
        Self {
            reused: counters.reused,
            inserted: counters.inserted,
            packs_created: counters.packs_created,
            pack_appends: counters.pack_appends,
            commits: counters.commits,
            full_records: counters.full_records,
            prefix_records: counters.prefix_records,
            delta: counters.delta,
            chain: counters.chain,
            pool: counters.pool,
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
}

/// A content-addressed Store at one filesystem path.
#[derive(Clone, Debug)]
pub struct Store {
    path: PathBuf,
    policy: StoragePolicy,
    capacities: StorageCapacities,
    /// Store-owned bounded ordered set of pooled value candidates.
    ///
    /// It is disposable derivation: the catalogue is authoritative, a failed save
    /// invalidates it whole, and a reopened Store synchronizes it from the
    /// catalogue on first use.
    pool_index: Arc<Mutex<PoolIndex>>,
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
            let stored = schema::create(&connection, policy)?;
            let capacities = StorageCapacities::from_policy(stored)?;
            drop(connection);
            Ok(Self {
                path,
                policy: stored,
                capacities,
                pool_index: Arc::new(Mutex::new(PoolIndex::new())),
            })
        })
    }

    /// Opens an existing Store and validates its identity and policy.
    pub fn open(path: impl AsRef<Path>, scope: TimingScope<'_>) -> StorageResult<Self> {
        scope.run(|_open| {
            let path = path.as_ref().to_path_buf();
            let connection = connection::open(&path, false)?;
            let stored = schema::validate(&connection, None)?;
            let capacities = StorageCapacities::from_policy(stored)?;
            drop(connection);
            Ok(Self {
                path,
                policy: stored,
                capacities,
                pool_index: Arc::new(Mutex::new(PoolIndex::new())),
            })
        })
    }

    /// The default policy this slice creates and opens.
    pub const fn default_policy() -> StoragePolicy {
        StoragePolicy::frozen_default()
    }

    /// Persisted policy of this Store.
    pub fn policy(&self) -> StoragePolicy {
        self.policy
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

    /// Live bytes charged by the pooled-value ordered set.
    pub fn pool_index_bytes(&self) -> usize {
        self.pool_index
            .lock()
            .map(|index| index.live_bytes())
            .unwrap_or(0)
    }

    /// Acquires exclusive write ownership for one save operation.
    pub fn begin_save(&self, scope: TimingScope<'_>) -> StorageResult<SaveOperation> {
        scope.run(|_acquire| {
            let connection = connection::open(&self.path, false)?;
            let owner =
                MutationOwner::acquire(connection, self.capacities, Arc::clone(&self.pool_index))?;
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
            // The ceiling is the publication watermark: the last pack belonging to
            // a COMPLETED save. Deriving it from MAX(pack_id) would let an
            // unfinished save's early-committed packs leak into this read.
            let ceiling = schema::retained_pack_ceiling(&connection)?;
            let mut workspace = read_scope
                .child("storage.decode")
                .run(|_| DecompressionWorkspace::new())?;
            let (values, counters) = read_scope.child("storage.read").run(|_| {
                read::read_objects(&connection, ids, ceiling, &self.capacities, &mut workspace)
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
            let connection = connection::open(&self.path, false)?;
            let ceiling = schema::retained_pack_ceiling(&connection)?;
            lookup::present(&connection, ids, ceiling)
        })
    }
}

/// Refuses a demand larger than the declared read ceiling.
///
/// The ceiling is a caller-declared resource, not a trigger: a wave that exceeds
/// it fails before a connection is opened, so a caller cannot turn one query into
/// unbounded decode work by passing a longer slice.
fn check_read_demand(ids: &[ObjectId], limit: usize) -> StorageResult<()> {
    if ids.len() > limit {
        return Err(StorageError::CapacityExceeded {
            what: "storage.read_objects",
            limit: limit as u64,
            actual: ids.len() as u64,
        });
    }
    Ok(())
}

/// One exclusive save operation with bounded acceptance.
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
        if self.finished || self.terminal {
            return Err(StorageError::Aborted);
        }
        match self.batch.push(object) {
            Ok(Some(drained)) => self.flush(drained),
            Ok(None) => Ok(()),
            Err(error) => Err(self.terminate(error)),
        }
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
        let result = scope.run(|_finish| {
            let remaining = self.batch.drain();
            self.flush(remaining)?;
            let owner = self.owner.as_mut().ok_or(StorageError::Aborted)?;
            let counters = owner.finish()?;
            Ok(SaveOutcome::from(counters))
        });
        match result {
            Ok(outcome) => {
                self.finished = true;
                self.owner = None;
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
    pub fn pool_counters(&self) -> crate::cas::owner::PoolCounters {
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
