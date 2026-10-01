//! Fixed acknowledged graph totals; no population retained by the producer.
use super::GraphCapacity;
use crate::error::{ContentError, ContentResult};
/// Distinct records and source multiplicity, separately accounted.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GraphTotals {
    nodes: u64,
    edges: u64,
    multiplicity: u64,
}
impl GraphTotals {
    /// Validates arithmetic and the relation to distinct edge records.
    pub fn new(nodes: u64, edges: u64, multiplicity: u64) -> ContentResult<Self> {
        nodes
            .checked_mul(60)
            .and_then(|a| edges.checked_mul(43).and_then(|b| a.checked_add(b)))
            .ok_or(ContentError::LengthOverflow)?;
        if multiplicity < edges || (edges == 0) != (multiplicity == 0) {
            return Err(ContentError::InvalidOrderingRecord(
                "graph multiplicity totals",
            ));
        }
        Ok(Self {
            nodes,
            edges,
            multiplicity,
        })
    }
    /// Distinct directory vertices.
    pub const fn nodes(self) -> u64 {
        self.nodes
    }
    /// Distinct parent/child arcs.
    pub const fn edges(self) -> u64 {
        self.edges
    }
    /// All source directory-arc occurrences, including duplicate endpoints.
    pub const fn multiplicity(self) -> u64 {
        self.multiplicity
    }
    /// Complete current Node60 framed population.
    pub const fn node_bytes(self) -> u64 {
        self.nodes * 60
    }
    /// Complete current Edge43 framed population.
    pub const fn edge_bytes(self) -> u64 {
        self.edges * 43
    }
    /// Aggregate full current frames.
    pub const fn encoded_bytes(self) -> u64 {
        self.node_bytes() + self.edge_bytes()
    }
    /// Same selected capacity across both tables.
    pub fn check(self, capacity: GraphCapacity) -> ContentResult<()> {
        capacity.check_growth(self.nodes, self.edges, 0, 0)
    }
}
