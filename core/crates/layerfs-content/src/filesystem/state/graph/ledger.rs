//! Fixed hash/count/order owners for complete canonical private graph streams.
use super::{GraphEdge, GraphEdgeKey, GraphNode, GraphNodeKey, GraphScope};
use crate::error::{ContentError, ContentResult};
/// One selected ordered node stream; hashing never keeps prior records.
pub struct GraphNodeLedger {
    scope: GraphScope,
    hash: blake3::Hasher,
    records: u64,
    last: Option<GraphNodeKey>,
    projection: bool,
}
impl GraphNodeLedger {
    /// Normalized immutable adjacency projection.
    pub fn adjacency(scope: GraphScope) -> Self {
        Self::new(scope, true)
    }
    /// Full terminal current-node proof.
    pub fn proof(scope: GraphScope) -> Self {
        Self::new(scope, false)
    }
    fn new(scope: GraphScope, projection: bool) -> Self {
        let mut hash = blake3::Hasher::new();
        hash.update(if projection {
            b"layerfs/effective-graph/adjacency-nodes/v1\0"
        } else {
            b"layerfs/effective-graph/proof-nodes/v1\0"
        });
        hash.update(&scope.as_bytes());
        Self {
            scope,
            hash,
            records: 0,
            last: None,
            projection,
        }
    }
    /// Checks all ordered frames before advancing acknowledged hash state.
    pub fn acknowledge(&mut self, rows: &[GraphNode]) -> ContentResult<()> {
        super::ack::check_window(rows.len(), rows.len(), 350 + rows.len() * 60)?;
        let mut last = self.last;
        for node in rows {
            GraphNode::decode(&self.scope, &node.encode())?;
            if self.projection && node.projection() != *node
                || last.is_some_and(|key| key >= node.key())
            {
                return Err(ContentError::InvalidOrderingRecord(
                    "graph node ledger order/projection",
                ));
            }
            last = Some(node.key());
        }
        let count = self
            .records
            .checked_add(rows.len() as u64)
            .ok_or(ContentError::LengthOverflow)?;
        if count > self.scope.capacity().records() {
            return Err(ContentError::InvalidOrderingRecord(
                "graph node ledger count",
            ));
        }
        for node in rows {
            self.hash.update(&node.encode());
        }
        self.records = count;
        self.last = last;
        Ok(())
    }
    /// Exact acknowledged count.
    pub const fn records(&self) -> u64 {
        self.records
    }
    /// Exact full framed bytes.
    pub const fn encoded_bytes(&self) -> u64 {
        self.records * 60
    }
    /// Last complete selected key.
    pub const fn last(&self) -> Option<GraphNodeKey> {
        self.last
    }
    /// Snapshots hash with count8/recordbytes8 terminal suffix.
    pub fn digest(&self) -> [u8; 32] {
        let mut hash = self.hash.clone();
        hash.update(&self.records.to_be_bytes());
        hash.update(&self.encoded_bytes().to_be_bytes());
        *hash.finalize().as_bytes()
    }
}
/// One selected ordered immutable edge stream.
pub struct GraphEdgeLedger {
    scope: GraphScope,
    hash: blake3::Hasher,
    records: u64,
    last: Option<GraphEdgeKey>,
    multiplicity: u64,
}
impl GraphEdgeLedger {
    /// Begins a complete key-ordered arc transcript.
    pub fn new(scope: GraphScope) -> Self {
        let mut hash = blake3::Hasher::new();
        hash.update(b"layerfs/effective-graph/adjacency-edges/v1\0");
        hash.update(&scope.as_bytes());
        Self {
            scope,
            hash,
            records: 0,
            last: None,
            multiplicity: 0,
        }
    }
    /// Advances only after all current records/order/counts were checked.
    pub fn acknowledge(&mut self, rows: &[GraphEdge]) -> ContentResult<()> {
        super::ack::check_window(rows.len(), rows.len(), 318 + rows.len() * 43)?;
        let mut last = self.last;
        let mut multiplicity = self.multiplicity;
        for edge in rows {
            GraphEdge::decode(&self.scope, &edge.encode())?;
            if last.is_some_and(|key| key >= edge.key()) {
                return Err(ContentError::InvalidOrderingRecord(
                    "graph edge ledger order",
                ));
            }
            last = Some(edge.key());
            multiplicity = multiplicity
                .checked_add(u64::from(edge.multiplicity()))
                .ok_or(ContentError::LengthOverflow)?;
        }
        let count = self
            .records
            .checked_add(rows.len() as u64)
            .ok_or(ContentError::LengthOverflow)?;
        if count > self.scope.capacity().records() {
            return Err(ContentError::InvalidOrderingRecord(
                "graph edge ledger count",
            ));
        }
        for edge in rows {
            self.hash.update(&edge.encode());
        }
        self.records = count;
        self.last = last;
        self.multiplicity = multiplicity;
        Ok(())
    }
    /// Distinct acknowledged arcs.
    pub const fn records(&self) -> u64 {
        self.records
    }
    /// Full encoded bytes.
    pub const fn encoded_bytes(&self) -> u64 {
        self.records * 43
    }
    /// Exact source arc multiplicity.
    pub const fn multiplicity(&self) -> u64 {
        self.multiplicity
    }
    /// Last exact full key.
    pub const fn last(&self) -> Option<GraphEdgeKey> {
        self.last
    }
    /// Domain/scope/records/count8/bytes8 digest.
    pub fn digest(&self) -> [u8; 32] {
        let mut h = self.hash.clone();
        h.update(&self.records.to_be_bytes());
        h.update(&self.encoded_bytes().to_be_bytes());
        *h.finalize().as_bytes()
    }
}
