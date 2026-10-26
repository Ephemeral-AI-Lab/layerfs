//! C5 in-memory Init authority; other operations are explicit refusals.
use layerfs_history::*;
use std::{collections::BTreeMap, sync::Mutex};
#[derive(Default)]
pub struct MemoryHistory {
    stacks: Mutex<BTreeMap<LayerStackId, LayerStackRecord>>,
    layers: Mutex<BTreeMap<LayerId, LayerRecord>>,
    highwater: Mutex<BTreeMap<layerfs_content::ObjectId, u64>>,
}
impl HistoryCatalog for MemoryHistory {
    fn catalog_id(&self) -> CatalogId {
        CatalogId::derive(b"project-memory").unwrap()
    }
    fn incarnation(&self) -> u64 {
        1
    }
    fn initialize_layerstack(&self, r: &StackInitialization) -> HistoryResult<LayerStackRecord> {
        let mut stacks = self.stacks.lock().unwrap();
        if stacks.contains_key(&r.stack) || stacks.values().any(|s| s.name == r.name) {
            return Err(HistoryError::Integrity("LayerStack identity"));
        }
        let layer = LayerRecord {
            overwrote_count: 0,
            logical: 0,
            id: LayerId::derive(r.stack, None, r.genesis_root),
            stack: r.stack,
            parent: None,
            root: r.genesis_root,
            source_branch: None,
            source_commit: None,
        };
        let stack = LayerStackRecord {
            id: r.stack,
            name: r.name.clone(),
            scope: r.scope,
            profile: r.profile,
            head_layer: layer.id,
        };
        self.layers.lock().unwrap().insert(layer.id, layer);
        stacks.insert(stack.id, stack.clone());
        Ok(stack)
    }
    fn reserve_inodes(&self, r: &ReserveRequest) -> HistoryResult<Reservation> {
        let mut map = self.highwater.lock().unwrap();
        let high = map.entry(r.scope).or_default();
        let start = *high + 1;
        *high = high
            .checked_add(r.count)
            .filter(|n| *n < i64::MAX as u64)
            .ok_or(HistoryError::Capacity("inode serials"))?;
        Ok(Reservation {
            scope: r.scope,
            start,
            count: r.count,
        })
    }
    fn layer_stack(&self, id: LayerStackId) -> HistoryResult<Option<LayerStackRecord>> {
        Ok(self.stacks.lock().unwrap().get(&id).cloned())
    }
    fn layer(&self, id: LayerId) -> HistoryResult<Option<LayerRecord>> {
        Ok(self.layers.lock().unwrap().get(&id).cloned())
    }
    fn layer_stacks(&self, _page: &Page) -> HistoryResult<PageResult<LayerStackRecord>> {
        Err(HistoryError::Unsupported("memory Init fixture operation"))
    }
    fn branch(&self, _id: BranchId) -> HistoryResult<Option<BranchRecord>> {
        Err(HistoryError::Unsupported("memory Init fixture operation"))
    }
    fn branch_named(
        &self,
        _stack: LayerStackId,
        _name: &HistoryName,
    ) -> HistoryResult<Option<BranchRecord>> {
        Err(HistoryError::Unsupported("memory Init fixture operation"))
    }
    fn branch_snapshot(&self, _id: BranchId) -> HistoryResult<Option<BranchSnapshot>> {
        Err(HistoryError::Unsupported("memory Init fixture operation"))
    }
    fn branches(
        &self,
        _stack: LayerStackId,
        _page: &Page,
    ) -> HistoryResult<PageResult<BranchRecord>> {
        Err(HistoryError::Unsupported("memory Init fixture operation"))
    }
    fn commit(&self, _id: CommitId) -> HistoryResult<Option<CommitRecord>> {
        Err(HistoryError::Unsupported("memory Init fixture operation"))
    }
    fn commit_history(
        &self,
        _request: &CommitHistoryRequest,
    ) -> HistoryResult<PageResult<CommitRecord>> {
        Err(HistoryError::Unsupported("memory Init fixture operation"))
    }
    fn layer_history(
        &self,
        _request: &LayerHistoryRequest,
    ) -> HistoryResult<PageResult<LayerRecord>> {
        Err(HistoryError::Unsupported("memory Init fixture operation"))
    }
    fn fork(&self, _request: &ForkRequest) -> HistoryResult<BranchSnapshot> {
        Err(HistoryError::Unsupported("memory Init fixture operation"))
    }
    fn merge_base(&self, _left: NodeId, _right: NodeId) -> HistoryResult<Option<NodeId>> {
        Err(HistoryError::Unsupported("memory Init fixture operation"))
    }
    fn is_ancestor(&self, _ancestor: NodeId, _descendant: NodeId) -> HistoryResult<bool> {
        Err(HistoryError::Unsupported("memory Init fixture operation"))
    }
    fn commit_branch(&self, _request: &CommitRequest) -> HistoryResult<CommitOutcome> {
        Err(HistoryError::Unsupported("memory Init fixture operation"))
    }
    fn merge_branch(&self, _request: &BranchMergeRequest) -> HistoryResult<BranchSnapshot> {
        Err(HistoryError::Unsupported("memory Init fixture operation"))
    }
    fn add_layer(&self, _request: &AddLayerRequest) -> HistoryResult<AddLayerOutcome> {
        Err(HistoryError::Unsupported("memory Init fixture operation"))
    }
}
