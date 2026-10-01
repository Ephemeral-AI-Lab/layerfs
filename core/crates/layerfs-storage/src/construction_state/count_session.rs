//! Deferred real count authority and exact point mutation semantics.
use super::{
    count_index, count_mutation,
    count_state::{CountAttempt, Counts},
    session::Resource,
    ScratchSession,
};
use crate::{StorageError, StorageResult};
use layerfs_content::filesystem::references::record::Row;
use layerfs_content::filesystem::state::{
    CanonicalCapacity, CanonicalScope, CountRecord, GraphMemory, GraphStage,
};
use layerfs_content::ContentError;
impl ScratchSession {
    pub(crate) fn count_context(&self, scope: &CanonicalScope) -> StorageResult<&Resource> {
        let r = self
            .resource
            .as_ref()
            .ok_or(StorageError::Integrity("count owner released"))?;
        let facts = r
            .facts
            .as_ref()
            .ok_or(StorageError::Integrity("count original facts unavailable"))?;
        if scope.state().table().code() != 19
            || scope.state().selection() != &r.selection
            || scope.subject().selected() != &facts.subject
            || facts
                .base
                .scope
                .as_ref()
                .is_none_or(|s| s.subject() != scope.subject())
            || facts
                .parent
                .scope
                .as_ref()
                .is_none_or(|s| s.subject() != scope.subject())
            || r.counts.as_ref().is_some_and(|s| &s.scope != scope)
        {
            return Err(StorageError::Content(ContentError::InvalidOrderingRecord(
                "count foreign scope",
            )));
        }
        r.check_live()?;
        if r.canonical_capacity.is_none()
            || r.graph.as_ref().is_none_or(|g| {
                g.stage != GraphStage::Retired || g.failed.get() || g.attempt.is_some()
            })
            || facts.failed.get()
            || !matches!(facts.base.stage, 1 | 4)
            || r.counts
                .as_ref()
                .is_some_and(|c| c.failed.get() || c.attempt.is_some())
        {
            return Err(StorageError::Integrity("count selected owner unavailable"));
        }
        Ok(r)
    }
    /// Read the previously captured independent record classes and one shared S.
    /// This verifies the actual selected native owner and makes no physical-fit claim.
    pub fn count_capacity(&self, scope: &CanonicalScope) -> StorageResult<CanonicalCapacity> {
        self.finish((|| {
            let r = self.count_context(scope)?;
            r.verify()?;
            Ok(r.counts
                .as_ref()
                .map_or(r.canonical_capacity.unwrap(), |c| c.capacity))
        })())
    }
    /// Borrow the same scoped64KiB namespace working controller.
    /// Before deferred Count creation, require all preceding Graph consumers to
    /// end; no separate controller or additional working allowance is granted.
    pub fn count_memory(&self, scope: &CanonicalScope) -> StorageResult<GraphMemory> {
        self.finish((|| {
            let r = self.count_context(scope)?;
            r.verify()?;
            let memory = r.graph_memory.as_ref().unwrap();
            if r.counts.is_none() {
                let expected = GraphMemory::control_bytes()
                    + std::mem::size_of::<super::graph_state::Graph>()
                    + super::graph_layout::ALIAS_PERSISTENT
                    + super::graph_layout::FACT_PERSISTENT;
                if memory.reserved_bytes() != expected {
                    return Err(StorageError::Integrity(
                        "canonical preceding working consumers",
                    ));
                }
            }
            Ok(memory.clone())
        })())
    }
    /// Bind this actual authenticated subject and admit the deferred Count owner.
    /// Require known Graph retirement, open BaseFacts and sealed ParentEligibility;
    /// prepare real owner/attempt allocations before dependent SQL effects.
    pub fn count_begin(&mut self, scope: &CanonicalScope) -> StorageResult<()> {
        self.count_context(scope)?;
        let result = (|| {
            let r = self.resource.as_mut().unwrap();
            if r.counts.is_some()
                || r.facts.as_ref().unwrap().base.stage != 1
                || r.facts.as_ref().unwrap().parent.stage != 3
            {
                return Err(StorageError::Integrity("count deferred begin phase"));
            }
            let memory = r.graph_memory.as_ref().unwrap();
            let baseline = GraphMemory::control_bytes()
                + std::mem::size_of::<super::graph_state::Graph>()
                + super::graph_layout::ALIAS_PERSISTENT
                + super::graph_layout::FACT_PERSISTENT;
            if memory.reserved_bytes() != baseline
                && memory.reserved_bytes()
                    != baseline
                        + layerfs_content::filesystem::update::canonical_reduction_working_bytes()
            {
                return Err(StorageError::Integrity(
                    "canonical preceding working consumers",
                ));
            }
            r.verify()?;
            count_index::deferred(r.connection.as_ref().unwrap())?;
            r.counts = Some(Counts::new(
                scope.clone(),
                r.canonical_capacity.unwrap(),
                r.graph_memory.as_ref().unwrap().clone(),
            )?);
            let state = r.counts.as_mut().unwrap();
            let mut a = CountAttempt::new(state, "begin", 0, 0)?;
            a.after.stage = 1;
            state.attempt = Some(a);
            r.native.reserve()?;
            count_mutation::commit(
                r.connection.as_ref().unwrap(),
                r.engine,
                r.counts.as_ref().unwrap(),
            )?;
            r.native.observe_allocation()?;
            r.counts.as_mut().unwrap().acknowledge();
            Ok(())
        })();
        self.finish(result)
    }
    pub(crate) fn count_change(
        &mut self,
        scope: &CanonicalScope,
        attempt: CountAttempt,
    ) -> StorageResult<()> {
        self.count_context(scope)?;
        let result = (|| {
            let r = self.resource.as_mut().unwrap();
            r.verify()?;
            r.canonical_growth(
                Some(if attempt.after.stage >= 5 {
                    attempt.after.count_remaining
                } else {
                    attempt.after.records
                }),
                Some(if attempt.after.stage >= 5 {
                    attempt.after.zero_remaining
                } else {
                    attempt.after.zeros
                }),
                None,
                None,
            )?;
            r.counts.as_mut().unwrap().attempt = Some(attempt);
            r.native.reserve()?;
            count_mutation::commit(
                r.connection.as_ref().unwrap(),
                r.engine,
                r.counts.as_ref().unwrap(),
            )?;
            r.native.observe_allocation()?;
            r.counts.as_mut().unwrap().acknowledge();
            Ok(())
        })();
        self.finish(result)
    }
    /// Read the exact selected mutable or immutable Count row without insertion.
    /// Retired, foreign, failed or pending ownership cannot supply a point answer.
    pub fn count_get(
        &mut self,
        scope: &CanonicalScope,
        serial: u64,
    ) -> StorageResult<Option<CountRecord>> {
        self.count_context(scope)?;
        self.finish((|| {
            let r = self.count_context(scope)?;
            let state = r
                .counts
                .as_ref()
                .ok_or(StorageError::Integrity("count not begun"))?;
            if !(1..=4).contains(&state.snapshot.stage) {
                return Err(StorageError::Integrity("count point phase"));
            }
            count_index::get(r.verify()?, scope, serial)
        })())
    }
    /// Compare the complete actual before row and admit one proposed Count change.
    /// Preserve its variant and monotone touched membership. New declarations are
    /// initial-epoch untouched Count rows; new Effect rows must be touched.
    /// Keep complete before/proposed custody through an uncertain acknowledgement.
    pub fn count_cas(
        &mut self,
        scope: &CanonicalScope,
        before: Option<CountRecord>,
        after: &CountRecord,
    ) -> StorageResult<CountRecord> {
        self.count_context(scope)?;
        let result = (|| {
            let r = self.count_context(scope)?;
            let state = r
                .counts
                .as_ref()
                .ok_or(StorageError::Integrity("count not begun"))?;
            if !matches!(state.snapshot.stage, 1 | 3) {
                return Err(StorageError::Integrity("count mutable epoch ended"));
            }
            let key = scope.key(after.serial())?;
            CountRecord::decode(scope, &key, &after.encode_value()?)?;
            if before.is_some_and(|old| {
                old.serial() != after.serial()
                    || (old.touched && !after.touched)
                    || std::mem::discriminant(&old.row) != std::mem::discriminant(&after.row)
            }) || (before.is_none()
                && match after.row {
                    Row::Count { .. } => state.snapshot.stage != 1 || after.touched,
                    Row::Effect { .. } => !after.touched,
                })
            {
                return Err(StorageError::Integrity("count CAS variant/touched"));
            }
            if count_index::get(r.verify()?, scope, after.serial())? != before {
                return Err(StorageError::Integrity("count exact CAS before"));
            }
            let mut a = CountAttempt::new(state, "CAS", 1, 0)?;
            a.after.records = a
                .after
                .records
                .checked_add(u64::from(before.is_none()))
                .ok_or(StorageError::Integrity("count records overflow"))?;
            a.after.touched = a
                .after
                .touched
                .checked_add(u64::from(
                    after.touched && before.is_none_or(|r| !r.touched),
                ))
                .ok_or(StorageError::Integrity("count touched overflow"))?;
            a.after.maximum = Some(
                a.after
                    .maximum
                    .map_or(after.serial(), |v| v.max(after.serial())),
            );
            a.rows.push((before, Some(*after)));
            self.count_change(scope, a)?;
            Ok(*after)
        })();
        self.finish(result)
    }
    /// Mark only this selected Count owner failed without cleanup or refund.
    /// Foreign scope refusal leaves the actual current owner untouched.
    pub fn count_abandon(&mut self, scope: &CanonicalScope) -> StorageResult<()> {
        self.count_context(scope)?;
        self.resource
            .as_ref()
            .unwrap()
            .counts
            .as_ref()
            .ok_or(StorageError::Integrity("count not begun"))?
            .failed
            .set(true);
        self.finish(Ok(()))
    }
}
