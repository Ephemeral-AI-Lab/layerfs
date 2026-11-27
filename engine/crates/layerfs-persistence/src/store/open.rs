//! Explicit creation/open, binding validation and read-only authority.
use crate::{
    backend::{records::BackendError, sqlite::lifecycle::identity::FileIdentity, Session},
    AcquisitionProvider, Committer, HistoryProvider, MergeScratch, PersistenceConfig,
    SqliteHandles, StorageProvider,
};
use layerfs_history::{CatalogId, HistoryCatalogConfig};
use layerfs_storage::{port::PersistenceError, StoragePolicy};
use std::{
    fs::OpenOptions,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};
impl SqliteHandles {
    /// Creates a fresh combined Store, refusing existing files and unsupported engines.
    pub fn create(
        config: PersistenceConfig,
        policy: StoragePolicy,
        history: &HistoryCatalogConfig,
    ) -> Result<Self, PersistenceError> {
        if !cfg!(any(target_os = "macos", target_os = "linux")) {
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
        Ok(Self::validated(
            session,
            directory,
            &history.binding_key,
            history.cursor_key,
        )?)
    }
    /// Opens validated authority with the explicitly selected profile; acknowledged reservations persist.
    pub fn open_writable(
        config: PersistenceConfig,
        binding: &[u8],
        cursor_key: [u8; 32],
    ) -> Result<Self, PersistenceError> {
        if !cfg!(any(target_os = "macos", target_os = "linux")) {
            return Err(PersistenceError::BackendUnavailable);
        }
        Ok(Self::open(config, binding, cursor_key, true, None)?)
    }
    /// Opens one read-only session. Mutation is refused before BEGIN or SQL.
    pub fn open_read_only(
        config: PersistenceConfig,
        binding: &[u8],
        cursor_key: [u8; 32],
    ) -> Result<Self, PersistenceError> {
        if !cfg!(any(target_os = "macos", target_os = "linux")) {
            return Err(PersistenceError::BackendUnavailable);
        }
        Ok(Self::open(config, binding, cursor_key, false, None)?)
    }
    /// Opens for diagnosis without changing journal/checkpoint settings or
    /// migrating the source. Uses the Normal profile, read-only.
    pub fn open_diagnostic(
        path: &std::path::Path,
        binding: &[u8],
        cursor_key: [u8; 32],
    ) -> Result<Self, PersistenceError> {
        let session = Arc::new(Session::connect_diagnostic(path)?);
        Ok(Self::validated(
            session,
            path.parent().map(std::path::Path::to_path_buf),
            binding,
            cursor_key,
        )?)
    }
    /// Whether the sqlite Store at `path` records the identity `binding`,
    /// read without validating its schema (diagnosis only).
    pub fn diagnostic_identity(
        path: &std::path::Path,
        binding: &[u8],
    ) -> Result<bool, PersistenceError> {
        let session = Session::connect_diagnostic(path)?;
        let catalog_id = CatalogId::derive(binding).map_err(|_| PersistenceError::Malformed)?;
        Ok(session.identity_matches(catalog_id, binding)?)
    }
    fn open(
        config: PersistenceConfig,
        binding: &[u8],
        cursor_key: [u8; 32],
        writable: bool,
        expected: Option<FileIdentity>,
    ) -> Result<Self, BackendError> {
        let directory = config.path.parent().map(Path::to_path_buf);
        let session = Arc::new(match expected {
            Some(identity) => Session::connect_pinned_writer(
                &config.path,
                config.sqlite_profile,
                config.sqlite_pack_layout,
                config.sqlite_acquisition,
                identity,
            )?,
            None => Session::connect(
                &config.path,
                writable,
                false,
                config.sqlite_profile,
                config.sqlite_pack_layout,
                config.sqlite_acquisition,
            )?,
        });
        // An existing Store keeps the layout and acquisition schema it was
        // written with; the creation-only config selection does not name it.
        // A current Store runs none of the additive migrations below; only one
        // that recorded an older source (a converted schema version 1 Store)
        // does, each step in its own write transaction.
        if writable && session.needs_migration()? {
            // A Store at the released 2.0 table set recorded its schema
            // without the position columns; it gains them before any table
            // migration runs.
            session.add_gc_columns()?;
            session.add_binding_hints()?;
            session.add_lineage(crate::history::backfill_lineage)?;
            session.add_merge_scratch()?;
            session.add_file_digests()?;
            // A Store an earlier migration just brought forward recorded its
            // source without the position columns, so it gains them here,
            // before `add_gc` records the full schema.
            session.add_gc_columns()?;
            session.add_gc()?;
            session.add_kept_node()?;
            session.add_position_state()?;
        }
        let handles = Self::validated(session, directory, binding, cursor_key)?;
        if writable {
            crate::backend::sqlite::storage::merge_records::sweep(&handles.storage.session)?;
        }
        Ok(handles)
    }
    fn validated(
        session: Arc<Session>,
        directory: Option<PathBuf>,
        binding: &[u8],
        cursor_key: [u8; 32],
    ) -> Result<Self, BackendError> {
        if cursor_key == [0; 32] {
            return Err(BackendError::Integrity);
        }
        let catalog_id = CatalogId::derive(binding).map_err(|_| BackendError::Integrity)?;
        let (incarnation, digest_floor, trace_salt) = session.validate(catalog_id, binding)?;
        let storage = Arc::new(StorageProvider {
            session: session.clone(),
        });
        Ok(Self {
            storage,
            acquisition: AcquisitionProvider::new(session.clone(), directory),
            history: HistoryProvider {
                sqlite_writer: None,
                engine: crate::backend::Engine::Sqlite(session),
                catalog_id,
                incarnation,
                cursor_key,
                digest_floor,
                trace_salt,
                mark_budget: std::sync::atomic::AtomicU64::new(crate::history::spill::BUDGET_BYTES),
            },
        })
    }
}

/// Writer capabilities published together after the first successful start.
pub struct SqliteWriter {
    /// The writable physical provider, also used for checkpoints.
    pub storage: Arc<StorageProvider>,
    /// Fused Commit publication on the writable session.
    pub committer: Committer,
    /// Merge build records on the writable session.
    pub merge_scratch: MergeScratch,
}

/// One optional writable session for a Store opened for in-process calls.
/// A failed open leaves the slot empty. Only the caller holding the
/// initializer makes an attempt, and readers never wait for that turn.
pub struct DeferredSqliteWriter {
    config: PersistenceConfig,
    binding: Vec<u8>,
    cursor_key: [u8; 32],
    identity: FileIdentity,
    initialize: Mutex<()>,
    opened: OnceLock<SqliteWriter>,
}

impl DeferredSqliteWriter {
    /// Opens a validated reader and the mixed read/write catalog over that
    /// authority. The reader's catalog remains read-only for control lookups.
    pub fn open(
        config: PersistenceConfig,
        binding: &[u8],
        cursor_key: [u8; 32],
    ) -> Result<(SqliteHandles, HistoryProvider, Arc<Self>), PersistenceError> {
        let reader = SqliteHandles::open_read_only(config.clone(), binding, cursor_key)?;
        let deferred = Arc::new(Self {
            config,
            binding: binding.to_vec(),
            cursor_key,
            identity: reader
                .storage
                .session
                .identity
                .ok_or(PersistenceError::BackendUnavailable)?,
            initialize: Mutex::default(),
            opened: OnceLock::new(),
        });
        let history = reader.history.with_sqlite_writer(deferred.clone());
        Ok((reader, history, deferred))
    }

    /// The current writer, without opening one or waiting for a start.
    pub fn get(&self) -> Option<&SqliteWriter> {
        self.opened.get()
    }

    /// Returns the writer, making at most one opening attempt for this call.
    pub fn start(&self) -> Result<&SqliteWriter, PersistenceError> {
        Ok(self.start_writer()?)
    }

    /// Starts the writer before a history mutation prepares its effect.
    pub fn prepare_history_write(&self) -> layerfs_history::HistoryResult<()> {
        self.start_writer()?;
        Ok(())
    }

    pub(crate) fn start_writer(&self) -> Result<&SqliteWriter, BackendError> {
        if let Some(writer) = self.opened.get() {
            return Ok(writer);
        }
        let _turn = self.initialize.lock().map_err(|_| BackendError::Closed)?;
        if let Some(writer) = self.opened.get() {
            return Ok(writer);
        }
        let handles = SqliteHandles::open(
            self.config.clone(),
            &self.binding,
            self.cursor_key,
            true,
            Some(self.identity),
        )?;
        Ok(self.opened.get_or_init(|| SqliteWriter {
            committer: handles.history.committer(),
            merge_scratch: handles.storage.merge_scratch(),
            storage: handles.storage,
        }))
    }
}
