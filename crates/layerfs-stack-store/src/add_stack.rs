use crate::StackStore;
use layerfs_storage::{
    merge_candidate, AddResult, BaseId, BranchCommit, BranchId, CandidateMergeOutcome, CommitId,
    Result, ResultId, SourceId, StackHistoryId, StackId, StackRecord, StorageError,
};

impl StackStore {
    pub fn add_stack(&self, source: BranchCommit) -> Result<AddResult<StackId>> {
        let branch = self
            .db
            .branch(source.branch_id)?
            .ok_or(StorageError::MissingBaseData)?;
        let BaseId::Stack(base_stack_id) = branch.base_id else {
            return Err(StorageError::WrongSourceRoute);
        };
        let history_id = self
            .db
            .stack(base_stack_id)?
            .ok_or(StorageError::MissingBaseData)?
            .history_id;
        self.add_stack_to_history(history_id, source.branch_id, source.commit_id)
    }

    pub(crate) fn add_stack_to_history(
        &self,
        stack_history_id: StackHistoryId,
        branch_id: BranchId,
        commit_id: CommitId,
    ) -> Result<AddResult<StackId>> {
        let _operation = self.db.enter_operation()?;
        if stack_history_id.verification_key_digest()
            != *blake3::hash(&self.writer.public_key()).as_bytes()
        {
            return Err(StorageError::ReadOnlyStackHistory(
                layerfs_storage::ReadOnlyHistory {
                    history_id: stack_history_id,
                },
            ));
        }
        if let Some(existing) = self.db.add_result(SourceId::Branch(branch_id))? {
            return self.existing_result(stack_history_id, existing.result_id);
        }
        let branch = self
            .db
            .branch(branch_id)?
            .ok_or(StorageError::MissingBaseData)?;
        if branch.head_commit_id != commit_id {
            return Err(StorageError::CommitHeadMoved(layerfs_storage::HeadMoved {
                expected: Some(commit_id),
                actual: Some(branch.head_commit_id),
            }));
        }
        let BaseId::Stack(base_stack_id) = branch.base_id else {
            return Err(StorageError::WrongSourceRoute);
        };
        let base = self
            .db
            .stack(base_stack_id)?
            .ok_or(StorageError::MissingBaseData)?;
        if base.history_id != stack_history_id {
            return Err(StorageError::WrongStackHistory(
                layerfs_storage::WrongHistory {
                    expected: stack_history_id,
                    actual: base.history_id,
                },
            ));
        }
        let candidate = self
            .db
            .commit(commit_id)?
            .ok_or(StorageError::MissingBaseData)?;
        let history = self
            .db
            .stack_history(stack_history_id)?
            .ok_or(StorageError::NotFound("StackHistory"))?;
        let current = self
            .db
            .stack(history.head_stack_id)?
            .ok_or(StorageError::MissingBaseData)?;
        let merged =
            match merge_candidate(&self.db, base.root_id, current.root_id, candidate.root_id)? {
                CandidateMergeOutcome::Conflict(conflict) => {
                    return Err(StorageError::Conflict(Box::new(conflict)))
                }
                CandidateMergeOutcome::Clean(merged) => merged,
            };
        let stack = StackRecord {
            id: StackId::derive(stack_history_id, Some(current.id), merged.root_id),
            history_id: stack_history_id,
            parent_id: Some(current.id),
            root_id: merged.root_id,
        };
        let result_id = self
            .db
            .add_stack_atomic(branch_id, current.id, stack, &merged.objects)?;
        Ok(AddResult { result_id })
    }

    fn existing_result(
        &self,
        expected: StackHistoryId,
        result: ResultId,
    ) -> Result<AddResult<StackId>> {
        let ResultId::Stack(result_id) = result else {
            return Err(StorageError::WrongSourceRoute);
        };
        let stack = self
            .db
            .stack(result_id)?
            .ok_or(StorageError::MissingBaseData)?;
        if stack.history_id != expected {
            return Err(StorageError::WrongStackHistory(
                layerfs_storage::WrongHistory {
                    expected,
                    actual: stack.history_id,
                },
            ));
        }
        if !self.db.has_object(stack.root_id)? {
            return Err(StorageError::MissingBaseData);
        }
        Ok(AddResult { result_id })
    }
}
