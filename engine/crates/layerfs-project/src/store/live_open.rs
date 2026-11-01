//! The live Store's handles on its engine, for one add (#533 step 10b): a
//! SQLite file, or with the `postgresql` feature a PostgreSQL schema. Built
//! without that feature, a PostgreSQL selection is `BackendUnavailable`.
use crate::store::InitError;
use layerfs_persistence::{BackendSelection, Handles, HistoryProvider, PersistenceConfig};
use layerfs_storage::port::{acquisition::Acquisition, PackPersistence};
use std::sync::Arc;

/// One writable session on the live Store.
pub(crate) enum Opened {
    Sqlite(Handles),
    #[cfg(feature = "postgresql")]
    Postgres(layerfs_persistence::PostgresHandles),
}

impl Opened {
    /// Opens the Store `store` selects, validated against its authority.
    pub(crate) fn writable(
        store: &PersistenceConfig,
        binding: &[u8],
        cursor_key: [u8; 32],
    ) -> Result<Self, InitError> {
        let opened = match &store.backend {
            BackendSelection::Sqlite => {
                Handles::open_writable(store.clone(), binding, cursor_key).map(Self::Sqlite)
            }
            #[cfg(feature = "postgresql")]
            BackendSelection::Postgres(selection) => {
                layerfs_persistence::PostgresHandles::open_writable(selection, binding, cursor_key)
                    .map(Self::Postgres)
            }
            #[cfg(not(feature = "postgresql"))]
            BackendSelection::Postgres(_) => {
                Err(layerfs_storage::port::PersistenceError::BackendUnavailable)
            }
        };
        opened.map_err(InitError::Persistence)
    }

    pub(crate) fn storage(&self) -> Arc<dyn PackPersistence> {
        match self {
            Self::Sqlite(handles) => handles.storage.clone(),
            #[cfg(feature = "postgresql")]
            Self::Postgres(handles) => handles.storage.clone(),
        }
    }

    pub(crate) fn history(&self) -> &HistoryProvider {
        match self {
            Self::Sqlite(handles) => &handles.history,
            #[cfg(feature = "postgresql")]
            Self::Postgres(handles) => &handles.history,
        }
    }

    pub(crate) fn acquisition(&self) -> &dyn Acquisition {
        match self {
            Self::Sqlite(handles) => &handles.acquisition,
            #[cfg(feature = "postgresql")]
            Self::Postgres(handles) => &handles.acquisition,
        }
    }

    /// The engine version: SQLite's, or `postgresql <server version>`.
    pub(crate) fn version(&self) -> String {
        match self {
            Self::Sqlite(handles) => handles.profile().sqlite_version.clone(),
            #[cfg(feature = "postgresql")]
            Self::Postgres(handles) => format!("postgresql {}", handles.storage.server_version()),
        }
    }

    /// Refuses every later call of this session; see [`Handles::close`].
    pub(crate) fn close(&self) {
        match self {
            Self::Sqlite(handles) => handles.close(),
            #[cfg(feature = "postgresql")]
            Self::Postgres(handles) => handles.close(),
        }
    }
}

/// Creates the new PostgreSQL schema `store` selects, holding no project,
/// with the acquisition tables projects are added through.
pub(crate) fn create_postgresql(
    store: &PersistenceConfig,
    policy: layerfs_storage::StoragePolicy,
    catalog: &layerfs_history::HistoryCatalogConfig,
) -> Result<(), InitError> {
    let BackendSelection::Postgres(selection) = &store.backend else {
        return Err(InitError::Manifest("a PostgreSQL Store selection required"));
    };
    #[cfg(feature = "postgresql")]
    {
        let handles = layerfs_persistence::PostgresHandles::create(selection, policy, catalog)
            .map_err(InitError::Persistence)?;
        handles.close();
        Ok(())
    }
    #[cfg(not(feature = "postgresql"))]
    {
        let _ = (selection, policy, catalog);
        Err(InitError::Persistence(
            layerfs_storage::port::PersistenceError::BackendUnavailable,
        ))
    }
}
