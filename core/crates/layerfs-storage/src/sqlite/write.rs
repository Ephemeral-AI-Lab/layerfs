//! Batched bindings, the lazily started shared write transaction and in-place
//! pack writes.
//!
//! One transaction spans as many preparation batches and files as its declared
//! row and byte bounds allow. `BEGIN IMMEDIATE` is attempted exactly once: a lost
//! write lock is an immediate failure, never a wait. A failed `COMMIT` leaves the
//! persistence outcome unproven and is reported as such.
//!
//! **A pack row is written through incremental BLOB I/O.** A pack's directory
//! region is reserved by the format and its assembled length is declared in its
//! own control area (`pack::layout`), so an append adds bytes without moving any
//! byte already written. The alternative - binding the reassembled pack to
//! `UPDATE object_packs SET data = ?2` - rewrites the row's whole BLOB for every
//! append, and a small companion column would not help: any `UPDATE` of a row
//! holding a 256 KiB BLOB rebuilds and rewrites that BLOB, measured at ~72 us per
//! statement on a 256 KiB row against ~11 us for a four-byte in-place BLOB write
//! (`#219`). The declaration therefore rides in the BLOB. The sole SQL BLOB
//! rewrite is the final mostly empty pooled row's bounded shrink at Save finish,
//! after its last append and before publication, to make sparse Saves reuse
//! freed pages without rewriting a mostly used row.

use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension};

use layerfs_content::ObjectId;

use crate::error::{StorageError, StorageResult};
use crate::pack::layout::HEADER_LEN;
use crate::pack::SelectedWrite;
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

/// Inserts a newly created pack row and writes its first content in place.
///
/// The row is created zero-filled at the pack's capacity and then written through
/// the BLOB handle, so the allocation and the content are one statement plus one
/// set of in-place writes: the pack is never built as a whole in memory, and the
/// pages the write does not touch are never dirtied.
pub fn insert_pack(
    connection: &Connection,
    pack_id: i64,
    write: &SelectedWrite,
) -> StorageResult<()> {
    let capacity =
        i64::try_from(write.capacity).map_err(|_| StorageError::Integrity("pack capacity"))?;
    let affected = connection.execute(
        "INSERT INTO object_packs (pack_id, data, save_id) VALUES (?1, zeroblob(?2), (SELECT save_id FROM temp.layerfs_read_scope))",
        rusqlite::params![pack_id, capacity],
    )?;
    if affected != 1 {
        return Err(StorageError::Integrity("pack insert cardinality"));
    }
    write_in_place(connection, pack_id, write)
}

/// Shrinks a mostly empty final pooled row before its publication transaction
/// commits. Earlier appends used the full BLOB; no later append may need its tail.
pub fn shrink_final_pooled_pack(
    connection: &Connection,
    pack_id: i64,
    used: usize,
) -> StorageResult<()> {
    let limit = crate::policy::PACK_LIMIT;
    if pack_id <= 0
        || used <= crate::pack::layout::body_area_offset(crate::pack::PackLane::PooledMetadata)
        || used > limit
    {
        return Err(StorageError::Integrity("final pooled pack length"));
    }
    if used > limit / 2 {
        return Ok(());
    }
    let used_bytes = (used as u32).to_le_bytes();
    let version_bytes = crate::pack::layout::VERSION_POOLED_METADATA.to_le_bytes();
    let affected = connection.execute(
        "UPDATE object_packs SET data = substr(data, 1, ?2) \
         WHERE pack_id = ?1 AND save_id = (SELECT save_id FROM temp.layerfs_read_scope) \
         AND EXISTS (SELECT 1 FROM saves WHERE saves.save_id = object_packs.save_id \
                     AND active_slot IS NOT NULL AND publication IS NULL) \
         AND length(data) = ?3 AND substr(data, 17, 4) = ?4 \
         AND substr(data, 9, 4) = ?5",
        rusqlite::params![
            pack_id,
            used as i64,
            limit as i64,
            used_bytes.as_slice(),
            version_bytes.as_slice()
        ],
    )?;
    if affected != 1 {
        return Err(StorageError::Integrity("final pooled pack cardinality"));
    }
    Ok(())
}

/// Appends to an existing pack row without rewriting it.
///
/// The ownership predicate is the one the rewriting form carried in its `WHERE`
/// clause - this save's scope, and a save that is not yet published - stated as a
/// read that touches only the row's own small columns. It runs before any byte is
/// written, so a pack this save does not own is refused rather than written into
/// and rolled back, and it never modifies the row: an `UPDATE` of any column
/// would rewrite the whole BLOB, which is exactly the cost this path removes.
pub fn append_pack(
    connection: &Connection,
    pack_id: i64,
    write: &SelectedWrite,
) -> StorageResult<()> {
    let owned: Option<i64> = connection
        .query_row(
            "SELECT pack_id FROM object_packs WHERE pack_id = ?1 AND save_id = (SELECT save_id FROM temp.layerfs_read_scope) AND EXISTS (SELECT 1 FROM saves WHERE saves.save_id = object_packs.save_id AND publication IS NULL)",
            rusqlite::params![pack_id],
            |row| row.get(0),
        )
        .optional()?;
    if owned != Some(pack_id) {
        return Err(StorageError::Integrity("pack update cardinality"));
    }
    write_in_place(connection, pack_id, write)
}

/// Writes one increment into an existing pack BLOB, in place.
///
/// Three writes at most - the bodies at the pack's previous assembled length, the
/// new directory entries in the reserved region, and the control area last, so a
/// pack is never momentarily described as longer than the bytes that are in it.
/// Each write dirties only the pages it covers.
fn write_in_place(
    connection: &Connection,
    pack_id: i64,
    write: &SelectedWrite,
) -> StorageResult<()> {
    let mut blob = connection
        .blob_open("main", "object_packs", "data", pack_id, false)
        .map_err(|error| match error {
            rusqlite::Error::SqliteFailure(_, _) => StorageError::Integrity("pack blob handle"),
            other => StorageError::Engine(other),
        })?;
    let capacity = blob.len();
    if capacity != write.capacity || capacity < HEADER_LEN {
        return Err(StorageError::Integrity("pack capacity"));
    }
    for (offset, bytes) in [
        (write.body_offset, write.bodies.as_slice()),
        (write.directory_offset, write.directory.as_slice()),
        (0, write.control.as_slice()),
    ] {
        if bytes.is_empty() {
            continue;
        }
        let end = offset
            .checked_add(bytes.len())
            .ok_or(StorageError::Integrity("pack write extent"))?;
        if end > capacity {
            return Err(StorageError::Integrity("pack write extent"));
        }
        blob.write_at(bytes, offset).map_err(StorageError::Engine)?;
    }
    blob.close().map_err(StorageError::Engine)
}

/// Columns one object row binds.
///
/// Six, not seven: the direct base identity is a property of the packed record
/// and is not a column, so the writer binds nothing for it.
const OBJECT_INSERT_PARAMETERS: usize = 6;
/// SQL text one bound row contributes to a multi-row `INSERT`: its placeholder
/// group `(?,?,?,?,?,?)` and the separating comma.
const OBJECT_INSERT_ROW_SQL_BYTES: usize =
    b"(?,?,?,?,?,?,(SELECT save_id FROM temp.layerfs_read_scope)),".len();
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
         (object_id, object_role, canonical_length, pack_id, group_number, record_number, save_id) VALUES ",
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
        sql.push_str(",(SELECT save_id FROM temp.layerfs_read_scope))");
    }
    sql
}
