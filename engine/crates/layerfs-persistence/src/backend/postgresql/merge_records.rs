//! Merge build records on a PostgreSQL Store (#506 d): the statements of
//! `backend/sqlite/storage/merge_records.rs` over temporary tables of the
//! writer's connection.
//!
//! The tables are `pg_temp` tables, so they are outside the Store's schema
//! (whose tables are checked exactly at open), are seen by this connection
//! only, and go with it: a process that ends mid-merge leaves nothing, and
//! no lease sweep is needed. Every write runs as a `Stage` transaction,
//! which never takes the publication lane. Owner and token checks match the
//! SQLite engine's, so a removed owner is never read as an empty merge.
use super::session::{
    connection::Session,
    rows,
    transaction::{Mode, Transaction},
};
use crate::backend::records::{BackendError, Param};
use crate::backend::sqlite::storage::merge_records::Window;
use layerfs_content::EditRecordKey;
use layerfs_storage::port::PersistenceError;
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

const TABLES: &str = "CREATE TEMP TABLE IF NOT EXISTS merge_operation (
    operation bigserial PRIMARY KEY,
    token bytea NOT NULL CHECK (octet_length(token) = 16),
    touched bigint NOT NULL CHECK (touched >= 0));
  CREATE TEMP TABLE IF NOT EXISTS merge_record (
    operation bigint NOT NULL,
    kind bigint NOT NULL CHECK (kind >= 0),
    key bytea NOT NULL CHECK (octet_length(key) = 32),
    value bytea NOT NULL,
    PRIMARY KEY (operation, kind, key))";
const OWNER: &str = "SELECT 1 FROM pg_temp.merge_operation WHERE operation=$1 AND token=$2";
const KEYS_AFTER: &str =
    "SELECT r.key FROM pg_temp.merge_operation o LEFT JOIN pg_temp.merge_record r
    ON r.operation=o.operation AND r.kind=$3 AND r.key>$4
    WHERE o.operation=$1 AND o.token=$2 ORDER BY r.key LIMIT 64";
const KEYS_EXCLUDING: &str =
    "SELECT r.key FROM pg_temp.merge_operation o LEFT JOIN pg_temp.merge_record r
    ON r.operation=o.operation AND r.kind=$3 AND r.key<>$4
    WHERE o.operation=$1 AND o.token=$2 ORDER BY r.key LIMIT 64";
const STORED: &str = "SELECT k.i, r.value FROM pg_temp.merge_operation o
    CROSS JOIN unnest($3::bigint[], $4::bytea[]) WITH ORDINALITY AS k(kind, key, i)
    LEFT JOIN pg_temp.merge_record r ON r.operation=o.operation AND r.kind=k.kind AND r.key=k.key
    WHERE o.operation=$1 AND o.token=$2 ORDER BY k.i";
const PUT: &str = "INSERT INTO pg_temp.merge_record(operation,kind,key,value)
    SELECT $1, k, y, v FROM unnest($2::bigint[], $3::bytea[], $4::bytea[]) AS t(k, y, v)
    ON CONFLICT (operation,kind,key) DO UPDATE SET value=excluded.value";
const DELETE: &str = "DELETE FROM pg_temp.merge_record WHERE operation=$1
    AND (kind, key) IN (SELECT * FROM unnest($2::bigint[], $3::bytea[]))";
/// Records one removal job deletes, as on SQLite.
const REMOVE_ROWS: i64 = 4096;
const REMOVE: &str = "WITH gone AS (DELETE FROM pg_temp.merge_record WHERE operation=$1
    AND (kind, key) IN (SELECT kind, key FROM pg_temp.merge_record WHERE operation=$1
    ORDER BY kind, key LIMIT $2) RETURNING 1) SELECT count(*)::bigint FROM gone";

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

fn owned(tx: &Transaction<'_>, operation: i64, token: &[u8]) -> Result<(), PersistenceError> {
    let rows = tx.query(
        OWNER,
        vec![Param::I64(operation), Param::Bytes(token.to_vec())],
    )?;
    if rows.is_empty() {
        return Err(PersistenceError::Missing);
    }
    Ok(())
}

/// Registers one merge's owner, making this connection's tables first (a
/// committed transaction of their own, so a later failure keeps them).
pub(crate) fn register(session: &Session) -> Result<(i64, Vec<u8>), PersistenceError> {
    session.run_as(Mode::Stage, |tx| {
        tx.batch(TABLES)?;
        Ok::<_, PersistenceError>(())
    })?;
    session.run_as(Mode::Stage, |tx| {
        let rows = tx.query(
            "INSERT INTO pg_temp.merge_operation(token,touched)
             VALUES(decode(replace(gen_random_uuid()::text,'-',''),'hex'),$1)
             RETURNING operation,token",
            vec![Param::I64(now_ms())],
        )?;
        let row = rows.first().ok_or(BackendError::Integrity)?;
        Ok::<_, PersistenceError>((row.get::<i64>(0)?, row.get::<Vec<u8>>(1)?))
    })
}

/// One removal window, then the owner once none is left.
pub(crate) fn remove(
    session: &Session,
    operation: i64,
    token: &[u8],
) -> Result<i64, PersistenceError> {
    session.run_as(Mode::Stage, |tx| {
        owned(tx, operation, token)?;
        let rows = tx.query(REMOVE, vec![Param::I64(operation), Param::I64(REMOVE_ROWS)])?;
        let removed = rows.first().ok_or(BackendError::Integrity)?.get::<i64>(0)?;
        if removed == 0 {
            tx.query(
                "DELETE FROM pg_temp.merge_operation WHERE operation=$1",
                vec![Param::I64(operation)],
            )?;
        }
        Ok::<_, PersistenceError>(removed)
    })
}

pub(crate) fn refresh(
    session: &Session,
    operation: i64,
    token: &[u8],
) -> Result<(), PersistenceError> {
    session.run_as(Mode::Stage, |tx| {
        owned(tx, operation, token)?;
        tx.query(
            "UPDATE pg_temp.merge_operation SET touched=$2 WHERE operation=$1",
            vec![Param::I64(operation), Param::I64(now_ms())],
        )?;
        Ok::<_, PersistenceError>(())
    })
}

/// One owner-checked key window: no rows when the owner is gone, one NULL
/// row when it has no keys. Temporary tables are this connection's, so the
/// read runs on the writer connection as a `Stage` transaction.
pub(crate) fn keys(
    session: &Session,
    operation: i64,
    token: &[u8],
    window: Window,
    kind: u32,
    bound: &[u8],
) -> Result<Vec<Option<Vec<u8>>>, PersistenceError> {
    let sql = match window {
        Window::After => KEYS_AFTER,
        Window::Excluding => KEYS_EXCLUDING,
    };
    session.run_as(Mode::Stage, |tx| {
        let found = tx.typed(
            sql,
            &[&operation, &token, &i64::from(kind), &bound],
            (24 + bound.len()) as u64,
        )?;
        found
            .iter()
            .map(|row| {
                row.try_get::<_, Option<Vec<u8>>>(0)
                    .map_err(|e| rows::error(&e).into())
            })
            .collect::<Result<Vec<_>, PersistenceError>>()
    })
}

/// The values of `keys`, in order, in one owner-checked statement.
pub(crate) fn stored(
    session: &Session,
    operation: i64,
    token: &[u8],
    keys: &[EditRecordKey],
) -> Result<Vec<Option<Vec<u8>>>, PersistenceError> {
    let kinds: Vec<i64> = keys.iter().map(|key| i64::from(key.kind)).collect();
    let names: Vec<&[u8]> = keys.iter().map(|key| key.key.as_slice()).collect();
    session.run_as(Mode::Stage, |tx| {
        let found = tx.typed(
            STORED,
            &[&operation, &token, &kinds, &names],
            (24 + 40 * keys.len()) as u64,
        )?;
        found
            .iter()
            .map(|row| {
                row.try_get::<_, Option<Vec<u8>>>(1)
                    .map_err(|e| rows::error(&e).into())
            })
            .collect::<Result<Vec<_>, PersistenceError>>()
    })
}

/// Writes buffered changes in one owner-checked transaction: one upsert of
/// every value and one deletion of every removed key.
pub(crate) fn flush(
    session: &Session,
    operation: i64,
    token: &[u8],
    pending: &BTreeMap<EditRecordKey, Option<Vec<u8>>>,
) -> Result<(), PersistenceError> {
    let (mut put_kinds, mut put_keys, mut put_values) = (Vec::new(), Vec::new(), Vec::new());
    let (mut gone_kinds, mut gone_keys) = (Vec::new(), Vec::new());
    let mut bytes = 0u64;
    for (key, value) in pending {
        bytes += 40 + value.as_ref().map_or(0, Vec::len) as u64;
        match value {
            Some(value) => {
                put_kinds.push(i64::from(key.kind));
                put_keys.push(key.key.as_slice());
                put_values.push(value.as_slice());
            }
            None => {
                gone_kinds.push(i64::from(key.kind));
                gone_keys.push(key.key.as_slice());
            }
        }
    }
    session.run_as(Mode::Stage, |tx| {
        owned(tx, operation, token)?;
        if !put_kinds.is_empty() {
            tx.typed(
                PUT,
                &[&operation, &put_kinds, &put_keys, &put_values],
                bytes,
            )?;
        }
        if !gone_kinds.is_empty() {
            tx.typed(DELETE, &[&operation, &gone_kinds, &gone_keys], bytes)?;
        }
        Ok::<_, PersistenceError>(())
    })
}
