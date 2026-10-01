//! Explicit legacy Sites plus resident alias authority, selected before effects.
use super::*;
use crate::ContentResult;

pub(crate) struct CompatibilityAliases<'a, S: BindingSiteState + ?Sized> {
    pub(crate) sites: &'a mut S,
    pub(crate) aliases: ResidentAliasFrontier,
}
impl<S: BindingSiteState + ?Sized> BindingSiteState for CompatibilityAliases<'_, S> {
    fn site_capacity(&self, s: &SiteScope) -> ContentResult<SiteCapacity> {
        self.sites.site_capacity(s)
    }
    fn site_get(&mut self, s: &SiteScope, k: SiteKey) -> ContentResult<Option<SiteRecord>> {
        self.sites.site_get(s, k)
    }
    fn site_insert_batch(
        &mut self,
        s: &SiteScope,
        r: &[SiteRecord],
    ) -> ContentResult<ClaimAdmission> {
        self.sites.site_insert_batch(s, r)
    }
    fn site_close_membership(&mut self, s: &SiteBirthSeal) -> ContentResult<SiteMembership> {
        self.sites.site_close_membership(s)
    }
    fn site_parent_present(&mut self, s: &SiteMembership, p: u64) -> ContentResult<bool> {
        self.sites.site_parent_present(s, p)
    }
    fn site_parent_page(
        &mut self,
        s: &SiteMembership,
        p: u64,
        a: Option<u32>,
        l: SiteParentPageLimit,
    ) -> ContentResult<SiteParentPage> {
        self.sites.site_parent_page(s, p, a, l)
    }
    fn site_observe_base_batch(
        &mut self,
        s: &SiteMembership,
        o: &[SiteObservation],
    ) -> ContentResult<()> {
        self.sites.site_observe_base_batch(s, o)
    }
    fn site_final_seal(&mut self, s: &SiteMembership) -> ContentResult<SiteSeal> {
        self.sites.site_final_seal(s)
    }
    fn site_sealed_page(
        &mut self,
        s: &SiteSeal,
        a: Option<SiteKey>,
        l: SitePageLimit,
    ) -> ContentResult<SitePage> {
        self.sites.site_sealed_page(s, a, l)
    }
    fn site_retire(&mut self, s: &SiteSeal) -> ContentResult<()> {
        self.sites.site_retire(s)
    }
    fn site_abandon(&mut self, s: &SiteScope) -> ContentResult<()> {
        self.sites.site_abandon(s)
    }
}
impl<S: BindingSiteState + ?Sized> AliasFrontier for CompatibilityAliases<'_, S> {
    fn alias_capacity(&self, m: &SiteMembership) -> ContentResult<AliasCapacity> {
        self.aliases.alias_capacity(m)
    }
    fn alias_begin(&mut self, m: &SiteMembership, r: Option<u64>) -> ContentResult<()> {
        self.aliases.alias_begin(m, r)
    }
    fn alias_enqueue(&mut self, m: &SiteMembership, c: &[u64]) -> ContentResult<()> {
        self.aliases.alias_enqueue(m, c)
    }
    fn alias_take(&mut self, m: &SiteMembership) -> ContentResult<Option<AliasCurrent>> {
        self.aliases.alias_take(m)
    }
    fn alias_advance(
        &mut self,
        m: &SiteMembership,
        c: AliasCurrent,
        b: &AliasProgress,
        a: &AliasProgress,
    ) -> ContentResult<()> {
        self.aliases.alias_advance(m, c, b, a)
    }
    fn alias_complete(&mut self, m: &SiteMembership, c: AliasCurrent) -> ContentResult<()> {
        self.aliases.alias_complete(m, c)
    }
    fn alias_finish(&mut self, m: &SiteMembership) -> ContentResult<AliasSeal> {
        self.aliases.alias_finish(m)
    }
    fn alias_retire(&mut self, s: &AliasSeal) -> ContentResult<()> {
        self.aliases.alias_retire(s)
    }
    fn alias_abandon(&mut self, m: &SiteMembership) -> ContentResult<()> {
        self.aliases.alias_abandon(m)
    }
}
