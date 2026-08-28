use crate::recovery;
use layerfs_storage::{EngineError, FullStorage};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

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
    pub(crate) root: PathBuf,
    pub(crate) storage: FullStorage,
}

impl DurableStore {
    pub fn open(root: &Path) -> Result<Self> {
        fs::create_dir_all(root)?;
        set_private(root)?;
        if fs::symlink_metadata(root)?.file_type().is_symlink() {
            return Err(DurableError::InvalidPath);
        }
        let root = fs::canonicalize(root)?;
        let generation_root = root.join("durable.sqlite.generations");
        let driver = layerfs_storage::generation::NativeGenerationDriver;
        if !generation_root.join("CURRENT").exists() {
            drop(layerfs_storage::generation::open_or_create_with_legacy(
                &generation_root,
                &root.join("durable.sqlite"),
                &driver,
                layerfs_storage::integrity::IntegrityMode::Verified,
            )?);
        }
        let storage = match layerfs_storage::generation::open_current_full_durable(&generation_root)
        {
            Ok(storage) => storage,
            Err(EngineError::ProfileMismatch) => {
                layerfs_storage::migration::migrate_selected_legacy_durable_generation(
                    &generation_root,
                    &driver,
                )?
            }
            Err(error) => return Err(error.into()),
        };
        recovery::reap_startup_abandoned_sync(&storage)?;
        Ok(Self { root, storage })
    }

    pub fn storage_id(&self) -> [u8; 32] {
        self.storage.storage_id()
    }

    pub fn database_path(&self) -> PathBuf {
        self.storage.path().to_path_buf()
    }

    pub fn counters(&self) -> Result<layerfs_storage::FullStorageCounters> {
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
        owner_request_id: crate::RequestId,
        request_id: crate::RequestId,
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
}

#[cfg(unix)]
pub(crate) fn set_private(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
pub(crate) fn set_private(_path: &Path) -> std::io::Result<()> {
    Ok(())
}
