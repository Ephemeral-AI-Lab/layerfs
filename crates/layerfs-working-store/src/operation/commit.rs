use super::BeginOperation;
use crate::{Result, WorkingError, WorkingStore};
use layerfs_core::object::access::ObjectStore;
use layerfs_storage::{
    derive_id, BranchHead, OperationCandidate, OperationCommitOutcome, OperationId,
    OperationRecordRef, PreservedOperationCandidate, RequestId,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkingCandidate {
    pub operation_id: OperationId,
    pub expected_branch_generation: u64,
    pub base_root: layerfs_core::ObjectId,
    pub candidate_root: layerfs_core::ObjectId,
    pub normalized_transition: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitResult {
    WorkingRecorded {
        head: BranchHead,
        record: OperationRecordRef,
        reconciled: bool,
    },
    Conflict {
        actual: BranchHead,
        candidate: PreservedOperationCandidate,
    },
}

pub struct WorkingCandidateWrite<'a>(pub(crate) layerfs_storage::candidate::CandidateWrite<'a>);
pub struct WorkingTrustedCandidate(pub(crate) layerfs_storage::candidate::TrustedCandidate);

impl WorkingTrustedCandidate {
    pub fn root(&self) -> layerfs_core::ObjectId {
        self.0.root()
    }

    pub fn counters(&self) -> layerfs_core::logical::LogicalCounters {
        self.0.counters()
    }
}

impl ObjectStore for WorkingCandidateWrite<'_> {
    fn get(&self, id: layerfs_core::ObjectId) -> layerfs_core::CoreResult<Vec<u8>> {
        ObjectStore::get(&self.0, id)
    }

    fn put(&mut self, canonical: &[u8]) -> layerfs_core::CoreResult<layerfs_core::ObjectId> {
        self.0.put(canonical)
    }

    fn with_authenticated_canonical<T, F>(
        &self,
        id: layerfs_core::ObjectId,
        callback: F,
    ) -> layerfs_core::CoreResult<T>
    where
        F: FnOnce(&[u8]) -> layerfs_core::CoreResult<T>,
    {
        ObjectStore::with_authenticated_canonical(&self.0, id, callback)
    }
}

impl WorkingCandidateWrite<'_> {
    pub fn trusted_replace_file<R>(
        &mut self,
        root: layerfs_core::ObjectId,
        path: &layerfs_core::CanonicalPath,
        input: R,
        initialize: (layerfs_core::inode::InodeId, layerfs_core::ObjectId),
    ) -> Result<WorkingTrustedCandidate>
    where
        R: std::io::Read,
    {
        Ok(WorkingTrustedCandidate(
            self.0.trusted_replace_file(root, path, input, initialize)?,
        ))
    }

    pub fn trusted_replace_range(
        &mut self,
        root: layerfs_core::ObjectId,
        path: &layerfs_core::CanonicalPath,
        start: u64,
        delete_len: u64,
        replacement: impl std::io::Read,
    ) -> Result<WorkingTrustedCandidate> {
        Ok(WorkingTrustedCandidate(self.0.trusted_replace_range(
            root,
            path,
            start,
            delete_len,
            replacement,
        )?))
    }

    pub fn trusted_create_directory(
        &mut self,
        root: layerfs_core::ObjectId,
        path: &layerfs_core::CanonicalPath,
        inode: layerfs_core::inode::InodeId,
        metadata_root: layerfs_core::ObjectId,
    ) -> Result<WorkingTrustedCandidate> {
        Ok(WorkingTrustedCandidate(self.0.trusted_create_directory(
            root,
            path,
            inode,
            metadata_root,
        )?))
    }

    pub fn trusted_create_symlink(
        &mut self,
        root: layerfs_core::ObjectId,
        path: &layerfs_core::CanonicalPath,
        inode: layerfs_core::inode::InodeId,
        target: Vec<u8>,
        metadata_root: layerfs_core::ObjectId,
    ) -> Result<WorkingTrustedCandidate> {
        Ok(WorkingTrustedCandidate(self.0.trusted_create_symlink(
            root,
            path,
            inode,
            target,
            metadata_root,
        )?))
    }

    pub fn trusted_hard_link(
        &mut self,
        root: layerfs_core::ObjectId,
        source: &layerfs_core::CanonicalPath,
        target: &layerfs_core::CanonicalPath,
    ) -> Result<WorkingTrustedCandidate> {
        Ok(WorkingTrustedCandidate(
            self.0.trusted_hard_link(root, source, target)?,
        ))
    }

    pub fn trusted_rename(
        &mut self,
        root: layerfs_core::ObjectId,
        from: &layerfs_core::CanonicalPath,
        to: &layerfs_core::CanonicalPath,
        source_parent_metadata_root: layerfs_core::ObjectId,
        target_parent_metadata_root: layerfs_core::ObjectId,
    ) -> Result<WorkingTrustedCandidate> {
        Ok(WorkingTrustedCandidate(self.0.trusted_rename(
            root,
            from,
            to,
            source_parent_metadata_root,
            target_parent_metadata_root,
        )?))
    }

    pub fn trusted_remove_path(
        &mut self,
        root: layerfs_core::ObjectId,
        path: &layerfs_core::CanonicalPath,
    ) -> Result<WorkingTrustedCandidate> {
        Ok(WorkingTrustedCandidate(
            self.0.trusted_remove_path(root, path)?,
        ))
    }

    pub fn trusted_apply_inode_mutations(
        &mut self,
        root: layerfs_core::ObjectId,
        mutations: impl IntoIterator<Item = layerfs_core::logical::InodeMutation>,
    ) -> Result<WorkingTrustedCandidate> {
        Ok(WorkingTrustedCandidate(
            self.0.trusted_apply_inode_mutations(root, mutations)?,
        ))
    }

    pub fn allocate_inode_id(&mut self) -> Result<layerfs_core::inode::InodeId> {
        Ok(self.0.allocate_inode_id()?)
    }

    pub fn commit_candidate(self, root: layerfs_core::ObjectId) -> Result<layerfs_core::ObjectId> {
        Ok(self.0.commit_candidate(root)?)
    }

    pub fn commit_operation_candidate(
        self,
        operation_id: OperationId,
        root: layerfs_core::ObjectId,
    ) -> Result<layerfs_core::ObjectId> {
        Ok(self.0.commit_operation_candidate(operation_id, root)?)
    }

    pub fn commit_trusted_operation_candidate(
        self,
        operation_id: OperationId,
        candidate: WorkingTrustedCandidate,
    ) -> Result<layerfs_core::ObjectId> {
        Ok(self
            .0
            .commit_trusted_operation_candidate(operation_id, candidate.0)?)
    }

    pub fn commit_objects(self) -> Result<()> {
        Ok(self.0.commit_objects()?)
    }
}

impl WorkingStore {
    pub fn begin_candidate_write(&self) -> Result<WorkingCandidateWrite<'_>> {
        Ok(WorkingCandidateWrite(self.storage.begin_candidate_write()?))
    }

    pub fn operation_commit(
        &self,
        begin: BeginOperation,
        finalized: WorkingCandidate,
    ) -> Result<CommitResult> {
        if finalized.operation_id != begin.operation_id
            || finalized.expected_branch_generation != begin.branch_head_before.generation
            || finalized.base_root != begin.base.root()
        {
            return Err(WorkingError::InvalidReceipt);
        }
        let request_id = RequestId::from_bytes(derive_id(
            b"working-operation-commit",
            &[
                begin.operation_id.as_bytes(),
                finalized.candidate_root.as_bytes(),
            ],
        ));
        Ok(
            match self.storage.product_operation_commit(OperationCandidate {
                operation_id: begin.operation_id,
                expected: begin.branch_head_before,
                candidate_root: finalized.candidate_root,
                normalized_transition: finalized.normalized_transition,
                request_id,
            })? {
                OperationCommitOutcome::WorkingRecorded {
                    head,
                    record,
                    reconciled,
                } => CommitResult::WorkingRecorded {
                    head,
                    record,
                    reconciled,
                },
                OperationCommitOutcome::Conflict { actual, candidate } => {
                    CommitResult::Conflict { actual, candidate }
                }
            },
        )
    }
}
