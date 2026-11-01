//! The PostgreSQL Store schema in one named schema, and its validation.
//!
//! Tables mirror the SQLite Store's logical rows (same names and columns)
//! with these physical differences: integers are `bigint`, BLOBs `bytea`;
//! history names collate `"C"` so ordering and paging match SQLite's byte
//! order; pack bodies are whole `bytea` values stored `EXTERNAL`
//! (uncompressed, out of line) for ranged `substring` reads; `store_lane`
//! holds the one row writers lock for the publication lane. The pack-unit
//! tables are absent: group-row layouts are SQLite physical choices. A
//! schema made by [`create`] also has Init's acquisition tables (#533 step
//! 10b, `acquisition.sql`), recorded in its schema source; one imported
//! before them has none and refuses acquisition. A merge's records live in
//! the writer connection's temporary tables (`merge_records.rs`), not here.
use super::session::transaction::Transaction;
use crate::backend::records::{BackendError, Param};
use layerfs_history::{CatalogId, HistoryCatalogConfig};
use layerfs_storage::{port::PersistenceError, StoragePolicy};

pub(crate) const SCHEMA: &str = include_str!("../../../sql/postgresql/schema.sql");
/// Init's acquisition tables, created after [`SCHEMA`] in a new schema.
const ACQUISITION: &str = include_str!("../../../sql/postgresql/acquisition.sql");
/// The acquisition tables, in name order.
const ACQUISITION_TABLES: [&str; 3] = ["init_entry", "init_host_file", "init_operation"];
/// The engine-scoped schema version in `store_policy` and `history_meta`.
pub(crate) const SCHEMA_VERSION: i64 = 1;
/// Every table of one Store schema.
pub(crate) const TABLES: [&str; 19] = [
    "absorbed",
    "binding_hint",
    "branch",
    "commit",
    "commit_lineage",
    "content_signature",
    "file_digest",
    "history_meta",
    "inode_value",
    "layer",
    "layer_lineage",
    "layer_stack",
    "object_location",
    "pack",
    "reach",
    "scope_allocator",
    "store_lane",
    "store_policy",
    "store_totals",
];

/// The live column definition of the current schema, in one deterministic text.
pub(crate) fn definition(tx: &Transaction<'_>) -> Result<String, BackendError> {
    let rows = tx.query(
        "SELECT table_name::text, column_name::text, data_type::text, is_nullable::text \
         FROM information_schema.columns WHERE table_schema = current_schema() \
         ORDER BY table_name, ordinal_position",
        vec![],
    )?;
    let mut text = String::new();
    for row in rows {
        for index in 0..4 {
            text.push_str(&row.get::<String>(index)?);
            text.push(if index == 3 { '\n' } else { ' ' });
        }
    }
    Ok(text)
}

/// The recorded schema source: the Store tables, then the acquisition tables
/// when the schema has them.
fn source(acquisition: bool) -> String {
    match acquisition {
        true => format!("{SCHEMA}{ACQUISITION}"),
        false => SCHEMA.to_owned(),
    }
}

/// Creates the new schema `tx.schema` with the Store and acquisition tables,
/// its policy row and its catalog row, inside the caller's transaction. A
/// schema of that name that already exists is refused before anything is
/// written; nothing is merged into it.
pub(crate) fn create(
    tx: &Transaction<'_>,
    policy: StoragePolicy,
    history: &HistoryCatalogConfig,
    catalog_id: CatalogId,
) -> Result<(), PersistenceError> {
    let name = tx.schema;
    let found = tx.query(
        "SELECT 1 FROM pg_namespace WHERE nspname=$1",
        vec![Param::Text(name.to_owned())],
    )?;
    if !found.is_empty() {
        return Err(PersistenceError::Refused {
            status: "schema exists".to_owned(),
        });
    }
    tx.batch(&format!(
        "CREATE SCHEMA {name}; SET LOCAL search_path TO {name}; {SCHEMA}{ACQUISITION}"
    ))?;
    tx.query(
        "INSERT INTO store_policy(id,schema_version,format_profile,small_file_threshold_bytes,whole_file_delta_max_depth,chunk_delta_max_depth,metadata_delta_max_depth) VALUES(1,$1,$2,$3,$4,$5,$6)",
        vec![
            Param::I64(SCHEMA_VERSION),
            Param::I64(i64::from(policy.format_profile())),
            Param::I64(policy.small_file_threshold_bytes() as i64),
            Param::I64(i64::from(policy.whole_file_delta_max_depth())),
            Param::I64(i64::from(policy.chunk_delta_max_depth())),
            Param::I64(i64::from(policy.metadata_delta_max_depth())),
        ],
    )?;
    let definition = definition(tx)?;
    tx.query(
        "INSERT INTO history_meta(id,catalog_id,catalog_incarnation,binding_key,identity_format,schema_version,schema_source,schema_definition) VALUES(1,$1,$2,$3,3,$4,$5,$6)",
        vec![
            Param::Bytes(catalog_id.as_slice().to_vec()),
            Param::I64(history.incarnation as i64),
            Param::Bytes(history.binding_key.clone()),
            Param::I64(SCHEMA_VERSION),
            Param::Text(source(true)),
            Param::Text(definition),
        ],
    )?;
    Ok(())
}

/// The catalog's incarnation, and whether the schema has the acquisition
/// tables, once the schema, its recorded source and definition, and the
/// binding all match.
pub(crate) fn validate(
    tx: &Transaction<'_>,
    catalog_id: layerfs_history::CatalogId,
    binding: &[u8],
) -> Result<(u64, bool), BackendError> {
    let acquisition = tables(tx)?;
    let rows = tx.query(
        "SELECT catalog_id,catalog_incarnation,binding_key,identity_format,schema_version,schema_source,schema_definition FROM history_meta WHERE id=1",
        vec![],
    )?;
    let r = rows
        .first()
        .filter(|_| rows.len() == 1)
        .ok_or(BackendError::Integrity)?;
    let incarnation: i64 = r.get(1)?;
    if r.get::<Vec<u8>>(0)? != catalog_id.as_slice()
        || r.get::<Vec<u8>>(2)? != binding
        || incarnation <= 0
        || r.get::<i64>(3)? != 3
        || r.get::<i64>(4)? != SCHEMA_VERSION
        || r.get::<String>(5)? != source(acquisition)
        || r.get::<String>(6)? != definition(tx)?
    {
        return Err(BackendError::Integrity);
    }
    Ok((incarnation as u64, acquisition))
}

/// Records this engine's schema after rows were imported from another engine.
pub(crate) fn record(tx: &Transaction<'_>) -> Result<(), BackendError> {
    let definition = definition(tx)?;
    tx.query(
        "UPDATE history_meta SET schema_version=$1,schema_source=$2,schema_definition=$3 WHERE id=1",
        vec![
            Param::I64(SCHEMA_VERSION),
            Param::Text(SCHEMA.to_owned()),
            Param::Text(definition),
        ],
    )?;
    Ok(())
}

/// Refuses a schema whose tables are not exactly a LayerFS Store's, with or
/// without the acquisition tables; returns whether it has them.
fn tables(tx: &Transaction<'_>) -> Result<bool, BackendError> {
    let tables = tx.query(
        "SELECT table_name::text FROM information_schema.tables \
         WHERE table_schema = current_schema() AND table_type = 'BASE TABLE' ORDER BY table_name",
        vec![],
    )?;
    let actual: Vec<String> = tables.iter().map(|r| r.get(0)).collect::<Result<_, _>>()?;
    if actual == TABLES {
        return Ok(false);
    }
    let mut with = [TABLES.as_slice(), ACQUISITION_TABLES.as_slice()].concat();
    with.sort_unstable();
    match actual == with {
        true => Ok(true),
        false => Err(BackendError::Integrity),
    }
}

/// The binding key recorded in a LayerFS schema; opening checks the rest.
pub(crate) fn binding(tx: &Transaction<'_>) -> Result<Vec<u8>, BackendError> {
    tables(tx)?;
    let rows = tx.query("SELECT binding_key FROM history_meta WHERE id=1", vec![])?;
    match rows.as_slice() {
        [row] => row.get(0),
        _ => Err(BackendError::Integrity),
    }
}
