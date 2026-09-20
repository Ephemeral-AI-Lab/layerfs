//! Private clocks, bounded tree construction and attachment.
//!
//! A recording owns one flat slot arena and the monotonic instants that only it
//! reads. Scopes address nodes by index, so no internal borrow is held while a
//! user closure runs and independent recordings share no state.

use std::borrow::Cow;
use std::cell::RefCell;
use std::time::{Duration, Instant};

use crate::timer::limits::RecordingLimits;
use crate::timer::report::{NodeOutcome, TimingNode, TimingReport};

/// Maximum number of nodes in one report, including root and attached nodes.
pub const MAX_NODES: usize = 1_024;

/// Maximum number of levels in one report, including root and attached nodes.
pub const MAX_DEPTH: u8 = 32;

/// Maximum number of UTF-8 bytes retained for one label.
pub const MAX_LABEL_BYTES: usize = 128;

/// Index of a node inside one recording.
pub(crate) type NodeId = usize;

/// A label clipped to [`MAX_LABEL_BYTES`], remembering that clipping happened.
pub(crate) struct Label {
    text: Cow<'static, str>,
    clipped: bool,
}

impl Label {
    /// Stores `name`, truncating on a UTF-8 boundary when it exceeds the budget.
    pub(crate) fn bounded(name: impl Into<Cow<'static, str>>) -> Self {
        let text = name.into();
        let mut end = text.len().min(MAX_LABEL_BYTES);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let clipped = end != text.len();
        let text = match text {
            Cow::Borrowed(value) => Cow::Borrowed(&value[..end]),
            Cow::Owned(value) => Cow::Owned(value[..end].to_owned()),
        };
        Self { text, clipped }
    }

    /// An empty placeholder label, used when a label is moved out of a slot.
    pub(crate) fn empty() -> Self {
        Self {
            text: Cow::Borrowed(""),
            clipped: false,
        }
    }

    pub(crate) fn is_clipped(&self) -> bool {
        self.clipped
    }

    pub(crate) fn into_text(self) -> Cow<'static, str> {
        self.text
    }
}

/// A child node that has started and not yet finished.
pub(crate) struct Started {
    pub(crate) id: NodeId,
    started: Instant,
}

struct Slot {
    label: Label,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    depth: u8,
    elapsed: Duration,
    outcome: NodeOutcome,
    incomplete: bool,
    running: bool,
}

struct Inner {
    root: NodeId,
    limits: RecordingLimits,
    slots: Vec<Slot>,
}

/// Live state for one measured operation.
pub(crate) struct Recording {
    started: Instant,
    inner: RefCell<Inner>,
}

impl Recording {
    /// Starts the root node before the operation closure runs.
    pub(crate) fn start(label: Label, limits: RecordingLimits) -> Self {
        let started = Instant::now();
        let root = Slot {
            incomplete: label.is_clipped(),
            label,
            parent: None,
            children: Vec::new(),
            depth: 1,
            elapsed: Duration::ZERO,
            outcome: NodeOutcome::Ok,
            running: true,
        };
        Self {
            started,
            inner: RefCell::new(Inner {
                root: 0,
                limits,
                slots: vec![root],
            }),
        }
    }

    pub(crate) fn root(&self) -> NodeId {
        self.inner.borrow().root
    }

    /// Returns true while another child of `parent` still fits the budget.
    pub(crate) fn child_fits(&self, parent: NodeId) -> bool {
        self.inner.borrow().child_fits(parent)
    }

    /// Starts one child node, or reports that detail had to be clipped.
    ///
    /// A child that does not fit the node or depth budget is not created; the
    /// affected parent and its ancestors are marked incomplete and the caller
    /// continues with an unrecorded child.
    pub(crate) fn begin_child(&self, parent: NodeId, label: Label) -> Option<Started> {
        let id = {
            let mut inner = self.inner.borrow_mut();
            if !inner.child_fits(parent) {
                inner.mark_incomplete(parent);
                return None;
            }
            inner.push_child(parent, label)
        };
        Some(Started {
            id,
            started: Instant::now(),
        })
    }

    /// Finalizes a child with its measured elapsed time and returned outcome.
    pub(crate) fn finish(&self, started: Started, outcome: NodeOutcome) {
        let elapsed = started.started.elapsed();
        let mut inner = self.inner.borrow_mut();
        let slot = &mut inner.slots[started.id];
        slot.elapsed = elapsed;
        slot.outcome = outcome;
        slot.running = false;
    }

    /// Grafts an owned completed report below the running node `id`.
    ///
    /// `None` and a disabled report mark the node incomplete. A report that does
    /// not fit the remaining budget is clipped to what fits, which marks the
    /// affected nodes and their ancestors incomplete.
    pub(crate) fn attach(&self, id: NodeId, report: Option<TimingReport>) {
        let mut inner = self.inner.borrow_mut();
        if !inner.slots[id].running {
            return;
        }
        match report.and_then(TimingReport::into_root) {
            None => inner.mark_incomplete(id),
            Some(root) => {
                if Inner::graft(&mut inner, id, root) {
                    inner.mark_incomplete(id);
                }
            }
        }
    }

    /// Finalizes the root and returns the completed, owned report.
    pub(crate) fn complete(self, outcome: NodeOutcome) -> TimingReport {
        let elapsed = self.started.elapsed();
        let mut inner = self.inner.into_inner();
        let root = inner.root;
        let slot = &mut inner.slots[root];
        slot.elapsed = elapsed;
        slot.outcome = outcome;
        slot.running = false;
        TimingReport::from_root(Inner::collect(&mut inner, root))
    }
}

impl Inner {
    fn child_fits(&self, parent: NodeId) -> bool {
        let slot = &self.slots[parent];
        slot.running && slot.depth < self.limits.depth() && self.slots.len() < self.limits.nodes()
    }

    fn push_child(&mut self, parent: NodeId, label: Label) -> NodeId {
        let depth = self.slots[parent].depth + 1;
        let incomplete = label.is_clipped();
        let id = self.slots.len();
        self.slots.push(Slot {
            label,
            parent: Some(parent),
            children: Vec::new(),
            depth,
            elapsed: Duration::ZERO,
            outcome: NodeOutcome::Ok,
            incomplete,
            running: true,
        });
        self.slots[parent].children.push(id);
        id
    }

    fn mark_incomplete(&mut self, id: NodeId) {
        let mut current = Some(id);
        while let Some(node) = current {
            let slot = &mut self.slots[node];
            slot.incomplete = true;
            current = slot.parent;
        }
    }

    /// Moves one imported completed node below `parent` and returns true when
    /// any part of it was missing, invalid or clipped.
    fn graft(inner: &mut Inner, parent: NodeId, node: TimingNode) -> bool {
        let depth = inner.slots[parent].depth + 1;
        if depth > inner.limits.depth() || inner.slots.len() >= inner.limits.nodes() {
            return true;
        }
        let parts = node.into_parts();
        let label = Label::bounded(parts.name);
        let mut missing = parts.incomplete || label.is_clipped();
        let id = inner.slots.len();
        inner.slots.push(Slot {
            label,
            parent: Some(parent),
            children: Vec::new(),
            depth,
            elapsed: parts.elapsed,
            outcome: parts.outcome,
            incomplete: false,
            running: false,
        });
        inner.slots[parent].children.push(id);
        for child in parts.children {
            if Inner::graft(inner, id, child) {
                missing = true;
            }
        }
        if missing {
            inner.slots[id].incomplete = true;
        }
        missing
    }

    /// Moves the recorded slots into an owned, bounded report tree.
    fn collect(inner: &mut Inner, id: NodeId) -> TimingNode {
        let label = std::mem::replace(&mut inner.slots[id].label, Label::empty()).into_text();
        let children = std::mem::take(&mut inner.slots[id].children);
        let (outcome, incomplete, elapsed) = {
            let slot = &inner.slots[id];
            let incomplete = slot.incomplete || slot.running;
            // A slot that is still running when the tree is collected never
            // returned: its scope's closure panicked or was dropped. Reporting the
            // outcome it was created with would serialise a panic as success, so it
            // reports that its outcome is unknown.
            let outcome = if slot.running {
                NodeOutcome::Unknown
            } else {
                slot.outcome
            };
            let elapsed = if slot.running {
                Duration::ZERO
            } else {
                slot.elapsed
            };
            (outcome, incomplete, elapsed)
        };
        let mut nodes = Vec::with_capacity(children.len());
        let mut incomplete = incomplete;
        for child in children {
            let collected = Inner::collect(inner, child);
            incomplete |= collected.is_incomplete();
            nodes.push(collected);
        }
        TimingNode::from_bounded(label, elapsed, outcome, incomplete, nodes)
    }
}
