//! Header-inclusive bounded selected pages; no rank or prefix-count query.
use super::{GraphAdjacencySeal, GraphEdge, GraphNode, GraphNodeKey, GraphProofSeal};
use crate::error::{ContentError, ContentResult};
/// Adjacency285/presence1/key25/count2/bytes4/EOF1.
pub const GRAPH_NODE_PAGE_HEADER_BYTES: usize = 318;
/// Proof317 plus the same33-byte selected suffix.
pub const GRAPH_PROOF_PAGE_HEADER_BYTES: usize = 350;
/// Adjacency285/parent8/presence1/child8/presence1/MAX8/count2/bytes4/EOF1.
pub const GRAPH_EDGE_PAGE_HEADER_BYTES: usize = 318;
/// Count and encoded bytes checked independently of actual native record layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphPageLimit {
    records: usize,
    bytes: usize,
}
impl Default for GraphPageLimit {
    fn default() -> Self {
        Self {
            records: 128,
            bytes: 65536,
        }
    }
}
impl GraphPageLimit {
    /// Fixed window; an exact318-byte adjacency/edge empty EOF is supported.
    pub fn new(records: usize, bytes: usize) -> ContentResult<Self> {
        if records == 0 || records > 128 || !(318..=65536).contains(&bytes) {
            return Err(bad("graph page limit"));
        }
        Ok(Self { records, bytes })
    }
    /// Count ceiling and maximum retained Vec capacity.
    pub const fn records(self) -> usize {
        self.records
    }
    /// Complete page byte ceiling.
    pub const fn bytes(self) -> usize {
        self.bytes
    }
    /// Node adjacency records that fit both bounds.
    pub fn fitting_records(self) -> usize {
        self.fitting_nodes()
    }
    /// Node adjacency records that fit both bounds.
    pub fn fitting_nodes(self) -> usize {
        self.fit(318, 60)
    }
    /// Parent edge records that fit both bounds.
    pub fn fitting_edges(self) -> usize {
        self.fit(318, 43)
    }
    /// Full proof records that fit both bounds.
    pub fn fitting_proof_records(self) -> usize {
        self.fit(350, 60)
    }
    fn fit(self, header: usize, width: usize) -> usize {
        self.records.min(self.bytes.saturating_sub(header) / width)
    }
    /// Exact chosen frame/header count bound before allocation/query.
    pub fn check(
        self,
        count: usize,
        capacity: usize,
        header: usize,
        width: usize,
    ) -> ContentResult<()> {
        if count > self.records || capacity > self.records {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "graph.page_records",
                limit: self.records as u64,
                actual: count.max(capacity) as u64,
            });
        }
        let actual = count
            .checked_mul(width)
            .and_then(|n| n.checked_add(header))
            .ok_or(ContentError::LengthOverflow)?;
        if actual > self.bytes {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "graph.page_bytes",
                limit: self.bytes as u64,
                actual: actual as u64,
            });
        }
        Ok(())
    }
}
/// One immutable normalized adjacency projection page.
#[derive(Debug)]
pub struct GraphNodePage {
    seal: GraphAdjacencySeal,
    records: Vec<GraphNode>,
    last: Option<GraphNodeKey>,
    eof: bool,
}
impl GraphNodePage {
    /// Exact continuation; empty EOF preserves actual prior selected key.
    pub fn after(
        seal: GraphAdjacencySeal,
        after: Option<GraphNodeKey>,
        records: Vec<GraphNode>,
        eof: bool,
    ) -> ContentResult<Self> {
        let last = check_nodes(seal.scope(), after, &records, eof, true)?;
        GraphPageLimit::default().check(records.len(), records.capacity(), 318, 60)?;
        Ok(Self {
            seal,
            records,
            last,
            eof,
        })
    }
    /// Exact selected seal.
    pub fn seal(&self) -> &GraphAdjacencySeal {
        &self.seal
    }
    /// Strict key-ordered normalized records.
    pub fn records(&self) -> &[GraphNode] {
        &self.records
    }
    /// Transfers this page allocation.
    pub fn into_records(self) -> Vec<GraphNode> {
        self.records
    }
    /// Last emitted or retained prior key.
    pub const fn last(&self) -> Option<GraphNodeKey> {
        self.last
    }
    /// Provider EOF, independently counted/hashed by the cursor.
    pub const fn eof(&self) -> bool {
        self.eof
    }
    /// Header-inclusive actual encoding.
    pub fn encoded_len(&self) -> usize {
        318 + self.records.len() * 60
    }
    /// Smaller caller limit and actual Vec capacity.
    pub fn check_limit(&self, limit: GraphPageLimit) -> ContentResult<()> {
        limit.check(self.records.len(), self.records.capacity(), 318, 60)
    }
    /// Exact selected header318.
    pub fn encode_header(&self) -> [u8; 318] {
        let mut b = [0; 318];
        b[..285].copy_from_slice(&self.seal.encode());
        node_suffix(&mut b[285..], self.last, self.records.len(), self.eof);
        b
    }
}
/// Full immutable current-node proof page.
#[derive(Debug)]
pub struct GraphProofPage {
    seal: GraphProofSeal,
    records: Vec<GraphNode>,
    last: Option<GraphNodeKey>,
    eof: bool,
}
impl GraphProofPage {
    /// Checks current node grammar, keys/progress/capacity under the exact proof.
    pub fn after(
        seal: GraphProofSeal,
        after: Option<GraphNodeKey>,
        records: Vec<GraphNode>,
        eof: bool,
    ) -> ContentResult<Self> {
        let last = check_nodes(seal.scope(), after, &records, eof, false)?;
        GraphPageLimit::default().check(records.len(), records.capacity(), 350, 60)?;
        Ok(Self {
            seal,
            records,
            last,
            eof,
        })
    }
    /// Exact selected immutable proof.
    pub fn seal(&self) -> &GraphProofSeal {
        &self.seal
    }
    /// Complete terminal full nodes.
    pub fn records(&self) -> &[GraphNode] {
        &self.records
    }
    /// Transfers the current bounded allocation.
    pub fn into_records(self) -> Vec<GraphNode> {
        self.records
    }
    /// Actual selected last key.
    pub const fn last(&self) -> Option<GraphNodeKey> {
        self.last
    }
    /// Exact EOF assertion, checked by the cursor.
    pub const fn eof(&self) -> bool {
        self.eof
    }
    /// Full header plus actual record frames.
    pub fn encoded_len(&self) -> usize {
        350 + self.records.len() * 60
    }
    /// Checks a smaller limit including actual Vec ownership.
    pub fn check_limit(&self, limit: GraphPageLimit) -> ContentResult<()> {
        limit.check(self.records.len(), self.records.capacity(), 350, 60)
    }
    /// Exact selected header350.
    pub fn encode_header(&self) -> [u8; 350] {
        let mut b = [0; 350];
        b[..317].copy_from_slice(&self.seal.encode());
        node_suffix(&mut b[317..], self.last, self.records.len(), self.eof);
        b
    }
}
/// One selected parent's immutable arcs and acknowledged MAX child.
#[derive(Debug)]
pub struct GraphEdgePage {
    seal: GraphAdjacencySeal,
    parent: u64,
    records: Vec<GraphEdge>,
    last: Option<u64>,
    maximum: Option<u64>,
    eof: bool,
}
impl GraphEdgePage {
    /// No prefix count/rank: exact EOF is last==acknowledged MAX, including empty.
    pub fn after(
        seal: GraphAdjacencySeal,
        parent: u64,
        after: Option<u64>,
        maximum: Option<u64>,
        records: Vec<GraphEdge>,
        eof: bool,
    ) -> ContentResult<Self> {
        super::GraphNodeKey::new(seal.scope(), parent)?;
        for serial in [after, maximum].into_iter().flatten() {
            super::GraphNodeKey::new(seal.scope(), serial)?;
        }
        GraphPageLimit::default().check(records.len(), records.capacity(), 318, 43)?;
        let mut last = after;
        for edge in &records {
            GraphEdge::decode(seal.scope(), &edge.encode())?;
            if edge.key().parent() != parent
                || last.is_some_and(|last| last >= edge.key().child())
                || maximum.is_none_or(|max| edge.key().child() > max)
            {
                return Err(bad("graph edge page order/MAX"));
            }
            last = Some(edge.key().child());
        }
        if (records.is_empty() && !eof)
            || eof != (last == maximum)
            || after.zip(maximum).is_some_and(|(a, m)| a > m)
            || maximum.is_none() && after.is_some()
        {
            return Err(bad("graph edge page EOF"));
        }
        Ok(Self {
            seal,
            parent,
            records,
            last,
            maximum,
            eof,
        })
    }
    /// Selected adjacency.
    pub fn seal(&self) -> &GraphAdjacencySeal {
        &self.seal
    }
    /// Exact selected parent.
    pub const fn parent(&self) -> u64 {
        self.parent
    }
    /// Complete strictly increasing arcs.
    pub fn records(&self) -> &[GraphEdge] {
        &self.records
    }
    /// Transfers bounded current allocation.
    pub fn into_records(self) -> Vec<GraphEdge> {
        self.records
    }
    /// Actual last child or prior empty-EOF continuation.
    pub const fn last(&self) -> Option<u64> {
        self.last
    }
    /// Acknowledged selected parent's MAX; not inferred from page length.
    pub const fn maximum(&self) -> Option<u64> {
        self.maximum
    }
    /// Exact last==MAX assertion.
    pub const fn eof(&self) -> bool {
        self.eof
    }
    /// Header plus full immutable frames.
    pub fn encoded_len(&self) -> usize {
        318 + self.records.len() * 43
    }
    /// Actual current Vec capacity and encoding check.
    pub fn check_limit(&self, limit: GraphPageLimit) -> ContentResult<()> {
        limit.check(self.records.len(), self.records.capacity(), 318, 43)
    }
    /// Exact selected parent header318.
    pub fn encode_header(&self) -> [u8; 318] {
        let mut b = [0; 318];
        b[..285].copy_from_slice(&self.seal.encode());
        b[285..293].copy_from_slice(&self.parent.to_be_bytes());
        if let Some(last) = self.last {
            b[293] = 1;
            b[294..302].copy_from_slice(&last.to_be_bytes());
        }
        if let Some(max) = self.maximum {
            b[302] = 1;
            b[303..311].copy_from_slice(&max.to_be_bytes());
        }
        b[311..313].copy_from_slice(&(self.records.len() as u16).to_be_bytes());
        b[313..317].copy_from_slice(&((self.records.len() * 43) as u32).to_be_bytes());
        b[317] = u8::from(self.eof);
        b
    }
}
fn check_nodes(
    scope: &super::GraphScope,
    after: Option<GraphNodeKey>,
    rows: &[GraphNode],
    eof: bool,
    projection: bool,
) -> ContentResult<Option<GraphNodeKey>> {
    if let Some(key) = after {
        GraphNodeKey::decode(scope, key.as_bytes())?;
    }
    if rows.is_empty() && !eof {
        return Err(bad("graph empty continuation"));
    }
    let mut last = after;
    for node in rows {
        GraphNode::decode(scope, &node.encode())?;
        if projection && node.projection() != *node || last.is_some_and(|key| key >= node.key()) {
            return Err(bad("graph node page order/projection"));
        }
        last = Some(node.key());
    }
    Ok(last)
}
fn node_suffix(b: &mut [u8], last: Option<GraphNodeKey>, count: usize, eof: bool) {
    if let Some(last) = last {
        b[0] = 1;
        b[1..26].copy_from_slice(last.as_bytes());
    }
    b[26..28].copy_from_slice(&(count as u16).to_be_bytes());
    b[28..32].copy_from_slice(&((count * 60) as u32).to_be_bytes());
    b[32] = u8::from(eof);
}
fn bad(message: &'static str) -> ContentError {
    ContentError::InvalidOrderingRecord(message)
}
