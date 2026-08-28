use crate::{
    BranchHead, BranchId, BranchPushBundle, BranchPushOutcome, BranchPushRequest,
    BranchRollbackOutcome, BranchRollbackPublication, ChildMergeOutcome, ChildMergePublication,
    DurableError, DurableStore, RequestId, Result,
};
use layerfs_storage::EngineError;

impl DurableStore {
    pub fn drop_branch(&self, branch_id: BranchId) -> Result<()> {
        let _ = branch_id;
        Err(DurableError::Storage(EngineError::InvalidRecord(
            "Full Branch drop requires P4",
        )))
    }

    pub fn accept_child_branch_merge(
        &self,
        publication: ChildMergePublication,
    ) -> Result<ChildMergeOutcome> {
        let _ = publication;
        Err(DurableError::Storage(EngineError::InvalidRecord(
            "Full child Branch merge requires P4",
        )))
    }

    pub fn accept_branch_rollback(
        &self,
        publication: BranchRollbackPublication,
    ) -> Result<BranchRollbackOutcome> {
        let _ = publication;
        Err(DurableError::Storage(EngineError::InvalidRecord(
            "Full Branch rollback requires P4",
        )))
    }

    pub fn stage_branch_push_page(
        &self,
        transfer_id: RequestId,
        page_sequence: u64,
        data_request_id: RequestId,
        bundle: &BranchPushBundle,
        counters: layerfs_storage::SyncTransferCounters,
    ) -> Result<()> {
        Ok(self.storage.stage_verified_branch_push_page(
            transfer_id,
            page_sequence,
            data_request_id,
            bundle,
            counters,
        )?)
    }

    pub fn commit_staged_branch_push(
        &self,
        request: BranchPushRequest,
        branch_id: BranchId,
    ) -> Result<BranchPushOutcome> {
        Ok(self
            .storage
            .commit_verified_ordinary_branch_push(request, branch_id)?)
    }

    pub fn reconcile_branch_push(
        &self,
        request: BranchPushRequest,
        accepted: BranchHead,
    ) -> Result<BranchPushOutcome> {
        let outcome = self
            .storage
            .reconcile_verified_ordinary_branch_push(request, accepted.branch_id)?;
        if matches!(outcome, BranchPushOutcome::DurablyAccepted { head, .. } if head == accepted) {
            Ok(outcome)
        } else {
            Err(DurableError::Storage(EngineError::InvalidRecord(
                "Branch Push reconciliation identity",
            )))
        }
    }

    pub fn record_replayed_branch_push(
        &self,
        request: BranchPushRequest,
        accepted: BranchHead,
    ) -> Result<BranchPushOutcome> {
        self.reconcile_branch_push(request, accepted)
    }
}
