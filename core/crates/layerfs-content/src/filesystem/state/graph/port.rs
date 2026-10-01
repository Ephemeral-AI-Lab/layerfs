//! Closed effective graph construction, selective SCC and terminal custody port.
use super::*;
use crate::error::ContentResult;
/// Same live native owner; all methods validate selected context before effects.
pub trait EffectiveGraphState {
    /// Pure exact captured owner/context/budget selection, including while Sites is open.
    /// Foreign/getter refusal mutates no phase, query, quota or error custody.
    fn graph_select(&self, scope: &GraphScope) -> ContentResult<()>;
    /// Captured phase-admitted aggregate capacity; mutation requires known Sites retirement.
    fn graph_capacity(&self, scope: &GraphScope) -> ContentResult<GraphCapacity>;
    /// Update enrollment before any unexpanded selection.
    fn graph_seed_batch(
        &mut self,
        scope: &GraphScope,
        seeds: &[GraphNodeKey],
    ) -> ContentResult<GraphBuildAck>;
    /// Fresh root enrollment enters expansion directly.
    fn graph_root(&mut self, scope: &GraphScope) -> ContentResult<GraphBuildAck>;
    /// Exact partial-index next unexpanded row, closing seed enrollment.
    fn graph_unexpanded(&mut self, scope: &GraphScope) -> ContentResult<Option<GraphNode>>;
    /// At most63 raw children and <=127 combined affected node/edge records.
    fn graph_append(
        &mut self,
        scope: &GraphScope,
        parent: &GraphNode,
        children: &[GraphNodeKey],
    ) -> ContentResult<GraphBuildAck>;
    /// Exact adjacency EOF; native independently certifies SelfLoop.
    fn graph_expanded(
        &mut self,
        scope: &GraphScope,
        parent: &GraphNode,
    ) -> ContentResult<GraphNode>;
    /// Complete streamed immutable adjacency seal.
    fn graph_seal(&mut self, scope: &GraphScope) -> ContentResult<GraphAdjacencySeal>;
    /// Current mutable node; adjacency pages cannot replace this live lookup.
    fn graph_node(
        &mut self,
        seal: &GraphAdjacencySeal,
        key: GraphNodeKey,
    ) -> ContentResult<Option<GraphNode>>;
    /// Normalized immutable projection pages, including during solving.
    fn graph_node_page(
        &mut self,
        seal: &GraphAdjacencySeal,
        after: Option<GraphNodeKey>,
        limit: GraphPageLimit,
    ) -> ContentResult<GraphNodePage>;
    /// Parent-scoped immutable arcs, selected EOF from acknowledged MAX.
    fn graph_edge_page(
        &mut self,
        seal: &GraphAdjacencySeal,
        parent: u64,
        after: Option<u64>,
        limit: GraphPageLimit,
    ) -> ContentResult<GraphEdgePage>;
    /// Begins Update solver only after sealed construction.
    fn graph_begin_scc(&mut self, seal: &GraphAdjacencySeal) -> ContentResult<()>;
    /// Closed exact expected/proposed transitions; no arbitrary mutable put.
    fn graph_cas(
        &mut self,
        seal: &GraphAdjacencySeal,
        mutations: &[GraphMutation],
    ) -> ContentResult<GraphMutationAck>;
    /// Bounded descending SCC members; last known cycle is an acknowledged verdict.
    fn graph_pop(
        &mut self,
        seal: &GraphAdjacencySeal,
        root: &GraphNode,
        limit: GraphMutationLimit,
    ) -> ContentResult<GraphPopAck>;
    /// Exact terminal full-node proof, preserving immutable adjacency.
    fn graph_finish(&mut self, seal: &GraphAdjacencySeal) -> ContentResult<GraphProofSeal>;
    /// Full immutable terminal records; count/digest EOF remains independently checked.
    fn graph_proof_page(
        &mut self,
        seal: &GraphProofSeal,
        after: Option<GraphNodeKey>,
        limit: GraphPageLimit,
    ) -> ContentResult<GraphProofPage>;
    /// Known bounded retirement then empty indexes/COMMIT/native observation.
    fn graph_retire(&mut self, seal: &GraphProofSeal) -> ContentResult<()>;
    /// Selected metadata-only terminalization, including Unknown; no SQL/cleanup retry.
    fn graph_abandon(&mut self, scope: &GraphScope) -> ContentResult<()>;
}
