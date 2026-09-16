//! Public Store handle, the exclusive save operation and the C1 handoff adapter.
//!
//! A Store is a path plus the policy it was opened with. `begin_save` acquires
//! exclusive write ownership once; `accept` takes finalized canonical objects
//! under bounded batch limits; `finish` completes every remaining write and
//! acknowledges the final transaction. Reads are independent bounded waves that
//! capture their retained-pack ceiling once.

use std::path::{Path, PathBuf};

use layerfs_content::{ContentError, FinalizedConsumer, FinalizedObject, ObjectId};
use layerfs_telemetry::timer::TimingScope;

use crate::cas::batch::PendingBatch;
use crate::cas::owner::{MutationOwner, OutcomeCounters};
use crate::cas::{finish, read, save};
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
    pub commits: u64,
    /// True when the final transaction was acknowledged.
    pub acknowledged: bool,
}

impl From<OutcomeCounters> for SaveOutcome {
    fn from(counters: OutcomeCounters) -> Self {
        Self {
            reused: counters.reused,
            inserted: counters.inserted,
            packs_created: counters.packs_created,
            pack_appends: counters.pack_appends,
            commits: counters.commits,
            acknowledged: true,
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
}

/// A content-addressed Store at one filesystem path.
#[derive(Clone, Debug)]
pub struct Store {
    path: PathBuf,
    policy: StoragePolicy,
    capacities: StorageCapacities,
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
            })
        })
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

    /// Acquires exclusive write ownership for one save operation.
    pub fn begin_save(&self, scope: TimingScope<'_>) -> StorageResult<SaveOperation> {
        scope.run(|_acquire| {
            let connection = connection::open(&self.path, false)?;
            let owner = MutationOwner::acquire(connection, self.policy, self.capacities)?;
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
        scope.run(|read_scope| {
            let connection = connection::open(&self.path, false)?;
            let ceiling = lookup::highest_pack_id(&connection)?;
            let mut workspace = read_scope
                .child("storage.decode")
                .run(|_| DecompressionWorkspace::new())?;
            let (values, counters) = read_scope
                .child("storage.read")
                .run(|_| read::read_objects(&connection, ids, ceiling, &mut workspace))?;
            Ok((
                values,
                StoreReadCounters {
                    objects: counters.objects,
                    packs_read: counters.packs_read,
                    pages: counters.pages,
                    ceiling,
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
            lookup::present(&connection, ids, i64::MAX)
        })
    }
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
    pub fn accept(&mut self, object: FinalizedObject, scope: TimingScope<'_>) -> StorageResult<()> {
        scope.run(|accept| {
            accept
                .child("storage.batch")
                .run(|_| self.accept_inner(object))
        })
    }

    /// Accepts one object without a caller scope; used by the C1 handoff adapter.
    pub fn accept_inner(&mut self, object: FinalizedObject) -> StorageResult<()> {
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
        let result = scope.run(|finish_scope| {
            let remaining = self.batch.drain();
            self.flush(remaining)?;
            let owner = self.owner.as_mut().ok_or(StorageError::Aborted)?;
            let counters = finish_scope
                .child("storage.finish")
                .run(|_| owner.finish())?;
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
    /// state; the remainder is read through the open transaction. A same-save
    /// read therefore observes accepted output without a separate mechanism and
    /// without exposing it to unrelated readers.
    pub fn read_batch(
        &mut self,
        ids: &[ObjectId],
        scope: TimingScope<'_>,
    ) -> StorageResult<Vec<Vec<u8>>> {
        scope.run(|read_scope| {
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
            let stored = if remaining.is_empty() {
                Vec::new()
            } else {
                let owner = self.owner.as_mut().ok_or(StorageError::Aborted)?;
                read_scope
                    .child("storage.read")
                    .run(|_| owner.read_batch(&remaining))?
            };
            let mut stored = stored.into_iter();
            let mut values = Vec::with_capacity(ids.len());
            for id in ids {
                match pending.iter().find(|(candidate, _)| candidate == id) {
                    Some((_, bytes)) => values.push(bytes.clone()),
                    None => values.push(stored.next().ok_or(StorageError::ObjectMissing(*id))?),
                }
            }
            if stored.next().is_some() {
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
        match self.operation.accept_inner(object) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.failure = Some(error);
                Err(ContentError::OutputRejected)
            }
        }
    }
}
