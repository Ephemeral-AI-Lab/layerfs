//! Closed expected/proposed DFS transitions; native checks selected adjacency.
use super::{GraphEdge, GraphNode, GraphNodeKey, GraphScope};
use crate::error::{ContentError, ContentResult};
/// Only these transitions can change full Node60 solver state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphMutation {
    /// Any White root, exact native next discovery.
    EnterRoot {
        /// Exact previously acknowledged White row.
        before: GraphNode,
        /// Exact proposed parentless next-rank entry.
        after: GraphNode,
    },
    /// One FIRST edge and atomic parent/White child transition.
    Descend {
        /// Exact current parent expected before the atomic transition.
        parent_before: GraphNode,
        /// Exact proposed current parent after the atomic transition.
        parent_after: GraphNode,
        /// Exact White child expected before entry.
        child_before: GraphNode,
        /// Exact child entry proposed atomically with its parent.
        child_after: GraphNode,
        /// Exact selected FIRST immutable outgoing arc.
        edge: GraphEdge,
    },
    /// Selected FIRST edge to an entered/completed child.
    Advance {
        /// Exact previously acknowledged current row.
        before: GraphNode,
        /// Exact proposed closed-transition row.
        after: GraphNode,
        /// Exact selected FIRST immutable outgoing arc.
        edge: GraphEdge,
    },
    /// Native outgoing EOF followed by DfsFinished.
    Finish {
        /// Exact current active unfinished row.
        before: GraphNode,
        /// Exact DfsFinished proposal after native outgoing EOF.
        after: GraphNode,
    },
    /// Exact finished tree child returns to its live parent.
    Return {
        /// Exact current parent expected before the atomic transition.
        parent_before: GraphNode,
        /// Exact proposed current parent after the atomic transition.
        parent_after: GraphNode,
        /// Exact finished tree-child key checked by the native owner.
        child: GraphNodeKey,
    },
    /// Metadata-only departure from a completed DFS root.
    LeaveRoot {
        /// Exact completed parentless DFS root at native departure.
        finished_root: GraphNode,
    },
}
impl GraphMutation {
    /// Bounded expected/proposed pairs; second pair exists only for Descend.
    pub fn changes(self) -> [(Option<GraphNode>, Option<GraphNode>); 2] {
        match self {
            Self::EnterRoot { before, after }
            | Self::Advance { before, after, .. }
            | Self::Finish { before, after } => [(Some(before), Some(after)), (None, None)],
            Self::Descend {
                parent_before,
                parent_after,
                child_before,
                child_after,
                ..
            } => [
                (Some(parent_before), Some(parent_after)),
                (Some(child_before), Some(child_after)),
            ],
            Self::Return {
                parent_before,
                parent_after,
                ..
            } => [(Some(parent_before), Some(parent_after)), (None, None)],
            Self::LeaveRoot { .. } => [(None, None), (None, None)],
        }
    }
    /// Exact number of affected node targets, separate from logical items.
    pub const fn affected(self) -> usize {
        match self {
            Self::Descend { .. } => 2,
            Self::LeaveRoot { .. } => 0,
            _ => 1,
        }
    }
    /// Full encoded record/key arguments; separate from native Rust enum ownership.
    pub const fn encoded_record_bytes(self) -> usize {
        match self {
            Self::EnterRoot { .. } | Self::Finish { .. } => 120,
            Self::Descend { .. } => 283,
            Self::Advance { .. } => 163,
            Self::Return { .. } => 145,
            Self::LeaveRoot { .. } => 60,
        }
    }
    /// Checks records/context and elementary proposal custody before provider calls.
    pub fn validate(self, scope: &GraphScope) -> ContentResult<()> {
        for (before, after) in self.changes() {
            if let (Some(before), Some(after)) = (before, after) {
                GraphNode::decode(scope, &before.encode())?;
                GraphNode::decode(scope, &after.encode())?;
                if before.key() != after.key() || before.projection() != after.projection() {
                    return Err(ContentError::InvalidOrderingRecord(
                        "graph mutation immutable",
                    ));
                }
            }
        }
        match self {
            Self::Descend {
                parent_before,
                parent_after,
                child_before,
                child_after,
                edge,
            } => {
                GraphEdge::decode(scope, &edge.encode())?;
                if parent_before.key() == child_before.key()
                    || edge.key().parent() != parent_before.key().serial()
                    || edge.key().child() != child_before.key().serial()
                    || child_after
                        != child_before.enter(
                            scope,
                            child_after.discovery(),
                            parent_before.key().serial(),
                        )?
                    || parent_after
                        != parent_before.advance(
                            scope,
                            child_before.key().serial(),
                            parent_before.lowlink(),
                        )?
                {
                    return Err(ContentError::InvalidOrderingRecord("graph descend targets"));
                }
            }
            Self::Advance {
                before,
                after,
                edge,
            } => {
                GraphEdge::decode(scope, &edge.encode())?;
                if edge.key().parent() != before.key().serial()
                    || after != before.advance(scope, edge.key().child(), after.lowlink())?
                {
                    return Err(ContentError::InvalidOrderingRecord(
                        "graph advance proposal",
                    ));
                }
            }
            Self::EnterRoot { before, after } => {
                if after != before.enter(scope, after.discovery(), 0)? {
                    return Err(ContentError::InvalidOrderingRecord("graph enter proposal"));
                }
            }
            Self::Finish { before, after } => {
                if after != before.finish(scope)? {
                    return Err(ContentError::InvalidOrderingRecord("graph finish proposal"));
                }
            }
            Self::Return { child, .. } => {
                GraphNodeKey::decode(scope, child.as_bytes())?;
            }
            Self::LeaveRoot { finished_root } => {
                GraphNode::decode(scope, &finished_root.encode())?;
                if !finished_root.completed()
                    || !finished_root.dfs_finished()
                    || finished_root.parent() != 0
                {
                    return Err(ContentError::InvalidOrderingRecord(
                        "graph root departure proposal",
                    ));
                }
            }
        }
        Ok(())
    }
}
/// Simultaneous logical items, full expected/proposed targets and encoded bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphMutationLimit {
    records: usize,
    bytes: usize,
}
impl Default for GraphMutationLimit {
    fn default() -> Self {
        Self {
            records: 128,
            bytes: 65536,
        }
    }
}
impl GraphMutationLimit {
    /// Checks the fixed actual operation-window profile.
    pub fn new(records: usize, bytes: usize) -> ContentResult<Self> {
        if records == 0 || records > 128 || !(350..=65536).contains(&bytes) {
            return Err(ContentError::InvalidOrderingRecord("graph mutation limit"));
        }
        Ok(Self { records, bytes })
    }
    /// Affected target and logical item ceiling.
    pub const fn records(self) -> usize {
        self.records
    }
    /// Header-inclusive encoded ceiling.
    pub const fn bytes(self) -> usize {
        self.bytes
    }
    /// Validates every target, duplicate identity and combined framing before effects.
    pub fn check(self, scope: &GraphScope, mutations: &[GraphMutation]) -> ContentResult<()> {
        let affected = mutations.iter().map(|m| m.affected()).sum::<usize>();
        if mutations.len() > self.records
            || affected > self.records
            || mutations
                .iter()
                .try_fold(317usize, |bytes, mutation| {
                    bytes.checked_add(mutation.encoded_record_bytes())
                })
                .ok_or(ContentError::LengthOverflow)?
                > self.bytes
        {
            return Err(ContentError::InvalidOrderingRecord("graph mutation window"));
        }
        for (index, mutation) in mutations.iter().enumerate() {
            mutation.validate(scope)?;
            for (_, after) in mutation.changes() {
                if let Some(after) = after {
                    if mutations[..index].iter().any(|previous| {
                        previous.changes().iter().any(|(_, candidate)| {
                            candidate.is_some_and(|candidate| candidate.key() == after.key())
                        })
                    }) {
                        return Err(ContentError::InvalidOrderingRecord(
                            "graph mutation duplicate target",
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}
