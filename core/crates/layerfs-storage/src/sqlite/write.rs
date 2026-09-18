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
    /// Direct delta base this record was selected against, when it has one.
    ///
    /// Absent for a FULL record and for a pooled leaf stored as FULL; present for
    /// a PREFIX/DELTA record, whose base the owner records here so the dependency
    /// edge is visible to readers and to cleanup. The earlier doc comment said
    /// "always absent in this slice", which stopped being true when delta
    /// selection landed.
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

/// Columns one object row binds.
const OBJECT_INSERT_PARAMETERS: usize = 7;
/// SQL text one bound row contributes to a multi-row `INSERT`: its placeholder
/// group `(?,?,?,?,?,?,?)` and the separating comma.
const OBJECT_INSERT_ROW_SQL_BYTES: usize = 16;
/// Most rows this writer will ever put in one statement.
///
/// Not a tuning constant: a fixed cap keeps the prepared-statement cache bounded
/// (one entry per distinct chunk size) and keeps one statement's work within a
/// bound the caller can reason about. The engine's own limits decide the rest.
const OBJECT_INSERT_CHUNK_CAP: usize = 128;

/// Rows one multi-row `INSERT` may carry, derived from the engine's own limits.
///
/// `k = min(SQLITE_LIMIT_VARIABLE_NUMBER / columns, SQLITE_LIMIT_SQL_LENGTH /
/// row_sql_bytes, 128)` - read back from the connection, never hardcoded, so the
/// chunk follows the SQLite this build actually links rather than a figure copied
/// from somewhere else. The result is at least one row: a connection whose limits
/// could not hold a single row would be a broken engine, and this writer would
/// rather fail on that statement than silently fall back to a different shape.
pub fn insert_chunk_rows(connection: &Connection) -> StorageResult<usize> {
    let variables = connection.limit(rusqlite::limits::Limit::SQLITE_LIMIT_VARIABLE_NUMBER)?;
    let sql_length = connection.limit(rusqlite::limits::Limit::SQLITE_LIMIT_SQL_LENGTH)?;
    let by_variables = usize::try_from(variables)
        .unwrap_or(1)
        .checked_div(OBJECT_INSERT_PARAMETERS)
        .unwrap_or(0)
        .max(1);
    let by_sql = usize::try_from(sql_length)
        .unwrap_or(1)
        .checked_div(OBJECT_INSERT_ROW_SQL_BYTES)
        .unwrap_or(0)
        .max(1);
    Ok(by_variables.min(by_sql).min(OBJECT_INSERT_CHUNK_CAP))
}

/// Inserts a group's object rows in as few statements as the engine allows.
///
/// Returns the number of SQL statements issued, which the caller charges to its
/// statement counter - the counter is charged where the statement is issued, not
/// inferred from the row count. One multi-row `INSERT` is **one** statement, and
/// SQLite applies it atomically: every row of a chunk lands or none does, and a
/// failure is the same engine or integrity error the single-row form produced.
pub fn insert_objects(connection: &Connection, rows: &[ObjectRow]) -> StorageResult<u64> {
    if rows.is_empty() {
        return Ok(0);
    }
    let chunk = insert_chunk_rows(connection)?;
    let mut statements = 0_u64;
    for page in rows.chunks(chunk) {
        let sql = object_insert_sql(page.len());
        let mut parameters: Vec<Value> = Vec::with_capacity(page.len() * OBJECT_INSERT_PARAMETERS);
        for row in page {
            parameters.push(Value::Blob(row.object_id.to_bytes().to_vec()));
            parameters.push(Value::Integer(i64::from(row.role)));
            parameters.push(Value::Integer(row.canonical_length as i64));
            parameters.push(match row.base_object_id {
                Some(id) => Value::Blob(id.to_bytes().to_vec()),
                None => Value::Null,
            });
            parameters.push(Value::Integer(row.pack_id));
            parameters.push(Value::Integer(row.group_number as i64));
            parameters.push(Value::Integer(row.record_number as i64));
        }
        let affected = connection
            .prepare_cached(&sql)?
            .execute(rusqlite::params_from_iter(parameters))?;
        if affected != page.len() {
            return Err(StorageError::Integrity("object insert cardinality"));
        }
        statements = statements.saturating_add(1);
    }
    Ok(statements)
}

/// The `INSERT` text for `rows` bound rows.
///
/// The shape is constant; only the number of placeholder groups changes, so the
/// prepared-statement cache holds one entry per chunk size it is asked for and
/// every chunk after the first is a cache hit.
fn object_insert_sql(rows: usize) -> String {
    let mut sql = String::from(
        "INSERT INTO objects \
         (object_id, object_role, canonical_length, base_object_id, pack_id, group_number, record_number) VALUES ",
    );
    for index in 0..rows {
        if index > 0 {
            sql.push(',');
        }
        sql.push('(');
        for column in 0..OBJECT_INSERT_PARAMETERS {
            if column > 0 {
                sql.push(',');
            }
            sql.push('?');
        }
        sql.push(')');
    }
    sql
}
