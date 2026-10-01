//! Same hardzero namespace protocols; no growing directory/graph/fresh authority.
use super::*;
use crate::ContentResult;
impl BindingSiteState for VerifiedSmallFileState {
    fn site_capacity(&self, scope: &SiteScope) -> ContentResult<SiteCapacity> {
        self.inner.site_capacity(scope)
    }
    fn site_get(&mut self, scope: &SiteScope, key: SiteKey) -> ContentResult<Option<SiteRecord>> {
        self.inner.site_get(scope, key)
    }
    fn site_insert_batch(
        &mut self,
        scope: &SiteScope,
        records: &[SiteRecord],
    ) -> ContentResult<ClaimAdmission> {
        self.inner.site_insert_batch(scope, records)
    }
    fn site_close_membership(&mut self, expected: &SiteBirthSeal) -> ContentResult<SiteMembership> {
        self.inner.site_close_membership(expected)
    }
    fn site_parent_present(
        &mut self,
        members: &SiteMembership,
        parent: u64,
    ) -> ContentResult<bool> {
        self.inner.site_parent_present(members, parent)
    }
    fn site_parent_page(
        &mut self,
        members: &SiteMembership,
        parent: u64,
        after: Option<u32>,
        limit: SiteParentPageLimit,
    ) -> ContentResult<SiteParentPage> {
        self.inner.site_parent_page(members, parent, after, limit)
    }
    fn site_observe_base_batch(
        &mut self,
        members: &SiteMembership,
        observations: &[SiteObservation],
    ) -> ContentResult<()> {
        self.inner.site_observe_base_batch(members, observations)
    }
    fn site_final_seal(&mut self, members: &SiteMembership) -> ContentResult<SiteSeal> {
        self.inner.site_final_seal(members)
    }
    fn site_sealed_page(
        &mut self,
        seal: &SiteSeal,
        after: Option<SiteKey>,
        limit: SitePageLimit,
    ) -> ContentResult<SitePage> {
        self.inner.site_sealed_page(seal, after, limit)
    }
    fn site_retire(&mut self, seal: &SiteSeal) -> ContentResult<()> {
        self.inner.site_retire(seal)
    }
    fn site_abandon(&mut self, scope: &SiteScope) -> ContentResult<()> {
        self.inner.site_abandon(scope)
    }
}
impl AliasFrontier for VerifiedSmallFileState {
    fn alias_capacity(&self, members: &SiteMembership) -> ContentResult<AliasCapacity> {
        self.inner.alias_capacity(members)
    }
    fn alias_begin(&mut self, members: &SiteMembership, root: Option<u64>) -> ContentResult<()> {
        self.inner.alias_begin(members, root)
    }
    fn alias_enqueue(&mut self, members: &SiteMembership, children: &[u64]) -> ContentResult<()> {
        self.inner.alias_enqueue(members, children)
    }
    fn alias_take(&mut self, members: &SiteMembership) -> ContentResult<Option<AliasCurrent>> {
        self.inner.alias_take(members)
    }
    fn alias_advance(
        &mut self,
        members: &SiteMembership,
        _current: AliasCurrent,
        _before: &AliasProgress,
        _after: &AliasProgress,
    ) -> ContentResult<()> {
        self.inner.alias_advance(members, _current, _before, _after)
    }
    fn alias_complete(
        &mut self,
        members: &SiteMembership,
        _current: AliasCurrent,
    ) -> ContentResult<()> {
        self.inner.alias_complete(members, _current)
    }
    fn alias_finish(&mut self, members: &SiteMembership) -> ContentResult<AliasSeal> {
        self.inner.alias_finish(members)
    }
    fn alias_retire(&mut self, seal: &AliasSeal) -> ContentResult<()> {
        self.inner.alias_retire(seal)
    }
    fn alias_abandon(&mut self, members: &SiteMembership) -> ContentResult<()> {
        self.inner.alias_abandon(members)
    }
}
impl EffectiveGraphState for VerifiedSmallFileState {
    fn graph_select(&self, scope: &GraphScope) -> ContentResult<()> {
        self.inner.graph_select(scope)
    }
    fn graph_capacity(&self, scope: &GraphScope) -> ContentResult<GraphCapacity> {
        self.inner.graph_capacity(scope)
    }
    fn graph_seed_batch(
        &mut self,
        scope: &GraphScope,
        seeds: &[GraphNodeKey],
    ) -> ContentResult<GraphBuildAck> {
        self.inner.graph_seed_batch(scope, seeds)
    }
    fn graph_root(&mut self, scope: &GraphScope) -> ContentResult<GraphBuildAck> {
        self.inner.graph_root(scope)
    }
    fn graph_unexpanded(&mut self, scope: &GraphScope) -> ContentResult<Option<GraphNode>> {
        self.inner.graph_unexpanded(scope)
    }
    fn graph_append(
        &mut self,
        scope: &GraphScope,
        _parent: &GraphNode,
        _children: &[GraphNodeKey],
    ) -> ContentResult<GraphBuildAck> {
        self.inner.graph_append(scope, _parent, _children)
    }
    fn graph_expanded(
        &mut self,
        scope: &GraphScope,
        _parent: &GraphNode,
    ) -> ContentResult<GraphNode> {
        self.inner.graph_expanded(scope, _parent)
    }
    fn graph_seal(&mut self, scope: &GraphScope) -> ContentResult<GraphAdjacencySeal> {
        self.inner.graph_seal(scope)
    }
    fn graph_node(
        &mut self,
        seal: &GraphAdjacencySeal,
        key: GraphNodeKey,
    ) -> ContentResult<Option<GraphNode>> {
        self.inner.graph_node(seal, key)
    }
    fn graph_node_page(
        &mut self,
        seal: &GraphAdjacencySeal,
        after: Option<GraphNodeKey>,
        limit: GraphPageLimit,
    ) -> ContentResult<GraphNodePage> {
        self.inner.graph_node_page(seal, after, limit)
    }
    fn graph_edge_page(
        &mut self,
        seal: &GraphAdjacencySeal,
        _parent: u64,
        _after: Option<u64>,
        _limit: GraphPageLimit,
    ) -> ContentResult<GraphEdgePage> {
        self.inner.graph_edge_page(seal, _parent, _after, _limit)
    }
    fn graph_begin_scc(&mut self, seal: &GraphAdjacencySeal) -> ContentResult<()> {
        self.inner.graph_begin_scc(seal)
    }
    fn graph_cas(
        &mut self,
        seal: &GraphAdjacencySeal,
        _mutations: &[GraphMutation],
    ) -> ContentResult<GraphMutationAck> {
        self.inner.graph_cas(seal, _mutations)
    }
    fn graph_pop(
        &mut self,
        seal: &GraphAdjacencySeal,
        _root: &GraphNode,
        _limit: GraphMutationLimit,
    ) -> ContentResult<GraphPopAck> {
        self.inner.graph_pop(seal, _root, _limit)
    }
    fn graph_finish(&mut self, seal: &GraphAdjacencySeal) -> ContentResult<GraphProofSeal> {
        self.inner.graph_finish(seal)
    }
    fn graph_proof_page(
        &mut self,
        seal: &GraphProofSeal,
        after: Option<GraphNodeKey>,
        limit: GraphPageLimit,
    ) -> ContentResult<GraphProofPage> {
        self.inner.graph_proof_page(seal, after, limit)
    }
    fn graph_retire(&mut self, seal: &GraphProofSeal) -> ContentResult<()> {
        self.inner.graph_retire(seal)
    }
    fn graph_abandon(&mut self, scope: &GraphScope) -> ContentResult<()> {
        self.inner.graph_abandon(scope)
    }
}
impl VerifiedSmallFileState {
    fn parent_context(&self, scope: &FactScope) -> ContentResult<()> {
        self.inner.live()?;
        if scope.state().selection() != self.selection()
            || scope.subject().selected() != self.subject()
            || scope.subject().table() != Some(self.data.table)
            || scope.state().phase() != 5
            || scope.state().table() != StateTable::ParentEligibility
        {
            return Err(super::empty_owner::bad(
                "small file foreign actual Parents table",
            ));
        }
        Ok(())
    }
}
impl ParentEligibilityState for VerifiedSmallFileState {
    fn parent_bind(&mut self, scope: &FactScope) -> ContentResult<()> {
        self.parent_context(scope)?;
        self.inner.parent_bind(scope)
    }
    fn parent_insert(&mut self, scope: &FactScope, serials: &[u64]) -> ContentResult<()> {
        self.parent_context(scope)?;
        self.inner.parent_insert(scope, serials)
    }
    fn parent_close_declarations(&mut self, scope: &FactScope) -> ContentResult<()> {
        self.parent_context(scope)?;
        self.inner.parent_close_declarations(scope)
    }
    fn parent_mark_bound(&mut self, scope: &FactScope, children: &[u64]) -> ContentResult<()> {
        self.parent_context(scope)?;
        self.inner.parent_mark_bound(scope, children)
    }
    fn parent_seal(&mut self, scope: &FactScope) -> ContentResult<ParentSeal> {
        self.parent_context(scope)?;
        self.inner.parent_seal(scope)
    }
    fn parent_get(&mut self, seal: &ParentSeal, serial: u64) -> ContentResult<Option<ParentFact>> {
        self.parent_context(&seal.facts.scope)?;
        self.inner.parent_get(seal, serial)
    }
    fn parent_page(
        &mut self,
        seal: &ParentSeal,
        after: Option<u64>,
        records: usize,
        bytes: usize,
    ) -> ContentResult<FactPage<ParentFact>> {
        self.parent_context(&seal.facts.scope)?;
        self.inner.parent_page(seal, after, records, bytes)
    }
    fn parent_retire(&mut self, seal: &ParentSeal) -> ContentResult<()> {
        self.parent_context(&seal.facts.scope)?;
        self.inner.parent_retire(seal)
    }
    fn parent_abandon(&mut self, scope: &FactScope) -> ContentResult<()> {
        self.parent_context(scope)?;
        self.inner.parent_abandon(scope)
    }
}
