//! Central Verified authority over one physically distinct Durable StorageId.

#![forbid(unsafe_code)]

pub use layerfs_storage::product::{
    derive_id, BranchHead, BranchId, BranchPushBundle, BranchPushOutcome, BranchPushRequest,
    BranchRollbackOutcome, BranchRollbackPublication, ChildMergeOutcome, ChildMergePublication,
    LayerCandidate, LayerId, LayerStackHead, LayerStackId, LayerStackMergeOutcome,
    LayerStackRollbackOutcome, OperationVersionId, RequestId,
};
use layerfs_storage::{EngineError, Storage};
use std::fmt;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const COMPONENT: &str = "layerfs-durable-store";
const ABANDONED_SYNC_SECONDS: u64 = 24 * 60 * 60;
const STARTUP_SYNC_REAP_LIMIT: usize = 64;

#[derive(Debug)]
pub enum DurableError {
    Core(layerfs_core::CoreError),
    Storage(EngineError),
    Io(std::io::Error),
    InvalidPath,
}

impl fmt::Display for DurableError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for DurableError {}

impl From<EngineError> for DurableError {
    fn from(value: EngineError) -> Self {
        Self::Storage(value)
    }
}

impl From<layerfs_core::CoreError> for DurableError {
    fn from(value: layerfs_core::CoreError) -> Self {
        Self::Core(value)
    }
}

impl From<std::io::Error> for DurableError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub type Result<T> = std::result::Result<T, DurableError>;

pub struct DurableStore {
    root: PathBuf,
    storage: Storage,
}

impl DurableStore {
    pub fn open(root: &Path) -> Result<Self> {
        prepare_store_root(root)?;
        let root = fs::canonicalize(root)?;
        let generation_root = root.join("durable.sqlite.generations");
        let storage = layerfs_storage::generation::open_or_create_with_legacy(
            &generation_root,
            &root.join("durable.sqlite"),
            &layerfs_storage::generation::NativeGenerationDriver,
            layerfs_storage::integrity::IntegrityMode::Verified,
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

    pub fn bootstrap_layer_stack(
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

    pub fn layer_stack_head(&self, id: LayerStackId) -> Result<Option<LayerStackHead>> {
        Ok(self.storage.product_layer_stack_head(id)?)
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

    pub fn branch_head(&self, id: BranchId) -> Result<Option<BranchHead>> {
        Ok(self.storage.product_branch_head(id)?)
    }

    pub fn accept_layer_stack_merge(
        &self,
        candidate: LayerCandidate,
        expected: LayerStackHead,
    ) -> Result<LayerStackMergeOutcome> {
        let ancestry = self
            .storage
            .product_branch_ancestry(candidate.source.branch_id)?
            .ok_or(DurableError::Storage(EngineError::InvalidRecord(
                "Layer candidate source Branch",
            )))?;
        if ancestry.origin_layer_stack_id != expected.layer_stack_id
            || candidate.layer_stack_id != expected.layer_stack_id
            || candidate.parent_layer_id != expected.layer_id
            || candidate.layer_id
                != LayerId::from_bytes(derive_id(
                    b"candidate-layer",
                    &[
                        expected.layer_stack_id.as_bytes(),
                        candidate.request_id.as_bytes(),
                        candidate.root.as_bytes(),
                    ],
                ))
        {
            return Err(DurableError::Storage(EngineError::InvalidRecord(
                "Layer candidate identity",
            )));
        }
        let origin = self
            .storage
            .product_layer_root(ancestry.origin_layer_stack_id, ancestry.origin_layer_id)?
            .ok_or(DurableError::Storage(EngineError::InvalidRecord(
                "Layer candidate origin",
            )))?;
        self.verify_merge_root(origin, candidate.source.root, expected.root, candidate.root)?;
        self.storage
            .product_import_layer_candidate(candidate, expected)?;
        let request_id = RequestId::from_bytes(derive_id(
            b"durable-layer-stack-merge",
            &[
                self.storage_id().as_slice(),
                candidate.request_id.as_bytes(),
                candidate.layer_id.as_bytes(),
                expected.layer_id.as_bytes(),
            ],
        ));
        Ok(self
            .storage
            .product_accept_layer_stack_merge(candidate, expected, request_id)?)
    }

    pub fn layer_stack_rollback(
        &self,
        expected: LayerStackHead,
        target: LayerId,
    ) -> Result<LayerStackRollbackOutcome> {
        let request_id = RequestId::from_bytes(derive_id(
            b"durable-layer-stack-rollback",
            &[
                self.storage_id().as_slice(),
                expected.layer_stack_id.as_bytes(),
                &expected.generation.to_be_bytes(),
                target.as_bytes(),
            ],
        ));
        Ok(self
            .storage
            .product_layer_stack_rollback(expected, target, request_id)?)
    }

    pub fn drop_branch(&self, branch_id: BranchId) -> Result<()> {
        Ok(self.storage.product_drop_branch(branch_id)?)
    }

    pub fn accept_child_branch_merge(
        &self,
        publication: ChildMergePublication,
    ) -> Result<ChildMergeOutcome> {
        let expected = publication.accepted_parent;
        let candidate = &publication.candidate;
        let ancestry = self
            .storage
            .product_branch_ancestry(candidate.source.branch_id)?
            .ok_or(DurableError::Storage(EngineError::InvalidRecord(
                "ChildBranchMerge source",
            )))?;
        if ancestry.immediate_parent_branch_id != Some(candidate.expected_parent.branch_id) {
            return Err(DurableError::Storage(EngineError::InvalidRecord(
                "ChildBranchMerge parent",
            )));
        }
        let version = OperationVersionId::from_bytes(derive_id(
            b"child-merge-operation-version",
            &[
                candidate.expected_parent.branch_id.as_bytes(),
                candidate.request_id.as_bytes(),
                candidate.result_root.as_bytes(),
            ],
        ));
        if expected
            != (BranchHead {
                branch_id: candidate.expected_parent.branch_id,
                generation: candidate
                    .expected_parent
                    .generation
                    .checked_add(1)
                    .ok_or(DurableError::Storage(EngineError::CounterOverflow))?,
                operation_version_id: Some(version),
                root: candidate.result_root,
            })
        {
            return Err(DurableError::Storage(EngineError::InvalidRecord(
                "ChildBranchMerge accepted head",
            )));
        }
        self.verify_merge_root(
            ancestry.fork_root,
            candidate.source.root,
            candidate.expected_parent.root,
            candidate.result_root,
        )?;
        let outcome = self
            .storage
            .product_child_branch_merge(publication.candidate)?;
        if !matches!(
            outcome,
            ChildMergeOutcome::WorkingRecorded { parent_head, .. }
                if parent_head == expected
        ) {
            return Err(DurableError::Storage(EngineError::InvalidRecord(
                "ChildBranchMerge publication",
            )));
        }
        Ok(outcome)
    }

    pub fn accept_branch_rollback(
        &self,
        publication: BranchRollbackPublication,
    ) -> Result<BranchRollbackOutcome> {
        let outcome = self.storage.product_branch_rollback(
            publication.expected,
            publication.target,
            publication.request_id,
        )?;
        if !matches!(
            outcome,
            BranchRollbackOutcome::WorkingRecorded { head, .. }
                if head == publication.accepted
        ) {
            return Err(DurableError::Storage(EngineError::InvalidRecord(
                "BranchRollback publication",
            )));
        }
        Ok(outcome)
    }

    pub fn compact(self) -> Result<Self> {
        let Self { root, storage } = self;
        let generation_root = root.join("durable.sqlite.generations");
        let storage = layerfs_storage::generation::compact(
            storage,
            &generation_root,
            &layerfs_storage::generation::NativeGenerationDriver,
        )?;
        Ok(Self { root, storage })
    }

    pub fn backup(&self, destination: &Path) -> Result<()> {
        self.storage.backup_to(destination)?;
        Ok(())
    }

    pub fn restore(backup: &Path, root: &Path) -> Result<Self> {
        let backup_metadata = fs::symlink_metadata(backup)?;
        if !backup_metadata.file_type().is_file() {
            return Err(DurableError::InvalidPath);
        }
        let backup = fs::canonicalize(backup)?;
        let admitted = Storage::open(&backup)?;
        let expected_store_id = admitted.store_id()?;
        drop(admitted);
        if fs::symlink_metadata(root).is_ok() {
            return Err(DurableError::InvalidPath);
        }
        let parent = root.parent().ok_or(DurableError::InvalidPath)?;
        fs::create_dir_all(parent)?;
        let parent = fs::canonicalize(parent)?;
        let name = root.file_name().ok_or(DurableError::InvalidPath)?;
        let destination = parent.join(name);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| std::io::Error::other(error.to_string()))?
            .as_nanos();
        let staging = parent.join(format!(".layerfs-restore-{}-{nonce}", std::process::id()));
        fs::create_dir(&staging)?;
        set_private(&staging)?;
        let staging_identity = directory_identity(&staging)?;
        let result = (|| {
            let legacy = staging.join("durable.sqlite");
            let mut source = fs::File::open(&backup)?;
            let mut target = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&legacy)?;
            let mut buffer = vec![0_u8; 1024 * 1024];
            loop {
                let count = source.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                target.write_all(&buffer[..count])?;
            }
            target.sync_all()?;
            drop(target);
            fs::File::open(&staging)?.sync_all()?;
            let admitted = Self::open(&staging)?;
            if admitted.storage_id() != expected_store_id {
                return Err(DurableError::Storage(EngineError::InvalidRecord(
                    "restore StoreId",
                )));
            }
            drop(admitted);
            if directory_identity(&staging)? != staging_identity {
                return Err(DurableError::InvalidPath);
            }
            install_restore_no_replace(&staging, &destination)?;
            fs::File::open(&parent)?.sync_all()?;
            let restored = Self::open(&destination)?;
            if restored.storage_id() != expected_store_id {
                return Err(DurableError::Storage(EngineError::InvalidRecord(
                    "installed restore StoreId",
                )));
            }
            Ok(restored)
        })();
        if result.is_err()
            && staging.exists()
            && directory_identity(&staging).ok().as_ref() == Some(&staging_identity)
        {
            let _ = fs::remove_dir_all(&staging);
            let _ = fs::File::open(&parent).and_then(|directory| directory.sync_all());
        }
        result
    }

    pub fn database_path(&self) -> PathBuf {
        self.storage.path().to_path_buf()
    }

    pub fn counters(&self) -> Result<layerfs_storage::EngineCounters> {
        Ok(self.storage.counters()?)
    }

    pub fn reset_counters(&self) -> Result<()> {
        Ok(self.storage.reset_counters()?)
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

    pub fn stage_branch_push_page(
        &self,
        transfer_id: RequestId,
        page_sequence: u64,
        data_request_id: RequestId,
        bundle: &BranchPushBundle,
        counters: layerfs_storage::product::SyncTransferCounters,
    ) -> Result<()> {
        Ok(self.storage.product_stage_branch_push_page(
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
            .product_commit_staged_branch_push(request, branch_id)?)
    }

    pub fn reconcile_branch_push(
        &self,
        request_id: RequestId,
        expected: Option<BranchHead>,
        accepted: BranchHead,
    ) -> Result<BranchPushOutcome> {
        Ok(self
            .storage
            .product_reconcile_branch_push(request_id, expected, accepted)?)
    }

    pub fn export_branch_fetch(
        &self,
        branch_id: BranchId,
        base: Option<BranchHead>,
        origin_stack_base: Option<LayerStackHead>,
    ) -> Result<BranchPushBundle> {
        Ok(self
            .storage
            .product_export_branch_fetch_page(branch_id, base, origin_stack_base)?)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn branch_fetch_object_page(
        &self,
        branch_id: BranchId,
        base: Option<BranchHead>,
        origin_stack_base: Option<LayerStackHead>,
        expected_head: BranchHead,
        expected_stack_head: LayerStackHead,
        after: Option<layerfs_core::ObjectId>,
        limit: usize,
    ) -> Result<Vec<layerfs_core::ObjectId>> {
        Ok(self.storage.product_branch_fetch_object_page(
            branch_id,
            base,
            origin_stack_base,
            expected_head,
            expected_stack_head,
            after,
            limit,
        )?)
    }

    fn verify_merge_root(
        &self,
        base: layerfs_core::ObjectId,
        source: layerfs_core::ObjectId,
        destination: layerfs_core::ObjectId,
        claimed: layerfs_core::ObjectId,
    ) -> Result<()> {
        let mut writer = self.storage.begin_candidate_write()?;
        let recomputed =
            layerfs_core::logical::merge_roots(&mut writer, base, source, destination)?.map_err(
                |_| DurableError::Storage(EngineError::InvalidRecord("Durable merge conflict")),
            )?;
        if recomputed.root() != claimed {
            return Err(DurableError::Storage(EngineError::InvalidRecord(
                "Durable merge result",
            )));
        }
        writer.commit_candidate(recomputed.root())?;
        Ok(())
    }
}

fn prepare_store_root(root: &Path) -> Result<()> {
    match fs::symlink_metadata(root) {
        Ok(metadata) if !metadata.file_type().is_dir() => return Err(DurableError::InvalidPath),
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
        return Err(DurableError::InvalidPath);
    }
    set_private(root)?;
    Ok(())
}

#[cfg(any(target_os = "android", target_os = "linux", target_vendor = "apple"))]
fn install_restore_no_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        source,
        rustix::fs::CWD,
        destination,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(std::io::Error::from)
}

#[cfg(not(any(target_os = "android", target_os = "linux", target_vendor = "apple")))]
fn install_restore_no_replace(_source: &Path, _destination: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "atomic no-replace restore installation is unavailable",
    ))
}

#[cfg(all(
    test,
    any(target_os = "android", target_os = "linux", target_vendor = "apple")
))]
mod tests {
    use super::install_restore_no_replace;
    use std::fs;

    #[test]
    fn restore_install_refuses_a_destination_created_at_install_time() {
        let base =
            std::env::temp_dir().join(format!("layerfs-restore-noreplace-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir(&base).unwrap();
        let source = base.join("source");
        let destination = base.join("destination");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("source"), b"source").unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("incumbent"), b"incumbent").unwrap();

        assert!(install_restore_no_replace(&source, &destination).is_err());
        assert_eq!(
            fs::read(destination.join("incumbent")).unwrap(),
            b"incumbent"
        );
        assert_eq!(fs::read(source.join("source")).unwrap(), b"source");

        fs::remove_dir_all(base).unwrap();
    }
}

#[cfg(unix)]
fn directory_identity(path: &Path) -> std::io::Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() {
        return Err(std::io::Error::other("restore path is not a directory"));
    }
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(not(unix))]
fn directory_identity(path: &Path) -> std::io::Result<(u64, u64)> {
    let metadata = fs::metadata(path)?;
    Ok((metadata.len(), 0))
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
