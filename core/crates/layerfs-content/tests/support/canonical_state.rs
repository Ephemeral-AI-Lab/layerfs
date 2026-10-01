//! Finite external count/release oracle; maps/queues are not a native RAM proof.
use super::ObservedGraph;
use layerfs_content::filesystem::state::*;
use layerfs_content::{ContentError, ContentResult, ObjectId};
use std::collections::{BTreeMap, VecDeque};
#[derive(Default)]
pub struct FixtureCanonical {
    pub counts_scope: Option<CanonicalScope>,
    pub counts: BTreeMap<u64, CountRecord>,
    pub stage: u8,
    pub count_seal: Option<CountSeal>,
    pub zeros: BTreeMap<u64, BaseFact>,
    pub zero_seal: Option<ZeroSeal>,
    pub release_scope: Option<CanonicalScope>,
    pub seeds: Option<ZeroSeal>,
    pub jobs: VecDeque<ReleaseJob>,
    pub current: Option<ReleaseJob>,
    pub frames: Vec<ReleaseFrame>,
    pub sequence: u64,
    pub completed: u64,
    pub directories: u64,
    pub maximum_depth: u64,
    pub closed: bool,
    pub retired: bool,
}
fn bad() -> ContentError {
    ContentError::InvalidOrderingRecord("external canonical fixture")
}
impl CountState for ObservedGraph {
    fn count_capacity(&self, scope: &CanonicalScope) -> ContentResult<CanonicalCapacity> {
        self.count_memory(scope)?;
        CanonicalCapacity::new(100000, 100000, 100000, 100000, 16 * 1024 * 1024)
    }
    fn count_memory(&self, scope: &CanonicalScope) -> ContentResult<GraphMemory> {
        if scope.subject().selected() != self.scope.subject()
            || self
                .facts
                .base
                .as_ref()
                .is_none_or(|base| base.subject() != scope.subject())
        {
            return Err(bad());
        }
        Ok(self.facts.memory.clone())
    }
    fn count_begin(&mut self, scope: &CanonicalScope) -> ContentResult<()> {
        self.count_memory(scope)?;
        if self.canonical.counts_scope.is_some()
            || self.stage != GraphStage::Retired
            || self.facts.base_seal.is_some()
        {
            return Err(bad());
        }
        self.canonical.counts_scope = Some(scope.clone());
        self.canonical.stage = 1;
        Ok(())
    }
    fn count_get(
        &mut self,
        scope: &CanonicalScope,
        serial: u64,
    ) -> ContentResult<Option<CountRecord>> {
        if self.canonical.counts_scope.as_ref() != Some(scope) {
            return Err(bad());
        }
        Ok(self.canonical.counts.get(&serial).copied())
    }
    fn count_cas(
        &mut self,
        scope: &CanonicalScope,
        before: Option<CountRecord>,
        after: &CountRecord,
    ) -> ContentResult<CountRecord> {
        if self.canonical.counts_scope.as_ref() != Some(scope)
            || ![1, 3].contains(&self.canonical.stage)
            || self.canonical.counts.get(&after.serial()).copied() != before
        {
            return Err(bad());
        }
        self.canonical.counts.insert(after.serial(), *after);
        Ok(*after)
    }
    fn count_seal(
        &mut self,
        scope: &CanonicalScope,
        epoch: CountEpoch,
    ) -> ContentResult<CountSeal> {
        if self.canonical.counts_scope.as_ref() != Some(scope)
            || ![1, 3].contains(&self.canonical.stage)
        {
            return Err(bad());
        }
        let mut ledger = CountLedger::new(scope.clone(), epoch)?;
        for row in self.canonical.counts.values() {
            ledger.append(&[*row])?;
        }
        let seal = ledger.seal()?;
        self.canonical.count_seal = Some(seal.clone());
        self.canonical.stage = if epoch == CountEpoch::Effects { 2 } else { 4 };
        Ok(seal)
    }
    fn count_page(
        &mut self,
        seal: &CountSeal,
        after: Option<u64>,
        records: usize,
        bytes: usize,
    ) -> ContentResult<CountPage> {
        if self.canonical.count_seal.as_ref() != Some(seal)
            || records == 0
            || records > 128
            || bytes < 310
        {
            return Err(bad());
        }
        let count = records.min((bytes - 310) / 128);
        let lease = self.facts.memory.reserve(
            std::mem::size_of::<CountPage>() + count * std::mem::size_of::<CountRecord>(),
        )?;
        let mut rows = Vec::new();
        rows.try_reserve_exact(count).unwrap();
        rows.extend(
            self.canonical
                .counts
                .values()
                .filter(|r| after.is_none_or(|s| r.serial() > s))
                .take(count)
                .copied(),
        );
        let last = rows.last().map(|r| r.serial()).or(after);
        CountPage::new(lease, seal.clone(), rows, last, last == seal.maximum)
    }
    fn zero_append(&mut self, counts: &CountSeal, records: &[BaseFact]) -> ContentResult<()> {
        if self.canonical.stage != 2 || self.canonical.count_seal.as_ref() != Some(counts) {
            return Err(bad());
        }
        for row in records {
            if self.canonical.zeros.insert(row.serial, *row).is_some() {
                return Err(bad());
            }
        }
        Ok(())
    }
    fn zero_seal(&mut self, counts: &CountSeal) -> ContentResult<ZeroSeal> {
        if self.canonical.stage != 2 || self.canonical.count_seal.as_ref() != Some(counts) {
            return Err(bad());
        }
        let mut ledger = ZeroLedger::new(counts.clone())?;
        for row in self.canonical.zeros.values() {
            ledger.append(&[*row])?;
        }
        let seal = ledger.seal()?;
        self.canonical.zero_seal = Some(seal.clone());
        Ok(seal)
    }
    fn zero_page(
        &mut self,
        seal: &ZeroSeal,
        after: Option<u64>,
        records: usize,
        bytes: usize,
    ) -> ContentResult<ZeroPage> {
        if self.canonical.zero_seal.as_ref() != Some(seal)
            || records == 0
            || records > 128
            || bytes < 595
        {
            return Err(bad());
        }
        let count = records.min((bytes - 595) / 105);
        let lease = self
            .facts
            .memory
            .reserve(std::mem::size_of::<ZeroPage>() + count * std::mem::size_of::<BaseFact>())?;
        let mut rows = Vec::new();
        rows.try_reserve_exact(count).unwrap();
        rows.extend(
            self.canonical
                .zeros
                .values()
                .filter(|r| after.is_none_or(|s| r.serial > s))
                .take(count)
                .copied(),
        );
        let last = rows.last().map(|r| r.serial).or(after);
        ZeroPage::new(lease, seal.clone(), rows, last, last == seal.maximum)
    }
    fn count_resume(&mut self, seeds: &ZeroSeal) -> ContentResult<()> {
        if self.canonical.zero_seal.as_ref() != Some(seeds) || self.canonical.stage != 2 {
            return Err(bad());
        }
        self.canonical.stage = 3;
        Ok(())
    }
    fn count_retire(&mut self, seal: &CountSeal, seeds: Option<&ZeroSeal>) -> ContentResult<()> {
        if self.canonical.stage != 4
            || self.canonical.count_seal.as_ref() != Some(seal)
            || self.canonical.zero_seal.as_ref() != seeds
        {
            return Err(bad());
        }
        self.canonical.counts.clear();
        self.canonical.zeros.clear();
        self.canonical.stage = 5;
        Ok(())
    }
    fn count_abandon(&mut self, _scope: &CanonicalScope) -> ContentResult<()> {
        self.failed = true;
        Ok(())
    }
}
impl ReleaseState for ObservedGraph {
    fn release_begin(&mut self, scope: &CanonicalScope, seeds: &ZeroSeal) -> ContentResult<()> {
        if self.canonical.release_scope.is_some()
            || self.canonical.stage != 3
            || self.canonical.zero_seal.as_ref() != Some(seeds)
            || scope != &seeds.counts.scope.jobs()?
        {
            return Err(bad());
        }
        self.canonical.release_scope = Some(scope.clone());
        self.canonical.seeds = Some(seeds.clone());
        Ok(())
    }
    fn release_seed(&mut self, scope: &CanonicalScope, records: &[BaseFact]) -> ContentResult<()> {
        if self.canonical.release_scope.as_ref() != Some(scope) || self.canonical.closed {
            return Err(bad());
        }
        for r in records {
            self.canonical.sequence += 1;
            self.canonical.jobs.push_back(ReleaseJob {
                sequence: self.canonical.sequence,
                serial: r.serial,
                base: r.value,
            });
        }
        Ok(())
    }
    fn release_close_seeds(&mut self, scope: &CanonicalScope) -> ContentResult<()> {
        if self.canonical.release_scope.as_ref() != Some(scope)
            || self.canonical.jobs.len() as u64 != self.canonical.seeds.as_ref().unwrap().records
        {
            return Err(bad());
        }
        self.canonical.closed = true;
        Ok(())
    }
    fn release_take(&mut self, scope: &CanonicalScope) -> ContentResult<Option<ReleaseJob>> {
        if self.canonical.release_scope.as_ref() != Some(scope)
            || !self.canonical.closed
            || self.canonical.current.is_some()
        {
            return Err(bad());
        }
        self.canonical.current = self.canonical.jobs.pop_front();
        Ok(self.canonical.current)
    }
    fn release_complete_job(
        &mut self,
        scope: &CanonicalScope,
        job: &ReleaseJob,
        directory: Option<ObjectId>,
    ) -> ContentResult<()> {
        if self.canonical.release_scope.as_ref() != Some(scope)
            || self.canonical.current != Some(*job)
        {
            return Err(bad());
        }
        if let Some(root) = directory {
            let depth = self.canonical.frames.len() as u64 + 1;
            self.canonical.frames.push(ReleaseFrame {
                depth,
                root,
                after: None,
                finished: false,
            });
            self.canonical.directories += 1;
            self.canonical.maximum_depth = self.canonical.maximum_depth.max(depth);
        }
        self.canonical.current = None;
        self.canonical.completed += 1;
        Ok(())
    }
    fn release_frame(&mut self, scope: &CanonicalScope) -> ContentResult<Option<ReleaseFrame>> {
        if self.canonical.release_scope.as_ref() != Some(scope)
            || self.canonical.current.is_some()
            || !self.canonical.jobs.is_empty()
        {
            return Err(bad());
        }
        Ok(self.canonical.frames.last().copied())
    }
    fn release_advance(
        &mut self,
        scope: &CanonicalScope,
        before: &ReleaseFrame,
        after: &ReleaseFrame,
        children: &[BaseFact],
    ) -> ContentResult<ReleaseFrame> {
        if self.canonical.release_scope.as_ref() != Some(scope)
            || self.canonical.frames.last() != Some(before)
        {
            return Err(bad());
        }
        before.advances_to(*after)?;
        *self.canonical.frames.last_mut().unwrap() = *after;
        for row in children {
            self.canonical.sequence += 1;
            self.canonical.jobs.push_back(ReleaseJob {
                sequence: self.canonical.sequence,
                serial: row.serial,
                base: row.value,
            });
        }
        Ok(*after)
    }
    fn release_pop(&mut self, scope: &CanonicalScope, frame: &ReleaseFrame) -> ContentResult<()> {
        if self.canonical.release_scope.as_ref() != Some(scope)
            || self.canonical.frames.last() != Some(frame)
            || !frame.finished
        {
            return Err(bad());
        }
        self.canonical.frames.pop();
        Ok(())
    }
    fn release_seal(&mut self, scope: &CanonicalScope) -> ContentResult<ReleaseSeal> {
        if self.canonical.release_scope.as_ref() != Some(scope)
            || !self.canonical.jobs.is_empty()
            || self.canonical.current.is_some()
            || !self.canonical.frames.is_empty()
        {
            return Err(bad());
        }
        Ok(ReleaseSeal {
            scope: scope.clone(),
            sequence: self.canonical.sequence,
            jobs: self.canonical.completed,
            directories: self.canonical.directories,
            maximum_depth: self.canonical.maximum_depth,
        })
    }
    fn release_retire(&mut self, seal: &ReleaseSeal) -> ContentResult<()> {
        if self.release_seal(&seal.scope)? != *seal {
            return Err(bad());
        }
        self.canonical.retired = true;
        Ok(())
    }
    fn release_abandon(&mut self, _scope: &CanonicalScope) -> ContentResult<()> {
        self.failed = true;
        Ok(())
    }
}
