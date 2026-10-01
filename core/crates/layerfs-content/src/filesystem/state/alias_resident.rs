//! Explicit resident compatibility profile; never a native resource-fit proof.
use super::{
    AliasCapacity, AliasCurrent, AliasFact, AliasFrontier, AliasProgress, AliasSeal, SiteMembership,
};
use crate::{ContentError, ContentResult};
use std::collections::BTreeMap;
/// Caller-selected resident compatibility authority with the same discovery rules.
pub struct ResidentAliasFrontier {
    capacity: AliasCapacity,
    members: Option<SiteMembership>,
    facts: BTreeMap<u64, AliasFact>,
    jobs: BTreeMap<u64, u64>,
    current: Option<AliasCurrent>,
    progress: AliasProgress,
    sequence: u64,
    expanded: u64,
    stage: u8,
    seal: Option<AliasSeal>,
}
impl ResidentAliasFrontier {
    /// Select explicit logical compatibility limits before alias effects.
    pub fn new(capacity: AliasCapacity) -> Self {
        Self {
            capacity,
            members: None,
            facts: BTreeMap::new(),
            jobs: BTreeMap::new(),
            current: None,
            progress: AliasProgress::initial(),
            sequence: 0,
            expanded: 0,
            stage: 0,
            seal: None,
        }
    }
    fn selected(&self, members: &SiteMembership) -> ContentResult<()> {
        if self.members.as_ref().is_some_and(|m| m != members) {
            return Err(ContentError::InvalidOrderingRecord(
                "alias foreign membership",
            ));
        }
        if self.stage > 2 {
            return Err(ContentError::InvalidOrderingRecord("alias terminal"));
        }
        Ok(())
    }
}
impl AliasFrontier for ResidentAliasFrontier {
    fn alias_capacity(&self, members: &SiteMembership) -> ContentResult<AliasCapacity> {
        self.selected(members)?;
        Ok(self.capacity)
    }
    fn alias_begin(&mut self, members: &SiteMembership, root: Option<u64>) -> ContentResult<()> {
        self.selected(members)?;
        if self.stage != 0 {
            return Err(ContentError::InvalidOrderingRecord("alias begin"));
        }
        self.members = Some(members.clone());
        self.stage = 1;
        if let Some(root) = root {
            self.alias_enqueue(members, &[root])?;
        }
        Ok(())
    }
    fn alias_enqueue(&mut self, members: &SiteMembership, children: &[u64]) -> ContentResult<()> {
        self.selected(members)?;
        if self.stage != 1 || children.len() > 128 {
            return Err(ContentError::InvalidOrderingRecord("alias enqueue"));
        }
        let mut changes = Vec::new();
        changes.try_reserve_exact(children.len()).map_err(|_| {
            ContentError::ResourceUnavailable {
                what: "alias.compatibility_window",
            }
        })?;
        let mut facts = self.facts.len() as u64;
        let mut jobs = self.jobs.len() as u64;
        let mut sequence = self.sequence;
        let planned = (|| {
            for serial in children {
                AliasFact {
                    serial: *serial,
                    sequence: 1,
                    status: 1,
                }
                .check()?;
                let old = changes
                    .iter()
                    .rev()
                    .find(|(_, new): &&(Option<AliasFact>, AliasFact)| new.serial == *serial)
                    .map(|(_, new)| *new)
                    .or_else(|| self.facts.get(serial).copied());
                if old.is_some_and(|f| f.status != 1) {
                    continue;
                }
                facts += u64::from(old.is_none());
                jobs += u64::from(old.is_none());
                self.capacity.check(facts, jobs)?;
                sequence = sequence
                    .checked_add(1)
                    .filter(|s| *s <= i64::MAX as u64)
                    .ok_or(ContentError::LengthOverflow)?;
                changes.push((
                    old,
                    AliasFact {
                        serial: *serial,
                        sequence,
                        status: 1,
                    },
                ));
            }
            Ok(())
        })();
        if let Err(error) = planned {
            self.stage = 4;
            return Err(error);
        }
        for (old, new) in changes {
            if let Some(old) = old {
                self.jobs.remove(&old.sequence);
            }
            self.jobs.insert(new.sequence, new.serial);
            self.facts.insert(new.serial, new);
        }
        self.sequence = sequence;
        Ok(())
    }
    fn alias_take(&mut self, members: &SiteMembership) -> ContentResult<Option<AliasCurrent>> {
        self.selected(members)?;
        if self.stage != 1 || self.current.is_some() {
            return Err(ContentError::InvalidOrderingRecord("alias current"));
        }
        let Some((sequence, serial)) = self.jobs.pop_last() else {
            return Ok(None);
        };
        self.facts.get_mut(&serial).unwrap().status = 2;
        let current = AliasCurrent { serial, sequence };
        self.current = Some(current);
        self.progress = AliasProgress::initial();
        Ok(Some(current))
    }
    fn alias_advance(
        &mut self,
        members: &SiteMembership,
        current: AliasCurrent,
        before: &AliasProgress,
        after: &AliasProgress,
    ) -> ContentResult<()> {
        self.selected(members)?;
        if self.current != Some(current) || &self.progress != before {
            return Err(ContentError::InvalidOrderingRecord(
                "alias selected progress",
            ));
        }
        before.advances_to(after)?;
        self.progress = after.clone();
        Ok(())
    }
    fn alias_complete(
        &mut self,
        members: &SiteMembership,
        current: AliasCurrent,
    ) -> ContentResult<()> {
        self.selected(members)?;
        if self.current != Some(current) || self.progress.stage() != 2 {
            return Err(ContentError::InvalidOrderingRecord(
                "alias incomplete current",
            ));
        }
        self.facts.get_mut(&current.serial).unwrap().status = 3;
        self.current = None;
        self.expanded += 1;
        Ok(())
    }
    fn alias_finish(&mut self, members: &SiteMembership) -> ContentResult<AliasSeal> {
        self.selected(members)?;
        if self.stage != 1
            || self.current.is_some()
            || !self.jobs.is_empty()
            || self.expanded != self.facts.len() as u64
        {
            return Err(ContentError::InvalidOrderingRecord("alias EOF"));
        }
        let seal = AliasSeal {
            members: members.clone(),
            records: self.expanded,
            sequence: self.sequence,
            maximum: self.facts.last_key_value().map(|(s, _)| *s),
        };
        self.seal = Some(seal.clone());
        self.stage = 2;
        Ok(seal)
    }
    fn alias_retire(&mut self, seal: &AliasSeal) -> ContentResult<()> {
        self.selected(&seal.members)?;
        if self.seal.as_ref() != Some(seal) || self.stage != 2 {
            return Err(ContentError::InvalidOrderingRecord("alias selected seal"));
        }
        while !self.facts.is_empty() {
            for _ in 0..128 {
                if self.facts.pop_first().is_none() {
                    break;
                }
            }
        }
        self.stage = 3;
        Ok(())
    }
    fn alias_abandon(&mut self, members: &SiteMembership) -> ContentResult<()> {
        if self.members.as_ref().is_some_and(|m| m != members) {
            return Err(ContentError::InvalidOrderingRecord(
                "alias foreign membership",
            ));
        }
        self.stage = 4;
        Ok(())
    }
}
