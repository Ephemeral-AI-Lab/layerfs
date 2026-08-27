//! Host-recoverable Branch and Operation authority over one Working StorageId.

#![forbid(unsafe_code)]

use layerfs_core::object::access::{ObjectRead, ObjectStore};
pub use layerfs_storage::integrity::IntegrityMode;
use layerfs_storage::product::{
    derive_id, BranchPushBundle, BranchRollbackOutcome, ChildMergeCandidate, ChildMergeOutcome,
    LayerCandidateRequest, LeaseId, OperationCandidate, OperationCommitOutcome, RequestId,
    StoredTransferState, SyncTransferCounters, VerifiedFetchRequest,
};
pub use layerfs_storage::product::{
    BranchHead, BranchId, BranchPushOutcome, BranchRollbackPublication, ChildMergePublication,
    LayerCandidate, LayerId, LayerStackHead, LayerStackId, OperationId, OperationRecordRef,
    OperationVersionId, PreservedOperationCandidate, RecoverableOperation,
    RecoverableOperationState, VersionRef,
};
pub use layerfs_storage::scratch::ScratchObservation;
pub use layerfs_storage::scratch::{DiskNamespace, DiskTable};
pub use layerfs_storage::StorageError;
pub use layerfs_storage::{CompactionStorageObservation, EngineCounters};
use layerfs_storage::{EngineError, Storage};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_OPERATION: AtomicU64 = AtomicU64::new(0);

pub const COMPONENT: &str = "layerfs-working-store";
const ABANDONED_SYNC_SECONDS: u64 = 24 * 60 * 60;
const STARTUP_SYNC_REAP_LIMIT: usize = 64;

#[derive(Debug)]
pub enum WorkingError {
    Core(layerfs_core::CoreError),
    Storage(EngineError),
    InvalidReceipt,
    Io(std::io::Error),
}

impl WorkingError {
    pub fn is_no_space(&self) -> bool {
        matches!(
            self,
            Self::Storage(EngineError::Sqlite {
                kind: layerfs_storage::SqliteErrorKind::NoSpace,
                ..
            })
        )
    }

    pub fn is_read_only(&self) -> bool {
        matches!(
            self,
            Self::Storage(EngineError::Sqlite {
                kind: layerfs_storage::SqliteErrorKind::ReadOnly,
                ..
            })
        )
    }
}

impl fmt::Display for WorkingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for WorkingError {}

impl From<layerfs_core::CoreError> for WorkingError {
    fn from(value: layerfs_core::CoreError) -> Self {
        Self::Core(value)
    }
}

impl From<EngineError> for WorkingError {
    fn from(value: EngineError) -> Self {
        Self::Storage(value)
    }
}

impl From<std::io::Error> for WorkingError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub type Result<T> = std::result::Result<T, WorkingError>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BeginOperation {
    pub operation_id: OperationId,
    pub branch_head_before: BranchHead,
    pub base: VersionRef,
    pub lease_id: LeaseId,
    pub working_storage_id: [u8; 32],
    pub workspace_nonce: [u8; 16],
}

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

#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum ChildMergeResult {
    WorkingRecorded {
        parent_head: BranchHead,
        publication: ChildMergePublication,
        reconciled: bool,
    },
    ContentConflict(layerfs_core::logical::MergeConflict),
    DestinationConflict {
        actual_parent: BranchHead,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayerPreparationResult {
    Prepared(LayerCandidate),
    ContentConflict(layerfs_core::logical::MergeConflict),
}

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

pub struct WorkingStore {
    root: PathBuf,
    storage: Storage,
}

pub struct WorkingCandidateWrite<'a>(layerfs_storage::candidate::CandidateWrite<'a>);
pub struct WorkingTrustedCandidate(layerfs_storage::candidate::TrustedCandidate);

impl WorkingTrustedCandidate {
    pub fn root(&self) -> layerfs_core::ObjectId {
        self.0.root()
    }

    pub fn counters(&self) -> layerfs_core::logical::LogicalCounters {
        self.0.counters()
    }
}

impl ObjectRead for WorkingStore {
    fn get(&self, id: layerfs_core::ObjectId) -> layerfs_core::CoreResult<Vec<u8>> {
        ObjectRead::get(&self.storage, id)
    }

    fn with_authenticated_canonical<T, F>(
        &self,
        id: layerfs_core::ObjectId,
        callback: F,
    ) -> layerfs_core::CoreResult<T>
    where
        F: FnOnce(&[u8]) -> layerfs_core::CoreResult<T>,
    {
        ObjectRead::with_authenticated_canonical(&self.storage, id, callback)
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
        let candidate = self.0.trusted_replace_file(root, path, input, initialize)?;
        Ok(WorkingTrustedCandidate(candidate))
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
    pub fn open(root: &Path, mode: IntegrityMode) -> Result<Self> {
        prepare_store_root(root)?;
        let root = fs::canonicalize(root)?;
        let generation_root = root.join("working.sqlite.generations");
        let storage = layerfs_storage::generation::open_or_create_with_legacy(
            &generation_root,
            &root.join("working.sqlite"),
            &layerfs_storage::generation::NativeGenerationDriver,
            mode,
        )?;
        let cutoff = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| std::io::Error::other(error.to_string()))?
            .as_secs()
            .saturating_sub(ABANDONED_SYNC_SECONDS);
        let cutoff = i64::try_from(cutoff).map_err(|_| EngineError::CounterOverflow)?;
        for _ in 0..STARTUP_SYNC_REAP_LIMIT {
            if storage.product_reap_one_abandoned_sync(cutoff)?.is_none() {
                break;
            }
        }
        Ok(Self { root, storage })
    }

    pub fn storage_id(&self) -> [u8; 32] {
        self.storage.store_id_cached()
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn database_path(&self) -> &Path {
        self.storage.path()
    }

    pub fn begin_candidate_write(&self) -> Result<WorkingCandidateWrite<'_>> {
        Ok(WorkingCandidateWrite(self.storage.begin_candidate_write()?))
    }

    pub fn counters(&self) -> Result<EngineCounters> {
        Ok(self.storage.counters()?)
    }

    pub fn reset_counters(&self) -> Result<()> {
        Ok(self.storage.reset_counters()?)
    }

    pub fn active_connection_count(&self) -> Result<u64> {
        Ok(self.storage.active_connection_count()?)
    }

    pub fn close_primary_connection(&self) -> Result<()> {
        Ok(self.storage.close_primary_connection()?)
    }

    #[cfg(feature = "test-hooks")]
    #[doc(hidden)]
    pub fn inject_fetch_boundary_failure_for_test(&mut self) {
        self.storage.inject_fetch_boundary_failure();
    }

    #[cfg(feature = "test-hooks")]
    #[doc(hidden)]
    pub fn corrupt_object_for_test(
        &self,
        id: layerfs_core::ObjectId,
        canonical: &[u8],
    ) -> Result<()> {
        Ok(self.storage.corrupt_object_for_test(id, canonical)?)
    }

    pub fn create_scratch_table(&self, label: &str) -> Result<DiskTable> {
        Ok(self.storage.create_scratch_table(label)?)
    }

    pub fn create_layer_stack(
        &self,
        layer_stack_id: LayerStackId,
        layer_id: LayerId,
        name: &str,
        root: layerfs_core::ObjectId,
    ) -> Result<LayerStackHead> {
        Ok(self
            .storage
            .product_create_layer_stack(layer_stack_id, layer_id, name, root)?)
    }

    pub fn layer_stack_head(&self, layer_stack_id: LayerStackId) -> Result<Option<LayerStackHead>> {
        Ok(self.storage.product_layer_stack_head(layer_stack_id)?)
    }

    pub fn fetch_resume_layer_stack_head(
        &self,
        layer_stack_id: LayerStackId,
    ) -> Result<Option<LayerStackHead>> {
        Ok(self
            .storage
            .product_fetch_resume_layer_stack_head(layer_stack_id)?)
    }

    pub fn create_top_level_branch(
        &self,
        branch_id: BranchId,
        name: Option<&str>,
        origin: LayerStackHead,
    ) -> Result<BranchHead> {
        Ok(self
            .storage
            .product_create_top_level_branch(branch_id, name, origin)?)
    }

    pub fn create_child_branch(
        &self,
        branch_id: BranchId,
        name: Option<&str>,
        origin: OperationRecordRef,
    ) -> Result<BranchHead> {
        Ok(self
            .storage
            .product_create_child_branch(branch_id, name, origin)?)
    }

    pub fn branch_head(&self, branch_id: BranchId) -> Result<Option<BranchHead>> {
        Ok(self.storage.product_branch_head(branch_id)?)
    }

    pub fn branch_has_special_history_after(
        &self,
        branch_id: BranchId,
        generation: u64,
    ) -> Result<bool> {
        Ok(self
            .storage
            .product_branch_has_special_history_after(branch_id, generation)?)
    }

    pub fn fetch_resume_branch_head(&self, branch_id: BranchId) -> Result<Option<BranchHead>> {
        Ok(self.storage.product_fetch_resume_branch_head(branch_id)?)
    }

    pub fn branch_parent(&self, branch_id: BranchId) -> Result<Option<BranchId>> {
        Ok(self
            .storage
            .product_branch_ancestry(branch_id)?
            .and_then(|ancestry| ancestry.immediate_parent_branch_id))
    }

    pub fn contains_branch_head(&self, head: BranchHead) -> Result<bool> {
        Ok(self.storage.product_contains_branch_head(head)?)
    }

    pub fn branch_contains_root(
        &self,
        branch: BranchId,
        root: layerfs_core::ObjectId,
    ) -> Result<bool> {
        Ok(self.storage.product_branch_contains_root(branch, root)?)
    }

    pub fn pin_branch_version(&self, head: BranchHead) -> Result<VersionRef> {
        Ok(self.storage.product_pin_branch_version(head)?)
    }

    pub fn validate_version_ref(&self, version: VersionRef) -> Result<()> {
        Ok(self.storage.product_validate_version_ref(version)?)
    }

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
        let begin = BeginOperation {
            operation_id,
            branch_head_before: admission.branch_head,
            base: admission.base,
            lease_id,
            working_storage_id: self.storage_id(),
            workspace_nonce: nonce,
        };
        Ok(begin)
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

    pub fn child_branch_merge(
        &self,
        source: BranchHead,
        expected_parent: BranchHead,
    ) -> Result<ChildMergeResult> {
        let ancestry = self
            .storage
            .product_branch_ancestry(source.branch_id)?
            .ok_or(WorkingError::InvalidReceipt)?;
        if ancestry.immediate_parent_branch_id != Some(expected_parent.branch_id)
            || self.storage.product_branch_head(source.branch_id)? != Some(source)
        {
            return Err(WorkingError::InvalidReceipt);
        }
        let mut writer = self.storage.begin_candidate_write()?;
        let merged = match layerfs_core::logical::merge_roots(
            &mut writer,
            ancestry.fork_root,
            source.root,
            expected_parent.root,
        )? {
            Ok(candidate) => candidate,
            Err(conflict) => return Ok(ChildMergeResult::ContentConflict(conflict)),
        };
        writer.commit_candidate(merged.root())?;
        let source_version = source
            .operation_version_id
            .ok_or(WorkingError::InvalidReceipt)?;
        let request_id = RequestId::from_bytes(derive_id(
            b"working-child-branch-merge",
            &[
                source.branch_id.as_bytes(),
                source_version.as_bytes(),
                expected_parent.branch_id.as_bytes(),
                &expected_parent.generation.to_be_bytes(),
                merged.root().as_bytes(),
            ],
        ));
        let candidate = ChildMergeCandidate {
            source,
            expected_parent,
            result_root: merged.root(),
            source_transition: Vec::new(),
            applied_transition: Vec::new(),
            request_id,
        };
        Ok(
            match self.storage.product_child_branch_merge(candidate.clone())? {
                ChildMergeOutcome::WorkingRecorded {
                    parent_head,
                    reconciled,
                } => ChildMergeResult::WorkingRecorded {
                    parent_head,
                    publication: ChildMergePublication {
                        candidate,
                        accepted_parent: parent_head,
                    },
                    reconciled,
                },
                ChildMergeOutcome::Conflict { actual_parent } => {
                    ChildMergeResult::DestinationConflict { actual_parent }
                }
            },
        )
    }

    pub fn prepare_layer_stack_merge(
        &self,
        source: BranchHead,
        expected_stack: LayerStackHead,
    ) -> Result<LayerPreparationResult> {
        let ancestry = self
            .storage
            .product_branch_ancestry(source.branch_id)?
            .ok_or(WorkingError::InvalidReceipt)?;
        if ancestry.origin_layer_stack_id != expected_stack.layer_stack_id
            || self.storage.product_branch_head(source.branch_id)? != Some(source)
            || self
                .storage
                .product_layer_stack_head(expected_stack.layer_stack_id)?
                != Some(expected_stack)
        {
            return Err(WorkingError::InvalidReceipt);
        }
        let origin_root = self
            .storage
            .product_layer_root(ancestry.origin_layer_stack_id, ancestry.origin_layer_id)?
            .ok_or(WorkingError::InvalidReceipt)?;
        let mut writer = self.storage.begin_candidate_write()?;
        let merged = match layerfs_core::logical::merge_roots(
            &mut writer,
            origin_root,
            source.root,
            expected_stack.root,
        )? {
            Ok(candidate) => candidate,
            Err(conflict) => return Ok(LayerPreparationResult::ContentConflict(conflict)),
        };
        writer.commit_candidate(merged.root())?;
        let entropy = operation_entropy(self.storage_id())?;
        let request_id = RequestId::from_bytes(derive_id(
            b"working-layer-stack-candidate",
            &[
                source.branch_id.as_bytes(),
                expected_stack.layer_stack_id.as_bytes(),
                &expected_stack.generation.to_be_bytes(),
                merged.root().as_bytes(),
                &entropy,
            ],
        ));
        Ok(LayerPreparationResult::Prepared(
            self.storage
                .product_prepare_layer_candidate(LayerCandidateRequest {
                    source,
                    expected_stack,
                    result_root: merged.root(),
                    source_transition: Vec::new(),
                    applied_transition: Vec::new(),
                    request_id,
                })?,
        ))
    }

    pub fn recoverable_layer_candidates_after(
        &self,
        after: Option<LayerId>,
        limit: usize,
    ) -> Result<Vec<LayerCandidate>> {
        Ok(self.storage.product_layer_candidates_after(after, limit)?)
    }

    pub fn drop_layer_candidate(&self, layer_id: LayerId) -> Result<bool> {
        Ok(self.storage.product_drop_layer_candidate(layer_id)?)
    }

    pub fn drop_branch(&self, branch_id: BranchId) -> Result<()> {
        Ok(self.storage.product_drop_branch(branch_id)?)
    }

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

    pub fn compact(self) -> Result<Self> {
        let Self { root, storage } = self;
        let generation_root = root.join("working.sqlite.generations");
        let storage = layerfs_storage::generation::compact(
            storage,
            &generation_root,
            &layerfs_storage::generation::NativeGenerationDriver,
        )?;
        Ok(Self { root, storage })
    }

    pub fn last_compaction_observation(&self) -> Option<CompactionStorageObservation> {
        self.storage.last_compaction_observation()
    }

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
    ) -> Result<layerfs_storage::product::BranchPushBundle> {
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
        expected_durable_generation: Option<u64>,
        state: &str,
    ) -> Result<bool> {
        Ok(self.storage.product_record_push_outbox(
            request_id,
            durable_storage_id,
            head,
            expected_durable_generation,
            state,
        )?)
    }

    pub fn push_outbox_state(&self, request_id: RequestId) -> Result<Option<String>> {
        Ok(self.storage.product_push_outbox_state(request_id)?)
    }

    pub fn push_outbox_head(&self, request_id: RequestId) -> Result<Option<(BranchHead, String)>> {
        Ok(self.storage.product_push_outbox_head(request_id)?)
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

fn prepare_store_root(root: &Path) -> Result<()> {
    match fs::symlink_metadata(root) {
        Ok(metadata) if !metadata.file_type().is_dir() => return Err(WorkingError::InvalidReceipt),
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = root
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                fs::create_dir_all(parent)?;
            }
            match fs::create_dir(root) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
        }
        Err(error) => return Err(error.into()),
    }
    if !fs::symlink_metadata(root)?.file_type().is_dir() {
        return Err(WorkingError::InvalidReceipt);
    }
    set_private(root)?;
    Ok(())
}

fn operation_entropy(storage_id: [u8; 32]) -> Result<[u8; 48]> {
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

#[cfg(unix)]
fn set_private(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn set_private(_path: &Path) -> std::io::Result<()> {
    Ok(())
}
