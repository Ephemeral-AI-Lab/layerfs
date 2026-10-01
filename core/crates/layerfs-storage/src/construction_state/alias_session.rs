//! Same native session AliasFrontier operations and exact retained failure custody.
use super::alias_state::{AliasAttempt, Aliases};
use super::{alias_index, alias_mutation, ScratchSession};
use crate::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{
    AliasCapacity, AliasCurrent, AliasFact, AliasProgress, AliasSeal, SiteMembership,
};
use rusqlite::Connection;

impl ScratchSession {
    fn alias_selected(&self, members: &SiteMembership) -> StorageResult<()> {
        let aliases = self
            .resource
            .as_ref()
            .and_then(|r| r.aliases.as_ref())
            .ok_or(StorageError::Content(
                layerfs_content::ContentError::UnsupportedProfile {
                    what: "native alias frontier",
                },
            ))?;
        aliases.check(members)?;
        self.resource.as_ref().unwrap().check_live()
    }
    fn alias_change<T>(
        &mut self,
        members: &SiteMembership,
        kind: &'static str,
        initialize: bool,
        body: impl FnOnce(&Connection, &mut Aliases) -> StorageResult<T>,
    ) -> StorageResult<T> {
        self.alias_selected(members)?;
        let result = (|| {
            let resource = self.resource.as_mut().unwrap();
            resource.verify()?;
            resource.sites.as_ref().unwrap().check_membership(members)?;
            let namespace = resource.namespace_alias_capacity();
            let canonical = resource.canonical_alias_capacity();
            let state = resource.aliases.as_mut().unwrap();
            state.attempt = Some(AliasAttempt::new(state, kind)?);
            resource.native.reserve()?;
            let value = alias_mutation::transaction(
                resource.connection.as_ref().unwrap(),
                resource.engine,
                state,
                |connection, state| {
                    let value = body(connection, state)?;
                    if let Some((capacity, mut live)) = namespace {
                        let attempt = state.attempt.as_ref().unwrap();
                        live.alias_facts = if attempt.new_stage >= 3 {
                            attempt.after.remaining
                        } else {
                            attempt.after.facts
                        };
                        live.alias_jobs = attempt.after.jobs;
                        capacity.check(live)?;
                    }
                    if let Some((capacity, mut live)) = canonical {
                        let attempt = state.attempt.as_ref().unwrap();
                        live.namespace.alias_facts = if attempt.new_stage >= 3 {
                            attempt.after.remaining
                        } else {
                            attempt.after.facts
                        };
                        live.namespace.alias_jobs = attempt.after.jobs;
                        capacity.check(live)?;
                    }
                    Ok(value)
                },
                initialize.then_some(members),
            )?;
            resource.native.observe_allocation()?;
            state.acknowledge();
            if initialize {
                state.members = Some(members.clone());
            }
            Ok(value)
        })();
        self.finish(result)
    }
    /// Captured logical aggregate class; no new file or per-table quota.
    pub fn alias_capacity(&self, members: &SiteMembership) -> StorageResult<AliasCapacity> {
        self.alias_selected(members)?;
        self.finish((|| {
            let resource = self.resource.as_ref().unwrap();
            resource.verify()?;
            resource.sites.as_ref().unwrap().check_membership(members)?;
            Ok(resource.aliases.as_ref().unwrap().capacity)
        })())
    }
    /// Begins once under the exact acknowledged Sites membership.
    pub fn alias_begin(
        &mut self,
        members: &SiteMembership,
        root: Option<u64>,
    ) -> StorageResult<()> {
        self.alias_change(members, "begin", true, |connection, state| {
            if state.stage != 0 {
                return Err(StorageError::Integrity("alias begin phase"));
            }
            state.attempt.as_mut().unwrap().new_stage = 1;
            if let Some(root) = root {
                alias_mutation::enqueue_plan(connection, state, &[root])?;
            }
            Ok(())
        })
    }
    /// Bounded ordered discoveries preserve repeated pending LIFO priority.
    pub fn alias_enqueue(
        &mut self,
        members: &SiteMembership,
        children: &[u64],
    ) -> StorageResult<()> {
        self.alias_change(members, "enqueue", false, |connection, state| {
            if state.stage != 1 {
                return Err(StorageError::Integrity("alias enqueue phase"));
            }
            alias_mutation::enqueue_plan(connection, state, children)
        })
    }
    /// One indexed MAX job; no complete queue is materialized.
    pub fn alias_take(&mut self, members: &SiteMembership) -> StorageResult<Option<AliasCurrent>> {
        self.alias_change(members, "take", false, |connection, state| {
            if state.stage != 1 || state.current.is_some() {
                return Err(StorageError::Integrity("alias take phase"));
            }
            let mut statement = connection
                .prepare("SELECT key,serial FROM alias_jobs ORDER BY key DESC LIMIT 1")?;
            let mut rows = statement.query([])?;
            let Some(row) = rows.next()? else {
                if state.totals.jobs != 0 {
                    return Err(StorageError::Integrity("alias job EOF count"));
                }
                return Ok(None);
            };
            let sequence =
                alias_index::scalar(&state.scope, 7, alias_index::blob(row.get_ref(0)?, 25)?)?;
            let serial = alias_index::unsigned(row.get::<_, i64>(1)?)?;
            let old = alias_index::fact(connection, state, serial)?
                .ok_or(StorageError::Integrity("alias job fact missing"))?;
            if old.status != 1 || old.sequence != sequence || state.totals.jobs == 0 {
                return Err(StorageError::Integrity("alias exact pending job"));
            }
            let current = AliasCurrent { serial, sequence };
            let a = state.attempt.as_mut().unwrap();
            a.after.jobs -= 1;
            a.new_current = Some(current);
            a.new_progress = AliasProgress::initial();
            a.changes
                .push((Some(old), Some(AliasFact { status: 2, ..old })));
            Ok(Some(current))
        })
    }
    /// Exact before/proposed listing continuation retained through COMMIT.
    pub fn alias_advance(
        &mut self,
        members: &SiteMembership,
        current: AliasCurrent,
        before: &AliasProgress,
        after: &AliasProgress,
    ) -> StorageResult<()> {
        self.alias_change(members, "advance", false, |_, state| {
            if state.stage != 1 || state.current != Some(current) || &state.progress != before {
                return Err(StorageError::Integrity("alias selected current progress"));
            }
            before.advances_to(after)?;
            AliasProgress::decode(&after.encode())?;
            state.attempt.as_mut().unwrap().new_progress = after.clone();
            Ok(())
        })
    }
    /// Marks an exactly completed current directory expanded once.
    pub fn alias_complete(
        &mut self,
        members: &SiteMembership,
        current: AliasCurrent,
    ) -> StorageResult<()> {
        self.alias_change(members, "complete", false, |connection, state| {
            if state.stage != 1 || state.current != Some(current) || state.progress.stage() != 2 {
                return Err(StorageError::Integrity("alias incomplete current"));
            }
            let old = alias_index::fact(connection, state, current.serial)?
                .ok_or(StorageError::Integrity("alias current fact missing"))?;
            if old.status != 2 || old.sequence != current.sequence {
                return Err(StorageError::Integrity("alias exact current fact"));
            }
            let a = state.attempt.as_mut().unwrap();
            a.after.expanded += 1;
            a.new_current = None;
            a.changes
                .push((Some(old), Some(AliasFact { status: 3, ..old })));
            Ok(())
        })
    }
    /// Closed exact EOF before bounded retirement.
    pub fn alias_finish(&mut self, members: &SiteMembership) -> StorageResult<AliasSeal> {
        let seal = self.alias_change(members, "finish", false, |connection, state| {
            if state.stage != 1
                || state.current.is_some()
                || state.totals.jobs != 0
                || state.totals.expanded != state.totals.facts
                || !alias_index::empty(connection, "alias_jobs")?
            {
                return Err(StorageError::Integrity("alias final EOF"));
            }
            let mut statement =
                connection.prepare("SELECT key FROM alias_facts ORDER BY key DESC LIMIT 1")?;
            let mut rows = statement.query([])?;
            let maximum = match rows.next()? {
                Some(row) => Some(alias_index::scalar(
                    &state.scope,
                    6,
                    alias_index::blob(row.get_ref(0)?, 25)?,
                )?),
                None => None,
            };
            if maximum.is_none() != (state.totals.facts == 0) {
                return Err(StorageError::Integrity("alias maximum count"));
            }
            let a = state.attempt.as_mut().unwrap();
            a.new_stage = 2;
            a.after.remaining = a.after.facts;
            Ok(AliasSeal {
                members: members.clone(),
                records: state.totals.facts,
                sequence: state.totals.sequence,
                maximum,
            })
        })?;
        self.resource
            .as_mut()
            .unwrap()
            .aliases
            .as_mut()
            .unwrap()
            .seal = Some(seal.clone());
        Ok(seal)
    }
    /// Exact indexed retirement, at most128 complete facts per transaction.
    pub fn alias_retire(&mut self, seal: &AliasSeal) -> StorageResult<()> {
        self.alias_selected(&seal.members)?;
        if self
            .resource
            .as_ref()
            .unwrap()
            .aliases
            .as_ref()
            .unwrap()
            .seal
            .as_ref()
            != Some(seal)
        {
            return self.finish(Err(StorageError::Integrity("alias exact terminal seal")));
        }
        loop {
            let finished=self.alias_change(&seal.members,"retire",false,|connection,state| {
                if !matches!(state.stage,2|3) {return Err(StorageError::Integrity("alias retirement phase"));}
                let count=state.totals.remaining.min(128);
                let after=state.totals.after.map(|s|alias_index::key(&state.scope,6,s));
                let mut statement=connection.prepare("SELECT key,sequence,status FROM alias_facts WHERE key>?1 ORDER BY key LIMIT ?2")?;
                let lower=after.as_ref().map_or(&[][..],|key|key.as_slice());
                let mut rows=statement.query(rusqlite::params![lower,count as i64])?;
                let mut found=0u64; let mut last=state.totals.after;
                while let Some(row)=rows.next()? {
                    let serial=alias_index::scalar(&state.scope,6,alias_index::blob(row.get_ref(0)?,25)?)?;
                    let fact=AliasFact {serial,sequence:alias_index::unsigned(row.get::<_, i64>(1)?)?,status:row.get(2)?}; fact.check()?;
                    if fact.status!=3 || last.is_some_and(|s|s>=serial) {return Err(StorageError::Integrity("alias retirement record"));}
                    state.attempt.as_mut().unwrap().changes.push((Some(fact),None));
                    last=Some(serial); found+=1;
                }
                if found!=count {return Err(StorageError::Integrity("alias retirement count"));}
                let a=state.attempt.as_mut().unwrap(); a.after.remaining-=count; a.after.after=last;
                let done=a.after.remaining==0;
                if done && last!=seal.maximum {return Err(StorageError::Integrity("alias retirement last key"));}
                a.new_stage=if done {4}else{3}; Ok(done)
            })?;
            if finished {
                let result = (|| {
                    let resource = self.resource.as_ref().unwrap();
                    let connection = resource.verify()?;
                    if !alias_index::empty(connection, "alias_facts")?
                        || !alias_index::empty(connection, "alias_jobs")?
                    {
                        return Err(StorageError::Integrity("alias retirement EOF"));
                    }
                    Ok(())
                })();
                return self.finish(result);
            }
        }
    }
    /// Metadata-only terminalization; no SQL, close, unlink or refund.
    pub fn alias_abandon(&mut self, members: &SiteMembership) -> StorageResult<()> {
        let resource = self
            .resource
            .as_ref()
            .ok_or(StorageError::Integrity("alias owner released"))?;
        let state = resource
            .aliases
            .as_ref()
            .ok_or(StorageError::Integrity("alias owner unavailable"))?;
        if &state.scope != members.birth().scope()
            || state.members.as_ref().is_some_and(|m| m != members)
        {
            return Err(StorageError::Content(
                layerfs_content::ContentError::InvalidOrderingRecord("alias foreign membership"),
            ));
        }
        state.failed.set(true);
        self.finish(Ok(()))
    }
}
