//! The live Store's handles on its engine, for one add (#533 step 10b): a
//! SQLite file or a PostgreSQL schema.
use crate::store::InitError;
use layerfs_persistence::{HistoryProvider, SqliteHandles, StoreSelection};
use layerfs_storage::port::{acquisition::Acquisition, PackPersistence};
use std::sync::Arc;

/// One writable session on the live Store.
pub(crate) enum Opened {
    Sqlite(SqliteHandles),
    Postgres(layerfs_persistence::PostgresHandles),
}

impl Opened {
    /// Opens the Store `store` selects, validated against its authority.
    pub(crate) fn writable(
        selection: &StoreSelection,
        binding: &[u8],
        cursor_key: [u8; 32],
    ) -> Result<Self, InitError> {
        let opened = match selection {
            StoreSelection::Sqlite(config) => {
                SqliteHandles::open_writable(config.clone(), binding, cursor_key).map(Self::Sqlite)
            }
            StoreSelection::Postgres(selection) => {
                layerfs_persistence::PostgresHandles::open_writable(selection, binding, cursor_key)
                    .map(Self::Postgres)
            }
        };
        opened.map_err(InitError::Persistence)
    }

    pub(crate) fn storage(&self) -> Arc<dyn PackPersistence> {
        match self {
            Self::Sqlite(handles) => handles.storage.clone(),
            Self::Postgres(handles) => handles.storage.clone(),
        }
    }

    pub(crate) fn history(&self) -> &HistoryProvider {
        match self {
            Self::Sqlite(handles) => &handles.history,
            Self::Postgres(handles) => &handles.history,
        }
    }

    pub(crate) fn acquisition(&self) -> &dyn Acquisition {
        match self {
            Self::Sqlite(handles) => &handles.acquisition,
            Self::Postgres(handles) => &handles.acquisition,
        }
    }

    /// The engine version: SQLite's, or `postgresql <server version>`.
    pub(crate) fn version(&self) -> String {
        match self {
            Self::Sqlite(handles) => handles.profile().sqlite_version.clone(),
            Self::Postgres(handles) => format!("postgresql {}", handles.storage.server_version()),
        }
    }

    /// Refuses every later call of this session; see [`SqliteHandles::close`].
    pub(crate) fn close(&self) {
        match self {
            Self::Sqlite(handles) => handles.close(),
            Self::Postgres(handles) => handles.close(),
        }
    }
}

/// Creates the new PostgreSQL schema `store` selects, holding no project,
/// with the acquisition tables projects are added through.
pub(crate) fn create_postgresql(
    selection: &StoreSelection,
    policy: layerfs_storage::StoragePolicy,
    catalog: &layerfs_history::HistoryCatalogConfig,
) -> Result<(), InitError> {
    let StoreSelection::Postgres(selection) = selection else {
        return Err(InitError::Manifest("a PostgreSQL Store selection required"));
    };
    {
        let handles = layerfs_persistence::PostgresHandles::create(selection, policy, catalog)
            .map_err(InitError::Persistence)?;
        handles.close();
        Ok(())
    }
}
