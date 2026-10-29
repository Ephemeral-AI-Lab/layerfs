//! The PostgreSQL Store schema in one named schema, and its validation.
//!
//! Tables mirror the SQLite Store's logical rows (same names and columns)
//! with these physical differences: integers are `bigint`, BLOBs `bytea`;
//! history names collate `"C"` so ordering and paging match SQLite's byte
//! order; pack bodies are whole `bytea` values stored `EXTERNAL`
//! (uncompressed, out of line) for ranged `substring` reads; `store_lane`
//! holds the one row writers lock for the publication lane. The pack-unit,
//! merge-record and acquisition tables are absent: group-row layouts are
//! SQLite physical choices, merges and Init stay SQLite-only in this phase.
use super::session::transaction::Transaction;
use crate::backend::records::BackendError;

pub(crate) const SCHEMA: &str = include_str!("../../../sql/postgresql/schema.sql");
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

/// The catalog's incarnation once the schema, its recorded source and
/// definition, and the binding all match.
pub(crate) fn validate(
    tx: &Transaction<'_>,
    catalog_id: layerfs_history::CatalogId,
    binding: &[u8],
) -> Result<u64, BackendError> {
    let tables = tx.query(
        "SELECT table_name::text FROM information_schema.tables \
         WHERE table_schema = current_schema() AND table_type = 'BASE TABLE' ORDER BY table_name",
        vec![],
    )?;
    let actual: Vec<String> = tables.iter().map(|r| r.get(0)).collect::<Result<_, _>>()?;
    if actual != TABLES {
        return Err(BackendError::Integrity);
    }
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
        || r.get::<String>(5)? != SCHEMA
        || r.get::<String>(6)? != definition(tx)?
    {
        return Err(BackendError::Integrity);
    }
    Ok(incarnation as u64)
}
