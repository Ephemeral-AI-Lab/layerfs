use crate::LayerStackStore;
use layerfs_content::ObjectId;
use layerfs_storage::{
    BranchFact, BranchId, BranchRecord, BranchRecordPage, CommitHistoryPage, CommitId,
    CommitRecord, Fact, FactKind, InventoryPage, LayerId, LayerPrefixPage, LayerRecord,
    LayerStackFact, LayerStackId, LayerStackRecord, LayerStackRecordPage, Result,
    StoreStorageSnapshot,
};

impl LayerStackStore {
    pub fn layer_stack(&self, id: LayerStackId) -> Result<Option<LayerStackRecord>> {
        self.db.layer_stack(id)
    }

    pub fn layer_stack_fact(&self, id: LayerStackId) -> Result<Option<LayerStackFact>> {
        self.db.layer_stack_fact(id)
    }

    pub fn layer(&self, id: LayerId) -> Result<Option<LayerRecord>> {
        self.db.layer(id)
    }

    pub fn branch(&self, id: BranchId) -> Result<Option<BranchRecord>> {
        self.db.branch(id)
    }

    pub fn branch_fact(&self, id: BranchId) -> Result<Option<BranchFact>> {
        self.db.branch_fact(id)
    }

    pub fn layer_stack_record_page(
        &self,
        after: Option<LayerStackId>,
        limit: u16,
    ) -> Result<LayerStackRecordPage> {
        self.db.layer_stack_record_page(after, limit)
    }

    pub fn branch_record_page(
        &self,
        layer_stack_id: Option<LayerStackId>,
        after: Option<BranchId>,
        limit: u16,
    ) -> Result<BranchRecordPage> {
        self.db.branch_record_page(layer_stack_id, after, limit)
    }

    pub fn commit(&self, id: CommitId) -> Result<Option<CommitRecord>> {
        self.db.commit(id)
    }

    pub fn layer_prefix_page(
        &self,
        through_layer_id: LayerId,
        cursor: Option<LayerId>,
        limit: u16,
    ) -> Result<LayerPrefixPage> {
        self.db.layer_prefix_page(through_layer_id, cursor, limit)
    }

    pub fn layer_ancestry_page(
        &self,
        through_layer_id: LayerId,
        stop_exclusive: Option<LayerId>,
        cursor: Option<LayerId>,
        limit: u16,
    ) -> Result<LayerPrefixPage> {
        self.db
            .layer_ancestry_page(through_layer_id, stop_exclusive, cursor, limit)
    }

    pub fn commit_history_page(
        &self,
        branch_id: BranchId,
        through_commit_id: CommitId,
        cursor: Option<CommitId>,
        limit: u16,
    ) -> Result<CommitHistoryPage> {
        self.db
            .commit_history_page(branch_id, through_commit_id, cursor, limit)
    }

    pub fn commit_ancestry_page(
        &self,
        through_commit_id: CommitId,
        stop_exclusive: Option<CommitId>,
        cursor: Option<CommitId>,
        limit: u16,
    ) -> Result<CommitHistoryPage> {
        self.db
            .commit_ancestry_page(through_commit_id, stop_exclusive, cursor, limit)
    }

    pub fn owned_commit_page(
        &self,
        branch_id: BranchId,
        through_commit_id: CommitId,
        cursor: Option<CommitId>,
        limit: u16,
    ) -> Result<CommitHistoryPage> {
        self.db
            .owned_commit_page(branch_id, through_commit_id, cursor, limit)
    }

    pub fn fact_page(
        &self,
        kind: FactKind,
        after: Option<&[u8]>,
        limit: u16,
    ) -> Result<(Vec<Fact>, Option<Vec<u8>>)> {
        self.db.fact_page(kind, after, limit)
    }

    pub fn inventory_page(&self, after: Option<ObjectId>, limit: u16) -> Result<InventoryPage> {
        self.db.inventory_page(after, limit)
    }

    pub fn storage_snapshot(&self) -> Result<StoreStorageSnapshot> {
        self.db.storage_snapshot()
    }

    pub fn visit_layer_diff(
        &self,
        from_layer_id: LayerId,
        to_layer_id: LayerId,
        mut visitor: impl FnMut(layerfs_content::filesystem::DiffEntry) -> Result<()>,
    ) -> Result<()> {
        let from = self
            .db
            .layer(from_layer_id)?
            .ok_or(layerfs_storage::StorageError::NotFound("Layer"))?;
        let to = self
            .db
            .layer(to_layer_id)?
            .ok_or(layerfs_storage::StorageError::NotFound("Layer"))?;
        if from.layer_stack_id != to.layer_stack_id {
            return Err(layerfs_storage::StorageError::InvalidInput(
                "LayerStack mismatch",
            ));
        }
        layerfs_content::filesystem::diff_roots(
            &layerfs_storage::CoreReader(self),
            from.root_id,
            to.root_id,
            |entry| visitor(entry).map_err(|_| layerfs_content::CoreError::Io),
        )?;
        Ok(())
    }
}
