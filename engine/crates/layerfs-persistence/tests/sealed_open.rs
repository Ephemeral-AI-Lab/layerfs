//! A sealed Store opens: the fresh file. One recorded by the phase-3 branch
//! before the intent-authority columns existed is refused: the lean 2.0
//! schema has no additive migration.
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{
    PersistenceConfig, SqliteAcquisitionSchema, SqliteHandles, SqlitePersistenceProfile,
};
use layerfs_storage::StoragePolicy;

const BINDING: &[u8] = b"sealed-open";
const CURSOR: [u8; 32] = [61; 32];

/// The definition `definition()` records: every non-internal `sqlite_schema`
/// row as `type\nname\nsql\n`, ordered as the engine reads it.
fn definition(connection: &rusqlite::Connection) -> String {
    let mut statement = connection
        .prepare(
            "SELECT type,name,sql FROM sqlite_schema \
             WHERE substr(name,1,7)!='sqlite_' ORDER BY type,name",
        )
        .unwrap();
    let rows: Vec<(String, String, String)> = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(|row| row.unwrap())
        .collect();
    let mut text = String::new();
    for (kind, name, sql) in rows {
        text.push_str(&kind);
        text.push('\n');
        text.push_str(&name);
        text.push('\n');
        text.push_str(&sql);
        text.push('\n');
    }
    text
}

fn catalog() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: BINDING.to_vec(),
        cursor_key: CURSOR,
        incarnation: 1,
    }
}

/// A fresh Normal Store with the acquisition tables `ProjectApi::init`
/// creates, sealed to its one closed file.
fn sealed_store(directory: &std::path::Path) -> std::path::PathBuf {
    let path = directory.join("sealed.sqlite");
    let handles = SqliteHandles::create(
        PersistenceConfig::sqlite(&path)
            .with_sqlite_profile(SqlitePersistenceProfile::Normal)
            .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables),
        StoragePolicy::frozen_default(),
        &catalog(),
    )
    .unwrap();
    handles.seal().unwrap();
    path
}

/// The profile every installed Store opens with (the daemon's `Normal`).
fn installed(path: &std::path::Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Normal)
}

#[test]
fn a_sealed_store_opens_read_only() {
    let directory = std::env::temp_dir().join(format!("sealed-open-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let path = sealed_store(&directory);
    SqliteHandles::open_writable(installed(&path), BINDING, CURSOR).unwrap();
    SqliteHandles::open_read_only(installed(&path), BINDING, CURSOR).unwrap();
    std::fs::remove_dir_all(&directory).unwrap();
}

/// The phase-3 branch sealed Stores with `gc.sql` ending in two extra
/// newlines and without the two `gc_intent` place columns. Opening one
/// cannot add them: the lean 2.0 schema is direct, so the recorded source is
/// refused `Malformed` and the explicit 2.0 conversion is required.
#[test]
fn a_sealed_store_recorded_before_the_intent_authority_columns_is_malformed() {
    let directory =
        std::env::temp_dir().join(format!("sealed-open-pre-columns-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let path = sealed_store(&directory);

    let columns = ["machine_incarnation", "sandbox_generation"];
    let (current_source, old_source) = {
        let connection = rusqlite::Connection::open(&path).unwrap();
        let source: String = connection
            .query_row(
                "SELECT schema_source FROM history_meta WHERE id=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let mut old = source.replacen(
            "    machine_incarnation TEXT,\n    sandbox_generation TEXT,\n",
            "",
            1,
        );
        old = old.replacen(
            "CREATE TABLE init_operation (\n",
            "\n\nCREATE TABLE init_operation (\n",
            1,
        );
        assert_ne!(old, source);
        for column in columns.iter().rev() {
            connection
                .execute(&format!("ALTER TABLE gc_intent DROP COLUMN {column}"), [])
                .unwrap();
        }
        let definition = definition(&connection);
        connection
            .execute(
                "UPDATE history_meta SET schema_source=?1, schema_definition=?2 WHERE id=1",
                rusqlite::params![old, definition],
            )
            .unwrap();
        (source, old)
    };
    assert!(!old_source.contains("machine_incarnation"));
    assert!(old_source.len() < current_source.len());

    // There is no additive migration: the old recorded source is refused at
    // the first writable open, and the Store is not rewritten for it.
    assert!(
        matches!(
            SqliteHandles::open_writable(installed(&path), BINDING, CURSOR),
            Err(layerfs_storage::port::PersistenceError::Malformed)
        ),
        "a pre-column sealed Store is not migrated"
    );
    assert!(
        matches!(
            SqliteHandles::open_read_only(installed(&path), BINDING, CURSOR),
            Err(layerfs_storage::port::PersistenceError::Malformed)
        ),
        "read-only authority refuses it too"
    );
    std::fs::remove_dir_all(&directory).unwrap();
}
