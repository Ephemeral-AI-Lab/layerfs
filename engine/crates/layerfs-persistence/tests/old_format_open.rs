//! A Store written before the `binding_hint` table existed opens writable and
//! gains the table (#352 F-011, REBUILD.md kind 2): nothing it holds is
//! rewritten, and hints are recorded from then on.
mod support;
use layerfs_history::{records::BindingHint, HistoryCatalog};
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use support::*;

const HISTORY: &str = include_str!("../sql/sqlite/history.sql");
const LINEAGE: &str = include_str!("../sql/sqlite/lineage.sql");
const SCRATCH: &str = include_str!("../sql/sqlite/merge_scratch.sql");
const DIGESTS: &str = include_str!("../sql/sqlite/file_digest.sql");

/// The table's definition as the history schema writes it, with its trailing
/// blank line: the text an old Store's recorded schema source lacks.
fn hint_table() -> String {
    let start = HISTORY.find("CREATE TABLE binding_hint (").unwrap();
    let end = start + HISTORY[start..].find(";\n").unwrap() + 2;
    format!("{}\n", &HISTORY[start..end])
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

/// Rewrites a fresh Store into the format the daemons before `49499abac`
/// wrote: no `binding_hint` table (nor the later #353 lineage and #355 merge
/// tables), and the
/// schema source and definition they recorded.
fn make_old(path: &std::path::Path) {
    let sql = rusqlite::Connection::open(path).unwrap();
    sql.execute_batch(
        "DROP TABLE binding_hint; DROP TABLE layer_lineage; DROP TABLE commit_lineage; \
         DROP TABLE absorbed; DROP TABLE reach; DROP TABLE merge_operation; \
         DROP TABLE merge_record; DROP TABLE file_digest;",
    )
    .unwrap();
    let source: String = sql
        .query_row("SELECT schema_source FROM history_meta", [], |r| r.get(0))
        .unwrap();
    let old = source
        .replacen(&hint_table(), "", 1)
        .replacen(LINEAGE, "", 1)
        .replacen(SCRATCH, "", 1)
        .replacen(DIGESTS, "", 1);
    assert_ne!(old, source, "the history schema holds the table");
    sql.execute(
        "UPDATE history_meta SET schema_source=?1, schema_definition=?2",
        rusqlite::params![old, definition(&sql)],
    )
    .unwrap();
}

fn writable(path: &std::path::Path) -> Result<Handles, layerfs_storage::port::PersistenceError> {
    Handles::open_writable(
        PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        BINDING,
        [71; 32],
    )
}

#[test]
fn an_old_format_store_opens_and_records_hints_from_then_on() {
    let t = Temp::new("old-format");
    let path = t.join("db");
    let fresh: String = {
        drop(create(&path));
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.query_row("SELECT schema_source FROM history_meta", [], |r| r.get(0))
            .unwrap()
    };
    make_old(&path);
    // Read-only authority cannot add the table, so it still refuses.
    assert!(sqlite::open_read_only(&path, BINDING, [71; 32]).is_err());

    let handles = writable(&path).expect("an old-format Store opens writable");
    let hint = BindingHint {
        child: 7,
        parent: 1,
        name: b"a.txt".to_vec(),
    };
    handles.history.record_bindings(&[hint.clone()]).unwrap();
    assert_eq!(handles.history.bindings_of(7).unwrap(), vec![hint]);
    drop(handles);

    let sql = rusqlite::Connection::open(&path).unwrap();
    let source: String = sql
        .query_row("SELECT schema_source FROM history_meta", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        source, fresh,
        "the migrated Store records the current schema"
    );
    drop(sql);
    reopen(&path);
    writable(&path).expect("a migrated Store reopens");
}

#[test]
fn a_store_with_any_other_schema_difference_is_still_refused() {
    let t = Temp::new("old-format-other");
    let path = t.join("db");
    drop(create(&path));
    make_old(&path);
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch("DROP INDEX layers_child;").unwrap();
    drop(sql);
    assert!(writable(&path).is_err());
}
