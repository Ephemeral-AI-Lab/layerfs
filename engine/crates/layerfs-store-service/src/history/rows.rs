//! One history call's rows (#818 H1): each Commit and Layer row a call
//! walks is read once and carried to the steps after it, and each Branch
//! name is read once per call.
//!
//! A walk reads a node, then its parent, then the parent as the next node:
//! the last [`RECENT`] rows are kept, so a node read as a parent is not read
//! again as the next node, nor for its `ref` or `comparedTo`. Saved rows
//! never change, so a held row is the row a read would return. Branch names
//! are kept for the call, at most [`NAMES`] of them: a name met once the
//! memo is full is read each time it is needed (a fresh read, not a retry),
//! so the memo never grows with the Store's history.
//!
//! A walk along first parents ([`Rows::walk`], [`Rows::walked`]) reads its
//! Commits in windows where the Store batches reads (PostgreSQL): 16, 32,
//! 64, then 128 Commits a statement, each with its `history_serial`, never
//! more than the walk's remaining budget (#818 H4). A window's row is the
//! row a read would return; a row that failed to read is that Commit's
//! refusal, met only when the walk reaches it. On SQLite no window is read
//! and the walk reads each row as before.
use super::{super::lookup as args, MAIN};
use crate::filesystem::tree::visibility::Visibility;
use crate::store::Store;
use layerfs_content::ObjectId;
use layerfs_history::{BranchId, ChainRow, CommitId, HistoryResult, NodeId, NodeRecord};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Rows kept: a node, its parent, its second parent and base Layer, and a
/// few more for a Layer's source Commit.
const RECENT: usize = 8;

/// Branch names kept per call.
const NAMES: usize = 1_024;

/// The window sizes, one after another, then the last for every window
/// after (#818 H4, D5).
const WINDOWS: [usize; 4] = [16, 32, 64, 128];

/// One revision step: the node's root, its parent along persisted first
/// parents (a Layer's source Commit before its parent Layer), and its
/// record as the history calls show it.
pub type Revision = (ObjectId, Option<NodeId>, SdkValue);

/// The rows and Branch names one call has read.
pub struct Rows<'a> {
    store: &'a Arc<Store>,
    /// Each held row with its `history_serial` when a window read it.
    recent: VecDeque<(NodeId, NodeRecord, Option<u64>)>,
    names: HashMap<BranchId, Option<String>>,
    /// The rest of the open window, in walk order, each by the Commit the
    /// walk reads there.
    ahead: VecDeque<(CommitId, HistoryResult<ChainRow>)>,
    /// Windows read so far, and the Commits the walk may still read; no
    /// window is read while `budget` is 0.
    windows: usize,
    budget: usize,
}

impl<'a> Rows<'a> {
    pub fn new(store: &'a Arc<Store>) -> Self {
        Self {
            store,
            recent: VecDeque::with_capacity(RECENT),
            names: HashMap::new(),
            ahead: VecDeque::new(),
            windows: 0,
            budget: 0,
        }
    }

    /// The Store the rows are read from.
    pub fn store(&self) -> &'a Arc<Store> {
        self.store
    }

    /// Starts a first-parent walk that reads at most `budget` more rows:
    /// [`Self::walked`] then reads Commits in windows, where the Store
    /// batches reads.
    pub fn walk(&mut self, budget: usize) {
        if self.store.batched_reads() {
            self.budget = budget;
        }
    }

    /// Keeps a row the call read elsewhere (the resolve's), with its
    /// recorded Branch name when that was read too (#818 R5).
    pub fn hold(&mut self, node: NodeId, record: NodeRecord, recorded: Option<Option<String>>) {
        if let (NodeRecord::Commit(commit), Some(name)) = (&record, recorded) {
            if let Some(branch) = commit.meta.branch {
                self.remember(branch, name);
            }
        }
        self.keep(node, record, None);
    }

    fn keep(&mut self, node: NodeId, record: NodeRecord, serial: Option<u64>) {
        if self.recent.len() == RECENT {
            self.recent.pop_front();
        }
        self.recent.push_back((node, record, serial));
    }

    fn held(&self, node: NodeId) -> Option<usize> {
        self.recent.iter().position(|(held, _, _)| *held == node)
    }

    /// The node's row, refused as [`args::node_record`] refuses.
    pub fn record(&mut self, node: NodeId) -> Result<&NodeRecord> {
        let at = match self.held(node) {
            Some(at) => at,
            None => {
                let record = args::node_record(self.store, node)?;
                self.keep(node, record, None);
                self.recent.len() - 1
            }
        };
        Ok(&self.recent[at].1)
    }

    /// The row of the node a walk reaches next, from the open window when
    /// it holds it, else from a new window ([`Self::walk`]), else read
    /// alone as [`Self::record`] reads it.
    pub fn walked(&mut self, node: NodeId) -> Result<&NodeRecord> {
        if let (None, NodeId::Commit(id)) = (self.held(node), node) {
            if self.ahead.front().is_none_or(|(next, _)| *next != id) {
                self.ahead.clear();
                self.open(id)?;
            }
            if let Some((_, row)) = self.ahead.pop_front() {
                let row = row.map_err(args::history)?;
                self.keep(
                    node,
                    NodeRecord::Commit(Box::new(row.record)),
                    Some(row.serial),
                );
            }
        }
        self.record(node)
    }

    /// The Commits the open window holds after the walk's current node, in
    /// walk order, up to `count`, with their roots: those a walk reads next
    /// if it goes on (#818 H5). It stops at a row that failed to read.
    pub fn upcoming(&self, count: usize) -> Vec<(NodeId, ObjectId)> {
        self.ahead
            .iter()
            .map_while(|(id, row)| {
                row.as_ref()
                    .ok()
                    .map(|row| (NodeId::Commit(*id), row.record.root))
            })
            .take(count)
            .collect()
    }

    /// Reads the window from `head`, sized by the windows read before and
    /// clipped to the walk's budget; none when the budget is spent.
    fn open(&mut self, head: CommitId) -> Result<()> {
        let size = WINDOWS[self.windows.min(WINDOWS.len() - 1)].min(self.budget);
        if size == 0 {
            return Ok(());
        }
        let rows = self
            .store
            .published(|history| history.first_parent_window(head, size))
            .map_err(args::published)?;
        self.windows += 1;
        self.budget -= rows.len().min(self.budget);
        let mut next = Some(head);
        for row in rows {
            let Some(id) = next else { break };
            next = row.as_ref().ok().and_then(|row| row.record.parent);
            self.ahead.push_back((id, row));
        }
        Ok(())
    }

    /// The node's `history_serial` as [`Visibility::node_serial`] reads it,
    /// from the window that read the row when one did.
    pub fn serial(&self, node: NodeId) -> Result<u64> {
        match self.held(node).and_then(|at| self.recent[at].2) {
            Some(serial) => Ok(serial),
            None => Visibility::node_serial(self.store, node),
        }
    }

    /// A Branch's name, without the visibility filter, once per call while
    /// the memo has room.
    pub fn name(&mut self, branch: BranchId) -> Result<Option<String>> {
        if let Some(name) = self.names.get(&branch) {
            return Ok(name.clone());
        }
        let name = self
            .store
            .published(|history| history.branch_any(branch))
            .map_err(args::published)?
            .map(|b| b.name.as_str().to_owned());
        self.remember(branch, name.clone());
        Ok(name)
    }

    /// Keeps a Branch's name while the memo has room.
    fn remember(&mut self, branch: BranchId, name: Option<String>) {
        if self.names.len() < NAMES {
            self.names.insert(branch, name);
        }
    }

    /// The node's root, parent and record value (`operator::files::revision`).
    pub fn revision(&mut self, node: NodeId) -> Result<Revision> {
        Ok(match self.record(node)?.clone() {
            NodeRecord::Commit(r) => {
                let branch = r.meta.branch.map(|id| self.name(id)).transpose()?.flatten();
                (
                    r.root,
                    Some(r.parent.map_or(NodeId::Layer(r.base_layer), NodeId::Commit)),
                    args::commit_value(&r, branch.as_deref()),
                )
            }
            NodeRecord::Layer(r) => (
                r.root,
                r.source_commit
                    .map(NodeId::Commit)
                    .or(r.parent.map(NodeId::Layer)),
                args::layer_value(&r, None),
            ),
        })
    }

    /// A full immutable reference with its recorded originating Branch
    /// (`saved::reference`).
    pub fn reference(&mut self, node: NodeId) -> Result<SdkValue> {
        let (root, branch) = match self.record(node)?.clone() {
            NodeRecord::Layer(record) => (record.root, Some(MAIN.to_owned())),
            NodeRecord::Commit(record) => {
                let branch = record.meta.branch.map(|id| self.name(id)).transpose()?;
                (record.root, branch.flatten())
            }
        };
        Ok(super::saved::reference_value(node, root, branch))
    }

    fn optional(&mut self, node: Option<NodeId>) -> Result<SdkValue> {
        node.map(|node| self.reference(node))
            .transpose()
            .map(|v| v.unwrap_or_default())
    }

    /// Saved details (`saved::record`): publication parent/source and their
    /// times remain distinct.
    pub fn details(&mut self, node: NodeId) -> Result<SdkValue> {
        let mut fields = match self.record(node)?.clone() {
            NodeRecord::Commit(r) => {
                let SdkValue::Map(mut fields) = args::commit_value(&r, None) else {
                    unreachable!()
                };
                fields.extend([
                    (
                        "parentRef".into(),
                        self.optional(r.parent.map(NodeId::Commit))?,
                    ),
                    ("secondParentRef".into(), self.optional(r.second_parent)?),
                    (
                        "baseLayer".into(),
                        self.reference(NodeId::Layer(r.base_layer))?,
                    ),
                    ("sourceCommitRef".into(), SdkValue::Null),
                    ("sourceTime".into(), SdkValue::Null),
                    ("timeStatus".into(), SdkValue::text("recorded")),
                ]);
                fields
            }
            NodeRecord::Layer(r) => {
                let source = r
                    .source_commit
                    .map(|id| match self.record(NodeId::Commit(id))? {
                        NodeRecord::Commit(commit) => Ok(commit.meta.time_ms),
                        NodeRecord::Layer(_) => Err(args::failed("Commit row kind")),
                    })
                    .transpose()?;
                let source_branch = r.source_branch.map(|id| self.name(id)).transpose()?;
                let value = args::layer_value(&r, source_branch.flatten().as_deref());
                let SdkValue::Map(mut fields) = value else {
                    unreachable!()
                };
                fields.extend([
                    (
                        "parentRef".into(),
                        self.optional(r.parent.map(NodeId::Layer))?,
                    ),
                    ("secondParentRef".into(), SdkValue::Null),
                    ("baseLayer".into(), SdkValue::Null),
                    (
                        "sourceCommitRef".into(),
                        self.optional(r.source_commit.map(NodeId::Commit))?,
                    ),
                    (
                        "sourceTime".into(),
                        source.map_or(SdkValue::Null, SdkValue::Int),
                    ),
                    ("time".into(), SdkValue::Null),
                    ("timeStatus".into(), SdkValue::text("unavailable")),
                    (
                        "via".into(),
                        SdkValue::text(if r.source_commit.is_some() {
                            "Publish"
                        } else {
                            "Init"
                        }),
                    ),
                    ("message".into(), SdkValue::Null),
                ]);
                fields
            }
        };
        fields.push(("ref".into(), self.reference(node)?));
        Ok(SdkValue::Map(fields))
    }
}
