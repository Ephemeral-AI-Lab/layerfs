//! The real C1 claim port over one phased SQL/native ownership capsule.

use layerfs_content::filesystem::state::{
    ClaimAdmission, ClaimCapacity, ClaimKey, ClaimPage, ClaimPageLimit, ClaimSeal, StateScope,
};

use crate::error::{StorageError, StorageResult};

use super::phased::{AttemptKind, ClaimPhase};
use super::{claim_index, claim_lifecycle, ScratchSession};

impl ScratchSession {
    /// Close only this exact logical attempt, including failed/Unknown custody.
    /// No SQL/native observation, rollback or release is attempted. Foreign
    /// scopes leave the selected owner and original failure unchanged.
    pub fn claim_abandon(&mut self, scope: &StateScope) -> StorageResult<()> {
        let resource = self.resource.as_ref().ok_or(StorageError::Content(
            layerfs_content::ContentError::InvalidOrderingRecord("claim abandon owner"),
        ))?;
        let state = resource.phased.as_ref().ok_or(StorageError::Content(
            layerfs_content::ContentError::InvalidOrderingRecord("claim abandon owner"),
        ))?;
        if scope != &state.claims {
            return Err(StorageError::Content(
                layerfs_content::ContentError::InvalidOrderingRecord("claim abandon scope"),
            ));
        }
        state.failed.set(true);
        self.finish(Ok(()))
    }

    /// Exact declared exclusive-claim class under the currently open scope.
    pub fn claim_capacity(&self, scope: &StateScope) -> StorageResult<ClaimCapacity> {
        self.finish((|| {
            let resource = self.claim_resource()?;
            let state = resource.phased.as_ref().unwrap();
            state.check_claims(scope, ClaimPhase::Open)?;
            resource.verify()?;
            Ok(ClaimCapacity::new(
                state.declared_claims,
                state.declared_claims * 32,
            )?)
        })())
    }

    /// Immediate exact acknowledged presence preserves C1 verdict precedence.
    pub fn claim_present(&mut self, scope: &StateScope, key: ClaimKey) -> StorageResult<bool> {
        self.finish((|| {
            let resource = self.claim_resource()?;
            let state = resource.phased.as_ref().unwrap();
            state.check_claims(scope, ClaimPhase::Open)?;
            ClaimKey::decode(scope, key.as_bytes())?;
            claim_index::present(resource.verify()?, key)
        })())
    }

    /// One globally unordered128-key window; Duplicate acknowledges no row.
    pub fn claim_batch(
        &mut self,
        scope: &StateScope,
        keys: &[ClaimKey],
    ) -> StorageResult<ClaimAdmission> {
        let result = (|| {
            let resource = self.claim_resource_mut()?;
            resource
                .phased
                .as_ref()
                .unwrap()
                .check_claims(scope, ClaimPhase::Open)?;
            if keys.len() > 128 {
                return Err(StorageError::CapacityExceeded {
                    what: "construction scratch claim batch rows",
                    limit: 128,
                    actual: keys.len() as u64,
                });
            }
            let mut serials = [0; 128];
            for (index, key) in keys.iter().enumerate() {
                ClaimKey::decode(scope, key.as_bytes())?;
                serials[index] = key.serial();
            }
            serials[..keys.len()].sort_unstable();
            resource.verify()?;
            let state = resource.phased.as_mut().unwrap();
            state.record_attempt(AttemptKind::Batch, keys, state.records + keys.len() as u64);
            if serials[..keys.len()]
                .windows(2)
                .any(|pair| pair[0] == pair[1])
            {
                state.failed.set(true);
                return Ok(ClaimAdmission::Duplicate);
            }
            resource.native.reserve()?;
            let outcome = claim_index::batch(
                resource.connection.as_ref().unwrap(),
                resource.phased.as_ref().unwrap(),
                keys,
            )?;
            resource.native.observe_allocation()?;
            let state = resource.phased.as_mut().unwrap();
            match outcome {
                ClaimAdmission::Fresh => {
                    state.records += keys.len() as u64;
                    state.attempt = None;
                }
                ClaimAdmission::Duplicate => state.failed.set(true),
            }
            Ok(outcome)
        })();
        self.finish(result)
    }

    /// One transaction selects the exact MAX, ordered digest and terminal seal.
    pub fn claim_seal(&mut self, scope: &StateScope) -> StorageResult<ClaimSeal> {
        let result = (|| {
            let resource = self.claim_resource_mut()?;
            resource
                .phased
                .as_ref()
                .unwrap()
                .check_claims(scope, ClaimPhase::Open)?;
            resource.verify()?;
            resource.native.reserve()?;
            let state = resource.phased.as_mut().unwrap();
            state.record_attempt(AttemptKind::Seal, &[], state.records);
            let (seal, maximum) =
                claim_lifecycle::seal(resource.connection.as_ref().unwrap(), state)?;
            resource.native.observe_allocation()?;
            let state = resource.phased.as_mut().unwrap();
            state.phase = ClaimPhase::Sealed;
            state.remaining = state.records;
            state.maximum = maximum;
            state.seal = Some(seal.clone());
            state.proposed_seal = None;
            state.attempt = None;
            Ok(seal)
        })();
        self.finish(result)
    }

    /// PK range continuation; exact acknowledged MAX supplies EOF without rank.
    pub fn claim_page(
        &mut self,
        seal: &ClaimSeal,
        after: Option<ClaimKey>,
        limit: ClaimPageLimit,
    ) -> StorageResult<ClaimPage> {
        let result = (|| {
            let resource = self.claim_resource()?;
            let state = resource.phased.as_ref().unwrap();
            state.check_seal(seal, ClaimPhase::Sealed)?;
            if let Some(key) = after {
                ClaimKey::decode(seal.scope(), key.as_bytes())?;
                if state.maximum.is_none_or(|maximum| key > maximum) {
                    return Err(StorageError::Integrity(
                        "construction scratch claim continuation",
                    ));
                }
            }
            let connection = resource.verify()?;
            let count = if state.maximum == after {
                0
            } else {
                limit.fitting_records()
            };
            if state.maximum != after && count == 0 {
                limit.check_records(1)?;
            }
            let rows = claim_index::read(connection, state, after, count)?;
            let last = rows.last().map(|record| record.key()).or(after);
            let eof = last == state.maximum;
            if last.is_some_and(|key| state.maximum.is_none_or(|maximum| key > maximum))
                || (!eof && (rows.is_empty() || rows.len() != count))
            {
                return Err(StorageError::Integrity(
                    "construction scratch claim page EOF",
                ));
            }
            let page = ClaimPage::after(seal.clone(), after, rows, eof)?;
            page.check_limit(limit)?;
            Ok(page)
        })();
        self.finish(result)
    }

    /// Bounded exact-seal retirement retains the full native class throughout.
    pub fn claim_retire(&mut self, seal: &ClaimSeal) -> StorageResult<()> {
        let result = (|| {
            let resource = self.claim_resource_mut()?;
            resource
                .phased
                .as_ref()
                .unwrap()
                .check_seal(seal, ClaimPhase::Sealed)?;
            loop {
                resource.verify()?;
                resource.native.reserve()?;
                let outcome = claim_lifecycle::retire_window(
                    resource.connection.as_ref().unwrap(),
                    resource.phased.as_mut().unwrap(),
                )?;
                // No in-memory Retired permission is issued before both the SQL
                // acknowledgement and actual native allocation observation.
                resource.native.observe_allocation()?;
                let state = resource.phased.as_mut().unwrap();
                state.remaining = outcome.remaining;
                state.after = outcome.after;
                state.phase = outcome.phase;
                state.attempt = None;
                if state.phase == ClaimPhase::Retired {
                    return Ok(());
                }
            }
        })();
        self.finish(result)
    }

    fn claim_resource(&self) -> StorageResult<&super::session::Resource> {
        let resource = self
            .resource
            .as_ref()
            .ok_or(StorageError::Integrity("construction scratch released"))?;
        if resource.phased.is_none() || resource.release_attempted || resource.unknown.get() {
            return Err(StorageError::Integrity(
                "construction scratch phased owner unavailable",
            ));
        }
        Ok(resource)
    }

    fn claim_resource_mut(&mut self) -> StorageResult<&mut super::session::Resource> {
        self.claim_resource()?;
        Ok(self.resource.as_mut().unwrap())
    }
}
