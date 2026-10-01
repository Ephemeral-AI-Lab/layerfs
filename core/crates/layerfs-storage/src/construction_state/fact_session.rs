//! Supplied BaseFacts/ParentEligibility operations on the existing native owner.
use super::fact_state::{FactAttempt, Facts};
use super::{fact_index, fact_mutation, ScratchSession};
use crate::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{
    BaseFact, FactCapacity, FactScope, GraphMemory, ParentFact, StateTable, BASE_FACT_BYTES,
    PARENT_ELIGIBILITY_BYTES,
};
use rusqlite::Connection;

impl ScratchSession {
    pub(crate) fn fact_selected(&self, scope: &FactScope) -> StorageResult<u8> {
        let resource = self
            .resource
            .as_ref()
            .ok_or(StorageError::Integrity("fact owner released"))?;
        if scope.state().selection() != &resource.selection {
            return Err(StorageError::Content(
                layerfs_content::ContentError::InvalidOrderingRecord("fact foreign scope"),
            ));
        }
        let code = resource
            .facts
            .as_ref()
            .ok_or(StorageError::Content(
                layerfs_content::ContentError::UnsupportedProfile {
                    what: "native namespace facts",
                },
            ))?
            .select(scope)?;
        resource.check_live()?;
        Ok(code)
    }
    pub(crate) fn fact_change<T>(
        &mut self,
        scope: &FactScope,
        kind: &'static str,
        count: usize,
        body: impl FnOnce(&Connection, &mut Facts) -> StorageResult<T>,
    ) -> StorageResult<T> {
        let table = self.fact_selected(scope)?;
        let result = (|| {
            let resource = self.resource.as_mut().unwrap();
            resource.verify()?;
            let state = resource.facts.as_mut().unwrap();
            state.attempt = Some(FactAttempt::new(state, table, kind, count)?);
            resource.native.reserve()?;
            let value = fact_mutation::transaction(
                resource.connection.as_ref().unwrap(),
                resource.engine,
                state,
                |connection, state| {
                    let value = body(connection, state)?;
                    // The shared Resource checks proposed populations before SQL changes.
                    // Other namespace growth uses the same captured class at its owning boundary.
                    Ok(value)
                },
            )?;
            resource.native.observe_allocation()?;
            state.acknowledge();
            Ok(value)
        })();
        self.finish(result)
    }
    /// Previously admitted fact/parent aggregate class, before dependent effects.
    pub fn fact_capacity(&self, scope: &FactScope) -> StorageResult<FactCapacity> {
        self.fact_selected(scope)?;
        self.finish((|| {
            let r = self.resource.as_ref().unwrap();
            r.verify()?;
            Ok(r.facts.as_ref().unwrap().capacity)
        })())
    }
    /// Shared scoped fixed owner/attempt/window/consumer admission.
    pub fn fact_memory(&self, scope: &FactScope) -> StorageResult<GraphMemory> {
        self.fact_selected(scope)?;
        self.finish((|| {
            let r = self.resource.as_ref().unwrap();
            r.verify()?;
            Ok(r.facts.as_ref().unwrap().memory.clone())
        })())
    }
    /// Bind exactly the authenticated BaseFact subject once.
    pub fn fact_bind(&mut self, scope: &FactScope) -> StorageResult<()> {
        if scope.state().table() != StateTable::BaseFacts {
            return Err(StorageError::Content(
                layerfs_content::ContentError::InvalidOrderingRecord("fact foreign scope"),
            ));
        }
        self.fact_change(scope, "bind", 0, |_, state| {
            if state.base.stage != 0 {
                return Err(StorageError::Integrity("base fact bind phase"));
            }
            let after = &mut state.attempt.as_mut().unwrap().after;
            after.scope = Some(scope.clone());
            after.stage = 1;
            Ok(())
        })
    }
    /// Exact current immutable answer; no row remains unknown.
    pub fn fact_get(&mut self, scope: &FactScope, serial: u64) -> StorageResult<Option<BaseFact>> {
        self.fact_selected(scope)?;
        self.finish((|| {
            let r = self.resource.as_ref().unwrap();
            let state = r.facts.as_ref().unwrap();
            if state.base.stage != 1 || state.base.scope.as_ref() != Some(scope) {
                return Err(StorageError::Integrity("base fact point phase"));
            }
            fact_index::base(r.verify()?, scope, serial)
        })())
    }
    /// One bounded immutable insertion, with aggregate admission before effects.
    pub fn fact_insert(&mut self, scope: &FactScope, records: &[BaseFact]) -> StorageResult<()> {
        self.fact_selected(scope)?;
        let proposed = self
            .resource
            .as_ref()
            .unwrap()
            .facts
            .as_ref()
            .unwrap()
            .base
            .records
            .checked_add(records.len() as u64)
            .ok_or(StorageError::Integrity("base fact count overflow"))?;
        let growth = self
            .resource
            .as_ref()
            .unwrap()
            .namespace_fact_growth(Some(proposed), None);
        self.finish(growth)?;
        self.fact_change(scope, "insert", records.len(), |connection, state| {
            if state.base.stage != 1 || scope.subject().table().is_none() {
                return Err(StorageError::Integrity("base fact insertion phase"));
            }
            for (i, fact) in records.iter().enumerate() {
                fact.encode_value()?;
                if records[..i].iter().any(|prior| prior.serial == fact.serial)
                    || fact_index::base(connection, scope, fact.serial)?.is_some()
                {
                    return Err(StorageError::Integrity("duplicate immutable base fact"));
                }
                state.attempt.as_mut().unwrap().bases.push(*fact);
            }
            let a = state.attempt.as_mut().unwrap();
            a.after.records = proposed;
            a.after.bytes = proposed
                .checked_mul(BASE_FACT_BYTES)
                .ok_or(StorageError::Integrity("base fact bytes"))?;
            for fact in records {
                a.after.maximum = Some(
                    a.after
                        .maximum
                        .map_or(fact.serial, |old| old.max(fact.serial)),
                );
            }
            Ok(())
        })
    }
    /// Bind the same authenticated subject for new-parent eligibility.
    pub fn parent_bind(&mut self, scope: &FactScope) -> StorageResult<()> {
        if scope.state().table() != StateTable::ParentEligibility {
            return Err(StorageError::Content(
                layerfs_content::ContentError::InvalidOrderingRecord("fact foreign scope"),
            ));
        }
        self.fact_change(scope, "parent_bind", 0, |_, state| {
            if state.parent.stage != 0
                || state
                    .base
                    .scope
                    .as_ref()
                    .is_none_or(|base| base.subject() != scope.subject())
            {
                return Err(StorageError::Integrity("parent selected base subject"));
            }
            let a = state.attempt.as_mut().unwrap();
            a.after.scope = Some(scope.clone());
            a.after.stage = 1;
            Ok(())
        })
    }
    /// Strict source-ordered new nonroot directory declarations.
    pub fn parent_insert(&mut self, scope: &FactScope, serials: &[u64]) -> StorageResult<()> {
        self.fact_selected(scope)?;
        let proposed = self
            .resource
            .as_ref()
            .unwrap()
            .facts
            .as_ref()
            .unwrap()
            .parent
            .records
            .checked_add(serials.len() as u64)
            .ok_or(StorageError::Integrity("parent count overflow"))?;
        let growth = self
            .resource
            .as_ref()
            .unwrap()
            .namespace_fact_growth(None, Some(proposed));
        self.finish(growth)?;
        self.fact_change(
            scope,
            "parent_insert",
            serials.len(),
            |connection, state| {
                if state.parent.stage != 1 {
                    return Err(StorageError::Integrity("parent declaration phase"));
                }
                let mut last = state.parent.maximum;
                for serial in serials {
                    scope.key(*serial)?;
                    if *serial == scope.subject().selected().root_serial()
                        || last.is_some_and(|old| old >= *serial)
                        || fact_index::parent(connection, scope, *serial)?.is_some()
                    {
                        return Err(StorageError::Integrity("parent declaration order"));
                    }
                    state.attempt.as_mut().unwrap().parents.push((
                        None,
                        Some(ParentFact {
                            serial: *serial,
                            bound: false,
                        }),
                    ));
                    last = Some(*serial);
                }
                let a = state.attempt.as_mut().unwrap();
                a.after.records = proposed;
                a.after.bytes = proposed
                    .checked_mul(PARENT_ELIGIBILITY_BYTES)
                    .ok_or(StorageError::Integrity("parent bytes"))?;
                a.after.maximum = last;
                Ok(())
            },
        )
    }
    /// Close declaration EOF once before incoming-binding observations.
    pub fn parent_close_declarations(&mut self, scope: &FactScope) -> StorageResult<()> {
        self.fact_change(scope, "parent_close", 0, |_, state| {
            if state.parent.stage != 1 {
                return Err(StorageError::Integrity("parent close phase"));
            }
            state.attempt.as_mut().unwrap().after.stage = 2;
            Ok(())
        })
    }
    /// Monotone bound facts; unknown keys remain outside the declared population.
    pub fn parent_mark_bound(&mut self, scope: &FactScope, children: &[u64]) -> StorageResult<()> {
        self.fact_change(scope, "parent_mark", children.len(), |connection, state| {
            if state.parent.stage != 2 {
                return Err(StorageError::Integrity("parent marking phase"));
            }
            for serial in children {
                scope.key(*serial)?;
                if state
                    .attempt
                    .as_ref()
                    .unwrap()
                    .parents
                    .iter()
                    .any(|(_, new)| new.is_some_and(|p| p.serial == *serial))
                {
                    continue;
                }
                let Some(old) = fact_index::parent(connection, scope, *serial)? else {
                    continue;
                };
                if !old.bound {
                    let a = state.attempt.as_mut().unwrap();
                    a.after.bound += 1;
                    a.parents
                        .push((Some(old), Some(ParentFact { bound: true, ..old })));
                }
            }
            Ok(())
        })
    }
    /// Selected metadata-only failure terminalization, no SQL/native refund.
    pub fn fact_abandon(&mut self, scope: &FactScope) -> StorageResult<()> {
        let resource = self
            .resource
            .as_ref()
            .ok_or(StorageError::Integrity("fact owner released"))?;
        if scope.state().selection() != &resource.selection {
            return Err(StorageError::Content(
                layerfs_content::ContentError::InvalidOrderingRecord("fact foreign scope"),
            ));
        }
        let state = resource
            .facts
            .as_ref()
            .ok_or(StorageError::Integrity("fact owner unavailable"))?;
        if scope.subject().selected() != &state.subject
            || state
                .section(scope.state().table().code())
                .scope
                .as_ref()
                .is_some_and(|old| old != scope)
        {
            return Err(StorageError::Content(
                layerfs_content::ContentError::InvalidOrderingRecord("fact foreign scope"),
            ));
        }
        state.failed.set(true);
        self.finish(Ok(()))
    }
}
