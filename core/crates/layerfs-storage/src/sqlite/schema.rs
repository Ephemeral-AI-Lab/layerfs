//! Exact candidate schema creation and validation.
//!
//! Create writes the shipped schema and its single policy row. Open validates the
//! schema identity, the four table shapes and the persisted policy before any
//! object work, and refuses a conflicting override. A schema that merely
//! resembles another product's version is rejected.
//!
//! The declared column lists are exact, so a version-4 Store — whose `objects`
//! row still carries `base_object_id` — fails the shape check and is refused at
//! open rather than read through a reader that would have to guess whether the
//! record or the column is authoritative.

use rusqlite::Connection;

use crate::error::{StorageError, StorageResult};
use crate::policy::{SchemaIdentity, StoragePolicy, SCHEMA_IDENTITY};
use crate::sqlite::connection::{pragma_i64, Pragma};

/// Shipped schema text of this crate.
pub const SCHEMA_SQL: &str = include_str!("../../sql/schema.sql");

const POLICY_ROW: i64 = 1;
/// A Store with no completed save has published no packs.
const NO_PACKS_PUBLISHED: i64 = 0;

/// Declared shape of one table: column names in declaration order.
///
/// **`content_signatures` is the W2 squad's table (#188d).** It is required here
/// because `Store::open` reads it back and a Store without it could not answer a
/// candidate lookup at all; it is not optional and not a floor.
const REQUIRED_TABLES: [(&str, &[&str]); 6] = [
    (
        "store_policy",
        &[
            "id",
            "format_profile",
            "small_file_threshold_bytes",
            "whole_file_delta_max_depth",
            "chunk_delta_max_depth",
            "metadata_delta_max_depth",
            "publication_sequence",
            "retained_pack_ceiling",
            "next_pack_id",
            "next_ordinal",
            "metadata_window_start",
            "metadata_window_values",
        ],
    ),
    (
        "saves",
        &["save_id", "active_slot", "publication", "pack_ceiling"],
    ),
    ("object_packs", &["pack_id", "save_id", "data"]),
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
            "save_id",
            "object_role",
            "canonical_length",
            "pack_id",
            "group_number",
            "record_number",
        ],
    ),
    (
        "content_signatures",
        &["slot", "stamp", "object_id", "signature", "save_id"],
    ),
];

/// Required secondary indexes or unique constraints, by SQLite object name.
///
/// **Empty by owner ruling C (R2, T1 #188).** The Store carried two secondary
/// indexes and neither earned its bytes: `objects_bases` had no query consumer at
/// all, and `objects_locations` served one bounded cleanup page query. A Store
/// that still has them validates — the requirement is a floor, not an equality —
/// so this change does **not** invalidate an existing Store and `SCHEMA_VERSION`
/// stays at 4. What is required is checked by [`REQUIRED_TABLES`], which is where
/// a reader's actual dependency lives.
const REQUIRED_INDEXES: [&str; 3] = ["packs_save", "objects_save", "signatures_save"];

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
          chunk_delta_max_depth, metadata_delta_max_depth, publication_sequence) \
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
         ('store_policy','object_packs','metadata_value_groups','objects',\
          'content_signatures','saves')",
        [],
        |row| row.get(0),
    )?;
    if unexpected != 0 {
        return Err(StorageError::Integrity("unexpected table in Store"));
    }
    // The retained range cannot be ahead of physical storage. Publication
    // eligibility is checked separately through the save catalog.
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
    let application_id = pragma_i64(connection, Pragma::ApplicationId)?;
    let user_version = pragma_i64(connection, Pragma::UserVersion)?;
    if application_id != expected.application_id || user_version != expected.user_version {
        return Err(StorageError::UnsupportedPolicy {
            field: "schema identity",
        });
    }
    Ok(())
}

/// Constraint text the persisted tables must carry.
///
/// The role ceiling is the one that changes without changing the column shape, so
/// a Store written by an older same-version build would otherwise open and then
/// refuse the first tree-role INSERT. Requiring the text at open makes the
/// refusal happen where the caller can see it: the Store is refused rather than
/// accepted and failed later.
const REQUIRED_CONSTRAINTS: &[(&str, &str)] =
    &[("objects", "CHECK (object_role BETWEEN 1 AND 13)")];

fn validate_table(connection: &Connection, table: &str, columns: &[&str]) -> StorageResult<()> {
    if let Some((_, constraint)) = REQUIRED_CONSTRAINTS.iter().find(|(name, _)| *name == table) {
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
        if !sql.contains(constraint) {
            return Err(StorageError::UnsupportedPolicy {
                field: "object role constraint",
            });
        }
    }
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

/// Highest published pack id; ownership filters still decide visibility below it.
pub fn retained_pack_ceiling(connection: &Connection) -> StorageResult<i64> {
    Ok(connection.query_row(
        "SELECT retained_pack_ceiling FROM store_policy WHERE id = 1",
        [],
        |row| row.get(0),
    )?)
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
