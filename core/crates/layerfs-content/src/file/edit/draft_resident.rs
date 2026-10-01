//! Explicit bounded resident compatibility authority, sharing the supplied algorithm.
use super::draft_port::{DraftCapacity, DraftJob, DraftState, DraftStats};
use super::draft_record::DraftRecord;
use crate::{ContentError, ContentResult, ObjectId};
use std::collections::BTreeMap;

struct Entry {
    record: DraftRecord,
    links: u64,
    queued: Option<u64>,
    charge: usize,
    records: u64,
}
/// Admitted compatibility owner for independent C1 callers, never a Server fallback.
pub struct ResidentDrafts {
    entries: BTreeMap<ObjectId, Entry>,
    jobs: BTreeMap<u64, ObjectId>,
    committed: BTreeMap<ObjectId, ObjectId>,
    emissions: BTreeMap<ObjectId, bool>,
    selected: Option<ObjectId>,
    next_job: u64,
    records: u64,
    stats: DraftStats,
    ended: bool,
}
impl Default for ResidentDrafts {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
            jobs: BTreeMap::new(),
            committed: BTreeMap::new(),
            emissions: BTreeMap::new(),
            selected: None,
            next_job: 1,
            records: 0,
            stats: DraftStats::default(),
            ended: false,
        }
    }
}
impl ResidentDrafts {
    /// Empty explicitly selected resident profile.
    pub fn new() -> Self {
        Self::default()
    }
    fn available(&self) -> ContentResult<()> {
        if self.ended {
            return Err(ContentError::InvalidRecord("draft resident terminal"));
        }
        Ok(())
    }
    fn admit(&mut self, bytes: usize) -> ContentResult<()> {
        let actual = self
            .stats
            .bytes
            .checked_add(bytes)
            .ok_or(ContentError::LengthOverflow)?;
        if actual > super::EDIT_DEFERRED_LIMIT {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "edit.deferred_nodes",
                limit: super::EDIT_DEFERRED_LIMIT as u64,
                actual: actual as u64,
            });
        }
        self.stats.bytes = actual;
        self.stats.peak_bytes = self.stats.peak_bytes.max(actual);
        Ok(())
    }
    fn admit_records(&mut self, added: u64) -> ContentResult<()> {
        let actual = self
            .records
            .checked_add(added)
            .ok_or(ContentError::LengthOverflow)?;
        if actual > DraftCapacity::default().records() {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "draft resident records",
                limit: DraftCapacity::default().records(),
                actual,
            });
        }
        self.records = actual;
        Ok(())
    }
    fn queue(&mut self, id: ObjectId) -> ContentResult<()> {
        let Some(entry) = self.entries.get(&id) else {
            return Ok(());
        };
        if entry.links != 0 || entry.queued.is_some() {
            return Ok(());
        }
        let next = self
            .next_job
            .checked_add(1)
            .ok_or(ContentError::LengthOverflow)?;
        self.admit_records(1)?;
        self.admit(71)?;
        self.entries.get_mut(&id).unwrap().queued = Some(self.next_job);
        self.jobs.insert(self.next_job, id);
        self.next_job = next;
        Ok(())
    }
    fn change_link(&mut self, id: ObjectId, add: bool) -> ContentResult<()> {
        let Some(entry) = self.entries.get_mut(&id) else {
            return Ok(());
        };
        entry.links = if add {
            entry.links.checked_add(1)
        } else {
            entry.links.checked_sub(1)
        }
        .ok_or(ContentError::LengthOverflow)?;
        if !add {
            self.queue(id)?;
        }
        Ok(())
    }
    fn temporaries(&mut self, ids: &[ObjectId], add: bool) -> ContentResult<()> {
        self.available()?;
        if ids.len() > 32 {
            return Err(ContentError::InvalidRecord("draft temporary window"));
        }
        for id in ids {
            if let Some(entry) = self.entries.get(id) {
                let multiplicity = ids.iter().filter(|other| *other == id).count() as u64;
                if add {
                    entry.links.checked_add(multiplicity)
                } else {
                    entry.links.checked_sub(multiplicity)
                }
                .ok_or(ContentError::LengthOverflow)?;
            }
        }
        for id in ids {
            self.change_link(*id, add)?;
        }
        Ok(())
    }
    fn supersede(&mut self, id: ObjectId, expected: Option<ObjectId>) -> ContentResult<()> {
        self.available()?;
        if self.selected != expected {
            return Err(ContentError::InvalidRecord("draft selected root expected"));
        }
        let selected = expected == Some(id);
        if let Some(entry) = self.entries.get(&id) {
            entry
                .links
                .checked_sub(1 + u64::from(selected))
                .ok_or(ContentError::LengthOverflow)?;
        }
        self.change_link(id, false)?;
        if selected {
            self.change_link(id, false)?;
            self.selected = None;
        }
        if let Some(entry) = self.entries.get(&id) {
            if entry.links == 0 {
                let sequence = entry
                    .queued
                    .ok_or(ContentError::InvalidRecord("draft zero job"))?;
                self.retire_exact(DraftJob {
                    sequence,
                    id,
                    links: 0,
                })?;
            }
        }
        Ok(())
    }
}
impl ResidentDrafts {
    fn capacity(&self) -> ContentResult<DraftCapacity> {
        self.available()?;
        Ok(DraftCapacity::default())
    }
    fn hold(&mut self, id: ObjectId, record: DraftRecord) -> ContentResult<()> {
        self.available()?;
        record.validate()?;
        if let Some(existing) = self.entries.get(&id) {
            if existing.record != record {
                return Err(ContentError::InvalidRecord("draft duplicate value"));
            }
            return Ok(());
        }
        let children = record.children()?;
        for child in &children {
            if let Some(entry) = self.entries.get(child) {
                entry
                    .links
                    .checked_add(children.iter().filter(|id| *id == child).count() as u64)
                    .ok_or(ContentError::LengthOverflow)?;
            }
        }
        let charge = record.charge() - 71;
        let deferred_body_bytes = self
            .stats
            .deferred_body_bytes
            .checked_add(record.deferred_body_charge())
            .ok_or(ContentError::LengthOverflow)?;
        // Container/node allowance accompanies each logical record, not just body bytes.
        let sequence = self.next_job;
        let next = sequence
            .checked_add(1)
            .ok_or(ContentError::LengthOverflow)?;
        let records =
            3 + record.references().len() as u64 + record.private_predecessors().len() as u64;
        self.admit_records(records + 1)?;
        self.admit(charge + 128 + 71)?;
        self.entries.insert(
            id,
            Entry {
                record,
                links: 0,
                queued: Some(sequence),
                charge: charge + 128,
                records,
            },
        );
        self.stats.deferred_body_bytes = deferred_body_bytes;
        self.stats.peak_deferred_body_bytes =
            self.stats.peak_deferred_body_bytes.max(deferred_body_bytes);
        for child in children {
            self.change_link(child, true)?;
        }
        self.jobs.insert(sequence, id);
        self.next_job = next;
        self.stats.created += 1;
        Ok(())
    }
    fn get(&mut self, id: ObjectId) -> ContentResult<Option<DraftRecord>> {
        self.available()?;
        Ok(self.entries.get(&id).map(|entry| entry.record.clone()))
    }
    fn select_root(&mut self, before: Option<ObjectId>, after: ObjectId) -> ContentResult<()> {
        self.available()?;
        if self.selected != before {
            return Err(ContentError::InvalidRecord("draft selected root expected"));
        }
        if before == Some(after) {
            return Ok(());
        }
        self.change_link(after, true)?;
        if let Some(before) = before {
            self.change_link(before, false)?;
        }
        self.selected = Some(after);
        Ok(())
    }
    fn next_job(&mut self) -> ContentResult<Option<DraftJob>> {
        self.available()?;
        let Some((sequence, id)) = self.jobs.first_key_value() else {
            return Ok(None);
        };
        let entry = self
            .entries
            .get(id)
            .ok_or(ContentError::InvalidRecord("draft job missing"))?;
        Ok(Some(DraftJob {
            sequence: *sequence,
            id: *id,
            links: entry.links,
        }))
    }
    fn retire_job(&mut self, job: DraftJob) -> ContentResult<()> {
        if self.next_job()? != Some(job) {
            return Err(ContentError::InvalidRecord("draft job expected"));
        }
        self.retire_exact(job)
    }
    fn retire_exact(&mut self, job: DraftJob) -> ContentResult<()> {
        let entry = self
            .entries
            .get(&job.id)
            .ok_or(ContentError::InvalidRecord("draft job missing"))?;
        if entry.queued != Some(job.sequence)
            || entry.links != job.links
            || self.jobs.get(&job.sequence) != Some(&job.id)
        {
            return Err(ContentError::InvalidRecord("draft exact job expected"));
        }
        let children = if job.links == 0 {
            entry.record.children()?
        } else {
            Vec::new()
        };
        for child in &children {
            if let Some(entry) = self.entries.get(child) {
                entry
                    .links
                    .checked_sub(children.iter().filter(|id| *id == child).count() as u64)
                    .ok_or(ContentError::LengthOverflow)?;
            }
        }
        self.jobs.remove(&job.sequence);
        self.records = self
            .records
            .checked_sub(1)
            .ok_or(ContentError::LengthOverflow)?;
        self.stats.bytes = self
            .stats
            .bytes
            .checked_sub(71)
            .ok_or(ContentError::LengthOverflow)?;
        self.entries.get_mut(&job.id).unwrap().queued = None;
        self.stats.jobs_consumed += 1;
        if job.links == 0 {
            let entry = self.entries.remove(&job.id).unwrap();
            let deferred_body_bytes = self
                .stats
                .deferred_body_bytes
                .checked_sub(entry.record.deferred_body_charge())
                .ok_or(ContentError::LengthOverflow)?;
            self.records = self
                .records
                .checked_sub(entry.records)
                .ok_or(ContentError::LengthOverflow)?;
            self.stats.bytes = self
                .stats
                .bytes
                .checked_sub(entry.charge)
                .ok_or(ContentError::LengthOverflow)?;
            drop(entry);
            self.stats.deferred_body_bytes = deferred_body_bytes;
            for child in children {
                self.change_link(child, false)?;
                self.stats.links_retired += 1;
            }
            self.stats.retired += 1;
        }
        Ok(())
    }
    fn resolved(&mut self, id: ObjectId) -> ContentResult<Option<ObjectId>> {
        self.available()?;
        Ok(self.committed.get(&id).copied())
    }
    fn begin_emission(&mut self, draft: ObjectId, canonical: ObjectId) -> ContentResult<bool> {
        self.available()?;
        if self.committed.contains_key(&draft) {
            return Err(ContentError::InvalidRecord("draft emission resolved"));
        }
        match self.emissions.get(&canonical) {
            Some(true) => {
                self.admit_records(1)?;
                self.admit(87)?;
                Ok(false)
            }
            Some(false) => Err(ContentError::InvalidRecord("draft emission pending")),
            None => {
                self.admit_records(2)?;
                self.admit(95 + 87)?;
                self.emissions.insert(canonical, false);
                Ok(true)
            }
        }
    }
    fn accepted(&mut self, draft: ObjectId, canonical: ObjectId) -> ContentResult<()> {
        self.available()?;
        let emission = self
            .emissions
            .get_mut(&canonical)
            .ok_or(ContentError::InvalidRecord("draft emission absent"))?;
        if self.committed.insert(draft, canonical).is_some() {
            return Err(ContentError::InvalidRecord("draft accepted twice"));
        }
        *emission = true;
        Ok(())
    }
    fn finish(&mut self) -> ContentResult<()> {
        self.available()?;
        if self.emissions.values().any(|accepted| !accepted) {
            return Err(ContentError::InvalidRecord("draft finish pending"));
        }
        if let Some(selected) = self.selected {
            self.change_link(selected, false)?;
            self.selected = None;
        }
        while let Some(job) = self.next_job()? {
            self.retire_job(job)?;
        }
        if !self.entries.is_empty() || !self.jobs.is_empty() {
            return Err(ContentError::InvalidRecord("draft final exact EOF"));
        }
        self.committed.clear();
        self.emissions.clear();
        self.stats.deferred_body_bytes = 0;
        self.stats.bytes = 0;
        self.records = 0;
        self.ended = true;
        Ok(())
    }
    fn abandon(&mut self) {
        self.ended = true;
    }
    fn stats(&self) -> DraftStats {
        self.stats
    }
}

impl ResidentDrafts {
    fn attempt<T>(
        &mut self,
        operation: impl FnOnce(&mut Self) -> ContentResult<T>,
    ) -> ContentResult<T> {
        let result = operation(self);
        if result.is_err() {
            self.ended = true;
        }
        result
    }
}
impl DraftState for ResidentDrafts {
    fn capacity(&self) -> ContentResult<DraftCapacity> {
        ResidentDrafts::capacity(self)
    }
    fn hold(&mut self, id: ObjectId, record: DraftRecord) -> ContentResult<()> {
        self.attempt(|state| state.hold(id, record))
    }
    fn get(&mut self, id: ObjectId) -> ContentResult<Option<DraftRecord>> {
        self.attempt(|state| state.get(id))
    }
    fn select_root(&mut self, before: Option<ObjectId>, after: ObjectId) -> ContentResult<()> {
        self.attempt(|state| state.select_root(before, after))
    }
    fn retain_temporaries(&mut self, ids: &[ObjectId]) -> ContentResult<()> {
        self.attempt(|state| state.temporaries(ids, true))
    }
    fn release_temporaries(&mut self, ids: &[ObjectId]) -> ContentResult<()> {
        self.attempt(|state| state.temporaries(ids, false))
    }
    fn supersede(&mut self, id: ObjectId, expected: Option<ObjectId>) -> ContentResult<()> {
        self.attempt(|state| state.supersede(id, expected))
    }
    fn next_job(&mut self) -> ContentResult<Option<DraftJob>> {
        self.attempt(ResidentDrafts::next_job)
    }
    fn retire_job(&mut self, job: DraftJob) -> ContentResult<()> {
        self.attempt(|state| state.retire_job(job))
    }
    fn resolved(&mut self, id: ObjectId) -> ContentResult<Option<ObjectId>> {
        self.attempt(|state| state.resolved(id))
    }
    fn begin_emission(&mut self, draft: ObjectId, canonical: ObjectId) -> ContentResult<bool> {
        self.attempt(|state| state.begin_emission(draft, canonical))
    }
    fn accepted(&mut self, draft: ObjectId, canonical: ObjectId) -> ContentResult<()> {
        self.attempt(|state| state.accepted(draft, canonical))
    }
    fn finish(&mut self) -> ContentResult<()> {
        self.attempt(ResidentDrafts::finish)
    }
    fn abandon(&mut self) {
        ResidentDrafts::abandon(self);
    }
    fn stats(&self) -> DraftStats {
        ResidentDrafts::stats(self)
    }
}
