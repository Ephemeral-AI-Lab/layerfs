//! Logical history facts reused by incremental canonical accounting.
use super::{BoundWorkspace, Store};
use layerfs_history::{HistoryError, HistoryResult, NodeId};
use std::sync::Mutex;
impl Store {
    pub fn logical_size(&self, node: NodeId) -> HistoryResult<u64> {
        match node {
            NodeId::Commit(id) => self
                .history()
                .commit(id)?
                .map(|record| record.meta.statistics.logical),
            NodeId::Layer(id) => self.history().layer(id)?.map(|record| record.logical),
        }
        .ok_or(HistoryError::Integrity("logical history node"))
    }
}
impl BoundWorkspace {
    /// The logical size of `node`, a Commit's `logical.known`. A node's
    /// recorded size never changes, so the one this Workspace, or else any
    /// Workspace of its Store (#818 W3), last read or published is answered
    /// from memory; any other node is read once ([`Store::logical_size`])
    /// and kept in its place. The bind snapshot carries no size, so the
    /// first Commit after a mount over a node no Workspace of this Store
    /// read or published reads it.
    pub(super) fn known_logical(&self, node: NodeId) -> HistoryResult<u64> {
        let known = |slot: &Mutex<Option<(NodeId, u64)>>| {
            let kept = slot.lock().ok()?;
            kept.filter(|(known, _)| *known == node)
        };
        if let Some((_, logical)) = known(&self.logical).or_else(|| known(&self.store.last_logical))
        {
            return Ok(logical);
        }
        let logical = self.store.logical_size(node)?;
        self.remember_logical(node, logical);
        Ok(logical)
    }
    /// Keeps `node`'s recorded size, in this Workspace and in its Store:
    /// one this Workspace read, or the statistics of a Commit it published.
    /// A poisoned slot keeps nothing, and the next Commit reads.
    pub(super) fn remember_logical(&self, node: NodeId, logical: u64) {
        for slot in [&self.logical, &self.store.last_logical] {
            if let Ok(mut kept) = slot.lock() {
                *kept = Some((node, logical));
            }
        }
    }
}
