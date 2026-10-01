//! Original subject selection, combined native capacity and exact seed admission.
use super::{
    release_index, release_mutation,
    release_state::{Release, ReleaseAttempt},
    session::Resource,
    ScratchSession,
};
use crate::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{BaseFact, CanonicalScope, ZeroSeal};
use layerfs_content::ContentError;
impl ScratchSession {
    pub(crate) fn release_context(&self, scope: &CanonicalScope) -> StorageResult<&Resource> {
        let r = self
            .resource
            .as_ref()
            .ok_or(StorageError::Integrity("release owner released"))?;
        let counts = r
            .counts
            .as_ref()
            .ok_or(StorageError::Integrity("release counts unavailable"))?;
        if scope.state().table().code() != 21
            || *scope != counts.scope.jobs()?
            || r.releasing.as_ref().is_some_and(|s| &s.scope != scope)
        {
            return Err(StorageError::Content(ContentError::InvalidOrderingRecord(
                "release foreign scope",
            )));
        }
        self.count_context(&counts.scope)?;
        if r.releasing
            .as_ref()
            .is_some_and(|s| Some(s.capacity) != r.canonical_capacity)
            || !matches!(counts.snapshot.stage, 2..=4)
            || r.releasing
                .as_ref()
                .is_some_and(|s| s.failed.get() || s.attempt.is_some())
        {
            return Err(StorageError::Integrity(
                "release selected owner unavailable",
            ));
        }
        Ok(r)
    }
    /// Admit the deferred Release owner for this exact complete zero epoch.
    /// Use the same namespace working controller and retain full seed context
    /// before dependent native/SQL effects; no fresh fallback is selected.
    pub fn release_begin(&mut self, scope: &CanonicalScope, seeds: &ZeroSeal) -> StorageResult<()> {
        self.release_context(scope)?;
        let result = (|| {
            let r = self.resource.as_mut().unwrap();
            let counts = r.counts.as_ref().unwrap();
            if r.releasing.is_some()
                || counts.snapshot.seeds.as_ref() != Some(seeds)
                || counts.scope != seeds.counts.scope
                || counts.snapshot.stage != 3
            {
                return Err(StorageError::Integrity("release exact deferred epoch"));
            }
            r.verify()?;
            release_index::deferred(r.connection.as_ref().unwrap())?;
            r.releasing = Some(Release::new(
                scope.clone(),
                r.canonical_capacity.unwrap(),
                r.graph_memory.as_ref().unwrap().clone(),
                seeds,
            )?);
            let state = r.releasing.as_mut().unwrap();
            let mut a = ReleaseAttempt::new(state, "begin", 0)?;
            a.after.stage = 1;
            a.after.seeds = Some(seeds.clone());
            state.attempt = Some(a);
            r.native.reserve()?;
            release_mutation::commit(
                r.connection.as_ref().unwrap(),
                r.engine,
                r.releasing.as_ref().unwrap(),
            )?;
            r.native.observe_allocation()?;
            r.releasing.as_mut().unwrap().acknowledge();
            Ok(())
        })();
        self.finish(result)
    }
    pub(crate) fn release_change(
        &mut self,
        scope: &CanonicalScope,
        a: ReleaseAttempt,
    ) -> StorageResult<()> {
        self.release_context(scope)?;
        let result = (|| {
            let r = self.resource.as_mut().unwrap();
            r.verify()?;
            let jobs = a
                .after
                .pending
                .checked_add(u64::from(a.after.current.is_some()))
                .ok_or(StorageError::Integrity("release live jobs overflow"))?;
            r.canonical_growth(None, None, Some(jobs), Some(a.after.frames))?;
            r.releasing.as_mut().unwrap().attempt = Some(a);
            r.native.reserve()?;
            release_mutation::commit(
                r.connection.as_ref().unwrap(),
                r.engine,
                r.releasing.as_ref().unwrap(),
            )?;
            r.native.observe_allocation()?;
            r.releasing.as_mut().unwrap().acknowledge();
            Ok(())
        })();
        self.finish(result)
    }
    /// Append at most128 complete ordered seeds from the selected zero authority.
    /// Compare full carried base records and fund exact job/transcript ownership
    /// before SQL; retain known/proposed seed folds through Unknown.
    pub fn release_seed(
        &mut self,
        scope: &CanonicalScope,
        records: &[BaseFact],
    ) -> StorageResult<()> {
        self.release_context(scope)?;
        let result = (|| {
            let r = self.release_context(scope)?;
            let state = r
                .releasing
                .as_ref()
                .ok_or(StorageError::Integrity("release not begun"))?;
            if state.snapshot.stage != 1 || records.len() > 128 {
                return Err(StorageError::Integrity("release seed window/phase"));
            }
            let mut a = ReleaseAttempt::new(state, "seed", records.len())?;
            let mut fold = state.fold.proposed(&state.memory)?;
            fold.ledger.append(records)?;
            for fact in records {
                if state
                    .snapshot
                    .seeds
                    .as_ref()
                    .unwrap()
                    .maximum
                    .is_none_or(|m| fact.serial > m)
                {
                    return Err(StorageError::Integrity("release seed selected maximum"));
                }
                let actual = super::count_index::zero(
                    r.verify()?,
                    &state.snapshot.seeds.as_ref().unwrap().scope,
                    fact.serial,
                )?;
                if actual != Some(*fact) {
                    return Err(StorageError::Integrity("release seed exact carried base"));
                }
                a.enqueue(fact.serial, fact.value, scope)?;
            }
            a.after.seeded = fold.ledger.records();
            a.after.seed_after = fold.ledger.last();
            a.fold = Some(fold);
            self.release_change(scope, a)
        })();
        self.finish(result)
    }
    /// Enable traversal only after the full independently folded seed seal agrees.
    /// No partial count, key or digest can authorize a FIFO take.
    pub fn release_close_seeds(&mut self, scope: &CanonicalScope) -> StorageResult<()> {
        self.release_context(scope)?;
        let result = (|| {
            let state = self
                .release_context(scope)?
                .releasing
                .as_ref()
                .ok_or(StorageError::Integrity("release not begun"))?;
            if state.snapshot.stage != 1
                || Some(state.fold.ledger.seal()?).as_ref() != state.snapshot.seeds.as_ref()
            {
                return Err(StorageError::Integrity("release complete seed transcript"));
            }
            let mut a = ReleaseAttempt::new(state, "close_seeds", 0)?;
            a.after.stage = 2;
            self.release_change(scope, a)
        })();
        self.finish(result)
    }
    /// Mark only this selected Release owner failed without cleanup or refund.
    /// Foreign scope refusal leaves the actual current owner untouched.
    pub fn release_abandon(&mut self, scope: &CanonicalScope) -> StorageResult<()> {
        self.release_context(scope)?;
        self.resource
            .as_ref()
            .unwrap()
            .releasing
            .as_ref()
            .ok_or(StorageError::Integrity("release not begun"))?
            .failed
            .set(true);
        self.finish(Ok(()))
    }
}
