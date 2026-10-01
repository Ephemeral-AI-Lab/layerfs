//! Explicit C1 graph delegation preserving the original typed provider failure.

use super::ScratchAdapter;
use crate::StorageError;
use layerfs_content::filesystem::state::{
    EffectiveGraphState, GraphAdjacencySeal, GraphBuildAck, GraphCapacity, GraphEdgePage,
    GraphMutation, GraphMutationAck, GraphMutationLimit, GraphNode, GraphNodeKey, GraphNodePage,
    GraphPageLimit, GraphPopAck, GraphProofPage, GraphProofSeal, GraphScope,
};
use layerfs_content::{ContentError, ContentResult};

impl ScratchAdapter<'_> {
    fn graph_failure(&self, error: StorageError) -> ContentError {
        match error {
            StorageError::Content(ContentError::InvalidOrderingRecord("graph foreign scope")) => {
                ContentError::InvalidOrderingRecord("graph foreign scope")
            }
            error => self.session.keep_failure(error),
        }
    }
}

impl EffectiveGraphState for ScratchAdapter<'_> {
    fn graph_select(&self, scope: &GraphScope) -> ContentResult<()> {
        match self.session.graph_select(scope) {
            Ok(()) => Ok(()),
            Err(StorageError::Content(error)) => Err(error),
            Err(_) => Err(ContentError::ProviderFailure {
                what: "construction scratch state",
            }),
        }
    }
    fn graph_capacity(&self, scope: &GraphScope) -> ContentResult<GraphCapacity> {
        self.session
            .graph_capacity(scope)
            .map_err(|error| self.graph_failure(error))
    }
    fn graph_seed_batch(
        &mut self,
        scope: &GraphScope,
        seeds: &[GraphNodeKey],
    ) -> ContentResult<GraphBuildAck> {
        self.session
            .graph_seed_batch(scope, seeds)
            .map_err(|error| self.graph_failure(error))
    }
    fn graph_root(&mut self, scope: &GraphScope) -> ContentResult<GraphBuildAck> {
        self.session
            .graph_root(scope)
            .map_err(|error| self.graph_failure(error))
    }
    fn graph_unexpanded(&mut self, scope: &GraphScope) -> ContentResult<Option<GraphNode>> {
        self.session
            .graph_unexpanded(scope)
            .map_err(|error| self.graph_failure(error))
    }
    fn graph_append(
        &mut self,
        scope: &GraphScope,
        parent: &GraphNode,
        children: &[GraphNodeKey],
    ) -> ContentResult<GraphBuildAck> {
        self.session
            .graph_append(scope, parent, children)
            .map_err(|error| self.graph_failure(error))
    }
    fn graph_expanded(
        &mut self,
        scope: &GraphScope,
        parent: &GraphNode,
    ) -> ContentResult<GraphNode> {
        self.session
            .graph_expanded(scope, parent)
            .map_err(|error| self.graph_failure(error))
    }
    fn graph_seal(&mut self, scope: &GraphScope) -> ContentResult<GraphAdjacencySeal> {
        self.session
            .graph_seal(scope)
            .map_err(|error| self.graph_failure(error))
    }
    fn graph_node(
        &mut self,
        seal: &GraphAdjacencySeal,
        key: GraphNodeKey,
    ) -> ContentResult<Option<GraphNode>> {
        self.session
            .graph_node(seal, key)
            .map_err(|error| self.graph_failure(error))
    }
    fn graph_node_page(
        &mut self,
        seal: &GraphAdjacencySeal,
        after: Option<GraphNodeKey>,
        limit: GraphPageLimit,
    ) -> ContentResult<GraphNodePage> {
        self.session
            .graph_node_page(seal, after, limit)
            .map_err(|error| self.graph_failure(error))
    }
    fn graph_edge_page(
        &mut self,
        seal: &GraphAdjacencySeal,
        parent: u64,
        after: Option<u64>,
        limit: GraphPageLimit,
    ) -> ContentResult<GraphEdgePage> {
        self.session
            .graph_edge_page(seal, parent, after, limit)
            .map_err(|error| self.graph_failure(error))
    }
    fn graph_begin_scc(&mut self, seal: &GraphAdjacencySeal) -> ContentResult<()> {
        self.session
            .graph_begin_scc(seal)
            .map_err(|error| self.graph_failure(error))
    }
    fn graph_cas(
        &mut self,
        seal: &GraphAdjacencySeal,
        mutations: &[GraphMutation],
    ) -> ContentResult<GraphMutationAck> {
        self.session
            .graph_cas(seal, mutations)
            .map_err(|error| self.graph_failure(error))
    }
    fn graph_pop(
        &mut self,
        seal: &GraphAdjacencySeal,
        root: &GraphNode,
        limit: GraphMutationLimit,
    ) -> ContentResult<GraphPopAck> {
        self.session
            .graph_pop(seal, root, limit)
            .map_err(|error| self.graph_failure(error))
    }
    fn graph_finish(&mut self, seal: &GraphAdjacencySeal) -> ContentResult<GraphProofSeal> {
        self.session
            .graph_finish(seal)
            .map_err(|error| self.graph_failure(error))
    }
    fn graph_proof_page(
        &mut self,
        seal: &GraphProofSeal,
        after: Option<GraphNodeKey>,
        limit: GraphPageLimit,
    ) -> ContentResult<GraphProofPage> {
        self.session
            .graph_proof_page(seal, after, limit)
            .map_err(|error| self.graph_failure(error))
    }
    fn graph_retire(&mut self, seal: &GraphProofSeal) -> ContentResult<()> {
        self.session
            .graph_retire(seal)
            .map_err(|error| self.graph_failure(error))
    }
    fn graph_abandon(&mut self, scope: &GraphScope) -> ContentResult<()> {
        self.session
            .graph_abandon(scope)
            .map_err(|error| self.graph_failure(error))
    }
}
