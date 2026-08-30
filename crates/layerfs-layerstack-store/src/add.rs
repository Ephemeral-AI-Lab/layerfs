use crate::LayerStackStore;
use layerfs_storage::{AuthorityAddResult, BranchId, LayerId, LayerRecord, Result, StorageError};

impl LayerStackStore {
    pub fn add_layer(&self, branch_id: BranchId) -> Result<AuthorityAddResult> {
        let _operation = self.db.enter_operation()?;
        let branch = self
            .db
            .branch(branch_id)?
            .ok_or(StorageError::NotFound("pushed Branch"))?;
        let head_commit_id = branch
            .head_commit_id
            .ok_or(StorageError::InvalidInput("Branch without Commit"))?;
        if let Some(layer) = self.db.layer_by_source(branch_id, head_commit_id)? {
            return Ok(AuthorityAddResult::UpToDate { layer_id: layer.id });
        }
        let commit = self
            .db
            .commit(head_commit_id)?
            .ok_or(StorageError::Integrity("pushed Branch head"))?;
        if commit.base_layer_id != branch.base_layer_id {
            return Err(StorageError::Integrity("Branch base"));
        }
        let base = self
            .db
            .layer(branch.base_layer_id)?
            .ok_or(StorageError::Integrity("Branch base Layer"))?;
        if base.layer_stack_id != branch.layer_stack_id {
            return Err(StorageError::Integrity("Branch LayerStack ownership"));
        }
        let stack = self
            .db
            .layer_stack(branch.layer_stack_id)?
            .ok_or(StorageError::Integrity("LayerStack"))?;
        if stack.head_layer_id != branch.base_layer_id {
            return Ok(AuthorityAddResult::HeadMoved {
                expected: branch.base_layer_id,
                actual: stack.head_layer_id,
            });
        }
        if commit.root_id == base.root_id {
            return Ok(AuthorityAddResult::NoChanges {
                head_layer_id: base.id,
            });
        }
        self.verify_complete(commit.root_id)?;
        let layer = LayerRecord {
            id: LayerId::derive(branch.layer_stack_id, Some(base.id), commit.root_id),
            layer_stack_id: branch.layer_stack_id,
            parent_layer_id: Some(base.id),
            root_id: commit.root_id,
            source_branch_id: Some(branch_id),
            source_commit_id: Some(head_commit_id),
        };
        match self.db.add_layer_cas(base.id, layer) {
            Ok(()) => Ok(AuthorityAddResult::Added { layer_id: layer.id }),
            Err(StorageError::LayerHeadMoved { expected, actual }) => {
                if let Some(existing) = self.db.layer_by_source(branch_id, head_commit_id)? {
                    Ok(AuthorityAddResult::UpToDate {
                        layer_id: existing.id,
                    })
                } else {
                    Ok(AuthorityAddResult::HeadMoved { expected, actual })
                }
            }
            Err(error) => Err(error),
        }
    }
}
