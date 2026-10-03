//! Explicit creation and validated writable/read-only PostgreSQL history handles.
use super::transaction::error;
use crate::{client::Client, config::PgConfig, params::Param, wire::PgDiagnostics};
use layerfs_history::{CatalogId, HistoryCatalogConfig, HistoryError, HistoryResult};
use std::sync::{Arc, Mutex};
const TABLES: [&str; 7] = [
    "branch",
    "commit",
    "history_meta",
    "layer",
    "layer_stack",
    "scope_allocator",
    "workspace_stage",
];
const STORAGE_TABLES: [&str; 5] = [
    "content_signature",
    "metadata_value_group",
    "object",
    "pack",
    "store_policy",
];
const SCHEMA: &str = include_str!("../../sql/history.sql");
pub(crate) struct State {
    pub(crate) quarantined: bool,
}
/// Complete history authority over a selected PostgreSQL schema.
///
/// Each handle owns one connection. Read-only handles refuse mutations before
/// sending SQL. Independent writable handles serialize through a NOWAIT lock;
/// an uncertain operation quarantines the handle rather than guessing rollback.
pub struct PgHistory {
    pub(crate) client: Arc<Client>,
    pub(crate) state: Mutex<State>,
    pub(crate) writable: bool,
    pub(crate) catalog_id: CatalogId,
    pub(crate) incarnation: u64,
    pub(crate) cursor_key: [u8; 32],
}
impl PgHistory {
    /// Creates a fresh history catalog. C2 tables may already share the schema.
    pub fn create(config: PgConfig, history: &HistoryCatalogConfig) -> HistoryResult<Self> {
        history.check()?;
        let catalog_id = CatalogId::derive(&history.binding_key)?;
        let client = Client::connect(config.clone()).map_err(error)?;
        let existing = tables(&client, &config)?;
        if existing
            .iter()
            .any(|name| !STORAGE_TABLES.contains(&name.as_str()))
        {
            return Err(HistoryError::InvalidInput("catalog already exists"));
        }
        client.bootstrap(config.sql(SCHEMA)).map_err(error)?;
        let definition = definition(&client, &config)?;
        client
            .query(
                include_str!("../../sql/queries/history/initialize_meta.sql"),
                vec![
                    Param::Bytes(catalog_id.as_slice().to_vec()),
                    Param::I64(history.incarnation as i64),
                    Param::Bytes(history.binding_key.clone()),
                    Param::Text(SCHEMA.to_owned()),
                    Param::Text(definition),
                ],
                true,
            )
            .map_err(error)?;
        let catalog = Self {
            client,
            state: Mutex::new(State { quarantined: false }),
            writable: true,
            catalog_id,
            incarnation: history.incarnation,
            cursor_key: history.cursor_key,
        };
        catalog.validate(&config)?;
        Ok(catalog)
    }
    /// Validates an existing catalog and grants writable authority to this handle.
    pub fn open_writable(
        config: PgConfig,
        binding: &[u8],
        cursor_key: [u8; 32],
    ) -> HistoryResult<Self> {
        Self::open(config, binding, cursor_key, true)
    }
    /// Validates an existing catalog without granting mutation/allocation authority.
    pub fn open_read_only(
        config: PgConfig,
        binding: &[u8],
        cursor_key: [u8; 32],
    ) -> HistoryResult<Self> {
        Self::open(config, binding, cursor_key, false)
    }
    fn open(
        config: PgConfig,
        binding: &[u8],
        cursor_key: [u8; 32],
        writable: bool,
    ) -> HistoryResult<Self> {
        if cursor_key == [0; 32] {
            return Err(HistoryError::InvalidInput("cursor capability"));
        }
        let catalog_id = CatalogId::derive(binding)?;
        let client = Client::connect(config.clone()).map_err(error)?;
        let mut catalog = Self {
            client,
            state: Mutex::new(State { quarantined: false }),
            writable,
            catalog_id,
            incarnation: 1,
            cursor_key,
        };
        catalog.incarnation = catalog.validate(&config)?;
        Ok(catalog)
    }
    fn validate(&self, config: &PgConfig) -> HistoryResult<u64> {
        self.read(|tx| {
            let names = tx.query(
                include_str!("../../sql/queries/history/tables.sql"),
                vec![Param::Text(config.schema.clone())],
            )?;
            let mut actual = Vec::new();
            for row in names {
                actual.push(
                    row.try_get::<_, String>(0)
                        .map_err(|_| HistoryError::Integrity("catalog table set"))?,
                );
            }
            if TABLES.iter().any(|name| !actual.iter().any(|v| v == name))
                || actual.iter().any(|name| {
                    !TABLES.contains(&name.as_str()) && !STORAGE_TABLES.contains(&name.as_str())
                })
            {
                return Err(HistoryError::Integrity("catalog table set"));
            }
            let counts = tx.query(
                include_str!("../../sql/queries/history/meta_count.sql"),
                Vec::<Param>::new(),
            )?;
            if counts.len() != 1 || counts[0].try_get::<_, i64>(0).ok() != Some(1) {
                return Err(HistoryError::Integrity("catalog metadata rows"));
            }
            let rows = tx.query(
                include_str!("../../sql/queries/history/meta.sql"),
                Vec::<Param>::new(),
            )?;
            let row = rows
                .first()
                .ok_or(HistoryError::Integrity("catalog metadata rows"))?;
            use super::rows::cell;
            let stored: Vec<u8> = cell(row, 0)?;
            let incarnation: i64 = cell(row, 1)?;
            let format: i64 = cell(row, 2)?;
            let next: i64 = cell(row, 3)?;
            let binding: Vec<u8> = cell(row, 4)?;
            let version: i64 = cell(row, 5)?;
            let source: String = cell(row, 6)?;
            let stored_definition: String = cell(row, 7)?;
            if stored != self.catalog_id.as_slice()
                || CatalogId::derive(&binding)
                    .map_err(|_| HistoryError::Integrity("catalog binding"))?
                    != self.catalog_id
            {
                return Err(HistoryError::Integrity("catalog binding"));
            }
            if format != 1 {
                return Err(HistoryError::Unsupported("catalog identity format"));
            }
            if version != 1 || source != SCHEMA {
                return Err(HistoryError::Unsupported("catalog schema version"));
            }
            if incarnation <= 0 || next <= 0 {
                return Err(HistoryError::Integrity("catalog counter range"));
            }
            let definitions = tx.query(
                include_str!("../../sql/queries/history/definition.sql"),
                definition_params(config),
            )?;
            if definitions.len() != 1 || cell::<String>(&definitions[0], 0)? != stored_definition {
                return Err(HistoryError::Integrity("catalog schema definition"));
            }
            Ok(incarnation as u64)
        })
    }
    /// Actual protocol exchanges and bytes, labelled diagnostics by callers.
    pub fn diagnostics(&self) -> HistoryResult<PgDiagnostics> {
        self.client.diagnostics().map_err(error)
    }
}
fn tables(client: &Client, config: &PgConfig) -> HistoryResult<Vec<String>> {
    client
        .query(
            include_str!("../../sql/queries/history/tables.sql"),
            vec![Param::Text(config.schema.clone())],
            false,
        )
        .map_err(error)?
        .iter()
        .map(|row| super::rows::cell(row, 0))
        .collect()
}
fn definition_params(config: &PgConfig) -> Vec<Param> {
    vec![
        Param::Text(config.schema.clone()),
        Param::Texts(TABLES.iter().map(|name| (*name).to_owned()).collect()),
    ]
}
fn definition(client: &Client, config: &PgConfig) -> HistoryResult<String> {
    let rows = client
        .query(
            include_str!("../../sql/queries/history/definition.sql"),
            definition_params(config),
            false,
        )
        .map_err(error)?;
    rows.first()
        .ok_or(HistoryError::Integrity("catalog schema definition"))
        .and_then(|row| super::rows::cell(row, 0))
}
