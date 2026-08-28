use crate::{OperationId, OperationRecordRef, PreservedOperationCandidate, RecoverableOperation};
use crate::{Result, VersionRef, WorkingStore};

impl WorkingStore {
    pub fn checkpoint_operation_candidate(
        &self,
        operation_id: OperationId,
        root: layerfs_core::ObjectId,
    ) -> Result<()> {
        self.storage
            .product_record_operation_candidate(operation_id, root)?;
        Ok(())
    }

    pub fn checkpoint_version_operation_candidate(
        &self,
        operation_id: OperationId,
        version: VersionRef,
    ) -> Result<()> {
        self.storage
            .product_record_version_operation_candidate(operation_id, version)?;
        Ok(())
    }

    pub fn discard_operation(&self, operation_id: OperationId) -> Result<bool> {
        Ok(self.storage.product_discard_operation(operation_id)?)
    }

    pub fn recoverable_operations(&self, limit: usize) -> Result<Vec<RecoverableOperation>> {
        Ok(self.storage.product_recoverable_operations(limit)?)
    }

    pub fn recoverable_operations_after(
        &self,
        after: Option<OperationId>,
        limit: usize,
    ) -> Result<Vec<RecoverableOperation>> {
        Ok(self
            .storage
            .product_recoverable_operations_after(after, limit)?)
    }

    pub fn acknowledge_operation(&self, record: OperationRecordRef) -> Result<bool> {
        Ok(self
            .storage
            .product_acknowledge_operation(record.operation_id, record.operation_version_id)?)
    }

    pub fn acknowledge_conflict(&self, candidate: PreservedOperationCandidate) -> Result<bool> {
        Ok(self
            .storage
            .product_acknowledge_conflict(candidate.operation_id, candidate.root)?)
    }
}
