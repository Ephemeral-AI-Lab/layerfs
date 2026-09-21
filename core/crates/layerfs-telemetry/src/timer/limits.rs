//! Validated recording ownership budgets.

use super::{MAX_DEPTH, MAX_NODES};

/// Conservative charge per admitted node, including arena/tree conversion,
/// label ownership, geometric child/slot capacity and conversion stack headroom.
pub const NODE_CHARGE: usize = 1024;
/// Largest one-recording allocation charge.
pub const MAX_RECORDING_BYTES: usize = MAX_NODES * NODE_CHARGE;

/// A validated per-recording profile. Caller-owned imported reports are separate
/// owners until attached; native recorder pools charge that overlap separately.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecordingLimits {
    nodes: usize,
    depth: u8,
}

impl RecordingLimits {
    /// Validates finite node/depth/byte limits before creating a recording.
    /// A byte budget limits admitted nodes conservatively; it is not total RSS.
    pub const fn new(nodes: usize, depth: u8, bytes: usize) -> Option<Self> {
        if nodes == 0
            || nodes > MAX_NODES
            || depth == 0
            || depth > MAX_DEPTH
            || bytes < NODE_CHARGE
            || bytes > MAX_RECORDING_BYTES
        {
            return None;
        }
        let capacity = bytes / NODE_CHARGE;
        Some(Self {
            nodes: if nodes < capacity { nodes } else { capacity },
            depth,
        })
    }
    /// Nodes admitted under both limits.
    pub const fn nodes(self) -> usize {
        self.nodes
    }
    /// Maximum nested levels.
    pub const fn depth(self) -> u8 {
        self.depth
    }
    /// Conservative ownership charge reserved before recording begins.
    pub const fn charge(self) -> usize {
        self.nodes * NODE_CHARGE
    }
}

impl Default for RecordingLimits {
    fn default() -> Self {
        Self {
            nodes: MAX_NODES,
            depth: MAX_DEPTH,
        }
    }
}
