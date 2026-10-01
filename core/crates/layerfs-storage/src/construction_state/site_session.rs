//! The supplied C1 site port and one exact v3 logical/native phase lifecycle.

use layerfs_content::filesystem::state::{
    ClaimAdmission, SiteBirthSeal, SiteCapacity, SiteKey, SiteMembership, SiteObservation,
    SiteRecord, SiteScope, SiteSeal,
};
use layerfs_content::ContentError;

use crate::error::{StorageError, StorageResult};

use super::session::Resource;
use super::sites::{SiteAttempt, SiteAttemptKind, SiteStage};
use super::{site_index, site_lifecycle, site_mutation, ScratchSession};

impl ScratchSession {
    pub(crate) fn ensure_site_scope(&self, scope: &SiteScope) -> StorageResult<()> {
        let state = self
            .resource
            .as_ref()
            .and_then(|resource| resource.sites.as_ref());
        if state.is_none_or(|state| &state.scope != scope) {
            return Err(StorageError::Content(ContentError::InvalidOrderingRecord(
                "site foreign scope",
            )));
        }
        Ok(())
    }

    pub(crate) fn site_resource(&self) -> StorageResult<&Resource> {
        let resource = self
            .resource
            .as_ref()
            .ok_or(StorageError::Integrity("construction scratch released"))?;
        resource.check_live()?;
        if resource.sites.is_none() || resource.release_attempted || resource.unknown.get() {
            return Err(StorageError::Integrity(
                "construction scratch site owner unavailable",
            ));
        }
        Ok(resource)
    }

    fn site_resource_mut(&mut self) -> StorageResult<&mut Resource> {
        self.site_resource()?;
        Ok(self.resource.as_mut().unwrap())
    }

    /// Exact selected logical terminalization, without native/SQL observation.
    /// A foreign operation/source never terminalizes this owner. Repeated own
    /// abandonment is metadata-only, including an already quarantined attempt.
    pub fn site_abandon(&mut self, scope: &SiteScope) -> StorageResult<()> {
        self.ensure_site_scope(scope)?;
        self.resource
            .as_ref()
            .unwrap()
            .sites
            .as_ref()
            .unwrap()
            .failed
            .set(true);
        self.finish(Ok(()))
    }

    /// Declared site rows and complete frame60 bytes before producer effects.
    pub fn site_capacity(&self, scope: &SiteScope) -> StorageResult<SiteCapacity> {
        self.ensure_site_scope(scope)?;
        self.finish((|| {
            let resource = self.site_resource()?;
            let state = resource.sites.as_ref().unwrap();
            state.check(scope, SiteStage::BirthOpen)?;
            resource.verify()?;
            Ok(SiteCapacity::new(
                state.declared_sites,
                state.declared_sites * 60,
            )?)
        })())
    }

    /// One indexed checked immutable/current point; no name or row collection.
    pub fn site_get(
        &mut self,
        scope: &SiteScope,
        key: SiteKey,
    ) -> StorageResult<Option<SiteRecord>> {
        self.ensure_site_scope(scope)?;
        self.finish((|| {
            let resource = self.site_resource()?;
            let state = resource.sites.as_ref().unwrap();
            if !matches!(state.stage, SiteStage::BirthOpen | SiteStage::Facts) || state.failed.get()
            {
                return Err(StorageError::Integrity(
                    "construction scratch readable site phase",
                ));
            }
            SiteKey::decode(scope, key.as_bytes())?;
            let record = site_index::get(resource.verify()?, scope, key)?;
            if state.stage == SiteStage::BirthOpen
                && record.is_some_and(|record| !record.is_birth())
            {
                return Err(StorageError::Integrity(
                    "construction scratch premature site facts",
                ));
            }
            Ok(record)
        })())
    }

    /// One unordered128 birth window. A Duplicate terminalizes and acknowledges
    /// no current row; all previously acknowledged rows keep their exact owner.
    pub fn site_insert_batch(
        &mut self,
        scope: &SiteScope,
        records: &[SiteRecord],
    ) -> StorageResult<ClaimAdmission> {
        self.ensure_site_scope(scope)?;
        let result = (|| {
            let resource = self.site_resource_mut()?;
            resource
                .sites
                .as_ref()
                .unwrap()
                .check(scope, SiteStage::BirthOpen)?;
            check_window(records.len())?;
            let mut keys = [0; 128];
            let mut points = [(0, 0); 128];
            for (index, record) in records.iter().enumerate() {
                SiteRecord::decode(scope, &record.encode())?;
                if !record.is_birth() {
                    return Err(StorageError::Integrity(
                        "construction scratch site birth flags",
                    ));
                }
                keys[index] = record.key().serial();
                points[index] = (record.point().parent(), record.point().binding_ordinal());
            }
            let state = resource.sites.as_ref().unwrap();
            let count = state
                .records
                .checked_add(records.len() as u64)
                .ok_or(StorageError::Integrity("construction scratch site count"))?;
            if count > state.declared_sites {
                return Err(StorageError::CapacityExceeded {
                    what: "construction scratch site rows",
                    limit: state.declared_sites,
                    actual: count,
                });
            }
            let bytes = count.checked_mul(60).ok_or(StorageError::Integrity(
                "construction scratch site record bytes",
            ))?;
            if bytes > state.declared_sites * 60 {
                return Err(StorageError::CapacityExceeded {
                    what: "construction scratch site record bytes",
                    limit: state.declared_sites * 60,
                    actual: bytes,
                });
            }
            resource.namespace_sites_growth(count)?;
            keys[..records.len()].sort_unstable();
            points[..records.len()].sort_unstable();
            resource.verify()?;
            let state = resource.sites.as_mut().unwrap();
            let mut attempt = SiteAttempt::new(SiteAttemptKind::Birth, state.records, count);
            for record in records {
                attempt.include(*record, record.flags());
            }
            state.attempt = Some(attempt);
            let local_duplicate = keys[..records.len()]
                .windows(2)
                .any(|pair| pair[0] == pair[1])
                || points[..records.len()]
                    .windows(2)
                    .any(|pair| pair[0] == pair[1]);
            resource.native.reserve()?;
            let outcome = site_mutation::insert(
                resource.connection.as_ref().unwrap(),
                resource.engine,
                resource.sites.as_ref().unwrap(),
                records,
                local_duplicate,
            )?;
            resource.native.observe_allocation()?;
            let state = resource.sites.as_mut().unwrap();
            match outcome {
                ClaimAdmission::Fresh => {
                    state.records += records.len() as u64;
                    state.attempt = None;
                }
                ClaimAdmission::Duplicate => state.failed.set(true),
            }
            Ok(outcome)
        })();
        self.finish(result)
    }

    /// Compare actual indexed source-order membership with the acknowledged
    /// caller transcript before admitting any mutable base facts.
    pub fn site_close_membership(
        &mut self,
        expected: &SiteBirthSeal,
    ) -> StorageResult<SiteMembership> {
        self.ensure_site_scope(expected.scope())?;
        let result = (|| {
            let resource = self.site_resource_mut()?;
            let state = resource.sites.as_ref().unwrap();
            state.check(expected.scope(), SiteStage::BirthOpen)?;
            if expected.records() != state.records || expected.encoded_bytes() != state.records * 60
            {
                return Err(StorageError::Integrity(
                    "construction scratch expected site birth counts",
                ));
            }
            resource.verify()?;
            let state = resource.sites.as_mut().unwrap();
            state.expected_birth = Some(expected.clone());
            state.attempt = Some(SiteAttempt::new(
                SiteAttemptKind::CloseMembership,
                state.records,
                state.records,
            ));
            resource.native.reserve()?;
            let members = site_lifecycle::close(
                resource.connection.as_ref().unwrap(),
                resource.engine,
                resource.sites.as_mut().unwrap(),
                expected,
            )?;
            resource.native.observe_allocation()?;
            let state = resource.sites.as_mut().unwrap();
            state.stage = SiteStage::Facts;
            state.remaining = state.records;
            state.membership = Some(members.clone());
            state.attempt = None;
            state.expected_birth = None;
            state.proposed_membership = None;
            Ok(members)
        })();
        self.finish(result)
    }

    /// OR only the two monotone base-fact bits, with each old point/flags and
    /// proposed flags retained until the whole bounded transaction is known.
    pub fn site_observe_base_batch(
        &mut self,
        members: &SiteMembership,
        observations: &[SiteObservation],
    ) -> StorageResult<()> {
        self.ensure_site_scope(members.birth().scope())?;
        let result = (|| {
            let resource = self.site_resource_mut()?;
            resource.sites.as_ref().unwrap().check_membership(members)?;
            check_window(observations.len())?;
            for observation in observations {
                SiteObservation::decode(members.birth().scope(), &observation.encode())?;
            }
            resource.verify()?;
            let state = resource.sites.as_mut().unwrap();
            let mut attempt =
                SiteAttempt::new(SiteAttemptKind::Observe, state.records, state.records);
            for (slot, observation) in attempt.observations.iter_mut().zip(observations) {
                *slot = Some(*observation);
            }
            attempt.observation_count = observations.len();
            state.attempt = Some(attempt);
            resource.native.reserve()?;
            site_mutation::observe(
                resource.connection.as_ref().unwrap(),
                resource.engine,
                resource.sites.as_mut().unwrap(),
                observations,
            )?;
            resource.native.observe_allocation()?;
            resource.sites.as_mut().unwrap().attempt = None;
            Ok(())
        })();
        self.finish(result)
    }

    /// Rehash immutable birth projection and full key-order current facts in
    /// one transaction, then retain the exact acknowledged final seal.
    pub fn site_final_seal(&mut self, members: &SiteMembership) -> StorageResult<SiteSeal> {
        self.ensure_site_scope(members.birth().scope())?;
        let result = (|| {
            let resource = self.site_resource_mut()?;
            resource.sites.as_ref().unwrap().check_membership(members)?;
            resource.verify()?;
            let state = resource.sites.as_mut().unwrap();
            state.attempt = Some(SiteAttempt::new(
                SiteAttemptKind::FinalSeal,
                state.records,
                state.records,
            ));
            resource.native.reserve()?;
            let seal = site_lifecycle::seal(
                resource.connection.as_ref().unwrap(),
                resource.engine,
                resource.sites.as_mut().unwrap(),
            )?;
            resource.native.observe_allocation()?;
            let state = resource.sites.as_mut().unwrap();
            state.stage = SiteStage::FinalSealed;
            state.seal = Some(seal.clone());
            state.proposed_seal = None;
            state.attempt = None;
            Ok(seal)
        })();
        self.finish(result)
    }

    /// Delete exact sealed keys in128-row transactions. SQL retirement plus
    /// actual native allocation observation both precede root permission.
    pub fn site_retire(&mut self, seal: &SiteSeal) -> StorageResult<()> {
        self.ensure_site_scope(seal.scope())?;
        let result = (|| {
            let resource = self.site_resource_mut()?;
            resource.sites.as_ref().unwrap().check_seal(seal)?;
            loop {
                resource.verify()?;
                resource.native.reserve()?;
                let outcome = site_lifecycle::retire_window(
                    resource.connection.as_ref().unwrap(),
                    resource.engine,
                    resource.sites.as_mut().unwrap(),
                )?;
                resource.native.observe_allocation()?;
                let state = resource.sites.as_mut().unwrap();
                state.stage = outcome.stage;
                state.remaining = outcome.remaining;
                state.after = outcome.after;
                state.attempt = None;
                if state.stage == SiteStage::Retired {
                    return Ok(());
                }
            }
        })();
        self.finish(result)
    }
}

fn check_window(count: usize) -> StorageResult<()> {
    if count > 128 {
        return Err(StorageError::CapacityExceeded {
            what: "construction scratch site batch rows",
            limit: 128,
            actual: count as u64,
        });
    }
    // The widest accepted frame is60;96 +128*60 is within the unchanged64KiB.
    Ok(())
}
