use crate::{
    BranchHead, BranchId, EngineCounters, IntegrityMode, ManagedMaterializedOperation,
    MaterializedOperation, OperationId, OperationRecordRef, RecoverableOperation, Result,
    VersionRef,
};
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
use layerfs_working_store::{BeginOperation, WorkingStore};
use layerfs_workspace::{OperationWorkspace, Presentation};
use std::io::Write;
use std::ops::Range;
use std::path::Path;

pub struct LayerFs {
    pub(crate) working: WorkingStore,
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

    pub fn create_top_level_branch(
        &self,
        id: BranchId,
        name: Option<&str>,
        origin: crate::LayerStackHead,
    ) -> Result<BranchHead> {
        Ok(self.working.create_top_level_branch(id, name, origin)?)
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

    pub fn read_range(
        &self,
        version: crate::VersionRef,
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
        version: crate::VersionRef,
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
        version: crate::VersionRef,
        path: &str,
    ) -> Result<(crate::Stat, logical::LogicalCounters)> {
        self.working.validate_version_ref(version)?;
        Ok(logical::stat(
            &self.working,
            version.root(),
            &CanonicalPath::new(path)?,
        )?)
    }

    pub fn list(
        &self,
        version: crate::VersionRef,
        path: &str,
        after: Option<&[u8]>,
        max_entries: usize,
        max_bytes: usize,
    ) -> Result<(crate::ListPage, logical::LogicalCounters)> {
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
        version: crate::VersionRef,
        path: &str,
    ) -> Result<(Vec<u8>, logical::LogicalCounters)> {
        self.working.validate_version_ref(version)?;
        Ok(logical::readlink(
            &self.working,
            version.root(),
            &CanonicalPath::new(path)?,
        )?)
    }

    pub fn begin_materialization(
        &self,
        expected: crate::BranchHead,
    ) -> Result<MaterializedOperation<'_>> {
        let (admission, workspace) = self.start_materialization(expected, false)?;
        Ok(MaterializedOperation::new(self, admission, workspace))
    }

    pub fn begin_managed_materialization(
        &self,
        expected: crate::BranchHead,
    ) -> Result<ManagedMaterializedOperation<'_>> {
        let (admission, workspace) = self.start_materialization(expected, true)?;
        Ok(ManagedMaterializedOperation::new(
            self, admission, workspace,
        ))
    }

    fn start_materialization(
        &self,
        expected: crate::BranchHead,
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
                let _ = paths.remove_owned();
                let _ = self.working.discard_operation(admission.operation_id);
                return Err(error.into());
            }
        };
        let (workspace, _) = OperationWorkspace::start(ticket, driver, Some(paths))?;
        Ok((admission, workspace))
    }
}

pub(crate) fn portable_metadata(
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
