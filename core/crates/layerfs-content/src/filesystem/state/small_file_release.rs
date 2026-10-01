//! Fixed once-owned regular-file jobs and explicitly absent directory frames.
use super::empty_owner::bad;
use super::small_file_owner::SmallStage;
use super::*;
use crate::{ContentResult, ObjectId};
impl VerifiedSmallFileState {
    fn release_context(&self, scope: &CanonicalScope) -> ContentResult<()> {
        self.selected(scope, StateTable::ReleaseJobs, 7)?;
        if self.data.release_scope.as_ref() != Some(scope) || self.data.release_retired {
            return Err(bad("small file release phase/context"));
        }
        Ok(())
    }
}
impl ReleaseState for VerifiedSmallFileState {
    fn release_begin(&mut self, scope: &CanonicalScope, seeds: &ZeroSeal) -> ContentResult<()> {
        self.selected(scope, StateTable::ReleaseJobs, 7)?;
        self.selected_zeros(seeds)?;
        if self.data.stage != SmallStage::Resumed
            || self.data.release_scope.is_some()
            || scope != &seeds.counts.scope.jobs()?
        {
            return Err(bad("small file release begin replay/phase"));
        }
        self.data.release_scope = Some(scope.clone());
        Ok(())
    }
    fn release_seed(&mut self, scope: &CanonicalScope, records: &[BaseFact]) -> ContentResult<()> {
        self.release_context(scope)?;
        if self.data.seeds_closed || self.data.seed_len + records.len() > self.data.zero_len {
            return Err(bad("small file release seed count/phase"));
        }
        for (ordinal, row) in records.iter().enumerate() {
            if self.data.zeros[self.data.seed_len + ordinal] != Some(*row)
                || *row != self.base(row.serial)?
            {
                return Err(bad("small file release exact source seeds"));
            }
        }
        self.occupancy(
            self.data.records.iter().flatten().count() as u64,
            self.data.zero_len as u64,
            (self.data.seed_len + records.len()) as u64,
        )?;
        for row in records {
            let at = self.data.seed_len;
            self.data.jobs[at] = Some(ReleaseJob {
                sequence: (at + 1) as u64,
                serial: row.serial,
                base: row.value,
            });
            self.data.seed_len += 1;
        }
        Ok(())
    }
    fn release_close_seeds(&mut self, scope: &CanonicalScope) -> ContentResult<()> {
        self.release_context(scope)?;
        if self.data.seeds_closed || self.data.seed_len != self.data.zero_len {
            return Err(bad("small file seed close before EOF/replay"));
        }
        let mut ledger = ZeroLedger::new(
            self.data
                .effects
                .clone()
                .ok_or(bad("small file effect seal"))?,
        )?;
        for job in self.data.jobs[..self.data.seed_len].iter().flatten() {
            ledger.append(&[BaseFact {
                serial: job.serial,
                value: job.base,
            }])?;
        }
        if self.data.zero_seal.as_ref() != Some(&ledger.seal()?) {
            return Err(bad("small file release seed transcript"));
        }
        self.data.seeds_closed = true;
        Ok(())
    }
    fn release_take(&mut self, scope: &CanonicalScope) -> ContentResult<Option<ReleaseJob>> {
        self.release_context(scope)?;
        if !self.data.seeds_closed
            || self.data.current.is_some()
            || self.data.release_seal.is_some()
        {
            return Err(bad("small file job take phase"));
        }
        if self.data.next_job == self.data.seed_len {
            self.data.release_eof = true;
            return Ok(None);
        }
        let at = self.data.next_job;
        let job = self.data.jobs[at]
            .take()
            .ok_or(bad("small file FIFO expected job"))?;
        self.data.next_job += 1;
        self.data.current = Some(job);
        Ok(Some(job))
    }
    fn release_complete_job(
        &mut self,
        scope: &CanonicalScope,
        job: &ReleaseJob,
        directory: Option<ObjectId>,
    ) -> ContentResult<()> {
        self.release_context(scope)?;
        if directory.is_some()
            || self.data.current.as_ref() != Some(job)
            || job.base != self.base(job.serial)?.value
        {
            return Err(bad("small file current job/no directory growth"));
        }
        self.data.current = None;
        self.data.completed_jobs += 1;
        Ok(())
    }
    fn release_frame(&mut self, scope: &CanonicalScope) -> ContentResult<Option<ReleaseFrame>> {
        self.release_context(scope)?;
        if !self.data.release_eof || self.data.current.is_some() {
            return Err(bad("small file frames before job EOF"));
        }
        Ok(None)
    }
    fn release_advance(
        &mut self,
        scope: &CanonicalScope,
        _before: &ReleaseFrame,
        _after: &ReleaseFrame,
        _children: &[BaseFact],
    ) -> ContentResult<ReleaseFrame> {
        self.release_context(scope)?;
        Err(bad("small file frame/descendant growth forbidden"))
    }
    fn release_pop(&mut self, scope: &CanonicalScope, _frame: &ReleaseFrame) -> ContentResult<()> {
        self.release_context(scope)?;
        Err(bad("small file has no frame"))
    }
    fn release_seal(&mut self, scope: &CanonicalScope) -> ContentResult<ReleaseSeal> {
        self.release_context(scope)?;
        if !self.data.release_eof
            || self.data.current.is_some()
            || self.data.release_seal.is_some()
            || self.data.completed_jobs != self.data.seed_len as u64
        {
            return Err(bad("small file release seal before EOF/replay"));
        }
        let seal = ReleaseSeal {
            scope: scope.clone(),
            sequence: self.data.seed_len as u64,
            jobs: self.data.completed_jobs,
            directories: 0,
            maximum_depth: 0,
        };
        self.data.release_seal = Some(seal.clone());
        Ok(seal)
    }
    fn release_retire(&mut self, seal: &ReleaseSeal) -> ContentResult<()> {
        self.release_context(&seal.scope)?;
        if self.data.release_seal.as_ref() != Some(seal)
            || self.data.jobs.iter().any(Option::is_some)
            || self.data.current.is_some()
        {
            return Err(bad("small file selected release EOF"));
        }
        self.data.release_retired = true;
        Ok(())
    }
    fn release_abandon(&mut self, scope: &CanonicalScope) -> ContentResult<()> {
        self.selected(scope, StateTable::ReleaseJobs, 7)?;
        self.abandon();
        Ok(())
    }
}
