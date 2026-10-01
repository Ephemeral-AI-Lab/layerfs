//! Exact zero Sites and Alias transitions; no nonempty authority is implied.
use super::empty_owner::{bad, growth};
use super::*;
use crate::ContentResult;
impl BindingSiteState for VerifiedEmptyState {
    fn site_capacity(&self, scope: &SiteScope) -> ContentResult<SiteCapacity> {
        self.site_scope(scope)?;
        if self.data.sites_retired {
            return Err(bad("empty Sites retired"));
        }
        SiteCapacity::new(0, 0)
    }
    fn site_get(&mut self, scope: &SiteScope, key: SiteKey) -> ContentResult<Option<SiteRecord>> {
        self.site_scope(scope)?;
        SiteKey::decode(scope, key.as_bytes())?;
        if self.data.sites_retired {
            return Err(bad("empty Sites retired"));
        }
        Ok(None)
    }
    fn site_insert_batch(
        &mut self,
        scope: &SiteScope,
        records: &[SiteRecord],
    ) -> ContentResult<ClaimAdmission> {
        self.site_scope(scope)?;
        growth(records.len())?;
        if self.data.sites_closed {
            return Err(bad("empty Sites births closed"));
        }
        Ok(ClaimAdmission::Fresh)
    }
    fn site_close_membership(&mut self, expected: &SiteBirthSeal) -> ContentResult<SiteMembership> {
        self.site_scope(expected.scope())?;
        if self.data.sites_closed {
            return Err(bad("empty membership replay"));
        }
        let actual = SiteBirthLedger::new(expected.scope().clone())?.seal();
        if &actual != expected {
            return Err(bad("empty birth transcript"));
        }
        let members = SiteMembership::new(actual, None)?;
        self.data.sites_closed = true;
        self.data.members = Some(members.clone());
        Ok(members)
    }
    fn site_parent_present(
        &mut self,
        members: &SiteMembership,
        parent: u64,
    ) -> ContentResult<bool> {
        self.members(members)?;
        SiteKey::new(members.birth().scope(), parent)?;
        Ok(false)
    }
    fn site_parent_page(
        &mut self,
        members: &SiteMembership,
        parent: u64,
        after: Option<u32>,
        limit: SiteParentPageLimit,
    ) -> ContentResult<SiteParentPage> {
        self.members(members)?;
        if after.is_some() {
            return Err(bad("empty parent continuation"));
        }
        limit.check_records(0)?;
        SiteParentPage::new(members.clone(), parent, None, Vec::new(), true)
    }
    fn site_observe_base_batch(
        &mut self,
        members: &SiteMembership,
        observations: &[SiteObservation],
    ) -> ContentResult<()> {
        self.members(members)?;
        growth(observations.len())
    }
    fn site_final_seal(&mut self, members: &SiteMembership) -> ContentResult<SiteSeal> {
        self.members(members)?;
        if !self.data.alias_retired || self.data.site_seal.is_some() {
            return Err(bad("empty Site seal phase/replay"));
        }
        let seal = SiteLedger::new(members.birth().scope().clone())?.seal();
        self.data.site_seal = Some(seal.clone());
        Ok(seal)
    }
    fn site_sealed_page(
        &mut self,
        seal: &SiteSeal,
        after: Option<SiteKey>,
        limit: SitePageLimit,
    ) -> ContentResult<SitePage> {
        self.site_scope(seal.scope())?;
        if self.data.site_seal.as_ref() != Some(seal) || self.data.sites_retired || after.is_some()
        {
            return Err(bad("empty Site selected EOF"));
        }
        limit.check_records(0)?;
        let page = SitePage::new(seal.clone(), Vec::new(), true)?;
        self.data.site_eof = true;
        Ok(page)
    }
    fn site_retire(&mut self, seal: &SiteSeal) -> ContentResult<()> {
        self.site_scope(seal.scope())?;
        if self.data.site_seal.as_ref() != Some(seal)
            || !self.data.site_eof
            || self.data.sites_retired
        {
            return Err(bad("empty Sites retirement"));
        }
        self.data.sites_retired = true;
        Ok(())
    }
    fn site_abandon(&mut self, scope: &SiteScope) -> ContentResult<()> {
        if scope != self.data.scopes.sites() {
            return Err(bad("empty foreign Sites"));
        }
        self.abandon();
        Ok(())
    }
}
impl AliasFrontier for VerifiedEmptyState {
    fn alias_capacity(&self, members: &SiteMembership) -> ContentResult<AliasCapacity> {
        self.members(members)?;
        Ok(AliasCapacity::verified_empty())
    }
    fn alias_begin(&mut self, members: &SiteMembership, root: Option<u64>) -> ContentResult<()> {
        self.members(members)?;
        growth(usize::from(root.is_some()))?;
        if self.data.alias_begun {
            return Err(bad("empty Alias begin replay"));
        }
        self.data.alias_begun = true;
        Ok(())
    }
    fn alias_enqueue(&mut self, members: &SiteMembership, children: &[u64]) -> ContentResult<()> {
        self.members(members)?;
        growth(children.len())?;
        if !self.data.alias_begun || self.data.alias_seal.is_some() {
            return Err(bad("empty Alias enqueue phase"));
        }
        Ok(())
    }
    fn alias_take(&mut self, members: &SiteMembership) -> ContentResult<Option<AliasCurrent>> {
        self.members(members)?;
        if !self.data.alias_begun || self.data.alias_seal.is_some() {
            return Err(bad("empty Alias selection phase"));
        }
        Ok(None)
    }
    fn alias_advance(
        &mut self,
        members: &SiteMembership,
        _current: AliasCurrent,
        _before: &AliasProgress,
        _after: &AliasProgress,
    ) -> ContentResult<()> {
        self.members(members)?;
        growth(1)
    }
    fn alias_complete(
        &mut self,
        members: &SiteMembership,
        _current: AliasCurrent,
    ) -> ContentResult<()> {
        self.members(members)?;
        growth(1)
    }
    fn alias_finish(&mut self, members: &SiteMembership) -> ContentResult<AliasSeal> {
        self.members(members)?;
        if !self.data.alias_begun || self.data.alias_seal.is_some() {
            return Err(bad("empty Alias finish phase/replay"));
        }
        let seal = AliasSeal {
            members: members.clone(),
            records: 0,
            sequence: 0,
            maximum: None,
        };
        self.data.alias_seal = Some(seal.clone());
        Ok(seal)
    }
    fn alias_retire(&mut self, seal: &AliasSeal) -> ContentResult<()> {
        self.members(&seal.members)?;
        if self.data.alias_seal.as_ref() != Some(seal) || self.data.alias_retired {
            return Err(bad("empty Alias retirement"));
        }
        self.data.alias_retired = true;
        Ok(())
    }
    fn alias_abandon(&mut self, members: &SiteMembership) -> ContentResult<()> {
        if members.birth().scope() != self.data.scopes.sites() {
            return Err(bad("empty foreign Alias"));
        }
        self.abandon();
        Ok(())
    }
}
