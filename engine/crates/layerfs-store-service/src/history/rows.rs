//! One history call's rows (#818 H1): each Commit and Layer row a call
//! walks is read once and carried to the steps after it, and each Branch
//! name is read once per call.
//!
//! A walk reads a node, then its parent, then the parent as the next node:
//! the last [`RECENT`] rows are kept, so a node read as a parent is not read
//! again as the next node, nor for its `ref` or `comparedTo`. Saved rows
//! never change, so a held row is the row a read would return. Branch names
//! are kept for the call (one entry per Branch id met, at most one per
//! walked node).
use super::{lookup as args, MAIN};
use crate::store::Store;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_content::ObjectId;
use layerfs_history::{BranchId, NodeId, NodeRecord};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Rows kept: a node, its parent, its second parent and base Layer, and a
/// few more for a Layer's source Commit.
const RECENT: usize = 8;

/// One revision step: the node's root, its parent along persisted first
/// parents (a Layer's source Commit before its parent Layer), and its
/// record as the history calls show it.
pub type Revision = (ObjectId, Option<NodeId>, SdkValue);

/// The rows and Branch names one call has read.
pub struct Rows<'a> {
    store: &'a Arc<Store>,
    recent: VecDeque<(NodeId, NodeRecord)>,
    names: HashMap<BranchId, Option<String>>,
}

impl<'a> Rows<'a> {
    pub fn new(store: &'a Arc<Store>) -> Self {
        Self {
            store,
            recent: VecDeque::with_capacity(RECENT),
            names: HashMap::new(),
        }
    }

    /// Keeps a row the call read elsewhere (the resolve's), with its
    /// recorded Branch name when that was read too (#818 R5).
    pub fn hold(&mut self, node: NodeId, record: NodeRecord, recorded: Option<Option<String>>) {
        if let (NodeRecord::Commit(commit), Some(name)) = (&record, recorded) {
            if let Some(branch) = commit.meta.branch {
                self.names.insert(branch, name);
            }
        }
        self.keep(node, record);
    }

    fn keep(&mut self, node: NodeId, record: NodeRecord) {
        if self.recent.len() == RECENT {
            self.recent.pop_front();
        }
        self.recent.push_back((node, record));
    }

    /// The node's row, refused as [`args::node_record`] refuses.
    pub fn record(&mut self, node: NodeId) -> Result<&NodeRecord> {
        let at = match self.recent.iter().position(|(held, _)| *held == node) {
            Some(at) => at,
            None => {
                let record = args::node_record(self.store, node)?;
                self.keep(node, record);
                self.recent.len() - 1
            }
        };
        Ok(&self.recent[at].1)
    }

    /// A Branch's name, without the visibility filter, once per call.
    pub fn name(&mut self, branch: BranchId) -> Result<Option<String>> {
        if let Some(name) = self.names.get(&branch) {
            return Ok(name.clone());
        }
        let name = self
            .store
            .published(|history| history.branch_any(branch))
            .map_err(args::published)?
            .map(|b| b.name.as_str().to_owned());
        self.names.insert(branch, name.clone());
        Ok(name)
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
