//! Schema claims are checked against shipped DDL, never trusted as authority.
mod support;
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{
    diagnostic::{Options, Outcome},
    PersistenceConfig, SqliteHandles, SqlitePersistenceProfile,
};
use layerfs_storage::StoragePolicy;

fn create(path: &std::path::Path) -> SqliteHandles {
    SqliteHandles::create(
        PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: b"doctor-schema".to_vec(),
            cursor_key: [71; 32],
            incarnation: 7,
        },
    )
    .unwrap()
}
fn definition(db: &rusqlite::Connection) -> String {
    let mut statement = db.prepare("SELECT type,name,sql FROM sqlite_schema WHERE substr(name,1,7)!='sqlite_' ORDER BY type,name").unwrap();
    let rows = statement
        .query_map([], |row| {
            Ok([row.get::<_, String>(0)?, row.get(1)?, row.get(2)?])
        })
        .unwrap();
    let mut value = String::new();
    for row in rows {
        for part in row.unwrap() {
            value.push_str(&part);
            value.push('\n');
        }
    }
    value
}
#[test]
fn changed_live_schema_cannot_be_blessed_by_rewriting_saved_definition() {
    let temp = support::Temp::new("doctor-schema-forged");
    let path = temp.join("store.sqlite");
    let writer = create(&path);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("DROP TRIGGER pack_immutable_update;")
        .unwrap();
    db.execute(
        "UPDATE history_meta SET schema_definition=?1",
        [definition(&db)],
    )
    .unwrap();
    let before = writer.diagnostics().unwrap();
    assert!(
        writer.diagnose(Options::default(), &mut |_| {}).is_err(),
        "live contract damage must not be blessed by saved metadata"
    );
    assert!(SqliteHandles::open_diagnostic(&path, b"doctor-schema", [71; 32]).is_err());
    let after = writer.diagnostics().unwrap();
    assert_eq!(before.write_transactions, after.write_transactions);
    assert_eq!(before.write_commits, after.write_commits);
}
#[test]
fn oversized_saved_schema_and_identity_are_refused_without_source_changes() {
    for column in [
        "schema_source",
        "schema_definition",
        "binding_key",
        "catalog_id",
    ] {
        let temp = support::Temp::new("doctor-schema-large");
        let path = temp.join("store.sqlite");
        let writer = create(&path);
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch("PRAGMA ignore_check_constraints=ON;")
            .unwrap();
        let value = if column.starts_with("schema_") {
            "CAST(?1 AS TEXT)"
        } else {
            "?1"
        };
        db.execute(
            &format!("UPDATE history_meta SET {column}={value}"),
            [vec![b'x'; 1024 * 1024]],
        )
        .unwrap();
        let before: Vec<u8> = db
            .query_row(
                &format!("SELECT CAST({column} AS BLOB) FROM history_meta"),
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(SqliteHandles::open_diagnostic(&path, b"doctor-schema", [71; 32]).is_err());
        let after: Vec<u8> = db
            .query_row(
                &format!("SELECT CAST({column} AS BLOB) FROM history_meta"),
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(before, after);
        drop(writer);
    }
}

#[test]
fn authority_changed_after_admission_cannot_be_a_clean_audit() {
    let temp = support::Temp::new("doctor-schema-identity-race");
    let path = temp.join("store.sqlite");
    let writer = create(&path);
    let reader = SqliteHandles::open_diagnostic(&path, b"doctor-schema", [71; 32]).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    let before = writer.diagnostics().unwrap();
    db.execute("UPDATE history_meta SET catalog_incarnation=8", [])
        .unwrap();
    let mut out = Vec::new();
    assert!(
        reader
            .diagnose(Options::default(), &mut |o| out.push(o))
            .is_err(),
        "stale admitted authority must not qualify a replacement: {out:?}"
    );
    assert!(out
        .iter()
        .any(|o| o.id == "store.identity" && o.outcome == Outcome::Fail));
    assert!(!out.iter().any(|o| o.id == "store.objects.summary"));
    let after = writer.diagnostics().unwrap();
    assert_eq!(after.write_commits, before.write_commits);
    assert_eq!(after.write_transactions, before.write_transactions);
}

#[test]
fn huge_text_in_integer_cell_is_rejected_before_pack_inventory() {
    use layerfs_content::{encode_whole_file_payload, FinalizedObject, ObjectRole};
    use layerfs_storage::Storage;
    let temp = support::Temp::new("doctor-schema-storage-class");
    let path = temp.join("store.sqlite");
    let writer = create(&path);
    let storage = Storage::new(writer.storage.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    save.accept(
        FinalizedObject::new(
            ObjectRole::WholeFile,
            encode_whole_file_payload(b"valid retained object").unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    save.finish().unwrap();
    let reader = SqliteHandles::open_diagnostic(&path, b"doctor-schema", [71; 32]).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    let trigger: String = db
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE name='pack_immutable_update'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let pack_sql: String = db
        .query_row("SELECT sql FROM sqlite_schema WHERE name='pack'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let version: i64 = db
        .query_row("PRAGMA schema_version", [], |r| r.get(0))
        .unwrap();
    db.execute_batch("DROP TRIGGER pack_immutable_update; PRAGMA writable_schema=ON;")
        .unwrap();
    db.execute(
        "UPDATE sqlite_schema SET sql=?1 WHERE name='pack'",
        [pack_sql.replace(") STRICT", ")")],
    )
    .unwrap();
    db.execute_batch(&format!(
        "PRAGMA schema_version={}; PRAGMA writable_schema=OFF;",
        version + 10
    ))
    .unwrap();
    drop(db);
    // Deliberately construct persisted storage-class damage, then restore the
    // exact shipped STRICT declaration before diagnosis. No user data is used.
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA ignore_check_constraints=ON;")
        .unwrap();
    let text = format!("\0{}", "x".repeat(1024 * 1024));
    db.execute("UPDATE pack SET domain=?1", [text]).unwrap();
    db.execute_batch("PRAGMA writable_schema=ON;").unwrap();
    db.execute(
        "UPDATE sqlite_schema SET sql=?1 WHERE name='pack'",
        [pack_sql],
    )
    .unwrap();
    db.execute_batch(&format!(
        "PRAGMA schema_version={}; PRAGMA writable_schema=OFF;",
        version + 20
    ))
    .unwrap();
    drop(db);
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(&trigger).unwrap();
    let actual: (String, i64, i64) = db
        .query_row(
            "SELECT typeof(domain),length(domain),length(CAST(domain AS BLOB)) FROM pack",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(actual, ("text".to_owned(), 0, 1024 * 1024 + 1));
    let before = reader.diagnostics().unwrap();
    let mut out = Vec::new();
    assert!(reader
        .diagnose(Options::default(), &mut |o| out.push(o))
        .is_err());
    assert!(out
        .iter()
        .any(|o| o.id == "store.database.types" && o.outcome == Outcome::Fail));
    assert!(!out.iter().any(|o| o.id == "store.packs.progress"));
    let after = reader.diagnostics().unwrap();
    assert_eq!(before.write_commits, after.write_commits);
    assert_eq!(before.write_transactions, after.write_transactions);
}
