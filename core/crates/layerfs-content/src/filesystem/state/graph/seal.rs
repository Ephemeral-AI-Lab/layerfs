//! Private ordered adjacency and terminal proof transcripts.
use super::{GraphScope, GraphTotals};
use crate::error::{ContentError, ContentResult};
/// version1/scope188/four totals8/two digests32.
pub const GRAPH_ADJACENCY_SEAL_BYTES: usize = 285;
/// Adjacency-sized proof followed by the exact adjacency transcript digest.
pub const GRAPH_PROOF_SEAL_BYTES: usize = 317;
/// Immutable normalized adjacency stream, selected before solving.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphAdjacencySeal {
    scope: GraphScope,
    nodes: u64,
    edges: u64,
    node_digest: [u8; 32],
    edge_digest: [u8; 32],
}
impl GraphAdjacencySeal {
    /// Builds a seal from separately streamed acknowledged hashes, not semantic expected roots.
    pub fn new(
        scope: GraphScope,
        nodes: u64,
        edges: u64,
        node_digest: [u8; 32],
        edge_digest: [u8; 32],
    ) -> ContentResult<Self> {
        GraphTotals::new(nodes, edges, edges)?.check(scope.capacity())?;
        Ok(Self {
            scope,
            nodes,
            edges,
            node_digest,
            edge_digest,
        })
    }
    /// Exact selected scope.
    pub fn scope(&self) -> &GraphScope {
        &self.scope
    }
    /// Terminal vertex count.
    pub const fn nodes(&self) -> u64 {
        self.nodes
    }
    /// Terminal distinct arc count.
    pub const fn edges(&self) -> u64 {
        self.edges
    }
    /// Full projected Node60 bytes.
    pub const fn node_bytes(&self) -> u64 {
        self.nodes * 60
    }
    /// Full Edge43 bytes.
    pub const fn edge_bytes(&self) -> u64 {
        self.edges * 43
    }
    /// Complete projected-node transcript digest.
    pub const fn node_digest(&self) -> &[u8; 32] {
        &self.node_digest
    }
    /// Complete immutable edge transcript digest.
    pub const fn edge_digest(&self) -> &[u8; 32] {
        &self.edge_digest
    }
    /// Exact frame285; no provider-independent semantic verdict is implied.
    pub fn encode(&self) -> [u8; 285] {
        let mut b = [0; 285];
        encode_prefix(
            &mut b,
            &self.scope,
            self.nodes,
            self.edges,
            &self.node_digest,
            &self.edge_digest,
        );
        b
    }
    /// Checks exact width/context and terminal arithmetic before exposure.
    pub fn decode(scope: &GraphScope, b: &[u8]) -> ContentResult<Self> {
        check_prefix(scope, b, 285)?;
        Self::new(
            scope.clone(),
            u64::from_be_bytes(b[189..197].try_into().unwrap()),
            u64::from_be_bytes(b[197..205].try_into().unwrap()),
            b[221..253].try_into().unwrap(),
            b[253..285].try_into().unwrap(),
        )
    }
    /// Domain-separated digest of this exact285-byte adjacency seal.
    pub fn digest(&self) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(b"layerfs/effective-graph/adjacency-seal/v1\0");
        h.update(&self.encode());
        *h.finalize().as_bytes()
    }
}
/// Immutable full current-node proof bound to a verified adjacency seal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphProofSeal {
    adjacency: GraphAdjacencySeal,
    node_digest: [u8; 32],
}
impl GraphProofSeal {
    /// Native final full-node stream; immutable adjacency is retained exactly.
    pub fn new(adjacency: GraphAdjacencySeal, node_digest: [u8; 32]) -> Self {
        Self {
            adjacency,
            node_digest,
        }
    }
    /// Exact selected graph.
    pub fn scope(&self) -> &GraphScope {
        self.adjacency.scope()
    }
    /// Terminal vertex count.
    pub fn nodes(&self) -> u64 {
        self.adjacency.nodes()
    }
    /// Terminal edge count.
    pub fn edges(&self) -> u64 {
        self.adjacency.edges()
    }
    /// Full current node bytes.
    pub fn node_bytes(&self) -> u64 {
        self.adjacency.node_bytes()
    }
    /// Immutable arc bytes.
    pub fn edge_bytes(&self) -> u64 {
        self.adjacency.edge_bytes()
    }
    /// Final full-node stream digest.
    pub const fn node_digest(&self) -> &[u8; 32] {
        &self.node_digest
    }
    /// Immutable edge stream digest.
    pub fn edge_digest(&self) -> &[u8; 32] {
        self.adjacency.edge_digest()
    }
    /// Exact adjacency whose projection was revalidated before proof.
    pub fn adjacency(&self) -> &GraphAdjacencySeal {
        &self.adjacency
    }
    /// Terminal frame317.
    pub fn encode(&self) -> [u8; 317] {
        let mut b = [0; 317];
        encode_prefix(
            &mut b,
            self.scope(),
            self.nodes(),
            self.edges(),
            &self.node_digest,
            self.edge_digest(),
        );
        b[285..].copy_from_slice(&self.adjacency.digest());
        b
    }
    /// Decodes only against the exact acknowledged adjacency; never guessed adoption.
    pub fn decode(adjacency: &GraphAdjacencySeal, b: &[u8]) -> ContentResult<Self> {
        check_prefix(adjacency.scope(), b, 317)?;
        if b[189..221] != adjacency.encode()[189..221]
            || b[253..285] != *adjacency.edge_digest()
            || b[285..] != adjacency.digest()
        {
            return Err(ContentError::InvalidOrderingRecord("graph proof adjacency"));
        }
        Ok(Self::new(
            adjacency.clone(),
            b[221..253].try_into().unwrap(),
        ))
    }
}
fn encode_prefix(
    b: &mut [u8],
    scope: &GraphScope,
    nodes: u64,
    edges: u64,
    nd: &[u8; 32],
    ed: &[u8; 32],
) {
    b[0] = 1;
    b[1..189].copy_from_slice(&scope.as_bytes());
    b[189..197].copy_from_slice(&nodes.to_be_bytes());
    b[197..205].copy_from_slice(&edges.to_be_bytes());
    b[205..213].copy_from_slice(&(nodes * 60).to_be_bytes());
    b[213..221].copy_from_slice(&(edges * 43).to_be_bytes());
    b[221..253].copy_from_slice(nd);
    b[253..285].copy_from_slice(ed);
}
fn check_prefix(scope: &GraphScope, b: &[u8], width: usize) -> ContentResult<()> {
    if b.len() != width || b[0] != 1 || b[1..189] != scope.as_bytes() {
        return Err(ContentError::InvalidOrderingRecord("graph seal framing"));
    }
    let n = u64::from_be_bytes(b[189..197].try_into().unwrap());
    let e = u64::from_be_bytes(b[197..205].try_into().unwrap());
    if n.checked_mul(60) != Some(u64::from_be_bytes(b[205..213].try_into().unwrap()))
        || e.checked_mul(43) != Some(u64::from_be_bytes(b[213..221].try_into().unwrap()))
    {
        return Err(ContentError::InvalidOrderingRecord("graph seal totals"));
    }
    GraphTotals::new(n, e, e)?.check(scope.capacity())
}
