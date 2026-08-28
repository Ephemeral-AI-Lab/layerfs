use layerfs_storage::integrity::IntegrityMode;
use layerfs_storage::scratch::DiskTable;
use layerfs_storage::{EngineCounters, EngineError, Storage};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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

pub struct WorkingStore {
    pub(crate) root: PathBuf,
    pub(crate) storage: Storage,
}

impl WorkingStore {
    pub fn open(root: &Path, mode: IntegrityMode) -> Result<Self> {
        fs::create_dir_all(root)?;
        set_private(root)?;
        if fs::symlink_metadata(root)?.file_type().is_symlink() {
            return Err(WorkingError::InvalidReceipt);
        }
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
