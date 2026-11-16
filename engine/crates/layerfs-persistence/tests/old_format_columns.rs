//! A Store written before `commit`/`layer.history_serial` and
//! `store_policy.trace_salt` existed opens writable and gains them (T1.2b):
//! the ALTER step matches the recorded schema source and definition, gives
//! existing rows serial 0 and the Store a random salt, and records the
//! current schema.
mod support;
use layerfs_persistence::{PersistenceConfig, SqliteHandles, SqlitePersistenceProfile};
use support::*;

const GC: &str = include_str!("../sql/sqlite/gc.sql");
/// The `commit` and `layer` column as `history.sql` writes it; it appears
/// once in each of the two tables.
const HISTORY_SERIAL: &str = "    history_serial INTEGER NOT NULL DEFAULT 0,\n";
/// The `store_policy` column as `metadata.sql` writes it.
const TRACE_SALT: &str = " trace_salt BLOB NOT NULL DEFAULT (X'0000000000000000000000000000000000000000000000000000000000000000') CHECK(length(trace_salt)=32)\n";

fn definition(sql: &rusqlite::Connection) -> String {
    let mut statement = sql
        .prepare("SELECT type,name,sql FROM sqlite_schema WHERE substr(name,1,7)!='sqlite_' ORDER BY type,name")
        .unwrap();
    let rows = statement
        .query_map([], |r| Ok([r.get::<_, String>(0)?, r.get(1)?, r.get(2)?]))
        .unwrap();
    rows.map(|r| r.unwrap().map(|v| v + "\n").concat())
        .collect()
}

/// Rewrites a fresh Store into the format written before the GC schema and
/// before the position columns existed: the seven GC tables and the three
/// columns are gone, and the recorded schema is what that revision wrote.
fn make_old(path: &std::path::Path) {
    let sql = rusqlite::Connection::open(path).unwrap();
    sql.execute_batch(
        "ALTER TABLE \"commit\" DROP COLUMN history_serial; \
         ALTER TABLE layer DROP COLUMN history_serial; \
         ALTER TABLE store_policy DROP COLUMN trace_salt; \
         DROP TABLE branch_retired; DROP TABLE layer_stack_retired; DROP TABLE path_removed; \
         DROP TABLE project_epoch; DROP TABLE gc_intent; DROP TABLE gc_op; DROP TABLE gc_dead;",
    )
    .unwrap();
    let source: String = sql
        .query_row("SELECT schema_source FROM history_meta", [], |r| r.get(0))
        .unwrap();
    assert_eq!(source.matches(HISTORY_SERIAL).count(), 2);
    assert_eq!(source.matches(TRACE_SALT).count(), 1);
    let old = source
        .replacen(HISTORY_SERIAL, "", 2)
        .replacen(TRACE_SALT, "", 1)
        .replacen(GC, "", 1);
    assert_ne!(old, source, "the current schema holds the columns and GC");
    sql.execute(
        "UPDATE history_meta SET schema_source=?1, schema_definition=?2",
        rusqlite::params![old, definition(&sql)],
    )
    .unwrap();
}

fn writable(
    path: &std::path::Path,
) -> Result<SqliteHandles, layerfs_storage::port::PersistenceError> {
    SqliteHandles::open_writable(
        PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        BINDING,
        [71; 32],
    )
}

#[test]
fn an_old_format_store_gains_the_position_columns_and_a_salt() {
    let t = Temp::new("old-format-columns");
    let path = t.join("db");
    let fresh_source: String = {
        drop(create(&path));
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.query_row("SELECT schema_source FROM history_meta", [], |r| r.get(0))
            .unwrap()
    };
    make_old(&path);
    // Read-only authority cannot alter, so it still refuses.
    assert!(sqlite::open_read_only(&path, BINDING, [71; 32]).is_err());

    let handles = writable(&path).expect("an old-format Store opens writable");
    drop(handles);

    let sql = rusqlite::Connection::open(&path).unwrap();
    for (table, column) in [
        ("commit", "history_serial"),
        ("layer", "history_serial"),
        ("store_policy", "trace_salt"),
    ] {
        let present: i64 = sql
            .query_row(
                "SELECT count(*) FROM pragma_table_info(?1) WHERE name=?2",
                rusqlite::params![table, column],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(present, 1, "{table}.{column} exists after the migration");
    }
    for table in ["commit", "layer"] {
        let default: Option<String> = sql
            .query_row(
                "SELECT dflt_value FROM pragma_table_info(?1) WHERE name='history_serial'",
                rusqlite::params![table],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            default.as_deref(),
            Some("0"),
            "{table}.history_serial defaults to 0"
        );
    }
    let (length, salt): (i64, String) = sql
        .query_row(
            "SELECT length(trace_salt),hex(trace_salt) FROM store_policy WHERE id=1",
            [],
            |r| Ok((r.get(0).unwrap(), r.get(1).unwrap())),
        )
        .unwrap();
    assert_eq!(length, 32, "the salt is 32 bytes");
    assert_ne!(
        salt,
        "00".repeat(32),
        "the salt is random, not the zero default"
    );
    let (source, recorded): (String, String) = sql
        .query_row(
            "SELECT schema_source,schema_definition FROM history_meta",
            [],
            |r| Ok((r.get(0).unwrap(), r.get(1).unwrap())),
        )
        .unwrap();
    assert_eq!(
        source, fresh_source,
        "the migrated Store records the current schema"
    );
    // SQLite rewrites the CREATE TABLE text on ADD COLUMN, so a migrated
    // table's definition differs from a fresh Store's in whitespace only;
    // `validate` requires the recorded definition to be the live one.
    assert_eq!(
        recorded,
        definition(&sql),
        "the recorded definition is the live definition"
    );
    drop(sql);
    reopen(&path);
    writable(&path).expect("a migrated Store reopens");
}
