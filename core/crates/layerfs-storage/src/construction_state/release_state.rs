//! Deferred release owners, exact current job and funded seed transcript custody.
use crate::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{
    CanonicalCapacity, CanonicalScope, GraphMemory, GraphMemoryLease, ReleaseFrame, ReleaseJob,
    ReleaseSeal, ZeroLedger, ZeroSeal,
};
use std::cell::Cell;
#[derive(Clone, Debug, Default)]
pub(crate) struct Snapshot {
    pub(crate) stage: u8,
    pub(crate) seeds: Option<ZeroSeal>,
    pub(crate) seeded: u64,
    pub(crate) seed_after: Option<u64>,
    pub(crate) sequence: u64,
    pub(crate) pending: u64,
    pub(crate) current: Option<ReleaseJob>,
    pub(crate) frames: u64,
    pub(crate) completed: u64,
    pub(crate) directories: u64,
    pub(crate) maximum_depth: u64,
}
pub(crate) struct FoldOwner {
    pub(crate) ledger: Box<ZeroLedger>,
    _memory: GraphMemoryLease,
}
impl FoldOwner {
    pub(crate) fn new(memory: &GraphMemory, seeds: &ZeroSeal) -> StorageResult<Self> {
        let lease =
            memory.reserve(std::mem::size_of::<Self>() + std::mem::size_of::<ZeroLedger>())?;
        Ok(Self {
            ledger: Box::new(ZeroLedger::new(seeds.counts.clone())?),
            _memory: lease,
        })
    }
    pub(crate) fn proposed(&self, memory: &GraphMemory) -> StorageResult<Self> {
        let lease =
            memory.reserve(std::mem::size_of::<Self>() + std::mem::size_of::<ZeroLedger>())?;
        Ok(Self {
            ledger: Box::new((*self.ledger).clone()),
            _memory: lease,
        })
    }
}
pub(crate) struct AttemptData {
    pub(crate) kind: &'static str,
    pub(crate) before: Snapshot,
    pub(crate) after: Snapshot,
    pub(crate) jobs: Vec<ReleaseJob>,
    pub(crate) taken: Option<ReleaseJob>,
    pub(crate) frame: Option<(Option<ReleaseFrame>, Option<ReleaseFrame>)>,
    pub(crate) fold: Option<FoldOwner>,
}
pub(crate) struct ReleaseAttempt {
    value: Box<AttemptData>,
    _memory: GraphMemoryLease,
}
impl std::ops::Deref for ReleaseAttempt {
    type Target = AttemptData;
    fn deref(&self) -> &AttemptData {
        &self.value
    }
}
impl std::ops::DerefMut for ReleaseAttempt {
    fn deref_mut(&mut self) -> &mut AttemptData {
        &mut self.value
    }
}
impl ReleaseAttempt {
    pub(crate) fn new(state: &Release, kind: &'static str, jobs: usize) -> StorageResult<Self> {
        if jobs > 128 {
            return Err(StorageError::Integrity("release attempt window"));
        }
        let lease = state.memory.reserve(
            std::mem::size_of::<Self>()
                + std::mem::size_of::<AttemptData>()
                + jobs * std::mem::size_of::<ReleaseJob>(),
        )?;
        let mut rows = Vec::new();
        rows.try_reserve_exact(jobs)
            .map_err(|_| StorageError::Integrity("release attempt allocation"))?;
        if rows.capacity() != jobs {
            return Err(StorageError::Integrity("release attempt actual capacity"));
        }
        Ok(Self {
            value: Box::new(AttemptData {
                kind,
                before: state.snapshot.clone(),
                after: state.snapshot.clone(),
                jobs: rows,
                taken: None,
                frame: None,
                fold: None,
            }),
            _memory: lease,
        })
    }
    pub(crate) fn enqueue(
        &mut self,
        serial: u64,
        base: Option<layerfs_content::object::inode_leaf::InodeValue>,
        scope: &CanonicalScope,
    ) -> StorageResult<()> {
        if self.jobs.len() == self.jobs.capacity() {
            return Err(StorageError::Integrity("release closed enqueue capacity"));
        }
        let sequence = self
            .after
            .sequence
            .checked_add(1)
            .filter(|n| *n <= i64::MAX as u64)
            .ok_or(StorageError::Integrity("release rank exhausted"))?;
        let job = ReleaseJob {
            sequence,
            serial,
            base,
        };
        ReleaseJob::decode(scope, &scope.key(sequence)?, &job.encode_value()?)?;
        self.jobs.push(job);
        self.after.sequence = sequence;
        self.after.pending = self
            .after
            .pending
            .checked_add(1)
            .ok_or(StorageError::Integrity("release pending overflow"))?;
        Ok(())
    }
}
pub(crate) struct Release {
    pub(crate) scope: CanonicalScope,
    pub(crate) capacity: CanonicalCapacity,
    pub(crate) memory: GraphMemory,
    pub(crate) snapshot: Snapshot,
    pub(crate) failed: Cell<bool>,
    pub(crate) attempt: Option<ReleaseAttempt>,
    pub(crate) fold: FoldOwner,
}
pub(crate) struct ReleaseOwner {
    value: Box<Release>,
    _memory: GraphMemoryLease,
}
impl std::ops::Deref for ReleaseOwner {
    type Target = Release;
    fn deref(&self) -> &Release {
        &self.value
    }
}
impl std::ops::DerefMut for ReleaseOwner {
    fn deref_mut(&mut self) -> &mut Release {
        &mut self.value
    }
}
impl Release {
    // Construction returns the funded owner so its allocation and lease stay inseparable.
    #[allow(clippy::new_ret_no_self)]
    pub(crate) fn new(
        scope: CanonicalScope,
        capacity: CanonicalCapacity,
        memory: GraphMemory,
        seeds: &ZeroSeal,
    ) -> StorageResult<ReleaseOwner> {
        let lease =
            memory.reserve(std::mem::size_of::<Self>() + std::mem::size_of::<ReleaseOwner>())?;
        let fold = FoldOwner::new(&memory, seeds)?;
        Ok(ReleaseOwner {
            value: Box::new(Self {
                scope,
                capacity,
                memory,
                snapshot: Snapshot::default(),
                failed: Cell::new(false),
                attempt: None,
                fold,
            }),
            _memory: lease,
        })
    }
    pub(crate) fn live_jobs(&self) -> u64 {
        self.snapshot.pending + u64::from(self.snapshot.current.is_some())
    }
    pub(crate) fn seal(&self) -> ReleaseSeal {
        ReleaseSeal {
            scope: self.scope.clone(),
            sequence: self.snapshot.sequence,
            jobs: self.snapshot.completed,
            directories: self.snapshot.directories,
            maximum_depth: self.snapshot.maximum_depth,
        }
    }
    pub(crate) fn acknowledge(&mut self) {
        let ReleaseAttempt { value, _memory } = self.attempt.take().unwrap();
        let AttemptData {
            after, jobs, fold, ..
        } = *value;
        self.snapshot = after;
        if let Some(fold) = fold {
            self.fold = fold;
        }
        drop(jobs);
        drop(_memory);
    }
    pub(crate) fn description(&self) -> String {
        match &self.attempt {
            Some(a) => format!(
                "release {:?}; {} {:?}->{:?}; jobs={:?}; taken={:?}; frame={:?}",
                self.scope.encode(),
                a.kind,
                a.before,
                a.after,
                a.jobs,
                a.taken,
                a.frame
            ),
            None => format!(
                "release acknowledged {:?}; failed={}",
                self.snapshot,
                self.failed.get()
            ),
        }
    }
}
