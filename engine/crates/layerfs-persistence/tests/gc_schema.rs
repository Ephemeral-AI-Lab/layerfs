//! The lean phase-3 schema (version 3): the four `gc_intent` authority
//! columns, and none of the removed Store-side machine-authority, Sandbox
//! tombstone or audit tables. A released schema version 2 Store is refused,
//! never silently migrated.
#![cfg(any(target_os = "macos", target_os = "linux"))]
mod support;

use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{PersistenceConfig, SqliteHandles, SqlitePersistenceProfile};
use layerfs_storage::StoragePolicy;
use support::{Temp, BINDING};

const SQLITE_GC: &str = include_str!("../sql/sqlite/gc.sql");
const POSTGRESQL_GC: &str = include_str!("../sql/postgresql/gc.sql");

/// The removed tables, in name order.
const REMOVED_TABLES: [&str; 3] = ["gc_audit", "gc_machine_authority", "gc_sandbox_tombstone"];

/// The two authority columns the lean schema keeps.
const AUTHORITY_COLUMNS: [&str; 2] = ["machine_incarnation", "sandbox_generation"];

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

fn table_present(sql: &rusqlite::Connection, table: &str) -> i64 {
    sql.query_row(
        "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name=?1",
        rusqlite::params![table],
        |r| r.get(0),
    )
    .unwrap()
}

#[test]
fn a_new_store_ships_the_authority_columns_and_no_authority_tables() {
    let t = Temp::new("gc-schema-new");
    let path = t.join("db");
    drop(create(&path));
    let sql = rusqlite::Connection::open(&path).unwrap();
    for table in REMOVED_TABLES {
        assert_eq!(table_present(&sql, table), 0, "{table} is gone");
    }
    for column in AUTHORITY_COLUMNS {
        let present: i64 = sql
            .query_row(
                "SELECT count(*) FROM pragma_table_info('gc_intent') WHERE name=?1",
                rusqlite::params![column],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(present, 1, "gc_intent keeps {column}");
    }
    let version: i64 = sql
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 39, "the Monolithic layout is schema version 3");
    drop(sql);
    let (source, definition) = recorded(&path);
    for table in REMOVED_TABLES {
        assert!(
            !source.contains(&format!("CREATE TABLE {table} (")),
            "the recorded source has no {table}"
        );
    }
    for column in AUTHORITY_COLUMNS {
        assert!(
            source.contains(&format!("    {column} ")),
            "the source keeps {column}"
        );
    }
    let sql = rusqlite::Connection::open(&path).unwrap();
    let live: String = {
        let mut statement = sql
            .prepare(
                "SELECT type,name,sql FROM sqlite_schema \
                 WHERE substr(name,1,7)!='sqlite_' ORDER BY type,name",
            )
            .unwrap();
        let rows = statement
            .query_map([], |r| Ok([r.get::<_, String>(0)?, r.get(1)?, r.get(2)?]))
            .unwrap();
        rows.map(|r| r.unwrap().map(|v| v + "\n").concat())
            .collect()
    };
    assert_eq!(definition, live, "the recorded definition is the live one");
    drop(sql);
    drop(writable(&path).expect("the new Store reopens writable"));
}

#[test]
fn both_engine_scripts_drop_the_authority_tables_and_keep_the_columns() {
    for (engine, gc) in [("SQLite", SQLITE_GC), ("PostgreSQL", POSTGRESQL_GC)] {
        for table in REMOVED_TABLES {
            assert!(
                !gc.contains(&format!("CREATE TABLE {table} (")),
                "{engine}: gc.sql creates no {table}"
            );
        }
        for column in AUTHORITY_COLUMNS {
            assert!(
                gc.contains(&format!("    {column} ")),
                "{engine}: gc.sql keeps the {column} column"
            );
        }
    }
}

#[test]
fn the_postgresql_table_set_has_no_authority_table() {
    let tables = layerfs_persistence::internals::POSTGRESQL_TABLES;
    assert_eq!(tables.len(), 27);
    for table in REMOVED_TABLES {
        assert!(!tables.contains(&table), "the table set omits {table}");
    }
    assert!(tables.contains(&"gc_intent"), "gc_intent remains");
    assert!(
        tables.windows(2).all(|pair| pair[0] < pair[1]),
        "the table set is in name order"
    );
}

#[test]
fn a_released_version_2_store_is_refused_with_its_conversion() {
    let t = Temp::new("gc-schema-v2");
    let path = t.join("db");
    drop(create(&path));
    {
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.pragma_update(None, "user_version", 36).unwrap();
    }
    let refusal = writable(&path).err().expect("a version 2 Store is refused");
    let text = format!("{refusal:?}");
    assert!(text.contains("schema version 2"), "{text}");
    assert!(text.contains("convert"), "{text}");
    assert!(text.contains("store_gc_convert.py"), "{text}");
}
