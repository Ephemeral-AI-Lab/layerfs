//! Closed exclusive sites, immutable membership, monotone facts and retirement.
use super::{
    ClaimAdmission, SiteBirthSeal, SiteCapacity, SiteKey, SiteMembership, SiteObservation,
    SitePage, SitePageLimit, SiteParentPage, SiteParentPageLimit, SiteRecord, SiteScope, SiteSeal,
};
use crate::error::ContentResult;
/// One selected native/source authority. C1 owns no SQL or native I/O.
pub trait BindingSiteState {
    /// Previously admitted logical class, before site mutation.
    fn site_capacity(&self, scope: &SiteScope) -> ContentResult<SiteCapacity>;
    /// Exact selected child, both during births and the acknowledged fact phase.
    fn site_get(&mut self, scope: &SiteScope, key: SiteKey) -> ContentResult<Option<SiteRecord>>;
    /// Unordered<=128 births: known duplicate terminalizes with no additions.
    fn site_insert_batch(
        &mut self,
        scope: &SiteScope,
        records: &[SiteRecord],
    ) -> ContentResult<ClaimAdmission>;
    /// Checks exact acknowledged source-order birth digest before facts.
    fn site_close_membership(&mut self, expected: &SiteBirthSeal) -> ContentResult<SiteMembership>;
    /// Indexed existing-parent presence under immutable approved membership.
    fn site_parent_present(&mut self, members: &SiteMembership, parent: u64)
        -> ContentResult<bool>;
    /// Immutable birth projection, in selected parent's ordinal order.
    fn site_parent_page(
        &mut self,
        members: &SiteMembership,
        parent: u64,
        after: Option<u32>,
        limit: SiteParentPageLimit,
    ) -> ContentResult<SiteParentPage>;
    /// Monotone selected key/legal observations only; no point or base-bit changes.
    fn site_observe_base_batch(
        &mut self,
        members: &SiteMembership,
        observations: &[SiteObservation],
    ) -> ContentResult<()>;
    /// Revalidates approved immutable births, then seals full current key order.
    fn site_final_seal(&mut self, members: &SiteMembership) -> ContentResult<SiteSeal>;
    /// One exact bounded current-record page under the final acknowledged seal.
    fn site_sealed_page(
        &mut self,
        seal: &SiteSeal,
        after: Option<SiteKey>,
        limit: SitePageLimit,
    ) -> ContentResult<SitePage>;
    /// Known exact retirement of the primary and both projections before roots.
    fn site_retire(&mut self, seal: &SiteSeal) -> ContentResult<()>;
    /// Exact metadata-only terminalization; no SQL/native/query/refund/retry.
    fn site_abandon(&mut self, scope: &SiteScope) -> ContentResult<()>;
}
