//! A PostgreSQL Store (#506): explicit creation and open.
//!
//! The same shared history and Metadata semantics as the SQLite Store run
//! over the PostgreSQL engine. Opening never creates or migrates a schema;
//! [`PostgresHandles::create`] makes a new empty one with Init's acquisition
//! tables (#533 step 10b). Projects reach it only by Init; nothing converts
//! a SQLite Store (step 10c).
use crate::backend::{
    postgresql::{
        schema,
        session::{connection::Session, transaction::Mode},
        storage::provider,
    },
    records::BackendError,
    Engine,
};
use crate::{HistoryProvider, PostgresCheck, PostgresSelection, SqlWork};
use layerfs_history::{CatalogId, HistoryCatalogConfig};
use layerfs_storage::{port::PersistenceError, StoragePolicy};
use std::sync::Arc;

pub use crate::backend::postgresql::acquisition::PostgresAcquisition;
pub use crate::backend::postgresql::lane::lane::LaneHold as PostgresLaneHold;
pub use provider::PostgresStorage;

/// Physical, history and acquisition handles over one PostgreSQL session.
pub struct PostgresHandles {
    /// Bounded pack publication/read/allocation port.
    pub storage: Arc<PostgresStorage>,
    /// History authority validated at open.
    pub history: HistoryProvider,
    /// Init's acquisition working state over the same session. Every unit is
    /// refused unless the schema was made with the acquisition tables.
    pub acquisition: PostgresAcquisition,
}

impl PostgresHandles {
    /// Creates the schema `selection.schema`, which must not exist, holding
    /// an empty Store with Init's acquisition tables, the frozen `policy`
    /// and the catalog authority `history`, in one transaction; then opens
    /// it writable. An existing schema of that name is refused
    /// (`Refused { status: "schema exists" }`) with nothing written.
    pub fn create(
        selection: &PostgresSelection,
        policy: StoragePolicy,
        history: &HistoryCatalogConfig,
    ) -> Result<Self, PersistenceError> {
        let policy = policy
            .validated()
            .map_err(|_| PersistenceError::Malformed)?;
        history.check().map_err(|_| PersistenceError::Malformed)?;
        let catalog_id =
            CatalogId::derive(&history.binding_key).map_err(|_| PersistenceError::Malformed)?;
        let session = Arc::new(Session::connect(selection, true)?);
        // The lane row does not exist until the schema does.
        session.run_as(Mode::Stage, |tx| {
            schema::create(tx, policy, history, catalog_id)
        })?;
        // Init's open checks the whole contract, whatever the selection says.
        Self::validated(
            session,
            PostgresCheck::Full,
            Some(&history.binding_key),
            history.cursor_key,
        )
        .map(|(_, handles)| handles)
    }

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

    /// Opens an independently owned observational connection whose DNS,
    /// TCP/TLS/authentication and every statement honor the absolute deadline.
    pub fn open_diagnostic(
        selection: &PostgresSelection,
        binding: &[u8],
        cursor_key: [u8; 32],
        deadline: Option<std::time::Instant>,
        resolver: &mut crate::diagnostic::Resolver<'_>,
    ) -> Result<Self, PersistenceError> {
        let admission = deadline
            .unwrap_or_else(|| std::time::Instant::now() + std::time::Duration::from_secs(10));
        Self::open_diagnostic_admitted(
            selection, binding, cursor_key, deadline, admission, resolver,
        )
    }

    /// Shares a caller's absolute admission budget across independent read-only
    /// preflight and pinned-open sessions. Logical audit stays unlimited when
    /// `deadline` is None after bounded schema/identity admission finishes.
    pub fn open_diagnostic_admitted(
        selection: &PostgresSelection,
        binding: &[u8],
        cursor_key: [u8; 32],
        deadline: Option<std::time::Instant>,
        admission: std::time::Instant,
        resolver: &mut crate::diagnostic::Resolver<'_>,
    ) -> Result<Self, PersistenceError> {
        let session = Arc::new(Session::connect_diagnostic_admitted(
            selection, deadline, admission, resolver,
        )?);
        session
            .state
            .lock()
            .map_err(|_| PersistenceError::Uncertain)?
            .client
            .set_diagnostic_deadline(Some(
                deadline.map_or(admission, |total| total.min(admission)),
            ));
        let (_, handles) = Self::validated(
            session.clone(),
            PostgresCheck::Full,
            Some(binding),
            cursor_key,
        )?;
        session
            .state
            .lock()
            .map_err(|_| PersistenceError::Uncertain)?
            .client
            .set_diagnostic_deadline(deadline);
        Ok(handles)
    }

    /// Opens one read-only session under the binding the schema records,
    /// returned for the caller to compare with its own (a Sandbox's reach
    /// check, #65): `selection.check` checks everything else, as an open
    /// does, in the same one round trip.
    pub fn attach_read_only(
        selection: &PostgresSelection,
        cursor_key: [u8; 32],
    ) -> Result<(Vec<u8>, Self), PersistenceError> {
        let session = Arc::new(Session::connect(selection, false)?);
        Self::validated(session, selection.check, None, cursor_key)
    }

    /// The binding key recorded in the schema `selection.schema`, read in one
    /// read-only session, so a home that did not create the schema can open
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
        CatalogId::derive(binding).map_err(|_| PersistenceError::Malformed)?;
        let session = Arc::new(Session::connect(selection, writable)?);
        Self::validated(session, selection.check, Some(binding), cursor_key)
            .map(|(_, handles)| handles)
    }

    /// Handles over `session` once `check` passed; the session keeps the
    /// policy and server version it read. With `binding` the recorded one
    /// must equal it; without, the recorded one is returned.
    fn validated(
        session: Arc<Session>,
        check: PostgresCheck,
        binding: Option<&[u8]>,
        cursor_key: [u8; 32],
    ) -> Result<(Vec<u8>, Self), PersistenceError> {
        if cursor_key == [0; 32] {
            return Err(PersistenceError::Malformed);
        }
        let checked = schema::check(&session, check, binding)?;
        let catalog_id =
            CatalogId::derive(&checked.binding).map_err(|_| PersistenceError::Malformed)?;
        let digest_floor = checked.policy.small_file_threshold_bytes();
        let _ = session.policy.set(checked.policy);
        let _ = session.server_version.set(checked.server_version);
        let (incarnation, tables) = (checked.incarnation, checked.acquisition);
        Ok((
            checked.binding,
            Self {
                storage: Arc::new(PostgresStorage {
                    session: session.clone(),
                }),
                acquisition: PostgresAcquisition::new(session.clone(), tables),
                history: HistoryProvider {
                    engine: Engine::Postgres(session),
                    catalog_id,
                    incarnation,
                    cursor_key,
                    digest_floor,
                },
            },
        ))
    }

    /// Refuses every queued and later call with `Closed`; see [`SqliteHandles::close`].
    pub fn close(&self) {
        self.storage.session.admission.close();
    }

    /// Counters of this session's attempted work.
    pub fn diagnostics(&self) -> Result<SqlWork, PersistenceError> {
        self.storage.diagnostics()
    }
}
