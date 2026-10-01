//! Exact immutable count/zero folds, epoch advance and bounded retirement.
use super::{
    count_index, count_read,
    count_state::{CountAttempt, RetireOwner},
    ScratchSession,
};
use crate::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{
    BaseFact, CanonicalScope, CountEpoch, CountLedger, CountRecord, CountSeal, GraphMode,
    ZeroLedger, ZeroSeal,
};
impl ScratchSession {
    /// Independently fold the selected complete ordered Effects or Final epoch.
    /// Freeze only after exact population, touched count and maximum agree; an
    /// uncertain SQL/native acknowledgement retains the proposed seal and owner.
    pub fn count_seal(
        &mut self,
        scope: &CanonicalScope,
        epoch: CountEpoch,
    ) -> StorageResult<CountSeal> {
        self.count_context(scope)?;
        let result = (|| {
            let r = self.count_context(scope)?;
            let state = r.counts.as_ref().unwrap();
            let allowed = match epoch {
                CountEpoch::Effects => {
                    state.snapshot.stage == 1
                        && scope.subject().selected().mode() == GraphMode::Update
                }
                CountEpoch::Final => {
                    state.snapshot.stage == 3
                        || (state.snapshot.stage == 1
                            && scope.subject().selected().mode() == GraphMode::Fresh)
                }
            };
            if !allowed {
                return Err(StorageError::Integrity("count seal selected epoch"));
            }
            let _fold = state.memory.reserve(
                std::mem::size_of::<CountLedger>() + 128 * std::mem::size_of::<CountRecord>(),
            )?;
            let mut ledger = CountLedger::new(scope.clone(), epoch)?;
            let mut after = None;
            loop {
                let (rows, last, eof) = count_read::read(
                    r.verify()?,
                    "canonical_counts",
                    scope,
                    after,
                    state.snapshot.maximum,
                    128,
                    |key, value| Ok(CountRecord::decode(scope, key, value)?),
                )?;
                ledger.append(&rows)?;
                after = last;
                if eof {
                    break;
                }
            }
            let seal = ledger.seal()?;
            if seal.records != state.snapshot.records
                || seal.touched != state.snapshot.touched
                || seal.maximum != state.snapshot.maximum
            {
                return Err(StorageError::Integrity("count exact folded totals"));
            }
            let mut a = CountAttempt::new(state, "seal", 0, 0)?;
            match epoch {
                CountEpoch::Effects => {
                    a.after.stage = 2;
                    a.after.effects = Some(seal.clone());
                }
                CountEpoch::Final => {
                    a.after.stage = 4;
                    a.after.final_seal = Some(seal.clone());
                }
            }
            self.count_change(scope, a)?;
            Ok(seal)
        })();
        self.finish(result)
    }
    /// Append at most128 strictly ordered zero candidates from this Effects seal.
    /// Each full carried base answer must match the bound fact authority and the
    /// selected zero effect/parent eligibility before any dependent SQL insert.
    pub fn zero_append(&mut self, counts: &CountSeal, records: &[BaseFact]) -> StorageResult<()> {
        self.count_context(&counts.scope)?;
        let result = (|| {
            let r = self.count_context(&counts.scope)?;
            let state = r.counts.as_ref().unwrap();
            if records.len() > 128
                || state.snapshot.stage != 2
                || state.snapshot.effects.as_ref() != Some(counts)
                || state.snapshot.seeds.is_some()
            {
                return Err(StorageError::Integrity("zero append selected epoch"));
            }
            let mut a = CountAttempt::new(state, "zero_insert", 0, records.len())?;
            let zeros = counts.scope.zeros()?;
            for fact in records {
                let key = zeros.key(fact.serial)?;
                BaseFact::decode(fact.serial, &fact.encode_value()?)?;
                if a.after.zero_maximum.is_some_and(|old| old >= fact.serial)
                    || fact.serial == counts.scope.subject().selected().root_serial()
                    || count_index::get(r.verify()?, &counts.scope, fact.serial)?.is_none()
                {
                    return Err(StorageError::Integrity("zero ordered candidate source"));
                }
                let facts = r.facts.as_ref().unwrap();
                let base_scope = facts.base.scope.as_ref().unwrap();
                if super::fact_index::base(r.verify()?, base_scope, fact.serial)? != Some(*fact) {
                    return Err(StorageError::Integrity("zero established base source"));
                }
                let record = count_index::get(r.verify()?, &counts.scope, fact.serial)?.unwrap();
                let zero = match record.row {
                    layerfs_content::filesystem::references::record::Row::Count {
                        count, ..
                    } => count == 0,
                    layerfs_content::filesystem::references::record::Row::Effect {
                        delta, ..
                    } => {
                        i128::from(fact.value.map_or(0, |v| v.namespace_ref_count))
                            + i128::from(delta)
                            <= 0
                    }
                };
                if !record.touched
                    || !zero
                    || super::fact_index::parent(
                        r.verify()?,
                        facts.parent.scope.as_ref().unwrap(),
                        fact.serial,
                    )?
                    .is_some_and(|p| !p.bound)
                {
                    return Err(StorageError::Integrity("zero eligibility/effect source"));
                }
                a.seeds.push((None, Some(*fact)));
                a.after.zeros = a
                    .after
                    .zeros
                    .checked_add(1)
                    .ok_or(StorageError::Integrity("zero records overflow"))?;
                a.after.zero_maximum = Some(zeros.scalar(&key)?);
            }
            self.count_change(&counts.scope, a)
        })();
        self.finish(result)
    }
    /// Independently fold and freeze the complete ordered carried-zero population.
    /// Retain the complete proposed seal if SQL/native acknowledgement is uncertain.
    pub fn zero_seal(&mut self, counts: &CountSeal) -> StorageResult<ZeroSeal> {
        self.count_context(&counts.scope)?;
        let result = (|| {
            let r = self.count_context(&counts.scope)?;
            let state = r.counts.as_ref().unwrap();
            if state.snapshot.stage != 2
                || state.snapshot.effects.as_ref() != Some(counts)
                || state.snapshot.seeds.is_some()
            {
                return Err(StorageError::Integrity("zero seal selected epoch"));
            }
            let _fold = state.memory.reserve(
                std::mem::size_of::<ZeroLedger>() + 128 * std::mem::size_of::<BaseFact>(),
            )?;
            let zeros = counts.scope.zeros()?;
            let mut ledger = ZeroLedger::new(counts.clone())?;
            let mut after = None;
            loop {
                let (rows, last, eof) = count_read::read(
                    r.verify()?,
                    "zero_seeds",
                    &zeros,
                    after,
                    state.snapshot.zero_maximum,
                    128,
                    |key, value| Ok(BaseFact::decode(zeros.scalar(key)?, value)?),
                )?;
                ledger.append(&rows)?;
                after = last;
                if eof {
                    break;
                }
            }
            let seal = ledger.seal()?;
            if seal.records != state.snapshot.zeros || seal.maximum != state.snapshot.zero_maximum {
                return Err(StorageError::Integrity("zero exact folded totals"));
            }
            let mut a = CountAttempt::new(state, "zero_seal", 0, 0)?;
            a.after.seeds = Some(seal.clone());
            self.count_change(&counts.scope, a)?;
            Ok(seal)
        })();
        self.finish(result)
    }
    /// Enable descendant effects only for the exact known complete zero seal.
    /// The original Effects epoch and its candidate transcript remain captured.
    pub fn count_resume(&mut self, seeds: &ZeroSeal) -> StorageResult<()> {
        self.count_context(&seeds.counts.scope)?;
        let result = (|| {
            let state = self
                .count_context(&seeds.counts.scope)?
                .counts
                .as_ref()
                .unwrap();
            if state.snapshot.stage != 2
                || state.snapshot.seeds.as_ref() != Some(seeds)
                || state.snapshot.effects.as_ref() != Some(&seeds.counts)
            {
                return Err(StorageError::Integrity("count resume seed epoch"));
            }
            let mut a = CountAttempt::new(state, "resume", 0, 0)?;
            a.after.stage = 3;
            self.count_change(&seeds.counts.scope, a)
        })();
        self.finish(result)
    }
    /// Retire exact Final Counts and optional ZeroSeeds after their consumers end.
    /// Replay complete transcripts in combined windows of at most128 rows, after
    /// BaseFacts retirement and any Release retirement. Unknown retains the
    /// selected rows, continuation and credit; this does not close/unlink native S.
    pub fn count_retire(
        &mut self,
        seal: &CountSeal,
        seeds: Option<&ZeroSeal>,
    ) -> StorageResult<()> {
        self.count_context(&seal.scope)?;
        let selected = (|| {
            let r = self.count_context(&seal.scope)?;
            let state = r.counts.as_ref().unwrap();
            if seal.epoch != CountEpoch::Final
                || state.snapshot.final_seal.as_ref() != Some(seal)
                || state.snapshot.seeds.as_ref() != seeds
                || !matches!(state.snapshot.stage, 4..=6)
                || r.facts.as_ref().unwrap().base.stage != 4
                || r.releasing.as_ref().is_some_and(|release| {
                    release.snapshot.stage != 5 || release.attempt.is_some() || release.failed.get()
                })
            {
                return Err(StorageError::Integrity(
                    "count retirement terminal consumers",
                ));
            }
            Ok(())
        })();
        self.finish(selected)?;
        if self
            .resource
            .as_ref()
            .unwrap()
            .counts
            .as_ref()
            .unwrap()
            .retirement
            .is_none()
        {
            let r = self.resource.as_mut().unwrap();
            let state = r.counts.as_mut().unwrap();
            state.retirement = Some(RetireOwner::new(&state.memory, seal, seeds)?);
        }
        loop {
            let state = self.count_context(&seal.scope)?.counts.as_ref().unwrap();
            if state.snapshot.stage == 6 {
                return Ok(());
            }
            let remaining_counts = if state.snapshot.stage == 4 {
                state.snapshot.records
            } else {
                state.snapshot.count_remaining
            };
            let remaining_zeros = if state.snapshot.stage == 4 {
                state.snapshot.zeros
            } else {
                state.snapshot.zero_remaining
            };
            let rows = remaining_counts.min(128) as usize;
            let zeros = remaining_zeros.min((128 - rows) as u64) as usize;
            let result = (|| {
                let r = self.count_context(&seal.scope)?;
                let state = r.counts.as_ref().unwrap();
                let mut a = CountAttempt::new(state, "retire", rows, zeros)?;
                let connection = r.verify()?;
                let mut proof = state.retirement.as_ref().unwrap().proposed(&state.memory)?;
                if rows > 0 {
                    let (n, last, _) = count_read::visit(
                        connection,
                        "canonical_counts",
                        &seal.scope,
                        state.snapshot.after_count,
                        state.snapshot.maximum,
                        rows,
                        |key, value| Ok(CountRecord::decode(&seal.scope, key, value)?),
                        |row| {
                            proof.value.counts.append(std::slice::from_ref(&row))?;
                            a.deleted_rows.push(row);
                            Ok(())
                        },
                    )?;
                    if n != rows {
                        return Err(StorageError::Integrity("count retirement row count"));
                    }
                    a.after.after_count = last;
                }
                if zeros > 0 {
                    let selected = seal.scope.zeros()?;
                    let (n, last, _) = count_read::visit(
                        connection,
                        "zero_seeds",
                        &selected,
                        state.snapshot.after_zero,
                        state.snapshot.zero_maximum,
                        zeros,
                        |key, value| Ok(BaseFact::decode(selected.scalar(key)?, value)?),
                        |row| {
                            proof
                                .value
                                .zeros
                                .as_mut()
                                .ok_or(StorageError::Integrity("zero retirement ledger"))?
                                .append(std::slice::from_ref(&row))?;
                            a.deleted_seeds.push(row);
                            Ok(())
                        },
                    )?;
                    if n != zeros {
                        return Err(StorageError::Integrity("zero retirement row count"));
                    }
                    a.after.after_zero = last;
                }
                a.after.count_remaining = remaining_counts - rows as u64;
                a.after.zero_remaining = remaining_zeros - zeros as u64;
                a.after.stage = if a.after.count_remaining == 0 && a.after.zero_remaining == 0 {
                    6
                } else {
                    5
                };
                if a.after.stage == 6
                    && (a.after.after_count != seal.maximum
                        || a.after.after_zero != state.snapshot.zero_maximum)
                {
                    return Err(StorageError::Integrity(
                        "count retirement final continuation",
                    ));
                }
                if a.after.stage == 6
                    && (proof.value.counts.seal()? != *seal
                        || proof
                            .value
                            .zeros
                            .as_ref()
                            .map(|l| l.seal())
                            .transpose()?
                            .as_ref()
                            != seeds)
                {
                    return Err(StorageError::Integrity("count retirement exact transcript"));
                }
                a.retirement = Some(proof);
                self.count_change(&seal.scope, a)
            })();
            self.finish(result)?;
        }
    }
}
