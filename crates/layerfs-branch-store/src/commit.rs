use crate::BranchStore;
use layerfs_storage_core::internal::StagedChange;
use layerfs_storage_core::{
    apply_changes, apply_staged_changes, BuiltRoot, Change, CommitId, CommitRecord, RefOutcome,
    Result, StorageError,
};

impl BranchStore {
    pub fn commit(
        &self,
        branch_id: layerfs_storage_core::BranchId,
        expected_head: CommitId,
        changes: &[Change],
    ) -> Result<RefOutcome<CommitId>> {
        let _operation = self.db.enter_operation()?;
        let branch = self
            .db
            .branch(branch_id)?
            .ok_or(StorageError::NotFound("Branch"))?;
        if branch.head_commit_id != expected_head {
            return Err(StorageError::CommitHeadMoved(
                layerfs_storage_core::HeadMoved {
                    expected: Some(expected_head),
                    actual: Some(branch.head_commit_id),
                },
            ));
        }
        let parent = self
            .db
            .commit(expected_head)?
            .ok_or(StorageError::MissingBaseData)?;
        let built = apply_changes(self, parent.root_id, changes, parent.root_id.to_bytes())?;
        self.finish_commit(branch_id, expected_head, parent.root_id, built)
    }

    #[doc(hidden)]
    pub fn commit_staged(
        &self,
        branch_id: layerfs_storage_core::BranchId,
        expected_head: CommitId,
        changes: &[StagedChange],
    ) -> Result<RefOutcome<CommitId>> {
        let _operation = self.db.enter_operation()?;
        let branch = self
            .db
            .branch(branch_id)?
            .ok_or(StorageError::NotFound("Branch"))?;
        if branch.head_commit_id != expected_head {
            return Err(StorageError::CommitHeadMoved(
                layerfs_storage_core::HeadMoved {
                    expected: Some(expected_head),
                    actual: Some(branch.head_commit_id),
                },
            ));
        }
        let parent = self
            .db
            .commit(expected_head)?
            .ok_or(StorageError::MissingBaseData)?;
        let built = apply_staged_changes(self, parent.root_id, changes, parent.root_id.to_bytes())?;
        self.finish_commit(branch_id, expected_head, parent.root_id, built)
    }

    fn finish_commit(
        &self,
        branch_id: layerfs_storage_core::BranchId,
        expected_head: CommitId,
        parent_root: layerfs_core::ObjectId,
        built: BuiltRoot,
    ) -> Result<RefOutcome<CommitId>> {
        if built.root_id == parent_root {
            return Ok(RefOutcome::UpToDate(expected_head));
        }
        let commit = CommitRecord {
            id: CommitId::derive(built.root_id, Some(expected_head), None),
            root_id: built.root_id,
            parent_id: Some(expected_head),
            merge_parent_id: None,
        };
        self.db
            .commit_branch(branch_id, expected_head, commit, Some(&built.objects))?;
        Ok(RefOutcome::Created(commit.id))
    }
}
