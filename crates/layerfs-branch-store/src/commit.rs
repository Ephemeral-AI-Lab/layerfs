use crate::BranchStore;
use layerfs_content::filesystem::ContentChange;
use layerfs_storage::{
    apply_changes, BuiltRoot, CommitId, CommitRecord, RefOutcome, Result, StorageError,
};

impl BranchStore {
    pub fn commit(
        &self,
        branch_id: layerfs_storage::BranchId,
        expected_head: CommitId,
        changes: &[ContentChange],
    ) -> Result<RefOutcome<CommitId>> {
        let _operation = self.db.enter_operation()?;
        let branch = self
            .db
            .branch(branch_id)?
            .ok_or(StorageError::NotFound("Branch"))?;
        if branch.head_commit_id != expected_head {
            return Err(StorageError::CommitHeadMoved(layerfs_storage::HeadMoved {
                expected: Some(expected_head),
                actual: Some(branch.head_commit_id),
            }));
        }
        let parent = self
            .db
            .commit(expected_head)?
            .ok_or(StorageError::MissingBaseData)?;
        let seed = *layerfs_content::filesystem::namespace(
            &layerfs_storage::CoreReader(self),
            parent.root_id,
        )?
        .root_directory_inode
        .as_bytes();
        let built = apply_changes(self, parent.root_id, changes, seed)?;
        self.finish_commit(branch_id, expected_head, parent.root_id, built)
    }

    #[doc(hidden)]
    pub fn commit_candidate(
        &self,
        branch_id: layerfs_storage::BranchId,
        expected_head: CommitId,
        base_root: layerfs_content::ObjectId,
        built: BuiltRoot,
    ) -> Result<RefOutcome<CommitId>> {
        let _operation = self.db.enter_operation()?;
        let branch = self
            .db
            .branch(branch_id)?
            .ok_or(StorageError::NotFound("Branch"))?;
        if branch.head_commit_id != expected_head {
            return Err(StorageError::CommitHeadMoved(layerfs_storage::HeadMoved {
                expected: Some(expected_head),
                actual: Some(branch.head_commit_id),
            }));
        }
        let actual_base = self
            .db
            .commit(expected_head)?
            .ok_or(StorageError::MissingBaseData)?
            .root_id;
        if actual_base != base_root {
            return Err(StorageError::Integrity("Workspace base root"));
        }
        self.finish_commit(branch_id, expected_head, base_root, built)
    }

    fn finish_commit(
        &self,
        branch_id: layerfs_storage::BranchId,
        expected_head: CommitId,
        parent_root: layerfs_content::ObjectId,
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
