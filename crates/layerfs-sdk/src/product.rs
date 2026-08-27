use layerfs_core::content::rope::build;
use layerfs_core::inode::{inode_table_from_root, InodeKind, InodeRecordV1};
use layerfs_core::logical;
use layerfs_core::metadata::{build_metadata_tree, MetadataEntryV1, MetadataKey};
use layerfs_core::namespace::{empty_directory, NamespaceRootV1};
use layerfs_core::namespace_codec::{
    encode_inode_record, encode_namespace_root, profile_id as namespace_profile_id,
};
use layerfs_core::object::access::ObjectStore;
use layerfs_core::{CanonicalPath, ObjectId};
pub use layerfs_sync::{
    BranchRollbackOutcome, BranchRollbackPublication, ChildMergeOutcome, ChildMergePublication,
    FetchBranchReceipt, LayerStackMergeOutcome, LayerStackRollbackOutcome, PushBranchReceipt,
    PushLayerStackGenesisReceipt, ResumeToken,
};
use layerfs_sync::{DurableControlEndpoint, DurableEndpoint};
use layerfs_working_store::{BeginOperation, WorkingStore};
pub use layerfs_working_store::{
    BranchHead, BranchId, BranchPushOutcome, BranchRollbackResult, ChildMergeResult, CommitResult,
    EngineCounters, IntegrityMode, LayerCandidate, LayerId, LayerPreparationResult, LayerStackHead,
    LayerStackId, OperationId, OperationRecordRef, OperationVersionId, RecoverableOperation,
    VersionRef,
};
pub use layerfs_workspace::LeaseKind;
use layerfs_workspace::{
    DirectDriver, EndOperationReceipt, FinalizedCandidate, OperationWorkspace, Presentation,
    WorkspaceTicket,
};
use std::fmt;
use std::io::{Read, Write};
use std::ops::Range;
use std::path::Path;
use std::time::{Duration, Instant};

pub use layerfs_core::logical::{ListPage, Stat};
pub use layerfs_materialization::{NativeRoute, OperationCounters};

#[derive(Debug)]
pub enum Error {
    Core(layerfs_core::CoreError),
    Working(layerfs_working_store::WorkingError),
    Workspace(layerfs_workspace::WorkspaceError),
    Sync(layerfs_sync::SyncError),
    Materialization(layerfs_materialization::VfsError),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for Error {}

impl From<layerfs_core::CoreError> for Error {
    fn from(error: layerfs_core::CoreError) -> Self {
        Self::Core(error)
    }
}

impl From<layerfs_working_store::WorkingError> for Error {
    fn from(error: layerfs_working_store::WorkingError) -> Self {
        Self::Working(error)
    }
}

impl From<layerfs_workspace::WorkspaceError> for Error {
    fn from(error: layerfs_workspace::WorkspaceError) -> Self {
        Self::Workspace(error)
    }
}

impl From<layerfs_sync::SyncError> for Error {
    fn from(error: layerfs_sync::SyncError) -> Self {
        Self::Sync(error)
    }
}

impl From<layerfs_materialization::VfsError> for Error {
    fn from(error: layerfs_materialization::VfsError) -> Self {
        Self::Materialization(error)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct LayerFs {
    working: WorkingStore,
}

impl LayerFs {
    pub fn open(root: &Path, integrity: IntegrityMode) -> Result<Self> {
        Ok(Self {
            working: WorkingStore::open(root, integrity)?,
        })
    }

    pub fn storage_id(&self) -> [u8; 32] {
        self.working.storage_id()
    }

    pub fn working_storage_counters(&self) -> Result<EngineCounters> {
        Ok(self.working.counters()?)
    }

    pub fn reset_working_storage_counters(&self) -> Result<()> {
        Ok(self.working.reset_counters()?)
    }

    pub fn compact(self) -> Result<Self> {
        Ok(Self {
            working: self.working.compact()?,
        })
    }

    pub fn object_ids_page(&self, after: Option<ObjectId>, limit: usize) -> Result<Vec<ObjectId>> {
        Ok(self.working.object_ids_page(after, limit)?)
    }

    pub fn initialize_empty_root(&self) -> Result<ObjectId> {
        let mut writer = self.working.begin_candidate_write()?;
        let root_inode = writer.allocate_inode_id()?;
        let metadata = portable_metadata(&mut writer, InodeKind::Directory)?;
        let directory = empty_directory(&mut writer)?;
        let record = writer.put(&encode_inode_record(InodeRecordV1 {
            kind: InodeKind::Directory,
            namespace_ref_count: 0,
            content_root: directory.0,
            metadata_root: metadata,
        })?)?;
        let table = inode_table_from_root(&mut writer, root_inode, record)?;
        let root = writer.put(&encode_namespace_root(NamespaceRootV1 {
            profile_id: namespace_profile_id(),
            root_directory_inode: root_inode,
            inode_table_root: table.0,
        })?)?;
        Ok(writer.commit_candidate(root)?)
    }

    pub fn create_layer_stack(
        &self,
        id: LayerStackId,
        genesis: LayerId,
        name: &str,
        root: ObjectId,
    ) -> Result<LayerStackHead> {
        Ok(self.working.create_layer_stack(id, genesis, name, root)?)
    }

    pub fn create_top_level_branch(
        &self,
        id: BranchId,
        name: Option<&str>,
        origin: LayerStackHead,
    ) -> Result<BranchHead> {
        Ok(self.working.create_top_level_branch(id, name, origin)?)
    }

    pub fn layer_stack_head(&self, id: LayerStackId) -> Result<Option<LayerStackHead>> {
        Ok(self.working.layer_stack_head(id)?)
    }

    pub fn create_child_branch(
        &self,
        id: BranchId,
        name: Option<&str>,
        origin: OperationRecordRef,
    ) -> Result<BranchHead> {
        Ok(self.working.create_child_branch(id, name, origin)?)
    }

    pub fn branch_head(&self, id: BranchId) -> Result<Option<BranchHead>> {
        Ok(self.working.branch_head(id)?)
    }

    pub fn pin_branch_version(&self, head: BranchHead) -> Result<VersionRef> {
        Ok(self.working.pin_branch_version(head)?)
    }

    pub fn read_range(
        &self,
        version: VersionRef,
        path: &str,
        range: Range<u64>,
        output: impl Write,
    ) -> Result<logical::LogicalCounters> {
        self.working.validate_version_ref(version)?;
        Ok(logical::read_range(
            &self.working,
            version.root(),
            &CanonicalPath::new(path)?,
            range,
            output,
        )?)
    }

    pub fn stream(
        &self,
        version: VersionRef,
        path: &str,
        output: impl Write,
    ) -> Result<logical::LogicalCounters> {
        self.working.validate_version_ref(version)?;
        Ok(logical::stream(
            &self.working,
            version.root(),
            &CanonicalPath::new(path)?,
            output,
        )?)
    }

    pub fn stat(
        &self,
        version: VersionRef,
        path: &str,
    ) -> Result<(Stat, logical::LogicalCounters)> {
        self.working.validate_version_ref(version)?;
        Ok(logical::stat(
            &self.working,
            version.root(),
            &CanonicalPath::new(path)?,
        )?)
    }

    pub fn list(
        &self,
        version: VersionRef,
        path: &str,
        after: Option<&[u8]>,
        max_entries: usize,
        max_bytes: usize,
    ) -> Result<(ListPage, logical::LogicalCounters)> {
        self.working.validate_version_ref(version)?;
        Ok(logical::list(
            &self.working,
            version.root(),
            &CanonicalPath::new(path)?,
            after
                .map(layerfs_core::CanonicalName::from_bytes)
                .transpose()?
                .as_ref(),
            max_entries,
            max_bytes,
        )?)
    }

    pub fn readlink(
        &self,
        version: VersionRef,
        path: &str,
    ) -> Result<(Vec<u8>, logical::LogicalCounters)> {
        self.working.validate_version_ref(version)?;
        Ok(logical::readlink(
            &self.working,
            version.root(),
            &CanonicalPath::new(path)?,
        )?)
    }

    pub fn begin_direct(&self, expected: BranchHead) -> Result<DirectOperation<'_>> {
        let (admission, ticket) =
            layerfs_workspace::begin_operation(&self.working, expected, Presentation::Direct)?;
        let (workspace, _) = OperationWorkspace::start(ticket, DirectDriver::default(), None)?;
        Ok(DirectOperation {
            fs: self,
            admission,
            workspace,
            candidate_root: expected.root,
            terminal: false,
        })
    }

    pub fn begin_materialization(&self, expected: BranchHead) -> Result<MaterializedOperation<'_>> {
        let (admission, workspace) = self.start_materialization(expected, false)?;
        Ok(MaterializedOperation {
            fs: self,
            admission,
            workspace,
            terminal: false,
        })
    }

    pub fn begin_managed_materialization(
        &self,
        expected: BranchHead,
    ) -> Result<ManagedMaterializedOperation<'_>> {
        let (admission, workspace) = self.start_materialization(expected, true)?;
        Ok(ManagedMaterializedOperation {
            fs: self,
            admission,
            workspace,
            terminal: false,
        })
    }

    fn start_materialization(
        &self,
        expected: BranchHead,
        managed: bool,
    ) -> Result<(
        BeginOperation,
        OperationWorkspace<layerfs_materialization::MaterializationDriver<'_>>,
    )> {
        let (admission, ticket) = layerfs_workspace::begin_operation(
            &self.working,
            expected,
            Presentation::Materialization,
        )?;
        let mut paths = layerfs_workspace::WorkspacePaths::create(self.working.root(), &ticket)?;
        let host = layerfs_materialization::host_driver();
        let driver = match if managed {
            layerfs_materialization::MaterializationDriver::start_managed(
                &self.working,
                host.as_ref(),
                paths.view().to_owned(),
                expected.root,
                admission.operation_id,
            )
        } else {
            layerfs_materialization::MaterializationDriver::start(
                &self.working,
                host.as_ref(),
                paths.view().to_owned(),
                expected.root,
                admission.operation_id,
            )
        } {
            Ok(driver) => driver,
            Err(error) => {
                if paths.remove_owned().is_ok() {
                    let _ = self.working.discard_operation(admission.operation_id);
                }
                return Err(error.into());
            }
        };
        let (workspace, _) = OperationWorkspace::start(ticket, driver, Some(paths))?;
        Ok((admission, workspace))
    }

    pub fn recoverable_operations(&self, limit: usize) -> Result<Vec<RecoverableOperation>> {
        Ok(self.working.recoverable_operations(limit)?)
    }

    pub fn recoverable_operations_after(
        &self,
        after: Option<OperationId>,
        limit: usize,
    ) -> Result<Vec<RecoverableOperation>> {
        Ok(self.working.recoverable_operations_after(after, limit)?)
    }

    pub fn discard_recovered_operation(&self, operation_id: OperationId) -> Result<bool> {
        Ok(self.working.discard_operation(operation_id)?)
    }

    pub fn child_branch_merge(
        &self,
        source: BranchHead,
        expected_parent: BranchHead,
    ) -> Result<ChildMergeResult> {
        Ok(self.working.child_branch_merge(source, expected_parent)?)
    }

    pub fn prepare_layer_stack_merge(
        &self,
        source: BranchHead,
        expected_stack: LayerStackHead,
    ) -> Result<LayerPreparationResult> {
        Ok(self
            .working
            .prepare_layer_stack_merge(source, expected_stack)?)
    }

    pub fn recoverable_layer_candidates_after(
        &self,
        after: Option<LayerId>,
        limit: usize,
    ) -> Result<Vec<LayerCandidate>> {
        Ok(self
            .working
            .recoverable_layer_candidates_after(after, limit)?)
    }

    pub fn drop_layer_candidate(&self, layer_id: LayerId) -> Result<bool> {
        Ok(self.working.drop_layer_candidate(layer_id)?)
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

    pub fn push_objects(
        &self,
        destination: &impl DurableEndpoint,
        request_id: [u8; 32],
        ids: impl IntoIterator<Item = ObjectId>,
        resume: ResumeToken,
    ) -> Result<layerfs_sync::TransferReceipt> {
        Ok(layerfs_sync::client::push_objects(
            &self.working,
            destination,
            request_id,
            ids,
            resume,
        )?)
    }

    pub fn push_branch(
        &self,
        destination: &impl DurableControlEndpoint,
        request_id: [u8; 32],
        branch: BranchId,
        expected: Option<BranchHead>,
        resume: ResumeToken,
    ) -> Result<PushBranchReceipt> {
        Ok(layerfs_sync::client::push_branch(
            &self.working,
            destination,
            request_id,
            branch,
            expected,
            resume,
        )?)
    }

    pub fn push_layer_stack_genesis(
        &self,
        destination: &impl DurableControlEndpoint,
        request_id: [u8; 32],
        branch: BranchId,
        stack: LayerStackHead,
        name: &str,
        resume: ResumeToken,
    ) -> Result<PushLayerStackGenesisReceipt> {
        Ok(layerfs_sync::client::push_layer_stack_genesis(
            &self.working,
            destination,
            request_id,
            branch,
            stack,
            name,
            resume,
        )?)
    }

    pub fn fetch_branch(
        &self,
        source: &impl DurableControlEndpoint,
        request_id: [u8; 32],
        branch: BranchId,
        resume: ResumeToken,
    ) -> Result<FetchBranchReceipt> {
        Ok(layerfs_sync::client::fetch_branch(
            source,
            &self.working,
            request_id,
            branch,
            resume,
        )?)
    }

    pub fn push_child_branch_merge(
        &self,
        destination: &impl DurableControlEndpoint,
        publication: ChildMergePublication,
    ) -> Result<ChildMergeOutcome> {
        Ok(layerfs_sync::client::push_child_branch_merge(
            destination,
            publication,
        )?)
    }

    pub fn push_branch_rollback(
        &self,
        destination: &impl DurableControlEndpoint,
        publication: BranchRollbackPublication,
    ) -> Result<BranchRollbackOutcome> {
        Ok(layerfs_sync::client::push_branch_rollback(
            destination,
            publication,
        )?)
    }

    pub fn push_layer_stack_merge(
        &self,
        destination: &impl DurableControlEndpoint,
        candidate: LayerCandidate,
        expected: LayerStackHead,
    ) -> Result<LayerStackMergeOutcome> {
        Ok(layerfs_sync::client::push_layer_stack_merge(
            destination,
            candidate,
            expected,
        )?)
    }

    pub fn push_layer_stack_rollback(
        &self,
        destination: &impl DurableControlEndpoint,
        expected: LayerStackHead,
        target: LayerId,
    ) -> Result<LayerStackRollbackOutcome> {
        Ok(layerfs_sync::client::push_layer_stack_rollback(
            destination,
            expected,
            target,
        )?)
    }
}

pub struct DirectOperation<'a> {
    fs: &'a LayerFs,
    admission: BeginOperation,
    workspace: OperationWorkspace<DirectDriver>,
    candidate_root: ObjectId,
    terminal: bool,
}

pub struct DirectCommitReceipt {
    pub operation_id: OperationId,
    pub candidate_root: ObjectId,
    pub outcome: CommitResult,
    pub cleanup: layerfs_workspace::Result<EndOperationReceipt>,
    pub acknowledgement: Option<layerfs_working_store::Result<bool>>,
    pub timers: OperationCommitTimers,
}

pub struct MaterializationCommitReceipt {
    pub operation_id: OperationId,
    pub candidate_root: ObjectId,
    pub outcome: CommitResult,
    pub cleanup: layerfs_workspace::Result<EndOperationReceipt>,
    pub acknowledgement: Option<layerfs_working_store::Result<bool>>,
    pub counters: OperationCounters,
    pub timers: OperationCommitTimers,
}

pub struct ManagedMaterializationCommitReceipt {
    pub operation_id: OperationId,
    pub candidate_root: ObjectId,
    pub outcome: Option<CommitResult>,
    pub cleanup: layerfs_workspace::Result<EndOperationReceipt>,
    pub acknowledgement: Option<layerfs_working_store::Result<bool>>,
    pub counters: OperationCounters,
    pub refresh_counters: OperationCounters,
    pub timers: OperationCommitTimers,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OperationCommitTimers {
    pub quiescence_ns: u128,
    pub candidate_ns: u128,
    pub working_commit_ns: u128,
    pub working_recorded_ns: u128,
    pub cleanup_ns: u128,
    pub unattributed_ns: u128,
    pub complete_wall_ns: u128,
    pub equation_closed: bool,
}

impl DirectOperation<'_> {
    fn apply_candidate(
        &mut self,
        candidate: layerfs_working_store::WorkingTrustedCandidate,
        writer: layerfs_working_store::WorkingCandidateWrite<'_>,
    ) -> Result<logical::LogicalCounters> {
        let root = candidate.root();
        let counters = candidate.counters();
        writer.commit_trusted_operation_candidate(self.admission.operation_id, candidate)?;
        self.candidate_root = root;
        Ok(counters)
    }

    pub fn replace_file(
        &mut self,
        path: &str,
        input: impl Read,
    ) -> Result<logical::LogicalCounters> {
        let mut writer = self.fs.working.begin_candidate_write()?;
        let inode = writer.allocate_inode_id()?;
        let metadata = portable_metadata(&mut writer, InodeKind::RegularFile)?;
        let candidate = writer.trusted_replace_file(
            self.candidate_root,
            &CanonicalPath::new(path)?,
            input,
            (inode, metadata),
        )?;
        self.apply_candidate(candidate, writer)
    }

    pub fn replace_range(
        &mut self,
        path: &str,
        start: u64,
        delete_len: u64,
        replacement: impl Read,
    ) -> Result<logical::LogicalCounters> {
        let mut writer = self.fs.working.begin_candidate_write()?;
        let candidate = writer.trusted_replace_range(
            self.candidate_root,
            &CanonicalPath::new(path)?,
            start,
            delete_len,
            replacement,
        )?;
        self.apply_candidate(candidate, writer)
    }

    pub fn create_directory(&mut self, path: &str) -> Result<logical::LogicalCounters> {
        let mut writer = self.fs.working.begin_candidate_write()?;
        let inode = writer.allocate_inode_id()?;
        let metadata = portable_metadata(&mut writer, InodeKind::Directory)?;
        let candidate = writer.trusted_create_directory(
            self.candidate_root,
            &CanonicalPath::new(path)?,
            inode,
            metadata,
        )?;
        self.apply_candidate(candidate, writer)
    }

    pub fn create_symlink(
        &mut self,
        path: &str,
        target: &[u8],
    ) -> Result<logical::LogicalCounters> {
        let mut writer = self.fs.working.begin_candidate_write()?;
        let inode = writer.allocate_inode_id()?;
        let metadata = portable_metadata(&mut writer, InodeKind::Symlink)?;
        let candidate = writer.trusted_create_symlink(
            self.candidate_root,
            &CanonicalPath::new(path)?,
            inode,
            target.to_vec(),
            metadata,
        )?;
        self.apply_candidate(candidate, writer)
    }

    pub fn hard_link(&mut self, source: &str, target: &str) -> Result<logical::LogicalCounters> {
        let mut writer = self.fs.working.begin_candidate_write()?;
        let candidate = writer.trusted_hard_link(
            self.candidate_root,
            &CanonicalPath::new(source)?,
            &CanonicalPath::new(target)?,
        )?;
        self.apply_candidate(candidate, writer)
    }

    pub fn rename(&mut self, from: &str, to: &str) -> Result<logical::LogicalCounters> {
        let mut writer = self.fs.working.begin_candidate_write()?;
        let from = CanonicalPath::new(from)?;
        let to = CanonicalPath::new(to)?;
        let mut counters = logical::LogicalCounters::default();
        let (source_parent, _) =
            logical::resolve_parent(&writer, self.candidate_root, &from, &mut counters)?;
        let (target_parent, _) =
            logical::resolve_parent(&writer, self.candidate_root, &to, &mut counters)?;
        let candidate = writer.trusted_rename(
            self.candidate_root,
            &from,
            &to,
            source_parent.record.metadata_root,
            target_parent.record.metadata_root,
        )?;
        self.apply_candidate(candidate, writer)
    }

    pub fn remove(&mut self, path: &str) -> Result<logical::LogicalCounters> {
        let mut writer = self.fs.working.begin_candidate_write()?;
        let candidate =
            writer.trusted_remove_path(self.candidate_root, &CanonicalPath::new(path)?)?;
        self.apply_candidate(candidate, writer)
    }

    pub fn commit(mut self) -> Result<DirectCommitReceipt> {
        let complete = Instant::now();
        let freeze = self.workspace.freeze_observed(Duration::from_secs(30))?;
        let candidate_started = Instant::now();
        let finalized: FinalizedCandidate = self.workspace.finalize_candidate(
            self.admission.base.root(),
            self.candidate_root,
            Vec::new(),
        )?;
        let candidate_ns = freeze.driver_freeze_ns + candidate_started.elapsed().as_nanos();
        let working_commit = Instant::now();
        let result = self
            .fs
            .working
            .operation_commit(self.admission, finalized.into_working());
        let working_commit_ns = working_commit.elapsed().as_nanos();
        match result {
            Ok(outcome) => {
                self.terminal = true;
                let cleanup_started = Instant::now();
                let cleanup = self.workspace.cleanup();
                let cleanup_ns = cleanup_started.elapsed().as_nanos();
                let acknowledgement = if cleanup.is_ok() {
                    match outcome {
                        CommitResult::WorkingRecorded { record, .. } => {
                            Some(self.fs.working.acknowledge_operation(record))
                        }
                        CommitResult::Conflict { candidate, .. } => {
                            Some(self.fs.working.acknowledge_conflict(candidate))
                        }
                    }
                } else {
                    None
                };
                Ok(DirectCommitReceipt {
                    operation_id: self.admission.operation_id,
                    candidate_root: self.candidate_root,
                    outcome,
                    cleanup,
                    acknowledgement,
                    timers: commit_timers(
                        complete,
                        freeze.quiescence_ns,
                        candidate_ns,
                        working_commit_ns,
                        cleanup_ns,
                    ),
                })
            }
            Err(error) => {
                if self.workspace.discard().is_ok() {
                    self.terminal = self
                        .fs
                        .working
                        .discard_operation(self.admission.operation_id)
                        .is_ok();
                }
                Err(error.into())
            }
        }
    }

    pub fn discard(mut self) -> Result<()> {
        self.workspace.discard()?;
        self.fs
            .working
            .discard_operation(self.admission.operation_id)?;
        self.terminal = true;
        Ok(())
    }

    pub fn candidate_root(&self) -> ObjectId {
        self.candidate_root
    }

    pub fn operation_id(&self) -> OperationId {
        self.admission.operation_id
    }

    pub fn ticket(&self) -> WorkspaceTicket {
        WorkspaceTicket::from_admission(&self.admission, Presentation::Direct)
    }
}

impl Drop for DirectOperation<'_> {
    fn drop(&mut self) {
        if !self.terminal
            && self.workspace.discard().is_ok()
            && self
                .fs
                .working
                .discard_operation(self.admission.operation_id)
                .is_ok()
        {
            self.terminal = true;
        }
    }
}

pub struct MaterializedOperation<'a> {
    fs: &'a LayerFs,
    admission: BeginOperation,
    workspace: OperationWorkspace<layerfs_materialization::MaterializationDriver<'a>>,
    terminal: bool,
}

impl MaterializedOperation<'_> {
    pub fn path(&self) -> &Path {
        self.workspace
            .paths()
            .expect("materialization always has custody")
            .view()
    }

    pub fn leases(&self) -> &layerfs_workspace::RuntimeLeases {
        self.workspace.leases()
    }

    pub fn managed_replace_range(
        &mut self,
        path: &str,
        start: u64,
        delete_len: u64,
        replacement: &[u8],
    ) -> Result<OperationCounters> {
        Ok(self.workspace.driver_mut().managed_replace_range(
            &CanonicalPath::new(path)?,
            start,
            delete_len,
            replacement,
        )?)
    }

    pub fn managed_rename(&mut self, from: &str, to: &str) -> Result<OperationCounters> {
        Ok(self
            .workspace
            .driver_mut()
            .managed_rename(&CanonicalPath::new(from)?, &CanonicalPath::new(to)?)?)
    }

    pub fn commit(mut self) -> Result<MaterializationCommitReceipt> {
        let complete = Instant::now();
        let freeze = self.workspace.freeze_observed(Duration::from_secs(30))?;
        let candidate_started = Instant::now();
        let candidate = self
            .workspace
            .driver()
            .candidate_root()
            .ok_or(Error::Workspace(
                layerfs_workspace::WorkspaceError::InvalidState,
            ))?;
        let finalized =
            self.workspace
                .finalize_candidate(self.admission.base.root(), candidate, Vec::new())?;
        let candidate_ns = freeze.driver_freeze_ns + candidate_started.elapsed().as_nanos();
        let working_commit = Instant::now();
        let result = self
            .fs
            .working
            .operation_commit(self.admission, finalized.into_working());
        let working_commit_ns = working_commit.elapsed().as_nanos();
        match result {
            Ok(outcome) => {
                let counters = self.workspace.driver().counters();
                self.terminal = true;
                let cleanup_started = Instant::now();
                let cleanup = self.workspace.cleanup();
                let cleanup_ns = cleanup_started.elapsed().as_nanos();
                let acknowledgement = if cleanup.is_ok() {
                    match outcome {
                        CommitResult::WorkingRecorded { record, .. } => {
                            Some(self.fs.working.acknowledge_operation(record))
                        }
                        CommitResult::Conflict { candidate, .. } => {
                            Some(self.fs.working.acknowledge_conflict(candidate))
                        }
                    }
                } else {
                    None
                };
                Ok(MaterializationCommitReceipt {
                    operation_id: self.admission.operation_id,
                    candidate_root: candidate,
                    outcome,
                    cleanup,
                    acknowledgement,
                    counters,
                    timers: commit_timers(
                        complete,
                        freeze.quiescence_ns,
                        candidate_ns,
                        working_commit_ns,
                        cleanup_ns,
                    ),
                })
            }
            Err(error) => {
                if self.workspace.discard().is_ok() {
                    self.terminal = self
                        .fs
                        .working
                        .discard_operation(self.admission.operation_id)
                        .is_ok();
                }
                Err(error.into())
            }
        }
    }

    pub fn discard(mut self) -> Result<()> {
        self.workspace.discard()?;
        self.fs
            .working
            .discard_operation(self.admission.operation_id)?;
        self.terminal = true;
        Ok(())
    }

    pub fn operation_id(&self) -> OperationId {
        self.admission.operation_id
    }
}

impl Drop for MaterializedOperation<'_> {
    fn drop(&mut self) {
        if !self.terminal
            && self.workspace.discard().is_ok()
            && self
                .fs
                .working
                .discard_operation(self.admission.operation_id)
                .is_ok()
        {
            self.terminal = true;
        }
    }
}

pub struct ManagedMaterializedOperation<'a> {
    fs: &'a LayerFs,
    admission: BeginOperation,
    workspace: OperationWorkspace<layerfs_materialization::MaterializationDriver<'a>>,
    terminal: bool,
}

impl ManagedMaterializedOperation<'_> {
    pub fn managed_replace_range(
        &mut self,
        path: &str,
        start: u64,
        delete_len: u64,
        replacement: &[u8],
    ) -> Result<OperationCounters> {
        Ok(self.workspace.driver_mut().managed_replace_range(
            &CanonicalPath::new(path)?,
            start,
            delete_len,
            replacement,
        )?)
    }

    pub fn refresh_to(&mut self, target: VersionRef) -> Result<OperationCounters> {
        self.fs.working.validate_version_ref(target)?;
        Ok(self.workspace.driver_mut().refresh_to(target)?)
    }

    pub fn read(&self, path: &str, start: u64, length: usize) -> Result<Vec<u8>> {
        Ok(self
            .workspace
            .driver()
            .managed_read(&CanonicalPath::new(path)?, start, length)?)
    }

    pub fn commit(mut self) -> Result<ManagedMaterializationCommitReceipt> {
        let complete = Instant::now();
        let freeze = self.workspace.freeze_observed(Duration::from_secs(30))?;
        let candidate_started = Instant::now();
        let candidate = self
            .workspace
            .driver()
            .candidate_root()
            .ok_or(Error::Workspace(
                layerfs_workspace::WorkspaceError::InvalidState,
            ))?;
        let counters = self.workspace.driver().counters();
        let refresh_counters = self.workspace.driver().refresh_counters();
        if candidate == self.admission.base.root() {
            let candidate_ns = freeze.driver_freeze_ns + candidate_started.elapsed().as_nanos();
            let cleanup_started = Instant::now();
            let cleanup = self.workspace.cleanup();
            let cleanup_ns = cleanup_started.elapsed().as_nanos();
            cleanup.as_ref().map_err(|error| {
                Error::Workspace(match error {
                    layerfs_workspace::WorkspaceError::Busy => {
                        layerfs_workspace::WorkspaceError::Busy
                    }
                    _ => layerfs_workspace::WorkspaceError::InvalidState,
                })
            })?;
            self.fs
                .working
                .discard_operation(self.admission.operation_id)?;
            self.terminal = true;
            return Ok(ManagedMaterializationCommitReceipt {
                operation_id: self.admission.operation_id,
                candidate_root: candidate,
                outcome: None,
                cleanup,
                acknowledgement: None,
                counters,
                refresh_counters,
                timers: commit_timers(complete, freeze.quiescence_ns, candidate_ns, 0, cleanup_ns),
            });
        }
        let finalized =
            self.workspace
                .finalize_candidate(self.admission.base.root(), candidate, Vec::new())?;
        let candidate_ns = freeze.driver_freeze_ns + candidate_started.elapsed().as_nanos();
        let working_commit = Instant::now();
        let result = self
            .fs
            .working
            .operation_commit(self.admission, finalized.into_working());
        let working_commit_ns = working_commit.elapsed().as_nanos();
        match result {
            Ok(outcome) => {
                self.terminal = true;
                let cleanup_started = Instant::now();
                let cleanup = self.workspace.cleanup();
                let cleanup_ns = cleanup_started.elapsed().as_nanos();
                let acknowledgement = if cleanup.is_ok() {
                    match outcome {
                        CommitResult::WorkingRecorded { record, .. } => {
                            Some(self.fs.working.acknowledge_operation(record))
                        }
                        CommitResult::Conflict { candidate, .. } => {
                            Some(self.fs.working.acknowledge_conflict(candidate))
                        }
                    }
                } else {
                    None
                };
                Ok(ManagedMaterializationCommitReceipt {
                    operation_id: self.admission.operation_id,
                    candidate_root: candidate,
                    outcome: Some(outcome),
                    cleanup,
                    acknowledgement,
                    counters,
                    refresh_counters,
                    timers: commit_timers(
                        complete,
                        freeze.quiescence_ns,
                        candidate_ns,
                        working_commit_ns,
                        cleanup_ns,
                    ),
                })
            }
            Err(error) => {
                if self.workspace.discard().is_ok() {
                    self.terminal = self
                        .fs
                        .working
                        .discard_operation(self.admission.operation_id)
                        .is_ok();
                }
                Err(error.into())
            }
        }
    }

    pub fn discard(mut self) -> Result<()> {
        self.workspace.discard()?;
        self.fs
            .working
            .discard_operation(self.admission.operation_id)?;
        self.terminal = true;
        Ok(())
    }
}

impl Drop for ManagedMaterializedOperation<'_> {
    fn drop(&mut self) {
        if !self.terminal
            && self.workspace.discard().is_ok()
            && self
                .fs
                .working
                .discard_operation(self.admission.operation_id)
                .is_ok()
        {
            self.terminal = true;
        }
    }
}

fn commit_timers(
    complete: Instant,
    quiescence_ns: u128,
    candidate_ns: u128,
    working_commit_ns: u128,
    cleanup_ns: u128,
) -> OperationCommitTimers {
    let working_recorded_ns = quiescence_ns + candidate_ns + working_commit_ns;
    let attributed_ns = working_recorded_ns + cleanup_ns;
    let complete_wall_ns = complete.elapsed().as_nanos();
    let unattributed_ns = complete_wall_ns.saturating_sub(attributed_ns);
    OperationCommitTimers {
        quiescence_ns,
        candidate_ns,
        working_commit_ns,
        working_recorded_ns,
        cleanup_ns,
        unattributed_ns,
        complete_wall_ns,
        equation_closed: working_recorded_ns == quiescence_ns + candidate_ns + working_commit_ns
            && complete_wall_ns == attributed_ns + unattributed_ns,
    }
}

fn portable_metadata(
    writer: &mut layerfs_working_store::WorkingCandidateWrite<'_>,
    kind: InodeKind,
) -> layerfs_core::CoreResult<ObjectId> {
    let mode = match kind {
        InodeKind::Directory => 0o755_u32,
        InodeKind::RegularFile => 0o644_u32,
        InodeKind::Symlink => 0o777_u32,
    };
    let (mode, _) = build(writer, mode.to_be_bytes().as_slice())?;
    let mut mtime = Vec::with_capacity(12);
    mtime.extend_from_slice(&0_i64.to_be_bytes());
    mtime.extend_from_slice(&0_u32.to_be_bytes());
    let (mtime, _) = build(writer, mtime.as_slice())?;
    build_metadata_tree(
        writer,
        &[
            MetadataEntryV1 {
                key: MetadataKey::new("portable".into(), b"mode".to_vec())?,
                value_file_root: mode.0,
            },
            MetadataEntryV1 {
                key: MetadataKey::new("portable".into(), b"mtime".to_vec())?,
                value_file_root: mtime.0,
            },
        ],
    )
}
