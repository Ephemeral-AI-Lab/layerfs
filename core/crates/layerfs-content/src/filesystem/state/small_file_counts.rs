//! Fixed Count/Zero expected-proposed authority and exact immutable snapshots.
use super::empty_owner::bad;
use super::small_file_owner::SmallStage;
use super::*;
use crate::filesystem::references::Row;
use crate::ContentResult;
impl VerifiedSmallFileState {
    fn selected_counts(&self, seal: &CountSeal) -> ContentResult<()> {
        self.counts_scope(&seal.scope)?;
        if match seal.epoch {
            CountEpoch::Effects => self.data.effects.as_ref(),
            CountEpoch::Final => self.data.final_seal.as_ref(),
        } != Some(seal)
        {
            return Err(bad("small file selected count epoch"));
        }
        Ok(())
    }
    pub(super) fn selected_zeros(&self, seal: &ZeroSeal) -> ContentResult<()> {
        self.counts_scope(&seal.counts.scope)?;
        if self.data.zero_seal.as_ref() != Some(seal) {
            return Err(bad("small file selected zero epoch"));
        }
        Ok(())
    }
}
impl CountState for VerifiedSmallFileState {
    fn count_capacity(&self, scope: &CanonicalScope) -> ContentResult<CanonicalCapacity> {
        self.selected(scope, StateTable::Counts, 6)?;
        Self::capacity_class()
    }
    fn count_memory(&self, scope: &CanonicalScope) -> ContentResult<GraphMemory> {
        self.selected(scope, StateTable::Counts, 6)?;
        Ok(self.inner.memory.clone())
    }
    fn count_begin(&mut self, scope: &CanonicalScope) -> ContentResult<()> {
        self.selected(scope, StateTable::Counts, 6)?;
        if self.data.stage != SmallStage::Unbegun
            || self.inner.data.graph_stage != GraphStage::Retired
            || !self.inner.data.parents_closed
            || self.inner.data.parent_seal.is_none()
            || self.data.fact_scope.as_ref().map(|s| s.subject()) != Some(scope.subject())
            || self.inner.data.facts_retired
        {
            return Err(bad("small file count begin phase"));
        }
        self.occupancy(0, 0, 0)?;
        self.data.counts = Some(scope.clone());
        self.data.stage = SmallStage::Mutable;
        Ok(())
    }
    fn count_get(
        &mut self,
        scope: &CanonicalScope,
        serial: u64,
    ) -> ContentResult<Option<CountRecord>> {
        self.counts_scope(scope)?;
        scope.key(serial)?;
        Ok(self.position(serial).and_then(|at| self.data.records[at]))
    }
    fn count_cas(
        &mut self,
        scope: &CanonicalScope,
        before: Option<CountRecord>,
        after: &CountRecord,
    ) -> ContentResult<CountRecord> {
        self.counts_scope(scope)?;
        if !matches!(self.data.stage, SmallStage::Mutable | SmallStage::Resumed) {
            return Err(bad("small file count immutable epoch"));
        }
        let at = self
            .position(after.serial())
            .ok_or(bad("small file count unselected growth"))?;
        if self.data.records[at] != before {
            return Err(bad("small file count expected value"));
        }
        let Row::Effect {
            serial,
            value: Some(value),
            delta: 0,
        } = after.row
        else {
            return Err(bad("small file forbidden count/namespace growth"));
        };
        if self.data.rows[at].unwrap() != (crate::filesystem::InodeUpdate { serial, value })
            || !after.touched
        {
            return Err(bad("small file unverified count value"));
        }
        CountRecord::decode(scope, &scope.key(serial)?, &after.encode_value()?)?;
        let counts = self.data.records.iter().flatten().count() + usize::from(before.is_none());
        self.occupancy(counts as u64, self.data.zero_len as u64, 0)?;
        self.data.records[at] = Some(*after);
        Ok(*after)
    }
    fn count_seal(
        &mut self,
        scope: &CanonicalScope,
        epoch: CountEpoch,
    ) -> ContentResult<CountSeal> {
        self.counts_scope(scope)?;
        if epoch == CountEpoch::Effects && self.data.stage != SmallStage::Mutable
            || epoch == CountEpoch::Final
                && (self.data.stage != SmallStage::Resumed || !self.data.release_retired)
        {
            return Err(bad("small file count seal phase/replay"));
        }
        let mut ledger = CountLedger::new(scope.clone(), epoch)?;
        for row in self.data.records.iter().flatten() {
            ledger.append(&[*row])?;
        }
        let seal = ledger.seal()?;
        match epoch {
            CountEpoch::Effects => {
                self.data.effects = Some(seal.clone());
                self.data.stage = SmallStage::Effects;
            }
            CountEpoch::Final => {
                self.data.final_seal = Some(seal.clone());
                self.data.stage = SmallStage::Final;
            }
        }
        Ok(seal)
    }
    fn count_page(
        &mut self,
        seal: &CountSeal,
        after: Option<u64>,
        records: usize,
        bytes: usize,
    ) -> ContentResult<CountPage> {
        self.selected_counts(seal)?;
        let capacity = Self::page_limit(
            after,
            seal.maximum,
            records,
            bytes,
            COUNT_PAGE_HEADER_BYTES,
            128,
        )?;
        let memory = self.inner.memory.reserve(
            std::mem::size_of::<CountPage>() + capacity * std::mem::size_of::<CountRecord>(),
        )?;
        let mut found = Vec::new();
        found.try_reserve_exact(capacity).map_err(|_| {
            crate::ContentError::ResourceUnavailable {
                what: "small_file.count_page",
            }
        })?;
        for row in self
            .data
            .records
            .iter()
            .flatten()
            .filter(|r| after.is_none_or(|s| r.serial() > s))
            .take(capacity)
        {
            found.push(*row);
        }
        let last = found.last().map(|r| r.serial()).or(after);
        let eof = last == seal.maximum;
        let page = CountPage::new(memory, seal.clone(), found, last, eof)?;
        if seal.epoch == CountEpoch::Final {
            self.data.final_eof |= eof;
        }
        Ok(page)
    }
    fn zero_append(&mut self, counts: &CountSeal, records: &[BaseFact]) -> ContentResult<()> {
        self.selected_counts(counts)?;
        if self.data.stage != SmallStage::Effects
            || self.data.zero_seal.is_some()
            || self.data.zero_len + records.len() > 8
        {
            return Err(bad("small file zero insertion phase/capacity"));
        }
        let mut last = self.data.zeros[..self.data.zero_len]
            .last()
            .copied()
            .flatten()
            .map(|f| f.serial);
        for row in records {
            if *row != self.base(row.serial)?
                || last.is_some_and(|s| s >= row.serial)
                || row.value.unwrap().namespace_ref_count != 0
            {
                return Err(bad("small file unverified zero seed"));
            }
            let at = self.position(row.serial).unwrap();
            if self.data.records[at].is_none() {
                return Err(bad("small file zero untouched serial"));
            }
            last = Some(row.serial);
        }
        self.occupancy(
            self.data.records.iter().flatten().count() as u64,
            (self.data.zero_len + records.len()) as u64,
            0,
        )?;
        for row in records {
            self.data.zeros[self.data.zero_len] = Some(*row);
            self.data.zero_len += 1;
        }
        Ok(())
    }
    fn zero_seal(&mut self, counts: &CountSeal) -> ContentResult<ZeroSeal> {
        self.selected_counts(counts)?;
        if self.data.stage != SmallStage::Effects || self.data.zero_seal.is_some() {
            return Err(bad("small file zero seal replay/phase"));
        }
        let mut ledger = ZeroLedger::new(counts.clone())?;
        for row in self.data.zeros[..self.data.zero_len].iter().flatten() {
            ledger.append(&[*row])?;
        }
        let seal = ledger.seal()?;
        self.data.zero_seal = Some(seal.clone());
        Ok(seal)
    }
    fn zero_page(
        &mut self,
        seal: &ZeroSeal,
        after: Option<u64>,
        records: usize,
        bytes: usize,
    ) -> ContentResult<ZeroPage> {
        self.selected_zeros(seal)?;
        let capacity = Self::page_limit(
            after,
            seal.maximum,
            records,
            bytes,
            ZERO_PAGE_HEADER_BYTES,
            105,
        )?;
        let memory = self.inner.memory.reserve(
            std::mem::size_of::<ZeroPage>() + capacity * std::mem::size_of::<BaseFact>(),
        )?;
        let mut found = Vec::new();
        found.try_reserve_exact(capacity).map_err(|_| {
            crate::ContentError::ResourceUnavailable {
                what: "small_file.zero_page",
            }
        })?;
        for row in self.data.zeros[..self.data.zero_len]
            .iter()
            .flatten()
            .filter(|r| after.is_none_or(|s| r.serial > s))
            .take(capacity)
        {
            found.push(*row);
        }
        let last = found.last().map(|r| r.serial).or(after);
        let eof = last == seal.maximum;
        let page = ZeroPage::new(memory, seal.clone(), found, last, eof)?;
        self.data.zero_eof |= eof;
        Ok(page)
    }
    fn count_resume(&mut self, seeds: &ZeroSeal) -> ContentResult<()> {
        self.selected_zeros(seeds)?;
        if self.data.stage != SmallStage::Effects || !self.data.zero_eof {
            return Err(bad("small file count resume before zero EOF/replay"));
        }
        self.data.stage = SmallStage::Resumed;
        Ok(())
    }
    fn count_retire(&mut self, seal: &CountSeal, seeds: Option<&ZeroSeal>) -> ContentResult<()> {
        self.selected_counts(seal)?;
        if self.data.stage != SmallStage::Final
            || !self.data.final_eof
            || !self.inner.data.facts_retired
            || !self.data.release_retired
            || seeds != self.data.zero_seal.as_ref()
        {
            return Err(bad("small file count retirement before final EOF"));
        }
        self.data.records = [None; 8];
        self.data.zeros = [None; 8];
        self.data.stage = SmallStage::Retired;
        Ok(())
    }
    fn count_abandon(&mut self, scope: &CanonicalScope) -> ContentResult<()> {
        self.selected(scope, StateTable::Counts, 6)?;
        self.abandon();
        Ok(())
    }
}
