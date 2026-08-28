use crate::{
    BranchHead, BranchId, BranchRollbackResult, ChildMergeResult, LayerFs, OperationVersionId,
    Result,
};

impl LayerFs {
    pub fn child_branch_merge(
        &self,
        source: BranchHead,
        expected_parent: BranchHead,
    ) -> Result<ChildMergeResult> {
        Ok(self.working.child_branch_merge(source, expected_parent)?)
    }

    pub fn branch_rollback(
        &self,
        expected: BranchHead,
        target: OperationVersionId,
    ) -> Result<BranchRollbackResult> {
        Ok(self.working.branch_rollback(expected, target)?)
    }

    pub fn drop_branch(&self, id: BranchId) -> Result<()> {
        Ok(self.working.drop_branch(id)?)
    }
}
