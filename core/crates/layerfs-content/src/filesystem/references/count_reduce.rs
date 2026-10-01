//! Nonborrowing supplied count coordinator; all growing membership is external.
use crate::filesystem::inode::read::{lookup_many_owned, InodeReadWork, InodeTable};
use crate::filesystem::references::record::Row;
use crate::filesystem::references::reduce::{FinalChange, PendingState, ReferenceWork};
use crate::filesystem::state::{
    BaseFact, CanonicalScope, CountEpoch, CountLedger, CountPage, CountRecord, CountSeal,
    CountState, EligibilityAuthority, FactScope, FactState, GraphMemoryLease, ParentCalls,
    ZeroLedger, ZeroSeal,
};
use crate::object::inode_leaf::InodeValue;
use crate::object::{AuthenticatedObjects, CanonicalBudget};
use crate::{ContentError, ContentResult};
const COUNT_WAVE: usize = 64;
/// Fixed supplied coordinator; growing declared/touched/count/zero membership stays native.
pub(crate) struct CountReducer {
    value: Option<Box<CountReducerData>>,
    _memory: Option<GraphMemoryLease>,
}
pub(crate) struct CountReducerData {
    pub(crate) scope: CanonicalScope,
    pub(crate) work: ReferenceWork,
    pub(crate) zeros: Option<ZeroSeal>,
    pub(crate) final_seal: Option<CountSeal>,
    canonical: CanonicalBudget,
}
impl std::ops::Deref for CountReducer {
    type Target = CountReducerData;
    fn deref(&self) -> &Self::Target {
        self.value.as_ref().unwrap()
    }
}
impl std::ops::DerefMut for CountReducer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value.as_mut().unwrap()
    }
}
impl CountReducer {
    pub(crate) fn new<S: CountState + ?Sized>(
        state: &mut S,
        scope: CanonicalScope,
        extra: usize,
    ) -> ContentResult<Self> {
        let lease = state
            .count_memory(&scope)?
            .reserve(canonical_count_control_working_bytes() + extra)?;
        state.count_begin(&scope)?;
        Ok(Self {
            value: Some(Box::new(CountReducerData {
                scope,
                work: ReferenceWork::default(),
                zeros: None,
                final_seal: None,
                canonical: CanonicalBudget::compatibility(),
            })),
            _memory: Some(lease),
        })
    }
    fn put<S: CountState + ?Sized>(
        &mut self,
        state: &mut S,
        before: Option<CountRecord>,
        after: CountRecord,
    ) -> ContentResult<()> {
        let ack = state.count_cas(&self.scope, before, &after)?;
        if ack != after {
            return Err(ContentError::InvalidOrderingRecord(
                "count acknowledged proposal",
            ));
        }
        if after.touched && before.is_none_or(|r| !r.touched) {
            self.work.rows_touched = self.work.rows_touched.saturating_add(1);
        }
        self.work.peak_pending = self.work.peak_pending.max(1);
        Ok(())
    }
    pub(crate) fn canonical(&self) -> CanonicalBudget {
        self.canonical.clone()
    }
    pub(crate) fn base_values<S: CountState + FactState + ?Sized>(
        &mut self,
        state: &mut S,
        reader: &dyn AuthenticatedObjects,
        table: InodeTable,
        serials: &[u64],
    ) -> ContentResult<BaseAnswers> {
        let memory = state.count_memory(&self.scope)?;
        let result_credit = memory.reserve(
            std::mem::size_of::<BaseAnswers>()
                + COUNT_WAVE * std::mem::size_of::<Option<InodeValue>>(),
        )?;
        let _working = memory.reserve(canonical_base_working_bytes())?;
        let wave = known_base_wave(state, &self.scope, reader, table, serials, &self.canonical)?;
        self.work.base_records_read = self.work.base_records_read.saturating_add(wave.reads);
        self.work.base_waves = self.work.base_waves.saturating_add(wave.calls);
        Ok(BaseAnswers {
            values: wave.values,
            _memory: Some(result_credit),
        })
    }
    pub(crate) fn declare_new<S: CountState + ?Sized>(
        &mut self,
        state: &mut S,
        serial: u64,
    ) -> ContentResult<()> {
        if serial == 0 {
            return Err(ContentError::InvalidRecord("inode serial"));
        }
        let before = state.count_get(&self.scope, serial)?;
        if before.is_some() {
            return Err(ContentError::InvalidRecord("duplicate new inode"));
        }
        self.put(
            state,
            None,
            CountRecord {
                row: Row::Count {
                    serial,
                    value: None,
                    count: 0,
                },
                touched: false,
            },
        )
    }
    fn entry<S: CountState + ?Sized>(
        &self,
        state: &mut S,
        serial: u64,
    ) -> ContentResult<(Option<CountRecord>, CountRecord)> {
        let before = state.count_get(&self.scope, serial)?;
        let row = before.unwrap_or(CountRecord {
            row: Row::Effect {
                serial,
                value: None,
                delta: 0,
            },
            touched: false,
        });
        Ok((before, row))
    }
    pub(crate) fn retained<S: CountState + ?Sized>(
        &mut self,
        state: &mut S,
        serial: u64,
    ) -> ContentResult<()> {
        let (before, mut after) = self.entry(state, serial)?;
        if self.scope.subject().table().is_none() && !matches!(after.row, Row::Count { .. }) {
            return Err(ContentError::InvalidRecord("effect inode record"));
        }
        match &mut after.row {
            Row::Count { count, .. } => {
                *count = count.checked_add(1).ok_or(ContentError::LengthOverflow)?
            }
            Row::Effect { delta, .. } => {
                *delta = delta.checked_add(1).ok_or(ContentError::LengthOverflow)?
            }
        }
        after.touched = true;
        self.put(state, before, after)
    }
    pub(crate) fn removed<S: CountState + ?Sized>(
        &mut self,
        state: &mut S,
        serial: u64,
    ) -> ContentResult<()> {
        let (before, mut after) = self.entry(state, serial)?;
        match &mut after.row {
            Row::Count { .. } => {
                return Err(ContentError::InvalidRecord("new inode loses a binding"))
            }
            Row::Effect { delta, .. } => {
                *delta = delta.checked_sub(1).ok_or(ContentError::LengthOverflow)?
            }
        }
        after.touched = true;
        self.put(state, before, after)
    }
    pub(crate) fn value<S: CountState + ?Sized>(
        &mut self,
        state: &mut S,
        serial: u64,
        value: InodeValue,
    ) -> ContentResult<()> {
        let (before, mut after) = self.entry(state, serial)?;
        if self.scope.subject().table().is_none() && !matches!(after.row, Row::Count { .. }) {
            return Err(ContentError::InvalidRecord("effect inode record"));
        }
        match &mut after.row {
            Row::Count { value: old, .. } | Row::Effect { value: old, .. } => *old = Some(value),
        }
        after.touched = true;
        self.put(state, before, after)
    }
    pub(crate) fn state<S: CountState + ?Sized>(
        &self,
        state: &mut S,
        serial: u64,
    ) -> ContentResult<Option<PendingState>> {
        Ok(state.count_get(&self.scope, serial)?.map(|r| match r.row {
            Row::Count { value, count, .. } => PendingState::New { value, count },
            Row::Effect { value, delta, .. } => PendingState::Existing { value, delta },
        }))
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn collect_zero<S: CountState + FactState + ?Sized>(
        &mut self,
        state: &mut S,
        reader: &dyn AuthenticatedObjects,
        table: InodeTable,
        batch: usize,
        root: u64,
        unreachable: &EligibilityAuthority,
        parents: ParentCalls<S>,
        maximum: usize,
    ) -> ContentResult<(ZeroSeal, u64)> {
        let seal = state.count_seal(&self.scope, CountEpoch::Effects)?;
        if seal.scope != self.scope || seal.epoch != CountEpoch::Effects {
            return Err(ContentError::InvalidOrderingRecord(
                "effects count selected epoch",
            ));
        }
        if seal.touched > maximum as u64 {
            return Err(ContentError::ObjectLimitExceeded {
                limit: maximum,
                actual: usize::try_from(seal.touched).unwrap_or(usize::MAX),
            });
        }
        let memory = state.count_memory(&self.scope)?;
        let _control = memory.reserve(canonical_zero_working_bytes())?;
        let mut ledger = CountLedger::new(self.scope.clone(), CountEpoch::Effects)?;
        let mut seeds = ZeroLedger::new(seal.clone())?;
        let mut after = None;
        let mut reads = 0u64;
        loop {
            let page = state.count_page(&seal, after, batch.clamp(1, COUNT_WAVE), 65536)?;
            accept_count(&mut ledger, &seal, after, &page)?;
            let mut serials = [0; COUNT_WAVE];
            let mut used = 0;
            for row in page.records() {
                if row.touched {
                    serials[used] = row.serial();
                    used += 1;
                }
            }
            let wave = known_base_wave(
                state,
                &self.scope,
                reader,
                table,
                &serials[..used],
                &self.canonical,
            )?;
            reads = reads.saturating_add(wave.reads);
            let mut bases = wave.values.into_iter();
            let mut records = [BaseFact {
                serial: 0,
                value: None,
            }; COUNT_WAVE];
            let mut count = 0;
            for row in page.records() {
                if !row.touched {
                    continue;
                }
                let base = bases.next().flatten();
                let final_count = match row.row {
                    Row::Count { count, .. } => count,
                    Row::Effect { delta, .. } => u64::try_from(
                        (base.map_or(0, |v| i128::from(v.namespace_ref_count)) + i128::from(delta))
                            .max(0),
                    )
                    .unwrap_or(0),
                };
                if final_count == 0
                    && row.serial() != root
                    && !unreachable.contains(state, row.serial(), parents)?
                {
                    records[count] = BaseFact {
                        serial: row.serial(),
                        value: base,
                    };
                    count += 1;
                }
            }
            seeds.append(&records[..count])?;
            state.zero_append(&seal, &records[..count])?;
            after = page.last;
            if page.eof {
                break;
            }
        }
        let expected = seeds.seal()?;
        let actual = state.zero_seal(&seal)?;
        if actual != expected {
            return Err(ContentError::InvalidOrderingRecord(
                "zero acknowledged transcript",
            ));
        }
        verify_zeros(state, &actual)?;
        state.count_resume(&actual)?;
        self.work.serials_scanned = self.work.serials_scanned.saturating_add(seal.touched);
        self.zeros = Some(actual.clone());
        Ok((actual, reads))
    }
    pub(crate) fn finish<'a, S: CountState + ?Sized>(
        &mut self,
        state: &mut S,
        reader: &'a dyn AuthenticatedObjects,
        table: InodeTable,
        batch: usize,
        root: u64,
    ) -> ContentResult<CountFinalRows<'a>> {
        let seal = state.count_seal(&self.scope, CountEpoch::Final)?;
        if seal.scope != self.scope || seal.epoch != CountEpoch::Final {
            return Err(ContentError::InvalidOrderingRecord(
                "final count selected epoch",
            ));
        }
        self.final_seal = Some(seal.clone());
        CountFinalRows::new(
            state,
            seal,
            reader,
            table,
            batch,
            root,
            self.work,
            self.canonical.clone(),
        )
    }
    pub(crate) fn retire<S: CountState + FactState + ?Sized>(
        &mut self,
        state: &mut S,
    ) -> ContentResult<()> {
        let seal = self
            .final_seal
            .as_ref()
            .ok_or(ContentError::InvalidOrderingRecord(
                "count final retirement",
            ))?;
        let base = FactScope::new(
            self.scope.state().selection().clone(),
            self.scope.subject().clone(),
        )?;
        crate::filesystem::validate::retire_canonical_facts(state, &base)?;
        state.count_retire(seal, self.zeros.as_ref())?;
        // End every C1 coordinator allocation before Roots' terminal pool gate.
        self.value = None;
        self._memory = None;
        Ok(())
    }
}
fn accept_count(
    ledger: &mut CountLedger,
    seal: &CountSeal,
    after: Option<u64>,
    page: &CountPage,
) -> ContentResult<()> {
    if page.seal != *seal || page.records().is_empty() && !page.eof {
        return Err(ContentError::InvalidOrderingRecord(
            "count page selected epoch",
        ));
    }
    ledger.append(page.records())?;
    if page.last != ledger.last().or(after)
        || page.eof != (ledger.records() == seal.records)
        || ledger.records() > seal.records
    {
        return Err(ContentError::InvalidOrderingRecord(
            "count page EOF/progress",
        ));
    }
    if page.eof && ledger.seal() != Ok(seal.clone()) {
        return Err(ContentError::InvalidOrderingRecord("count page transcript"));
    }
    Ok(())
}
fn verify_zeros<S: CountState + ?Sized>(state: &mut S, seal: &ZeroSeal) -> ContentResult<()> {
    let _fold = state
        .count_memory(&seal.counts.scope)?
        .reserve(std::mem::size_of::<ZeroLedger>())?;
    let mut ledger = ZeroLedger::new(seal.counts.clone())?;
    let mut after = None;
    loop {
        let page = state.zero_page(seal, after, 128, 65536)?;
        if page.seal != *seal {
            return Err(ContentError::InvalidOrderingRecord("zero selected page"));
        }
        ledger.append(page.records())?;
        if page.last != ledger.last().or(after)
            || page.eof != (ledger.records() == seal.records)
            || ledger.records() > seal.records
        {
            return Err(ContentError::InvalidOrderingRecord(
                "zero page EOF/progress",
            ));
        }
        after = page.last;
        if page.eof {
            break;
        }
    }
    if ledger.seal() != Ok(seal.clone()) {
        return Err(ContentError::InvalidOrderingRecord(
            "zero verified transcript",
        ));
    }
    Ok(())
}
/// Fixed final cursor; one leased current page and one bounded derived wave.
pub(crate) struct CountFinalRows<'a> {
    seal: CountSeal,
    ledger: CountLedger,
    page: Option<CountPage>,
    index: usize,
    after: Option<u64>,
    finished: bool,
    reader: &'a dyn AuthenticatedObjects,
    table: InodeTable,
    batch: usize,
    root: u64,
    bases: Vec<Option<InodeValue>>,
    base_index: usize,
    canonical: CanonicalBudget,
    work: ReferenceWork,
    _memory: GraphMemoryLease,
}
impl<'a> CountFinalRows<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new<S: CountState + ?Sized>(
        state: &mut S,
        seal: CountSeal,
        reader: &'a dyn AuthenticatedObjects,
        table: InodeTable,
        batch: usize,
        root: u64,
        work: ReferenceWork,
        canonical: CanonicalBudget,
    ) -> ContentResult<Self> {
        let batch = batch.clamp(1, COUNT_WAVE);
        let lease = state
            .count_memory(&seal.scope)?
            .reserve(canonical_final_working_bytes())?;
        Ok(Self {
            ledger: CountLedger::new(seal.scope.clone(), CountEpoch::Final)?,
            seal,
            page: None,
            index: 0,
            after: None,
            finished: false,
            reader,
            table,
            batch,
            root,
            bases: Vec::new(),
            base_index: 0,
            canonical,
            work,
            _memory: lease,
        })
    }
    pub(crate) fn work(&self) -> ReferenceWork {
        self.work
    }
    pub(crate) fn next_change<S: CountState + FactState + ?Sized>(
        &mut self,
        state: &mut S,
    ) -> ContentResult<Option<FinalChange>> {
        if self.finished {
            return Ok(None);
        }
        if self
            .page
            .as_ref()
            .is_none_or(|p| self.index == p.records().len())
        {
            if self.page.as_ref().is_some_and(|p| p.eof) {
                self.page = None;
                self.bases = Vec::new();
                self.finished = true;
                return Ok(None);
            }
            self.page = None;
            self.bases = Vec::new();
            let page = state.count_page(&self.seal, self.after, self.batch, 65536)?;
            accept_count(&mut self.ledger, &self.seal, self.after, &page)?;
            self.after = page.last;
            let mut serials = [0; COUNT_WAVE];
            let mut used = 0;
            for row in page.records() {
                if matches!(row.row, Row::Effect { .. }) {
                    serials[used] = row.serial();
                    used += 1;
                }
            }
            let wave = known_base_wave(
                state,
                &self.seal.scope,
                self.reader,
                self.table,
                &serials[..used],
                &self.canonical,
            )?;
            self.bases = wave.values;
            self.work.base_records_read = self.work.base_records_read.saturating_add(wave.reads);
            self.work.base_waves = self.work.base_waves.saturating_add(wave.calls);
            self.index = 0;
            self.base_index = 0;
            self.page = Some(page);
        }
        let Some(row) = self
            .page
            .as_ref()
            .and_then(|p| p.records().get(self.index))
            .copied()
        else {
            self.page = None;
            self.bases = Vec::new();
            self.finished = true;
            return Ok(None);
        };
        self.index += 1;
        let (serial, value) = match row.row {
            Row::Count {
                serial,
                value,
                count,
            } => {
                if count == 0 && serial != self.root {
                    return Err(ContentError::InvalidRecord("new inode without binding"));
                }
                let value = value.ok_or(ContentError::InvalidRecord("new inode value"))?;
                (
                    serial,
                    Some(InodeValue {
                        namespace_ref_count: count,
                        ..value
                    }),
                )
            }
            Row::Effect {
                serial,
                value,
                delta,
            } => {
                let base = self
                    .bases
                    .get(self.base_index)
                    .copied()
                    .flatten()
                    .ok_or(ContentError::InvalidRecord("effect inode record"))?;
                self.base_index += 1;
                let merged = InodeValue {
                    kind: value.map_or(base.kind, |v| v.kind),
                    namespace_ref_count: base.namespace_ref_count,
                    content_root: value.map_or(base.content_root, |v| v.content_root),
                    metadata_root: value.map_or(base.metadata_root, |v| v.metadata_root),
                };
                let count = i128::from(base.namespace_ref_count) + i128::from(delta);
                let final_value = if count <= 0 && serial == self.root {
                    Some(InodeValue {
                        namespace_ref_count: 0,
                        ..merged
                    })
                } else if count <= 0 {
                    None
                } else {
                    Some(InodeValue {
                        namespace_ref_count: count as u64,
                        ..merged
                    })
                };
                (serial, final_value)
            }
        };
        if value.is_some() {
            self.work.final_values = self.work.final_values.saturating_add(1);
        } else {
            self.work.final_removals = self.work.final_removals.saturating_add(1);
        }
        Ok(Some(FinalChange { serial, value }))
    }
}

/// One admitted answer vector with truthful new canonical acquisitions.
pub(crate) struct BaseWave {
    pub(crate) values: Vec<Option<InodeValue>>,
    pub(crate) reads: u64,
    pub(crate) calls: u64,
}
pub(crate) fn known_base_wave<S: FactState + ?Sized>(
    state: &mut S,
    scope: &CanonicalScope,
    reader: &dyn AuthenticatedObjects,
    table: InodeTable,
    serials: &[u64],
    canonical: &CanonicalBudget,
) -> ContentResult<BaseWave> {
    if serials.len() > COUNT_WAVE {
        return Err(ContentError::InvalidOrderingRecord("canonical base wave"));
    }
    if serials.is_empty() {
        return Ok(BaseWave {
            values: Vec::new(),
            reads: 0,
            calls: 0,
        });
    }
    if scope.subject().table() != Some(table) {
        return Err(ContentError::InvalidOrderingRecord(
            "canonical actual base table",
        ));
    }
    let base = FactScope::new(scope.state().selection().clone(), scope.subject().clone())?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(serials.len())
        .map_err(|_| ContentError::ResourceUnavailable {
            what: "canonical.base_answers",
        })?;
    if values.capacity() > COUNT_WAVE {
        return Err(ContentError::InvalidOrderingRecord(
            "canonical answer capacity",
        ));
    }
    values.resize(serials.len(), None);
    let mut missing = [0; COUNT_WAVE];
    let mut positions = [0; COUNT_WAVE];
    let mut used = 0;
    for (i, serial) in serials.iter().copied().enumerate() {
        match state.fact_get(&base, serial)? {
            Some(fact) => {
                if fact.serial != serial {
                    return Err(ContentError::InvalidOrderingRecord(
                        "canonical known base serial",
                    ));
                }
                BaseFact::decode(serial, &fact.encode_value()?)?;
                values[i] = fact.value;
            }
            None => {
                missing[used] = serial;
                positions[used] = i;
                used += 1;
            }
        }
    }
    let found = lookup_many_owned(
        reader,
        table,
        &missing[..used],
        &mut InodeReadWork::default(),
        canonical,
    )?;
    if found.len() != used {
        return Err(ContentError::InvalidOrderingRecord(
            "canonical base answer count",
        ));
    }
    // Point misses can contain repeated demand occurrences. Store every new
    // immutable serial once, while preserving every answer's original position.
    let mut inserted = [BaseFact {
        serial: 0,
        value: None,
    }; COUNT_WAVE];
    let mut count = 0;
    for (i, value) in found.into_iter().enumerate() {
        values[positions[i]] = value;
        if !inserted[..count].iter().any(|r| r.serial == missing[i]) {
            inserted[count] = BaseFact {
                serial: missing[i],
                value,
            };
            count += 1;
        }
    }
    if count != 0 {
        state.fact_insert(&base, &inserted[..count])?;
    }
    Ok(BaseWave {
        values,
        reads: used as u64,
        calls: u64::from(used != 0),
    })
}

/// Fixed supplied CountReducer control and its compatibility canonical-budget control.
pub const fn canonical_count_control_working_bytes() -> usize {
    std::mem::size_of::<CountReducer>()
        + std::mem::size_of::<CountReducerData>()
        + 4 * std::mem::size_of::<usize>()
}
/// Exact maximum zero-scan control/arrays and both simultaneous answer vectors.
/// The inserted-fact wave ends before the returned zero-candidate array begins;
/// that latter array plus one answer vector is narrower than the read wave.
pub const fn canonical_zero_working_bytes() -> usize {
    std::mem::size_of::<CountLedger>()
        + std::mem::size_of::<ZeroLedger>()
        + COUNT_WAVE
            * (std::mem::size_of::<BaseFact>()
                + 2 * std::mem::size_of::<u64>()
                + std::mem::size_of::<usize>()
                + 2 * std::mem::size_of::<Option<InodeValue>>())
}
/// Exact fixed final cursor/control/windows; the returned CountPage retains a separate lease.
pub const fn canonical_final_working_bytes() -> usize {
    std::mem::size_of::<CountFinalRows<'static>>()
        + COUNT_WAVE
            * (std::mem::size_of::<BaseFact>()
                + 2 * std::mem::size_of::<u64>()
                + std::mem::size_of::<usize>()
                + 2 * std::mem::size_of::<Option<InodeValue>>())
}

/// Bounded base answers; the optional supplied credit cannot detach from its Vec.
pub(crate) struct BaseAnswers {
    values: Vec<Option<InodeValue>>,
    _memory: Option<GraphMemoryLease>,
}
impl BaseAnswers {
    pub(crate) fn compatibility(values: Vec<Option<InodeValue>>) -> Self {
        Self {
            values,
            _memory: None,
        }
    }
}
impl std::ops::Deref for BaseAnswers {
    type Target = [Option<InodeValue>];
    fn deref(&self) -> &Self::Target {
        &self.values
    }
}
/// Exact fixed temporary known/unknown base wave, separate from the held answer Vec.
pub const fn canonical_base_working_bytes() -> usize {
    std::mem::size_of::<FactScope>()
        + std::mem::size_of::<BaseWave>()
        + COUNT_WAVE
            * (std::mem::size_of::<u64>()
                + std::mem::size_of::<usize>()
                + std::mem::size_of::<BaseFact>()
                + std::mem::size_of::<Option<InodeValue>>())
}
