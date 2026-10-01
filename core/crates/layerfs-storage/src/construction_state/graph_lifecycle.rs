//! Known graph seals, solver acknowledgements and checked same-file retirement.

use super::graph_state::{GraphAttempt, GraphAttemptKind};
use super::{
    graph_index, graph_pop, graph_retire, graph_scan, graph_solve, profile, ScratchSession,
};
use crate::error::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{
    GraphAdjacencySeal, GraphMode, GraphMutation, GraphMutationAck, GraphMutationLimit, GraphNode,
    GraphNodeChange, GraphPopAck, GraphProofSeal, GraphScope, GraphStage,
};

impl ScratchSession {
    /// Close the actual complete normalized adjacency in one bounded ordered walk.
    pub fn graph_seal(&mut self, scope: &GraphScope) -> StorageResult<GraphAdjacencySeal> {
        self.ensure_graph_scope(scope)?;
        let result = (|| {
            let resource = self.graph_resource_mut(scope)?;
            let graph = resource.graph.as_ref().unwrap();
            if graph.stage != GraphStage::Expanding {
                return Err(StorageError::Integrity(
                    "construction scratch graph seal phase",
                ));
            }
            resource.verify()?;
            let graph = resource.graph.as_mut().unwrap();
            graph.attempt = Some(GraphAttempt::new(
                GraphAttemptKind::AdjacencySeal,
                graph,
                GraphStage::AdjacencySealed,
            )?);
            resource.native.reserve()?;
            let seal = graph_scan::adjacency(
                resource.connection.as_ref().unwrap(),
                resource.engine,
                resource.graph.as_mut().unwrap(),
            )?;
            resource.native.observe_allocation()?;
            let graph = resource.graph.as_mut().unwrap();
            graph.adjacency = Some(seal.clone());
            graph.stage = GraphStage::AdjacencySealed;
            graph.proposed_adjacency = None;
            graph.attempt = None;
            Ok(seal)
        })();
        self.finish(result)
    }

    /// Select Update SCC only from the exact immutable adjacency.
    pub fn graph_begin_scc(&mut self, seal: &GraphAdjacencySeal) -> StorageResult<()> {
        self.ensure_graph_scope(seal.scope())?;
        let result = (|| {
            let resource = self.graph_resource_mut(seal.scope())?;
            let graph = resource.graph.as_ref().unwrap();
            graph.check_adjacency(seal)?;
            if graph.stage != GraphStage::AdjacencySealed
                || graph.scope.subject().mode() != GraphMode::Update
            {
                return Err(StorageError::Integrity(
                    "construction scratch graph begin SCC phase",
                ));
            }
            resource.verify()?;
            let graph = resource.graph.as_mut().unwrap();
            graph.attempt = Some(GraphAttempt::new(
                GraphAttemptKind::BeginScc,
                graph,
                GraphStage::Solving,
            )?);
            resource.native.reserve()?;
            let connection = resource.connection.as_ref().unwrap();
            resource.check_engine()?;
            crate::sqlite::write::begin_immediate(connection)?;
            let result = (|| {
                graph_index::verify(connection, resource.graph.as_ref().unwrap())?;
                if connection.execute("UPDATE graph_owner SET stage=4 WHERE id=1 AND stage=3 AND scope=?1 AND adjacency_seal=?2",
                    rusqlite::params![seal.scope().as_bytes().as_slice(), seal.encode().as_slice()])? != 1
                { return Err(StorageError::Integrity("construction scratch graph SCC acknowledgement")); }
                Ok(())
            })();
            profile::finish_write_guarded(connection, result, resource.engine)?;
            resource.native.observe_allocation()?;
            let graph = resource.graph.as_mut().unwrap();
            graph.stage = GraphStage::Solving;
            graph.attempt = None;
            Ok(())
        })();
        self.finish(result)
    }

    /// Closed exact expected/proposed targets; all selected rows precede writes.
    pub fn graph_cas(
        &mut self,
        seal: &GraphAdjacencySeal,
        mutations: &[GraphMutation],
    ) -> StorageResult<GraphMutationAck> {
        self.ensure_graph_scope(seal.scope())?;
        let result = (|| {
            let resource = self.graph_resource(seal.scope())?;
            let graph = resource.graph.as_ref().unwrap();
            graph.check_adjacency(seal)?;
            if graph.stage != GraphStage::Solving {
                return Err(StorageError::Integrity(
                    "construction scratch graph solver phase",
                ));
            }
            GraphMutationLimit::default().check(seal.scope(), mutations)?;
            let attempt = graph_solve::prepare(resource.verify()?, graph, mutations)?;
            let memory = graph.memory.reserve(
                std::mem::size_of::<GraphMutationAck>()
                    + attempt.nodes * std::mem::size_of::<GraphNodeChange>(),
            )?;
            let mut changes = graph_index::window(attempt.nodes)?;
            for index in 0..attempt.nodes {
                changes.push(GraphNodeChange::new(
                    seal.scope(),
                    attempt.old_nodes[index],
                    attempt.new_nodes[index].unwrap(),
                )?);
            }
            changes.sort_unstable_by_key(|row| row.after().key());
            let ack = GraphMutationAck::new(seal.scope().clone(), changes)?.with_memory(memory)?;
            let resource = self.graph_resource_mut(seal.scope())?;
            resource.graph.as_mut().unwrap().attempt = Some(attempt);
            resource.native.reserve()?;
            let graph = resource.graph.as_ref().unwrap();
            graph_solve::commit(
                resource.connection.as_ref().unwrap(),
                resource.engine,
                graph,
                graph.attempt.as_ref().unwrap(),
                mutations,
            )?;
            resource.native.observe_allocation()?;
            let graph = resource.graph.as_mut().unwrap();
            graph.solver = graph.attempt.take().unwrap().new_solver;
            Ok(ack)
        })();
        self.finish(result)
    }

    /// Return the bounded completed last window even for a known rejected Seed SCC.
    /// COMMIT/observation uncertainty returns the real provider failure without an Ack.
    pub fn graph_pop(
        &mut self,
        seal: &GraphAdjacencySeal,
        root: &GraphNode,
        limit: GraphMutationLimit,
    ) -> StorageResult<GraphPopAck> {
        self.ensure_graph_scope(seal.scope())?;
        let result = (|| {
            let resource = self.graph_resource(seal.scope())?;
            let graph = resource.graph.as_ref().unwrap();
            graph.check_adjacency(seal)?;
            GraphNode::decode(seal.scope(), &root.encode())?;
            if graph.stage != GraphStage::Solving
                || graph.solver.current != root.key().serial()
                || !root.on_stack()
                || !root.dfs_finished()
                || root.lowlink() != root.discovery()
                || graph.solver.scc_root != 0 && graph.solver.scc_root != root.key().serial()
            {
                return Err(StorageError::Integrity(
                    "construction scratch graph selected SCC root",
                ));
            }
            if limit.bytes().saturating_sub(350) / 60 == 0 {
                return Err(StorageError::CapacityExceeded {
                    what: "construction scratch graph pop bytes",
                    limit: limit.bytes() as u64,
                    actual: 410,
                });
            }
            let (attempt, ack) = graph_pop::prepare(resource.verify()?, graph, *root, limit)?;
            let resource = self.graph_resource_mut(seal.scope())?;
            resource.graph.as_mut().unwrap().attempt = Some(attempt);
            resource.native.reserve()?;
            let graph = resource.graph.as_ref().unwrap();
            graph_pop::commit(
                resource.connection.as_ref().unwrap(),
                resource.engine,
                graph,
                graph.attempt.as_ref().unwrap(),
            )?;
            resource.native.observe_allocation()?;
            let graph = resource.graph.as_mut().unwrap();
            let attempt = graph.attempt.take().unwrap();
            graph.solver = attempt.new_solver;
            graph.stage = attempt.proposed_stage;
            Ok(ack)
        })();
        self.finish(result)
    }

    /// Verify terminal rows/empty stack and rehash immutable adjacency before proof.
    pub fn graph_finish(&mut self, seal: &GraphAdjacencySeal) -> StorageResult<GraphProofSeal> {
        self.ensure_graph_scope(seal.scope())?;
        let result = (|| {
            let resource = self.graph_resource_mut(seal.scope())?;
            let graph = resource.graph.as_ref().unwrap();
            graph.check_adjacency(seal)?;
            graph.check_terminal_solver()?;
            resource.verify()?;
            let graph = resource.graph.as_mut().unwrap();
            graph.attempt = Some(GraphAttempt::new(
                GraphAttemptKind::Proof,
                graph,
                GraphStage::Proved,
            )?);
            resource.native.reserve()?;
            let proof = graph_scan::proof(
                resource.connection.as_ref().unwrap(),
                resource.engine,
                resource.graph.as_mut().unwrap(),
            )?;
            resource.native.observe_allocation()?;
            let graph = resource.graph.as_mut().unwrap();
            graph.stage = GraphStage::Proved;
            graph.proof = Some(proof.clone());
            graph.remaining_nodes = graph.totals.nodes();
            graph.remaining_edges = graph.totals.edges();
            graph.proposed_proof = None;
            graph.attempt = None;
            Ok(proof)
        })();
        self.finish(result)
    }

    /// Retire one shared <=128-key window at a time. No native credit is refunded.
    /// Roots become eligible only after the last known COMMIT and native observation.
    pub fn graph_retire(&mut self, seal: &GraphProofSeal) -> StorageResult<()> {
        self.ensure_graph_scope(seal.scope())?;
        let result = (|| {
            self.graph_resource(seal.scope())?
                .graph
                .as_ref()
                .unwrap()
                .check_proof(seal)?;
            loop {
                let resource = self.graph_resource_mut(seal.scope())?;
                resource.verify()?;
                let graph = resource.graph.as_ref().unwrap();
                let attempt = graph_retire::prepare(resource.connection.as_ref().unwrap(), graph)?;
                resource.graph.as_mut().unwrap().attempt = Some(attempt);
                resource.native.reserve()?;
                let graph = resource.graph.as_ref().unwrap();
                graph_retire::commit(
                    resource.connection.as_ref().unwrap(),
                    resource.engine,
                    graph,
                    graph.attempt.as_ref().unwrap(),
                )?;
                resource.native.observe_allocation()?;
                let graph = resource.graph.as_mut().unwrap();
                let attempt = graph.attempt.take().unwrap();
                graph.stage = attempt.proposed_stage;
                graph.remaining_nodes = attempt.remaining_nodes.1;
                graph.remaining_edges = attempt.remaining_edges.1;
                graph.after_node = attempt.proposed_node;
                graph.after_edge = attempt.proposed_edge;
                if graph.stage == GraphStage::Retired {
                    break;
                }
            }
            Ok(())
        })();
        self.finish(result)
    }
}
