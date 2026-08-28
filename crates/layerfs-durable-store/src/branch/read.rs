use crate::{BranchHead, BranchId, BranchPushBundle, DurableStore, LayerStackHead, Result};

impl DurableStore {
    pub fn branch_head(&self, id: BranchId) -> Result<Option<BranchHead>> {
        Ok(self.storage.authoritative_branch_head(id)?)
    }

    pub fn export_branch_fetch(
        &self,
        branch_id: BranchId,
        base: Option<BranchHead>,
        origin_stack_base: Option<LayerStackHead>,
    ) -> Result<BranchPushBundle> {
        let _ = (branch_id, base, origin_stack_base);
        Err(crate::DurableError::Storage(
            layerfs_storage::EngineError::InvalidRecord("Full Branch Fetch requires P4"),
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn branch_fetch_object_page(
        &self,
        branch_id: BranchId,
        base: Option<BranchHead>,
        origin_stack_base: Option<LayerStackHead>,
        expected_head: BranchHead,
        expected_stack_head: LayerStackHead,
        after: Option<layerfs_core::ObjectId>,
        limit: usize,
    ) -> Result<Vec<layerfs_core::ObjectId>> {
        let _ = (
            branch_id,
            base,
            origin_stack_base,
            expected_head,
            expected_stack_head,
            after,
            limit,
        );
        Err(crate::DurableError::Storage(
            layerfs_storage::EngineError::InvalidRecord("Full Branch Fetch requires P4"),
        ))
    }
}
