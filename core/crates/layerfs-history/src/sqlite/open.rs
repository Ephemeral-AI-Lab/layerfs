//! Catalog creation, writable reopen, read-only reopen and profile validation.
//!
//! A fresh writable catalog is created inside the process that will own it.
//! That ownership is the continuity evidence this provider has: a later process
//! can open the same file read-only, and every mutation or new allocation on
//! such a handle is refused with `ContinuityUnavailable` rather than attempted.
//! No clean-exit flag, no root scan and no `assume_clean` input establishes
//! continuity, because none of them can prove that exposed inode serials were
//! not already handed out by an authority that is gone.
//!
//! Open validates the application identity, the schema version, the exact
//! application-table set and the singleton metadata row, including the binding
//! derived catalog identity. An incomplete, foreign or inconsistent catalog is
//! refused; nothing is migrated, repaired or promoted.

use crate::catalog::{HistoryCatalog, HistoryCatalogConfig};
use crate::error::{HistoryError, HistoryResult};
use crate::identity::{
    BranchId, CatalogId, CommitId, LayerId, LayerStackId, WorkspaceId, IDENTITY_FORMAT,
};
use crate::records::{
    AddLayerOutcome, AddLayerRequest, BranchRecord, BranchSnapshot, CommitHistoryRequest,
    CommitRecord, CommitStagedOutcome, CommitStagedRequest, DiscardOutcome, DiscardRequest,
    ForkRequest, LayerHistoryRequest, LayerRecord, LayerStackRecord, Page, PageResult, Reservation,
    ReserveRequest, StackInitialization, StageRecord, StageRequest,
};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use super::{allocation, branch, commit, layerstack, staging};

/// A history catalog stored in one embedded SQLite file.
///
/// The cell holds one connection and the writable-authority flag established
/// when this process created the catalog. A handle opened read-only carries no
/// authority, so every mutation and every new allocation is refused with
/// `ContinuityUnavailable` rather than attempted.
pub struct SqliteCatalog {
    pub(crate) state: Mutex<Provider>,
    pub(crate) catalog_id: CatalogId,
    pub(crate) incarnation: u64,
    pub(crate) cursor_key: [u8; 32],
}

/// The single connection and the authority that owns it.
pub(crate) struct Provider {
    pub(crate) connection: Connection,
    pub(crate) writable: bool,
    pub(crate) quarantined: bool,
}

/// Application identity of the C5 catalog; distinct from C2's schema identity.
pub const APPLICATION_ID: i64 = 1_279_677_256;
/// Schema version of the C5 catalog.
pub const USER_VERSION: i64 = 1;
/// The exact application tables schema 1 defines, in name order.
pub const TABLES: [&str; 7] = [
    "branches",
    "commits",
    "history_meta",
    "layer_stacks",
    "layers",
    "scope_allocator",
    "workspace_stages",
];
/// The frozen schema text this build installs and validates against.
pub const SCHEMA: &str = include_str!("../../sql/schema-v1.sql");

/// Creates one fresh writable catalog inside this process.
pub fn create(path: &Path, config: &HistoryCatalogConfig) -> HistoryResult<SqliteCatalog> {
    config.check()?;
    let catalog_id = CatalogId::derive(&config.binding_key)?;
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE;
    let connection = Connection::open_with_flags(path, flags).map_err(super::rows::sql)?;
    configure(&connection, true)?;
    if application_id(&connection)? != 0 || application_tables(&connection)?.next().is_some() {
        return Err(HistoryError::InvalidInput("catalog already exists"));
    }
    connection.execute_batch(SCHEMA).map_err(super::rows::sql)?;
    connection
        .execute(
            "INSERT INTO history_meta \
             (id, catalog_id, catalog_incarnation, binding_key, identity_format, next_stage_token) \
             VALUES (1, ?1, ?2, ?3, ?4, 1)",
            rusqlite::params![
                catalog_id.as_slice(),
                i64::try_from(config.incarnation)
                    .map_err(|_| HistoryError::InvalidInput("catalog incarnation"))?,
                config.binding_key.as_slice(),
                IDENTITY_FORMAT
            ],
        )
        .map_err(super::rows::sql)?;
    let stored = read_meta(&connection, &catalog_id)?;
    Ok(SqliteCatalog {
        state: std::sync::Mutex::new(Provider {
            connection,
            writable: true,
            quarantined: false,
        }),
        catalog_id,
        incarnation: stored,
        cursor_key: config.cursor_key,
    })
}

/// Opens one existing, closed catalog for writable continuity in this process.
///
/// This is the reopen a prepared Store/history byte copy needs before a caller
/// can fork, commit or allocate: the catalog it names was created and closed by
/// a prior process, and this process now owns its continuity. The same
/// validation as the read-only open applies — application identity, schema
/// version, the exact application-table set, the singleton metadata row and the
/// binding-derived catalog identity — and nothing is migrated, repaired or
/// promoted. An incomplete, foreign or inconsistent catalog is refused.
pub fn open_writable(
    path: &Path,
    binding_key: &[u8],
    cursor_key: [u8; 32],
) -> HistoryResult<SqliteCatalog> {
    if cursor_key == [0; 32] {
        return Err(HistoryError::InvalidInput("cursor capability"));
    }
    let catalog_id = CatalogId::derive(binding_key)?;
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE;
    let connection = Connection::open_with_flags(path, flags).map_err(super::rows::sql)?;
    configure(&connection, true)?;
    if application_id(&connection)? != APPLICATION_ID {
        return Err(HistoryError::Integrity("catalog application identity"));
    }
    if user_version(&connection)? != USER_VERSION {
        return Err(HistoryError::Unsupported("catalog schema version"));
    }
    let stored = read_meta(&connection, &catalog_id)?;
    Ok(SqliteCatalog {
        state: std::sync::Mutex::new(Provider {
            connection,
            writable: true,
            quarantined: false,
        }),
        catalog_id,
        incarnation: stored,
        cursor_key,
    })
}

/// Opens an existing catalog for reading only, refusing every mutation.
pub fn open_read_only(
    path: &Path,
    binding_key: &[u8],
    cursor_key: [u8; 32],
) -> HistoryResult<SqliteCatalog> {
    if cursor_key == [0; 32] {
        return Err(HistoryError::InvalidInput("cursor capability"));
    }
    let catalog_id = CatalogId::derive(binding_key)?;
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(super::rows::sql)?;
    configure(&connection, false)?;
    if application_id(&connection)? != APPLICATION_ID {
        return Err(HistoryError::Integrity("catalog application identity"));
    }
    if user_version(&connection)? != USER_VERSION {
        return Err(HistoryError::Unsupported("catalog schema version"));
    }
    let stored = read_meta(&connection, &catalog_id)?;
    Ok(SqliteCatalog {
        state: std::sync::Mutex::new(Provider {
            connection,
            writable: false,
            quarantined: false,
        }),
        catalog_id,
        incarnation: stored,
        cursor_key,
    })
}

/// Applies the declared embedded profile: MEMORY journal, no sync, no waiting.
pub(crate) fn configure(connection: &Connection, writable: bool) -> HistoryResult<()> {
    if writable {
        let journal: String = connection
            .query_row("PRAGMA journal_mode = MEMORY", [], |row| row.get(0))
            .map_err(super::rows::sql)?;
        if !journal.eq_ignore_ascii_case("memory") {
            return Err(HistoryError::Integrity("catalog journal mode"));
        }
    }
    connection
        .execute_batch("PRAGMA synchronous = OFF; PRAGMA temp_store = MEMORY;")
        .map_err(super::rows::sql)?;
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(super::rows::sql)?;
    let foreign_keys: i64 = connection
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .map_err(super::rows::sql)?;
    if foreign_keys != 1 {
        return Err(HistoryError::Integrity("catalog foreign key enforcement"));
    }
    connection
        .busy_timeout(Duration::ZERO)
        .map_err(super::rows::sql)?;
    Ok(())
}

/// The engine's application identity for this file.
pub(crate) fn application_id(connection: &Connection) -> HistoryResult<i64> {
    connection
        .query_row("PRAGMA application_id", [], |row| row.get(0))
        .map_err(super::rows::sql)
}

/// The engine's schema version for this file.
pub(crate) fn user_version(connection: &Connection) -> HistoryResult<i64> {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(super::rows::sql)
}

fn application_tables(connection: &Connection) -> HistoryResult<impl Iterator<Item = String>> {
    let mut statement = connection
        .prepare(
            "SELECT name FROM sqlite_master \
             WHERE type = 'table' AND name NOT GLOB 'sqlite_*' ORDER BY name",
        )
        .map_err(super::rows::sql)?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(super::rows::sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(super::rows::sql)?;
    Ok(names.into_iter())
}

/// Validates the singleton metadata row and returns the catalog incarnation.
fn read_meta(connection: &Connection, expected: &CatalogId) -> HistoryResult<u64> {
    if application_id(connection)? != APPLICATION_ID {
        return Err(HistoryError::Integrity("catalog application identity"));
    }
    if user_version(connection)? != USER_VERSION {
        return Err(HistoryError::Unsupported("catalog schema version"));
    }
    let mut tables = application_tables(connection)?;
    for expected_name in TABLES {
        match tables.next() {
            Some(name) if name == expected_name => {}
            _ => return Err(HistoryError::Integrity("catalog table set")),
        }
    }
    if tables.next().is_some() {
        return Err(HistoryError::Integrity("catalog table set"));
    }
    validate_schema(connection)?;
    let rows: i64 = connection
        .query_row("SELECT count(*) FROM history_meta", [], |row| row.get(0))
        .map_err(super::rows::sql)?;
    if rows != 1 {
        return Err(HistoryError::Integrity("catalog metadata rows"));
    }
    let row = connection
        .query_row(
            "SELECT catalog_id, catalog_incarnation, identity_format, next_stage_token, binding_key \
             FROM history_meta WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                ))
            },
        )
        .map_err(super::rows::sql)?;
    let stored = CatalogId::from_bytes(
        row.0
            .try_into()
            .map_err(|_| HistoryError::Integrity("catalog identity width"))?,
    );
    if stored != *expected
        || CatalogId::derive(&row.4).map_err(|_| HistoryError::Integrity("catalog binding"))?
            != stored
    {
        return Err(HistoryError::Integrity("catalog binding"));
    }
    if row.2 != IDENTITY_FORMAT {
        return Err(HistoryError::Unsupported("catalog identity format"));
    }
    if !(1..=i64::MAX).contains(&row.1) || !(1..=i64::MAX).contains(&row.3) {
        return Err(HistoryError::Integrity("catalog counter range"));
    }
    Ok(row.1 as u64)
}

impl HistoryCatalog for SqliteCatalog {
    fn catalog_id(&self) -> CatalogId {
        self.catalog_id
    }

    fn incarnation(&self) -> u64 {
        self.incarnation
    }

    fn layer_stack(&self, id: LayerStackId) -> HistoryResult<Option<LayerStackRecord>> {
        self.read(|tx| layerstack::layer_stack(tx, id))
    }

    fn layer_stacks(&self, page: &Page) -> HistoryResult<PageResult<LayerStackRecord>> {
        self.read(|tx| {
            layerstack::layer_stacks(
                tx,
                self.catalog_id,
                self.incarnation,
                &self.cursor_key,
                page,
            )
        })
    }

    fn branch(&self, id: BranchId) -> HistoryResult<Option<BranchRecord>> {
        self.read(|tx| branch::branch(tx, id))
    }

    fn branch_snapshot(&self, id: BranchId) -> HistoryResult<Option<BranchSnapshot>> {
        self.read(|tx| branch::branch_snapshot(tx, id))
    }

    fn branches(
        &self,
        stack: LayerStackId,
        page: &Page,
    ) -> HistoryResult<PageResult<BranchRecord>> {
        self.read(|tx| {
            branch::branches(
                tx,
                self.catalog_id,
                self.incarnation,
                &self.cursor_key,
                stack,
                page,
            )
        })
    }

    fn commit(&self, id: CommitId) -> HistoryResult<Option<CommitRecord>> {
        self.read(|tx| commit::commit(tx, id))
    }

    fn layer(&self, id: LayerId) -> HistoryResult<Option<LayerRecord>> {
        self.read(|tx| layerstack::layer(tx, id))
    }

    fn stage(&self, workspace: WorkspaceId) -> HistoryResult<Option<StageRecord>> {
        self.read(|tx| staging::stage(tx, workspace))
    }

    fn stages(&self, branch: BranchId, page: &Page) -> HistoryResult<PageResult<StageRecord>> {
        self.read(|tx| {
            staging::stages(
                tx,
                self.catalog_id,
                self.incarnation,
                &self.cursor_key,
                branch,
                page,
            )
        })
    }

    fn commit_history(
        &self,
        request: &CommitHistoryRequest,
    ) -> HistoryResult<PageResult<CommitRecord>> {
        self.read(|tx| {
            commit::commit_history(
                tx,
                self.catalog_id,
                self.incarnation,
                &self.cursor_key,
                request,
            )
        })
    }

    fn layer_history(
        &self,
        request: &LayerHistoryRequest,
    ) -> HistoryResult<PageResult<LayerRecord>> {
        self.read(|tx| {
            layerstack::layer_history(
                tx,
                self.catalog_id,
                self.incarnation,
                &self.cursor_key,
                request,
            )
        })
    }

    fn initialize_layerstack(
        &self,
        request: &StackInitialization,
    ) -> HistoryResult<LayerStackRecord> {
        self.write(|tx| layerstack::initialize_layerstack(tx, request))
    }

    fn fork(&self, request: &ForkRequest) -> HistoryResult<BranchSnapshot> {
        self.write(|tx| branch::fork(tx, request))
    }

    fn stage_changes(&self, request: &StageRequest) -> HistoryResult<StageRecord> {
        self.write(|tx| staging::stage_changes(tx, request))
    }

    fn commit_staged(&self, request: &CommitStagedRequest) -> HistoryResult<CommitStagedOutcome> {
        let mut observed = None;
        self.write(|tx| commit::commit_staged(tx, request, &mut observed))
            .map_err(|error| error.with_observed_stage(request.workspace, observed))
    }

    fn add_layer(&self, request: &AddLayerRequest) -> HistoryResult<AddLayerOutcome> {
        self.write(|tx| layerstack::add_layer(tx, request))
    }

    fn discard_stage(&self, request: &DiscardRequest) -> HistoryResult<DiscardOutcome> {
        let mut observed = None;
        self.write(|tx| staging::discard_stage(tx, request, &mut observed))
            .map_err(|error| error.with_observed_stage(request.workspace, observed))
    }

    fn reserve_inodes(&self, request: &ReserveRequest) -> HistoryResult<Reservation> {
        self.write(|tx| allocation::reserve_inodes(tx, self.catalog_id, request))
    }
}

fn validate_schema(connection: &Connection) -> HistoryResult<()> {
    type Definition = (String, String, String, Option<String>);
    fn definitions(connection: &Connection) -> HistoryResult<Vec<Definition>> {
        let mut statement = connection.prepare(
            "SELECT type, name, tbl_name, sql FROM sqlite_schema WHERE name NOT GLOB 'sqlite_*' ORDER BY type, name"
        ).map_err(super::rows::sql)?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .map_err(super::rows::sql)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(super::rows::sql)
    }
    let expected = Connection::open_in_memory().map_err(super::rows::sql)?;
    configure(&expected, true)?;
    expected.execute_batch(SCHEMA).map_err(super::rows::sql)?;
    if definitions(connection)? != definitions(&expected)? {
        return Err(HistoryError::Integrity("catalog schema definition"));
    }
    Ok(())
}
