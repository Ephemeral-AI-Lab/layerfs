//! The history heads of a closed or live SQLite Store file (#506 E2): one
//! line per project, `stack <name> <head_layer_id>`, and one per Branch,
//! `branch <project> <name> <base_layer_id> <head_commit_id or ->`, sorted,
//! ids in lowercase hex. Exactly these rows change when history a pull
//! would bring changes: projects added or removed, Branches made, moved,
//! merged or deleted, and every Commit (a Branch Commit moves its head).
//!
//! A soft removal is a marker, not a delete, so the base rows can still be
//! present; the listing is filtered by the same `branch_retired` and
//! `layer_stack_retired` markers the catalog uses (spec 4.2). A retired
//! Branch leaves no `branch` line, and a retired project leaves neither its
//! `stack` line nor its Branches' lines: a backup copy's digest names the
//! visible history, not the leftover rows.
//!
//! Read on a connection of its own, opened for this read and closed before
//! it returns, so it never keeps the Store open. SQLite cannot open a WAL
//! database read-only while it has no `-shm` (a Store no process holds), so
//! the connection opens read-write but is `query_only`: it reads in one
//! transaction and writes nothing. When it is the last connection, closing
//! it checkpoints and removes the sidecars, as any last writable close does,
//! so the whole read holds the Store's writer lane from before the open to
//! after the close, like a writable session's open and close. Without it,
//! an open during a compaction's install left `-wal`/`-shm` beside the file
//! the rename installed, and the close tore them down unadmitted (#776 C3,
//! C4). The lane is waited for, as any writer waits; a Store busy once it
//! is held is reported, never retried.
use super::super::lane::writer_admission::Writer;
use super::super::session::rows;
use layerfs_storage::port::PersistenceError;
use rusqlite::{Connection, OpenFlags};
use sha2::Digest;
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
    // Declared first, so every exit closes the connection before the lane.
    let _lane = Writer::acquire(path).map_err(PersistenceError::from)?;
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
            .prepare(
                "SELECT s.name, s.head_layer_id FROM layer_stack s WHERE NOT EXISTS ( \
                 SELECT 1 FROM layer_stack_retired r WHERE r.layer_stack_id = s.layer_stack_id)",
            )
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
                 JOIN layer_stack s ON s.layer_stack_id = b.layer_stack_id \
                 WHERE NOT EXISTS (SELECT 1 FROM branch_retired r \
                                    WHERE r.branch_id = b.branch_id) \
                   AND NOT EXISTS (SELECT 1 FROM layer_stack_retired d \
                                    WHERE d.layer_stack_id = b.layer_stack_id)",
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

/// The heads digest of the Store file at `path`: the sha256 of the sorted
/// heads lines, each terminated by one `\n`. This is exactly the digest
/// store-backup attaches to a pushed copy.
pub fn heads_digest(path: &Path) -> Result<[u8; 32], PersistenceError> {
    let mut text = String::new();
    for line in read_heads(path)? {
        text.push_str(&line);
        text.push('\n');
    }
    Ok(sha2::Sha256::digest(text.as_bytes()).into())
}
