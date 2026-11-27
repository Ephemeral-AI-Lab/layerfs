//! A Store written before the binding hint table existed opens writable and
//! gains the table (#352 F-011, REBUILD.md kind 2): nothing it holds is
//! rewritten, and hints are recorded from then on.
use crate::support;
use layerfs_history::{records::BindingHint, HistoryCatalog};
use layerfs_persistence::{PersistenceConfig, SqliteHandles, SqlitePersistenceProfile};
use support::*;

const HISTORY: &str = include_str!("../../sql/sqlite/schema/history.sql");
const LINEAGE: &str = include_str!("../../sql/sqlite/schema/lineage.sql");
const SCRATCH: &str = include_str!("../../sql/sqlite/schema/merge_scratch.sql");
const DIGESTS: &str = include_str!("../../sql/sqlite/schema/file_digest.sql");
const GC: &str = include_str!("../../sql/sqlite/schema/gc.sql");

/// The three position columns T1.2a added, as the schemas write them; the old
/// format this fixture builds lacks them. `trace_salt` was appended as
/// `store_policy`\s last column, so its line carries the comma that ends
/// `next_ordinal`: a Store written before it recorded neither line nor comma.
const HISTORY_SERIAL: &str = "    history_serial INTEGER NOT NULL DEFAULT 0,\n";
const TRACE_SALT: &str = ",\n trace_salt BLOB NOT NULL DEFAULT (X'0000000000000000000000000000000000000000000000000000000000000000') CHECK(length(trace_salt)=32)";
/// The `history_meta` next serial column as `history.sql` writes it (T1.4).
const NEXT_SERIAL: &str = "    next_history_serial INTEGER NOT NULL DEFAULT 1,\n";

/// The table's definition as the history schema writes it, with its trailing
/// blank line: the text an old Store's recorded schema source lacks.
fn hint_table() -> String {
    let start = HISTORY.find("CREATE TABLE binding_hint_block (").unwrap();
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
/// tables), no position columns (including T1.4's next serial), and the
/// schema source and definition they recorded.
fn make_old(path: &std::path::Path) {
    let sql = rusqlite::Connection::open(path).unwrap();
    sql.execute_batch(
        "ALTER TABLE \"commit\" DROP COLUMN history_serial; \
         ALTER TABLE layer DROP COLUMN history_serial; \
         ALTER TABLE store_policy DROP COLUMN trace_salt; \
         ALTER TABLE history_meta DROP COLUMN next_history_serial; \
         DROP TABLE binding_hint_block; DROP TABLE layer_lineage; DROP TABLE commit_lineage; \
         DROP TABLE absorbed; DROP TABLE reach; DROP TABLE merge_operation; \
         DROP TABLE merge_record; DROP TABLE file_digest; \
         DROP TABLE branch_retired; DROP TABLE layer_stack_retired; DROP TABLE path_removed; \
         DROP TABLE project_epoch; DROP TABLE gc_intent; DROP TABLE gc_op; DROP TABLE gc_dead; DROP TABLE kept_node;",
    )
    .unwrap();
    let source: String = sql
        .query_row("SELECT schema_source FROM history_meta", [], |r| r.get(0))
        .unwrap();
    let old = source
        .replacen(HISTORY_SERIAL, "", 2)
        .replacen(TRACE_SALT, "", 1)
        .replacen(NEXT_SERIAL, "", 1)
        .replacen(&hint_table(), "", 1)
        .replacen(LINEAGE, "", 1)
        .replacen(SCRATCH, "", 1)
        .replacen(DIGESTS, "", 1)
        .replacen(GC, "", 1);
    assert_ne!(old, source, "the history schema holds the table");
    assert!(
        old.contains("4294967296)\n) STRICT;"),
        "a pre-T1.2a Store recorded no comma after the last store_policy column"
    );
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
        PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Normal),
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
