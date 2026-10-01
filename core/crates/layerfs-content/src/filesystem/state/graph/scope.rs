//! One live issued selection and its exact static graph subject.
use super::{GraphCapacity, GraphSubject};
use crate::error::ContentResult;
use crate::filesystem::state::{StateScope, StateSelection, StateTable};
/// Node StateScope81/edge-code1/subject106.
pub const GRAPH_SCOPE_BYTES: usize = 188;
/// Checked two-codec context belonging to the same operation and native owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphScope {
    nodes: StateScope,
    edges: StateScope,
    subject: GraphSubject,
}
impl GraphScope {
    /// Selects only the frozen phase2/table4+5 graph codecs.
    pub fn new(selection: StateSelection, subject: GraphSubject) -> ContentResult<Self> {
        Ok(Self {
            nodes: StateScope::new(selection.clone(), 2, StateTable::GraphNodes)?,
            edges: StateScope::new(selection, 2, StateTable::GraphEdges)?,
            subject,
        })
    }
    /// Exact node codec scope.
    pub fn nodes(&self) -> &StateScope {
        &self.nodes
    }
    /// Exact edge codec scope.
    pub fn edges(&self) -> &StateScope {
        &self.edges
    }
    /// Full immutable static subject.
    pub fn subject(&self) -> &GraphSubject {
        &self.subject
    }
    /// One aggregate graph capacity.
    pub fn capacity(&self) -> GraphCapacity {
        self.subject.capacity()
    }
    /// Frozen188-byte full context.
    pub fn as_bytes(&self) -> [u8; GRAPH_SCOPE_BYTES] {
        let mut b = [0; GRAPH_SCOPE_BYTES];
        b[..81].copy_from_slice(&self.nodes.as_bytes());
        b[81] = 5;
        b[82..].copy_from_slice(&self.subject.encode());
        b
    }
}
