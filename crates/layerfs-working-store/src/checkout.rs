use crate::{BranchHead, BranchId, BranchPushOutcome, Result, WorkingError, WorkingStore};
use layerfs_storage::{
    working::outbox::PushOutboxEntry, BranchPushBundle, BranchPushRequest, RequestId,
    StoredTransferState, SyncTransferCounters, VerifiedFetchRequest,
};

impl WorkingStore {
    pub fn sync_has_object(&self, id: layerfs_core::ObjectId) -> Result<bool> {
        Ok(self.storage.contains_authenticated_object(id)?)
    }

    pub fn sync_read_object(&self, id: layerfs_core::ObjectId, maximum: usize) -> Result<Vec<u8>> {
        Ok(self
            .storage
            .load_canonical_authenticated_bounded(id, maximum)?)
    }

    pub fn sync_accept_objects(
        &self,
        owner_request_id: RequestId,
        request_id: RequestId,
        direction: &str,
        objects: &[(layerfs_core::ObjectId, Vec<u8>)],
    ) -> Result<()> {
        Ok(self.storage.accept_canonical_batch_pinned(
            owner_request_id,
            request_id,
            direction,
            objects,
        )?)
    }

    pub fn abort_sync_transfer(&self, owner: RequestId, direction: &str) -> Result<u64> {
        Ok(self.storage.product_abort_sync_transfer(owner, direction)?)
    }

    pub fn reap_one_abandoned_sync(
        &self,
        older_than_unix_seconds: i64,
    ) -> Result<Option<(RequestId, String, u64)>> {
        Ok(self
            .storage
            .product_reap_one_abandoned_sync(older_than_unix_seconds)?)
    }

    pub fn sync_custody_rows(&self, owner: RequestId, direction: &str) -> Result<u64> {
        Ok(self.storage.product_sync_custody_rows(owner, direction)?)
    }

    pub fn export_branch_push(
        &self,
        branch_id: BranchId,
        base: Option<BranchHead>,
    ) -> Result<BranchPushBundle> {
        Ok(self.storage.product_export_branch_push(branch_id, base)?)
    }

    pub fn accept_verified_fetch(
        &self,
        durable_storage_id: [u8; 32],
        request_id: RequestId,
        bundle: &BranchPushBundle,
        counters: SyncTransferCounters,
    ) -> Result<BranchHead> {
        let expected = self
            .storage
            .product_fetch_resume_branch_head(bundle.head.branch_id)?;
        if expected != bundle.base {
            return Err(WorkingError::InvalidReceipt);
        }
        match self.storage.product_import_verified_branch_fetch(
            expected,
            bundle,
            VerifiedFetchRequest {
                request_id,
                durable_storage_id,
                counters,
            },
        )? {
            BranchPushOutcome::DurablyAccepted { head, .. } if head == bundle.head => {}
            _ => return Err(WorkingError::InvalidReceipt),
        }
        Ok(bundle.head)
    }

    pub fn has_verified_branch_tracking(
        &self,
        durable_storage_id: [u8; 32],
        head: BranchHead,
    ) -> Result<bool> {
        Ok(self
            .storage
            .product_has_verified_branch_tracking(durable_storage_id, head)?)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_transfer_state(
        &self,
        owner_request_id: RequestId,
        request_id: RequestId,
        batch_sequence: u64,
        direction: &str,
        cursor: &[u8],
        complete: bool,
        counters: SyncTransferCounters,
    ) -> Result<bool> {
        Ok(self.storage.product_record_transfer_state(
            owner_request_id,
            request_id,
            batch_sequence,
            direction,
            cursor,
            complete,
            counters,
        )?)
    }

    pub fn latest_transfer_state(
        &self,
        request_id: RequestId,
        direction: &str,
    ) -> Result<Option<StoredTransferState>> {
        Ok(self
            .storage
            .product_latest_transfer_state(request_id, direction)?)
    }

    pub fn clear_transfer_state(&self, request_id: RequestId, direction: &str) -> Result<bool> {
        Ok(self
            .storage
            .product_clear_transfer_state(request_id, direction)?)
    }

    pub fn clear_transfer_state_owner(
        &self,
        owner_request_id: RequestId,
        direction: &str,
    ) -> Result<bool> {
        Ok(self
            .storage
            .product_clear_transfer_state_owner(owner_request_id, direction)?)
    }

    pub fn record_push_outbox(
        &self,
        request_id: RequestId,
        durable_storage_id: [u8; 32],
        head: BranchHead,
        expected: Option<BranchHead>,
        request: Option<BranchPushRequest>,
        state: &str,
    ) -> Result<bool> {
        Ok(self.storage.product_record_push_outbox(
            request_id,
            durable_storage_id,
            head,
            expected,
            request,
            state,
        )?)
    }

    pub fn push_outbox_state(&self, request_id: RequestId) -> Result<Option<String>> {
        Ok(self.storage.product_push_outbox_state(request_id)?)
    }

    pub fn push_outbox_entry(&self, request_id: RequestId) -> Result<Option<PushOutboxEntry>> {
        Ok(self.storage.product_push_outbox_entry(request_id)?)
    }

    pub fn object_ids_page(
        &self,
        after: Option<layerfs_core::ObjectId>,
        limit: usize,
    ) -> Result<Vec<layerfs_core::ObjectId>> {
        Ok(self.storage.object_ids_page(after, limit)?)
    }

    pub fn branch_push_object_page(
        &self,
        branch: BranchId,
        base: Option<BranchHead>,
        after: Option<layerfs_core::ObjectId>,
        limit: usize,
    ) -> Result<Vec<layerfs_core::ObjectId>> {
        let bundle = self.storage.product_export_branch_push(branch, base)?;
        let stack = bundle.origin_stack.head;
        Ok(self.storage.product_branch_fetch_object_page(
            branch,
            base,
            Some(stack),
            bundle.head,
            bundle.origin_stack.head,
            after,
            limit,
        )?)
    }
}
