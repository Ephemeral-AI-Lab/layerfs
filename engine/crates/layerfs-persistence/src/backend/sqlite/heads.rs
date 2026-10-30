//! The history heads of a closed or live SQLite Store file (#506 E2): one
//! line per project, `stack <name> <head_layer_id>`, and one per Branch,
//! `branch <project> <name> <base_layer_id> <head_commit_id or ->`, sorted,
//! ids in lowercase hex. Exactly these rows change when history a pull
//! would bring changes: projects added or removed, Branches made, moved,
//! merged or deleted, and every Commit (a Branch Commit moves its head).
//!
//! Read on a connection of its own, opened for this read and closed before
//! it returns, so it never keeps the Store open. SQLite cannot open a WAL
//! database read-only while it has no `-shm` (a Store no process holds), so
//! the connection opens read-write but is `query_only`: it reads in one
//! transaction and writes nothing. When it is the last connection, closing
//! it removes the sidecars its open made, as any last opener does. A busy
//! Store is reported, never waited on or retried.
use super::session::rows;
use layerfs_storage::port::PersistenceError;
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[usize::from(byte >> 4)] as char);
        out.push(DIGITS[usize::from(byte & 15)] as char);
    }
    out
}

/// The sorted heads lines of the Store file at `path`, which must exist.
pub fn read_heads(path: &Path) -> Result<Vec<String>, PersistenceError> {
    let failed = |error: rusqlite::Error| PersistenceError::from(rows::error(error));
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(failed)?;
    connection
        .busy_timeout(std::time::Duration::ZERO)
        .map_err(failed)?;
    connection
        .execute_batch("PRAGMA query_only=1; BEGIN")
        .map_err(failed)?;
    let mut lines = Vec::new();
    {
        let mut stacks = connection
            .prepare("SELECT name, head_layer_id FROM layer_stack")
            .map_err(failed)?;
        let mut found = stacks.query([]).map_err(failed)?;
        while let Some(row) = found.next().map_err(failed)? {
            let name: String = row.get(0).map_err(failed)?;
            let head: Vec<u8> = row.get(1).map_err(failed)?;
            lines.push(format!("stack {name} {}", hex(&head)));
        }
        let mut branches = connection
            .prepare(
                "SELECT s.name, b.name, b.base_layer_id, b.head_commit_id FROM branch b \
                 JOIN layer_stack s ON s.layer_stack_id = b.layer_stack_id",
            )
            .map_err(failed)?;
        let mut found = branches.query([]).map_err(failed)?;
        while let Some(row) = found.next().map_err(failed)? {
            let stack: String = row.get(0).map_err(failed)?;
            let name: String = row.get(1).map_err(failed)?;
            let base: Vec<u8> = row.get(2).map_err(failed)?;
            let head: Option<Vec<u8>> = row.get(3).map_err(failed)?;
            let head = head.map_or_else(|| "-".to_owned(), |head| hex(&head));
            lines.push(format!("branch {stack} {name} {} {head}", hex(&base)));
        }
    }
    connection.execute_batch("COMMIT").map_err(failed)?;
    connection
        .close()
        .map_err(|(_, error)| PersistenceError::from(rows::error(error)))?;
    lines.sort();
    Ok(lines)
}
