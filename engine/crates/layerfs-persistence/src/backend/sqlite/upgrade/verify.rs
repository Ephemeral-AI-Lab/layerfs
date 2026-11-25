//! The SQLite upgrade's checks before COMMIT (#804 §7): unchanged content,
//! the exact schema, the open-time validation and the touched pages.
use super::frozen::{self, Row};
use super::step::error;
use crate::backend::sqlite::compact::verify::{file_heads_digest, TUPLE_SQL};
use crate::backend::sqlite::schema;
use crate::upgrade::UpgradeError;
use crate::{SqliteAcquisitionSchema, SqlitePackLayout};
use rusqlite::types::Value;
use rusqlite::Connection;

/// What must not change: the Commit tuple (compaction's first-statement
/// tuple), the live-heads digest and the Commit count; and the page counts
/// the touched-pages check compares.
#[derive(Debug, PartialEq)]
pub(crate) struct Values {
    tuple: Vec<Value>,
    heads: [u8; 32],
    commits: i64,
    pages: i64,
    free: i64,
}

/// The values of the transaction now.
pub(crate) fn values(connection: &Connection) -> Result<Values, UpgradeError> {
    let tuple = connection
        .query_row(TUPLE_SQL, [], |row| {
            (0..11)
                .map(|i| row.get::<_, Value>(i))
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(|e| error(e, "reading the Commit tuple"))?;
    let heads = file_heads_digest(connection)
        .map_err(|e| UpgradeError::Failed(format!("reading the heads digest: {e:?}")))?;
    let one = |sql: &str| {
        connection
            .query_row(sql, [], |r| r.get::<_, i64>(0))
            .map_err(|e| error(e, sql))
    };
    Ok(Values {
        tuple,
        heads,
        commits: one("SELECT count(*) FROM \"commit\"")?,
        pages: one("PRAGMA page_count")?,
        free: one("PRAGMA freelist_count")?,
    })
}

/// The live `sqlite_schema` rows (`gc_intent` read beside them).
pub(crate) fn rows(connection: &Connection) -> Result<Vec<Row>, UpgradeError> {
    frozen::rows(connection).map_err(|e| error(e, "reading the schema"))
}

/// Refuses `Busy` while any `gc_intent` row is live or uncertain: an
/// old-build mount or other GC op still names this Store (§5.1 step 3).
pub(crate) fn no_live_intent(connection: &Connection) -> Result<(), UpgradeError> {
    let live: i64 = connection
        .query_row(
            "SELECT count(*) FROM gc_intent WHERE state<>'done' OR uncertain<>0",
            [],
            |r| r.get(0),
        )
        .map_err(|e| error(e, "reading gc_intent"))?;
    match live {
        0 => Ok(()),
        n => Err(UpgradeError::Busy(format!(
            "{n} live or uncertain gc_intent rows (a mount or GC op of an older build) name \
             this Store"
        ))),
    }
}

/// §7 on the open transaction: `before` taken in `check`, `expected` the
/// frozen target rows for the recognized variant, `user_version` the new one.
pub(crate) fn verify(
    connection: &Connection,
    before: &Values,
    expected: &[Row],
    user_version: i64,
) -> Result<(), UpgradeError> {
    let failed = |check: &str| Err(UpgradeError::Failed(format!("verify: {check}")));
    let after = values(connection)?;
    if after.tuple != before.tuple {
        return failed("the Commit tuple changed");
    }
    if after.heads != before.heads {
        return failed("the heads digest changed");
    }
    if after.commits != before.commits {
        return failed("the Commit count changed");
    }
    let live = rows(connection)?;
    if live != expected {
        return failed("the live schema is not the frozen target definition");
    }
    open_validation(connection, &live, user_version)?;
    for table in ["sqlite_schema", "history_meta", "store_policy"] {
        let answer: String = connection
            .query_row(&format!("PRAGMA quick_check({table})"), [], |r| r.get(0))
            .map_err(|e| error(e, "quick_check"))?;
        if answer != "ok" {
            return failed(&format!("quick_check({table}): {answer}"));
        }
    }
    // Nothing is freed or moved: the index and the rewritten rows only add
    // pages, so the file grows or stays and every used page stays used.
    if after.pages < before.pages || after.pages - after.free < before.pages - before.free {
        return failed("page_count/freelist_count shrank");
    }
    Ok(())
}

/// The checks a current build's open makes (`schema::check` and the
/// `history_meta` comparison of `Session::validate`), on this transaction.
fn open_validation(
    connection: &Connection,
    live: &[Row],
    user_version: i64,
) -> Result<(), UpgradeError> {
    let failed = |check: &str| {
        Err(UpgradeError::Failed(format!(
            "verify: open validation: {check}"
        )))
    };
    let selection = selection(user_version);
    let work = frozen::work();
    if schema::check(connection, &work, selection).is_err() {
        return failed("header or table set");
    }
    let (identity, version, source, definition, incarnation): (i64, i64, String, String, i64) =
        connection
            .query_row(
                "SELECT identity_format,schema_version,schema_source,schema_definition,\
                 catalog_incarnation FROM history_meta WHERE id=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .map_err(|e| error(e, "reading history_meta"))?;
    let policy: (i64, i64) = connection
        .query_row(
            "SELECT schema_version,length(trace_salt) FROM store_policy WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| error(e, "reading store_policy"))?;
    if identity != 3 || version != user_version || incarnation <= 0 {
        return failed("history_meta identity, version or incarnation");
    }
    if source != schema::source(selection) {
        return failed("the recorded source is not this build's");
    }
    if definition != frozen::definition(live) {
        return failed("the recorded definition is not the live one");
    }
    if policy != (user_version, 32) {
        return failed("store_policy version or trace salt");
    }
    Ok(())
}

/// The engine selection of a schema-3 `user_version`.
pub(crate) fn selection(user_version: i64) -> (SqlitePackLayout, SqliteAcquisitionSchema) {
    let (acquisition, layout) = frozen::selection(user_version);
    let layout = match layout {
        0 => SqlitePackLayout::Monolithic,
        1 => SqlitePackLayout::GroupRows,
        _ => SqlitePackLayout::GroupRowsIndexed,
    };
    let acquisition = match acquisition {
        true => SqliteAcquisitionSchema::Tables,
        false => SqliteAcquisitionSchema::Absent,
    };
    (layout, acquisition)
}
