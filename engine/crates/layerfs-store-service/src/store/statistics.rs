//! Logical history facts reused by incremental canonical accounting.
use super::Store;
use layerfs_history::{HistoryError, HistoryResult, NodeId};
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
