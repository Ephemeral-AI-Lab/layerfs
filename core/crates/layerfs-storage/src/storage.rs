//! Engine-independent two-port handle and shared bounded physical indexes.

use crate::{
    encoding::{delta::candidates::Candidates, pool::PoolIndex},
    save::Save,
};
use crate::{
    error::StorageResult,
    policy::{StorageCapacities, StoragePolicy},
    port::PackPersistence,
    read::{Diagnostics, Fetch, Reader},
};
use std::{cell::RefCell, sync::Arc};

/// C2's immutable-body path, independent of engines and history.
pub struct Storage {
    pub(crate) source: Fetch,
    pub(crate) work: crate::save::Work,
    policy: StoragePolicy,
    capacities: StorageCapacities,
    pub(crate) candidates: RefCell<Candidates>,
    pub(crate) pool_index: RefCell<PoolIndex>,
}
impl Storage {
    /// Opens one handle with an acknowledged persisted policy. No service bootstrap.
    pub fn new(metadata: Arc<dyn PackPersistence>) -> StorageResult<Self> {
        let source = Fetch::new(metadata);
        source.note(|c| c.policy += 1);
        let policy = source.metadata.policy()?.validated()?;
        let capacities = StorageCapacities::from_policy(policy)?;
        Ok(Self {
            source,
            work: crate::save::Work::default(),
            policy,
            capacities,
            candidates: RefCell::new(Candidates::new()?),
            pool_index: RefCell::new(PoolIndex::new()),
        })
    }
    /// Persisted, validated storage policy.
    pub const fn policy(&self) -> StoragePolicy {
        self.policy
    }
    /// Existing canonical, pack, wave and chain bounds under that policy.
    pub const fn capacities(&self) -> StorageCapacities {
        self.capacities
    }
    /// Begins one producer's bounded save. Separate handles may write concurrently.
    pub fn begin_save(&self) -> StorageResult<Save<'_>> {
        Save::new(self)
    }
    /// Creates an operation-owned authenticated reader with bounded caches.
    pub fn reader(&self) -> StorageResult<Reader<'_>> {
        Reader::new(self)
    }
    /// Bounded recent and cumulative save-stage wall observations.
    pub fn save_work(&self) -> crate::save::SaveHistory {
        self.work.snapshot()
    }
    /// Cumulative operation counts, labelled diagnostics; no timing claims.
    pub fn diagnostics(&self) -> Diagnostics {
        self.source.counters()
    }
}
