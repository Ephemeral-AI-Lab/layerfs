//! Immutable effective arcs with exact checked multiplicity.
use super::{GraphEdgeKey, GraphScope};
use crate::error::{ContentError, ContentResult};
/// key33/value4/framing6.
pub const GRAPH_EDGE_BYTES: usize = 43;
/// One ordered outgoing arc; repeated names contribute multiplicity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphEdge {
    key: GraphEdgeKey,
    multiplicity: u32,
}
impl GraphEdge {
    /// Both endpoints are positive and multiplicity is nonzero.
    pub fn new(
        scope: &GraphScope,
        parent: u64,
        child: u64,
        multiplicity: u32,
    ) -> ContentResult<Self> {
        if multiplicity == 0 {
            return Err(ContentError::InvalidOrderingRecord(
                "graph edge multiplicity",
            ));
        }
        Ok(Self {
            key: GraphEdgeKey::new(scope, parent, child)?,
            multiplicity,
        })
    }
    /// Full selected frame43.
    pub fn decode(scope: &GraphScope, b: &[u8]) -> ContentResult<Self> {
        if b.len() != 43 || b[..2] != 33u16.to_be_bytes() || b[35..39] != 4u32.to_be_bytes() {
            return Err(ContentError::InvalidOrderingRecord("graph edge framing"));
        }
        let key = GraphEdgeKey::decode(scope, &b[2..35])?;
        Self::new(
            scope,
            key.parent(),
            key.child(),
            u32::from_be_bytes(b[39..].try_into().unwrap()),
        )
    }
    /// Native value width checked before constructing the full frame.
    pub fn decode_value(scope: &GraphScope, key: GraphEdgeKey, b: &[u8]) -> ContentResult<Self> {
        if b.len() != 4 {
            return Err(ContentError::InvalidOrderingRecord(
                "graph edge value width",
            ));
        }
        GraphEdgeKey::decode(scope, key.as_bytes())?;
        Self::new(
            scope,
            key.parent(),
            key.child(),
            u32::from_be_bytes(b.try_into().unwrap()),
        )
    }
    /// Full frame43.
    pub fn encode(self) -> [u8; 43] {
        let mut b = [0; 43];
        b[..2].copy_from_slice(&33u16.to_be_bytes());
        b[2..35].copy_from_slice(self.key.as_bytes());
        b[35..39].copy_from_slice(&4u32.to_be_bytes());
        b[39..].copy_from_slice(&self.value());
        b
    }
    /// Native exact BLOB4.
    pub const fn value(self) -> [u8; 4] {
        self.multiplicity.to_be_bytes()
    }
    /// Complete selected edge key.
    pub const fn key(self) -> GraphEdgeKey {
        self.key
    }
    /// Positive occurrence count.
    pub const fn multiplicity(self) -> u32 {
        self.multiplicity
    }
    /// Checked acknowledged construction increment.
    pub fn add(self, scope: &GraphScope, increment: u32) -> ContentResult<Self> {
        Self::new(
            scope,
            self.key.parent(),
            self.key.child(),
            self.multiplicity
                .checked_add(increment)
                .ok_or(ContentError::LengthOverflow)?,
        )
    }
}
