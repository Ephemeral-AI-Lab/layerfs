//! Completed timing data: owned nodes, outcomes and completeness.

use std::borrow::Cow;
use std::time::Duration;

use crate::timer::recording::{MAX_DEPTH, MAX_NODES};

/// Outcome of the operation a node measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeOutcome {
    /// The measured operation returned success.
    Ok,
    /// The measured operation returned an error.
    Error,
}

impl NodeOutcome {
    /// Returns true when the measured operation returned an error.
    pub const fn is_error(self) -> bool {
        matches!(self, Self::Error)
    }

    pub(crate) fn from_result<T, E>(result: &Result<T, E>) -> Self {
        match result {
            Ok(_) => Self::Ok,
            Err(_) => Self::Error,
        }
    }
}

/// One completed measurement and the completed measurements it caused.
///
/// Labels are owned, so a node built from decoded, generated or otherwise
/// temporary text stays valid after its source is gone. Values are the reported
/// elapsed time (inclusive, monotonic), the returned outcome and whether
/// requested detail is missing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimingNode {
    name: Cow<'static, str>,
    elapsed: Duration,
    outcome: NodeOutcome,
    incomplete: bool,
    children: Vec<TimingNode>,
    nodes: u32,
    height: u8,
}

/// Owned fields of a [`TimingNode`], consumed by report composition.
pub(crate) struct NodeParts {
    pub(crate) name: Cow<'static, str>,
    pub(crate) elapsed: Duration,
    pub(crate) outcome: NodeOutcome,
    pub(crate) incomplete: bool,
    pub(crate) children: Vec<TimingNode>,
}

impl TimingNode {
    /// Creates a successful leaf node with the given label and elapsed time.
    ///
    /// Labels are owned: `&'static str`, `String` and `Cow` all work.
    pub fn new(name: impl Into<Cow<'static, str>>, elapsed: Duration) -> Self {
        Self {
            name: name.into(),
            elapsed,
            outcome: NodeOutcome::Ok,
            incomplete: false,
            children: Vec::new(),
            nodes: 1,
            height: 1,
        }
    }

    /// Sets the outcome of the measured operation.
    pub fn with_outcome(mut self, outcome: NodeOutcome) -> Self {
        self.outcome = outcome;
        self
    }

    /// Sets whether requested detail is missing.
    pub fn with_incomplete(mut self, incomplete: bool) -> Self {
        self.incomplete = incomplete;
        self
    }

    /// Adds `children` in order, bounded by [`MAX_NODES`] and [`MAX_DEPTH`].
    ///
    /// A child that would push this subtree past either limit is omitted and
    /// this node is marked incomplete, matching how a bounded recording clips
    /// optional detail instead of failing the measured operation. Adding a child
    /// that is itself incomplete marks this node incomplete, so missing detail
    /// is always visible on the affected node and its ancestors.
    pub fn with_children(mut self, children: Vec<TimingNode>) -> Self {
        for child in children {
            self.push_child(child);
        }
        self
    }

    /// Adds one child in order; returns false when it does not fit and was omitted.
    ///
    /// The child is omitted and this node is marked incomplete when the combined
    /// subtree would exceed [`MAX_NODES`] nodes or [`MAX_DEPTH`] levels. An
    /// incomplete child marks this node incomplete as well.
    pub fn push_child(&mut self, child: TimingNode) -> bool {
        let nodes = self.nodes + child.nodes;
        let height = self.height.max(child.height + 1);
        if nodes > MAX_NODES as u32 || height > MAX_DEPTH {
            self.incomplete = true;
            return false;
        }
        if child.incomplete {
            self.incomplete = true;
        }
        self.nodes = nodes;
        self.height = height;
        self.children.push(child);
        true
    }

    /// Returns this node's label.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the reported elapsed time of the measured operation.
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// Returns the outcome of the measured operation.
    pub fn outcome(&self) -> NodeOutcome {
        self.outcome
    }

    /// Returns true when requested detail is missing for this node or an ancestor.
    pub fn is_incomplete(&self) -> bool {
        self.incomplete
    }

    /// Returns the child nodes in start order.
    pub fn children(&self) -> &[TimingNode] {
        &self.children
    }

    /// Returns the number of nodes in this subtree, including this node.
    pub fn node_count(&self) -> usize {
        self.nodes as usize
    }

    /// Returns the number of levels in this subtree, including this node.
    pub fn levels(&self) -> usize {
        usize::from(self.height)
    }

    pub(crate) fn from_bounded(
        name: Cow<'static, str>,
        elapsed: Duration,
        outcome: NodeOutcome,
        incomplete: bool,
        children: Vec<TimingNode>,
    ) -> Self {
        let mut nodes = 1_u32;
        let mut height = 1_u8;
        for child in &children {
            nodes += child.nodes;
            height = height.max(child.height + 1);
        }
        Self {
            name,
            elapsed,
            outcome,
            incomplete,
            children,
            nodes,
            height,
        }
    }

    pub(crate) fn into_parts(self) -> NodeParts {
        NodeParts {
            name: self.name,
            elapsed: self.elapsed,
            outcome: self.outcome,
            incomplete: self.incomplete,
            children: self.children,
        }
    }
}

/// A completed recording: either one measured root or no root at all.
///
/// Disabled recording produces a report with no root, which stays distinct from
/// a measured zero-duration operation. Reports built by this crate are bounded:
/// they hold at most [`MAX_NODES`] nodes across at most [`MAX_DEPTH`] levels.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TimingReport {
    root: Option<TimingNode>,
}

impl TimingReport {
    /// Creates a report with no root, as disabled recording produces.
    pub const fn disabled() -> Self {
        Self { root: None }
    }

    /// Creates a report from a completed root node.
    pub fn from_root(root: TimingNode) -> Self {
        Self { root: Some(root) }
    }

    /// Returns the root node, or `None` when recording was disabled.
    pub fn root(&self) -> Option<&TimingNode> {
        self.root.as_ref()
    }

    /// Consumes the report and returns its root node.
    pub fn into_root(self) -> Option<TimingNode> {
        self.root
    }

    /// Returns true when a measured root node is present.
    pub const fn has_root(&self) -> bool {
        self.root.is_some()
    }

    /// Returns the number of nodes in the report, or zero when disabled.
    pub fn node_count(&self) -> usize {
        match &self.root {
            Some(root) => root.node_count(),
            None => 0,
        }
    }

    /// Returns the number of levels in the report, or zero when disabled.
    pub fn levels(&self) -> usize {
        match &self.root {
            Some(root) => root.levels(),
            None => 0,
        }
    }

    /// Returns true when the report has no root or missing detail is flagged.
    pub fn is_incomplete(&self) -> bool {
        match &self.root {
            Some(root) => root.is_incomplete(),
            None => true,
        }
    }
}
