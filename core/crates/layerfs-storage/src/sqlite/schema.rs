//! Exact candidate schema creation and validation.
//!
//! Create writes the shipped schema and its single policy row. Open validates the
//! schema identity, the four table shapes, the two required indexes and the
//! persisted policy before any object work, and refuses a conflicting override.
//! A schema that merely resembles another product's version is rejected.

use rusqlite::Connection;

use crate::error::{StorageError, StorageResult};
use crate::policy::{SchemaIdentity, StoragePolicy, SCHEMA_IDENTITY};
use crate::sqlite::connection::pragma_i64;

/// Shipped schema text of this crate.
pub const SCHEMA_SQL: &str = include_str!("../../sql/schema.sql");

const POLICY_ROW: i64 = 1;
/// A Store with no completed save has published no packs.
const NO_PACKS_PUBLISHED: i64 = 0;

/// Declared shape of one table: column names in declaration order.
const REQUIRED_TABLES: [(&str, &[&str]); 4] = [
    (
        "store_policy",
        &[
            "id",
            "format_profile",
            "small_file_threshold_bytes",
            "whole_file_delta_max_depth",
            "chunk_delta_max_depth",
            "metadata_delta_max_depth",
            "retained_pack_ceiling",
        ],
    ),
    ("object_packs", &["pack_id", "data"]),
    (
        "metadata_value_groups",
        &[
            "first_ordinal",
            "count",
            "pack_id",
            "group_number",
            "digest",
        ],
    ),
    (
        "objects",
        &[
            "object_id",
            "object_role",
            "canonical_length",
            "base_object_id",
            "pack_id",
            "group_number",
            "record_number",
        ],
    ),
];

/// Required secondary indexes or unique constraints, by SQLite object name.
const REQUIRED_INDEXES: [&str; 2] = ["objects_locations", "objects_bases"];

/// Creates a fresh Store with `policy` and returns the stored policy.
pub fn create(connection: &Connection, policy: StoragePolicy) -> StorageResult<StoragePolicy> {
    let policy = policy.validated()?;
    if existing_object_count(connection)? != 0 {
        return Err(StorageError::Integrity("Store is not empty"));
    }
    connection.execute_batch(SCHEMA_SQL)?;
    connection.execute(
        "INSERT INTO store_policy \
         (id, format_profile, small_file_threshold_bytes, whole_file_delta_max_depth, \
          chunk_delta_max_depth, metadata_delta_max_depth, retained_pack_ceiling) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            POLICY_ROW,
            policy.format_profile(),
            policy.small_file_threshold_bytes() as i64,
            policy.whole_file_delta_max_depth(),
            policy.chunk_delta_max_depth(),
            policy.metadata_delta_max_depth(),
            NO_PACKS_PUBLISHED,
        ],
    )?;
    validate(connection, Some(policy))
}

/// Validates the schema, indexes and policy row; rejects a conflicting override.
pub fn validate(
    connection: &Connection,
    expected: Option<StoragePolicy>,
) -> StorageResult<StoragePolicy> {
    identity(connection, SCHEMA_IDENTITY)?;
    for (table, columns) in REQUIRED_TABLES {
        validate_table(connection, table, columns)?;
    }
    for index in REQUIRED_INDEXES {
        let present: i64 = connection.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = ?1 AND type = 'index'",
            [index],
            |row| row.get(0),
        )?;
        if present != 1 {
            return Err(StorageError::Integrity("required index is missing"));
        }
    }
    let unexpected: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master \
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT IN \
         ('store_policy','object_packs','metadata_value_groups','objects')",
        [],
        |row| row.get(0),
    )?;
    if unexpected != 0 {
        return Err(StorageError::Integrity("unexpected table in Store"));
    }
    // I1: the watermark can never be ahead of storage. A Store whose watermark
    // exceeds its highest pack id has been corrupted or written out of band.
    let ceiling = retained_pack_ceiling(connection)?;
    let highest = crate::sqlite::lookup::highest_pack_id(connection)?;
    if ceiling > highest {
        return Err(StorageError::Integrity(
            "publication watermark is ahead of storage",
        ));
    }
    let stored = load_policy(connection)?;
    if let Some(expected) = expected {
        if expected.validated()? != stored {
            return Err(StorageError::UnsupportedPolicy {
                field: "store_policy row",
            });
        }
    }
    Ok(stored)
}

/// Checks the persisted schema identity exactly.
pub fn identity(connection: &Connection, expected: SchemaIdentity) -> StorageResult<()> {
    let application_id = pragma_i64(connection, "application_id")?;
    let user_version = pragma_i64(connection, "user_version")?;
    if application_id != expected.application_id || user_version != expected.user_version {
        return Err(StorageError::UnsupportedPolicy {
            field: "schema identity",
        });
    }
    Ok(())
}

fn validate_table(connection: &Connection, table: &str, columns: &[&str]) -> StorageResult<()> {
    let sql: Option<String> = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |row| row.get(0),
        )
        .map(Some)
        .or_else(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    let sql = sql.ok_or(StorageError::Integrity("required table is missing"))?;
    if !sql.to_ascii_uppercase().contains("STRICT") {
        return Err(StorageError::Integrity("table is not STRICT"));
    }
    let declared: Vec<String> = connection
        .prepare(&format!("PRAGMA table_info({table})"))?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<_, _>>()?;
    if declared.len() != columns.len()
        || declared
            .iter()
            .zip(columns)
            .any(|(declared, required)| declared != required)
    {
        return Err(StorageError::Integrity("table column shape"));
    }
    Ok(())
}

fn load_policy(connection: &Connection) -> StorageResult<StoragePolicy> {
    let row = connection
        .query_row(
            "SELECT format_profile, small_file_threshold_bytes, \
             whole_file_delta_max_depth, chunk_delta_max_depth, \
             metadata_delta_max_depth \
             FROM store_policy WHERE id = ?1",
            [POLICY_ROW],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => {
                StorageError::Integrity("policy row is missing")
            }
            other => StorageError::Engine(other),
        })?;
    let profile = u8::try_from(row.0).map_err(|_| StorageError::UnsupportedPolicy {
        field: "format_profile",
    })?;
    let threshold = u64::try_from(row.1).map_err(|_| StorageError::UnsupportedPolicy {
        field: "small_file_threshold_bytes",
    })?;
    let whole = u8::try_from(row.2).map_err(|_| StorageError::UnsupportedPolicy {
        field: "whole_file_delta_max_depth",
    })?;
    let chunk = u8::try_from(row.3).map_err(|_| StorageError::UnsupportedPolicy {
        field: "chunk_delta_max_depth",
    })?;
    let metadata = u8::try_from(row.4).map_err(|_| StorageError::UnsupportedPolicy {
        field: "metadata_delta_max_depth",
    })?;
    StoragePolicy::new(profile, threshold, whole, chunk)
        .with_metadata_depth(metadata)
        .validated()
}

/// Highest pack id belonging to a completed save.
pub fn retained_pack_ceiling(connection: &Connection) -> StorageResult<i64> {
    connection
        .query_row(
            "SELECT retained_pack_ceiling FROM store_policy WHERE id = ?1",
            [POLICY_ROW],
            |row| row.get(0),
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => {
                StorageError::Integrity("policy row is missing")
            }
            other => StorageError::Engine(other),
        })
}

/// Advances the publication watermark inside the caller's open transaction.
///
/// The caller must hold write ownership; the caller also decides whether the
/// advance shares the transaction that publishes the packs it names.
pub fn advance_retained_pack_ceiling(connection: &Connection, ceiling: i64) -> StorageResult<()> {
    let affected = connection.execute(
        "UPDATE store_policy SET retained_pack_ceiling = ?2 \
         WHERE id = ?1 AND retained_pack_ceiling < ?2",
        rusqlite::params![POLICY_ROW, ceiling],
    )?;
    if affected > 1 {
        return Err(StorageError::Integrity("watermark update cardinality"));
    }
    Ok(())
}

/// Number of stored object rows; used to reject a non-empty create target.
pub fn existing_object_count(connection: &Connection) -> StorageResult<i64> {
    let tables: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'objects'",
        [],
        |row| row.get(0),
    )?;
    if tables == 0 {
        return Ok(0);
    }
    Ok(connection.query_row("SELECT COUNT(*) FROM objects", [], |row| row.get(0))?)
}
