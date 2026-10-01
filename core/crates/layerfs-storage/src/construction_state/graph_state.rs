//! Fixed graph lifecycle, captured subject and bounded unacknowledged custody.

use std::cell::Cell;

use layerfs_content::filesystem::state::{
    GraphAdjacencySeal, GraphEdge, GraphEdgeKey, GraphNode, GraphNodeKey, GraphProofSeal,
    GraphScope, GraphStage, GraphSubject, GraphTotals, StateScope, StateSelection, StateTable,
};

use crate::error::{StorageError, StorageResult};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Solver {
    pub(crate) current: u64,
    pub(crate) root: u64,
    pub(crate) next: u64,
    pub(crate) scc_root: u64,
    pub(crate) boundary: u32,
    pub(crate) popped: u32,
    pub(crate) last_stack: u32,
    pub(crate) any_seed: bool,
    pub(crate) singleton_loop: bool,
}

impl Solver {
    pub(crate) fn initial() -> Self {
        Self {
            next: 1,
            ..Self::default()
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum GraphAttemptKind {
    Seeds,
    Root,
    CloseSeeds,
    Append,
    Expanded,
    AdjacencySeal,
    BeginScc,
    Mutation,
    Pop,
    Proof,
    Retire,
}

pub(crate) struct GraphAttempt {
    pub(crate) kind: GraphAttemptKind,
    pub(crate) prior_stage: GraphStage,
    pub(crate) proposed_stage: GraphStage,
    pub(crate) prior: GraphTotals,
    pub(crate) proposed: GraphTotals,
    pub(crate) old_nodes: [Option<GraphNode>; 128],
    pub(crate) new_nodes: [Option<GraphNode>; 128],
    pub(crate) nodes: usize,
    pub(crate) old_edges: [Option<GraphEdge>; 128],
    pub(crate) new_edges: [Option<GraphEdge>; 128],
    pub(crate) edges: usize,
    pub(crate) old_solver: Solver,
    pub(crate) new_solver: Solver,
    pub(crate) prior_node: Option<GraphNodeKey>,
    pub(crate) proposed_node: Option<GraphNodeKey>,
    pub(crate) prior_edge: Option<GraphEdgeKey>,
    pub(crate) proposed_edge: Option<GraphEdgeKey>,
    pub(crate) maximum_node: Option<GraphNodeKey>,
    pub(crate) maximum_edge: Option<GraphEdgeKey>,
    pub(crate) seed_count: u64,
    pub(crate) mutation_codes: [u8; 128],
    pub(crate) selected_children: [Option<GraphNode>; 128],
    pub(crate) selected_edges: [Option<GraphEdge>; 128],
    pub(crate) logical_items: usize,
    pub(crate) remaining_nodes: (u64, u64),
    pub(crate) remaining_edges: (u64, u64),
    pub(crate) reject_cycle: bool,
}

impl GraphAttempt {
    pub(crate) fn new(kind: GraphAttemptKind, graph: &Graph, stage: GraphStage) -> Self {
        Self {
            kind,
            prior_stage: graph.stage,
            proposed_stage: stage,
            prior: graph.totals,
            proposed: graph.totals,
            old_nodes: [None; 128],
            new_nodes: [None; 128],
            nodes: 0,
            old_edges: [None; 128],
            new_edges: [None; 128],
            edges: 0,
            old_solver: graph.solver,
            new_solver: graph.solver,
            prior_node: graph.after_node,
            proposed_node: graph.after_node,
            prior_edge: graph.after_edge,
            proposed_edge: graph.after_edge,
            maximum_node: graph.maximum_node,
            maximum_edge: graph.maximum_edge,
            seed_count: graph.seed_count,
            mutation_codes: [0; 128],
            selected_children: [None; 128],
            selected_edges: [None; 128],
            logical_items: 0,
            remaining_nodes: (graph.remaining_nodes, graph.remaining_nodes),
            remaining_edges: (graph.remaining_edges, graph.remaining_edges),
            reject_cycle: false,
        }
    }

    pub(crate) fn node(&mut self, old: Option<GraphNode>, new: Option<GraphNode>) {
        self.old_nodes[self.nodes] = old;
        self.new_nodes[self.nodes] = new;
        self.nodes += 1;
    }

    pub(crate) fn edge(&mut self, old: Option<GraphEdge>, new: Option<GraphEdge>) {
        self.old_edges[self.edges] = old;
        self.new_edges[self.edges] = new;
        self.edges += 1;
    }
}

pub(crate) struct Graph {
    pub(crate) scope: GraphScope,
    pub(crate) roots: StateScope,
    pub(crate) declared_seeds: u64,
    pub(crate) stage: GraphStage,
    pub(crate) totals: GraphTotals,
    pub(crate) seed_count: u64,
    pub(crate) remaining_nodes: u64,
    pub(crate) remaining_edges: u64,
    pub(crate) maximum_node: Option<GraphNodeKey>,
    pub(crate) maximum_edge: Option<GraphEdgeKey>,
    pub(crate) after_node: Option<GraphNodeKey>,
    pub(crate) after_edge: Option<GraphEdgeKey>,
    pub(crate) solver: Solver,
    pub(crate) failed: Cell<bool>,
    pub(crate) attempt: Option<GraphAttempt>,
    pub(crate) adjacency: Option<GraphAdjacencySeal>,
    pub(crate) proof: Option<GraphProofSeal>,
    pub(crate) proposed_adjacency: Option<GraphAdjacencySeal>,
    pub(crate) proposed_proof: Option<GraphProofSeal>,
}

impl Graph {
    pub(crate) fn new(
        selection: &StateSelection,
        subject: GraphSubject,
        declared_seeds: u64,
    ) -> StorageResult<Self> {
        Ok(Self {
            scope: GraphScope::new(selection.clone(), subject)?,
            roots: StateScope::new(selection.clone(), 3, StateTable::DirectoryRoots)?,
            declared_seeds,
            stage: GraphStage::Deferred,
            totals: GraphTotals::default(),
            seed_count: 0,
            remaining_nodes: 0,
            remaining_edges: 0,
            maximum_node: None,
            maximum_edge: None,
            after_node: None,
            after_edge: None,
            solver: Solver::initial(),
            failed: Cell::new(false),
            attempt: None,
            adjacency: None,
            proof: None,
            proposed_adjacency: None,
            proposed_proof: None,
        })
    }

    pub(crate) fn check_scope(&self, scope: &GraphScope) -> StorageResult<()> {
        if scope != &self.scope {
            return Err(StorageError::Content(
                layerfs_content::ContentError::InvalidOrderingRecord("graph foreign scope"),
            ));
        }
        if self.failed.get() || self.stage == GraphStage::Rejected {
            return Err(StorageError::Integrity(
                "construction scratch graph terminal phase",
            ));
        }
        Ok(())
    }

    pub(crate) fn check_roots(&self, scope: &StateScope) -> StorageResult<()> {
        if scope != &self.roots || self.stage != GraphStage::Retired || self.failed.get() {
            return Err(StorageError::Integrity(
                "construction scratch graph not retired",
            ));
        }
        Ok(())
    }

    pub(crate) fn check_adjacency(&self, seal: &GraphAdjacencySeal) -> StorageResult<()> {
        self.check_scope(seal.scope())?;
        if self.adjacency.as_ref() != Some(seal)
            || !matches!(
                self.stage,
                GraphStage::AdjacencySealed | GraphStage::Solving | GraphStage::Proved
            )
        {
            return Err(StorageError::Integrity(
                "construction scratch exact graph adjacency",
            ));
        }
        Ok(())
    }

    pub(crate) fn check_proof(&self, seal: &GraphProofSeal) -> StorageResult<()> {
        self.check_scope(seal.scope())?;
        if self.proof.as_ref() != Some(seal) || self.stage != GraphStage::Proved {
            return Err(StorageError::Integrity(
                "construction scratch exact graph proof",
            ));
        }
        Ok(())
    }

    pub(crate) fn check_terminal_solver(&self) -> StorageResult<()> {
        let mode = self.scope.subject().mode();
        if !(mode == layerfs_content::filesystem::state::GraphMode::Fresh
            && self.stage == GraphStage::AdjacencySealed
            || mode == layerfs_content::filesystem::state::GraphMode::Update
                && self.stage == GraphStage::Solving)
            || self.solver.current != 0
            || self.solver.root != 0
            || self.solver.scc_root != 0
            || mode == layerfs_content::filesystem::state::GraphMode::Update
                && self.solver.next != self.totals.nodes() + 1
        {
            return Err(StorageError::Integrity(
                "construction scratch graph terminal solver",
            ));
        }
        Ok(())
    }

    pub(crate) fn description(&self) -> String {
        let mut text = format!("graph custody: scope={:?}, stage={:?}, totals={:?}, seeds={}, remaining=({},{}), maximum=({:?},{:?}), after=({:?},{:?}), solver={:?}",
            self.scope.as_bytes(), self.stage, self.totals, self.seed_count,
            self.remaining_nodes, self.remaining_edges, self.maximum_node, self.maximum_edge,
            self.after_node, self.after_edge, self.solver);
        if let Some(attempt) = &self.attempt {
            text.push_str(&format!("; graph attempt {:?}: stage={:?}->{:?}, totals={:?}->{:?}, old_nodes={:?}, proposed_nodes={:?}, old_edges={:?}, proposed_edges={:?}, solver={:?}->{:?}, cursor=({:?},{:?})->({:?},{:?}), reject_cycle={}",
                attempt.kind, attempt.prior_stage, attempt.proposed_stage, attempt.prior, attempt.proposed,
                &attempt.old_nodes[..attempt.nodes], &attempt.new_nodes[..attempt.nodes],
                &attempt.old_edges[..attempt.edges], &attempt.new_edges[..attempt.edges],
                attempt.old_solver, attempt.new_solver, attempt.prior_node, attempt.prior_edge,
                attempt.proposed_node, attempt.proposed_edge, attempt.reject_cycle));
            text.push_str(&format!("; mutation_codes={:?}, selected_children={:?}, selected_edges={:?}, maxima=({:?},{:?}), seed_count={}",
                &attempt.mutation_codes[..attempt.logical_items], &attempt.selected_children[..attempt.logical_items],
                &attempt.selected_edges[..attempt.logical_items], attempt.maximum_node, attempt.maximum_edge, attempt.seed_count));
            text.push_str(&format!(
                "; pending remaining_nodes={:?}, remaining_edges={:?}",
                attempt.remaining_nodes, attempt.remaining_edges
            ));
        }
        text.push_str(&format!(
            "; adjacency={:?}, proof={:?}, proposed_adjacency={:?}, proposed_proof={:?}",
            self.adjacency.as_ref().map(GraphAdjacencySeal::encode),
            self.proof.as_ref().map(GraphProofSeal::encode),
            self.proposed_adjacency
                .as_ref()
                .map(GraphAdjacencySeal::encode),
            self.proposed_proof.as_ref().map(GraphProofSeal::encode)
        ));
        text
    }
}
