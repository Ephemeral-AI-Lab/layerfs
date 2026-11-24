//! Phase 3: the two `gc_intent` place columns. A new Store ships them and
//! records them in its schema source. The 2.0 schema has no additive
//! migration: a Store written before them is refused `Malformed` at open and
//! needs the explicit 2.0 conversion.
#![cfg(any(target_os = "macos", target_os = "linux"))]
mod support;

use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{PersistenceConfig, SqliteHandles, SqlitePersistenceProfile};
use layerfs_storage::StoragePolicy;
use support::{Temp, BINDING};

/// The two `gc_intent` place columns as `sql/sqlite/gc.sql` writes them
/// after `phase`.
const INTENT_COLUMNS: &str = "    machine_incarnation TEXT,\n    sandbox_generation TEXT,\n";

/// The two columns, in the order the schema declares them.
const INTENT_COLUMN_NAMES: [&str; 2] = ["machine_incarnation", "sandbox_generation"];

fn config(path: &std::path::Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Normal)
}

fn create(path: &std::path::Path) -> SqliteHandles {
    SqliteHandles::create(
        config(path),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: [71; 32],
            incarnation: 7,
        },
    )
    .expect("Store creation")
}

fn writable(
    path: &std::path::Path,
) -> Result<SqliteHandles, layerfs_storage::port::PersistenceError> {
    SqliteHandles::open_writable(config(path), BINDING, [71; 32])
}

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

fn recorded(path: &std::path::Path) -> (String, String) {
    let sql = rusqlite::Connection::open(path).unwrap();
    let row = sql
        .query_row(
            "SELECT schema_source,schema_definition FROM history_meta WHERE id=1",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .unwrap();
    drop(sql);
    row
}

fn column_present(sql: &rusqlite::Connection, column: &str) -> i64 {
    sql.query_row(
        "SELECT count(*) FROM pragma_table_info('gc_intent') WHERE name=?1",
        rusqlite::params![column],
        |r| r.get(0),
    )
    .unwrap()
}

/// Rewrites a fresh Store into the format a p3-schema Store recorded: the four
/// columns are gone and the recorded source no longer holds them. Returns the
/// fresh source it replaced.
fn make_pre_intent_authority(path: &std::path::Path) -> String {
    let fresh = recorded(path).0;
    let sql = rusqlite::Connection::open(path).unwrap();
    for column in INTENT_COLUMN_NAMES.into_iter().rev() {
        sql.execute_batch(&format!("ALTER TABLE gc_intent DROP COLUMN {column};"))
            .unwrap();
    }
    let old = fresh.replacen(INTENT_COLUMNS, "", 1);
    assert_ne!(
        old, fresh,
        "a fresh Store holds the intent authority columns"
    );
    sql.execute(
        "UPDATE history_meta SET schema_source=?1,schema_definition=?2",
        rusqlite::params![old, definition(&sql)],
    )
    .unwrap();
    fresh
}

#[test]
fn a_new_store_ships_the_intent_authority_columns() {
    let t = Temp::new("gc-intent-authority-new");
    let path = t.join("db");
    drop(create(&path));
    let (source, recorded_definition) = recorded(&path);
    assert_eq!(
        source.matches(INTENT_COLUMNS).count(),
        1,
        "the source holds the four columns once"
    );
    let sql = rusqlite::Connection::open(&path).unwrap();
    for column in INTENT_COLUMN_NAMES {
        assert_eq!(column_present(&sql, column), 1, "{column} is created");
    }
    assert_eq!(
        recorded_definition,
        definition(&sql),
        "the recorded definition is the live one"
    );
    drop(sql);
    support::sqlite::open_read_only(&path, BINDING, [71; 32]).expect("a new Store opens read-only");
}

/// The 2.0 schema has no additive migration: a Store recorded before the
/// four authority columns is refused at its first writable open instead of
/// being altered, and read-only authority refuses it too.
#[test]
fn an_old_store_without_the_intent_authority_columns_is_refused() {
    let t = Temp::new("gc-intent-authority-old");
    let path = t.join("db");
    drop(create(&path));
    let _ = make_pre_intent_authority(&path);
    // Read-only authority cannot add the columns, so it refuses.
    assert!(support::sqlite::open_read_only(&path, BINDING, [71; 32]).is_err());
    // Nor does a writable open migrate: the direct-schema 2.0 Store refuses.
    assert!(
        matches!(
            writable(&path),
            Err(layerfs_storage::port::PersistenceError::Malformed)
        ),
        "a pre-column Store is not migrated"
    );
}
