use crate::BranchStore;
use layerfs_storage::{BranchId, BranchRecord, EntityName, LocalForkSource, Result, StorageError};

impl BranchStore {
    pub fn fork_branch(&self, name: EntityName, source: LocalForkSource) -> Result<BranchId> {
        let _operation = self.db.enter_operation()?;
        match source {
            LocalForkSource::Layer { layer_id } => self.fork_layer(name, layer_id),
            LocalForkSource::Branch {
                branch_id,
                commit_id,
            } => self.fork_branch_commit(name, branch_id, commit_id),
        }
    }

    fn fork_layer(&self, name: EntityName, layer_id: layerfs_storage::LayerId) -> Result<BranchId> {
        let layer = self
            .layer(layer_id)?
            .ok_or(StorageError::NotFound("pulled Layer"))?;
        let branch = BranchRecord {
            id: BranchId::new(),
            layer_stack_id: layer.layer_stack_id,
            name,
            base_layer_id: layer_id,
            head_commit_id: None,
            forked_from_layer_id: Some(layer_id),
            forked_from_branch_id: None,
            forked_from_commit_id: None,
        };
        self.db.publish_local_branch(&branch)?;
        Ok(branch.id)
    }

    fn fork_branch_commit(
        &self,
        name: EntityName,
        source_branch_id: BranchId,
        source_commit_id: layerfs_storage::CommitId,
    ) -> Result<BranchId> {
        let source = self
            .db
            .branch(source_branch_id)?
            .ok_or(StorageError::NotFound("source Branch"))?;
        if !self.full_history_contains(&source, source_commit_id)? {
            return Err(StorageError::NotFound("Commit in source Branch history"));
        }
        let commit = self
            .db
            .commit(source_commit_id)?
            .ok_or(StorageError::Integrity("source Commit"))?;
        let base = self
            .db
            .layer(commit.base_layer_id)?
            .ok_or(StorageError::Integrity("source Commit base Layer"))?;
        if base.layer_stack_id != source.layer_stack_id {
            return Err(StorageError::Integrity("Branch LayerStack ownership"));
        }
        let branch = BranchRecord {
            id: BranchId::new(),
            layer_stack_id: source.layer_stack_id,
            name,
            base_layer_id: commit.base_layer_id,
            head_commit_id: Some(source_commit_id),
            forked_from_layer_id: None,
            forked_from_branch_id: Some(source_branch_id),
            forked_from_commit_id: Some(source_commit_id),
        };
        self.db.publish_local_branch(&branch)?;
        Ok(branch.id)
    }
}
