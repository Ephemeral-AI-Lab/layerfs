//! Merge build records on a SQLite Store (#355): the statements of
//! `storage/merge_records.rs` over the Store file's `merge_*` tables, and the
//! lease sweep a writable open runs.
use crate::backend::{error::BackendError, Session, Transaction};
use crate::storage::merge_records::{Window, MERGE_LEASE, WINDOW};
use layerfs_content::EditRecordKey;
use layerfs_storage::port::PersistenceError;
use rusqlite::ToSql;
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Records one removal job deletes.
const REMOVE_ROWS: i64 = 4096;
const OWNER: &str = "SELECT 1 FROM merge_operation WHERE operation=?1 AND token=?2";
// The owner row joins the record keys directly, so the window is a range read
// of the primary key: no keys answer one NULL row, no owner answers none (#364).
const KEYS_AFTER: &str = "SELECT r.key FROM merge_operation o LEFT JOIN merge_record r
    ON r.operation=o.operation AND r.kind=?3 AND r.key>?4
    WHERE o.operation=?1 AND o.token=?2 ORDER BY r.key LIMIT 64";
const KEYS_EXCLUDING: &str = "SELECT r.key FROM merge_operation o LEFT JOIN merge_record r
    ON r.operation=o.operation AND r.kind=?3 AND r.key<>?4
    WHERE o.operation=?1 AND o.token=?2 ORDER BY r.key LIMIT 64";
const PUT: &str = "INSERT INTO merge_record(operation,kind,key,value) VALUES(?1,?2,?3,?4)
    ON CONFLICT(operation,kind,key) DO UPDATE SET value=excluded.value";
const DELETE: &str = "DELETE FROM merge_record WHERE operation=?1 AND kind=?2 AND key=?3";
const REMOVE: &str = "DELETE FROM merge_record WHERE operation=?1 AND (kind,key) IN
    (SELECT kind,key FROM merge_record WHERE operation=?1 ORDER BY kind,key LIMIT ?2)";

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

/// Registers one merge's owner: `(operation, token)`.
pub(crate) fn register(session: &Session) -> Result<(i64, Vec<u8>), PersistenceError> {
    session
        .run(true, |tx| {
            let row = tx.mapped(
                "INSERT INTO merge_operation(token,touched) VALUES(randomblob(16),?1)
         RETURNING operation,token",
                &[&now_ms()],
                8,
                1,
                |row| {
                    let token: Vec<u8> = row
                        .get(1)
                        .map_err(crate::backend::sqlite::session::rows::error)?;
                    Ok((
                        row.get::<_, i64>(0)
                            .map_err(crate::backend::sqlite::session::rows::error)?,
                        token,
                    ))
                },
            )?;
            row.into_iter().next().ok_or(BackendError::Integrity)
        })
        .map_err(Into::into)
}

/// Removes one window of the merge's records, then, once none is left,
/// its owner: the records removed (0 once the owner went).
pub(crate) fn remove(
    session: &Session,
    operation: i64,
    token: &[u8],
) -> Result<i64, PersistenceError> {
    session.run(true, |tx| {
        owned(tx, operation, token)?;
        tx.borrowed(REMOVE, &[&operation, &REMOVE_ROWS], 16)?;
        let changed = changes(tx)?;
        if changed == 0 {
            tx.borrowed(
                "DELETE FROM merge_operation WHERE operation=?1",
                &[&operation],
                8,
            )?;
        }
        Ok::<_, PersistenceError>(changed)
    })
}

/// Marks the owner live now.
pub(crate) fn refresh(
    session: &Session,
    operation: i64,
    token: &[u8],
) -> Result<(), PersistenceError> {
    session.run(true, |tx| {
        owned(tx, operation, token)?;
        tx.borrowed(
            "UPDATE merge_operation SET touched=?2 WHERE operation=?1",
            &[&operation, &now_ms()],
            16,
        )?;
        Ok::<_, PersistenceError>(())
    })
}

/// One owner-checked key window: no rows means the owner is gone, and a
/// NULL row means no keys.
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
    let kind = i64::from(kind);
    let values: Vec<&dyn ToSql> = vec![&operation, &token, &kind, &bound];
    session.read_mapped(sql, &values, 64, WINDOW, |row| {
        row.get::<_, Option<Vec<u8>>>(0).map(Some).map_err(|error| {
            PersistenceError::from(crate::backend::sqlite::session::rows::error(error))
        })
    })
}

/// The stored values of `keys`, in order, in one owner-checked
/// statement; no rows means the owner is gone.
pub(crate) fn stored(
    session: &Session,
    operation: i64,
    token: &[u8],
    keys: &[EditRecordKey],
) -> Result<Vec<Option<Vec<u8>>>, PersistenceError> {
    let rows = (0..keys.len())
        .map(|i| format!("(?{},?{},?{})", 3 * i + 3, 3 * i + 4, 3 * i + 5))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "WITH k(i,kind,key) AS (VALUES {rows}) SELECT k.i, r.value FROM merge_operation o
     JOIN k LEFT JOIN merge_record r
     ON r.operation=o.operation AND r.kind=k.kind AND r.key=k.key
     WHERE o.operation=?1 AND o.token=?2 ORDER BY k.i"
    );
    let indexes: Vec<i64> = (0..keys.len() as i64).collect();
    let kinds: Vec<i64> = keys.iter().map(|key| i64::from(key.kind)).collect();
    let mut values: Vec<&dyn ToSql> = vec![&operation, &token];
    for (i, key) in keys.iter().enumerate() {
        values.push(&indexes[i]);
        values.push(&kinds[i]);
        values.push(&key.key);
    }
    let error = |error| PersistenceError::from(crate::backend::sqlite::session::rows::error(error));
    session.read_mapped(&sql, &values, 64, keys.len(), |row| {
        Ok::<_, PersistenceError>(Some(row.get::<_, Option<Vec<u8>>>(1).map_err(error)?))
    })
}

/// Writes buffered changes in one owner-checked write transaction.
pub(crate) fn flush(
    session: &Session,
    operation: i64,
    token: &[u8],
    pending: &BTreeMap<EditRecordKey, Option<Vec<u8>>>,
) -> Result<(), PersistenceError> {
    session.run(true, |tx| {
        owned(tx, operation, token)?;
        for (key, value) in pending {
            let kind = i64::from(key.kind);
            let bytes = key.key.as_slice();
            match value {
                Some(value) => tx.borrowed(
                    PUT,
                    &[&operation, &kind, &bytes, &value.as_slice()],
                    48 + value.len() as u64,
                )?,
                None => tx.borrowed(DELETE, &[&operation, &kind, &bytes], 48)?,
            };
        }
        Ok::<_, PersistenceError>(())
    })
}

fn owned(tx: &Transaction<'_>, operation: i64, token: &[u8]) -> Result<(), PersistenceError> {
    let rows = tx.borrowed(OWNER, &[&operation, &token], 24)?;
    if rows.is_empty() {
        return Err(PersistenceError::Missing);
    }
    Ok(())
}

fn changes(tx: &Transaction<'_>) -> Result<i64, BackendError> {
    tx.mapped("SELECT changes()", &[], 0, 1, |row| {
        row.get::<_, i64>(0)
            .map_err(crate::backend::sqlite::session::rows::error)
    })?
    .into_iter()
    .next()
    .ok_or(BackendError::Integrity)
}

/// Removes the records of merges idle past [`MERGE_LEASE`], one window per
/// job. The owner's token is revoked first, so the merge, should it resume,
/// fails `Missing` instead of reading records already removed as absent.
pub(crate) fn sweep(session: &Session) -> Result<(), BackendError> {
    let expired = now_ms() - MERGE_LEASE.as_millis() as i64;
    loop {
        let removed = session.run(true, |tx| {
            let owners = tx.mapped(
                "SELECT operation,token=zeroblob(16) FROM merge_operation
                 WHERE touched<?1 OR token=zeroblob(16) LIMIT 1",
                &[&expired],
                16,
                1,
                |row| {
                    let error = crate::backend::sqlite::session::rows::error;
                    Ok((
                        row.get::<_, i64>(0).map_err(error)?,
                        row.get::<_, bool>(1).map_err(error)?,
                    ))
                },
            )?;
            let Some((operation, revoked)) = owners.into_iter().next() else {
                return Ok(false);
            };
            if !revoked {
                tx.borrowed(
                    "UPDATE merge_operation SET token=zeroblob(16) WHERE operation=?1",
                    &[&operation],
                    8,
                )?;
                return Ok(true);
            }
            tx.borrowed(REMOVE, &[&operation, &REMOVE_ROWS], 16)?;
            if changes(tx)? == 0 {
                tx.borrowed(
                    "DELETE FROM merge_operation WHERE operation=?1",
                    &[&operation],
                    8,
                )?;
            }
            Ok::<_, BackendError>(true)
        })?;
        if !removed {
            return Ok(());
        }
    }
}
