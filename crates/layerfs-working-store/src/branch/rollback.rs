use crate::operation::operation_entropy;
use crate::{BranchHead, BranchRollbackPublication, OperationVersionId, Result, WorkingStore};
use layerfs_storage::{derive_id, BranchRollbackOutcome, RequestId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum BranchRollbackResult {
    WorkingRecorded {
        head: BranchHead,
        publication: BranchRollbackPublication,
        reconciled: bool,
    },
    Conflict {
        actual: BranchHead,
    },
    Blocked,
}

impl WorkingStore {
    pub fn branch_rollback(
        &self,
        expected: BranchHead,
        target: OperationVersionId,
    ) -> Result<BranchRollbackResult> {
        let entropy = operation_entropy(self.storage_id())?;
        let request_id = RequestId::from_bytes(derive_id(
            b"working-branch-rollback",
            &[
                expected.branch_id.as_bytes(),
                &expected.generation.to_be_bytes(),
                target.as_bytes(),
                &entropy,
            ],
        ));
        Ok(
            match self
                .storage
                .product_branch_rollback(expected, target, request_id)?
            {
                BranchRollbackOutcome::WorkingRecorded { head, reconciled } => {
                    BranchRollbackResult::WorkingRecorded {
                        head,
                        publication: BranchRollbackPublication {
                            expected,
                            target,
                            request_id,
                            accepted: head,
                        },
                        reconciled,
                    }
                }
                BranchRollbackOutcome::Conflict { actual } => {
                    BranchRollbackResult::Conflict { actual }
                }
                BranchRollbackOutcome::Blocked => BranchRollbackResult::Blocked,
            },
        )
    }
}
