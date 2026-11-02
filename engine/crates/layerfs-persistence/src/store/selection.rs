//! The one Store selection: a SQLite file or a PostgreSQL schema.
#[cfg(feature = "postgresql")]
use crate::PostgresHandles;
use crate::{PersistenceConfig, PostgresSelection, SqlWork, SqliteHandles, StorageProvider};
use layerfs_storage::port::PersistenceError;
use std::path::Path;
/// The one Store selection: a SQLite Store file (the spec's `SqliteSelection`
/// is the existing `PersistenceConfig`) or a PostgreSQL schema (#506).
#[derive(Clone, Debug)]
pub enum StoreSelection {
    /// A SQLite Store file, described by its complete config.
    Sqlite(PersistenceConfig),
    /// A PostgreSQL Store.
    Postgres(PostgresSelection),
}
impl From<PersistenceConfig> for StoreSelection {
    fn from(config: PersistenceConfig) -> Self {
        Self::Sqlite(config)
    }
}
impl From<PostgresSelection> for StoreSelection {
    fn from(selection: PostgresSelection) -> Self {
        Self::Postgres(selection)
    }
}
/// Operations only a Store file has (snapshot, heads digest), as a capability.
pub trait StoreFile {
    /// One consistent copy of this live Store to `destination`, a new file;
    /// see [`StorageProvider::snapshot_observed`].
    fn snapshot(
        &self,
        destination: &Path,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<crate::SealedStore, PersistenceError>;
    /// The history heads digest of this Store's own file.
    fn heads_digest(&self) -> Result<[u8; 32], PersistenceError>;
}
impl StoreFile for StorageProvider {
    fn snapshot(
        &self,
        destination: &Path,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<crate::SealedStore, PersistenceError> {
        self.snapshot_observed(destination, progress)
    }
    fn heads_digest(&self) -> Result<[u8; 32], PersistenceError> {
        crate::heads_digest(&self.session.path)
    }
}
/// One Store session's handles, SQLite or PostgreSQL.
pub enum Handles {
    /// A SQLite Store file session.
    Sqlite(SqliteHandles),
    /// A PostgreSQL Store session (#506).
    #[cfg(feature = "postgresql")]
    Postgres(PostgresHandles),
}
impl Handles {
    /// Opens validated authority for `selection` with the explicitly selected
    /// profile; acknowledged reservations persist.
    pub fn open_writable(
        selection: &StoreSelection,
        binding: &[u8],
        cursor_key: [u8; 32],
    ) -> Result<Self, PersistenceError> {
        match selection {
            StoreSelection::Sqlite(config) => Ok(Self::Sqlite(SqliteHandles::open_writable(
                config.clone(),
                binding,
                cursor_key,
            )?)),
            StoreSelection::Postgres(selection) => {
                Self::postgres(selection, true, binding, cursor_key)
            }
        }
    }
    /// [`Self::open_writable`], read-only.
    pub fn open_read_only(
        selection: &StoreSelection,
        binding: &[u8],
        cursor_key: [u8; 32],
    ) -> Result<Self, PersistenceError> {
        match selection {
            StoreSelection::Sqlite(config) => Ok(Self::Sqlite(SqliteHandles::open_read_only(
                config.clone(),
                binding,
                cursor_key,
            )?)),
            StoreSelection::Postgres(selection) => {
                Self::postgres(selection, false, binding, cursor_key)
            }
        }
    }
    #[cfg(feature = "postgresql")]
    fn postgres(
        selection: &PostgresSelection,
        writable: bool,
        binding: &[u8],
        cursor_key: [u8; 32],
    ) -> Result<Self, PersistenceError> {
        Ok(Self::Postgres(if writable {
            PostgresHandles::open_writable(selection, binding, cursor_key)?
        } else {
            PostgresHandles::open_read_only(selection, binding, cursor_key)?
        }))
    }
    #[cfg(not(feature = "postgresql"))]
    fn postgres(
        _selection: &PostgresSelection,
        _writable: bool,
        _binding: &[u8],
        _cursor_key: [u8; 32],
    ) -> Result<Self, PersistenceError> {
        Err(PersistenceError::BackendUnavailable)
    }
    /// Refuses every later call of this session; see the kind's `close`.
    pub fn close(&self) {
        match self {
            Self::Sqlite(handles) => handles.close(),
            #[cfg(feature = "postgresql")]
            Self::Postgres(handles) => handles.close(),
        }
    }
    /// Actual SQL, VM, binding, transaction and BLOB counts.
    pub fn diagnostics(&self) -> Result<SqlWork, PersistenceError> {
        match self {
            Self::Sqlite(handles) => handles.diagnostics(),
            #[cfg(feature = "postgresql")]
            Self::Postgres(handles) => handles.diagnostics(),
        }
    }
    /// The Store-file operations of a SQLite session; `None` for PostgreSQL.
    pub fn file(&self) -> Option<&dyn StoreFile> {
        match self {
            Self::Sqlite(handles) => Some(handles.storage.as_ref()),
            #[cfg(feature = "postgresql")]
            Self::Postgres(_) => None,
        }
    }
}
