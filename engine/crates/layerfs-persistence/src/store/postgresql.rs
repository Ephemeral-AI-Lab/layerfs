//! A PostgreSQL Store (#506): explicit open and import, no Init.
//!
//! The same shared history and Metadata semantics as the SQLite Store run
//! over the PostgreSQL engine. Opening never creates or migrates a schema;
//! a schema is made only by [`PostgresHandles::import`] from a SQLite Store.
use crate::backend::{
    postgresql::{import::import, schema, session::connection::Session, storage::provider},
    records::BackendError,
    Engine,
};
use crate::{Handles, HistoryProvider, PersistenceConfig, PostgresSelection, SqlWork};
use layerfs_history::CatalogId;
use layerfs_storage::port::PersistenceError;
use std::sync::Arc;

pub use crate::backend::postgresql::import::ImportReceipt;
pub use crate::backend::postgresql::lane::lane::LaneHold as PostgresLaneHold;
pub use provider::PostgresStorage;

/// Physical and history handles over one PostgreSQL session.
pub struct PostgresHandles {
    /// Bounded pack publication/read/allocation port.
    pub storage: Arc<PostgresStorage>,
    /// History authority validated at open.
    pub history: HistoryProvider,
}

impl PostgresHandles {
    /// Opens validated authority for reads and writes.
    pub fn open_writable(
        selection: &PostgresSelection,
        binding: &[u8],
        cursor_key: [u8; 32],
    ) -> Result<Self, PersistenceError> {
        Self::open(selection, binding, cursor_key, true)
    }

    /// Opens one read-only session; every transaction is `READ ONLY`.
    pub fn open_read_only(
        selection: &PostgresSelection,
        binding: &[u8],
        cursor_key: [u8; 32],
    ) -> Result<Self, PersistenceError> {
        Self::open(selection, binding, cursor_key, false)
    }

    /// The binding key recorded in the schema `selection.schema`, read in one
    /// read-only session, so a home that did not import the schema can open
    /// it. Only the table set is checked here; opening checks the rest.
    pub fn binding(selection: &PostgresSelection) -> Result<Vec<u8>, PersistenceError> {
        let session = Session::connect(selection, false)?;
        Ok(session.run::<_, BackendError>(false, schema::binding)?)
    }

    fn open(
        selection: &PostgresSelection,
        binding: &[u8],
        cursor_key: [u8; 32],
        writable: bool,
    ) -> Result<Self, PersistenceError> {
        if cursor_key == [0; 32] {
            return Err(PersistenceError::Malformed);
        }
        let catalog_id = CatalogId::derive(binding).map_err(|_| PersistenceError::Malformed)?;
        let session = Arc::new(Session::connect(selection, writable)?);
        let incarnation = session
            .run::<_, BackendError>(false, |tx| schema::validate(tx, catalog_id, binding))?;
        Ok(Self {
            storage: Arc::new(PostgresStorage {
                session: session.clone(),
            }),
            history: HistoryProvider {
                engine: Engine::Postgres(session),
                catalog_id,
                incarnation,
                cursor_key,
            },
        })
    }

    /// Copies the SQLite Store `source` into the new schema `target.schema`,
    /// which must not exist, in one PostgreSQL transaction over one SQLite
    /// snapshot. The source's binding is checked at open; nothing is written to it.
    pub fn import(
        source: PersistenceConfig,
        target: &PostgresSelection,
        binding: &[u8],
        cursor_key: [u8; 32],
    ) -> Result<ImportReceipt, PersistenceError> {
        // A read-only SQLite connection cannot open a closed WAL Store that
        // has no `-shm` sidecar (a sealed or snapshot file), so the source
        // opens writable; the import itself only reads, in one snapshot.
        let sqlite = Handles::open_writable(source, binding, cursor_key)?;
        let session = Session::connect(target, true)?;
        sqlite.storage.session.run(false, |tx| import(tx, &session))
    }

    /// Refuses every queued and later call with `Closed`; see [`Handles::close`].
    pub fn close(&self) {
        self.storage.session.admission.close();
    }

    /// Counters of this session's attempted work.
    pub fn diagnostics(&self) -> Result<SqlWork, PersistenceError> {
        self.storage.diagnostics()
    }
}
