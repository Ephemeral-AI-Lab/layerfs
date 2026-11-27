//! #776: a writable open runs the additive migrations only for a Store that
//! needs them. A current Store takes no write transaction for them (each one
//! took the writer lane), while a Store an earlier step left behind still
//! completes.
#![cfg(any(target_os = "macos", target_os = "linux"))]
use crate::support;

use layerfs_history::{HistoryCatalog, HistoryCatalogConfig, Page};
use layerfs_persistence::{
    thread_work, DeferredSqliteWriter, PersistenceConfig, SqliteHandles, SqlitePersistenceProfile,
};
use layerfs_storage::StoragePolicy;
use support::{Temp, BINDING};

fn config(path: &std::path::Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Normal)
}

fn create(path: &std::path::Path) {
    let handles = SqliteHandles::create(
        config(path),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: [76; 32],
            incarnation: 1,
        },
    )
    .expect("Store creation");
    drop(handles);
}

/// The write transactions one writable open of `path` ran on this thread.
fn open_write_transactions(path: &std::path::Path) -> u64 {
    let before = thread_work();
    let handles = SqliteHandles::open_writable(config(path), BINDING, [76; 32]).expect("open");
    let done = thread_work().since(&before);
    drop(handles);
    done.write_transactions
}

fn has_next_serial(path: &std::path::Path) -> bool {
    let sql = rusqlite::Connection::open(path).unwrap();
    sql.query_row(
        "SELECT count(*) FROM pragma_table_info('history_meta') WHERE name='next_history_serial'",
        [],
        |r| r.get::<_, i64>(0),
    )
    .unwrap()
        == 1
}

#[test]
fn a_current_store_opens_without_a_migration_transaction() {
    let temp = Temp::new("open-current");
    let path = temp.join("store.sqlite");
    create(&path);
    // The open's one write transaction is the merge record sweep; the nine
    // migration steps took nine more before #776.
    assert_eq!(open_write_transactions(&path), 1);
    assert_eq!(open_write_transactions(&path), 1);
}

#[test]
fn a_store_left_before_the_next_serial_column_still_gains_it() {
    let temp = Temp::new("open-interrupted");
    let path = temp.join("store.sqlite");
    create(&path);
    remove_next_serial(&path);
    assert!(!has_next_serial(&path));
    assert!(open_write_transactions(&path) > 1);
    assert!(has_next_serial(&path));
    assert_eq!(open_write_transactions(&path), 1);
}

fn remove_next_serial(path: &std::path::Path) {
    // What an open interrupted between `add_gc` and `add_position_state`
    // leaves: the current source recorded, the column not yet appended.
    let sql = rusqlite::Connection::open(path).unwrap();
    sql.execute_batch("ALTER TABLE history_meta DROP COLUMN next_history_serial")
        .unwrap();
    let definition: String = sql
        .prepare("SELECT type,name,sql FROM sqlite_schema WHERE substr(name,1,7)!='sqlite_' ORDER BY type,name")
        .unwrap()
        .query_map([], |r| Ok([r.get::<_, String>(0)?, r.get(1)?, r.get(2)?]))
        .unwrap()
        .map(|r| r.unwrap().map(|v| v + "\n").concat())
        .collect();
    sql.execute(
        "UPDATE history_meta SET schema_definition=?1",
        rusqlite::params![definition],
    )
    .unwrap();
}

#[test]
fn a_deferred_writer_completes_the_interrupted_open_on_first_use() {
    let temp = Temp::new("open-interrupted-deferred");
    let path = temp.join("store.sqlite");
    create(&path);
    remove_next_serial(&path);

    let (_, history, writer) =
        DeferredSqliteWriter::open(config(&path), BINDING, [76; 32]).unwrap();
    let stacks = history
        .layer_stacks(&Page {
            cursor: None,
            limit: 1,
        })
        .unwrap();
    assert!(stacks.records.is_empty());
    assert!(writer.get().is_none());
    assert!(!has_next_serial(&path));

    writer.start().unwrap();
    assert!(has_next_serial(&path));
}
