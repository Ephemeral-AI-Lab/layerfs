//! Step 2 -> 3's frozen texts (#804 rule 5): the schema-2 source variant (d)
//! every installed schema-2 build recorded (91635f51f8 through dcd0fa2c30,
//! byte-identical), and the schema-3 source the step records. Shipped SQL
//! copied once and never edited (rule 9); nothing here reads the current
//! schema constants, so recognition never drifts when the DDL changes.
use std::cell::RefCell;

/// Schema 2 (d), in the order the engine concatenated its scripts.
mod v2 {
    pub(super) const OBJECTS: &str = include_str!("../../../../sql/upgrade/sqlite/v2/objects.sql");
    pub(super) const UNITS: &str =
        include_str!("../../../../sql/upgrade/sqlite/v2/objects_units.sql");
    pub(super) const UNIT_INDEX: &str =
        include_str!("../../../../sql/upgrade/sqlite/v2/objects_units_index.sql");
    pub(super) const METADATA: &str =
        include_str!("../../../../sql/upgrade/sqlite/v2/metadata.sql");
    pub(super) const HISTORY: &str = include_str!("../../../../sql/upgrade/sqlite/v2/history.sql");
    pub(super) const LINEAGE: &str = include_str!("../../../../sql/upgrade/sqlite/v2/lineage.sql");
    pub(super) const SCRATCH: &str =
        include_str!("../../../../sql/upgrade/sqlite/v2/merge_scratch.sql");
    pub(super) const DIGESTS: &str =
        include_str!("../../../../sql/upgrade/sqlite/v2/file_digest.sql");
    pub(super) const GC: &str = include_str!("../../../../sql/upgrade/sqlite/v2/gc.sql");
    pub(super) const ACQUISITION: &str =
        include_str!("../../../../sql/upgrade/sqlite/v2/acquisition.sql");
}

/// Schema 3's history script; its other scripts are schema 2's unchanged.
const V3_HISTORY: &str = include_str!("../../../../sql/upgrade/sqlite/v3/history.sql");

/// The position columns the schema-2 open migrations appended to a Store
/// written before them (`schema.rs` `POSITION_COLUMNS` as of dcd0fa2c30):
/// `(table, column, ALTER)`.
const POSITION_COLUMNS: [(&str, &str, &str); 4] = [
    (
        "\"commit\"",
        "history_serial",
        "ALTER TABLE \"commit\" ADD COLUMN history_serial INTEGER NOT NULL DEFAULT 0",
    ),
    (
        "layer",
        "history_serial",
        "ALTER TABLE layer ADD COLUMN history_serial INTEGER NOT NULL DEFAULT 0",
    ),
    (
        "store_policy",
        "trace_salt",
        "ALTER TABLE store_policy ADD COLUMN trace_salt BLOB NOT NULL DEFAULT (X'0000000000000000000000000000000000000000000000000000000000000000') CHECK(length(trace_salt)=32)",
    ),
    (
        "history_meta",
        "next_history_serial",
        "ALTER TABLE history_meta ADD COLUMN next_history_serial INTEGER NOT NULL DEFAULT 1",
    ),
];

/// The column step 2 -> 3 appends, as `ALTER TABLE` writes it.
pub(crate) const ADD_ATTEMPT: &str =
    "ALTER TABLE \"commit\" ADD COLUMN attempt BLOB CHECK(attempt IS NULL OR length(attempt)=16)";
/// The index step 2 -> 3 creates, as its schema row records it.
pub(crate) const ATTEMPT_INDEX: &str =
    "CREATE UNIQUE INDEX commit_attempt ON \"commit\"(attempt) WHERE attempt IS NOT NULL";
/// The last column of `commit` in every schema-2 (d) form, fresh or
/// position-migrated: SQLite's `ADD COLUMN` writes the new column right
/// after it.
const COMMIT_LAST: &str = "history_serial INTEGER NOT NULL DEFAULT 0";
/// What `ADD COLUMN` inserts after [`COMMIT_LAST`].
const ATTEMPT_TEXT: &str = ", attempt BLOB CHECK(attempt IS NULL OR length(attempt)=16)";

/// `(acquisition tables, layout 0/1/2)` of a schema-2 or 3 `user_version`.
pub(crate) fn selection(user_version: i64) -> (bool, i64) {
    let offset = (user_version - 16) % 20;
    (offset >= 6, offset % 6)
}

fn compose(user_version: i64, history: &str) -> Vec<String> {
    let (acquisition, layout) = selection(user_version);
    let objects = match layout {
        0 => v2::OBJECTS.to_owned(),
        1 => v2::UNITS.to_owned(),
        _ => format!("{}{}", v2::UNITS, v2::UNIT_INDEX),
    };
    let version = format!("schema_version={user_version}");
    let mut scripts = vec![
        objects,
        v2::METADATA.replace("schema_version=1", &version),
        history.replace("schema_version=1", &version),
        v2::LINEAGE.to_owned(),
        v2::SCRATCH.to_owned(),
        v2::DIGESTS.to_owned(),
        v2::GC.to_owned(),
    ];
    if acquisition {
        scripts.push(v2::ACQUISITION.to_owned());
    }
    scripts
}

/// The source a schema-2 (d) Store of `user_version` (36–38, 42–44) recorded.
pub(crate) fn source_v2(user_version: i64) -> String {
    compose(user_version, v2::HISTORY).concat()
}

/// The source step 2 -> 3 records for a Store of schema-3 `user_version`
/// (56–58, 62–64): exactly what a fresh schema-3 Store records.
pub(crate) fn source_v3(user_version: i64) -> String {
    compose(user_version, V3_HISTORY).concat()
}

/// One `sqlite_schema` row: `(type, name, sql)`.
pub(crate) type Row = (String, String, String);

/// The schema rows a fresh schema-2 (d) Store of `user_version` has, and
/// the rows of one the position migrations brought forward (each appended
/// column at the end of its table), both built from the frozen scripts in
/// a private in-memory database.
pub(crate) fn rows_v2(user_version: i64) -> Result<(Vec<Row>, Vec<Row>), rusqlite::Error> {
    let scratch = rusqlite::Connection::open_in_memory()?;
    for script in compose(user_version, v2::HISTORY) {
        scratch.execute_batch(&script)?;
    }
    let fresh = rows(&scratch)?;
    for (table, column, _) in POSITION_COLUMNS {
        scratch.execute_batch(&format!("ALTER TABLE {table} DROP COLUMN {column}"))?;
    }
    for (_, _, alter) in POSITION_COLUMNS {
        scratch.execute_batch(alter)?;
    }
    Ok((fresh, rows(&scratch)?))
}

/// The rows of `connection`'s schema in the order the engine records them.
pub(crate) fn rows(connection: &rusqlite::Connection) -> Result<Vec<Row>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "SELECT type,name,sql FROM sqlite_schema WHERE substr(name,1,7)!='sqlite_' ORDER BY type,name",
    )?;
    let found = statement.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    found.collect()
}

/// The recorded definition text of `rows` (`schema.rs` `definition`).
pub(crate) fn definition(rows: &[Row]) -> String {
    let mut text = String::new();
    for (kind, name, sql) in rows {
        for part in [kind, name, sql] {
            text.push_str(part);
            text.push('\n');
        }
    }
    text
}

/// Which variant the live rows are: every row byte-identical to the fresh
/// (d) row (`"d"`), or each row either that or the position-migrated form
/// token for token (`"d+appended"`, the doctor's rule for SQLite's rewrite
/// of an appended column). `None` for anything else.
pub(crate) fn recognize_rows(
    live: &[Row],
    fresh: &[Row],
    migrated: &[Row],
) -> Option<&'static str> {
    if live == fresh {
        return Some("d");
    }
    if live.len() != fresh.len() {
        return None;
    }
    let same = live.iter().zip(fresh).zip(migrated).all(|((l, f), m)| {
        l.0 == f.0
            && l.1 == f.1
            && (l.2 == f.2 || crate::backend::sqlite::schema_diagnostic::same_schema(&l.2, &m.2))
    });
    same.then_some("d+appended")
}

/// The exact rows step 2 -> 3 leaves, from the exact rows before: `commit`
/// gains [`ATTEMPT_TEXT`] right after its last column, the two version
/// CHECKs read the new `user_version`, and [`ATTEMPT_INDEX`] is added in
/// its sorted place. `None` when a row does not hold the text once.
pub(crate) fn rows_after(before: &[Row], old: i64, new: i64) -> Option<Vec<Row>> {
    let once = |sql: &str, from: &str, to: &str| {
        (sql.matches(from).count() == 1).then(|| sql.replacen(from, to, 1))
    };
    let mut after = Vec::with_capacity(before.len() + 1);
    for (kind, name, sql) in before {
        let sql = match (kind.as_str(), name.as_str()) {
            ("table", "commit") => once(sql, COMMIT_LAST, &format!("{COMMIT_LAST}{ATTEMPT_TEXT}"))?,
            ("table", "store_policy" | "history_meta") => once(
                sql,
                &format!("CHECK(schema_version={old})"),
                &format!("CHECK(schema_version={new})"),
            )?,
            _ => sql.clone(),
        };
        after.push((kind.clone(), name.clone(), sql));
    }
    after.push((
        "index".to_owned(),
        "commit_attempt".to_owned(),
        ATTEMPT_INDEX.to_owned(),
    ));
    after.sort_by(|a, b| (a.0.as_bytes(), a.1.as_bytes()).cmp(&(b.0.as_bytes(), b.1.as_bytes())));
    Some(after)
}

/// Scratch work counter for the shared schema checks.
pub(crate) fn work() -> RefCell<crate::SqlWork> {
    RefCell::new(crate::SqlWork::default())
}
