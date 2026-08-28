use crate::{
    BranchId, BranchRollbackOutcome, BranchRollbackPublication, ChildMergeOutcome,
    ChildMergePublication, LayerFs, PushBranchReceipt, Result, ResumeToken,
};
use layerfs_sync::DurableControlEndpoint;

impl LayerFs {
    pub fn push_branch(
        &self,
        destination: &impl DurableControlEndpoint,
        request_id: [u8; 32],
        branch: BranchId,
        expected: Option<crate::BranchHead>,
        resume: ResumeToken,
    ) -> Result<PushBranchReceipt> {
        Ok(layerfs_sync::push_branch(
            &self.working,
            destination,
            request_id,
            branch,
            expected,
            resume,
        )?)
    }

    pub fn push_child_branch_merge(
        &self,
        destination: &impl DurableControlEndpoint,
        publication: ChildMergePublication,
    ) -> Result<ChildMergeOutcome> {
        Ok(layerfs_sync::push_child_branch_merge(
            destination,
            publication,
        )?)
    }

    pub fn push_branch_rollback(
        &self,
        destination: &impl DurableControlEndpoint,
        publication: BranchRollbackPublication,
    ) -> Result<BranchRollbackOutcome> {
        Ok(layerfs_sync::push_branch_rollback(
            destination,
            publication,
        )?)
    }
}
