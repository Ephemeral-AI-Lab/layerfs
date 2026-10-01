//! Exact bounded old/new acknowledgements; no population-sized result.
use super::{GraphEdge, GraphNode, GraphNodeKey, GraphScope, GraphTotals};
use crate::error::{ContentError, ContentResult};
/// One exact native node change, including births.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphNodeChange {
    before: Option<GraphNode>,
    after: GraphNode,
}
impl GraphNodeChange {
    /// Before/after must identify the same selected node.
    pub fn new(
        scope: &GraphScope,
        before: Option<GraphNode>,
        after: GraphNode,
    ) -> ContentResult<Self> {
        GraphNode::decode(scope, &after.encode())?;
        if let Some(old) = before {
            GraphNode::decode(scope, &old.encode())?;
            if old.key() != after.key() {
                return Err(bad("graph node change key"));
            }
        }
        Ok(Self { before, after })
    }
    /// Exact previously acknowledged row, absent only for birth.
    pub const fn before(self) -> Option<GraphNode> {
        self.before
    }
    /// Exact newly acknowledged row.
    pub const fn after(self) -> GraphNode {
        self.after
    }
}
/// One exact native edge change, including births.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphEdgeChange {
    before: Option<GraphEdge>,
    after: GraphEdge,
}
impl GraphEdgeChange {
    /// Before/after must identify the same selected arc.
    pub fn new(
        scope: &GraphScope,
        before: Option<GraphEdge>,
        after: GraphEdge,
    ) -> ContentResult<Self> {
        GraphEdge::decode(scope, &after.encode())?;
        if let Some(old) = before {
            GraphEdge::decode(scope, &old.encode())?;
            if old.key() != after.key() {
                return Err(bad("graph edge change key"));
            }
        }
        Ok(Self { before, after })
    }
    /// Exact old arc or absent before a birth.
    pub const fn before(self) -> Option<GraphEdge> {
        self.before
    }
    /// Exact acknowledged arc.
    pub const fn after(self) -> GraphEdge {
        self.after
    }
}
/// Native build acknowledgement with exact combined affected window and totals.
#[derive(Debug)]
pub struct GraphBuildAck {
    scope: GraphScope,
    before: GraphTotals,
    after: GraphTotals,
    nodes: Vec<GraphNodeChange>,
    edges: Vec<GraphEdgeChange>,
    parent: Option<GraphNode>,
}
impl GraphBuildAck {
    /// Checks selected scope, ordered unique targets, full old/new windows and totals.
    pub fn new(
        scope: GraphScope,
        before: GraphTotals,
        after: GraphTotals,
        nodes: Vec<GraphNodeChange>,
        edges: Vec<GraphEdgeChange>,
        parent: Option<GraphNode>,
    ) -> ContentResult<Self> {
        check_window(
            nodes.len() + edges.len(),
            nodes.capacity() + edges.capacity(),
            317 + nodes.len() * 120 + edges.len() * 86,
        )?;
        before.check(scope.capacity())?;
        after.check(scope.capacity())?;
        let mut last = None;
        let mut added_nodes = 0;
        for row in &nodes {
            GraphNodeChange::new(&scope, row.before(), row.after())?;
            if last.is_some_and(|key| key >= row.after().key()) {
                return Err(bad("graph node ack order"));
            }
            last = Some(row.after().key());
            added_nodes += u64::from(row.before().is_none());
        }
        let mut last = None;
        let mut added_edges = 0;
        for row in &edges {
            GraphEdgeChange::new(&scope, row.before(), row.after())?;
            if last.is_some_and(|key| key >= row.after().key()) {
                return Err(bad("graph edge ack order"));
            }
            last = Some(row.after().key());
            added_edges += u64::from(row.before().is_none());
        }
        if before.nodes().checked_add(added_nodes) != Some(after.nodes())
            || before.edges().checked_add(added_edges) != Some(after.edges())
            || after.multiplicity() < before.multiplicity()
        {
            return Err(bad("graph ack totals"));
        }
        if let Some(parent) = parent {
            GraphNode::decode(&scope, &parent.encode())?;
            if !nodes.iter().any(|row| row.after() == parent) {
                return Err(bad("graph ack parent"));
            }
        }
        Ok(Self {
            scope,
            before,
            after,
            nodes,
            edges,
            parent,
        })
    }
    /// Selected graph context.
    pub fn scope(&self) -> &GraphScope {
        &self.scope
    }
    /// Acknowledged totals before this batch.
    pub const fn before(&self) -> GraphTotals {
        self.before
    }
    /// Acknowledged totals after this batch.
    pub const fn after(&self) -> GraphTotals {
        self.after
    }
    /// Exact ordered node changes, parent included when supplied.
    pub fn nodes(&self) -> &[GraphNodeChange] {
        &self.nodes
    }
    /// Exact ordered arc changes.
    pub fn edges(&self) -> &[GraphEdgeChange] {
        &self.edges
    }
    /// Updated parent after a construction append.
    pub const fn parent(&self) -> Option<GraphNode> {
        self.parent
    }
    /// Distinct node births.
    pub fn added_nodes(&self) -> u64 {
        self.after.nodes() - self.before.nodes()
    }
    /// Distinct edge births.
    pub fn added_edges(&self) -> u64 {
        self.after.edges() - self.before.edges()
    }
}
/// Exact acknowledged solver targets, including unchanged legal transitions.
#[derive(Debug)]
pub struct GraphMutationAck {
    scope: GraphScope,
    nodes: Vec<GraphNodeChange>,
}
impl GraphMutationAck {
    /// Checks one bounded unique selected target window.
    pub fn new(scope: GraphScope, nodes: Vec<GraphNodeChange>) -> ContentResult<Self> {
        check_window(nodes.len(), nodes.capacity(), 317 + nodes.len() * 120)?;
        let mut last = None;
        for row in &nodes {
            GraphNodeChange::new(&scope, row.before(), row.after())?;
            if row.before().is_none() || last.is_some_and(|key| key >= row.after().key()) {
                return Err(bad("graph mutation ack"));
            }
            last = Some(row.after().key());
        }
        Ok(Self { scope, nodes })
    }
    /// Selected graph.
    pub fn scope(&self) -> &GraphScope {
        &self.scope
    }
    /// Ordered exact old/new node targets.
    pub fn nodes(&self) -> &[GraphNodeChange] {
        &self.nodes
    }
}
/// Known selected component disposition after the acknowledged window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphPopDisposition {
    /// More native stack members remain in this component.
    More,
    /// The exact component ended with no seeded cycle.
    Complete,
    /// Known last COMMIT/observation rejected a cyclic Seed component.
    RejectedCycle,
}
/// Fixed cumulative metadata; members independently prove these predicates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphPopTotals {
    popped: u64,
    any_seed: bool,
    singleton_self_loop: bool,
}
impl GraphPopTotals {
    /// Singleton loop is meaningful only at exactly one popped member.
    pub fn new(popped: u64, any_seed: bool, singleton_self_loop: bool) -> ContentResult<Self> {
        if singleton_self_loop && popped != 1 {
            return Err(bad("graph pop singleton total"));
        }
        Ok(Self {
            popped,
            any_seed,
            singleton_self_loop,
        })
    }
    /// Exact cumulative member count.
    pub const fn popped(self) -> u64 {
        self.popped
    }
    /// Cumulative Seed OR.
    pub const fn any_seed(self) -> bool {
        self.any_seed
    }
    /// Sole-member actual self-loop, false when count exceeds one.
    pub const fn singleton_self_loop(self) -> bool {
        self.singleton_self_loop
    }
}
/// A bounded completed-member window, never a whole-component collection.
#[derive(Debug)]
pub struct GraphPopAck {
    scope: GraphScope,
    root: GraphNodeKey,
    component: u32,
    members: Vec<GraphNode>,
    popped: u64,
    any_seed: bool,
    singleton_self_loop: bool,
    disposition: GraphPopDisposition,
}
impl GraphPopAck {
    /// Checks completed records, descending discovery and terminal root membership.
    pub fn new(
        scope: GraphScope,
        root: GraphNodeKey,
        component: u32,
        members: Vec<GraphNode>,
        totals: GraphPopTotals,
        disposition: GraphPopDisposition,
    ) -> ContentResult<Self> {
        let popped = totals.popped();
        let any_seed = totals.any_seed();
        let singleton_self_loop = totals.singleton_self_loop();
        check_window(members.len(), members.capacity(), 350 + members.len() * 60)?;
        GraphNodeKey::decode(&scope, root.as_bytes())?;
        if members.is_empty()
            || component == 0
            || popped < members.len() as u64
            || popped > scope.capacity().records()
        {
            return Err(bad("graph pop totals"));
        }
        let mut last = u32::MAX;
        for node in &members {
            GraphNode::decode(&scope, &node.encode())?;
            if !node.completed()
                || !node.dfs_finished()
                || node.lowlink() != component
                || node.discovery() >= last
            {
                return Err(bad("graph pop member"));
            }
            last = node.discovery();
        }
        if disposition != GraphPopDisposition::More
            && members
                .last()
                .is_none_or(|node| node.key() != root || node.discovery() != component)
        {
            return Err(bad("graph pop terminal root"));
        }
        if disposition == GraphPopDisposition::RejectedCycle
            && (!any_seed || !(popped > 1 || singleton_self_loop))
        {
            return Err(bad("graph pop rejection"));
        }
        Ok(Self {
            scope,
            root,
            component,
            members,
            popped,
            any_seed,
            singleton_self_loop,
            disposition,
        })
    }
    /// Selected graph.
    pub fn scope(&self) -> &GraphScope {
        &self.scope
    }
    /// Exact SCC root key.
    pub const fn root(&self) -> GraphNodeKey {
        self.root
    }
    /// Assigned component root discovery.
    pub const fn component(&self) -> u32 {
        self.component
    }
    /// Actual completed members in this window.
    pub fn members(&self) -> &[GraphNode] {
        &self.members
    }
    /// Cumulative members popped in this component.
    pub const fn popped(&self) -> u64 {
        self.popped
    }
    /// Native cumulative Seed predicate, independently folded by C1.
    pub const fn any_seed(&self) -> bool {
        self.any_seed
    }
    /// Native singleton self-loop predicate, independently folded by C1.
    pub const fn singleton_self_loop(&self) -> bool {
        self.singleton_self_loop
    }
    /// Known window disposition, not a substitute for member proof.
    pub const fn disposition(&self) -> GraphPopDisposition {
        self.disposition
    }
}
pub(super) fn check_window(count: usize, capacity: usize, bytes: usize) -> ContentResult<()> {
    if count > 128 || capacity > 128 {
        return Err(ContentError::BoundedCapacityExceeded {
            what: "graph.window_records",
            limit: 128,
            actual: count.max(capacity) as u64,
        });
    }
    if bytes > 65536 {
        return Err(ContentError::BoundedCapacityExceeded {
            what: "graph.window_bytes",
            limit: 65536,
            actual: bytes as u64,
        });
    }
    Ok(())
}
fn bad(reason: &'static str) -> ContentError {
    ContentError::InvalidOrderingRecord(reason)
}
