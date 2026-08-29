use crate::BranchStore;
use layerfs_storage_core::{
    commit_merge_base, three_way, CommitId, CommitRecord, MergeBaseOutcome, MergeOutcome, Result,
    StorageError, ThreeWayOutcome,
};

impl BranchStore {
    pub fn merge(
        &self,
        source_branch_id: layerfs_storage_core::BranchId,
        target_branch_id: layerfs_storage_core::BranchId,
        expected_target_head: CommitId,
    ) -> Result<MergeOutcome> {
        let _operation = self.db.enter_operation()?;
        let source = self
            .db
            .branch(source_branch_id)?
            .ok_or(StorageError::NotFound("source Branch"))?;
        let target = self
            .db
            .branch(target_branch_id)?
            .ok_or(StorageError::NotFound("target Branch"))?;
        if target.head_commit_id != expected_target_head {
            return Err(StorageError::CommitHeadMoved(
                layerfs_storage_core::HeadMoved {
                    expected: Some(expected_target_head),
                    actual: Some(target.head_commit_id),
                },
            ));
        }
        let fallback_base = self.parent.common_base(source.base_id, target.base_id)?;
        if source.head_commit_id == target.head_commit_id
            || self
                .db
                .is_commit_ancestor(source.head_commit_id, target.head_commit_id)?
        {
            return Ok(MergeOutcome::UpToDate(target.head_commit_id));
        }
        if source.base_id == target.base_id
            && self
                .db
                .is_commit_ancestor(target.head_commit_id, source.head_commit_id)?
        {
            let source_commit = self
                .db
                .commit(source.head_commit_id)?
                .ok_or(StorageError::MissingBaseData)?;
            self.db.commit_branch(
                target.id,
                target.head_commit_id,
                CommitRecord {
                    id: source_commit.id,
                    ..source_commit
                },
                None,
            )?;
            return Ok(MergeOutcome::FastForwarded(source.head_commit_id));
        }
        let base_root =
            match commit_merge_base(&self.db, source.head_commit_id, target.head_commit_id)? {
                MergeBaseOutcome::Commit(id) => {
                    self.db
                        .commit(id)?
                        .ok_or(StorageError::MissingBaseData)?
                        .root_id
                }
                MergeBaseOutcome::None => fallback_base.root_id,
            };
        let source_root = self
            .db
            .commit(source.head_commit_id)?
            .ok_or(StorageError::MissingBaseData)?
            .root_id;
        let target_root = self
            .db
            .commit(target.head_commit_id)?
            .ok_or(StorageError::MissingBaseData)?
            .root_id;
        let merged = match three_way(self, base_root, target_root, source_root)? {
            ThreeWayOutcome::Conflict(conflict) => {
                return Err(StorageError::Conflict(Box::new(conflict)))
            }
            ThreeWayOutcome::Clean(merged) => merged,
        };
        let commit = CommitRecord {
            id: CommitId::derive(
                merged.root_id,
                Some(target.head_commit_id),
                Some(source.head_commit_id),
            ),
            root_id: merged.root_id,
            parent_id: Some(target.head_commit_id),
            merge_parent_id: Some(source.head_commit_id),
        };
        self.db.commit_branch(
            target.id,
            target.head_commit_id,
            commit,
            Some(&merged.objects),
        )?;
        Ok(MergeOutcome::Merged(commit.id))
    }
}
