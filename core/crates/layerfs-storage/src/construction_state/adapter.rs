//! C1 IndexedState adapter that preserves the original typed C2 failure.

use layerfs_content::filesystem::state::{
    BindingClaimState, BindingSiteState, ClaimAdmission, ClaimCapacity, ClaimKey, ClaimPage,
    ClaimPageLimit, ClaimSeal, IndexedState, PageLimit, SiteBirthSeal, SiteCapacity, SiteKey,
    SiteMembership, SiteObservation, SitePage, SitePageLimit, SiteParentPage, SiteParentPageLimit,
    SiteRecord, SiteScope, SiteSeal, StateCapacity, StateKey, StatePage, StateRecord, StateScope,
    StateSeal,
};
use layerfs_content::ContentResult;

use super::ScratchSession;

/// Borrowed C1 port for one exact private C2 session.
pub struct ScratchAdapter<'a> {
    pub(super) session: &'a mut ScratchSession,
}

impl ScratchAdapter<'_> {
    fn site_failure(&self, error: crate::StorageError) -> layerfs_content::ContentError {
        match error {
            crate::StorageError::Content(
                error @ layerfs_content::ContentError::InvalidOrderingRecord("site foreign scope"),
            ) => error,
            error => self.session.keep_failure(error),
        }
    }
}

impl BindingSiteState for ScratchAdapter<'_> {
    fn site_capacity(&self, scope: &SiteScope) -> ContentResult<SiteCapacity> {
        self.session
            .site_capacity(scope)
            .map_err(|error| self.site_failure(error))
    }
    fn site_get(&mut self, scope: &SiteScope, key: SiteKey) -> ContentResult<Option<SiteRecord>> {
        self.session
            .site_get(scope, key)
            .map_err(|error| self.site_failure(error))
    }
    fn site_insert_batch(
        &mut self,
        scope: &SiteScope,
        records: &[SiteRecord],
    ) -> ContentResult<ClaimAdmission> {
        self.session
            .site_insert_batch(scope, records)
            .map_err(|error| self.site_failure(error))
    }
    fn site_close_membership(&mut self, expected: &SiteBirthSeal) -> ContentResult<SiteMembership> {
        self.session
            .site_close_membership(expected)
            .map_err(|error| self.site_failure(error))
    }
    fn site_parent_present(
        &mut self,
        members: &SiteMembership,
        parent: u64,
    ) -> ContentResult<bool> {
        self.session
            .site_parent_present(members, parent)
            .map_err(|error| self.site_failure(error))
    }
    fn site_parent_page(
        &mut self,
        members: &SiteMembership,
        parent: u64,
        after: Option<u32>,
        limit: SiteParentPageLimit,
    ) -> ContentResult<SiteParentPage> {
        self.session
            .site_parent_page(members, parent, after, limit)
            .map_err(|error| self.site_failure(error))
    }
    fn site_observe_base_batch(
        &mut self,
        members: &SiteMembership,
        observations: &[SiteObservation],
    ) -> ContentResult<()> {
        self.session
            .site_observe_base_batch(members, observations)
            .map_err(|error| self.site_failure(error))
    }
    fn site_final_seal(&mut self, members: &SiteMembership) -> ContentResult<SiteSeal> {
        self.session
            .site_final_seal(members)
            .map_err(|error| self.site_failure(error))
    }
    fn site_sealed_page(
        &mut self,
        seal: &SiteSeal,
        after: Option<SiteKey>,
        limit: SitePageLimit,
    ) -> ContentResult<SitePage> {
        self.session
            .site_sealed_page(seal, after, limit)
            .map_err(|error| self.site_failure(error))
    }
    fn site_retire(&mut self, seal: &SiteSeal) -> ContentResult<()> {
        self.session
            .site_retire(seal)
            .map_err(|error| self.site_failure(error))
    }
    fn site_abandon(&mut self, scope: &SiteScope) -> ContentResult<()> {
        self.session
            .site_abandon(scope)
            .map_err(|error| self.site_failure(error))
    }
}

impl BindingClaimState for ScratchAdapter<'_> {
    fn claim_abandon(&mut self, scope: &StateScope) -> ContentResult<()> {
        match self.session.claim_abandon(scope) {
            Ok(()) => Ok(()),
            Err(crate::StorageError::Content(error)) => Err(error),
            Err(error) => Err(self.session.keep_failure(error)),
        }
    }
    fn claim_capacity(&self, scope: &StateScope) -> ContentResult<ClaimCapacity> {
        self.session
            .claim_capacity(scope)
            .map_err(|error| self.session.keep_failure(error))
    }
    fn claim_present(&mut self, scope: &StateScope, key: ClaimKey) -> ContentResult<bool> {
        self.session
            .claim_present(scope, key)
            .map_err(|error| self.session.keep_failure(error))
    }
    fn claim_batch(
        &mut self,
        scope: &StateScope,
        keys: &[ClaimKey],
    ) -> ContentResult<ClaimAdmission> {
        self.session
            .claim_batch(scope, keys)
            .map_err(|error| self.session.keep_failure(error))
    }
    fn claim_seal(&mut self, scope: &StateScope) -> ContentResult<ClaimSeal> {
        self.session
            .claim_seal(scope)
            .map_err(|error| self.session.keep_failure(error))
    }
    fn claim_page(
        &mut self,
        seal: &ClaimSeal,
        after: Option<ClaimKey>,
        limit: ClaimPageLimit,
    ) -> ContentResult<ClaimPage> {
        self.session
            .claim_page(seal, after, limit)
            .map_err(|error| self.session.keep_failure(error))
    }
    fn claim_retire(&mut self, seal: &ClaimSeal) -> ContentResult<()> {
        self.session
            .claim_retire(seal)
            .map_err(|error| self.session.keep_failure(error))
    }
}

impl ScratchSession {
    /// Supplies the same selected C2 state to the real C1 producer/consumer.
    pub fn adapter(&mut self) -> ScratchAdapter<'_> {
        ScratchAdapter { session: self }
    }
}

impl IndexedState for ScratchAdapter<'_> {
    fn capacity(&self, scope: &StateScope) -> ContentResult<StateCapacity> {
        self.session
            .capacity(scope)
            .map_err(|error| self.session.keep_failure(error))
    }
    fn append(&mut self, scope: &StateScope, records: &[StateRecord]) -> ContentResult<()> {
        self.session
            .append(scope, records)
            .map_err(|error| self.session.keep_failure(error))
    }
    fn seal(&mut self, scope: &StateScope) -> ContentResult<StateSeal> {
        self.session
            .seal(scope)
            .map_err(|error| self.session.keep_failure(error))
    }
    fn get(&mut self, seal: &StateSeal, key: StateKey) -> ContentResult<Option<StateRecord>> {
        self.session
            .get(seal, key)
            .map_err(|error| self.session.keep_failure(error))
    }
    fn page(
        &mut self,
        seal: &StateSeal,
        after: Option<StateKey>,
        limit: PageLimit,
    ) -> ContentResult<StatePage> {
        self.session
            .page(seal, after, limit)
            .map_err(|error| self.session.keep_failure(error))
    }
    fn release(&mut self, scope: &StateScope) -> ContentResult<()> {
        self.session
            .complete_phase(scope)
            .map_err(|error| self.session.keep_failure(error))
    }
}
