use crate::{
    BranchHead, BranchId, LayerId, LayerStackHead, LayerStackId, OperationRecordRef, Result,
    VersionRef, WorkingStore,
};

impl WorkingStore {
    pub fn create_layer_stack(
        &self,
        layer_stack_id: LayerStackId,
        layer_id: LayerId,
        name: &str,
        root: layerfs_core::ObjectId,
    ) -> Result<LayerStackHead> {
        Ok(self
            .storage
            .product_create_layer_stack(layer_stack_id, layer_id, name, root)?)
    }

    pub fn layer_stack_head(&self, layer_stack_id: LayerStackId) -> Result<Option<LayerStackHead>> {
        Ok(self.storage.product_layer_stack_head(layer_stack_id)?)
    }

    pub fn fetch_resume_layer_stack_head(
        &self,
        layer_stack_id: LayerStackId,
    ) -> Result<Option<LayerStackHead>> {
        Ok(self
            .storage
            .product_fetch_resume_layer_stack_head(layer_stack_id)?)
    }

    pub fn create_top_level_branch(
        &self,
        branch_id: BranchId,
        name: Option<&str>,
        origin: LayerStackHead,
    ) -> Result<BranchHead> {
        Ok(self
            .storage
            .product_create_top_level_branch(branch_id, name, origin)?)
    }

    pub fn create_child_branch(
        &self,
        branch_id: BranchId,
        name: Option<&str>,
        origin: OperationRecordRef,
    ) -> Result<BranchHead> {
        Ok(self
            .storage
            .product_create_child_branch(branch_id, name, origin)?)
    }

    pub fn branch_head(&self, branch_id: BranchId) -> Result<Option<BranchHead>> {
        Ok(self.storage.product_branch_head(branch_id)?)
    }

    pub fn branch_has_special_history_after(
        &self,
        branch_id: BranchId,
        generation: u64,
    ) -> Result<bool> {
        Ok(self
            .storage
            .product_branch_has_special_history_after(branch_id, generation)?)
    }

    pub fn fetch_resume_branch_head(&self, branch_id: BranchId) -> Result<Option<BranchHead>> {
        Ok(self.storage.product_fetch_resume_branch_head(branch_id)?)
    }

    pub fn branch_parent(&self, branch_id: BranchId) -> Result<Option<BranchId>> {
        Ok(self
            .storage
            .product_branch_ancestry(branch_id)?
            .and_then(|ancestry| ancestry.immediate_parent_branch_id))
    }

    pub fn contains_branch_head(&self, head: BranchHead) -> Result<bool> {
        Ok(self.storage.product_contains_branch_head(head)?)
    }

    pub fn branch_contains_root(
        &self,
        branch: BranchId,
        root: layerfs_core::ObjectId,
    ) -> Result<bool> {
        Ok(self.storage.product_branch_contains_root(branch, root)?)
    }

    pub fn pin_branch_version(&self, head: BranchHead) -> Result<VersionRef> {
        Ok(self.storage.product_pin_branch_version(head)?)
    }

    pub fn validate_version_ref(&self, version: VersionRef) -> Result<()> {
        Ok(self.storage.product_validate_version_ref(version)?)
    }

    pub fn drop_branch(&self, branch_id: BranchId) -> Result<()> {
        Ok(self.storage.product_drop_branch(branch_id)?)
    }
}
