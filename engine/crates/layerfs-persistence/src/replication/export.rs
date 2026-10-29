//! Exporting a live Store's changed rows into a new bundle file.
use super::connection::{open, open_store};
use super::digest::{bundle_digest, next_state};
use super::lookup::{catalog_of, last_seq, state_of, table_exists};
use super::tables::{key_columns, quoted, BUNDLE_FORMAT, BUNDLE_SCHEMA, LOGGED};
use super::{ChangeCursor, ChangeError, ExportReceipt};
use rusqlite::{params, Connection, OpenFlags};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{fs, io};

/// Writes every row of the live Store at `store` changed after `since` into a
/// new bundle file at `bundle`, from one read snapshot. Takes no write lock
/// and does not stop writers. When nothing changed, writes no file and
/// returns `after == before`.
///
/// `since` is the replica's cursor: the one `mark_base` returned, or the
/// `after` of the bundle the replica applied last. Above the base, the Store
/// checks the cursor's sequence, not its state, because it keeps no record of
/// the states it exported. A cursor from a separately written copy of this
/// Store is therefore not detected here.
///
/// The bundle is written under a temporary name beside `bundle` and linked
/// into place, so `bundle` names a complete file or nothing. A failed or
/// interrupted export can leave a `.<name>.<pid>-<n>.partial` file there,
/// which nothing reads.
pub fn export_changes(
    store: &Path,
    since: ChangeCursor,
    bundle: &Path,
) -> Result<ExportReceipt, ChangeError> {
    let source = open_store(store, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    source.execute_batch("BEGIN")?;
    let exported = export_in(&source, since, bundle);
    // Ends the read snapshot; a read transaction has nothing to commit.
    let _ = source.execute_batch("COMMIT");
    exported
}

fn export_in(
    source: &Connection,
    since: ChangeCursor,
    bundle: &Path,
) -> Result<ExportReceipt, ChangeError> {
    let base = state_of(source, "main")?.ok_or(ChangeError::NotBase)?;
    if since.seq < base.seq || (since.seq == base.seq && since.state != base.state) {
        return Err(ChangeError::Cursor(
            "the cursor precedes or differs from the Store's recorded state",
        ));
    }
    let last = last_seq(source, "main")?.unwrap_or(0).max(base.seq);
    if since.seq > last {
        return Err(ChangeError::Cursor("the cursor is ahead of the Store"));
    }
    if since.seq == last {
        return Ok(ExportReceipt {
            before: since,
            after: since,
            rows: 0,
            deletes: 0,
            log_entries: 0,
            bytes: 0,
            digest: [0; 32],
        });
    }
    // Refuses an existing path before the work; the link below decides.
    if bundle.try_exists()? {
        return Err(ChangeError::Exists);
    }
    let catalog = catalog_of(source, "main")?;
    let log_entries: i64 = source.query_row(
        "SELECT count(*) FROM change_log WHERE tbl>0 AND seq>?1 AND seq<=?2",
        [since.seq as i64, last as i64],
        |r| r.get(0),
    )?;
    let partial = partial_path(bundle)?;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&partial)?;
    let published = write_bundle(source, since, last, &catalog, &partial).and_then(|written| {
        let bytes = fs::metadata(&partial)?.len();
        match fs::hard_link(&partial, bundle) {
            Ok(()) => Ok((written, bytes)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Err(ChangeError::Exists),
            Err(error) => Err(error.into()),
        }
    });
    // The outcome above is the result: the bundle is published or the export
    // failed. A temporary file that cannot be removed is never read.
    let _ = fs::remove_file(&partial);
    let ((after, digest, rows, deletes), bytes) = published?;
    Ok(ExportReceipt {
        before: since,
        after,
        rows,
        deletes,
        log_entries: log_entries as u64,
        bytes,
        digest,
    })
}

/// `.<name>.<pid>-<n>.partial` beside `bundle`, unique within this process.
fn partial_path(bundle: &Path) -> Result<PathBuf, ChangeError> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = bundle
        .file_name()
        .ok_or(ChangeError::Integrity("the bundle path names no file"))?;
    let mut partial = OsString::from(".");
    partial.push(name);
    partial.push(format!(
        ".{}-{}.partial",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    Ok(bundle.with_file_name(partial))
}

fn write_bundle(
    source: &Connection,
    since: ChangeCursor,
    last: u64,
    catalog: &[u8],
    bundle: &Path,
) -> Result<(ChangeCursor, [u8; 32], u64, u64), ChangeError> {
    let out = open(bundle, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    // The bundle holds copies of some rows; the replica checks references
    // when the bundle is applied. The bundled SQLite enables them by default.
    // Small pages keep a bundle of a few rows small: every table it carries
    // takes at least one page. Large payloads lose 4 bytes per overflow page.
    out.execute_batch(
        "PRAGMA page_size=1024; PRAGMA foreign_keys=OFF; PRAGMA journal_mode=OFF; \
         PRAGMA synchronous=OFF; BEGIN;",
    )?;
    out.execute_batch(BUNDLE_SCHEMA)?;
    let mut logged = source
        .prepare("SELECT EXISTS(SELECT 1 FROM change_log WHERE tbl=?1 AND seq>?2 AND seq<=?3)")?;
    for t in &LOGGED {
        if !table_exists(source, "main", t.table)? {
            continue;
        }
        // A table carries its changed rows and its deleted keys; a table
        // with neither is left out of the bundle.
        let changed: bool =
            logged.query_row(params![t.code, since.seq as i64, last as i64], |r| r.get(0))?;
        if !changed {
            continue;
        }
        let definition: String = source.query_row(
            "SELECT sql FROM sqlite_schema WHERE type='table' AND name=?1",
            [t.table],
            |r| r.get(0),
        )?;
        out.execute_batch(&definition)?;
        let columns = key_columns(t.keys.len());
        let join = t
            .keys
            .iter()
            .enumerate()
            .map(|(i, key)| format!("r.{key}=c.k{}", i + 1))
            .collect::<Vec<_>>()
            .join(" AND ");
        let mut read = source.prepare(&format!(
            "SELECT {columns},(r.{} IS NOT NULL),r.* FROM \
             (SELECT DISTINCT {columns} FROM change_log WHERE tbl=?1 AND seq>?2 AND seq<=?3) c \
             LEFT JOIN {} r ON {join}",
            t.keys[0],
            quoted(t.table)
        ))?;
        let offset = t.keys.len() + 1;
        let width = read.column_count() - offset;
        let mut insert = out.prepare(&format!(
            "INSERT INTO {} VALUES({})",
            quoted(t.table),
            vec!["?"; width].join(",")
        ))?;
        let mut delete = out.prepare(&format!(
            "INSERT INTO bundle_delete(tbl,{columns}) VALUES(?1,{})",
            (2..=t.keys.len() + 1)
                .map(|i| format!("?{i}"))
                .collect::<Vec<_>>()
                .join(",")
        ))?;
        let mut rows = read.query([t.code, since.seq as i64, last as i64])?;
        while let Some(row) = rows.next()? {
            let present: bool = row.get(t.keys.len())?;
            if present {
                let values = (offset..offset + width)
                    .map(|i| row.get_ref(i).map(rusqlite::types::ToSqlOutput::Borrowed))
                    .collect::<Result<Vec<_>, _>>()?;
                insert.execute(rusqlite::params_from_iter(values))?;
            } else if t.immutable {
                return Err(ChangeError::Integrity("a logged immutable row is absent"));
            } else {
                let mut values = vec![rusqlite::types::Value::Integer(t.code)];
                for i in 0..t.keys.len() {
                    values.push(row.get(i)?);
                }
                delete.execute(rusqlite::params_from_iter(values))?;
            }
        }
    }
    let (digest, rows, deletes) = bundle_digest(&out, "main")?;
    let after = ChangeCursor {
        state: next_state(catalog, &since, last, &digest),
        seq: last,
    };
    out.execute(
        "INSERT INTO bundle_meta(id,format,catalog_id,before_state,before_seq,after_state,after_seq,rows,deletes,digest) \
         VALUES(1,?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![
            BUNDLE_FORMAT,
            catalog,
            &since.state[..],
            since.seq as i64,
            &after.state[..],
            after.seq as i64,
            rows as i64,
            deletes as i64,
            &digest[..]
        ],
    )?;
    out.execute_batch("COMMIT")?;
    Ok((after, digest, rows, deletes))
}
