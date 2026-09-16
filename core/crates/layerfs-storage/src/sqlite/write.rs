//! Batched bindings and the lazily started shared write transaction.
//!
//! One transaction spans as many preparation batches and files as its declared
//! row and byte bounds allow. `BEGIN IMMEDIATE` is attempted exactly once: a lost
//! write lock is an immediate failure, never a wait. A failed `COMMIT` leaves the
//! persistence outcome unproven and is reported as such.

use rusqlite::types::Value;
use rusqlite::Connection;

use layerfs_content::ObjectId;

use crate::error::{StorageError, StorageResult};
use crate::sqlite::connection::ownership_error;

/// Accumulated work inside the open transaction.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TransactionState {
    /// Rows submitted since the transaction started.
    pub rows: u64,
    /// Canonical bytes submitted since the transaction started.
    pub bytes: u64,
}

/// One object row to insert.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectRow {
    /// Canonical identity.
    pub object_id: ObjectId,
    /// Logical role code.
    pub role: u8,
    /// Canonical object length.
    pub canonical_length: usize,
    /// Direct delta base, always absent in this slice.
    pub base_object_id: Option<ObjectId>,
    /// Pack holding the record.
    pub pack_id: i64,
    /// Group ordinal inside the pack.
    pub group_number: usize,
    /// Record ordinal inside the group.
    pub record_number: usize,
}

/// Attempts the single exclusive acquisition of this operation.
pub fn begin_immediate(connection: &Connection) -> StorageResult<()> {
    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(ownership_error)
}

/// Acknowledges the open transaction; a failure leaves the outcome unproven.
pub fn commit(connection: &Connection) -> StorageResult<()> {
    connection
        .execute_batch("COMMIT")
        .map_err(|error| StorageError::UnknownOutcome {
            original: Box::new(StorageError::Engine(error)),
        })
}

/// Rolls the open transaction back; a failure leaves the outcome unproven.
pub fn rollback(connection: &Connection) -> StorageResult<()> {
    connection
        .execute_batch("ROLLBACK")
        .map_err(|error| StorageError::UnknownOutcome {
            original: Box::new(StorageError::Engine(error)),
        })
}

/// Inserts a newly created pack row.
pub fn insert_pack(connection: &Connection, pack_id: i64, bytes: &[u8]) -> StorageResult<()> {
    let affected = connection.execute(
        "INSERT INTO object_packs (pack_id, data) VALUES (?1, ?2)",
        rusqlite::params![pack_id, bytes],
    )?;
    if affected != 1 {
        return Err(StorageError::Integrity("pack insert cardinality"));
    }
    Ok(())
}

/// Rewrites an existing pack row with its appended groups.
pub fn append_pack(connection: &Connection, pack_id: i64, bytes: &[u8]) -> StorageResult<()> {
    let affected = connection.execute(
        "UPDATE object_packs SET data = ?2 WHERE pack_id = ?1",
        rusqlite::params![pack_id, bytes],
    )?;
    if affected != 1 {
        return Err(StorageError::Integrity("pack update cardinality"));
    }
    Ok(())
}

/// Inserts one object row inside the open transaction.
pub fn insert_object(connection: &Connection, row: &ObjectRow) -> StorageResult<()> {
    let base: Value = match row.base_object_id {
        Some(id) => Value::Blob(id.to_bytes().to_vec()),
        None => Value::Null,
    };
    let affected = connection.execute(
        "INSERT INTO objects \
         (object_id, object_role, canonical_length, base_object_id, pack_id, group_number, record_number) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            row.object_id.to_bytes().to_vec(),
            i64::from(row.role),
            row.canonical_length as i64,
            base,
            row.pack_id,
            row.group_number as i64,
            row.record_number as i64,
        ],
    )?;
    if affected != 1 {
        return Err(StorageError::Integrity("object insert cardinality"));
    }
    Ok(())
}
