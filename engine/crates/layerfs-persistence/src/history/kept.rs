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
use super::objects::spill::{Item, Sorter, Space};
use super::params;
use super::provider::HistoryProvider;
use super::rows::cell;
use super::transaction::Transaction;
use crate::backend::records::{Param, Record};
use layerfs_content::ObjectId;
use layerfs_history::error::{HistoryError, HistoryResult};
use layerfs_history::identity::{BranchId, CommitId, LayerId, LayerStackId, NodeId};
use std::sync::Arc;

const CLEAR: &str = "kept_node_clear";
const ROOTS: &str = "kept_node_roots";
const KEPT_ROOTS: &str = "kept_node_kept_roots";
const UNREACHABLE: &str = "history_unreachable_count";
/// The accounting statement the prune runs while the rows still exist:
/// `store_totals` is maintained by AFTER INSERT triggers alone, so a
/// delete does not retract what the deleted Layer and Commit rows held.
const TOTALS: &str = "history_remove_unreachable_totals";
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
const SEED: &str = "kept_node_seed";
const EXPAND: &str = "kept_node_expand";
const PAGE: &str = "kept_node_page";
/// Rows one read of `kept_node` returns.
const PAGE_ROWS: usize = 1024;
/// Nodes one closure statement starts from, and the most rows it may reach
/// before it stops: what one statement holds is bounded by the second.
const BATCH_NODES: usize = 256;
const BATCH_ROWS: usize = 8192;
/// Bytes of nodes still to follow that the closure holds before it writes
/// them to a sort file.
const FRONTIER_SORT_BYTES: usize = 1024 * 1024;

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
/// The closure itself stays in `kept_node`. A mark that lists what it kept
/// ([`HistoryProvider::mark`], [`HistoryProvider::mark_history_closure`])
/// also returns the nodes here; a compaction's mark returns their count
/// only, so what it holds does not grow with the history.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct KeptHistory {
    nodes: Vec<KeptNode>,
    count: u64,
}

impl KeptHistory {
    /// Every retained node, in `kept_node` order, when the mark listed them.
    pub fn nodes(&self) -> &[KeptNode] {
        &self.nodes
    }

    /// How many nodes the closure holds, listed or not.
    pub fn len(&self) -> usize {
        usize::try_from(self.count).unwrap_or(usize::MAX)
    }

    /// Whether the closure holds no node.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Whether `id` is among the listed nodes.
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
        let budget = self.mark_budget();
        self.write_snapshot(|tx| {
            let space = Space::sized(tx.mechanics().scratch_directory(), budget);
            close(tx, &space)?;
            let mut history = KeptHistory::default();
            each(tx, |node, _| {
                history.note(node, true);
                Ok(())
            })?;
            Ok(history)
        })
    }
}

impl KeptHistory {
    /// Counts one node of the closure, and keeps it when the mark lists.
    pub(super) fn note(&mut self, node: KeptNode, listed: bool) {
        self.count += 1;
        if listed {
            self.nodes.push(node);
        }
    }
}

/// One node the closure has still to follow: its LayerStack and its tagged
/// identity, which names its kind.
#[derive(Clone, Copy)]
struct Pending {
    stack: [u8; 17],
    id: [u8; 33],
}

impl Pending {
    fn new(stack: &[u8], id: &[u8]) -> HistoryResult<Self> {
        let tagged = matches!(
            (id.first(), id.len()),
            (Some(0x12 | 0x32), 33) | (Some(0x11 | 0x31), 17)
        );
        if stack.len() != 17 || !tagged {
            return Err(HistoryError::Integrity("kept node identity"));
        }
        let mut pending = Self {
            stack: [0; 17],
            id: [0; 33],
        };
        pending.stack.copy_from_slice(stack);
        pending.id[..id.len()].copy_from_slice(id);
        Ok(pending)
    }
}

impl Item for Pending {
    const BYTES: usize = 50;
    fn encode(&self, out: &mut [u8]) {
        out[..17].copy_from_slice(&self.stack);
        out[17..].copy_from_slice(&self.id);
    }
    fn decode(bytes: &[u8]) -> Option<Self> {
        let mut pending = Self {
            stack: [0; 17],
            id: [0; 33],
        };
        pending.stack.copy_from_slice(&bytes[..17]);
        pending.id.copy_from_slice(&bytes[17..50]);
        Some(pending)
    }
    fn absorb(&mut self, _other: &Self) -> bool {
        true
    }
}

fn pending_order(left: &Pending, right: &Pending) -> std::cmp::Ordering {
    (left.stack, left.id).cmp(&(right.stack, right.id))
}

/// Recomputes the §6.1 history closure into `kept_node`. The rows stay in
/// the table; [`each`] reads them.
///
/// An engine that computes temporary results in its own process
/// ([`crate::backend::Mechanics::closure_in_batches`]) is given the closure
/// a bounded piece at a time: the roots are written by one statement, then
/// each statement follows the edges of at most [`BATCH_NODES`] kept nodes
/// until it has reached [`BATCH_ROWS`] rows, writes the nodes it found that
/// were not kept and returns them. A statement that stopped at its bound
/// has not followed everything it found, so what it returned and what it
/// started from are followed again; the nodes waiting for that are sorted in
/// bounded memory ([`Sorter`]) and written to a sort file beyond it. Every
/// statement that stops at its bound keeps at least
/// `BATCH_ROWS - BATCH_NODES` new nodes, so the closure ends.
///
/// Elsewhere it is the one recursive statement, and what that holds is the
/// server's to bound.
pub(super) fn close(tx: &Transaction<'_>, space: &Arc<Space>) -> HistoryResult<()> {
    tx.execute(CLEAR, Vec::<Param>::new())?;
    if !tx.mechanics().closure_in_batches() {
        tx.execute(MARK, Vec::<Param>::new())?;
        return Ok(());
    }
    tx.execute(SEED, Vec::<Param>::new())?;
    let mut pending = Sorter::new(space, FRONTIER_SORT_BYTES, pending_order);
    each(tx, |node, _| {
        pending.push(Pending::new(node.stack.as_slice(), node.id.as_slice())?)
    })?;
    while !pending.is_empty() {
        let mut found = Sorter::new(space, FRONTIER_SORT_BYTES, pending_order);
        let mut nodes = pending.sorted()?;
        let mut batch: Vec<Pending> = Vec::with_capacity(BATCH_NODES);
        loop {
            batch.clear();
            while batch.len() < BATCH_NODES {
                match nodes.next()? {
                    Some(node) => batch.push(node),
                    None => break,
                }
            }
            if batch.is_empty() {
                break;
            }
            let mut from = vec![0u8; batch.len() * Pending::BYTES];
            for (node, out) in batch.iter().zip(from.chunks_exact_mut(Pending::BYTES)) {
                node.encode(out);
            }
            let rows = tx.query(EXPAND, params![from, batch.len() as i64, BATCH_ROWS as i64])?;
            if rows.len() + batch.len() < BATCH_ROWS {
                // The statement ran to its end: everything it found is
                // kept and followed.
                continue;
            }
            for row in &rows {
                let stack: Vec<u8> = cell(row, 0)?;
                let id: Vec<u8> = cell(row, 1)?;
                found.push(Pending::new(&stack, &id)?)?;
            }
            for node in &batch {
                found.push(*node)?;
            }
        }
        drop(nodes);
        pending = found;
    }
    Ok(())
}

/// Hands every node of `kept_node` to `each`, in table order, with the root
/// object of a Commit or a Layer. The rows are read a bounded page at a
/// time and never held together.
pub(super) fn each(
    tx: &Transaction<'_>,
    mut each: impl FnMut(KeptNode, Option<ObjectId>) -> HistoryResult<()>,
) -> HistoryResult<()> {
    let mut after: (Vec<u8>, String, Vec<u8>) = (Vec::new(), String::new(), Vec::new());
    loop {
        let rows = tx.query(
            PAGE,
            params![
                after.0.clone(),
                after.1.clone(),
                after.2.clone(),
                PAGE_ROWS as i64
            ],
        )?;
        for row in &rows {
            let node = kept_node(row)?;
            let root: Option<Vec<u8>> = cell(row, 4)?;
            let root = match (node.id, root) {
                (KeptNodeId::Commit(_) | KeptNodeId::Layer(_), Some(root)) => Some(
                    ObjectId::from_bytes(&root)
                        .map_err(|_| HistoryError::Integrity("kept node root"))?,
                ),
                (KeptNodeId::Commit(_) | KeptNodeId::Layer(_), None) => {
                    return Err(HistoryError::Integrity("kept node root"))
                }
                (_, _) => None,
            };
            each(node, root)?;
        }
        let Some(last) = rows.last() else {
            break;
        };
        after = (cell(last, 0)?, cell(last, 1)?, cell(last, 2)?);
        if rows.len() < PAGE_ROWS {
            break;
        }
    }
    Ok(())
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

/// How many history rows of one Stack a mark did not retain. The caller runs
/// the mark in the same write transaction, so an empty `kept_node` is a real
/// empty closure, never the absence of a mark.
pub(super) fn unreachable_count(tx: &Transaction<'_>, stack: LayerStackId) -> HistoryResult<u64> {
    let rows = tx.query(UNREACHABLE, params![stack.as_slice()])?;
    let count: i64 = cell(
        rows.first()
            .ok_or(HistoryError::Integrity("unreachable count"))?,
        0,
    )?;
    u64::try_from(count).map_err(|_| HistoryError::Integrity("unreachable count"))
}

/// Removes one Stack's unreachable history rows, returning how many of that
/// Stack's Commit, Layer, Branch and LayerStack rows it removed (the cleanup
/// also drops the orphaned lineage and reach rows those deletes leave behind;
/// they are not in the count).
///
/// The mark that decides reachability runs in this same write transaction,
/// immediately before the deletes, so the closure is never older than the
/// history it prunes: a Branch forked, a Commit written or a project
/// initialized after an earlier mark is retained, not collected. A dry run
/// runs the same fresh mark and reports the count without deleting, and an
/// empty closure over a Store with no live project still counts that Store's
/// rows as unreachable. The logical bytes those rows contributed are
/// retracted from `store_totals` in the same transaction, while the rows the
/// statement reads still exist.
pub(super) fn remove_unreachable(
    tx: &Transaction<'_>,
    stack: LayerStackId,
    dry_run: bool,
) -> HistoryResult<u64> {
    close(tx, &Space::new(tx.mechanics().scratch_directory()))?;
    let count = unreachable_count(tx, stack)?;
    if dry_run || count == 0 {
        return Ok(count);
    }
    // The rows still exist here, so the statement reads exactly what the
    // deletes below take away. It subtracts only this Stack's unreachable
    // rows: a live Stack keeps the rows the mark retained.
    tx.execute(TOTALS, params![stack.as_slice()])?;
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
