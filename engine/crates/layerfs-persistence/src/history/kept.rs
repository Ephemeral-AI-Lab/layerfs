//! The §6.1 history closure: the retained roots and every history node they
//! reach, written to `kept_node` by a mark.
//!
//! The roots are every LayerStack with no `layer_stack_retired` row and every
//! Branch with no own or stack marker. A live stack keeps all its Layers; from
//! there the walk follows every history edge (Branch head/base/forked-from,
//! Layer parent/source Commit/source Branch, Commit parents/base Branch/merge
//! origin/displaced node) to its fixpoint. A retired Branch a retained node
//! reaches is kept through that node with its own edges, and a deleted project
//! is excluded because every history edge is stack-local.
use super::provider::HistoryProvider;
use super::params;
use super::rows::cell;
use super::transaction::Transaction;
use crate::backend::records::{Param, Record};
use layerfs_history::error::{HistoryError, HistoryResult};
use layerfs_history::identity::{BranchId, CommitId, LayerId, LayerStackId, NodeId};

const CLEAR: &str = "kept_node_clear";
const ROOTS: &str = "kept_node_roots";
const KEPT_ROOTS: &str = "kept_node_kept_roots";
const UNREACHABLE: &str = "history_unreachable_count";
/// The statements that remove one Stack's unreachable history rows, in the
/// order the deferred foreign keys need: children before their parents.
const UNREACHABLE_DELETES: [&str; 8] = [
    "history_remove_unreachable_commit",
    "history_remove_unreachable_layer",
    "history_remove_unreachable_branch",
    "history_remove_unreachable_stack",
    "history_remove_unreachable_lineage",
    "history_remove_unreachable_commit_lineage",
    "history_remove_unreachable_absorbed",
    "history_remove_unreachable_reach",
];
const MARK: &str = "kept_node_mark";
const READ: &str = "kept_node_read";

/// The identity of one node in a Store's retained history.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum KeptNodeId {
    /// A LayerStack (a project) no `layer_stack_retired` marker hides.
    Stack(LayerStackId),
    /// A Branch: a live one, or a retired one a retained node still reaches.
    Branch(BranchId),
    /// One immutable Commit.
    Commit(CommitId),
    /// One published Layer.
    Layer(LayerId),
}

impl KeptNodeId {
    /// The `kept_node.kind` text of this identity.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Stack(_) => "stack",
            Self::Branch(_) => "branch",
            Self::Commit(_) => "commit",
            Self::Layer(_) => "layer",
        }
    }

    /// The tagged identity bytes.
    pub fn as_slice(&self) -> &[u8] {
        match self {
            Self::Stack(id) => id.as_slice(),
            Self::Branch(id) => id.as_slice(),
            Self::Commit(id) => id.as_slice(),
            Self::Layer(id) => id.as_slice(),
        }
    }

    /// Checks `kind` and its tagged identity bytes.
    pub fn from_slice(kind: &str, bytes: &[u8]) -> HistoryResult<Self> {
        match kind {
            "stack" => Ok(Self::Stack(LayerStackId::from_slice(bytes)?)),
            "branch" => Ok(Self::Branch(BranchId::from_slice(bytes)?)),
            "commit" => Ok(Self::Commit(CommitId::from_slice(bytes)?)),
            "layer" => Ok(Self::Layer(LayerId::from_slice(bytes)?)),
            _ => Err(HistoryError::Integrity("kept node kind")),
        }
    }
}

/// One node a mark retains, with the history serial the mark saw for it.
///
/// `serial` is the node's own `history_serial`. A Branch and a LayerStack have
/// no serial column, so their nodes carry 0.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeptNode {
    stack: LayerStackId,
    id: KeptNodeId,
    serial: i64,
}

impl KeptNode {
    /// The LayerStack the node belongs to.
    pub fn stack(&self) -> LayerStackId {
        self.stack
    }

    /// The node identity.
    pub fn id(&self) -> KeptNodeId {
        self.id
    }

    /// The node's history serial (0 for a Branch or a LayerStack).
    pub fn serial(&self) -> i64 {
        self.serial
    }
}

/// The retained history closure a mark wrote to `kept_node` (§6.1), in
/// `(layer_stack_id, kind, node_id)` order.
///
/// The object closure (§6.2) starts from [`Self::commits`] and [`Self::layers`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct KeptHistory {
    nodes: Vec<KeptNode>,
}

impl KeptHistory {
    /// Every retained node, in `kept_node` order.
    pub fn nodes(&self) -> &[KeptNode] {
        &self.nodes
    }

    /// How many nodes the closure holds.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the closure holds no node.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Whether `id` is retained.
    pub fn contains(&self, id: KeptNodeId) -> bool {
        self.nodes.iter().any(|node| node.id == id)
    }

    /// The node for `id`, when retained.
    pub fn node(&self, id: KeptNodeId) -> Option<KeptNode> {
        self.nodes.iter().copied().find(|node| node.id == id)
    }

    /// The retained Commits, in order.
    pub fn commits(&self) -> impl Iterator<Item = CommitId> + '_ {
        self.nodes.iter().filter_map(|node| match node.id {
            KeptNodeId::Commit(id) => Some(id),
            _ => None,
        })
    }

    /// The retained Layers, in order.
    pub fn layers(&self) -> impl Iterator<Item = LayerId> + '_ {
        self.nodes.iter().filter_map(|node| match node.id {
            KeptNodeId::Layer(id) => Some(id),
            _ => None,
        })
    }

    /// The retained Branches, in order.
    pub fn branches(&self) -> impl Iterator<Item = BranchId> + '_ {
        self.nodes.iter().filter_map(|node| match node.id {
            KeptNodeId::Branch(id) => Some(id),
            _ => None,
        })
    }

    /// The retained LayerStacks, in order.
    pub fn stacks(&self) -> impl Iterator<Item = LayerStackId> + '_ {
        self.nodes.iter().filter_map(|node| match node.id {
            KeptNodeId::Stack(id) => Some(id),
            _ => None,
        })
    }
}

impl HistoryProvider {
    /// Clears the transient §6.1 `kept_node` rows a mark wrote.
    ///
    /// A `store.compact` that takes the §3 `none` path calls this before it
    /// replies, so the live Store carries no mark state afterwards: `kept_node`
    /// is a working table, empty before and after every compaction.
    pub fn clear_kept_node(&self) -> HistoryResult<()> {
        self.write(clear)
    }

    /// Recomputes the §6.1 history closure into `kept_node` and returns it.
    ///
    /// It runs in one write transaction: `kept_node` is cleared and refilled,
    /// so a second mark over unchanged history returns the same set and writes
    /// the same rows. Nothing else is written; the object closure (§6.2) starts
    /// from the returned [`KeptHistory::commits`] and [`KeptHistory::layers`].
    pub fn mark_history_closure(&self) -> HistoryResult<KeptHistory> {
        self.write(mark)
    }
}

pub(super) fn mark(tx: &Transaction<'_>) -> HistoryResult<KeptHistory> {
    tx.execute(CLEAR, Vec::<Param>::new())?;
    tx.execute(MARK, Vec::<Param>::new())?;
    let rows = tx.query(READ, Vec::<Param>::new())?;
    let mut nodes = Vec::with_capacity(rows.len());
    for row in &rows {
        nodes.push(kept_node(row)?);
    }
    Ok(KeptHistory { nodes })
}

/// The retained history roots of one Stack: every Layer of a live Stack,
/// which is the root set the mark closure itself starts from. A retired or
/// deleted Stack has no roots. Node identities are Commits and Layers, so the
/// Stack and Branch roots the closure also names are represented by the
/// Layers that reach them.
pub(super) fn retained_roots(
    tx: &Transaction<'_>,
    stack: LayerStackId,
) -> HistoryResult<Vec<NodeId>> {
    let rows = tx.query(ROOTS, params![stack.as_slice()])?;
    roots(&rows)
}

/// The roots a mark recorded in kept_node for one Stack, for a caller that
/// resolves them separately. A Stack no mark has reached yet falls back to
/// the live roots.
pub(super) fn kept_roots(tx: &Transaction<'_>, stack: LayerStackId) -> HistoryResult<Vec<NodeId>> {
    let rows = tx.query(KEPT_ROOTS, params![stack.as_slice()])?;
    if rows.is_empty() {
        return retained_roots(tx, stack);
    }
    roots(&rows)
}

fn roots(rows: &[Record]) -> HistoryResult<Vec<NodeId>> {
    rows.iter()
        .map(|row| {
            let id: Vec<u8> = cell(row, 0)?;
            NodeId::from_slice(&id)
        })
        .collect()
}

/// How many history rows of one Stack the last mark did not retain. It is
/// zero before any mark, when the closure that decides reachability is not
/// recorded yet.
pub(super) fn unreachable_count(
    tx: &Transaction<'_>,
    stack: LayerStackId,
) -> HistoryResult<u64> {
    let rows = tx.query(UNREACHABLE, params![stack.as_slice()])?;
    let count: i64 = cell(
        rows.first()
            .ok_or(HistoryError::Integrity("unreachable count"))?,
        0,
    )?;
    u64::try_from(count).map_err(|_| HistoryError::Integrity("unreachable count"))
}

/// Removes one Stack's unreachable history rows (the rows the last mark did
/// not retain), returning how many were removed. Reachability is the
/// kept_node closure a mark wrote; the same set a rebuild copies and a
/// compaction collects. It runs in one write transaction.
pub(super) fn remove_unreachable(
    tx: &Transaction<'_>,
    stack: LayerStackId,
) -> HistoryResult<u64> {
    let count = unreachable_count(tx, stack)?;
    if count == 0 {
        return Ok(0);
    }
    for key in &UNREACHABLE_DELETES[..5] {
        tx.execute(key, params![stack.as_slice()])?;
    }
    for key in &UNREACHABLE_DELETES[5..] {
        tx.execute(key, params![])?;
    }
    Ok(count)
}

/// Empties the §6.1 working table without recomputing the closure.
pub(super) fn clear(tx: &Transaction<'_>) -> HistoryResult<()> {
    tx.execute(CLEAR, Vec::<Param>::new())?;
    Ok(())
}

fn kept_node(row: &Record) -> HistoryResult<KeptNode> {
    let stack: Vec<u8> = cell(row, 0)?;
    let kind: String = cell(row, 1)?;
    let id: Vec<u8> = cell(row, 2)?;
    Ok(KeptNode {
        stack: LayerStackId::from_slice(&stack)?,
        id: KeptNodeId::from_slice(&kind, &id)?,
        serial: cell(row, 3)?,
    })
}
