//! Full selected keys; no truncation or interchangeable table codecs.
use super::GraphScope;
use crate::error::{ContentError, ContentResult};
/// token8/phase8/table4/serial8.
pub const GRAPH_NODE_KEY_BYTES: usize = 25;
/// token8/phase8/table5/parent8/child8.
pub const GRAPH_EDGE_KEY_BYTES: usize = 33;
/// Exact node key in one live graph context.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct GraphNodeKey([u8; 25]);
impl GraphNodeKey {
    /// Issues a positive node serial under the selected prefix.
    pub fn new(scope: &GraphScope, serial: u64) -> ContentResult<Self> {
        check_serial(serial)?;
        let mut b = [0; 25];
        b[..8].copy_from_slice(&scope.nodes().selection().token().to_be_bytes());
        b[8..16].copy_from_slice(&scope.nodes().phase().to_be_bytes());
        b[16] = 4;
        b[17..].copy_from_slice(&serial.to_be_bytes());
        Ok(Self(b))
    }
    /// Checks complete width, prefix and positive serial.
    pub fn decode(scope: &GraphScope, b: &[u8]) -> ContentResult<Self> {
        if b.len() != 25 {
            return Err(ContentError::InvalidOrderingRecord("graph node key width"));
        }
        let key = Self::new(scope, u64::from_be_bytes(b[17..].try_into().unwrap()))?;
        if key.0 != b {
            return Err(ContentError::InvalidOrderingRecord("graph node key scope"));
        }
        Ok(key)
    }
    /// Complete selected key.
    pub const fn as_bytes(&self) -> &[u8; 25] {
        &self.0
    }
    /// Positive local directory serial.
    pub fn serial(self) -> u64 {
        u64::from_be_bytes(self.0[17..].try_into().unwrap())
    }
}
/// Exact ordered outgoing arc key, parent then child.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct GraphEdgeKey([u8; 33]);
impl GraphEdgeKey {
    /// Requires both complete positive endpoint serials.
    pub fn new(scope: &GraphScope, parent: u64, child: u64) -> ContentResult<Self> {
        check_serial(parent)?;
        check_serial(child)?;
        let mut b = [0; 33];
        b[..8].copy_from_slice(&scope.edges().selection().token().to_be_bytes());
        b[8..16].copy_from_slice(&scope.edges().phase().to_be_bytes());
        b[16] = 5;
        b[17..25].copy_from_slice(&parent.to_be_bytes());
        b[25..].copy_from_slice(&child.to_be_bytes());
        Ok(Self(b))
    }
    /// Exact edge prefix/width/range checks before value access.
    pub fn decode(scope: &GraphScope, b: &[u8]) -> ContentResult<Self> {
        if b.len() != 33 {
            return Err(ContentError::InvalidOrderingRecord("graph edge key width"));
        }
        let key = Self::new(
            scope,
            u64::from_be_bytes(b[17..25].try_into().unwrap()),
            u64::from_be_bytes(b[25..].try_into().unwrap()),
        )?;
        if key.0 != b {
            return Err(ContentError::InvalidOrderingRecord("graph edge key scope"));
        }
        Ok(key)
    }
    /// Complete ordered key.
    pub const fn as_bytes(&self) -> &[u8; 33] {
        &self.0
    }
    /// Positive parent serial.
    pub fn parent(self) -> u64 {
        u64::from_be_bytes(self.0[17..25].try_into().unwrap())
    }
    /// Positive child serial.
    pub fn child(self) -> u64 {
        u64::from_be_bytes(self.0[25..].try_into().unwrap())
    }
}
pub(super) fn check_serial(serial: u64) -> ContentResult<()> {
    if serial == 0 || serial > i64::MAX as u64 {
        return Err(ContentError::InvalidOrderingRecord("graph serial"));
    }
    Ok(())
}
