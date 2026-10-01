//! Deferred fixed count epochs and charged exact before/proposed custody.
use crate::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{
    BaseFact, CanonicalCapacity, CanonicalScope, CountEpoch, CountLedger, CountRecord, CountSeal,
    GraphMemory, GraphMemoryLease, ZeroLedger, ZeroSeal,
};
use std::cell::Cell;
#[derive(Clone, Debug, Default)]
pub(crate) struct Snapshot {
    pub(crate) stage: u8,
    pub(crate) records: u64,
    pub(crate) touched: u64,
    pub(crate) maximum: Option<u64>,
    pub(crate) zeros: u64,
    pub(crate) zero_maximum: Option<u64>,
    pub(crate) count_remaining: u64,
    pub(crate) zero_remaining: u64,
    pub(crate) after_count: Option<u64>,
    pub(crate) after_zero: Option<u64>,
    pub(crate) effects: Option<CountSeal>,
    pub(crate) final_seal: Option<CountSeal>,
    pub(crate) seeds: Option<ZeroSeal>,
}
pub(crate) struct RetireProof {
    pub(crate) counts: CountLedger,
    pub(crate) zeros: Option<ZeroLedger>,
}
pub(crate) struct RetireOwner {
    pub(crate) value: Box<RetireProof>,
    _memory: GraphMemoryLease,
}
impl RetireOwner {
    pub(crate) fn new(
        memory: &GraphMemory,
        seal: &CountSeal,
        seeds: Option<&ZeroSeal>,
    ) -> StorageResult<Self> {
        let lease =
            memory.reserve(std::mem::size_of::<Self>() + std::mem::size_of::<RetireProof>())?;
        Ok(Self {
            value: Box::new(RetireProof {
                counts: CountLedger::new(seal.scope.clone(), CountEpoch::Final)?,
                zeros: seeds
                    .map(|s| ZeroLedger::new(s.counts.clone()))
                    .transpose()?,
            }),
            _memory: lease,
        })
    }
    pub(crate) fn proposed(&self, memory: &GraphMemory) -> StorageResult<Self> {
        let lease =
            memory.reserve(std::mem::size_of::<Self>() + std::mem::size_of::<RetireProof>())?;
        Ok(Self {
            value: Box::new(RetireProof {
                counts: self.value.counts.clone(),
                zeros: self.value.zeros.clone(),
            }),
            _memory: lease,
        })
    }
}
pub(crate) struct AttemptData {
    pub(crate) retirement: Option<RetireOwner>,
    pub(crate) deleted_rows: Vec<CountRecord>,
    pub(crate) deleted_seeds: Vec<BaseFact>,
    pub(crate) kind: &'static str,
    pub(crate) before: Snapshot,
    pub(crate) after: Snapshot,
    pub(crate) rows: Vec<(Option<CountRecord>, Option<CountRecord>)>,
    pub(crate) seeds: Vec<(Option<BaseFact>, Option<BaseFact>)>,
}
pub(crate) struct CountAttempt {
    value: Box<AttemptData>,
    _memory: GraphMemoryLease,
}
impl CountAttempt {
    pub(crate) fn new(
        state: &Counts,
        kind: &'static str,
        rows: usize,
        seeds: usize,
    ) -> StorageResult<Self> {
        if rows.checked_add(seeds).is_none_or(|n| n > 128) {
            return Err(StorageError::Integrity("count attempt window"));
        }
        let deleting = kind == "retire";
        let count_width = if deleting {
            std::mem::size_of::<CountRecord>()
        } else {
            std::mem::size_of::<(Option<CountRecord>, Option<CountRecord>)>()
        };
        let seed_width = if deleting {
            std::mem::size_of::<BaseFact>()
        } else {
            std::mem::size_of::<(Option<BaseFact>, Option<BaseFact>)>()
        };
        let bytes = std::mem::size_of::<AttemptData>()
            + std::mem::size_of::<Self>()
            + rows * count_width
            + seeds * seed_width;
        let lease = state.memory.reserve(bytes)?;
        let mut count_rows = Vec::new();
        let mut zero_rows = Vec::new();
        let mut deleted_rows = Vec::new();
        let mut deleted_seeds = Vec::new();
        if deleting {
            window(&mut deleted_rows, rows)?;
            window(&mut deleted_seeds, seeds)?;
        } else {
            window(&mut count_rows, rows)?;
            window(&mut zero_rows, seeds)?;
        }
        Ok(Self {
            value: Box::new(AttemptData {
                retirement: None,
                deleted_rows,
                deleted_seeds,
                kind,
                before: state.snapshot.clone(),
                after: state.snapshot.clone(),
                rows: count_rows,
                seeds: zero_rows,
            }),
            _memory: lease,
        })
    }
}
impl std::ops::Deref for CountAttempt {
    type Target = AttemptData;
    fn deref(&self) -> &AttemptData {
        &self.value
    }
}
impl std::ops::DerefMut for CountAttempt {
    fn deref_mut(&mut self) -> &mut AttemptData {
        &mut self.value
    }
}
fn window<T>(rows: &mut Vec<T>, count: usize) -> StorageResult<()> {
    rows.try_reserve_exact(count)
        .map_err(|_| StorageError::Integrity("count attempt allocation"))?;
    if rows.capacity() != count {
        return Err(StorageError::Integrity("count attempt actual capacity"));
    }
    Ok(())
}
pub(crate) struct Counts {
    pub(crate) scope: CanonicalScope,
    pub(crate) capacity: CanonicalCapacity,
    pub(crate) memory: GraphMemory,
    pub(crate) retirement: Option<RetireOwner>,
    pub(crate) snapshot: Snapshot,
    pub(crate) failed: Cell<bool>,
    pub(crate) attempt: Option<CountAttempt>,
}
pub(crate) struct CountsOwner {
    value: Box<Counts>,
    _memory: GraphMemoryLease,
}
impl std::ops::Deref for CountsOwner {
    type Target = Counts;
    fn deref(&self) -> &Counts {
        &self.value
    }
}
impl std::ops::DerefMut for CountsOwner {
    fn deref_mut(&mut self) -> &mut Counts {
        &mut self.value
    }
}
impl Counts {
    // Construction returns the funded owner so its allocation and lease stay inseparable.
    #[allow(clippy::new_ret_no_self)]
    pub(crate) fn new(
        scope: CanonicalScope,
        capacity: CanonicalCapacity,
        memory: GraphMemory,
    ) -> StorageResult<CountsOwner> {
        let lease =
            memory.reserve(std::mem::size_of::<Self>() + std::mem::size_of::<CountsOwner>())?;
        Ok(CountsOwner {
            value: Box::new(Self {
                scope,
                capacity,
                memory,
                retirement: None,
                snapshot: Snapshot::default(),
                failed: Cell::new(false),
                attempt: None,
            }),
            _memory: lease,
        })
    }
    pub(crate) fn acknowledge(&mut self) {
        let CountAttempt { value, _memory } = self.attempt.take().unwrap();
        let AttemptData {
            after,
            rows,
            seeds,
            retirement,
            deleted_rows,
            deleted_seeds,
            ..
        } = *value;
        self.snapshot = after;
        if retirement.is_some() {
            self.retirement = retirement;
        }
        drop(rows);
        drop(seeds);
        drop(deleted_rows);
        drop(deleted_seeds);
        drop(_memory);
    }
    pub(crate) fn description(&self) -> String {
        match &self.attempt {
            Some(a) => format!(
                "counts {:?}; attempt {} {:?}->{:?}; rows={:?}; seeds={:?}; deleted_rows={:?}; deleted_seeds={:?}",
                self.scope.encode(),
                a.kind,
                a.before,
                a.after,
                a.rows,
                a.seeds,
                a.deleted_rows,
                a.deleted_seeds
            ),
            None => format!(
                "counts acknowledged {:?}; failed={}",
                self.snapshot,
                self.failed.get()
            ),
        }
    }
    pub(crate) fn live(&self) -> (u64, u64) {
        if self.snapshot.stage >= 5 {
            (self.snapshot.count_remaining, self.snapshot.zero_remaining)
        } else {
            (self.snapshot.records, self.snapshot.zeros)
        }
    }
}
