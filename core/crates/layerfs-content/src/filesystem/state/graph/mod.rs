//! Closed effective graph formats and provider ports; declarations only.
mod ack;
mod capacity;
mod construction;
mod cursor;
mod edge;
mod keys;
mod ledger;
mod mutation;
mod node;
mod page;
mod port;
mod scope;
mod seal;
mod stage;
mod subject;
mod totals;

pub use capacity::{GraphCapacity, DEFAULT_GRAPH_SCRATCH_BYTES, MAXIMUM_GRAPH_SCRATCH_BYTES};
pub use construction::{GraphConstructionScopes, GraphConstructionState};
pub use edge::{GraphEdge, GRAPH_EDGE_BYTES};
pub use keys::{GraphEdgeKey, GraphNodeKey, GRAPH_EDGE_KEY_BYTES, GRAPH_NODE_KEY_BYTES};
pub use node::{GraphNode, GRAPH_NODE_BYTES};
pub use scope::{GraphScope, GRAPH_SCOPE_BYTES};
pub use stage::GraphStage;
pub use subject::{GraphMode, GraphSubject, GRAPH_SUBJECT_BYTES};
pub use totals::GraphTotals;

pub use ack::{
    GraphBuildAck, GraphEdgeChange, GraphMutationAck, GraphNodeChange, GraphPopAck,
    GraphPopDisposition, GraphPopTotals,
};
pub use ledger::{GraphEdgeLedger, GraphNodeLedger};
pub use mutation::{GraphMutation, GraphMutationLimit};
pub use page::{
    GraphEdgePage, GraphNodePage, GraphPageLimit, GraphProofPage, GRAPH_EDGE_PAGE_HEADER_BYTES,
    GRAPH_NODE_PAGE_HEADER_BYTES, GRAPH_PROOF_PAGE_HEADER_BYTES,
};
pub use port::EffectiveGraphState;
pub use seal::{
    GraphAdjacencySeal, GraphProofSeal, GRAPH_ADJACENCY_SEAL_BYTES, GRAPH_PROOF_SEAL_BYTES,
};

pub use cursor::{GraphEdgeCursor, GraphNodeCursor, GraphProofCursor};
