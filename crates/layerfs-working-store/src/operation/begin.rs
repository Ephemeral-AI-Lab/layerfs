use crate::{Result, WorkingError, WorkingStore};
use layerfs_storage::{derive_id, BranchHead, LeaseId, OperationId, VersionRef};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_OPERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BeginOperation {
    pub operation_id: OperationId,
    pub branch_head_before: BranchHead,
    pub base: VersionRef,
    pub lease_id: LeaseId,
    pub working_storage_id: [u8; 32],
    pub workspace_nonce: [u8; 16],
}

impl WorkingStore {
    pub fn begin_operation(&self, expected: BranchHead) -> Result<BeginOperation> {
        let entropy = operation_entropy(self.storage_id())?;
        let operation_id = OperationId::from_bytes(derive_id(b"operation", &[&entropy]));
        let lease_id = LeaseId::from_bytes(derive_id(
            b"operation-lease",
            &[operation_id.as_bytes(), &entropy],
        ));
        let admission = self
            .storage
            .product_begin_operation(operation_id, expected, lease_id)?;
        let nonce_id = derive_id(b"workspace-nonce", &[operation_id.as_bytes(), &entropy]);
        let mut nonce = [0_u8; 16];
        nonce.copy_from_slice(&nonce_id[..16]);
        Ok(BeginOperation {
            operation_id,
            branch_head_before: admission.branch_head,
            base: admission.base,
            lease_id,
            working_storage_id: self.storage_id(),
            workspace_nonce: nonce,
        })
    }
}

pub(crate) fn operation_entropy(storage_id: [u8; 32]) -> Result<[u8; 48]> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| WorkingError::InvalidReceipt)?
        .as_nanos();
    let sequence = NEXT_OPERATION.fetch_add(1, Ordering::Relaxed);
    let mut entropy = [0_u8; 48];
    entropy[..32].copy_from_slice(&storage_id);
    entropy[32..40].copy_from_slice(&sequence.to_be_bytes());
    entropy[40..].copy_from_slice(&(now as u64).to_be_bytes());
    Ok(entropy)
}
