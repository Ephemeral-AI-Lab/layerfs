use crate::BranchStore;
use layerfs_storage_core::{
    BaseId, BranchId, BranchRecord, CommitId, CommitRecord, LayerHistoryId, LayerId, Result,
    StackHistoryId, StackId, StorageError,
};

impl BranchStore {
    pub fn create_branch_from_layer(
        &self,
        layer_history_id: LayerHistoryId,
        layer_id: LayerId,
    ) -> Result<BranchRecord> {
        self.create_from_base(BaseId::Layer(layer_id), Some(layer_history_id), None)
    }

    pub fn create_branch_from_stack(
        &self,
        stack_history_id: StackHistoryId,
        stack_id: StackId,
    ) -> Result<BranchRecord> {
        self.create_from_base(BaseId::Stack(stack_id), None, Some(stack_history_id))
    }

    pub fn create_branch_from_commit(
        &self,
        source_branch_id: BranchId,
        source_commit_id: CommitId,
    ) -> Result<BranchRecord> {
        let _operation = self.db.enter_operation()?;
        let source = self
            .db
            .branch(source_branch_id)?
            .ok_or(StorageError::NotFound("source Branch"))?;
        if !self
            .db
            .is_commit_ancestor(source_commit_id, source.head_commit_id)?
        {
            return Err(StorageError::NotFound("reachable Commit"));
        }
        let branch = BranchRecord {
            id: BranchId::new(),
            head_commit_id: source_commit_id,
            base_id: source.base_id,
        };
        self.db.create_subbranch(branch)?;
        Ok(branch)
    }

    fn create_from_base(
        &self,
        base_id: BaseId,
        layer_history: Option<LayerHistoryId>,
        stack_history: Option<StackHistoryId>,
    ) -> Result<BranchRecord> {
        let _operation = self.db.enter_operation()?;
        let base = self.parent.base_snapshot(base_id)?;
        if layer_history.is_some_and(|history| history != base.layer_history_id) {
            return Err(StorageError::WrongLayerHistory(
                layerfs_storage_core::WrongHistory {
                    expected: layer_history.unwrap(),
                    actual: base.layer_history_id,
                },
            ));
        }
        if let (BaseId::Stack(stack_id), Some(expected)) = (base_id, stack_history) {
            self.parent.visit_stacks(
                expected,
                stack_id,
                &mut |_, ids| {
                    layerfs_storage_core::internal::MissingBitmap::from_missing(ids.len(), |_| true)
                },
                &mut |_| Ok(()),
            )?;
        }
        layerfs_core::logical::namespace(&layerfs_storage_core::CoreReader(self), base.root_id)?;
        let anchor = CommitRecord {
            id: CommitId::derive(base.root_id, None, None),
            root_id: base.root_id,
            parent_id: None,
            merge_parent_id: None,
        };
        let branch = BranchRecord {
            id: BranchId::new(),
            head_commit_id: anchor.id,
            base_id,
        };
        self.db.create_branch(branch, anchor)?;
        Ok(branch)
    }
}
