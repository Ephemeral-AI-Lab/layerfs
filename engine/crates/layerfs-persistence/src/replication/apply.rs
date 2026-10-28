//! Installing a bundle into an offline replica: parse its meta, verify it
//! against the replica, then install it.
use super::connection::{finish, open_store};
use super::digest::{bundle_digest, next_state};
use super::lookup::{catalog_of, last_seq, state_of, table_exists};
use super::tables::{key_columns, quoted, BUNDLE_FORMAT, LOGGED, TOTALS};
use super::{ApplyReceipt, ChangeCursor, ChangeError};
use rusqlite::{params, Connection, OpenFlags};
use std::path::Path;

/// Installs the bundle at `bundle` into the offline replica at `store`, which
/// must be at the bundle's starting cursor and unwritten since. Verifies the
/// bundle's digest and state, installs every row and deletion in one
/// transaction with foreign keys checked at commit, and records the bundle's
/// ending cursor. A refused bundle leaves the replica unchanged.
///
/// When another connection reads the replica, the bundle still commits but the
/// call returns [`ChangeError::NotCheckpointed`].
pub fn apply_changes(store: &Path, bundle: &Path) -> Result<ApplyReceipt, ChangeError> {
    let c = open_store(store, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    c.execute_batch("PRAGMA foreign_keys=ON")?;
    let name = bundle
        .to_str()
        .ok_or(ChangeError::Integrity("the bundle path is not UTF-8"))?;
    c.execute("ATTACH DATABASE ?1 AS b", [name])?;
    c.execute_batch("BEGIN IMMEDIATE; PRAGMA defer_foreign_keys=ON;")?;
    let applied = apply_in(&c);
    let finished = finish(&c, applied);
    let _ = c.execute_batch("DETACH DATABASE b");
    finished
}

fn apply_in(c: &Connection) -> Result<ApplyReceipt, ChangeError> {
    let catalog = catalog_of(c, "main")?;
    let meta = read_meta(c, &catalog)?;
    verify(c, &catalog, &meta)?;
    install(c, &meta)?;
    Ok(ApplyReceipt {
        before: meta.before,
        after: meta.after,
        rows: meta.rows,
        deletes: meta.deletes,
    })
}

/// The `bundle_meta` row of an attached bundle, parsed.
struct BundleMeta {
    before: ChangeCursor,
    after: ChangeCursor,
    rows: u64,
    deletes: u64,
    digest: [u8; 32],
}

/// Parses the attached bundle's one `bundle_meta` row. A bundle comes from
/// outside the replica, so its own constraints are not trusted: the row count,
/// format, catalog, value types and lengths are checked before a value is
/// read, and the sequences after.
fn read_meta(c: &Connection, catalog: &[u8]) -> Result<BundleMeta, ChangeError> {
    let (count, format, same_catalog, shaped): (i64, bool, bool, bool) = c.query_row(
        "SELECT count(*), coalesce(min(format IS ?1),0), coalesce(min(catalog_id IS ?2),0), \
         coalesce(min(typeof(before_state)='blob' AND length(before_state)=32 \
          AND typeof(after_state)='blob' AND length(after_state)=32 \
          AND typeof(digest)='blob' AND length(digest)=32 \
          AND typeof(before_seq)='integer' AND typeof(after_seq)='integer' \
          AND typeof(rows)='integer' AND typeof(deletes)='integer'),0) \
         FROM b.bundle_meta",
        params![BUNDLE_FORMAT, catalog],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    if count != 1 {
        return Err(ChangeError::Integrity("the bundle meta is not one row"));
    }
    if !format {
        return Err(ChangeError::Integrity("unknown bundle format"));
    }
    if !same_catalog {
        return Err(ChangeError::Integrity(
            "the bundle belongs to another catalog",
        ));
    }
    if !shaped {
        return Err(ChangeError::Integrity("the bundle meta is malformed"));
    }
    let (before_state, before_seq, after_state, after_seq, rows, deletes, digest) = c.query_row(
        "SELECT before_state,before_seq,after_state,after_seq,rows,deletes,digest \
         FROM b.bundle_meta",
        [],
        |r| {
            let number = |i| r.get::<_, i64>(i);
            Ok((
                r.get(0)?,
                number(1)?,
                r.get(2)?,
                number(3)?,
                number(4)?,
                number(5)?,
                r.get(6)?,
            ))
        },
    )?;
    let count = |n: i64| {
        u64::try_from(n).map_err(|_| ChangeError::Integrity("the bundle meta is malformed"))
    };
    let before = ChangeCursor {
        state: before_state,
        seq: count(before_seq)?,
    };
    let after = ChangeCursor {
        state: after_state,
        seq: count(after_seq)?,
    };
    if after.seq <= before.seq {
        return Err(ChangeError::Integrity(
            "the bundle's sequences are out of order",
        ));
    }
    Ok(BundleMeta {
        before,
        after,
        rows: count(rows)?,
        deletes: count(deletes)?,
        digest,
    })
}

/// Refuses a bundle that does not apply to this replica or is not intact,
/// before anything is installed.
fn verify(c: &Connection, catalog: &[u8], meta: &BundleMeta) -> Result<(), ChangeError> {
    let current = state_of(c, "main")?.ok_or(ChangeError::NotBase)?;
    if current != meta.before {
        return Err(ChangeError::Cursor(
            "the replica is not at the bundle's starting state",
        ));
    }
    if last_seq(c, "main")?.is_some_and(|seq| seq != meta.before.seq) {
        return Err(ChangeError::Diverged);
    }
    for t in LOGGED.iter().filter(|t| t.immutable) {
        let deleted: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM b.bundle_delete WHERE tbl=?1)",
            [t.code],
            |r| r.get(0),
        )?;
        if deleted {
            return Err(ChangeError::Integrity("a bundle deletes an immutable row"));
        }
    }
    let (computed, rows, deletes) = bundle_digest(c, "b")?;
    if computed != meta.digest || rows != meta.rows || deletes != meta.deletes {
        return Err(ChangeError::Integrity("bundle digest"));
    }
    if next_state(catalog, &meta.before, meta.after.seq, &computed) != meta.after.state {
        return Err(ChangeError::Integrity("bundle ending state"));
    }
    Ok(())
}

/// Installs a verified bundle's rows and deletions and records its ending
/// cursor.
fn install(c: &Connection, meta: &BundleMeta) -> Result<(), ChangeError> {
    let order = LOGGED
        .iter()
        .filter(|t| t.code != TOTALS)
        .chain(LOGGED.iter().filter(|t| t.code == TOTALS));
    for t in order {
        if !table_exists(c, "b", t.table)? {
            continue;
        }
        if !table_exists(c, "main", t.table)? {
            return Err(ChangeError::Integrity(
                "a bundled table is absent from the replica",
            ));
        }
        let table = quoted(t.table);
        if !t.immutable {
            let key = t.keys.join(",");
            let columns = key_columns(t.keys.len());
            c.execute(
                &format!("DELETE FROM main.{table} WHERE ({key}) IN (SELECT {key} FROM b.{table})"),
                [],
            )?;
            c.execute(
                &format!(
                    "DELETE FROM main.{table} WHERE ({key}) IN \
                     (SELECT {columns} FROM b.bundle_delete WHERE tbl=?1)"
                ),
                [t.code],
            )?;
        }
        c.execute(
            &format!("INSERT INTO main.{table} SELECT * FROM b.{table}"),
            [],
        )?;
    }
    // The installed rows logged themselves; the replica now stands at `after`.
    c.execute("DELETE FROM main.change_log", [])?;
    c.execute(
        "INSERT INTO main.change_log(seq,tbl) VALUES(?1,0)",
        [meta.after.seq as i64],
    )?;
    c.execute(
        "UPDATE main.change_state SET state=?1,seq=?2 WHERE id=1",
        params![&meta.after.state[..], meta.after.seq as i64],
    )?;
    Ok(())
}
