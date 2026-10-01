//! The same checked selective-SCC protocol over a proven zero population.
use super::empty_owner::{bad, growth};
use super::*;
use crate::ContentResult;
impl VerifiedEmptyState {
    fn selected_adjacency(&self, seal: &GraphAdjacencySeal) -> ContentResult<()> {
        self.graph_scope(seal.scope())?;
        if self.data.adjacency.as_ref() != Some(seal)
            || !matches!(
                self.data.graph_stage,
                GraphStage::AdjacencySealed | GraphStage::Solving | GraphStage::Proved
            )
        {
            return Err(bad("empty selected adjacency"));
        }
        Ok(())
    }
}
impl EffectiveGraphState for VerifiedEmptyState {
    fn graph_select(&self, scope: &GraphScope) -> ContentResult<()> {
        self.graph_scope(scope)
    }
    fn graph_capacity(&self, scope: &GraphScope) -> ContentResult<GraphCapacity> {
        self.graph_scope(scope)?;
        Ok(scope.capacity())
    }
    fn graph_seed_batch(
        &mut self,
        scope: &GraphScope,
        seeds: &[GraphNodeKey],
    ) -> ContentResult<GraphBuildAck> {
        self.graph_scope(scope)?;
        growth(seeds.len())?;
        if !self.data.sites_retired
            || !matches!(
                self.data.graph_stage,
                GraphStage::Deferred | GraphStage::Seeding
            )
        {
            return Err(bad("empty Graph enrollment phase"));
        }
        let memory = self.memory.reserve(std::mem::size_of::<GraphBuildAck>())?;
        let ack = GraphBuildAck::new(
            scope.clone(),
            GraphTotals::default(),
            GraphTotals::default(),
            Vec::new(),
            Vec::new(),
            None,
        )?
        .with_memory(memory)?;
        self.data.graph_stage = GraphStage::Seeding;
        Ok(ack)
    }
    fn graph_root(&mut self, scope: &GraphScope) -> ContentResult<GraphBuildAck> {
        self.graph_scope(scope)?;
        Err(bad("empty UpdateBase cannot enroll fresh root"))
    }
    fn graph_unexpanded(&mut self, scope: &GraphScope) -> ContentResult<Option<GraphNode>> {
        self.graph_scope(scope)?;
        if !self.data.sites_retired
            || !matches!(
                self.data.graph_stage,
                GraphStage::Deferred | GraphStage::Seeding | GraphStage::Expanding
            )
        {
            return Err(bad("empty Graph expansion phase"));
        }
        self.data.graph_stage = GraphStage::Expanding;
        Ok(None)
    }
    fn graph_append(
        &mut self,
        scope: &GraphScope,
        _parent: &GraphNode,
        _children: &[GraphNodeKey],
    ) -> ContentResult<GraphBuildAck> {
        self.graph_scope(scope)?;
        Err(bad("empty Graph has no parent"))
    }
    fn graph_expanded(
        &mut self,
        scope: &GraphScope,
        _parent: &GraphNode,
    ) -> ContentResult<GraphNode> {
        self.graph_scope(scope)?;
        Err(bad("empty Graph has no expansion owner"))
    }
    fn graph_seal(&mut self, scope: &GraphScope) -> ContentResult<GraphAdjacencySeal> {
        self.graph_scope(scope)?;
        if self.data.graph_stage != GraphStage::Expanding {
            return Err(bad("empty Graph seal before expansion EOF"));
        }
        let seal = GraphAdjacencySeal::new(
            scope.clone(),
            0,
            0,
            GraphNodeLedger::adjacency(scope.clone()).digest(),
            GraphEdgeLedger::new(scope.clone()).digest(),
        )?;
        self.data.adjacency = Some(seal.clone());
        self.data.graph_stage = GraphStage::AdjacencySealed;
        Ok(seal)
    }
    fn graph_node(
        &mut self,
        seal: &GraphAdjacencySeal,
        key: GraphNodeKey,
    ) -> ContentResult<Option<GraphNode>> {
        self.selected_adjacency(seal)?;
        GraphNodeKey::decode(seal.scope(), key.as_bytes())?;
        Ok(None)
    }
    fn graph_node_page(
        &mut self,
        seal: &GraphAdjacencySeal,
        after: Option<GraphNodeKey>,
        limit: GraphPageLimit,
    ) -> ContentResult<GraphNodePage> {
        self.selected_adjacency(seal)?;
        if after.is_some() {
            return Err(bad("empty Graph continuation"));
        }
        limit.check(0, 0, 318, 60)?;
        let page = GraphNodePage::after(seal.clone(), None, Vec::new(), true)?;
        if self.data.graph_stage == GraphStage::Solving {
            self.data.solving_eof = true;
        } else {
            self.data.adjacency_eof = true;
        }
        Ok(page)
    }
    fn graph_edge_page(
        &mut self,
        seal: &GraphAdjacencySeal,
        _parent: u64,
        _after: Option<u64>,
        _limit: GraphPageLimit,
    ) -> ContentResult<GraphEdgePage> {
        self.selected_adjacency(seal)?;
        Err(bad("empty Graph parent is absent"))
    }
    fn graph_begin_scc(&mut self, seal: &GraphAdjacencySeal) -> ContentResult<()> {
        self.selected_adjacency(seal)?;
        if self.data.graph_stage != GraphStage::AdjacencySealed || !self.data.adjacency_eof {
            return Err(bad("empty SCC before adjacency EOF"));
        }
        self.data.graph_stage = GraphStage::Solving;
        Ok(())
    }
    fn graph_cas(
        &mut self,
        seal: &GraphAdjacencySeal,
        _mutations: &[GraphMutation],
    ) -> ContentResult<GraphMutationAck> {
        self.selected_adjacency(seal)?;
        Err(bad("empty SCC has no mutation owner"))
    }
    fn graph_pop(
        &mut self,
        seal: &GraphAdjacencySeal,
        _root: &GraphNode,
        _limit: GraphMutationLimit,
    ) -> ContentResult<GraphPopAck> {
        self.selected_adjacency(seal)?;
        Err(bad("empty SCC has no stack owner"))
    }
    fn graph_finish(&mut self, seal: &GraphAdjacencySeal) -> ContentResult<GraphProofSeal> {
        self.selected_adjacency(seal)?;
        if self.data.graph_stage != GraphStage::Solving || !self.data.solving_eof {
            return Err(bad("empty Graph proof before SCC EOF"));
        }
        let proof = GraphProofSeal::new(
            seal.clone(),
            GraphNodeLedger::proof(seal.scope().clone()).digest(),
        );
        self.data.proof = Some(proof.clone());
        self.data.graph_stage = GraphStage::Proved;
        Ok(proof)
    }
    fn graph_proof_page(
        &mut self,
        seal: &GraphProofSeal,
        after: Option<GraphNodeKey>,
        limit: GraphPageLimit,
    ) -> ContentResult<GraphProofPage> {
        self.graph_scope(seal.scope())?;
        if self.data.graph_stage != GraphStage::Proved
            || self.data.proof.as_ref() != Some(seal)
            || after.is_some()
        {
            return Err(bad("empty selected Graph proof"));
        }
        limit.check(0, 0, 350, 60)?;
        let page = GraphProofPage::after(seal.clone(), None, Vec::new(), true)?;
        self.data.proof_eof = true;
        Ok(page)
    }
    fn graph_retire(&mut self, seal: &GraphProofSeal) -> ContentResult<()> {
        self.graph_scope(seal.scope())?;
        if self.data.graph_stage != GraphStage::Proved
            || self.data.proof.as_ref() != Some(seal)
            || !self.data.proof_eof
        {
            return Err(bad("empty Graph retirement before proof EOF"));
        }
        self.data.graph_stage = GraphStage::Retired;
        Ok(())
    }
    fn graph_abandon(&mut self, scope: &GraphScope) -> ContentResult<()> {
        if scope != self.data.scopes.graph() {
            return Err(bad("empty foreign Graph"));
        }
        self.abandon();
        Ok(())
    }
}
