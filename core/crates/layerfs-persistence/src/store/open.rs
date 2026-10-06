//! Explicit creation/open, binding validation and read-only authority.
use crate::{
    backend::Session, AcquisitionProvider, BackendSelection, Handles, HistoryProvider,
    PersistenceConfig, StorageProvider,
};
use layerfs_history::{CatalogId, HistoryCatalogConfig};
use layerfs_storage::{port::PersistenceError, StoragePolicy};
use std::{
    fs::OpenOptions,
    path::{Path, PathBuf},
    sync::Arc,
};
impl Handles {
    /// Creates a fresh combined Store, refusing existing files and unsupported engines.
    pub fn create(
        config: PersistenceConfig,
        policy: StoragePolicy,
        history: &HistoryCatalogConfig,
    ) -> Result<Self, PersistenceError> {
        if config.backend != BackendSelection::Sqlite || !cfg!(target_os = "macos") {
            return Err(PersistenceError::BackendUnavailable);
        }
        let policy = policy
            .validated()
            .map_err(|_| PersistenceError::Malformed)?;
        history.check().map_err(|_| PersistenceError::Malformed)?;
        let catalog_id =
            CatalogId::derive(&history.binding_key).map_err(|_| PersistenceError::Malformed)?;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&config.path)
            .map_err(|e| PersistenceError::Refused {
                status: e.kind().to_string(),
            })?;
        let session = Arc::new(Session::connect(
            &config.path,
            true,
            true,
            config.sqlite_profile,
            config.sqlite_pack_layout,
            config.sqlite_acquisition,
        )?);
        session.initialize(policy, history, catalog_id)?;
        let directory = config.path.parent().map(Path::to_path_buf);
        Self::validated(session, directory, &history.binding_key, history.cursor_key)
    }
    /// Opens validated authority with the explicitly selected profile; acknowledged reservations persist.
    pub fn open_writable(
        config: PersistenceConfig,
        binding: &[u8],
        cursor_key: [u8; 32],
    ) -> Result<Self, PersistenceError> {
        Self::open(config, binding, cursor_key, true)
    }
    /// Opens one read-only session. Mutation is refused before BEGIN or SQL.
    pub fn open_read_only(
        config: PersistenceConfig,
        binding: &[u8],
        cursor_key: [u8; 32],
    ) -> Result<Self, PersistenceError> {
        Self::open(config, binding, cursor_key, false)
    }
    fn open(
        config: PersistenceConfig,
        binding: &[u8],
        cursor_key: [u8; 32],
        writable: bool,
    ) -> Result<Self, PersistenceError> {
        if config.backend != BackendSelection::Sqlite || !cfg!(target_os = "macos") {
            return Err(PersistenceError::BackendUnavailable);
        }
        let directory = config.path.parent().map(Path::to_path_buf);
        Self::validated(
            Arc::new(Session::connect(
                &config.path,
                writable,
                false,
                config.sqlite_profile,
                config.sqlite_pack_layout,
                config.sqlite_acquisition,
            )?),
            directory,
            binding,
            cursor_key,
        )
    }
    fn validated(
        session: Arc<Session>,
        directory: Option<PathBuf>,
        binding: &[u8],
        cursor_key: [u8; 32],
    ) -> Result<Self, PersistenceError> {
        if cursor_key == [0; 32] {
            return Err(PersistenceError::Malformed);
        }
        let catalog_id = CatalogId::derive(binding).map_err(|_| PersistenceError::Malformed)?;
        let incarnation = session.validate(catalog_id, binding)?;
        let storage = Arc::new(StorageProvider {
            session: session.clone(),
        });
        Ok(Self {
            storage,
            acquisition: AcquisitionProvider::new(session.clone(), directory),
            history: HistoryProvider {
                session,
                catalog_id,
                incarnation,
                cursor_key,
            },
        })
    }
}
