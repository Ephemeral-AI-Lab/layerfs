//! Fixed graph lifecycle, captured subject and bounded unacknowledged custody.

use std::cell::Cell;

use layerfs_content::filesystem::state::{
    GraphAdjacencySeal, GraphEdge, GraphEdgeKey, GraphMemory, GraphMemoryLease, GraphNode,
    GraphNodeKey, GraphProofSeal, GraphScope, GraphStage, GraphSubject, GraphTotals, StateScope,
    StateSelection, StateTable,
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
    value: Box<GraphAttemptData>,
    pub(crate) memory: GraphMemoryLease,
}

impl std::ops::Deref for GraphAttempt {
    type Target = GraphAttemptData;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}
impl std::ops::DerefMut for GraphAttempt {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

pub(crate) struct GraphAttemptData {
    pub(crate) kind: GraphAttemptKind,
    pub(crate) prior_stage: GraphStage,
    pub(crate) proposed_stage: GraphStage,
    pub(crate) prior: GraphTotals,
    pub(crate) proposed: GraphTotals,
    pub(crate) old_nodes: Vec<Option<GraphNode>>,
    pub(crate) new_nodes: Vec<Option<GraphNode>>,
    pub(crate) nodes: usize,
    pub(crate) old_edges: Vec<Option<GraphEdge>>,
    pub(crate) new_edges: Vec<Option<GraphEdge>>,
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
    pub(crate) mutation_codes: Vec<u8>,
    pub(crate) selected_children: Vec<Option<GraphNode>>,
    pub(crate) selected_edges: Vec<Option<GraphEdge>>,
    pub(crate) logical_items: usize,
    pub(crate) remaining_nodes: (u64, u64),
    pub(crate) remaining_edges: (u64, u64),
    pub(crate) reject_cycle: bool,
}

impl GraphAttemptKind {
    pub(crate) const fn windows(self) -> (usize, usize, usize, usize, usize) {
        match self {
            Self::Seeds => (128, 0, 0, 0, 0),
            Self::Root | Self::Expanded => (1, 0, 0, 0, 0),
            Self::Append => (64, 63, 0, 0, 0),
            Self::Mutation => (128, 0, 128, 128, 128),
            Self::Pop => (128, 0, 1, 0, 0),
            Self::Retire => (128, 128, 0, 0, 0),
            _ => (0, 0, 0, 0, 0),
        }
    }
    pub(crate) const fn allocation_bytes(self) -> usize {
        let (nodes, edges, children, selected_edges, codes) = self.windows();
        std::mem::size_of::<GraphAttemptData>()
            + std::mem::size_of::<GraphAttempt>()
            + (2 * nodes + children) * std::mem::size_of::<Option<GraphNode>>()
            + (2 * edges + selected_edges) * std::mem::size_of::<Option<GraphEdge>>()
            + codes * std::mem::size_of::<u8>()
    }
}
fn window<T: Copy>(count: usize, empty: T) -> StorageResult<Vec<T>> {
    let mut rows = Vec::new();
    rows.try_reserve_exact(count).map_err(|_| {
        StorageError::Content(layerfs_content::ContentError::ResourceUnavailable {
            what: "graph.attempt_window",
        })
    })?;
    if rows.capacity() != count {
        return Err(StorageError::Integrity(
            "graph closed attempt actual capacity",
        ));
    }
    rows.resize(count, empty);
    Ok(rows)
}

impl GraphAttempt {
    pub(crate) fn new(
        kind: GraphAttemptKind,
        graph: &Graph,
        stage: GraphStage,
    ) -> StorageResult<Self> {
        Self::new_windows(kind, graph, stage, kind.windows())
    }

    pub(crate) fn new_with_items(
        kind: GraphAttemptKind,
        graph: &Graph,
        stage: GraphStage,
        items: usize,
        targets: usize,
    ) -> StorageResult<Self> {
        if !matches!(kind, GraphAttemptKind::Mutation) || items > 128 || targets > 128 {
            return Err(StorageError::Integrity(
                "graph closed mutation request window",
            ));
        }
        Self::new_windows(kind, graph, stage, (targets, 0, items, items, items))
    }
    fn new_windows(
        kind: GraphAttemptKind,
        graph: &Graph,
        stage: GraphStage,
        windows: (usize, usize, usize, usize, usize),
    ) -> StorageResult<Self> {
        let (node_window, edge_window, children_window, selected_edge_window, code_window) =
            windows;
        let bytes = std::mem::size_of::<GraphAttemptData>()
            + std::mem::size_of::<Self>()
            + (2 * node_window + children_window) * std::mem::size_of::<Option<GraphNode>>()
            + (2 * edge_window + selected_edge_window) * std::mem::size_of::<Option<GraphEdge>>()
            + code_window;
        let memory = graph.memory.reserve(bytes)?;
        Ok(Self {
            value: Box::new(GraphAttemptData {
                kind,
                prior_stage: graph.stage,
                proposed_stage: stage,
                prior: graph.totals,
                proposed: graph.totals,
                old_nodes: window(node_window, None)?,
                new_nodes: window(node_window, None)?,
                nodes: 0,
                old_edges: window(edge_window, None)?,
                new_edges: window(edge_window, None)?,
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
                mutation_codes: window(code_window, 0)?,
                selected_children: window(children_window, None)?,
                selected_edges: window(selected_edge_window, None)?,
                logical_items: 0,
                remaining_nodes: (graph.remaining_nodes, graph.remaining_nodes),
                remaining_edges: (graph.remaining_edges, graph.remaining_edges),
                reject_cycle: false,
            }),
            memory,
        })
    }

    pub(crate) fn node(
        &mut self,
        old: Option<GraphNode>,
        new: Option<GraphNode>,
    ) -> StorageResult<()> {
        let index = self.nodes;
        if index >= self.old_nodes.len() || index >= self.new_nodes.len() {
            return Err(StorageError::Integrity("graph closed node window"));
        }
        self.old_nodes[index] = old;
        self.new_nodes[index] = new;
        self.nodes += 1;
        Ok(())
    }

    pub(crate) fn edge(
        &mut self,
        old: Option<GraphEdge>,
        new: Option<GraphEdge>,
    ) -> StorageResult<()> {
        let index = self.edges;
        if index >= self.old_edges.len() || index >= self.new_edges.len() {
            return Err(StorageError::Integrity("graph closed edge window"));
        }
        self.old_edges[index] = old;
        self.new_edges[index] = new;
        self.edges += 1;
        Ok(())
    }
}

pub(crate) struct Graph {
    pub(crate) memory: GraphMemory,
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
    // Construction returns the funded owner so its allocation and lease stay inseparable.
    #[allow(clippy::new_ret_no_self)]
    pub(crate) fn new(
        selection: &StateSelection,
        subject: GraphSubject,
        declared_seeds: u64,
        memory: GraphMemory,
        memory_lease: GraphMemoryLease,
    ) -> StorageResult<GraphOwner> {
        Ok(GraphOwner {
            value: Box::new(Self {
                memory,
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
            }),
            _memory: memory_lease,
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
                &attempt.mutation_codes[..attempt.logical_items.min(attempt.mutation_codes.len())], &attempt.selected_children[..attempt.logical_items.min(attempt.selected_children.len())],
                &attempt.selected_edges[..attempt.logical_items.min(attempt.selected_edges.len())], attempt.maximum_node, attempt.maximum_edge, attempt.seed_count));
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

/// Drop the actual boxed graph before returning its last-owner credit.
pub(crate) struct GraphOwner {
    value: Box<Graph>,
    _memory: GraphMemoryLease,
}
impl std::ops::Deref for GraphOwner {
    type Target = Graph;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}
impl std::ops::DerefMut for GraphOwner {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}
